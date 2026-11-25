// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Golden tests: the 1 x 1200 mm CPC at Km 39+136 (Iganga-Bulopa-Kamuli road, Uganda).
//!
//! Tests named `documented_difference_*` pin a figure we do not reproduce. The full reference bar
//! bending schedule is `docs/reference/culvert/km39136-1x1200-cpc-bbs.csv` (85 rows).

use afrobim_culvert::*;

fn reference() -> CulvertDesign {
    design(&CulvertInput::reference()).expect("the reference culvert designs")
}

/// The reference with the barrel split of the drawing, 8347 mm left and 8653 mm right of the centreline.
fn reference_split() -> CulvertInput {
    let mut i = CulvertInput::reference();
    i.length = LengthRule::LeftRight {
        left_mm: 8347.0,
        right_mm: 8653.0,
    };
    i
}

fn near(what: &str, got: f64, want: f64, tol: f64) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: got {got}, drawing {want}, tolerance {tol}"
    );
}

fn within_percent(what: &str, got: f64, want: f64, pct: f64) {
    let diff = (got - want).abs() / want * 100.0;
    assert!(
        diff <= pct,
        "{what}: got {got}, drawing {want}, {diff:.2} % apart (limit {pct} %)"
    );
}

#[test]
fn geometry_of_the_end_structures() {
    let d = reference();
    let l = &d.layout;
    for e in [&l.left, &l.right] {
        near("headwall width", e.headwall_width_mm, 1850.0, 0.01);
        near("headwall height", e.spec.headwall_height_mm, 2000.0, 0.01);
        near(
            "headwall above the apron",
            e.spec.headwall_above_apron_mm,
            1500.0,
            0.01,
        );
        near("headwall below the apron", e.below_apron_mm, 500.0, 0.01);
        near(
            "headwall thickness",
            e.spec.headwall_thickness_mm,
            200.0,
            0.01,
        );
        near("wing reach in elevation", e.wing_spread_mm, 2310.0, 0.5);
        near("wing length", e.spec.wing.length_mm, 3267.0, 0.01);
        near(
            "overall width of the end structure",
            e.overall_width_mm,
            6753.0,
            1.0,
        );
        near("apron plus toe", e.apron_and_toe_mm, 2310.0, 0.01);
        near("toe beam total depth", e.toe_total_depth_mm, 500.0, 0.01);
        near("weep hole diameter", e.spec.weep.diameter_mm, 75.0, 0.01);
    }
    // 2310 + 17000 + 2310 overall, within the barrel tolerance below.
    near(
        "overall length",
        l.left.apron_and_toe_mm + l.barrel_len_mm + l.right.apron_and_toe_mm,
        21620.0,
        50.0,
    );
}

#[test]
fn barrel_length_within_50_mm_and_the_split_documented() {
    let d = reference();
    let l = &d.layout;
    near("barrel length", l.barrel_len_mm, 17000.0, 50.0);
    // The drawing's split is 8347 left and 8653 right. A single side slope that gives the total
    // gives 8276 and 8727: the drawing's 306 mm of asymmetry would need a 1:1.2 slope, which does
    // not give 17 000 mm. The difference is documented in the README; the split is not forced.
    near("left length", l.left_len_mm, 8347.0, 100.0);
    near("right length", l.right_len_mm, 8653.0, 100.0);
    assert!(l.left_len_mm < l.right_len_mm);
}

#[test]
fn inverts_and_gradient() {
    let l = reference().layout;
    near("inlet invert", l.invert_left_m, 1107.595, 1e-9);
    near("outlet invert", l.invert_right_m, 1107.340, 1e-9);
    // The drawing's centre invert 1107.470 follows from its end inverts and its split.
    near("centre invert", l.invert_cl_m, 1107.470, 0.001);
    let split = design(&reference_split()).expect("design").layout;
    near(
        "centre invert, drawing split",
        split.invert_cl_m,
        1107.470,
        0.0005,
    );
    // The drawing says 1.0 % but its inverts over 17 m give 1.5 %: we report what the levels give.
    near(
        "gradient from the drawing's inverts",
        l.gradient * 100.0,
        1.5,
        0.01,
    );
}

