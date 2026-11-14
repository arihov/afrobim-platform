//! The surface and corridor elements in a document, and a timing on a large surface.
//!
//! All dimensions are arbitrary test numbers, not design values.

use std::path::PathBuf;

use afrobim_alignment::{AlignmentBuilder, Profile, Pvi};
use afrobim_corridor::{
    add_corridor, add_surface, build_corridor, corridor_element, corridor_meshes, corridor_of, earthworks, earthworks_csv, import_landxml_surfaces,
    refresh_corridor, set_corridor_definition, surface_display_mesh, surface_of, surfaces, Corridor, CorridorDefinition, CorridorError, CorridorModule,
    Daylight, Link, Slope, SurfaceModule, Template, Tin, VolumeOptions, CORRIDORS_LAYER, CORRIDOR_CATEGORY, SURFACES_LAYER, SURFACE_CATEGORY,
};
use afrobim_kernel::{Document, ElementId, Module, ModuleRegistry, Pt, Severity};

fn close(got: f64, want: f64, what: &str) {
    assert!((got - want).abs() <= 1e-9 * (1.0 + want.abs()), "{what}: got {got}, want {want}");
}

/// Level 5 m lanes, cut 1:1, fill 1 in 2.
fn template(lane_m: f64) -> Template {
    Template::symmetric("flat", vec![Link::new("lane", lane_m, Slope::Percent(0.0))], Daylight { cut_run_per_rise: 1.0, fill_run_per_rise: 2.0 }, 0.0)
}

/// A document with a straight alignment east from (1000, 2000), level profile at `level`, and
/// level ground at 100 around it. Returns the alignment and surface ids.
fn document(level: f64) -> (Document, ElementId, ElementId) {
    let mut doc = Document::default();
    let mut a = AlignmentBuilder::new("MC01", 0.0, 1000.0, 2000.0, 0.0).line(100.0).build().unwrap();
    a.profile = Some(Profile::new("FG", vec![Pvi::point(0.0, level), Pvi::point(100.0, level)]).unwrap());
    let alignment = doc.add(afrobim_alignment::CATEGORY, "MC01", Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&a).unwrap());
    let ground = Tin::from_grid(980.0, 1960.0, 20.0, 20.0, 8, 5, |_, _| 100.0).unwrap(); // x 980..1120, y 1960..2040
    let surface = add_surface(&mut doc, "EG", &ground, Some("survey.xml")).unwrap();
    (doc, alignment, surface)
}

#[test]
fn a_surface_element_holds_and_draws_its_tin() {
    let (doc, _, id) = document(102.0);
    let e = doc.find(id).unwrap();
    assert_eq!((e.category.as_str(), e.name.as_str(), e.layer.as_str()), (SURFACE_CATEGORY, "EG", SURFACES_LAYER));
    assert_eq!((e.width, e.height), (140_000.0, 80_000.0));
    let site = e.site.as_ref().unwrap();
    assert_eq!((site.x, site.y, site.z, site.rotation_deg), (980_000.0, 1_960_000.0, 0.0, 0.0));
    assert!(!doc.layer_visible(SURFACES_LAYER), "hidden on the design sheet, like alignments");
    assert_eq!(surfaces(&doc), vec![(id, "EG".to_string())]);
    assert_eq!(e.data["source"], "survey.xml");
    assert_eq!(e.data["tin"]["point_count"], 40);

    let tin = surface_of(&doc, id).unwrap();
    assert_eq!(tin.triangles().len(), 56);
    assert_eq!(tin.elevation_at(1000.0, 2000.0), Some(100.0));

    let g = SurfaceModule::default().generate(e).unwrap();
    // 7 × 4 squares: 8×4 + 7×5 = 67 grid edges and 28 diagonals; 22 of the grid edges are the boundary.
    assert_eq!(g.drawing.primitives.len(), 95);
    assert_eq!((g.drawing.extents.max.x, g.drawing.extents.max.y), (140_000.0, 80_000.0));
    assert_eq!(g.derived["triangles"], "56");
    assert_eq!(g.derived["plan_area_m2"], "11200.000");
    assert_eq!(g.derived["plan_origin_x_m"], "980.000000");
    assert_eq!((g.derived["min_level_m"].as_str(), g.derived["max_level_m"].as_str()), ("100.000", "100.000"));
    assert!(g.notes.is_empty());
    let mesh = &g.solids[0];
    assert_eq!((mesh.name.as_str(), mesh.triangles.len()), ("Ground surface", 56));
    assert_eq!(mesh.vertices[0], [0.0, 0.0, 100_000.0]);
    assert_eq!(mesh.vertices[39], [140_000.0, 80_000.0, 100_000.0]);
}

