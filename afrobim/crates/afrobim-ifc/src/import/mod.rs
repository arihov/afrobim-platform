// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! IFC import as a reference model: read an IFC STEP file for display and clash checking.
//!
//! Nothing here becomes an editable AfroBIM element. [`import_ifc`] reads the file into a
//! [`ReferenceModel`]: the spatial tree, every product with its name, GlobalId, entity type,
//! predefined type, property sets and (for the geometry this reader supports) triangle meshes in
//! world millimetres, plus an [`ImportReport`] that counts what was and was not read. The module
//! depends on nothing in `afrobim-coordination`; the app turns [`ReferenceProduct::meshes`] into
//! clash bodies.
//!
//! Supported schemas: IFC2X3, IFC4 and IFC4X3 (attribute positions from the EXPRESS schemas
//! compiled into IfcOpenShell 0.9.0.post1, see `tables.rs`).

mod reader;
mod stepfile;
mod tables;
mod tess;
mod xform;

use std::collections::BTreeMap;
use std::path::Path;

use afrobim_draw::Mesh3;

pub use stepfile::{decode_string, parse_value, Entity, StepError, StepFile, StepHeader, Value};
pub use tess::{triangulate, triangulate_3d};

/// What to read and how.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportOptions {
    /// Vertices per circle profile (`IfcCircleProfileDef`). The polygon is inscribed: its vertices
    /// lie on the circle, the first on the profile's +X axis, so with a multiple of 4 the bounding
    /// box on the profile axes is exact. Default 32.
    pub circle_segments: u32,
    /// Apply `IfcMapConversion` (when the file has one) so coordinates are in the projected CRS.
    /// Default `false`: coordinates are the engineering (local) coordinates of the placements,
    /// which is what IfcOpenShell's geometry iterator returns by default.
    pub apply_map_conversion: bool,
    /// Read property sets and quantity sets. Costs memory on files with many products.
    pub properties: bool,
    /// Give a product without readable body geometry a box from an `IfcBoundingBox` "Box"
    /// representation, when it has one.
    pub bounding_box_placeholders: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions { circle_segments: 32, apply_map_conversion: false, properties: true, bounding_box_placeholders: true }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportError {
    Step(StepError),
    /// The FILE_SCHEMA is not one of IFC2X3, IFC4, IFC4X3.
    UnsupportedSchema(String),
    Io(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Step(e) => write!(f, "{e}"),
            ImportError::UnsupportedSchema(s) => write!(f, "unsupported IFC schema {s:?} (supported: IFC2X3, IFC4, IFC4X3)"),
            ImportError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<StepError> for ImportError {
    fn from(e: StepError) -> Self {
        ImportError::Step(e)
    }
}

/// How much of a product's body geometry was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryStatus {
    /// Every body item was read.
    Read,
    /// Some body items were read, others (listed in `unread_items`) were not.
    Partial,
    /// Nothing readable; a box from an `IfcBoundingBox` representation stands in.
    BoundingBoxPlaceholder,
    /// Body items exist but none could be read (see `unread_items`).
    NotRead,
    /// The product has no body representation (or none at all).
    NoRepresentation,
    /// Its placement chain contains something this reader does not resolve (see the report).
    PlacementUnresolved,
}

/// One property or quantity as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    /// Name of the property set or quantity set.
    pub set: String,
    pub name: String,
    pub value: String,
    /// The IFC value type (`IFCLABEL`, `IFCLENGTHMEASURE`, ...) or the quantity entity.
    pub value_type: String,
}

/// A product (`IfcProduct` subtype) of the file.
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceProduct {
    /// Instance id in the file (`#n`).
    pub step_id: u32,
    pub global_id: String,
    pub name: String,
    /// Upper case entity name, e.g. `IFCWALL`.
    pub entity: String,
    /// The entity's `PredefinedType` enumeration, when it has one and it is set.
    pub predefined_type: Option<String>,
    /// `IfcObject.ObjectType`.
    pub object_type: Option<String>,
    /// A spatial structure element (site, building, storey, space, facility, ...).
    pub is_spatial: bool,
    /// An `IfcOpeningElement` (a void, normally not shown).
    pub is_opening: bool,
    /// Triangle meshes in world millimetres, one per body item. Counter-clockwise seen from outside
    /// for solids; open for surface models.
    pub meshes: Vec<Mesh3>,
    /// Bounding box of the meshes, world mm: `(min, max)`.
    pub bounds: Option<([f64; 3], [f64; 3])>,
    pub geometry: GeometryStatus,
    /// Entity names of body items that were not read.
    pub unread_items: Vec<String>,
    /// `RepresentationType` of the body representation read (`Tessellation`, `SweptSolid`, ...).
    pub representation_type: Option<String>,
    /// The spatial structure node that contains it (`IfcRelContainedInSpatialStructure`).
    pub container: Option<usize>,
    /// The product it is aggregated into (`IfcRelAggregates`), as an index into `products`.
    pub parent: Option<usize>,
    pub properties: Vec<Property>,
}

impl ReferenceProduct {
    /// True when this product should take part in clash tests: it has meshes and is neither a
    /// spatial element nor an opening.
    pub fn clashable(&self) -> bool {
        !self.meshes.is_empty() && !self.is_spatial && !self.is_opening
    }
}

