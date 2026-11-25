// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Vertical alignment, sight distance and cross-section tables of the Uganda MoWT Road Design Manual Volume I
//! (January 2010). Page numbers are printed page (PDF page, printed + 9).

use afrobim_design::mowt::classes::Terrain;
use afrobim_design::mowt::cross_section::*;
use afrobim_design::mowt::sight::*;
use afrobim_design::mowt::vertical::*;
use afrobim_design::mowt::RoadClass;

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: got {a}, expected {b} (tolerance {tol})");
}

const SPEEDS: [f64; 10] = [30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 110.0, 120.0];

#[test]
fn table_6_11_crest_k_stopping_every_row() {
    // Table 6-11: K value for crest vertical curve based on stopping sight distance as design control, section 6.3.1,
    // p. 108 (PDF p. 117). Per speed: assumed speed range, f, SSD range, K computed range, K rounded range.
    let rows: [((f64, f64), f64, (f64, f64), (f64, f64), (f64, f64)); 10] = [
        ((30.0, 30.0), 0.40, (29.7, 29.7), (2.18, 2.18), (3.0, 3.0)),
        ((40.0, 40.0), 0.38, (44.4, 44.4), (4.88, 4.88), (5.0, 5.0)),
        ((47.0, 50.0), 0.35, (57.5, 62.8), (8.18, 9.76), (9.0, 10.0)),
        ((55.0, 60.0), 0.33, (74.3, 84.6), (13.66, 17.72), (14.0, 18.0)),
        ((63.0, 70.0), 0.31, (94.2, 110.8), (21.96, 30.39), (22.0, 31.0)),
        ((70.0, 80.0), 0.30, (112.9, 139.5), (31.55, 48.17), (32.0, 49.0)),
        ((77.0, 90.0), 0.30, (131.3, 168.8), (42.67, 70.53), (43.0, 71.0)),
        ((85.0, 100.0), 0.29, (157.0, 205.2), (61.01, 104.23), (62.0, 105.0)),
        ((91.0, 110.0), 0.28, (179.6, 246.5), (79.84, 150.40), (80.0, 151.0)),
        ((98.0, 120.0), 0.28, (203.1, 285.8), (102.10, 202.18), (103.0, 203.0)),
    ];
    for (v, (assumed, f, ssd, computed, rounded)) in SPEEDS.iter().zip(rows) {
        let c = crest_k_stopping(*v).unwrap();
        assert_eq!((c.value.assumed_speed_kmh, c.value.friction, c.value.stopping_sight_distance_m, c.value.k_rounded), (assumed, f, ssd, rounded), "{v}");
        assert_eq!(c.value.k_computed, computed, "{v}");
        assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page), ("Table 6-11", 108, 117));
        // The computed K is S^2 / 404 for the low and the high stopping sight distance (formula of p. 107).
        close(ssd.0 * ssd.0 / 404.0, computed.0, 0.011, &format!("S^2/404 low at {v}"));
        close(ssd.1 * ssd.1 / 404.0, computed.1, 0.011, &format!("S^2/404 high at {v}"));
        // ...with the exception of the first row, printed 2.18 where 29.7^2 / 404 = 2.18; (kept as a check, not a slip)
    }
    assert!(crest_k_stopping(35.0).is_err());
}

#[test]
fn table_6_12_crest_k_passing_every_row() {
    // Table 6-12 (titled "stopping" in error, slip S18): crest K from passing sight distance, p. 110 (PDF p. 119).
    let rows = [
        (217.0, 49.78, 50.0),
        (285.0, 85.86, 86.0),
        (345.0, 125.82, 126.0),
        (407.0, 175.10, 176.0),
        (482.0, 245.59, 246.0),
        (541.0, 309.39, 310.0),
        (605.0, 386.92, 387.0),
        (670.0, 474.52, 475.0),
        (728.0, 560.24, 561.0),
        (792.0, 663.07, 664.0),
    ];
    for (v, (psd, computed, rounded)) in SPEEDS.iter().zip(rows) {
        let c = crest_k_passing(*v).unwrap();
        assert_eq!((c.value.passing_sight_distance_m, c.value.k_computed, c.value.k_rounded), (psd, computed, rounded), "{v}");
        assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page), ("Table 6-12", 110, 119));
        // K = S^2 / 946 (p. 109)
        close(psd * psd / 946.0, computed, 0.011, &format!("S^2/946 at {v}"));
    }
}

