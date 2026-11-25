// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! What each civil object becomes in the IFC export. Only mappings verified against the IFC4X3_ADD2
//! schema (through IfcOpenShell, see `u3_tests`) are written; the rest are reported as not mapped.
//!
//! * A civil surface is written as `IfcGeographicElement` with predefined type `TERRAIN`, by the
//!   same exporter code that writes an imported surface (which the road tests of `afrobim-ifc`
//!   validate): the surface is handed to it as an element of the imported surface category holding
//!   the built TIN.
//! * Alignments are written as they are today (`IfcAlignment`).
//! * A grading surface is written the same way (`IfcGeographicElement`, `TERRAIN`).
//! * A corridor is written from its built shapes (`civil_ifc_pkg`): each shape code as the entity its
//!   classification names (`IfcCourse`, `IfcPavement`, `IfcKerb`, `IfcEarthworksCut`,
//!   `IfcEarthworksFill`), or `IfcCivilElement` when none is entered, as solids between the sections
//!   with its material and base quantities, and the cut and fill from the earthworks lines.
//!   They sit in an `IfcRoadPart` of the road.
//! * A pipe network is written as `IfcPipeSegment`, `IfcDistributionChamberElement` and an
//!   `IfcDistributionSystem` that groups them, with the diameters and inverts as properties.
//! * Not mapped: point sets, parcels (no verified entity fits), ground profiles, profile views,
//!   assemblies, sample line groups, feature lines, survey figures, point clouds. Alignments are
//!   written by the exporter's alignment mode (horizontal, vertical, superelevation).

use afrobim_kernel::{Element, Generated, Module};

use crate::civil::*;

/// One line for the Properties panel.
pub fn mapping_note(category: &str) -> &'static str {
    match category {
        SURFACE => "IfcGeographicElement, TERRAIN (checked against IFC4X3_ADD2)",
        GRADING => "IfcGeographicElement, TERRAIN, from its surface (checked against IFC4X3_ADD2)",
        CORRIDOR => "Shapes as IfcCourse, IfcPavement, IfcKerb or IfcCivilElement by their classification, cut and fill as IfcEarthworksCut and IfcEarthworksFill, in an IfcRoadPart (checked against IFC4X3_ADD2)",
        PIPENET => "IfcPipeSegment and IfcDistributionChamberElement in an IfcDistributionSystem (checked against IFC4X3_ADD2)",
        POINTS => "Not mapped: no verified IFC entity fits a point set (kept in the document and in LandXML CgPoints)",
        PARCEL => "Not mapped: no verified IFC entity fits a parcel (kept in the document and in LandXML Parcels)",
        PROFILE | PROFILE_VIEW => "Not mapped: a profile is written with its alignment (the vertical layout of the alignment mode) and in LandXML",
        PIPEPROFILE => "Not mapped: a drawing of a pipe run; the pipes themselves are written with their network",
        FEATURELINE => "Not mapped: no verified IFC entity",
        FIGURES | POINTCLOUD => "Not mapped: no verified IFC entity",
        INTERSECTION | ROUNDABOUT => "Through their alignments and corridors, which are written as above; the intersection object itself is not",
        ASSEMBLY | SAMPLES => "Not mapped: a template and a set of section lines, no building element",
        _ => "Not mapped",
    }
}

/// One row of the export dialog: a category, how many objects, what it is written as, and what is not.
pub struct ExportRow {
    pub label: String,
    pub count: usize,
    pub written: String,
    pub not_mapped: String,
}

