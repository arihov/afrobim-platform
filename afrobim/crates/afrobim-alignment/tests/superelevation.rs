//! Superelevation: stored cross-fall and interpolation between stated stations.
//!
//! The cross-falls and stations here are arbitrary test numbers, not design values. Expected
//! values are straight-line interpolation, a + (b − a)(s − s_a)/(s_b − s_a), worked in the comments.

use afrobim_alignment::{CriticalStation, CrossFall, CrossFallPoint, Superelevation, SuperelevationError, SuperelevationSection};

fn fall(left: Option<f64>, right: Option<f64>) -> CrossFall {
    CrossFall { left_pct: left, right_pct: right }
}

fn section() -> Vec<CrossFallPoint> {
    vec![
        CrossFallPoint::new(100.0, Some(-2.0), Some(-2.0)),
        CrossFallPoint::new(130.0, Some(-2.0), Some(0.0)),
        CrossFallPoint::new(190.0, Some(-6.0), Some(6.0)),
        CrossFallPoint::new(250.0, Some(-6.0), Some(6.0)),
        CrossFallPoint::new(300.0, None, Some(0.0)),
    ]
}

#[test]
fn interpolates_each_side_between_stated_stations() {
    let s = Superelevation::from_points(section()).unwrap();
    assert_eq!(s.range(), Some((100.0, 300.0)));
    // At stated stations: what is stated.
    assert_eq!(s.cross_fall_at(100.0), fall(Some(-2.0), Some(-2.0)));
    assert_eq!(s.cross_fall_at(190.0), fall(Some(-6.0), Some(6.0)));
    assert_eq!(s.cross_fall_at(300.0), fall(None, Some(0.0)));
    // 115 is halfway from 100 to 130: left −2 to −2, right −2 to 0 → −1.
    assert_eq!(s.cross_fall_at(115.0), fall(Some(-2.0), Some(-1.0)));
    // 160 is halfway from 130 to 190: left −2 to −6 → −4, right 0 to 6 → 3.
    assert_eq!(s.cross_fall_at(160.0), fall(Some(-4.0), Some(3.0)));
    // 145 is a quarter of the way: left −2 − 4/4 = −3, right 6/4 = 1.5.
    assert_eq!(s.cross_fall_at(145.0), fall(Some(-3.0), Some(1.5)));
    assert_eq!(s.cross_fall_at(220.0), fall(Some(-6.0), Some(6.0)));
    // 275 is halfway from 250 to 300. The right goes 6 to 0 → 3. The left is not stated at 300,
    // so between 250 and 300 it is not known: it is not carried forward from 250.
    assert_eq!(s.cross_fall_at(275.0), fall(None, Some(3.0)));
    // Outside the stated stations nothing is known: no extrapolation, no normal crown.
    assert_eq!(s.cross_fall_at(99.9), fall(None, None));
    assert_eq!(s.cross_fall_at(300.1), fall(None, None));
    assert_eq!(s.cross_fall_at(f64::NAN), fall(None, None));
    assert_eq!(Superelevation::default().cross_fall_at(150.0), fall(None, None));
    assert_eq!(Superelevation::default().range(), None);
}

#[test]
fn nothing_is_interpolated_between_sections() {
    let s = Superelevation::new(vec![
        SuperelevationSection { points: vec![CrossFallPoint::new(100.0, Some(-5.0), Some(5.0)), CrossFallPoint::new(200.0, Some(-5.0), Some(5.0))] },
        SuperelevationSection { points: vec![CrossFallPoint::new(400.0, Some(4.0), Some(-4.0)), CrossFallPoint::new(500.0, Some(4.0), Some(-4.0))] },
        // This one starts where the one before ends.
        SuperelevationSection { points: vec![CrossFallPoint::new(500.0, Some(1.0), Some(-1.0)), CrossFallPoint::new(600.0, Some(3.0), Some(-3.0))] },
        // A single stated station.
        SuperelevationSection { points: vec![CrossFallPoint::new(700.0, Some(2.0), None)] },
    ])
    .unwrap();
    assert_eq!(s.range(), Some((100.0, 700.0)));
    assert_eq!(s.cross_fall_at(150.0), fall(Some(-5.0), Some(5.0)));
    // Between the first two sections the data says nothing, though both ends have values.
    assert_eq!(s.cross_fall_at(300.0), fall(None, None));
    assert_eq!(s.cross_fall_at(450.0), fall(Some(4.0), Some(-4.0)));
    // On the shared station the later section answers.
    assert_eq!(s.cross_fall_at(500.0), fall(Some(1.0), Some(-1.0)));
    assert_eq!(s.cross_fall_at(550.0), fall(Some(2.0), Some(-2.0)));
    assert_eq!(s.cross_fall_at(650.0), fall(None, None));
    assert_eq!(s.cross_fall_at(700.0), fall(Some(2.0), None));
}

#[test]
fn validation() {
    let p = CrossFallPoint::new;
    assert_eq!(Superelevation::from_points(vec![]), Err(SuperelevationError::EmptySection { section: 0 }));
    assert_eq!(
        Superelevation::from_points(vec![p(100.0, None, None), p(100.0, None, None)]),
        Err(SuperelevationError::StationsNotIncreasing { section: 0, index: 1 })
    );
    assert!(matches!(Superelevation::from_points(vec![p(f64::NAN, None, None)]), Err(SuperelevationError::BadPoint { section: 0, index: 0, .. })));
    assert!(matches!(
        Superelevation::from_points(vec![p(1.0, None, None), p(2.0, Some(f64::INFINITY), None)]),
        Err(SuperelevationError::BadPoint { section: 0, index: 1, .. })
    ));
    let overlapping = Superelevation::new(vec![
        SuperelevationSection { points: vec![p(100.0, None, None), p(200.0, None, None)] },
        SuperelevationSection { points: vec![p(150.0, None, None), p(300.0, None, None)] },
    ]);
    assert_eq!(overlapping, Err(SuperelevationError::SectionsOverlap { previous: 0, section: 1 }));
    assert_eq!(overlapping.unwrap_err().to_string(), "superelevation section 1 starts before section 0 ends");
}

#[test]
fn persists_as_json() {
    let mut points = section();
    points[2].kind = Some(CriticalStation::FullSuper);
    let s = Superelevation::from_points(points).unwrap();
    let json = serde_json::to_string(&s).unwrap();
    assert!(json.contains(r#"{"station_m":190.0,"left_pct":-6.0,"right_pct":6.0,"kind":"full_super"}"#), "{json}");
    assert!(json.contains(r#"{"station_m":300.0,"left_pct":null,"right_pct":0.0}"#), "{json}");
    assert_eq!(serde_json::from_str::<Superelevation>(&json).unwrap(), s);
    // A side left out of the JSON is absent, not zero.
    let sparse: Superelevation = serde_json::from_str(r#"{"sections":[{"points":[{"station_m":10,"right_pct":2.5}]}]}"#).unwrap();
    assert_eq!(sparse.cross_fall_at(10.0), fall(None, Some(2.5)));
}
