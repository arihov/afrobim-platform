//! Side drains: the definition stored in an element's `data`, its route along an alignment and
//! the kernel module that draws it.

use std::collections::BTreeMap;

use afrobim_alignment::Alignment;
use afrobim_draw::{Colour, Drawing, Extents, Mesh3, Primitive, Pt2};
use afrobim_kernel::{Element, Generated, Module, Note};
use serde::{Deserialize, Serialize};

use crate::design::{check_channel, ChannelCheck, ChannelInput, DesignBasis};
use crate::quantities::DrainQuantities;
use crate::{
    error_note, error_text, fixed, info_note, not_negative, positive, warning_note, ChannelSection, DrainageError, Lining, NOTE_PLACEHOLDER,
    PLACEHOLDER_DRAIN_BOTTOM_WIDTH_MM, PLACEHOLDER_DRAIN_DEPTH_MM, PLACEHOLDER_DRAIN_SIDE_SLOPE, UNLINED_DISPLAY_SKIN_MM,
};

/// The element category [`SideDrainModule`] owns.
pub const DRAIN_CATEGORY: &str = "drain";

/// Display colours in the 3D view: a lined drain and a cut in the ground. Screen colours only.
const LINED_RGB: [u8; 3] = [0xB4, 0xB7, 0xBA];
const UNLINED_RGB: [u8; 3] = [0x8C, 0x6E, 0x4B];

/// Greatest distance allowed between a chord of the drawn route and the true curve, m.
const CHORD_SAG_M: f64 = 0.005;

/// One point of a drain's centreline in plan.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RoutePoint {
    /// Station on the alignment, m.
    pub station_m: f64,
    /// Easting of the drain centreline, m.
    pub x_m: f64,
    /// Northing of the drain centreline, m.
    pub y_m: f64,
    /// Direction of the alignment there, radians counter-clockwise from +X.
    pub heading_rad: f64,
}

/// Where a drain runs, worked out from its alignment and kept with the definition so that the
/// module can draw the drain without the alignment to hand. It records the stations and offset
/// it was made for; [`SideDrainDefinition::notes`] warns when they no longer match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrainRoute {
    pub start_station_m: f64,
    pub end_station_m: f64,
    pub offset_m: f64,
    /// Length of the alignment between the two stations, m.
    pub centreline_length_m: f64,
    /// Length of the drain's own centreline, m: `∫ (1 + κ·o) ds` over the stations, with κ the
    /// alignment's curvature (positive turning left) and o the offset (positive right). Exact
    /// for straights and circular arcs (`L·(R ± o)/R`) and for clothoids, whose curvature is
    /// linear. It is not measured from the points below.
    pub length_m: f64,
    /// The centreline as a polyline: every element joint, and curves in chords within 5 mm of
    /// the true line.
    pub points: Vec<RoutePoint>,
}

impl DrainRoute {
    /// The route of a drain at `offset_m` from `alignment` between two stations.
    pub fn along(alignment: &Alignment, start_station_m: f64, end_station_m: f64, offset_m: f64) -> Result<DrainRoute, DrainageError> {
        let route = |m: String| Err(DrainageError::Route(m));
        if !(start_station_m.is_finite() && end_station_m.is_finite() && offset_m.is_finite()) {
            return route("the stations and the offset must be numbers".to_string());
        }
        if end_station_m <= start_station_m {
            return route(format!("the end station ({end_station_m}) must be greater than the start station ({start_station_m})"));
        }
        if alignment.locate(start_station_m).is_none() || alignment.locate(end_station_m).is_none() {
            return route(format!(
                "stations {start_station_m} to {end_station_m} are not all on alignment '{}' ({} to {})",
                alignment.name,
                alignment.start_station_m,
                alignment.end_station_m()
            ));
        }
        let mut stations: Vec<f64> = vec![start_station_m];
        let mut length_m = 0.0;
        let starts = alignment.element_stations();
        for (e, &es) in alignment.elements.iter().zip(&starts) {
            let (s0, s1) = (start_station_m.max(es), end_station_m.min(es + e.length));
            if s1 <= s0 {
                continue;
            }
            let (k0, k1) = (e.curvature_at(s0 - es), e.curvature_at(s1 - es));
            // 1 + κ·o is the ratio of the offset line's length to the centreline's. It reaches
            // zero where the offset equals the radius on the inside of a curve.
            if 1.0 + k0 * offset_m <= 0.0 || 1.0 + k1 * offset_m <= 0.0 {
                return route(format!("an offset of {offset_m} m is at or beyond the centre of the curve near station {s0:.3}"));
            }
            length_m += (s1 - s0) * (1.0 + offset_m * (k0 + k1) / 2.0);
            let k_max = k0.abs().max(k1.abs());
            let chords = if k_max < 1e-12 {
                1
            } else {
                // Sagitta of a chord c on radius R is c²/8R.
                let chord = (8.0 * CHORD_SAG_M / k_max).sqrt().clamp(0.25, 20.0);
                ((s1 - s0) / chord).ceil().max(1.0) as usize
            };
            for i in 1..=chords {
                stations.push(s0 + (s1 - s0) * i as f64 / chords as f64);
            }
        }
        stations.dedup_by(|b, a| (*b - *a).abs() < 1e-9);
        let mut points = Vec::with_capacity(stations.len());
        for s in stations {
            let Some(at) = alignment.point_at(s) else {
                return route(format!("station {s} is not on alignment '{}'", alignment.name));
            };
            let (x_m, y_m) = at.offset(offset_m);
            points.push(RoutePoint { station_m: s, x_m, y_m, heading_rad: at.heading_rad });
        }
        if points.len() < 2 {
            return route("the route has no length".to_string());
        }
        Ok(DrainRoute { start_station_m, end_station_m, offset_m, centreline_length_m: end_station_m - start_station_m, length_m, points })
    }
}