#[test]
fn table_6_13_sag_k_every_row() {
    // Table 6-13: K value for sag vertical curve based on stopping sight distance, section 6.3.2, p. 111 (PDF p. 120).
    let rows: [((f64, f64), (f64, f64)); 10] = [
        ((3.94, 3.94), (4.0, 4.0)),
        ((7.16, 7.16), (8.0, 8.0)),
        ((10.29, 11.61), (11.0, 12.0)),
        ((14.53, 17.20), (15.0, 18.0)),
        ((19.73, 24.18), (20.0, 25.0)),
        ((24.74, 31.99), (25.0, 32.0)),
        ((29.75, 40.09), (30.0, 41.0)),
        ((36.82, 50.24), (37.0, 51.0)),
        ((43.09, 61.83), (44.0, 62.0)),
        ((49.65, 72.91), (50.0, 73.0)),
    ];
    for (v, (computed, rounded)) in SPEEDS.iter().zip(rows) {
        let c = sag_k(*v).unwrap();
        assert_eq!((c.value.k_computed, c.value.k_rounded), (computed, rounded), "{v}");
        assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page), ("Table 6-13", 111, 120));
        // K = S^2 / (120 + 3.5 S) with the low and high stopping sight distance of Table 6.19 (p. 111)
        let s = c.value.stopping_sight_distance_m;
        close(s.0 * s.0 / (120.0 + 3.5 * s.0), computed.0, 0.011, &format!("sag K low at {v}"));
        close(s.1 * s.1 / (120.0 + 3.5 * s.1), computed.1, 0.011, &format!("sag K high at {v}"));
        // the rounded value never falls below the computed one
        assert!(rounded.0 >= computed.0 - 1e-9 && rounded.1 >= computed.1 - 1e-9);
    }
}

