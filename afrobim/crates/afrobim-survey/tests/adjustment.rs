// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Adjustment tests: simulated networks with known truth, closed forms and the published levelling
//! example of Teunissen, Network Quality Control (TU Delft OPEN, 2024), section 4.2.4.

use afrobim_survey::*;

const E0: f64 = 500_000.0;
const N0: f64 = 9_960_000.0; // true UTM magnitude

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
    fn normal(&mut self) -> f64 {
        let (a, b) = (self.next(), self.next());
        (-2.0 * a.ln()).sqrt() * (2.0 * std::f64::consts::PI * b).cos()
    }
}

fn equipment() -> Equipment {
    Equipment {
        name: "T1".into(),
        sigma_distance_const_m: 0.003,
        sigma_distance_ppm: 2.0,
        sigma_pointing_arcsec: 1.0,
        sigma_reading_arcsec: 1.0,
        sigma_zenith_arcsec: 2.0,
        centring_instrument_m: 0.001,
        centring_target_m: 0.001,
        sigma_instrument_height_m: 0.001,
        sigma_target_height_m: 0.001,
        level_sigma_const_m: 0.0,
        level_sigma_per_sqrt_km_m: 0.0,
        horizontal_collimation_deg: None,
        vertical_index_error_deg: None,
    }
}

fn truth() -> Vec<(&'static str, f64, f64)> {
    vec![("A", E0, N0), ("B", E0 + 400.0, N0 + 50.0), ("P1", E0 + 150.0, N0 + 220.0), ("P2", E0 + 300.0, N0 + 260.0), ("P3", E0 + 50.0, N0 + 120.0)]
}

/// Build a database whose observations are computed from the true coordinates, with optional noise.
/// A and B are fixed control points; P1 to P3 start from perturbed approximate coordinates.
fn simulate(noise: Option<(&mut Lcg, &Equipment)>, gross: Option<(&str, &str, f64)>) -> SurveyDatabase {
    let tr = truth();
    let mut db = SurveyDatabase::new("sim");
    db.add_network("N", "t").unwrap();
    db.equipment.push(equipment());
    let (mut rng, eq) = match noise {
        Some((r, e)) => (Some(r), Some(e)),
        None => (None, None),
    };
    let net = db.network_mut("N").unwrap();
    for (i, (n, e, nn)) in tr.iter().enumerate() {
        if i < 2 {
            net.points.push(SurveyPoint::control(n, *e, *nn, None));
        } else {
            net.points.push(SurveyPoint::given(n, e + 3.0 - i as f64, nn - 2.0 + i as f64, None));
        }
    }
    let stations = [("A", "B"), ("B", "A"), ("P1", "A"), ("P2", "B"), ("P3", "A")];
    let mut jitter = |sigma: f64| match (&mut rng, eq) {
        (Some(r), Some(_)) => r.normal() * sigma,
        _ => 0.0,
    };
    for (k, (st, bs)) in stations.iter().enumerate() {
        let (se, sn) = tr.iter().find(|t| t.0 == *st).map(|t| (t.1, t.2)).unwrap();
        let omega = 17.3 * (k as f64 + 1.0);
        let mut s = Setup::new(st, 1.5);
        s.equipment = Some("T1".into());
        s.backsight_point = Some((*bs).into());
        for (name, e, n) in &tr {
            if name == st {
                continue;
            }
            let (de, dn) = (e - se, n - sn);
            let az = de.atan2(dn).to_degrees();
            let d = de.hypot(dn);
            let mut o = Observation::new(name);
            o.hz_kind = HzKind::Direction;
            // the model of the adjustment: pointing and reading, plus centring over the distance
            let sig_dir = ((2.0 / 206_264.806_247_096_36f64.powi(2)) + 2e-6 / (d * d)).sqrt().to_degrees();
            let reading = (az - omega + jitter(sig_dir) + 720.0) % 360.0;
            o.hz_deg = Some(reading);
            let mut dist = d + jitter((0.003f64.powi(2) + (2e-6 * d).powi(2) + 2e-6).sqrt());
            if let Some((a, b, err)) = gross {
                if a == *st && b == *name {
                    dist += err;
                }
            }
            o.slope_distance = Some(dist);
            o.zenith_deg = Some(90.0);
            if name == bs {
                s.backsight_reading_deg = reading;
            }
            s.observations.push(o);
        }
        net.setups.push(s);
    }
    db
}

