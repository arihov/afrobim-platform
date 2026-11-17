//! The pipe network object: structures, pipes and the shapes, materials and roughness they carry.
//!
//! Units are metres, cubic metres per second and radians throughout. Coordinates are exactly the
//! eastings and northings given (true UTM magnitude is fine): nothing is shifted or rounded, and
//! distances are taken from coordinate differences.
//!
//! Nothing here has a built-in size, roughness, cover or slope. A part list (see
//! [`crate::parts`]) or the caller supplies them.

use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::PipesError;

/// Identifier of a structure within one network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StructureId(pub u32);

/// Identifier of a pipe within one network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PipeId(pub u32);

impl fmt::Display for StructureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "S{}", self.0)
    }
}
impl fmt::Display for PipeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "P{}", self.0)
    }
}

/// An element a finding refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum ElementRef {
    Structure(StructureId),
    Pipe(PipeId),
}

impl fmt::Display for ElementRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ElementRef::Structure(s) => s.fmt(f),
            ElementRef::Pipe(p) => p.fmt(f),
        }
    }
}

/// Lifecycle state of a network (the LandXML `stateType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkStatus {
    Existing,
    Proposed,
    Abandoned,
    Destroyed,
}

/// What the network carries (the LandXML `pipeNetworkType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkType {
    Storm,
    Sanitary,
    Water,
    Other,
}

/// Kind of structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureKind {
    Manhole,
    /// Catch basin or gully (an inlet from the surface).
    CatchBasin,
    Headwall,
    Outfall,
    Junction,
    /// A point where pipes meet with no physical structure (a zero-size node).
    Null,
}

impl StructureKind {
    pub const ALL: [StructureKind; 6] = [
        StructureKind::Manhole,
        StructureKind::CatchBasin,
        StructureKind::Headwall,
        StructureKind::Outfall,
        StructureKind::Junction,
        StructureKind::Null,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StructureKind::Manhole => "manhole",
            StructureKind::CatchBasin => "catch basin or gully",
            StructureKind::Headwall => "headwall",
            StructureKind::Outfall => "outfall",
            StructureKind::Junction => "junction",
            StructureKind::Null => "null structure",
        }
    }

    /// The key used in files (`manhole`, `catch_basin`, ...).
    pub fn key(self) -> &'static str {
        match self {
            StructureKind::Manhole => "manhole",
            StructureKind::CatchBasin => "catch_basin",
            StructureKind::Headwall => "headwall",
            StructureKind::Outfall => "outfall",
            StructureKind::Junction => "junction",
            StructureKind::Null => "null",
        }
    }

    pub fn from_key(key: &str) -> Option<StructureKind> {
        StructureKind::ALL.into_iter().find(|k| k.key() == key.trim().to_ascii_lowercase())
    }
}

/// Inner plan dimensions of a structure, metres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum StructureInner {
    Circular { diameter_m: f64 },
    /// Axis-aligned in plan: `length_m` along easting, `width_m` along northing.
    Rectangular { length_m: f64, width_m: f64 },
}

impl StructureInner {
    /// Radius of the circle that contains the inner plan shape, m.
    pub fn bounding_radius_m(&self) -> f64 {
        match *self {
            StructureInner::Circular { diameter_m } => diameter_m / 2.0,
            StructureInner::Rectangular { length_m, width_m } => 0.5 * length_m.hypot(width_m),
        }
    }
}

/// A structure: a manhole, catch basin, headwall, outfall, junction or null node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Structure {
    pub id: StructureId,
    pub name: String,
    pub kind: StructureKind,
    /// Easting of the centre, m.
    pub easting_m: f64,
    /// Northing of the centre, m.
    pub northing_m: f64,
    /// Rim (lid or top) level, m.
    pub rim_level_m: f64,
    /// Sump (floor) level, m. `None` when not set.
    pub sump_level_m: Option<f64>,
    pub inner: Option<StructureInner>,
    /// Wall thickness, m (for the outer size in clearance checks).
    pub wall_thickness_m: Option<f64>,
    /// Id of the part in a parts catalogue.
    pub part: Option<String>,
    /// Inflow entering the network at this structure, m3/s (zero when none).
    pub inflow_m3_s: f64,
    /// Head-loss coefficient K for `h = K V^2 / 2g` at this structure, an input; see
    /// [`crate::grade`]. `None` means friction only.
    pub loss_coefficient: Option<f64>,
    pub description: Option<String>,
}

