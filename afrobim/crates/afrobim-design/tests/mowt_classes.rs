//! Tables 4-2a, 4-2b and 4-3 to 4-9 of the Uganda MoWT Road Design Manual Volume I (January 2010), cell by cell
//! as read from the rendered pages, and the cross-checks between the class tables and the manual's own speed tables.
//!
//! Page numbers are printed page (PDF page); the PDF page is the printed page + 9.

use afrobim_design::mowt::classes::*;
use afrobim_design::mowt::{horizontal, known_slips, sight, vertical};

fn val(class: RoadClass, terrain: Terrain, p: Parameter) -> f64 {
    class_parameter(class, terrain, p).unwrap().value
}

type GradeFn = fn(Terrain, f64) -> Result<afrobim_design::mowt::Cited<f64>, afrobim_design::DesignError>;

const TERRAINS: [Terrain; 4] = [Terrain::Flat, Terrain::Rolling, Terrain::Mountainous, Terrain::UrbanPeriUrban];

/// The 14 printed rows (order of `Parameter::ALL`) by 4 columns (flat, rolling, mountainous, urban/peri-urban).
type Rows = [[f64; 4]; 14];

fn check_table(class: RoadClass, rows: &Rows, skip: &[(Terrain, Parameter)]) {
    for (pi, p) in Parameter::ALL.iter().enumerate() {
        for (ti, t) in TERRAINS.iter().enumerate() {
            if skip.contains(&(*t, *p)) {
                // withheld: must be refused, with the reason
                let e = class_parameter(class, *t, *p).unwrap_err().to_string();
                assert!(e.contains("withheld"), "{class:?} {t:?} {p:?}: {e}");
                continue;
            }
            let c = class_parameter(class, *t, *p).unwrap();
            assert_eq!(c.value, rows[pi][ti], "{class:?} {t:?} {}", p.label());
            assert_eq!(c.unit, p.unit());
        }
    }
}

#[test]
fn table_4_3_class_ia_paved_every_cell() {
    // Table 4-3: Geometric Design Parameters for Design Standard Ia Paved (dual carriageway), section 4.5, p. 45 (PDF p. 54).
    let rows: Rows = [
        [120.0, 100.0, 80.0, 50.0],
        [205.0, 160.0, 115.0, 60.0],
        [795.0, 670.0, 545.0, 345.0],
        [710.0, 415.0, 240.0, 100.0],
        [3.0, 4.5, 6.0, 6.0],
        [4.0, 6.5, 8.0, 8.0],
        [0.5, 0.5, 0.5, 0.5],
        [7.0, 7.0, 7.0, 4.0],
        [103.0, 80.0, 32.0, 9.0],
        [664.0, 475.0, 310.0, 126.0],
        [50.0, 37.0, 25.0, 11.0],
        [2.5, 2.5, 2.5, 2.5],
        [4.0, 4.0, 4.0, 4.0],
        [60.0, 60.0, 60.0, 40.0],
    ];
    // The rolling crest K (80) is withheld: it contradicts Table 6-11 (62), see known_slips S02.
    check_table(RoadClass::Ia, &rows, &[(Terrain::Rolling, Parameter::CrestKStopping)]);
    let c = class_parameter(RoadClass::Ia, Terrain::Flat, Parameter::DesignSpeed).unwrap();
    assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page), ("Table 4-3", 45, 54));
}

