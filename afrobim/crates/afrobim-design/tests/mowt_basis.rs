// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The built-in `mowt_2010` basis in the check framework: row contents and sources, refusals, the list of built-in
//! bases, the user table overriding it, and a full alignment check with hand-worked expected statuses.

use afrobim_design::mowt::classes::{RoadClass, Terrain};
use afrobim_design::mowt::{known_slips, mowt_2010_row_label, not_checked_under_mowt_2010, GradeLimit, MowtOptions};
use afrobim_design::*;

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: got {a}, expected {b} (tolerance {tol})");
}

#[test]
fn class_ii_rolling_70_row_carries_table_values_and_their_sources() {
    let basis = DesignBasis::mowt_2010(RoadClass::II, Terrain::Rolling, 70.0, 7.0).unwrap();
    assert!(basis.name.contains("mowt_2010") && basis.name.contains("January 2010"));
    assert!(basis.source.contains("January 2010") && basis.source.contains("uace.or.ug") && basis.source.contains("third party"));
    let label = mowt_2010_row_label(RoadClass::II, Terrain::Rolling, 70.0, 7.0);
    assert_eq!(label, "II Paved, Rolling, 70 km/h, emax 7 %");
    let row = basis.row(&label).unwrap();
    // Table 6-1 (p. 80, PDF p. 89): 70 km/h, emax 7 %: f 0.14, rounded radius 185. Table 4-5 (p. 46, PDF p. 55) agrees.
    assert_eq!(row.design_speed_kmh, Some(70.0));
    assert_eq!(row.min_radius_m, Some(185.0));
    assert_eq!(row.max_superelevation, Some(0.07));
    assert_eq!(row.max_side_friction, Some(0.14));
    // Table 4-5: stopping sight distance 95, crest K 22, sag K 20, desirable gradient 5.5 %.
    assert_eq!(row.stopping_sight_distance_m, Some(95.0));
    assert_eq!(row.min_k_crest, Some(22.0));
    assert_eq!(row.min_k_sag, Some(20.0));
    close(row.max_grade.unwrap(), 0.055, 1e-12, "desirable gradient");
    assert_eq!(row.min_grade, None);
    // Inputs stated by the manual: prt 2.5 s, eye 1.07 m, object 0.15 m (p. 128), headlight 0.6 m and 1 degree (p. 110).
    assert_eq!((row.reaction_time_s, row.eye_height_m, row.object_height_m, row.headlight_height_m, row.headlight_beam_deg), (Some(2.5), Some(1.07), Some(0.15), Some(0.6), Some(1.0)));
    // What the manual does not give is None, with the reason.
    assert_eq!((row.min_curve_length_m, row.min_spiral_length_m, row.min_tangent_same_direction_m, row.min_tangent_reverse_m, row.min_vertical_curve_length_m), (None, None, None, None, None));
    assert!(row.source_of("min_curve_length_m").unwrap().contains("no minimum length of horizontal curve"));
    // Every limit that is set carries table, section, printed page and PDF page.
    let s = row.source_of("min_radius_m").unwrap();
    assert!(s.contains("Table 6-1") && s.contains("section 6.2.3") && s.contains("p. 80 (PDF p. 89)"), "{s}");
    let s = row.source_of("stopping_sight_distance_m").unwrap();
    assert!(s.contains("Table 4-5") && s.contains("p. 46 (PDF p. 55)"), "{s}");
    let s = row.source_of("max_grade").unwrap();
    assert!(s.contains("Table 4-5") && s.contains("desirable"), "{s}");
    assert!(row.source_of("min_k_crest").unwrap().contains("Table 4-5"));
    assert!(row.source_of("eye_height_m").unwrap().contains("p. 128 (PDF p. 137)"));
    assert!(row.source_of("headlight_height_m").unwrap().contains("p. 110 (PDF p. 119)"));
    assert!(row.source_of("reaction_time_s").unwrap().contains("p. 55 (PDF p. 64)"));
    // The notes say what is not checked.
    for id in ["H.curve_length", "H.spiral_length", "H.tangent_same", "H.tangent_reverse", "V.min_grade", "V.curve_length"] {
        assert!(basis.notes.contains(id), "{id} in notes");
    }
    // JSON round trip keeps the sources.
    let back = DesignBasis::from_json_str(&basis.to_json()).unwrap();
    assert_eq!(back, basis);
}