/// Cross-section of a pipe. Dimensions are inside dimensions, metres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum PipeShape {
    Circular { diameter_m: f64 },
    Box { width_m: f64, height_m: f64 },
    /// An egg-shaped section. Egg profiles differ between manufacturers and standards, so none is
    /// built in: the width at depth is entered as `(depth from invert, full width)` points in
    /// metres, from depth 0 to `height_m`. Without a profile the hydraulics are not computed.
    Egg { width_m: f64, height_m: f64, profile: Vec<[f64; 2]> },
    /// A true ellipse with its major axis horizontal or vertical as `width_m` and `height_m` say.
    Elliptical { width_m: f64, height_m: f64 },
}

/// Area, wetted perimeter and top width of the flow at one depth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionProps {
    pub area_m2: f64,
    pub wetted_perimeter_m: f64,
    pub top_width_m: f64,
}

/// Points of a half-profile of an ellipse, as `(depth, full width)`.
fn ellipse_profile(width: f64, height: f64) -> Vec<[f64; 2]> {
    let n = 400;
    (0..=n)
        .map(|k| {
            // cosine spacing: the width changes fastest at the invert and the crown
            let y = height * 0.5 * (1.0 - (PI * k as f64 / n as f64).cos());
            let u = (y - height / 2.0) / (height / 2.0);
            [y, width * (1.0 - u * u).max(0.0).sqrt()]
        })
        .collect()
}

fn profile_props(profile: &[[f64; 2]], y: f64) -> SectionProps {
    let mut area = 0.0;
    let mut perim = 0.0;
    let mut top = profile.first().map_or(0.0, |p| p[1]);
    for w in profile.windows(2) {
        let (d0, w0) = (w[0][0], w[0][1]);
        let (d1, w1) = (w[1][0], w[1][1]);
        if y <= d0 {
            break;
        }
        let d = d1.min(y);
        let wd = if d1 > d0 { w0 + (w1 - w0) * (d - d0) / (d1 - d0) } else { w1 };
        area += 0.5 * (w0 + wd) * (d - d0);
        perim += 2.0 * ((d - d0).powi(2) + (0.5 * (wd - w0)).powi(2)).sqrt();
        top = wd;
    }
    SectionProps { area_m2: area, wetted_perimeter_m: perim + profile.first().map_or(0.0, |p| p[1]), top_width_m: top }
}

impl PipeShape {
    /// Inside height (rise), m.
    pub fn height_m(&self) -> f64 {
        match *self {
            PipeShape::Circular { diameter_m } => diameter_m,
            PipeShape::Box { height_m, .. } | PipeShape::Egg { height_m, .. } | PipeShape::Elliptical { height_m, .. } => height_m,
        }
    }

    /// Inside width (span), m.
    pub fn width_m(&self) -> f64 {
        match *self {
            PipeShape::Circular { diameter_m } => diameter_m,
            PipeShape::Box { width_m, .. } | PipeShape::Egg { width_m, .. } | PipeShape::Elliptical { width_m, .. } => width_m,
        }
    }

    /// A label made from the dimensions only (no nominal-size naming is built in), such as
    /// `1.200 m dia` or `1.500 x 1.000 m box`.
    pub fn size_label(&self) -> String {
        match self {
            PipeShape::Circular { diameter_m } => format!("{diameter_m:.3} m dia"),
            PipeShape::Box { width_m, height_m } => format!("{width_m:.3} x {height_m:.3} m box"),
            PipeShape::Egg { width_m, height_m, .. } => format!("{width_m:.3} x {height_m:.3} m egg"),
            PipeShape::Elliptical { width_m, height_m } => format!("{width_m:.3} x {height_m:.3} m elliptical"),
        }
    }

