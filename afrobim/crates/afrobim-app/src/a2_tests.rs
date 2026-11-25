// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
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
    doc.add("mystery-kind", "A thing", afrobim_kernel::Pt::new(1.0, 1.0), 10.0, 10.0, "0", serde_json::json!({"at": [1.0, 2.0]}));
    let before = doc.to_json();
    let plan = plan_for(&doc, None);
    let to = ProjectCrs::from_epsg(32736).unwrap();
    let err = td::apply_plan(&mut doc, None, &plan, &to, "x", "t".into()).unwrap_err();
    assert!(err.contains("mystery-kind"), "{err}");
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
    // A cloud's points stay in their file: a calibration of the document is refused while one is there.
    let cloud = doc.elements().iter().find(|e| e.category == crate::civil::POINTCLOUD).unwrap().id;
    let err = td::apply_calibration(&mut doc.clone(), &cal, &target, "t".into()).unwrap_err();
    assert!(err.contains("point cloud"), "{err}");
    doc.remove(cloud);
    let back = Document::from_json(&doc.to_json()).unwrap();
    assert_eq!(td::calibrations_of(&back).len(), 1);
    let before = doc.to_json();
    let e = td::apply_calibration(&mut doc, &cal, &target, "t".into()).unwrap();
    assert!(e.calibration.is_some());
    assert_eq!(project_crs(&doc).unwrap().epsg, Some(32736));
    assert_ne!(doc.to_json(), before);
}

// ─── survey database ────────────────────────────────────────────────────

use crate::survey_db as sd;
use afrobim_survey::*;

fn example_options(db: &SurveyDatabase) -> AdjustOptions {
    // The test's own numbers: the application supplies none.
    let _ = db;
    AdjustOptions::new(20, 1e-10, 0.95, 0.05, normal_two_sided_critical(0.05))
}

#[test]
fn the_synthetic_survey_database_adjusts_and_the_result_is_near_the_truth() {
    let db = sd::demo_database(450_000.0, 9_960_000.0);
    assert!(db.import_events.len() == 1 && db.import_events[0].description.contains("SYNTHETIC"));
    // The database file survives a round trip bit for bit in its coordinates.
    let back = SurveyDatabase::from_json(&db.to_json().unwrap()).unwrap();
    assert_eq!(back, db);
    let req = sd::AdjRequest { db: db.clone(), network: "Main".into(), mode: sd::AdjMode::Plan2d, options: example_options(&db), snoop_max: None };
    let out = sd::run_adjustment(&req).expect("adjusts");
    let sd::AdjOut::Plan { adjustment: a, report, .. } = out else { panic!("a plan adjustment") };
    assert!(a.converged && a.dof > 0, "{} {}", a.converged, a.dof);
    assert!(a.global_test.is_some());
    assert!(report.contains("Global test"));
    // P2 is at E0+100, N0+120 in the synthetic geometry; the wobble is millimetres.
    let p2 = a.point("P2").unwrap();
    assert!((p2.e - 450_100.0).abs() < 0.05 && (p2.n - 9_960_120.0).abs() < 0.05, "{} {}", p2.e, p2.n);
    // Ellipses, standardised residuals and the relative precision exist.
    assert!(p2.ellipse.semi_major > 0.0);
    assert!(a.observations.iter().any(|o| o.std_residual.is_some()));
    assert!(a.relative_precision("A", "P2").unwrap().sigma_distance > 0.0);
    // Snooping runs the same network.
    let req2 = sd::AdjRequest { snoop_max: Some(2), ..req };
    assert!(sd::run_adjustment(&req2).is_ok());
}