#[test]
fn a_speed_below_the_class_maximum_uses_the_speed_tables() {
    // Class Ia flat is 120 km/h at most (Table 4-2a). At 100 km/h: Table 6-1 radius 415 (7 %), Table 6.19 low end of the
    // range 157.0, Table 6-11 K 62, Table 6-13 K 37 (low ends, as the class tables use).
    let basis = DesignBasis::mowt_2010(RoadClass::Ia, Terrain::Flat, 100.0, 7.0).unwrap();
    let row = &basis.rows[0];
    assert_eq!(row.min_radius_m, Some(415.0));
    assert_eq!(row.stopping_sight_distance_m, Some(157.0));
    assert_eq!(row.min_k_crest, Some(62.0));
    assert_eq!(row.min_k_sag, Some(37.0));
    assert!(row.source_of("stopping_sight_distance_m").unwrap().contains("Table 6.19") && row.source_of("min_k_crest").unwrap().contains("Table 6-11,"));
    assert!(row.source_of("min_k_sag").unwrap().contains("Table 6-13"));
    // Gradients stay those of the class and terrain: Ia flat 3 % desirable.
    close(row.max_grade.unwrap(), 0.03, 1e-12, "gradient");
    // At the class maximum the class table value (rounded up) is used: 205 against 203.1.
    let top = DesignBasis::mowt_2010(RoadClass::Ia, Terrain::Flat, 120.0, 7.0).unwrap();
    assert_eq!(top.rows[0].stopping_sight_distance_m, Some(205.0));
    assert_eq!(top.rows[0].min_radius_m, Some(710.0));
    assert_eq!(top.rows[0].min_k_crest, Some(103.0));
}

#[test]
fn emax_changes_radius_and_the_options_change_grade_and_minimum_grade() {
    let r = |emax: f64| DesignBasis::mowt_2010(RoadClass::II, Terrain::Rolling, 70.0, emax).unwrap().rows[0].min_radius_m.unwrap();
    // Table 6-1 rounded radii at 70 km/h: 4 % 215, 6 % 195, 7 % 185, 8 % 175.
    assert_eq!((r(4.0), r(6.0), r(7.0), r(8.0)), (215.0, 195.0, 185.0, 175.0));
    let abs = DesignBasis::mowt_2010_with(RoadClass::II, Terrain::Rolling, 70.0, 7.0, &MowtOptions { grade: GradeLimit::Absolute, minimum_grade_in_cut: true }).unwrap();
    let row = &abs.rows[0];
    close(row.max_grade.unwrap(), 0.075, 1e-12, "absolute gradient"); // Table 4-5: 7.5 %
    close(row.min_grade.unwrap(), 0.005, 1e-12, "minimum gradient in cut"); // Table 4-5: 0.5 %
    assert!(row.source_of("min_grade").unwrap().contains("cuttings only"));
    // Urban column: emax 4 %, 50 km/h, radius 100.
    let u = DesignBasis::mowt_2010(RoadClass::II, Terrain::UrbanPeriUrban, 50.0, 4.0).unwrap();
    assert_eq!(u.rows[0].min_radius_m, Some(100.0));
    assert_eq!(u.rows[0].stopping_sight_distance_m, Some(58.0)); // Table 4-5 prints 58 (slip S06)
}