    pub fn is_valid(&self) -> bool {
        let pos = |v: f64| v.is_finite() && v > 0.0;
        match self {
            PipeShape::Circular { diameter_m } => pos(*diameter_m),
            PipeShape::Box { width_m, height_m } | PipeShape::Elliptical { width_m, height_m } => pos(*width_m) && pos(*height_m),
            PipeShape::Egg { width_m, height_m, profile } => {
                pos(*width_m) && pos(*height_m) && profile.iter().all(|p| p[0].is_finite() && p[1].is_finite() && p[1] >= 0.0) && profile.windows(2).all(|w| w[1][0] >= w[0][0])
            }
        }
    }

    /// Full-bore area, m2. `None` for an egg without a profile.
    pub fn full_area_m2(&self) -> Option<f64> {
        self.section_at(self.height_m()).map(|s| s.area_m2)
    }

    /// Full-bore wetted perimeter, m.
    pub fn full_perimeter_m(&self) -> Option<f64> {
        self.section_at(self.height_m()).map(|s| s.wetted_perimeter_m)
    }

    /// The flow section at a depth above the invert (clamped to 0..=height). At and above the
    /// height the pipe is full: the whole perimeter is wetted and the free surface is gone, so
    /// the top width is zero. `None` for an egg without a profile (or with fewer than two points).
    pub fn section_at(&self, depth_m: f64) -> Option<SectionProps> {
        let h = self.height_m();
        let y = depth_m.clamp(0.0, h);
        match self {
            PipeShape::Circular { diameter_m: d } => {
                if y >= *d {
                    return Some(SectionProps { area_m2: PI * d * d / 4.0, wetted_perimeter_m: PI * d, top_width_m: 0.0 });
                }
                // Chow (1959), geometric elements of a circular section: theta is the angle
                // subtended at the centre by the water surface.
                let theta = 2.0 * (1.0 - 2.0 * y / d).clamp(-1.0, 1.0).acos();
                Some(SectionProps { area_m2: d * d / 8.0 * (theta - theta.sin()), wetted_perimeter_m: d * theta / 2.0, top_width_m: d * (theta / 2.0).sin() })
            }
            PipeShape::Box { width_m: w, height_m: h } => {
                if y >= *h {
                    return Some(SectionProps { area_m2: w * h, wetted_perimeter_m: 2.0 * (w + h), top_width_m: 0.0 });
                }
                Some(SectionProps { area_m2: w * y, wetted_perimeter_m: w + 2.0 * y, top_width_m: *w })
            }
            PipeShape::Elliptical { width_m, height_m } => {
                let p = ellipse_profile(*width_m, *height_m);
                let full = y >= *height_m;
                let mut s = profile_props(&p, y);
                if full {
                    s.top_width_m = 0.0;
                }
                Some(s)
            }
            PipeShape::Egg { profile, .. } => {
                if profile.len() < 2 {
                    return None;
                }
                let full = y >= h;
                let mut s = profile_props(profile, y);
                if full {
                    // the lid: the top width closes the perimeter
                    s.wetted_perimeter_m += s.top_width_m;
                    s.top_width_m = 0.0;
                }
                Some(s)
            }
        }
    }
}

/// Roughness of the pipe wall. No value is built in.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "law", rename_all = "snake_case")]
pub enum Roughness {
    /// Manning's n, s/m^(1/3).
    Manning { n: f64 },
    /// Colebrook-White equivalent sand roughness ks, m. Needs the kinematic viscosity as an input
    /// ([`crate::hydraulics::HydraulicSettings`]).
    ColebrookWhite { ks_m: f64 },
}

/// A curved pipe is a circular arc in plan through its start, a given point and its end.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PipeVia {
    pub easting_m: f64,
    pub northing_m: f64,
}

