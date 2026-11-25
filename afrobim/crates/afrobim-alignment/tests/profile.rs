// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Vertical profile.
//!
//! Every expected value is worked by hand from the closed forms written beside it. The grades,
//! lengths and radii are arbitrary numbers chosen to make the arithmetic exact; they are not
//! design values.

use afrobim_alignment::{Alignment, AlignmentBuilder, Profile, ProfileError, Pvi, VerticalCurve};

fn assert_close(got: f64, want: f64, tol: f64, what: &str) {
    assert!((got - want).abs() <= tol, "{what}: got {got}, want {want} (off by {:e})", got - want);
}

/// Elevation and grade at a station.
fn at(p: &Profile, station: f64) -> (f64, f64) {
    let q = p.point_at(station).unwrap();
    (q.elevation_m, q.grade)
}

fn check(p: &Profile, station: f64, elevation: f64, grade: f64) {
    let (z, g) = at(p, station);
    assert_close(z, elevation, 1e-9, &format!("elevation at {station}"));
    assert_close(g, grade, 1e-12, &format!("grade at {station}"));
}

#[test]
fn grades_and_a_symmetric_parabola() {
    // PVIs (300, 96), (500, 100), (800, 97): g1 = 4/200 = +0.02, g2 = -3/300 = -0.01, A = -0.03.
    // Parabola L = 120 at the middle PVI: PVC at 440, z_pvc = 100 - 0.02*60 = 98.8; PVT at 560.
    //   y = z_pvc + g1 x + A x^2 / (2L),  grade = g1 + A x / L,  x from the PVC.
    let p = Profile::new("FG", vec![Pvi::point(300.0, 96.0), Pvi::parabola(500.0, 100.0, 120.0), Pvi::point(800.0, 97.0)]).unwrap();
    assert_eq!(p.range(), Some((300.0, 800.0)));
    assert_close(p.grade_between(0).unwrap(), 0.02, 1e-15, "g1");
    assert_close(p.grade_between(1).unwrap(), -0.01, 1e-15, "g2");
    let extents = p.curve_extents().unwrap();
    assert_eq!(extents.len(), 1);
    assert_eq!((extents[0].pvi, extents[0].start_station_m, extents[0].end_station_m), (1, 440.0, 560.0));

    check(&p, 300.0, 96.0, 0.02); // first PVI
    check(&p, 400.0, 98.0, 0.02); // 96 + 0.02*100
    check(&p, 440.0, 98.8, 0.02); // PVC
    check(&p, 500.0, 99.55, 0.005); // x = 60: 98.8 + 1.2 - 0.03*3600/240 = 99.55; 0.02 - 0.03*60/120
    check(&p, 520.0, 99.6, 0.0); // high point, x = g1 L / -A = 80: 98.8 + 1.6 - 0.03*6400/240 = 99.6
    check(&p, 560.0, 99.4, -0.01); // PVT, x = 120: 98.8 + 2.4 - 0.03*14400/240 = 99.4 = 100 - 0.01*60
    check(&p, 700.0, 98.0, -0.01); // 100 - 0.01*200
    check(&p, 800.0, 97.0, -0.01); // last PVI
    assert_close(p.elevation_at(520.0).unwrap(), 99.6, 1e-9, "elevation_at");
    assert_close(p.grade_at(400.0).unwrap(), 0.02, 1e-12, "grade_at");

    // Outside the range there is no elevation.
    for station in [299.0, 800.5, f64::NAN, f64::INFINITY] {
        assert!(matches!(p.elevation_at(station), Err(ProfileError::OutOfRange { .. })), "{station}");
        assert!(matches!(p.grade_at(station), Err(ProfileError::OutOfRange { .. })), "{station}");
    }
    assert_eq!(p.elevation_at(299.0).unwrap_err().to_string(), "station 0+299.000 is outside the profile, which runs from 0+300.000 to 0+800.000");
}

