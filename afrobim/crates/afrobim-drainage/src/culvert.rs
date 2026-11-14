//! Pipe culverts: the definition stored in an element's `data` and the kernel module that
//! draws it.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use afrobim_alignment::Alignment;
use afrobim_draw::{Colour, Drawing, Extents, Mesh3, Primitive, Pt2};
use afrobim_kernel::{Element, Generated, Module, Note, SitePlacement};
use serde::{Deserialize, Serialize};

use crate::design::mowt::{culvert_structure_type, outlet_control, OutletControl, OutletControlInput};
use crate::design::DesignBasis;
use crate::{
    error_note, error_text, fixed, info_note, not_negative, positive, warning_note, DrainageError, NOTE_PLACEHOLDER, PLACEHOLDER_CULVERT_DIAMETER_MM,
    PLACEHOLDER_CULVERT_LENGTH_MM, PLACEHOLDER_CULVERT_WALL_MM, ROUND_SEGMENTS,
};

/// The element category [`PipeCulvertModule`] owns.
pub const CULVERT_CATEGORY: &str = "culvert";

/// Display colour of a pipe in the 3D view. A screen colour, not a finish.
const PIPE_RGB: [u8; 3] = [0xA8, 0xAB, 0xAE];

/// A side of the road, looking up-station.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Left,
    Right,
}

/// A pipe culvert as the engineer describes it: one or more parallel circular barrels crossing
/// an alignment. Headwalls, wingwalls, aprons and bedding are not modelled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipeCulvertDefinition {
    /// Name of the alignment the culvert crosses.
    pub alignment: String,
    /// Station at which the culvert's centre line crosses the alignment, m.
    pub station_m: f64,
    /// Skew, degrees: the angle between the culvert and the line square to the alignment
    /// (0 = a square crossing), positive turning the culvert counter-clockwise in plan.
    #[serde(default)]
    pub skew_deg: f64,
    /// Number of barrels.
    pub barrels: u32,
    pub internal_diameter_mm: f64,
    pub wall_thickness_mm: f64,
    /// Length of each barrel, measured along the pipe, mm.
    pub length_mm: f64,
    /// Distance between the axes of adjacent barrels, mm. Needed when there is more than one.
    #[serde(default)]
    pub barrel_spacing_mm: Option<f64>,
    /// Invert level at the inlet, m above datum.
    pub invert_level_inlet_m: f64,
    /// Invert level at the outlet, m above datum.
    pub invert_level_outlet_m: f64,
    /// The side of the road the water comes from.
    pub inlet_side: Side,
    /// Offset from the alignment of the mid-length of the culvert, m, positive right. Zero
    /// centres the culvert on the alignment.
    #[serde(default)]
    pub centre_offset_m: f64,
    /// Finished road level on the alignment over the culvert, m above datum, if entered. It
    /// gives the cover shown with the element and puts the culvert below the road in the 3D
    /// view, where the road is at level zero.
    #[serde(default)]
    pub road_level_m: Option<f64>,
    /// Free text, e.g. the pipe class the specification calls for. Not interpreted.
    #[serde(default)]
    pub material: String,
    /// True while the dimensions are still the `PLACEHOLDER_*` starting values.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub placeholder: bool,
}

impl PipeCulvertDefinition {
    /// A drawable starting definition made of the `PLACEHOLDER_*` constants, flagged as such:
    /// one level barrel square to the alignment. It is not a recommended or adequate culvert.
    pub fn placeholder(alignment: &str, station_m: f64) -> Self {
        PipeCulvertDefinition {
            alignment: alignment.to_string(),
            station_m,
            skew_deg: 0.0,
            barrels: 1,
            internal_diameter_mm: PLACEHOLDER_CULVERT_DIAMETER_MM,
            wall_thickness_mm: PLACEHOLDER_CULVERT_WALL_MM,
            length_mm: PLACEHOLDER_CULVERT_LENGTH_MM,
            barrel_spacing_mm: None,
            invert_level_inlet_m: 0.0,
            invert_level_outlet_m: 0.0,
            inlet_side: Side::Left,
            centre_offset_m: 0.0,
            road_level_m: None,
            material: String::new(),
            placeholder: true,
        }
    }

    pub fn from_value(data: &serde_json::Value) -> Result<Self, DrainageError> {
        serde_json::from_value(data.clone()).map_err(|e| DrainageError::InvalidDefinition(format!("the culvert definition cannot be read: {e}")))
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a culvert definition always serialises")
    }

    pub fn outside_diameter_mm(&self) -> f64 {
        self.internal_diameter_mm + 2.0 * self.wall_thickness_mm
    }

    /// Drop from the inlet invert to the outlet invert, m. Negative when the outlet is higher.
    pub fn fall_m(&self) -> f64 {
        self.invert_level_inlet_m - self.invert_level_outlet_m
    }

    /// Plan length of a barrel, mm: `√(L² − fall²)`.
    pub fn plan_length_mm(&self) -> f64 {
        let fall_mm = self.fall_m() * 1000.0;
        (self.length_mm * self.length_mm - fall_mm * fall_mm).max(0.0).sqrt()
    }

    /// Gradient of the barrel, m/m: fall over plan length.
    pub fn gradient_m_per_m(&self) -> f64 {
        self.fall_m() * 1000.0 / self.plan_length_mm()
    }

    /// Total clear opening, m: barrels × internal diameter (the "span" of Note 1 to MoWT
    /// Drainage Design Manual Table 3.2; the DMRB documents define no such sum).
    pub fn total_opening_m(&self) -> f64 {
        self.barrels as f64 * self.internal_diameter_mm / 1000.0
    }

