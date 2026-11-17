//! LandXML profile, station equations and superelevation, and placing signs with them.
//!
//! `tests/landxml/corridor.xml` and `corridor_feet.xml` were built by hand to the LandXML 1.2
//! schema (they validate against LandXML-1.2.xsd). **Neither was exported by Civil 3D.** Their
//! geometry is tangents along the grid and quarter circles so that every expected value below is
//! worked by hand; the grades, curve lengths, cross-fall and stations in them are arbitrary test
//! numbers, not design values.
//!
//! MC10, internal stations: line east 1000 to 1300, quarter circle left (R 200, centre E 300
//! N 200) 1300 to 1300 + 100π, line north to 1600 + 100π.

use std::f64::consts::PI;
use std::path::PathBuf;

use afrobim_alignment::{
    place, place_with, read_landxml, Alignment, AlignmentBuilder, CriticalStation, CrossFall, CrossFallAdjust, CrossFallPoint, DisplayedStation,
    LandXmlImport, PlaceOptions, PlacementError, Profile, ProfileError, Pvi, SignLocation, SignSide, StationEquationError, StationReference,
    Superelevation, TrafficSide, VerticalCurve,
};

fn import(name: &str) -> LandXmlImport {
    read_landxml(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("landxml").join(name)).unwrap()
}

fn assert_close(got: f64, want: f64, tol: f64, what: &str) {
    assert!((got - want).abs() <= tol, "{what}: got {got}, want {want} (off by {:e})", got - want);
}

fn mc10() -> Alignment {
    import("corridor.xml").alignments.into_iter().find(|a| a.name == "MC10").unwrap()
}

fn sign(station: f64, offset: f64) -> SignLocation {
    SignLocation { alignment: Some("MC10".into()), station: Some(station), offset: Some(offset), side: None }
}

#[test]
fn reads_the_profile() {
    let a = mc10();
    assert_close(a.end_station_m(), 1600.0 + 100.0 * PI, 1e-9, "end station");
    let p = a.profile.as_ref().unwrap();
    assert_eq!(p.name, "FG"); // the ProfAlign; the ProfSurf beside it is not read
    assert_eq!(
        p.pvis,
        [
            Pvi::point(1000.0, 100.0),
            Pvi::parabola(1200.0, 104.0, 120.0),
            Pvi::asymmetric(1500.0, 101.0, 80.0, 40.0),
            Pvi::circular(1700.0, 105.0, 2000.0),
            Pvi::point(1900.0, 101.0),
        ]
    );
    assert_eq!(p.range(), Some((1000.0, 1900.0)));
    let z = |s: f64| p.elevation_at(s).unwrap();
    let g = |s: f64| p.grade_at(s).unwrap();
    // Grades: (104 − 100)/200 = +0.02, (101 − 104)/300 = −0.01, (105 − 101)/200 = +0.02, (101 − 105)/200 = −0.02.
    assert_close(z(1100.0), 102.0, 1e-9, "on the first grade");
    // Parabola L = 120 at 1200, A = −0.03: PVC 1140 at 104 − 0.02·60 = 102.8; y = 102.8 + 0.02x − 0.03x²/240.
    assert_close(z(1140.0), 102.8, 1e-9, "PVC");
    assert_close(z(1200.0), 103.55, 1e-9, "at the PVI: 102.8 + 1.2 − 0.45");
    assert_close(z(1220.0), 103.6, 1e-9, "high point: 102.8 + 1.6 − 0.8");
    assert_close(g(1220.0), 0.0, 1e-12, "high point grade");
    assert_close(z(1260.0), 103.4, 1e-9, "PVT: 104 − 0.01·60");
    assert_close(z(1350.0), 102.5, 1e-9, "second grade: 104 − 0.01·150");
    // Asymmetric at 1500, L1 = 80, L2 = 40, A = +0.03: r1 = 0.03·40/9600 = 0.000125, r2 = 0.03·80/4800 = 0.0005.
    // PVC 1420 at 101 + 0.01·80 = 101.8.
    assert_close(z(1420.0), 101.8, 1e-9, "PVC");
    assert_close(z(1460.0), 101.5, 1e-9, "x = 40: 101.8 − 0.4 + 0.000125·1600/2");
    assert_close(z(1500.0), 101.4, 1e-9, "at the PVI: 101 + 0.03·80·40/240");
    assert_close(g(1500.0), 0.0, 1e-12, "grade where the two parabolas meet: −0.01 + 0.03·40/120");
    assert_close(z(1520.0), 101.5, 1e-9, "x' = 20: 101.4 + 0.0005·400/2");
    assert_close(z(1540.0), 101.8, 1e-9, "PVT: 101 + 0.02·40");
    assert_close(z(1650.0), 104.0, 1e-9, "third grade: 101 + 0.02·150");
    // Circle R = 2000 between +0.02 and −0.02 at (1700, 105): centre below the PVI by 2000·√1.0004.
    let top = 105.0 - 2000.0 * (1.0004_f64.sqrt() - 1.0);
    assert_close(z(1700.0), top, 1e-9, "top of the circular curve");
    assert_close(z(1720.0), 105.0 - 2000.0 * 1.0004_f64.sqrt() + (2000.0_f64.powi(2) - 400.0).sqrt(), 1e-9, "20 m past it");
    assert_close(z(1800.0), 103.0, 1e-9, "last grade: 105 − 0.02·100");
    assert_close(z(1900.0), 101.0, 1e-9, "last PVI");
    // The profile stops at 1900; the alignment goes on to 1914.159.
    assert_eq!(p.elevation_at(1910.0), Err(ProfileError::OutOfRange { station_m: 1910.0, start_m: 1000.0, end_m: 1900.0 }));
}

