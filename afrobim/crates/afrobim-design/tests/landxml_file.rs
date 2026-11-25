// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Whole-file LandXML round trips: `parse_landxml_file` then `write_landxml_file` then
//! `parse_landxml_file` again.
//!
//! The files in `tests/landxml/` were written by hand to the LandXML 1.2 schema (they validate
//! against LandXML-1.2.xsd) and carry coordinates of true UTM size, so that any rounding shows:
//! easting about 450 000 m, northing about 35 000 m (north of the equator) and about 9 960 000 m
//! (south of it). The writer prints every `f64` in its shortest round-trip form, so the tests
//! compare coordinates for exact equality, which is stricter than the 0.0001 m asked for. None of
//! these files was exported by Civil 3D; the Civil 3D file used in the last test is the one in
//! `afrobim-alignment/tests/landxml/`.
//!
//! Set `AFROBIM_LANDXML_OUT` to a directory to have the tests leave what they wrote there, for
//! `xmllint --noout --schema LandXML-1.2.xsd`.

use std::path::PathBuf;

use afrobim_alignment::{
    parse_landxml_file, read_landxml_file, Alignment, CriticalStation, CrossFallPoint, LandXmlFile, LandXmlParcel, Superelevation, SuperelevationSection,
};
use afrobim_corridor::Tin;
use afrobim_design::{surface_from_tin, write_landxml_file, LandXmlOptions};

fn file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("landxml").join(name)
}

fn options() -> LandXmlOptions {
    LandXmlOptions { date: "2026-11-16".into(), time: "09:00:00".into(), project: String::new() }
}

/// Writes, keeps a copy when asked, and reads back.
fn round_trip(label: &str, f: &LandXmlFile) -> (String, Vec<String>, LandXmlFile) {
    let out = write_landxml_file(f, &options());
    if let Ok(dir) = std::env::var("AFROBIM_LANDXML_OUT") {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(PathBuf::from(dir).join(format!("{label}.xml")), &out.text).unwrap();
    }
    let back = parse_landxml_file(&out.text).unwrap_or_else(|e| panic!("{label}: {e}\n{}", out.text));
    (out.text, out.warnings, back)
}

fn areas_in_square_metres(parcels: &[LandXmlParcel]) -> Vec<LandXmlParcel> {
    parcels
        .iter()
        .map(|p| LandXmlParcel { area: p.area_m2, parcels: areas_in_square_metres(&p.parcels), ..p.clone() })
        .collect()
}

#[test]
fn a_whole_file_north_of_the_equator_round_trips() {
    let f = read_landxml_file(&file("full_utm_north.xml")).unwrap();
    let (text, warnings, back) = round_trip("north", &f);
    assert!(warnings.is_empty(), "{warnings:#?}");

    // Coordinates are written as they are held: the digits of the fixture, nothing more or less.
    assert!(text.contains("<CgPoint name=\"CP1\""), "{text}");
    assert!(text.contains(">35012.3456 450123.4567 1150.1234</CgPoint>"), "{text}");
    assert!(text.contains("<P id=\"3\">35010.0001 450010.0001 1152</P>"), "{text}");

    // CoordinateSystem, with every attribute it had; the project; the units' angles.
    assert_eq!(back.coordinate_system, f.coordinate_system);
    assert_eq!(back.coordinate_system.as_ref().unwrap().rotation_angle, Some(0.5));
    assert_eq!(back.project, f.project);
    assert_eq!((back.units.angular_unit.as_str(), back.units.direction_unit.as_str()), ("decimal degrees", "decimal degrees"));
    // The source's Application is not carried over: the output says what made it.
    assert_eq!(back.applications.len(), 1);
    assert_eq!(back.applications[0].name, "AfroBIM afrobim-design");

    // CgPoints with their group.
    assert_eq!(back.cg_points, f.cg_points);
    let p = &back.cg_points[0].points[0];
    assert_eq!((p.northing_m, p.easting_m, p.elevation_m), (35012.3456, 450123.4567, Some(1150.1234)));

    // Parcels: the area is written in square metres, so compare in those.
    assert_eq!(areas_in_square_metres(&back.parcels), areas_in_square_metres(&f.parcels));
    assert_eq!(back.parcels[0].area_m2, Some(2500.0));
    assert_eq!(back.parcels[0].parcels[0].name, "Lot 1A");

    // Plan features, with the irregular line and the chain.
    assert_eq!(back.plan_features, f.plan_features);

    // Surfaces: points, faces (with the invisible one) and every kind of source data.
    assert_eq!(back.surfaces, f.surfaces);
    assert_eq!(back.surfaces[0].points[0].northing_m, 35000.0001);
    assert!(back.surfaces[0].faces[2].invisible);

    // Existing-ground profile and cross-sections.
    assert_eq!(back.ground_profiles, f.ground_profiles);
    assert_eq!(back.cross_sections, f.cross_sections);
    assert_eq!(back.cross_sections[0].design_surfaces[0].points[2].offset_m, 3.5);

    // The alignment keeps its start exactly, and its geometry and design profile to rounding of the arithmetic.
    let (a, b) = (&f.alignments[0], &back.alignments[0]);
    assert_eq!((b.elements[0].x, b.elements[0].y), (450000.5678, 35000.1234));
    assert_eq!(b.elements.len(), a.elements.len());
    for (p, q) in a.elements.iter().zip(&b.elements) {
        assert_eq!((p.x, p.y), (q.x, q.y));
        assert!((p.length - q.length).abs() < 1e-6);
    }
    assert_eq!(b.profile, a.profile);
    assert_eq!(b.start_station_m, 1000.0);

    // The summary of what was written equals the summary of what was read, class by class.
    assert_eq!(back.summary.cg_points, f.summary.cg_points);
    assert_eq!(back.summary.parcels, f.summary.parcels);
    assert_eq!(back.summary.plan_features, f.summary.plan_features);
    assert_eq!(back.summary.surfaces, f.summary.surfaces);
    assert_eq!(back.summary.counts, f.summary.counts);
}

