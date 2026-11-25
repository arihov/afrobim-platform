// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! South African design basis from SAPEM Chapter 10 (second edition 2014): the tables read from
//! the rendered pages, and the worked examples of the chapter. Page numbers are the printed
//! page numbers of the chapter; the flip-book page is the printed page plus 11.

use afrobim_pavement::design::sa_trh4::*;
use afrobim_pavement::design::{design as run, DesignError, DesignInput, DesignMethod, DesignResult};

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn worked_example_input() -> Trh4Input {
    // Table 22 (p. 52): CTO data, 682 heavy vehicles per day in the one lane, E80/HV 1.5,
    // HV growth 5 %, E80/HV growth 2 %, survey 2010-12-31, opening 2013-01-01 (2 years),
    // structural design period 20 years.
    Trh4Input {
        category: RoadCategory::B,
        loading: BaseLoading::HeavyVehicles { hvv_per_lane_per_day: 682.0, e80_per_hv: 1.5 },
        hv_growth: 0.05,
        e80_per_hv_growth: 0.02,
        years_survey_to_opening: 2.0,
        structural_design_period_years: 20.0,
        climate: Climate::Region(ClimaticRegion::Moderate),
        subgrade_design_cbr_percent: None,
    }
}

/// Table 22, p. 52 (flip-book page 63): the whole chain.
#[test]
fn table_22_worked_example_design_traffic() {
    let d = design(&worked_example_input()).unwrap();
    assert_eq!(d.ade, 1023.0);
    // Equation 13 gives 0.0710; the example shows it as 0.07 and carries 0.07 on.
    assert!(close(d.combined_growth, 0.071, 1e-12));
    // The printed steps with the example's rounded 0.07: AADE at opening 1171, ET 18 748 635.
    let i = 0.07;
    assert_eq!(project_aade(1023.0, i, 2.0).unwrap().round(), 1171.0);
    let et = cumulative_equivalent_traffic(1171.0, i, 20.0).unwrap();
    assert_eq!(format!("{et:.0}"), "18748635");
    // Carrying the unrounded growth rate of 0.0710 through gives a larger figure, still
    // between 18 and 25 million, in class ES30.
    assert!(d.design_traffic_e80 > 18.0e6 && d.design_traffic_e80 < 25.0e6);
    assert_eq!(d.traffic_class, Some(TrafficClass::Es30));
    // With the example's own 0.07 entered as the two growth rates (h = 0.07, v = 0) the
    // module reproduces the printed AADE at opening.
    let mut same = worked_example_input();
    same.hv_growth = 0.07;
    same.e80_per_hv_growth = 0.0;
    let d2 = design(&same).unwrap();
    assert_eq!(format!("{:.0}", d2.aade_at_opening), "1171");
    // The module keeps the AADE unrounded (1171.23), so its ET is 18 752 360, not the printed
    // 18 748 635 which uses 1171.
    assert_eq!(format!("{:.0}", d2.design_traffic_e80), "18752360");
}

/// Equation 13 as in the example: (1.05)(1.02) - 1 = 0.071, shown as 0.07.
#[test]
fn equation_13_combined_growth() {
    assert!(close(combined_growth_rate(0.05, 0.02).unwrap(), 0.071, 1e-12));
}

/// Figure 20, p. 34 (flip-book page 45): E80 per axle, per vehicle, weighted average.
#[test]
fn figure_20_e80_per_heavy_vehicle() {
    let v1: f64 = [0.4, 0.4, 0.2].iter().sum();
    let v2: f64 = [0.9, 0.9, 1.2, 0.5].iter().sum();
    let v3: f64 = [0.8, 0.8, 0.7, 0.7, 1.0, 0.7].iter().sum();
    assert!(close(v1, 1.0, 1e-12) && close(v2, 3.5, 1e-12) && close(v3, 4.7, 1e-12));
    let w = weighted_e80_per_hv(&[(250.0, v1), (100.0, v2), (50.0, v3)]).unwrap();
    // Printed terms 0.63, 0.88, 0.59 and total 2.09; unrounded 0.625 + 0.875 + 0.5875.
    assert!(close(w, 2.0875, 1e-9));
    assert_eq!(format!("{w:.2}"), "2.09");
    assert!(weighted_e80_per_hv(&[]).is_err());
    assert!(weighted_e80_per_hv(&[(0.0, 1.0)]).is_err());
}