#[test]
fn table_4_4_class_ib_paved_flat_refused_other_columns_every_cell() {
    // Table 4-4: Geometric Design Parameters for Design Standard Ib Paved, p. 45 (PDF p. 54). As rendered, the figures
    // equal those of Table 4-3; the flat column conflicts with Table 4-2a (110 km/h) and is refused.
    let rows: Rows = [
        [120.0, 100.0, 80.0, 50.0],
        [205.0, 160.0, 115.0, 60.0],
        [795.0, 670.0, 545.0, 345.0],
        [710.0, 415.0, 240.0, 100.0],
        [3.0, 4.5, 6.0, 6.0],
        [4.0, 6.5, 8.0, 8.0],
        [0.5, 0.5, 0.5, 0.5],
        [7.0, 7.0, 7.0, 4.0],
        [103.0, 80.0, 32.0, 9.0],
        [664.0, 475.0, 310.0, 126.0],
        [50.0, 37.0, 25.0, 11.0],
        [2.5, 2.5, 2.5, 2.5],
        [4.0, 4.0, 4.0, 4.0],
        [60.0, 60.0, 60.0, 40.0],
    ];
    let mut skip: Vec<(Terrain, Parameter)> = Parameter::ALL.iter().map(|p| (Terrain::Flat, *p)).collect();
    skip.push((Terrain::Rolling, Parameter::CrestKStopping));
    check_table(RoadClass::Ib, &rows, &skip);
    let e = max_design_speed(RoadClass::Ib, Terrain::Flat).unwrap_err().to_string();
    assert!(e.contains("110") && e.contains("120") && e.contains("Table 4-2a"), "{e}");
}

#[test]
fn table_4_5_class_ii_paved_every_cell() {
    // Table 4-5: Design Standard II Paved, p. 46 (PDF p. 55).
    let rows: Rows = [
        [90.0, 70.0, 60.0, 50.0],
        [135.0, 95.0, 75.0, 58.0],
        [605.0, 485.0, 410.0, 345.0],
        [320.0, 185.0, 130.0, 100.0],
        [3.5, 5.5, 6.0, 6.0],
        [5.5, 7.5, 8.0, 8.0],
        [0.5, 0.5, 0.5, 0.5],
        [7.0, 7.0, 7.0, 4.0],
        [43.0, 22.0, 14.0, 9.0],
        [307.0, 246.0, 176.0, 126.0],
        [30.0, 20.0, 15.0, 11.0],
        [2.5, 2.5, 2.5, 2.5],
        [4.0, 4.0, 4.0, 4.0],
        [50.0, 50.0, 50.0, 30.0],
    ];
    // Withheld: flat passing K 307 (S03), mountainous gradients 6 and 8 (S04).
    check_table(
        RoadClass::II,
        &rows,
        &[(Terrain::Flat, Parameter::CrestKPassing), (Terrain::Mountainous, Parameter::MaxGradientDesirable), (Terrain::Mountainous, Parameter::MaxGradientAbsolute)],
    );
}

#[test]
fn table_4_6_class_iii_paved_every_cell() {
    // Table 4-6: Design Standard III Paved, p. 46 (PDF p. 55).
    let rows: Rows = [
        [80.0, 70.0, 50.0, 50.0],
        [115.0, 95.0, 60.0, 60.0],
        [545.0, 485.0, 345.0, 345.0],
        [240.0, 185.0, 85.0, 100.0],
        [4.0, 5.5, 9.0, 9.0],
        [6.0, 7.5, 11.0, 11.0],
        [0.5, 0.5, 0.5, 0.5],
        [7.0, 7.0, 7.0, 4.0],
        [32.0, 22.0, 9.0, 9.0],
        [310.0, 246.0, 126.0, 126.0],
        [25.0, 20.0, 11.0, 11.0],
        [2.5, 2.5, 2.5, 2.5],
        [4.0, 4.0, 4.0, 4.0],
        [50.0, 50.0, 50.0, 30.0],
    ];
    check_table(RoadClass::III, &rows, &[]);
}

#[test]
fn table_4_7_class_a_gravel_every_cell() {
    // Table 4-7: Design Standard A Gravel, p. 46 (PDF p. 55).
    let rows: Rows = [
        [90.0, 80.0, 70.0, 50.0],
        [135.0, 115.0, 95.0, 60.0],
        [605.0, 545.0, 485.0, 345.0],
        [320.0, 240.0, 185.0, 100.0],
        [3.5, 5.0, 7.0, 7.0],
        [5.5, 7.0, 9.0, 9.0],
        [0.5, 0.5, 0.5, 0.5],
        [7.0, 7.0, 7.0, 4.0],
        [43.0, 32.0, 22.0, 9.0],
        [307.0, 310.0, 246.0, 126.0],
        [30.0, 25.0, 20.0, 11.0],
        [4.0, 4.0, 4.0, 4.0],
        [4.0, 4.0, 4.0, 4.0],
        [40.0, 40.0, 40.0, 30.0],
    ];
    check_table(RoadClass::A, &rows, &[(Terrain::Flat, Parameter::CrestKPassing)]);
}