#[test]
fn an_asymmetric_parabola() {
    // Same PVIs; L1 = 80 in, L2 = 40 out, L = 120, A = -0.03.
    //   r1 = A L2 / (L L1) = -0.03*40/9600 = -0.000125;  r2 = A L1 / (L L2) = -0.03*80/4800 = -0.0005.
    //   PVC at 420: 100 - 0.02*80 = 98.4.   First:  y = 98.4 + 0.02 x + r1 x^2/2.
    //   At the PVI station (x = 80): 98.4 + 1.6 - 0.000125*6400/2 = 99.6, which is also
    //   z + A L1 L2 / (2L) = 100 - 0.03*3200/240; grade 0.02 - 0.000125*80 = 0.01.
    //   Second, x' from the PVI station: y = 99.6 + 0.01 x' + r2 x'^2/2.
    let p = Profile::new("FG", vec![Pvi::point(300.0, 96.0), Pvi::asymmetric(500.0, 100.0, 80.0, 40.0), Pvi::point(800.0, 97.0)]).unwrap();
    let e = p.curve_extents().unwrap();
    assert_eq!((e[0].start_station_m, e[0].end_station_m), (420.0, 540.0));
    check(&p, 420.0, 98.4, 0.02);
    check(&p, 460.0, 99.1, 0.015); // x = 40: 98.4 + 0.8 - 0.000125*1600/2 = 99.1; 0.02 - 0.005
    check(&p, 500.0, 99.6, 0.01);
    check(&p, 520.0, 99.7, 0.0); // x' = 20: 99.6 + 0.2 - 0.0005*400/2 = 99.7; 0.01 - 0.01
    check(&p, 540.0, 99.6, -0.01); // PVT, x' = 40: 99.6 + 0.4 - 0.0005*1600/2 = 99.6 = 100 - 0.01*40
    check(&p, 600.0, 99.0, -0.01);

    // With equal lengths it is the symmetric parabola.
    let equal = Profile::new("a", vec![Pvi::point(300.0, 96.0), Pvi::asymmetric(500.0, 100.0, 60.0, 60.0), Pvi::point(800.0, 97.0)]).unwrap();
    let symmetric = Profile::new("s", vec![Pvi::point(300.0, 96.0), Pvi::parabola(500.0, 100.0, 120.0), Pvi::point(800.0, 97.0)]).unwrap();
    for station in [440.0, 455.5, 500.0, 512.25, 560.0] {
        let (a, s) = (at(&equal, station), at(&symmetric, station));
        assert_close(a.0, s.0, 1e-12, "elevation");
        assert_close(a.1, s.1, 1e-15, "grade");
    }
}

#[test]
fn a_circular_curve() {
    // Numbers chosen for exact right triangles (3-4-5 and 28-96-100), not as a road.
    // Crest: PVIs (0, 0), (100, 75), (200, 0): g1 = +0.75, g2 = -0.75; R = 100.
    //   a = atan 0.75, so cos a = 0.8, sin a = 0.6. T = R tan a = 75 along each grade;
    //   on plan T cos a = 60, so PVC (40, 30) and PVT (160, 30).
    //   Centre below the PVI by R / cos a = 125: (100, -50). z = -50 + sqrt(100^2 - (x - 100)^2),
    //   grade = -(x - 100) / sqrt(100^2 - (x - 100)^2).
    let crest = Profile::new("c", vec![Pvi::point(0.0, 0.0), Pvi::circular(100.0, 75.0, 100.0), Pvi::point(200.0, 0.0)]).unwrap();
    let e = crest.curve_extents().unwrap();
    assert_close(e[0].start_station_m, 40.0, 1e-12, "PVC");
    assert_close(e[0].end_station_m, 160.0, 1e-12, "PVT");
    let tol = |p: &Profile, s: f64, z: f64, g: f64| {
        let (gz, gg) = at(p, s);
        assert_close(gz, z, 1e-11, &format!("elevation at {s}"));
        assert_close(gg, g, 1e-12, &format!("grade at {s}"));
    };
    tol(&crest, 20.0, 15.0, 0.75); // on the grade
    tol(&crest, 40.0, 30.0, 0.75); // PVC
    tol(&crest, 72.0, 46.0, 28.0 / 96.0); // -50 + sqrt(10000 - 784) = -50 + 96
    tol(&crest, 100.0, 50.0, 0.0); // top
    tol(&crest, 128.0, 46.0, -28.0 / 96.0);
    tol(&crest, 160.0, 30.0, -0.75); // PVT
    tol(&crest, 180.0, 15.0, -0.75);

    // Sag: the same turned over. Centre (100, 125); z = 125 - sqrt(100^2 - (x - 100)^2).
    let sag = Profile::new("s", vec![Pvi::point(0.0, 75.0), Pvi::circular(100.0, 0.0, 100.0), Pvi::point(200.0, 75.0)]).unwrap();
    tol(&sag, 40.0, 45.0, -0.75);
    tol(&sag, 72.0, 29.0, -28.0 / 96.0);
    tol(&sag, 100.0, 25.0, 0.0);
    tol(&sag, 128.0, 29.0, 28.0 / 96.0);
    tol(&sag, 160.0, 45.0, 0.75);

    // Flat grades, where a circle and a parabola nearly agree: g = ±0.02, R = 2000, PVI (1700, 105).
    //   T = R tan(atan 0.02) = 40; on plan 40 / sqrt(1.0004). Centre below the PVI by
    //   R / cos a = 2000 sqrt(1.0004): z = 105 - 2000 sqrt(1.0004) + sqrt(2000^2 - (x - 1700)^2).
    let flat = Profile::new("f", vec![Pvi::point(1500.0, 101.0), Pvi::circular(1700.0, 105.0, 2000.0), Pvi::point(1900.0, 101.0)]).unwrap();
    let root = 1.0004_f64.sqrt();
    let e = flat.curve_extents().unwrap();
    assert_close(e[0].start_station_m, 1700.0 - 40.0 / root, 1e-9, "PVC");
    assert_close(e[0].end_station_m, 1700.0 + 40.0 / root, 1e-9, "PVT");
    for dx in [-30.0_f64, 0.0, 12.5, 39.0] {
        let want = 105.0 - 2000.0 * root + (2000.0_f64.powi(2) - dx * dx).sqrt();
        assert_close(flat.elevation_at(1700.0 + dx).unwrap(), want, 1e-9, "circle");
        assert_close(flat.grade_at(1700.0 + dx).unwrap(), -dx / (2000.0_f64.powi(2) - dx * dx).sqrt(), 1e-12, "circle grade");
    }
    // The parabola of the same plan length differs from it by well under a millimetre at the PVI:
    // parabola 105 + A L / 8 with A = -0.04, L = 80 / sqrt(1.0004).
    let parabola = 105.0 - 0.04 * (80.0 / root) / 8.0;
    assert!((flat.elevation_at(1700.0).unwrap() - parabola).abs() < 1e-3);
}

