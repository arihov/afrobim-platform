// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Horizontal alignment tables and rules of the Uganda MoWT Road Design Manual Volume I (January 2010), and the
//! manual's worked examples. Page numbers are printed page (PDF page, printed + 9).

use afrobim_design::mowt::horizontal::*;
use afrobim_design::mowt::RoadClass;
use afrobim_design::{HorizontalDesign, Pi};

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: got {a}, expected {b} (tolerance {tol})");
}

fn dms(d: f64, m: f64, s: f64) -> f64 {
    d + m / 60.0 + s / 3600.0
}

const SPEEDS: [f64; 10] = [30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 110.0, 120.0];

#[test]
fn table_6_1_every_row_and_its_calculated_radius_from_the_formula() {
    // Table 6-1: minimum radii for horizontal curves on non-urban roads for emax 4, 6, 7, 8 % and the limiting f,
    // section 6.2.3, p. 80 (PDF p. 89). Columns: speed, emax, limiting f, e/100 + f, calculated radius, rounded radius.
    let f = [0.17, 0.17, 0.16, 0.15, 0.14, 0.14, 0.13, 0.12, 0.11, 0.09];
    let e_plus_f = [
        [0.21, 0.21, 0.20, 0.19, 0.18, 0.18, 0.17, 0.16, 0.15, 0.13],
        [0.23, 0.23, 0.22, 0.21, 0.20, 0.20, 0.19, 0.18, 0.17, 0.15],
        [0.24, 0.24, 0.23, 0.22, 0.21, 0.21, 0.20, 0.19, 0.18, 0.16],
        [0.25, 0.25, 0.24, 0.23, 0.22, 0.22, 0.21, 0.20, 0.19, 0.17],
    ];
    let calc = [
        [33.7, 60.0, 98.4, 149.2, 214.3, 280.0, 375.2, 492.1, 635.2, 872.2],
        [30.8, 54.8, 89.5, 135.0, 192.9, 252.0, 335.7, 437.4, 560.4, 755.9],
        [29.5, 52.5, 85.6, 128.8, 183.7, 240.0, 318.9, 414.4, 529.3, 708.7],
        [28.3, 50.4, 82.0, 123.2, 175.4, 229.1, 303.7, 393.7, 501.5, 667.0],
    ];
    let rounded = [
        [35.0, 60.0, 100.0, 150.0, 215.0, 280.0, 375.0, 490.0, 635.0, 870.0],
        [30.0, 55.0, 90.0, 135.0, 195.0, 250.0, 335.0, 440.0, 560.0, 755.0],
        [30.0, 55.0, 85.0, 130.0, 185.0, 240.0, 320.0, 415.0, 530.0, 710.0],
        [30.0, 50.0, 80.0, 125.0, 175.0, 230.0, 305.0, 400.0, 500.0, 670.0],
    ];
    for (ei, emax) in EMAX_OPTIONS_PERCENT.iter().enumerate() {
        for (si, v) in SPEEDS.iter().enumerate() {
            let r = min_radius(*v, *emax).unwrap();
            assert_eq!(r.value.limiting_f, f[si]);
            assert_eq!(r.value.e_plus_f, e_plus_f[ei][si], "e+f at {v}, {emax}");
            assert_eq!(r.value.calculated_radius_m, calc[ei][si]);
            assert_eq!(r.value.rounded_radius_m, rounded[ei][si]);
            assert_eq!((r.source.what, r.source.printed_page, r.source.pdf_page), ("Table 6-1", 80, 89));
            // The manual's relation R = V^2 / (127 (e + f)) reproduces the calculated column (rounding to 0.1 m).
            let formula = manual_min_radius_m(*v, emax / 100.0, f[si]);
            close(formula, calc[ei][si], 0.06, &format!("calculated radius at {v} km/h, emax {emax}"));
        }
    }
    // Refusals: a speed or emax the table does not have.
    assert!(min_radius(75.0, 7.0).unwrap_err().to_string().contains("not one of them"));
    assert!(min_radius(80.0, 5.0).unwrap_err().to_string().contains("4, 6, 7 and 8"));
    assert!(min_radius(130.0, 7.0).is_err());
    close(limiting_side_friction(80.0).unwrap().value, 0.14, 0.0, "f");
    close(comfort_side_friction().value, 0.22, 0.0, "comfort f");
}

#[test]
fn the_manuals_127_against_the_physics_derivation() {
    // Section 6.2.3, p. 79: R = V^2 / (127 (e + f)). The crate's physics uses 12.96 g = 127.0966.
    // V = 80, e = 7 %, f = 0.14: manual 239.97, rounded 240 (Table 6-1), physics 239.79. The table value is the limit.
    let manual = manual_min_radius_m(80.0, 0.07, 0.14);
    let physics = afrobim_design::physics::min_radius_m(80.0, 0.07, 0.14).unwrap();
    close(manual, 240.0, 0.05, "manual relation");
    close(physics, 239.79, 0.01, "physics relation");
    assert!(manual > physics && (manual - physics) / manual < 0.001);
    assert_eq!(min_radius(80.0, 7.0).unwrap().value.rounded_radius_m, 240.0);
}

