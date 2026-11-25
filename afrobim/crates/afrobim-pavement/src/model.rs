// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The pavement definition: where it runs, how wide it is and what it is made of.

use afrobim_alignment::Alignment;
use afrobim_draw::Colour;
use afrobim_kernel::{Note, Severity};
use serde::{Deserialize, Serialize};

use crate::PavementError;

/// What a layer is made of.
///
/// The manual has no material codes. It names generic classes (Part I, Table 5.1 and the keys
/// of the Appendix C charts); those are the named variants here. Anything else is free text.
/// The crate never interprets the material beyond choosing a display colour and quoting the
/// nominal strength the manual states for the class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Material {
    /// A sprayed seal. The catalogue gives it no thickness ("SD").
    SurfaceDressing,
    /// Hot mix asphalt surfacing.
    HotMixAsphalt,
    GranularBase,
    CementedBase,
    BituminousBase,
    GranularSubbase,
    CementedSubbase,
    /// The upper of two cemented subbase layers in charts D2 and W2.
    CementedUpperSubbase,
    /// Capping or selected layer.
    SelectedLayer,
    Other(String),
}

impl Material {
    pub fn label(&self) -> &str {
        match self {
            Material::SurfaceDressing => "Surface dressing",
            Material::HotMixAsphalt => "Hot mix asphalt",
            Material::GranularBase => "Granular base",
            Material::CementedBase => "Cemented base",
            Material::BituminousBase => "Bituminous base",
            Material::GranularSubbase => "Granular subbase",
            Material::CementedSubbase => "Cemented subbase",
            Material::CementedUpperSubbase => "Cemented upper subbase",
            Material::SelectedLayer => "Selected layer",
            Material::Other(text) => text,
        }
    }

    /// The nominal strength the manual's Part I, Table 5.1 gives for this class, quoted. `None`
    /// for surfacings, for the cemented upper subbase (which is only in the chart keys, see
    /// `design`) and for free text.
    pub fn nominal_strength(&self) -> Option<&'static str> {
        // Part I, Table 5.1 "Nominal strength classification of materials in the design catalogue".
        match self {
            Material::GranularBase => Some("Soaked CBR > 80% @ 98% mod. AASHTO density"),
            Material::CementedBase => {
                Some("7 day UCS 1.5 - 3.0 MPa @ 100% mod. AASHTO density (or 1.0 - 1.5 MPa @ 97% if modified test is followed)")
            }
            Material::BituminousBase => Some("See specification"),
            Material::GranularSubbase => Some("Soaked CBR > 30% @ 95% mod. AASHTO density"),
            Material::CementedSubbase => {
                Some("7 day UCS 0.75 - 1.5 MPa @ 100% mod. AASHTO density (or 0.5 - 0.75 MPa @ 97% if modified test is followed)")
            }
            Material::SelectedLayer => Some("Soaked CBR > 15% @ 93% mod. AASHTO density"),
            _ => None,
        }
    }

    /// Fill colour on the cross-section. A drawing convention of this crate, nothing more.
    pub fn colour(&self) -> Colour {
        match self {
            Material::SurfaceDressing | Material::HotMixAsphalt | Material::BituminousBase => Colour::Black,
            Material::GranularBase => Colour::Grey,
            Material::CementedBase | Material::CementedUpperSubbase | Material::CementedSubbase => Colour::Blue,
            Material::GranularSubbase => Colour::Brown,
            Material::SelectedLayer => Colour::Yellow,
            Material::Other(_) => Colour::White,
        }
    }

    /// Display colour of the slab in the 3D view. A drawing convention of this crate.
    pub fn rgb(&self) -> [u8; 3] {
        match self {
            Material::SurfaceDressing | Material::HotMixAsphalt => [0x2B, 0x2B, 0x2E],
            Material::BituminousBase => [0x45, 0x42, 0x40],
            Material::GranularBase => [0x9A, 0x9C, 0xA0],
            Material::CementedBase => [0x7C, 0x8F, 0xA6],
            Material::CementedUpperSubbase => [0x8E, 0xA0, 0xB5],
            Material::CementedSubbase => [0xA3, 0xB2, 0xC4],
            Material::GranularSubbase => [0xA8, 0x7B, 0x4F],
            Material::SelectedLayer => [0xD2, 0xB4, 0x6E],
            Material::Other(_) => [0xC8, 0xC8, 0xC8],
        }
    }
}

