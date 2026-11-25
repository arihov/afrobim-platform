// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Tests of the round 6 work package A3, second part: the derived rows of the Properties panel for
//! the round 6 categories, and that the targets a corridor declares block a corridor until bound.

use crate::app::AfroBimApp;
use crate::civil;

fn demo_app() -> (egui::Context, AfroBimApp) {
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let options = crate::Options { screenshot: Some(std::env::temp_dir().join("afrobim-a3b-test.png")), civil_demo: true, ..crate::Options::default() };
    let mut app = AfroBimApp::new(&cc, options);
    app.civil_build_now();
    (ctx, app)
}

fn rows_of(app: &AfroBimApp, category: &str) -> Vec<Vec<String>> {
    app.doc.elements().iter().filter(|e| e.category == category).filter_map(|e| app.civil_derived(e.id)).map(|(d, _)| d.keys().cloned().collect()).collect()
}

#[test]
fn the_round_6_categories_show_their_derived_rows() {
    let (_c, app) = demo_app();
    let has = |category: &str, wanted: &[&str]| {
        let all = rows_of(&app, category);
        assert!(!all.is_empty(), "the demo has no {category}");
        for keys in &all {
            for w in wanted {
                assert!(keys.iter().any(|k| k == w), "{category}: no row '{w}' in {keys:?}");
            }
        }
    };
    has(civil::FEATURELINE, &["Length (m)", "Lowest level (m)", "Highest level (m)", "Steepest grade (%)"]);
    has(civil::GRADING, &["Footprint length (m)", "Surface area (m2)", "Triangles"]);
    has(civil::INTERSECTION, &["Kind", "Kerb returns"]);
    has(civil::ROUNDABOUT, &["Kind", "Inscribed diameter (m)", "Circulatory width (m)"]);
    has(civil::POINTCLOUD, &["Points", "Class meanings", "Coordinate system"]);
}

#[test]
fn an_unbound_declared_target_is_named_and_a_bound_one_is_not() {
    use afrobim_corridor::{library, Assembly, ElevationTarget, OffsetTarget};
    let mut a = Assembly::new("a");
    a.right.push(library::link_to_offset_and_elevation("Tie", &["Top"], "ETW", OffsetTarget::Polyline(vec![]), ElevationTarget::Table(vec![])));
    let d = crate::civil_targets::declared(&a);
    assert_eq!(d.len(), 2);
    assert!(d.iter().all(|x| crate::civil_targets::declared_status(&mut a, x).is_some()));
    *crate::civil_targets::offset_mut(&mut a, &d[0]).unwrap() = OffsetTarget::Fixed(8.0);
    *crate::civil_targets::elevation_mut(&mut a, &d[1]).unwrap() = ElevationTarget::Fixed(101.0);
    assert!(d.iter().all(|x| crate::civil_targets::declared_status(&mut a, x).is_none()));
}
