// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The gantry as a kernel module.

use std::collections::BTreeMap;

use afrobim_kernel::{Element, Generated, Module, Note, Severity};

use crate::design::{GantryChecks, Status};
use crate::{fixed, GantryDefinition, GantryQuantities, GantryType, GANTRY_CATEGORY};

/// Draws a `gantry` element and reports on it.
///
/// `generate` returns the sheet elevation ([`GantryDefinition::drawing`]), the solids
/// ([`GantryDefinition::solids`]), the definition's validation notes and these derived values:
///
/// | key | value |
/// |---|---|
/// | `Type` | portal or cantilever |
/// | `Clear span` / `Outreach` | mm, along the gantry line |
/// | `Skew` | degrees |
/// | `Leg lengths` | envelope lengths, base to top of boom, mm |
/// | `Boom underside` | mm above the reference level |
/// | `Reference level` | m, when the gantry has been set out on a profile |
/// | `Signs carried`, `Sign area` | number and m² |
/// | `Foundation concrete` | m³ of the entered blocks |
/// | `Steel mass` | kg, only when every member has a mass per metre entered |
/// | `Headroom`, `Headroom required` | mm, from the requirement checks |
///
/// The requirement checks are run without the alignment (the module sees only the element), so
/// the headroom here takes the longitudinal grade as 0; a failed check is added as a Warning
/// note and an incomplete headroom check as an Info note. Use
/// [`GantryChecks::run`](crate::design::GantryChecks::run) with the alignment for the report.
#[derive(Debug, Clone, Copy, Default)]
pub struct GantryModule;

impl Module for GantryModule {
    fn category(&self) -> &str {
        GANTRY_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Sign gantries"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let def = GantryDefinition::from_value(&element.data).map_err(|e| e.to_string())?;
        let mut notes = def.notes();
        if notes.iter().any(|n| n.severity == Severity::Error) {
            return Ok(Generated { notes, ..Generated::default() });
        }
        let q = GantryQuantities::measure(&element.name, &def).map_err(|e| e.to_string())?;
        let mut derived = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            derived.insert(k.to_string(), v);
        };
        put("Type", def.gantry_type.label().to_string());
        let span_key = match def.gantry_type {
            GantryType::Portal => "Clear span",
            GantryType::Cantilever => "Outreach",
        };
        put(span_key, format!("{} mm", fixed(q.span_or_reach_mm, 0, 1)));
        put("Skew", format!("{}°", fixed(def.skew_deg, 0, 3)));
        put("Leg lengths", format!("{} mm", q.leg_lengths_mm.iter().map(|l| fixed(*l, 0, 1)).collect::<Vec<_>>().join("; ")));
        put("Boom underside", format!("{} mm above reference level", fixed(def.boom.underside_above_reference_mm, 0, 1)));
        if let Some(level) = def.reference_level_m {
            put("Reference level", format!("{} m", fixed(level, 3, 3)));
        }
        put("Signs carried", q.sign_count.to_string());
        put("Sign area", format!("{} m²", fixed(q.sign_area_m2, 3, 3)));
        put("Foundation concrete", format!("{} m³", fixed(q.foundation_concrete_m3, 3, 3)));
        if let Some(mass) = q.steel_mass_kg {
            put("Steel mass", format!("{} kg", fixed(mass, 1, 1)));
        }
        let checks = GantryChecks::run(&element.name, &def, None).map_err(|e| e.to_string())?;
        put("Headroom", format!("{} mm", fixed(checks.headroom.headroom_mm, 0, 1)));
        put("Headroom required", format!("{} mm", fixed(checks.headroom.required_mm, 0, 1)));
        for line in &checks.lines {
            let (severity, code) = match line.status {
                Status::Fails => (Severity::Warning, "gantry.check.fails"),
                Status::Incomplete => (Severity::Info, "gantry.check.incomplete"),
                _ => continue,
            };
            notes.push(Note { severity, code: code.to_string(), message: format!("{} ({})", line.text, line.citation) });
        }
        Ok(Generated { drawing: def.drawing(), notes, derived, solids: def.solids() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;
    use crate::{add_gantry, NOTE_SCHEMATIC};
    use afrobim_kernel::{Document, ModuleRegistry};

    #[test]
    fn generates_drawing_solids_notes_and_derived_values() {
        let mut doc = Document::default();
        let mut def = portal();
        def.legs.iter_mut().for_each(|l| l.mass_kg_per_m = Some(200.0));
        def.boom.mass_kg_per_m = Some(150.0);
        let id = add_gantry(&mut doc, "G1", &def, None).unwrap();
        let mut registry = ModuleRegistry::default();
        registry.register(Box::new(GantryModule));
        assert!(registry.categories().any(|(c, n)| c == "gantry" && n == "Sign gantries"));
        let e = doc.find(id).unwrap();
        let g = registry.generate(e).unwrap();
        // The element's sheet size is the drawing's.
        assert_eq!((g.drawing.extents.width(), g.drawing.extents.height()), (e.width, e.height));
        assert_eq!(g.solids.len(), 5);
        assert_eq!(g.derived["Type"], "portal");
        assert_eq!(g.derived["Clear span"], "17400 mm");
        assert_eq!(g.derived["Leg lengths"], "7000; 6500 mm");
        assert_eq!(g.derived["Foundation concrete"], "18.000 m³");
        assert_eq!(g.derived["Steel mass"], "5310.0 kg");
        assert_eq!((g.derived["Headroom"].as_str(), g.derived["Headroom required"].as_str()), ("6000 mm", "5700 mm"));
        assert!(!g.derived.contains_key("Reference level"));
        assert!(g.notes.iter().any(|n| n.code == NOTE_SCHEMATIC));
        // The deflection has not been entered: said so, as information.
        assert!(g.notes.iter().any(|n| n.code == "gantry.check.incomplete" && n.severity == Severity::Info));
        assert!(!g.notes.iter().any(|n| n.code == "gantry.check.fails"));
    }

    #[test]
    fn a_low_boom_is_warned_about_and_an_invalid_gantry_is_not_drawn() {
        let mut doc = Document::default();
        let mut low = cantilever();
        low.boom.underside_above_reference_mm = 5000.0;
        let id = add_gantry(&mut doc, "C1", &low, None).unwrap();
        let g = GantryModule.generate(doc.find(id).unwrap()).unwrap();
        assert_eq!(g.derived["Outreach"], "7750 mm");
        assert!(!g.derived.contains_key("Steel mass"));
        assert!(g.notes.iter().any(|n| n.code == "gantry.check.fails" && n.severity == Severity::Warning && n.message.contains("CD 127 v1.0.1")));
        // Invalid data: notes only.
        let mut bad = portal();
        bad.legs.truncate(1);
        doc.find_mut(id).unwrap().data = bad.to_value();
        let g = GantryModule.generate(doc.find(id).unwrap()).unwrap();
        assert!(g.drawing.primitives.is_empty() && g.solids.is_empty());
        assert!(g.notes.iter().any(|n| n.severity == Severity::Error));
        doc.find_mut(id).unwrap().data = serde_json::json!({"nonsense": true});
        assert!(GantryModule.generate(doc.find(id).unwrap()).is_err());
    }
}