/// A side drain as the engineer describes it: an open channel beside an alignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SideDrainDefinition {
    /// Name of the alignment the drain follows.
    pub alignment: String,
    pub start_station_m: f64,
    pub end_station_m: f64,
    /// Offset of the drain's centreline (the middle of its bed; the invert of a V) from the
    /// alignment, m, positive to the right looking up-station.
    pub offset_m: f64,
    /// The finished, water-side section.
    pub section: ChannelSection,
    /// What the wetted faces are: for an unlined drain, the ground it is cut in.
    pub lining: Lining,
    /// Thickness of the lining, mm, measured square to the faces. None or zero for an unlined drain.
    #[serde(default)]
    pub lining_thickness_mm: Option<f64>,
    /// Longitudinal gradient of the bed, m/m, entered as a positive fall per unit length. The
    /// alignment has no vertical geometry yet, so it cannot be derived; which way the drain
    /// falls is not modelled.
    pub gradient_m_per_m: f64,
    /// Manning's n for the hydraulic checks, entered. None leaves the capacity unreported.
    #[serde(default)]
    pub manning_n: Option<f64>,
    /// Where the drain runs, from [`DrainRoute::along`]. None until it has been routed.
    #[serde(default)]
    pub route: Option<DrainRoute>,
    /// True while the dimensions are still the `PLACEHOLDER_*` starting values.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub placeholder: bool,
}

/// The excavated outline of a lined drain: see [`SideDrainDefinition::excavation_area_m2`].
struct Outline {
    /// Bed width, depth and side slopes of the finished section, mm.
    b: f64,
    d: f64,
    zl: f64,
    zr: f64,
    /// Lining thickness, mm.
    t: f64,
}

impl Outline {
    /// Half-widths (left, right) of the finished section at its top edges.
    fn inner_top(&self) -> (f64, f64) {
        (self.b / 2.0 + self.zl * self.d, self.b / 2.0 + self.zr * self.d)
    }
    /// Half-widths of the excavation at ground level: the finished face moved out square to
    /// itself by t is t·√(1 + z²) further out at any level.
    fn outer_top(&self) -> (f64, f64) {
        let (l, r) = self.inner_top();
        (l + self.t * self.zl.hypot(1.0), r + self.t * self.zr.hypot(1.0))
    }
    /// Half-widths of the excavation bed, t below the finished bed.
    fn outer_bottom(&self) -> (f64, f64) {
        (self.b / 2.0 + self.t * (self.zl.hypot(1.0) - self.zl), self.b / 2.0 + self.t * (self.zr.hypot(1.0) - self.zr))
    }
}

impl SideDrainDefinition {
    /// A drawable starting definition made of the `PLACEHOLDER_*` constants, flagged as such,
    /// on the given alignment and stations. It is not a recommended, typical or adequate drain.
    pub fn placeholder(alignment: &str, start_station_m: f64, end_station_m: f64, offset_m: f64) -> Self {
        SideDrainDefinition {
            alignment: alignment.to_string(),
            start_station_m,
            end_station_m,
            offset_m,
            section: ChannelSection::Trapezoidal {
                bottom_width_mm: PLACEHOLDER_DRAIN_BOTTOM_WIDTH_MM,
                depth_mm: PLACEHOLDER_DRAIN_DEPTH_MM,
                left_side_slope: PLACEHOLDER_DRAIN_SIDE_SLOPE,
                right_side_slope: PLACEHOLDER_DRAIN_SIDE_SLOPE,
            },
            lining: Lining::Other(String::new()),
            lining_thickness_mm: None,
            gradient_m_per_m: 0.0,
            manning_n: None,
            route: None,
            placeholder: true,
        }
    }

    pub fn from_value(data: &serde_json::Value) -> Result<Self, DrainageError> {
        serde_json::from_value(data.clone()).map_err(|e| DrainageError::InvalidDefinition(format!("the side drain definition cannot be read: {e}")))
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a side drain definition always serialises")
    }

    /// Lining thickness, mm; zero for an unlined drain.
    pub fn lining_mm(&self) -> f64 {
        self.lining_thickness_mm.unwrap_or(0.0)
    }

    fn outline(&self) -> Outline {
        let (zl, zr) = self.section.side_slopes();
        Outline { b: self.section.bottom_width_mm(), d: self.section.depth_mm(), zl, zr, t: self.lining_mm() }
    }

    /// Area of the finished section to its top edges, m²: `d·(b + (zl + zr)·d / 2)`.
    pub fn finished_area_m2(&self) -> f64 {
        self.section.geometry().area_m2(self.section.depth_mm() / 1000.0)
    }

    /// Length of the finished faces across the section, m: `b + d·(√(1+zl²) + √(1+zr²))`.
    pub fn lined_perimeter_m(&self) -> f64 {
        self.section.geometry().wetted_perimeter_m(self.section.depth_mm() / 1000.0)
    }

