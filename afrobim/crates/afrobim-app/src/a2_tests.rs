//! Tests of the round 6 work package A2: reprojection, the survey database, point clouds and data
//! shortcuts as document elements and commands.

use afrobim_kernel::crs_plan::{plan_transform, DataExtent};
use afrobim_kernel::{project_crs, set_project_crs, CrsKind, Document, ProjectCrs};

use crate::a1_tests::demo_doc;
use crate::transform_doc as td;

fn arc_doc() -> Document {
    let mut doc = demo_doc();
    set_project_crs(&mut doc, &ProjectCrs::from_epsg(21036).unwrap());
    doc
}

fn plan_for(doc: &Document, selected: Option<&[afrobim_kernel::ElementId]>) -> afrobim_kernel::crs_plan::TransformPlan {
    let from = project_crs(doc).unwrap();
    let to = ProjectCrs::from_epsg(32736).unwrap();
    let (a, b) = td::extent_of(doc, selected).unwrap();
    plan_transform(&from, &to, Some(DataExtent::new(a, b))).unwrap().into_iter().next().unwrap()
}

#[test]
fn the_whole_document_transform_moves_every_category_logs_and_undo_restores_bit_for_bit() {
    let mut doc = arc_doc();
    let before = doc.snapshot();
    let json_before = doc.to_json();
    let plan = plan_for(&doc, None);
    assert_eq!(plan.kind, afrobim_kernel::crs_plan::PlanKind::DatumTransformation);
    let to = ProjectCrs::from_epsg(32736).unwrap();
    let entry = td::apply_plan(&mut doc, None, &plan, &to, "Whole document", "2026-11-16T09:30:00Z".into()).expect("the demonstration transforms");
    assert!(entry.positions > 1000, "{}", entry.positions);
    // The log is in the document and survives a save.
    let back = Document::from_json(&doc.to_json()).unwrap();
    assert_eq!(td::log_of(&back).len(), 1);
    assert_eq!(project_crs(&back).unwrap().epsg, Some(32736));
    // A surface point moved by the datum shift (hundreds of metres), a level did not change.
    let surf = |d: &Document| d.elements().iter().find(|e| e.category == crate::civil::SURFACE).unwrap().data.clone();
    let (a, b) = (surf(&Document::from_json(&json_before).unwrap()), surf(&doc));
    let p = |v: &serde_json::Value| {
        let o = &v["surface"]["definition"]["operations"][0]["points"][0];
        (o["x"].as_f64().unwrap(), o["y"].as_f64().unwrap(), o["z"].as_f64().unwrap())
    };
    let (pa, pb) = (p(&a), p(&b));
    let shift = (pa.0 - pb.0).hypot(pa.1 - pb.1);
    assert!(shift > 100.0 && shift < 600.0, "shift {shift}");
    assert_eq!(pa.2.to_bits(), pb.2.to_bits(), "levels are not converted");
    // Every alignment element moved too, and the joints were reported.
    assert!(entry.notes.iter().any(|n| n.contains("joints")), "{:?}", entry.notes);
    // Undo restores exactly.
    doc.restore(before.clone());
    assert_eq!(doc.snapshot(), before);
    assert_eq!(doc.to_json(), json_before);
}

#[test]
fn undo_in_the_application_restores_the_document_exactly() {
    let doc0 = arc_doc();
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx);
    let mut app = crate::app::AfroBimApp::new(&cc, crate::Options { civil_demo: false, screenshot: Some(std::env::temp_dir().join("afrobim-a2-test.png")), ..crate::Options::default() });
    app.doc = doc0.clone();
    app.flush_commit();
    let plan = plan_for(&app.doc, None);
    let to = ProjectCrs::from_epsg(32736).unwrap();
    td::apply_plan(&mut app.doc, None, &plan, &to, "Whole document", "t".into()).unwrap();
    app.schedule_commit_now();
    app.flush_commit();
    assert_ne!(app.doc.to_json(), doc0.to_json());
    app.undo();
    assert_eq!(app.doc.to_json(), doc0.to_json(), "undo restores every coordinate exactly");
}

#[test]
fn a_category_the_transform_cannot_move_refuses_the_whole_document_and_changes_nothing() {
    let mut doc = arc_doc();
    doc.add("ann-text", "A note", afrobim_kernel::Pt::new(1.0, 1.0), 10.0, 10.0, "0", serde_json::json!({"at": [1.0, 2.0]}));
    let before = doc.to_json();
    let plan = plan_for(&doc, None);
    let to = ProjectCrs::from_epsg(32736).unwrap();
    let err = td::apply_plan(&mut doc, None, &plan, &to, "x", "t".into()).unwrap_err();
    assert!(err.contains("ann-text"), "{err}");
    assert_eq!(doc.to_json(), before);
    assert_eq!(td::unsupported(&doc, None).len(), 1);
}

