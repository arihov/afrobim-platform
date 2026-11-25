// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The model export: signs with their posts and foundations, where they stand on site, and the
//! alignments they were placed from.
//!
//! [`export_ifc_model`] takes the same input as [`crate::export_ifc`] and adds what that function
//! leaves out:
//!
//! * posts as `IfcMember` (`POST`) and foundations as `IfcFooting` (`PAD_FOOTING`), with swept
//!   solids built from their definitions;
//! * a sign and its supports as one `IfcElementAssembly` (`SIGNALASSEMBLY`);
//! * `IfcLocalPlacement`s from each element's `SitePlacement`, inside an `IfcRoad`;
//! * alignments as `IfcAlignment` with their horizontal layout;
//! * the road modules' elements, in `road.rs`.
//!
//! A document with none of these gives the same text as [`crate::export_ifc`].

mod alignment;
mod road;

use std::collections::{BTreeMap, BTreeSet};

use afrobim_alignment::{wrap_pi, Alignment, ElementKind};
use afrobim_draw::{Colour, Mesh3};
use afrobim_kernel::{Document, Element, ElementId, SitePlacement};
use afrobim_supports::{
    FoundationDefinition, FoundationShape, PostDefinition, Section, Socket, PROP_MOUNTING_HEIGHT_MM, PROP_POSITION_ALONG_SIGN_MM, PROP_SETBACK_MM,
};

use crate::rot;
use crate::step::{list, opt_string, reals, refs, step_fine, step_measure};
use crate::{clip, element_global_id, step_real, step_string, IfcElementInput, IfcExport, IfcOptions, Plan, Value, Writer, SIGN_CATEGORY};

/// Category of the elements exported as `IfcMember` (`POST`).
pub const POST_CATEGORY: &str = afrobim_supports::POST_CATEGORY;
/// Category of the elements exported as `IfcFooting` (`PAD_FOOTING`).
pub const FOUNDATION_CATEGORY: &str = afrobim_supports::FOUNDATION_CATEGORY;
pub use road::{
    entity_ok, CivilGroup, CivilPackage, CivilPart, CivilSet, CivilValue, GroupKind, CIVIL_CATEGORY, ENTITIES, QUANTITY_SETS, SYSTEM_TYPES, ext_feature_type, FeatureType, MARKING_EXT_CATEGORY, CORRIDOR_CATEGORY, CULVERT_CATEGORY, DRAIN_CATEGORY, GANTRY_CATEGORY, LIGHTING_CATEGORY, MARKING_CATEGORY, PAVEMENT_CATEGORY, RESTRAINT_CATEGORY, SURFACE_CATEGORY,
};

/// Category of the elements exported as `IfcAlignment`.
pub const ALIGNMENT_CATEGORY: &str = afrobim_alignment::CATEGORY;
/// The element property that holds the free text describing what a support was designed to.
/// It is copied, unchanged, to `DesignBasis` in the support's and the assembly's property sets.
pub const PROP_DESIGN_BASIS: &str = "design_basis";
/// Longest chord of the polyline that represents an alignment's curves and spirals, metres.
pub const ALIGNMENT_CHORD_M: f64 = 1.0;

/// Writes the model as IFC. Same as [`export_ifc_model`] without the notes.
pub fn write_ifc_model(document: &Document, elements: &[IfcElementInput], options: &IfcOptions) -> String {
    export_ifc_model(document, elements, options).text
}

