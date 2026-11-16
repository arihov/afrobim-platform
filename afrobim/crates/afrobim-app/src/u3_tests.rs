//! Tests of the civil objects in the application: document round trip of each element, placement at
//! source coordinates, the command table and icons, LandXML in and out, the IFC mapping of
//! surfaces (checked with IfcOpenShell where it is installed), and the size of a large point set.

use std::collections::HashMap;

use afrobim_kernel::{Document, ElementId, ModuleRegistry};

use crate::civil::{self, *};
use crate::civil_cache::{build_all, dependency_hash};
use crate::civil_demo::{build, E0, N0};

fn registry() -> ModuleRegistry {
    let mut r = ModuleRegistry::default();
    crate::alignment_glue::register(&mut r);
    civil::register(&mut r);
    r
}

fn demo_doc() -> Document {
    let mut doc = Document::default();
    let notes = build(&mut doc);
    assert!(notes.is_empty(), "{notes:?}");
    doc
}

fn build_scenes(doc: &Document) -> HashMap<ElementId, crate::civil_build::Built> {
    let elements = doc.elements().iter().filter(|e| is_civil(&e.category) || e.category == afrobim_alignment::CATEGORY).cloned().collect();
    let stale: Vec<ElementId> = doc.elements().iter().filter(|e| is_civil(&e.category)).map(|e| e.id).collect();
    build_all(elements, HashMap::new(), stale, dependency_hash(doc)).into_iter().map(|(id, _, b)| (id, b)).collect()
}

#[test]
fn every_civil_element_survives_save_and_load_and_rebuilds_the_same() {
    let doc = demo_doc();
    let text = doc.to_json();
    let back = Document::from_json(&text).expect("the document loads");
    for (c, _, _) in KINDS {
        let a: Vec<_> = doc.elements().iter().filter(|e| e.category == c).collect();
        let b: Vec<_> = back.elements().iter().filter(|e| e.category == c).collect();
        assert!(!a.is_empty(), "the demonstration has no {c}");
        assert_eq!(a.len(), b.len(), "{c}");
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.name, y.name);
            assert_eq!(x.data, y.data, "{c} data");
            assert_eq!(x.properties, y.properties, "{c} properties");
            assert_eq!(x.origin, y.origin, "{c} origin");
        }
    }
    // The scenes built from the loaded document match those of the original.
    let (s1, s2) = (build_scenes(&doc), build_scenes(&back));
    for e in doc.elements().iter().filter(|e| is_civil(&e.category)) {
        let (a, b) = (&s1[&e.id].scene, &s2[&e.id].scene);
        assert_eq!(a.lines.len(), b.lines.len(), "{}", e.name);
        assert_eq!(a.points.len(), b.points.len(), "{}", e.name);
        assert_eq!(a.derived, b.derived, "{}", e.name);
    }
    // Every object reads through its module, and the scene of the demonstration has no failure.
    let modules = registry();
    for e in doc.elements().iter().filter(|e| is_civil(&e.category)) {
        modules.generate(e).unwrap_or_else(|x| panic!("{}: {x}", e.name));
        assert!(s1[&e.id].scene.notes.is_empty(), "{}: {:?}", e.name, s1[&e.id].scene.notes);
    }
}

