//! The MoWT Road Design Manual Volume 3 Part I (January 2010) design data, table by table, and
//! its Appendix D worked example.

use afrobim_pavement::design::mowt::*;
use afrobim_pavement::design::{design as run, DesignInput, DesignMethod, DesignResult};
use afrobim_pavement::Material;

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// Appendix D: a 7.0 m single carriageway, 1 100 mm of rain a year, subgrade CBR 6 %, 15 years at
/// 2.5 % growth, and the four vehicle categories with their equivalence factors.
fn appendix_d() -> MowtInput {
    MowtInput {
        traffic: TrafficInput {
            direction_a: vec![
                VehicleClass::new("2 axle and tandem trucks", 70.0, 2.0),
                VehicleClass::new("Trucks with drawbar trailer", 15.0, 6.0),
                VehicleClass::new("Articulated units", 8.0, 6.0),
                VehicleClass::new("Buses", 20.0, 1.0),
            ],
            direction_b: None,
            growth_percent: 2.5,
            design_period_years: 15,
            road: RoadType::SingleCarriageway { paved_width_m: 7.0, narrow_road_factor: None },
        },
        subgrade_cbr_percent: 6.0,
        // "The climate shows predominantly wet region"; chart W2 is used, so drainage and
        // maintenance are not both good (Table 4.1). The example does not say which.
        climate: Climate::PredominantlyWet { drainage: Provision::Good, maintenance: Provision::Deficient },
        pavement: PavementType::GranularBaseCementedSubbase,
    }
}

#[test]
fn appendix_d_worked_example() {
    let d = design(&appendix_d()).unwrap();
    // Equation 1 with r = 2.5 %, p = 15: the growth factor is (1.025^15 − 1) / 0.025 = 17.932.
    assert!(close(cumulative_traffic(1.0, 2.5, 15) / 365.0, 17.931927, 1e-6));
    assert!(close(d.directions[0][0].cumulative_vehicles, 458_160.7, 0.1));
    assert!(close(d.directions[0][0].cumulative_esa, 916_321.5, 0.1));
    // "The sum of the cumulative number of standard axles is 1.95 x 10^6 ESA."
    assert!(close(d.direction_esa[0], 1_950_456.0, 1.0));
    assert_eq!(format!("{:.2}", d.direction_esa[0] / 1.0e6), "1.95");
    // Wider than 6.0 m: the most heavily trafficked direction (Table 2.4).
    assert_eq!(d.design_esa, d.direction_esa[0]);
    // "The traffic class is thus T4." "... the soil is classified in S3 subgrade class."
    assert_eq!(d.traffic_class, Some(TrafficClass::T4));
    assert_eq!(d.subgrade_class, Some(SubgradeClass::S3));
    assert_eq!(d.chart_set, Some(ChartSet::Wet));
    // "Design Chart W2 ... Asphalt Concrete 50 mm, Granular Base 150 mm, Cemented Subbase
    // (7 day UCS 1.5 - 3 MPa) 150 mm, Selected Layer 150 mm."
    let s = d.structure.clone().unwrap();
    assert_eq!(s.chart, "W2");
    assert_eq!((s.surfacing, s.base_mm, s.upper_subbase_mm, s.subbase_mm, s.selected_layer_mm), (Surfacing::HotMixAsphalt(50.0), 150.0, None, Some(150.0), Some(150.0)));
    let layers = d.layers().unwrap();
    let got: Vec<(&str, f64)> = layers.iter().map(|l| (l.name.as_str(), l.thickness_mm)).collect();
    assert_eq!(
        got,
        [
            ("Hot mix asphalt surfacing", 50.0),
            ("Granular base (soaked CBR > 80%)", 150.0),
            ("Cemented subbase (7 day UCS 1.5 - 3 MPa)", 150.0),
            ("Selected layer (soaked CBR > 15%)", 150.0)
        ]
    );
    assert_eq!(layers[2].material, Material::CementedSubbase);

    let calc = d.calculation();
    assert!(calc[0].contains("Road Design Manual Volume 3") && calc[0].contains("January 2010"));
    assert!(calc.iter().any(|l| l.contains("Direction A: sum = 1950456 ESAs")));
    assert!(calc.iter().any(|l| l.contains("Traffic class T4 (1.5 to 3 million ESAs, Table 2.5)")));
    assert!(calc.iter().any(|l| l.contains("subgrade class S3 (CBR 5-7%, Table 3.1)") && l.contains("350 mm")));
    assert!(calc.iter().any(|l| l.starts_with("Chart W2: granular base / cemented subbase")));
    assert!(calc.iter().any(|l| l == "  150 mm  Cemented subbase (7 day UCS 1.5 - 3 MPa)"));
    assert!(calc.iter().any(|l| l.contains("Total thickness of the layers = 500 mm")));
    assert!(calc.last().unwrap().contains("not a certified design"));
    // The doubt about the cemented subbase strength travels with the answer.
    assert!(d.warnings().iter().any(|w| w.contains("Table 5.1 gives 0.75 - 1.5 MPa")));

    // The same through the method-neutral entry point.
    let boxed = run(&DesignInput::Mowt(appendix_d())).unwrap();
    assert_eq!(boxed.method(), DesignMethod::MowtRdmVol3);
    assert_eq!(boxed.calculation(), calc);
    assert!(DesignMethod::all().contains(&DesignMethod::MowtRdmVol3));
}