#[test]
fn setting_out_points_from_point_1_and_from_point_2() {
    let want: [(f64, f64); 6] = [
        (526022.2344, 94396.1058),
        (526019.3247, 94379.3571),
        (526025.9323, 94397.6646),
        (526019.2785, 94398.8202),
        (526015.6268, 94377.7983),
        (526022.2803, 94376.6423),
    ];
    let (p1, p2) = (want[0], want[1]);
    let bearing = (p2.0 - p1.0)
        .atan2(p2.1 - p1.1)
        .to_degrees()
        .rem_euclid(360.0);
    let road_bearing = bearing - 90.0;
    let u = (bearing.to_radians().sin(), bearing.to_radians().cos());
    // The drawing's own split, anchored on point 1 and on point 2.
    for anchor in [1, 2] {
        let mut i = reference_split();
        let cl = if anchor == 1 {
            (p1.0 + 8.347 * u.0, p1.1 + 8.347 * u.1)
        } else {
            (p2.0 - 8.653 * u.0, p2.1 - 8.653 * u.1)
        };
        i.setting_out = Some(SettingOutInput {
            cl_easting_m: cl.0,
            cl_northing_m: cl.1,
            road_bearing_deg: road_bearing,
        });
        let d = design(&i).expect("design");
        assert_eq!(d.setting_out.len(), 6);
        for (p, w) in d.setting_out.iter().zip(want) {
            let dist = ((p.easting_m - w.0).powi(2) + (p.northing_m - w.1).powi(2)).sqrt() * 1000.0;
            assert!(
                dist <= 5.0,
                "point {} from point {anchor}: {dist:.2} mm off (got {:.4}, {:.4})",
                p.number,
                p.easting_m,
                p.northing_m
            );
        }
    }
    // With the barrel length found from the embankment (17 003 mm) the points are within 5 mm
    // when anchored on point 1.
    let mut i = CulvertInput::reference();
    let l = design(&i).expect("design").layout;
    let cl = (
        p1.0 + l.left_len_mm / 1000.0 * u.0,
        p1.1 + l.left_len_mm / 1000.0 * u.1,
    );
    i.setting_out = Some(SettingOutInput {
        cl_easting_m: cl.0,
        cl_northing_m: cl.1,
        road_bearing_deg: road_bearing,
    });
    for (p, w) in design(&i).expect("design").setting_out.iter().zip(want) {
        let dist = ((p.easting_m - w.0).powi(2) + (p.northing_m - w.1).powi(2)).sqrt() * 1000.0;
        assert!(
            dist <= 5.0,
            "point {} with the computed length: {dist:.2} mm off",
            p.number
        );
    }
}

#[test]
fn payable_quantities_within_5_percent() {
    let q = reference_quantities();
    within_percent("1200 CPC length", q.pipe_length_m, 17.0, 5.0);
    within_percent(
        "C20 blinding under the pipe",
        q.barrel_blinding_m3,
        1.530,
        5.0,
    );
    within_percent("C20 bedding", q.bedding_m3, 6.120, 5.0);
    within_percent("C20 surround", q.surround_m3, 31.817, 5.0);
    within_percent("end structures", q.end_structures_m3, 9.646, 5.0);
    near("weep holes", q.weep_holes_m, 0.8, 1e-9);
    // The pieces of the end structures, for the record.
    near("headwalls", q.headwalls_m3, 2.0 * 1.85 * 0.2 * 2.0, 1e-9);
    near("wings", q.wings_m3, 4.0 * 0.2 * 3.267 * 1.25, 1e-3);
}

fn reference_quantities() -> Quantities {
    design(&reference_split()).expect("design").quantities
}

#[test]
fn documented_difference_end_structure_blinding() {
    // The drawing pays 7.360 m3 of C20 blinding to the end structures. At 50 mm thick that is
    // 147 m2, about six times the footprint of both end structures (about 23 m2), so the drawing's
    // thickness or its scope differs from what it shows. We measure 50 mm under the footprint.
    let q = reference_quantities();
    near(
        "footprint of both end structures",
        q.left.footprint_m2 + q.right.footprint_m2,
        22.8,
        0.5,
    );
    near("blinding at 50 mm", q.end_blinding_m3, 1.14, 0.02);
    assert!(
        (q.end_blinding_m3 - 7.360).abs() / 7.360 > 0.5,
        "if this now matches, update the README"
    );
}

#[test]
fn documented_difference_steel_in_the_bbs_and_the_payable_table() {
    // The drawing's payable table lists 1.329 t of steel; its own bar bending schedule totals
    // 1498.176 kg. The engine follows the schedule, so the two differ by about 169 kg.
    let d = reference();
    let kg = d.bbs.total_kg;
    near("BBS total", kg, 1498.176, 0.001);
    assert!(1329.0 < kg);
    near(
        "steel lines add up to the BBS",
        d.quantities
            .lines
            .iter()
            .filter(|l| l.category == "culvert.steel")
            .map(|l| l.quantity)
            .sum::<f64>(),
        kg,
        1e-6,
    );
}

