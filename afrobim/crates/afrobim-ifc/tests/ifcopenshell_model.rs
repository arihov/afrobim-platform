//! Validation of the model export by an independent IFC toolkit.
//!
//! Exports the site fixture, writes what the model says next to it as JSON, and runs
//! `tests/check_model_ifc.py`, which opens the file with IfcOpenShell, validates it against the
//! schema, reads back every placement and lets IfcOpenShell's geometry kernel build every solid.
//! As in `tests/ifcopenshell.rs`, a missing `python3` or `ifcopenshell` makes the test print why
//! and pass, unless `AFROBIM_REQUIRE_IFCOPENSHELL` is set.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use afrobim_alignment::ElementKind;
use afrobim_ifc::{element_global_id, global_id, write_ifc_model};
use serde_json::{json, Value};

use common::site::{expected_assemblies, expected_products, site_fixture, DESIGN_BASIS};
use common::{options, python_with_ifcopenshell};

fn expectations() -> Value {
    let f = site_fixture();
    let project = options().project_name;
    let guid = |name: &str| element_global_id(&project, f.id(name));
    let products: Vec<Value> = expected_products(&f)
        .into_iter()
        .map(|mut p| {
            let name = p["name"].as_str().unwrap().to_string();
            p["global_id"] = json!(guid(&name));
            p
        })
        .collect();
    let assemblies: Vec<Value> = expected_assemblies(&f)
        .into_iter()
        .map(|(sign, pose, parts)| {
            json!({
                "sign": sign,
                "global_id": global_id(&format!("{project}/element/{}/assembly", f.id(sign).0)),
                "pose": pose,
                "parts": parts,
            })
        })
        .collect();
    let a = &f.alignment;
    let segments: Vec<Value> = a
        .elements
        .iter()
        .map(|e| {
            json!({
                "x_m": e.x, "y_m": e.y, "heading_rad": e.heading_rad, "length_m": e.length,
                "curvature_start": e.curvature_start, "curvature_end": e.curvature_end,
                "type": match e.kind() { ElementKind::Line => "LINE", ElementKind::Arc => "CIRCULARARC", ElementKind::Spiral => "CLOTHOID" },
            })
        })
        .collect();
    let sign = f.document.find(f.id("ADS-101")).unwrap().site.clone().unwrap();
    json!({
        "project_name": project,
        "products": products,
        "assemblies": assemblies,
        "design_basis": DESIGN_BASIS,
        "placed_sign": { "name": "ADS-101", "x": sign.x, "y": sign.y, "z": sign.z, "rotation_deg": sign.rotation_deg, "station_m": 1250.0, "offset_m": 4.5, "alignment": "MC01" },
        "alignment": {
            "global_id": guid("MC01"),
            "name": "MC01",
            "start_station_m": a.start_station_m,
            "length_m": a.length(),
            "segments": segments,
            "positions": ["ADS-101", "MKR-106"],
        },
        "counts": {
            "IfcSign": 6, "IfcMember": 8, "IfcFooting": 6, "IfcElementAssembly": 5, "IfcBuildingElementProxy": 1,
            "IfcRoad": 1, "IfcSite": 1, "IfcAlignment": 1, "IfcAlignmentHorizontal": 1, "IfcAlignmentSegment": 8,
            "IfcReferent": 1, "IfcExtrudedAreaSolid": 14, "IfcMaterial": 2, "IfcRelAssociatesMaterial": 2,
            "IfcRelContainedInSpatialStructure": 1, "IfcRelAggregates": 8, "IfcRelNests": 3, "IfcRelPositions": 1
        }
    })
}

/// Runs `check_model_ifc.py` on `ifc` (and `expected`, when given); skips when there is no toolkit.
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
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("check_model_ifc.py");
    let mut command = Command::new(python);
    command.arg(&script).arg(ifc);
    if let Some(expected) = expected {
        command.arg(expected);
    }
    let out = command.output().expect("run check_model_ifc.py");
    let stdout = String::from_utf8_lossy(&out.stdout);
    println!("{stdout}");
    assert!(out.status.success(), "check_model_ifc.py failed on {}\n{stdout}\n{}", ifc.display(), String::from_utf8_lossy(&out.stderr));
}

#[test]
fn ifcopenshell_validates_the_model_export() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ifc = dir.join("afrobim-ifc-site-fixture.ifc");
    let expected = dir.join("afrobim-ifc-site-fixture.expected.json");
    let f = site_fixture();
    std::fs::write(&ifc, write_ifc_model(&f.document, &f.inputs(), &options())).unwrap();
    std::fs::write(&expected, serde_json::to_string_pretty(&expectations()).unwrap()).unwrap();
    run_check(&ifc, Some(&expected));
}

/// The same model before anything is placed on site and without the alignment: supports under
/// their signs on the upright sheet, assemblies contained in the site, no road.
#[test]
fn ifcopenshell_validates_a_model_that_is_not_on_site() {
    let mut f = site_fixture();
    let ids: Vec<_> = f.document.elements().iter().map(|e| e.id).collect();
    for id in ids {
        if f.document.find(id).unwrap().category == "alignment" {
            f.document.remove(id);
        } else {
            f.document.find_mut(id).unwrap().site = None;
        }
    }
    let text = write_ifc_model(&f.document, &f.inputs(), &options());
    assert!(!text.contains("IFCROAD") && !text.contains("IFCALIGNMENT") && text.contains("IFCELEMENTASSEMBLY"));
    let ifc = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("afrobim-ifc-sheet-supports.ifc");
    std::fs::write(&ifc, text).unwrap();
    run_check(&ifc, None);
}