#[test]
fn equation_1_and_equation_2() {
    // Equation 1 is a geometric series: one year is 365 days of T; two years at 10 % is 365·T·2.1.
    assert!(close(cumulative_traffic(100.0, 7.0, 1), 36_500.0, 1e-9));
    assert!(close(cumulative_traffic(100.0, 10.0, 2), 36_500.0 * 2.1, 1e-6));
    // The limit at no growth.
    assert!(close(cumulative_traffic(100.0, 0.0, 20), 36_500.0 * 20.0, 1e-9));
    assert!(close(cumulative_traffic(100.0, 1e-9, 20), 36_500.0 * 20.0, 1e-2));
    // Equation 2: the standard axle is 80 kN or 8 160 kg.
    for n in [3.0, 4.0, 4.5] {
        assert_eq!(load_equivalency(80.0, n), 1.0);
        assert_eq!(load_equivalency_kg(8160.0, n), 1.0);
    }
    assert!(close(load_equivalency(160.0, 4.0), 16.0, 1e-12));
    assert!(close(load_equivalency_kg(16_320.0, 3.0), 8.0, 1e-12));
    // Axle loads go through Equation 2 with the exponent of the pavement type (Table 2.2).
    let axles = DamageFactor::AxleLoadsKn(vec![60.0, 100.0, 100.0]);
    assert!(close(axles.esa_per_vehicle(4.0), 0.75f64.powi(4) + 2.0 * 1.25f64.powi(4), 1e-12));
    let mut input = appendix_d();
    input.traffic.direction_a = vec![VehicleClass { name: "Truck".into(), daily_first_year: 100.0, growth_percent: Some(4.0), damage: axles.clone() }];
    let d = design(&input).unwrap();
    assert_eq!(d.exponent, 3.0);
    assert!(close(d.directions[0][0].esa_per_vehicle, 0.75f64.powi(3) + 2.0 * 1.25f64.powi(3), 1e-12));
    assert_eq!(d.directions[0][0].growth_percent, 4.0);
    assert!(d.calculation().iter().any(|l| l.contains("Equation 2") && l.contains("n = 3")));
}

#[test]
fn table_2_2_damage_exponents() {
    use PavementType::*;
    let n: Vec<f64> = PavementType::ALL.iter().map(|p| p.damage_exponent()).collect();
    assert_eq!(PavementType::ALL, [GranularBaseGranularSubbase, GranularBaseCementedSubbase, CementedBaseCementedSubbase, BituminousBaseGranularSubbase, BituminousBaseCementedSubbase]);
    assert_eq!(n, [4.0, 3.0, 4.5, 4.0, 4.0]);
}

