//! GNSS baselines, known azimuths and weighted control coordinates in the adjustment.

use afrobim_survey::*;

const E0: f64 = 500_000.0;
const N0: f64 = 9_960_000.0;

fn network() -> SurveyDatabase {
    let mut db = SurveyDatabase::new("g");
    db.add_network("N", "t").unwrap();
    let net = db.network_mut("N").unwrap();
    net.points.push(SurveyPoint::control("A", E0, N0, Some(100.0)));
    // B is a control point with GNSS coordinates and stated sigmas (free, weighted)
    let mut b = SurveyPoint::control("B", E0 + 200.0, N0 + 0.05, Some(110.0));
    b.fix_e = false;
    b.fix_n = false;
    b.fix_z = false;
    b.sigma_e = Some(0.02);
    b.sigma_n = Some(0.02);
    b.sigma_z = Some(0.03);
    net.points.push(b);
    net.points.push(SurveyPoint::given("C", E0 + 100.0, N0 + 100.0, Some(105.0)));
    net.baselines.push(Baseline { from: "A".into(), to: "C".into(), de: 100.0, dn: 100.0, du: 5.0, sigma_e: 0.01, sigma_n: 0.01, sigma_u: 0.02, event: None });
    net.baselines.push(Baseline { from: "B".into(), to: "C".into(), de: -100.0, dn: 100.0, du: -5.0, sigma_e: 0.01, sigma_n: 0.01, sigma_u: 0.02, event: None });
    // a known azimuth A to B with a stated sigma (east, 90 degrees)
    net.directions.push(KnownDirection { from: "A".into(), to: "B".into(), azimuth_deg: 90.0, sigma_arcsec: Some(5.0), event: None });
    db
}

#[test]
fn baselines_azimuth_and_weighted_control_plan() {
    let db = network();
    let o = AdjustOptions::new(10, 1e-10, 0.95, 0.05, 3.0);
    let a = adjust_network_2d(&db, "N", &o).unwrap();
    assert!(a.converged);
    let pb = a.point("B").unwrap();
    assert!((pb.n - N0).abs() < 0.03, "B north {}", pb.n - N0);
    let pc = a.point("C").unwrap();
    assert!((pc.e - (E0 + 100.0)).abs() < 0.02 && (pc.n - (N0 + 100.0)).abs() < 0.03);
    assert!(a.dof > 0);
    assert!(a.observations.iter().any(|o| o.kind == ObsKind::Azimuth));
    assert!(a.observations.iter().any(|o| o.kind == ObsKind::CoordinateE));
}

#[test]
fn baselines_and_weighted_control_heights() {
    let db = network();
    let o = AdjustOptions::new(10, 1e-10, 0.95, 0.05, 3.0);
    let h = adjust_levelling(&db, "N", &o).unwrap();
    let hc = h.heights.iter().find(|x| x.name == "C").unwrap();
    assert!((hc.z - 105.0).abs() < 0.03);
    assert!(h.heights.iter().find(|x| x.name == "A").unwrap().fixed);
    assert_eq!(h.dof, 1);
}

#[test]
fn left_handed_circle_gives_the_same_adjustment() {
    // mirror the readings: a circle that increases anticlockwise reads 360 - r
    let mut db = network();
    let net = db.network_mut("N").unwrap();
    net.baselines.clear();
    net.directions.clear();
    let mut eq = Equipment::exact("T");
    eq.sigma_distance_const_m = 0.003;
    eq.sigma_pointing_arcsec = 1.0;
    eq.sigma_reading_arcsec = 1.0;
    db.equipment.push(eq);
    let net = db.network_mut("N").unwrap();
    net.point_mut("B").unwrap().n = Some(N0);
    net.point_mut("B").unwrap().sigma_e = None;
    net.point_mut("B").unwrap().sigma_n = None;
    net.point_mut("B").unwrap().fix_e = true;
    net.point_mut("B").unwrap().fix_n = true;
    let mut s = Setup::new("A", 0.0);
    s.equipment = Some("T".into());
    s.backsight_point = Some("B".into());
    let (b_az, c_az) = (90.0f64, 45.0f64);
    let omega = 10.0;
    for sign in [1.0, -1.0] {
        let mut s2 = s.clone();
        let rb = (sign * (b_az - omega) + 720.0) % 360.0;
        let rc = (sign * (c_az - omega) + 720.0) % 360.0;
        s2.backsight_reading_deg = rb;
        let mut ob = Observation::new("B");
        ob.hz_deg = Some(rb);
        ob.horizontal_distance = Some(200.0);
        let mut oc = Observation::new("C");
        oc.hz_deg = Some(rc);
        oc.horizontal_distance = Some(100.0 * 2f64.sqrt());
        s2.observations = vec![ob, oc];
        let mut d = db.clone();
        d.settings.hz_direction = if sign > 0.0 { HzDirection::Right } else { HzDirection::Left };
        let net = d.network_mut("N").unwrap();
        net.setups = vec![s2];
        net.point_mut("C").unwrap().e = Some(E0 + 99.0);
        net.point_mut("C").unwrap().n = Some(N0 + 101.0);
        let o = AdjustOptions::new(10, 1e-10, 0.95, 0.05, 3.0);
        let a = adjust_network_2d(&d, "N", &o).unwrap();
        let c = a.point("C").unwrap();
        assert!((c.e - (E0 + 100.0)).abs() < 1e-6 && (c.n - (N0 + 100.0)).abs() < 1e-6, "sign {sign}: {} {}", c.e - E0, c.n - N0);
    }
}