#[test]
fn survey_points_are_inserted_with_a_link_and_follow_the_database() {
    let mut db = sd::demo_database(450_000.0, 9_960_000.0);
    let mut doc = demo_doc();
    let id = sd::insert_points(&mut doc, &db, "survey.afrosurvey", "Main", 500).unwrap();
    let l = sd::link_of(&doc, id).unwrap();
    assert_eq!((l.network.as_str(), l.db_path.as_str()), ("Main", "survey.afrosurvey"));
    // The link survives save and load.
    let back = Document::from_json(&doc.to_json()).unwrap();
    assert_eq!(sd::link_of(&back, id), Some(l));
    assert!(sd::update_points(&mut doc, id, &db, false).unwrap().changed() == false);
    // The database changes: apply an adjustment.
    let req = sd::AdjRequest { db: db.clone(), network: "Main".into(), mode: sd::AdjMode::Plan2d, options: example_options(&db), snoop_max: None };
    let out = sd::run_adjustment(&req).unwrap();
    let n = sd::apply_adjusted(&mut db, "Main", &out, "t").unwrap();
    assert!(n >= 4);
    let state = |doc: &Document, db: &SurveyDatabase| sd::linked_sets(doc, Some(db)).into_iter().find(|s| s.0 == id).unwrap().3;
    assert_eq!(state(&doc, &db), Some(true), "the set notices that the database changed");
    let check = sd::update_points(&mut doc, id, &db, false).unwrap();
    assert!(check.moved.len() >= 4);
    let done = sd::update_points(&mut doc, id, &db, true).unwrap();
    assert_eq!(done.moved, check.moved);
    assert_eq!(state(&doc, &db), Some(false));
    // A point removed from the database leaves the set.
    db.network_mut("Main").unwrap().points.retain(|p| p.name != "P4");
    assert!(sd::update_points(&mut doc, id, &db, true).unwrap().removed.contains(&"P4".to_string()));
}

#[test]
fn figures_from_linework_are_drawn_in_plan_and_survive_a_save() {
    let db = sd::demo_database(450_000.0, 9_960_000.0);
    let data = sd::figure_records(&db, "Main", 0.05).unwrap();
    assert_eq!(data.figures.len(), 1, "{data:?}");
    assert_eq!(data.figures[0].vertices.len(), 4);
    assert!(data.figures[0].breakline, "the figure prefix table says EP is a breakline");
    let mut doc = Document::default();
    let id = sd::add_figures(&mut doc, &data);
    assert_eq!(sd::add_figures(&mut doc, &data), id, "the same network replaces its figures");
    let back = Document::from_json(&doc.to_json()).unwrap();
    assert_eq!(back.find(id).unwrap().data, doc.find(id).unwrap().data);
    // A vertex with no level is kept as such through a save.
    let mut holey = data.clone();
    holey.figures[0].vertices[0][2] = f64::NAN;
    let mut d2 = Document::default();
    let id2 = sd::add_figures(&mut d2, &holey);
    let again: sd::FigureData = serde_json::from_value(Document::from_json(&d2.to_json()).unwrap().find(id2).unwrap().data.clone()).unwrap();
    assert!(again.figures[0].vertices[0][2].is_nan() && again.figures[0].vertices[1][2].is_finite());
    // A figure flagged as a breakline, with a level on every vertex, is offered as one.
    assert_eq!(sd::breaklines_of(&data).len(), 1);
    let mut no_z = data.clone();
    no_z.figures[0].vertices[1][2] = f64::NAN;
    assert!(sd::breaklines_of(&no_z).is_empty(), "a vertex with no level makes it unfit");
}

#[test]
fn a_bad_line_stops_a_strict_import_and_is_listed_by_a_lenient_one() {
    let text = "AFROFB 1\nUNITS DIST=m ANGLE=deg\nCONTROL A 500000 9960000 100\nBOGUS 1 2 3\nCONTROL B 500100 9960000 100\n";
    let strict = sd::read_text(&ImportFormat::Fieldbook, text, true);
    assert!(strict.data.is_none() && strict.errors[0].starts_with("line 4"), "{:?}", strict.errors);
    let lenient = sd::read_text(&ImportFormat::Fieldbook, text, false);
    assert_eq!(lenient.dropped, vec![4]);
    assert_eq!(lenient.data.unwrap().points.len(), 2);
}

