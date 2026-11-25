// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Foundations: the definition stored in an element's `data` and the kernel module that draws it.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use afrobim_draw::{Colour, Drawing, Extents, Mesh3, Primitive, Pt2};
use afrobim_kernel::{Element, Generated, Module, Note};
use serde::{Deserialize, Serialize};

use crate::quantities::{fixed, FoundationQuantities};
use crate::{error_note, error_text, info_note, positive, SupportError, NOTE_PLACEHOLDER, PLACEHOLDER_FOUNDATION_SIDE_MM, ROUND_SEGMENTS};

/// The element category [`FoundationModule`] owns.
pub const FOUNDATION_CATEGORY: &str = "foundation";

/// Display colours in the 3D view. Screen colours only.
const CONCRETE_RGB: [u8; 3] = [0xB4, 0xB4, 0xAC];
const BLINDING_RGB: [u8; 3] = [0x8C, 0x86, 0x78];

/// The concrete body. `length` runs along +X (parallel to the sign face), `width` along +Y,
/// `depth` is vertical.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FoundationShape {
    Block { length_mm: f64, width_mm: f64, depth_mm: f64 },
    Cylinder { diameter_mm: f64, depth_mm: f64 },
}

impl FoundationShape {
    /// Plan size as (along X, along Y), mm.
    pub fn plan_mm(&self) -> (f64, f64) {
        match *self {
            FoundationShape::Block { length_mm, width_mm, .. } => (length_mm, width_mm),
            FoundationShape::Cylinder { diameter_mm, .. } => (diameter_mm, diameter_mm),
        }
    }

    pub fn depth_mm(&self) -> f64 {
        match *self {
            FoundationShape::Block { depth_mm, .. } | FoundationShape::Cylinder { depth_mm, .. } => depth_mm,
        }
    }

    /// Plan area, mm²: L·W for a block, π/4·D² for a cylinder.
    pub fn plan_area_mm2(&self) -> f64 {
        match *self {
            FoundationShape::Block { length_mm, width_mm, .. } => length_mm * width_mm,
            FoundationShape::Cylinder { diameter_mm, .. } => PI / 4.0 * diameter_mm * diameter_mm,
        }
    }

    /// Plan outline length, mm: 2(L + W) for a block, π·D for a cylinder.
    pub fn plan_perimeter_mm(&self) -> f64 {
        match *self {
            FoundationShape::Block { length_mm, width_mm, .. } => 2.0 * (length_mm + width_mm),
            FoundationShape::Cylinder { diameter_mm, .. } => PI * diameter_mm,
        }
    }

    /// Plan area after growing the outline by `margin` on every side, mm².
    pub fn plan_area_with_margin_mm2(&self, margin: f64) -> f64 {
        match *self {
            FoundationShape::Block { length_mm, width_mm, .. } => (length_mm + 2.0 * margin) * (width_mm + 2.0 * margin),
            FoundationShape::Cylinder { diameter_mm, .. } => PI / 4.0 * (diameter_mm + 2.0 * margin).powi(2),
        }
    }

    /// A plain description of the entered dimensions.
    pub fn label(&self) -> String {
        match *self {
            FoundationShape::Block { length_mm, width_mm, depth_mm } => format!("Block {length_mm}x{width_mm}x{depth_mm}"),
            FoundationShape::Cylinder { diameter_mm, depth_mm } => format!("Cylinder D{diameter_mm}x{depth_mm}"),
        }
    }
}

/// A void left in the top of the foundation for the post (a sleeve or pocket), centred in plan
/// and running down from the top surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Socket {
    Round { diameter_mm: f64, depth_mm: f64 },
    Rect { length_mm: f64, width_mm: f64, depth_mm: f64 },
}

impl Socket {
    pub fn depth_mm(&self) -> f64 {
        match *self {
            Socket::Round { depth_mm, .. } | Socket::Rect { depth_mm, .. } => depth_mm,
        }
    }

