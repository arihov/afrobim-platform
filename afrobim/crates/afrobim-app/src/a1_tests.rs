// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Tests of the round 6 work package A1: the defaults in the interface, pipe networks, grading and
//! intersections as document elements.

use afrobim_defaults::Conditions;

use crate::defaults_glue as dg;

#[test]
fn the_defaults_basis_gives_a_cited_value_or_nothing() {
    dg::set_basis("dmrb");
    let v = dg::get("link.ssd.desirable_min", &Conditions::none().design_speed(100.0)).expect("a value at 100 km/h");
    assert_eq!(v.number(), Some(215.0));
    assert!(dg::citation_line(&v).contains("CD 109"), "{}", dg::citation_line(&v));
    // A speed the table does not list has no default: nothing is borrowed from a neighbour.
    assert!(dg::get("link.ssd.desirable_min", &Conditions::none().design_speed(95.0)).is_none());
    // A key the pack does not hold has no default.
    assert!(dg::get("drainage.pipe.min_cover", &Conditions::none()).is_none());
    // With no basis chosen every field is an empty input.
    dg::set_basis(dg::NO_BASIS);
    assert!(dg::get("link.ssd.desirable_min", &Conditions::none().design_speed(100.0)).is_none());
    assert_eq!(dg::basis_id(), "none");
    dg::set_basis("dmrb");
    assert_eq!(dg::basis_id(), "dmrb");
}

#[test]
fn the_basis_is_kept_in_the_project_settings_and_read_back() {
    let mut doc = afrobim_kernel::Document::default();
    dg::store_basis(&mut doc, dg::NO_BASIS);
    let back = afrobim_kernel::Document::from_json(&doc.to_json()).unwrap();
    dg::set_basis("dmrb");
    dg::sync(&back, true);
    assert_eq!(dg::basis_id(), "none");
    dg::store_basis(&mut doc, "dmrb");
    dg::sync(&doc, true);
    assert_eq!(dg::basis_id(), "dmrb");
}

#[test]
fn road_type_choices_come_from_the_pack() {
    dg::set_basis("dmrb");
    let mut c = dg::RoadChoice { group: 3, road: "S2".into() };
    assert_eq!(c.prefix(), Some("xs.rural_all_purpose"));
    let lane = dg::number("xs.rural_all_purpose.lane_width", &Conditions::none().label("road_type/lane", "S2/L1"));
    assert_eq!(lane, Some(3.65));
    // The urban all-purpose cross-section has no verge width in the pack ("Varies" in the figure).
    c.group = 4;
    c.road = "SU2".into();
    assert!(dg::number(&c.key("verge_width").unwrap(), &c.cond()).is_none());
    assert!(dg::labels_of("xs.rural_all_purpose.carriageway_width").contains(&"WS2".to_string()));
}

pub(crate) fn demo_doc() -> afrobim_kernel::Document {
    let mut doc = afrobim_kernel::Document::default();
    let notes = crate::civil_demo::build(&mut doc);
    assert!(notes.is_empty(), "{notes:?}");
    doc
}

pub(crate) fn ctx_with_tins(doc: &afrobim_kernel::Document) -> crate::civil_build::Ctx {
    let base = crate::civil_build::Ctx { elements: doc.elements().to_vec(), tins: Default::default() };
    let mut tins = std::collections::HashMap::new();
    for e in doc.elements().iter().filter(|e| e.category == crate::civil::SURFACE) {
        if let Some(t) = crate::civil_build::build(e, &base).tin {
            tins.insert(e.id.0, t);
        }
    }
    crate::civil_build::Ctx { elements: doc.elements().to_vec(), tins }
}


/// Every command A1 added opens a window of its own on the civil demonstration, in both looks, and
/// draws something (the dialog kit's own windows are checked for every command by `u4_dialogs_test`).
#[test]
fn every_a1_command_opens_its_dialog_on_the_civil_demonstration() {
    use crate::civil_cmds::Cv;
    use crate::commands::{Action, COMMANDS};
    use crate::u4_dialogs_test::{app_for, frame, windows};
    let new: [Cv; 17] = [Cv::Defaults, Cv::PipeNew, Cv::PipeParts, Cv::PipeProps, Cv::PipeEdit, Cv::PipeCheck, Cv::PipeTables, Cv::PipeQty, Cv::PipeProfile, Cv::FeatureLine, Cv::FeatureLineEdit, Cv::GradeSite, Cv::GradeCreate, Cv::GradeGroup, Cv::GradeHelpers, Cv::Intersection, Cv::Roundabout];
    let mut problems = Vec::new();
    for theme in ["dark-glass", "light-glass"] {
        for cv in new {
            let info = COMMANDS.iter().find(|c| c.action == Action::Civil(cv)).unwrap_or_else(|| panic!("{cv:?} is not in the command table"));
            let (ctx, mut app) = app_for(theme, true);
            frame(&ctx, &mut app, 2);
            let before = windows(&ctx);
            app.options_mut().run = Some(info.name.to_string());
            let shapes = frame(&ctx, &mut app, 4);
            if windows(&ctx) <= before {
                problems.push(format!("{} ({theme}) opened no window", info.name));
            }
            if shapes == 0 {
                problems.push(format!("{} ({theme}) drew nothing", info.name));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The new element categories load from a document saved without them (an old document).
#[test]
fn a_document_saved_before_round_six_still_loads() {
    let mut doc = afrobim_kernel::Document::default();
    doc.add("signs", "x", afrobim_kernel::Pt::new(0.0, 0.0), 10.0, 10.0, "0", serde_json::json!({}));
    let back = afrobim_kernel::Document::from_json(&doc.to_json()).unwrap();
    assert_eq!(back.elements().len(), 1);
    let mut reg = afrobim_kernel::ModuleRegistry::default();
    crate::civil::register(&mut reg);
    for (c, _, _) in crate::civil::KINDS {
        assert!(reg.categories().any(|(cat, _)| cat == c), "{c} is registered");
    }
}