/// What the export will write, per category of civil object in the document.
pub fn export_rows(doc: &afrobim_kernel::Document) -> Vec<ExportRow> {
    let counts = counts(doc);
    let n = |c: &str| counts.get(c).copied().unwrap_or(0);
    let mut rows = Vec::new();
    let alignments = doc.elements().iter().filter(|e| e.category == afrobim_alignment::CATEGORY).count();
    if alignments > 0 {
        rows.push(ExportRow { label: "Alignments".into(), count: alignments, written: "IfcAlignment: horizontal, vertical and superelevation segments in the Geometry and Linear modes, a plan polyline in Simple".into(), not_mapped: "station equations beyond the referents the mode writes".into() });
    }
    for (cat, label, written, not_mapped) in [
        (SURFACE, "Surfaces", "IfcGeographicElement, TERRAIN", ""),
        (GRADING, "Grading surfaces", "IfcGeographicElement, TERRAIN", "the grading's feature lines"),
        (CORRIDOR, "Corridors", "IfcCourse, IfcPavement, IfcKerb or IfcCivilElement per shape, IfcEarthworksCut and IfcEarthworksFill, in an IfcRoadPart", "a shape with no classification entered is an IfcCivilElement"),
        (PIPENET, "Pipe networks", "IfcPipeSegment, IfcDistributionChamberElement, IfcDistributionSystem", "headwalls, outfalls and null structures"),
        (POINTS, "Point sets", "", "no verified IFC entity fits a point"),
        (PARCEL, "Parcels", "", "no verified IFC entity fits a parcel"),
        (PROFILE, "Ground profiles", "", "written with the alignment's vertical layout only where it is a design profile"),
        (PROFILE_VIEW, "Profile views", "", "a drawing"),
        (SAMPLES, "Sample line groups", "", "section lines, not building elements"),
        (ASSEMBLY, "Assemblies", "", "templates, not building elements"),
        (FEATURELINE, "Feature lines", "", "no verified IFC entity"),
        (INTERSECTION, "Intersections", "through their alignments and corridors", "the intersection object itself"),
        (ROUNDABOUT, "Roundabouts", "through their alignments and corridors", "the roundabout object itself"),
        (FIGURES, "Survey figures", "", "no verified IFC entity"),
        (POINTCLOUD, "Point clouds", "", "no verified IFC entity"),
    ] {
        if n(cat) > 0 {
            rows.push(ExportRow { label: label.into(), count: n(cat), written: written.into(), not_mapped: not_mapped.into() });
        }
    }
    rows
}

impl crate::app::AfroBimApp {
    /// The civil surfaces as elements the existing surface exporter writes, with their generated
    /// meshes. Built scenes are brought up to date first.
    pub(crate) fn civil_terrain_inputs(&mut self) -> Vec<(Element, Option<Generated>)> {
        self.civil_build_now();
        let mut out = Vec::new();
        for e in self.doc.elements().iter().filter(|e| e.category == SURFACE || e.category == GRADING) {
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

    /// The civil packages of corridors and pipe networks: elements and solids the exporter writes,
    /// and what could not be built, in words.
    pub(crate) fn civil_packages(&mut self) -> (Vec<crate::civil_ifc_pkg::Package>, Vec<String>) {
        let ctx = self.civil_ctx();
        let mut out = Vec::new();
        let mut notes = Vec::new();
        for e in self.doc.elements().iter().filter(|e| (e.category == CORRIDOR || e.category == PIPENET) && !hidden(e)) {
            let r = if e.category == CORRIDOR {
                crate::civil_ifc_pkg::corridor_package(e, &ctx)
            } else {
                get::<crate::civil_pipes::PipeNetData>(&self.doc, e.id, PIPENET).ok_or_else(|| "its data cannot be read".to_string()).and_then(|d| crate::civil_ifc_pkg::pipe_package(e, &d))
            };
            match r {
                Ok(p) => out.push(p),
                Err(why) => notes.push(format!("{} not written: {why}", e.name)),
            }
        }
        (out, notes)
    }

    /// What the export leaves out, in words, for the command history.
    pub(crate) fn civil_unmapped_note(&self) -> Option<String> {
        let rows = export_rows(&self.doc);
        let parts: Vec<String> = rows.iter().filter(|r| r.written.is_empty()).map(|r| format!("{} {}", r.count, r.label.to_lowercase())).collect();
        (!parts.is_empty()).then(|| format!("Civil objects not mapped to IFC: {}. They stay in the document and in LandXML (CIVILXML).", parts.join(", ")))
    }
}