#[test]
fn reads_the_station_equations() {
    let a = mc10();
    let e = &a.station_equations;
    assert_eq!(e.equations.len(), 2);
    assert_eq!((e.equations[0].internal_m, e.equations[0].back_m, e.equations[0].ahead_m), (1300.0, Some(1300.0), 1350.0));
    assert_eq!((e.equations[1].internal_m, e.equations[1].back_m, e.equations[1].ahead_m), (1600.0, Some(1650.0), 1500.0));
    // Region 1 is internal + 50, region 2 internal − 100.
    assert_eq!(a.displayed_station(1200.0), Some(DisplayedStation { station_m: 1200.0, region: 0 }));
    assert_eq!(a.displayed_station(1450.0), Some(DisplayedStation { station_m: 1500.0, region: 1 }));
    assert_eq!(a.displayed_station(1900.0), Some(DisplayedStation { station_m: 1800.0, region: 2 }));
    assert_eq!(a.station_label(1900.0, 3), "1+800.000");
    assert_eq!(a.internal_station(1400.0, None), Ok(1350.0));
    assert!(matches!(a.internal_station(1320.0, None), Err(StationEquationError::NotOnAlignment { .. }))); // the gap
    assert!(matches!(a.internal_station(1550.0, None), Err(StationEquationError::Ambiguous { .. }))); // the overlap
    assert_eq!(a.internal_station(1550.0, Some(1)), Ok(1500.0));
    assert_eq!(a.internal_station(1550.0, Some(2)), Ok(1650.0));
}