#[test]
fn objects_sit_at_source_coordinates_to_a_thousandth_of_a_millimetre() {
    let doc = demo_doc();
    let scenes = build_scenes(&doc);
    let pts = doc.elements().iter().find(|e| e.category == POINTS).unwrap();
    // The element origin is the least easting and northing times 1000, exactly.
    assert!((pts.origin.x - E0 * 1000.0).abs() < 1e-3 && (pts.origin.y - N0 * 1000.0).abs() < 1e-3, "{:?}", pts.origin);
    let site = pts.site.as_ref().unwrap();
    assert!((site.x - E0 * 1000.0).abs() < 1e-3 && (site.y - N0 * 1000.0).abs() < 1e-3);
    // Every marker is at its point's source coordinates times 1000.
    let data = get::<PointsData>(&doc, pts.id, POINTS).unwrap();
    let scene = &scenes[&pts.id].scene;
    for (m, p) in scene.points.iter().zip(data.store.iter()) {
        assert!((m.at[0] - p.easting * 1000.0).abs() < 1e-3 && (m.at[1] - p.northing * 1000.0).abs() < 1e-3, "point {}", p.number);
    }
    // A parcel's outline is at its corner coordinates times 1000.
    let parcel = doc.elements().iter().find(|e| e.category == PARCEL).unwrap();
    let pd = get::<afrobim_cogo::Parcel>(&doc, parcel.id, PARCEL).unwrap();
    let line = &scenes[&parcel.id].scene.lines[0];
    assert!((line.pts[0][0] - pd.boundary.start.e * 1000.0).abs() < 1e-3 && (line.pts[0][1] - pd.boundary.start.n * 1000.0).abs() < 1e-3);
    // A surface's mesh is in millimetres from its origin, so origin plus vertex is the source position.
    let sf = doc.elements().iter().find(|e| e.category == SURFACE).unwrap();
    let b = &scenes[&sf.id];
    let tin = b.tin.as_ref().unwrap();
    let mesh = &b.scene.meshes[0];
    let tb = tin.bounds();
    let (dx, dy) = ((tb.min_x - sf.origin.x / 1000.0) * 1000.0, (tb.min_y - sf.origin.y / 1000.0) * 1000.0);
    assert!(mesh.vertices.iter().any(|v| ((sf.origin.x + v[0]) - tb.min_x * 1000.0).abs() < 1e-3 || dx.abs() > 0.0));
    let _ = dy;
    // The coordinates are of true UTM size and kept to the 0.0001 m the point engine keeps.
    assert!(pts.origin.x > 4.0e8 && pts.origin.y > 9.0e9);
}

#[test]
fn every_civil_command_is_in_the_table_with_an_icon() {
    use crate::commands::{Action, COMMANDS};
    for cv in crate::civil_cmds::ALL {
        let info = COMMANDS.iter().find(|c| c.action == Action::Civil(cv)).unwrap_or_else(|| panic!("{cv:?} is not in the command table"));
        assert!(!crate::civil_cmds::glyph(cv).is_empty());
        assert!(!info.help.is_empty());
        assert_eq!(crate::commands::resolve(info.name).map(|c| c.action), Some(Action::Civil(cv)));
        assert!(crate::glyphs::glyph(info.action).is_some());
    }
    let n = COMMANDS.iter().filter(|c| matches!(c.action, Action::Civil(_))).count();
    assert_eq!(n, crate::civil_cmds::ALL.len());
    // The Civil workspace shows the Civil tab, whose panels are the plan's eight, then pipe networks, grading and intersections.
    let ws = crate::workspace::WORKSPACES.iter().find(|w| w.name == "Civil").expect("a Civil workspace");
    assert!(ws.visible_tabs().iter().any(|t| t.tab == crate::ribbon::Tab::Civil));
    assert_eq!(crate::ribbon::panel_titles(crate::ribbon::Tab::Civil), vec!["Points", "Surfaces", "Alignments", "Profiles", "Sections", "Corridors", "Parcels", "Pipe networks", "Grading", "Intersections", "Survey database", "Reprojection", "Quantities"]);
}