#[test]
fn a_pvi_without_a_curve_and_curves_that_touch() {
    // (0, 10), (100, 12), (200, 11): +0.02 then -0.01, meeting at an angle.
    let p = Profile::new("k", vec![Pvi::point(0.0, 10.0), Pvi::point(100.0, 12.0), Pvi::point(200.0, 11.0)]).unwrap();
    check(&p, 100.0, 12.0, -0.01); // the grade ahead
    check(&p, 99.0, 11.98, 0.02);
    check(&p, 200.0, 11.0, -0.01); // at the end, the grade behind
    assert!(p.curve_extents().unwrap().is_empty());

    // (0,0) (100,2) (200,0) (300,2), parabolas of 100 at both interior PVIs: PVT of the first and
    // PVC of the second are both station 150, elevation 1.
    let touching = Profile::new("t", vec![Pvi::point(0.0, 0.0), Pvi::parabola(100.0, 2.0, 100.0), Pvi::parabola(200.0, 0.0, 100.0), Pvi::point(300.0, 2.0)]).unwrap();
    check(&touching, 150.0, 1.0, -0.02);
    // First curve: PVC (50, 1), A = -0.04: at x = 50 (the PVI) 1 + 1 - 0.04*2500/200 = 1.5.
    check(&touching, 100.0, 1.5, 0.0);
    // Second: PVC (150, 1), g1 = -0.02, A = +0.04: at x = 50, 1 - 1 + 0.04*2500/200 = 0.5.
    check(&touching, 200.0, 0.5, 0.0);
}