#[test]
fn a_large_surface_is_shown_resampled_but_kept_whole() {
    // 61 × 41 points: 4800 triangles, on the plane z = 50 + 0.01 x + 0.02 y over 600 m × 400 m.
    let tin = Tin::from_grid(0.0, 0.0, 10.0, 10.0, 61, 41, |x, y| 50.0 + 0.01 * x + 0.02 * y).unwrap();
    let (mesh, resampled) = surface_display_mesh(&tin, 1000);
    assert!(resampled);
    assert!(mesh.triangles.len() <= 1000 && mesh.triangles.len() > 800, "{}", mesh.triangles.len());
    // Every display vertex is on the surface: z = 50 + 0.01 x + 0.02 y, in millimetres.
    for v in &mesh.vertices {
        close(v[2], 50_000.0 + 0.01 * v[0] + 0.02 * v[1], "display vertex on the plane");
    }
    for t in &mesh.triangles {
        let [p, q, r] = t.map(|i| mesh.vertices[i as usize]);
        assert!((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]) > 0.0, "faces up");
    }
    assert!(!surface_display_mesh(&tin, 4800).1, "at the cap the TIN itself is shown");

    let mut doc = Document::default();
    let id = add_surface(&mut doc, "", &tin, None).unwrap();
    let e = doc.find(id).unwrap();
    assert_eq!(e.name, "Surface");
    let g = SurfaceModule::with_display_cap(1000).generate(e).unwrap();
    assert_eq!(g.notes.len(), 1);
    assert_eq!((g.notes[0].severity, g.notes[0].code.as_str()), (Severity::Info, "surface.display_resampled"));
    // Boundary only in plan: 2 × (60 + 40) edges.
    assert_eq!(g.drawing.primitives.len(), 200);
    assert_eq!(g.derived["triangles"], "4800");
    // The stored surface is still the whole one.
    assert_eq!(surface_of(&doc, id).unwrap(), tin);
}

/// Centre at 102 on level ground at 100: area = 10 × 2 + 2 × ½ × 4 × 2 = 28 m², 2800 m³ over 100 m.
#[test]
fn a_corridor_element_refers_to_its_alignment_and_surface() {
    let (mut doc, alignment, surface) = document(102.0);
    let (id, corridor) = add_corridor(&mut doc, alignment, surface, CorridorDefinition::single("Main road", template(5.0), 20.0)).unwrap();
    assert_eq!(corridor.sections.len(), 6);
    let e = doc.find(id).unwrap().clone();
    assert_eq!((e.category.as_str(), e.name.as_str(), e.layer.as_str()), (CORRIDOR_CATEGORY, "Main road", CORRIDORS_LAYER));
    assert!(!doc.layer_visible(CORRIDORS_LAYER));
    // Daylight 9 m each side of a centreline at northing 2000, from easting 1000 to 1100.
    assert_eq!((e.width, e.height), (100_000.0, 18_000.0));
    let site = e.site.as_ref().unwrap();
    assert_eq!((site.x, site.y), (1_000_000.0, 1_991_000.0));
    let stored = corridor_element(&doc, id).unwrap();
    assert_eq!((stored.alignment, stored.surface), (alignment, surface));
    let summary = stored.computed.as_ref().unwrap();
    close(summary.total_fill_m3, 2800.0, "stored total fill");
    assert_eq!((summary.sections, summary.sections_without_areas), (6, 0));

    let g = CorridorModule.generate(&e).unwrap();
    // Centreline, two edges, two daylight lines: 5 segments each.
    assert_eq!(g.drawing.primitives.len(), 25);
    assert_eq!((g.drawing.extents.max.x, g.drawing.extents.max.y), (100_000.0, 18_000.0));
    assert_eq!(g.derived["total_fill_m3"], "2800.000");
    assert_eq!(g.derived["total_cut_m3"], "0.000");
    assert_eq!(g.derived["sections"], "6");
    assert_eq!(g.derived["plan_origin_y_m"], "1991.000000");
    assert!(g.notes.is_empty() && g.solids.is_empty());

    // The registry finds both modules by category.
    let mut modules = ModuleRegistry::default();
    modules.register(Box::new(SurfaceModule::default()));
    modules.register(Box::new(CorridorModule));
    assert!(modules.generate(&e).is_ok() && modules.generate(doc.find(surface).unwrap()).is_ok());

    // Built again from the document, and from a TIN the caller already holds: the same corridor.
    assert_eq!(corridor_of(&doc, id).unwrap(), corridor);
    assert_eq!(build_corridor(&doc, id, &surface_of(&doc, surface).unwrap()).unwrap(), corridor);
    let csv = earthworks_csv(&earthworks(&corridor.sections, &stored.definition.volumes));
    assert_eq!(csv.lines().last().unwrap(), "0+100.000,0.000,28.000,0.000,560.000,0.000,2800.000,-2800.000");
    let meshes = corridor_meshes(&corridor);
    assert_eq!((meshes.origin_x_m, meshes.origin_y_m), (1000.0, 2000.0));
}