    /// Area of the excavated section, m².
    ///
    /// The excavation is the trapezoid with the same side slopes whose sloping faces are the
    /// lining thickness `t` outside the finished faces (measured square to them) and whose bed
    /// is `t` below the finished bed, cut from ground taken as level with both top edges:
    ///
    /// ```text
    /// be = b + t·(√(1+zl²) − zl + √(1+zr²) − zr)      bed width of the excavation
    /// Ae = (d + t)·(be + (zl + zr)·(d + t) / 2)
    /// ```
    ///
    /// With no lining (`t = 0`) this is the finished area. No working space, overbreak, topsoil
    /// strip or bulking is included.
    pub fn excavation_area_m2(&self) -> f64 {
        let o = self.outline();
        let (bl, br) = o.outer_bottom();
        let depth = o.d + o.t;
        depth * ((bl + br) + (o.zl + o.zr) * depth / 2.0) / 1.0e6
    }

    /// Validation and information notes. Error notes mean the drain cannot be drawn or measured.
    pub fn notes(&self) -> Vec<Note> {
        let mut out = self.section.issues();
        if !(self.start_station_m.is_finite() && self.end_station_m.is_finite() && self.end_station_m > self.start_station_m) {
            out.push(error_note(
                "DRAIN_STATIONS_NOT_IN_ORDER",
                format!("The end station ({}) must be greater than the start station ({}).", self.end_station_m, self.start_station_m),
            ));
        }
        if !self.offset_m.is_finite() {
            out.push(error_note("DRAIN_OFFSET_NOT_A_NUMBER", "The offset must be a number.".to_string()));
        }
        if !not_negative(self.lining_mm()) {
            out.push(error_note("DRAIN_LINING_THICKNESS_NEGATIVE", format!("The lining thickness must not be negative (it is {}).", self.lining_mm())));
        }
        if !not_negative(self.gradient_m_per_m) {
            out.push(error_note("DRAIN_GRADIENT_NEGATIVE", format!("The gradient is entered as a positive fall per unit length (it is {}).", self.gradient_m_per_m)));
        } else if self.gradient_m_per_m == 0.0 {
            out.push(info_note("DRAIN_GRADIENT_NOT_ENTERED", "No longitudinal gradient is entered, so the drain has no flow capacity yet."));
        }
        if let Some(n) = self.manning_n {
            if !positive(n) {
                out.push(error_note("DRAIN_ROUGHNESS_NOT_POSITIVE", format!("Manning's n must be greater than zero (it is {n}).")));
            }
        } else {
            out.push(info_note("DRAIN_ROUGHNESS_NOT_ENTERED", "No Manning's n is entered, so no flow capacity is shown."));
        }
        match &self.route {
            None => out.push(warning_note(
                "DRAIN_NOT_ROUTED",
                format!("The drain has not been routed along alignment '{}', so it has no length, quantities or 3D form yet.", self.alignment),
            )),
            Some(r) => {
                if r.start_station_m != self.start_station_m || r.end_station_m != self.end_station_m || r.offset_m != self.offset_m {
                    out.push(warning_note(
                        "DRAIN_ROUTE_OUT_OF_DATE",
                        "The stations or offset have changed since the drain was routed; its length and 3D form are those of the old route until it is routed again.".to_string(),
                    ));
                }
            }
        }
        if self.placeholder {
            out.push(info_note(
                NOTE_PLACEHOLDER,
                "The drain dimensions are placeholders so that the element can be drawn. They are not design values; the engineer must set the section, lining and gradient.",
            ));
        }
        if self.lining.label().trim().is_empty() {
            out.push(info_note("DRAIN_LINING_NOT_STATED", "No lining or channel material is stated for this drain."));
        }
        out
    }

    /// Ok when the definition has no Error notes.
    pub fn check(&self) -> Result<(), DrainageError> {
        match error_text(&self.notes()) {
            Some(text) => Err(DrainageError::InvalidDefinition(text)),
            None => Ok(()),
        }
    }

    /// Works out the route from the alignment and stores it. Call again whenever the alignment,
    /// the stations or the offset change.
    pub fn route_along(&mut self, alignment: &Alignment) -> Result<(), DrainageError> {
        self.route = Some(DrainRoute::along(alignment, self.start_station_m, self.end_station_m, self.offset_m)?);
        Ok(())
    }

    /// The input of [`check_channel`] for this drain at a design flow, to the chosen basis: its
    /// section, depth, gradient and entered n. Under [`DesignBasis::Mowt`] the permissible
    /// velocity is that of DDM Table 9.1 when the lining is one of that table's materials; the
    /// DMRB documents held state no permissible velocity, so under [`DesignBasis::Dmrb`] none
    /// is set (set one on the returned input if the project has it from elsewhere).
    /// `required_freeboard_m` is passed through.
    pub fn channel_input(&self, basis: DesignBasis, design_flow_m3_s: f64, required_freeboard_m: Option<f64>) -> Result<ChannelInput, DrainageError> {
        self.check()?;
        let Some(manning_n) = self.manning_n else {
            return Err(DrainageError::InvalidInput("enter Manning's n for the drain before checking its capacity".to_string()));
        };
        let material = self.lining.table_material().filter(|_| basis == DesignBasis::Mowt);
        Ok(ChannelInput {
            basis,
            geometry: self.section.geometry(),
            depth_m: self.section.depth_mm() / 1000.0,
            manning_n,
            roughness_source: "entered for the drain".to_string(),
            gradient_m_per_m: self.gradient_m_per_m,
            design_flow_m3_s,
            permissible_velocity_m_s: material.map(|m| m.permissible_velocity_m_s()),
            permissible_source: material.map(|m| format!("DDM Table 9.1, {}", m.label().to_lowercase())).unwrap_or_default(),
            required_freeboard_m,
        })
    }

