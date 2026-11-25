// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! U2: objects at the coordinates of their source. Fixtures with UTM-sized coordinates (easting
//! about 450 000 m with northing about 35 000 m north of the equator, and about 9 960 000 m south
//! of it) go through the import paths, and the model positions are checked against the numbers
//! written in the files, to a thousandth of a millimetre.

use std::path::{Path, PathBuf};

use afrobim_alignment::AlignmentModule;
use afrobim_draw::Primitive;
use afrobim_kernel::{project_crs, CrsKind, Document, ElementId, Module, Pt};

use crate::alignment_glue;
use crate::crs_ui;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml").join(name)
}

const TOL_MM: f64 = 0.001;

fn near(p: Pt, easting_m: f64, northing_m: f64, what: &str) {
    let (ex, ey) = (easting_m * 1000.0, northing_m * 1000.0);
    assert!((p.x - ex).abs() < TOL_MM && (p.y - ey).abs() < TOL_MM, "{what}: model ({}, {}) mm, the file says ({ex}, {ey}) mm", p.x, p.y);
}

/// Every line end of an element's drawing, in model millimetres: the element origin plus the
/// drawing's own (f64, element-local) coordinates.
fn drawing_vertices(doc: &Document, id: ElementId, module: &dyn Module) -> Vec<Pt> {
    let e = doc.find(id).unwrap();
    let g = module.generate(e).unwrap();
    let mut v = Vec::new();
    for prim in &g.drawing.primitives {
        if let Primitive::Line { from, to, .. } = prim {
            v.push(Pt::new(e.origin.x + from.x, e.origin.y + from.y));
            v.push(Pt::new(e.origin.x + to.x, e.origin.y + to.y));
        }
    }
    v
}

fn has_vertex(vertices: &[Pt], easting_m: f64, northing_m: f64) -> bool {
    vertices.iter().any(|p| (p.x - easting_m * 1000.0).abs() < TOL_MM && (p.y - northing_m * 1000.0).abs() < TOL_MM)
}

/// One alignment: a 100 m straight, a 200 m radius arc through 90 degrees, a 150 m straight.
fn check_alignment_import(file: &str, e0: f64, n0: f64, epsg: u32) {
    let mut doc = Document::default();
    let (added, notes) = alignment_glue::import_landxml(&mut doc, &fixture(file)).unwrap();
    assert_eq!(added, 1);
    // The file's coordinate system became the project's, since the project had none.
    let crs = project_crs(&doc).expect("the file's system is taken");
    assert_eq!((crs.epsg, crs.kind), (Some(epsg), CrsKind::Projected), "{notes:?}");
    let (id, _) = alignment_glue::alignments(&doc).remove(0);
    let e = doc.find(id).unwrap();

    // The element origin carries the true position: the plan origin of the drawing times 1000.
    let site = e.site.as_ref().unwrap();
    assert_eq!((e.origin.x, e.origin.y), (site.x, site.y));
    assert!(e.origin.x > 4.0e8 && e.origin.y > 3.0e7, "{:?}", e.origin);

    let v = drawing_vertices(&doc, id, &AlignmentModule);
    // Start, the two joints and the end are where the file puts them.
    near(v[0], e0, n0, "start");
    assert!(has_vertex(&v, e0 + 100.0, n0), "first joint");
    assert!(has_vertex(&v, e0 + 300.0, n0 + 200.0), "second joint, the end of the arc");
    assert!(has_vertex(&v, e0 + 300.0, n0 + 350.0), "end");
    // A point in the middle of the arc is on the circle of radius 200 m about (e0 + 100, n0 + 200).
    let on_arc = v.iter().filter(|p| ((p.x - (e0 + 100.0) * 1000.0).powi(2) + (p.y - (n0 + 200.0) * 1000.0).powi(2)).sqrt() - 200_000.0 < 1.0e-3 && p.x > (e0 + 100.0) * 1000.0 + 1.0 && p.y < (n0 + 200.0) * 1000.0 - 1.0).count();
    assert!(on_arc >= 2, "chord vertices of the arc lie on the circle: {on_arc}");

    // The geometry of the stored alignment, read at a station, says the same (this is what the
    // station commands, snaps and placement use).
    let a = alignment_glue::alignment_of(&doc, id).unwrap();
    near(alignment_glue::model_point_at(&doc, id, a.start_station_m).unwrap(), e0, n0, "start by station");
    near(alignment_glue::model_point_at(&doc, id, a.start_station_m + 100.0).unwrap(), e0 + 100.0, n0, "joint by station");
    near(alignment_glue::model_point_at(&doc, id, a.end_station_m()).unwrap(), e0 + 300.0, n0 + 350.0, "end by station");

    // The status bar over the start reads the file's start coordinates in metres.
    let start = Pt::new(e0 * 1000.0, n0 * 1000.0);
    let text = crs_ui::en_text(v[0], Some(&crs));
    assert_eq!(text, crs_ui::en_text(start, Some(&crs)));
    assert!(text.starts_with("E 450\u{2009}123.456"), "{text}");
    // ... and the full report says the latitude and longitude on the datum of the system.
    let lines = crs_ui::where_lines(v[0], Some(&crs));
    assert!(lines.iter().any(|l| l.contains("lat ") && l.contains("WGS 84")), "{lines:?}");
}