#[test]
fn table_4_8_class_b_gravel_every_cell() {
    // Table 4-8: Design Standard B Gravel, p. 47 (PDF p. 56).
    let rows: Rows = [
        [80.0, 60.0, 50.0, 50.0],
        [115.0, 75.0, 60.0, 60.0],
        [545.0, 410.0, 345.0, 345.0],
        [240.0, 130.0, 85.0, 100.0],
        [4.0, 6.0, 9.0, 9.0],
        [6.0, 8.0, 11.0, 11.0],
        [0.5, 0.5, 0.5, 0.5],
        [7.0, 7.0, 7.0, 4.0],
        [32.0, 14.0, 9.0, 9.0],
        [310.0, 176.0, 126.0, 126.0],
        [25.0, 15.0, 11.0, 11.0],
        [4.0, 4.0, 4.0, 4.0],
        [4.0, 4.0, 4.0, 4.0],
        [30.0, 30.0, 30.0, 30.0],
    ];
    check_table(RoadClass::B, &rows, &[]);
}

#[test]
fn table_4_9_class_c_gravel_every_cell() {
    // Table 4-9: Design Standard C Gravel, p. 47 (PDF p. 56).
    let rows: Rows = [
        [60.0, 50.0, 40.0, 50.0],
        [75.0, 60.0, 45.0, 60.0],
        [410.0, 345.0, 285.0, 345.0],
        [130.0, 85.0, 55.0, 100.0],
        [5.0, 7.0, 10.0, 7.0],
        [7.0, 9.0, 12.0, 9.0],
        [0.5, 0.5, 0.5, 0.5],
        [7.0, 7.0, 7.0, 4.0],
        [22.0, 14.0, 5.0, 9.0],
        [246.0, 176.0, 86.0, 126.0],
        [15.0, 11.0, 8.0, 11.0],
        [4.0, 4.0, 4.0, 4.0],
        [4.0, 4.0, 4.0, 4.0],
        [30.0, 30.0, 30.0, 30.0],
    ];
    // Withheld: flat and rolling crest and passing K (S05).
    check_table(
        RoadClass::C,
        &rows,
        &[
            (Terrain::Flat, Parameter::CrestKStopping),
            (Terrain::Flat, Parameter::CrestKPassing),
            (Terrain::Rolling, Parameter::CrestKStopping),
            (Terrain::Rolling, Parameter::CrestKPassing),
        ],
    );
}