#[test]
fn a_withheld_cell_is_left_out_and_the_reason_is_in_the_row() {
    // Class Ia rolling: the crest K cell (80) is withheld; the row has no min_k_crest and says why.
    let basis = DesignBasis::mowt_2010(RoadClass::Ia, Terrain::Rolling, 100.0, 7.0).unwrap();
    let row = &basis.rows[0];
    assert_eq!(row.min_k_crest, None);
    let why = row.source_of("min_k_crest").unwrap();
    assert!(why.contains("withheld") && why.contains("62"), "{why}");
    // Class II mountainous: the gradients are withheld.
    let b = DesignBasis::mowt_2010(RoadClass::II, Terrain::Mountainous, 60.0, 7.0).unwrap();
    assert_eq!(b.rows[0].max_grade, None);
    assert!(b.rows[0].source_of("max_grade").unwrap().contains("withheld"));
    // run_checks says "not checked" for it and prints the reason.
    let v = VerticalDesign::new("V", vec![VPvi::point(0.0, 100.0), VPvi::point(500.0, 110.0)]).solve().unwrap();
    let report = run_checks(&b, &b.rows[0], None, Some(&v), &CheckOptions::default());
    let g = report.items.iter().find(|i| i.id == "V.max_grade").unwrap();
    assert_eq!(g.status, CheckStatus::NotChecked);
    assert!(g.limit_basis.contains("withheld"), "{}", g.limit_basis);
}

#[test]
fn refusals_are_plain() {
    let e = |r: Result<DesignBasis, DesignError>| r.unwrap_err().to_string();
    assert!(e(DesignBasis::mowt_2010(RoadClass::II, Terrain::Escarpment, 50.0, 7.0)).contains("escarpment"));
    let m = e(DesignBasis::mowt_2010(RoadClass::Ib, Terrain::Flat, 100.0, 7.0));
    assert!(m.contains("110") && m.contains("120"), "{m}");
    let m = e(DesignBasis::mowt_2010(RoadClass::II, Terrain::Rolling, 80.0, 7.0));
    assert!(m.contains("above the maximum of 70 km/h") && m.contains("Table 4-5"), "{m}");
    assert!(e(DesignBasis::mowt_2010(RoadClass::II, Terrain::Rolling, 65.0, 7.0)).contains("not one of them"));
    assert!(e(DesignBasis::mowt_2010(RoadClass::II, Terrain::Rolling, 70.0, 5.0)).contains("4, 6, 7 and 8"));
    assert!(e(DesignBasis::from_builtin("nope", &selection())).contains("no built-in basis"));
}

fn selection() -> BuiltinSelection {
    BuiltinSelection { class: RoadClass::III, terrain: Terrain::Rolling, design_speed_kmh: 70.0, emax_percent: 7.0, options: MowtOptions::default() }
}

#[test]
fn the_list_of_built_in_bases_for_the_app() {
    let list = builtin_bases();
    assert_eq!(list.len(), 1);
    let b = &list[0];
    assert_eq!(b.id, "mowt_2010");
    assert!(b.source.contains("January 2010") && b.source.contains("third party"));
    assert_eq!(b.emax_percent_options, vec![4.0, 6.0, 7.0, 8.0]);
    assert_eq!(b.classes.len(), 7);
    let ii = b.classes.iter().find(|c| c.class == RoadClass::II).unwrap();
    assert_eq!(ii.name, "II Paved");
    assert_eq!(ii.traffic_kpcu_per_day, Some((4.0, 8.0)));
    let rolling = ii.terrains.iter().find(|t| t.terrain == Terrain::Rolling).unwrap();
    assert_eq!(rolling.max_design_speed_kmh, 70.0);
    assert_eq!(rolling.speeds_kmh, vec![30.0, 40.0, 50.0, 60.0, 70.0]);
    assert_eq!(ii.terrains.len(), 4);
    assert!(ii.refused_terrains.iter().any(|(t, why)| *t == Terrain::Escarpment && why.contains("escarpment")));
    let ib = b.classes.iter().find(|c| c.class == RoadClass::Ib).unwrap();
    assert!(!ib.terrains.iter().any(|t| t.terrain == Terrain::Flat));
    assert!(ib.refused_terrains.iter().any(|(t, why)| *t == Terrain::Flat && why.contains("Table 4-2a")));
    let c = b.classes.iter().find(|c| c.class == RoadClass::C).unwrap();
    assert_eq!(c.traffic_kpcu_per_day, None);
    // every offered combination builds
    let mut n = 0;
    for class in &b.classes {
        for t in &class.terrains {
            for s in &t.speeds_kmh {
                for emax in &b.emax_percent_options {
                    let basis = DesignBasis::from_builtin("mowt_2010", &BuiltinSelection { class: class.class, terrain: t.terrain, design_speed_kmh: *s, emax_percent: *emax, options: MowtOptions::default() }).unwrap();
                    assert_eq!(basis.rows.len(), 1);
                    assert!(basis.rows[0].min_radius_m.is_some());
                    n += 1;
                }
            }
        }
    }
    // Ia 3 terrains (12, 10, 8 speeds? 30..120 = 10, 30..100 = 8, 30..80 = 6) plus urban 30..50 = 3, and so on.
    assert!(n > 400, "{n} built-in combinations");
    // The not-checked list: six checks the framework has, six limits it has no check for.
    let nc = not_checked_under_mowt_2010();
    assert_eq!(nc.iter().filter(|i| i.framework_has_check).count(), 6);
    assert_eq!(nc.iter().filter(|i| !i.framework_has_check).count(), 6);
    assert!(!known_slips().is_empty());
}

