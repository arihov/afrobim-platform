// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Glue tests: the alignment check against the built-in MoWT basis, the overlay and old stored
//! selections. Every number is an arbitrary test value.

use afrobim_design::{CheckStatus, VPvi, VerticalDesign};
use afrobim_design::mowt::{RoadClass, Terrain};
use afrobim_kernel::{Document, ElementId};

use crate::design_glue::{self as dg, AlignCheckInputs, AlignmentCheck, BuiltinChoice, LimitSource};

fn doc_with_design() -> (Document, ElementId, ElementId) {
    let mut doc = Document::default();
    let (d, a) = dg::create(&mut doc, "Test road", 1000.0, &[(0.0, 0.0), (400.0, 0.0), (400.0, 300.0)]).unwrap();
    dg::edit_horizontal(&mut doc, d, dg::HEdit::SetRadius(1, 100.0)).unwrap();
    let v = VerticalDesign::new("Test road", vec![VPvi::point(1000.0, 100.0), VPvi::with_length(1300.0, 104.0, 100.0), VPvi::point(1650.0, 101.0)]);
    dg::set_vertical(&mut doc, d, Some(v)).unwrap();
    (doc, d, a)
}

fn choice(class: RoadClass, terrain: Terrain, speed: f64) -> BuiltinChoice {
    BuiltinChoice { class: Some(class), terrain: Some(terrain), speed_kmh: Some(speed), emax_percent: Some(7.0), ..Default::default() }
}

fn report_of(doc: &Document, a: ElementId) -> afrobim_design::CheckReport {
    match dg::check_for_report(doc, a) {
        Some(AlignmentCheck::Report(r)) => r,
        _ => panic!("expected a report"),
    }
}

#[test]
fn the_builtin_mowt_basis_gives_statuses_with_table_citations_and_a_withheld_cell_is_not_checked() {
    let (mut doc, _, a) = doc_with_design();
    // Nothing chosen yet: input needed, then an incomplete selection says what is missing.
    assert!(matches!(dg::check_for_report(&doc, a), Some(AlignmentCheck::InputNeeded(_))));
    let mut inputs = AlignCheckInputs { source: LimitSource::Builtin, ..Default::default() };
    dg::store_check(&mut doc, a, &inputs);
    match dg::check_for_report(&doc, a) {
        Some(AlignmentCheck::InputNeeded(why)) => assert!(why.contains("road class"), "{why}"),
        _ => panic!("expected input needed"),
    }
    // Class II, rolling, 60 km/h: a radius of 100 m is below the table's minimum, so it fails, citing the table.
    inputs.builtin = choice(RoadClass::II, Terrain::Rolling, 60.0);
    dg::store_check(&mut doc, a, &inputs);
    let r = report_of(&doc, a);
    let radius = r.items.iter().find(|i| i.id == "H.radius").unwrap();
    assert_eq!(radius.status, CheckStatus::Fail);
    assert!(radius.limit_basis.contains("Table 6-1"), "{}", radius.limit_basis);
    let grade = r.items.iter().find(|i| i.id == "V.max_grade").unwrap();
    assert!(grade.limit_basis.contains("Table"), "{}", grade.limit_basis);
    // A check the manual does not cover stays not checked, never passed.
    assert_eq!(r.items.iter().find(|i| i.id == "H.curve_length").unwrap().status, CheckStatus::NotChecked);
    // Class II mountainous: the gradient cells are withheld (slip S04), so the grade is not checked.
    inputs.builtin = choice(RoadClass::II, Terrain::Mountainous, 60.0);
    dg::store_check(&mut doc, a, &inputs);
    let r = report_of(&doc, a);
    assert_eq!(r.items.iter().find(|i| i.id == "V.max_grade").unwrap().status, CheckStatus::NotChecked);
    // The selection survives save and open.
    let reopened = Document::from_json(&doc.to_json()).unwrap();
    assert_eq!(dg::stored_check(&reopened, a), Some(inputs));
}

#[test]
fn a_user_overlay_changes_one_limit_and_an_old_stored_selection_still_loads() {
    let (mut doc, _, a) = doc_with_design();
    let dir = std::env::temp_dir().join(format!("afrobim-overlay-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("over.csv");
    // The user's table states only a minimum radius (a test value) and its source.
    std::fs::write(&path, "#name,Overlay\n#source,test fixture, not a standard\nlabel,min_radius_m\nmine,50\n").unwrap();
    let mut inputs = AlignCheckInputs { source: LimitSource::BuiltinWithOverlay, builtin: choice(RoadClass::II, Terrain::Rolling, 60.0), limits_path: path.display().to_string(), row_label: "mine".into() };
    dg::store_check(&mut doc, a, &inputs);
    let r = report_of(&doc, a);
    // The user's 50 m replaces the manual's, so a radius of 100 m now passes; other limits stay the manual's.
    assert_eq!(r.items.iter().find(|i| i.id == "H.radius").unwrap().status, CheckStatus::Pass);
    assert!(r.source.contains("test fixture"), "{}", r.source);
    assert!(r.items.iter().find(|i| i.id == "V.max_grade").unwrap().limit_basis.contains("Table"));
    // Built-in alone still fails the same radius.
    inputs.source = LimitSource::Builtin;
    dg::store_check(&mut doc, a, &inputs);
    assert_eq!(report_of(&doc, a).items.iter().find(|i| i.id == "H.radius").unwrap().status, CheckStatus::Fail);
    std::fs::remove_dir_all(&dir).ok();
    // A selection stored before the source existed (path and row only) reads as the user's file.
    let old: AlignCheckInputs = serde_json::from_str(r#"{"limits_path":"x.csv","row_label":"row A"}"#).unwrap();
    assert_eq!(old.source, LimitSource::UserFile);
}