#[test]
fn a_whole_file_south_of_the_equator_round_trips() {
    let f = read_landxml_file(&file("full_utm_south.xml")).unwrap();
    let (text, warnings, back) = round_trip("south", &f);
    assert!(warnings.is_empty(), "{warnings:#?}");
    assert!(text.contains(">9960012.3456 450123.4567 1150.1234</CgPoint>"), "{text}");
    assert_eq!(back.cg_points, f.cg_points);
    assert_eq!(back.surfaces, f.surfaces);
    assert_eq!(back.coordinate_system.as_ref().unwrap().epsg_code.as_deref(), Some("32736"));
    assert_eq!((back.alignments[0].elements[0].x, back.alignments[0].elements[0].y), (450123.4567, 9960012.3456));
    assert_eq!(areas_in_square_metres(&back.parcels), areas_in_square_metres(&f.parcels));
    let e = back.summary.overall_extent().unwrap();
    assert_eq!(e.min_northing, 9960000.0001);
}

#[test]
fn a_tin_becomes_a_surface_and_comes_back_point_for_point() {
    // UTM-sized, south of the equator, with sub-millimetre digits.
    let points = vec![[450000.1234, 9960000.5678, 1150.25], [450010.1234, 9960000.5678, 1151.0], [450010.1234, 9960010.5678, 1152.5], [450000.1234, 9960010.5678, 1151.5]];
    let tin = Tin::new(points.clone(), vec![[0, 1, 2], [0, 2, 3]]).unwrap();
    let surface = surface_from_tin("EG", &tin);
    assert_eq!((surface.surf_type.as_deref(), surface.points.len(), surface.faces.len()), (Some("TIN"), 4, 2));
    let f = LandXmlFile { surfaces: vec![surface], ..Default::default() };
    let (text, warnings, back) = round_trip("tin", &f);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(text.contains("<Faces>") && text.contains("<F>1 2 3</F>") && text.contains("<F>1 3 4</F>"), "{text}");
    let s = &back.surfaces[0];
    for (p, q) in points.iter().zip(&s.points) {
        assert_eq!([q.easting_m, q.northing_m, q.elevation_m], *p);
    }
    assert_eq!((s.faces[0].ids.clone(), s.faces[1].ids.clone()), (vec![1, 2, 3], vec![1, 3, 4]));
    // And it is a TIN again.
    let corridor = afrobim_corridor::parse_landxml_surfaces(&text).unwrap();
    assert_eq!(corridor.surfaces[0].tin.points(), &points[..]);
    assert_eq!(corridor.surfaces[0].tin.triangles(), tin.triangles());
}

#[test]
fn superelevation_is_read_written_and_read_again() {
    let f = read_landxml_file(&file("superelevation.xml")).unwrap();
    let a = &f.alignments[0];
    let se = a.superelevation.as_ref().expect("the superelevation was read");
    assert_eq!(se.sections.len(), 1);
    let kinds: Vec<Option<CriticalStation>> = se.sections[0].points.iter().map(|p| p.kind).collect();
    assert_eq!(kinds.len(), 6);
    // The curve turns left and is not adverse: the left side falls at 6 % and the right side rises.
    let full = se.sections[0].points.iter().find(|p| p.kind == Some(CriticalStation::FullSuper)).unwrap();
    assert_eq!((full.station_m, full.left_pct, full.right_pct), (100.0, Some(-6.0), Some(6.0)));

    let (text, warnings, back) = round_trip("superelevation", &f);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(text.contains("<FullSuperelev>6</FullSuperelev>") && text.contains("<AdverseSE>non-adverse</AdverseSE>"), "{text}");
    assert_eq!(back.alignments[0].superelevation, a.superelevation);
    assert_eq!(back.coordinate_system.as_ref().unwrap().rotation_angle, Some(0.25));
    assert_eq!((back.alignments[0].elements[0].x, back.alignments[0].elements[0].y), (450000.1234, 9960000.5678));

    // An adverse section (the left side rises on a left-hand curve) comes back adverse.
    let mut adverse = a.clone();
    for s in &mut adverse.superelevation.as_mut().unwrap().sections {
        for p in &mut s.points {
            p.left_pct = p.left_pct.map(|v| -v);
            p.right_pct = p.right_pct.map(|v| -v);
        }
    }
    let f2 = LandXmlFile { alignments: vec![adverse.clone()], ..Default::default() };
    let (text, _, back) = round_trip("superelevation_adverse", &f2);
    assert!(text.contains("<AdverseSE>adverse</AdverseSE>"), "{text}");
    assert_eq!(back.alignments[0].superelevation, adverse.superelevation);
}

