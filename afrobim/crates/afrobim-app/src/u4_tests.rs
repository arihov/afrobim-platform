// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Tests of the U4 work: what the 3D view receives from the civil objects, and the dependency
//! refresh of the dependent views.

use std::collections::HashMap;

use afrobim_kernel::{Document, ElementId};

use crate::civil::*;
use crate::civil_cache::{build_all, dependency_hash};
use crate::civil_demo::build;

fn demo_doc() -> Document {
    let mut doc = Document::default();
    let notes = build(&mut doc);
    assert!(notes.is_empty(), "{notes:?}");
    doc
}

fn scenes(doc: &Document) -> HashMap<ElementId, crate::civil_build::Built> {
    let elements = doc.elements().iter().filter(|e| is_civil(&e.category) || e.category == afrobim_alignment::CATEGORY).cloned().collect();
    let stale: Vec<ElementId> = doc.elements().iter().filter(|e| is_civil(&e.category)).map(|e| e.id).collect();
    build_all(elements, HashMap::new(), stale, dependency_hash(doc)).into_iter().map(|(id, _, b)| (id, b)).collect()
}

#[test]
fn the_corridor_gives_solids_and_linework_for_the_3d_view() {
    let doc = demo_doc();
    let built = scenes(&doc);
    let cor = doc.elements().iter().find(|e| e.category == CORRIDOR).unwrap();
    let sc = &built[&cor.id].scene;
    let names: Vec<&str> = sc.meshes.iter().map(|m| m.name.as_str()).collect();
    eprintln!("corridor meshes: {:?} lines3 {}", sc.meshes.iter().map(|m| (m.name.clone(), m.triangles.len())).collect::<Vec<_>>(), sc.lines3.len());
    assert!(sc.meshes.iter().any(|m| !m.triangles.is_empty()), "{names:?}");
    assert!(!sc.lines3.is_empty());
}

#[test]
fn parcels_and_surfaces_have_3d_outlines() {
    let doc = demo_doc();
    let built = scenes(&doc);
    for cat in [PARCEL, SURFACE] {
        for e in doc.elements().iter().filter(|e| e.category == cat) {
            assert!(!built[&e.id].scene.lines3.is_empty(), "{} has no 3D linework", e.name);
        }
    }
}

#[test]
fn an_edited_alignment_or_imported_surface_changes_what_the_dependent_views_read() {
    let mut doc = demo_doc();
    let before = dependency_hash(&doc);
    assert_eq!(before, dependency_hash(&doc), "the hash is stable");
    // A new alignment, as an edit of the alignments would make.
    let al = crate::civil_demo::alignment();
    let id = crate::alignment_glue::alignments(&doc).first().map(|a| a.0).unwrap();
    let mut other = al.clone();
    other.profile = None;
    if let Some(e) = doc.find_mut(id) {
        e.data = serde_json::to_value(&other).unwrap();
    }
    assert_ne!(before, dependency_hash(&doc), "an alignment edit is seen");
}

#[test]
fn a_changed_imported_surface_is_seen_by_the_digest() {
    use crate::civil_cache::dependency_hash;
    let mut doc = Document::default();
    let tin = afrobim_corridor::Tin::new(vec![[0.0, 0.0, 1.0], [10.0, 0.0, 2.0], [0.0, 10.0, 3.0]], vec![[0, 1, 2]]).unwrap();
    let id = crate::alignment_glue::add_surface_at_source(&mut doc, "Imported", &tin, Some("test")).unwrap();
    let a = dependency_hash(&doc);
    let tin2 = afrobim_corridor::Tin::new(vec![[0.0, 0.0, 1.0], [10.0, 0.0, 2.0], [0.0, 10.0, 9.0]], vec![[0, 1, 2]]).unwrap();
    let def = afrobim_corridor::SurfaceDefinition { tin: tin2, ..serde_json::from_value(doc.find(id).unwrap().data.clone()).unwrap() };
    doc.find_mut(id).unwrap().data = serde_json::to_value(&def).unwrap();
    assert_ne!(a, dependency_hash(&doc));
}

