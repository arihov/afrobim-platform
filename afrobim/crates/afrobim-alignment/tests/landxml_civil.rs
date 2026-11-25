// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! What a Civil 3D file holds beyond the geometry: `civil3d_like.xml` has the structure that
//! Autodesk Civil 3D 2025 writes (spirals with `PI`, `ProfSurf` and `ProfAlign`, `CrossSects` with
//! named `CrossSectSurf`, `Superelevation` elements that are partial or empty, `Feature` and
//! `FieldNote`, a TIN with invisible faces and `PointFiles`), with made-up names, coordinates and
//! numbers. It validates against LandXML-1.2.xsd. The spiral end points are exact to the digits
//! written, so the clothoid check below is a check of the code, not of the file.

mod common;

use std::path::PathBuf;

use afrobim_alignment::{read_landxml_file, CriticalStation, LandXmlProfEntryKind, Turn};
use common::{geometry_report, station_round_trip};

fn file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("landxml").join(name)
}

fn civil() -> afrobim_alignment::LandXmlFile {
    read_landxml_file(&file("civil3d_like.xml")).unwrap()
}

#[test]
fn the_geometry_agrees_with_the_points_the_file_states() {
    let f = civil();
    let r = geometry_report(&f);
    assert_eq!((r.lines, r.curves, r.spirals), (6, 4, 5));
    assert!(r.not_compared.is_empty(), "{:?}", r.not_compared);
    assert!(r.join.value < 1e-6, "{:?}", r.join);
    assert!(r.radius.value < 1e-6, "{:?}", r.radius);
    assert!(r.length.value < 1e-6, "{:?}", r.length);
    assert!(r.element_length.value < 1e-6, "{:?}", r.element_length);
    // The clothoid code, checked against an integration that shares nothing with it.
    assert!(r.spiral_end_independent.value < 1e-6, "{:?}", r.spiral_end_independent);
    assert!(r.spiral_end_engine.value < 1e-6, "{:?}", r.spiral_end_engine);
    assert!(r.element_end_engine.value < 1e-6 && r.element_start_engine.value == 0.0, "{r:?}");
    assert!(r.start_heading.value < 1e-8, "{:?}", r.start_heading);
    // The one vertex of the second alignment that is not a curve: 4.5 degrees.
    assert_eq!(r.kinks.len(), 1);
    assert_eq!((r.kinks[0].0, r.kinks[0].1), (1, 1));
    assert!((r.kinks[0].2.abs() - 4.5).abs() < 1e-9, "{:?}", r.kinks);
    for a in &f.alignments {
        let (ds, off, n) = station_round_trip(a, 2.5);
        assert!(n > 70 && ds < 1e-6 && off < 1e-6, "{}: {ds} {off} {n}", a.name);
    }
}