#[test]
fn the_document_round_trips_and_the_corridor_follows_changes() {
    let (mut doc, alignment, surface) = document(102.0);
    let (id, corridor) = add_corridor(&mut doc, alignment, surface, CorridorDefinition::single("C", template(5.0), 20.0)).unwrap();

    // Through the document's own JSON: the surface, the references and the summary all survive.
    let back = Document::from_json(&doc.to_json()).unwrap();
    assert_eq!(back.elements(), doc.elements());
    assert_eq!(surface_of(&back, surface), surface_of(&doc, surface));
    assert_eq!(corridor_of(&back, id).unwrap(), corridor);

    // A wider template: 14 × 2 + 8 = 36 m².
    let wider = set_corridor_definition(&mut doc, id, CorridorDefinition::single("C", template(7.0), 20.0)).unwrap();
    close(wider.sections[0].fill_area_m2.unwrap(), 36.0, "wider template");
    close(corridor_element(&doc, id).unwrap().computed.unwrap().total_fill_m3, 3600.0, "summary follows");
    assert_eq!(doc.find(id).unwrap().height, 22_000.0);
    // A definition that is refused leaves the element as it was.
    assert!(matches!(set_corridor_definition(&mut doc, id, CorridorDefinition::single("C", template(7.0), -1.0)), Err(CorridorError::Definition(_))));
    assert_eq!(corridor_element(&doc, id).unwrap().definition.interval_m, 20.0);

    // The profile is lowered to 98 behind the corridor's back: the summary is stale until refreshed.
    let mut a: afrobim_alignment::Alignment = serde_json::from_value(doc.find(alignment).unwrap().data.clone()).unwrap();
    a.profile = Some(Profile::new("FG", vec![Pvi::point(0.0, 98.0), Pvi::point(100.0, 98.0)]).unwrap());
    doc.find_mut(alignment).unwrap().data = serde_json::to_value(&a).unwrap();
    close(corridor_element(&doc, id).unwrap().computed.unwrap().total_fill_m3, 3600.0, "stale");
    let cut: Corridor = refresh_corridor(&mut doc, id).unwrap();
    // 14 × 2 + 2 × ½ × 2 × 2 = 32 m² of cut.
    close(cut.sections[3].cut_area_m2.unwrap(), 32.0, "now in cut");
    let summary = corridor_element(&doc, id).unwrap().computed.unwrap();
    close(summary.total_cut_m3, 3200.0, "refreshed cut");
    close(summary.total_fill_m3, 0.0, "refreshed fill");

    // Wrong references are refused by name.
    let err = add_corridor(&mut doc, surface, surface, CorridorDefinition::single("X", template(5.0), 20.0)).unwrap_err();
    assert_eq!(err.to_string(), format!("element {} is not an alignment in the document", surface.0));
    let err = add_corridor(&mut doc, alignment, alignment, CorridorDefinition::single("X", template(5.0), 20.0)).unwrap_err();
    assert_eq!(err.to_string(), format!("element {} is not a surface in the document", alignment.0));
    assert_eq!(doc.elements().len(), 3, "nothing was added");

    // An element with a definition but no summary yet says so; a bad definition is an Error note.
    let mut e = doc.find(id).unwrap().clone();
    e.data["computed"] = serde_json::Value::Null;
    e.data["definition"]["interval_m"] = serde_json::json!(0.0);
    let g = CorridorModule.generate(&e).unwrap();
    assert_eq!(g.notes.iter().map(|n| (n.severity, n.code.as_str())).collect::<Vec<_>>(), [(Severity::Error, "corridor.definition"), (Severity::Info, "corridor.not_computed")]);
    assert!(g.drawing.primitives.is_empty());
}

