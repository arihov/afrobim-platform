// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Fit to ground: a starting-point helper, tested on ground whose answer is known.

use afrobim_corridor::Tin;
use afrobim_design::*;

fn inputs() -> FitInputs {
    // Fixture entries, not standard values.
    FitInputs { sample_spacing_m: 10.0, tolerance_m: 0.3, min_segment_length_m: 100.0, max_grade: 0.06, min_curve_length_m: 60.0, k_crest: None, k_sag: None }
}

fn straight(len: f64) -> afrobim_alignment::Alignment {
    HorizontalDesign::new("G", 0.0, vec![Pi::point(0.0, 0.0), Pi::point(len, 0.0)]).solve().unwrap().alignment
}

#[test]
fn uniform_slope_gives_one_grade_and_no_curve() {
    // Ground z = 100 + 0.02 x: one least squares line passes through every sample.
    let tin = Tin::from_grid(-50.0, -50.0, 50.0, 50.0, 22, 3, |x, _| 100.0 + 0.02 * x).unwrap();
    let r = fit_to_ground(&straight(1000.0), &tin, &inputs()).unwrap();
    assert_eq!(r.design.pvis.len(), 2);
    assert!((r.design.pvis[0].elevation_m - 100.0).abs() < 1e-6);
    assert!((r.design.pvis[1].elevation_m - 120.0).abs() < 1e-6);
    assert!(r.max_above_ground_m < 1e-6 && r.max_below_ground_m < 1e-6 && r.rms_difference_m < 1e-6);
    assert!(r.label.starts_with("STARTING POINT ONLY"));
}

#[test]
fn a_ridge_gives_a_pvi_near_the_ridge_and_a_curve() {
    // Ground rises 3 % to x = 500 (z = 115), then falls 2 %.
    let tin = Tin::from_grid(-50.0, -50.0, 10.0, 50.0, 111, 3, |x, _| if x < 500.0 { 100.0 + 0.03 * x } else { 115.0 - 0.02 * (x - 500.0) }).unwrap();
    let r = fit_to_ground(&straight(1000.0), &tin, &inputs()).unwrap();
    assert_eq!(r.design.pvis.len(), 3, "{:?}", r.design.pvis);
    assert!((r.design.pvis[1].station_m - 500.0).abs() <= 30.0, "{}", r.design.pvis[1].station_m);
    // The curve is at least the entered minimum, and the result solves.
    match r.design.pvis[1].curve {
        CurveSpec::Length { length_m } => assert!(length_m >= 60.0 - 1e-9),
        other => panic!("{other:?}"),
    }
    let s = r.design.solve().unwrap();
    assert!(s.max_abs_grade() <= 0.06 + 1e-9);
    assert!(r.max_above_ground_m < 3.0 && r.max_below_ground_m < 3.0);
}

#[test]
fn the_entered_maximum_grade_is_enforced_and_listed() {
    // A steady 8 % climb against a 6 % limit: every PVI moves, and the notes say so.
    let tin = Tin::from_grid(-50.0, -50.0, 50.0, 50.0, 22, 3, |x, _| 100.0 + 0.08 * x).unwrap();
    let r = fit_to_ground(&straight(1000.0), &tin, &inputs()).unwrap();
    let s = r.design.solve().unwrap();
    assert!(s.max_abs_grade() <= 0.06 + 1e-9);
    assert!(r.notes.iter().any(|n| n.contains("above the entered maximum")));
    assert!(r.max_below_ground_m > 10.0, "the profile falls well below the ground: it needs deep cut");
}

#[test]
fn a_centreline_off_the_surface_is_reported() {
    let tin = Tin::from_grid(-50.0, -50.0, 50.0, 50.0, 12, 3, |x, _| 100.0 + 0.01 * x).unwrap();
    // The alignment is 1000 m long but the surface only reaches x = 500.
    let r = fit_to_ground(&straight(1000.0), &tin, &inputs()).unwrap();
    assert_eq!(r.gaps.len(), 1);
    assert!(r.notes.iter().any(|n| n.contains("off the ground surface")));
    // The profile still spans the alignment (extended along its last grade).
    assert!((r.design.pvis.last().unwrap().station_m - 1000.0).abs() < 1e-6);
    // Bad inputs are refused.
    let mut bad = inputs();
    bad.max_grade = 0.0;
    assert!(fit_to_ground(&straight(1000.0), &tin, &bad).is_err());
}
