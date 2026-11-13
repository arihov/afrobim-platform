//! Posts: the definition stored in an element's `data` and the kernel module that draws it.

use std::collections::BTreeMap;

use afrobim_draw::{Colour, Drawing, Extents, Mesh3, Primitive, Pt2};
use afrobim_kernel::{Element, Generated, Module, Note};
use serde::{Deserialize, Serialize};

use crate::quantities::{fixed, PostQuantities};
use crate::{
    error_note, error_text, info_note, positive, Section, SupportError, NOTE_PLACEHOLDER, PLACEHOLDER_POST_EMBEDMENT_MM,
    PLACEHOLDER_POST_LENGTH_MM, PLACEHOLDER_POST_OUTSIDE_DIAMETER_MM, PLACEHOLDER_POST_WALL_MM, STEEL_DENSITY_KG_M3_NOMINAL,
};

/// The element category [`PostModule`] owns.
pub const POST_CATEGORY: &str = "post";

/// Display colour of a post in the 3D view. A screen colour, not a finish.
const POST_RGB: [u8; 3] = [0x8A, 0x8F, 0x94];

/// A post as the engineer describes it. Nothing here is checked against any standard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostDefinition {
    pub section: Section,
    /// Total length of the post, including the buried part, mm.
    pub length_mm: f64,
    /// Length below ground level, mm. Zero for a post that stops at ground (e.g. on a base).
    pub embedment_mm: f64,
    /// Free text, e.g. the grade the specification calls for. Not interpreted.
    #[serde(default)]
    pub material: String,
    /// Free text, e.g. the protective coating. Not interpreted.
    #[serde(default)]
    pub finish: String,
    /// True while the dimensions are still the `PLACEHOLDER_*` starting values. The module
    /// raises an Info note on the element until the engineer's editor clears this.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub placeholder: bool,
}

impl PostDefinition {
    /// A drawable starting definition made of the `PLACEHOLDER_*` constants, flagged as such.
    /// It is not a recommended, typical or compliant post.
    pub fn placeholder() -> Self {
        PostDefinition {
            section: Section::Chs { outside_diameter_mm: PLACEHOLDER_POST_OUTSIDE_DIAMETER_MM, wall_mm: PLACEHOLDER_POST_WALL_MM },
            length_mm: PLACEHOLDER_POST_LENGTH_MM,
            embedment_mm: PLACEHOLDER_POST_EMBEDMENT_MM,
            material: String::new(),
            finish: String::new(),
            placeholder: true,
        }
    }

    pub fn from_value(data: &serde_json::Value) -> Result<Self, SupportError> {
        serde_json::from_value(data.clone()).map_err(|e| SupportError::InvalidDefinition(format!("the post definition cannot be read: {e}")))
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a post definition always serialises")
    }

    /// Height of the post top above ground, mm.
    pub fn above_ground_mm(&self) -> f64 {
        self.length_mm - self.embedment_mm
    }