/// One layer of the stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// What the layer is called on the drawing and in the schedule, e.g. "Base".
    pub name: String,
    pub material: Material,
    /// Compacted thickness, mm, measured vertically. Zero is allowed for a surface treatment
    /// that is measured by area only (a surface dressing): it is drawn as a heavy line, has no
    /// slab and no volume.
    pub thickness_mm: f64,
}

impl Layer {
    pub fn new(name: &str, material: Material, thickness_mm: f64) -> Layer {
        Layer { name: name.to_string(), material, thickness_mm }
    }
}

/// A side of the road looking up-station.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Left,
    Right,
}

/// How the entered cross-fall is applied to the carriageway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CrossfallShape {
    /// A crown on the carriageway centre, falling to both edges (camber).
    #[default]
    Crowned,
    /// One plane falling to the named side.
    OneWay { low_side: Side },
}

/// What a shoulder is built of.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum ShoulderLayers {
    /// The carriageway's layers carried through the shoulder.
    #[default]
    SameAsCarriageway,
    /// Its own stack, top first.
    Own(Vec<Layer>),
}

/// A shoulder. Its surface starts at the level of the carriageway edge and falls away from the
/// carriageway at its own cross-fall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shoulder {
    pub width_m: f64,
    /// Fall away from the carriageway, per cent, as entered.
    pub crossfall_percent: f64,
    #[serde(default)]
    pub layers: ShoulderLayers,
}

/// Which strip of the cross-section a quantity or a solid belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StripKind {
    LeftShoulder,
    Carriageway,
    RightShoulder,
}

impl StripKind {
    pub fn label(self) -> &'static str {
        match self {
            StripKind::LeftShoulder => "Left shoulder",
            StripKind::Carriageway => "Carriageway",
            StripKind::RightShoulder => "Right shoulder",
        }
    }
}

/// One strip of the cross-section, resolved: its offsets from the alignment, its surface and
/// its layers.
#[derive(Debug, Clone, PartialEq)]
pub struct Strip {
    pub kind: StripKind,
    /// Surface break points from left to right: (offset from the alignment in metres, positive
    /// to the right looking up-station; level in metres, zero at the carriageway centre).
    pub profile: Vec<(f64, f64)>,
    /// Top first.
    pub layers: Vec<Layer>,
}

impl Strip {
    pub fn left_offset_m(&self) -> f64 {
        self.profile[0].0
    }
    pub fn right_offset_m(&self) -> f64 {
        self.profile[self.profile.len() - 1].0
    }
    pub fn width_m(&self) -> f64 {
        self.right_offset_m() - self.left_offset_m()
    }
    /// Offset of the strip's own centre from the alignment, m.
    pub fn mean_offset_m(&self) -> f64 {
        0.5 * (self.left_offset_m() + self.right_offset_m())
    }
    pub fn total_thickness_mm(&self) -> f64 {
        self.layers.iter().map(|l| l.thickness_mm).sum()
    }
}

/// A length of pavement along an alignment.
///
/// The definition carries its own copy of the alignment so that the module can draw and measure
/// it from the element alone; [`crate::sync_alignments`] refreshes the copy from the document.
/// The horizontal geometry sets the plan. The vertical profile, where the alignment has one, sets
/// the level of the carriageway centre in the 3D solids only (quantities do not use it).
/// Superelevation is not read: the cross-fall is the entered value over the whole length.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PavementDefinition {
    pub alignment: Alignment,
    pub start_station_m: f64,
    pub end_station_m: f64,
    /// Width of the carriageway, m, between the inner edges of the shoulders.
    pub carriageway_width_m: f64,
    /// Offset of the carriageway centre from the alignment, m, positive to the right looking
    /// up-station. Zero puts the carriageway centre on the alignment.
    #[serde(default)]
    pub centre_offset_m: f64,
    /// Cross-fall of the carriageway, per cent, as entered.
    pub crossfall_percent: f64,
    #[serde(default)]
    pub crossfall_shape: CrossfallShape,
    /// The carriageway's layers, top first.
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub left_shoulder: Option<Shoulder>,
    #[serde(default)]
    pub right_shoulder: Option<Shoulder>,
}