    /// Distance between barrel axes used for the geometry, mm; zero for a single barrel.
    fn spacing_mm(&self) -> f64 {
        if self.barrels > 1 {
            self.barrel_spacing_mm.unwrap_or(0.0)
        } else {
            0.0
        }
    }

    /// Overall width across the barrels, mm.
    pub fn overall_width_mm(&self) -> f64 {
        self.outside_diameter_mm() + self.barrels.saturating_sub(1) as f64 * self.spacing_mm()
    }

    /// Validation and information notes, with the layout checks of the chosen design basis
    /// ([`PipeCulvertDefinition::layout_notes`]). Error notes mean the culvert cannot be drawn
    /// or measured, whatever the basis.
    pub fn notes(&self, basis: DesignBasis) -> Vec<Note> {
        let mut out = self.validity_notes();
        if error_text(&out).is_some() {
            return out;
        }
        out.extend(self.layout_notes(basis));
        if self.road_level_m.is_none() {
            out.push(info_note("CULVERT_ROAD_LEVEL_NOT_ENTERED", "No road level is entered, so no cover is shown and the 3D view puts the inlet invert at level zero."));
        }
        if self.placeholder {
            out.push(info_note(
                NOTE_PLACEHOLDER,
                "The culvert dimensions and levels are placeholders so that the element can be drawn. They are not design values; the engineer must set the barrels, size, length and invert levels.",
            ));
        }
        if self.material.trim().is_empty() {
            out.push(info_note("CULVERT_MATERIAL_NOT_STATED", "No pipe material is stated for this culvert."));
        }
        out
    }

    /// Error notes for a definition that cannot be drawn or measured. They do not depend on
    /// the design basis.
    fn validity_notes(&self) -> Vec<Note> {
        let mut out = Vec::new();
        if self.barrels == 0 {
            out.push(error_note("CULVERT_NO_BARRELS", "A culvert needs at least one barrel.".to_string()));
        }
        if !positive(self.internal_diameter_mm) {
            out.push(error_note("CULVERT_DIAMETER_NOT_POSITIVE", format!("The internal diameter must be greater than zero (it is {}).", self.internal_diameter_mm)));
        }
        if !not_negative(self.wall_thickness_mm) {
            out.push(error_note("CULVERT_WALL_NEGATIVE", format!("The wall thickness must not be negative (it is {}).", self.wall_thickness_mm)));
        }
        if !positive(self.length_mm) {
            out.push(error_note("CULVERT_LENGTH_NOT_POSITIVE", format!("The barrel length must be greater than zero (it is {}).", self.length_mm)));
        }
        if !(self.station_m.is_finite() && self.skew_deg.is_finite() && self.centre_offset_m.is_finite()) {
            out.push(error_note("CULVERT_POSITION_NOT_A_NUMBER", "The station, skew and centre offset must be numbers.".to_string()));
        } else if self.skew_deg.abs() >= 90.0 {
            out.push(error_note("CULVERT_SKEW_TOO_LARGE", format!("A skew of {}° does not cross the alignment.", self.skew_deg)));
        }
        if !(self.invert_level_inlet_m.is_finite() && self.invert_level_outlet_m.is_finite()) {
            out.push(error_note("CULVERT_LEVELS_NOT_NUMBERS", "The invert levels must be numbers.".to_string()));
        } else if positive(self.length_mm) && self.fall_m().abs() * 1000.0 >= self.length_mm {
            out.push(error_note(
                "CULVERT_FALL_NOT_LESS_THAN_LENGTH",
                format!("A level difference of {} m cannot be made by a barrel {} m long.", fixed(self.fall_m().abs(), 3, 3), fixed(self.length_mm / 1000.0, 3, 3)),
            ));
        }
        if self.barrels > 1 {
            match self.barrel_spacing_mm {
                None => out.push(error_note("CULVERT_SPACING_NOT_ENTERED", "Enter the distance between barrel axes for a culvert of more than one barrel.".to_string())),
                Some(s) if !(s.is_finite() && s >= self.outside_diameter_mm()) => out.push(error_note(
                    "CULVERT_BARRELS_OVERLAP",
                    format!("Barrel axes {s} mm apart overlap: the outside diameter is {} mm.", fixed(self.outside_diameter_mm(), 0, 1)),
                )),
                Some(_) => {}
            }
        }
        if let Some(level) = self.road_level_m {
            if !level.is_finite() {
                out.push(error_note("CULVERT_LEVELS_NOT_NUMBERS", "The road level must be a number.".to_string()));
            }
        }
        out
    }

