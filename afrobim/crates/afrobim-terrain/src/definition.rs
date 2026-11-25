// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The surface object: a name, a role, a source record, a status and an ordered definition.
//!
//! The definition is a list of operations, like the definition of a TIN surface in a civil
//! design package: rebuilding the surface means replaying the list on an empty surface. The
//! list is plain data (serde), so a document can store it and rebuild the surface later.
//!
//! Order matters in two ways, stated here once and repeated in the README:
//!
//! * Data operations (points, breaklines, boundaries, contour data, exclusions, add or delete
//!   point, raise or lower, paste) act on the data gathered *up to that point in the list*.
//! * Edits of the triangulation (delete triangle, swap edge) and the triangle filters (maximum
//!   edge length, boundaries) act on the triangulation of *all* the data, in list order among
//!   themselves, whatever their place among the data operations.

use afrobim_corridor::Tin;
use serde::{Deserialize, Serialize};

use crate::error::TerrainError;
use crate::triangulate::Crossing;

/// What a surface is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceRole {
    /// The ground as it is (survey, scan, photogrammetry).
    ExistingGround,
    /// The finished design surface.
    Design,
    /// The top of the subgrade.
    Subgrade,
    /// The formation level (top of the earthworks, under the pavement layers).
    Formation,
    /// Anything else; the text says what.
    Other(String),
}

/// Where the data of a surface came from.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceRecord {
    /// What the data is, in words (for example "topographic survey, total station").
    pub description: String,
    /// The file, document or drawing number the data was taken from.
    pub reference: String,
    /// Who produced the data.
    pub author: String,
    /// When the data was captured or issued, as text (ISO 8601 recommended).
    pub date: String,
    /// The coordinate system and vertical datum, as text (the engine does not interpret it).
    pub coordinate_system: String,
    /// Anything else worth keeping.
    pub notes: String,
}

/// Where a surface stands in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceStatus {
    /// Being built; not to be relied on.
    #[default]
    Draft,
    /// Complete and waiting for checking.
    ForReview,
    /// Checked and accepted for use.
    Approved,
    /// Replaced by a later surface; kept for the record.
    Superseded,
}

/// A surveyed or designed point: an identifier and its easting, northing and elevation, m.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfacePoint {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl SurfacePoint {
    pub fn new(id: impl Into<String>, x: f64, y: f64, z: f64) -> SurfacePoint {
        SurfacePoint { id: id.into(), x, y, z }
    }
}

/// How a breakline is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreaklineKind {
    /// A 3D line: its vertices are surface points with their own elevations and its segments
    /// are edges of the triangulation.
    Standard,
    /// A plan line whose own elevations are not trusted: each vertex takes its elevation from
    /// the surface built from the other data (the ground under it), and the line then acts as
    /// a standard breakline. See the README for how this crate reads the term.
    Proximity,
}

/// A line the triangulation must follow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Breakline {
    pub name: String,
    pub kind: BreaklineKind,
    /// Vertices `[x, y, z]`, m. For a proximity breakline z is ignored.
    pub vertices: Vec<[f64; 3]>,
}

/// What a boundary does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryKind {
    /// Keep only the triangles inside.
    Outer,
    /// Remove the triangles inside (a pond, a building footprint).
    Hide,
    /// Bring back every triangle inside, including ones an earlier hide or the maximum edge
    /// length removed.
    Show,
}

/// Which point to act on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointRef {
    /// Every loose point with this identifier.
    Id(String),
    /// Every loose point within `tolerance` metres in plan of (`x`, `y`).
    Near { x: f64, y: f64, tolerance: f64 },
}

/// The surface pasted into another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PasteSource {
    /// Another definition, rebuilt when this one is built.
    Definition(Box<SurfaceDefinition>),
    /// A finished surface.
    Tin(Tin),
}

/// One step of a definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Operation {
    /// Add a group of points (a survey file, a point group).
    AddPoints { group: String, points: Vec<SurfacePoint> },
    /// Add breaklines.
    AddBreaklines { lines: Vec<Breakline> },
    /// Add a boundary: a closed ring, `[x, y]` in metres, not repeating the first vertex.
    AddBoundary { name: String, kind: BoundaryKind, vertices: Vec<[f64; 2]> },
    /// Add contour lines as data: every vertex `[x, y, z]` becomes a point. The lines are not
    /// breaklines. `thin_distance` drops a vertex closer than that to the last one kept;
    /// `max_segment` adds vertices (elevation interpolated) so no segment is longer.
    AddContourData { name: String, lines: Vec<Vec<[f64; 3]>>, thin_distance: Option<f64>, max_segment: Option<f64> },
    /// Remove every triangle with a plan side longer than `length` metres. A later operation of
    /// this kind replaces an earlier one.
    MaxTriangleEdge { length: f64 },
    /// Remove loose points (and contour data) below `below` or above `above`.
    ExcludeElevations { below: Option<f64>, above: Option<f64> },
    /// Delete the triangle that contains this plan point.
    DeleteTriangle { x: f64, y: f64 },
    /// Swap the diagonal of the two triangles that share the edge between the vertices nearest
    /// to `a` and `b` (each within `tolerance` metres, default 0.001).
    SwapEdge { a: [f64; 2], b: [f64; 2], tolerance: Option<f64> },
    /// Add one point.
    AddPoint { x: f64, y: f64, z: f64 },
    /// Delete loose points.
    DeletePoint { target: PointRef },
    /// Raise (positive) or lower (negative) all the data gathered so far by `delta` metres.
    RaiseLower { delta: f64 },
    /// Paste another surface. Where it overlaps the existing data it wins: loose points inside
    /// its extent are dropped, its points are added and its triangle edges become breaklines.
    Paste { source: PasteSource },
}