fn opts() -> AdjustOptions {
    let mut o = AdjustOptions::new(10, 1e-10, 0.95, 0.01, 3.29);
    o.default_equipment = Some("T1".into());
    o
}

#[test]
fn noise_free_network_returns_true_coordinates() {
    let db = simulate(None, None);
    let a = adjust_network_2d(&db, "N", &opts()).unwrap();
    assert!(a.converged, "iterations {}", a.iterations);
    for (name, e, n) in truth() {
        let p = a.point(name).unwrap();
        assert!((p.e - e).abs() < 1e-6 && (p.n - n).abs() < 1e-6, "{name}: {} {} vs {e} {n}", p.e, p.n);
    }
    assert!(a.vtpv < 1e-6, "noise free: v'Pv = {}", a.vtpv);
    assert!(a.dof > 20);
    assert!(a.observations.iter().all(|o| !o.flagged));
    // sum of redundancy numbers equals the degrees of freedom
    let sum: f64 = a.observations.iter().map(|o| o.redundancy).sum();
    assert!((sum - a.dof as f64).abs() < 1e-6);
    assert!(a.point("A").unwrap().fixed);
    // the report mentions the global test and every point
    let rep = adjustment_report(&a);
    assert!(rep.contains("Global test") && rep.contains("P3") && rep.contains("Blunder rule"));
}

#[test]
fn gross_error_is_flagged_on_the_right_observation_and_snooping_removes_it() {
    let db = simulate(None, Some(("A", "P1", 0.5)));
    let a = adjust_network_2d(&db, "N", &opts()).unwrap();
    let flagged: Vec<&ObsResult> = a.observations.iter().filter(|o| o.flagged).collect();
    let worst = a.observations.iter().filter(|o| o.std_residual.is_some()).max_by(|x, y| x.std_residual.unwrap().abs().partial_cmp(&y.std_residual.unwrap().abs()).unwrap()).unwrap();
    assert!(!flagged.is_empty());
    assert_eq!((worst.kind, worst.from.as_str(), worst.to.as_str()), (ObsKind::Distance, "A", "P1"), "largest |w| is on {:?}", worst);
    assert!(worst.flagged);
    // automatic snooping removes it and then recovers the truth
    let s = adjust_with_snooping(&db, "N", &opts(), 3).unwrap();
    assert_eq!(s.removed.len(), 1, "{:?}", s.removed);
    assert!(s.removed[0].0.ends_with(":P1:dist") && s.removed[0].0.starts_with("S"), "{:?}", s.removed);
    let setup_a = db.network("N").unwrap().setups[0].id;
    let _ = setup_a;
    for (name, e, n) in truth() {
        let p = s.adjustment.point(name).unwrap();
        assert!((p.e - e).abs() < 1e-5 && (p.n - n).abs() < 1e-5, "{name}");
    }
}

#[test]
fn gross_error_in_a_direction_is_found() {
    let mut db = simulate(None, None);
    // spoil the reading from P1 to P2 by 0.01 degrees (36 arc seconds)
    let net = db.network_mut("N").unwrap();
    let su = net.setups.iter_mut().find(|s| s.station == "P1").unwrap();
    let o = su.observations.iter_mut().find(|o| o.target == "P2").unwrap();
    o.hz_deg = Some(o.hz_deg.unwrap() + 0.01);
    let a = adjust_network_2d(&db, "N", &opts()).unwrap();
    let worst = a.observations.iter().filter(|o| o.std_residual.is_some()).max_by(|x, y| x.std_residual.unwrap().abs().partial_cmp(&y.std_residual.unwrap().abs()).unwrap()).unwrap();
    assert_eq!((worst.kind, worst.from.as_str(), worst.to.as_str()), (ObsKind::Direction, "P1", "P2"));
    assert!(worst.flagged);
}