    /// The layout limits the chosen basis states in words ([`DesignBasis::culvert_layout_limits`]),
    /// as Warning or Info notes that quote the clause. None is a hydraulic check, and a limit the
    /// basis' documents do not state raises nothing. Empty for a definition with Error notes.
    pub fn layout_notes(&self, basis: DesignBasis) -> Vec<Note> {
        let mut out = Vec::new();
        if error_text(&self.validity_notes()).is_some() {
            return out;
        }
        let limits = basis.culvert_layout_limits();
        let d_m = self.internal_diameter_mm / 1000.0;
        let diameter = fixed(self.internal_diameter_mm, 0, 1);
        if let Some(l) = limits.minimum_diameter_m {
            if d_m < l.value {
                out.push(warning_note("CULVERT_BELOW_MINIMUM_DIAMETER", format!("The internal diameter is {diameter} mm. {}.", l.citation.trim_end_matches('.'))));
            }
        }
        if let Some(rule) = limits.long_culvert {
            if self.length_mm / 1000.0 > rule.longer_than_m && d_m < rule.minimum_diameter_m {
                out.push(warning_note(
                    "CULVERT_TOO_SMALL_FOR_ITS_LENGTH",
                    format!("The barrel is {} m long and {diameter} mm in diameter. {}.", fixed(self.length_mm / 1000.0, 2, 3), rule.citation.trim_end_matches('.')),
                ));
            }
        }
        if let Some(l) = limits.structure_above_diameter_m {
            if d_m > l.value {
                out.push(info_note("CULVERT_IS_A_STRUCTURE", &format!("The internal diameter is {diameter} mm. {}.", l.citation.trim_end_matches('.'))));
            }
        }
        if let Some(l) = limits.maximum_skew_deg {
            if self.skew_deg.abs() > l.value {
                out.push(warning_note("CULVERT_SKEW_ABOVE_LIMIT", format!("The skew is {}°. {}.", fixed(self.skew_deg, 0, 2), l.citation.trim_end_matches('.'))));
            }
        }
        let s = self.gradient_m_per_m();
        let percent = fixed(s * 100.0, 2, 2);
        let below = |l: Option<crate::design::Limit>| l.filter(|l| s < l.value);
        let above = |l: Option<crate::design::Limit>| l.filter(|l| s > l.value);
        if s < 0.0 {
            out.push(warning_note("CULVERT_FALLS_TOWARDS_INLET", format!("The outlet invert is {} m above the inlet invert.", fixed(-self.fall_m(), 3, 3))));
        } else if let Some(l) = below(limits.gradient_not_below) {
            out.push(warning_note("CULVERT_GRADIENT_BELOW_MINIMUM", format!("The gradient is {percent} %. {}.", l.citation.trim_end_matches('.'))));
        } else if let Some(l) = below(limits.gradient_avoid_below) {
            out.push(info_note("CULVERT_GRADIENT_FLAT", &format!("The gradient is {percent} %. {}.", l.citation.trim_end_matches('.'))));
        } else if let Some(l) = above(limits.gradient_special_anchors_above) {
            out.push(warning_note("CULVERT_GRADIENT_NEEDS_SPECIAL_ANCHORS", format!("The gradient is {percent} %. {}.", l.citation.trim_end_matches('.'))));
        } else if let Some(l) = above(limits.gradient_anchors_above) {
            out.push(info_note("CULVERT_GRADIENT_NEEDS_ANCHORS", &format!("The gradient is {percent} %. {}.", l.citation.trim_end_matches('.'))));
        }
        if basis == DesignBasis::Mowt && culvert_structure_type(self.barrels, d_m).is_none() {
            out.push(info_note(
                "CULVERT_OPENING_SIX_METRES_OR_MORE",
                &format!("The total clear opening is {} m. DDM Table 3.2 (Note 1) classes an opening of 6 m or more with bridges, and Section 3.2.2 puts it in the major drainage system.", fixed(self.total_opening_m(), 2, 2)),
            ));
        }
        out
    }

    /// Ok when the definition has no Error notes.
    pub fn check(&self) -> Result<(), DrainageError> {
        match error_text(&self.validity_notes()) {
            Some(text) => Err(DrainageError::InvalidDefinition(text)),
            None => Ok(()),
        }
    }

    /// Cover over the pipe on the alignment, m: road level less the level of the outside top
    /// of the pipe where the alignment crosses it. None without a road level.
    ///
    /// The invert there is interpolated along the barrel from the mid-length, moved by the
    /// centre offset: `invert = inlet − fall · (½ ∓ offset / (plan length · cos skew))`. The
    /// top of the pipe is taken as invert + internal diameter + wall (the slope of the pipe is
    /// ignored in that sum).
    pub fn cover_at_alignment_m(&self) -> Option<f64> {
        let road = self.road_level_m?;
        let plan_m = self.plan_length_mm() / 1000.0;
        // Distance along the plan axis from the inlet to the alignment. The mid-length is
        // `centre_offset` to the right; the inlet is on `inlet_side`.
        let towards_right = self.centre_offset_m / self.skew_deg.to_radians().cos();
        let from_inlet = match self.inlet_side {
            Side::Left => plan_m / 2.0 - towards_right,
            Side::Right => plan_m / 2.0 + towards_right,
        };
        let invert = self.invert_level_inlet_m - self.fall_m() * from_inlet / plan_m;
        Some(road - (invert + (self.internal_diameter_mm + self.wall_thickness_mm) / 1000.0))
    }

    /// Where the culvert lies on site: the inlet end of its centre line, with the rotation that
    /// points the solids' +X along the plan direction of flow.
    ///
    /// The culvert's axis is square to the alignment at the station, turned by the skew; its
    /// mid-length is at the centre offset. `z` is the inlet invert level relative to the road
    /// level in millimetres when a road level is entered (so the road is at zero, as the rest
    /// of the site model is), and zero otherwise. The placement's `station_m` and `offset_m`
    /// are those of the crossing, not of the inlet.
    pub fn site_placement(&self, alignment: &Alignment) -> Result<SitePlacement, DrainageError> {
        self.check()?;
        let Some(at) = alignment.point_at(self.station_m) else {
            return Err(DrainageError::Route(format!(
                "station {} is not on alignment '{}' ({} to {})",
                self.station_m,
                alignment.name,
                alignment.start_station_m,
                alignment.end_station_m()
            )));
        };
        let (cx, cy) = at.offset(self.centre_offset_m);
        // Towards the right of the alignment is heading − 90°.
        let to_right = at.heading_rad - PI / 2.0 + self.skew_deg.to_radians();
        let flow = match self.inlet_side {
            Side::Left => to_right,
            Side::Right => to_right + PI,
        };
        let half_m = self.plan_length_mm() / 2000.0;
        Ok(SitePlacement {
            x: (cx - half_m * flow.cos()) * 1000.0,
            y: (cy - half_m * flow.sin()) * 1000.0,
            z: self.road_level_m.map_or(0.0, |road| (self.invert_level_inlet_m - road) * 1000.0),
            rotation_deg: flow.to_degrees().rem_euclid(360.0),
            alignment: Some(alignment.name.clone()),
            station_m: Some(self.station_m),
            offset_m: Some(self.centre_offset_m),
        })
    }