#[test]
fn a_landxml_alignment_north_of_the_equator_lands_at_source_coordinates() {
    check_alignment_import("utm_true_north.xml", 450_123.456, 35_012.345, 32636);
}

#[test]
fn a_landxml_alignment_at_northing_9_960_000_lands_at_source_coordinates() {
    check_alignment_import("utm_true_south.xml", 450_123.456, 9_960_012.345, 32736);
}

#[test]
fn a_landxml_surface_lands_at_source_coordinates() {
    for (file, e0, n0) in [("utm_true_north.xml", 450_123.456, 35_012.345), ("utm_true_south.xml", 450_123.456, 9_960_012.345)] {
        let import = afrobim_corridor::read_landxml_surfaces(&fixture(file)).unwrap();
        let mut doc = Document::default();
        let id = alignment_glue::add_surface_at_source(&mut doc, &import.surfaces[0].name, &import.surfaces[0].tin, Some(file)).unwrap();
        let v = drawing_vertices(&doc, id, &afrobim_corridor::SurfaceModule::default());
        // The surface covers the alignment's start by 20 m on each side: its corners are at the file's points.
        assert!(has_vertex(&v, e0 - 20.0, n0 - 20.0), "south-west corner of {file}");
        assert!(has_vertex(&v, e0 + 380.0, n0 + 380.0), "north-east corner of {file}");
        let e = doc.find(id).unwrap();
        near(e.origin, e0 - 20.0, n0 - 20.0, "surface origin");
    }
}

