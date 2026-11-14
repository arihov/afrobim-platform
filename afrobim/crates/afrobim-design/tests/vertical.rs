//! Hand-worked vertical tests, with the working in the comments.

use afrobim_design::*;

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: got {a}, expected {b} (tolerance {tol})");
}

#[test]
fn crest_curve_high_point_and_levels() {
    // PVIs (0,100), (400,108), (800,100): grades +2 %, -2 %. Crest at PVI 1, L = 200.
    //   A = -0.04, K = L / (100 |A|) = 200 / 4 = 50
    //   PVC at 400 - 100 = 300, z_pvc = 108 - 0.02 x 100 = 106;  PVT at 500, z = 108 - 0.02 x 100 = 106
    //   high point where grade = 0: x = -g1 L / A = 0.02 x 200 / 0.04 = 100 from the PVC, i.e. station 400
    //   z = z_pvc + g1 x + A x^2 / (2L) = 106 + 2 - 0.04 x 10000 / 400 = 107   (= 108 - |A| L / 8 = 107)
    //   level at 350 (x = 50): 106 + 0.02 x 50 - 0.04 x 2500 / 400 = 106 + 1 - 0.25 = 106.75
    let d = VerticalDesign::new("P", vec![VPvi::point(0.0, 100.0), VPvi::with_length(400.0, 108.0, 200.0), VPvi::point(800.0, 100.0)]);
    let s = d.solve().unwrap();
    let c = &s.curves[0];
    assert_eq!(c.kind, CurveKind::Crest);
    close(c.k, 50.0, 1e-9, "K");
    close(c.pvc_station_m, 300.0, 1e-9, "PVC");
    close(c.pvt_station_m, 500.0, 1e-9, "PVT");
    close(c.pvc_elevation_m, 106.0, 1e-9, "PVC level");
    close(c.pvt_elevation_m, 106.0, 1e-9, "PVT level");
    close(c.offset_at_pvi_m, 1.0, 1e-9, "offset at PVI");
    let (hs, hz) = s.high_points()[0];
    close(hs, 400.0, 1e-9, "high point station");
    close(hz, 107.0, 1e-9, "high point level");
    close(s.level_at(350.0).unwrap(), 106.75, 1e-9, "level at 350");
    close(s.level_at(400.0).unwrap(), 107.0, 1e-9, "level at the PVI");
    close(s.level_at(100.0).unwrap(), 102.0, 1e-9, "level on the first grade");
    close(s.level_at(700.0).unwrap(), 102.0, 1e-9, "level on the second grade");
    close(s.grade_at(350.0).unwrap(), 0.02 - 0.04 * 50.0 / 200.0, 1e-12, "grade at 350");
    assert!(s.low_points().is_empty());
    // Rise 7 m (100 to the high point at 107), fall 7 m, max grade 2 %.
    let (rise, fall) = s.rise_and_fall();
    close(rise, 7.0, 1e-9, "rise: 100 to 107 at the high point");
    close(fall, 7.0, 1e-9, "fall: 107 down to 100");
    close(s.max_abs_grade(), 0.02, 1e-12, "max grade");
}

#[test]
fn sag_curve_low_point_set_by_k() {
    // PVIs (0,100), (400,92), (800,100): -2 %, +2 %. Sag with K = 40: L = K x 100 x |A| = 40 x 4 = 160.
    //   PVC at 320, z_pvc = 92 + 0.02 x 80 = 93.6; low point at x = 0.02 x 160 / 0.04 = 80 -> station 400
    //   z = 93.6 - 0.02 x 80 + 0.04 x 6400 / 320 = 93.6 - 1.6 + 0.8 = 92.8  (= 92 + |A| L / 8 = 92.8)
    //   level at 360 (x = 40): 93.6 - 0.8 + 0.04 x 1600 / 320 = 93.0
    let d = VerticalDesign::new("P", vec![VPvi::point(0.0, 100.0), VPvi::with_k(400.0, 92.0, 40.0), VPvi::point(800.0, 100.0)]);
    let s = d.solve().unwrap();
    let c = &s.curves[0];
    assert_eq!(c.kind, CurveKind::Sag);
    close(c.length_m, 160.0, 1e-9, "L from K");
    close(c.pvc_station_m, 320.0, 1e-9, "PVC");
    let (ls, lz) = s.low_points()[0];
    close(ls, 400.0, 1e-9, "low point station");
    close(lz, 92.8, 1e-9, "low point level");
    close(s.level_at(360.0).unwrap(), 93.0, 1e-9, "level at 360");
}

