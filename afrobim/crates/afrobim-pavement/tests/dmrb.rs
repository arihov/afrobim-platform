//! UK DMRB CD 224 Revision 0 (March 2020) and CD 226 Version 0.1.0 (November 2021): the tables,
//! the three worked examples of CD 224 and worked example A1.1 of CD 226.

use afrobim_pavement::design::dmrb::*;
use afrobim_pavement::design::{design as run, DesignInput, DesignMethod, DesignResult};

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn traffic(flows: Vec<Flow>, scheme: Scheme, years: u32, lanes: u32) -> TrafficInput {
    TrafficInput { flows, scheme, design_period_years: years, lanes_one_direction: lanes, past_traffic: false }
}

/// CD 224 Table 2.23.1N1: maintenance scheme, by class, 20 years.
#[test]
fn cd_224_example_for_maintenance_schemes() {
    use VehicleGroup::*;
    let flows = [(BusesAndCoaches, 77.0), (Rigid2Axle, 914.0), (Rigid3Axle, 59.0), (Rigid4Axle, 53.0), (Articulated3And4Axle, 302.0), (Articulated5Axle, 1021.0), (Articulated6Axle, 574.0)];
    let t = design_traffic(&traffic(flows.iter().map(|(g, f)| Flow::new(*g, *f)).collect(), Scheme::Maintenance, 20, 2)).unwrap();
    let growth: Vec<f64> = t.flows.iter().map(|f| f.growth_factor).collect();
    assert_eq!(growth, [1.11, 1.11, 1.11, 1.23, 1.23, 1.23, 1.23]);
    let wear: Vec<f64> = t.flows.iter().map(|f| f.wear_factor).collect();
    assert_eq!(wear, [2.6, 0.4, 2.3, 3.0, 1.7, 2.9, 3.7]);
    // The printed Tc column, to its two decimals.
    let tc: Vec<String> = t.flows.iter().map(|f| format!("{:.2}", f.weighted_annual_msa)).collect();
    // A slip in the printed example: the 6-axle articulated line, 365 × 574 × 1.23 × 3.7 × 10⁻⁶,
    // is 0.95 msa; the table prints 0.93. The printed total of 2.87 agrees with 0.95, not 0.93.
    assert_eq!(tc, ["0.08", "0.15", "0.05", "0.07", "0.23", "1.33", "0.95"]);
    assert_eq!(t.total_flow, 3000.0);
    assert_eq!(format!("{:.2}", t.total_weighted_annual_msa), "2.87");
    assert!(close(t.heaviest_lane_percent, 89.2, 1e-9));
    assert_eq!(format!("{:.0}", t.design_traffic_msa), "51");
}

/// CD 224 Table 2.23.1N2: new road scheme, by category, 40 years, 70 % OGV2.
#[test]
fn cd_224_example_for_new_road_schemes_by_category() {
    let t = design_traffic(&traffic(vec![Flow::new(VehicleGroup::Ogv1PlusPsv, 360.0), Flow::new(VehicleGroup::Ogv2, 840.0)], Scheme::New, 40, 2)).unwrap();
    assert_eq!((t.flows[0].growth_factor, t.flows[0].wear_factor), (1.24, 1.9));
    assert_eq!((t.flows[1].growth_factor, t.flows[1].wear_factor), (1.54, 4.9));
    assert_eq!(format!("{:.2} {:.2}", t.flows[0].weighted_annual_msa, t.flows[1].weighted_annual_msa), "0.31 2.31");
    assert_eq!(format!("{:.2}", t.total_weighted_annual_msa), "2.62");
    assert_eq!(format!("{:.1}", t.heaviest_lane_percent), "95.7");
    assert_eq!(format!("{:.0}", t.design_traffic_msa), "100");
    // Exactly 70 % OGV2: no warning about clause 2.7. Below it, one.
    assert!(!t.warnings.iter().any(|w| w.contains("clause 2.7")));
    let low = design_traffic(&traffic(vec![Flow::new(VehicleGroup::Ogv1PlusPsv, 600.0), Flow::new(VehicleGroup::Ogv2, 600.0)], Scheme::New, 40, 2)).unwrap();
    assert!(low.warnings.iter().any(|w| w.contains("clause 2.7")));
}