    /// The outlet-control headwater of this culvert by the MoWT manual ([`outlet_control`]; the
    /// DMRB basis has no culvert hydraulics, see [`crate::design::dmrb`]) for a total design
    /// flow, with the barrel count, diameter, length and fall of the definition. Manning's n,
    /// the entrance loss coefficient and the tailwater depth are inputs.
    pub fn outlet_control(&self, design_flow_m3_s: f64, manning_n: f64, entrance_loss_ke: f64, tailwater_m: f64) -> Result<OutletControl, DrainageError> {
        self.check()?;
        outlet_control(&OutletControlInput {
            design_flow_m3_s,
            barrels: self.barrels,
            diameter_m: self.internal_diameter_mm / 1000.0,
            length_m: self.length_mm / 1000.0,
            fall_m: self.fall_m(),
            manning_n,
            entrance_loss_ke,
            tailwater_m,
        })
    }

    /// Positions of the barrel axes across the culvert, mm from its centre line, left to right
    /// looking along the flow.
    fn barrel_offsets_mm(&self) -> Vec<f64> {
        let n = self.barrels as f64;
        (0..self.barrels).map(|i| ((n - 1.0) / 2.0 - i as f64) * self.spacing_mm()).collect()
    }

    /// The barrels for the 3D view, one hollow pipe each, in the frame the crate documentation
    /// describes: x from 0 at the inlet to the plan length at the outlet, y = 0 on the centre
    /// line of the group, z = 0 at the inlet invert, the pipes falling to the outlet. Each end
    /// is cut square to the pipe.
    pub fn solids(&self) -> Vec<Mesh3> {
        let (ri, ro) = (self.internal_diameter_mm / 2.0, self.outside_diameter_mm() / 2.0);
        let (plan, fall) = (self.plan_length_mm(), self.fall_m() * 1000.0);
        let axis = [plan / self.length_mm, 0.0, -fall / self.length_mm];
        // e1 × e2 = axis, with e1 across the culvert and e2 upward, square to the pipe.
        let e1 = [0.0, 1.0, 0.0];
        let e2 = [-axis[2], 0.0, axis[0]];
        let n = ROUND_SEGMENTS;
        self.barrel_offsets_mm()
            .into_iter()
            .enumerate()
            .map(|(index, y)| {
                let start = [0.0, y, ri];
                let end = [plan, y, ri - fall];
                let mut vertices = Vec::with_capacity(4 * n as usize);
                // Rings: outer start, outer end, inner start, inner end.
                for (centre, r) in [(start, ro), (end, ro), (start, ri), (end, ri)] {
                    for i in 0..n {
                        let a = i as f64 / n as f64 * 2.0 * PI;
                        let (c, s) = (r * a.cos(), r * a.sin());
                        vertices.push([centre[0] + c * e1[0] + s * e2[0], centre[1] + c * e1[1] + s * e2[1], centre[2] + c * e1[2] + s * e2[2]]);
                    }
                }
                let mut triangles = Vec::with_capacity(8 * n as usize);
                for i in 0..n {
                    let j = (i + 1) % n;
                    let (os_i, os_j, oe_i, oe_j) = (i, j, n + i, n + j);
                    let (is_i, is_j, ie_i, ie_j) = (2 * n + i, 2 * n + j, 3 * n + i, 3 * n + j);
                    // Outside face.
                    triangles.push([os_i, os_j, oe_j]);
                    triangles.push([os_i, oe_j, oe_i]);
                    // Bore, facing the axis.
                    triangles.push([is_i, ie_j, is_j]);
                    triangles.push([is_i, ie_i, ie_j]);
                    // Inlet end ring, facing upstream.
                    triangles.push([is_i, is_j, os_j]);
                    triangles.push([is_i, os_j, os_i]);
                    // Outlet end ring, facing downstream.
                    triangles.push([ie_i, oe_j, ie_j]);
                    triangles.push([ie_i, oe_i, oe_j]);
                }
                Mesh3 { name: format!("Barrel {}", index + 1), vertices, triangles, colour: PIPE_RGB }
            })
            .collect()
    }

    /// Elevation for the design sheet, mm: a section along one barrel seen from its right-hand
    /// side, the inlet at x = 0, the lowest point of the pipe on y = 0. The wall is filled grey
    /// with the bore left open, the invert and soffit are lines, and the road level, when
    /// entered, is a brown line across the top. The ends are drawn vertical.
    pub fn drawing(&self) -> Drawing {
        let (d, t) = (self.internal_diameter_mm, self.wall_thickness_mm);
        let (plan, fall) = (self.plan_length_mm(), self.fall_m() * 1000.0);
        // y of the outside bottom at the inlet and at the outlet.
        let (y_in, y_out) = (fall.max(0.0), (-fall).max(0.0));
        let od = d + 2.0 * t;
        let thin = (od * 0.01).max(2.0);
        let band = |a: f64, b: f64, colour: Colour| Primitive::Fill {
            outline: vec![Pt2::new(0.0, y_in + a), Pt2::new(plan, y_out + a), Pt2::new(plan, y_out + b), Pt2::new(0.0, y_in + b)],
            colour,
        };
        let mut primitives = vec![band(0.0, od, Colour::Grey)];
        if t > 0.0 {
            primitives.push(band(t, t + d, Colour::White));
        }
        for h in [t, t + d] {
            primitives.push(Primitive::Line { from: Pt2::new(0.0, y_in + h), to: Pt2::new(plan, y_out + h), width: thin, colour: None });
        }
        let mut top = y_in.max(y_out) + od;
        if let Some(road) = self.road_level_m {
            // Datum of the drawing: y = 0 is the lower outside bottom.
            let lowest_invert = self.invert_level_inlet_m.min(self.invert_level_outlet_m);
            let y_road = (road - lowest_invert) * 1000.0 + t;
            if y_road.is_finite() && y_road > 0.0 {
                primitives.push(Primitive::Line { from: Pt2::new(0.0, y_road), to: Pt2::new(plan, y_road), width: thin, colour: Some(Colour::Brown) });
                top = top.max(y_road);
            }
        }
        Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(plan, top) } }
    }
}

