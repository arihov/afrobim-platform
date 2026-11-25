// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Tests against a private LandXML file (a real Civil 3D export that is not in this repository and
//! must never be). They skip, passing with a printed note, unless the environment says where it is:
//!
//! - `AFROBIM_PRIVATE_LANDXML`: the file itself. The headline facts are checked against a second,
//!   simple reading of the same file inside this test (names and lengths are never written in the
//!   repository), and the geometry is checked against the points the file states.
//! - `AFROBIM_PRIVATE_LANDXML_EXPECT` (optional): a JSON file of expectations made by an independent
//!   reading in Python (standard library only, outside the repository). When it is set, every
//!   per-alignment, per-profile, per-surface and per-section number is compared with the reader's.
//!
//! Counts are compared exactly; coordinates and levels to 1e-9 m (both sides read the same decimal
//! text); lengths computed from geometry to 1e-6 m.

mod common;

use std::path::PathBuf;
use std::time::Instant;

use afrobim_alignment::{read_landxml, read_landxml_file, LandXmlFile, LandXmlGeomElement, LandXmlProfEntryKind};
use common::{geometry_report, station_round_trip};

fn private_path() -> Option<PathBuf> {
    match std::env::var("AFROBIM_PRIVATE_LANDXML") {
        Ok(p) if PathBuf::from(&p).is_file() => Some(PathBuf::from(p)),
        Ok(p) => {
            println!("AFROBIM_PRIVATE_LANDXML is set to '{p}', which is not a file: private test skipped.");
            None
        }
        Err(_) => {
            println!("AFROBIM_PRIVATE_LANDXML is not set: private test skipped.");
            None
        }
    }
}

fn expectations() -> Option<serde_json::Value> {
    let p = std::env::var("AFROBIM_PRIVATE_LANDXML_EXPECT").ok()?;
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("AFROBIM_PRIVATE_LANDXML_EXPECT '{p}': {e}"));
    Some(serde_json::from_str(&text).expect("the expectations are JSON"))
}

fn peak_memory_mb() -> Option<f64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb: f64 = s.lines().find(|l| l.starts_with("VmHWM:"))?.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0)
}

fn read() -> Option<(PathBuf, LandXmlFile)> {
    let path = private_path()?;
    let t = Instant::now();
    let f = read_landxml_file(&path).expect("the file reads");
    println!("read_landxml_file: {:?}; peak memory of this process {:.0} MB", t.elapsed(), peak_memory_mb().unwrap_or(f64::NAN));
    Some((path, f))
}

/// A second, deliberately simple reading of the headline facts: roxmltree only, no reader code.
struct Simple {
    alignments: Vec<(String, f64)>,
    surfaces: usize,
    prof_align: usize,
    prof_surf: usize,
    cross_sects: usize,
    epsg: Option<String>,
}

fn simple_reading(path: &std::path::Path) -> Simple {
    let text = std::fs::read_to_string(path).unwrap();
    let doc = roxmltree::Document::parse(&text).unwrap();
    let count = |name: &str| doc.descendants().filter(|n| n.is_element() && n.tag_name().name() == name).count();
    Simple {
        alignments: doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "Alignment")
            .map(|n| (n.attribute("name").unwrap().to_string(), n.attribute("length").unwrap().parse().unwrap()))
            .collect(),
        surfaces: count("Surface"),
        prof_align: count("ProfAlign"),
        prof_surf: count("ProfSurf"),
        cross_sects: count("CrossSect"),
        epsg: doc.descendants().find(|n| n.is_element() && n.tag_name().name() == "CoordinateSystem").and_then(|n| n.attribute("epsgCode")).map(str::to_string),
    }
}