/// CD 224 Table 2.23.1N3: no information on vehicle categories.
#[test]
fn cd_224_example_without_vehicle_categories() {
    let t = design_traffic(&traffic(vec![Flow::new(VehicleGroup::AllCommercialVehicles, 2000.0)], Scheme::New, 40, 3)).unwrap();
    assert_eq!((t.flows[0].growth_factor, t.flows[0].wear_factor), (1.45, 4.0));
    assert_eq!(format!("{:.2}", t.total_weighted_annual_msa), "4.23");
    assert_eq!(format!("{:.1}", t.heaviest_lane_percent), "92.8");
    assert_eq!(format!("{:.0}", t.design_traffic_msa), "157");
    // Clause 2.13: only with a 40 year period.
    assert!(design_traffic(&traffic(vec![Flow::new(VehicleGroup::AllCommercialVehicles, 2000.0)], Scheme::New, 20, 3)).is_err());
    assert!(design_traffic(&traffic(vec![Flow::new(VehicleGroup::AllCommercialVehicles, 1.0), Flow::new(VehicleGroup::Ogv2, 1.0)], Scheme::New, 40, 3)).is_err());
}

#[test]
fn cd_224_table_2_12_growth_factors() {
    use VehicleGroup::*;
    let row = |g| (1..=8).map(|i| growth_factor(g, i * 5).unwrap()).collect::<Vec<f64>>();
    assert_eq!(row(Ogv1PlusPsv), [1.02, 1.05, 1.08, 1.11, 1.14, 1.17, 1.21, 1.24]);
    assert_eq!(row(Ogv2), [1.04, 1.10, 1.16, 1.23, 1.30, 1.37, 1.46, 1.54]);
    for g in [BusesAndCoaches, Rigid2Axle, Rigid3Axle] {
        assert_eq!(row(g), row(Ogv1PlusPsv));
    }
    for g in [Rigid4Axle, Articulated3And4Axle, Articulated5Axle, Articulated6Axle] {
        assert_eq!(row(g), row(Ogv2));
    }
    assert_eq!(growth_factor(AllCommercialVehicles, 40), Some(1.45));
    assert_eq!(growth_factor(AllCommercialVehicles, 35), None);
    for years in [0, 3, 12, 45] {
        assert_eq!(growth_factor(Ogv2, years), None);
    }
    // Not listed: G must be entered, and the result says it was.
    let mut input = traffic(vec![Flow::new(Ogv2, 1000.0)], Scheme::Maintenance, 12, 1);
    assert!(design_traffic(&input).is_err());
    input.flows[0].growth_factor = Some(1.12);
    let t = design_traffic(&input).unwrap();
    assert!(close(t.design_traffic_msa, 365.0 * 1000.0 * 1.12 * 3.2e-6 * 12.0, 1e-12));
    assert!(t.warnings.iter().any(|w| w.contains("entered value 1.12")) && t.warnings.iter().any(|w| w.contains("one lane")));
    // Past traffic: G = 1.0 (clause 2.17).
    input.flows[0].growth_factor = None;
    input.past_traffic = true;
    assert_eq!(design_traffic(&input).unwrap().flows[0].growth_factor, 1.0);
    // Equation 2.14 and its note: OGV2 after 10 years is 1.021^10 = 1.23 times the current flow.
    assert_eq!((annual_growth_rate_percent(Ogv1PlusPsv), annual_growth_rate_percent(Ogv2), annual_growth_rate_percent(AllCommercialVehicles)), (1.07, 2.10, 1.54));
    assert_eq!(format!("{:.2}", flow_after_years(1.0, Ogv2, 10)), "1.23");
}