#[test]
fn accessors_give_everything_the_file_holds_per_alignment() {
    let f = civil();
    assert_eq!(f.alignment_names(), ["Alpha Road", "Beta Spur"]);
    assert_eq!(f.surface_names(), ["Test OGL"]);
    let alpha = f.alignment_view("Alpha Road").unwrap();
    assert_eq!(alpha.info().unwrap().desc.as_deref(), Some("Made-up spirals"));
    assert_eq!(alpha.info().unwrap().declared_length_m, Some(430.0));
    let xs = alpha.cross_sections();
    assert_eq!(xs.len(), 3);
    assert_eq!(xs.iter().map(|c| c.station_m).collect::<Vec<_>>(), [0.0, 20.0, 40.0]);
    assert_eq!(xs[1].name.as_deref(), Some("0+020"));
    assert_eq!(xs[0].surface_names(), ["Test OGL", "Layout"]);
    // Offset and elevation lists, runs kept apart: the gap between offsets 6 and 8 is a gap.
    let g = xs[0].surface("Test OGL").unwrap();
    assert_eq!(g.point_lists.len(), 2);
    assert_eq!(g.point_lists[0][2], [0.000000000002, 1200.5]);
    assert_eq!(g.offset_range(), Some((-10.0, 12.25)));
    assert_eq!(g.elevation_at(-5.5), Some(1200.25));
    assert!((g.elevation_at(3.0).unwrap() - (1200.5 + (1201.0 - 1200.5) * (3.0 - 0.000000000002) / (6.0 - 0.000000000002))).abs() < 1e-12);
    assert_eq!(g.elevation_at(7.0), None);
    assert_eq!(g.elevation_at(20.0), None);
    // The section attributes beyond sta, name, desc.
    let d = f.cross_sect_details.iter().find(|d| d.station_m == 20.0).unwrap();
    assert_eq!((d.area_cut, d.area_fill, d.volume_cut, d.volume_fill, d.sect_type.as_deref()), (Some(12.5), Some(3.25), Some(200.0), Some(40.0), Some("made-up")));
    assert_eq!(f.cross_sect_details.len(), 1);
    assert_eq!(f.cross_sect_groups.len(), 1);
    // Existing ground profile with its name and state, kept in two runs.
    let gp = alpha.ground_profile().unwrap();
    assert_eq!((gp.name.as_str(), gp.state.as_deref(), gp.profile.as_deref(), gp.point_lists.len()), ("Test OGL", Some("existing"), Some("Alpha Road"), 2));
    assert_eq!(gp.station_range(), Some((0.0, 300.0)));
    assert_eq!(gp.elevation_at(75.0), Some(1202.75));
    assert_eq!(gp.elevation_at(230.0), None);
    // Design profile as written.
    let dp = alpha.design_profile().unwrap();
    assert_eq!(dp.name, "Alpha layout");
    assert_eq!(dp.counts(), [2, 1, 0, 0]);
    assert_eq!(dp.entries[1].kind, LandXmlProfEntryKind::ParaCurve);
    assert_eq!((dp.entries[1].station_m, dp.entries[1].elevation_m, dp.entries[1].length_m), (120.0, 1204.0, Some(60.0)));
    assert_eq!(dp.to_profile().unwrap(), *f.alignment("Alpha Road").unwrap().profile.as_ref().unwrap());
    // Features: the property bag of the alignment.
    let feats = alpha.features();
    assert_eq!(feats.len(), 2);
    assert_eq!((feats[0].name.as_deref(), feats[0].code.as_deref(), feats[0].source.as_deref()), (Some("SpeedStation"), Some("0"), Some("Autodesk Civil 3D")));
    assert_eq!((feats[0].number("station"), feats[0].number("speed")), (Some(0.0), Some(60.0)));
    assert_eq!(feats[1].property("speed"), Some("40.00000000"));
    assert_eq!(f.alignment_view("Beta Spur").unwrap().features().len(), 1);
    assert!(f.alignment_view("nope").is_none());
    assert_eq!(f.alignment_views().len(), 2);
    // Nothing the file held is counted as skipped.
    assert!(f.summary.skipped.is_empty(), "{:?}", f.summary.skipped);
    assert_eq!(f.summary.counts.get("features"), Some(&3));
}

#[test]
fn a_profile_with_a_vertical_step_is_kept_whole_and_the_engine_profile_says_why_it_is_not_one() {
    let f = civil();
    let beta = f.alignment("Beta Spur").unwrap();
    assert!(beta.profile.is_none());
    let w = f.warnings.iter().find(|w| w.contains("profile 'Beta layout'")).expect("a warning about the profile");
    assert!(w.contains("steps the level from 1210.5 m to 1209.5 m at station 0+031.500") && w.contains("design_profiles"), "{w}");
    let dp = f.design_profile("Beta Spur").unwrap();
    assert_eq!(dp.entries.len(), 5);
    assert_eq!(dp.steps(), [(31.5, 1210.5, 1209.5)]);
    assert!(dp.to_profile().is_err());
    let parts = dp.segments().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!((parts[0].name.as_str(), parts[0].pvis.len(), parts[1].pvis.len()), ("Beta layout (part 1)", 2, 3));
    assert_eq!(parts[0].range(), Some((0.0, 31.5)));
    assert_eq!(parts[1].range(), Some((31.5, 195.0)));
    assert_eq!(parts[1].elevation_at(31.5).unwrap(), 1209.5);
    // The ground profile of the same alignment is still there.
    assert_eq!(f.ground_profile("Beta Spur").unwrap().points().len(), 5);
}

