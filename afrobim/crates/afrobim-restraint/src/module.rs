// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The barrier run as a kernel element: category `"restraint"`, `data` = [`BarrierElement`].
//!
//! A kernel module sees one element at a time, so it cannot look the alignment up. The element
//! therefore stores the run **and** its geometry resolved against the alignment
//! ([`ResolvedRun`]). [`add_barrier_run`] and [`update_barrier_run`] resolve it;
//! [`sync_barrier_runs`] resolves every run again from the alignments in the document and is to
//! be called after an alignment is edited or re-imported.
//!
//! Like an alignment, a run lives in plan (site) coordinates: its drawing is the plan at 1 m =
//! 1000 units with its lower-left on the drawing origin, and its `SitePlacement` gives where
//! that origin is on site (x, y in millimetres; z the lowest level of the run, the origin of
//! the solids' z; rotation 0).

use std::collections::BTreeMap;

use afrobim_alignment::Alignment;
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, Note, Pt, Severity, SitePlacement};
use serde::{Deserialize, Serialize};

use crate::{fixed, plan_drawing, resolve, solids, solids_origin, BarrierRun, ResolvedRun, RestraintError};

/// The element category this crate owns.
pub const CATEGORY: &str = "restraint";
/// The layer runs are put on. Hidden on the design sheet when first created, as the alignment
/// layer is: the plan is kilometres away from the sheet where sign faces are laid out.
pub const RESTRAINT_LAYER: &str = "Road restraint";

/// Note code (Info) on every run: its solids are a schematic.
pub const NOTE_SCHEMATIC: &str = "RESTRAINT_SCHEMATIC";
/// Note code (Info) on a run whose levels are 0 because the alignment's profile is missing or
/// does not cover it.
pub const NOTE_LEVELS_ZERO: &str = "RESTRAINT_LEVELS_ZERO";

/// What a restraint element stores.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarrierElement {
    pub run: BarrierRun,
    pub resolved: ResolvedRun,
}

impl BarrierElement {
    pub fn new(run: BarrierRun, alignment: &Alignment) -> Result<BarrierElement, RestraintError> {
        let resolved = resolve(&run, alignment)?;
        Ok(BarrierElement { run, resolved })
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a barrier run serialises")
    }

    pub fn from_value(value: &serde_json::Value) -> Result<BarrierElement, RestraintError> {
        serde_json::from_value(value.clone()).map_err(|e| RestraintError::NotABarrierRun("element".into(), e.to_string()))
    }
}

/// The kernel module for barrier runs.
///
/// `generate` returns the plan drawing ([`plan_drawing`]), the schematic solids ([`solids`])
/// and these derived values:
///
/// | key | value |
/// |---|---|
/// | `Alignment` | name of the alignment |
/// | `Start station`, `End station` | displayed stations, as `1+250.000` |
/// | `Offset` | signed offset of the traffic face, m |
/// | `Traffic face` | `left`, `right` or `both` |
/// | `Containment level`, `Working width class`, `Impact severity level` | as entered |
/// | `Length along barrier` | m, 3 decimals |
/// | `Centreline length` | m, 3 decimals |
/// | `Posts` | number, with the entered spacing, or `none` |
/// | `Beam height` | as entered, m |
/// | `Start end`, `End end` | the end treatments |
/// | `Levels` | `alignment profile (centreline)` or `none: z = 0` |
/// | `plan_origin_x_m`, `plan_origin_y_m` | plan coordinates of the drawing origin, m |
///
/// Notes: the run's own validation notes ([`BarrierRun::notes`]); [`NOTE_SCHEMATIC`] always;
/// [`NOTE_LEVELS_ZERO`] when there is no usable profile. A run with Error notes gives the notes
/// and no geometry. Nothing here checks a requirement: see [`crate::design`].
#[derive(Debug, Clone, Copy, Default)]
pub struct RestraintModule;

impl Module for RestraintModule {
    fn category(&self) -> &str {
        CATEGORY
    }