#[test]
fn worked_example_minimum_radius_p79() {
    // Section 6.2.3, p. 79 (PDF p. 88): "VD = 80 km/hr, emax = 7 %, f = 0.14, find minimum radius." The manual prints
    // no answer (slip S24); the formula gives 80^2 / (127 x 0.21) = 6400 / 26.67 = 239.97, the Table 6-1 row (240).
    close(manual_min_radius_m(80.0, 0.07, 0.14), 6400.0 / (127.0 * 0.21), 1e-9, "formula");
    close(manual_min_radius_m(80.0, 0.07, 0.14), 240.0, 0.05, "answer");
}

#[test]
fn tables_6_2_and_6_3_every_row() {
    // Table 6-2, p. 92 (PDF p. 101): maximum relative gradient ratio, N:1.
    let t = [133.0, 143.0, 150.0, 167.0, 182.0, 200.0, 210.0, 210.0, 238.0, 250.0];
    for (v, r) in SPEEDS.iter().zip(t) {
        let c = max_relative_gradient_ratio(*v).unwrap();
        assert_eq!(c.value, r);
        assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page), ("Table 6-2", 92, 101));
    }
    assert!(max_relative_gradient_ratio(35.0).is_err());
    // Table 6-3, p. 93 (PDF p. 102): 2 lanes 1.0, 3 lanes 1.2, 4 lanes 1.5.
    assert_eq!(runoff_length_factor(2).unwrap().value, 1.0);
    assert_eq!(runoff_length_factor(3).unwrap().value, 1.2);
    assert_eq!(runoff_length_factor(4).unwrap().value, 1.5);
    assert_eq!(runoff_length_factor(4).unwrap().source.printed_page, 93);
    assert!(runoff_length_factor(6).unwrap_err().to_string().contains("2, 3 and 4 lanes"));
}

#[test]
fn runoff_formula_and_two_second_minimum() {
    // L = i x Lw x ratio (p. 92): V 50, e 4.2 %, lane 3.5: 0.042 x 3.5 x 150 = 22.05 m; 2 s at 50 km/h = 27.8 m governs.
    let r = runoff_length(50.0, 4.2, 3.5, 2).unwrap().value;
    close(r.by_relative_gradient_m, 22.05, 1e-9, "by gradient");
    close(r.two_second_minimum_m, 27.78, 0.01, "2 s");
    close(r.governing_m, 27.78, 0.01, "governing");
    // V 80, e 7 %, lane 3.5: 0.07 x 3.5 x 200 = 49.0 m governs over 44.4 m (Table 6-6 prints 49).
    let r = runoff_length(80.0, 7.0, 3.5, 2).unwrap().value;
    close(r.governing_m, 49.0, 1e-9, "V 80");
    // Four lanes: 1.5 times.
    let r4 = runoff_length(80.0, 7.0, 3.5, 4).unwrap().value;
    close(r4.by_relative_gradient_m, 73.5, 1e-9, "4 lanes");
    // Tangent runout of the examples: 2.5 x 28 / 4.2 = 16.67; 2.5 x 49 / 7 = 17.5.
    close(tangent_runout_length(2.5, 28.0, 4.2).unwrap().value, 16.6667, 1e-3, "example 1 runout");
    close(tangent_runout_length(2.5, 49.0, 7.0).unwrap().value, 17.5, 1e-9, "example 2 runout");
}

type Cell = (&'static str, f64, f64);

fn check_cells(emax: f64, nc: f64, name: &str, printed: u16, pdf: u16, cells: &[(f64, f64, Cell)]) {
    for (radius, speed, (kind, rate, l)) in cells {
        let c = superelevation(emax, nc, *speed, *radius).unwrap_or_else(|e| panic!("{name} R {radius} V {speed}: {e}"));
        let got = match c.value.rate {
            SuperelevationRate::NormalCrown => ("NC", 0.0),
            SuperelevationRate::RemoveAdverseCrown => ("RC", 0.0),
            SuperelevationRate::Rate(e) => ("E", e),
        };
        assert_eq!(got, (*kind, *rate), "{name} R {radius} V {speed}");
        assert_eq!(c.value.min_runoff_length_m, *l, "{name} L at R {radius} V {speed}");
        assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page), (name, printed, pdf));
    }
}

#[test]
fn table_6_4_emax_4_nc_2_5_cells() {
    // Table 6-4: superelevation rates and runoff, emax 4 %, normal crown 2.5 %, p. 98 (PDF p. 107).
    // (R, V, (NC/RC/E, rate %, L m)): first row, last rows, and interior rows in every speed group.
    let cells: Vec<(f64, f64, Cell)> = vec![
        (7000.0, 30.0, ("NC", 0.0, 0.0)),
        (7000.0, 120.0, ("NC", 0.0, 0.0)),
        (3000.0, 100.0, ("RC", 0.0, 56.0)),
        (3000.0, 120.0, ("RC", 0.0, 67.0)),
        (2000.0, 120.0, ("E", 2.8, 67.0)),
        (1500.0, 100.0, ("E", 2.6, 56.0)),
        (1500.0, 110.0, ("E", 2.9, 61.0)),
        (1000.0, 80.0, ("E", 2.5, 44.0)),
        (900.0, 120.0, ("E", 4.0, 67.0)),
        (500.0, 60.0, ("E", 2.7, 33.0)),
        (400.0, 50.0, ("E", 2.5, 28.0)),
        (250.0, 40.0, ("E", 2.6, 22.0)),
        (150.0, 30.0, ("E", 2.5, 17.0)),
        (100.0, 50.0, ("E", 4.0, 28.0)),
        (60.0, 40.0, ("E", 4.0, 22.0)),
        (40.0, 30.0, ("E", 3.9, 18.0)),
    ];
    check_cells(4.0, 2.5, "Table 6-4", 98, 107, &cells);
    // R = 30 m is the Rmin cell (35 m) of 30 km/h: below it, refused.
    assert!(superelevation(4.0, 2.5, 30.0, 30.0).unwrap_err().to_string().contains("below the minimum radius"));
    // Rmin cells as printed: 35, 60, 100, 150, 215, 280, 375, 490, 635, 870.
    let rmin = table_rmin(4.0, 2.5).unwrap().value;
    assert_eq!(rmin, [35.0, 60.0, 100.0, 150.0, 215.0, 280.0, 375.0, 490.0, 635.0, 870.0]);
}