/// Equation 1 and the example on p. 32 (flip-book 42): a 100 kN axle at n = 4.
#[test]
fn equation_1_load_equivalency() {
    let lef = load_equivalency(100.0, 4.0).unwrap();
    assert!(close(lef, 2.44140625, 1e-9));
    assert_eq!(format!("{lef:.1}"), "2.4"); // the page prints 2.4
    assert!(close(load_equivalency(80.0, 4.2).unwrap(), 1.0, 1e-12));
    assert!(close(load_equivalency(40.0, 4.0).unwrap(), 0.0625, 1e-12));
    assert!(load_equivalency(-1.0, 4.0).is_err());
    assert!(load_equivalency(80.0, 0.0).is_err());
}

/// Equation 2, p. 32: mean of the limits' values.
#[test]
fn equation_2_interval() {
    let v = load_equivalency_interval(60.0, 80.0, 4.0).unwrap();
    assert!(close(v, (0.75f64.powi(4) + 1.0) / 2.0, 1e-12));
    assert!(load_equivalency_interval(80.0, 60.0, 4.0).is_err());
}

/// Table 10, p. 30 (flip-book 41): the exponent rows.
#[test]
fn table_10_damage_exponents() {
    use DamageExponent::*;
    assert_eq!((GranularGranular.range(), GranularGranular.recommended()), (Some((3.0, 6.0)), 4.0));
    assert_eq!((GranularCemented.range(), GranularCemented.recommended()), (Some((2.0, 4.0)), 3.0));
    assert_eq!((CementedGranularPreCracked.range(), CementedGranularPreCracked.recommended()), (Some((4.0, 10.0)), 5.0));
    assert_eq!((CementedGranularPostCracked.range(), CementedGranularPostCracked.recommended()), (Some((3.0, 6.0)), 5.0));
    assert_eq!(CementedCementedPreCracked.range(), Some((3.0, 6.0)));
    assert_eq!(CementedCementedPostCracked.range(), Some((2.0, 5.0)));
    assert_eq!(CementedCementedPostCracked.recommended_text(), "4 - 5");
    assert_eq!((BsmGranular.range(), BsmGranular.recommended()), (Some((2.0, 6.0)), 4.0));
    assert_eq!((HotMixAsphaltCemented.range(), HotMixAsphaltCemented.recommended()), (Some((2.0, 5.0)), 4.0));
    assert_eq!((Concrete.range(), Concrete.recommended()), (None, 4.5));
}

/// Table 11, p. 34 (flip-book 45): every class.
#[test]
fn table_11_traffic_classes() {
    let rows = [
        ("ES0.003", "< 0.003", "< 3"),
        ("ES0.01", "0.003 - 0.01", "3 - 10"),
        ("ES0.03", "0.01 - 0.03", "10 - 20"),
        ("ES0.1", "0.03 - 0.1", "20 - 75"),
        ("ES0.3", "0.1 - 0.3", "75 - 220"),
        ("ES1", "0.3 - 1", "220 - 700"),
        ("ES3", "1 - 3", "> 700"),
        ("ES10", "3 - 10", "> 700 (per direction)"),
        ("ES30", "10 - 30", "> 2200 (per direction)"),
        ("ES100", "30 - 100", "> 6500 (per direction)"),
    ];
    for (c, (n, cap, vpd)) in TrafficClass::all().iter().zip(rows) {
        assert_eq!((c.name(), c.capacity_text(), c.approximate_vpd_text()), (n, cap, vpd));
    }
    assert_eq!(TrafficClass::Es30.lower_million_e80(), Some(10.0));
    assert_eq!(TrafficClass::Es0_003.lower_million_e80(), None);
}

#[test]
fn class_from_traffic_and_refusal_above_100_million() {
    assert_eq!(TrafficClass::from_million_e80(0.002), Some(TrafficClass::Es0_003));
    assert_eq!(TrafficClass::from_million_e80(0.5), Some(TrafficClass::Es1));
    assert_eq!(TrafficClass::from_million_e80(3.0), Some(TrafficClass::Es3));
    assert_eq!(TrafficClass::from_million_e80(3.0001), Some(TrafficClass::Es10));
    assert_eq!(TrafficClass::from_million_e80(18.7), Some(TrafficClass::Es30));
    assert_eq!(TrafficClass::from_million_e80(100.0), Some(TrafficClass::Es100));
    assert_eq!(TrafficClass::from_million_e80(100.1), None);
    assert!(TrafficClass::is_on_limit(10.0) && !TrafficClass::is_on_limit(10.5));
    let mut i = worked_example_input();
    i.loading = BaseLoading::DailyE80PerLane(20000.0);
    let d = design(&i).unwrap();
    assert_eq!(d.traffic_class, None);
    assert!(d.warnings().iter().any(|w| w.contains("above ES100")));
    assert!(d.layers().is_none());
}