/// Writes the IFC file for `elements` (normally every element of `document`, each with what its
/// module generated for it) with supports, site placement and alignments, and reports what was
/// left out or approximated.
///
/// * A sign with posts or foundations (elements whose chain of `host`s leads to it) becomes an
///   `IfcElementAssembly` that aggregates the `IfcSign`, the `IfcMember`s and the `IfcFooting`s.
/// * An element with a `SitePlacement` stands at that position and rotation; one without keeps
///   the upright-sheet position [`crate::export_ifc`] gives it. As soon as one element is placed
///   on site, or an alignment is exported, the elements are contained in an `IfcRoad`.
/// * An element of category `alignment` becomes an `IfcAlignment`.
///
/// The output depends only on the arguments. For a document without supports, site placements
/// and alignments it is byte-identical to that of [`crate::export_ifc`].
pub fn export_ifc_model(document: &Document, elements: &[IfcElementInput], options: &IfcOptions) -> IfcExport {
    // The first occurrence of every element, by id.
    let mut by_id: BTreeMap<ElementId, &IfcElementInput> = BTreeMap::new();
    for input in elements {
        by_id.entry(input.element.id).or_insert(input);
    }
    let is_support = |e: &Element| e.category == POST_CATEGORY || e.category == FOUNDATION_CATEGORY;

    // Which sign each support belongs to: its chain of hosts, as far as the elements given.
    let mut supports: BTreeMap<ElementId, Vec<ElementId>> = BTreeMap::new();
    let mut sign_of: BTreeMap<ElementId, ElementId> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for input in elements {
        let e = input.element;
        if !seen.insert(e.id) || !is_support(e) {
            continue;
        }
        let mut host = e.host;
        for _ in 0..8 {
            let Some(h) = host.and_then(|id| by_id.get(&id)) else { break };
            if h.element.category == SIGN_CATEGORY {
                supports.entry(h.element.id).or_default().push(e.id);
                sign_of.insert(e.id, h.element.id);
                break;
            }
            if !is_support(h.element) {
                break;
            }
            host = h.element.host;
        }
    }

    // Signs hung on a gantry are parts of the gantry's assembly, written with it.
    let mut hung: BTreeSet<ElementId> = BTreeSet::new();
    for input in elements {
        let e = input.element;
        let on_gantry = e.host.and_then(|id| by_id.get(&id)).is_some_and(|h| h.element.category == road::GANTRY_CATEGORY);
        if e.category == SIGN_CATEGORY && on_gantry && valid_site(e).is_some() && !supports.contains_key(&e.id) {
            hung.insert(e.id);
        }
    }

    let alignments: BTreeMap<ElementId, Alignment> = by_id
        .values()
        .filter(|i| i.element.category == ALIGNMENT_CATEGORY)
        .filter_map(|i| serde_json::from_value::<Alignment>(i.element.data.clone()).ok().map(|a| (i.element.id, a)))
        .filter(|(_, a)| alignment_problem(a).is_none())
        .collect();
    let placed = by_id.values().any(|i| i.element.category != ALIGNMENT_CATEGORY && valid_site(i.element).is_some());
    let on_road = by_id.values().any(|i| road::is_road_category(&i.element.category));
    // The local origin of a georeferenced file (see `IfcGeoreference`). Only when something has a
    // position on the site; a sheet of signs has nothing to place on a map.
    let mut alignments = alignments;
    let georef_origin_m = options.georeference.as_ref().and_then(|g| {
        if let Some(o) = g.origin {
            return Some(o);
        }
        let (mut min_e, mut min_n) = (f64::INFINITY, f64::INFINITY);
        for i in by_id.values() {
            if i.element.category == ALIGNMENT_CATEGORY {
                continue;
            }
            if let Some(s) = valid_site(i.element) {
                min_e = min_e.min(s.x / 1000.0);
                min_n = min_n.min(s.y / 1000.0);
            }
        }
        for a in alignments.values() {
            for el in &a.elements {
                min_e = min_e.min(el.x);
                min_n = min_n.min(el.y);
            }
        }
        (min_e.is_finite() && min_n.is_finite()).then(|| [(min_e / 1000.0).floor() * 1000.0, (min_n / 1000.0).floor() * 1000.0, 0.0])
    });
    if let Some(o) = georef_origin_m {
        // Alignments are written in the file's coordinates: move them by the same origin.
        for a in alignments.values_mut() {
            for el in &mut a.elements {
                el.x -= o[0];
                el.y -= o[1];
            }
        }
    }
    let plan = Plan { road: placed || !alignments.is_empty() || on_road, volume_unit: on_road || by_id.values().any(|i| is_support(i.element)), georef_origin_m };

    let mut m = Model {
        w: Writer::new(options, plan),
        document,
        by_id: &by_id,
        materials: BTreeMap::new(),
        positioned: Vec::new(),
        axis_context: None,
        road: road::RoadState::default(),
        geo: Vec::new(),
        geo_by_name: BTreeMap::new(),
        geo_of_element: BTreeMap::new(),
        footprint_context: None,
        enumerations: BTreeMap::new(),
        extra_notes: Vec::new(),
        linear_count: 0,
    };
    m.road.hung = hung.clone();
    m.prepare_road(elements);
    let mut notes = Vec::new();
    if options.alignment.is_full() {
        // The alignments come first: what is placed on them refers to their curves.
        let mut written = BTreeSet::new();
        for input in elements {
            let e = input.element;
            if e.category == ALIGNMENT_CATEGORY && written.insert(e.id) {
                if let Some(a) = alignments.get(&e.id) {
                    let index = m.alignment_full(e, a, &mut notes);
                    m.geo_of_element.insert(e.id, index);
                }
            }
        }
    }
    let mut roots = Vec::new();
    let mut aligned = Vec::new();
    let mut unplaced = 0usize;
    let mut seen = BTreeSet::new();
    for input in elements {
        let e = input.element;
        if !seen.insert(e.id) {
            notes.push(format!("{}: element id {} given more than once; exported once", e.name, e.id.0));
            continue;
        }
        if e.category == ALIGNMENT_CATEGORY {
            match alignments.get(&e.id) {
                Some(a) => aligned.push((input, a)),
                None => {
                    let why = serde_json::from_value::<Alignment>(e.data.clone()).map_or_else(|error| error.to_string(), |a| alignment_problem(&a).unwrap_or_default());
                    notes.push(format!("{}: alignment not exported, {why}", e.name));
                }
            }
            continue;
        }
        if sign_of.contains_key(&e.id) || hung.contains(&e.id) {
            continue; // written with its sign, or with its gantry
        }
        if road::is_road_category(&e.category) {
            roots.extend(m.road_element(input, &mut notes));
            continue;
        }
        if valid_site(e).is_none() {
            unplaced += 1;
            if e.site.is_some() {
                notes.push(format!("{}: the site placement is not a finite position; exported at its sheet position", e.name));
            }
        }
        let root = match supports.get(&e.id) {
            Some(parts) => m.assembly(input, parts, &mut notes),
            None if is_support(e) => {
                notes.push(format!("{}: no sign among the exported elements hosts this {}; exported on its own", e.name, e.category));
                let pose = valid_site(e).map_or(Pose::sheet(e.origin.x, e.origin.y), |s| Pose::site(s, 0.0));
                let placement = m.root_placement(e, pose);
                let product = m.support(input, placement, None, &mut notes);
                m.site_properties(product, &format!("element/{}", e.id.0), e);
                product
            }
            None => {
                let pose = valid_site(e).map_or(Pose::sheet(e.origin.x, e.origin.y), |s| Pose::site(s, 0.0));
                let placement = m.root_placement(e, pose);
                let product = m.w.product(document, input, placement, &mut notes);
                m.site_properties(product, &format!("element/{}", e.id.0), e);
                product
            }
        };
        if let Some(name) = valid_site(e).and_then(|s| s.alignment.clone()) {
            m.positioned.push((root, name));
        }
        m.w.frame = None;
        roots.push(root);
    }
    if placed && unplaced > 0 {
        notes.push(format!("{unplaced} element(s) have no site placement and stand at their sheet position, upright in the plane Y = 0"));
    }
    m.finish_road();
    if !roots.is_empty() {
        m.w.contain(&roots);
    }
    m.write_materials();
    m.write_alignments(&aligned, &mut notes);
    notes.append(&mut m.extra_notes);
    if m.linear_count > 0 {
        notes.push(format!(
            "{} element(s) are placed by IfcLinearPlacement on their alignment, with the absolute placement as CartesianPosition; the rest keep absolute placements",
            m.linear_count
        ));
    }
    IfcExport { text: m.w.finish(), notes }
}

/// The element's site placement when every number in it is finite.
fn valid_site(e: &Element) -> Option<&SitePlacement> {
    e.site.as_ref().filter(|s| s.x.is_finite() && s.y.is_finite() && s.z.is_finite() && s.rotation_deg.is_finite())
}

/// A number recorded as a property by `afrobim_supports::add_supports`.
fn number(e: &Element, key: &str) -> Option<f64> {
    e.properties.get(key).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

/// A length as people write it: no trailing zeros.
fn mm(v: f64) -> String {
    format!("{}", (v * 1e3).round() / 1e3)
}

/// Where a frame sits in the world: its origin (mm) and its turn about +Z, counter-clockwise
/// from +X. This is the convention of the 3D view: local (u along the width, v into the depth,
/// w up) maps to `origin + u·(cos, sin, 0) + v·(−sin, cos, 0) + w·(0, 0, 1)`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pose {
    x: f64,
    y: f64,
    z: f64,
    rotation_deg: f64,
}

impl Pose {
    /// A point of the design sheet stood upright: sheet X is world X, sheet Y is world Z.
    fn sheet(x: f64, y: f64) -> Pose {
        Pose { x, y: 0.0, z: y, rotation_deg: 0.0 }
    }

    /// A site placement, `lift` above its level.
    fn site(s: &SitePlacement, lift: f64) -> Pose {
        Pose { x: s.x, y: s.y, z: s.z + lift, rotation_deg: s.rotation_deg }
    }