#[test]
fn landxml_surfaces_become_elements() {
    let mut doc = Document::default();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("landxml").join("surface.xml");
    let (ids, notes) = import_landxml_surfaces(&mut doc, &path).unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(notes.len(), 3, "{notes:?}");
    let e = doc.find(ids[0]).unwrap();
    assert_eq!(e.name, "EG");
    assert_eq!(e.data["source"], "surface.xml");
    assert_eq!(surface_of(&doc, ids[0]).unwrap().elevation_at(5010.0, 1010.0), Some(1205.0));
    assert!(import_landxml_surfaces(&mut doc, &path.with_file_name("missing.xml")).is_err());
}

/// Timing on a synthetic survey: 200 000 triangles and 500 sections. Not run by default:
/// `cargo test -p afrobim-corridor --release --test module -- --ignored --nocapture`.
#[test]
#[ignore]
fn timing_on_a_large_surface() {
    use std::time::Instant;
    // 5 km × 1 km of rolling ground on a 10 m × 5 m grid: 501 × 201 points, 200 000 triangles.
    let t = Instant::now();
    let tin = Tin::from_grid(0.0, 0.0, 10.0, 5.0, 501, 201, |x, y| 1200.0 + 8.0 * (x / 300.0).sin() + 5.0 * (y / 120.0).cos() + 0.004 * x).unwrap();
    println!("built {} triangles in {:?}", tin.triangles().len(), t.elapsed());
    let mut a = AlignmentBuilder::new("Long", 0.0, 10.0, 400.0, 0.03).line(4995.0).build().unwrap();
    a.profile = Some(Profile::new("FG", vec![Pvi::point(0.0, 1203.0), Pvi::parabola(2500.0, 1215.0, 400.0), Pvi::point(4995.0, 1222.0)]).unwrap());
    let links = vec![Link::new("lane", 3.5, Slope::Percent(-2.5)).paved(), Link::new("shoulder", 1.5, Slope::Percent(-4.0)).paved(), Link::new("verge", 1.0, Slope::Ratio { run: 4.0, rise: -1.0 })];
    let t_def = Template::symmetric("timing", links, Daylight { cut_run_per_rise: 1.5, fill_run_per_rise: 2.0 }, 0.4);
    let definition = CorridorDefinition::single("timing", t_def, 10.0);

    let t = Instant::now();
    let first = tin.elevation_at(2500.0, 500.0).unwrap();
    println!("spatial index and first point query in {:?} ({first:.3})", t.elapsed());
    let t = Instant::now();
    let mut sum = 0.0;
    for i in 0..100_000 {
        sum += tin.elevation_at(1.0 + (i as f64 * 37.7) % 4998.0, 1.0 + (i as f64 * 11.3) % 998.0).unwrap();
    }
    println!("100 000 point queries in {:?} (mean level {:.3})", t.elapsed(), sum / 100_000.0);

    let t = Instant::now();
    let corridor = Corridor::build(&definition, &a, &tin).unwrap();
    let built = t.elapsed();
    let t = Instant::now();
    let e = earthworks(&corridor.sections, &VolumeOptions::default());
    println!(
        "{} sections in {:?}; volumes in {:?}; cut {:.0} m3, fill {:.0} m3; {} without areas",
        corridor.sections.len(),
        built,
        t.elapsed(),
        e.total_cut_m3,
        e.total_fill_m3,
        corridor.sections.iter().filter(|s| s.cut_area_m2.is_none()).count()
    );
    assert!(corridor.sections.len() >= 500);

    let t = Instant::now();
    let json = serde_json::to_string(&tin).unwrap();
    let encoded = t.elapsed();
    let t = Instant::now();
    let back: Tin = serde_json::from_str(&json).unwrap();
    println!("stored form {:.1} MB, written in {:?}, read back in {:?}", json.len() as f64 / 1.0e6, encoded, t.elapsed());
    assert_eq!(back, tin);
    let t = Instant::now();
    let (mesh, resampled) = surface_display_mesh(&tin, afrobim_corridor::DISPLAY_TRIANGLE_CAP);
    println!("display mesh of {} triangles (resampled: {resampled}) in {:?}", mesh.triangles.len(), t.elapsed());
}