#[test]
fn table_2_3_as_printed() {
    use LoadUnit::*;
    // Kilograms.
    assert_eq!(table_2_3(1000.0, Kilograms, 3.0), None); // "Less than 1500": dashes
    assert_eq!(table_2_3(1500.0, Kilograms, 3.0), Some(0.02));
    assert_eq!(table_2_3(2499.0, Kilograms, 4.0), None); // a dash
    assert_eq!(table_2_3(3000.0, Kilograms, 4.5), Some(0.01));
    assert_eq!(table_2_3(8000.0, Kilograms, 3.0), Some(0.95));
    assert_eq!(table_2_3(8000.0, Kilograms, 4.0), Some(0.95));
    assert_eq!(table_2_3(8000.0, Kilograms, 4.5), Some(0.94));
    assert_eq!(table_2_3(13_000.0, Kilograms, 4.0), Some(6.50));
    assert_eq!(table_2_3(14_000.0, Kilograms, 4.5), Some(11.46));
    assert_eq!(table_2_3(20_499.0, Kilograms, 3.0), Some(14.75));
    assert_eq!(table_2_3(20_499.0, Kilograms, 4.5), Some(56.77));
    assert_eq!(table_2_3(20_500.0, Kilograms, 4.0), None); // above the table
    // Kilonewtons.
    assert_eq!(table_2_3(14.0, Kilonewtons, 3.0), None);
    assert_eq!(table_2_3(20.0, Kilonewtons, 3.0), Some(0.02));
    assert_eq!(table_2_3(40.0, Kilonewtons, 3.0), Some(0.13));
    assert_eq!(table_2_3(80.0, Kilonewtons, 3.0), Some(0.99));
    assert_eq!(table_2_3(80.0, Kilonewtons, 4.5), Some(1.00));
    assert_eq!(table_2_3(140.0, Kilonewtons, 4.0), Some(9.3));
    assert_eq!(table_2_3(200.0, Kilonewtons, 4.0), Some(38.79));
    assert_eq!(table_2_3(200.0, Kilonewtons, 4.5), Some(61.32));
    assert_eq!(table_2_3(205.0, Kilonewtons, 3.0), None);
    assert_eq!(table_2_3(100.0, Kilonewtons, 5.0), None); // not a tabulated exponent
    // Every band rises with load and, above the standard axle, with the exponent.
    for unit in [Kilograms, Kilonewtons] {
        let step = if unit == Kilograms { 1000.0 } else { 10.0 };
        for n in [3.0, 4.0, 4.5] {
            let column: Vec<f64> = (0..19).filter_map(|i| table_2_3((i as f64 + 2.0) * step, unit, n)).collect();
            assert!(column.windows(2).all(|w| w[1] > w[0]), "{unit:?} n = {n}: {column:?}");
        }
        let heavy = 12.0 * step;
        assert!(table_2_3(heavy, unit, 3.0) < table_2_3(heavy, unit, 4.0) && table_2_3(heavy, unit, 4.0) < table_2_3(heavy, unit, 4.5));
    }
    // Recorded, not corrected: the table is not Equation 2 at the middle of the band. At
    // 8 000 kg and n = 4 the equation gives 0.92 where the table prints 0.95; at the top band it
    // gives 56.5 (kg, n = 4.5) where the table prints 56.77.
    assert_eq!(format!("{:.2}", load_equivalency_kg(8000.0, 4.0)), "0.92");
    assert_eq!(format!("{:.1}", load_equivalency_kg(20_000.0, 4.5)), "56.5");
    // It is close to the equation all the same: within 5 % from the standard-axle band upwards.
    for i in 8..=20 {
        for n in [3.0, 4.0, 4.5] {
            let t = table_2_3(i as f64 * 1000.0, Kilograms, n).unwrap();
            assert!((t / load_equivalency_kg(i as f64 * 1000.0, n) - 1.0).abs() < 0.05, "{i} t, n = {n}");
        }
    }
}

#[test]
fn table_2_4_design_traffic_loading() {
    let loading = |road: RoadType, b: f64| {
        let mut i = appendix_d();
        i.traffic.road = road;
        // Direction B carries `b` times direction A.
        i.traffic.direction_b = Some(i.traffic.direction_a.iter().cloned().map(|mut c| { c.daily_first_year *= b; c }).collect());
        let d = design(&i).unwrap();
        (d.design_esa / d.direction_esa[0], d.loading_rule.clone(), d.warnings.clone())
    };
    let single = |w: f64, f: Option<f64>| RoadType::SingleCarriageway { paved_width_m: w, narrow_road_factor: f };
    let dual = |cv: f64| RoadType::DualCarriageway { commercial_vehicles_per_day_one_direction: cv };
    // B = 0.5 A, so the sum is 1.5 A and the heavier direction is A.
    assert!(close(loading(single(3.0, None), 0.5).0, 3.0, 1e-12)); // double the sum
    assert!(close(loading(single(3.5, None), 0.5).0, 3.0, 1e-12));
    assert!(close(loading(single(4.0, None), 0.5).0, 3.0, 1e-12)); // "up to twice": 2 unless entered
    assert!(close(loading(single(4.5, Some(1.0)), 0.5).0, 1.5, 1e-12)); // at least the total
    assert!(close(loading(single(4.5, Some(1.4)), 0.5).0, 2.1, 1e-12));
    assert!(close(loading(single(5.0, None), 0.5).0, 1.2, 1e-12)); // 80 % of the sum
    assert!(close(loading(single(6.0, None), 0.5).0, 1.2, 1e-12));
    assert!(close(loading(single(6.1, None), 0.5).0, 1.0, 1e-12)); // heavier direction
    assert!(close(loading(single(7.0, None), 2.0).0, 2.0, 1e-12)); // which is B here
    assert!(close(loading(dual(1500.0), 0.5).0, 0.9, 1e-12));
    assert!(close(loading(dual(2500.0), 0.5).0, 0.8, 1e-12));
    let (factor, rule, warnings) = loading(dual(2000.0), 0.5);
    assert!(close(factor, 0.9, 1e-12) && rule.contains("90%") && warnings.iter().any(|w| w.contains("neither row")));
    let mut bad = appendix_d();
    bad.traffic.road = single(4.0, Some(2.5));
    assert!(design(&bad).is_err());
}