#[test]
fn superelevation_is_kept_as_the_full_station_table_with_every_rate() {
    let f = civil();
    let t = f.superelevation_table("Alpha Road").unwrap();
    assert_eq!(t.records.len(), 4);
    let r = &t.records[0];
    assert_eq!((r.sta_start_m, r.sta_end_m), (Some(100.0), Some(200.0)));
    assert_eq!(r.stations.len(), 6);
    assert_eq!(r.station(CriticalStation::BeginRunout), Some(60.0));
    assert_eq!(r.station(CriticalStation::EndOfRunout), Some(240.0));
    assert_eq!(r.rate_pct(), Some(4.0));
    assert!(r.stations_ascending());
    assert_eq!(r.turn, Some(Turn::Right));
    // A negative rate and the hand of the curve, which comes from the geometry.
    let r = &t.records[1];
    assert_eq!((r.rate_pct(), r.station(CriticalStation::BeginRunout), r.turn), (Some(-3.5), Some(270.0), Some(Turn::Left)));
    // An element with only staStart and staEnd is kept, empty.
    assert!(t.records[2].is_empty() && t.records[2].sta_start_m == Some(50.0));
    // Stations written in descending order (full super after runoff) are kept as written.
    assert!(!t.records[3].stations_ascending());
    assert_eq!(t.non_empty(), 3);
    // The flat table: every station with the rate on the full-super rows.
    let rows = t.rows();
    assert_eq!(rows.len(), 6 + 4 + 2);
    assert_eq!((rows[2].kind, rows[2].station_m, rows[2].full_rate_pct), (CriticalStation::FullSuper, 120.0, Some(4.0)));
    assert_eq!(rows[0].full_rate_pct, None);
    let b = f.superelevation_table("Beta Spur").unwrap();
    assert_eq!((b.records[0].rate_pct(), b.records[0].turn, b.records[1].rate_pct(), b.records[1].turn), (Some(-4.5), Some(Turn::Left), Some(3.0), Some(Turn::Right)));

    // The sign follows the curve in all four sections that have a curve: said once, and applied.
    assert!(f.warnings.iter().any(|w| w.contains("positive on 2 right-hand and negative on 2 left-hand curves")), "{:#?}", f.warnings);
    let alpha = f.alignment("Alpha Road").unwrap();
    let se = alpha.superelevation.as_ref().unwrap();
    // Left-hand curve, rate -3.5: the inside (left) falls.
    let cf = se.cross_fall_at(350.0);
    assert_eq!((cf.left_pct, cf.right_pct), (Some(-3.5), Some(3.5)));
    let cf = se.cross_fall_at(150.0);
    assert_eq!((cf.left_pct, cf.right_pct), (Some(4.0), Some(-4.0)));
    // The empty element and the descending one are named, not silently dropped.
    assert!(f.warnings.iter().any(|w| w.contains("'Alpha Road': superelevation 3 was not applied as cross-fall: it states no stations")), "{:#?}", f.warnings);
    assert!(f.warnings.iter().any(|w| w.contains("'Alpha Road': superelevation 4 was not applied as cross-fall: its stations are not in order")), "{:#?}", f.warnings);
    // The table can be rebuilt from the file: the engine superelevation that was derived is kept with it.
    assert_eq!(t.derived, alpha.superelevation);
}