#[test]
fn the_section_sheets_draw_the_views_of_a_sample_line_group() {
    use afrobim_sheets::sheetset::{SectionSeries, SheetSet, SheetSource, SheetSpec};
    use afrobim_sheets::{Page, Scale, SectionsView, ViewDef, ViewKind};
    let doc = demo_doc();
    let group = doc.elements().iter().find(|e| e.category == SAMPLES).unwrap().name.clone();
    let mut view = SectionsView::of_sample_group(&group, Scale(200.0), 2);
    view.vertical_exaggeration = 2.0;
    let n = crate::civil_sheets::feed_sample_sections(&mut view, &doc).expect("the group is cut");
    assert!(n > 0, "no section views");
    assert_eq!(view.fed.len(), view.fed_stations_m.len());
    assert!(view.fed.iter().all(|d| !d.drawing.primitives.is_empty()));
    // As a series on sheets: the views are shared out and drawn.
    let spec = SheetSpec::new("SECTIONS", Page::default(), vec![ViewDef::full(ViewKind::Sections(view))]);
    let mut set = SheetSet::new("test");
    set.sources.push(SheetSource::SectionSeries(SectionSeries { template: spec, sections_view: 0, per_sheet: 4 }));
    let registry = crate::background::worker_registry();
    let model = afrobim_sheets::Model { doc: &doc, registry: &registry };
    let info = afrobim_sheets::ProjectInfo::default();
    let composed = crate::sheets_glue::compose_with_annotations(&model, &set, &info);
    assert!(!composed.sheets.is_empty());
    let texts: usize = composed.sheets.iter().map(|s| s.items.len()).sum();
    assert!(texts > 10, "the sheets are empty");
}

#[test]
fn a_missing_sample_line_group_is_reported_and_draws_nothing() {
    use afrobim_sheets::{Scale, SectionsView};
    let doc = demo_doc();
    let mut view = SectionsView::of_sample_group("No such group", Scale(200.0), 2);
    let e = crate::civil_sheets::feed_sample_sections(&mut view, &doc).unwrap_err();
    assert!(e.contains("No such group"), "{e}");
}

#[test]
fn the_assembly_editor_describes_every_kind_the_library_has() {
    use crate::civil_assembly::{describe, EdPart, KINDS};
    use afrobim_corridor::{library, Bench, Ditch, DitchShape, ElevationTarget, OffsetTarget, Slope};
    let s = Slope::Percent(-2.0);
    let parts = vec![
        library::lane("a", 3.5, s, vec![]),
        library::lane_superelevated("b", 3.5, s, vec![]),
        library::lane_to_offset("c", OffsetTarget::Fixed(5.0), s, vec![]),
        library::shoulder("d", 1.0, s, vec![]),
        library::shoulder_superelevated("e", 1.0, s, vec![]),
        library::footway("f", 1.5, s, vec![]),
        library::verge("g", 1.0, s),
        library::kerb("h", 0.15, 0.15, 0.1),
        library::kerb_and_channel("i", 0.3, s, 0.15, 0.15, 0.1),
        library::median_raised("j", 0.15, 1.0, 0.1),
        library::median_depressed("k", 1.0, 0.5, 4.0),
        library::side_ditch_v("l", 0.6, 4.0, 3.0),
        library::side_ditch_trapezoid("m", 0.6, 0.5, 4.0, 3.0),
        library::lined_drain("n", Ditch { shape: DitchShape::V, depth_m: 0.6, inner_run_per_rise: 4.0, outer_run_per_rise: 3.0 }, 0.1),
        library::retaining_edge("o", 0.3, 1.0),
        library::link_width_slope("p", &["Top"], "P", 2.0, s),
        library::link_to_elevation("q", &["Top"], "Q", ElevationTarget::Fixed(1.0), s),
        library::link_to_offset("r", &["Top"], "R", OffsetTarget::Fixed(4.0), s),
        library::daylight(2.0, 2.0),
        library::daylight_benched(2.0, 2.0, Some(Bench { step_height_m: 5.0, width_m: 2.0, slope: s }), None),
    ];
    for p in &parts {
        let rows = describe(p);
        assert!(!rows.is_empty(), "{} has no parameters shown", p.name);
        assert!(rows.iter().all(|(a, b)| !a.is_empty() && !b.is_empty()));
    }
    // Every part the editor can make builds from its example values and describes itself.
    for (i, k) in KINDS.iter().enumerate() {
        let mut p = EdPart::new(i);
        p.example();
        let built = p.build().unwrap_or_else(|e| panic!("{}: {e}", k.name));
        assert!(!describe(&built).is_empty(), "{}", k.name);
    }
    let mut lane = EdPart::new(0);
    lane.example();
    lane.superelevated = true;
    let built = lane.build().unwrap();
    assert!(describe(&built).iter().any(|(a, _)| a == "Cross-fall"));
}