#[test]
fn global_test_behaves_with_simulated_noise() {
    // With errors drawn from exactly the stated standard errors the statistic v'Pv is chi-square with
    // dof degrees of freedom. Over many runs it must fall in the two sided 1 percent limits in most runs
    // and its mean must be close to dof.
    let eq = equipment();
    let (mut pass, mut sum_ratio, runs) = (0, 0.0, 60);
    for seed in 0..runs {
        let mut rng = Lcg(1234 + 7919 * seed as u64);
        let db = simulate(Some((&mut rng, &eq)), None);
        let a = adjust_network_2d(&db, "N", &opts()).unwrap();
        assert!(a.converged, "seed {seed}: iterations {} last step {:e}", a.iterations, a.last_step_norm);
        let g = a.global_test.unwrap();
        if g.passes {
            pass += 1;
        }
        sum_ratio += g.statistic / g.dof as f64;
    }
    assert!(pass as f64 >= 0.93 * runs as f64, "only {pass} of {runs} runs within the 99 percent limits");
    let mean = sum_ratio / runs as f64;
    assert!(mean > 0.8 && mean < 1.2, "mean v'Pv/dof = {mean}");
}

fn two_distance_network(s1: f64, s2: f64) -> SurveyDatabase {
    // P at (E0+100, N0+100). Distance from A (E0, N0+100), due west of P: constrains E.
    // Distance from B (E0+100, N0), due south of P: constrains N. Independent, so Q = diag(s1^2, s2^2).
    let mut db = SurveyDatabase::new("e");
    db.add_network("N", "t").unwrap();
    let mut e = Equipment::exact("E");
    e.sigma_distance_const_m = 1.0; // overwritten per setup below through two equipment records
    db.equipment.push(Equipment { name: "E1".into(), sigma_distance_const_m: s1, ..Equipment::exact("E1") });
    db.equipment.push(Equipment { name: "E2".into(), sigma_distance_const_m: s2, ..Equipment::exact("E2") });
    let net = db.network_mut("N").unwrap();
    net.points.push(SurveyPoint::control("A", E0, N0 + 100.0, None));
    net.points.push(SurveyPoint::control("B", E0 + 100.0, N0, None));
    net.points.push(SurveyPoint::given("P", E0 + 101.0, N0 + 99.0, None));
    for (st, eq) in [("A", "E1"), ("B", "E2")] {
        let mut s = Setup::new(st, 0.0);
        s.equipment = Some(eq.into());
        let mut o = Observation::new("P");
        o.horizontal_distance = Some(100.0);
        s.observations.push(o);
        net.setups.push(s);
    }
    db
}