    /// Size seen in elevation along X, mm.
    pub fn length_mm(&self) -> f64 {
        match *self {
            Socket::Round { diameter_mm, .. } => diameter_mm,
            Socket::Rect { length_mm, .. } => length_mm,
        }
    }

    /// Volume of the void, mm³: π/4·d²·h or l·w·h.
    pub fn volume_mm3(&self) -> f64 {
        match *self {
            Socket::Round { diameter_mm, depth_mm } => PI / 4.0 * diameter_mm * diameter_mm * depth_mm,
            Socket::Rect { length_mm, width_mm, depth_mm } => length_mm * width_mm * depth_mm,
        }
    }
}

/// A foundation as the engineer describes it. Nothing here is checked against any standard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoundationDefinition {
    pub shape: FoundationShape,
    /// Depth of the top of the concrete below ground level, mm. Zero puts the top at ground.
    #[serde(default)]
    pub top_below_ground_mm: f64,
    /// Free text, e.g. the concrete the specification calls for. Not interpreted.
    #[serde(default)]
    pub concrete: String,
    /// Thickness of a blinding layer under the foundation, over the same plan outline, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blinding_mm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket: Option<Socket>,
    /// True while the dimensions are still the `PLACEHOLDER_*` starting values.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub placeholder: bool,
}

impl FoundationDefinition {
    /// A drawable starting definition (a cube of [`PLACEHOLDER_FOUNDATION_SIDE_MM`] with its top
    /// at ground), flagged as a placeholder. It is not a recommended, typical or compliant
    /// foundation.
    pub fn placeholder() -> Self {
        let s = PLACEHOLDER_FOUNDATION_SIDE_MM;
        FoundationDefinition {
            shape: FoundationShape::Block { length_mm: s, width_mm: s, depth_mm: s },
            top_below_ground_mm: 0.0,
            concrete: String::new(),
            blinding_mm: None,
            socket: None,
            placeholder: true,
        }
    }

    pub fn from_value(data: &serde_json::Value) -> Result<Self, SupportError> {
        serde_json::from_value(data.clone())
            .map_err(|e| SupportError::InvalidDefinition(format!("the foundation definition cannot be read: {e}")))
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a foundation definition always serialises")
    }

    /// Blinding thickness, zero when there is none.
    pub fn blinding_thickness_mm(&self) -> f64 {
        self.blinding_mm.unwrap_or(0.0)
    }

    /// Depth from ground level to the underside of the blinding (or of the concrete), mm.
    pub fn formation_depth_mm(&self) -> f64 {
        self.top_below_ground_mm + self.shape.depth_mm() + self.blinding_thickness_mm()
    }