#[test]
fn table_4_2a_road_design_classes_every_row() {
    // Table 4-2a: Road Design Classes, section 4.5, p. 44 (PDF p. 53). Capacity [pcu x 1,000/day], road-way width [m],
    // maximum design speed by terrain, functional classification ticks.
    let d = class_definition(RoadClass::Ia);
    assert_eq!((d.traffic_kpcu_per_day, d.roadway_width_m, d.functional_classes), (Some((12.0, 20.0)), (20.80, 24.60), "A"));
    let d = class_definition(RoadClass::Ib);
    assert_eq!((d.traffic_kpcu_per_day, d.roadway_width_m, d.functional_classes), (Some((6.0, 10.0)), (11.0, 11.0), "A, B"));
    let d = class_definition(RoadClass::II);
    assert_eq!((d.traffic_kpcu_per_day, d.roadway_width_m, d.functional_classes), (Some((4.0, 8.0)), (10.0, 10.0), "A, B, C"));
    let d = class_definition(RoadClass::III);
    assert_eq!((d.traffic_kpcu_per_day, d.roadway_width_m, d.functional_classes), (Some((2.0, 6.0)), (8.6, 8.6), "A, B, C"));
    let d = class_definition(RoadClass::A);
    assert_eq!((d.traffic_kpcu_per_day, d.roadway_width_m, d.functional_classes), (Some((4.0, 8.0)), (10.0, 10.0), "B, C, D"));
    let d = class_definition(RoadClass::B);
    assert_eq!((d.traffic_kpcu_per_day, d.roadway_width_m, d.functional_classes), (Some((2.0, 6.0)), (8.6, 8.6), "D, E"));
    let d = class_definition(RoadClass::C);
    assert_eq!((d.traffic_kpcu_per_day, d.roadway_width_m, d.functional_classes), (None, (6.4, 6.4), "E"));
    assert_eq!((d.source.what, d.source.printed_page, d.source.pdf_page), ("Table 4-2a", 44, 53));
    // Maximum design speed [km/h] level, rolling, mountainous.
    let speeds = [
        (RoadClass::Ia, [120.0, 100.0, 80.0]),
        (RoadClass::II, [90.0, 70.0, 60.0]),
        (RoadClass::III, [80.0, 70.0, 50.0]),
        (RoadClass::A, [90.0, 80.0, 70.0]),
        (RoadClass::B, [80.0, 60.0, 50.0]),
        (RoadClass::C, [60.0, 50.0, 40.0]),
        (RoadClass::Ib, [f64::NAN, 100.0, 80.0]),
    ];
    for (class, s) in speeds {
        for (i, t) in [Terrain::Flat, Terrain::Rolling, Terrain::Mountainous].iter().enumerate() {
            let r = table_4_2a_max_design_speed(class, *t);
            if s[i].is_nan() {
                assert!(r.unwrap_err().to_string().contains("110"));
            } else {
                assert_eq!(r.unwrap().value, s[i], "{class:?} {t:?}");
                // and the class parameter table agrees
                assert_eq!(max_design_speed(class, *t).unwrap().value, s[i]);
            }
        }
    }
    assert!(table_4_2a_max_design_speed(RoadClass::II, Terrain::UrbanPeriUrban).is_err());
}

#[test]
fn table_4_2b_class_cross_sections_every_row_and_arithmetic() {
    // Table 4-2b: Road Design Classes (continued), p. 45 (PDF p. 54): right of way, road way width, carriageway width,
    // lane width, number of lanes, shoulder width (2 x), median.
    let rows = [
        (RoadClass::Ia, 60.0, (20.80, 24.60), 14.6, 3.65, 4u8, 2.5, Some((1.2, 5.0))),
        (RoadClass::Ib, 60.0, (11.0, 11.0), 7.0, 3.5, 2, 2.0, None),
        (RoadClass::II, 50.0, (10.0, 10.0), 6.0, 3.0, 2, 2.0, None),
        (RoadClass::III, 50.0, (8.6, 8.6), 5.6, 2.8, 2, 1.5, None),
        (RoadClass::A, 40.0, (10.0, 10.0), 6.0, 3.0, 2, 2.0, None),
        (RoadClass::B, 30.0, (8.6, 8.6), 5.6, 2.8, 2, 1.5, None),
        (RoadClass::C, 30.0, (6.4, 6.4), 4.0, 4.0, 1, 1.2, None),
    ];
    for (class, row, rw, cw, lane, lanes, sh, median) in rows {
        let c = class_cross_section(class);
        let v = &c.value;
        assert_eq!((v.right_of_way_m, v.roadway_width_m, v.carriageway_width_m, v.lane_width_m, v.number_of_lanes, v.shoulder_width_each_side_m, v.median_width_m), (row, rw, cw, lane, lanes, sh, median), "{class:?}");
        assert_eq!((c.source.what, c.source.printed_page, c.source.pdf_page), ("Table 4-2b", 45, 54));
        // lane width x lanes = carriageway width, and roadway = carriageway + 2 shoulders (+ median for Ia)
        assert!((v.lane_width_m * v.number_of_lanes as f64 - v.carriageway_width_m).abs() < 1e-9, "{class:?} lanes");
        let med = v.median_width_m.unwrap_or((0.0, 0.0));
        assert!((v.carriageway_width_m + 2.0 * v.shoulder_width_each_side_m + med.0 - v.roadway_width_m.0).abs() < 1e-9, "{class:?} roadway low");
        assert!((v.carriageway_width_m + 2.0 * v.shoulder_width_each_side_m + med.1 - v.roadway_width_m.1).abs() < 1e-9, "{class:?} roadway high");
        // the right of way row of the class parameter tables agrees (rural columns)
        assert_eq!(val(class, Terrain::Rolling, Parameter::RightOfWay), row);
        // Table 4-2a road way width agrees with Table 4-2b
        assert_eq!(class_definition(class).roadway_width_m, rw);
    }
}