#[test]
fn reads_the_superelevation_as_far_as_the_file_states_it() {
    let all = import("corridor.xml");
    let a = all.alignments.iter().find(|a| a.name == "MC10").unwrap();
    let s = a.superelevation.as_ref().unwrap();
    assert_eq!(s.sections.len(), 1);
    let end_of_curve = 1614.159265;
    let want = [
        (1240.0, None, CriticalStation::BeginRunout),
        (1260.0, None, CriticalStation::BeginRunoff),
        // The curve turns left, so the left side is the inside and falls at the full rate.
        (1300.0, Some((-5.0, 5.0)), CriticalStation::FullSuper),
        (end_of_curve, Some((-5.0, 5.0)), CriticalStation::Runoff),
        (end_of_curve + 40.0, None, CriticalStation::StartOfRunout),
        (end_of_curve + 60.0, None, CriticalStation::EndOfRunout),
    ];
    assert_eq!(s.sections[0].points.len(), want.len());
    for (p, (station, falls, kind)) in s.sections[0].points.iter().zip(want) {
        assert_close(p.station_m, station, 1e-9, "station");
        assert_eq!((p.left_pct, p.right_pct), (falls.map(|f| f.0), falls.map(|f| f.1)), "{kind:?}");
        assert_eq!(p.kind, Some(kind));
    }
    let known = CrossFall { left_pct: Some(-5.0), right_pct: Some(5.0) };
    assert_eq!(s.cross_fall_at(1300.0), known);
    assert_eq!(s.cross_fall_at(1450.0), known);
    // LandXML gives one number, the full rate. Through the runoff and runout, and on the
    // tangents, the file states no cross-fall, so there is none: no normal crown is assumed.
    for station in [1100.0, 1240.0, 1250.0, 1280.0, 1630.0, 1660.0, 1800.0] {
        assert_eq!(s.cross_fall_at(station), CrossFall::default(), "{station}");
    }

    // A right-hand curve: the right side is the inside. With AdverseSE = adverse it is the other way.
    let full = |name: &str| all.alignments.iter().find(|a| a.name == name).unwrap().superelevation.as_ref().unwrap().cross_fall_at(175.0);
    assert_eq!(full("Right"), CrossFall { left_pct: Some(4.0), right_pct: Some(-4.0) });
    assert_eq!(full("Adverse"), CrossFall { left_pct: Some(-4.0), right_pct: Some(4.0) });

    // Everything the importer decided is in the notes.
    let expected = [
        "Alignment 'MC10': profile 'FG' runs from station 1+000.000 to 1+900.000, short of the alignment (1+000.000 to 1+914.159); there is no level outside the profile.",
        "Alignment 'MC10': profile 'FG': the alignment has station equations; the profile's stations are taken to be internal stations (no equations applied).",
        "Alignment 'MC10': superelevation 1: FullSuperelev 5 % was applied from station 1+300.000 to 1+614.159 as left -5 %, right +5 % (the horizontal curve there turns left); the file gives no cross-fall at its other stations.",
        "Alignment 'Right': superelevation 1: FullSuperelev 4 % was applied from station 0+100.000 to 0+250.000 as left +4 %, right -4 % (the horizontal curve there turns right); the file gives no cross-fall at its other stations.",
        "Alignment 'Adverse': superelevation 1: FullSuperelev 4 % was applied from station 0+100.000 to 0+250.000 as left -4 %, right +4 % (the horizontal curve there turns right, and AdverseSE is adverse); the file gives no cross-fall at its other stations.",
    ];
    assert_eq!(all.notes, expected, "{:#?}", all.notes);
}

