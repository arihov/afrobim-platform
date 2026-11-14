//! Validation of the road modules' export by an independent IFC toolkit.
//!
//! Exports the document of `tests/common/roads.rs` (an element of every module), writes what the
//! modules produced next to it as JSON, and runs `tests/check_roads_ifc.py`, which opens the file
//! with IfcOpenShell, validates it against the IFC4X3_ADD2 schema and EXPRESS rules, checks every
//! standard property and quantity set against the IFC4X3 templates, lets the geometry iterator
//! build every shape and compares its bounding boxes with the modules' solids. As in
//! `tests/ifcopenshell_model.rs`, a missing `python3` or `ifcopenshell` makes the test print why
//! and pass, unless `AFROBIM_REQUIRE_IFCOPENSHELL` is set. The file is also left in
//! `target/tmp` for anyone who wants to open it.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use afrobim_draw::Mesh3;
use afrobim_ifc::write_ifc_model;
use serde_json::{json, Value};

use common::roads::{road_fixture, RoadFixture};
use common::{options, python_with_ifcopenshell};

fn bounds(meshes: &[&Mesh3], origin: [f64; 3], turn_deg: f64) -> ([f64; 3], [f64; 3]) {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    let (sin, cos) = turn_deg.to_radians().sin_cos();
    for m in meshes {
        for v in &m.vertices {
            let w = [origin[0] + v[0] * cos - v[1] * sin, origin[1] + v[0] * sin + v[1] * cos, origin[2] + v[2]];
            for k in 0..3 {
                lo[k] = lo[k].min(w[k]);
                hi[k] = hi[k].max(w[k]);
            }
        }
    }
    (lo, hi)
}