#[test]
fn cd_224_table_2_18_wear_factors() {
    let wm: Vec<f64> = VehicleGroup::ALL.iter().map(|g| wear_factor(*g, Scheme::Maintenance)).collect();
    let wn: Vec<f64> = VehicleGroup::ALL.iter().map(|g| wear_factor(*g, Scheme::New)).collect();
    assert_eq!(wm, [2.6, 0.4, 2.3, 3.0, 1.7, 2.9, 3.7, 1.3, 3.2, 2.7]);
    assert_eq!(wn, [3.9, 0.6, 3.4, 4.6, 2.5, 4.4, 5.6, 1.9, 4.9, 4.0]);
}

#[test]
fn cd_224_table_2_21_heaviest_loaded_lane() {
    for lanes in [2, 3] {
        assert!(close(heaviest_lane_percent(lanes, 0.0), 100.0, 1e-12));
        assert!(close(heaviest_lane_percent(lanes, 5000.0), 82.0, 1e-9));
        assert!(close(heaviest_lane_percent(lanes, 5001.0), 89.0 - 0.0014 * 5001.0, 1e-9));
        assert!(close(heaviest_lane_percent(lanes, 25_000.0), 54.0, 1e-9));
        assert_eq!(heaviest_lane_percent(lanes, 30_000.0), 54.0);
    }
    for lanes in [4, 5] {
        assert!(close(heaviest_lane_percent(lanes, 10_500.0), 62.2, 1e-9));
        // The printed expressions do not quite meet at 10 500.
        assert!(close(heaviest_lane_percent(lanes, 10_501.0), 75.0 - 0.0012 * 10_501.0, 1e-9));
        assert!(close(heaviest_lane_percent(lanes, 25_000.0), 45.0, 1e-9));
        assert_eq!(heaviest_lane_percent(lanes, 26_000.0), 45.0);
    }
    assert_eq!(heaviest_lane_percent(1, 4000.0), 100.0);
}

#[test]
fn cd_226_equation_2_24_asphalt_over_an_hbgm_base() {
    // H = −16.05 (log T)² + 101 log T + 45.8.
    assert!(close(equation_2_24(1.0), 45.8, 1e-12));
    assert!(close(equation_2_24(10.0), -16.05 + 101.0 + 45.8, 1e-12));
    assert!(close(equation_2_24(100.0), -64.2 + 202.0 + 45.8, 1e-9));
    // Notes 2 and 3: between 100 and 180 mm, and 180 mm from 80 msa, where the equation itself
    // arrives (179.9 mm).
    assert_eq!(asphalt_over_hbgm_mm(1.0), Some(100.0));
    assert_eq!(asphalt_over_hbgm_mm(10.0), Some(135.0)); // 130.75, rounded up to 5 mm
    assert_eq!(format!("{:.1}", equation_2_24(80.0)), "179.9");
    assert_eq!(asphalt_over_hbgm_mm(80.0), Some(180.0));
    assert_eq!(asphalt_over_hbgm_mm(400.0), Some(180.0));
    assert_eq!(asphalt_over_hbgm_mm(401.0), None); // Note 1
    assert_eq!(asphalt_over_hbgm_mm(0.0), None);
    // Never falling as traffic rises.
    let mut last = 0.0;
    for i in 1..=400 {
        let h = asphalt_over_hbgm_mm(i as f64).unwrap();
        assert!(h >= last && h % 5.0 == 0.0 && (100.0..=180.0).contains(&h));
        last = h;
    }
    // Worked example A1.1: 60 msa, foundation class 2, HBGM category C: "Total asphalt thickness
    // of 180 mm" read from Figure 2.20. Equation 2.24 gives 174.6 mm, 175 mm rounded up to 5 mm.
    // Recorded, not reconciled: the nomograph's scale is in 10 mm bands.
    assert_eq!(format!("{:.1}", equation_2_24(60.0)), "174.6");
    assert_eq!(asphalt_over_hbgm_mm(60.0), Some(175.0));
}