    /// Validation and information notes. Error notes mean the foundation cannot be drawn or
    /// measured.
    pub fn notes(&self) -> Vec<Note> {
        let mut out = Vec::new();
        let mut sized = true;
        let dims: Vec<(&str, f64)> = match self.shape {
            FoundationShape::Block { length_mm, width_mm, depth_mm } => vec![("length", length_mm), ("width", width_mm), ("depth", depth_mm)],
            FoundationShape::Cylinder { diameter_mm, depth_mm } => vec![("diameter", diameter_mm), ("depth", depth_mm)],
        };
        for (name, v) in dims {
            if !positive(v) {
                out.push(error_note("FOUNDATION_DIMENSION_NOT_POSITIVE", format!("The foundation's {name} must be greater than zero (it is {v}).")));
                sized = false;
            }
        }
        if !(self.top_below_ground_mm.is_finite() && self.top_below_ground_mm >= 0.0) {
            out.push(error_note(
                "FOUNDATION_TOP_ABOVE_GROUND",
                format!("The depth of the top below ground must not be negative (it is {}); a foundation standing above ground is not modelled.", self.top_below_ground_mm),
            ));
        }
        if let Some(b) = self.blinding_mm {
            if !positive(b) {
                out.push(error_note("FOUNDATION_BLINDING_NOT_POSITIVE", format!("The blinding thickness must be greater than zero when given (it is {b}).")));
            }
        }
        if let Some(socket) = &self.socket {
            let dims: Vec<(&str, f64)> = match *socket {
                Socket::Round { diameter_mm, depth_mm } => vec![("diameter", diameter_mm), ("depth", depth_mm)],
                Socket::Rect { length_mm, width_mm, depth_mm } => vec![("length", length_mm), ("width", width_mm), ("depth", depth_mm)],
            };
            let mut socket_sized = true;
            for (name, v) in dims {
                if !positive(v) {
                    out.push(error_note("SOCKET_DIMENSION_NOT_POSITIVE", format!("The socket's {name} must be greater than zero (it is {v}).")));
                    socket_sized = false;
                }
            }
            if sized && socket_sized {
                if socket.depth_mm() > self.shape.depth_mm() {
                    out.push(error_note(
                        "SOCKET_DEEPER_THAN_FOUNDATION",
                        format!("A socket {} mm deep is deeper than the foundation ({} mm).", socket.depth_mm(), self.shape.depth_mm()),
                    ));
                }
                let fits = match (&self.shape, socket) {
                    (FoundationShape::Block { length_mm: l, width_mm: w, .. }, Socket::Round { diameter_mm: d, .. }) => *d < l.min(*w),
                    (FoundationShape::Block { length_mm: l, width_mm: w, .. }, Socket::Rect { length_mm: sl, width_mm: sw, .. }) => sl < l && sw < w,
                    (FoundationShape::Cylinder { diameter_mm: big, .. }, Socket::Round { diameter_mm: d, .. }) => d < big,
                    // A centred rectangle fits a circle when its diagonal is less than the diameter.
                    (FoundationShape::Cylinder { diameter_mm: big, .. }, Socket::Rect { length_mm: sl, width_mm: sw, .. }) => sl.hypot(*sw) < *big,
                };
                if !fits {
                    out.push(error_note("SOCKET_WIDER_THAN_FOUNDATION", "The socket does not fit inside the plan outline of the foundation.".to_string()));
                }
            }
        }
        if self.placeholder {
            out.push(info_note(
                NOTE_PLACEHOLDER,
                "The foundation dimensions are placeholders so that the element can be drawn. They are not design values; the engineer must set the shape, size and depth.",
            ));
        }
        if self.concrete.trim().is_empty() {
            out.push(info_note("FOUNDATION_CONCRETE_NOT_STATED", "No concrete is stated for this foundation."));
        }
        out
    }

    /// Ok when the definition has no Error notes.
    pub fn check(&self) -> Result<(), SupportError> {
        match error_text(&self.notes()) {
            Some(text) => Err(SupportError::InvalidDefinition(text)),
            None => Ok(()),
        }
    }

    /// The foundation for the 3D view, in its own frame: plan bounding box on x ∈ [0, length],
    /// y ∈ [0, width], **z = 0 at ground level**, the concrete from z = −(top below ground +
    /// depth) to z = −top below ground, and a second solid named "Blinding" beneath it when
    /// there is one. The socket is not cut out of the display solid, and a cylinder is a prism
    /// of [`ROUND_SEGMENTS`] sides; quantities do not come from these meshes.
    pub fn solids(&self) -> Vec<Mesh3> {
        let top = -self.top_below_ground_mm;
        let bottom = top - self.shape.depth_mm();
        let body = |name: &str, z0: f64, z1: f64, colour: [u8; 3]| match self.shape {
            FoundationShape::Block { length_mm, width_mm, .. } => Mesh3::cuboid(name, [0.0, 0.0, z0], [length_mm, width_mm, z1], colour),
            FoundationShape::Cylinder { diameter_mm: d, .. } => Mesh3::cylinder(name, d / 2.0, d / 2.0, z0, z1, d / 2.0, ROUND_SEGMENTS, colour),
        };
        let mut out = vec![body("Foundation", bottom, top, CONCRETE_RGB)];
        if let Some(b) = self.blinding_mm {
            out.push(body("Blinding", bottom - b, bottom, BLINDING_RGB));
        }
        out
    }