#[test]
fn respects_the_file_units_and_reports_what_it_could_not_use() {
    let all = import("corridor_feet.xml");
    let ft = 0.3048;
    let note = |needle: &str| all.notes.iter().find(|n| n.contains(needle)).unwrap_or_else(|| panic!("no note about {needle} in {:#?}", all.notes));

    let a = &all.alignments[0];
    assert_eq!(a.name, "Straight");
    assert_close(a.length(), 500.0 * ft, 1e-9, "length");
    // PVIs (0, 100), (250, 110), (500, 100) ft with a 200 ft parabola: g = ±0.04, A = −0.08.
    // On the curve at the PVI: 110 + A·L/8 = 110 − 0.08·200/8 = 108 ft.
    let p = a.profile.as_ref().unwrap();
    assert_eq!(p.name, "FG");
    assert_eq!(p.pvis[1].curve, VerticalCurve::Parabola { length_m: 200.0 * ft });
    assert_close(p.elevation_at(250.0 * ft).unwrap(), 108.0 * ft, 1e-9, "level at the PVI station");
    assert_close(p.elevation_at(100.0 * ft).unwrap(), 104.0 * ft, 1e-9, "on the grade");
    assert_close(p.grade_at(100.0 * ft).unwrap(), 0.04, 1e-12, "a grade has no unit");
    assert_eq!(note("design profiles"), "Alignment 'Straight': it has 2 design profiles (ProfAlign); 'FG' is used and 'Alt' ignored.");
    // The equation has no staBack: at internal 300 ft the displayed station becomes 1000 ft.
    assert_eq!(a.station_equations.equations.len(), 1);
    assert_eq!(a.station_equations.equations[0].back_m, None);
    assert_close(a.displayed_station(400.0 * ft).unwrap().station_m, 1100.0 * ft, 1e-9, "displayed");
    assert_close(a.internal_station(1100.0 * ft, None).unwrap(), 400.0 * ft, 1e-9, "internal");
    // The superelevation is on a straight: its stations are kept (in metres), but no side can be told.
    let s = a.superelevation.as_ref().unwrap();
    assert_close(s.sections[0].points[0].station_m, 100.0 * ft, 1e-9, "FullSuperSta");
    assert_close(s.sections[0].points[1].station_m, 200.0 * ft, 1e-9, "RunoffSta");
    assert_eq!(s.cross_fall_at(150.0 * ft), CrossFall::default());
    assert_eq!(
        note("'Straight': superelevation 1"),
        "Alignment 'Straight': superelevation 1: no cross-fall was assigned, because the horizontal alignment is straight at station 0+045.720, so the inside of the curve cannot be told."
    );

    let b = &all.alignments[1];
    assert_eq!(b.name, "Faulty");
    assert_close(b.length(), 500.0 * ft, 1e-9, "the horizontal alignment is still imported");
    // staBack 250 ft does not agree with the stationing before the equation (200 ft): noted, and the equation kept.
    assert!(note("'Faulty': the station equation").contains("staBack 76.2 m, but the stationing before it arrives at 60.96 m; the latter is used."));
    assert_eq!(b.station_equations.equations[0].back_m, None);
    assert_close(b.displayed_station(300.0 * ft).unwrap().station_m, 500.0 * ft, 1e-9, "400 + (300 − 200) ft");
    assert!(b.profile.is_none());
    assert_eq!(note("profile 'Bad'"), "Alignment 'Faulty': profile 'Bad' was not imported: PVI 0 is the first point of the profile and cannot carry a vertical curve");
    assert_eq!(
        note("superelevation 1 was not"),
        "Alignment 'Faulty': superelevation 1 was not applied as cross-fall: its stations are not in order (RunoffSta is not after FullSuperSta); the element is kept as written in the superelevation table of parse_landxml_file."
    );
    assert!(note("'Faulty': superelevation 2").contains("its FullSuperelev is negative (-2 %) and the schema does not say what a sign means"));
    let s = b.superelevation.as_ref().unwrap();
    assert_eq!(s.sections.len(), 1);
    assert_eq!(s.cross_fall_at(120.0 * ft), CrossFall::default());
    assert_eq!(all.notes.len(), 7, "{:#?}", all.notes);
}