#[test]
fn table_6_5_emax_4_nc_4_cells() {
    // Table 6-5: emax 4 %, normal crown 4 %, p. 99 (PDF p. 108). With a 4 % crown the adverse crown is removed (RC)
    // all the way to Rmin; only NC, RC and the lengths appear (R 60 at 40 km/h is the one printed rate, 4.0 %).
    let cells: Vec<(f64, f64, Cell)> = vec![
        (7000.0, 30.0, ("NC", 0.0, 0.0)),
        (3000.0, 120.0, ("RC", 0.0, 67.0)),
        (1300.0, 60.0, ("RC", 0.0, 33.0)),
        (900.0, 50.0, ("RC", 0.0, 28.0)),
        (500.0, 100.0, ("RC", 0.0, 56.0)),
        (300.0, 30.0, ("RC", 0.0, 19.0)),
        (300.0, 80.0, ("RC", 0.0, 44.0)),
        (60.0, 30.0, ("RC", 0.0, 19.0)),
        (60.0, 40.0, ("E", 4.0, 22.0)),
        (40.0, 30.0, ("RC", 0.0, 19.0)),
    ];
    check_cells(4.0, 4.0, "Table 6-5", 99, 108, &cells);
    assert_eq!(table_rmin(4.0, 4.0).unwrap().value, [35.0, 60.0, 100.0, 150.0, 215.0, 280.0, 375.0, 490.0, 635.0, 870.0]);
}

#[test]
fn table_6_6_emax_7_nc_2_5_cells() {
    // Table 6-6: emax 7 %, normal crown 2.5 %, p. 100 (PDF p. 109).
    let cells: Vec<(f64, f64, Cell)> = vec![
        (7000.0, 60.0, ("NC", 0.0, 0.0)),
        (3000.0, 100.0, ("RC", 0.0, 56.0)),
        (2500.0, 120.0, ("E", 2.8, 67.0)),
        (2000.0, 100.0, ("E", 2.6, 56.0)),
        (1500.0, 120.0, ("E", 4.4, 67.0)),
        (1000.0, 70.0, ("E", 2.7, 39.0)),
        (800.0, 120.0, ("E", 6.9, 67.0)),
        (700.0, 120.0, ("E", 7.0, 67.0)),
        (500.0, 90.0, ("E", 6.1, 50.0)),
        (400.0, 100.0, ("E", 7.0, 56.0)),
        (300.0, 50.0, ("E", 4.2, 28.0)),
        (300.0, 90.0, ("E", 7.0, 51.0)),
        (250.0, 70.0, ("E", 6.6, 42.0)),
        (250.0, 80.0, ("E", 7.0, 49.0)),
        (200.0, 50.0, ("E", 5.3, 28.0)),
        (175.0, 40.0, ("E", 4.6, 23.0)),
        (150.0, 60.0, ("E", 6.9, 40.0)),
        (119.0, 30.0, ("E", 4.2, 19.0)),
        (100.0, 30.0, ("E", 4.5, 21.0)),
        (100.0, 50.0, ("E", 6.9, 36.0)),
        (60.0, 40.0, ("E", 6.9, 35.0)),
        (40.0, 30.0, ("E", 6.7, 31.0)),
        (30.0, 30.0, ("E", 7.0, 33.0)),
    ];
    check_cells(7.0, 2.5, "Table 6-6", 100, 109, &cells);
    // The printed Rmin of the 7 % tables differ from Table 6-1 (slip S07): 30, 50, 90, 140, 200, 250, 300, 400, 500, 700.
    assert_eq!(table_rmin(7.0, 2.5).unwrap().value, [30.0, 50.0, 90.0, 140.0, 200.0, 250.0, 300.0, 400.0, 500.0, 700.0]);
    let t61: Vec<f64> = SPEEDS.iter().map(|v| min_radius(*v, 7.0).unwrap().value.rounded_radius_m).collect();
    assert_eq!(t61, vec![30.0, 55.0, 85.0, 130.0, 185.0, 240.0, 320.0, 415.0, 530.0, 710.0]);
}

#[test]
fn table_6_7_emax_7_nc_4_cells() {
    // Table 6-7: emax 7 %, normal crown 4 %, p. 101 (PDF p. 110).
    let cells: Vec<(f64, f64, Cell)> = vec![
        (7000.0, 120.0, ("NC", 0.0, 0.0)),
        (3000.0, 110.0, ("RC", 0.0, 61.0)),
        (1500.0, 120.0, ("E", 4.4, 67.0)),
        (1300.0, 110.0, ("E", 4.2, 61.0)),
        (1000.0, 100.0, ("E", 4.6, 56.0)),
        (700.0, 80.0, ("E", 4.3, 44.0)),
        (400.0, 30.0, ("RC", 0.0, 19.0)),
        (250.0, 80.0, ("E", 7.0, 49.0)),
        (200.0, 40.0, ("E", 4.3, 22.0)),
        (120.0, 30.0, ("E", 4.1, 19.0)),
        (30.0, 30.0, ("E", 7.0, 33.0)),
    ];
    check_cells(7.0, 4.0, "Table 6-7", 101, 110, &cells);
    assert_eq!(table_rmin(7.0, 4.0).unwrap().value, [30.0, 50.0, 90.0, 140.0, 200.0, 250.0, 300.0, 400.0, 500.0, 700.0]);
}