    /// Elevation for the design sheet, mm: the foundation seen looking along +Y with its lowest
    /// point (underside of blinding, or of concrete) on y = 0 and its left edge on x = 0. The
    /// socket is outlined. The cover between the top of the concrete and ground is not drawn.
    pub fn drawing(&self) -> Drawing {
        let (l, _) = self.shape.plan_mm();
        let b = self.blinding_thickness_mm();
        let top = b + self.shape.depth_mm();
        let thin = (l * 0.005).max(0.5);
        let rect = |y0: f64, y1: f64, colour: Colour| Primitive::Fill {
            outline: vec![Pt2::new(0.0, y0), Pt2::new(l, y0), Pt2::new(l, y1), Pt2::new(0.0, y1)],
            colour,
        };
        let line = |x0: f64, y0: f64, x1: f64, y1: f64| Primitive::Line { from: Pt2::new(x0, y0), to: Pt2::new(x1, y1), width: thin, colour: None };
        let mut primitives = Vec::new();
        if b > 0.0 {
            primitives.push(rect(0.0, b, Colour::Brown));
        }
        primitives.push(rect(b, top, Colour::Grey));
        if b > 0.0 {
            primitives.push(line(0.0, b, l, b));
        }
        if let Some(s) = &self.socket {
            let (x0, x1, y) = ((l - s.length_mm()) / 2.0, (l + s.length_mm()) / 2.0, top - s.depth_mm());
            primitives.push(line(x0, top, x0, y));
            primitives.push(line(x0, y, x1, y));
            primitives.push(line(x1, y, x1, top));
        }
        Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(l, top) } }
    }
}

/// The kernel module for category [`FOUNDATION_CATEGORY`].
///
/// `generate` reads a [`FoundationDefinition`] from the element's `data`. A definition with
/// Error notes yields the notes and no geometry.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FoundationModule {
    /// Working space added on every side of the plan outline for the excavation volume shown
    /// with the element, mm. Zero by default: the excavation is then the net prism.
    pub working_space_mm: f64,
}

impl Module for FoundationModule {
    fn category(&self) -> &str {
        FOUNDATION_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Sign foundations"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let def = FoundationDefinition::from_value(&element.data).map_err(|e| e.to_string())?;
        let notes = def.notes();
        if error_text(&notes).is_some() {
            return Ok(Generated { notes, ..Generated::default() });
        }
        let q = FoundationQuantities::measure(&element.name, &def, self.working_space_mm).map_err(|e| e.to_string())?;
        let mut derived = BTreeMap::new();
        derived.insert("Shape".to_string(), def.shape.label());
        derived.insert("Top below ground".to_string(), format!("{} mm", fixed(def.top_below_ground_mm, 0, 1)));
        derived.insert("Formation depth below ground".to_string(), format!("{} mm", fixed(def.formation_depth_mm(), 0, 1)));
        derived.insert("Concrete volume".to_string(), format!("{} m³", fixed(q.concrete_m3, 3, 3)));
        if def.socket.is_some() {
            derived.insert("Socket void (deducted)".to_string(), format!("{} m³", fixed(q.socket_void_m3, 3, 3)));
        }
        if def.blinding_mm.is_some() {
            derived.insert("Blinding volume".to_string(), format!("{} m³", fixed(q.blinding_m3, 3, 3)));
        }
        derived.insert(
            format!("Excavation (prism, {} mm working space)", fixed(q.working_space_mm, 0, 1)),
            format!("{} m³", fixed(q.excavation_m3, 3, 3)),
        );
        derived.insert("Side face area".to_string(), format!("{} m²", fixed(q.side_area_m2, 3, 3)));
        Ok(Generated { drawing: def.drawing(), notes, derived, solids: def.solids() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_kernel::{Document, Pt, Severity};

    fn block() -> FoundationDefinition {
        FoundationDefinition {
            shape: FoundationShape::Block { length_mm: 600.0, width_mm: 500.0, depth_mm: 900.0 },
            top_below_ground_mm: 100.0,
            concrete: "as specified".into(),
            blinding_mm: Some(50.0),
            socket: Some(Socket::Round { diameter_mm: 150.0, depth_mm: 500.0 }),
            placeholder: false,
        }
    }

    fn element(def: &FoundationDefinition) -> Element {
        let mut d = Document::default();
        let id = d.add(FOUNDATION_CATEGORY, "ADS-001-F1", Pt::new(0.0, 0.0), 600.0, 950.0, "Supports", def.to_value());
        d.find(id).unwrap().clone()
    }

    fn error_codes(def: &FoundationDefinition) -> Vec<String> {
        def.notes().into_iter().filter(|n| n.severity == Severity::Error).map(|n| n.code).collect()
    }

    fn z_range(m: &Mesh3) -> (f64, f64) {
        m.vertices.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v[2]), hi.max(v[2])))
    }