#[test]
fn the_headline_facts_of_the_private_file_hold() {
    let Some((path, f)) = read() else { return };
    let s = simple_reading(&path);
    assert_eq!(f.alignments.len(), 17);
    assert_eq!(f.alignments.len(), s.alignments.len());
    for (a, (name, length)) in f.alignments.iter().zip(&s.alignments) {
        assert_eq!(&a.name, name);
        assert!((a.length() - length).abs() < 1e-6, "{}: {} against {length}", a.name, a.length());
    }
    assert_eq!(f.alignment_names(), s.alignments.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>());
    assert_eq!((f.surfaces.len(), s.surfaces), (4, 4));
    assert_eq!((f.design_profiles.len(), s.prof_align), (17, 17));
    assert_eq!((f.ground_profiles.len(), s.prof_surf), (17, 17));
    assert_eq!((f.cross_sections.len(), s.cross_sects), (1055, 1055));
    assert_eq!(f.coordinate_system.as_ref().and_then(|c| c.epsg_code.as_deref()), Some("21036"));
    assert_eq!(s.epsg.as_deref(), Some("21036"));
    assert_eq!(f.units.linear_unit, "meter");
    assert!(f.is_civil_3d());
    // 128 clothoids among the elements.
    let spirals = f.alignment_infos.iter().flat_map(|i| i.coord_geoms.iter()).flat_map(|g| g.elements.iter()).filter(|e| matches!(e, LandXmlGeomElement::Spiral { .. })).count();
    assert_eq!(spirals, 128);
    // Nothing the file holds is skipped as an element; the engine alignments are all built.
    assert!(f.summary.skipped.is_empty(), "{:?}", f.summary.skipped);
    assert!(f.alignment_infos.iter().all(|i| i.imported));
    assert_eq!(f.summary.alignments.count, 17);
    // The old alignment-only reader agrees on the alignments.
    let old = read_landxml(&path).unwrap();
    assert_eq!(old.alignments.len(), 17);
    for (a, b) in old.alignments.iter().zip(&f.alignments) {
        assert_eq!(a.elements.len(), b.elements.len());
        assert_eq!(a.length(), b.length());
    }
}

#[test]
fn the_geometry_of_the_private_file_agrees_with_the_points_it_states() {
    let Some((_, f)) = read() else { return };
    let r = geometry_report(&f);
    println!("elements: {} lines, {} curves, {} spirals; not compared (polylines or dropped elements): {:?}", r.lines, r.curves, r.spirals, r.not_compared);
    println!("worst join of stated elements       {:.3e} m  (alignment index {:?})", r.join.value, r.join.alignment);
    println!("worst curve radius difference       {:.3e} m  (alignment index {:?})", r.radius.value, r.radius.alignment);
    println!("worst alignment length difference   {:.3e} m  (alignment index {:?})", r.length.value, r.length.alignment);
    println!("worst element length difference     {:.3e} m  (alignment index {:?})", r.element_length.value, r.element_length.alignment);
    println!("worst spiral end, independent       {:.3e} m  (alignment index {:?})", r.spiral_end_independent.value, r.spiral_end_independent.alignment);
    println!("worst spiral end, engine element    {:.3e} m  (alignment index {:?})", r.spiral_end_engine.value, r.spiral_end_engine.alignment);
    println!("worst element end, engine           {:.3e} m  (alignment index {:?})", r.element_end_engine.value, r.element_end_engine.alignment);
    println!("worst element start, engine         {:.3e} m  (alignment index {:?})", r.element_start_engine.value, r.element_start_engine.alignment);
    println!("worst start heading                 {:.3e} rad (alignment index {:?})", r.start_heading.value, r.start_heading.alignment);
    println!("joints that are not tangent: {:?}", r.kinks);
    assert_eq!((r.lines, r.curves, r.spirals), (103, 85, 128));
    assert!(r.not_compared.is_empty(), "{:?}", r.not_compared);
    assert!(r.join.value < 1e-3, "{:?}", r.join);
    assert!(r.radius.value < 1e-6, "{:?}", r.radius);
    assert!(r.length.value < 1e-6, "{:?}", r.length);
    assert!(r.element_length.value < 1e-6, "{:?}", r.element_length);
    assert!(r.spiral_end_independent.value < 1e-6, "{:?}", r.spiral_end_independent);
    assert!(r.spiral_end_engine.value < 1e-6, "{:?}", r.spiral_end_engine);
    assert!(r.element_end_engine.value < 1e-6 && r.element_start_engine.value == 0.0, "{r:?}");
    assert!(r.start_heading.value < 1e-8, "{:?}", r.start_heading);
    // Station to point and point to station along every alignment, every 5 m and at both ends.
    let (mut ws, mut wo, mut n) = (0.0f64, 0.0f64, 0usize);
    for (i, a) in f.alignments.iter().enumerate() {
        let (ds, off, k) = station_round_trip(a, 5.0);
        println!("alignment index {i:2}: {k:5} stations, worst station difference {ds:.2e} m, worst offset {off:.2e} m");
        ws = ws.max(ds);
        wo = wo.max(off);
        n += k;
    }
    println!("round trip: {n} stations, worst station difference {ws:.3e} m, worst offset {wo:.3e} m");
    assert!(ws < 1e-6 && wo < 1e-6, "{ws} {wo}");
    // Where the file turns a corner without a curve, the reader says so.
    for &(ai, _, _) in &r.kinks {
        let name = &f.alignments[ai].name;
        assert!(f.warnings.iter().any(|w| w.contains(&format!("Alignment '{name}': at station"))), "{name}: {:#?}", f.warnings);
    }
}