/// A pipe from a start structure to an end structure; it flows from start to end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pipe {
    pub id: PipeId,
    pub name: String,
    pub start: StructureId,
    pub end: StructureId,
    /// Inside bottom level at the start centre, m.
    pub start_invert_m: f64,
    /// Inside bottom level at the end centre, m.
    pub end_invert_m: f64,
    pub shape: PipeShape,
    pub wall_thickness_m: Option<f64>,
    pub material: Option<String>,
    pub roughness: Option<Roughness>,
    pub part: Option<String>,
    /// A design flow that replaces the accumulated flow for this pipe, m3/s.
    pub design_flow_m3_s: Option<f64>,
    /// `Some` makes the pipe curved in plan.
    pub via: Option<PipeVia>,
    pub description: Option<String>,
}

/// A gravity pipe network.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipeNetwork {
    pub name: String,
    pub status: NetworkStatus,
    pub network_type: NetworkType,
    /// Where the network came from (a file name, a record number), free text.
    pub source: Option<String>,
    /// Name of the reference surface (ground) the network was designed against.
    pub surface_name: Option<String>,
    /// Name of the reference alignment.
    pub alignment_name: Option<String>,
    pub(crate) structures: BTreeMap<StructureId, Structure>,
    pub(crate) pipes: BTreeMap<PipeId, Pipe>,
    pub(crate) next_structure: u32,
    pub(crate) next_pipe: u32,
}

/// Plan geometry of one pipe: the centreline in plan with the arc fraction of each vertex.
#[derive(Debug, Clone, PartialEq)]
pub struct PipePath {
    /// `[easting, northing]` vertices.
    pub points: Vec<[f64; 2]>,
    /// Cumulative plan distance of each vertex from the start, m.
    pub chainage_m: Vec<f64>,
    /// Plan length, m (analytic for an arc).
    pub length_m: f64,
}

impl PipePath {
    /// The plan point at a distance along the path (clamped to the ends).
    pub fn point_at(&self, chainage_m: f64) -> [f64; 2] {
        let c = chainage_m.clamp(0.0, self.length_m);
        for i in 1..self.points.len() {
            if c <= self.chainage_m[i] || i == self.points.len() - 1 {
                let (c0, c1) = (self.chainage_m[i - 1], self.chainage_m[i]);
                let t = if c1 > c0 { ((c - c0) / (c1 - c0)).clamp(0.0, 1.0) } else { 0.0 };
                let (a, b) = (self.points[i - 1], self.points[i]);
                return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            }
        }
        self.points[0]
    }
}

impl PipeNetwork {
    pub fn new(name: &str) -> PipeNetwork {
        PipeNetwork {
            name: name.to_string(),
            status: NetworkStatus::Proposed,
            network_type: NetworkType::Other,
            source: None,
            surface_name: None,
            alignment_name: None,
            structures: BTreeMap::new(),
            pipes: BTreeMap::new(),
            next_structure: 1,
            next_pipe: 1,
        }
    }

    pub fn structures(&self) -> impl Iterator<Item = &Structure> {
        self.structures.values()
    }
    pub fn pipes(&self) -> impl Iterator<Item = &Pipe> {
        self.pipes.values()
    }
    pub fn structure(&self, id: StructureId) -> Result<&Structure, PipesError> {
        self.structures.get(&id).ok_or(PipesError::NoStructure(id))
    }
    pub fn pipe(&self, id: PipeId) -> Result<&Pipe, PipesError> {
        self.pipes.get(&id).ok_or(PipesError::NoPipe(id))
    }
    pub fn structure_mut(&mut self, id: StructureId) -> Result<&mut Structure, PipesError> {
        self.structures.get_mut(&id).ok_or(PipesError::NoStructure(id))
    }
    pub fn pipe_mut(&mut self, id: PipeId) -> Result<&mut Pipe, PipesError> {
        self.pipes.get_mut(&id).ok_or(PipesError::NoPipe(id))
    }
    pub fn structure_by_name(&self, name: &str) -> Option<&Structure> {
        self.structures.values().find(|s| s.name == name)
    }
    pub fn pipe_by_name(&self, name: &str) -> Option<&Pipe> {
        self.pipes.values().find(|p| p.name == name)
    }
    pub fn structure_count(&self) -> usize {
        self.structures.len()
    }
    pub fn pipe_count(&self) -> usize {
        self.pipes.len()
    }