fn new_road(ogv1: f64, ogv2: f64, foundation_class: u8, pavement: FlexiblePavement) -> DmrbInput {
    DmrbInput {
        traffic: traffic(vec![Flow::new(VehicleGroup::Ogv1PlusPsv, ogv1), Flow::new(VehicleGroup::Ogv2, ogv2)], Scheme::New, 40, 2),
        foundation_class,
        pavement,
    }
}

#[test]
fn cd_226_requirements_and_what_is_left_to_the_nomograph() {
    let hbgm = FlexiblePavement::HbgmBase { category: HbgmCategory::C };
    // The 100 msa road of CD 224's example on a class 2 foundation: over the 80 msa limit.
    let d = design(&new_road(360.0, 840.0, 2, hbgm)).unwrap();
    assert!(close(d.pavement_traffic_msa, d.traffic.design_traffic_msa, 0.0));
    assert_eq!(d.asphalt_over_hbgm_mm, Some(180.0));
    for clause in ["clause 2.5", "clause 2.22", "Figure 2.20", "superseded"] {
        assert!(d.warnings().iter().any(|w| w.contains(clause)), "{clause}");
    }
    assert!(d.layers().is_none(), "no complete structure is claimed");
    let calc = d.calculation();
    assert!(calc[0].contains("CD 224") && calc[0].contains("Revision 0") && calc[0].contains("Version 0.1.0"));
    assert!(calc.iter().any(|l| l.starts_with("OGV2: F = 840 cv/day, G = 1.54, W = 4.9")));
    assert!(calc.iter().any(|l| l.contains("Total thickness of asphalt = 180 mm")));
    assert!(calc.iter().any(|l| l == "Thickness of the HBGM base: not computed (Figure 2.20 only)."));
    // A light road: under 1 msa is designed for 1 msa; class 1 is allowed up to 20 msa.
    let d = design(&new_road(3.0, 7.0, 1, hbgm)).unwrap();
    assert!(d.traffic.design_traffic_msa < 1.0 && d.pavement_traffic_msa == 1.0 && d.asphalt_over_hbgm_mm == Some(100.0));
    assert!(d.warnings.iter().any(|w| w.contains("clause 2.19")) && !d.warnings.iter().any(|w| w.contains("clause 2.4")));
    let d = design(&new_road(100.0, 300.0, 1, hbgm)).unwrap();
    assert!(d.pavement_traffic_msa > 20.0 && d.warnings.iter().any(|w| w.contains("clause 2.4")));
    assert!(d.calculation().iter().any(|l| l.contains("H = −16.05 (log T)² + 101 log T + 45.8")));
    // EME2 on a class 2 foundation; an asphalt base has no computed thickness at all.
    let d = design(&new_road(100.0, 300.0, 2, FlexiblePavement::AsphaltBase { material: AsphaltMaterial::Eme2 })).unwrap();
    assert!(d.asphalt_over_hbgm_mm.is_none() && d.warnings.iter().any(|w| w.contains("clause 2.7")));
    assert!(d.calculation().iter().any(|l| l.contains("EME2") && l.contains("not computed")));
    // A design period other than 40 years on a new carriageway.
    let mut short = new_road(100.0, 300.0, 3, hbgm);
    short.traffic.design_period_years = 20;
    assert!(design(&short).unwrap().warnings.iter().any(|w| w.contains("clause 2.17")));
    assert!(design(&new_road(1.0, 1.0, 5, hbgm)).is_err());

    let boxed = run(&DesignInput::Dmrb(new_road(360.0, 840.0, 3, hbgm))).unwrap();
    assert_eq!(boxed.method(), DesignMethod::DmrbCd226);
    assert!(DesignMethod::all().contains(&DesignMethod::DmrbCd226));
}