#[test]
fn table_6_8_emax_8_nc_2_5_cells() {
    // Table 6-8: emax 8 %, normal crown 2.5 %, p. 102 (PDF p. 111).
    let cells: Vec<(f64, f64, Cell)> = vec![
        (7000.0, 90.0, ("NC", 0.0, 0.0)),
        (3000.0, 120.0, ("RC", 0.0, 67.0)),
        (2500.0, 120.0, ("E", 2.9, 67.0)),
        (1400.0, 80.0, ("E", 2.5, 44.0)),
        (1000.0, 120.0, ("E", 6.5, 67.0)),
        (700.0, 120.0, ("E", 8.0, 70.0)),
        (600.0, 110.0, ("E", 7.7, 64.0)),
        (500.0, 100.0, ("E", 7.6, 59.0)),
        (400.0, 100.0, ("E", 8.0, 62.0)),
        (300.0, 60.0, ("E", 5.6, 33.0)),
        (300.0, 80.0, ("E", 7.6, 53.0)),
        (250.0, 80.0, ("E", 7.9, 56.0)),
        (200.0, 50.0, ("E", 5.8, 30.0)),
        (150.0, 40.0, ("E", 5.4, 27.0)),
        (100.0, 30.0, ("E", 5.0, 23.0)),
        (90.0, 50.0, ("E", 8.0, 42.0)),
        (40.0, 30.0, ("E", 7.5, 35.0)),
        (30.0, 30.0, ("E", 8.0, 37.0)),
    ];
    check_cells(8.0, 2.5, "Table 6-8", 102, 111, &cells);
    // Printed Rmin: 30, 50, 80, 125, 175, 230, 305, 395, 500, 655 (Table 6-1: 400 and 670 at 100 and 120 km/h, slip S08).
    assert_eq!(table_rmin(8.0, 2.5).unwrap().value, [30.0, 50.0, 80.0, 125.0, 175.0, 230.0, 305.0, 395.0, 500.0, 655.0]);
}

#[test]
fn table_6_9_emax_8_nc_4_cells() {
    // Table 6-9: emax 8 %, normal crown 4 %, p. 103 (PDF p. 112).
    let cells: Vec<(f64, f64, Cell)> = vec![
        (7000.0, 30.0, ("NC", 0.0, 0.0)),
        (3000.0, 110.0, ("RC", 0.0, 61.0)),
        (1500.0, 120.0, ("E", 4.6, 67.0)),
        (1200.0, 100.0, ("E", 4.1, 56.0)),
        (900.0, 90.0, ("E", 4.4, 50.0)),
        (700.0, 120.0, ("E", 8.0, 70.0)),
        (500.0, 100.0, ("E", 7.6, 59.0)),
        (300.0, 70.0, ("E", 6.7, 43.0)),
        (200.0, 50.0, ("E", 5.8, 30.0)),
        (130.0, 30.0, ("E", 4.2, 19.0)),
        (30.0, 30.0, ("E", 8.0, 37.0)),
    ];
    check_cells(8.0, 4.0, "Table 6-9", 103, 112, &cells);
    assert_eq!(table_rmin(8.0, 4.0).unwrap().value, [30.0, 50.0, 80.0, 125.0, 175.0, 230.0, 305.0, 395.0, 500.0, 655.0]);
}

#[test]
fn superelevation_tables_refuse_between_rows_below_rmin_and_unknown_tables() {
    // R = 320 is between the rows 400 and 300 of Table 6-6: refused, with the neighbours available.
    let e = superelevation(7.0, 2.5, 80.0, 320.0).unwrap_err().to_string();
    assert!(e.contains("not a row") && e.contains("no rule for interpolating"), "{e}");
    let (flatter, sharper) = superelevation_neighbours(7.0, 2.5, 80.0, 320.0).unwrap();
    assert_eq!((flatter.value.radius_m, sharper.value.radius_m), (400.0, 300.0));
    assert_eq!(flatter.value.rate, SuperelevationRate::Rate(6.0));
    assert_eq!(sharper.value.rate, SuperelevationRate::Rate(6.8));
    // Below Rmin (250 m at 80 km/h in Table 6-6).
    let e = superelevation(7.0, 2.5, 80.0, 200.0).unwrap_err().to_string();
    assert!(e.contains("Rmin = 250"), "{e}");
    // Above the first row.
    assert!(superelevation_neighbours(7.0, 2.5, 80.0, 9000.0).is_err());
    // emax 6 % has no superelevation table; 5 % neither.
    assert!(superelevation(6.0, 2.5, 80.0, 400.0).unwrap_err().to_string().contains("not tabulated"));
    assert!(superelevation(7.0, 3.0, 80.0, 400.0).is_err());
    assert!(superelevation(7.0, 2.5, 85.0, 400.0).is_err());
    assert_eq!(tabulated_radii().len(), 34);
    assert_eq!(tabulated_radii()[0], 7000.0);
    assert_eq!(tabulated_radii()[33], 30.0);
}