#[test]
fn error_ellipse_closed_form() {
    let (s1, s2) = (0.010, 0.004);
    let db = two_distance_network(s1, s2);
    let mut o = AdjustOptions::new(10, 1e-14, 0.95, 0.05, 3.0);
    o.include_centring = false;
    let a = adjust_network_2d(&db, "N", &o).unwrap();
    let p = a.point("P").unwrap();
    assert!((p.e - (E0 + 100.0)).abs() < 1e-9 && (p.n - (N0 + 100.0)).abs() < 1e-9);
    assert!((p.sigma_e - s1).abs() < 1e-12 && (p.sigma_n - s2).abs() < 1e-12);
    // k from chi-square with 2 degrees of freedom at 95 percent: -2 ln 0.05, closed form
    let k = (-2.0 * (0.05f64).ln()).sqrt();
    assert!((a.ellipse_scale - k).abs() < 1e-12);
    assert!((p.ellipse.semi_major - k * s1).abs() < 1e-10);
    assert!((p.ellipse.semi_minor - k * s2).abs() < 1e-10);
    assert!((p.ellipse.azimuth_major_deg - 90.0).abs() < 1e-9, "major axis is east-west: {}", p.ellipse.azimuth_major_deg);
    // probability content: the quadratic form on the ellipse boundary is chi-square(2) at 95 percent
    assert!((chi2_cdf(k * k, 2.0) - 0.95).abs() < 1e-12);
    // swapped precisions turn the major axis to north
    let a2 = adjust_network_2d(&two_distance_network(0.004, 0.010), "N", &o).unwrap();
    assert!(a2.point("P").unwrap().ellipse.azimuth_major_deg.abs() < 1e-9);
    // relative precision A to P: the distance from A is observed directly, so sigma_s = s1 exactly;
    // the azimuth is controlled by the north component: sigma_az = s2 / 100
    let r = a.relative_precision("A", "P").unwrap();
    assert!((r.distance - 100.0).abs() < 1e-9);
    assert!((r.sigma_distance - s1).abs() < 1e-12);
    assert!((r.sigma_azimuth_arcsec - s2 / 100.0 * 206_264.806_247_096_36).abs() < 1e-6);
    assert!((r.ratio.unwrap() - 100.0 / s1).abs() < 1e-6);
    assert_eq!(a.dof, 0);
    assert!(a.global_test.is_none());
}

#[test]
fn centring_terms_enter_the_standard_errors() {
    let mut db = two_distance_network(0.003, 0.003);
    db.equipment[0].centring_instrument_m = 0.002;
    db.equipment[0].centring_target_m = 0.001;
    db.equipment[0].sigma_distance_ppm = 10.0;
    let o = AdjustOptions::new(10, 1e-14, 0.95, 0.05, 3.0);
    let a = adjust_network_2d(&db, "N", &o).unwrap();
    let ob = a.observations.iter().find(|o| o.from == "A").unwrap();
    let want = (0.003f64.powi(2) + (10e-6 * 100.0f64).powi(2) + 0.002f64.powi(2) + 0.001f64.powi(2)).sqrt();
    assert!((ob.sigma - want).abs() < 1e-12, "{} vs {want}", ob.sigma);
    let mut o2 = o.clone();
    o2.include_centring = false;
    let a2 = adjust_network_2d(&db, "N", &o2).unwrap();
    let want2 = (0.003f64.powi(2) + (10e-6 * 100.0f64).powi(2)).sqrt();
    assert!((a2.observations.iter().find(|o| o.from == "A").unwrap().sigma - want2).abs() < 1e-12);
}

#[test]
fn missing_equipment_and_datum_defect_are_errors() {
    let mut db = simulate(None, None);
    db.network_mut("N").unwrap().setups[0].equipment = None;
    let mut o = opts();
    o.default_equipment = None;
    assert!(matches!(adjust_network_2d(&db, "N", &o), Err(SurveyError::Invalid(_))));
    // no fixed point at all: the network can translate and rotate
    let mut db2 = simulate(None, None);
    for p in db2.network_mut("N").unwrap().points.iter_mut() {
        p.fix_e = false;
        p.fix_n = false;
    }
    assert!(matches!(adjust_network_2d(&db2, "N", &opts()), Err(SurveyError::Singular(_))));
}