    /// [`check_channel`] on [`SideDrainDefinition::channel_input`].
    pub fn check_capacity(&self, basis: DesignBasis, design_flow_m3_s: f64, required_freeboard_m: Option<f64>) -> Result<ChannelCheck, DrainageError> {
        check_channel(&self.channel_input(basis, design_flow_m3_s, required_freeboard_m)?)
    }

    /// The shell shown in 3D, as eight (lateral, z) corners in mm: lateral positive to the
    /// right looking up-station, z = 0 at the top edges. Counter-clockwise seen looking
    /// up-station: outer top-left, outer bottom-left, outer bottom-right, outer top-right, inner
    /// top-right, inner bottom-right, inner bottom-left, inner top-left.
    fn shell(&self) -> [(f64, f64); 8] {
        let mut o = self.outline();
        if o.t <= 0.0 {
            o.t = UNLINED_DISPLAY_SKIN_MM;
        }
        let (itl, itr) = o.inner_top();
        let (otl, otr) = o.outer_top();
        let (obl, obr) = o.outer_bottom();
        [(-otl, 0.0), (-obl, -(o.d + o.t)), (obr, -(o.d + o.t)), (otr, 0.0), (itr, 0.0), (o.b / 2.0, -o.d), (-o.b / 2.0, -o.d), (-itl, 0.0)]
    }

    /// The drain for the 3D view: one closed trough swept along the route, in the frame the
    /// crate documentation describes (origin at the first route point, not rotated, z = 0 at the
    /// top edges, level). The trough is the lining; for an unlined drain it is a skin of
    /// [`UNLINED_DISPLAY_SKIN_MM`] so that the cut can be seen. Empty until the drain is routed.
    pub fn solids(&self) -> Vec<Mesh3> {
        let Some(route) = &self.route else { return Vec::new() };
        let shell = self.shell();
        let origin = route.points[0];
        let mut vertices: Vec<[f64; 3]> = Vec::with_capacity(route.points.len() * 8);
        for p in &route.points {
            // Right of the heading is (sin h, −cos h).
            let (rx, ry) = (p.heading_rad.sin(), -p.heading_rad.cos());
            let (cx, cy) = ((p.x_m - origin.x_m) * 1000.0, (p.y_m - origin.y_m) * 1000.0);
            vertices.extend(shell.iter().map(|&(u, z)| [cx + u * rx, cy + u * ry, z]));
        }
        let rings = route.points.len() as u32;
        let mut triangles: Vec<[u32; 3]> = Vec::new();
        let mut push = |a: u32, b: u32, c: u32| {
            let (pa, pb, pc) = (vertices[a as usize], vertices[b as usize], vertices[c as usize]);
            if pa != pb && pb != pc && pa != pc {
                triangles.push([a, b, c]);
            }
        };
        // The three quads of the shell: left wall, bed, right wall.
        const QUADS: [[u32; 4]; 3] = [[0, 1, 6, 7], [1, 2, 5, 6], [2, 3, 4, 5]];
        for q in QUADS {
            // Start cap faces down-station, end cap up-station.
            push(q[0], q[1], q[2]);
            push(q[0], q[2], q[3]);
            let e = (rings - 1) * 8;
            push(e + q[0], e + q[2], e + q[1]);
            push(e + q[0], e + q[3], e + q[2]);
        }
        for r in 0..rings - 1 {
            let (a, b) = (r * 8, (r + 1) * 8);
            for j in 0..8u32 {
                let k = (j + 1) % 8;
                push(a + j, b + k, a + k);
                push(a + j, b + j, b + k);
            }
        }
        let lined = self.lining_mm() > 0.0;
        vec![Mesh3 { name: if lined { "Drain lining" } else { "Drain" }.to_string(), vertices, triangles, colour: if lined { LINED_RGB } else { UNLINED_RGB } }]
    }

    /// Cross-section for the design sheet, mm, seen looking up-station: the lowest point of the
    /// excavation on y = 0, the ground line running a margin either side of the top edges, the
    /// lining (if any) filled grey and the finished faces as lines.
    pub fn drawing(&self) -> Drawing {
        let o = self.outline();
        let (itl, itr) = o.inner_top();
        let (otl, otr) = o.outer_top();
        let (obl, obr) = o.outer_bottom();
        let top = o.d + o.t;
        let margin = ((otl + otr) * 0.15).max(100.0);
        // Drawing x = lateral + shift, y = z + top.
        let shift = otl + margin;
        let p = |u: f64, z: f64| Pt2::new(u + shift, z + top);
        let thin = ((otl + otr) * 0.004).max(2.0);
        let mut primitives = Vec::new();
        if o.t > 0.0 {
            primitives.push(Primitive::Fill {
                outline: vec![p(-otl, 0.0), p(-obl, -top), p(obr, -top), p(otr, 0.0), p(itr, 0.0), p(o.b / 2.0, -o.d), p(-o.b / 2.0, -o.d), p(-itl, 0.0)],
                colour: Colour::Grey,
            });
        }
        let faces = [p(-itl, 0.0), p(-o.b / 2.0, -o.d), p(o.b / 2.0, -o.d), p(itr, 0.0)];
        for w in faces.windows(2) {
            if w[0] != w[1] {
                primitives.push(Primitive::Line { from: w[0], to: w[1], width: thin, colour: None });
            }
        }
        for (from, to) in [(p(-otl - margin, 0.0), p(-itl, 0.0)), (p(itr, 0.0), p(otr + margin, 0.0))] {
            primitives.push(Primitive::Line { from, to, width: thin, colour: Some(Colour::Brown) });
        }
        Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(otl + otr + 2.0 * margin, top) } }
    }
}