/// The whole of Tables 6-4 to 6-9, cell by cell, against the manual's own formulae. The minimum runoff length L is
/// the greater of `e x 3.5 x ratio` (Table 6-2; for NC 0 and for RC the crown rate) and the distance in 2 s; every
/// printed L matches within 1 m except four cells (slip S09). Superelevation does not decrease as the radius falls.
#[test]
fn every_cell_of_tables_6_4_to_6_9_reproduces_from_the_formula() {
    let mut exceptions: Vec<String> = Vec::new();
    let tables = [(4.0, 2.5), (4.0, 4.0), (7.0, 2.5), (7.0, 4.0), (8.0, 2.5), (8.0, 4.0)];
    let mut cells = 0;
    for (emax, nc) in tables {
        for v in SPEEDS {
            let ratio = max_relative_gradient_ratio(v).unwrap().value;
            let mut prev_rate = 0.0;
            for r in tabulated_radii() {
                let Ok(c) = superelevation(emax, nc, v, r) else { continue };
                cells += 1;
                let c = c.value;
                let (rate_for_l, numeric) = match c.rate {
                    SuperelevationRate::NormalCrown => (None, 0.0),
                    SuperelevationRate::RemoveAdverseCrown => (Some(nc), nc),
                    SuperelevationRate::Rate(e) => (Some(e), e),
                };
                assert!(numeric + 1e-9 >= prev_rate, "{} V {v}: rate falls from {prev_rate} to {numeric} at R {r}", c.table);
                prev_rate = numeric;
                assert!(numeric <= emax + 1e-9);
                let expected = match rate_for_l {
                    None => 0.0,
                    Some(e) => (e / 100.0 * 3.5 * ratio).max(two_second_distance_m(v)),
                };
                if (expected - c.min_runoff_length_m).abs() > 1.0 {
                    exceptions.push(format!("{} V {v} R {r}: printed {} formula {expected:.1}", c.table, c.min_runoff_length_m));
                }
            }
        }
    }
    // 202 + 202 + 214 + 214 + 216 + 216 printed cells with a rate or a crown (the Rmin cells and the empty cells excluded).
    assert_eq!(cells, 1264);
    assert_eq!(
        exceptions,
        vec![
            "Table 6-8 V 100 R 500: printed 59 formula 55.9".to_string(),
            "Table 6-8 V 100 R 400: printed 62 formula 58.8".to_string(),
            "Table 6-9 V 100 R 500: printed 59 formula 55.9".to_string(),
            "Table 6-9 V 100 R 400: printed 62 formula 58.8".to_string(),
        ]
    );
}

#[test]
fn table_6_10_widening_every_row() {
    // Table 6-10: widening on curves and on high fill, section 6.2.4, p. 104 (PDF p. 113).
    let rows: [(f64, f64, f64, f64); 9] = [
        (20.0, 0.6, 1.5, 0.0), // radius, single lane, two lane (third value unused)
        (40.0, 0.6, 1.5, 0.0),
        (41.0, 0.6, 1.2, 0.0),
        (60.0, 0.6, 1.2, 0.0),
        (61.0, 0.0, 0.9, 0.0),
        (120.0, 0.0, 0.9, 0.0),
        (121.0, 0.0, 0.6, 0.0),
        (250.0, 0.0, 0.6, 0.0),
        (251.0, 0.0, 0.0, 0.0),
    ];
    for (r, single, two, _) in rows {
        assert_eq!(curve_widening(r, WideningLanes::Single).unwrap().value, single, "single lane R {r}");
        assert_eq!(curve_widening(r, WideningLanes::Two).unwrap().value, two, "two lane R {r}");
    }
    assert_eq!(curve_widening(1500.0, WideningLanes::Two).unwrap().value, 0.0);
    let c = curve_widening(100.0, WideningLanes::Two).unwrap();
    assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page, c.unit), ("Table 6-10", 104, 113, "m"));
    // Below 20 m and in the gaps between bands: refused.
    assert!(curve_widening(15.0, WideningLanes::Two).unwrap_err().to_string().contains("starts at a radius of 20 m"));
    for gap in [40.5, 60.5, 120.5] {
        assert!(curve_widening(gap, WideningLanes::Two).unwrap_err().to_string().contains("falls between"), "{gap}");
    }
    // Fill widening: 0.0-3.0 0.3; 3.0-6.0 0.6; 6.0-9.0 0.9; over 9.0 0.9. Classes I, II, III, A, B only.
    let fill = |h: f64| fill_widening(RoadClass::II, h).map(|c| c.value);
    assert_eq!(fill(0.0).unwrap(), 0.3);
    assert_eq!(fill(2.5).unwrap(), 0.3);
    assert_eq!(fill(4.5).unwrap(), 0.6);
    assert_eq!(fill(7.5).unwrap(), 0.9);
    assert_eq!(fill(9.0).unwrap(), 0.9);
    assert_eq!(fill(12.0).unwrap(), 0.9);
    assert!(fill(3.0).unwrap_err().to_string().contains("two rows"));
    assert!(fill(6.0).is_err());
    assert!(fill(-1.0).is_err());
    assert!(fill_widening(RoadClass::C, 4.0).unwrap_err().to_string().contains("Class C"));
    assert_eq!(fill_widening(RoadClass::Ia, 4.0).unwrap().value, 0.6);
}