/// Teunissen (2024), section 4.2.4: levelling network 1-2-3-4 with observables h12, h23, h31, h34,
/// h42, equal variance s^2, point 1 held (the minimal constraint h1 = 0). Equation 4.41 gives the
/// overall model test statistic T and equation 4.42 the w-test statistics. In the printed 4.42 the
/// fifth observable appears as h41, which is the misprint of h42 (the printed conditions are
/// h12 + h23 + h31 = 0 and h23 + h34 + h42 = 0, and the combinations 3c1 - c2, c1 + c2, -c1 + 3c2 below
/// reproduce the printed coefficient patterns).
#[test]
fn published_levelling_example_teunissen_4_2_4() {
    let s = 0.002;
    let (h12, h23, h31, h34, h42) = (1.2345, 0.5231, -1.7590, 0.8812, -1.4020);
    let mut db = SurveyDatabase::new("lev");
    db.add_network("N", "t").unwrap();
    db.equipment.push(Equipment { name: "L".into(), level_sigma_const_m: s, ..Equipment::exact("L") });
    let net = db.network_mut("N").unwrap();
    let mut p1 = SurveyPoint::control("1", 0.0, 0.0, Some(0.0));
    p1.fix_z = true;
    net.points.push(p1);
    let leg = |a: &str, b: &str, dh: f64| LevelLeg { from: a.into(), to: b.into(), dh, length_m: 0.0 };
    net.level_runs.push(LevelRun { id: 1, name: "R".into(), legs: vec![leg("1", "2", h12), leg("2", "3", h23), leg("3", "1", h31), leg("3", "4", h34), leg("4", "2", h42)], equipment: Some("L".into()), event: None });
    let o = AdjustOptions::new(5, 1e-20, 0.95, 0.05, 3.0);
    let a = adjust_levelling(&db, "N", &o).unwrap();
    assert_eq!(a.dof, 2);
    let c1 = h12 + h23 + h31;
    let c2 = h23 + h34 + h42;
    let t = (3.0 * c1 * c1 - 2.0 * c1 * c2 + 3.0 * c2 * c2) / (8.0 * s * s); // eq. 4.41 expanded
    assert!((a.vtpv - t).abs() < 1e-6 * t.max(1.0), "T = {} vs {}", a.vtpv, t);
    let w_pub = [
        (3.0 * c1 - c2) / (2.0 * s * 6f64.sqrt()), // w1
        (c1 + c2) / (s * 8f64.sqrt()),             // w2
        (3.0 * c1 - c2) / (2.0 * s * 6f64.sqrt()), // w3 = w1
        (-c1 + 3.0 * c2) / (2.0 * s * 6f64.sqrt()), // w4
        (-c1 + 3.0 * c2) / (2.0 * s * 6f64.sqrt()), // w5 = w4
    ];
    for (i, o) in a.observations.iter().enumerate() {
        // sign convention: the book's w is e_hat / sigma with e_hat = y - A x_hat; ours is v = -e_hat
        assert!((o.std_residual.unwrap().abs() - w_pub[i].abs()).abs() < 1e-6 * w_pub[i].abs().max(1.0), "w{} = {:?} vs {}", i + 1, o.std_residual, w_pub[i]);
    }
    // blunder in h34 and h42 cannot be told apart (w4 = w5) and w1 = w3 as printed
    assert!((a.observations[3].std_residual.unwrap() - a.observations[4].std_residual.unwrap()).abs() < 1e-9);
    assert!((a.observations[0].std_residual.unwrap() - a.observations[2].std_residual.unwrap()).abs() < 1e-9);
    // global test uses chi-square with 2 degrees of freedom: critical value is -2 ln(alpha) in closed form
    let g = a.global_test.unwrap();
    assert!((g.one_sided_critical - (-2.0 * 0.05f64.ln())).abs() < 1e-8);
}