#[test]
fn a_sign_takes_its_level_from_the_profile() {
    let a = mc10();
    // Internal station 1200, 3.5 m left, on the first tangent (heading east): E 200, N 3.5.
    // Level: the parabola at its PVI station, 103.55 m (worked in `reads_the_profile`).
    let placed = place(&a, &sign(1200.0, -3.5), TrafficSide::Left).unwrap();
    let p = placed.positions[0];
    assert_eq!(p.side, SignSide::Left);
    assert_close(p.x_mm, 200_000.0, 1e-6, "x");
    assert_close(p.y_mm, 3_500.0, 1e-6, "y");
    assert_close(p.z_mm, 103_550.0, 1e-6, "z");
    assert_eq!((p.station_m, p.offset_m, p.cross_fall_pct), (1200.0, -3.5, None));
    let site = p.site_placement(&a.name);
    assert_close(site.z, 103_550.0, 1e-6, "kernel z, mm");
    assert_eq!((site.station_m, site.offset_m), (Some(1200.0), Some(-3.5)));

    // The default makes no cross-fall adjustment even where the alignment has cross-fall:
    // internal 1350 is 50 m into the curve, level 104 − 0.01·150 = 102.5 on both sides.
    let both = place(&a, &SignLocation { side: Some(SignSide::Both), ..sign(1350.0, 3.5) }, TrafficSide::Left).unwrap();
    assert_eq!(both.positions.len(), 2);
    for p in &both.positions {
        assert_close(p.z_mm, 102_500.0, 1e-6, "centreline level");
        assert_eq!(p.cross_fall_pct, None);
    }

    // Beyond the end of the profile there is no level, and place says so rather than use 0.
    let off = place(&a, &sign(1910.0, -3.5), TrafficSide::Left).unwrap_err();
    assert_eq!(off, PlacementError::Profile { alignment: "MC10".into(), source: ProfileError::OutOfRange { station_m: 1910.0, start_m: 1000.0, end_m: 1900.0 } });
    assert_eq!(off.to_string(), "alignment 'MC10': no level for the sign: station 1+910.000 is outside the profile, which runs from 1+000.000 to 1+900.000");

    // Without a profile nothing changes: level 0, as before.
    let flat = AlignmentBuilder::new("flat", 0.0, 0.0, 0.0, 0.0).line(100.0).build().unwrap();
    let p = place(&flat, &sign(50.0, 2.0), TrafficSide::Left).unwrap().positions[0];
    assert_eq!((p.x_mm, p.y_mm, p.z_mm, p.cross_fall_pct), (50_000.0, -2_000.0, 0.0, None));
    assert_eq!(p.site_placement("flat").z, 0.0);
}

#[test]
fn a_displayed_station_is_taken_through_the_equations_only_when_asked() {
    let a = mc10();
    let displayed = |region| PlaceOptions { station: StationReference::Displayed { region }, ..Default::default() };

    // Displayed 1400 is in region 1 only: internal 1350, 50 m into the curve. The curve's centre is
    // (E 300, N 200); after 50 m the radius has turned 50/200 = 0.25 rad from pointing south, so the
    // centreline is at (300 + 200 sin 0.25, 200 − 200 cos 0.25), and 3.5 m left (inside) at radius 196.5.
    let p = place_with(&a, &sign(1400.0, -3.5), TrafficSide::Left, &displayed(None)).unwrap().positions[0];
    assert_eq!(p.station_m, 1350.0); // the internal station is what is stored
    assert_close(p.x_mm, (300.0 + 196.5 * 0.25_f64.sin()) * 1000.0, 1e-5, "x");
    assert_close(p.y_mm, (200.0 - 196.5 * 0.25_f64.cos()) * 1000.0, 1e-5, "y");
    assert_close(p.z_mm, 102_500.0, 1e-6, "z: 104 − 0.01·150");
    assert_eq!(a.station_label(p.station_m, 3), "1+400.000");

    // The same number as an internal station is a different place: 100 m into the curve.
    let q = place(&a, &sign(1400.0, -3.5), TrafficSide::Left).unwrap().positions[0];
    assert_eq!(q.station_m, 1400.0);
    assert_close(q.x_mm, (300.0 + 196.5 * 0.5_f64.sin()) * 1000.0, 1e-5, "x");

    // Displayed 1550 occurs twice. Without a region it is refused, and the error lists both places.
    let err = place_with(&a, &sign(1550.0, -3.5), TrafficSide::Left, &displayed(None)).unwrap_err();
    let PlacementError::Station { source: StationEquationError::Ambiguous { candidates, .. }, .. } = &err else { panic!("{err:?}") };
    assert_eq!(candidates.iter().map(|c| (c.region, c.internal_m)).collect::<Vec<_>>(), [(1, 1500.0), (2, 1650.0)]);
    assert_eq!(err.to_string(), "alignment 'MC10': station 1+550.000 occurs 2 times on the alignment (regions 1, 2); say which region is meant");
    // With the region it is placed. Region 2: internal 1650, on the last tangent (heading north,
    // E 500), 1650 − (1300 + 100π) m past the end of the curve at N 200. Left of north is west.
    let p = place_with(&a, &sign(1550.0, -3.5), TrafficSide::Left, &displayed(Some(2))).unwrap().positions[0];
    assert_eq!(p.station_m, 1650.0);
    assert_close(p.x_mm, 496_500.0, 1e-5, "x");
    assert_close(p.y_mm, (200.0 + 1650.0 - 1300.0 - 100.0 * PI) * 1000.0, 1e-5, "y");
    assert_close(p.z_mm, 104_000.0, 1e-6, "z: 101 + 0.02·150");
    // Region 1: internal 1500.
    let p = place_with(&a, &sign(1550.0, -3.5), TrafficSide::Left, &displayed(Some(1))).unwrap().positions[0];
    assert_eq!(p.station_m, 1500.0);
    assert_close(p.z_mm, 101_400.0, 1e-6, "z: on the asymmetric curve at its PVI");

    // A station in the gap is nowhere.
    let err = place_with(&a, &sign(1320.0, -3.5), TrafficSide::Left, &displayed(None)).unwrap_err();
    assert!(matches!(err, PlacementError::Station { source: StationEquationError::NotOnAlignment { .. }, .. }), "{err:?}");
    // ... though as an internal station 1320 is 20 m into the curve.
    assert_eq!(place(&a, &sign(1320.0, -3.5), TrafficSide::Left).unwrap().positions[0].station_m, 1320.0);

    // On an alignment without equations a displayed station is the internal station.
    let plain = AlignmentBuilder::new("plain", 0.0, 0.0, 0.0, 0.0).line(100.0).build().unwrap();
    let p = place_with(&plain, &sign(40.0, 2.0), TrafficSide::Left, &displayed(None)).unwrap().positions[0];
    assert_eq!((p.station_m, p.x_mm), (40.0, 40_000.0));
}