fn finite_positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

fn note(severity: Severity, code: &str, message: String) -> Note {
    Note { severity, code: code.to_string(), message }
}

impl PavementDefinition {
    /// A pavement over the whole of an alignment with no shoulders, centred on it and crowned.
    pub fn new(alignment: Alignment, carriageway_width_m: f64, crossfall_percent: f64, layers: Vec<Layer>) -> PavementDefinition {
        let (start, end) = (alignment.start_station_m, alignment.end_station_m());
        PavementDefinition {
            alignment,
            start_station_m: start,
            end_station_m: end,
            carriageway_width_m,
            centre_offset_m: 0.0,
            crossfall_percent,
            crossfall_shape: CrossfallShape::Crowned,
            layers,
            left_shoulder: None,
            right_shoulder: None,
        }
    }

    pub fn from_value(data: &serde_json::Value) -> Result<Self, PavementError> {
        serde_json::from_value(data.clone()).map_err(|e| PavementError::InvalidDefinition(format!("the pavement definition cannot be read: {e}")))
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a pavement definition always serialises")
    }

    /// Length along the alignment between the two stations, m.
    pub fn length_m(&self) -> f64 {
        self.end_station_m - self.start_station_m
    }

    /// Level of the carriageway surface at `offset` from the carriageway centre (positive to
    /// the right), m, zero at the centre. Not clamped to the carriageway.
    fn carriageway_level(&self, offset_from_centre_m: f64) -> f64 {
        let c = self.crossfall_percent / 100.0;
        match self.crossfall_shape {
            CrossfallShape::Crowned => -c * offset_from_centre_m.abs(),
            CrossfallShape::OneWay { low_side: Side::Right } => -c * offset_from_centre_m,
            CrossfallShape::OneWay { low_side: Side::Left } => c * offset_from_centre_m,
        }
    }

    /// The strips of the cross-section from left to right. Call only on a definition without
    /// Error notes.
    pub fn strips(&self) -> Vec<Strip> {
        let half = 0.5 * self.carriageway_width_m;
        let centre = self.centre_offset_m;
        let (left_edge, right_edge) = (self.carriageway_level(-half), self.carriageway_level(half));
        let mut out = Vec::new();
        let shoulder_layers = |s: &Shoulder| match &s.layers {
            ShoulderLayers::SameAsCarriageway => self.layers.clone(),
            ShoulderLayers::Own(layers) => layers.clone(),
        };
        if let Some(s) = &self.left_shoulder {
            let fall = s.crossfall_percent / 100.0 * s.width_m;
            out.push(Strip {
                kind: StripKind::LeftShoulder,
                profile: vec![(centre - half - s.width_m, left_edge - fall), (centre - half, left_edge)],
                layers: shoulder_layers(s),
            });
        }
        let mut profile = vec![(centre - half, left_edge)];
        if self.crossfall_shape == CrossfallShape::Crowned && self.crossfall_percent != 0.0 {
            profile.push((centre, 0.0));
        }
        profile.push((centre + half, right_edge));
        out.push(Strip { kind: StripKind::Carriageway, profile, layers: self.layers.clone() });
        if let Some(s) = &self.right_shoulder {
            let fall = s.crossfall_percent / 100.0 * s.width_m;
            out.push(Strip {
                kind: StripKind::RightShoulder,
                profile: vec![(centre + half, right_edge), (centre + half + s.width_m, right_edge - fall)],
                layers: shoulder_layers(s),
            });
        }
        out
    }