    #[test]
    fn block_solids_sit_below_ground_with_the_blinding_under_them() {
        let g = FoundationModule::default().generate(&element(&block())).unwrap();
        assert!(g.notes.is_empty());
        assert_eq!(g.solids.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Foundation", "Blinding"]);
        // Top 100 below ground, 900 deep: z from −1000 to −100. Blinding 50: −1050 to −1000.
        assert_eq!(z_range(&g.solids[0]), (-1000.0, -100.0));
        assert_eq!(z_range(&g.solids[1]), (-1050.0, -1000.0));
        // The display block is the gross block: 600 × 500 × 900 mm³ exactly (socket not cut).
        assert!((g.solids[0].volume() - 600.0 * 500.0 * 900.0).abs() < 1e-3);
        assert!((g.solids[1].volume() - 600.0 * 500.0 * 50.0).abs() < 1e-3);
    }

    #[test]
    fn shown_quantities_by_hand() {
        let g = FoundationModule { working_space_mm: 150.0 }.generate(&element(&block())).unwrap();
        // Gross 0.6 × 0.5 × 0.9 = 0.270 m³. Socket π/4 × 0.15² × 0.5 = 0.008836 m³.
        // Concrete 0.270 − 0.008836 = 0.261164 → 0.261 m³.
        assert_eq!(g.derived["Concrete volume"], "0.261 m³");
        assert_eq!(g.derived["Socket void (deducted)"], "0.009 m³");
        // Blinding 0.6 × 0.5 × 0.05 = 0.015 m³.
        assert_eq!(g.derived["Blinding volume"], "0.015 m³");
        // Excavation (0.6 + 0.3)(0.5 + 0.3)(0.1 + 0.9 + 0.05) = 0.9 × 0.8 × 1.05 = 0.756 m³.
        assert_eq!(g.derived["Excavation (prism, 150 mm working space)"], "0.756 m³");
        // Side faces 2 × (0.6 + 0.5) × 0.9 = 1.98 m².
        assert_eq!(g.derived["Side face area"], "1.980 m²");
        assert_eq!(g.derived["Formation depth below ground"], "1050 mm");
        assert_eq!(g.derived["Shape"], "Block 600x500x900");
    }

    #[test]
    fn cylinder_mesh_is_within_the_stated_tolerance_of_the_formula() {
        let def = FoundationDefinition {
            shape: FoundationShape::Cylinder { diameter_mm: 600.0, depth_mm: 1200.0 },
            top_below_ground_mm: 0.0,
            blinding_mm: None,
            socket: None,
            ..block()
        };
        // π/4 × 600² × 1200 = 108 000 000π = 339 292 006.6 mm³ = 0.339292 m³.
        let exact = PI / 4.0 * 600.0 * 600.0 * 1200.0;
        assert!((exact - 339_292_006.6).abs() < 1.0);
        let g = FoundationModule::default().generate(&element(&def)).unwrap();
        assert_eq!(g.solids.len(), 1);
        assert_eq!(z_range(&g.solids[0]), (-1200.0, 0.0));
        // 48-sided prism: 0.29 % under the cylinder (see the section tests), never over.
        let mesh = g.solids[0].volume();
        assert!(mesh < exact && mesh > exact * 0.997);
        // The reported volume is the formula's 0.339 m³, not the mesh's 0.3383 m³.
        assert_eq!(g.derived["Concrete volume"], "0.339 m³");
        assert_eq!(fixed(mesh / 1.0e9, 4, 4), "0.3383");
        // Plan bounding box on [0, 600]²: centre at (300, 300).
        for v in &g.solids[0].vertices {
            assert!(((v[0] - 300.0).hypot(v[1] - 300.0) - 300.0).abs() < 1e-6 || (v[0] == 300.0 && v[1] == 300.0));
        }
    }

    #[test]
    fn elevation_has_its_lower_left_on_the_origin_and_outlines_the_socket() {
        let d = block().drawing();
        // 600 long, 50 blinding + 900 concrete = 950 high.
        assert_eq!(d.extents, Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(600.0, 950.0) });
        let fills: Vec<(f64, f64)> = d
            .primitives
            .iter()
            .filter_map(|p| if let Primitive::Fill { outline, .. } = p { Some((outline[0].y, outline[2].y)) } else { None })
            .collect();
        assert_eq!(fills, vec![(0.0, 50.0), (50.0, 950.0)]);
        // Socket 150 wide, 500 deep, centred: x from 225 to 375, down to y = 950 − 500 = 450.
        let lines: Vec<(Pt2, Pt2)> =
            d.primitives.iter().filter_map(|p| if let Primitive::Line { from, to, .. } = p { Some((*from, *to)) } else { None }).collect();
        assert_eq!(
            lines,
            vec![
                (Pt2::new(0.0, 50.0), Pt2::new(600.0, 50.0)),
                (Pt2::new(225.0, 950.0), Pt2::new(225.0, 450.0)),
                (Pt2::new(225.0, 450.0), Pt2::new(375.0, 450.0)),
                (Pt2::new(375.0, 450.0), Pt2::new(375.0, 950.0)),
            ]
        );
    }

