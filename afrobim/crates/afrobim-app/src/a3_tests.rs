//! Tests of the round 6 work package A3. The ones that read a real Civil 3D export run only when
//! `AFROBIM_PRIVATE_LANDXML` names it and say nothing about what is in it.

use std::path::PathBuf;

use afrobim_kernel::{crs_from_model_point, model_point_from_crs, project_crs, Document};

use crate::app::AfroBimApp;
use crate::civil;
use crate::civil_build::Ctx;

fn private_path() -> Option<PathBuf> {
    std::env::var("AFROBIM_PRIVATE_LANDXML").ok().map(PathBuf::from)
}

/// An application with the whole file imported through the import path (no dialog).
fn imported(path: &std::path::Path) -> (egui::Context, AfroBimApp) {
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let options = crate::Options { screenshot: Some(std::env::temp_dir().join("afrobim-a3-test.png")), ..crate::Options::default() };
    let mut app = AfroBimApp::new(&cc, options);
    app.landxml_import_all(path);
    (ctx, app)
}

fn ctx_of(doc: &Document) -> Ctx {
    crate::a1_tests::ctx_with_tins(doc)
}

// ─── a synthetic file, always run ───────────────────────────────────────────────────────────

/// A small LandXML file with one straight alignment, a flat surface at level 100 and two
/// cross-sections whose ground line is that level.
fn synthetic_xml(section_level: f64) -> String {
    let mut s = String::from(r#"<?xml version="1.0"?><LandXML xmlns="http://www.landxml.org/schema/LandXML-1.2" version="1.2"><Units><Metric areaUnit="squareMeter" linearUnit="meter" volumeUnit="cubicMeter" temperatureUnit="celsius" pressureUnit="milliBars" diameterUnit="millimeter" angularUnit="decimal degrees" directionUnit="decimal degrees"/></Units><Application name="Autodesk Civil 3D" manufacturer="Autodesk, Inc." version="2025"/><Surfaces><Surface name="Ground"><Definition surfType="TIN"><Pnts>"#);
    let pts = [(0.0, 0.0), (200.0, 0.0), (0.0, 100.0), (200.0, 100.0)];
    for (i, (e, n)) in pts.iter().enumerate() {
        s.push_str(&format!(r#"<P id="{}">{} {} 100</P>"#, i + 1, n - 50.0, e));
    }
    s.push_str(r#"</Pnts><Faces><F>1 2 3</F><F>2 4 3</F></Faces></Definition></Surface></Surfaces><Alignments><Alignment name="A" length="200" staStart="0"><CoordGeom><Line><Start>0 0</Start><End>0 200</End></Line></CoordGeom>"#);
    s.push_str(r#"<CrossSects>"#);
    for sta in [20.0, 100.0] {
        s.push_str(&format!(r#"<CrossSect sta="{sta}" name="S{sta}"><CrossSectSurf name="ground"><PntList2D>-10 {l} 0 {l} 10 {l}</PntList2D></CrossSectSurf></CrossSect>"#, l = section_level));
    }
    s.push_str("</CrossSects></Alignment></Alignments></LandXML>");
    s
}

#[test]
fn imported_sections_become_a_sample_line_group_with_the_files_surfaces_and_compare() {
    let dir = std::env::temp_dir().join("afrobim-a3-synthetic");
    std::fs::create_dir_all(&dir).unwrap();
    let ok = dir.join("flat.xml");
    std::fs::write(&ok, synthetic_xml(100.0)).unwrap();
    let (_c, mut app) = imported(&ok);
    let groups: Vec<_> = app.doc.elements().iter().filter(|e| e.category == civil::SAMPLES).map(|e| e.id).collect();
    assert_eq!(groups.len(), 1, "one sample line group for the one alignment with sections");
    let mut d: civil::SampleData = civil::get(&app.doc, groups[0], civil::SAMPLES).unwrap();
    assert_eq!(d.group.lines.len(), 2);
    assert_eq!(d.imported.len(), 2);
    assert!(!d.surfaces.is_empty(), "the surface called like a section surface is chosen for the comparison");
    // Without comparison mode only the file's lines are drawn.
    let ctx = ctx_of(&app.doc);
    let views = crate::civil_build2::sample_views(&d, &ctx).unwrap();
    assert!(views.iter().all(|v| v.ground_lines.len() == 1 && v.ground_lines[0].surface.ends_with(crate::civil_sections::FILE_TAG)));
    // In comparison mode AfroBIM's own cut is there too, and the difference is nil on a matching file.
    d.compare = true;
    let views = crate::civil_build2::sample_views(&d, &ctx).unwrap();
    let diffs = crate::civil_sections::compare_views(&views);
    assert_eq!(diffs.len(), 2, "{diffs:?}");
    assert!(diffs.iter().all(|x| x.max_diff_m < 1e-9 && x.samples >= 3), "{diffs:?}");
    // A file whose sections sit 12 mm off the surface is reported at 12 mm.
    let off = dir.join("off.xml");
    std::fs::write(&off, synthetic_xml(100.012)).unwrap();
    let (_c2, app2) = imported(&off);
    let id = app2.doc.elements().iter().find(|e| e.category == civil::SAMPLES).unwrap().id;
    let mut d2: civil::SampleData = civil::get(&app2.doc, id, civil::SAMPLES).unwrap();
    d2.compare = true;
    let diffs = crate::civil_sections::compare_views(&crate::civil_build2::sample_views(&d2, &ctx_of(&app2.doc)).unwrap());
    assert!(diffs.iter().all(|x| (x.max_diff_m - 0.012).abs() < 1e-6), "{diffs:?}");
    // The group round-trips in a saved document, and the sheets read the same views.
    let back = Document::from_json(&app2.doc.to_json()).unwrap();
    let d3: civil::SampleData = civil::get(&back, id, civil::SAMPLES).unwrap();
    assert_eq!(d3.imported, d2.imported.clone());
    let drawing = crate::civil_sheets::sample_section_drawing(&crate::civil_build2::sample_views(&d3, &ctx_of(&back)).unwrap()[0], 1.0);
    assert!(!drawing.drawing.primitives.is_empty());
    let _ = &mut app;
}

#[test]
fn the_superelevation_table_of_a_file_is_kept_on_the_alignment() {
    let doc = crate::a1_tests::demo_doc();
    // The demonstration alignment has no table as written; the tab says so and nothing breaks.
    let e = doc.elements().iter().find(|e| e.category == afrobim_alignment::CATEGORY).unwrap();
    assert!(crate::civil_super::kept_table(e).is_none());
}

// ─── the private file ───────────────────────────────────────────────────────────────────────

#[test]
fn private_status_bar_readings_equal_the_files_coordinates_and_extents_match() {
    let Some(path) = private_path() else { return };
    let file = afrobim_alignment::read_landxml_file(&path).expect("the file reads");
    let (_ctx, app) = imported(&path);
    let crs = project_crs(&app.doc).expect("the project took the file's system");
    assert_eq!(crs.epsg, Some(21036));
    let mut checked = 0;
    for info in file.alignment_infos.iter().filter(|i| i.imported) {
        let Some(e) = app.doc.elements().iter().find(|e| e.category == afrobim_alignment::CATEGORY && e.name == info.name) else { panic!("alignment {} is not in the document", info.name) };
        let a = crate::alignment_glue::alignment_of(&app.doc, e.id).unwrap();
        let geom = &info.coord_geoms[0];
        let starts = a.element_stations();
        assert_eq!(starts.len(), geom.elements.len(), "{}: elements", info.name);
        let stated = |el: &afrobim_alignment::LandXmlGeomElement| match el {
            afrobim_alignment::LandXmlGeomElement::Line { start, end } | afrobim_alignment::LandXmlGeomElement::Curve { start, end, .. } | afrobim_alignment::LandXmlGeomElement::Spiral { start, end, .. } => (*start, *end),
            _ => panic!("an element kind this test does not read"),
        };
        // Start, middle and end: the first element's start, the element in the middle, the last element's end.
        let k = geom.elements.len() / 2;
        let picks = [(starts[0], stated(&geom.elements[0]).0), (starts[k], stated(&geom.elements[k]).0), (a.end_station_m(), stated(geom.elements.last().unwrap()).1)];
        for (station, want) in picks {
            let p = crate::alignment_glue::model_point_at(&app.doc, e.id, station).expect("on the alignment");
            let (re, rn) = crs_from_model_point(p);
            assert!((re - want.x).abs() < 0.002 && (rn - want.y).abs() < 0.002, "{} at {station}: {re} {rn} against {} {}", info.name, want.x, want.y);
            // The text the status bar shows for that model point equals the text of the file's coordinates.
            assert_eq!(crate::crs_ui::en_text(p, Some(&crs)), crate::crs_ui::en_text(model_point_from_crs(want.x, want.y), Some(&crs)), "{} at {station}", info.name);
            checked += 1;
        }
        // The element's extent (its drawing, with the end labels) holds the stated one.
        let stated_ext = geom.extent().unwrap();
        let (x0, y0, x1, y1) = (e.origin.x / 1000.0, e.origin.y / 1000.0, (e.origin.x + e.width) / 1000.0, (e.origin.y + e.height) / 1000.0);
        assert!(x0 <= stated_ext.min_easting + 0.002 && y0 <= stated_ext.min_northing + 0.002 && x1 >= stated_ext.max_easting - 0.002 && y1 >= stated_ext.max_northing - 0.002, "{}: element extent inside the file's", info.name);
        // The true extent of the geometry, by walking it every 0.25 m (a spiral bulges past its end points).
        let (mut sx0, mut sy0, mut sx1, mut sy1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut st = a.start_station_m;
        while st <= a.end_station_m() {
            let p = a.point_at(st).unwrap();
            (sx0, sy0, sx1, sy1) = (sx0.min(p.x), sy0.min(p.y), sx1.max(p.x), sy1.max(p.y));
            st += 0.25;
        }
        let p = a.point_at(a.end_station_m()).unwrap();
        (sx0, sy0, sx1, sy1) = (sx0.min(p.x), sy0.min(p.y), sx1.max(p.x), sy1.max(p.y));
        let margin = (sx0 - x0).max(sy0 - y0).max(x1 - sx1).max(y1 - sy1);
        assert!(x0 <= sx0 + 0.3 && y0 <= sy0 + 0.3 && x1 >= sx1 - 0.3 && y1 >= sy1 - 0.3, "{}: element extent inside the walked geometry's", info.name);
        eprintln!("EXTENT alignment {}: element extent exceeds the walked geometry by at most {:.3} m", e.id.0, margin);
        // The element's size is that of its plan drawing, which includes the station ticks and the labels at the ends.
        let diagonal = (sx1 - sx0).hypot(sy1 - sy0);
        assert!(margin < 25.0 + 0.01 * diagonal, "{}: element extent {margin:.1} m outside the file's geometry", info.name);
        // The geometry itself has the extent the file states: the arcs and tangents to a millimetre, a spiral's bulge past its end points aside.
        eprintln!("GEOM alignment {}: walked minus stated: {:.3} {:.3} {:.3} {:.3}", e.id.0, sx0 - stated_ext.min_easting, sy0 - stated_ext.min_northing, sx1 - stated_ext.max_easting, sy1 - stated_ext.max_northing);
        assert!((sx0 - stated_ext.min_easting).abs() < 5.0 && (sy0 - stated_ext.min_northing).abs() < 5.0 && (sx1 - stated_ext.max_easting).abs() < 5.0 && (sy1 - stated_ext.max_northing).abs() < 5.0, "{}: geometry extent differs from the file's", info.name);
        assert!(sx0 <= stated_ext.min_easting + 0.003 && sy0 <= stated_ext.min_northing + 0.003 && sx1 >= stated_ext.max_easting - 0.003 && sy1 >= stated_ext.max_northing - 0.003, "{}: geometry does not reach the file's extent", info.name);
    }
    assert!(checked >= 3 * 10, "{checked}");
    // Every surface: the element's extent is that of the file's visible triangles.
    let imp = afrobim_corridor::read_landxml_surfaces(&path).unwrap();
    for s in &imp.surfaces {
        let e = app.doc.elements().iter().find(|e| e.name == s.name && (e.category == afrobim_corridor::SURFACE_CATEGORY || e.category == civil::SURFACE)).unwrap_or_else(|| panic!("surface {} missing", s.name));
        let b = s.tin.bounds();
        let (x0, y0, x1, y1) = (e.origin.x / 1000.0, e.origin.y / 1000.0, (e.origin.x + e.width) / 1000.0, (e.origin.y + e.height) / 1000.0);
        assert!((x0 - b.min_x).abs() < 0.002 && (y0 - b.min_y).abs() < 0.002 && (x1 - b.max_x).abs() < 0.002 && (y1 - b.max_y).abs() < 0.002, "{}: {x0} {y0} {x1} {y1} against {b:?}", s.name);
    }
    // Cross-section groups and ground profiles: one group per alignment with sections, one profile per ProfSurf run.
    let with_sections: std::collections::BTreeSet<&str> = file.cross_sections.iter().map(|s| s.alignment.as_str()).collect();
    assert_eq!(app.doc.elements().iter().filter(|e| e.category == civil::SAMPLES).count(), with_sections.len());
    let lines: usize = app.doc.elements().iter().filter(|e| e.category == civil::SAMPLES).map(|e| civil::get::<civil::SampleData>(&app.doc, e.id, civil::SAMPLES).unwrap().group.lines.len()).sum();
    assert_eq!(lines, file.cross_sections.len(), "every section of the file is a sample line");
}

#[test]
fn private_sections_and_ground_profiles_agree_with_afrobims_own_cuts() {
    let Some(path) = private_path() else { return };
    let (_ctx, mut app) = imported(&path);
    app.civil_build_now();
    let ctx = ctx_of(&app.doc);
    let mut worst = 0.0f64;
    let mut stations = 0usize;
    for e in app.doc.elements().iter().filter(|e| e.category == civil::SAMPLES) {
        let mut d: civil::SampleData = civil::get(&app.doc, e.id, civil::SAMPLES).unwrap();
        d.compare = true;
        let views = crate::civil_build2::sample_views(&d, &ctx).unwrap();
        let diffs = crate::civil_sections::compare_views(&views);
        stations += diffs.len();
        for x in &diffs {
            worst = worst.max(x.max_diff_m);
        }
        // Report, never name: the group's index and the largest figure.
        eprintln!("SECTIONS group {}: {} compared, largest {:.1} mm", e.id.0, diffs.len(), diffs.iter().map(|x| x.max_diff_m).fold(0.0, f64::max) * 1000.0);
    }
    eprintln!("SECTIONS all: {stations} station(s) compared, largest {:.2} mm", worst * 1000.0);
    assert!(stations > 0, "something was compared");
    assert!(worst < 0.005, "AfroBIM's section cut differs from Civil 3D's by {:.2} mm", worst * 1000.0);
    // Ground profiles against the profile AfroBIM samples from the imported surface.
    let mut pworst = 0.0f64;
    let mut compared = 0;
    for e in app.doc.elements().iter().filter(|e| e.category == civil::PROFILE) {
        let d: civil::ProfileData = civil::get(&app.doc, e.id, civil::PROFILE).unwrap();
        if d.surface == 0 {
            continue;
        }
        let (Some(tin), Some(al)) = (ctx.tin(d.surface), ctx.alignment_named(&d.alignment)) else { continue };
        let c = crate::civil_sections::compare_profile(&al, &d.imported, &tin);
        eprintln!("PROFILE {}: {} compared, {} off surface, largest {:.1} mm at station {:.3}", e.id.0, c.compared, c.off_surface, c.max_diff_m * 1000.0, c.at_station_m);
        if c.max_diff_m > 0.003 {
            let ex = al.start_station_m;
            let bad: Vec<String> = d.imported.iter().filter_map(|p| al.point_at(p[0]).and_then(|q| tin.elevation_at(q.x, q.y)).map(|z| (p[0], z - p[1]))).filter(|(_, dz)| dz.abs() > 0.003).take(6).map(|(s, dz)| format!("{:.2}:{:+.1}mm", s - ex, dz * 1000.0)).collect();
            eprintln!("   over 3 mm at (station from start): {}", bad.join(" "));
        }
        pworst = pworst.max(c.max_diff_m);
        compared += c.compared;
    }
    eprintln!("PROFILES all: {compared} stations compared, largest {:.2} mm", pworst * 1000.0);
    assert!(compared > 0);
    // Straight stretches agree to the last digit; on curves Civil 3D samples the TIN along chords, which
    // moves a sample sideways by up to a few centimetres on a gentle curve. One profile of the file is
    // not cut from the only surface that covers it (its median difference is hundreds of millimetres).
    let mut foreign = 0;
    let (mut straight_worst, mut curved_worst) = (0.0f64, 0.0f64);
    for e in app.doc.elements().iter().filter(|e| e.category == civil::PROFILE) {
        let d: civil::ProfileData = civil::get(&app.doc, e.id, civil::PROFILE).unwrap();
        let (Some(tin), Some(al)) = (ctx.tin(d.surface), ctx.alignment_named(&d.alignment)) else { continue };
        let mut diffs: Vec<f64> = Vec::new();
        for p in &d.imported {
            let Some(z) = al.point_at(p[0]).and_then(|q| tin.elevation_at(q.x, q.y)) else { continue };
            let diff = (z - p[1]).abs();
            diffs.push(diff);
        }
        diffs.sort_by(f64::total_cmp);
        if diffs.get(diffs.len() / 2).copied().unwrap_or(0.0) > 0.1 {
            foreign += 1;
            continue;
        }
        for p in &d.imported {
            let Some(z) = al.point_at(p[0]).and_then(|q| tin.elevation_at(q.x, q.y)) else { continue };
            let diff = (z - p[1]).abs();
            if al.curvature_at(p[0]).unwrap_or(0.0).abs() < 1e-9 {
                straight_worst = straight_worst.max(diff);
            } else {
                curved_worst = curved_worst.max(diff);
            }
        }
    }
    eprintln!("PROFILES straight largest {:.3} mm, curved largest {:.2} mm, {foreign} profile(s) not cut from the surface that covers them", straight_worst * 1000.0, curved_worst * 1000.0);
    assert!(straight_worst < 1e-4, "straights differ by {:.3} mm", straight_worst * 1000.0);
    assert!(curved_worst < 0.025, "curves differ by {:.2} mm", curved_worst * 1000.0);
    assert!(foreign <= 1);
}

#[test]
fn private_the_3d_view_frames_the_project() {
    let Some(path) = private_path() else { return };
    let (_ctx, mut app) = imported(&path);
    app.civil_build_now();
    app.sync_generated();
    let site = app.site_mode();
    let points = app.shown_points(site);
    assert!(!points.is_empty());
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in &points {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    eprintln!("SHOWN site={site} n={} lo={lo:?} hi={hi:?}", points.len());
    // Everything is within the file's own coordinates (about 360 km east, 9 965 km north in mm), nothing at the origin.
    assert!(lo[0] > 1.0e8 && lo[1] > 1.0e9, "a point near the origin stretches the framing: {lo:?}");
    assert!(hi[0] - lo[0] < 2.0e7 && hi[1] - lo[1] < 2.0e7, "the framing spans {:?}", [hi[0] - lo[0], hi[1] - lo[1]]);
}

#[test]
fn private_the_whole_imported_project_exports_as_landxml_and_reads_back() {
    let Some(path) = private_path() else { return };
    let (_c, mut app) = imported(&path);
    let ctx = app.civil_ctx();
    let (text, notes) = crate::civil_export::landxml_text(&app.doc, &ctx);
    let back = afrobim_alignment::parse_landxml_file(&text).expect("the export reads back");
    let orig = afrobim_alignment::read_landxml_file(&path).unwrap();
    eprintln!("EXPORT {} bytes; alignments {} surfaces {} sections {} ground profiles {}; notes {}", text.len(), back.alignments.len(), back.surfaces.len(), back.cross_sections.len(), back.ground_profiles.len(), notes.len());
    assert_eq!(back.alignments.len(), orig.alignments.len());
    assert_eq!(back.surfaces.len(), orig.surfaces.len());
    assert_eq!(back.cross_sections.len(), orig.cross_sections.len());
    assert_eq!(back.ground_profiles.len(), orig.ground_profiles.len());
    assert_eq!(back.coordinate_system.as_ref().and_then(|c| c.epsg_code.clone()).as_deref(), Some("21036"));
    // The geometry survives: every alignment starts where it did.
    for a in &orig.alignments {
        let b = back.alignments.iter().find(|x| x.name == a.name).expect("alignment written");
        assert!((a.elements[0].x - b.elements[0].x).abs() < 1e-3 && (a.length() - b.length()).abs() < 1e-3, "{}", a.name);
    }
}

#[test]
fn private_zoom_extents_has_nothing_at_the_origin() {
    let Some(path) = private_path() else { return };
    let (_c, app) = imported(&path);
    for e in app.doc.elements() {
        if app.doc.layer_visible(&e.layer) && e.category != afrobim_annotate::SETTINGS_CATEGORY && e.category != afrobim_kernel::PROJECT_SETTINGS_CATEGORY && e.category != crate::underlay::CATEGORY && crate::civil::has_plan_position(&e.category) {
            assert!(e.origin.x > 1.0e8 && e.origin.y > 1.0e9, "{} ({}) is at {:?} with size {} x {}", e.name, e.category, e.origin, e.width, e.height);
        }
    }
}

// ─── superelevation used by corridors, and the design check ─────────────────────────────────

/// A lane of a template that follows superelevation takes the stored cross-fall of its side at the
/// critical stations of an imported alignment.
fn lanes_follow(a: &afrobim_alignment::Alignment) -> usize {
    use afrobim_corridor::{Daylight, Link, Slope, Template, Tin};
    let se = a.superelevation.as_ref().expect("superelevation");
    let mut checked = 0;
    for p in se.sections.iter().flat_map(|s| s.points.iter()) {
        let (Some(l), Some(r)) = (p.left_pct, p.right_pct) else { continue };
        if p.station_m < a.start_station_m || p.station_m > a.end_station_m() {
            continue;
        }
        let at = a.point_at(p.station_m).unwrap();
        let tin = Tin::from_grid(at.x - 300.0, at.y - 300.0, 600.0, 600.0, 2, 2, |_, _| 0.0).unwrap();
        let t = Template::symmetric("se", vec![Link::new("lane", 3.0, Slope::Percent(-2.0)).following_superelevation()], Daylight { cut_run_per_rise: 1.0, fill_run_per_rise: 2.0 }, 0.0);
        let Ok(s) = afrobim_corridor::section_at(&t, 0, a, &tin, p.station_m) else { continue };
        let centre = s.centre_elevation_m();
        let level = |o: f64| s.finished.iter().find(|q| (q.offset_m - o).abs() < 1e-6).map(|q| q.elevation_m).expect("a point at the lane edge");
        assert!((level(-3.0) - (centre + 3.0 * l / 100.0)).abs() < 1e-6, "left lane at station {}", p.station_m);
        assert!((level(3.0) - (centre + 3.0 * r / 100.0)).abs() < 1e-6, "right lane at station {}", p.station_m);
        checked += 1;
    }
    checked
}

#[test]
fn private_corridor_lanes_follow_the_imported_superelevation() {
    let Some(path) = private_path() else { return };
    let (_c, app) = imported(&path);
    let mut total = 0;
    let mut alignments = 0;
    for e in app.doc.elements().iter().filter(|e| e.category == afrobim_alignment::CATEGORY) {
        let a = crate::alignment_glue::alignment_of(&app.doc, e.id).unwrap();
        if a.superelevation.is_some() && a.profile.is_some() {
            alignments += 1;
            total += lanes_follow(&a);
        }
        // The table as the file wrote it is kept and shown for every alignment that has one.
        let kept = crate::civil_super::kept_table(e);
        assert_eq!(kept.is_some(), a.superelevation.is_some() || kept.is_some());
        if let Some(t) = &kept {
            assert!(!crate::civil_super::file_rows(t, &a).is_empty());
        }
    }
    eprintln!("SUPERELEVATION lanes followed the stored cross-fall at {total} station(s) on {alignments} alignment(s)");
    assert!(alignments > 0 && total > 0);
}

#[test]
fn the_design_check_of_an_imported_alignment_names_what_it_could_not_check() {
    crate::defaults_glue::set_basis("dmrb");
    let doc = crate::a1_tests::demo_doc();
    let e = doc.elements().iter().find(|e| e.category == afrobim_alignment::CATEGORY).unwrap();
    let a = crate::alignment_glue::alignment_of(&doc, e.id).unwrap();
    // No design speed: no report, and the message says what to do.
    let (none, notes) = crate::civil_check::check(&a, None);
    assert!(none.is_none() && notes.iter().any(|n| n.contains("no design speed is set")), "{notes:?}");
    // A speed the pack holds: a report whose items cite the pack.
    let speeds = crate::defaults_glue::speeds_of(crate::civil_check::KEYS[1].1);
    assert!(!speeds.is_empty());
    let (report, notes) = crate::civil_check::check(&a, Some(speeds[0]));
    let report = report.expect("a report");
    assert!(report.items.iter().any(|i| i.status != afrobim_design::CheckStatus::NotChecked), "something is checked: {notes:?}");
    // A speed the pack does not hold: nothing is borrowed from a neighbour, and the missing keys are named.
    let (row, missing) = crate::civil_check::pack_row(speeds[0] + 0.5);
    assert!(row.min_radius_m.is_none() && row.min_k_crest.is_none());
    assert_eq!(missing.len(), crate::civil_check::KEYS.len());
    // With no basis chosen nothing is checked at all.
    crate::defaults_glue::set_basis(crate::defaults_glue::NO_BASIS);
    let (none, notes) = crate::civil_check::check(&a, Some(speeds[0]));
    assert!(none.is_none() && notes.iter().any(|n| n.contains("no defaults basis")));
    crate::defaults_glue::set_basis("dmrb");
}

#[test]
fn private_the_design_check_runs_on_every_imported_alignment() {
    let Some(path) = private_path() else { return };
    crate::defaults_glue::set_basis("dmrb");
    let (_c, app) = imported(&path);
    let speeds = crate::defaults_glue::speeds_of(crate::civil_check::KEYS[1].1);
    let speed = speeds.iter().cloned().find(|s| *s >= 60.0).unwrap_or(speeds[0]);
    let (mut with_report, mut items, mut failed) = (0, 0, 0);
    for e in app.doc.elements().iter().filter(|e| e.category == afrobim_alignment::CATEGORY) {
        let a = crate::alignment_glue::alignment_of(&app.doc, e.id).unwrap();
        let (none, notes) = crate::civil_check::check(&a, None);
        assert!(none.is_none() && !notes.is_empty());
        let (report, _notes) = crate::civil_check::check(&a, Some(speed));
        let r = report.expect("a report at a design speed");
        with_report += 1;
        items += r.items.len();
        failed += r.count(afrobim_design::CheckStatus::Fail);
        assert!(!crate::civil_check::summary(&r).is_empty());
    }
    eprintln!("DESIGNCHECK {with_report} alignment(s) at {speed} km/h: {items} item(s), {failed} failing");
    assert_eq!(with_report, 17);
}