#[test]
fn validation() {
    let err = |pvis: Vec<Pvi>| Profile::new("x", pvis).unwrap_err();
    assert_eq!(err(vec![]), ProfileError::TooFewPvis);
    assert_eq!(err(vec![Pvi::point(0.0, 1.0)]), ProfileError::TooFewPvis);
    assert_eq!(err(vec![Pvi::point(0.0, 1.0), Pvi::point(0.0, 2.0)]), ProfileError::StationsNotIncreasing { index: 1 });
    assert_eq!(err(vec![Pvi::point(0.0, 1.0), Pvi::point(50.0, 2.0), Pvi::point(40.0, 2.0)]), ProfileError::StationsNotIncreasing { index: 2 });
    assert!(matches!(err(vec![Pvi::point(0.0, f64::NAN), Pvi::point(10.0, 2.0)]), ProfileError::BadPvi { index: 0, .. }));
    assert_eq!(err(vec![Pvi::parabola(0.0, 1.0, 50.0), Pvi::point(100.0, 2.0)]), ProfileError::CurveAtEnd { index: 0, which: "first" });
    assert_eq!(err(vec![Pvi::point(0.0, 1.0), Pvi::circular(100.0, 2.0, 500.0)]), ProfileError::CurveAtEnd { index: 1, which: "last" });
    for bad in [0.0, -10.0, f64::NAN, f64::INFINITY] {
        for curve in [
            VerticalCurve::Parabola { length_m: bad },
            VerticalCurve::AsymmetricParabola { length_in_m: 10.0, length_out_m: bad },
            VerticalCurve::AsymmetricParabola { length_in_m: bad, length_out_m: 10.0 },
            VerticalCurve::Circular { radius_m: bad },
        ] {
            let e = err(vec![Pvi::point(0.0, 0.0), Pvi { station_m: 100.0, elevation_m: 2.0, curve }, Pvi::point(200.0, 0.0)]);
            assert!(matches!(e, ProfileError::BadPvi { index: 1, .. }), "{curve:?}: {e:?}");
        }
    }
    // Curves of 120 at stations 100 and 200: the first ends at 160, the second starts at 140.
    let overlap = err(vec![Pvi::point(0.0, 0.0), Pvi::parabola(100.0, 2.0, 120.0), Pvi::parabola(200.0, 0.0, 120.0), Pvi::point(300.0, 2.0)]);
    assert_eq!(overlap, ProfileError::CurvesOverlap { index: 1, next: 2, overlap_m: 20.0 });
    assert_eq!(overlap.to_string(), "the vertical curves at PVI 1 and PVI 2 overlap by 20.000 m");
    // A curve of 120 at station 50 would start 10 m before the first PVI.
    assert_eq!(
        err(vec![Pvi::point(0.0, 0.0), Pvi::parabola(50.0, 1.0, 120.0), Pvi::point(300.0, 0.0)]),
        ProfileError::CurvesOverlap { index: 0, next: 1, overlap_m: 10.0 }
    );
    // ... and one of 80 out at station 250 would end 30 m after the last.
    assert_eq!(
        err(vec![Pvi::point(0.0, 0.0), Pvi::asymmetric(250.0, 1.0, 20.0, 80.0), Pvi::point(300.0, 0.0)]),
        ProfileError::CurvesOverlap { index: 1, next: 2, overlap_m: 30.0 }
    );

    // A profile that did not come through `new` is checked when it is used: an error, not a panic.
    let unchecked = Profile { name: "u".into(), pvis: vec![Pvi::point(5.0, 1.0)] };
    assert_eq!(unchecked.elevation_at(5.0), Err(ProfileError::TooFewPvis));
    assert_eq!(unchecked.range(), None);
    assert_eq!(Profile::default().validate(), Err(ProfileError::TooFewPvis));
}

#[test]
fn profile_persists_with_the_alignment_and_old_files_still_load() {
    let mut a: Alignment = AlignmentBuilder::new("MC01", 300.0, 0.0, 0.0, 0.0).line(500.0).build().unwrap();
    // Without the new parts the JSON has none of their keys: what was written before is what is written now.
    let plain = serde_json::to_value(&a).unwrap();
    let keys: Vec<&str> = plain.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["elements", "name", "start_station_m"]);

    a.profile = Some(
        Profile::new(
            "FG",
            vec![Pvi::point(300.0, 96.0), Pvi::parabola(500.0, 100.0, 120.0), Pvi::asymmetric(600.0, 99.0, 10.0, 20.0), Pvi::circular(700.0, 99.5, 900.0), Pvi::point(800.0, 97.0)],
        )
        .unwrap(),
    );
    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains(r#""curve":{"type":"parabola","length_m":120.0}"#), "{json}");
    assert!(json.contains(r#""type":"asymmetric_parabola""#) && json.contains(r#""type":"circular""#), "{json}");
    let back: Alignment = serde_json::from_str(&json).unwrap();
    assert_eq!(back.profile, a.profile);

    // A file written before profiles, superelevation and station equations existed.
    let old: Alignment = serde_json::from_str(r#"{"name":"old","start_station_m":10.0,"elements":[{"x":0,"y":0,"heading_rad":0,"length":100}]}"#).unwrap();
    assert!(old.profile.is_none() && old.superelevation.is_none() && old.station_equations.is_empty());
    assert_eq!(old.end_station_m(), 110.0);
    // A PVI written without a curve reads as one without.
    let p: Profile = serde_json::from_str(r#"{"pvis":[{"station_m":0,"elevation_m":1},{"station_m":10,"elevation_m":2}]}"#).unwrap();
    assert_eq!(p.pvis[0].curve, VerticalCurve::None);
    assert_close(p.elevation_at(5.0).unwrap(), 1.5, 1e-12, "midway");
}