#[test]
fn an_import_event_can_be_removed_and_reprocessed() {
    let mut db = sd::demo_database(450_000.0, 9_960_000.0);
    let before = db.network("Main").unwrap().points.len();
    let id = db.import_events[0].id;
    let new = db.reprocess_import_event(id, None, "t", DuplicatePolicy::Replace).unwrap();
    assert_ne!(new, id);
    assert!(db.network("Main").unwrap().points.len() >= 6 && before >= 6);
    db.remove_import_event(new, "t").unwrap();
    // What the event added is gone (the control points); points computed from it by radiation are not its.
    assert!(db.network("Main").unwrap().points.iter().all(|p| !p.control));
    assert!(db.import_events.is_empty());
}

#[test]
fn the_database_path_is_stored_relative_to_the_project() {
    let project = std::path::Path::new("/work/n5/project.afrobim");
    assert_eq!(sd::stored_path(Some(project), std::path::Path::new("/work/n5/survey/job.afrosurvey")), "survey/job.afrosurvey");
    assert_eq!(sd::resolve_path(Some(project), "survey/job.afrosurvey"), std::path::PathBuf::from("/work/n5/survey/job.afrosurvey"));
    assert_eq!(sd::stored_path(Some(project), std::path::Path::new("/else/job.afrosurvey")), "/else/job.afrosurvey");
    let mut doc = Document::default();
    sd::set_db_ref(&mut doc, &sd::DbRef { path: "survey/job.afrosurvey".into(), name: "Job".into() });
    let back = Document::from_json(&doc.to_json()).unwrap();
    assert_eq!(sd::db_ref(&back).unwrap().name, "Job");
}