#[test]
fn table_2_5_traffic_classes() {
    use TrafficClass::*;
    let ranges: Vec<(f64, f64)> = TrafficClass::ALL.iter().map(|c| c.range_million_esa()).collect();
    assert_eq!(ranges, [(0.0, 0.3), (0.3, 0.7), (0.7, 1.5), (1.5, 3.0), (3.0, 6.0), (6.0, 10.0), (10.0, 17.0), (17.0, 30.0)]);
    for (esa, class) in [
        (0.0, T1), (0.29, T1), (0.3, T2), (0.69, T2), (0.7, T3), (1.49, T3), (1.5, T4), (1.95, T4), (2.99, T4), (3.0, T5), (5.9, T5), (6.0, T6),
        (9.9, T6), (10.0, T7), (16.9, T7), (17.0, T8), (29.9, T8), (30.0, T8),
    ] {
        assert_eq!(TrafficClass::for_million_esa(esa), Some(class), "{esa}");
    }
    assert_eq!(TrafficClass::for_million_esa(30.1), None);
    assert_eq!(TrafficClass::for_million_esa(-1.0), None);
    assert_eq!(TrafficClass::for_million_esa(f64::NAN), None);
    assert_eq!(T6.label(), "T6");
}

#[test]
fn tables_3_1_and_3_3_subgrade_classes() {
    use SubgradeClass::*;
    for (cbr, class) in [(2.0, S1), (2.9, S1), (3.0, S2), (4.0, S2), (4.5, S2), (4.99, S2), (5.0, S3), (6.0, S3), (7.0, S3), (7.9, S3), (8.0, S4), (14.0, S4), (15.0, S5), (29.0, S5), (30.0, S6), (80.0, S6)] {
        assert_eq!(SubgradeClass::for_cbr(cbr), Some(class), "{cbr}");
    }
    assert_eq!(SubgradeClass::for_cbr(1.9), None);
    assert_eq!(SubgradeClass::for_cbr(f64::NAN), None);
    let printed: Vec<&str> = SubgradeClass::ALL.iter().map(|c| c.printed_range()).collect();
    assert_eq!(printed, ["2", "3-4", "5-7", "8-14", "15-29", "30+"]);
    let depth: Vec<f64> = SubgradeClass::ALL.iter().map(|c| c.minimum_depth_mm()).collect();
    assert_eq!(depth, [250.0, 250.0, 350.0, 450.0, 550.0, 650.0]);
}

#[test]
fn table_2_1_equation_3_and_table_4_1() {
    use Level::*;
    assert_eq!(design_life_guidance_years(Low, Low), (10, 15));
    assert_eq!(design_life_guidance_years(Low, High), (15, 15));
    assert_eq!(design_life_guidance_years(High, Low), (10, 20));
    assert_eq!(design_life_guidance_years(High, High), (15, 20));
    assert!(close(design_moisture_content_percent(12.0), 8.84, 1e-12));

    use Provision::*;
    let wet = |drainage, maintenance| Climate::PredominantlyWet { drainage, maintenance };
    for class in TrafficClass::ALL {
        let low = class <= TrafficClass::T2;
        assert_eq!(Climate::PredominantlyDry.chart_set(class), ChartSet::Dry);
        assert_eq!(wet(Good, Good).chart_set(class), ChartSet::Dry);
        assert_eq!(wet(Deficient, Deficient).chart_set(class), ChartSet::Wet);
        let mixed = if low { ChartSet::Dry } else { ChartSet::Wet };
        assert_eq!(wet(Good, Deficient).chart_set(class), mixed);
        assert_eq!(wet(Deficient, Good).chart_set(class), mixed);
    }
}