#[test]
fn levelling_blunder_and_fixed_heights() {
    let mut db = SurveyDatabase::new("lev");
    db.add_network("N", "t").unwrap();
    db.equipment.push(Equipment { name: "L".into(), level_sigma_const_m: 0.001, level_sigma_per_sqrt_km_m: 0.002, ..Equipment::exact("L") });
    let net = db.network_mut("N").unwrap();
    net.points.push(SurveyPoint::control("BM1", 0.0, 0.0, Some(1250.123)));
    net.points.push(SurveyPoint::control("BM2", 0.0, 0.0, Some(1262.873)));
    let h = [("BM1", "T1", 3.000), ("T1", "T2", 4.000), ("T2", "BM2", 5.750), ("BM1", "T2", 7.000), ("T1", "BM2", 9.750)];
    let mut legs: Vec<LevelLeg> = h.iter().map(|(a, b, d)| LevelLeg { from: a.to_string(), to: b.to_string(), dh: *d, length_m: 400.0 }).collect();
    legs[1].dh += 0.050; // blunder on T1 -> T2
    net.level_runs.push(LevelRun { id: 1, name: "R".into(), legs, equipment: Some("L".into()), event: None });
    let o = AdjustOptions::new(5, 1e-20, 0.95, 0.05, 2.5);
    let a = adjust_levelling(&db, "N", &o).unwrap();
    let worst = a.observations.iter().max_by(|x, y| x.std_residual.unwrap().abs().partial_cmp(&y.std_residual.unwrap().abs()).unwrap()).unwrap();
    assert_eq!((worst.from.as_str(), worst.to.as_str()), ("T1", "T2"));
    assert!(worst.flagged);
    assert!(a.heights.iter().find(|h| h.name == "BM1").unwrap().fixed);
    assert!(level_report(&a).contains("LEAST SQUARES ADJUSTMENT (heights)"));
}

#[test]
fn database_pipeline_from_field_book_text() {
    // simulate, write a field book, import it into a new database, adjust: same result as direct
    let sim = simulate(None, None);
    let text = write_fieldbook(sim.network("N").unwrap());
    let mut db = SurveyDatabase::new("pipeline");
    db.equipment.push(equipment());
    db.add_network("N", "t").unwrap();
    db.import_text("N", ImportFormat::Fieldbook, &text, "sim.afrofb", "2026-11-16T12:00:00Z", "simulated", true, DuplicatePolicy::Error).unwrap();
    let a = adjust_network_2d(&db, "N", &opts()).unwrap();
    for (name, e, n) in truth() {
        let p = a.point(name).unwrap();
        assert!((p.e - e).abs() < 1e-6 && (p.n - n).abs() < 1e-6);
    }
    // export to COGO keeps coordinates exactly
    let adj_pts: Vec<_> = a.points.iter().map(|p| (p.name.clone(), p.e, p.n)).collect();
    for (name, e, n) in adj_pts {
        let net = db.network_mut("N").unwrap();
        let p = net.point_mut(&name).unwrap();
        p.e = Some(e);
        p.n = Some(n);
    }
    let (cp, _) = to_cogo_points(db.network("N").unwrap(), 1);
    assert_eq!(cp.len(), 5);
    // the whole database survives a JSON round trip
    let j = db.to_json().unwrap();
    assert_eq!(SurveyDatabase::from_json(&j).unwrap(), db);
}

#[test]
fn radiation_then_adjustment_start_from_computed_approximations() {
    // non control points without coordinates are computed by radiation first, then adjusted
    let mut db = simulate(None, None);
    {
        let net = db.network_mut("N").unwrap();
        for n in ["P1", "P2", "P3"] {
            let p = net.point_mut(n).unwrap();
            p.e = None;
            p.n = None;
        }
        // keep only the setups at the control points A and B so that radiation can reach P1..P3
        net.setups.retain(|s| s.station == "A" || s.station == "B");
    }
    let r = compute_points(&mut db, "N").unwrap();
    assert_eq!(r.points_computed, ["P1", "P2", "P3"]);
    let p1 = db.network("N").unwrap().point("P1").unwrap();
    assert!((p1.e.unwrap() - (E0 + 150.0)).abs() < 1e-6 && (p1.n.unwrap() - (N0 + 220.0)).abs() < 1e-6);
    assert!(r.max_difference_from_mean.iter().all(|(_, d)| *d < 1e-6));
    let a = adjust_network_2d(&db, "N", &opts()).unwrap();
    for (name, e, n) in truth() {
        let p = a.point(name).unwrap();
        assert!((p.e - e).abs() < 1e-6 && (p.n - n).abs() < 1e-6);
    }
}