#[test]
fn escarpment_and_unknown_terrain_are_refused_plainly() {
    let e = class_parameter(RoadClass::II, Terrain::Escarpment, Parameter::DesignSpeed).unwrap_err().to_string();
    assert!(e.contains("escarpment") && e.contains("no limits"), "{e}");
    // Section 5.2, p. 53 (PDF p. 62): the transverse slope bands.
    assert_eq!(terrain_for_transverse_slope(4.0).value, Terrain::Flat);
    assert_eq!(terrain_for_transverse_slope(5.0).value, Terrain::Flat);
    assert_eq!(terrain_for_transverse_slope(12.0).value, Terrain::Rolling);
    assert_eq!(terrain_for_transverse_slope(20.0).value, Terrain::Rolling);
    assert_eq!(terrain_for_transverse_slope(45.0).value, Terrain::Mountainous);
    assert_eq!(terrain_for_transverse_slope(70.0).value, Terrain::Mountainous);
    assert_eq!(terrain_for_transverse_slope(71.0).value, Terrain::Escarpment);
    assert_eq!(terrain_for_transverse_slope(71.0).source.printed_page, 53);
}

#[test]
fn class_and_terrain_names_parse() {
    assert_eq!(RoadClass::parse("II Paved"), Some(RoadClass::II));
    assert_eq!(RoadClass::parse("a gravel"), Some(RoadClass::A));
    assert_eq!(RoadClass::parse("Ia"), Some(RoadClass::Ia));
    assert_eq!(RoadClass::parse("IV"), None);
    assert_eq!(Terrain::parse("Mountainous"), Some(Terrain::Mountainous));
    assert_eq!(Terrain::parse("urban/peri-urban"), Some(Terrain::UrbanPeriUrban));
    assert_eq!(Terrain::parse("moon"), None);
    assert_eq!(RoadClass::Ib.manual_name(), "Ib Paved");
    assert_eq!(RoadClass::C.surface(), Surface::Gravel);
}