struct Golden {
    mark: &'static str,
    per_element: u32,
    elements: u32,
    total: u32,
    cut: f64,
    code: u16,
    dims: [Option<f64>; 6],
    kg: f64,
}

#[allow(clippy::too_many_arguments)]
fn g(
    mark: &'static str,
    per_element: u32,
    elements: u32,
    total: u32,
    cut: f64,
    code: u16,
    dims: &[f64],
    kg: f64,
) -> Golden {
    let mut d = [None; 6];
    for (i, v) in dims.iter().enumerate() {
        if !v.is_nan() {
            d[i] = Some(*v);
        }
    }
    Golden {
        mark,
        per_element,
        elements,
        total,
        cut,
        code,
        dims: d,
        kg,
    }
}

fn check(d: &CulvertDesign, w: &Golden) {
    let r = d
        .bbs
        .rows
        .iter()
        .find(|r| r.mark == w.mark)
        .unwrap_or_else(|| panic!("row {} missing", w.mark));
    assert_eq!(
        (r.bars_per_element, r.elements, r.total_bars),
        (w.per_element, w.elements, w.total),
        "{} counts",
        w.mark
    );
    assert_eq!(r.cutting_length_mm, w.cut, "{} cutting length", w.mark);
    assert_eq!(r.shape_code, w.code, "{} code", w.mark);
    assert_eq!(r.dims, w.dims, "{} dimensions", w.mark);
    assert!(
        (r.mass_kg - w.kg).abs() < 0.0006,
        "{} weight {} against {}",
        w.mark,
        r.mass_kg,
        w.kg
    );
}

#[test]
fn bbs_rows_read_from_bs_8666() {
    // Shapes 11, 21, 51 and 99 with the minimum bend radius of BS 8666:2020, cutting lengths
    // rounded to 10 mm as the drawing does.
    let d = reference();
    let n = f64::NAN;
    for w in [
        g("01a", 4, 4, 16, 650.0, 200, &[100.0, 500.0, 100.0], 9.235),
        g(
            "01l",
            4,
            4,
            16,
            2520.0,
            200,
            &[100.0, 2370.0, 100.0],
            35.804,
        ),
        g(
            "02",
            18,
            4,
            72,
            2580.0,
            200,
            &[100.0, 2430.0, 100.0],
            164.955,
        ),
        g("03a", 2, 4, 8, 2530.0, 200, &[100.0, 2380.0, 100.0], 17.973),
        g("03k", 2, 4, 8, 6610.0, 200, &[170.0, 6440.0, 50.0], 46.957),
        g("06", 28, 4, 112, 880.0, 111, &[100.0, 800.0], 87.521),
        g("06Ba", 1, 4, 4, 1540.0, 200, &[710.0, 170.0, 710.0], 5.470),
        g("07", 8, 2, 16, 2000.0, 111, &[100.0, 1920.0], 28.416),
        g("08a", 2, 2, 4, 1980.0, 500, &[768.0, 120.0, 160.0], 7.033),
        g("10", 4, 2, 8, 1670.0, 200, &[800.0, 120.0, 800.0], 11.864),
        g("11a", 1, 2, 2, 1180.0, 500, &[404.0, 120.0, 105.0], 0.932),
        g("12", 2, 2, 4, 710.0, 500, &[168.0, 120.0, 105.0], 1.122),
        g("17", 5, 2, 10, 630.0, 111, &[150.0, 500.0], 2.489),
        g("13", 6, 2, 12, 6930.0, 200, &[150.0, 6673.0, 150.0], 73.846),
        g(
            "add1",
            11,
            2,
            22,
            790.0,
            900,
            &[250.0, 70.0, 250.0, 70.0, 250.0],
            15.433,
        ),
    ] {
        check(&d, &w);
    }
    let _ = n;
}

#[test]
fn rows_with_the_drawings_own_shape_codes() {
    // Shape codes 202 and 213, the Y10 link (mark 14) and the half-round bar (its 14.6 mm offset
    // is an input, not an allowance).
    let d = reference();
    let n = f64::NAN;
    for w in [
        g(
            "04a",
            3,
            4,
            12,
            3490.0,
            202,
            &[170.0, 3244.0, 100.0],
            37.189,
        ),
        g("05Aa", 1, 4, 4, 560.0, 202, &[170.0, 310.0, 100.0], 1.989),
        g("16", 2, 4, 8, 3790.0, 202, &[3561.0, 151.0, 100.0], 18.707),
        g(
            "09a",
            5,
            2,
            10,
            2480.0,
            213,
            &[300.0, 300.0, n, n, 1883.0],
            22.022,
        ),
        g("14", 23, 2, 46, 1240.0, 500, &[420.0, 120.0, 130.0], 35.194),
        g("15", 4, 2, 8, 2250.0, 117, &[2245.0], 7.110),
    ] {
        check(&d, &w);
    }
    let defs = shape_defs();
    assert_eq!(defs.len(), 9);
    assert!(defs.iter().all(|s| !s.formula.is_empty()));
}