    /// The frame moved by (u, v, w) along its own axes.
    fn offset(&self, u: f64, v: f64, w: f64) -> Pose {
        let (sin, cos) = self.rotation_deg.to_radians().sin_cos();
        Pose { x: self.x + u * cos - v * sin, y: self.y + u * sin + v * cos, z: self.z + w, rotation_deg: self.rotation_deg }
    }

    /// This frame expressed in the axes of `base`.
    fn relative_to(&self, base: &Pose) -> Pose {
        let (sin, cos) = base.rotation_deg.to_radians().sin_cos();
        let (dx, dy) = (self.x - base.x, self.y - base.y);
        Pose { x: dx * cos + dy * sin, y: -dx * sin + dy * cos, z: self.z - base.z, rotation_deg: self.rotation_deg - base.rotation_deg }
    }
}

/// One base quantity.
enum Quantity<'a> {
    /// mm.
    Length(&'a str, f64),
    /// m².
    Area(&'a str, f64),
    /// m³.
    Volume(&'a str, f64),
    /// A number of items.
    Count(&'a str, i64),
}

impl Writer<'_> {
    /// An `IfcAxis2Placement3D` at `pose`: the location, and a reference direction turned about
    /// +Z unless the turn is nil.
    fn axes_of(&mut self, pose: Pose) -> u32 {
        let location = self.data.add("IFCCARTESIANPOINT", &reals(&[pose.x, pose.y, pose.z]));
        let turn = pose.rotation_deg.rem_euclid(360.0);
        if !(1e-9..=360.0 - 1e-9).contains(&turn) {
            self.data.add("IFCAXIS2PLACEMENT3D", &format!("#{location},$,$"))
        } else {
            let (sin, cos) = turn.to_radians().sin_cos();
            let x = self.data.add("IFCDIRECTION", &format!("({},{},0.)", step_fine(cos), step_fine(sin)));
            self.data.add("IFCAXIS2PLACEMENT3D", &format!("#{location},#{},#{x}", self.up))
        }
    }

    /// An `IfcAxis2Placement3D` from a turn and a location: the Z axis and the reference
    /// direction are the third and first columns of the turn.
    fn axes_of_turn(&mut self, q: &rot::Rot, at: [f64; 3]) -> u32 {
        let location = self.data.add("IFCCARTESIANPOINT", &reals(&at));
        let (x, z) = (rot::column(q, 0), rot::column(q, 2));
        let axis = self.data.add("IFCDIRECTION", &format!("({},{},{})", step_fine(z[0]), step_fine(z[1]), step_fine(z[2])));
        let x = self.data.add("IFCDIRECTION", &format!("({},{},{})", step_fine(x[0]), step_fine(x[1]), step_fine(x[2])));
        self.data.add("IFCAXIS2PLACEMENT3D", &format!("#{location},#{axis},#{x}"))
    }

    /// An `IfcLocalPlacement` at `pose` in the axes of the placement `relative_to`.
    ///
    /// When `relative_to` is the placement of an element that was put on an alignment, `pose` is
    /// still the pose the part had in the element's old (absolute) frame; it is written in the
    /// frame the linear placement gives, so the part stays where it was.
    fn placement(&mut self, relative_to: u32, pose: Pose) -> u32 {
        let axes = match self.reframed.get(&relative_to).copied() {
            Some(q) => {
                // The element's own geometry has been written by now; what follows is its parts.
                self.frame = None;
                let turned = rot::mul(&q, &rot::about_z(pose.rotation_deg));
                self.axes_of_turn(&turned, rot::apply(&q, [pose.x, pose.y, pose.z]))
            }
            None => self.axes_of(pose),
        };
        self.data.add("IFCLOCALPLACEMENT", &format!("#{relative_to},#{axes}"))
    }

    /// A styled `IfcExtrudedAreaSolid`: `profile`, centred on (`cx`, `cy`), from `z0` upwards.
    fn extrusion(&mut self, profile: u32, cx: f64, cy: f64, z0: f64, depth: f64, style: u32) -> u32 {
        let position = match self.frame {
            Some(q) => self.axes_of_turn(&q, rot::apply(&q, [cx, cy, z0])),
            None => {
                let location = self.data.add("IFCCARTESIANPOINT", &reals(&[cx, cy, z0]));
                self.data.add("IFCAXIS2PLACEMENT3D", &format!("#{location},$,$"))
            }
        };
        let solid = self.data.add("IFCEXTRUDEDAREASOLID", &format!("#{profile},#{position},#{},{}", self.up, step_real(depth)));
        self.data.add("IFCSTYLEDITEM", &format!("#{solid},(#{style}),$"));
        solid
    }

    /// The Body representation of `items`.
    fn body(&mut self, kind: &str, items: &[u32]) -> u32 {
        let representation = self.data.add("IFCSHAPEREPRESENTATION", &format!("#{},'Body','{kind}',{}", self.body_context, refs(items)));
        self.data.add("IFCPRODUCTDEFINITIONSHAPE", &format!("$,$,(#{representation})"))
    }

    /// The display style of a solid called `name`.
    fn solid_style(&mut self, name: &str, rgb: [u8; 3]) -> u32 {
        self.style_keyed(&format!("solid/{name}/{},{},{}", rgb[0], rgb[1], rgb[2]), name, rgb)
    }

    /// The parameterized profile of a post section, centred on its bounding box.
    fn section_profile(&mut self, section: &Section) -> u32 {
        let name = step_string(&clip(&section.label()));
        let radius = |r: f64| if r > 0.0 { step_real(r) } else { "$".to_string() };
        match *section {
            Section::Chs { outside_diameter_mm: d, wall_mm: t } => {
                self.data.add("IFCCIRCLEHOLLOWPROFILEDEF", &format!(".AREA.,{name},$,{},{}", step_real(d / 2.0), step_real(t)))
            }
            Section::SolidRound { diameter_mm: d } => self.data.add("IFCCIRCLEPROFILEDEF", &format!(".AREA.,{name},$,{}", step_real(d / 2.0))),
            Section::Rhs { width_mm: b, depth_mm: d, wall_mm: t, outer_corner_radius_mm: ro, inner_corner_radius_mm: ri } => self.data.add(
                "IFCRECTANGLEHOLLOWPROFILEDEF",
                &format!(".AREA.,{name},$,{},{},{},{},{}", step_real(b), step_real(d), step_real(t), radius(ri), radius(ro)),
            ),
            Section::SolidRect { width_mm: b, depth_mm: d } => {
                self.data.add("IFCRECTANGLEPROFILEDEF", &format!(".AREA.,{name},$,{},{}", step_real(b), step_real(d)))
            }
            // Flanges parallel to X, the web along Y on the centre line: the IfcIShapeProfileDef frame.
            Section::ISection { depth_mm: d, flange_width_mm: b, web_mm: tw, flange_mm: tf } => self.data.add(
                "IFCISHAPEPROFILEDEF",
                &format!(".AREA.,{name},$,{},{},{},{},$,$,$", step_real(b), step_real(d), step_real(tw), step_real(tf)),
            ),
            // Flanges parallel to X, the web along Y on the low-X side: the IfcUShapeProfileDef frame.
            Section::Channel { depth_mm: d, flange_width_mm: b, web_mm: tw, flange_mm: tf } => self.data.add(
                "IFCUSHAPEPROFILEDEF",
                &format!(".AREA.,{name},$,{},{},{},{},$,$,$", step_real(d), step_real(b), step_real(tw), step_real(tf)),
            ),
        }
    }

    /// An element quantity set and its relationship to `product`.
    fn quantities(&mut self, product: u32, element: ElementId, name: &str, items: &[Quantity]) {
        self.quantities_of(product, &format!("element/{}", element.0), name, items);
    }

    /// The same for any object; `owner` scopes the GlobalIds of the set and its relationship.
    fn quantities_of(&mut self, product: u32, owner: &str, name: &str, items: &[Quantity]) {
        let ids: Vec<u32> = items
            .iter()
            .map(|q| match q {
                Quantity::Length(n, v) => self.data.add("IFCQUANTITYLENGTH", &format!("{},$,$,{},$", step_string(n), step_real(*v))),
                // Square and cubic metres need more than six decimals to hold a square millimetre, and
                // no more significant digits than are the same on every platform (see `step_measure`).
                Quantity::Area(n, v) => self.data.add("IFCQUANTITYAREA", &format!("{},$,$,{},$", step_string(n), step_measure(*v))),
                Quantity::Volume(n, v) => self.data.add("IFCQUANTITYVOLUME", &format!("{},$,$,{},$", step_string(n), step_measure(*v))),
                Quantity::Count(n, v) => self.data.add("IFCQUANTITYCOUNT", &format!("{},$,$,{v},$", step_string(n))),
            })
            .collect();
        let guid = step_string(&self.guid(&format!("{owner}/qto/{name}")));
        let set = self.data.add("IFCELEMENTQUANTITY", &format!("{guid},$,{},$,$,{}", step_string(name), refs(&ids)));
        self.define_of(product, owner, name, set);
    }
}

/// True when every directed edge of the mesh is matched by its reverse: a closed shell.
fn is_closed(mesh: &Mesh3) -> bool {
    let mut edges = BTreeSet::new();
    for t in &mesh.triangles {
        for k in 0..3 {
            if !edges.insert((t[k], t[(k + 1) % 3])) {
                return false;
            }
        }
    }
    !edges.is_empty() && edges.iter().all(|(a, b)| edges.contains(&(*b, *a)))
}

/// Why an alignment cannot be exported, if it cannot.
fn alignment_problem(a: &Alignment) -> Option<String> {
    if a.elements.is_empty() {
        return Some("it has no elements".to_string());
    }
    if !a.start_station_m.is_finite() {
        return Some("its start station is not a number".to_string());
    }
    for (i, e) in a.elements.iter().enumerate() {
        let finite = [e.x, e.y, e.heading_rad, e.length, e.curvature_start, e.curvature_end].iter().all(|v| v.is_finite());
        if !finite || e.length <= 0.0 {
            return Some(format!("element {} has a length or coordinate that is not a positive, finite number", i + 1));
        }
        if e.curvature_start * e.curvature_end < 0.0 {
            return Some(format!("element {} is a spiral that changes hand, which this exporter does not write", i + 1));
        }
    }
    None
}

struct Model<'a> {
    w: Writer<'a>,
    document: &'a Document,
    by_id: &'a BTreeMap<ElementId, &'a IfcElementInput<'a>>,
    /// Material name → the products made of it.
    materials: BTreeMap<String, Vec<u32>>,
    /// Root products and the name of the alignment their position was taken from.
    positioned: Vec<(u32, String)>,
    /// The 'Axis' subcontext, created with the first alignment.
    axis_context: Option<u32>,
    /// What the road modules' elements leave for the end of the export.
    road: road::RoadState,
    /// The alignments written in full (`AlignmentExport::Geometry` and `::Linear`), by element
    /// name and by alignment name, and the 'FootPrint' sub-context they share.
    geo: Vec<alignment::AlignGeo>,
    geo_by_name: BTreeMap<String, usize>,
    geo_of_element: BTreeMap<ElementId, usize>,
    footprint_context: Option<u32>,
    /// `IfcPropertyEnumeration`s written, by name.
    enumerations: BTreeMap<String, u32>,
    /// Remarks that arise while elements are placed, for the export's notes.
    extra_notes: Vec<String>,
    /// Elements placed by an `IfcLinearPlacement`.
    linear_count: usize,
}

impl Model<'_> {
    /// `AfroBIM_SitePlacement` on `product`, when the element stands on site.
    fn site_properties(&mut self, product: u32, owner: &str, e: &Element) {
        let Some(s) = valid_site(e) else { return };
        let mut p = vec![
            ("X".to_string(), Value::Length(s.x)),
            ("Y".to_string(), Value::Length(s.y)),
            ("Z".to_string(), Value::Length(s.z)),
            ("RotationDeg".to_string(), Value::Real(s.rotation_deg)),
        ];
        if let Some(a) = s.alignment.as_ref().filter(|a| !a.is_empty()) {
            p.push(("Alignment".to_string(), Value::Text(a.clone())));
        }
        if let Some(v) = s.station_m.filter(|v| v.is_finite()) {
            p.push(("StationM".to_string(), Value::Real(v)));
        }
        if let Some(v) = s.offset_m.filter(|v| v.is_finite()) {
            p.push(("OffsetM".to_string(), Value::Real(v)));
        }
        self.w.property_set_of(product, owner, "AfroBIM_SitePlacement", &p);
    }

    /// The mounting height recorded on a sign's first post, or 0.
    fn mounting_height(&self, sign: ElementId, parts: &[ElementId]) -> f64 {
        parts
            .iter()
            .filter_map(|id| self.by_id.get(id))
            .map(|i| i.element)
            .find(|p| p.category == POST_CATEGORY && p.host == Some(sign))
            .and_then(|p| number(p, PROP_MOUNTING_HEIGHT_MM))
            .unwrap_or(0.0)
    }

    /// Plan size of a support's bounding box (along X, along Y), from its definition or, failing
    /// that, its solids.
    fn plan_size(input: &IfcElementInput) -> Option<(f64, f64)> {
        let e = input.element;
        if e.category == POST_CATEGORY {
            if let Some(d) = PostDefinition::from_value(&e.data).ok().filter(|d| d.check().is_ok()) {
                return Some(d.section.bounding_mm());
            }
        } else if let Some(d) = FoundationDefinition::from_value(&e.data).ok().filter(|d| d.check().is_ok()) {
            return Some(d.shape.plan_mm());
        }
        let mut size: Option<(f64, f64)> = None;
        for v in input.generated.iter().flat_map(|g| g.solids.iter()).flat_map(|m| m.vertices.iter()).filter(|v| v.iter().all(|c| c.is_finite())) {
            let (x, y) = size.unwrap_or((0.0, 0.0));
            size = Some((x.max(v[0]), y.max(v[1])));
        }
        size
    }

    /// Where a support stands: its own site placement, else under its sign from the position
    /// and setback recorded on its post (the frame the 3D view uses on the upright sheet).
    fn support_pose(&self, input: &IfcElementInput, ground: &Pose) -> Option<Pose> {
        let e = input.element;
        if let Some(s) = valid_site(e) {
            return Some(Pose::site(s, 0.0));
        }
        let post = if e.category == POST_CATEGORY { e } else { self.by_id.get(&e.host?)?.element };
        let u = number(e, PROP_POSITION_ALONG_SIGN_MM).or_else(|| number(post, PROP_POSITION_ALONG_SIGN_MM))?;
        let setback = number(post, PROP_SETBACK_MM)?;
        let (w, d) = Self::plan_size(input)?;
        Some(ground.offset(u - w / 2.0, setback - d / 2.0, 0.0))
    }

    /// A sign with its posts and foundations: an `IfcElementAssembly` at the point on the ground
    /// under the sign's lower-left corner, aggregating the sign and every support.
    fn assembly(&mut self, input: &IfcElementInput, parts: &[ElementId], notes: &mut Vec<String>) -> u32 {
        let e = input.element;
        let mounting = self.mounting_height(e.id, parts);
        // A sign's site level is the ground under it; its face starts one mounting height up. On
        // the sheet the face keeps its place, so the ground is one mounting height below it.
        let ground = valid_site(e).map_or(Pose::sheet(e.origin.x, e.origin.y - mounting), |s| Pose::site(s, 0.0));
        let placement = self.root_placement(e, ground);
        let part_inputs: Vec<&IfcElementInput> = parts.iter().filter_map(|id| self.by_id.get(id).copied()).collect();
        let posts = part_inputs.iter().filter(|p| p.element.category == POST_CATEGORY).count();
        let foundations = part_inputs.len() - posts;
        let owner = format!("element/{}/assembly", e.id.0);
        let description = format!("Sign with {posts} post(s) and {foundations} foundation(s)");
        // SIGNALASSEMBLY: "an assembly to physically aggregate together one or more signal
        // instances (and also sign instances) including any supporting structural elements such
        // as a simple pole" (IfcElementAssemblyTypeEnum, IFC 4.3).
        let assembly = self.w.data.add(
            "IFCELEMENTASSEMBLY",
            &format!(
                "{},$,{},{},$,#{placement},$,{},$,.SIGNALASSEMBLY.",
                step_string(&self.w.guid(&owner)),
                step_string(&clip(&e.name)),
                step_string(&description),
                step_string(&e.id.0.to_string()),
            ),
        );
        let mut identity = vec![
            ("Mark".to_string(), Value::Text(e.name.clone())),
            ("Posts".to_string(), Value::Integer(posts as i64)),
            ("Foundations".to_string(), Value::Integer(foundations as i64)),
            ("MountingHeight".to_string(), Value::Length(mounting)),
        ];
        // The design basis of the supports, when they all state the same one.
        let bases: BTreeSet<&String> = part_inputs.iter().filter_map(|p| p.element.properties.get(PROP_DESIGN_BASIS)).filter(|b| !b.is_empty()).collect();
        if bases.len() == 1 && part_inputs.iter().all(|p| p.element.properties.get(PROP_DESIGN_BASIS).is_some_and(|b| !b.is_empty())) {
            identity.push(("DesignBasis".to_string(), Value::Text(bases.iter().next().map(|b| b.to_string()).unwrap_or_default())));
        }
        self.w.property_set_of(assembly, &owner, "AfroBIM_SignAssembly", &identity);
        self.site_properties(assembly, &owner, e);

        let sign_placement = self.w.placement(placement, Pose { x: 0.0, y: 0.0, z: mounting, rotation_deg: 0.0 });
        let sign = self.w.product(self.document, input, sign_placement, notes);
        self.site_properties(sign, &format!("element/{}", e.id.0), e);
        let mut members = vec![sign];
        for part in part_inputs {
            let p = part.element;
            let pose = self.support_pose(part, &ground).unwrap_or_else(|| {
                notes.push(format!("{}: its position under {} is not recorded; exported at its sheet position", p.name, e.name));
                Pose::sheet(p.origin.x, p.origin.y)
            });
            let part_placement = self.w.placement(placement, pose.relative_to(&ground));
            let product = self.support(part, part_placement, Some(e), notes);
            self.site_properties(product, &format!("element/{}", p.id.0), p);
            members.push(product);
        }
        let guid = step_string(&self.w.guid(&format!("{owner}/rel-aggregates")));
        self.w.data.add("IFCRELAGGREGATES", &format!("{guid},$,$,$,#{assembly},{}", refs(&members)));
        assembly
    }

    /// A post or a foundation at `placement`.
    fn support(&mut self, input: &IfcElementInput, placement: u32, sign: Option<&Element>, notes: &mut Vec<String>) -> u32 {
        if input.element.category == POST_CATEGORY {
            self.post(input, placement, sign, notes)
        } else {
            self.foundation(input, placement, sign, notes)
        }
    }

    /// The colour the module gave the solid called `name`, or `fallback`.
    fn solid_colour(input: &IfcElementInput, name: &str, fallback: Colour) -> [u8; 3] {
        input.generated.iter().flat_map(|g| g.solids.iter()).find(|m| m.name == name).map_or(fallback.display_rgb(), |m| m.colour)
    }

    /// The module's solids as closed (or open) triangulated face sets: the Body of a support
    /// whose definition this crate cannot read.
    fn tessellated(&mut self, input: &IfcElementInput, notes: &mut Vec<String>) -> Option<u32> {
        let e = input.element;
        let mut items = Vec::new();
        for mesh in input.generated.iter().flat_map(|g| g.solids.iter()) {
            let n = mesh.vertices.len() as u32;
            let sound = !mesh.triangles.is_empty()
                && mesh.triangles.iter().all(|t| t.iter().all(|i| *i < n))
                && mesh.vertices.iter().all(|v| v.iter().all(|c| c.is_finite()));
            if !sound {
                notes.push(format!("{}: solid '{}' is not a sound triangle mesh; not exported", e.name, mesh.name));
                continue;
            }
            let style = self.w.solid_style(&mesh.name, mesh.colour);
            items.push(self.w.face_set(&mesh.vertices, &mesh.triangles, is_closed(mesh), style));
        }
        if items.is_empty() {
            notes.push(format!("{}: no geometry exported, its definition cannot be read and it has no solids", e.name));
            return None;
        }
        notes.push(format!("{}: its definition cannot be read; the module's solids are exported as a tessellation", e.name));
        Some(self.w.body("Tessellation", &items))
    }

    /// Identity properties every support has.
    fn support_identity(&self, e: &Element, sign: Option<&Element>) -> Vec<(String, Value)> {
        let mut p = vec![
            ("Mark".to_string(), Value::Text(e.name.clone())),
            ("Category".to_string(), Value::Text(e.category.clone())),
            ("Layer".to_string(), Value::Text(e.layer.clone())),
        ];
        if let Some(s) = sign {
            p.push(("SignMark".to_string(), Value::Text(s.name.clone())));
        }
        if let Some(host) = e.host.and_then(|id| self.by_id.get(&id)).filter(|h| Some(h.element.id) != sign.map(|s| s.id)) {
            p.push(("HostMark".to_string(), Value::Text(host.element.name.clone())));
        }
        p
    }

    /// What follows the identity on every support: the design basis and the general sets.
    fn support_sets(&mut self, product: u32, input: &IfcElementInput, set: &str, mut identity: Vec<(String, Value)>) {
        let e = input.element;
        if let Some(basis) = e.properties.get(PROP_DESIGN_BASIS).filter(|b| !b.is_empty()) {
            identity.push(("DesignBasis".to_string(), Value::Text(basis.clone())));
        }
        self.w.property_set(product, e.id, set, &identity);
        let text = |map: &BTreeMap<String, String>| map.iter().map(|(k, v)| (k.clone(), Value::Text(v.clone()))).collect::<Vec<_>>();
        self.w.property_set(product, e.id, "AfroBIM_Properties", &text(&e.properties));
        if let Some(generated) = input.generated {
            self.w.property_set(product, e.id, "AfroBIM_Derived", &text(&generated.derived));
        }
    }

    /// A post: `IfcMember` with `PredefinedType` `POST`, its section extruded over its length.
    fn post(&mut self, input: &IfcElementInput, placement: u32, sign: Option<&Element>, notes: &mut Vec<String>) -> u32 {
        let e = input.element;
        let def = PostDefinition::from_value(&e.data).ok().filter(|d| d.check().is_ok());
        let shape = match &def {
            Some(d) => {
                let (w, depth) = d.section.bounding_mm();
                let profile = self.w.section_profile(&d.section);
                let style = self.w.solid_style("Post", Self::solid_colour(input, "Post", Colour::Grey));
                // z = 0 is ground level: the post runs from −embedment to length − embedment.
                let solid = self.w.extrusion(profile, w / 2.0, depth / 2.0, -d.embedment_mm, d.length_mm, style);
                Some(self.w.body("SweptSolid", &[solid]))
            }
            None => self.tessellated(input, notes),
        };
        let description = def.as_ref().map(|d| format!("{}, length {} mm, {} mm below ground", d.section.label(), mm(d.length_mm), mm(d.embedment_mm)));
        let product = self.w.data.add(
            "IFCMEMBER",
            &format!(
                "{},$,{},{},$,#{placement},{},{},.POST.",
                step_string(&element_global_id(&self.w.options.project_name, e.id)),
                step_string(&clip(&e.name)),
                opt_string(description.as_deref().map(clip).as_deref()),
                shape.map_or("$".to_string(), |s| format!("#{s}")),
                step_string(&e.id.0.to_string()),
            ),
        );
        let mut identity = self.support_identity(e, sign);
        if let Some(d) = &def {
            identity.push(("Section".to_string(), Value::Text(d.section.label())));
            identity.push(("Length".to_string(), Value::Length(d.length_mm)));
            identity.push(("Embedment".to_string(), Value::Length(d.embedment_mm)));
            identity.push(("AboveGround".to_string(), Value::Length(d.above_ground_mm())));
            if !d.material.trim().is_empty() {
                identity.push(("Material".to_string(), Value::Text(d.material.clone())));
                self.materials.entry(d.material.trim().to_string()).or_default().push(product);
            }
            if !d.finish.trim().is_empty() {
                identity.push(("Finish".to_string(), Value::Text(d.finish.clone())));
            }
            if d.placeholder {
                identity.push(("PlaceholderDimensions".to_string(), Value::Boolean(true)));
                notes.push(format!("{}: its dimensions are placeholders, not design values", e.name));
            }
        }
        self.support_sets(product, input, "AfroBIM_Post", identity);
        // Pset_MemberCommon (IFC 4.3): only IsExternal has a value here; a sign post stands outdoors.
        self.w.property_set(product, e.id, "Pset_MemberCommon", &[("IsExternal".to_string(), Value::Boolean(true))]);
        if let Some(d) = &def {
            // Qto_MemberBaseQuantities (IFC 4.3). The areas and the volume are those of the
            // section as entered, which is also what the swept solid is.
            let area = d.section.area_mm2();
            let items = [
                Quantity::Length("Length", d.length_mm),
                Quantity::Area("CrossSectionArea", area / 1e6),
                Quantity::Area("OuterSurfaceArea", d.section.outer_perimeter_mm() * d.length_mm / 1e6),
                Quantity::Volume("NetVolume", area * d.length_mm / 1e9),
            ];
            self.w.quantities(product, e.id, "Qto_MemberBaseQuantities", &items);
        }
        product
    }

    /// A foundation: `IfcFooting` with `PredefinedType` `PAD_FOOTING`, its plan outline extruded
    /// over its depth, with the blinding as a second solid.
    fn foundation(&mut self, input: &IfcElementInput, placement: u32, sign: Option<&Element>, notes: &mut Vec<String>) -> u32 {
        let e = input.element;
        let def = FoundationDefinition::from_value(&e.data).ok().filter(|d| d.check().is_ok());
        let shape = match &def {
            Some(d) => {
                let (l, w) = d.shape.plan_mm();
                let name = step_string(&clip(&d.shape.label()));
                let profile = match d.shape {
                    FoundationShape::Block { .. } => self.w.data.add("IFCRECTANGLEPROFILEDEF", &format!(".AREA.,{name},$,{},{}", step_real(l), step_real(w))),
                    FoundationShape::Cylinder { diameter_mm, .. } => self.w.data.add("IFCCIRCLEPROFILEDEF", &format!(".AREA.,{name},$,{}", step_real(diameter_mm / 2.0))),
                };
                // z = 0 is ground level: the concrete hangs below its top.
                let bottom = -d.top_below_ground_mm - d.shape.depth_mm();
                let style = self.w.solid_style("Foundation", Self::solid_colour(input, "Foundation", Colour::Grey));
                let mut items = vec![self.w.extrusion(profile, l / 2.0, w / 2.0, bottom, d.shape.depth_mm(), style)];
                if let Some(b) = d.blinding_mm {
                    let style = self.w.solid_style("Blinding", Self::solid_colour(input, "Blinding", Colour::Brown));
                    items.push(self.w.extrusion(profile, l / 2.0, w / 2.0, bottom - b, b, style));
                }
                if d.socket.is_some() {
                    notes.push(format!("{}: the socket is not cut out of the solid; the net volume in the quantities deducts it", e.name));
                }
                Some(self.w.body("SweptSolid", &items))
            }
            None => self.tessellated(input, notes),
        };
        let description = def.as_ref().map(|d| format!("{}, top {} mm below ground", d.shape.label(), mm(d.top_below_ground_mm)));
        let product = self.w.data.add(
            "IFCFOOTING",
            &format!(
                "{},$,{},{},$,#{placement},{},{},.PAD_FOOTING.",
                step_string(&element_global_id(&self.w.options.project_name, e.id)),
                step_string(&clip(&e.name)),
                opt_string(description.as_deref().map(clip).as_deref()),
                shape.map_or("$".to_string(), |s| format!("#{s}")),
                step_string(&e.id.0.to_string()),
            ),
        );
        let mut identity = self.support_identity(e, sign);
        if let Some(d) = &def {
            identity.push(("Shape".to_string(), Value::Text(d.shape.label())));
            match d.shape {
                FoundationShape::Block { length_mm, width_mm, depth_mm } => {
                    identity.push(("Length".to_string(), Value::Length(length_mm)));
                    identity.push(("Width".to_string(), Value::Length(width_mm)));
                    identity.push(("Depth".to_string(), Value::Length(depth_mm)));
                }
                FoundationShape::Cylinder { diameter_mm, depth_mm } => {
                    identity.push(("Diameter".to_string(), Value::Length(diameter_mm)));
                    identity.push(("Depth".to_string(), Value::Length(depth_mm)));
                }
            }
            identity.push(("TopBelowGround".to_string(), Value::Length(d.top_below_ground_mm)));
            identity.push(("FormationDepth".to_string(), Value::Length(d.formation_depth_mm())));
            if let Some(b) = d.blinding_mm {
                identity.push(("BlindingThickness".to_string(), Value::Length(b)));
            }
            match d.socket {
                Some(Socket::Round { diameter_mm, depth_mm }) => {
                    identity.push(("Socket".to_string(), Value::Text(format!("round, diameter {} mm, depth {} mm", mm(diameter_mm), mm(depth_mm)))));
                }
                Some(Socket::Rect { length_mm, width_mm, depth_mm }) => {
                    identity.push(("Socket".to_string(), Value::Text(format!("rectangular, {} x {} mm, depth {} mm", mm(length_mm), mm(width_mm), mm(depth_mm)))));
                }
                None => {}
            }
            if !d.concrete.trim().is_empty() {
                identity.push(("Concrete".to_string(), Value::Text(d.concrete.clone())));
                self.materials.entry(d.concrete.trim().to_string()).or_default().push(product);
            }
            if d.placeholder {
                identity.push(("PlaceholderDimensions".to_string(), Value::Boolean(true)));
                notes.push(format!("{}: its dimensions are placeholders, not design values", e.name));
            }
        }
        self.support_sets(product, input, "AfroBIM_Foundation", identity);
        if let Some(d) = &def {
            // Qto_FootingBaseQuantities (IFC 4.3): the concrete body, without the blinding.
            let gross = d.shape.plan_area_mm2() * d.shape.depth_mm();
            let net = gross - d.socket.as_ref().map_or(0.0, Socket::volume_mm3);
            let mut items = Vec::new();
            if let FoundationShape::Block { length_mm, width_mm, .. } = d.shape {
                items.push(Quantity::Length("Length", length_mm));
                items.push(Quantity::Length("Width", width_mm));
            }
            items.push(Quantity::Length("Height", d.shape.depth_mm()));
            items.push(Quantity::Area("CrossSectionArea", d.shape.plan_area_mm2() / 1e6));
            items.push(Quantity::Volume("GrossVolume", gross / 1e9));
            items.push(Quantity::Volume("NetVolume", net / 1e9));
            self.w.quantities(product, e.id, "Qto_FootingBaseQuantities", &items);
        }
        product
    }

    /// One `IfcMaterial` per stated material text, associated with everything made of it.
    fn write_materials(&mut self) {
        for (name, products) in std::mem::take(&mut self.materials) {
            let material = self.w.data.add("IFCMATERIAL", &format!("{},$,$", step_string(&clip(&name))));
            let guid = step_string(&self.w.guid(&format!("material/{name}/rel")));
            self.w.data.add("IFCRELASSOCIATESMATERIAL", &format!("{guid},$,$,$,{},#{material}", refs(&products)));
        }
    }

    /// The alignments, aggregated by the project, and what each positions.
    fn write_alignments(&mut self, aligned: &[(&IfcElementInput, &Alignment)], notes: &mut Vec<String>) {
        if aligned.is_empty() {
            return;
        }
        let full = self.w.options.alignment.is_full();
        let ids: Vec<u32> = aligned
            .iter()
            .map(|(input, a)| match self.geo_of_element.get(&input.element.id) {
                Some(index) if full => self.geo[*index].ifc,
                _ => self.alignment(input.element, a, notes),
            })
            .collect();
        if full {
            self.reference_alignments(&ids);
        }
        // "Every IfcAlignment must be related to IfcProject using the IfcRelAggregates
        // relationship", and none is contained in a spatial element (IfcAlignment, IFC 4.3).
        let guid = step_string(&self.w.guid("rel-alignments"));
        self.w.data.add("IFCRELAGGREGATES", &format!("{guid},$,$,$,#{},{}", self.w.project, refs(&ids)));
        for ((input, a), id) in aligned.iter().zip(&ids) {
            let e = input.element;
            let products: Vec<u32> = self.positioned.iter().filter(|(_, name)| *name == e.name || *name == a.name).map(|(p, _)| *p).collect();
            if !products.is_empty() {
                let guid = step_string(&self.w.guid(&format!("element/{}/rel-positions", e.id.0)));
                self.w.data.add("IFCRELPOSITIONS", &format!("{guid},$,$,$,#{id},{}", refs(&products)));
            }
        }
    }

    /// One alignment: the `IfcAlignment` with a 2D polyline as its Axis, its horizontal layout
    /// as nested segments ending in the zero-length segment, and a referent at its start station.
    fn alignment(&mut self, e: &Element, a: &Alignment, notes: &mut Vec<String>) -> u32 {
        let owner = format!("element/{}", e.id.0);
        let context = match self.axis_context {
            Some(c) => c,
            None => {
                let c = self.w.data.add("IFCGEOMETRICREPRESENTATIONSUBCONTEXT", &format!("'Axis','Model',*,*,*,*,#{},$,.MODEL_VIEW.,$", self.w.context));
                self.axis_context = Some(c);
                c
            }
        };
        // Alignment coordinates are plan metres in the same system as the site placements.
        let origin = self.w.data.add("IFCCARTESIANPOINT", "(0.,0.,0.)");
        let axes = self.w.data.add("IFCAXIS2PLACEMENT3D", &format!("#{origin},$,$"));
        let placement = self.w.data.add("IFCLOCALPLACEMENT", &format!("$,#{axes}"));
        let points = a.polyline(ALIGNMENT_CHORD_M);
        let coords = self.w.data.add("IFCCARTESIANPOINTLIST2D", &format!("{},$", list(points.iter().map(|(x, y)| reals(&[x * 1000.0, y * 1000.0])))));
        let curve = self.w.data.add("IFCINDEXEDPOLYCURVE", &format!("#{coords},$,$"));
        let representation = self.w.data.add("IFCSHAPEREPRESENTATION", &format!("#{context},'Axis','Curve2D',(#{curve})"));
        let shape = self.w.data.add("IFCPRODUCTDEFINITIONSHAPE", &format!("$,$,(#{representation})"));
        let alignment = self.w.data.add(
            "IFCALIGNMENT",
            &format!("{},$,{},$,$,#{placement},#{shape},$", step_string(&element_global_id(&self.w.options.project_name, e.id)), step_string(&clip(&e.name))),
        );

        let horizontal = self.w.data.add("IFCALIGNMENTHORIZONTAL", &format!("{},$,'Horizontal',$,$,$,$", step_string(&self.w.guid(&format!("{owner}/horizontal")))));
        let guid = step_string(&self.w.guid(&format!("{owner}/rel-nests-layouts")));
        self.w.data.add("IFCRELNESTS", &format!("{guid},$,$,$,#{alignment},(#{horizontal})"));

        // Radius: 0 means infinite, positive turns left (counter-clockwise), in mm.
        let radius = |curvature: f64| if curvature == 0.0 { 0.0 } else { 1000.0 / curvature };
        let mut segments = Vec::new();
        let mut segment = |m: &mut Model, i: usize, x: f64, y: f64, heading: f64, r0: f64, r1: f64, length: f64, kind: &str| {
            let start = m.w.data.add("IFCCARTESIANPOINT", &reals(&[x * 1000.0, y * 1000.0]));
            let design = m.w.data.add(
                "IFCALIGNMENTHORIZONTALSEGMENT",
                &format!("$,$,#{start},{},{},{},{},$,.{kind}.", step_fine(wrap_pi(heading)), step_real(r0), step_real(r1), step_real(length * 1000.0)),
            );
            let guid = step_string(&m.w.guid(&format!("{owner}/horizontal/segment/{i}")));
            segments.push(m.w.data.add("IFCALIGNMENTSEGMENT", &format!("{guid},$,'H{}',$,$,$,$,#{design}", i + 1)));
        };
        for (i, el) in a.elements.iter().enumerate() {
            let (r0, r1, kind) = match el.kind() {
                ElementKind::Line => (0.0, 0.0, "LINE"),
                ElementKind::Arc => (radius(el.curvature_start), radius(el.curvature_start), "CIRCULARARC"),
                ElementKind::Spiral => (radius(el.curvature_start), radius(el.curvature_end), "CLOTHOID"),
            };
            segment(self, i, el.x, el.y, el.heading_rad, r0, r1, el.length, kind);
        }
        // Each layout ends with a segment of length 0 that states the end point and direction.
        let last = a.elements[a.elements.len() - 1];
        let (x, y, heading) = last.end();
        segment(self, a.elements.len(), x, y, heading, 0.0, 0.0, 0.0, "LINE");
        let guid = step_string(&self.w.guid(&format!("{owner}/rel-nests-segments")));
        self.w.data.add("IFCRELNESTS", &format!("{guid},$,$,$,#{horizontal},{}", refs(&segments)));

        // The first nested referent is the alignment's start station (Pset_Stationing, IFC 4.3).
        let first = a.elements[0];
        let start = Pose { x: first.x * 1000.0, y: first.y * 1000.0, z: 0.0, rotation_deg: first.heading_rad.to_degrees() };
        let referent_placement = self.w.placement(placement, start);
        let referent_owner = format!("{owner}/start-station");
        let referent = self.w.data.add(
            "IFCREFERENT",
            &format!(
                "{},$,{},$,$,#{referent_placement},$,.STATION.",
                step_string(&self.w.guid(&referent_owner)),
                step_string(&afrobim_alignment::format_station(a.start_station_m, 3))
            ),
        );
        self.w.property_set_of(referent, &referent_owner, "Pset_Stationing", &[("Station".to_string(), Value::Length(a.start_station_m * 1000.0))]);
        let guid = step_string(&self.w.guid(&format!("{owner}/rel-nests-referents")));
        self.w.data.add("IFCRELNESTS", &format!("{guid},$,$,$,#{alignment},(#{referent})"));

        let identity = vec![
            ("Mark".to_string(), Value::Text(e.name.clone())),
            ("Category".to_string(), Value::Text(e.category.clone())),
            ("Layer".to_string(), Value::Text(e.layer.clone())),
            ("StartStationM".to_string(), Value::Real(a.start_station_m)),
            ("EndStationM".to_string(), Value::Real(a.end_station_m())),
            ("LengthM".to_string(), Value::Real(a.length())),
            ("Elements".to_string(), Value::Integer(a.elements.len() as i64)),
        ];
        self.w.property_set(alignment, e.id, "AfroBIM_Alignment", &identity);
        let text: Vec<(String, Value)> = e.properties.iter().map(|(k, v)| (k.clone(), Value::Text(v.clone()))).collect();
        self.w.property_set(alignment, e.id, "AfroBIM_Properties", &text);

        let joints = a.discontinuities(1e-3, 1e-6);
        if !joints.is_empty() {
            notes.push(format!("{}: {} joint(s) between elements are not continuous (gap over 1 mm or a change of direction); exported as given", e.name, joints.len()));
        }
        let curved = a.elements.iter().filter(|el| el.kind() != ElementKind::Line).count();
        if curved > 0 {
            notes.push(format!("{}: the Axis is a polyline with chords of at most {ALIGNMENT_CHORD_M} m on {curved} curve(s) and spiral(s); the exact geometry is in the horizontal segments", e.name));
        }
        alignment
    }
}