    /// Pipes that end at a structure, in id order.
    pub fn incoming(&self, id: StructureId) -> Vec<PipeId> {
        self.pipes.values().filter(|p| p.end == id).map(|p| p.id).collect()
    }
    /// Pipes that start at a structure, in id order.
    pub fn outgoing(&self, id: StructureId) -> Vec<PipeId> {
        self.pipes.values().filter(|p| p.start == id).map(|p| p.id).collect()
    }

    /// Plan centre of a structure.
    pub fn position(&self, id: StructureId) -> Result<[f64; 2], PipesError> {
        let s = self.structure(id)?;
        Ok([s.easting_m, s.northing_m])
    }

    /// The plan path of a pipe. Straight pipes have two vertices; a curved pipe is cut into
    /// chords of at most 5 degrees of arc.
    pub fn pipe_path(&self, id: PipeId) -> Result<PipePath, PipesError> {
        let p = self.pipe(id)?;
        let a = self.position(p.start)?;
        let c = self.position(p.end)?;
        Ok(match p.via {
            None => straight_path(a, c),
            Some(v) => arc_path(a, [v.easting_m, v.northing_m], c),
        })
    }

    /// Plan length of a pipe between structure centres, m.
    pub fn plan_length_m(&self, id: PipeId) -> Result<f64, PipesError> {
        Ok(self.pipe_path(id)?.length_m)
    }

    /// Slope of the pipe (positive when it falls from start to end), m/m, on the plan length.
    pub fn slope(&self, id: PipeId) -> Result<f64, PipesError> {
        let p = self.pipe(id)?;
        let l = self.plan_length_m(id)?;
        if l <= 0.0 {
            return Ok(f64::NAN);
        }
        Ok((p.start_invert_m - p.end_invert_m) / l)
    }

    /// Length along the sloping centreline, m: `sqrt(plan^2 + fall^2)` for a straight pipe and
    /// the same applied to the arc length for a curved one (the fall is spread evenly along the
    /// arc, so the helix length is exact).
    pub fn sloping_length_m(&self, id: PipeId) -> Result<f64, PipesError> {
        let p = self.pipe(id)?;
        let l = self.plan_length_m(id)?;
        Ok(l.hypot(p.start_invert_m - p.end_invert_m))
    }

    /// Centreline of a pipe in 3D as `[easting, northing, level]` vertices, with the level at the
    /// middle of the inside height, interpolated linearly with plan distance along the path.
    pub fn centreline_3d(&self, id: PipeId) -> Result<Vec<[f64; 3]>, PipesError> {
        let p = self.pipe(id)?;
        let path = self.pipe_path(id)?;
        let half = p.shape.height_m() / 2.0;
        let l = path.length_m;
        Ok(path
            .points
            .iter()
            .zip(&path.chainage_m)
            .map(|(pt, &c)| {
                let t = if l > 0.0 { c / l } else { 0.0 };
                [pt[0], pt[1], p.start_invert_m + (p.end_invert_m - p.start_invert_m) * t + half]
            })
            .collect())
    }

    /// Radius of the circle that contains the outside of the pipe, m: half the larger inside
    /// dimension plus the wall thickness (zero when none is set).
    pub fn pipe_outer_radius_m(&self, id: PipeId) -> Result<f64, PipesError> {
        let p = self.pipe(id)?;
        Ok(0.5 * p.shape.width_m().max(p.shape.height_m()) + p.wall_thickness_m.unwrap_or(0.0))
    }

