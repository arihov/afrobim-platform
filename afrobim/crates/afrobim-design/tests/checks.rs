// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The check framework and the physics relations. Every limit used here is a **test fixture**
//! chosen to be easy to work by hand. None is taken from any standard.

use afrobim_design::physics::*;
use afrobim_design::*;

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: got {a}, expected {b} (tolerance {tol})");
}

#[test]
fn physics_relations_worked_by_hand() {
    // R = V^2 / (12.96 g (e + f)), g = 9.80665: 12.96 g = 127.0966.
    // V = 100, e = 0.08, f = 0.12: 10000 / (127.0966 x 0.20) = 10000 / 25.41932 = 393.4
    close(min_radius_m(100.0, 0.08, 0.12).unwrap(), 393.40, 0.01, "min radius");
    // Inverse: friction demanded on that radius with e = 0.08 is 0.12.
    close(side_friction_demand(100.0, min_radius_m(100.0, 0.08, 0.12).unwrap(), 0.08).unwrap(), 0.12, 1e-12, "f");
    // Stopping distance: V = 80, t = 2.5 s, f = 0.35, level.
    //   v = 22.2222 m/s; v t = 55.5556; v^2 / (2 g f) = 493.827 / 6.864655 = 71.939; total 127.495
    close(stopping_distance_m(80.0, 2.5, 0.35, 0.0).unwrap(), 127.495, 0.01, "stopping distance");
    // Downhill 5 %: f + G = 0.30, braking 493.827 / 5.88399 = 83.93, total 139.49.
    close(stopping_distance_m(80.0, 2.5, 0.35, -0.05).unwrap(), 139.49, 0.01, "downhill");
    // Crest K for S = 127.5, h1 = 1.08, h2 = 0.6: (1.03923 + 0.774597)^2 = 3.28999; K = 16256.25 / 657.998 = 24.706
    close(crest_k_for_sight(127.5, 1.08, 0.6).unwrap(), 24.706, 0.001, "crest K");
    // Sag K for S = 100, H = 0.6, 1 deg: 10000 / (200 (0.6 + 100 x 0.0174551)) = 10000 / 469.10 = 21.317
    close(sag_k_for_sight(100.0, 0.6, 1.0).unwrap(), 21.317, 0.001, "sag K");
    assert!(min_radius_m(100.0, 0.0, 0.0).is_none());
}

#[test]
fn crest_sight_line_test_agrees_with_the_closed_form() {
    // Symmetric crest, L = 400, A = 0.04: S = sqrt(2L/A) (sqrt h1 + sqrt h2) with S < L.
    //   sqrt(2 x 400 / 0.04) = 141.421; x 1.81383 = 256.52 m
    let d = VerticalDesign::new("P", vec![VPvi::point(0.0, 100.0), VPvi::with_length(1000.0, 120.0, 400.0), VPvi::point(2000.0, 100.0)]);
    let s = d.solve().unwrap();
    let c = &s.curves[0];
    close(c.grade_change, -0.04, 1e-12, "A");
    let (dist, _) = crest_available_sight_distance(&s.profile, c.pvc_station_m - 400.0, c.pvt_station_m, 1.08, 0.6, 400.0, 1.0);
    close(dist, 256.52, 2.0, "numeric sight distance");
    // And K for that S is L / (100 A) = 100 = S^2 / (200 (..)^2) = 65800 / 658 = 100.
    close(crest_k_for_sight(256.52, 1.08, 0.6).unwrap(), 100.0, 0.1, "K at that S");
}

#[test]
fn sag_headlight_test_agrees_with_the_closed_form() {
    // Sag, L = 600, A = 0.04, H = 0.6, 1 degree: A S^2 / (2L) = H + S tan(1 deg) gives S = 556.0 (< L).
    let d = VerticalDesign::new("P", vec![VPvi::point(0.0, 120.0), VPvi::with_length(1500.0, 90.0, 600.0), VPvi::point(3000.0, 120.0)]);
    let s = d.solve().unwrap();
    let c = &s.curves[0];
    close(c.grade_change, 0.04, 1e-12, "A");
    let mut worst = f64::MAX;
    let mut st = c.pvc_station_m - 800.0;
    while st <= c.pvt_station_m {
        worst = worst.min(sag_sight_distance(&s.profile, st, 0.6, 1.0, 800.0, 1.0));
        st += 2.0;
    }
    close(worst, 556.0, 3.0, "headlight distance");
}