#[test]
fn bs_8666_formulas_before_rounding() {
    let dims = |v: &[f64]| {
        let mut d = [None; 6];
        for (i, x) in v.iter().enumerate() {
            if !x.is_nan() {
                d[i] = Some(*x);
            }
        }
        d
    };
    let n = f64::NAN;
    let raw = |code, v: &[f64], dia| raw_cutting_length_mm(code, &dims(v), dia).expect("shape");
    // r = 2 d and b = 0.5 r + d = 24 for Y12.
    assert_eq!(raw(100, &[500.0], 12.0), 500.0);
    assert_eq!(raw(117, &[2245.0], 8.0), 2245.0);
    assert_eq!(raw(111, &[100.0, 800.0], 12.0), 876.0);
    assert_eq!(raw(113, &[300.0, n, 523.0], 12.0), 823.0);
    assert_eq!(raw(200, &[100.0, 500.0, 100.0], 12.0), 652.0);
    assert_eq!(raw(202, &[170.0, 3244.0, 100.0], 12.0), 3490.0);
    assert_eq!(raw(213, &[300.0, 300.0, n, n, 1883.0], 12.0), 2483.0);
    // Link: 2 (A + B) + 2 C - 2.5 r - 5 d.
    assert_eq!(
        raw(500, &[168.0, 120.0, 105.0], 8.0),
        2.0 * 288.0 + 210.0 - 40.0 - 40.0
    );
    // Four-bend crank: sum of A to E less 4 b.
    assert_eq!(
        raw(900, &[250.0, 70.0, 250.0, 70.0, 250.0], 12.0),
        890.0 - 96.0
    );
    // Y20 uses a former of 7 d: r = 3.5 d.
    assert_eq!(min_bend_radius_mm(20.0), 70.0);
    assert_eq!(raw(111, &[500.0, 800.0], 20.0), 1300.0 - 35.0 - 20.0);
    // A shape with a missing dimension is refused, and so is an unknown code.
    assert!(raw_cutting_length_mm(200, &dims(&[100.0, 500.0]), 12.0).is_err());
    assert!(raw_cutting_length_mm(213, &dims(&[300.0, 300.0, n, 1883.0]), 12.0).is_err());
    assert!(raw_cutting_length_mm(999, &dims(&[1.0]), 12.0).is_err());
}

#[test]
fn unit_masses_and_weights() {
    for (d, m) in [
        (8, 0.395),
        (10, 0.617),
        (12, 0.888),
        (16, 1.579),
        (20, 2.466),
        (25, 3.854),
    ] {
        assert_eq!(unit_mass_kg_per_m(d), Some(m));
    }
    let d = reference();
    let sum: f64 = d.bbs.rows.iter().map(|r| r.mass_kg).sum();
    near(
        "grand total is the sum of the rows",
        d.bbs.total_kg,
        sum,
        1e-9,
    );
    let by_d: f64 = d.bbs.by_diameter_kg.values().sum();
    near("totals by diameter", by_d, sum, 1e-9);
    // The csv carries the same grand total.
    let csv = d.bbs.to_csv();
    assert!(csv
        .lines()
        .next()
        .is_some_and(|h| h.starts_with("element,bar mark")));
    assert!(csv.contains(&format!("Grand total,,,,,,,,,,,,,{:.3}", d.bbs.total_kg)));
    assert!(csv.contains("Total Y12"));
}

#[test]
fn the_lettered_series_run_in_full() {
    // 01a to 01l step B by 170 mm; 03a to 03k are the eleven apron bars of the 03 series.
    let d = reference();
    let marks: Vec<&str> = d.bbs.rows.iter().map(|r| r.mark.as_str()).collect();
    assert_eq!(marks.iter().filter(|m| m.starts_with("01")).count(), 12);
    assert_eq!(marks.iter().filter(|m| m.starts_with("03")).count(), 11);
    let b: Vec<f64> = d
        .bbs
        .rows
        .iter()
        .filter(|r| r.mark.starts_with("01"))
        .map(|r| r.dims[1].unwrap_or(0.0))
        .collect();
    assert_eq!(b[..4], [500.0, 670.0, 840.0, 1010.0]);
}