/// Which elevation a merged duplicate takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicatePolicy {
    /// The first point read (the default).
    #[default]
    First,
    Lowest,
    Highest,
    Average,
}

/// Settings of a build.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildSettings {
    /// Points closer than this in plan are one point, m (default 0.001). Zero merges only
    /// points at exactly the same position.
    pub duplicate_tolerance: f64,
    /// Which elevation a merged group takes. A point on a breakline always takes the
    /// breakline's elevation.
    pub duplicate_policy: DuplicatePolicy,
}

impl Default for BuildSettings {
    fn default() -> BuildSettings {
        BuildSettings { duplicate_tolerance: 0.001, duplicate_policy: DuplicatePolicy::First }
    }
}

/// The ordered list of operations that builds a surface.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SurfaceDefinition {
    #[serde(default)]
    pub settings: BuildSettings,
    pub operations: Vec<Operation>,
}

impl SurfaceDefinition {
    /// An empty definition with default settings.
    pub fn new() -> SurfaceDefinition {
        SurfaceDefinition::default()
    }

    /// Appends an operation and returns the definition, for chaining.
    pub fn with(mut self, op: Operation) -> SurfaceDefinition {
        self.operations.push(op);
        self
    }

    /// Appends an operation.
    pub fn push(&mut self, op: Operation) {
        self.operations.push(op);
    }

    /// The definition as JSON text.
    pub fn to_json(&self) -> Result<String, TerrainError> {
        serde_json::to_string(self).map_err(|e| TerrainError::Encoding(e.to_string()))
    }

    /// A definition from JSON text written by [`SurfaceDefinition::to_json`].
    pub fn from_json(text: &str) -> Result<SurfaceDefinition, TerrainError> {
        serde_json::from_str(text).map_err(|e| TerrainError::Encoding(e.to_string()))
    }
}

/// A surface: identity, role, source, status and the definition that rebuilds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Surface {
    pub name: String,
    pub role: SurfaceRole,
    #[serde(default)]
    pub source: SourceRecord,
    #[serde(default)]
    pub status: SurfaceStatus,
    pub definition: SurfaceDefinition,
}

impl Surface {
    /// A draft surface with an empty definition.
    pub fn new(name: impl Into<String>, role: SurfaceRole) -> Surface {
        Surface { name: name.into(), role, source: SourceRecord::default(), status: SurfaceStatus::Draft, definition: SurfaceDefinition::new() }
    }

    /// Rebuilds the surface from its definition.
    pub fn build(&self) -> Result<BuiltSurface, TerrainError> {
        self.definition.build()
    }

    pub fn to_json(&self) -> Result<String, TerrainError> {
        serde_json::to_string(self).map_err(|e| TerrainError::Encoding(e.to_string()))
    }

    pub fn from_json(text: &str) -> Result<Surface, TerrainError> {
        serde_json::from_str(text).map_err(|e| TerrainError::Encoding(e.to_string()))
    }
}

/// What a build did.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BuildReport {
    /// Loose points read from the definition (points, contour data, added and pasted points).
    pub points_input: usize,
    /// Breakline vertices read.
    pub breakline_vertices: usize,
    /// Points in the finished surface.
    pub points_used: usize,
    /// Points merged into another point within the tolerance.
    pub duplicates_merged: usize,
    /// The largest elevation difference among merged points, m.
    pub duplicate_max_dz: f64,
    /// Points removed by an elevation exclusion.
    pub points_excluded: usize,
    /// Points removed by a delete-point operation.
    pub points_deleted: usize,
    /// Points dropped because no triangle uses them (for example inside a hidden area).
    pub points_unused: usize,
    /// Where lines crossed, with the elevation each carried and the one used.
    pub crossings: Vec<Crossing>,
    /// Triangles formed before any filter or edit.
    pub triangles_formed: usize,
    /// Triangles removed by the maximum edge length.
    pub triangles_removed_max_edge: usize,
    /// Triangles removed by outer and hide boundaries (after the maximum edge length).
    pub triangles_removed_boundary: usize,
    /// Triangles the maximum edge length had removed and a show boundary brought back.
    pub triangles_restored_by_show: usize,
    /// Loose points dropped because a pasted surface covers their position.
    pub points_replaced_by_paste: usize,
    /// Triangles removed by delete-triangle operations.
    pub triangles_deleted: usize,
    /// Edges swapped.
    pub edges_swapped: usize,
    /// Triangles of zero plan area dropped.
    pub triangles_degenerate: usize,
    /// Triangles in the finished surface.
    pub triangles: usize,
    /// Things the build could not do or did differently, in plain words.
    pub warnings: Vec<String>,
}

/// A rebuilt surface and what the build did.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltSurface {
    pub tin: Tin,
    pub report: BuildReport,
}