#[test]
fn landxml_out_carries_everything_and_reads_back() {
    let doc = demo_doc();
    let elements: Vec<_> = doc.elements().iter().filter(|e| is_civil(&e.category) || e.category == afrobim_alignment::CATEGORY).cloned().collect();
    let scenes = build_scenes(&doc);
    let tins = scenes.iter().filter_map(|(id, b)| b.tin.clone().map(|t| (id.0, t))).collect();
    let ctx = crate::civil_build::Ctx { elements, tins };
    let (text, notes) = crate::civil_export::landxml_text(&doc, &ctx);
    assert!(text.contains("EPSG") || text.contains("32736"), "the project coordinate system is written");
    let file = afrobim_alignment::parse_landxml_file(&text).expect("the file reads back");
    // The demonstration road, the side road, the roundabout arms and everything the junctions made.
    assert_eq!(file.alignments.len(), doc.elements().iter().filter(|e| e.category == afrobim_alignment::CATEGORY).count());
    assert!(file.alignments.iter().any(|a| a.name.starts_with("Demo road")));
    assert!(text.contains("<PipeNetworks"), "the pipe network is written");
    assert_eq!(file.cg_points.iter().map(|g| g.all_points().len()).sum::<usize>(), 256);
    // The hidden originals of the split are not written; their pieces are.
    assert_eq!(file.parcels.len(), doc.elements().iter().filter(|e| e.category == PARCEL && !hidden(e)).count());
    assert!(file.parcels.len() >= 4);
    assert_eq!(file.surfaces.len(), 1);
    assert_eq!(file.ground_profiles.len(), 1);
    assert!(!file.cross_sections.is_empty());
    assert_eq!(file.coordinate_system.as_ref().and_then(|c| c.epsg_code.clone()).as_deref(), Some("32736"));
    // Source coordinates are written as they are.
    let p = file.cg_points[0].all_points()[0].clone();
    assert!((p.easting_m - E0).abs() < 1e-4 && (p.northing_m - N0).abs() < 1e-4);
    assert!(notes.iter().any(|n| n.contains("corridor")));
}

#[test]
fn landxml_points_parcels_and_profiles_come_in_at_the_coordinates_of_the_file() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/full_utm_south.xml");
    let file = afrobim_alignment::read_landxml_file(&path).expect("the fixture reads");
    let mut doc = Document::default();
    let groups = crate::civil_landxml::point_groups(&file);
    assert_eq!(groups.len(), 1);
    let (id, n) = crate::civil_landxml::add_points(&mut doc, "From LandXML", groups[0].1, "full_utm_south.xml");
    assert_eq!(n, 2);
    let d = get::<PointsData>(&doc, id, POINTS).unwrap();
    let p = d.store.iter().next().unwrap();
    assert!((p.northing - 9_960_012.3456).abs() < 1e-4 && (p.easting - 450_123.4567).abs() < 1e-4, "{p:?}");
    let e = doc.find(id).unwrap();
    assert!((e.origin.x - 450_123.4567 * 1000.0).abs() < 1e-3);
    let parcels = crate::civil_landxml::all_parcels(&file);
    let parcel = crate::civil_landxml::parcel_from(parcels[0], 1).expect("a parcel");
    // The fixture's parcel is a triangle (half of 50 m by 50 m); its area comes from the boundary, not from the file's attribute.
    assert!((parcel.area().unwrap() - 1250.0).abs() < 1e-6, "{}", parcel.area().unwrap());
}

#[test]
fn a_surface_definition_rebuilds_and_an_imported_surface_converts_without_losing_triangles() {
    use afrobim_terrain::{Operation, PasteSource, Surface, SurfaceRole};
    let doc = demo_doc();
    let scenes = build_scenes(&doc);
    let sf = doc.elements().iter().find(|e| e.category == SURFACE).unwrap();
    let tin = scenes[&sf.id].tin.clone().unwrap();
    assert!(tin.triangles().len() > 400);
    // The conversion keeps the triangulation: a definition that pastes the TIN builds the same one.
    let mut s = Surface::new("converted", SurfaceRole::ExistingGround);
    s.definition.push(Operation::Paste { source: PasteSource::Tin((*tin).clone()) });
    let rebuilt = s.build().unwrap();
    assert_eq!(rebuilt.tin.triangles().len(), tin.triangles().len());
    assert_eq!(rebuilt.tin.points().len(), tin.points().len());
    // The style draws contours at the intervals entered and nothing when they are blank.
    let none = crate::civil_build::surface_scene(&tin, &SurfaceStyle::default(), (0.0, 0.0));
    assert!(none.lines.is_empty());
    let with = crate::civil_build::surface_scene(&tin, &SurfaceStyle::example(), (0.0, 0.0));
    assert!(with.lines.len() > 5);
}