/// The kernel module for category [`CULVERT_CATEGORY`].
///
/// `generate` reads a [`PipeCulvertDefinition`] from the element's `data`. A definition with
/// Error notes yields the notes and no geometry. The layout notes are those of the module's
/// design basis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipeCulvertModule {
    /// The basis whose layout limits are checked. The engineer's choice; [`Default`] is MoWT.
    pub basis: DesignBasis,
}

impl Default for PipeCulvertModule {
    /// Uses [`DesignBasis::Mowt`].
    fn default() -> Self {
        PipeCulvertModule { basis: DesignBasis::Mowt }
    }
}

impl PipeCulvertModule {
    pub fn with_basis(basis: DesignBasis) -> Self {
        PipeCulvertModule { basis }
    }
}

impl Module for PipeCulvertModule {
    fn category(&self) -> &str {
        CULVERT_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Pipe culverts"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let def = PipeCulvertDefinition::from_value(&element.data).map_err(|e| e.to_string())?;
        let notes = def.notes(self.basis);
        if error_text(&notes).is_some() {
            return Ok(Generated { notes, ..Generated::default() });
        }
        let mut derived = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            derived.insert(k.to_string(), v);
        };
        put("Design basis for layout checks", self.basis.name().to_string());
        let d_m = def.internal_diameter_mm / 1000.0;
        put("Barrels", def.barrels.to_string());
        put("Internal diameter", format!("{} mm", fixed(def.internal_diameter_mm, 0, 1)));
        put("Outside diameter", format!("{} mm", fixed(def.outside_diameter_mm(), 0, 1)));
        put("Length per barrel", format!("{} m", fixed(def.length_mm / 1000.0, 3, 3)));
        put("Total pipe length", format!("{} m", fixed(def.barrels as f64 * def.length_mm / 1000.0, 3, 3)));
        put("Fall, inlet to outlet", format!("{} m", fixed(def.fall_m(), 3, 3)));
        put("Gradient", format!("{} %", fixed(def.gradient_m_per_m() * 100.0, 2, 2)));
        put("Skew", format!("{}°", fixed(def.skew_deg, 0, 2)));
        put("Waterway area, all barrels", format!("{} m²", fixed(def.barrels as f64 * PI * d_m * d_m / 4.0, 3, 3)));
        put("Total clear opening", format!("{} m", fixed(def.total_opening_m(), 2, 3)));
        if self.basis == DesignBasis::Mowt {
            if let Some(kind) = culvert_structure_type(def.barrels, d_m) {
                put("Structure type (DDM Table 3.2)", kind.label().to_string());
            }
        }
        if let Some(cover) = def.cover_at_alignment_m() {
            put("Cover on the alignment", format!("{} m", fixed(cover, 3, 3)));
        }
        Ok(Generated { drawing: def.drawing(), notes, derived, solids: def.solids() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_alignment::AlignmentBuilder;
    use afrobim_kernel::{Document, Pt, Severity};

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!((actual - expected).abs() <= tolerance, "expected {expected} ± {tolerance}, got {actual}");
    }

    /// 200 m heading east from (1000, 2000).
    fn alignment() -> Alignment {
        AlignmentBuilder::new("MC10", 0.0, 1000.0, 2000.0, 0.0).line(200.0).build().unwrap()
    }

    /// Two 900 mm pipes, 12 m long, falling 0.24 m from the left to the right.
    fn twin() -> PipeCulvertDefinition {
        PipeCulvertDefinition {
            alignment: "MC10".into(),
            station_m: 50.0,
            skew_deg: 0.0,
            barrels: 2,
            internal_diameter_mm: 900.0,
            wall_thickness_mm: 100.0,
            length_mm: 12_000.0,
            barrel_spacing_mm: Some(1500.0),
            invert_level_inlet_m: 1150.24,
            invert_level_outlet_m: 1150.00,
            inlet_side: Side::Left,
            centre_offset_m: 0.0,
            road_level_m: Some(1152.5),
            material: "as specified".into(),
            placeholder: false,
        }
    }

    fn element(def: &PipeCulvertDefinition) -> Element {
        let mut d = Document::default();
        let id = d.add(CULVERT_CATEGORY, "C-01", Pt::new(0.0, 0.0), 1.0, 1.0, "Culverts", def.to_value());
        d.find(id).unwrap().clone()
    }

    #[test]
    fn derived_lengths_fall_and_gradient() {
        let c = twin();
        close(c.fall_m(), 0.24, 1e-9);
        // Plan length √(12² − 0.24²) = 11.99760 m; gradient 0.24 / 11.9976 = 2.0004 %.
        close(c.plan_length_mm(), 11_997.600, 1e-3);
        close(c.gradient_m_per_m(), 0.020004, 1e-6);
        assert_eq!(c.outside_diameter_mm(), 1100.0);
        assert_eq!(c.overall_width_mm(), 2600.0);
        close(c.total_opening_m(), 1.8, 1e-12);
        // Cover: the invert on the alignment is half-way, 1150.12; top of pipe 1150.12 + 0.9 +
        // 0.1 = 1151.12; cover 1152.5 − 1151.12 = 1.38 m.
        close(c.cover_at_alignment_m().unwrap(), 1.38, 1e-9);
        // With the mid-length 2 m right of the alignment the crossing is 2 m nearer the inlet
        // (which is on the left): invert 1150.24 − 0.24 × (5.9988 − 2) / 11.9976 = 1150.16001.
        let shifted = PipeCulvertDefinition { centre_offset_m: 2.0, ..twin() };
        close(shifted.cover_at_alignment_m().unwrap(), 1152.5 - (1150.16001 + 1.0), 1e-5);
        assert_eq!(PipeCulvertDefinition { road_level_m: None, ..twin() }.cover_at_alignment_m(), None);

        let g = PipeCulvertModule::default().generate(&element(&c)).unwrap();
        assert_eq!(g.derived["Barrels"], "2");
        assert_eq!(g.derived["Length per barrel"], "12.000 m");
        assert_eq!(g.derived["Total pipe length"], "24.000 m");
        assert_eq!(g.derived["Fall, inlet to outlet"], "0.240 m");
        assert_eq!(g.derived["Gradient"], "2.00 %");
        // 2 × π × 0.9² / 4 = 1.272 m².
        assert_eq!(g.derived["Waterway area, all barrels"], "1.272 m²");
        assert_eq!(g.derived["Total clear opening"], "1.80 m");
        assert_eq!(g.derived["Structure type (DDM Table 3.2)"], "Culvert, pipe, span < 2 m");
        assert_eq!(g.derived["Cover on the alignment"], "1.380 m");
        assert!(g.notes.is_empty(), "{:?}", g.notes);
    }

    #[test]
    fn solids_are_closed_hollow_pipes_falling_to_the_outlet() {
        let c = twin();
        let solids = c.solids();
        assert_eq!(solids.len(), 2);
        assert_eq!(solids[0].name, "Barrel 1");
        // A 32-sided hollow prism: (32/2)·sin(2π/32)·(550² − 450²) × 12 000 mm³ = 3.7455 m³,
        // a little under the true π·(0.55² − 0.45²)·12 = 3.7699 m³. Positive: wound outwards.
        let prism = 16.0 * (2.0 * PI / 32.0).sin() * (550.0f64.powi(2) - 450.0f64.powi(2)) * 12_000.0 / 1.0e9;
        for m in &solids {
            close(m.volume() / 1.0e9, prism, 1e-6);
        }
        assert!(prism < 3.7699 && prism > 3.74);
        // Barrel axes 750 mm either side of the centre line; the first is on the left (+y).
        let mean_y = |m: &Mesh3| m.vertices.iter().map(|v| v[1]).sum::<f64>() / m.vertices.len() as f64;
        close(mean_y(&solids[0]), 750.0, 1e-9);
        close(mean_y(&solids[1]), -750.0, 1e-9);
        // The invert is at z = 0 at the inlet and 240 mm lower at the outlet.
        let lowest = |m: &Mesh3, inner: bool, end: bool| {
            let n = ROUND_SEGMENTS as usize;
            let base = match (inner, end) {
                (false, false) => 0,
                (false, true) => n,
                (true, false) => 2 * n,
                (true, true) => 3 * n,
            };
            m.vertices[base..base + n].iter().map(|v| v[2]).fold(f64::INFINITY, f64::min)
        };
        // Square-cut ends on a 2 % slope: the lowest point of each ring is 450 × cos(slope).
        let cos = c.plan_length_mm() / c.length_mm;
        close(lowest(&solids[0], true, false), 450.0 - 450.0 * cos, 1e-6);
        close(lowest(&solids[0], true, true), 450.0 - 240.0 - 450.0 * cos, 1e-6);
        let xs: Vec<f64> = solids[0].vertices.iter().map(|v| v[0]).collect();
        assert!(xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max) > 11_997.0);
    }