#[test]
fn spiral_rules_of_section_6_2_3() {
    // Transition required if R < V^3 / 432 (p. 88, PDF p. 97): V 80: 512000 / 432 = 1185.19 m.
    let t = transition_required(80.0, 250.0).value;
    close(t.threshold_radius_m, 1185.185, 0.001, "threshold");
    assert!(t.required);
    assert!(!transition_required(80.0, 1500.0).value.required);
    // L = 0.0702 V^3 / (R C): V 80, R 250, C 1: 0.0702 x 512000 / 250 = 143.77 m; C 3: 47.9 m.
    close(min_spiral_length_comfort(80.0, 250.0, 1.0).unwrap().value, 143.77, 0.01, "C 1");
    close(min_spiral_length_comfort(80.0, 250.0, 3.0).unwrap().value, 47.92, 0.01, "C 3");
    assert!(min_spiral_length_comfort(80.0, 250.0, 0.5).unwrap_err().to_string().contains("1 to 3"));
    // Clothoid, p. 89 (PDF p. 98): A = sqrt(v^3 / k), k 0.45: V 80 -> v 22.22 m/s: v^3 = 10973.9; /0.45 = 24386; A = 156.2 m.
    close(clothoid_parameter(80.0, CLOTHOID_K_STANDARD).unwrap().value, 156.16, 0.01, "A");
    // L = A^2 / R = 24386 / 250 = 97.5 m
    close(spiral_length_for_k(80.0, 250.0, 0.45).unwrap().value, 97.54, 0.01, "L");
    assert!(clothoid_parameter(80.0, 0.2).is_err() && clothoid_parameter(80.0, 0.7).is_err());
    // Figure 6-14 (p. 90): the curve for a speed limit of 50 km/h passes near (R 100, L 60): v^3 / (k R) = 2679 / (0.45 x 100) = 59.5.
    close(spiral_length_for_k(50.0, 100.0, 0.45).unwrap().value, 59.5, 0.1, "figure 6-14 point");
    // Aesthetic limits R/3 < A < R, 0.1 R < L < R, shift at least 0.25 m (p. 90).
    let a = clothoid_aesthetic_limits(300.0).value;
    assert_eq!((a.a_min_m, a.a_max_m, a.l_min_m, a.l_max_m), (100.0, 300.0, 30.0, 300.0));
    assert_eq!(min_circular_shift().value, 0.25);
}

#[test]
fn rules_stated_in_the_text() {
    let r = rules();
    assert_eq!(r.max_tangent_length_m.value, 4000.0); // 6.2.2, p. 79 (PDF p. 88)
    assert_eq!(r.compound_ratio_desirable.value, 1.5); // 6.2.3 i, p. 84 (PDF p. 93)
    assert_eq!(r.compound_ratio_maximum.value, 1.75);
    assert_eq!(r.isolated_curve_radius_factor.value, 1.5); // 6.2.3 ii, p. 85
    assert_eq!((r.switchback_radius_max_m.value, r.switchback_radius_min_m.value), (20.0, 10.0)); // 6.2.3 iii, p. 87
    assert_eq!(r.s_curve_parameter_ratio.value, 1.5); // 6.2.3 v, p. 91
    assert_eq!(r.max_tangent_length_m.source.printed_page, 79);
    assert_eq!(r.max_tangent_length_m.source.pdf_page, 88);
}

#[test]
fn worked_example_curve_elements_p82_to_83() {
    // Section 6.2.3, EXAMPLE 1, p. 83 (PDF p. 92): degree of curve D = 4 deg, PI station 5+053.87.
    // R = 1145.916 / 4 = 286.479; the working uses the deflection 23 deg 18' 02" (the givens say 20 deg 18' 02", slip S14).
    let d = dms(23.0, 18.0, 2.0);
    let r = 1145.916 / 4.0;
    close(r, 286.479, 1e-3, "R");
    close(degree_of_curve(r).value, 4.0, 1e-4, "D");
    let t = r * (d.to_radians() / 2.0).tan();
    close(t, 59.07, 0.01, "T"); // printed 59.07
    let l = 20.0 * d / 4.0;
    close(l, 116.50, 0.01, "L"); // printed 116.50 (20 x 23.30 / 4)
    close(5053.87 - t, 4994.80, 0.01, "PC"); // printed 4+994.8
    close(5053.87 - t + l, 5111.30, 0.01, "PT"); // printed 5+111.3
    // E: printed 4.55; R (sec(23.3/2) - 1) = 6.03; the printed value is the one for the 20 deg 18' 02" of the givens (half: 10 deg 09' 01"), 4.555.
    let e_printed_basis = r * (1.0 / (dms(20.0, 18.0, 2.0).to_radians() / 2.0).cos() - 1.0);
    close(e_printed_basis, 4.555, 0.001, "E from the givens' deflection");
    let e_exact = r * (1.0 / (d.to_radians() / 2.0).cos() - 1.0);
    close(e_exact, 6.03, 0.01, "E for 23 deg 18' 02\"");
    // The same curve through the crate's own geometry.
    let delta = d.to_radians();
    let h = HorizontalDesign::new("example", 5053.87 - 1000.0, vec![Pi::point(0.0, 0.0), Pi::curve(1000.0, 0.0, r), Pi::point(1000.0 + 1000.0 * delta.cos(), 1000.0 * delta.sin())]);
    let s = h.solve().unwrap();
    let c = s.curve_at_pi(1).unwrap();
    close(c.tangent_in_m, t, 1e-9, "crate T");
    close(c.total_length_m, 116.50, 0.01, "crate L");
    close(c.ts_station_m, 4994.80, 0.01, "crate PC");
    close(c.st_station_m, 5111.30, 0.01, "crate PT");
    close(c.external_m, e_exact, 1e-9, "crate E");
}