#[test]
fn vertical_curve_length_formulas_of_section_6_3() {
    // Crest, stopping, p. 107 (PDF p. 116): L = G S^2 / 404 when S < L; L = 2 S - 404 / G when S > L.
    // G = 4 %, S = 160: 4 x 25600 / 404 = 253.5 m (> S: valid).
    close(crest_min_length_stopping_m(160.0, 4.0).unwrap().value, 253.47, 0.01, "S < L");
    // G = 1 %, S = 160: 25600 / 404 = 63.4 < S, so L = 2 x 160 - 404 / 1 = -84 (negative: no curve needed for sight).
    assert!(crest_min_length_stopping_m(160.0, 1.0).unwrap().value < 0.0);
    // G = 2 %, S = 160: 2 x 25600 / 404 = 126.7 < 160, so L = 320 - 404 / 2 = 118.0 m.
    close(crest_min_length_stopping_m(160.0, 2.0).unwrap().value, 118.0, 1e-9, "S > L");
    // The general form with h1 1.07 and h2 0.15 gives 404.2 in place of 404: 4 % x 160^2 / 404.24 = 253.3.
    close(crest_min_length_general_m(160.0, 4.0, 1.07, 0.15).unwrap().value, 4.0 * 160.0 * 160.0 / 404.24, 0.05, "general crest");
    // Passing, p. 109 (PDF p. 118): L = G S^2 / 946; G = 4 %, S = 541: 4 x 292681 / 946 = 1237.6 m.
    close(crest_min_length_passing_m(541.0, 4.0).unwrap().value, 1237.5, 0.5, "passing");
    // Sag, p. 111 (PDF p. 120): L = G S^2 / (120 + 3.5 S); G = 8 %, S = 100: 8 x 10000 / 470 = 170.2 m (> S, valid).
    close(sag_min_length_m(100.0, 8.0).unwrap().value, 170.21, 0.01, "sag S < L");
    // G = 4 %, S = 113: 4 x 12769 / 515.5 = 99.1 < S, so L = 2 S - (120 + 3.5 S) / G = 226 - 128.9 = 97.1 m.
    close(sag_min_length_m(113.0, 4.0).unwrap().value, 97.125, 1e-9, "sag S > L");
    // General sag (p. 110): 200 (0.6 + S tan 1 deg) = 200 (0.6 + 100 x 0.0174551) = 469.1 against 120 + 3.5 x 100 = 470.
    close(sag_min_length_general_m(100.0, 8.0, 0.6, 1.0).unwrap().value, 8.0 * 10000.0 / (200.0 * (0.6 + 100.0 * 1.0_f64.to_radians().tan())), 1e-9, "general sag");
    // Comfort (p. 111): L = G V^2 / 395; G = 4 %, V = 100: 4 x 10000 / 395 = 101.3 m
    close(sag_comfort_length_m(100.0, 4.0).value, 101.27, 0.01, "comfort");
    assert_eq!(appearance_rule_of_thumb_k().value, 30.0);
    assert_eq!(kerbed_drainage_minimum_grade().value, (0.30, 15.0));
    assert!(crest_min_length_stopping_m(160.0, 0.0).is_err());
    // Inputs the manual states (pp. 107, 109, 110): eye 1.07, object 0.15 and 1.3, headlight 0.6 m and 1 degree.
    assert_eq!((EYE_HEIGHT_M, OBJECT_HEIGHT_STOPPING_M, OBJECT_HEIGHT_PASSING_M, HEADLIGHT_HEIGHT_M, HEADLIGHT_BEAM_DEG), (1.07, 0.15, 1.3, 0.6, 1.0));
    // 404 and 946 follow from the heights: 100 (sqrt(2 x 1.07) + sqrt(2 x 0.15))^2 = 404.2; 100 (sqrt(2 x 1.07) + sqrt(2 x 1.3))^2 = 945.8
    close(100.0 * ((2.0 * 1.07_f64).sqrt() + (2.0 * 0.15_f64).sqrt()).powi(2), 404.24, 0.01, "404");
    close(100.0 * ((2.0 * 1.07_f64).sqrt() + (2.0 * 1.3_f64).sqrt()).powi(2), 945.76, 0.01, "946");
}

#[test]
fn tables_6_14_and_6_15_every_cell() {
    // Table 6-14: maximum gradients (desirable), section 6.5.1, p. 122 (PDF p. 131); speeds 40 to 120, a dash is None.
    let desirable: [[Option<f64>; 9]; 3] = [
        [None, None, Some(5.0), Some(4.0), Some(4.0), Some(3.5), Some(3.0), Some(3.0), Some(3.0)],
        [None, Some(7.0), Some(6.0), Some(5.5), Some(5.0), Some(4.5), Some(4.5), None, None],
        [Some(10.0), Some(9.0), Some(8.0), Some(7.0), None, None, None, None, None],
    ];
    // Table 6-15: absolute maximum gradients, same page.
    let absolute: [[Option<f64>; 9]; 3] = [
        [None, None, None, Some(6.0), Some(6.0), Some(5.5), Some(5.0), Some(4.0), Some(4.0)],
        [None, Some(9.0), Some(8.0), Some(7.5), Some(7.0), Some(6.5), None, None, None],
        [Some(12.0), Some(11.0), Some(10.0), Some(9.0), None, None, None, None, None],
    ];
    let speeds = [40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 110.0, 120.0];
    for (ti, t) in [Terrain::Flat, Terrain::Rolling, Terrain::Mountainous].iter().enumerate() {
        for (si, v) in speeds.iter().enumerate() {
            match desirable[ti][si] {
                Some(x) => {
                    let c = max_gradient_desirable(*t, *v).unwrap();
                    assert_eq!(c.value, x, "{t:?} {v}");
                    assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page, c.unit), ("Table 6-14", 122, 131, "%"));
                }
                None => assert!(max_gradient_desirable(*t, *v).unwrap_err().to_string().contains("dash"), "{t:?} {v}"),
            }
            match absolute[ti][si] {
                Some(x) => assert_eq!(max_gradient_absolute(*t, *v).unwrap().value, x, "{t:?} {v}"),
                None => assert!(max_gradient_absolute(*t, *v).unwrap_err().to_string().contains("dash")),
            }
        }
    }
    assert!(max_gradient_desirable(Terrain::Flat, 30.0).is_err());
    assert!(max_gradient_desirable(Terrain::UrbanPeriUrban, 60.0).is_err());
    // Desirable never exceeds absolute where both are printed.
    for t in [Terrain::Flat, Terrain::Rolling, Terrain::Mountainous] {
        for v in speeds {
            if let (Ok(d), Ok(a)) = (max_gradient_desirable(t, v), max_gradient_absolute(t, v)) {
                assert!(d.value < a.value);
            }
        }
    }
}