#[test]
fn the_database_is_saved_and_opened_as_a_file() {
    let dir = std::env::temp_dir().join(format!("afrobim-a2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("job.afrosurvey");
    let db = sd::demo_database(450_000.0, 9_960_000.0);
    sd::save_file(&db, &path).unwrap();
    assert_eq!(sd::load_file(&path).unwrap(), db);
    std::fs::write(&path, "{\"format\":\"other\"}").unwrap();
    assert!(sd::load_file(&path).unwrap_err().contains("format tag"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ─── data shortcuts ─────────────────────────────────────────────────────

mod shortcuts {
    use super::*;
    use afrobim_shortcuts::*;

    fn temp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("afrobim-sc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A source document (the demonstration), a manifest in a folder, and a host document.
    fn setup(tag: &str) -> (std::path::PathBuf, ShortcutProject, Document, std::path::PathBuf) {
        let dir = temp(tag);
        let src_path = dir.join("source.afrobim");
        let src = demo_doc();
        src.save(&src_path).unwrap();
        let mut p = ShortcutProject::create(&dir, "Test project").unwrap();
        let id = src.elements().iter().find(|e| e.category == crate::civil::SURFACE).unwrap().id;
        p.publish(&PublishRequest::new(&src_path, id, "tester", "t0")).unwrap();
        (dir, p, Document::default(), src_path)
    }

    #[test]
    fn a_reference_is_a_normal_element_at_source_coordinates_and_refuses_edits() {
        let (dir, p, mut host, src_path) = setup("ref");
        let key = p.list()[0].key.clone();
        let rep = create_reference(&mut host, &p, &key, &ReferenceOptions::default()).expect("a reference");
        let e = host.find(rep.root).unwrap().clone();
        assert_eq!(e.category, crate::civil::SURFACE);
        assert!(is_reference(&e) && e.layer.starts_with("REF-"));
        // At the source's coordinates, bit for bit.
        let src = Document::load(&src_path).unwrap();
        let s = src.elements().iter().find(|x| x.category == crate::civil::SURFACE).unwrap();
        assert_eq!((e.origin, &e.data), (s.origin, &s.data));
        // It is a surface the corridor machinery can use as ground.
        assert!(crate::civil::surface_choices(&host).iter().any(|c| c.0 == e.id));
        // check_editable refuses; the guard puts a changed reference back at the commit.
        assert!(check_editable(&host, e.id).is_err());
        assert!(crate::shortcuts_ui::refuse_if_referenced(&host, &[e.id]).is_some());
        let committed = host.snapshot();
        host.find_mut(e.id).unwrap().origin.x += 5000.0;
        let notes = crate::shortcuts_ui::guard(&mut host, &committed);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert_eq!(host.find(e.id).unwrap().origin, e.origin, "the move was undone");
        // An erase is put back too.
        host.remove(e.id);
        assert_eq!(crate::shortcuts_ui::guard(&mut host, &committed).len(), 1);
        assert!(host.find(e.id).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn publishing_again_makes_a_reference_out_of_date_and_synchronise_reports_the_change() {
        let (dir, mut p, mut host, src_path) = setup("sync");
        let key = p.list()[0].key.clone();
        create_reference(&mut host, &p, &key, &ReferenceOptions::default()).unwrap();
        let cheap = status(&host, &p, &StatusOptions { check_sources: false });
        assert!(matches!(cheap[0].state, RefState::UpToDate));
        // The source changes and is published again.
        let mut src = Document::load(&src_path).unwrap();
        let id = src.elements().iter().find(|e| e.category == crate::civil::SURFACE).unwrap().id;
        let mut sd: crate::civil::SurfaceData = crate::civil::get(&src, id, crate::civil::SURFACE).unwrap();
        sd.style.minor_interval = Some(0.25);
        crate::civil::set(&mut src, id, serde_json::to_value(&sd).unwrap(), None);
        src.save(&src_path).unwrap();
        p.publish(&PublishRequest::new(&src_path, id, "tester", "t1")).unwrap();
        let st = status(&host, &p, &StatusOptions { check_sources: false });
        assert!(matches!(st[0].state, RefState::OutOfDate { .. }), "{:?}", st[0].state);
        assert_eq!(crate::shortcuts_ui::badge(&st[0]).0, "out of date");
        let rep = synchronise(&mut host, &p, &SyncTarget::All, &ReferenceOptions::default());
        assert_eq!(rep.changed_elements().len(), 1);
        let SyncOutcome::Refreshed { changes, .. } = &rep.items[0].outcome else { panic!("refreshed") };
        assert!(changes.fields.iter().any(|f| f.path.contains("minor_interval")), "{:?}", changes.fields);
        // Promote: an editable independent element.
        let rid = rep.items[0].element;
        promote(&mut host, rid, "tester", "t2").unwrap();
        assert!(check_editable(&host, rid).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_source_that_moved_is_reported_missing_and_repaired_and_the_project_validates() {
        let (dir, mut p, mut host, src_path) = setup("repair");
        let key = p.list()[0].key.clone();
        create_reference(&mut host, &p, &key, &ReferenceOptions::default()).unwrap();
        let moved = dir.join("moved.afrobim");
        std::fs::rename(&src_path, &moved).unwrap();
        let st = status(&host, &p, &StatusOptions { check_sources: true });
        assert!(matches!(st[0].state, RefState::SourceMissing { .. }));
        assert_eq!(crate::shortcuts_ui::badge(&st[0]).0, "source missing");
        assert!(!p.validate().is_empty());
        let r = p.repair_source_path("source.afrobim", &moved).unwrap();
        assert_eq!(r.repaired.len(), 1);
        assert!(p.validate().is_empty());
        assert!(matches!(status(&host, &p, &StatusOptions { check_sources: true })[0].state, RefState::UpToDate));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_round_6_categories_bring_what_their_fields_name() {
        let t = DependencyTable::civil();
        for (cat, field) in [("civil-pipenet", "surface"), ("civil-pipeprofile", "network"), ("civil-grading", "ground"), ("civil-intersection", "inputs.primary"), ("civil-intersection", "corridor"), ("civil-roundabout", "inputs.arms[].alignment")] {
            assert!(t.rules_for(cat).any(|r| r.path == field), "{cat} {field}");
        }
        // A pipe profile of the demonstration depends on its network: the closure holds both.
        let doc = demo_doc();
        let prof = doc.elements().iter().find(|e| e.category == crate::civil::PIPEPROFILE);
        if let Some(pp) = prof {
            let names: Vec<_> = local_closure(&doc, pp.id, &t).into_iter().filter_map(|i| doc.find(i)).map(|e| e.category.clone()).collect();
            assert!(names.contains(&crate::civil::PIPENET.to_string()), "{names:?}");
        }
        // A roundabout needs its arms' alignments.
        let rb = doc.elements().iter().find(|e| e.category == crate::civil::ROUNDABOUT).unwrap();
        let cats: Vec<_> = local_closure(&doc, rb.id, &t).into_iter().filter_map(|i| doc.find(i)).map(|e| e.category.clone()).collect();
        assert!(cats.contains(&"alignment".to_string()), "{cats:?}");
    }

    #[test]
    fn a_whole_document_is_attached_as_an_underlay_and_never_modified() {
        let (dir, _p, mut host, src_path) = setup("under");
        let before = std::fs::read(&src_path).unwrap();
        let r = attach_document(&mut host, &dir, "Source", &src_path, &[], true, "tester", "t").unwrap();
        assert!(r.element_count > 10);
        let u = load_underlay(&dir, &document_references(&host)[0]).unwrap();
        assert!(u.elements().iter().any(|e| e.category == crate::civil::SURFACE));
        assert_eq!(std::fs::read(&src_path).unwrap(), before, "the source is untouched");
        let st = document_status(&host, &dir, true);
        assert!(matches!(st[0].state, DocState::UpToDate));
        let back = Document::from_json(&host.to_json()).unwrap();
        assert_eq!(document_references(&back).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_referenced_document_refuses_the_whole_document_transform_until_it_is_promoted() {
        let (dir, p, mut host, _) = setup("tr");
        let key = p.list()[0].key.clone();
        let rep = create_reference(&mut host, &p, &key, &ReferenceOptions::default()).unwrap();
        set_project_crs(&mut host, &ProjectCrs::from_epsg(21036).unwrap());
        let plan = plan_for(&host, None);
        let to = ProjectCrs::from_epsg(32736).unwrap();
        let err = crate::transform_doc::apply_plan(&mut host, None, &plan, &to, "x", "t".into()).unwrap_err();
        assert!(err.contains("referenced objects"), "{err}");
        promote(&mut host, rep.root, "t", "t").unwrap();
        assert!(crate::transform_doc::apply_plan(&mut host, None, &plan, &to, "x", "t".into()).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_application_guard_stops_a_move_command_on_a_reference() {
        let (dir, p, _, _) = setup("app");
        let key = p.list()[0].key.clone();
        let (ctx, mut app) = crate::u4_dialogs_test::app_for("dark-glass", false);
        crate::u4_dialogs_test::frame(&ctx, &mut app, 2);
        let rep = create_reference(&mut app.doc, &p, &key, &ReferenceOptions::default()).unwrap();
        app.civil_ui.shortcuts.bypass = true;
        app.flush_commit();
        let before = app.doc.find(rep.root).unwrap().origin;
        app.doc.select(&[rep.root], afrobim_kernel::SelectMode::Replace);
        app.run(&ctx, crate::commands::Action::Erase);
        assert!(app.status.contains("read-only") || app.status.to_lowercase().contains("reference"), "{}", app.status);
        // Whatever reaches the document directly is undone at the commit.
        app.doc.find_mut(rep.root).unwrap().origin.y += 1000.0;
        app.flush_commit();
        assert_eq!(app.doc.find(rep.root).unwrap().origin, before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_roundabout_and_a_pipe_profile_are_published_with_their_dependencies_and_referenced_whole() {
        let dir = temp("r6");
        let src_path = dir.join("source.afrobim");
        let src = demo_doc();
        src.save(&src_path).unwrap();
        let mut p = ShortcutProject::create(&dir, "R6").unwrap();
        let table = DependencyTable::civil();
        let rb = src.elements().iter().find(|e| e.category == crate::civil::ROUNDABOUT).unwrap().id;
        let outs = p.publish_closure(&PublishRequest::new(&src_path, rb, "t", "t0"), &table).unwrap();
        assert!(outs.len() >= 2, "the roundabout and its alignments: {outs:?}");
        let key = p.list().iter().find(|s| s.category == crate::civil::ROUNDABOUT).unwrap().key.clone();
        let mut host = Document::default();
        let rep = create_reference(&mut host, &p, &key, &ReferenceOptions::default()).expect("a roundabout reference");
        assert!(rep.items.len() >= 2 && host.elements().iter().any(|e| e.category == "alignment" && is_reference(e)));
        // The references draw: the host builds the roundabout scene without the source.
        let r = host.find(rep.root).unwrap();
        assert!(crate::civil_junction::roundabout_data_ok(r));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Writes a host document with two references and a source that has moved on, for a screenshot
/// (`cargo test -p afrobim-app write_shortcut_screenshot_documents -- --ignored`, folder in `AFROBIM_SC_DIR`).
#[test]
#[ignore]
fn write_shortcut_screenshot_documents() {
    use afrobim_shortcuts::*;
    let Ok(dir) = std::env::var("AFROBIM_SC_DIR") else { return };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src_path = dir.join("survey-and-roads.afrobim");
    let src = demo_doc();
    src.save(&src_path).unwrap();
    let mut p = ShortcutProject::create(&dir, "Demonstration project").unwrap();
    let table = DependencyTable::civil();
    let rb = src.elements().iter().find(|e| e.category == crate::civil::ROUNDABOUT).unwrap().id;
    p.publish_closure(&PublishRequest::new(&src_path, rb, "demo", "2026-11-16T08:00:00Z"), &table).unwrap();
    let sid = src.elements().iter().find(|e| e.category == crate::civil::SURFACE).unwrap().id;
    p.publish(&PublishRequest::new(&src_path, sid, "demo", "2026-11-16T08:00:00Z")).unwrap();
    let mut host = Document::default();
    set_project_crs(&mut host, &ProjectCrs::from_epsg(32736).unwrap());
    for k in p.list().iter().map(|s| s.key.clone()).collect::<Vec<_>>() {
        if k.starts_with("civil-roundabout") || k.starts_with("civil-surface") {
            create_reference(&mut host, &p, &k, &ReferenceOptions::default()).unwrap();
        }
    }
    // The source moves on and publishes the surface again.
    let mut s2 = Document::load(&src_path).unwrap();
    let mut sd: crate::civil::SurfaceData = crate::civil::get(&s2, sid, crate::civil::SURFACE).unwrap();
    sd.style.minor_interval = Some(0.25);
    crate::civil::set(&mut s2, sid, serde_json::to_value(&sd).unwrap(), None);
    s2.save(&src_path).unwrap();
    p.publish(&PublishRequest::new(&src_path, sid, "demo", "2026-11-16T09:00:00Z")).unwrap();
    set_project_setting_for_test(&mut host);
    host.save(&dir.join("host.afrobim")).unwrap();
}

fn set_project_setting_for_test(host: &mut Document) {
    afrobim_kernel::set_project_setting(host, crate::shortcuts_ui::FOLDER_KEY, serde_json::Value::String(".".into()));
}

// ─── the commands of this work package ──────────────────────────────────

#[test]
fn every_a2_command_is_registered_with_a_glyph_and_the_dialog_test_opens_it() {
    use crate::civil_cmds::Cv;
    let new = [
        Cv::Transform, Cv::Calibrate, Cv::SurveyNew, Cv::SurveyOpen, Cv::SurveySettings, Cv::SurveyImport, Cv::SurveyObs, Cv::SurveyTraverse, Cv::SurveyAdjust, Cv::SurveyApply, Cv::SurveyFigures, Cv::SurveyInsert, Cv::SurveyUpdate, Cv::SurveyEvents, Cv::CloudAttach, Cv::CloudProps, Cv::CloudClip, Cv::CloudSlice, Cv::CloudSurface, Cv::CloudCompare, Cv::CloudExport, Cv::ShortcutsManager, Cv::ShortcutFolder, Cv::ShortcutPublish, Cv::ShortcutReference, Cv::ShortcutSync, Cv::ShortcutPromote, Cv::ShortcutRepair, Cv::ShortcutValidate, Cv::ShortcutUnderlay,
    ];
    assert_eq!(new.len(), 30);
    for cv in new {
        let c = crate::commands::COMMANDS.iter().find(|c| c.action == crate::commands::Action::Civil(cv)).unwrap_or_else(|| panic!("{cv:?} is not in the command table"));
        assert!(crate::glyphs::glyph(c.action).is_some(), "{} has no glyph", c.name);
        assert!(!c.help.contains('\u{2014}') && !c.help.contains('\u{2013}'), "{}: no dashes of that kind in text", c.name);
        // Each command is also in the toolspace Prospector tree or its Survey tab, the ribbon and a menu (the ribbon test lists every command's button).
        assert!(crate::ribbon::all_commands().iter().any(|n| *n == c.name), "{} has no ribbon button", c.name);
    }
}

#[test]
fn a_cloud_a_figure_set_and_a_linked_point_set_round_trip_in_the_document() {
    let (_ctx, app) = crate::u4_dialogs_test::app_for("dark-glass", true);
    let json = app.doc.to_json();
    let back = Document::from_json(&json).unwrap();
    for cat in [crate::civil::POINTCLOUD, crate::civil::FIGURES] {
        let a: Vec<_> = app.doc.elements().iter().filter(|e| e.category == cat).collect();
        let b: Vec<_> = back.elements().iter().filter(|e| e.category == cat).collect();
        assert!(!a.is_empty() && a.len() == b.len(), "{cat}");
        for (x, y) in a.iter().zip(&b) {
            assert_eq!((&x.data, &x.properties, x.origin), (&y.data, &y.properties, y.origin), "{cat}");
        }
    }
    assert!(!crate::survey_db::linked_sets(&back, None).is_empty(), "the link of the survey points survives a save");
}

/// The owner's private LandXML export (Arc 1960 / UTM zone 36S) is used only when
/// `AFROBIM_PRIVATE_LANDXML` names it. Nothing from it is written anywhere.
#[test]
fn a_real_civil_3d_export_is_reprojected_by_the_plan_it_declares() {
    let Ok(path) = std::env::var("AFROBIM_PRIVATE_LANDXML") else { return };
    let path = std::path::PathBuf::from(path);
    let file = afrobim_alignment::read_landxml_file(&path).expect("the file reads");
    let file_crs = file.coordinate_system.as_ref().and_then(crate::crs_ui::from_landxml).expect("the file declares a system");
    assert_eq!(file_crs.epsg, Some(21036));
    let project = ProjectCrs::from_epsg(32736).unwrap();
    assert!(crate::transform_ui::differs(Some(&file_crs), Some(&project)));
    let mut doc = Document::default();
    set_project_crs(&mut doc, &file_crs);
    let imp = afrobim_alignment::read_landxml(&path).unwrap();
    for al in &imp.alignments {
        crate::alignment_glue::add(&mut doc, al);
    }
    assert!(!doc.elements().is_empty());
    let ext = td::extent_of(&doc, None).unwrap();
    let plans = plan_transform(&file_crs, &project, Some(DataExtent::new(ext.0, ext.1))).expect("a path exists");
    let plan = plans.into_iter().next().unwrap();
    let before = doc.snapshot();
    let entry = td::apply_plan(&mut doc, None, &plan, &project, "private file", "t".into()).expect("the alignments transform");
    assert!(entry.positions > 0 && project_crs(&doc).unwrap().epsg == Some(32736));
    doc.restore(before.clone());
    assert_eq!(doc.snapshot(), before);
}