#[test]
fn feature_field_note_point_files_and_surface_statistics_are_read_not_skipped() {
    let f = civil();
    assert_eq!(f.field_notes.len(), 1);
    let n = &f.field_notes[0];
    assert_eq!(n.owner.kind, "CoordinateSystem");
    assert_eq!(n.prefix.as_deref(), Some("Transformation"));
    assert_eq!(n.items.len(), 4);
    assert_eq!(f.field_note_value("LocalPoint"), Some("(1000.5 2000.25 0.0)"));
    assert_eq!(f.field_note_value("Scale"), Some("1.0"));
    // The transformation is text: no coordinate was touched.
    assert_eq!(f.alignments[0].elements[0].x, 358000.25);

    let s = &f.surfaces[0];
    assert_eq!(s.source_data.point_files.len(), 1);
    assert_eq!((s.source_data.point_files[0].file_type.as_str(), s.source_data.point_files[0].file_format.as_str()), ("binary", "xyz"));
    assert!(s.source_data.point_files[0].file_name.ends_with("ground points.csv"));
    assert_eq!((s.points.len(), s.faces.len(), s.faces.iter().filter(|f| f.invisible).count()), (13, 14, 2));
    // What Definition states is what the visible faces give.
    let st = s.stated.unwrap();
    assert_eq!((st.area_2d_m2, st.elev_min_m, st.elev_max_m), (Some(600.0), Some(1183.25), Some(1204.0)));
    let tri = |ids: &[u64]| -> [(f64, f64, f64); 3] {
        let p = |i: u64| s.points.iter().find(|p| p.id == i).map(|p| (p.easting_m, p.northing_m, p.elevation_m)).unwrap();
        [p(ids[0]), p(ids[1]), p(ids[2])]
    };
    let visible_2d: f64 = s.faces.iter().filter(|f| !f.invisible).map(|f| { let t = tri(&f.ids); ((t[1].0 - t[0].0) * (t[2].1 - t[0].1) - (t[2].0 - t[0].0) * (t[1].1 - t[0].1)).abs() / 2.0 }).sum();
    assert!((visible_2d - st.area_2d_m2.unwrap()).abs() < 1e-6, "{visible_2d}");
    // One stray point, far from the rest: kept, and said so.
    assert_eq!(s.outlier_points(), [12]);
    assert_eq!((s.points[12].easting_m, s.points[12].northing_m), (312.25, 10766.5));
    assert!(f.warnings.iter().any(|w| w.contains("Surface 'Test OGL': 1 of 13 points lie more than 100 times the median distance")), "{:#?}", f.warnings);
    let all = f.summary.surfaces.extent.unwrap();
    assert_eq!((all.min_easting, all.min_northing), (312.25, 10766.5));
    let core = f.summary.surfaces_core_extent.unwrap();
    assert_eq!((core.min_easting, core.max_easting, core.min_northing, core.max_northing), (358100.5, 358130.5, 9964050.25, 9964070.25));
    assert_eq!(f.summary.counts.get("surface_outlier_points"), Some(&1));
    // The redundant attributes are counted, not read.
    let ig = &f.summary.ignored_attributes;
    assert_eq!(ig.get("LandXML/Alignments/Alignment/CoordGeom/Curve@chord"), Some(&4));
    assert_eq!(ig.get("LandXML/Alignments/Alignment/CoordGeom/Line@dir"), Some(&6));
    assert_eq!(ig.get("LandXML/Surfaces/Surface/Definition/Faces/F@n"), Some(&14));
    assert!(f.warnings.iter().any(|w| w.contains("attribute value(s) of") && w.contains("were not read")), "{:#?}", f.warnings);
}

#[test]
fn text_with_a_byte_order_mark_or_not_utf8_is_read_and_said() {
    let dir = std::env::temp_dir().join(format!("afrobim-landxml-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let xml = "<LandXML><Units><Metric linearUnit=\"meter\"/></Units><CgPoints><CgPoint name=\"Caf\u{e9}\">1.5 2.5 3.5</CgPoint></CgPoints></LandXML>";
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(xml.as_bytes());
    std::fs::write(dir.join("bom.xml"), &bom).unwrap();
    let f = read_landxml_file(&dir.join("bom.xml")).unwrap();
    assert_eq!(f.cg_points[0].points[0].name, "Caf\u{e9}");
    assert!(f.warnings.iter().all(|w| !w.contains("ISO 8859-1")));
    let latin1: Vec<u8> = xml.chars().map(|c| c as u32 as u8).collect();
    std::fs::write(dir.join("latin1.xml"), &latin1).unwrap();
    let f = read_landxml_file(&dir.join("latin1.xml")).unwrap();
    assert_eq!(f.cg_points[0].points[0].name, "Caf\u{e9}");
    assert!(f.warnings[0].contains("not valid UTF-8") && f.warnings[0].contains("ISO 8859-1"), "{:?}", f.warnings);
    let mut utf16 = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        utf16.extend_from_slice(&u.to_le_bytes());
    }
    std::fs::write(dir.join("utf16.xml"), &utf16).unwrap();
    let f = read_landxml_file(&dir.join("utf16.xml")).unwrap();
    assert_eq!(f.cg_points[0].points[0].elevation_m, Some(3.5));
    assert!(f.warnings[0].contains("UTF-16"));
    let _ = std::fs::remove_dir_all(&dir);
}