/// The kernel module for category [`DRAIN_CATEGORY`].
///
/// `generate` reads a [`SideDrainDefinition`] from the element's `data`. A definition with
/// Error notes yields the notes and no geometry. The drawing is the cross-section; the solids
/// follow the stored route. When Manning's n and a gradient are entered the derived values
/// include the discharge and velocity with the drain flowing to its top edges (Manning's
/// equation, the same under every basis). Under [`DesignBasis::Mowt`] a warning is raised if
/// that velocity is above the permissible velocity of DDM Table 9.1 for the lining; the DMRB
/// documents held state no permissible velocity, so under [`DesignBasis::Dmrb`] there is none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SideDrainModule {
    /// The basis the velocity check is made to. The engineer's choice; [`Default`] is MoWT.
    pub basis: DesignBasis,
}

impl Default for SideDrainModule {
    /// Uses [`DesignBasis::Mowt`].
    fn default() -> Self {
        SideDrainModule { basis: DesignBasis::Mowt }
    }
}

impl SideDrainModule {
    pub fn with_basis(basis: DesignBasis) -> Self {
        SideDrainModule { basis }
    }
}

impl Module for SideDrainModule {
    fn category(&self) -> &str {
        DRAIN_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Side drains"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let def = SideDrainDefinition::from_value(&element.data).map_err(|e| e.to_string())?;
        let mut notes = def.notes();
        if error_text(&notes).is_some() {
            return Ok(Generated { notes, ..Generated::default() });
        }
        let mut derived = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            derived.insert(k.to_string(), v);
        };
        put("Section", def.section.label());
        put("Top width", format!("{} mm", fixed(def.section.top_width_mm(), 0, 1)));
        put("Lining", if def.lining_mm() > 0.0 { format!("{}, {} mm", def.lining.label(), fixed(def.lining_mm(), 0, 1)) } else { format!("{} (unlined)", def.lining.label()) });
        put("Gradient", format!("{} %", fixed(def.gradient_m_per_m * 100.0, 2, 3)));
        put("Finished section area", format!("{} m²", fixed(def.finished_area_m2(), 3, 3)));
        put("Excavated section area", format!("{} m²", fixed(def.excavation_area_m2(), 3, 3)));
        if def.route.is_some() {
            let q = DrainQuantities::measure(&element.name, &def).map_err(|e| e.to_string())?;
            put("Length along the drain", format!("{} m", fixed(q.length_m, 3, 3)));
            put("Length along the alignment", format!("{} m", fixed(q.centreline_length_m, 3, 3)));
            put("Excavation", format!("{} m³", fixed(q.excavation_m3, 2, 2)));
            if def.lining_mm() > 0.0 {
                put("Lining area", format!("{} m²", fixed(q.lining_area_m2, 2, 2)));
                put("Lining volume", format!("{} m³", fixed(q.lining_volume_m3, 2, 2)));
            }
        }
        if let (Some(n), true) = (def.manning_n, def.gradient_m_per_m > 0.0) {
            let g = def.section.geometry();
            let d = def.section.depth_mm() / 1000.0;
            let q = g.discharge_m3_s(d, n, def.gradient_m_per_m);
            let v = q / g.area_m2(d);
            put("Capacity at full depth (Manning)", format!("{} m³/s", fixed(q, 3, 3)));
            put("Velocity at full depth", format!("{} m/s", fixed(v, 2, 2)));
            if let Some(m) = def.lining.table_material().filter(|_| self.basis == DesignBasis::Mowt) {
                if v > m.permissible_velocity_m_s() {
                    notes.push(warning_note(
                        "DRAIN_VELOCITY_ABOVE_PERMISSIBLE",
                        format!(
                            "Flowing full the velocity is {v:.2} m/s, above the maximum permissible {} m/s for {} (MoWT Drainage Design Manual 2010, Table 9.1). Check the velocity at the design flow.",
                            m.permissible_velocity_m_s(),
                            m.label().to_lowercase()
                        ),
                    ));
                }
            }
        }
        Ok(Generated { drawing: def.drawing(), notes, derived, solids: def.solids() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::mowt::ChannelMaterial;
    use afrobim_alignment::{AlignmentBuilder, Turn};
    use afrobim_kernel::{Document, Pt, Severity};

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!((actual - expected).abs() <= tolerance, "expected {expected} ± {tolerance}, got {actual}");
    }

    /// 100 m straight heading east from (1000, 2000), then a 200 m radius left-hand arc 100 m long.
    fn alignment() -> Alignment {
        AlignmentBuilder::new("MC10", 0.0, 1000.0, 2000.0, 0.0).line(100.0).arc(200.0, 100.0, Turn::Left).build().unwrap()
    }

    fn lined() -> SideDrainDefinition {
        SideDrainDefinition {
            alignment: "MC10".into(),
            start_station_m: 10.0,
            end_station_m: 60.0,
            offset_m: 5.0,
            section: ChannelSection::Trapezoidal { bottom_width_mm: 500.0, depth_mm: 600.0, left_side_slope: 1.5, right_side_slope: 1.5 },
            lining: Lining::Table(ChannelMaterial::Concrete),
            lining_thickness_mm: Some(100.0),
            gradient_m_per_m: 0.01,
            manning_n: Some(0.015),
            route: None,
            placeholder: false,
        }
    }

    fn routed(mut def: SideDrainDefinition) -> SideDrainDefinition {
        def.route_along(&alignment()).unwrap();
        def
    }

    fn element(def: &SideDrainDefinition) -> Element {
        let mut d = Document::default();
        let id = d.add(DRAIN_CATEGORY, "SD-01", Pt::new(0.0, 0.0), 1.0, 1.0, "Drains", def.to_value());
        d.find(id).unwrap().clone()
    }

    #[test]
    fn route_on_a_straight_is_parallel_and_as_long_as_the_centreline() {
        let r = DrainRoute::along(&alignment(), 10.0, 60.0, 5.0).unwrap();
        assert_eq!(r.points.len(), 2);
        // Heading east, so 5 m to the right is 5 m south.
        close(r.points[0].x_m, 1010.0, 1e-9);
        close(r.points[0].y_m, 1995.0, 1e-9);
        close(r.points[1].x_m, 1060.0, 1e-9);
        close(r.points[1].y_m, 1995.0, 1e-9);
        close(r.length_m, 50.0, 1e-12);
        close(r.centreline_length_m, 50.0, 1e-12);
    }

    #[test]
    fn route_on_a_curve_is_longer_outside_and_shorter_inside() {
        // The arc turns left with R = 200 m over stations 100 to 200. A drain 5 m to the right is
        // outside the curve on R = 205: 100 × 205 / 200 = 102.5 m. 5 m to the left is inside on
        // R = 195: 100 × 195 / 200 = 97.5 m.
        let outside = DrainRoute::along(&alignment(), 100.0, 200.0, 5.0).unwrap();
        close(outside.length_m, 102.5, 1e-9);
        close(outside.centreline_length_m, 100.0, 1e-12);
        let inside = DrainRoute::along(&alignment(), 100.0, 200.0, -5.0).unwrap();
        close(inside.length_m, 97.5, 1e-9);
        // Across the tangent point, stations 50 to 150: 50 + 50 × 205 / 200 = 101.25 m.
        let both = DrainRoute::along(&alignment(), 50.0, 150.0, 5.0).unwrap();
        close(both.length_m, 101.25, 1e-9);
        assert!(both.points.iter().any(|p| (p.station_m - 100.0).abs() < 1e-9), "the joint is a route point");
        // Every point is 5 m from the centre line's point at its station, and the chords stay
        // within 5 mm of the 205 m circle about the arc's centre (1100, 2200).
        for p in &outside.points {
            close((p.x_m - 1100.0).hypot(p.y_m - 2200.0), 205.0, 1e-6);
        }
        let chord_sum: f64 = outside.points.windows(2).map(|w| (w[1].x_m - w[0].x_m).hypot(w[1].y_m - w[0].y_m)).sum();
        assert!(chord_sum < 102.5 && chord_sum > 102.49, "the polyline is a close inscribed approximation, got {chord_sum}");
    }

    #[test]
    fn routes_off_the_alignment_or_through_the_curve_centre_are_refused() {
        let a = alignment();
        assert!(matches!(DrainRoute::along(&a, 150.0, 250.0, 5.0), Err(DrainageError::Route(_))));
        assert!(matches!(DrainRoute::along(&a, 60.0, 10.0, 5.0), Err(DrainageError::Route(_))));
        // 200 m to the left of a 200 m left-hand curve is its centre.
        let e = DrainRoute::along(&a, 100.0, 200.0, -200.0).unwrap_err();
        assert!(e.to_string().contains("centre of the curve"));
        assert!(DrainRoute::along(&a, 0.0, 100.0, -200.0).is_ok(), "the straight has no centre");
    }

    #[test]
    fn section_areas_of_a_lined_trapezoid() {
        let d = lined();
        // Finished: 0.6 × (0.5 + 3.0 × 0.6 / 2) = 0.6 × 1.4 = 0.84 m².
        close(d.finished_area_m2(), 0.84, 1e-12);
        // Faces: 0.5 + 0.6 × 2 × √3.25 = 2.663331 m.
        close(d.lined_perimeter_m(), 2.663331, 1e-6);
        // Excavation, t = 0.1: be = 0.5 + 0.1 × 2 × (√3.25 − 1.5) = 0.5 + 0.2 × 0.302776 = 0.560555;
        // Ae = 0.7 × (0.560555 + 3.0 × 0.7 / 2) = 0.7 × 1.610555 = 1.127389 m².
        close(d.excavation_area_m2(), 1.127389, 1e-6);
        // Rectangle 450 × 600 with 75 mm walls and bed: (0.45 + 0.15) × 0.675 = 0.405 m².
        let r = SideDrainDefinition { section: ChannelSection::Rectangular { width_mm: 450.0, depth_mm: 600.0 }, lining_thickness_mm: Some(75.0), ..lined() };
        close(r.excavation_area_m2(), 0.405, 1e-12);
        // Unlined: the excavation is the finished section.
        let u = SideDrainDefinition { lining_thickness_mm: None, ..lined() };
        close(u.excavation_area_m2(), 0.84, 1e-12);
    }

    #[test]
    fn solids_are_a_closed_trough_of_the_lining_below_ground() {
        let def = routed(lined());
        let g = SideDrainModule::default().generate(&element(&def)).unwrap();
        assert_eq!(g.solids.len(), 1);
        let m = &g.solids[0];
        assert_eq!(m.name, "Drain lining");
        // Closed and wound outwards: the mesh volume is the lining's section × length,
        // (1.127389 − 0.84) m² × 50 m = 14.369 m³.
        close(m.volume() / 1.0e9, (1.127389 - 0.84) * 50.0, 1e-3);
        // Top edges at z = 0, the excavation bed 700 mm down.
        let zs: Vec<f64> = m.vertices.iter().map(|v| v[2]).collect();
        assert_eq!(zs.iter().cloned().fold(f64::NEG_INFINITY, f64::max), 0.0);
        close(zs.iter().cloned().fold(f64::INFINITY, f64::min), -700.0, 1e-9);
        // The frame starts at the first route point and runs 50 m east.
        let xs: Vec<f64> = m.vertices.iter().map(|v| v[0]).collect();
        close(xs.iter().cloned().fold(f64::INFINITY, f64::min), 0.0, 1e-6);
        close(xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max), 50_000.0, 1e-6);
        // Looking east, right is south: the right-hand top edge of the excavation is at
        // y = −(250 + 900 + 100 × √3.25) = −1330.28 mm.
        let ys: Vec<f64> = m.vertices.iter().map(|v| v[1]).collect();
        close(ys.iter().cloned().fold(f64::INFINITY, f64::min), -1330.278, 1e-3);

        // On the curve the trough still closes, and holds a little under section × 102.5 m.
        let curved = routed(SideDrainDefinition { start_station_m: 100.0, end_station_m: 200.0, ..lined() });
        let v = curved.solids()[0].volume() / 1.0e9;
        let exact = (1.127389 - 0.84) * 102.5;
        assert!(v > exact * 0.999 && v < exact * 1.001, "expected about {exact}, got {v}");

        // An unlined drain shows a display skin, named and coloured as a cut.
        let unlined = routed(SideDrainDefinition { lining: Lining::Table(ChannelMaterial::StiffClay), lining_thickness_mm: None, ..lined() });
        let s = unlined.solids();
        assert_eq!(s[0].name, "Drain");
        assert!(s[0].volume() > 0.0);
        // A V drain has no bed: the degenerate faces are left out and the solid still closes.
        let v_drain = routed(SideDrainDefinition { section: ChannelSection::V { depth_mm: 400.0, left_side_slope: 2.0, right_side_slope: 2.0 }, ..lined() });
        let lining = v_drain.excavation_area_m2() - v_drain.finished_area_m2();
        close(v_drain.solids()[0].volume() / 1.0e9, lining * 50.0, 1e-3);
    }