    /// Validation and information notes. Error notes mean the post cannot be drawn or measured.
    pub fn notes(&self) -> Vec<Note> {
        let mut out = self.section.issues();
        if !positive(self.length_mm) {
            out.push(error_note("POST_LENGTH_NOT_POSITIVE", format!("The post length must be greater than zero (it is {}).", self.length_mm)));
        }
        if !(self.embedment_mm.is_finite() && self.embedment_mm >= 0.0) {
            out.push(error_note("POST_EMBEDMENT_NEGATIVE", format!("The embedment must not be negative (it is {}).", self.embedment_mm)));
        } else if positive(self.length_mm) && self.embedment_mm >= self.length_mm {
            out.push(error_note(
                "POST_EMBEDMENT_NOT_LESS_THAN_LENGTH",
                format!("An embedment of {} mm leaves nothing above ground on a post {} mm long.", self.embedment_mm, self.length_mm),
            ));
        }
        if self.placeholder {
            out.push(info_note(
                NOTE_PLACEHOLDER,
                "The post dimensions are placeholders so that the element can be drawn. They are not design values; the engineer must set the section, length and embedment.",
            ));
        }
        if self.material.trim().is_empty() {
            out.push(info_note("POST_MATERIAL_NOT_STATED", "No material is stated for this post."));
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

    /// The post for the 3D view, in its own frame: plan bounding box on x ∈ [0, width],
    /// y ∈ [0, depth], **z = 0 at ground level**, from z = −embedment to z = length − embedment.
    /// A hollow post is shown as its outer solid (see [`Section::solids`]).
    pub fn solids(&self) -> Vec<Mesh3> {
        self.section.solids("Post", -self.embedment_mm, self.above_ground_mm(), POST_RGB)
    }

    /// Elevation for the design sheet, mm: the post seen looking along +Y, its foot on y = 0
    /// and its left edge on x = 0. The inside faces of a hollow section and the ground level
    /// are shown as lines.
    pub fn drawing(&self) -> Drawing {
        let (w, _) = self.section.bounding_mm();
        let l = self.length_mm;
        let thin = (w * 0.02).max(0.5);
        let mut primitives = vec![Primitive::Fill {
            outline: vec![Pt2::new(0.0, 0.0), Pt2::new(w, 0.0), Pt2::new(w, l), Pt2::new(0.0, l)],
            colour: Colour::Grey,
        }];
        if let Some(t) = self.section.wall_mm() {
            for x in [t, w - t] {
                primitives.push(Primitive::Line { from: Pt2::new(x, 0.0), to: Pt2::new(x, l), width: thin, colour: None });
            }
        }
        if self.embedment_mm > 0.0 {
            primitives.push(Primitive::Line {
                from: Pt2::new(0.0, self.embedment_mm),
                to: Pt2::new(w, self.embedment_mm),
                width: thin,
                colour: Some(Colour::Brown),
            });
        }
        Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(w, l) } }
    }
}

/// The kernel module for category [`POST_CATEGORY`].
///
/// `generate` reads a [`PostDefinition`] from the element's `data`. A definition with Error
/// notes yields the notes and no geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct PostModule {
    /// Density used for the mass shown with the element, kg/m³.
    pub density_kg_m3: f64,
}

impl Default for PostModule {
    /// Uses [`STEEL_DENSITY_KG_M3_NOMINAL`].
    fn default() -> Self {
        PostModule { density_kg_m3: STEEL_DENSITY_KG_M3_NOMINAL }
    }
}

impl PostModule {
    pub fn with_density(density_kg_m3: f64) -> Self {
        PostModule { density_kg_m3 }
    }
}

