//! What each civil object becomes in the IFC export. Only mappings verified against the IFC4X3_ADD2
//! schema (through IfcOpenShell, see `u3_tests`) are written; the rest are reported as not mapped.
//!
//! * A civil surface is written as `IfcGeographicElement` with predefined type `TERRAIN`, by the
//!   same exporter code that writes an imported surface (which the road tests of `afrobim-ifc`
//!   validate): the surface is handed to it as an element of the imported surface category holding
//!   the built TIN.
//! * Alignments are written as they are today (`IfcAlignment`).
//! * Not mapped: point sets, parcels, ground profiles, profile views, assemblies, corridors and
//!   sample line groups. The corridor shapes could be `IfcPavement` and `IfcCourse` through the
//!   pavement exporter, but that takes a pavement definition per layer, which the corridor model
//!   does not hold; no other mapping was verified.

use afrobim_kernel::{Element, Generated, Module};

use crate::civil::*;

/// One line for the Properties panel.
pub fn mapping_note(category: &str) -> &'static str {
    match category {
        SURFACE => "IfcGeographicElement, TERRAIN (checked against IFC4X3_ADD2)",
        POINTS => "Not mapped: kept in the document and in LandXML (CgPoints)",
        PARCEL => "Not mapped: kept in the document and in LandXML (Parcels)",
        PROFILE | PROFILE_VIEW => "Not mapped: a profile is written with its alignment only as LandXML",
        PIPENET | PIPEPROFILE => "Not mapped: no verified IFC entity (LandXML only)",
        FEATURELINE | GRADING => "Not mapped: no verified IFC entity",
        INTERSECTION | ROUNDABOUT => "Not mapped: no verified IFC entity",
        ASSEMBLY | CORRIDOR | SAMPLES => "Not mapped yet: no verified IFC entity for this object",
        _ => "Not mapped",
    }
}

impl crate::app::AfroBimApp {
    /// The civil surfaces as elements the existing surface exporter writes, with their generated
    /// meshes. Built scenes are brought up to date first.
    pub(crate) fn civil_terrain_inputs(&mut self) -> Vec<(Element, Option<Generated>)> {
        self.civil_build_now();
        let mut out = Vec::new();
        for e in self.doc.elements().iter().filter(|e| e.category == SURFACE) {
            let Some(tin) = self.civil.tin_of(e.id) else { continue };
            let source = e.properties.get(PROP_SOURCE).cloned();
            let def = afrobim_corridor::SurfaceDefinition { tin: (*tin).clone(), source, crs: None };
            let mut el = e.clone();
            el.category = afrobim_corridor::SURFACE_CATEGORY.to_string();
            el.data = serde_json::to_value(&def).unwrap_or_default();
            let b = tin.bounds();
            let at = afrobim_kernel::model_point_from_crs(b.min_x, b.min_y);
            el.origin = at;
            if let Some(s) = &mut el.site {
                s.x = at.x;
                s.y = at.y;
            }
            let g = afrobim_corridor::SurfaceModule::default().generate(&el).ok();
            out.push((el, g));
        }
        out
    }

    /// What the export leaves out, in words, for the command history.
    pub(crate) fn civil_unmapped_note(&self) -> Option<String> {
        let counts = counts(&self.doc);
        let parts: Vec<String> = KINDS.iter().filter(|k| k.0 != SURFACE).filter_map(|k| counts.get(k.0).filter(|n| **n > 0).map(|n| format!("{n} {}", k.2.to_lowercase()))).collect();
        (!parts.is_empty()).then(|| format!("Civil objects not mapped to IFC: {}. They stay in the document and in LandXML (CIVILXML).", parts.join(", ")))
    }
}