#[test]
fn a_selection_moves_only_the_selected_objects_and_keeps_the_project_system() {
    let mut doc = arc_doc();
    let pts = doc.elements().iter().find(|e| e.category == crate::civil::POINTS).unwrap().id;
    let other = doc.elements().iter().find(|e| e.category == crate::civil::SURFACE).unwrap().data.clone();
    let plan = plan_for(&doc, Some(&[pts]));
    let to = ProjectCrs::from_epsg(32736).unwrap();
    let e = td::apply_plan(&mut doc, Some(&[pts]), &plan, &to, "Selected", "t".into()).unwrap();
    assert_eq!(e.elements, 1);
    assert_eq!(project_crs(&doc).unwrap().epsg, Some(21036));
    assert_eq!(doc.elements().iter().find(|e| e.category == crate::civil::SURFACE).unwrap().data, other);
    assert!(e.notes.iter().any(|n| n.contains("two systems")));
}

#[test]
fn a_placed_element_that_holds_a_coordinate_is_refused_not_left_behind() {
    let mut doc = arc_doc();
    doc.add("sign", "S1", afrobim_kernel::Pt::new(450_000_000.0, 9_960_000_000.0), 600.0, 600.0, "0", serde_json::json!({"anchor": [450_000.0, 9_960_000.0]}));
    let plan = plan_for(&doc, None);
    let to = ProjectCrs::from_epsg(32736).unwrap();
    let err = td::apply_plan(&mut doc, None, &plan, &to, "x", "t".into()).unwrap_err();
    assert!(err.contains("looks like a coordinate"), "{err}");
}

#[test]
fn a_transform_and_its_reverse_return_the_points_to_a_few_millimetres() {
    let mut doc = arc_doc();
    let plan = plan_for(&doc, None);
    let to = ProjectCrs::from_epsg(32736).unwrap();
    let first = |d: &Document| {
        let e = d.elements().iter().find(|e| e.category == crate::civil::POINTS).unwrap();
        let p = e.data["store"]["points"].as_object().unwrap().values().next().unwrap().clone();
        (p["easting"].as_f64().unwrap(), p["northing"].as_f64().unwrap())
    };
    let p0 = first(&doc);
    td::apply_plan(&mut doc, None, &plan, &to, "there", "t".into()).unwrap();
    let back = plan.reversed();
    let from = ProjectCrs::from_epsg(21036).unwrap();
    td::apply_plan(&mut doc, None, &back, &from, "back", "t".into()).unwrap();
    let p1 = first(&doc);
    assert!((p0.0 - p1.0).hypot(p0.1 - p1.1) < 0.01, "{p0:?} {p1:?}");
    assert_eq!(td::log_of(&doc).len(), 2);
}

#[test]
fn geographic_systems_are_their_own_kind_and_old_documents_still_load() {
    assert_eq!(ProjectCrs::parse("EPSG:4326").unwrap().kind, CrsKind::Geographic);
    assert_eq!(ProjectCrs::geographic(4210).unwrap().horizontal_datum, "Arc 1960");
    // A document with the old named-only value reads as before.
    let old: ProjectCrs = serde_json::from_str(r#"{"epsg":4326,"name":"WGS 84","kind":"NamedOnly"}"#).unwrap();
    assert_eq!(old.kind, CrsKind::NamedOnly);
    let g: ProjectCrs = serde_json::from_str(&serde_json::to_string(&ProjectCrs::geographic(4326).unwrap()).unwrap()).unwrap();
    assert_eq!(g.kind, CrsKind::Geographic);
}

#[test]
fn the_status_bar_offers_wgs84_with_the_transformation_code_and_its_accuracy() {
    let crs = ProjectCrs::from_epsg(21036).unwrap();
    let p = afrobim_kernel::model_point_from_crs(450_000.0, 9_960_000.0);
    let t = crate::crs_ui::wgs84_text(p, &crs).expect("a reading");
    assert!(t.contains("EPSG:") && t.contains("stated accuracy") && t.contains("WGS 84"), "{t}");
    // The project's own datum reading stays the primary one.
    assert!(crate::crs_ui::lat_lon_text(p, &crs).unwrap().contains("Arc 1960"));
    // A WGS 84 project has nothing to add.
    assert!(crate::crs_ui::wgs84_text(p, &ProjectCrs::from_epsg(32736).unwrap()).is_none());
}

#[test]
fn a_site_calibration_is_stored_applied_and_logged() {
    use afrobim_kernel::crs_plan::{ControlPair, SiteCalibration};
    let target = ProjectCrs::from_epsg(32736).unwrap();
    let pairs: Vec<ControlPair> = [(0.0, 0.0), (100.0, 0.0), (0.0, 100.0), (100.0, 100.0)].iter().enumerate().map(|(i, (x, y))| ControlPair { label: format!("C{i}"), local: (*x, *y), target: (450_000.0 + x, 9_960_000.0 + y) }).collect();
    let cal = SiteCalibration::fit("Site", &target, &pairs, None).unwrap();
    assert!(cal.max_m < 1e-6);
    let mut doc = demo_doc();
    set_project_crs(&mut doc, &ProjectCrs::local_grid("Site grid"));
    td::store_calibration(&mut doc, cal.clone());
    let back = Document::from_json(&doc.to_json()).unwrap();
    assert_eq!(td::calibrations_of(&back).len(), 1);
    let before = doc.to_json();
    let e = td::apply_calibration(&mut doc, &cal, &target, "t".into()).unwrap();
    assert!(e.calibration.is_some());
    assert_eq!(project_crs(&doc).unwrap().epsg, Some(32736));
    assert_ne!(doc.to_json(), before);
}