#[test]
fn superelevation_that_landxml_cannot_hold_is_left_out_with_a_warning() {
    let f = read_landxml_file(&file("superelevation.xml")).unwrap();
    let mut a: Alignment = f.alignments[0].clone();
    // A crowned section: -2 % on both sides is not one rate with opposite signs.
    a.superelevation = Some(
        Superelevation::new(vec![SuperelevationSection {
            points: vec![
                CrossFallPoint { station_m: 100.0, left_pct: Some(-2.0), right_pct: Some(-2.0), kind: Some(CriticalStation::FullSuper) },
                CrossFallPoint { station_m: 200.0, left_pct: Some(-2.0), right_pct: Some(-2.0), kind: Some(CriticalStation::Runoff) },
            ],
        }])
        .unwrap(),
    );
    let out = write_landxml_file(&LandXmlFile { alignments: vec![a.clone()], ..Default::default() }, &options());
    assert!(!out.text.contains("<Superelevation"), "{}", out.text);
    assert!(out.warnings.iter().any(|w| w.contains("left -2 % and right -2 %") && w.contains("left out")), "{:?}", out.warnings);

    // A section with no cross-fall at its full-super station (the reader's "no rate" case).
    a.superelevation = Some(Superelevation::new(vec![SuperelevationSection { points: vec![CrossFallPoint { station_m: 100.0, left_pct: None, right_pct: None, kind: Some(CriticalStation::FullSuper) }] }]).unwrap());
    let out = write_landxml_file(&LandXmlFile { alignments: vec![a], ..Default::default() }, &options());
    assert!(!out.text.contains("<Superelevation") && out.warnings.len() == 1, "{:?}", out.warnings);
}

#[test]
fn what_cannot_be_placed_in_the_file_is_reported() {
    let f = read_landxml_file(&file("full_utm_north.xml")).unwrap();
    // Take the alignment away: its ground profile and cross-sections have nowhere to go.
    let orphaned = LandXmlFile { alignments: Vec::new(), ..f };
    let out = write_landxml_file(&orphaned, &options());
    assert!(out.warnings.iter().any(|w| w.contains("Ground profile 'EG' belongs to alignment 'CL1'")), "{:?}", out.warnings);
    assert!(out.warnings.iter().any(|w| w.contains("2 cross-section(s) belong to alignments that are not in the file")), "{:?}", out.warnings);
    assert!(!out.text.contains("<ProfSurf") && !out.text.contains("<CrossSect "));
    // What did not need the alignment is there.
    assert!(out.text.contains("<Parcel name=\"Lot 1\"") && out.text.contains("<Surface name=\"EG\""));
}

#[test]
fn an_empty_file_is_still_a_file() {
    let out = write_landxml_file(&LandXmlFile::default(), &LandXmlOptions { project: "Empty".into(), ..options() });
    let back = parse_landxml_file(&out.text).unwrap();
    assert_eq!(back.project.as_ref().unwrap().name, "Empty");
    assert!(back.alignments.is_empty() && back.surfaces.is_empty() && back.cg_points.is_empty());
    assert!(out.warnings.is_empty());
}

#[test]
fn a_real_civil_3d_export_survives_a_round_trip_at_its_coordinates() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("afrobim-alignment").join("tests").join("landxml").join("civil3d_sr15_export.xml");
    let f = read_landxml_file(&path).unwrap();
    assert!(f.is_civil_3d());
    let (_, warnings, back) = round_trip("civil3d", &f);
    // Metres from US survey feet: the coordinates read back are the very numbers that were written.
    assert_eq!(back.plan_features, f.plan_features);
    assert_eq!(back.ground_profiles, f.ground_profiles);
    assert_eq!(back.coordinate_system, f.coordinate_system);
    assert_eq!(back.alignments.len(), 1);
    assert_eq!((back.alignments[0].elements[0].x, back.alignments[0].elements[0].y), (f.alignments[0].elements[0].x, f.alignments[0].elements[0].y));
    assert_eq!(back.alignments[0].profile, f.alignments[0].profile);
    assert!(!back.is_civil_3d());
    assert_eq!(back.units.linear_unit, "meter");
    assert!(warnings.is_empty(), "{warnings:?}");
}