    #[test]
    fn site_placement_puts_the_inlet_end_on_its_side_of_the_road() {
        let a = alignment();
        // Heading east at station 50: the crossing is (1050, 2000). Inlet on the left (north),
        // flow due south: rotation 270°, inlet half the plan length north.
        let s = twin().site_placement(&a).unwrap();
        close(s.x, 1_050_000.0, 1e-6);
        close(s.y, 2_000_000.0 + 11_997.6 / 2.0, 1e-3);
        close(s.rotation_deg, 270.0, 1e-9);
        // The inlet invert is 2.26 m below the road.
        close(s.z, -2260.0, 1e-6);
        assert_eq!((s.alignment.as_deref(), s.station_m, s.offset_m), (Some("MC10"), Some(50.0), Some(0.0)));
        // Inlet on the right: flow due north, inlet to the south.
        let r = PipeCulvertDefinition { inlet_side: Side::Right, ..twin() }.site_placement(&a).unwrap();
        close(r.rotation_deg, 90.0, 1e-9);
        close(r.y, 2_000_000.0 - 11_997.6 / 2.0, 1e-3);
        // A skew of +30° turns the flow counter-clockwise: 300°. The mid-length stays on the crossing.
        let k = PipeCulvertDefinition { skew_deg: 30.0, ..twin() }.site_placement(&a).unwrap();
        close(k.rotation_deg, 300.0, 1e-9);
        let (half, dir) = (11_997.6 / 2.0, 300.0f64.to_radians());
        close(k.x + half * dir.cos(), 1_050_000.0, 1e-3);
        close(k.y + half * dir.sin(), 2_000_000.0, 1e-3);
        // Mid-length 2 m to the right (south) of the alignment.
        let o = PipeCulvertDefinition { centre_offset_m: 2.0, ..twin() }.site_placement(&a).unwrap();
        close(o.y, 2_000_000.0 - 2000.0 + half, 1e-3);
        // No road level: the inlet invert is at zero.
        assert_eq!(PipeCulvertDefinition { road_level_m: None, ..twin() }.site_placement(&a).unwrap().z, 0.0);
        assert!(matches!(PipeCulvertDefinition { station_m: 500.0, ..twin() }.site_placement(&a), Err(DrainageError::Route(_))));
    }