/// Table 2, p. 8 (flip-book 19).
#[test]
fn table_2_road_categories() {
    use RoadCategory::*;
    assert_eq!([A, B, C, D].map(|c| c.design_reliability_percent()), [95.0, 90.0, 80.0, 50.0]);
    assert_eq!([A, B, C, D].map(|c| c.length_exceeding_terminal_percent()), [5.0, 10.0, 20.0, 50.0]);
    assert_eq!(A.description(), "Major inter-urban freeways and major rural roads");
    assert_eq!(D.description(), "Rural access roads");
    assert_eq!((A.importance(), A.level_of_service()), ("Very important", "Very high"));
    assert_eq!((B.importance(), B.level_of_service()), ("Important", "High"));
    assert_eq!((C.importance(), C.level_of_service()), ("Less important", "Moderate"));
    assert_eq!(A.typical_class_text(), "ES10 - ES100");
    assert_eq!(B.typical_class_range(), (TrafficClass::Es1, TrafficClass::Es10));
    assert_eq!(D.typical_class_range(), (TrafficClass::Es0_003, TrafficClass::Es1));
}

/// Table 6, p. 24.
#[test]
fn table_6_paved_shoulders() {
    use RoadCategory::*;
    assert_eq!([A, B, C].map(|c| c.minimum_paved_shoulder_mm()), [Some(1200.0), Some(1000.0), Some(800.0)]);
    assert_eq!(D.minimum_paved_shoulder_mm(), None);
}

/// Table 13, p. 39 (flip-book 50): all printed cells and the dashes.
#[test]
fn table_13_lane_distribution() {
    use Lane::*;
    assert_eq!(lane_distribution_factor(2, Lane1), Some(1.00));
    assert_eq!(lane_distribution_factor(2, SurfacedSlowShoulder), Some(1.00));
    assert_eq!(lane_distribution_factor(2, Lane2), None);
    assert_eq!(lane_distribution_factor(4, SurfacedSlowShoulder), Some(0.95));
    assert_eq!(lane_distribution_factor(4, Lane1), Some(0.95));
    assert_eq!(lane_distribution_factor(4, Lane2), Some(0.30));
    assert_eq!(lane_distribution_factor(4, Lane3), None);
    assert_eq!(lane_distribution_factor(6, Lane1), Some(0.70));
    assert_eq!(lane_distribution_factor(6, Lane2), Some(0.60));
    assert_eq!(lane_distribution_factor(6, Lane3), Some(0.25));
    assert_eq!(lane_distribution_factor(3, Lane1), None);
    assert_eq!(lane_distribution_factor(8, Lane1), None);
}

/// Table 39, p. 90 (flip-book 101) and Table 5, p. 22 (flip-book 33).
#[test]
fn climate_classes() {
    use ClimaticRegion::*;
    assert_eq!(ClimaticRegion::from_weinert_n(1.9).unwrap(), (Wet, false));
    assert_eq!(ClimaticRegion::from_weinert_n(2.0).unwrap(), (Moderate, true));
    assert_eq!(ClimaticRegion::from_weinert_n(3.5).unwrap(), (Moderate, false));
    assert_eq!(ClimaticRegion::from_weinert_n(5.0).unwrap(), (Moderate, true));
    assert_eq!(ClimaticRegion::from_weinert_n(5.1).unwrap(), (Dry, false));
    assert!(ClimaticRegion::from_weinert_n(-1.0).is_err());
    assert_eq!(ClimaticRegion::from_thornthwaite(-50.0).unwrap(), (Dry, false)); // arid
    assert_eq!(ClimaticRegion::from_thornthwaite(-30.0).unwrap(), (Dry, false)); // semi-arid
    assert_eq!(ClimaticRegion::from_thornthwaite(-10.0).unwrap(), (Moderate, false)); // dry sub-humid
    assert_eq!(ClimaticRegion::from_thornthwaite(10.0).unwrap(), (Moderate, false)); // moist sub-humid
    assert_eq!(ClimaticRegion::from_thornthwaite(25.0).unwrap(), (Wet, false)); // humid
    assert_eq!(ClimaticRegion::from_thornthwaite(-20.0).unwrap(), (Dry, true));
    assert_eq!(ClimaticRegion::from_thornthwaite(20.0).unwrap(), (Moderate, true));
}