#[test]
fn tables_6_16_to_6_18_and_the_minimum_gradient() {
    // Table 6-16: maximum desirable length of gradient, section 6.5.4, p. 123 (PDF p. 132).
    for (g, l) in [(12.0, 300.0), (11.0, 350.0), (10.0, 400.0), (9.0, 450.0)] {
        let c = critical_length_of_gradient_m(g).unwrap();
        assert_eq!(c.value, l);
        assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page), ("Table 6-16", 123, 132));
    }
    assert!(critical_length_of_gradient_m(8.0).unwrap_err().to_string().contains("not a row"));
    assert!(critical_length_of_gradient_m(10.5).is_err());
    // Table 6-17: climbing lane threshold, section 6.6, p. 125 (PDF p. 134).
    for (g, l) in [(2.0, 1500.0), (3.0, 500.0), (4.0, 300.0)] {
        assert_eq!(climbing_lane_threshold_length_m(g).unwrap().value, l);
    }
    assert!(climbing_lane_threshold_length_m(5.0).is_err());
    // Table 6-18: entry and exit tapers, same page.
    assert_eq!(climbing_lane_tapers_m(80.0).unwrap().value, (150.0, 200.0));
    assert_eq!(climbing_lane_tapers_m(100.0).unwrap().value, (200.0, 300.0));
    assert!(climbing_lane_tapers_m(90.0).is_err());
    // Minimum gradient in cuttings, section 6.5.2, p. 123 (PDF p. 132): 0.3 - 0.5 percent.
    let m = min_gradient_in_cuttings_range_percent();
    assert_eq!((m.value, m.source.printed_page, m.source.pdf_page), ((0.3, 0.5), 123, 132));
}

#[test]
fn table_6_19_stopping_sight_distance_every_row_and_the_formula() {
    // Table 6.19: stopping sight distance on level ground for wet pavement, section 6.7.2, p. 129 (PDF p. 138).
    // Per speed: assumed speed, reaction distance (low, high), f, braking distance (low, high), SSD (low, high).
    #[allow(clippy::type_complexity)]
    let rows: [((f64, f64), (f64, f64), f64, (f64, f64), (f64, f64)); 10] = [
        ((30.0, 30.0), (20.8, 20.8), 0.40, (8.9, 8.9), (29.7, 29.7)),
        ((40.0, 40.0), (27.8, 27.8), 0.38, (16.6, 16.6), (44.4, 44.4)),
        ((47.0, 50.0), (32.6, 34.7), 0.35, (24.8, 28.1), (57.5, 62.8)),
        ((55.0, 60.0), (38.2, 41.7), 0.33, (36.1, 42.9), (74.3, 84.6)),
        ((63.0, 70.0), (43.8, 48.6), 0.31, (50.4, 62.2), (94.2, 110.8)),
        ((70.0, 80.0), (48.6, 55.6), 0.30, (64.3, 84.0), (112.9, 139.5)),
        ((77.0, 90.0), (53.5, 62.5), 0.30, (77.8, 106.3), (131.3, 168.8)),
        ((85.0, 100.0), (59.0, 69.4), 0.29, (98.1, 135.8), (157.0, 205.2)),
        ((91.0, 110.0), (63.2, 76.4), 0.28, (116.4, 170.1), (179.6, 246.5)),
        ((98.0, 120.0), (68.1, 83.3), 0.28, (135.0, 202.5), (203.1, 285.8)),
    ];
    for (v, (assumed, rd, f, bd, ssd)) in SPEEDS.iter().zip(rows) {
        let c = stopping_sight_distance(*v).unwrap();
        let r = &c.value;
        assert_eq!((r.assumed_speed_kmh, r.reaction_distance_m, r.friction, r.braking_distance_m, r.stopping_sight_distance_m, r.reaction_time_s), (assumed, rd, f, bd, ssd, 2.5), "{v}");
        assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page), ("Table 6.19", 129, 138));
        // The manual's formula SSD = 0.278 prt V + V^2 / (254 f) with prt 2.5 s, at the low (assumed) and high (design) speed.
        for (k, sp) in [assumed.0, assumed.1].iter().enumerate() {
            let want = [ssd.0, ssd.1][k];
            let got = stopping_sight_distance_formula_m(2.5, *sp, f, 0.0).unwrap();
            close(got, want, 0.2, &format!("SSD formula at {sp} km/h"));
        }
    }
    // Inputs of the manual: prt 2.5 s, eye 1.07 m, object 0.15 m (sections 5.3 and 6.7.2).
    let i = inputs();
    assert_eq!((i.reaction_time_s.value, i.eye_height_m.value, i.object_height_stopping_m.value), (2.5, 1.07, 0.15));
    assert_eq!((i.reaction_time_s.source.printed_page, i.reaction_time_s.source.pdf_page), (55, 64));
    assert_eq!((i.eye_height_m.source.printed_page, i.eye_height_m.source.pdf_page), (128, 137));
    assert_eq!((i.object_height_passing_m.value, i.headlight_height_m.value, i.headlight_beam_deg.value, i.meeting_safety_distance_m.value), (1.3, 0.6, 1.0, 10.0));
    assert!(stopping_sight_distance_formula_m(2.5, 80.0, 0.3, -0.3).is_none());
}