    #[test]
    fn elevation_shows_the_wall_the_bore_and_the_road() {
        let d = twin().drawing();
        // Width = plan length; height to the road: (1152.5 − 1150.0) × 1000 + 100 = 2600.
        close(d.extents.max.x, 11_997.6, 1e-3);
        close(d.extents.max.y, 2600.0, 1e-6);
        match &d.primitives[0] {
            Primitive::Fill { outline, colour } => {
                assert_eq!(*colour, Colour::Grey);
                // Outside bottom: 240 above the base at the inlet, on it at the outlet.
                close(outline[0].y, 240.0, 1e-6);
                close(outline[1].y, 0.0, 1e-9);
                close(outline[3].y, 240.0 + 1100.0, 1e-6);
            }
            other => panic!("expected the pipe body first, got {other:?}"),
        }
        assert!(matches!(d.primitives[1], Primitive::Fill { colour: Colour::White, .. }));
        assert!(d.primitives.iter().any(|p| matches!(p, Primitive::Line { colour: Some(Colour::Brown), .. })));
        // Without a road level the drawing stops at the top of the pipe.
        let d = PipeCulvertDefinition { road_level_m: None, ..twin() }.drawing();
        close(d.extents.max.y, 240.0 + 1100.0, 1e-6);
    }

    #[test]
    fn layout_limits_of_the_manual_are_notes_with_their_sections() {
        let note = |d: &PipeCulvertDefinition, code: &str| d.notes(DesignBasis::Mowt).into_iter().find(|n| n.code == code);
        let small = PipeCulvertDefinition { internal_diameter_mm: 450.0, ..twin() };
        let n = note(&small, "CULVERT_BELOW_MINIMUM_DIAMETER").unwrap();
        assert!(n.severity == Severity::Warning && n.message.contains("Section 9.10") && n.message.contains("600 mm"));
        assert!(note(&PipeCulvertDefinition { internal_diameter_mm: 600.0, barrel_spacing_mm: Some(1000.0), ..twin() }, "CULVERT_BELOW_MINIMUM_DIAMETER").is_none());

        let n = note(&PipeCulvertDefinition { skew_deg: -50.0, ..twin() }, "CULVERT_SKEW_ABOVE_LIMIT").unwrap();
        assert!(n.severity == Severity::Warning && n.message.contains("Section 9.8"));
        assert!(note(&PipeCulvertDefinition { skew_deg: 45.0, ..twin() }, "CULVERT_SKEW_ABOVE_LIMIT").is_none());

        // Gradients: 0.3 % must not be used, 0.8 % is to be avoided, 2 % is silent, 12 % needs
        // anchors, 25 % special anchors, a rising pipe is flagged.
        let at = |fall: f64| PipeCulvertDefinition { invert_level_inlet_m: 1150.0 + fall, ..twin() };
        assert_eq!(note(&at(0.036), "CULVERT_GRADIENT_BELOW_MINIMUM").unwrap().severity, Severity::Warning);
        assert_eq!(note(&at(0.096), "CULVERT_GRADIENT_FLAT").unwrap().severity, Severity::Info);
        assert!(at(0.24).notes(DesignBasis::Mowt).is_empty());
        assert!(note(&at(1.44), "CULVERT_GRADIENT_NEEDS_ANCHORS").unwrap().message.contains("Section 9.9.2"));
        assert_eq!(note(&at(3.0), "CULVERT_GRADIENT_NEEDS_SPECIAL_ANCHORS").unwrap().severity, Severity::Warning);
        assert!(note(&at(-0.1), "CULVERT_FALLS_TOWARDS_INLET").is_some());

        // The same culverts under the DMRB basis: CD 529 has a 450 mm minimum, no skew or
        // gradient limits, a 1.2 m minimum for culverts over 12 m, and calls pipes over 900 mm structures.
        let dmrb = |d: &PipeCulvertDefinition| d.notes(DesignBasis::Dmrb).into_iter().map(|n| n.code).collect::<Vec<_>>();
        assert!(dmrb(&small).is_empty(), "450 mm meets CD 529 3.5.1: {:?}", dmrb(&small));
        let tiny = PipeCulvertDefinition { internal_diameter_mm: 300.0, ..twin() };
        let n = tiny.notes(DesignBasis::Dmrb).into_iter().find(|n| n.code == "CULVERT_BELOW_MINIMUM_DIAMETER").unwrap();
        assert!(n.message.contains("CD 529 v1.1.0 clause 3.5.1") && !n.message.contains("DDM"));
        assert!(dmrb(&PipeCulvertDefinition { skew_deg: -50.0, ..twin() }).is_empty());
        assert!(dmrb(&at(0.036)).is_empty() && dmrb(&at(3.0)).is_empty());
        assert_eq!(dmrb(&at(-0.1)), ["CULVERT_FALLS_TOWARDS_INLET"]);
        // 12 m is not "longer than 12 metres"; 12.5 m at 900 mm is.
        let long = PipeCulvertDefinition { length_mm: 12_500.0, ..twin() };
        let n = long.notes(DesignBasis::Dmrb).into_iter().find(|n| n.code == "CULVERT_TOO_SMALL_FOR_ITS_LENGTH").unwrap();
        assert!(n.severity == Severity::Warning && n.message.contains("clause 3.4") && n.message.contains("12.50 m"));
        assert!(note(&long, "CULVERT_TOO_SMALL_FOR_ITS_LENGTH").is_none(), "MoWT states no such rule");
        let big = PipeCulvertDefinition { internal_diameter_mm: 1200.0, barrel_spacing_mm: Some(1800.0), length_mm: 12_500.0, ..twin() };
        assert_eq!(dmrb(&big), ["CULVERT_IS_A_STRUCTURE"]);
        let g = PipeCulvertModule::with_basis(DesignBasis::Dmrb).generate(&element(&big)).unwrap();
        assert_eq!(g.derived["Design basis for layout checks"], "UK DMRB");
        assert!(!g.derived.contains_key("Structure type (DDM Table 3.2)"));
        assert!(g.notes.iter().all(|n| !n.message.contains("DDM")));

        // Four 1.5 m pipes are a 6 m opening.
        let wide = PipeCulvertDefinition { barrels: 4, internal_diameter_mm: 1500.0, barrel_spacing_mm: Some(2200.0), ..twin() };
        assert!(note(&wide, "CULVERT_OPENING_SIX_METRES_OR_MORE").unwrap().message.contains("Table 3.2"));
        let g = PipeCulvertModule::default().generate(&element(&wide)).unwrap();
        assert!(!g.derived.contains_key("Structure type (DDM Table 3.2)"));
    }