    fn display_name(&self) -> &str {
        "Road restraint"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let b: BarrierElement =
            serde_json::from_value(element.data.clone()).map_err(|e| format!("'{}' is not a valid barrier run: {e}", element.name))?;
        let (run, resolved) = (&b.run, &b.resolved);
        let mut notes = run.notes();
        if notes.iter().any(|n| n.severity == Severity::Error) {
            return Ok(Generated { notes, ..Default::default() });
        }
        notes.push(Note {
            severity: Severity::Info,
            code: NOTE_SCHEMATIC.into(),
            message: "The 3D form is a schematic at the entered beam height and post spacing, not a model of the product.".into(),
        });
        if !resolved.levels_from_profile {
            notes.push(Note {
                severity: Severity::Info,
                code: NOTE_LEVELS_ZERO.into(),
                message: "The alignment has no profile covering this run, so the run is drawn at level 0.".into(),
            });
        }
        let plan = plan_drawing(run, resolved);
        let mut derived = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            derived.insert(k.to_string(), v);
        };
        put("Alignment", run.alignment.clone());
        put("Start station", resolved.start_label.clone());
        put("End station", resolved.end_label.clone());
        put("Offset", format!("{} m", fixed(run.offset_m, 0, 3)));
        put("Traffic face", run.traffic_face.label().into());
        put("Containment level", run.containment.label().into());
        put("Working width class", run.working_width.label().into());
        put("Impact severity level", run.impact_severity.label().into());
        put("Length along barrier", format!("{} m", fixed(resolved.length_m, 3, 3)));
        put("Centreline length", format!("{} m", fixed(resolved.centreline_length_m, 3, 3)));
        put(
            "Posts",
            match run.post_spacing_m {
                Some(s) => format!("{} at {} m", resolved.posts.len(), fixed(s, 0, 3)),
                None => "none".into(),
            },
        );
        put("Beam height", format!("{} m", fixed(run.beam_height_m, 0, 3)));
        put("Start end", run.start_end.label());
        put("End end", run.end_end.label());
        put("Levels", if resolved.levels_from_profile { "alignment profile (centreline)".into() } else { "none: z = 0".into() });
        put("plan_origin_x_m", format!("{:.6}", plan.origin_x_m));
        put("plan_origin_y_m", format!("{:.6}", plan.origin_y_m));
        let meshes = solids(run, resolved, solids_origin(resolved, &plan));
        Ok(Generated { drawing: plan.drawing, notes, derived, solids: meshes })
    }
}

/// Writes a resolved run into an element: data, size and site placement.
fn store(e: &mut Element, b: &BarrierElement) {
    let plan = plan_drawing(&b.run, &b.resolved);
    let origin = solids_origin(&b.resolved, &plan);
    e.data = b.to_value();
    e.width = plan.drawing.extents.width().max(1.0);
    e.height = plan.drawing.extents.height().max(1.0);
    e.site = Some(SitePlacement {
        x: origin[0] * 1000.0,
        y: origin[1] * 1000.0,
        z: origin[2] * 1000.0,
        rotation_deg: 0.0,
        alignment: Some(b.run.alignment.clone()),
        station_m: Some(b.run.start_station_m),
        offset_m: Some(b.run.offset_m),
    });
}

/// Adds a barrier run on `alignment` and returns its id.
///
/// * `run.alignment` is set to the alignment's name, and `run.name` is made unique in the
///   document (an empty name becomes `VRS-001`).
/// * The run is resolved against the alignment; nothing is added when it is invalid, off the
///   alignment or beyond a curve centre.
/// * Layer [`RESTRAINT_LAYER`], which is hidden when this call creates it.
/// * The site placement is as the module documentation describes, and records the alignment
///   name, the start station and the offset.
pub fn add_barrier_run(doc: &mut Document, alignment: &Alignment, mut run: BarrierRun) -> Result<ElementId, RestraintError> {
    run.alignment = alignment.name.clone();
    let wanted = if run.name.trim().is_empty() { "VRS-001" } else { run.name.trim() };
    run.name = doc.unique_name(wanted);
    let b = BarrierElement::new(run, alignment)?;
    let new_layer = !doc.layers().iter().any(|l| l.name == RESTRAINT_LAYER);
    let id = doc.add(CATEGORY, &b.run.name, Pt::new(0.0, 0.0), 1.0, 1.0, RESTRAINT_LAYER, serde_json::Value::Null);
    store(doc.find_mut(id).expect("just added"), &b);
    if new_layer {
        if let Some(layer) = doc.layers_mut().iter_mut().find(|l| l.name == RESTRAINT_LAYER) {
            layer.visible = false;
        }
    }
    Ok(id)
}