    #[test]
    fn every_invalid_case_is_an_error_note() {
        let nd = "FOUNDATION_DIMENSION_NOT_POSITIVE";
        let mut d = block();
        d.shape = FoundationShape::Block { length_mm: 0.0, width_mm: -1.0, depth_mm: f64::NAN };
        d.socket = None;
        assert_eq!(error_codes(&d), [nd, nd, nd]);
        let mut d = block();
        d.shape = FoundationShape::Cylinder { diameter_mm: 0.0, depth_mm: 900.0 };
        assert_eq!(error_codes(&d), [nd]);
        let mut d = block();
        d.top_below_ground_mm = -50.0;
        assert_eq!(error_codes(&d), ["FOUNDATION_TOP_ABOVE_GROUND"]);
        let mut d = block();
        d.blinding_mm = Some(0.0);
        assert_eq!(error_codes(&d), ["FOUNDATION_BLINDING_NOT_POSITIVE"]);
        let mut d = block();
        d.socket = Some(Socket::Round { diameter_mm: 0.0, depth_mm: 500.0 });
        assert_eq!(error_codes(&d), ["SOCKET_DIMENSION_NOT_POSITIVE"]);
        let mut d = block();
        d.socket = Some(Socket::Round { diameter_mm: 150.0, depth_mm: 901.0 });
        assert_eq!(error_codes(&d), ["SOCKET_DEEPER_THAN_FOUNDATION"]);
        // A socket the full depth (a through sleeve) is allowed.
        d.socket = Some(Socket::Round { diameter_mm: 150.0, depth_mm: 900.0 });
        assert!(error_codes(&d).is_empty());
        // Round socket as wide as the narrower side (500) of the block.
        d.socket = Some(Socket::Round { diameter_mm: 500.0, depth_mm: 500.0 });
        assert_eq!(error_codes(&d), ["SOCKET_WIDER_THAN_FOUNDATION"]);
        d.socket = Some(Socket::Rect { length_mm: 600.0, width_mm: 100.0, depth_mm: 500.0 });
        assert_eq!(error_codes(&d), ["SOCKET_WIDER_THAN_FOUNDATION"]);
        d.socket = Some(Socket::Rect { length_mm: 200.0, width_mm: 100.0, depth_mm: 500.0 });
        assert!(error_codes(&d).is_empty());
        // In a 500 cylinder a 300 × 400 pocket has a 500 diagonal: its corners touch the edge.
        d.shape = FoundationShape::Cylinder { diameter_mm: 500.0, depth_mm: 900.0 };
        d.socket = Some(Socket::Rect { length_mm: 300.0, width_mm: 400.0, depth_mm: 500.0 });
        assert_eq!(error_codes(&d), ["SOCKET_WIDER_THAN_FOUNDATION"]);
        d.socket = Some(Socket::Round { diameter_mm: 500.0, depth_mm: 500.0 });
        assert_eq!(error_codes(&d), ["SOCKET_WIDER_THAN_FOUNDATION"]);
        d.socket = Some(Socket::Round { diameter_mm: 200.0, depth_mm: 500.0 });
        assert!(error_codes(&d).is_empty());

        let mut bad = block();
        bad.top_below_ground_mm = -1.0;
        assert!(bad.check().is_err());
        let g = FoundationModule::default().generate(&element(&bad)).unwrap();
        assert!(g.solids.is_empty() && g.drawing.primitives.is_empty() && g.derived.is_empty());
        assert!(g.notes.iter().all(|n| n.severity == Severity::Error) && g.notes.len() == 1);
    }