#[test]
fn section_d_d_widths_and_cover() {
    let d = reference();
    let b = CulvertInput::reference().bedding;
    // 200 | 100 | 1200 | 100 | 200 with the blinding to 1900 in the drawing: our blinding is as wide as the surround, 1800.
    assert_eq!(b.surround_side_mm, 200.0);
    near("surround width", d.layout.surround_width_mm, 1800.0, 1e-9);
    assert_eq!(b.bedding_mm, 200.0);
    assert_eq!(b.blinding_mm, 50.0);
    // The drawing's D-D uses another datum (cover 873); ours is computed from the long-section levels.
    near("cover at the centreline", d.layout.cover_cl_mm, 1424.0, 1.0);
}

/// One row of the reference schedule, read from the CSV.
struct RefRow {
    mark: String,
    dia: u32,
    bars_per_element: u32,
    elements: u32,
    total: u32,
    cut: f64,
    code: u16,
    dims: [Option<f64>; 6],
    kg: f64,
}

/// The 85-row reference. A dimension that is a letter in the CSV (a label of the drawing's
/// sketch, such as `C` or `D`) is not a length and reads as `None`.
fn reference_rows() -> Vec<RefRow> {
    const CSV: &str = include_str!("../../../../docs/reference/culvert/km39136-1x1200-cpc-bbs.csv");
    CSV.lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            assert_eq!(f.len(), 17, "row {l}");
            let num = |i: usize| f[i].parse::<f64>().ok();
            let mut dims = [None; 6];
            for (k, slot) in dims.iter_mut().enumerate() {
                *slot = num(10 + k);
            }
            RefRow {
                mark: f[1].to_string(),
                dia: f[4].trim_start_matches('Y').parse().expect("diameter"),
                bars_per_element: f[5].parse().expect("bars per element"),
                elements: f[6].parse().expect("elements"),
                total: f[7].parse().expect("total"),
                cut: f[8].parse().expect("cutting length"),
                code: f[9].parse().expect("code"),
                dims,
                kg: f[16].parse().expect("weight"),
            }
        })
        .collect()
}

#[test]
fn reference_csv_cutting_lengths_and_masses_are_exact() {
    let rows = reference_rows();
    assert_eq!(rows.len(), 85);
    let table = ReinforcementTable::reference();
    let mut total = 0.0;
    for r in &rows {
        let raw = raw_cutting_length_mm(r.code, &r.dims, r.dia as f64)
            .unwrap_or_else(|e| panic!("{}: {e}", r.mark));
        // Nearest 10 mm, halves up.
        let cut = (raw / table.rounding_mm + 0.5).floor() * table.rounding_mm;
        assert_eq!(cut, r.cut, "{} cutting length (raw {raw})", r.mark);
        assert_eq!(r.bars_per_element * r.elements, r.total, "{} count", r.mark);
        let unit = unit_mass_kg_per_m(r.dia).expect("unit mass");
        let kg = (r.total as f64 * cut / 1000.0 * unit * 1000.0).round() / 1000.0;
        assert!(
            (kg - r.kg).abs() < 1e-9,
            "{} mass {kg} against {}",
            r.mark,
            r.kg
        );
        total += r.kg;
    }
    near("CSV grand total", total, 1498.176, 0.001);
}

#[test]
fn the_engine_reproduces_the_reference_schedule() {
    let rows = reference_rows();
    let d = reference();
    assert_eq!(d.bbs.rows.len(), rows.len(), "row count");
    for (e, r) in d.bbs.rows.iter().zip(&rows) {
        assert_eq!(e.mark, r.mark, "mark order");
        assert_eq!(e.diameter_mm, r.dia, "{} bar size", r.mark);
        assert_eq!(e.shape_code, r.code, "{} shape", r.mark);
        assert_eq!(e.dims, r.dims, "{} dimensions", r.mark);
        assert_eq!(e.cutting_length_mm, r.cut, "{} cutting length", r.mark);
        assert_eq!(e.total_bars, r.total, "{} total bars", r.mark);
        assert!(
            (e.mass_kg - r.kg).abs() < 1e-9,
            "{} mass {} against {}",
            r.mark,
            e.mass_kg,
            r.kg
        );
        // The schedule's two count columns read the other way round from the engine's: the CSV
        // gives the elements first, then the bars in each. Only add4 (20 bars shared by the two
        // ends) cannot be split in the same way.
        if r.mark != "add4" {
            assert_eq!(
                (e.bars_per_element, e.elements),
                (r.elements, r.bars_per_element),
                "{} bars per element and elements",
                r.mark
            );
        }
    }
    near("BBS total", d.bbs.total_kg, 1498.176, 0.001);
}