#[test]
fn a_user_table_overrides_the_built_in_one_when_chosen() {
    let basis = DesignBasis::mowt_2010(RoadClass::II, Terrain::Rolling, 70.0, 7.0).unwrap();
    let csv = "#name,User limits\n#source,user input\nlabel,min_radius_m,max_grade\nmine,300,0.04\n";
    let user = DesignBasis::from_csv_str(csv).unwrap();
    let merged = basis.rows[0].overlay(&user.rows[0]);
    assert_eq!(merged.min_radius_m, Some(300.0));
    close(merged.max_grade.unwrap(), 0.04, 1e-12, "user gradient");
    assert_eq!(merged.min_k_crest, Some(22.0), "values the user did not give stay");
    assert!(merged.source_of("min_radius_m").is_none(), "the replaced value no longer claims the manual");
    assert!(merged.source_of("min_k_crest").unwrap().contains("Table 4-5"));
    assert_eq!(merged.label, "mine");
    // The CSV path is unchanged: a row loaded from CSV has no per-limit sources and reports the plain basis text.
    let h = HorizontalDesign::new("R", 0.0, vec![Pi::point(0.0, 0.0), Pi::curve(500.0, 0.0, 250.0), Pi::point(900.0, 300.0)]).solve().unwrap();
    let report = run_checks(&user, &user.rows[0], Some(&h), None, &CheckOptions::default());
    let radius = report.items.iter().find(|i| i.id == "H.radius").unwrap();
    assert_eq!(radius.limit_basis, "table value (minimum radius)");
    assert_eq!(radius.status, CheckStatus::Fail); // 250 < 300
    assert_eq!(report.source, "user input");
}