    #[test]
    fn placeholder_definition_is_flagged_with_an_info_note() {
        let p = FoundationDefinition::placeholder();
        assert_eq!(p.shape, FoundationShape::Block { length_mm: 1000.0, width_mm: 1000.0, depth_mm: 1000.0 });
        assert_eq!((p.top_below_ground_mm, p.blinding_mm, p.socket.clone()), (0.0, None, None));
        let g = FoundationModule::default().generate(&element(&p)).unwrap();
        let note = g.notes.iter().find(|n| n.code == NOTE_PLACEHOLDER).expect("the placeholder note");
        assert_eq!(note.severity, Severity::Info);
        assert!(note.message.contains("placeholders") && note.message.contains("engineer"));
        assert!(g.notes.iter().any(|n| n.code == "FOUNDATION_CONCRETE_NOT_STATED"));
        assert_eq!(g.solids.len(), 1);
        assert!(block().notes().is_empty());
    }

    #[test]
    fn optional_parts_stay_out_of_the_stored_data_when_absent() {
        let plain = FoundationDefinition { blinding_mm: None, socket: None, ..block() };
        let v = plain.to_value();
        let o = v.as_object().unwrap();
        assert!(!o.contains_key("blinding_mm") && !o.contains_key("socket") && !o.contains_key("placeholder"));
        assert_eq!(FoundationDefinition::from_value(&v).unwrap(), plain);
        assert_eq!(FoundationDefinition::from_value(&block().to_value()).unwrap(), block());
        let mut e = element(&block());
        e.data = serde_json::json!({"shape": {"type": "Pyramid"}});
        assert!(FoundationModule::default().generate(&e).is_err());
    }
}