#[test]
fn turning_point_off_the_pvi() {
    // PVIs (0,100), (400,112), (800,108): +3 %, -1 %, A = -0.04, L = 200, PVC 300, z_pvc = 112 - 3 = 109.
    //   x = 0.03 x 200 / 0.04 = 150 -> station 450; z = 109 + 0.03 x 150 - 0.04 x 22500 / 400 = 109 + 4.5 - 2.25 = 111.25
    let d = VerticalDesign::new("P", vec![VPvi::point(0.0, 100.0), VPvi::with_length(400.0, 112.0, 200.0), VPvi::point(800.0, 108.0)]);
    let s = d.solve().unwrap();
    let (hs, hz) = s.high_points()[0];
    close(hs, 450.0, 1e-9, "high point station");
    close(hz, 111.25, 1e-9, "high point level");
    // And it is the highest level on the profile, found by brute force.
    let max = (0..=800).map(|i| s.level_at(i as f64).unwrap()).fold(f64::MIN, f64::max);
    assert!(max <= 111.25 + 1e-9);
    close(s.level_at(450.0).unwrap(), 111.25, 1e-9, "level there");
}

#[test]
fn impossible_vertical_edits_are_refused() {
    let mut d = VerticalDesign::new("P", vec![VPvi::point(0.0, 100.0), VPvi::with_length(400.0, 108.0, 200.0), VPvi::with_length(700.0, 100.0, 200.0), VPvi::point(1000.0, 104.0)]);
    let before = d.clone();
    // Lengths of 200 at PVI 1 and 2 are 300 m apart: 100 + 100 = 200 < 300, fine. A length of 700 at PVI 2 is not.
    let e = d.set_curve_length(2, 700.0).unwrap_err();
    assert!(e.to_string().contains("overlap"), "{e}");
    assert_eq!(d, before);
    // A curve running past the first PVI.
    let e = d.set_curve_length(1, 900.0).unwrap_err();
    assert!(e.to_string().contains("before the first PVI") || e.to_string().contains("overlap"), "{e}");
    assert_eq!(d, before);
    // A PVI out of order, a curve on an end PVI.
    assert!(d.move_pvi(1, -5.0, 100.0).is_err());
    assert!(d.set_curve_length(0, 50.0).is_err());
    assert_eq!(d, before);
    // Good edits return the recomputed profile.
    let s = d.move_pvi(1, 400.0, 110.0).unwrap();
    close(s.curves[0].pvi_elevation_m, 110.0, 1e-12, "moved");
    let s = d.insert_pvi_at_station(550.0).unwrap();
    assert_eq!(s.profile.pvis.len(), 5);
    close(s.level_at(550.0).unwrap(), d.pvis[2].elevation_m, 1e-12, "inserted on the profile");
    d.delete_pvi(2).unwrap();
    assert_eq!(d.pvis.len(), 4);
    // K cannot be applied where the grades are equal: a warning, no curve.
    let flat = VerticalDesign::new("P", vec![VPvi::point(0.0, 100.0), VPvi::with_k(100.0, 101.0, 30.0), VPvi::point(200.0, 102.0)]);
    let s = flat.solve().unwrap();
    assert!(s.curves.is_empty() && !s.warnings.is_empty());
}

#[test]
fn levels_match_the_alignment_crate_profile() {
    // The produced Profile is evaluated by afrobim-alignment itself.
    let d = VerticalDesign::new("P", vec![VPvi::point(0.0, 100.0), VPvi::with_length(400.0, 108.0, 200.0), VPvi::point(800.0, 100.0)]);
    let s = d.solve().unwrap();
    for st in [0.0, 120.0, 299.0, 300.0, 333.3, 400.0, 480.0, 500.0, 650.0, 800.0] {
        close(s.profile.elevation_at(st).unwrap(), s.level_at(st).unwrap(), 0.0, "profile");
    }
    assert!(s.level_at(900.0).is_err());
}