    pub(crate) fn alloc_structure(&mut self) -> StructureId {
        let id = StructureId(self.next_structure);
        self.next_structure += 1;
        id
    }
    pub(crate) fn alloc_pipe(&mut self) -> PipeId {
        let id = PipeId(self.next_pipe);
        self.next_pipe += 1;
        id
    }

    /// Insert whole elements as they are (used by file readers). Ids must be unused.
    pub fn insert_structure(&mut self, s: Structure) -> Result<StructureId, PipesError> {
        if self.structures.contains_key(&s.id) {
            return Err(PipesError::Invalid(format!("structure id {} is already used", s.id)));
        }
        self.next_structure = self.next_structure.max(s.id.0 + 1);
        let id = s.id;
        self.structures.insert(id, s);
        Ok(id)
    }

    /// Insert a whole pipe as it is. Its structures must exist and differ, and its id be unused.
    pub fn insert_pipe(&mut self, p: Pipe) -> Result<PipeId, PipesError> {
        if self.pipes.contains_key(&p.id) {
            return Err(PipesError::Invalid(format!("pipe id {} is already used", p.id)));
        }
        self.structure(p.start)?;
        self.structure(p.end)?;
        if p.start == p.end {
            return Err(PipesError::Invalid(format!("pipe {} starts and ends at the same structure", p.name)));
        }
        if !p.shape.is_valid() {
            return Err(PipesError::Invalid(format!("pipe {} has an invalid shape", p.name)));
        }
        self.next_pipe = self.next_pipe.max(p.id.0 + 1);
        let id = p.id;
        self.pipes.insert(id, p);
        Ok(id)
    }
}

fn straight_path(a: [f64; 2], c: [f64; 2]) -> PipePath {
    let l = (c[0] - a[0]).hypot(c[1] - a[1]);
    PipePath { points: vec![a, c], chainage_m: vec![0.0, l], length_m: l }
}

/// The circular arc from `a` through `b` to `c` in plan, cut into chords of at most 5 degrees.
/// Collinear points give the polyline a-b-c. All arithmetic is relative to `a` so that large
/// coordinates lose nothing.
fn arc_path(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> PipePath {
    let (bx, by) = (b[0] - a[0], b[1] - a[1]);
    let (cx, cy) = (c[0] - a[0], c[1] - a[1]);
    let d = 2.0 * (bx * cy - by * cx);
    let scale = (bx.hypot(by)).max(cx.hypot(cy));
    if d.abs() <= 1e-12 * scale * scale {
        let l1 = bx.hypot(by);
        let l2 = (cx - bx).hypot(cy - by);
        return PipePath { points: vec![a, b, c], chainage_m: vec![0.0, l1, l1 + l2], length_m: l1 + l2 };
    }
    let (b2, c2) = (bx * bx + by * by, cx * cx + cy * cy);
    let ux = (cy * b2 - by * c2) / d;
    let uy = (bx * c2 - cx * b2) / d;
    let r = ux.hypot(uy);
    let ta = (-uy).atan2(-ux);
    let tc = (cy - uy).atan2(cx - ux);
    let two_pi = 2.0 * PI;
    let sweep = if d > 0.0 { (tc - ta).rem_euclid(two_pi) } else { -((ta - tc).rem_euclid(two_pi)) };
    let n = ((sweep.abs() / (5.0_f64).to_radians()).ceil() as usize).max(2);
    let mut points = Vec::with_capacity(n + 1);
    let mut chain = Vec::with_capacity(n + 1);
    let length = r * sweep.abs();
    for k in 0..=n {
        let t = k as f64 / n as f64;
        if k == 0 {
            points.push(a);
        } else if k == n {
            points.push(c);
        } else {
            let th = ta + sweep * t;
            points.push([a[0] + ux + r * th.cos(), a[1] + uy + r * th.sin()]);
        }
        chain.push(length * t);
    }
    PipePath { points, chainage_m: chain, length_m: length }
}