#[test]
fn worked_example_1_superelevation_simple_curve_p94_to_95() {
    // Section 6.2.3 vii, EXAMPLE 1, p. 94 to 95 (PDF p. 103 to 104): simple curve, delta 52 deg 27' 32", R 300 m,
    // V 50 km/h, roadway 7 m, normal crown 2.5 %, emax 7 %, PI at 10+684.231.
    let delta = dms(52.0, 27.0, 32.0);
    let t = 300.0 * (delta.to_radians() / 2.0).tan();
    close(t, 147.81, 0.01, "T"); // printed 147.81
    let lc = 20.0 * 52.459 / (1145.916 / 300.0);
    close(lc, 274.674, 0.005, "Lc"); // printed 274.674
    let pc = 10684.231 - 147.81;
    close(pc, 10536.421, 1e-9, "PC");
    let pt = pc + 274.674;
    close(pt, 10811.095, 1e-9, "PT");
    // "From table FSE = 4.2 %, Ls = 28 m": Table 6-6, V 50, R 300.
    let cell = superelevation(7.0, 2.5, 50.0, 300.0).unwrap().value;
    assert_eq!((cell.rate, cell.min_runoff_length_m), (SuperelevationRate::Rate(4.2), 28.0));
    let lra = tangent_runout_length(2.5, 28.0, 4.2).unwrap().value;
    close(lra, 16.67, 0.005, "Lra");
    let st = simple_curve_superelevation_stations(pc, pt, 28.0, lra).value;
    close(st.start_of_superelevation, 10501.081, 0.01, "start of superelevation"); // printed 10+501.081
    close(st.start_of_runoff, 10517.751, 0.01, "start of runoff"); // printed 10+517.751 (PC - 18.67)
    close(st.start_of_equal_superelevation, 10534.421, 0.01, "start of equal superelevation");
    close(st.start_of_full_superelevation, 10545.754, 0.01, "start of FSE");
    close(st.end_of_full_superelevation, 10801.762, 0.01, "end of FSE");
    close(st.end_of_runoff, 10829.765, 0.01, "end of runoff");
    // Printed "10+8219.765 - 16.67 = 10+813.095": 8219.765 is a typing slip for 829.765 (S16).
    close(st.end_of_equal_superelevation, 10813.095, 0.01, "end of equal superelevation");
    close(st.end_of_superelevation, 10846.435, 0.01, "end of superelevation"); // printed 10+846.435
    // The same curve through the crate's geometry: PC and PT.
    let h = HorizontalDesign::new("e1", 10684.231 - 1000.0, vec![Pi::point(0.0, 0.0), Pi::curve(1000.0, 0.0, 300.0), Pi::point(1000.0 + 1000.0 * delta.to_radians().cos(), 1000.0 * delta.to_radians().sin())]);
    let c = h.solve().unwrap().curves[0].clone();
    close(c.ts_station_m, pc, 0.01, "crate PC");
    close(c.st_station_m, pt, 0.01, "crate PT");
    close(c.pi_station_m, 10684.231, 1e-6, "crate PI");
}

#[test]
fn worked_example_2_superelevation_with_spirals_p95_to_96() {
    // EXAMPLE 2, p. 95 to 96 (PDF p. 104 to 105): delta 23 deg 18' 02", R 250 m, V 80 km/h, roadway 7 m, normal crown
    // 2.5 %, emax 7 %, PI at 30+270.759.
    let delta_deg = dms(23.0, 18.0, 2.0);
    let t = transition_required(80.0, 250.0).value;
    close(t.threshold_radius_m, 1185.185, 0.001, "V^3/432"); // printed 1185.185
    assert!(t.required);
    // "From table 6-7 FSE = 7 %, NC = 2.5 %, Ls = 49 m": the cell is the same in Tables 6-6 and 6-7 (slip S11).
    let cell = superelevation(7.0, 2.5, 80.0, 250.0).unwrap().value;
    assert_eq!((cell.rate, cell.min_runoff_length_m), (SuperelevationRate::Rate(7.0), 49.0));
    let cell7 = superelevation(7.0, 4.0, 80.0, 250.0).unwrap().value;
    assert_eq!((cell7.rate, cell7.min_runoff_length_m), (SuperelevationRate::Rate(7.0), 49.0));
    let ls = 49.0;
    let d = degree_of_curve(250.0).value;
    close(d, 4.5837, 1e-4, "D"); // printed 4.5836 (truncated)
    let theta_s = ls * 4.5836 / 40.0;
    close(theta_s, 5.615, 0.001, "theta s"); // printed 5.615
    // T = R tan(delta/2) + Ls/2
    let t_total = 250.0 * (delta_deg.to_radians() / 2.0).tan() + ls / 2.0;
    close(t_total, 76.045, 0.01, "T"); // printed 76.045
    // L = 20 (delta - 2 theta_s) / D: printed 53.188 reproduces with D = 4.5386 (the printed working), not with 4.5836 (S15).
    close(20.0 * (23.3 - 2.0 * 5.615) / 4.5386, 53.188, 0.002, "L as printed");
    close(20.0 * (23.3 - 2.0 * 5.615) / 4.5836, 52.66, 0.01, "L with D = 4.5836");
    let ts = 30270.759 - 76.045;
    close(ts, 30194.714, 1e-9, "TS");
    let sc = ts + ls;
    let cs = sc + 53.188;
    let st = cs + ls;
    close(sc, 30243.714, 1e-9, "SC");
    close(cs, 30296.902, 1e-9, "CS");
    close(st, 30345.902, 1e-9, "ST");
    let lra = tangent_runout_length(2.5, ls, 7.0).unwrap().value;
    close(lra, 17.5, 1e-9, "Lra");
    let s = spiral_curve_superelevation_stations(ts, sc, cs, st, lra).value;
    close(s.start_of_superelevation, 30177.214, 1e-6, "start of superelevation");
    close(s.start_of_equal_superelevation, 30212.214, 1e-6, "start of equal superelevation");
    close(s.start_of_full_superelevation, 30243.714, 1e-6, "start of FSE");
    close(s.end_of_full_superelevation, 30296.902, 1e-6, "end of FSE");
    close(s.end_of_equal_superelevation, 30328.402, 1e-6, "end of equal superelevation");
    close(s.end_of_superelevation, 30363.402, 1e-6, "end of superelevation");
    // The crate's exact spiral geometry: the total tangent is larger than the manual's approximation by about 0.1 m.
    let dr = delta_deg.to_radians();
    let h = HorizontalDesign::new("e2", 30270.759 - 1000.0, vec![Pi::point(0.0, 0.0), Pi::spiral_curve(1000.0, 0.0, 250.0, 49.0, 49.0), Pi::point(1000.0 + 1000.0 * dr.cos(), 1000.0 * dr.sin())]);
    let c = h.solve().unwrap().curves[0].clone();
    assert!(c.tangent_in_m > t_total && c.tangent_in_m - t_total < 0.2, "exact T {} against the manual's {t_total}", c.tangent_in_m);
    close(c.tangent_in_m, 76.12, 0.01, "exact T");
    close(c.arc_length_m, 52.66, 0.02, "exact circular arc between the spirals");
}