#[test]
fn table_6_20_effect_of_grade_every_cell_and_the_withheld_column() {
    // Table 6.20: effect of grade on stopping sight distance, p. 129 (PDF p. 138). Downgrades 3, 5, 7 % and upgrades 3, 5, 7 %.
    let down = [
        [30.4, 30.7, 31.6],
        [45.8, 46.3, 48.1],
        [65.5, 66.5, 69.9],
        [88.9, 90.6, 96.2],
        [117.5, 120.1, 129.0],
        [148.9, 152.5, 165.2],
        [180.7, 185.2, 201.2],
        [220.9, 227.0, 248.5],
        [267.0, 274.9, 303.3],
        [310.2, 319.6, 353.4],
    ];
    let up = [
        [29.1, 28.7, 28.4],
        [43.2, 42.4, 41.8],
        [55.6, 54.4, 53.4],
        [71.6, 69.6, 68.0],
        [89.7, 87.2, 84.9],
        [107.1, 103.8, 100.8],
        [124.2, 120.2, 116.6],
        [148.0, 142.7, 138.1],
        [168.4, 162.0, 156.4],
        [190.1, 182.7, 176.1],
    ];
    let f = [0.40, 0.38, 0.35, 0.33, 0.31, 0.30, 0.30, 0.29, 0.28, 0.28];
    let assumed_low = [30.0, 40.0, 47.0, 55.0, 63.0, 70.0, 77.0, 85.0, 91.0, 98.0];
    let mut off = Vec::new();
    for (i, v) in SPEEDS.iter().enumerate() {
        for (j, g) in [3.0, 5.0, 7.0].iter().enumerate() {
            // Upgrade cells: assumed speed, f, +G; downgrade: design speed, -G.
            let u = stopping_sight_distance_on_grade(*v, *g, GradeDirection::Upgrade).unwrap();
            assert_eq!(u.value, up[i][j]);
            let uf = stopping_sight_distance_formula_m(2.5, assumed_low[i], f[i], g / 100.0).unwrap();
            if (uf - up[i][j]).abs() > 0.15 {
                off.push(format!("up {v} {g}: {uf:.2} vs {}", up[i][j]));
            }
            match stopping_sight_distance_on_grade(*v, *g, GradeDirection::Downgrade) {
                Ok(d) => {
                    assert_eq!(d.value, down[i][j]);
                    close(stopping_sight_distance_formula_m(2.5, *v, f[i], -g / 100.0).unwrap(), down[i][j], 0.15, &format!("down {v} {g}"));
                }
                Err(e) => {
                    assert_eq!(j, 1, "only the 5 % downgrade column is withheld");
                    assert!(e.to_string().contains("4 % downgrade"));
                    // The printed 5 % column equals the formula for a 4 % downgrade, row by row (slip S19).
                    close(stopping_sight_distance_formula_m(2.5, *v, f[i], -0.04).unwrap(), down[i][1], 0.1, &format!("5 % column is 4 % at {v}"));
                }
            }
        }
    }
    // One further cell differs by 0.3 m: upgrade 3 %, 60 km/h (S20).
    assert_eq!(off, vec!["up 60 3: 71.31 vs 71.6".to_string()]);
    assert!(stopping_sight_distance_on_grade(80.0, 4.0, GradeDirection::Upgrade).is_err());
}