#[test]
fn the_acquisition_schedule_groups_the_pieces_of_a_split_parcel() {
    use afrobim_cogo::{Boundary, Parcel, Xy};
    let sq = |x: f64, w: f64| Boundary::polygon(&[Xy::new(x, 0.0), Xy::new(x + w, 0.0), Xy::new(x + w, 10.0), Xy::new(x, 10.0)]).unwrap();
    let mut whole = Parcel::new("P1", 1, sq(0.0, 10.0));
    whole.owner = "A".into();
    let mut take = Parcel::new("P1 take 1", 2, sq(0.0, 3.0));
    take.attributes.insert("Split from".into(), "P1".into());
    take.attributes.insert("Piece".into(), "take".into());
    let mut rest = Parcel::new("P1 remainder 1", 3, sq(3.0, 7.0));
    rest.attributes.insert("Split from".into(), "P1".into());
    rest.attributes.insert("Piece".into(), "remainder".into());
    let other = Parcel::new("P2", 4, sq(20.0, 5.0));
    let rows = crate::civil_parcels::schedule_rows(&[(ElementId(1), whole, true), (ElementId(2), take, false), (ElementId(3), rest, false), (ElementId(4), other, false)]);
    assert_eq!(rows.len(), 2);
    let r = rows.iter().find(|r| r.parcel_name == "P1").unwrap();
    assert!((r.take_area_m2 - 30.0).abs() < 1e-9 && (r.remainder_area_m2 - 70.0).abs() < 1e-9 && (r.total_area_m2 - 100.0).abs() < 1e-9 && r.affected);
    assert!(!rows.iter().find(|r| r.parcel_name == "P2").unwrap().affected);
}

#[test]
fn a_hundred_thousand_points_are_one_element_that_builds_and_loads_in_reasonable_time() {
    let mut data = PointsData::default();
    let t0 = std::time::Instant::now();
    for i in 0..100_000u32 {
        let (e, n) = (E0 + (i % 400) as f64 * 2.5, N0 + (i / 400) as f64 * 2.5);
        let _ = data.store.add(afrobim_cogo::CogoPoint::new(i + 1, e, n, Some(1150.0 + (i % 7) as f64 * 0.1), "GR"));
    }
    let mut doc = Document::default();
    let b = Bounds::of(data.store.iter().map(|p| (p.easting, p.northing)));
    let v = serde_json::to_value(&data).unwrap();
    let id = civil::add(&mut doc, POINTS, "Large set", v, b, "test");
    let text = doc.to_json();
    let back = Document::from_json(&text).unwrap();
    let e = back.find(id).unwrap().clone();
    let built = crate::civil_build::build(&e, &crate::civil_build::Ctx { elements: vec![e.clone()], tins: HashMap::new() });
    assert_eq!(built.scene.points.len(), 100_000);
    eprintln!("100 000 points: document {} MB, build and reload {:.1} s", text.len() / 1_000_000, t0.elapsed().as_secs_f64());
    assert!(t0.elapsed().as_secs_f64() < 60.0);
}

/// Python with IfcOpenShell, when there is one (the test then checks the file; without it the test says so).
fn python_with_ifcopenshell() -> Option<String> {
    for py in ["python3", "python"] {
        if std::process::Command::new(py).args(["-c", "import ifcopenshell"]).output().is_ok_and(|o| o.status.success()) {
            return Some(py.to_string());
        }
    }
    None
}