/// Replaces the definition of an existing run and resolves it again. The element keeps its id
/// and name (`run.name` is set to the element's). Nothing changes on an error.
pub fn update_barrier_run(doc: &mut Document, id: ElementId, alignment: &Alignment, mut run: BarrierRun) -> Result<(), RestraintError> {
    let e = doc.find(id).ok_or(RestraintError::ElementNotFound(id.0))?;
    if e.category != CATEGORY {
        return Err(RestraintError::NotABarrierRun(e.name.clone(), format!("its category is '{}'", e.category)));
    }
    run.alignment = alignment.name.clone();
    run.name = e.name.clone();
    let b = BarrierElement::new(run, alignment)?;
    store(doc.find_mut(id).expect("found above"), &b);
    Ok(())
}

/// The run stored in an element, with its resolved geometry.
pub fn barrier_run_of(doc: &Document, id: ElementId) -> Result<BarrierElement, RestraintError> {
    let e = doc.find(id).ok_or(RestraintError::ElementNotFound(id.0))?;
    if e.category != CATEGORY {
        return Err(RestraintError::NotABarrierRun(e.name.clone(), format!("its category is '{}'", e.category)));
    }
    serde_json::from_value(e.data.clone()).map_err(|err| RestraintError::NotABarrierRun(e.name.clone(), err.to_string()))
}

/// The barrier runs in the document: element id and mark.
pub fn barrier_runs(doc: &Document) -> Vec<(ElementId, String)> {
    doc.elements().iter().filter(|e| e.category == CATEGORY).map(|e| (e.id, e.name.clone())).collect()
}

/// The alignment of that name among the document's alignment elements.
pub fn alignment_named(doc: &Document, name: &str) -> Option<Alignment> {
    doc.elements()
        .iter()
        .filter(|e| e.category == afrobim_alignment::CATEGORY)
        .filter_map(|e| serde_json::from_value::<Alignment>(e.data.clone()).ok())
        .find(|a| a.name == name)
}