#[test]
fn tables_6_21_and_6_22_every_row_and_meeting_sight_distance() {
    // Table 6.21: elements of safe passing sight distance, two lane roads, section 6.7.3, p. 132 (PDF p. 141).
    // (speed in group, average speed, a, t1, d1, t2, d2, d3, d4, total)
    let groups = [
        (60.0, (50.0, 65.0), 56.2, 2.25, 3.6, 45.0, 9.3, 145.0, 30.0, 95.0, 315.0),
        (70.0, (66.0, 80.0), 70.0, 2.30, 4.0, 65.0, 10.0, 195.0, 55.0, 130.0, 445.0),
        (85.0, (81.0, 95.0), 84.5, 2.37, 4.3, 90.0, 10.7, 250.0, 75.0, 165.0, 580.0),
        (100.0, (96.0, 110.0), 99.80, 2.41, 4.5, 110.0, 11.3, 315.0, 90.0, 210.0, 725.0),
    ];
    for (speed, group, avg, a, t1, d1, t2, d2, d3, d4, total) in groups {
        let c = passing_elements(speed).unwrap();
        let e = &c.value;
        assert_eq!((e.speed_group_kmh, e.average_passing_speed_kmh, e.acceleration_kmh_per_s, e.t1_s, e.d1_m, e.t2_s, e.d2_m, e.d3_m, e.d4_m, e.total_m), (group, avg, a, t1, d1, t2, d2, d3, d4, total));
        assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page), ("Table 6.21", 132, 141));
        // The total is d1 + d2 + d3 + d4, d2 = 0.278 v t2 and d4 = 2/3 d2.
        assert_eq!(d1 + d2 + d3 + d4, total);
        close(passing_d2_m(avg, t2), d2, 2.0, "d2 formula");
        close(passing_d4_m(d2), d4, 2.0, "d4 = 2/3 d2");
    }
    assert!(passing_elements(65.5).is_err());
    assert!(passing_elements(40.0).is_err());
    assert!(passing_elements(115.0).is_err());
    // Table 6.22: minimum passing sight distance for design, same page: (design speed, passed, passing, PSD).
    let t622 = [
        (30.0, 29.0, 44.0, 217.0),
        (40.0, 36.0, 51.0, 285.0),
        (50.0, 44.0, 59.0, 345.0),
        (60.0, 51.0, 66.0, 407.0),
        (70.0, 59.0, 74.0, 482.0),
        (80.0, 65.0, 80.0, 541.0),
        (90.0, 73.0, 88.0, 605.0),
        (100.0, 79.0, 94.0, 670.0),
        (110.0, 85.0, 100.0, 728.0),
        (120.0, 91.0, 106.0, 792.0),
    ];
    for (v, passed, passing, psd) in t622 {
        let c = minimum_passing_sight_distance(v).unwrap();
        assert_eq!((c.value.passed_vehicle_speed_kmh, c.value.passing_vehicle_speed_kmh, c.value.passing_sight_distance_m), (passed, passing, psd));
        assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page), ("Table 6.22", 132, 141));
    }
    // "The minimum passing sight distance for a two-lane road is about four times as great as the minimum stopping sight
    // distance at the same speed" (p. 130): 541 against the high end 139.5 of the 80 km/h range is 3.9 times.
    close(541.0 / 139.5, 3.88, 0.01, "about four times");
    // Meeting sight distance (section 6.7.5, p. 133 (PDF p. 142)): two stopping sight distances plus 10.0 m.
    close(meeting_sight_distance_m(115.0, 115.0).value, 240.0, 1e-9, "meeting");
    assert_eq!(meeting_sight_distance_m(115.0, 115.0).source.printed_page, 133);
    // d1 formula (p. 131): 0.278 t1 (v - m + a t1 / 2): v 56.2, m 15, a 2.25, t1 3.6 -> 45.3 (the manual states no m; with 15 it matches 45)
    close(passing_d1_m(56.2, 15.0, 2.25, 3.6), 45.3, 0.1, "d1 with m = 15");
}