fn row() -> StandardRow {
    // Fixture values: illustrative only.
    StandardRow {
        label: "fixture 80".into(),
        design_speed_kmh: Some(80.0),
        min_radius_m: Some(250.0),
        min_curve_length_m: Some(100.0),
        min_tangent_same_direction_m: Some(100.0),
        min_tangent_reverse_m: Some(80.0),
        max_grade: Some(0.06),
        min_grade: Some(0.005),
        min_k_crest: Some(30.0),
        min_k_sag: Some(25.0),
        min_vertical_curve_length_m: Some(60.0),
        ..StandardRow::default()
    }
}

fn basis() -> DesignBasis {
    DesignBasis { name: "Fixture".into(), source: "test fixture, not a standard".into(), notes: String::new(), rows: vec![row()] }
}

#[test]
fn horizontal_checks_pass_and_fail_with_the_stated_limit() {
    // The 3-4-5 reverse pair: R 400 and 400, straight 66.667 between reverse curves.
    let h = HorizontalDesign::new("R", 0.0, vec![Pi::point(0.0, 0.0), Pi::curve(500.0, 0.0, 400.0), Pi::curve(900.0, 300.0, 400.0), Pi::point(1300.0, 0.0)]).solve().unwrap();
    let report = run_checks(&basis(), &row(), Some(&h), None, &CheckOptions::default());
    let tangent = report.items.iter().find(|i| i.id == "H.tangent_reverse").unwrap();
    assert_eq!(tangent.status, CheckStatus::Fail);
    close(tangent.value.unwrap(), 66.6667, 1e-3, "tangent");
    assert_eq!(tangent.limit, Some(80.0));
    assert!(tangent.message.contains("at least 80.000"));
    let radius: Vec<_> = report.items.iter().filter(|i| i.id == "H.radius").collect();
    assert!(radius.iter().all(|i| i.status == CheckStatus::Pass));
    // Curve length: PI1 R 400 delta 36.87 deg = 257.4 m passes 100; PI2 passes.
    assert!(report.items.iter().filter(|i| i.id == "H.curve_length").all(|i| i.status == CheckStatus::Pass));
    // Raising the minimum radius above 400 fails both curves, and the report carries the source.
    let mut r2 = row();
    r2.min_radius_m = Some(450.0);
    let report = run_checks(&basis(), &r2, Some(&h), None, &CheckOptions::default());
    assert_eq!(report.count(CheckStatus::Fail), 3);
    assert!(report.source.contains("test fixture"));
    assert!(report.to_csv().contains("# source,\"test fixture, not a standard\""));
}

#[test]
fn a_missing_limit_is_not_checked_never_passed() {
    let h = HorizontalDesign::new("R", 0.0, vec![Pi::point(0.0, 0.0), Pi::curve(500.0, 0.0, 400.0), Pi::point(900.0, 300.0)]).solve().unwrap();
    let empty = StandardRow { label: "empty".into(), ..StandardRow::default() };
    let report = run_checks(&basis(), &empty, Some(&h), None, &CheckOptions::default());
    assert!(report.items.iter().all(|i| i.status == CheckStatus::NotChecked));
    assert!(report.items[0].message.contains("not checked"));
}

#[test]
fn minimum_radius_can_be_derived_from_e_and_f() {
    let h = HorizontalDesign::new("R", 0.0, vec![Pi::point(0.0, 0.0), Pi::curve(500.0, 0.0, 300.0), Pi::point(900.0, 300.0)]).solve().unwrap();
    let r = StandardRow { label: "derived".into(), design_speed_kmh: Some(100.0), max_superelevation: Some(0.08), max_side_friction: Some(0.12), ..StandardRow::default() };
    let report = run_checks(&basis(), &r, Some(&h), None, &CheckOptions::default());
    let item = report.items.iter().find(|i| i.id == "H.radius").unwrap();
    // 393.4 m minimum: R = 300 fails.
    assert_eq!(item.status, CheckStatus::Fail);
    close(item.limit.unwrap(), 393.40, 0.01, "derived limit");
    assert!(item.limit_basis.contains("derived") && item.limit_basis.contains("12.96"));
}