    #[test]
    fn module_reports_quantities_and_full_depth_capacity() {
        let def = routed(lined());
        let g = SideDrainModule::default().generate(&element(&def)).unwrap();
        assert_eq!(g.derived["Section"], "Trapezoidal b500 d600 1.5:1/1.5:1");
        assert_eq!(g.derived["Top width"], "2300 mm");
        assert_eq!(g.derived["Lining"], "Concrete, 100 mm");
        assert_eq!(g.derived["Gradient"], "1.00 %");
        assert_eq!(g.derived["Length along the drain"], "50.000 m");
        // 1.127389 × 50 = 56.37 m³; 2.663331 × 50 = 133.17 m²; 0.287389 × 50 = 14.37 m³.
        assert_eq!(g.derived["Excavation"], "56.37 m³");
        assert_eq!(g.derived["Lining area"], "133.17 m²");
        assert_eq!(g.derived["Lining volume"], "14.37 m³");
        // Full depth: A = 0.84, P = 2.663331, R = 0.315394, R^(2/3) = 0.463345;
        // V = 0.463345 × 0.1 / 0.015 = 3.089 m/s; Q = 2.595 m³/s. Above 3.0 m/s for concrete.
        assert_eq!(g.derived["Velocity at full depth"], "3.09 m/s");
        assert_eq!(g.derived["Capacity at full depth (Manning)"], "2.595 m³/s");
        let w = g.notes.iter().find(|n| n.code == "DRAIN_VELOCITY_ABOVE_PERMISSIBLE").expect("the velocity warning");
        assert_eq!(w.severity, Severity::Warning);
        assert!(w.message.contains("Table 9.1") && w.message.contains("3 m/s"));
        // Drawing: excavation top width 250 + 900 + 180.278 each side = 2660.555, margin 15 %.
        let top = 2.0 * (250.0 + 900.0 + 100.0 * 3.25f64.sqrt());
        close(g.drawing.extents.max.x, top * 1.3, 1e-6);
        close(g.drawing.extents.max.y, 700.0, 1e-9);
        assert!(matches!(g.drawing.primitives[0], Primitive::Fill { colour: Colour::Grey, .. }));
    }