/// Resolves every barrier run again from the alignment it names, as found in the document.
/// Returns one entry per run: `Ok` when it was refreshed, or why it was left as it was (its
/// alignment is gone, the run no longer fits on it, its data cannot be read).
pub fn sync_barrier_runs(doc: &mut Document) -> Vec<(ElementId, Result<(), RestraintError>)> {
    barrier_runs(doc)
        .into_iter()
        .map(|(id, _)| {
            let result = barrier_run_of(doc, id).and_then(|b| {
                let alignment = alignment_named(doc, &b.run.alignment).ok_or_else(|| RestraintError::AlignmentNotFound(b.run.alignment.clone()))?;
                update_barrier_run(doc, id, &alignment, b.run)
            });
            (id, result)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::sample_run;
    use crate::EndTreatment;
    use afrobim_alignment::{AlignmentBuilder, Profile, Pvi};
    use afrobim_kernel::ModuleRegistry;

    /// 1000 m heading north from (500 000, 30 000), station 2+000 at the start.
    fn alignment() -> Alignment {
        AlignmentBuilder::new("A1", 2000.0, 500_000.0, 30_000.0, std::f64::consts::FRAC_PI_2).line(1000.0).build().unwrap()
    }

    fn run() -> BarrierRun {
        let mut r = sample_run();
        r.start_station_m = 2100.0;
        r.end_station_m = 2200.0;
        r
    }

    #[test]
    fn adding_a_run_stores_a_placed_element_on_a_hidden_layer() {
        let mut doc = Document::default();
        let id = add_barrier_run(&mut doc, &alignment(), run()).unwrap();
        let e = doc.find(id).unwrap();
        assert_eq!((e.category.as_str(), e.name.as_str(), e.layer.as_str()), (CATEGORY, "VRS-001", RESTRAINT_LAYER));
        assert!(!doc.layer_visible(RESTRAINT_LAYER));
        // Heading north, 5 m right is east: the line is x = 500 005, y = 30 100 .. 30 200.
        // Ticks go right of it by 0.3 m and the terminal bars 0.3 m each side.
        let site = e.site.as_ref().unwrap();
        assert!((site.x - 500_004_700.0).abs() < 1e-3 && (site.y - 30_100_000.0).abs() < 1e-3 && site.z == 0.0);
        assert_eq!((site.alignment.as_deref(), site.station_m, site.offset_m), (Some("A1"), Some(2100.0), Some(5.0)));
        assert!((e.width - 600.0).abs() < 1e-3 && (e.height - 100_000.0).abs() < 1e-3);
        // A second run of the same mark gets the next one; the layer is not hidden again.
        doc.layers_mut().iter_mut().find(|l| l.name == RESTRAINT_LAYER).unwrap().visible = true;
        let id2 = add_barrier_run(&mut doc, &alignment(), run()).unwrap();
        assert_eq!(doc.find(id2).unwrap().name, "VRS-002");
        assert_eq!(barrier_run_of(&doc, id2).unwrap().run.name, "VRS-002");
        assert!(doc.layer_visible(RESTRAINT_LAYER));
        assert_eq!(barrier_runs(&doc).len(), 2);
        // An invalid run adds nothing.
        let mut bad = run();
        bad.end_station_m = 5000.0;
        assert!(add_barrier_run(&mut doc, &alignment(), bad).is_err());
        assert_eq!(doc.elements().len(), 2);
    }

    #[test]
    fn the_module_generates_plan_solids_and_derived_values() {
        let mut doc = Document::default();
        let id = add_barrier_run(&mut doc, &alignment(), run()).unwrap();
        let mut modules = ModuleRegistry::default();
        modules.register(Box::new(RestraintModule));
        let g = modules.generate(doc.find(id).unwrap()).unwrap();
        assert_eq!(g.derived["Alignment"], "A1");
        assert_eq!(g.derived["Start station"], "2+100.000");
        assert_eq!(g.derived["End station"], "2+200.000");
        assert_eq!(g.derived["Offset"], "5 m");
        assert_eq!(g.derived["Containment level"], "N2");
        assert_eq!(g.derived["Working width class"], "W4");
        assert_eq!(g.derived["Impact severity level"], "A");
        assert_eq!(g.derived["Length along barrier"], "100.000 m");
        assert_eq!(g.derived["Posts"], "51 at 2 m");
        assert_eq!(g.derived["Beam height"], "0.6 m");
        assert_eq!(g.derived["Start end"], "terminal T110");
        assert_eq!(g.derived["Levels"], "none: z = 0");
        assert_eq!(g.derived["plan_origin_x_m"], "500004.700000");
        // 1 line + 51 ticks + 2 terminal bars; posts mesh and beam mesh.
        assert_eq!(g.drawing.primitives.len(), 54);
        assert_eq!(g.solids.len(), 2);
        assert!((g.solids[1].volume() - 100_000.0 * 50.0 * 300.0).abs() < 1.0);
        let codes: Vec<&str> = g.notes.iter().map(|n| n.code.as_str()).collect();
        assert_eq!(codes, [NOTE_SCHEMATIC, NOTE_LEVELS_ZERO]);
        assert!(g.notes.iter().all(|n| n.severity == Severity::Info));
    }

    #[test]
    fn invalid_and_unreadable_elements() {
        let mut doc = Document::default();
        let id = add_barrier_run(&mut doc, &alignment(), run()).unwrap();
        let mut b = barrier_run_of(&doc, id).unwrap();
        b.run.beam_height_m = -1.0;
        b.run.start_end = EndTreatment::Unspecified;
        doc.find_mut(id).unwrap().data = b.to_value();
        let g = RestraintModule.generate(doc.find(id).unwrap()).unwrap();
        assert!(g.drawing.primitives.is_empty() && g.solids.is_empty() && g.derived.is_empty());
        let codes: Vec<&str> = g.notes.iter().map(|n| n.code.as_str()).collect();
        assert_eq!(codes, ["RESTRAINT_BEAM_HEIGHT_NOT_POSITIVE", "RESTRAINT_END_UNSPECIFIED"]);

        doc.find_mut(id).unwrap().data = serde_json::json!("nothing");
        assert!(RestraintModule.generate(doc.find(id).unwrap()).unwrap_err().contains("VRS-001"));
        assert!(matches!(barrier_run_of(&doc, id), Err(RestraintError::NotABarrierRun(..))));
        assert!(matches!(barrier_run_of(&doc, ElementId(999)), Err(RestraintError::ElementNotFound(999))));
    }

    #[test]
    fn the_document_round_trips_through_json() {
        let mut doc = Document::default();
        let id = add_barrier_run(&mut doc, &alignment(), run()).unwrap();
        let before = barrier_run_of(&doc, id).unwrap();
        let loaded = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(barrier_run_of(&loaded, id).unwrap(), before);
        assert_eq!(loaded.find(id).unwrap().site, doc.find(id).unwrap().site);
        let g = RestraintModule.generate(loaded.find(id).unwrap()).unwrap();
        assert_eq!(g.derived["Length along barrier"], "100.000 m");
    }

    #[test]
    fn syncing_picks_up_a_changed_alignment() {
        let mut doc = Document::default();
        let a = alignment();
        let alignment_id = doc.add(afrobim_alignment::CATEGORY, "A1", Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&a).unwrap());
        let id = add_barrier_run(&mut doc, &a, run()).unwrap();
        let mut other = run();
        other.name = "VRS-050".into();
        let orphan = add_barrier_run(&mut doc, &AlignmentBuilder::new("Gone", 0.0, 0.0, 0.0, 0.0).line(5000.0).build().unwrap(), other).unwrap();
        assert_eq!(alignment_named(&doc, "A1").unwrap(), a);

        // The alignment gains a profile: level 1200 m at 2+000 rising at 1 %.
        let mut changed = a.clone();
        changed.profile = Some(Profile::new("P", vec![Pvi::point(2000.0, 1200.0), Pvi::point(3000.0, 1210.0)]).unwrap());
        doc.find_mut(alignment_id).unwrap().data = serde_json::to_value(&changed).unwrap();
        let results = sync_barrier_runs(&mut doc);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], (id, Ok(())));
        assert_eq!(results[1], (orphan, Err(RestraintError::AlignmentNotFound("Gone".into()))));
        let b = barrier_run_of(&doc, id).unwrap();
        assert!(b.resolved.levels_from_profile);
        assert!((b.resolved.line[0].z - 1201.0).abs() < 1e-9);
        // The site level is the lowest level of the run, and the solids start from it.
        assert!((doc.find(id).unwrap().site.as_ref().unwrap().z - 1_201_000.0).abs() < 1e-6);
        let g = RestraintModule.generate(doc.find(id).unwrap()).unwrap();
        assert_eq!(g.derived["Levels"], "alignment profile (centreline)");
        let top = g.solids[1].vertices.iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
        assert!((top - (1000.0 + 750.0)).abs() < 1e-6); // 1 m rise over the run + top of beam

        // Updating a run keeps its id and mark.
        let mut longer = b.run.clone();
        longer.name = "ignored".into();
        longer.end_station_m = 2300.0;
        update_barrier_run(&mut doc, id, &changed, longer).unwrap();
        let b = barrier_run_of(&doc, id).unwrap();
        assert_eq!((b.run.name.as_str(), b.resolved.length_m), ("VRS-001", 200.0));
        assert!(update_barrier_run(&mut doc, alignment_id, &changed, run()).is_err());
    }
}