fn close(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() <= 1e-9, "{what}: {a} against {b}");
}

/// A number from the expectations. The independent reading writes them as text (serde_json's
/// default float parser is not correctly rounded, and these are compared to 1e-9).
fn num(v: &serde_json::Value) -> f64 {
    match v.as_str() {
        Some(t) => t.parse().unwrap_or_else(|_| panic!("not a number: {t}")),
        None => v.as_f64().unwrap_or_else(|| panic!("not a number: {v}")),
    }
}

fn opt_num(v: &serde_json::Value) -> Option<f64> {
    if v.is_null() {
        None
    } else {
        Some(num(v))
    }
}

#[test]
fn the_reader_agrees_with_the_independent_python_reading() {
    let Some((_, f)) = read() else { return };
    let Some(e) = expectations() else {
        println!("AFROBIM_PRIVATE_LANDXML_EXPECT is not set: the comparison with the independent reading is skipped.");
        return;
    };
    let als = e["alignments"].as_array().unwrap();
    assert_eq!(f.alignments.len(), als.len());
    println!("idx |  sta0 |   length attr |  engine len | L/C/S |  PVI/Para | ProfSurf pts | xsects | sta range | super | features");
    let mut worst_sample = 0.0f64;
    let mut worst_len = 0.0f64;
    for (i, x) in als.iter().enumerate() {
        let a = &f.alignments[i];
        assert_eq!(a.name, x["name"].as_str().unwrap());
        let info = f.alignment_info(&a.name).unwrap();
        assert_eq!(info.desc.as_deref(), x["desc"].as_str());
        close(info.sta_start_m.unwrap(), num(&x["sta_start"]), "staStart");
        close(a.start_station_m, num(&x["sta_start"]), "engine staStart");
        close(info.declared_length_m.unwrap(), num(&x["length_attr"]), "length attribute");
        // Element counts and the ends.
        let raw: Vec<&LandXmlGeomElement> = info.coord_geoms.iter().flat_map(|g| g.elements.iter()).collect();
        let lcs = (
            raw.iter().filter(|e| matches!(e, LandXmlGeomElement::Line { .. })).count() as u64,
            raw.iter().filter(|e| matches!(e, LandXmlGeomElement::Curve { .. })).count() as u64,
            raw.iter().filter(|e| matches!(e, LandXmlGeomElement::Spiral { .. })).count() as u64,
        );
        assert_eq!(lcs, (x["n_line"].as_u64().unwrap(), x["n_curve"].as_u64().unwrap(), x["n_spiral"].as_u64().unwrap()), "element counts, alignment index {i}");
        let start = match raw[0] { LandXmlGeomElement::Line { start, .. } | LandXmlGeomElement::Curve { start, .. } | LandXmlGeomElement::Spiral { start, .. } => *start, _ => panic!() };
        let end = match raw[raw.len() - 1] { LandXmlGeomElement::Line { end, .. } | LandXmlGeomElement::Curve { end, .. } | LandXmlGeomElement::Spiral { end, .. } => *end, _ => panic!() };
        close(start.x, num(&x["start"][0]), "start easting");
        close(start.y, num(&x["start"][1]), "start northing");
        close(end.x, num(&x["end"][0]), "end easting");
        close(end.y, num(&x["end"][1]), "end northing");
        close(a.elements[0].x, num(&x["start"][0]), "engine start easting");
        close(a.elements[0].y, num(&x["start"][1]), "engine start northing");
        let (ex, ey, _) = a.elements[a.elements.len() - 1].end();
        assert!((ex - num(&x["end"][0])).hypot(ey - num(&x["end"][1])) < 1e-6, "engine end, alignment index {i}");
        // Lengths from geometry.
        let dl = (a.length() - num(&x["geom_length"])).abs();
        worst_len = worst_len.max(dl);
        assert!(dl < 1e-6, "length, alignment index {i}: {dl}");
        assert!((a.length() - num(&x["sum_declared"])).abs() < 1e-6, "sum of stated element lengths, alignment index {i}");
        // Points along the alignment, computed independently.
        for s in x["samples"].as_array().unwrap() {
            let p = a.point_at(a.start_station_m + num(&s[0])).unwrap();
            let miss = (p.x - num(&s[1])).hypot(p.y - num(&s[2]));
            worst_sample = worst_sample.max(miss);
            assert!(miss < 1e-6, "alignment index {i}, distance {}: {miss}", num(&s[0]));
        }

        // Design profile as written.
        let dp = f.design_profile(&a.name).unwrap();
        let p = &x["profile"];
        assert_eq!(dp.name, p["name"].as_str().unwrap());
        let entries = p["entries"].as_array().unwrap();
        assert_eq!(dp.entries.len(), entries.len());
        let c = dp.counts();
        assert_eq!((c[0] as u64, c[1] as u64), (p["n_pvi"].as_u64().unwrap(), p["n_para"].as_u64().unwrap()));
        for (de, xe) in dp.entries.iter().zip(entries) {
            let kind = match xe[0].as_str().unwrap() { "PVI" => LandXmlProfEntryKind::Pvi, "ParaCurve" => LandXmlProfEntryKind::ParaCurve, k => panic!("{k}") };
            assert_eq!(de.kind, kind);
            close(de.station_m, num(&xe[1]), "PVI station");
            close(de.elevation_m, num(&xe[2]), "PVI level");
            match opt_num(&xe[3]) { Some(l) => close(de.length_m.unwrap(), l, "curve length"), None => assert!(de.length_m.is_none()) }
        }
        let first = (dp.entries[0].station_m, dp.entries[0].elevation_m);
        let last = (dp.entries[dp.entries.len() - 1].station_m, dp.entries[dp.entries.len() - 1].elevation_m);
        close(first.0, num(&entries[0][1]), "first PVI station");
        close(last.1, num(&entries[entries.len() - 1][2]), "last PVI level");
        // Existing ground.
        let gp = f.ground_profile(&a.name).unwrap();
        let q = &x["prof_surf"];
        assert_eq!((gp.name.as_str(), gp.state.as_deref(), gp.profile.as_deref()), (q["name"].as_str().unwrap(), q["state"].as_str(), q["profile_name"].as_str()));
        assert_eq!((gp.point_lists.len() as u64, gp.points().len() as u64), (q["n_lists"].as_u64().unwrap(), q["n_points"].as_u64().unwrap()));
        close(gp.points()[0][0], num(&q["first"][0]), "ProfSurf first station");
        close(gp.points()[0][1], num(&q["first"][1]), "ProfSurf first level");
        let l = gp.points()[gp.points().len() - 1];
        close(l[0], num(&q["last"][0]), "ProfSurf last station");
        close(l[1], num(&q["last"][1]), "ProfSurf last level");
        // Cross-sections.
        let xs = f.cross_sections_of(&a.name);
        let c = &x["cross_sects"];
        let want = c["count"].as_u64().unwrap_or(0);
        assert_eq!(xs.len() as u64, want, "cross-section count, alignment index {i}");
        if want > 0 {
            let stas: Vec<f64> = xs.iter().map(|s| s.station_m).collect();
            close(stas.iter().cloned().fold(f64::INFINITY, f64::min), num(&c["sta_min"]), "first section station");
            close(stas.iter().cloned().fold(f64::NEG_INFINITY, f64::max), num(&c["sta_max"]), "last section station");
            assert_eq!(xs.iter().map(|s| s.surfaces.len() as u64).sum::<u64>(), c["n_surf"].as_u64().unwrap());
            assert_eq!(xs.iter().flat_map(|s| s.surfaces.iter()).flat_map(|s| s.point_lists.iter()).map(|l| l.len() as u64).sum::<u64>(), c["n_points"].as_u64().unwrap());
            let mut names: Vec<&str> = xs.iter().flat_map(|s| s.surfaces.iter()).map(|s| s.name.as_str()).collect();
            names.sort();
            names.dedup();
            let want_names: Vec<&str> = c["surf_names"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(names, want_names);
            assert_eq!(xs[0].name.as_deref(), c["first_name"].as_str());
        }
        // Superelevation as the full table.
        let table = f.superelevation_table(&a.name);
        let want = x["superelevation"].as_array().unwrap();
        assert_eq!(table.map_or(0, |t| t.records.len()), want.len(), "superelevation records, alignment index {i}");
        for (rec, w) in table.iter().flat_map(|t| t.records.iter()).zip(want) {
            close(rec.sta_start_m.unwrap(), num(&w["sta_start"]), "staStart");
            close(rec.sta_end_m.unwrap(), num(&w["sta_end"]), "staEnd");
            let mut stations = Vec::new();
            let (mut rates, mut adverse) = (Vec::new(), Vec::new());
            for item in w["items"].as_array().unwrap() {
                let (tag, text) = (item[0].as_str().unwrap(), item[1].as_str().unwrap());
                match tag {
                    "FullSuperelev" => rates.push(text.parse::<f64>().unwrap()),
                    "AdverseSE" => adverse.push(text.to_string()),
                    _ => stations.push((tag.to_string(), text.parse::<f64>().unwrap())),
                }
            }
            let got: Vec<(String, f64)> = rec.stations.iter().map(|s| (afrobim_alignment::critical_station_element(s.kind).to_string(), s.station_m)).collect();
            assert_eq!(got, stations, "critical stations in file order, alignment index {i}");
            assert_eq!(rec.full_super_rates_pct, rates);
            assert_eq!(rec.adverse, adverse);
        }
        // Features.
        let feats = f.features_of("Alignment", &a.name);
        let want = x["features"].as_array().unwrap();
        assert_eq!(feats.len(), want.len());
        for (g, w) in feats.iter().zip(want) {
            assert_eq!((g.name.as_deref(), g.code.as_deref(), g.source.as_deref()), (w["name"].as_str(), w["code"].as_str(), w["source"].as_str()));
            let props: Vec<(&str, &str)> = g.properties.iter().map(|p| (p.label.as_str(), p.value.as_str())).collect();
            let wp: Vec<(&str, &str)> = w["props"].as_array().unwrap().iter().map(|p| (p[0].as_str().unwrap(), p[1].as_str().unwrap())).collect();
            assert_eq!(props, wp);
        }
        println!(
            "{i:3} | {:5.1} | {:13.6} | {:11.6} | {}/{}/{} | {:4}/{:<4} | {:12} | {:6} | {:9} | {:5} | {:4}",
            num(&x["sta_start"]),
            num(&x["length_attr"]),
            a.length(),
            lcs.0,
            lcs.1,
            lcs.2,
            c_pvi(dp),
            dp.counts()[1],
            gp.points().len(),
            xs.len(),
            if xs.is_empty() { "-".to_string() } else { format!("{:.0}..{:.0}", xs[0].station_m, xs[xs.len() - 1].station_m) },
            table.map_or(0, |t| t.records.len()),
            feats.len()
        );
    }
    println!("worst length difference against the independent geometry {worst_len:.3e} m; worst sampled point {worst_sample:.3e} m");
    assert_eq!(e["totals"]["alignments"].as_u64().unwrap(), 17);

    // Surfaces.
    let surfs = e["surfaces"].as_array().unwrap();
    assert_eq!(f.surfaces.len(), surfs.len());
    println!("surface | points | faces | invisible | easting min..max | northing min..max | level min..max | stated 2D area | visible 2D area");
    for (s, x) in f.surfaces.iter().zip(surfs) {
        assert_eq!(s.name, x["name"].as_str().unwrap());
        assert_eq!(s.desc.as_deref(), x["desc"].as_str());
        assert_eq!(s.surf_type.as_deref(), x["surf_type"].as_str());
        assert_eq!((s.points.len() as u64, s.faces.len() as u64, s.faces.iter().filter(|f| f.invisible).count() as u64), (x["n_points"].as_u64().unwrap(), x["n_faces"].as_u64().unwrap(), x["n_invisible"].as_u64().unwrap()));
        let fold = |f: fn(&afrobim_alignment::LandXmlSurfPoint) -> f64, min: bool| s.points.iter().map(f).fold(if min { f64::INFINITY } else { f64::NEG_INFINITY }, |a, b| if min { a.min(b) } else { a.max(b) });
        close(fold(|p| p.easting_m, true), num(&x["e_min"]), "easting min");
        close(fold(|p| p.easting_m, false), num(&x["e_max"]), "easting max");
        close(fold(|p| p.northing_m, true), num(&x["n_min"]), "northing min");
        close(fold(|p| p.northing_m, false), num(&x["n_max"]), "northing max");
        close(fold(|p| p.elevation_m, true), num(&x["z_min"]), "level min");
        close(fold(|p| p.elevation_m, false), num(&x["z_max"]), "level max");
        let st = s.stated.unwrap();
        let sx = &x["stated"];
        close(st.area_2d_m2.unwrap(), sx["area2DSurf"].as_str().unwrap().parse().unwrap(), "stated 2D area");
        close(st.area_3d_m2.unwrap(), sx["area3DSurf"].as_str().unwrap().parse().unwrap(), "stated 3D area");
        close(st.elev_min_m.unwrap(), sx["elevMin"].as_str().unwrap().parse().unwrap(), "stated min level");
        close(st.elev_max_m.unwrap(), sx["elevMax"].as_str().unwrap().parse().unwrap(), "stated max level");
        // The stated levels are those of the points; the stated areas are those of the visible faces.
        close(st.elev_min_m.unwrap(), fold(|p| p.elevation_m, true), "stated min level against the points");
        close(st.elev_max_m.unwrap(), fold(|p| p.elevation_m, false), "stated max level against the points");
        let by_id: std::collections::HashMap<u64, &afrobim_alignment::LandXmlSurfPoint> = s.points.iter().map(|p| (p.id, p)).collect();
        let visible_2d: f64 = s
            .faces
            .iter()
            .filter(|f| !f.invisible)
            .map(|f| {
                let t: Vec<_> = f.ids.iter().map(|i| by_id[i]).collect();
                ((t[1].easting_m - t[0].easting_m) * (t[2].northing_m - t[0].northing_m) - (t[2].easting_m - t[0].easting_m) * (t[1].northing_m - t[0].northing_m)).abs() / 2.0
            })
            .sum();
        println!(
            "{:7} | {:6} | {:5} | {:9} | {:.1}..{:.1} | {:.1}..{:.1} | {:.3}..{:.3} | {:.6} | {visible_2d:.6}",
            s.name.len(),
            s.points.len(),
            s.faces.len(),
            s.faces.iter().filter(|f| f.invisible).count(),
            fold(|p| p.easting_m, true),
            fold(|p| p.easting_m, false),
            fold(|p| p.northing_m, true),
            fold(|p| p.northing_m, false),
            fold(|p| p.elevation_m, true),
            fold(|p| p.elevation_m, false),
            st.area_2d_m2.unwrap()
        );
        assert!((visible_2d - st.area_2d_m2.unwrap()).abs() < 1e-3, "the visible faces give {visible_2d} m2, the file states {} m2", st.area_2d_m2.unwrap());
        let files: Vec<&str> = x["point_files"].as_array().unwrap().iter().map(|p| p["fileName"].as_str().unwrap()).collect();
        assert_eq!(s.source_data.point_files.iter().map(|p| p.file_name.as_str()).collect::<Vec<_>>(), files);
        assert_eq!(s.source_data.point_files.iter().map(|p| p.file_format.as_str()).collect::<Vec<_>>(), x["point_files"].as_array().unwrap().iter().map(|p| p["fileFormat"].as_str().unwrap()).collect::<Vec<_>>());
    }
    assert_eq!(f.summary.counts.get("surface_points"), Some(&(surfs.iter().map(|s| s["n_points"].as_u64().unwrap()).sum::<u64>() as usize)));
    assert_eq!(f.summary.counts.get("surface_faces"), Some(&(surfs.iter().map(|s| s["n_faces"].as_u64().unwrap()).sum::<u64>() as usize)));
    let t = &e["totals"];
    assert_eq!(f.cross_sections.len() as u64, t["cross_sects"].as_u64().unwrap());
    assert_eq!(f.cross_sections.iter().map(|c| c.surfaces.len() as u64).sum::<u64>(), t["cross_sect_surf"].as_u64().unwrap());
    assert_eq!(f.superelevation_tables.iter().map(|t| t.records.len() as u64).sum::<u64>(), t["superelevation"].as_u64().unwrap());
    assert_eq!(f.object_features.iter().map(|o| o.features.len() as u64).sum::<u64>(), t["features"].as_u64().unwrap());
    let h = &e["header"];
    assert_eq!(f.header.version.as_deref(), h["version"].as_str());
    assert_eq!(f.coordinate_system.as_ref().unwrap().epsg_code.as_deref(), h["epsg"].as_str());
    assert_eq!(f.project.as_ref().map(|p| p.name.as_str()), h["project"].as_str());
    assert_eq!(f.applications[0].name, h["app"].as_str().unwrap());
    assert_eq!((f.units.angular_unit.as_str(), f.units.direction_unit.as_str()), (h["angular"].as_str().unwrap(), h["direction"].as_str().unwrap()));
}

fn c_pvi(d: &afrobim_alignment::LandXmlDesignProfile) -> usize {
    d.counts()[0]
}

#[test]
fn superelevation_of_the_private_file_is_in_the_table_and_its_sign_is_the_curves() {
    let Some((_, f)) = read() else { return };
    let records: Vec<_> = f.superelevation_tables.iter().flat_map(|t| t.records.iter()).collect();
    let with_rate = records.iter().filter(|r| r.rate_pct().is_some()).count();
    let empty = records.iter().filter(|r| r.is_empty()).count();
    let descending = records.iter().filter(|r| !r.is_empty() && !r.stations_ascending()).count();
    println!("{} records: {with_rate} with one rate, {empty} empty, {descending} with stations in descending order", records.len());
    assert_eq!(records.len(), 81);
    assert_eq!(with_rate + empty, 81);
    assert_eq!(empty, 15);
    // Every record that has a rate and a curve under it: the sign is the hand of the curve.
    let mut agree = 0;
    for r in records.iter().filter(|r| r.rate_pct().is_some()) {
        if let Some(turn) = r.turn {
            assert_eq!(r.rate_pct().unwrap() > 0.0, turn == afrobim_alignment::Turn::Right);
            agree += 1;
        }
    }
    assert_eq!(agree, with_rate);
    assert!(f.warnings.iter().any(|w| w.contains("is positive on") && w.contains("with no exception")));
    // Applied as cross-fall wherever the stations allow; the rest is named in the warnings.
    let applied = f.alignments.iter().filter_map(|a| a.superelevation.as_ref()).map(|s| s.sections.len()).sum::<usize>();
    println!("sections applied as cross-fall: {applied} of {with_rate} with a rate");
    assert!(applied >= with_rate - 2, "{applied}");
}

#[test]
fn what_the_private_file_leaves_unread_is_counted_and_named() {
    let Some((_, f)) = read() else { return };
    println!("warnings: {}", f.warnings.len());
    for w in f.warnings.iter().filter(|w| !w.contains("superelevation ")) {
        println!("  {}", &w[..w.len().min(220)]);
    }
    println!("ignored attributes: {:?}", f.summary.ignored_attributes);
    println!("counts: {:?}", f.summary.counts);
    // Two design profiles have a vertical step: the engine profile is absent, the design profile is whole.
    let steps: Vec<_> = f.design_profiles.iter().filter(|d| !d.steps().is_empty()).collect();
    assert_eq!(steps.len(), 2);
    for d in steps {
        assert!(f.alignment(&d.alignment).unwrap().profile.is_none());
        let parts = d.segments(); assert!(parts.is_ok(), "{}: {:?}", d.name, parts.err());
        assert!(f.warnings.iter().any(|w| w.contains(&format!("profile '{}' was not imported", d.name)) && w.contains("steps the level")));
    }
    assert_eq!(f.alignments.iter().filter(|a| a.profile.is_some()).count(), 15);
    // Features: one per alignment, 22 in all; the transformation of the drawing is a field note.
    assert_eq!(f.object_features.iter().map(|o| o.features.len()).sum::<usize>(), 22);
    assert_eq!(f.field_notes.len(), 1);
    assert!(f.field_note_value("LocalPoint").is_some());
    // Surface sources are named; one surface has points far from the rest.
    assert_eq!(f.surfaces.iter().map(|s| s.source_data.point_files.len()).sum::<usize>(), 4);
    assert_eq!(f.surfaces.iter().map(|s| s.outlier_points().len()).collect::<Vec<_>>().iter().filter(|n| **n > 0).count(), 1);
    assert!(f.summary.surfaces_core_extent.unwrap().min_northing > 9.0e6);
    assert!(f.summary.surfaces.extent.unwrap().min_northing < 1.0e5);
    // The Civil 3D alignments carry no station equations; the stationing is continuous from 0.
    assert!(f.alignments.iter().all(|a| a.station_equations.is_empty()));
}