    #[test]
    fn an_unrouted_or_stale_drain_is_drawn_with_a_warning() {
        let g = SideDrainModule::default().generate(&element(&lined())).unwrap();
        assert!(g.notes.iter().any(|n| n.code == "DRAIN_NOT_ROUTED" && n.severity == Severity::Warning));
        assert!(g.solids.is_empty() && !g.drawing.primitives.is_empty());
        assert!(!g.derived.contains_key("Excavation"));
        let mut def = routed(lined());
        def.offset_m = 6.0;
        assert!(def.notes().iter().any(|n| n.code == "DRAIN_ROUTE_OUT_OF_DATE"));
        def.route_along(&alignment()).unwrap();
        assert!(def.notes().is_empty(), "{:?}", def.notes());
    }

    #[test]
    fn invalid_definitions_give_error_notes_and_no_geometry() {
        let codes = |d: &SideDrainDefinition| d.notes().into_iter().filter(|n| n.severity == Severity::Error).map(|n| n.code).collect::<Vec<_>>();
        assert_eq!(codes(&SideDrainDefinition { end_station_m: 10.0, ..lined() }), ["DRAIN_STATIONS_NOT_IN_ORDER"]);
        assert_eq!(codes(&SideDrainDefinition { lining_thickness_mm: Some(-5.0), ..lined() }), ["DRAIN_LINING_THICKNESS_NEGATIVE"]);
        assert_eq!(codes(&SideDrainDefinition { gradient_m_per_m: -0.01, ..lined() }), ["DRAIN_GRADIENT_NEGATIVE"]);
        assert_eq!(codes(&SideDrainDefinition { manning_n: Some(0.0), ..lined() }), ["DRAIN_ROUGHNESS_NOT_POSITIVE"]);
        let bad = SideDrainDefinition { section: ChannelSection::Rectangular { width_mm: 0.0, depth_mm: 500.0 }, ..lined() };
        assert!(bad.check().is_err());
        let g = SideDrainModule::default().generate(&element(&bad)).unwrap();
        assert!(g.solids.is_empty() && g.drawing.primitives.is_empty() && g.derived.is_empty());
        let mut e = element(&lined());
        e.data = serde_json::json!({"section": {"type": "Nonsense"}});
        assert!(SideDrainModule::default().generate(&e).is_err());
    }