#[test]
fn worked_example_sight_distance_on_horizontal_curve_p105() {
    // Figure 6-21 and its example, p. 105 (PDF p. 114): V 80 km/h, R 500 m, deflection 25 deg.
    let ls = line_of_sight_m(500.0, 25.0);
    close(ls.value, 216.44, 0.005, "LS"); // printed 216.44 m
    let m_a = middle_ordinate_m(500.0, 25.0);
    close(m_a.value, 11.85, 0.005, "M [a]"); // printed 11.85 m
    // [b] with S = 126 m: 500 (1 - cos(28.65 x 126 / 500)) = 3.96; the printed 3.94 follows 28.55 (slip S17).
    let m_b = required_middle_ordinate_m(500.0, 126.0);
    close(m_b.value, 3.964, 0.001, "M [b]");
    close(500.0 * (1.0 - (28.55_f64 * 126.0 / 500.0).to_radians().cos()), 3.94, 0.005, "printed value with 28.55");
    // The available clearance [a] exceeds the required [b]: "11.85 >> 3.94 design is OK".
    assert!(m_a.value > m_b.value);
    assert_eq!((m_b.source.what, m_b.source.printed_page, m_b.source.pdf_page), ("Figure 6-21", 105, 114));
    // 28.65 is 90 / pi.
    close(28.65, 90.0 / std::f64::consts::PI, 0.003, "28.65");
}

/// Tables 6-5, 6-7 and 6-9 (normal crown 4 %) against 6-4, 6-6 and 6-8 (normal crown 2.5 %): where the rate is above
/// 4 % the two tables print the same cell; where it is not above 4 % the 4 % table prints RC (adverse crown removed).
/// Two cells print the rate 4.0 % where that convention gives RC (slip S25).
#[test]
fn the_two_normal_crown_tables_of_each_emax_agree() {
    let mut exceptions = Vec::new();
    for emax in [4.0, 7.0, 8.0] {
        for v in SPEEDS {
            for r in tabulated_radii() {
                let a = superelevation(emax, 2.5, v, r);
                let b = superelevation(emax, 4.0, v, r);
                match (a, b) {
                    (Err(_), Err(_)) => {}
                    (Ok(a), Ok(b)) => {
                        let (a, b) = (a.value, b.value);
                        match a.rate {
                            SuperelevationRate::NormalCrown => assert_eq!(b.rate, SuperelevationRate::NormalCrown),
                            SuperelevationRate::RemoveAdverseCrown => assert_eq!(b.rate, SuperelevationRate::RemoveAdverseCrown),
                            SuperelevationRate::Rate(e) if e > 4.0 => assert_eq!((b.rate, b.min_runoff_length_m), (a.rate, a.min_runoff_length_m), "{emax} {v} {r}"),
                            SuperelevationRate::Rate(e) => {
                                if b.rate != SuperelevationRate::RemoveAdverseCrown {
                                    exceptions.push(format!("emax {emax} V {v} R {r}: {e} % against {:?}", b.rate));
                                }
                            }
                        }
                    }
                    (a, b) => panic!("one table has a cell and the other not: emax {emax} V {v} R {r}: {a:?} {b:?}"),
                }
            }
        }
    }
    assert_eq!(exceptions, vec!["emax 4 V 40 R 60: 4 % against Rate(4.0)".to_string(), "emax 8 V 90 R 1000: 4 % against Rate(4.0)".to_string()]);
}