#[test]
fn the_cross_fall_adjustment_is_opt_in() {
    let a = mc10();
    let with = |cross_fall| PlaceOptions { cross_fall, ..Default::default() };
    let both = |station: f64| SignLocation { side: Some(SignSide::Both), ..sign(station, 3.5) };

    // Internal 1350: centreline 102.5 m; cross-fall left −5 %, right +5 %.
    // Left, 3.5 m: 102.5 − 3.5·0.05 = 102.325. Right, 3.5 m: 102.5 + 0.175 = 102.675.
    for mode in [CrossFallAdjust::WhereKnown, CrossFallAdjust::Required] {
        let placed = place_with(&a, &both(1350.0), TrafficSide::Left, &with(mode)).unwrap();
        let (left, right) = (placed.positions[0], placed.positions[1]);
        assert_eq!((left.side, left.cross_fall_pct), (SignSide::Left, Some(-5.0)));
        assert_close(left.z_mm, 102_325.0, 1e-6, "left");
        assert_eq!((right.side, right.cross_fall_pct), (SignSide::Right, Some(5.0)));
        assert_close(right.z_mm, 102_675.0, 1e-6, "right");
        assert_close(left.site_placement("MC10").z, 102_325.0, 1e-6, "kernel z");
    }
    // Off (the default) leaves the centreline level.
    let off = place_with(&a, &both(1350.0), TrafficSide::Left, &with(CrossFallAdjust::Off)).unwrap();
    assert_eq!(off, place(&a, &both(1350.0), TrafficSide::Left).unwrap());
    assert_close(off.positions[1].z_mm, 102_500.0, 1e-6, "no adjustment");

    // Internal 1650 is in the runoff, where the file states no cross-fall. Centreline 104.0 m.
    let kept = place_with(&a, &both(1650.0), TrafficSide::Left, &with(CrossFallAdjust::WhereKnown)).unwrap();
    for p in &kept.positions {
        assert_close(p.z_mm, 104_000.0, 1e-6, "centreline level kept");
        assert_eq!(p.cross_fall_pct, None);
    }
    let err = place_with(&a, &both(1650.0), TrafficSide::Left, &with(CrossFallAdjust::Required)).unwrap_err();
    assert_eq!(err, PlacementError::CrossFallUnknown { alignment: "MC10".into(), station: "1+550.000".into(), side: "left" });
    assert_eq!(err.to_string(), "alignment 'MC10' has no stored cross-fall for the left side at station 1+550.000");

    // Only one side stated: that side is adjusted, the other is not (or is refused).
    let mut one_sided = a.clone();
    one_sided.superelevation = Some(Superelevation::from_points(vec![CrossFallPoint::new(1300.0, None, Some(2.0)), CrossFallPoint::new(1400.0, None, Some(4.0))]).unwrap());
    // Right at 1350: halfway from 2 % to 4 % = 3 %; 102.5 + 3.5·0.03 = 102.605.
    let placed = place_with(&one_sided, &both(1350.0), TrafficSide::Left, &with(CrossFallAdjust::WhereKnown)).unwrap();
    assert_eq!(placed.positions[0].cross_fall_pct, None);
    assert_close(placed.positions[0].z_mm, 102_500.0, 1e-6, "left kept");
    assert_eq!(placed.positions[1].cross_fall_pct, Some(3.0));
    assert_close(placed.positions[1].z_mm, 102_605.0, 1e-6, "right adjusted");
    assert!(place_with(&one_sided, &sign(1350.0, 3.5), TrafficSide::Left, &with(CrossFallAdjust::Required)).is_ok());
    assert!(matches!(
        place_with(&one_sided, &sign(1350.0, -3.5), TrafficSide::Left, &with(CrossFallAdjust::Required)),
        Err(PlacementError::CrossFallUnknown { side: "left", .. })
    ));

    // Cross-fall without a profile has no level to adjust.
    let mut no_profile = a.clone();
    no_profile.profile = None;
    let p = place_with(&no_profile, &sign(1350.0, -3.5), TrafficSide::Left, &with(CrossFallAdjust::WhereKnown)).unwrap().positions[0];
    assert_eq!((p.z_mm, p.cross_fall_pct), (0.0, None));
    assert_eq!(
        place_with(&no_profile, &sign(1350.0, -3.5), TrafficSide::Left, &with(CrossFallAdjust::Required)),
        Err(PlacementError::NoProfile { alignment: "MC10".into() })
    );
    // A profile but no superelevation at all.
    let mut no_super = a.clone();
    no_super.superelevation = None;
    let p = place_with(&no_super, &sign(1350.0, -3.5), TrafficSide::Left, &with(CrossFallAdjust::WhereKnown)).unwrap().positions[0];
    assert_close(p.z_mm, 102_500.0, 1e-6, "centreline level");
    assert!(matches!(
        place_with(&no_super, &sign(1350.0, -3.5), TrafficSide::Left, &with(CrossFallAdjust::Required)),
        Err(PlacementError::CrossFallUnknown { .. })
    ));
}