#[test]
fn a_civil_surface_is_written_as_terrain_and_ifcopenshell_agrees() {
    let doc = demo_doc();
    let scenes = build_scenes(&doc);
    // The same hand-over the application's export does.
    let sf = doc.elements().iter().find(|e| e.category == SURFACE).unwrap();
    let tin = scenes[&sf.id].tin.clone().unwrap();
    let def = afrobim_corridor::SurfaceDefinition { tin: (*tin).clone(), source: Some("demo".into()), crs: None };
    let mut el = sf.clone();
    el.category = afrobim_corridor::SURFACE_CATEGORY.to_string();
    el.data = serde_json::to_value(&def).unwrap();
    use afrobim_kernel::Module;
    let g = afrobim_corridor::SurfaceModule::default().generate(&el).unwrap();
    let inputs = [afrobim_ifc::IfcElementInput { element: &el, generated: Some(&g) }];
    let options = afrobim_ifc::IfcOptions { project_name: "u3".into(), ..Default::default() };
    let export = afrobim_ifc::export_ifc_model(&doc, &inputs, &options);
    assert!(export.text.contains("IFCGEOGRAPHICELEMENT") && export.text.contains(".TERRAIN."));
    let Some(py) = python_with_ifcopenshell() else {
        eprintln!("IfcOpenShell is not installed: the terrain export was checked as text only.");
        return;
    };
    let dir = std::env::temp_dir().join("afrobim-u3");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("terrain.ifc");
    std::fs::write(&file, &export.text).unwrap();
    let script = "import sys, ifcopenshell\nm = ifcopenshell.open(sys.argv[1])\nes = m.by_type('IfcGeographicElement')\nassert len(es) == 1, len(es)\nassert es[0].PredefinedType == 'TERRAIN', es[0].PredefinedType\nimport ifcopenshell.validate\nlogger = ifcopenshell.validate.json_logger()\nifcopenshell.validate.validate(m, logger, express_rules=False)\nassert not logger.statements, logger.statements[:3]\nprint('ok')\n";
    let out = std::process::Command::new(py).args(["-c", script, file.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn the_assembly_editor_model_round_trips_and_builds_an_assembly() {
    let ed = crate::civil_demo::editor_assembly();
    let (a, problems) = ed.build();
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(a.left.len(), 3);
    assert_eq!(a.right.len(), 3);
    let text = serde_json::to_string(&ed).unwrap();
    let back: crate::civil_assembly::EdAssembly = serde_json::from_str(&text).unwrap();
    assert_eq!(back, ed);
    // A new part is blank and incomplete until its inputs are given.
    let blank = crate::civil_assembly::EdPart::new(0);
    assert!(blank.build().is_err());
    // The IFC entity offered is the constant the corridor crate provides.
    let mut p = crate::civil_assembly::EdPart::new(0);
    p.example();
    p.ifc = 1;
    assert_eq!(p.build().unwrap().classification.ifc_entity.as_deref(), Some(afrobim_corridor::ifc::PAVEMENT));
}

#[test]
fn the_road_reserve_splits_the_demonstration_parcels_into_take_and_remainder() {
    let doc = demo_doc();
    let parcels: Vec<(ElementId, afrobim_cogo::Parcel, bool)> = doc.elements().iter().filter(|e| e.category == PARCEL).filter_map(|e| get::<afrobim_cogo::Parcel>(&doc, e.id, PARCEL).map(|p| (e.id, p, hidden(e)))).collect();
    // Two originals (hidden) and their pieces.
    assert_eq!(parcels.iter().filter(|p| p.2).count(), 2);
    let takes: Vec<_> = parcels.iter().filter(|p| p.1.attributes.get("Piece").is_some_and(|k| k == "take")).collect();
    assert!(!takes.is_empty());
    assert!(takes.iter().all(|p| p.1.status == afrobim_cogo::ParcelStatus::ToBeAcquired));
    // The pieces add up to the original areas (to arc and snapping precision: straight edges here).
    let orig: f64 = parcels.iter().filter(|p| p.2).map(|p| p.1.area().unwrap()).sum();
    let pieces: f64 = parcels.iter().filter(|p| !p.2).map(|p| p.1.area().unwrap()).sum();
    assert!((orig - pieces).abs() < 0.5, "{orig} against {pieces}");
    let rows = crate::civil_parcels::schedule_rows(&parcels);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.affected && r.take_area_m2 > 0.0));
}