    #[test]
    fn placeholder_is_flagged_and_serde_round_trips() {
        let p = SideDrainDefinition::placeholder("MC10", 0.0, 100.0, -4.5);
        assert!(p.placeholder);
        let g = SideDrainModule::default().generate(&element(&routed(p.clone()))).unwrap();
        assert!(g.notes.iter().any(|n| n.code == NOTE_PLACEHOLDER && n.severity == Severity::Info));
        assert!(g.notes.iter().any(|n| n.code == "DRAIN_LINING_NOT_STATED"));
        assert!(g.notes.iter().any(|n| n.code == "DRAIN_GRADIENT_NOT_ENTERED"));
        assert!(!g.solids.is_empty(), "a placeholder drain is still drawable");
        assert!(!g.derived.contains_key("Capacity at full depth (Manning)"));

        let def = routed(lined());
        assert_eq!(SideDrainDefinition::from_value(&def.to_value()).unwrap(), def);
        assert!(!def.to_value().as_object().unwrap().contains_key("placeholder"));
        let other = SideDrainDefinition { lining: Lining::Other("stone pitching".into()), ..lined() };
        assert_eq!(SideDrainDefinition::from_value(&other.to_value()).unwrap(), other);
        // A definition written without the optional fields still reads.
        let minimal = serde_json::json!({
            "alignment": "MC10", "start_station_m": 0.0, "end_station_m": 20.0, "offset_m": 4.0,
            "section": {"type": "V", "depth_mm": 300.0, "left_side_slope": 3.0, "right_side_slope": 1.0},
            "lining": {"Other": "grass"}, "gradient_m_per_m": 0.02
        });
        let read = SideDrainDefinition::from_value(&minimal).unwrap();
        assert_eq!((read.manning_n, read.lining_thickness_mm, read.route.is_none(), read.placeholder), (None, None, true, false));
    }

    #[test]
    fn capacity_check_takes_the_table_9_1_velocity_for_the_lining() {
        let def = lined();
        let input = def.channel_input(DesignBasis::Mowt, 1.0, Some(0.15)).unwrap();
        assert_eq!(input.permissible_velocity_m_s, Some(3.0));
        assert_eq!(input.permissible_source, "DDM Table 9.1, concrete");
        assert_eq!((input.depth_m, input.manning_n, input.gradient_m_per_m), (0.6, 0.015, 0.01));
        let c = def.check_capacity(DesignBasis::Mowt, 1.0, Some(0.15)).unwrap();
        assert!(c.capacity_ok && c.normal_depth_m < 0.6);
        // Under the DMRB basis there is no permissible velocity to take, and the calculation
        // cites CD 521; the hydraulics are the same.
        let dmrb = def.check_capacity(DesignBasis::Dmrb, 1.0, Some(0.15)).unwrap();
        assert_eq!(dmrb.input.permissible_velocity_m_s, None);
        assert_eq!(dmrb.normal_depth_m, c.normal_depth_m);
        assert!(dmrb.calculation().iter().any(|l| l.contains("CD 521 Equation 5.25")));
        let g = SideDrainModule::with_basis(DesignBasis::Dmrb).generate(&element(&routed(lined()))).unwrap();
        assert!(g.notes.iter().all(|n| n.code != "DRAIN_VELOCITY_ABOVE_PERMISSIBLE"));
        assert_eq!(g.derived["Velocity at full depth"], "3.09 m/s");
        // Free-text lining: no permissible velocity; no n: refused.
        let other = SideDrainDefinition { lining: Lining::Other("grass".into()), ..lined() };
        assert_eq!(other.channel_input(DesignBasis::Mowt, 1.0, None).unwrap().permissible_velocity_m_s, None);
        assert!(SideDrainDefinition { manning_n: None, ..lined() }.check_capacity(DesignBasis::Mowt, 1.0, None).is_err());
    }
}