impl Module for PostModule {
    fn category(&self) -> &str {
        POST_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Sign posts"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let def = PostDefinition::from_value(&element.data).map_err(|e| e.to_string())?;
        let notes = def.notes();
        if error_text(&notes).is_some() {
            return Ok(Generated { notes, ..Generated::default() });
        }
        let q = PostQuantities::measure(&element.name, &def, self.density_kg_m3).map_err(|e| e.to_string())?;
        let mut derived = BTreeMap::new();
        derived.insert("Section".to_string(), def.section.label());
        derived.insert("Length".to_string(), format!("{} mm", fixed(def.length_mm, 0, 1)));
        derived.insert("Above ground".to_string(), format!("{} mm", fixed(def.above_ground_mm(), 0, 1)));
        derived.insert("Below ground".to_string(), format!("{} mm", fixed(def.embedment_mm, 0, 1)));
        derived.insert("Section area".to_string(), format!("{} mm²", fixed(q.section_area_mm2, 0, 1)));
        derived.insert("Material volume".to_string(), format!("{} m³", fixed(q.volume_m3, 6, 6)));
        derived.insert("Coating area (outside only)".to_string(), format!("{} m²", fixed(q.coating_area_m2, 3, 3)));
        derived.insert(format!("Mass at {} kg/m³", fixed(q.density_kg_m3, 0, 1)), format!("{} kg", fixed(q.mass_kg, 2, 2)));
        Ok(Generated { drawing: def.drawing(), notes, derived, solids: def.solids() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_kernel::{Document, Pt, Severity};

    fn tube() -> PostDefinition {
        PostDefinition {
            section: Section::Chs { outside_diameter_mm: 100.0, wall_mm: 10.0 },
            length_mm: 4000.0,
            embedment_mm: 1000.0,
            material: "as specified".into(),
            finish: "as specified".into(),
            placeholder: false,
        }
    }

    fn element(def: &PostDefinition) -> Element {
        let mut d = Document::default();
        let id = d.add(POST_CATEGORY, "ADS-001-P1", Pt::new(0.0, 0.0), 100.0, 4000.0, "Supports", def.to_value());
        d.find(id).unwrap().clone()
    }

    fn error_codes(def: &PostDefinition) -> Vec<String> {
        def.notes().into_iter().filter(|n| n.severity == Severity::Error).map(|n| n.code).collect()
    }

    #[test]
    fn solids_put_ground_at_z_zero_and_the_buried_part_below() {
        let g = PostModule::default().generate(&element(&tube())).unwrap();
        assert!(g.notes.is_empty());
        assert_eq!(g.solids.len(), 1);
        let m = &g.solids[0];
        assert_eq!(m.name, "Post");
        let zs: Vec<f64> = m.vertices.iter().map(|v| v[2]).collect();
        assert_eq!(zs.iter().cloned().fold(f64::INFINITY, f64::min), -1000.0);
        assert_eq!(zs.iter().cloned().fold(f64::NEG_INFINITY, f64::max), 3000.0);
        // Plan bounding box on [0, 100] × [0, 100]: the axis is at (50, 50).
        let xs: Vec<f64> = m.vertices.iter().map(|v| v[0]).collect();
        assert!((xs.iter().cloned().fold(f64::INFINITY, f64::min) - 0.0).abs() < 1e-9);
        assert!((xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn the_shown_quantities_use_the_hollow_section_not_the_display_mesh() {
        let g = PostModule::default().generate(&element(&tube())).unwrap();
        // Steel: 900π mm² × 4000 mm = 3 600 000π mm³ = 0.011309734 m³.
        // Mass at 7850 kg/m³: 0.011309734 × 7850 = 88.78 kg.
        // Coating: π × 100 × 4000 = 400 000π mm² = 1.257 m².
        assert_eq!(g.derived["Section"], "CHS D100 t10");
        assert_eq!(g.derived["Section area"], "2827.4 mm²");
        assert_eq!(g.derived["Material volume"], "0.011310 m³");
        assert_eq!(g.derived["Mass at 7850 kg/m³"], "88.78 kg");
        assert_eq!(g.derived["Coating area (outside only)"], "1.257 m²");
        assert_eq!(g.derived["Above ground"], "3000 mm");
        assert_eq!(g.derived["Below ground"], "1000 mm");
        // The display mesh is the outer cylinder (about 2500π × 4000 mm³ = 0.0314 m³), nearly
        // three times the steel; it must not be what is reported.
        let mesh_m3 = g.solids[0].volume() / 1.0e9;
        assert!(mesh_m3 > 0.031 && mesh_m3 < 0.0315);
        // A caller-supplied density replaces the nominal one: 0.011309734 × 2700 = 30.54 kg.
        let g = PostModule::with_density(2700.0).generate(&element(&tube())).unwrap();
        assert_eq!(g.derived["Mass at 2700 kg/m³"], "30.54 kg");
    }

    #[test]
    fn elevation_has_its_lower_left_on_the_origin() {
        let d = tube().drawing();
        assert_eq!(d.extents, Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(100.0, 4000.0) });
        match &d.primitives[0] {
            Primitive::Fill { outline, .. } => {
                assert_eq!(outline, &vec![Pt2::new(0.0, 0.0), Pt2::new(100.0, 0.0), Pt2::new(100.0, 4000.0), Pt2::new(0.0, 4000.0)]);
            }
            other => panic!("expected the outline first, got {other:?}"),
        }
        // Two wall lines at x = 10 and x = 90, then the ground line at y = embedment.
        let lines: Vec<(Pt2, Pt2)> =
            d.primitives.iter().filter_map(|p| if let Primitive::Line { from, to, .. } = p { Some((*from, *to)) } else { None }).collect();
        assert_eq!(
            lines,
            vec![
                (Pt2::new(10.0, 0.0), Pt2::new(10.0, 4000.0)),
                (Pt2::new(90.0, 0.0), Pt2::new(90.0, 4000.0)),
                (Pt2::new(0.0, 1000.0), Pt2::new(100.0, 1000.0)),
            ]
        );
        // A solid post with no embedment is just its outline.
        let solid = PostDefinition { section: Section::SolidRect { width_mm: 80.0, depth_mm: 40.0 }, embedment_mm: 0.0, ..tube() };
        assert_eq!(solid.drawing().primitives.len(), 1);
    }

    #[test]
    fn invalid_dimensions_are_error_notes_and_give_no_geometry() {
        let mut d = tube();
        d.length_mm = 0.0;
        assert_eq!(error_codes(&d), ["POST_LENGTH_NOT_POSITIVE"]);
        let mut d = tube();
        d.length_mm = f64::NAN;
        assert_eq!(error_codes(&d), ["POST_LENGTH_NOT_POSITIVE"]);
        let mut d = tube();
        d.embedment_mm = -1.0;
        assert_eq!(error_codes(&d), ["POST_EMBEDMENT_NEGATIVE"]);
        let mut d = tube();
        d.embedment_mm = 4000.0; // equal to the length
        assert_eq!(error_codes(&d), ["POST_EMBEDMENT_NOT_LESS_THAN_LENGTH"]);
        d.embedment_mm = 5000.0;
        assert_eq!(error_codes(&d), ["POST_EMBEDMENT_NOT_LESS_THAN_LENGTH"]);
        let mut d = tube();
        d.section = Section::Chs { outside_diameter_mm: 100.0, wall_mm: 50.0 };
        assert_eq!(error_codes(&d), ["SECTION_WALL_TOO_THICK"]);
        assert!(d.check().is_err());

        let g = PostModule::default().generate(&element(&d)).unwrap();
        assert!(g.solids.is_empty() && g.drawing.primitives.is_empty() && g.derived.is_empty());
        assert_eq!(g.notes.len(), 1);
        assert_eq!(g.notes[0].severity, Severity::Error);
        // Zero embedment is allowed.
        let mut d = tube();
        d.embedment_mm = 0.0;
        assert!(error_codes(&d).is_empty());
    }

    #[test]
    fn placeholder_definition_is_flagged_with_an_info_note() {
        let p = PostDefinition::placeholder();
        assert!(p.placeholder);
        assert_eq!(p.section, Section::Chs { outside_diameter_mm: 100.0, wall_mm: 10.0 });
        assert_eq!((p.length_mm, p.embedment_mm), (3000.0, 1000.0));
        let g = PostModule::default().generate(&element(&p)).unwrap();
        let note = g.notes.iter().find(|n| n.code == NOTE_PLACEHOLDER).expect("the placeholder note");
        assert_eq!(note.severity, Severity::Info);
        assert!(note.message.contains("placeholders") && note.message.contains("engineer"));
        assert!(g.notes.iter().any(|n| n.code == "POST_MATERIAL_NOT_STATED" && n.severity == Severity::Info));
        assert!(!g.solids.is_empty(), "a placeholder post is still drawable");
        // Once the engineer's values are in, the note goes.
        assert!(tube().notes().is_empty());
    }

    #[test]
    fn unreadable_data_is_an_error_not_a_panic() {
        let mut e = element(&tube());
        e.data = serde_json::json!({"section": {"type": "Nonsense"}});
        assert!(PostModule::default().generate(&e).is_err());
    }

    #[test]
    fn the_placeholder_flag_is_only_stored_when_set() {
        assert!(!tube().to_value().as_object().unwrap().contains_key("placeholder"));
        assert_eq!(PostDefinition::placeholder().to_value()["placeholder"], serde_json::json!(true));
        assert_eq!(PostDefinition::from_value(&tube().to_value()).unwrap(), tube());
    }
}