#[test]
fn cross_section_tables() {
    // Table 7-3: width of edge strip and edge line, section 7.3, p. 136 (PDF p. 145).
    for (v, strip, line) in [(120.0, 0.75, 0.15), (100.0, 0.45, 0.15), (80.0, 0.45, 0.10), (60.0, 0.20, 0.10)] {
        let c = edge_strip(RoadClass::Ia, v).unwrap();
        assert_eq!((c.value.edge_strip_width_m, c.value.edge_line_width_m), (strip, line), "{v}");
        assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page), ("Table 7-3", 136, 145));
    }
    assert!(edge_strip(RoadClass::II, 50.0).unwrap_err().to_string().contains("dash"));
    assert!(edge_strip(RoadClass::II, 90.0).unwrap_err().to_string().contains("not a row"));
    assert!(edge_strip(RoadClass::A, 60.0).is_err());
    // Table 7-1: additions to headroom at crests, section 7.2, p. 134 (PDF p. 143).
    for (r, add) in [(100.0, 0.12), (200.0, 0.06), (300.0, 0.04), (400.0, 0.03), (500.0, 0.02), (600.0, 0.02), (700.0, 0.02)] {
        let c = headroom_addition_at_crest_m(r).unwrap();
        assert_eq!(c.value, add, "{r}");
        assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page), ("Table 7-1", 134, 143));
    }
    assert!(headroom_addition_at_crest_m(250.0).is_err());
    assert!(headroom_addition_at_crest_m(800.0).is_err());
    // Lane, shoulder, cross fall by class (Table 4-2b p. 45; Tables 4-3 to 4-9).
    assert_eq!(lane_width_m(RoadClass::Ia).value, 3.65);
    assert_eq!(lane_width_m(RoadClass::III).value, 2.8);
    assert_eq!(lane_width_m(RoadClass::C).value, 4.0);
    assert_eq!(shoulder_width_m(RoadClass::Ia).value, 2.5);
    assert_eq!(shoulder_width_m(RoadClass::C).value, 1.2);
    assert_eq!(normal_cross_fall_percent(RoadClass::II).unwrap().value, 2.5);
    assert_eq!(normal_cross_fall_percent(RoadClass::A).unwrap().value, 4.0);
    assert_eq!(shoulder_cross_fall_percent(RoadClass::B).unwrap().value, 4.0);
    assert_eq!(class_cross_section(RoadClass::Ib).value.lane_width_m, 3.5);
    // Section 7.5, p. 137 (PDF p. 146).
    let r = cross_fall_rules();
    assert_eq!((r.paved_max_percent.value, r.paved_max_in_intense_rainfall_percent.value, r.paved_high_type_min_percent.value, r.unpaved_percent.value), (2.5, 3.0, 2.0, 4.0));
    assert_eq!(r.paved_max_percent.source.printed_page, 137);
    // Section 7.4, p. 137: shoulder slopes.
    assert_eq!(shoulder_slope_range_percent(ShoulderSurface::Paved).value, (Some(3.0), 6.0));
    assert_eq!(shoulder_slope_range_percent(ShoulderSurface::GravelOrCrushedRock).value, (Some(4.0), 6.0));
    assert_eq!(shoulder_slope_range_percent(ShoulderSurface::Turf).value, (None, 8.0));
}