    #[test]
    fn invalid_definitions_give_error_notes_and_no_geometry() {
        let codes = |d: &PipeCulvertDefinition| d.notes(DesignBasis::Mowt).into_iter().filter(|n| n.severity == Severity::Error).map(|n| n.code).collect::<Vec<_>>();
        assert_eq!(codes(&PipeCulvertDefinition { barrels: 0, ..twin() }), ["CULVERT_NO_BARRELS"]);
        assert_eq!(codes(&PipeCulvertDefinition { internal_diameter_mm: 0.0, ..twin() }), ["CULVERT_DIAMETER_NOT_POSITIVE"]);
        assert_eq!(codes(&PipeCulvertDefinition { wall_thickness_mm: -1.0, ..twin() }), ["CULVERT_WALL_NEGATIVE"]);
        assert_eq!(codes(&PipeCulvertDefinition { length_mm: f64::NAN, ..twin() }), ["CULVERT_LENGTH_NOT_POSITIVE"]);
        assert_eq!(codes(&PipeCulvertDefinition { skew_deg: 90.0, ..twin() }), ["CULVERT_SKEW_TOO_LARGE"]);
        assert_eq!(codes(&PipeCulvertDefinition { invert_level_inlet_m: 1170.0, ..twin() }), ["CULVERT_FALL_NOT_LESS_THAN_LENGTH"]);
        assert_eq!(codes(&PipeCulvertDefinition { barrel_spacing_mm: None, ..twin() }), ["CULVERT_SPACING_NOT_ENTERED"]);
        assert_eq!(codes(&PipeCulvertDefinition { barrel_spacing_mm: Some(1000.0), ..twin() }), ["CULVERT_BARRELS_OVERLAP"]);
        // One barrel needs no spacing.
        assert!(codes(&PipeCulvertDefinition { barrels: 1, barrel_spacing_mm: None, ..twin() }).is_empty());

        let bad = PipeCulvertDefinition { barrels: 0, ..twin() };
        let g = PipeCulvertModule::default().generate(&element(&bad)).unwrap();
        assert!(g.solids.is_empty() && g.drawing.primitives.is_empty() && g.derived.is_empty());
        assert!(bad.site_placement(&alignment()).is_err());
        let mut e = element(&twin());
        e.data = serde_json::json!({"barrels": "two"});
        assert!(PipeCulvertModule::default().generate(&e).is_err());
    }

    #[test]
    fn placeholder_is_flagged_and_serde_round_trips() {
        let p = PipeCulvertDefinition::placeholder("MC10", 20.0);
        let g = PipeCulvertModule::default().generate(&element(&p)).unwrap();
        assert!(g.notes.iter().any(|n| n.code == NOTE_PLACEHOLDER && n.severity == Severity::Info));
        assert!(g.notes.iter().any(|n| n.code == "CULVERT_GRADIENT_BELOW_MINIMUM"), "a level placeholder is below the minimum gradient");
        assert!(g.notes.iter().any(|n| n.code == "CULVERT_ROAD_LEVEL_NOT_ENTERED"));
        assert_eq!(g.solids.len(), 1);

        let c = twin();
        assert_eq!(PipeCulvertDefinition::from_value(&c.to_value()).unwrap(), c);
        assert!(!c.to_value().as_object().unwrap().contains_key("placeholder"));
        let minimal = serde_json::json!({
            "alignment": "MC10", "station_m": 10.0, "barrels": 1, "internal_diameter_mm": 600.0, "wall_thickness_mm": 60.0,
            "length_mm": 9000.0, "invert_level_inlet_m": 10.2, "invert_level_outlet_m": 10.0, "inlet_side": "Right"
        });
        let read = PipeCulvertDefinition::from_value(&minimal).unwrap();
        assert_eq!((read.skew_deg, read.centre_offset_m, read.road_level_m, read.barrel_spacing_mm, read.inlet_side), (0.0, 0.0, None, None, Side::Right));
    }

    #[test]
    fn outlet_control_uses_the_definition() {
        let r = twin().outlet_control(3.0, 0.012, 0.5, 1.0).unwrap();
        assert_eq!((r.input.barrels, r.input.diameter_m, r.input.length_m), (2, 0.9, 12.0));
        close(r.input.fall_m, 0.24, 1e-9);
        close(r.flow_per_barrel_m3_s, 1.5, 1e-12);
        assert!(PipeCulvertDefinition { barrels: 0, ..twin() }.outlet_control(3.0, 0.012, 0.5, 1.0).is_err());
    }
}