#[test]
fn the_whole_alignment_persists_as_json() {
    let a = mc10();
    let json = serde_json::to_string(&a).unwrap();
    let back: Alignment = serde_json::from_str(&json).unwrap();
    assert_eq!(back.profile, a.profile);
    assert_eq!(back.superelevation, a.superelevation);
    assert_eq!(back.station_equations, a.station_equations);
    // It places the same sign in the same place.
    let options = PlaceOptions { station: StationReference::Displayed { region: Some(1) }, cross_fall: CrossFallAdjust::Required };
    let (p, q) = (
        place_with(&a, &sign(1400.0, -3.5), TrafficSide::Left, &options).unwrap().positions[0],
        place_with(&back, &sign(1400.0, -3.5), TrafficSide::Left, &options).unwrap().positions[0],
    );
    assert_close(q.z_mm, p.z_mm, 1e-9, "z");
    assert_close(p.z_mm, 102_325.0, 1e-6, "z: 102.5 − 3.5·0.05");
    // A profile set by hand is used the same way.
    let mut by_hand = AlignmentBuilder::new("h", 0.0, 0.0, 0.0, 0.0).line(100.0).build().unwrap();
    by_hand.profile = Some(Profile::new("p", vec![Pvi::point(0.0, 10.0), Pvi::point(100.0, 12.0)]).unwrap());
    assert_close(place(&by_hand, &sign(25.0, 1.0), TrafficSide::Left).unwrap().positions[0].z_mm, 10_500.0, 1e-9, "10 + 0.02·25");
}