/// A node of the spatial structure tree (`IfcRelAggregates` and `IfcRelContainedInSpatialStructure`).
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialNode {
    pub step_id: u32,
    pub entity: String,
    pub name: String,
    pub global_id: String,
    /// Index into `ReferenceModel::products` when the node is a product.
    pub product: Option<usize>,
    /// Aggregated children (indices into `ReferenceModel::tree`).
    pub children: Vec<usize>,
    /// Products contained in this spatial element (indices into `ReferenceModel::products`).
    pub contained: Vec<usize>,
}

/// `IfcMapConversion` of the file's first representation context.
#[derive(Debug, Clone, PartialEq)]
pub struct MapConversion {
    pub eastings: f64,
    pub northings: f64,
    pub orthogonal_height: f64,
    pub x_axis_abscissa: f64,
    pub x_axis_ordinate: f64,
    pub scale: f64,
    /// Millimetres per unit of the eastings, northings and height (the target CRS map unit, or
    /// the project length unit when the CRS gives none).
    pub unit_mm: f64,
    /// `IfcProjectedCRS` of the target: `Name` (for example `EPSG:32736`) and the other attributes
    /// the file gives, as the file wrote them.
    pub crs_name: Option<String>,
    pub crs_description: Option<String>,
    pub geodetic_datum: Option<String>,
    pub vertical_datum: Option<String>,
    pub map_projection: Option<String>,
    pub map_zone: Option<String>,
}

/// Geometry coverage and everything the reader skipped.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportReport {
    /// First `FILE_SCHEMA` identifier.
    pub schema: String,
    pub file_bytes: usize,
    pub instances: usize,
    pub complex_instances: usize,
    /// Millimetres per project length unit, and the unit's name.
    pub length_unit_mm: f64,
    pub length_unit: String,
    /// Every `IfcProduct` of the file, spatial elements included.
    pub products: usize,
    /// Spatial structure elements (site, building, storey, space, facility, ...). The coverage
    /// counters below leave them out: they rarely carry body geometry and are not clash material.
    pub products_spatial: usize,
    /// Per entity type: products, and how many of them have meshes.
    pub by_type: BTreeMap<String, TypeCoverage>,
    pub products_read: usize,
    pub products_partial: usize,
    pub products_placeholder: usize,
    pub products_not_read: usize,
    pub products_no_representation: usize,
    pub products_placement_unresolved: usize,
    /// Per `RepresentationType` of the body representation: products fully or partly read, and
    /// products with nothing read.
    pub by_representation: BTreeMap<String, RepresentationCoverage>,
    /// Geometry items read, by entity type.
    pub items_read: BTreeMap<String, usize>,
    /// "Geometry not read": items skipped, by entity type (every use counts).
    pub items_not_read: BTreeMap<String, usize>,
    /// Placement entities that were not resolved, by entity type.
    pub placements_unresolved: BTreeMap<String, usize>,
    /// `IfcLinearPlacement`s evaluated on their basis curve (each placement once).
    pub linear_placements_resolved: usize,
    /// `IfcLinearPlacement`s whose basis curve was not understood and whose `CartesianPosition`
    /// fallback was used instead.
    pub linear_placements_fallback: usize,
    /// Property types not read, by entity type.
    pub properties_not_read: BTreeMap<String, usize>,
    pub triangles: usize,
    /// Faces whose triangulation did not add up to outline minus holes, counted over every use of
    /// every item (see the warning).
    pub faces_inconsistent: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TypeCoverage {
    pub products: usize,
    pub with_geometry: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RepresentationCoverage {
    pub read: usize,
    pub not_read: usize,
}

/// A file read for reference.
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceModel {
    pub schema: String,
    pub project_name: String,
    pub products: Vec<ReferenceProduct>,
    pub tree: Vec<SpatialNode>,
    /// Tree nodes with no parent (normally the `IfcProject`).
    pub roots: Vec<usize>,
    pub map_conversion: Option<MapConversion>,
    pub report: ImportReport,
}

impl ReferenceModel {
    /// Union of the bounds of all products with meshes (world mm).
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        self.products.iter().filter_map(|p| p.bounds).reduce(|a, b| ([a.0[0].min(b.0[0]), a.0[1].min(b.0[1]), a.0[2].min(b.0[2])], [a.1[0].max(b.1[0]), a.1[1].max(b.1[1]), a.1[2].max(b.1[2])]))
    }

    /// Indices of the products that should take part in clash tests.
    pub fn clashable(&self) -> Vec<usize> {
        (0..self.products.len()).filter(|&i| self.products[i].clashable()).collect()
    }

    pub fn find_global_id(&self, guid: &str) -> Option<&ReferenceProduct> {
        self.products.iter().find(|p| p.global_id == guid)
    }
}

/// Reads an IFC file from disk.
pub fn import_ifc(path: &Path, options: &ImportOptions) -> Result<ReferenceModel, ImportError> {
    let data = std::fs::read(path).map_err(|e| ImportError::Io(format!("{}: {e}", path.display())))?;
    import_ifc_bytes(data, options)
}

/// Reads an IFC file held in memory (the buffer is kept while importing).
pub fn import_ifc_bytes(data: Vec<u8>, options: &ImportOptions) -> Result<ReferenceModel, ImportError> {
    let file = StepFile::parse(data)?;
    reader::import(&file, options)
}