fn expectations(f: &RoadFixture) -> Value {
    let site = |name: &str| {
        let s = f.document.find(f.id(name)).unwrap().site.clone().unwrap();
        ([s.x, s.y, s.z], s.rotation_deg)
    };
    let solids = |name: &str| -> Vec<&Mesh3> { f.generated[&f.id(name)].solids.iter().collect() };
    let triangles = |meshes: &[&Mesh3]| meshes.iter().map(|m| m.triangles.len()).sum::<usize>();
    let group = |label: &str, roots: Vec<(&str, String)>, parts: bool, meshes: Vec<&Mesh3>, origin: [f64; 3], turn: f64| {
        let (lo, hi) = bounds(&meshes, origin, turn);
        json!({ "label": label, "roots": roots.iter().map(|(k, n)| json!([k, n])).collect::<Vec<_>>(), "parts": parts, "lo": lo, "hi": hi, "triangles": triangles(&meshes) })
    };
    let mut groups = Vec::new();
    let mut world = Vec::new();
    let mut single = |kind: &str, name: &str, groups: &mut Vec<Value>| {
        let (at, turn) = site(name);
        world.push(json!({ "class": kind, "name": name, "pose": [at[0], at[1], at[2], turn] }));
        groups.push(group(name, vec![(kind, name.to_string())], false, solids(name), at, turn));
    };
    for name in ["RM-001", "RM-002"] {
        single("IfcSurfaceFeature", name, &mut groups);
    }
    single("IfcBuildingElementProxy", "RM-003", &mut groups);
    single("IfcPipeSegment", "SD-001", &mut groups);
    single("IfcPipeSegment", "CU-001", &mut groups);
    single("IfcRailing", "VRS-001", &mut groups);
    single("IfcGeographicElement", "Ground", &mut groups);
    // The twin culvert: both barrels together.
    let (at, turn) = site("CU-002");
    groups.push(group("CU-002", vec![("IfcPipeSegment", "CU-002, barrel 1".into()), ("IfcPipeSegment", "CU-002, barrel 2".into())], false, solids("CU-002"), at, turn));
    // Gantries: legs, foundations and boom.
    let mut aggregates = Vec::new();
    for (name, legs) in [("G-001", 2), ("G-002", 1)] {
        let (at, turn) = site(name);
        world.push(json!({ "class": "IfcElementAssembly", "name": name, "pose": [at[0], at[1], at[2], turn] }));
        let mut parts = Vec::new();
        let mut roots = Vec::new();
        for i in 1..=legs {
            parts.push(format!("{name}, leg {i}"));
            parts.push(format!("{name}, foundation {i}"));
            roots.push(("IfcColumn", format!("{name}, leg {i}")));
            roots.push(("IfcFooting", format!("{name}, foundation {i}")));
        }
        parts.push(format!("{name}, boom"));
        roots.push(("IfcBeam", format!("{name}, boom")));
        if name == "G-001" {
            parts.push("GS-001".to_string());
        }
        groups.push(group(name, roots, false, solids(name), at, turn));
        aggregates.push(json!({ "class": "IfcElementAssembly", "name": name, "parts": parts }));
    }
    let (at, turn) = site("GS-001");
    world.push(json!({ "class": "IfcSign", "name": "GS-001", "pose": [at[0], at[1], at[2], turn] }));
    // The pavement and its slabs.
    let (at, turn) = site("PV-001");
    world.push(json!({ "class": "IfcPavement", "name": "PV-001", "pose": [at[0], at[1], at[2], turn] }));
    groups.push(group("PV-001", vec![("IfcPavement", "PV-001".into())], true, solids("PV-001"), at, turn));
    aggregates.push(json!({ "class": "IfcPavement", "name": "PV-001", "parts": ["PV-001, Carriageway: Surfacing", "PV-001, Carriageway: Base"] }));
    aggregates.push(json!({ "class": "IfcRailing", "name": "VRS-001", "parts": ["VRS-001, start terminal", "VRS-001, end transition"] }));
    // Lighting: every part of every mast is all of the element's solids.
    let mut positions = vec!["RM-001", "SD-001", "CU-001", "G-001", "VRS-001", "PV-001"].into_iter().map(String::from).collect::<Vec<_>>();
    let mut masts_all = Vec::new();
    for name in ["LZ-01", "LC-900"] {
        let data = afrobim_lighting::LightingData::from_value(&f.document.find(f.id(name)).unwrap().data).unwrap();
        let masts: Vec<String> = data.layout.columns.iter().map(|c| c.mark.clone()).collect();
        for c in &data.layout.columns {
            world.push(json!({ "class": "IfcElementAssembly", "name": c.mark, "pose": [c.easting_m * 1000.0, c.northing_m * 1000.0, c.level_m.unwrap_or(0.0) * 1000.0, 0.0] }));
        }
        let (at, turn) = site(name);
        groups.push(group(name, masts.iter().map(|m| ("IfcElementAssembly", m.clone())).collect(), true, solids(name), at, turn));
        positions.extend(masts.iter().cloned());
        masts_all.push((name, masts));
    }
    // The corridor, built the way the 3D view builds it.
    let built = afrobim_corridor::corridor_of(&f.document, f.id("Test corridor")).unwrap();
    let m = afrobim_corridor::corridor_meshes(&built);
    let surfaces: Vec<&Mesh3> = [&m.top, &m.cut_slopes, &m.fill_slopes].into_iter().filter(|m| !m.triangles.is_empty()).collect();
    let origin = [m.origin_x_m * 1000.0, m.origin_y_m * 1000.0, 0.0];
    world.push(json!({ "class": "IfcBuildingElementProxy", "name": "Test corridor", "pose": [origin[0], origin[1], 0.0, 0.0] }));
    groups.push(group("Test corridor", vec![("IfcBuildingElementProxy", "Test corridor".into())], false, surfaces, origin, 0.0));

    let gantry = afrobim_gantry::GantryDefinition::from_value(&f.document.find(f.id("G-001")).unwrap().data).unwrap();
    let foundation = gantry.legs[0].foundation.unwrap().volume_m3();
    let mut lighting_groups = vec![json!({ "name": "LZ-01", "members": masts_all[0].1 })];
    lighting_groups.push(json!({ "name": "CU-002", "members": ["CU-002, barrel 1", "CU-002, barrel 2"] }));
    json!({
        "counts": {
            "IfcRoad": 1, "IfcSite": 1, "IfcAlignment": 1, "IfcSurfaceFeature": 2, "IfcPipeSegment": 4, "IfcElementAssembly": 8, "IfcColumn": 3, "IfcBeam": 2,
            "IfcFooting": 3, "IfcSign": 1, "IfcMember": 13, "IfcLightFixture": 7, "IfcRailing": 3, "IfcPavement": 1, "IfcCourse": 2,
            "IfcBuildingElementProxy": 2, "IfcGeographicElement": 1, "IfcGroup": 2, "IfcRelAdheresToElement": 1, "IfcRelAssignsToGroup": 2,
        },
        "world": world,
        "groups": groups,
        "marking_thickness_mm": afrobim_markings::DISPLAY_THICKNESS_MM,
        "markings_with_volume": ["RM-001", "RM-002"],
        "foundation_volumes": [["G-001, foundation 1", foundation], ["G-001, foundation 2", foundation]],
        "course_volumes": [["PV-001, Carriageway: Surfacing", 0.01], ["PV-001, Carriageway: Base", 0.01]],
        "adheres": { "pavement": "PV-001", "markings": ["RM-001", "RM-002"], "contained": ["RM-003"] },
        "groups_ifc": lighting_groups,
        "aggregates": aggregates,
        "positions": positions,
    })
}