    /// Validation and information notes. Error notes mean the pavement cannot be drawn or
    /// measured.
    ///
    /// The warnings quote the conditions the manual's flexible pavement guide says its designs
    /// assume (Part I, sections 1.2 and 5.9, Appendix A.4.1). They are reminders against the
    /// entered values, not a design check.
    pub fn notes(&self) -> Vec<Note> {
        use Severity::{Error, Info, Warning};
        let mut out = Vec::new();
        let (s0, s1) = (self.start_station_m, self.end_station_m);
        let mut stations_ok = false;
        if self.alignment.elements.is_empty() || !(self.alignment.length() > 0.0) {
            out.push(note(Error, "PAVEMENT_NO_ALIGNMENT", "The pavement has no alignment to follow.".to_string()));
        } else if !(s0.is_finite() && s1.is_finite()) || s1 <= s0 {
            out.push(note(Error, "PAVEMENT_STATIONS", format!("The end station ({s1}) must be greater than the start station ({s0}).")));
        } else if self.alignment.locate(s0).is_none() || self.alignment.locate(s1).is_none() {
            out.push(note(
                Error,
                "PAVEMENT_STATIONS_OFF_ALIGNMENT",
                format!(
                    "Stations {s0} to {s1} are not all on alignment '{}', which runs from {} to {}.",
                    self.alignment.name,
                    self.alignment.start_station_m,
                    self.alignment.end_station_m()
                ),
            ));
        } else {
            stations_ok = true;
        }
        if !finite_positive(self.carriageway_width_m) {
            out.push(note(Error, "PAVEMENT_WIDTH", format!("The carriageway width must be greater than zero (it is {}).", self.carriageway_width_m)));
        }
        if !self.centre_offset_m.is_finite() {
            out.push(note(Error, "PAVEMENT_OFFSET", "The offset of the carriageway centre is not a number.".to_string()));
        }
        if !(self.crossfall_percent.is_finite() && self.crossfall_percent >= 0.0 && self.crossfall_percent < 100.0) {
            out.push(note(Error, "PAVEMENT_CROSSFALL", format!("The cross-fall must be between 0 and 100 per cent (it is {}).", self.crossfall_percent)));
        }
        let check_layers = |layers: &[Layer], what: &str, out: &mut Vec<Note>| {
            if layers.is_empty() {
                out.push(note(Error, "PAVEMENT_NO_LAYERS", format!("The {what} has no layers.")));
                return;
            }
            for l in layers {
                if !(l.thickness_mm.is_finite() && l.thickness_mm >= 0.0) {
                    out.push(note(Error, "PAVEMENT_LAYER_THICKNESS", format!("Layer '{}' of the {what} has a thickness of {} mm.", l.name, l.thickness_mm)));
                }
            }
            if layers.iter().all(|l| l.thickness_mm == 0.0) {
                out.push(note(Error, "PAVEMENT_NO_THICKNESS", format!("Every layer of the {what} has zero thickness.")));
            }
        };
        check_layers(&self.layers, "carriageway", &mut out);
        for (shoulder, what) in [(&self.left_shoulder, "left shoulder"), (&self.right_shoulder, "right shoulder")] {
            let Some(s) = shoulder else { continue };
            if !finite_positive(s.width_m) {
                out.push(note(Error, "PAVEMENT_SHOULDER_WIDTH", format!("The {what} width must be greater than zero (it is {}).", s.width_m)));
            }
            if !(s.crossfall_percent.is_finite() && s.crossfall_percent >= 0.0 && s.crossfall_percent < 100.0) {
                out.push(note(Error, "PAVEMENT_CROSSFALL", format!("The {what} cross-fall must be between 0 and 100 per cent (it is {}).", s.crossfall_percent)));
            }
            if let ShoulderLayers::Own(layers) = &s.layers {
                check_layers(layers, what, &mut out);
            }
        }
        if out.iter().any(|n| n.severity == Error) {
            return out;
        }

        // The offset strip formula needs every edge to stay on its own side of the centre of
        // curvature: 1 + k·o > 0 at every station.
        let strips = self.strips();
        let (left, right) = (strips[0].left_offset_m(), strips[strips.len() - 1].right_offset_m());
        if stations_ok {
            let (k_min, k_max) = crate::geometry::curvature_range(&self.alignment, s0, s1);
            let worst = [left, right].into_iter().flat_map(|o| [1.0 + k_min * o, 1.0 + k_max * o]).fold(f64::INFINITY, f64::min);
            if worst <= 0.0 {
                out.push(note(
                    Error,
                    "PAVEMENT_WIDER_THAN_RADIUS",
                    "An edge of the pavement reaches the centre of a curve: the pavement is wider than the radius allows.".to_string(),
                ));
                return out;
            }
            let joints = self.alignment.discontinuities(1e-3, 1e-4);
            if joints.iter().any(|j| j.station_m > s0 && j.station_m < s1) {
                out.push(note(
                    Warning,
                    "PAVEMENT_ALIGNMENT_NOT_SMOOTH",
                    "The alignment has a gap or an angle between elements inside this length. Areas are computed as if each element were followed to its end; the wedge at an angle point is not measured and the slabs overlap or open there.".to_string(),
                ));
            }
        }

        // Reminders from the manual, against the entered values.
        if self.crossfall_percent < 2.5 {
            out.push(note(
                Warning,
                "PAVEMENT_CROSSFALL_BELOW_GUIDE",
                format!(
                    "The entered cross-fall is {} per cent. The flexible pavement guide states that its structures assume a minimum camber or cross-fall of 2.5 per cent (Road Design Manual Vol. 3 Part I, section 1.2 c).",
                    self.crossfall_percent
                ),
            ));
        }
        for (shoulder, what) in [(&self.left_shoulder, "left"), (&self.right_shoulder, "right")] {
            let narrow = shoulder.as_ref().is_none_or(|s| s.width_m < 1.0);
            if narrow {
                let has = shoulder.as_ref().map_or("no shoulder".to_string(), |s| format!("a shoulder of {} m", s.width_m));
                out.push(note(
                    Warning,
                    "PAVEMENT_SHOULDER_BELOW_GUIDE",
                    format!("There is {has} on the {what}. The flexible pavement guide states that its structures assume shoulders at least 1 m wide (Road Design Manual Vol. 3 Part I, section 1.2 b)."),
                ));
            }
        }
        for strip in &strips {
            if let Some(top) = strip.layers.first() {
                if top.material == Material::HotMixAsphalt && top.thickness_mm > 0.0 && top.thickness_mm < 40.0 {
                    out.push(note(
                        Warning,
                        "PAVEMENT_ASPHALT_BELOW_GUIDE",
                        format!(
                            "{}: the asphalt surfacing is {} mm. For the designs in the flexible pavement guide the minimum asphalt premix surfacing thickness is 40 mm (Road Design Manual Vol. 3 Part I, Appendix A.4.1).",
                            strip.kind.label(),
                            top.thickness_mm
                        ),
                    ));
                }
            }
            if strip.kind == StripKind::Carriageway || !matches!(self.shoulder(strip.kind).map(|s| &s.layers), Some(ShoulderLayers::SameAsCarriageway)) {
                for l in strip.layers.iter().filter(|l| l.thickness_mm > 200.0) {
                    out.push(note(
                        Info,
                        "PAVEMENT_LAYER_IN_LIFTS",
                        format!(
                            "{}: layer '{}' is {} mm thick. The guide recommends that compacted lift thicknesses greater than 200 mm or less than 100 mm are generally avoided, the lifts being defined by the Engineer (Road Design Manual Vol. 3 Part I, section 5.9, construction note).",
                            strip.kind.label(),
                            l.name,
                            l.thickness_mm
                        ),
                    ));
                }
            }
        }
        out.push(note(
            Info,
            "PAVEMENT_CROSSFALL_AS_ENTERED",
            "The cross-fall is the entered value over the whole length; superelevation and the vertical profile are not read.".to_string(),
        ));
        out
    }

    fn shoulder(&self, kind: StripKind) -> Option<&Shoulder> {
        match kind {
            StripKind::LeftShoulder => self.left_shoulder.as_ref(),
            StripKind::RightShoulder => self.right_shoulder.as_ref(),
            StripKind::Carriageway => None,
        }
    }

    /// `Err` with the Error notes joined when there are any.
    pub fn check(&self) -> Result<(), PavementError> {
        let errors: Vec<String> = self.notes().into_iter().filter(|n| n.severity == Severity::Error).map(|n| n.message).collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(PavementError::InvalidDefinition(errors.join(" ")))
        }
    }
}