#[test]
fn survey_points_land_at_source_coordinates() {
    // PNEZD: point, northing, easting, level, description.
    let dir = std::env::temp_dir().join(format!("afrobim-u2-survey-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("true.csv");
    let (e0, n0) = (450_123.456, 9_960_012.345);
    let mut text = String::new();
    for (i, (de, dn, z)) in [(0.0, 0.0, 100.0), (50.0, 0.0, 101.0), (50.0, 40.0, 102.0), (0.0, 40.0, 101.5), (25.0, 20.0, 100.7)].iter().enumerate() {
        text.push_str(&format!("{},{:.3},{:.3},{:.3},GL\n", i + 1, n0 + dn, e0 + de, z));
    }
    std::fs::write(&path, text).unwrap();
    let request = crate::survey_ui::request("", afrobim_corridor::ColumnOrder::Pnezd, "WGS 84 / UTM zone 36S", "", "").unwrap();
    let mut doc = Document::default();
    let (id, _) = crate::survey_ui::import(&mut doc, &path, &request).unwrap();
    let v = drawing_vertices(&doc, id, &afrobim_corridor::SurfaceModule::default());
    assert!(has_vertex(&v, e0, n0), "south-west corner");
    assert!(has_vertex(&v, e0 + 50.0, n0 + 40.0), "north-east corner");
    near(doc.find(id).unwrap().origin, e0, n0, "survey surface origin");
}

#[test]
fn arrange_never_moves_what_sits_at_source_coordinates() {
    let mut doc = Document::default();
    alignment_glue::import_landxml(&mut doc, &fixture("utm_true_south.xml")).unwrap();
    let (id, _) = alignment_glue::alignments(&doc).remove(0);
    let before = doc.find(id).unwrap().origin;
    doc.add("sign", "S1", Pt::new(5000.0, 5000.0), 1000.0, 1000.0, "Signs", serde_json::json!({}));
    // What the Arrange action does:
    doc.arrange_where(|e| e.category != alignment_glue::CATEGORY);
    assert_eq!(doc.find(id).unwrap().origin, before);
    // The sheet's own elements are still laid out from the sheet's origin, not from the alignment.
    let sign = doc.elements().iter().find(|e| e.category == "sign").unwrap();
    assert!(sign.origin.x.abs() < 1.0e6 && sign.origin.y.abs() < 1.0e6, "{:?}", sign.origin);
    // The settings record at the origin does not drag the extents of far data to (0, 0).
    let ext = doc.extents().unwrap();
    assert!(ext.max.x > 4.0e8 || ext.max.y > 9.0e9);
    afrobim_kernel::set_project_crs(&mut doc, &afrobim_kernel::ProjectCrs::from_epsg(32736).unwrap());
    let doc_only_far = {
        let mut d = Document::default();
        alignment_glue::import_landxml(&mut d, &fixture("utm_true_south.xml")).unwrap();
        d
    };
    let (a, b) = (doc_only_far.extents().unwrap(), {
        let mut d = Document::default();
        alignment_glue::import_landxml(&mut d, &fixture("utm_true_south.xml")).unwrap();
        afrobim_kernel::set_project_crs(&mut d, &afrobim_kernel::ProjectCrs::from_epsg(32736).unwrap());
        d.extents().unwrap()
    });
    assert_eq!((a.min.x, a.min.y, a.max.x, a.max.y), (b.min.x, b.min.y, b.max.x, b.max.y));
    assert!(a.min.x > 4.0e8, "the extents start at the data, not at the origin: {a:?}");
}

#[test]
fn a_document_saved_without_a_coordinate_system_opens_undeclared_and_a_declared_one_round_trips() {
    let mut doc = Document::default();
    alignment_glue::import_landxml(&mut doc, &fixture("corridor.xml")).unwrap();
    assert!(project_crs(&doc).is_none(), "corridor.xml declares none");
    let back = Document::from_json(&doc.to_json()).unwrap();
    assert!(project_crs(&back).is_none());
    afrobim_kernel::set_project_crs(&mut doc, &afrobim_kernel::ProjectCrs::from_epsg(21036).unwrap());
    let back = Document::from_json(&doc.to_json()).unwrap();
    assert_eq!(project_crs(&back).unwrap().epsg, Some(21036));
    // Removing it leaves the objects where they are.
    let before: Vec<_> = back.elements().iter().map(|e| e.origin).collect();
    let mut back = back;
    afrobim_kernel::clear_project_crs(&mut back);
    assert!(project_crs(&back).is_none());
    assert_eq!(before, back.elements().iter().map(|e| e.origin).collect::<Vec<_>>());
}

#[test]
fn the_import_check_names_each_trouble() {
    use afrobim_kernel::{FindingKind, PlanExtent, ProjectCrs};
    let p36s = ProjectCrs::from_epsg(32736).unwrap();
    let ext = |e0: f64, e1: f64, n0: f64, n1: f64| Some(PlanExtent { min_easting: e0, max_easting: e1, min_northing: n0, max_northing: n1 });
    let kinds = |declared: Option<&ProjectCrs>, e: Option<PlanExtent>| crs_ui::dataset_check(Some(&p36s), "f.xml", declared, e).into_iter().map(|f| f.kind).collect::<Vec<_>>();
    let good = ext(450_000.0, 450_400.0, 9_960_000.0, 9_960_400.0);
    assert!(kinds(Some(&p36s), good).contains(&FindingKind::SameSystem));
    assert!(kinds(Some(&ProjectCrs::from_epsg(32636).unwrap()), good).contains(&FindingKind::DifferentSystem));
    assert!(kinds(None, good).contains(&FindingKind::Undeclared));
    assert!(kinds(Some(&p36s), ext(9_960_000.0, 9_960_400.0, 450_000.0, 450_400.0)).contains(&FindingKind::LikelySwapped));
    assert!(kinds(Some(&p36s), ext(450_000_000.0, 450_400_000.0, 9_960_000_000.0, 9_960_400_000.0)).contains(&FindingKind::LikelyMillimetres));
    assert!(kinds(Some(&p36s), ext(1_476_000.0, 1_477_300.0, 32_680_000.0, 32_681_300.0)).contains(&FindingKind::LikelyFeet));
    assert!(kinds(Some(&p36s), ext(450_000.0, 450_400.0, 35_000.0, 35_400.0)).contains(&FindingKind::OutsideValidArea));
}

#[test]
fn an_ifc_export_with_a_crs_imports_back_at_source_coordinates_and_passes_the_check() {
    use afrobim_kernel::SitePlacement;
    let (e0, n0) = (450_123.456, 9_960_012.345);
    let mut doc = Document::default();
    let id = doc.add("sign", "S-1", Pt::new(0.0, 0.0), 600.0, 600.0, "Signs", serde_json::json!({}));
    doc.find_mut(id).unwrap().site = Some(SitePlacement { x: e0 * 1000.0, y: n0 * 1000.0, z: 0.0, rotation_deg: 0.0, alignment: None, station_m: None, offset_m: None });
    let crs = afrobim_kernel::ProjectCrs::from_epsg(32736).unwrap();
    let inputs: Vec<_> = doc.elements().iter().map(|e| afrobim_ifc::IfcElementInput { element: e, generated: None }).collect();
    let options = afrobim_ifc::IfcOptions { timestamp: Some(0), georeference: crs_ui::georeference_of(Some(&crs)), ..Default::default() };
    let text = afrobim_ifc::export_ifc_model(&doc, &inputs, &options).text;
    let dir = std::env::temp_dir().join(format!("afrobim-u2-ifc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("geo.ifc");
    std::fs::write(&path, text).unwrap();

    // Applied (the default): the model is at its map coordinates, in millimetres.
    let read = |apply: bool| match crate::reference_models::read_file(&path, apply) {
        crate::reference_models::ReadOutcome::Read { model, .. } => *model,
        crate::reference_models::ReadOutcome::Failed(why) => panic!("{why}"),
    };
    assert!(crate::reference_models::RefUi::new().apply_map_conversion, "applied by default");
    let applied = read(true);
    let (lo, hi) = applied.bounds().unwrap();
    assert!((lo[0] - e0 * 1000.0).abs() < 1.0 && (lo[1] - n0 * 1000.0).abs() < 1.0, "{lo:?}");
    assert!(hi[0] - lo[0] < 700.0, "the plate is 600 mm wide");
    // Not applied: the file's own coordinates, near the local origin the exporter chose.
    let local = read(false);
    assert!(local.bounds().unwrap().0[0].abs() < 2.0e6);
    // The declared system and the check against the project.
    let declared = crate::reference_models::declared_crs(&applied).unwrap();
    assert_eq!((declared.epsg, declared.horizontal_datum.as_str()), (Some(32736), "WGS 84"));
    let lines = crate::reference_models::crs_lines(&applied, true, applied.bounds(), Some(&crs)).join("\n");
    assert!(lines.contains("the same system as the project"), "{lines}");
    let other = afrobim_kernel::ProjectCrs::from_epsg(32636).unwrap();
    let lines = crate::reference_models::crs_lines(&applied, true, applied.bounds(), Some(&other)).join("\n");
    assert!(lines.contains("CHECK:") && lines.contains("Nothing is reprojected"), "{lines}");
}

#[test]
fn a_typed_easting_and_northing_is_read_in_metres_at_true_magnitude() {
    let units = afrobim_annotate::DisplayUnits::default();
    let crs = afrobim_kernel::ProjectCrs::from_epsg(32736).unwrap();
    let p = crate::anno_glue::parse_point_crs("450123.456,9960012.345", None, &units, Some(&crs)).unwrap();
    near(p, 450_123.456, 9_960_012.345, "typed E,N");
    // Relative input stays in the display unit; with no system an absolute point is as before.
    let q = crate::anno_glue::parse_point_crs("@1,2", Some(p), &units, Some(&crs)).unwrap();
    assert!((q.x - p.x - units.parse_length("1").unwrap()).abs() < 1e-6);
    let plain = crate::anno_glue::parse_point_crs("3,4", None, &units, None).unwrap();
    assert_eq!(plain, crate::anno_glue::parse_point("3,4", None, &units).unwrap());
}