#[test]
fn vertical_checks_grade_k_length_and_sight_distance() {
    // Crest: PVIs (0,100), (400,108), (800,100): +-2 %, L = 200, K = 50.
    let d = VerticalDesign::new("P", vec![VPvi::point(0.0, 100.0), VPvi::with_length(400.0, 108.0, 200.0), VPvi::point(800.0, 100.0)]);
    let v = d.solve().unwrap();
    let mut r = row();
    r.min_k_crest = Some(60.0); // fails: K = 50
    r.max_grade = Some(0.015); // fails: 2 %
    r.min_grade = Some(0.005); // passes
    let report = run_checks(&basis(), &r, None, Some(&v), &CheckOptions::default());
    assert_eq!(report.items.iter().find(|i| i.id == "V.k_crest").unwrap().status, CheckStatus::Fail);
    assert_eq!(report.items.iter().filter(|i| i.id == "V.max_grade" && i.status == CheckStatus::Fail).count(), 2);
    assert!(report.items.iter().filter(|i| i.id == "V.min_grade").all(|i| i.status == CheckStatus::Pass));
    assert_eq!(report.items.iter().find(|i| i.id == "V.curve_length").unwrap().status, CheckStatus::Pass);

    // Stopping sight distance: derived from t and f, with eye and object heights. K = 50, A = 4 %:
    //   S provided = sqrt(2 L / A) (sqrt 1.08 + sqrt 0.6) = sqrt(10000) x 1.81383 = 181.4 m.
    // Required (V = 80, t = 2.5, f = 0.35 level) = 127.5 m: passes.
    r.min_k_crest = Some(10.0);
    r.max_grade = Some(0.06);
    r.reaction_time_s = Some(2.5);
    r.longitudinal_friction = Some(0.35);
    r.eye_height_m = Some(1.08);
    r.object_height_m = Some(0.6);
    let report = run_checks(&basis(), &r, None, Some(&v), &CheckOptions::default());
    let ssd = report.items.iter().find(|i| i.id == "V.ssd").unwrap();
    assert_eq!(ssd.status, CheckStatus::Pass, "{}", ssd.message);
    // A shorter curve cannot give it. L = 60 is shorter than the sight distance, so the line of
    // sight touches the grades beyond the curve and the S < L relation no longer holds; the
    // relation for S > L is L = 2 S - 200 (sqrt h1 + sqrt h2)^2 / A% (A in percent), i.e.
    //   S = (L + 200 x 3.28999 / 4) / 2 = (60 + 164.5) / 2 = 112.2 m < 127.5. The numeric test
    // has no such restriction and finds the same value.
    let mut d2 = d.clone();
    d2.set_curve_length(1, 60.0).unwrap();
    let v2 = d2.solve().unwrap();
    let report = run_checks(&basis(), &r, None, Some(&v2), &CheckOptions::default());
    let ssd = report.items.iter().find(|i| i.id == "V.ssd").unwrap();
    assert_eq!(ssd.status, CheckStatus::Fail, "{}", ssd.message);
    close(ssd.value.unwrap(), 112.2, 3.0, "sight distance provided");
}

#[test]
fn limits_tables_load_from_json_and_csv() {
    let csv = "#name,Fixture table\n#source,\"user input, test\"\nlabel,design_speed_kmh,min_radius_m,max_grade,min_k_crest\nA,80,250,0.06,30\nB,60,,0.08,\n";
    let b = DesignBasis::from_csv_str(csv).unwrap();
    assert_eq!(b.name, "Fixture table");
    assert_eq!(b.rows.len(), 2);
    assert_eq!(b.row("A").unwrap().min_radius_m, Some(250.0));
    assert_eq!(b.row("B").unwrap().min_radius_m, None);
    let back = DesignBasis::from_json_str(&b.to_json()).unwrap();
    assert_eq!(back, b);
    // No source: refused. Unknown column and bad number: named.
    assert!(DesignBasis::from_csv_str("label,max_grade\nA,0.06\n").unwrap_err().to_string().contains("source"));
    assert!(DesignBasis::from_csv_str("#source,x\nlabel,bogus\nA,1\n").unwrap_err().to_string().contains("bogus"));
    assert!(DesignBasis::from_csv_str("#source,x\nlabel,max_grade\nA,six\n").unwrap_err().to_string().contains("not a number"));
}

#[test]
fn a_default_basis_has_no_rows_and_no_source() {
    // `DesignBasis::default()` is empty. The one built-in basis is a separate constructor,
    // `DesignBasis::mowt_2010()`, and carries its sources.
    let b = DesignBasis::default();
    assert!(b.rows.is_empty() && b.source.is_empty());
}