/// Runs `check_roads_ifc.py` on `ifc` (and `expected`, when given); skips when there is no toolkit.
fn run_check(ifc: &Path, expected: Option<&Path>) {
    let python = match python_with_ifcopenshell() {
        Ok(python) => python,
        Err(why) => {
            let message = format!("SKIPPED: IfcOpenShell validation not run, no Python with ifcopenshell ({why}). Install it with `pip install ifcopenshell`.");
            assert!(std::env::var_os("AFROBIM_REQUIRE_IFCOPENSHELL").is_none(), "{message}");
            println!("{message}");
            eprintln!("{message}");
            return;
        }
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("check_roads_ifc.py");
    let mut command = Command::new(python);
    command.arg(&script).arg(ifc);
    if let Some(expected) = expected {
        command.arg(expected);
    }
    let out = command.output().expect("run check_roads_ifc.py");
    let stdout = String::from_utf8_lossy(&out.stdout);
    println!("{stdout}");
    assert!(out.status.success(), "check_roads_ifc.py failed on {}\n{stdout}\n{}", ifc.display(), String::from_utf8_lossy(&out.stderr));
}

#[test]
fn ifcopenshell_validates_the_road_modules_export() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ifc = dir.join("afrobim-ifc-roads.ifc");
    let expected = dir.join("afrobim-ifc-roads.expected.json");
    let f = road_fixture();
    std::fs::write(&ifc, write_ifc_model(&f.document, &f.inputs(), &options())).unwrap();
    std::fs::write(&expected, serde_json::to_string_pretty(&expectations(&f)).unwrap()).unwrap();
    run_check(&ifc, Some(&expected));
}

/// The same document without its pavement and corridor: no pavement carries a marking, so the
/// markings are proxies (an `IfcSurfaceFeature` must adhere to an element), and the rest is
/// unchanged. The generic checks must still hold.
#[test]
fn ifcopenshell_validates_a_road_document_without_a_pavement() {
    let mut f = road_fixture();
    for name in ["PV-001", "Test corridor"] {
        let id = f.id(name);
        f.document.remove(id);
        f.generated.remove(&id);
    }
    let text = write_ifc_model(&f.document, &f.inputs(), &options());
    assert!(!text.contains("IFCSURFACEFEATURE") && !text.contains("IFCPAVEMENT") && text.contains("IFCBUILDINGELEMENTPROXY"));
    let ifc = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("afrobim-ifc-roads-no-pavement.ifc");
    std::fs::write(&ifc, text).unwrap();
    run_check(&ifc, None);
}