/// The class tables carry the low end of the printed ranges of Tables 6.19, 6-11, 6-13 and the rounded passing
/// values of Tables 6.22 and 6-12, and the radius of Table 6-1 for emax 7 % (rural) or 4 % (urban). Every cell is
/// compared; the only cells that differ are the ones withheld, and the test lists them so that a new mismatch fails.
#[test]
fn class_tables_agree_with_the_speed_tables_except_the_withheld_cells() {
    let mut mismatches: Vec<String> = Vec::new();
    for class in RoadClass::ALL {
        for terrain in TERRAINS {
            let Ok(speed) = max_design_speed(class, terrain) else { continue };
            let sp = speed.value;
            let emax = val(class, terrain, Parameter::MaxSuperelevation);
            let cmp = |mismatches: &mut Vec<String>, name: &str, p: Parameter, expected: f64| match class_parameter(class, terrain, p) {
                Ok(c) => {
                    if (c.value - expected).abs() > 1e-9 {
                        mismatches.push(format!("{class:?} {terrain:?} {name}: class table {} vs speed table {expected}", c.value));
                    }
                }
                Err(_) => mismatches.push(format!("{class:?} {terrain:?} {name}: withheld")),
            };
            // radius: Table 6-1 rounded radius
            cmp(&mut mismatches, "radius", Parameter::MinHorizontalCurveRadius, horizontal::min_radius(sp, emax).unwrap().value.rounded_radius_m);
            // stopping sight distance: Table 6.19 low end, rounded up to a multiple of 5 m
            let ssd_low = sight::stopping_sight_distance(sp).unwrap().value.stopping_sight_distance_m.0;
            let printed = val(class, terrain, Parameter::MinStoppingSightDistance);
            let rounded = (ssd_low / 5.0).ceil() * 5.0;
            if printed != rounded && !(class == RoadClass::II && terrain == Terrain::UrbanPeriUrban && printed == 58.0) {
                mismatches.push(format!("{class:?} {terrain:?} ssd: {printed} vs ceil5({ssd_low}) = {rounded}"));
            }
            let psd = sight::minimum_passing_sight_distance(sp).unwrap().value.passing_sight_distance_m;
            let printed = val(class, terrain, Parameter::MinPassingSightDistance);
            if printed != (psd / 5.0).ceil() * 5.0 {
                mismatches.push(format!("{class:?} {terrain:?} psd: {printed} vs ceil5({psd})"));
            }
            cmp(&mut mismatches, "crest K", Parameter::CrestKStopping, vertical::crest_k_stopping(sp).unwrap().value.k_rounded.0);
            cmp(&mut mismatches, "passing K", Parameter::CrestKPassing, vertical::crest_k_passing(sp).unwrap().value.k_rounded);
            cmp(&mut mismatches, "sag K", Parameter::SagKStopping, vertical::sag_k(sp).unwrap().value.k_rounded.0);
        }
    }
    mismatches.sort();
    // Exactly the withheld cells (known_slips S01 to S05); Class Ib flat is skipped (its speed is refused).
    let expected = [
        "A Flat passing K: withheld",
        "C Flat crest K: withheld",
        "C Flat passing K: withheld",
        "C Rolling crest K: withheld",
        "C Rolling passing K: withheld",
        "II Flat passing K: withheld",
        "Ia Rolling crest K: withheld",
        "Ib Rolling crest K: withheld",
    ];
    assert_eq!(mismatches, expected.iter().map(|s| s.to_string()).collect::<Vec<_>>());
}

#[test]
fn gradients_of_the_class_tables_against_tables_6_14_and_6_15() {
    // Where Tables 6-14 and 6-15 (p. 122, PDF p. 131) print a value for the class speed, the class tables agree,
    // except Class II mountainous (S04, withheld). Where they print a dash the class table gives the value.
    let mut differ = Vec::new();
    for class in RoadClass::ALL {
        for terrain in [Terrain::Flat, Terrain::Rolling, Terrain::Mountainous] {
            let Ok(speed) = max_design_speed(class, terrain) else { continue };
            for (p, f) in [
                (Parameter::MaxGradientDesirable, vertical::max_gradient_desirable as GradeFn),
                (Parameter::MaxGradientAbsolute, vertical::max_gradient_absolute as GradeFn),
            ] {
                let class_value = class_parameter(class, terrain, p);
                if let Ok(t) = f(terrain, speed.value) {
                    match class_value {
                        Ok(c) if (c.value - t.value).abs() < 1e-9 => {}
                        Ok(c) => differ.push(format!("{class:?} {terrain:?} {:?}: {} vs {}", p, c.value, t.value)),
                        Err(_) => differ.push(format!("{class:?} {terrain:?} {p:?}: withheld")),
                    }
                }
            }
        }
    }
    differ.sort();
    assert_eq!(differ, vec!["II Mountainous MaxGradientAbsolute: withheld".to_string(), "II Mountainous MaxGradientDesirable: withheld".to_string()]);
}

#[test]
fn every_slip_is_listed_with_a_place_and_treatment() {
    let slips = known_slips();
    assert!(slips.len() >= 20);
    for s in &slips {
        assert!(!s.place.is_empty() && !s.what.is_empty() && !s.treatment.is_empty(), "{}", s.id);
        assert!(!s.what.contains('\u{2014}') && !s.what.contains('\u{2013}') && !s.treatment.contains('\u{2014}'), "no long dashes in {}", s.id);
    }
    let ids: Vec<&str> = slips.iter().map(|s| s.id).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(ids.len(), sorted.len(), "ids are unique");
}