/// A full alignment check, class II paved, rolling terrain, 70 km/h, emax 7 %, worked by hand.
///
/// Limits (Table 6-1, p. 80, PDF p. 89, and Table 4-5, p. 46, PDF p. 55): radius 185 m, stopping sight distance 95 m,
/// crest K 22, sag K 20, desirable gradient 5.5 %; eye 1.07 m, object 0.15 m (p. 128), headlights 0.6 m and 1 degree (p. 110).
///
/// Plan: PIs (0,0), (500,0) R 200, (800,300) R 150, (1300,300).
///   PI 1: deflection 45 deg left, R 200 >= 185: pass.   PI 2: deflection 45 deg right, R 150 < 185: fail.
///   Leg PI 1 to PI 2 is 300 sqrt 2 = 424.264; tangents 200 tan 22.5 = 82.843 and 150 tan 22.5 = 62.132 leave a straight of
///   279.289 between reverse curves: no limit in the manual, not checked. Curve lengths: no limit, not checked.
///
/// Profile: PVIs (0,100), (400,124), (800,116), (1200,128).
///   Grades 24/400 = +6.0 % (fails 5.5 %), -8/400 = -2.0 % (pass), +12/400 = +3.0 % (pass).
///   PVI 1 crest, L 160, A = -8 %: K = 160 / 8 = 20 < 22: fail. Sight distance S = sqrt(2 L / A) (sqrt h1 + sqrt h2)
///     = sqrt(4000) x 1.4217 = 89.9 m < 95: fail.
///   PVI 2 sag, L 125, A = +5 %: K = 125 / 5 = 25 >= 20: pass. Headlight distance: A S^2 / (2 L) = H + S tan 1 deg gives
///     S = 113.7 m >= 95: pass.
///   Vertical curve length: no limit, not checked (twice).
#[test]
fn full_alignment_check_with_hand_worked_statuses() {
    let basis = DesignBasis::mowt_2010(RoadClass::II, Terrain::Rolling, 70.0, 7.0).unwrap();
    let row = &basis.rows[0];
    let h = HorizontalDesign::new("plan", 0.0, vec![Pi::point(0.0, 0.0), Pi::curve(500.0, 0.0, 200.0), Pi::curve(800.0, 300.0, 150.0), Pi::point(1300.0, 300.0)]).solve().unwrap();
    // The tangents worked by hand.
    close(h.tangents[1].leg_m, 424.264, 1e-3, "leg");
    close(h.tangents[1].straight_m, 279.289, 1e-3, "straight");
    assert_eq!(h.tangents[1].reverse, Some(true));
    let v = VerticalDesign::new(
        "profile",
        vec![VPvi::point(0.0, 100.0), VPvi::with_length(400.0, 124.0, 160.0), VPvi::with_length(800.0, 116.0, 125.0), VPvi::point(1200.0, 128.0)],
    )
    .solve()
    .unwrap();
    close(v.curves[0].k, 20.0, 1e-9, "crest K");
    close(v.curves[1].k, 25.0, 1e-9, "sag K");
    let report = run_checks(&basis, row, Some(&h), Some(&v), &CheckOptions::default());

    let items = |id: &str| report.items.iter().filter(|i| i.id == id).collect::<Vec<_>>();
    // H.radius
    let r = items("H.radius");
    assert_eq!(r.len(), 2);
    assert_eq!((r[0].location.as_str(), r[0].status, r[0].value, r[0].limit), ("PI 1", CheckStatus::Pass, Some(200.0), Some(185.0)));
    assert_eq!((r[1].location.as_str(), r[1].status, r[1].value, r[1].limit), ("PI 2", CheckStatus::Fail, Some(150.0), Some(185.0)));
    assert!(r[0].limit_basis.contains("table value (minimum radius)") && r[0].limit_basis.contains("Table 6-1") && r[0].limit_basis.contains("p. 80 (PDF p. 89)"), "{}", r[0].limit_basis);
    // No limits: not checked.
    for id in ["H.curve_length", "H.tangent_reverse", "V.curve_length"] {
        assert!(!items(id).is_empty(), "{id} present");
        assert!(items(id).iter().all(|i| i.status == CheckStatus::NotChecked), "{id} not checked");
    }
    assert_eq!(items("H.curve_length").len(), 2);
    assert_eq!(items("V.curve_length").len(), 2);
    assert_eq!(items("H.tangent_reverse").len(), 1);
    assert!(items("H.tangent_same").is_empty() && items("H.spiral_length").is_empty() && items("V.min_grade").is_empty());
    assert!(items("H.curve_length")[0].limit_basis.contains("no minimum length of horizontal curve"));
    // V.max_grade: +6 % fail, -2 % pass, +3 % pass, against 5.5 %.
    let g = items("V.max_grade");
    assert_eq!(g.len(), 3);
    assert_eq!((g[0].status, g[0].value, g[0].limit), (CheckStatus::Fail, Some(6.0), Some(5.5)));
    assert_eq!(g[1].status, CheckStatus::Pass);
    assert_eq!(g[2].status, CheckStatus::Pass);
    assert!(g[0].limit_basis.contains("Table 4-5") && g[0].limit_basis.contains("p. 46 (PDF p. 55)"));
    // K
    let kc = items("V.k_crest");
    assert_eq!((kc[0].location.as_str(), kc[0].status, kc[0].limit), ("PVI 1", CheckStatus::Fail, Some(22.0)));
    close(kc[0].value.unwrap(), 20.0, 1e-9, "crest K");
    let ks = items("V.k_sag");
    assert_eq!((ks[0].location.as_str(), ks[0].status, ks[0].limit), ("PVI 2", CheckStatus::Pass, Some(20.0)));
    // Sight distance: crest 89.9 m (< 95) fails, sag 113.7 m (>= 95) passes.
    let ssd = items("V.ssd");
    assert_eq!(ssd.len(), 2);
    assert_eq!((ssd[0].location.as_str(), ssd[0].status, ssd[0].limit), ("PVI 1", CheckStatus::Fail, Some(95.0)));
    assert!(ssd[0].value.unwrap() > 87.5 && ssd[0].value.unwrap() < 92.0, "crest sight distance {:?}", ssd[0].value);
    assert_eq!((ssd[1].location.as_str(), ssd[1].status), ("PVI 2", CheckStatus::Pass));
    // The framework searches out to the required distance only, so a pass reads "95.0" (at least the required 95 m).
    assert_eq!(ssd[1].value, Some(95.0));
    // The headlight distance itself, by the physics relation on this curve: 113.7 m.
    let sag = &v.curves[1];
    let s_head = afrobim_design::physics::sag_sight_distance(&v.profile, sag.pvc_station_m, 0.6, 1.0, 200.0, 0.1);
    close(s_head, 113.7, 0.3, "headlight distance at the PVC");
    assert!(ssd[0].limit_basis.contains("Table 4-5"));
    // Totals.
    assert_eq!(report.count(CheckStatus::Pass), 5);
    assert_eq!(report.count(CheckStatus::Fail), 4);
    assert_eq!(report.count(CheckStatus::NotChecked), 5);
    assert_eq!(report.items.len(), 14);
    assert!(report.source.contains("January 2010"));
    // The CSV carries the table and page of every limit.
    let csv = report.to_csv();
    assert!(csv.contains("Table 6-1") && csv.contains("PDF p. 55") && csv.contains("# basis,"));

    // With the absolute gradient (7.5 %) the 6 % grade passes; with the minimum grade in cut all grades (2, 3, 6 %) pass 0.5 %.
    let abs = DesignBasis::mowt_2010_with(RoadClass::II, Terrain::Rolling, 70.0, 7.0, &MowtOptions { grade: GradeLimit::Absolute, minimum_grade_in_cut: true }).unwrap();
    let report = run_checks(&abs, &abs.rows[0], Some(&h), Some(&v), &CheckOptions::default());
    assert!(report.items.iter().filter(|i| i.id == "V.max_grade").all(|i| i.status == CheckStatus::Pass));
    assert_eq!(report.items.iter().filter(|i| i.id == "V.min_grade").count(), 3);
    assert!(report.items.iter().filter(|i| i.id == "V.min_grade").all(|i| i.status == CheckStatus::Pass));
}

#[test]
fn a_withheld_crest_k_is_derived_and_says_so() {
    // Class Ia rolling at 100 km/h: the K cell is withheld; with the stopping sight distance 160 m and the manual's
    // heights run_checks derives K = S^2 / (200 (sqrt 1.07 + sqrt 0.15)^2) = 25600 / 404.2 = 63.3 and labels it derived.
    let basis = DesignBasis::mowt_2010(RoadClass::Ia, Terrain::Rolling, 100.0, 7.0).unwrap();
    let v = VerticalDesign::new("V", vec![VPvi::point(0.0, 100.0), VPvi::with_length(500.0, 120.0, 300.0), VPvi::point(1000.0, 100.0)]).solve().unwrap();
    let report = run_checks(&basis, &basis.rows[0], None, Some(&v), &CheckOptions::default());
    let k = report.items.iter().find(|i| i.id == "V.k_crest").unwrap();
    assert!(k.limit_basis.starts_with("derived"), "{}", k.limit_basis);
    close(k.limit.unwrap(), 63.33, 0.05, "derived crest K");
}