type Row = (&'static str, f64, Option<f64>, Option<f64>, Option<f64>);

fn cell(pavement: PavementType, set: ChartSet, s: SubgradeClass, t: TrafficClass) -> Option<Row> {
    catalogue_structure(pavement, set, t, s).map(|c| {
        let surfacing = match c.surfacing {
            Surfacing::SurfaceDressing => "SD",
            Surfacing::HotMixAsphalt(t) if t == 40.0 => "40",
            Surfacing::HotMixAsphalt(t) if t == 50.0 => "50",
            Surfacing::HotMixAsphalt(t) if t == 100.0 => "100",
            Surfacing::HotMixAsphalt(t) if t == 125.0 => "125",
            Surfacing::HotMixAsphalt(t) if t == 150.0 => "150",
            Surfacing::HotMixAsphalt(_) => "?",
        };
        (surfacing, c.base_mm, c.upper_subbase_mm, c.subbase_mm, c.selected_layer_mm)
    })
}

/// Number of structures on a chart and the sum of all their thicknesses, mm.
fn chart_totals(pavement: PavementType, set: ChartSet) -> (usize, f64) {
    let mut out = (0, 0.0);
    for s in SubgradeClass::ALL {
        for t in TrafficClass::ALL {
            if let Some(c) = catalogue_structure(pavement, set, t, s) {
                out = (out.0 + 1, out.1 + c.total_thickness_mm());
            }
        }
    }
    out
}

/// Appendix C, chart by chart: the corners and a middle cell of each chart as read from the
/// page, the extent of each chart, and the sum of every thickness on it (which fixes the
/// table against later edits; the sums were taken from the reading that was checked against the
/// drawn column heights, see the `mowt_catalogue` source).
#[test]
fn appendix_c_catalogue() {
    use ChartSet::*;
    use PavementType::*;
    use SubgradeClass::*;
    use TrafficClass::*;

    // D1, granular base / granular subbase, dry.
    assert_eq!(cell(GranularBaseGranularSubbase, Dry, S1, T1), Some(("SD", 150.0, None, Some(175.0), Some(300.0))));
    assert_eq!(cell(GranularBaseGranularSubbase, Dry, S1, T8), Some(("50", 200.0, None, Some(325.0), Some(400.0))));
    assert_eq!(cell(GranularBaseGranularSubbase, Dry, S3, T5), Some(("50", 200.0, None, Some(300.0), None)));
    assert_eq!(cell(GranularBaseGranularSubbase, Dry, S3, T6), Some(("50", 200.0, None, Some(200.0), Some(150.0))));
    assert_eq!(cell(GranularBaseGranularSubbase, Dry, S4, T5), Some(("SD", 200.0, None, Some(200.0), None)));
    assert_eq!(cell(GranularBaseGranularSubbase, Dry, S6, T1), Some(("SD", 100.0, None, None, None)));
    assert_eq!(cell(GranularBaseGranularSubbase, Dry, S6, T8), Some(("SD", 200.0, None, None, None)));
    assert_eq!(chart_totals(GranularBaseGranularSubbase, Dry), (48, 21_850.0));

    // D2, granular base / cemented subbase, dry.
    assert_eq!(cell(GranularBaseCementedSubbase, Dry, S1, T1), Some(("SD", 150.0, None, Some(150.0), Some(300.0))));
    assert_eq!(cell(GranularBaseCementedSubbase, Dry, S1, T5), Some(("SD", 150.0, Some(125.0), Some(125.0), Some(300.0))));
    assert_eq!(cell(GranularBaseCementedSubbase, Dry, S1, T8), Some(("50", 150.0, Some(150.0), Some(150.0), Some(300.0))));
    assert_eq!(cell(GranularBaseCementedSubbase, Dry, S3, T5), Some(("SD", 150.0, None, Some(225.0), Some(150.0))));
    assert_eq!(cell(GranularBaseCementedSubbase, Dry, S4, T5), Some(("SD", 150.0, None, Some(150.0), Some(100.0))));
    assert_eq!(cell(GranularBaseCementedSubbase, Dry, S6, T6), Some(("SD", 175.0, None, None, None)));
    assert_eq!(cell(GranularBaseCementedSubbase, Dry, S6, T8), Some(("50", 200.0, None, None, None)));
    assert_eq!(chart_totals(GranularBaseCementedSubbase, Dry), (48, 21_150.0));

    // D3, cemented base / cemented subbase, dry: surface dressing throughout, T1 to T6 only.
    assert_eq!(cell(CementedBaseCementedSubbase, Dry, S1, T1), Some(("SD", 150.0, None, Some(150.0), Some(350.0))));
    assert_eq!(cell(CementedBaseCementedSubbase, Dry, S1, T6), Some(("SD", 200.0, None, Some(250.0), Some(350.0))));
    assert_eq!(cell(CementedBaseCementedSubbase, Dry, S2, T4), Some(("SD", 175.0, None, Some(200.0), Some(225.0))));
    assert_eq!(cell(CementedBaseCementedSubbase, Dry, S4, T6), Some(("SD", 150.0, None, Some(150.0), Some(200.0))));
    assert_eq!(cell(CementedBaseCementedSubbase, Dry, S6, T6), Some(("SD", 200.0, None, None, None)));
    assert_eq!(cell(CementedBaseCementedSubbase, Dry, S1, T7), None);
    assert_eq!(chart_totals(CementedBaseCementedSubbase, Dry), (36, 15_825.0));
    assert!(catalogue_structure(CementedBaseCementedSubbase, Dry, T4, S2).unwrap().note.unwrap().contains("drawn about 25 mm shorter"));
    assert!(catalogue_structure(CementedBaseCementedSubbase, Dry, T3, S2).unwrap().note.is_none());

    // D4, bituminous base / granular subbase, dry: T5 to T8 only.
    assert_eq!(cell(BituminousBaseGranularSubbase, Dry, S1, T4), None);
    assert_eq!(cell(BituminousBaseGranularSubbase, Dry, S1, T5), Some(("SD", 125.0, None, Some(200.0), Some(350.0))));
    assert_eq!(cell(BituminousBaseGranularSubbase, Dry, S1, T8), Some(("50", 200.0, None, Some(200.0), Some(350.0))));
    assert_eq!(cell(BituminousBaseGranularSubbase, Dry, S3, T7), Some(("50", 125.0, None, Some(200.0), Some(100.0))));
    assert_eq!(cell(BituminousBaseGranularSubbase, Dry, S6, T8), Some(("50", 175.0, None, None, None)));
    assert_eq!(chart_totals(BituminousBaseGranularSubbase, Dry), (24, 10_325.0));

    // D5, bituminous base / cemented subbase, dry: T5 to T8 only; 40 mm surfacings, 90 and 120 mm bases.
    assert_eq!(cell(BituminousBaseCementedSubbase, Dry, S1, T5), Some(("SD", 90.0, None, Some(250.0), Some(300.0))));
    assert_eq!(cell(BituminousBaseCementedSubbase, Dry, S1, T8), Some(("40", 120.0, None, Some(300.0), Some(300.0))));
    assert_eq!(cell(BituminousBaseCementedSubbase, Dry, S3, T7), Some(("40", 90.0, None, Some(350.0), None)));
    assert_eq!(cell(BituminousBaseCementedSubbase, Dry, S3, T8), Some(("40", 120.0, None, Some(300.0), Some(100.0))));
    assert_eq!(cell(BituminousBaseCementedSubbase, Dry, S6, T5), Some(("SD", 50.0, None, Some(150.0), None)));
    assert_eq!(chart_totals(BituminousBaseCementedSubbase, Dry), (24, 10_485.0));

    // W1, granular base / granular subbase, wet: asphalt from T3, thickening to 150 mm at T8.
    assert_eq!(cell(GranularBaseGranularSubbase, Wet, S1, T1), Some(("SD", 150.0, None, Some(175.0), Some(300.0))));
    assert_eq!(cell(GranularBaseGranularSubbase, Wet, S1, T3), Some(("50", 175.0, None, Some(200.0), Some(300.0))));
    assert_eq!(cell(GranularBaseGranularSubbase, Wet, S1, T8), Some(("150", 200.0, None, Some(300.0), Some(350.0))));
    assert_eq!(cell(GranularBaseGranularSubbase, Wet, S3, T6), Some(("100", 200.0, None, Some(250.0), None)));
    assert_eq!(cell(GranularBaseGranularSubbase, Wet, S5, T7), Some(("125", 175.0, None, Some(150.0), None)));
    assert_eq!(cell(GranularBaseGranularSubbase, Wet, S6, T8), Some(("150", 250.0, None, None, None)));
    assert_eq!(chart_totals(GranularBaseGranularSubbase, Wet), (48, 23_925.0));

    // W2, granular base / cemented subbase, wet.
    assert_eq!(cell(GranularBaseCementedSubbase, Wet, S1, T1), Some(("SD", 150.0, None, Some(150.0), Some(300.0))));
    assert_eq!(cell(GranularBaseCementedSubbase, Wet, S1, T7), Some(("125", 150.0, Some(125.0), Some(125.0), Some(350.0))));
    assert_eq!(cell(GranularBaseCementedSubbase, Wet, S2, T7), Some(("125", 150.0, None, Some(250.0), Some(200.0))));
    assert_eq!(cell(GranularBaseCementedSubbase, Wet, S2, T8), Some(("150", 150.0, Some(125.0), Some(125.0), Some(200.0))));
    assert_eq!(cell(GranularBaseCementedSubbase, Wet, S3, T4), Some(("50", 150.0, None, Some(150.0), Some(150.0)))); // Appendix D
    assert_eq!(cell(GranularBaseCementedSubbase, Wet, S6, T5), Some(("50", 200.0, None, None, None)));
    assert_eq!(cell(GranularBaseCementedSubbase, Wet, S6, T8), Some(("150", 100.0, None, Some(150.0), None)));
    assert_eq!(chart_totals(GranularBaseCementedSubbase, Wet), (48, 22_900.0));

    // W3, cemented base / cemented subbase, wet: surface dressing throughout, T1 to T6 only.
    assert_eq!(cell(CementedBaseCementedSubbase, Wet, S1, T1), Some(("SD", 150.0, None, Some(150.0), Some(350.0))));
    assert_eq!(cell(CementedBaseCementedSubbase, Wet, S2, T4), Some(("SD", 200.0, None, Some(175.0), Some(225.0))));
    assert_eq!(cell(CementedBaseCementedSubbase, Wet, S2, T6), Some(("SD", 200.0, None, Some(275.0), Some(225.0))));
    assert_eq!(cell(CementedBaseCementedSubbase, Wet, S4, T4), Some(("SD", 200.0, None, Some(100.0), Some(100.0))));
    assert_eq!(cell(CementedBaseCementedSubbase, Wet, S6, T6), Some(("SD", 250.0, None, None, None)));
    assert_eq!(cell(CementedBaseCementedSubbase, Wet, S6, T8), None);
    assert_eq!(chart_totals(CementedBaseCementedSubbase, Wet), (36, 16_125.0));

    // W4, bituminous base / granular subbase, wet: 50 mm asphalt throughout, T5 to T8 only.
    assert_eq!(cell(BituminousBaseGranularSubbase, Wet, S1, T5), Some(("50", 125.0, None, Some(200.0), Some(350.0))));
    assert_eq!(cell(BituminousBaseGranularSubbase, Wet, S1, T8), Some(("50", 200.0, None, Some(200.0), Some(400.0))));
    assert_eq!(cell(BituminousBaseGranularSubbase, Wet, S3, T5), Some(("50", 125.0, None, Some(175.0), Some(100.0))));
    assert_eq!(cell(BituminousBaseGranularSubbase, Wet, S6, T8), Some(("50", 200.0, None, None, None)));
    assert_eq!(cell(BituminousBaseGranularSubbase, Wet, S6, T4), None);
    assert_eq!(chart_totals(BituminousBaseGranularSubbase, Wet), (24, 11_500.0));

    // W5, bituminous base / cemented subbase, wet: T5 to T8 only.
    assert_eq!(cell(BituminousBaseCementedSubbase, Wet, S1, T5), Some(("40", 90.0, None, Some(250.0), Some(300.0))));
    assert_eq!(cell(BituminousBaseCementedSubbase, Wet, S1, T8), Some(("50", 150.0, None, Some(300.0), Some(300.0))));
    assert_eq!(cell(BituminousBaseCementedSubbase, Wet, S3, T8), Some(("40", 120.0, None, Some(400.0), None)));
    assert_eq!(cell(BituminousBaseCementedSubbase, Wet, S6, T8), Some(("40", 90.0, None, Some(150.0), None)));
    assert_eq!(chart_totals(BituminousBaseCementedSubbase, Wet), (24, 11_545.0));

    // Chart names, and no thickness that is not a whole number of the catalogue's steps: every
    // layer is a multiple of 25 mm except the 40, 90 and 120 mm bituminous layers of D5 and W5.
    assert_eq!(chart_name(GranularBaseGranularSubbase, Dry), "D1");
    assert_eq!(chart_name(BituminousBaseCementedSubbase, Wet), "W5");
    for p in PavementType::ALL {
        for set in [Dry, Wet] {
            for s in SubgradeClass::ALL {
                for t in TrafficClass::ALL {
                    let Some(c) = catalogue_structure(p, set, t, s) else { continue };
                    assert_eq!(c.chart, chart_name(p, set));
                    for l in c.layers() {
                        let odd = p == BituminousBaseCementedSubbase && [40.0, 90.0, 120.0].contains(&l.thickness_mm);
                        assert!(l.thickness_mm % 25.0 == 0.0 || odd, "{} {:?} {:?}: {}", c.chart, s, t, l.thickness_mm);
                    }
                    // A thicker structure is never given for a stronger subgrade at the same traffic.
                    if let Some(weaker) = SubgradeClass::ALL.get(s.index().wrapping_sub(1)).and_then(|w| catalogue_structure(p, set, t, *w)) {
                        assert!(c.total_thickness_mm() <= weaker.total_thickness_mm(), "{} {:?} {:?}", c.chart, s, t);
                    }
                }
            }
        }
    }
}

#[test]
fn outside_the_manual_there_is_no_structure_but_there_is_a_calculation() {
    // More than 30 million ESAs (section 5.6).
    let mut heavy = appendix_d();
    heavy.traffic.direction_a[0].daily_first_year = 5000.0;
    let d = design(&heavy).unwrap();
    assert!(d.traffic_class.is_none() && d.structure.is_none() && d.layers().is_none());
    assert!(d.warnings.iter().any(|w| w.contains("section 5.6")));
    assert!(d.calculation().iter().any(|l| l.contains("No catalogue structure")));
    // CBR below 2 % (section 5.5).
    let mut soft = appendix_d();
    soft.subgrade_cbr_percent = 1.5;
    let d = design(&soft).unwrap();
    assert!(d.subgrade_class.is_none() && d.structure.is_none() && d.warnings.iter().any(|w| w.contains("section 5.5")));
    // A chart with nothing at this traffic class.
    let mut bituminous = appendix_d();
    bituminous.pavement = PavementType::BituminousBaseGranularSubbase;
    let d = design(&bituminous).unwrap();
    assert!(d.structure.is_none() && d.warnings.iter().any(|w| w.contains("Chart W4") && w.contains("no structure for traffic class T4")));
    // Reminders: a 25 year period, 1 % growth, very light traffic.
    let mut odd = appendix_d();
    odd.traffic.design_period_years = 25;
    odd.traffic.growth_percent = 1.0;
    odd.traffic.direction_a.truncate(1);
    odd.traffic.direction_a[0].daily_first_year = 1.0;
    let d = design(&odd).unwrap();
    assert_eq!(d.traffic_class, Some(TrafficClass::T1));
    for clause in ["section 2.1", "section 2.2 (a)(iii)", "section 2.3"] {
        assert!(d.warnings.iter().any(|w| w.contains(clause)), "{clause}");
    }
    // Input that cannot be used.
    let mut bad = appendix_d();
    bad.traffic.design_period_years = 0;
    assert!(design(&bad).is_err());
    let mut bad = appendix_d();
    bad.traffic.direction_a.clear();
    assert!(design(&bad).is_err());
    let mut bad = appendix_d();
    bad.subgrade_cbr_percent = 0.0;
    assert!(design(&bad).is_err());
}

#[test]
fn a_design_becomes_a_pavement() {
    use afrobim_alignment::AlignmentBuilder;
    use afrobim_pavement::{measure_definition, PavementDefinition};
    let layers = design(&appendix_d()).unwrap().layers().unwrap();
    let a = AlignmentBuilder::new("A", 0.0, 0.0, 0.0, 0.0).line(1000.0).build().unwrap();
    let rows = measure_definition("P", &PavementDefinition::new(a, 7.0, 2.5, layers)).unwrap();
    let volumes: Vec<f64> = rows.iter().map(|r| r.volume_m3).collect();
    assert_eq!(volumes, [350.0, 1050.0, 1050.0, 1050.0]);
    // A surface dressing structure: the seal is measured by area only.
    let mut light = appendix_d();
    light.traffic.direction_a.truncate(1);
    light.traffic.direction_a[0].daily_first_year = 5.0;
    let d = design(&light).unwrap();
    let layers = d.layers().unwrap();
    assert_eq!((layers[0].material.clone(), layers[0].thickness_mm), (Material::SurfaceDressing, 0.0));
    assert!(d.calculation().iter().any(|l| l.contains("shown as \"SD\"")));
}