/// Table 4, p. 22 (flip-book 33).
#[test]
fn table_4_subgrade_action() {
    let (row, acts, lim) = subgrade_action(20.0).unwrap();
    assert_eq!((row, acts.len(), lim), ("> 15", 2, false));
    let (row, acts, _) = subgrade_action(10.0).unwrap();
    assert_eq!((row, acts.len()), ("7 to 15", 3));
    assert!(acts[2].contains("150 mm thick layer of G7"));
    let (row, acts, _) = subgrade_action(5.0).unwrap();
    assert_eq!((row, acts.len()), ("3 to 7", 4));
    assert!(acts[2].contains("G9") && acts[3].contains("second 150 mm"));
    let (row, _, _) = subgrade_action(2.0).unwrap();
    assert_eq!(row, "< 3");
    // 7 is printed in two rows: the weaker, flagged. 3 and 15 are not ambiguous.
    assert_eq!(subgrade_action(15.0).map(|(r, _, l)| (r, l)).unwrap(), ("7 to 15", false));
    assert_eq!(subgrade_action(7.0).map(|(r, _, l)| (r, l)).unwrap(), ("3 to 7", true));
    assert_eq!(subgrade_action(3.0).map(|(r, _, l)| (r, l)).unwrap(), ("3 to 7", false));
    assert!(subgrade_action(0.0).is_err());
}

#[test]
fn cumulative_traffic_edge_cases() {
    // Zero growth: 365 x AADE x years.
    assert_eq!(cumulative_equivalent_traffic(100.0, 0.0, 10.0).unwrap(), 365_000.0);
    assert!(cumulative_equivalent_traffic(100.0, 0.05, 0.0).is_err());
    assert!(cumulative_equivalent_traffic(-1.0, 0.05, 10.0).is_err());
    assert!(combined_growth_rate(-1.0, 0.0).is_err());
}

#[test]
fn warnings_for_class_outside_category_and_cbr() {
    let mut i = worked_example_input();
    i.category = RoadCategory::D; // ES30 is far outside ES0.003 - ES1
    i.subgrade_design_cbr_percent = Some(2.0);
    i.climate = Climate::WeinertN(2.0);
    let d = design(&i).unwrap();
    let w = d.warnings().join("\n");
    assert!(w.contains("outside the typical pavement class of Category D"));
    assert!(w.contains("CBR below 3"));
    assert!(w.contains("exactly on a limit"));
    assert_eq!(d.climate, ClimaticRegion::Moderate);
}

#[test]
fn invalid_inputs_are_errors() {
    let mut i = worked_example_input();
    i.structural_design_period_years = 0.0;
    assert!(matches!(design(&i), Err(DesignError::InvalidInput(_))));
    let mut i = worked_example_input();
    i.loading = BaseLoading::DailyE80PerLane(-5.0);
    assert!(design(&i).is_err());
    let mut i = worked_example_input();
    i.hv_growth = -1.5;
    assert!(design(&i).is_err());
}

#[test]
fn dispatch_calculation_lines_cite_documents_and_say_draft() {
    let input = DesignInput::SaTrh4(worked_example_input());
    assert_eq!(input.method(), DesignMethod::SaTrh4);
    let boxed = run(&input).unwrap();
    assert_eq!(boxed.method(), DesignMethod::SaTrh4);
    assert!(boxed.layers().is_none());
    let text = boxed.calculation().join("\n");
    assert!(text.contains("second edition 2014"));
    assert!(text.contains("1996 draft"));
    assert!(text.contains("Table 2, p. 8"));
    assert!(text.contains("Equation 15, p. 45"));
    assert!(text.contains("Table 11, p. 34"));
    assert!(text.contains("Table 39, p. 90"));
    assert!(text.contains("not selected"));
    assert!(text.contains("not the same quantity as the MoWT, AASHTO or DMRB"));
    // The existing menu is unchanged; the extended one adds the South African basis.
    assert_eq!(DesignMethod::all().len(), 3);
    assert_eq!(DesignMethod::extended().len(), 4);
    assert!(!DesignMethod::all().contains(&DesignMethod::SaTrh4));
}
