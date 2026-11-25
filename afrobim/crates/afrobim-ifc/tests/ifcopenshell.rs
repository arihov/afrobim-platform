// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Validation by an independent IFC toolkit.
//!
//! Exports the hand-built document, writes what the document says next to it as JSON, and runs
//! `tests/check_ifc.py`, which opens the file with IfcOpenShell, validates it against the schema
//! and compares every element. IfcOpenShell is a Python package (`pip install ifcopenshell`); when
//! `python3` or the package is missing the test prints why and passes, unless
//! `AFROBIM_REQUIRE_IFCOPENSHELL` is set, which turns a missing toolkit into a failure.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use afrobim_draw::Colour;
use afrobim_ifc::{element_global_id, global_id, write_ifc};
use serde_json::{json, Value};

use common::{fixture, options, OFFSET, THICKNESS};

/// A Python interpreter that can import ifcopenshell, if there is one.
fn python_with_ifcopenshell() -> Result<&'static str, String> {
    let mut problems = Vec::new();
    for python in ["python3", "python"] {
        match Command::new(python).args(["-c", "import ifcopenshell, ifcopenshell.validate"]).output() {
            Ok(out) if out.status.success() => return Ok(python),
            Ok(out) => problems.push(format!("{python}: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("cannot import ifcopenshell"))),
            Err(error) => problems.push(format!("{python}: {error}")),
        }
    }
    Err(problems.join("; "))
}

/// What the document says about each element, independent of the exporter's code.
fn expectations() -> Value {
    let f = fixture();
    let project = options().project_name;
    let standard = "uganda-tsm-2004";
    let groups = |i: usize| -> Vec<Value> {
        f.groups[i]
            .iter()
            .map(|g| json!({ "colour": format!("{:?}", g.colour), "rgb": g.colour.display_rgb(), "layer": g.layer, "area": g.area }))
            .collect()
    };
    let sign_qto = |w: f64, h: f64| json!({ "Width": w, "Height": h, "Thickness": THICKNESS });
    let base_qto = |w: f64, h: f64| json!({ "Width": w, "Height": h, "GrossArea": w * h / 1e6 });
    json!({
        "project_name": project,
        "project_global_id": global_id(&format!("{project}/project")),
        "thickness": THICKNESS,
        "offset": OFFSET,
        "plate_rgb": Colour::Grey.display_rgb(),
        "counts": {
            "IfcSign": 3,
            "IfcBuildingElementProxy": 1,
            "IfcPropertySet": 9,
            "IfcElementQuantity": 7,
            "IfcRelDefinesByProperties": 16,
            "IfcRelContainedInSpatialStructure": 1,
            "IfcRelAggregates": 1,
            "IfcOwnerHistory": 0
        },
        "elements": [
            {
                "global_id": element_global_id(&project, f.ids[0]),
                "class": "IfcSign",
                "name": "ADS-001",
                "tag": f.ids[0].0.to_string(),
                "object_type": "StackType",
                "predefined_type": "PICTORAL",
                "description": "kind StackType, catalog key GFS-B1, design speed 80 km/h",
                "origin": [0.0, 0.0],
                "width": 1200.0,
                "height": 800.0,
                "groups": groups(0),
                "psets": {
                    "AfroBIM_Sign": {
                        "Mark": "ADS-001", "Category": "sign", "Layer": "Signs", "Standard": standard,
                        "Kind": "StackType", "CatalogKey": "GFS-B1", "DesignSpeedKph": 80
                    },
                    "Pset_SignCommon": { "Reference": "GFS-B1", "IsExternal": true },
                    "AfroBIM_Properties": {
                        "Material": "Aluminium 3 mm",
                        "Drawing ref": "O'Brien / Zürich №7 \\ rev B",
                        "Remarks": "x".repeat(300)
                    },
                    "AfroBIM_Derived": { "x_height_mm": "100", "area_m2": "0.96" },
                    "Qto_SignBaseQuantities": sign_qto(1200.0, 800.0),
                    "AfroBIM_BaseQuantities": base_qto(1200.0, 800.0)
                }
            },
            {
                "global_id": element_global_id(&project, f.ids[1]),
                "class": "IfcSign",
                "name": "REG-002 'Yield'",
                "tag": f.ids[1].0.to_string(),
                "object_type": "Regulatory",
                "predefined_type": "PICTORAL",
                "description": "kind Regulatory",
                "origin": [1700.0, 0.0],
                "width": 600.0,
                "height": 600.0,
                "groups": groups(1),
                "psets": {
                    "AfroBIM_Sign": { "Mark": "REG-002 'Yield'", "Category": "sign", "Layer": "Signs", "Standard": standard, "Kind": "Regulatory" },
                    "Pset_SignCommon": { "IsExternal": true },
                    "Qto_SignBaseQuantities": sign_qto(600.0, 600.0),
                    "AfroBIM_BaseQuantities": base_qto(600.0, 600.0)
                }
            },
            {
                "global_id": element_global_id(&project, f.ids[2]),
                "class": "IfcSign",
                "name": "BARE-003",
                "tag": f.ids[2].0.to_string(),
                "object_type": null,
                "predefined_type": "PICTORAL",
                "description": null,
                "origin": [0.0, 1300.0],
                "width": 900.0,
                "height": 300.0,
                "groups": groups(2),
                "psets": {
                    "AfroBIM_Sign": { "Mark": "BARE-003", "Category": "sign", "Layer": "0", "Standard": standard },
                    "Pset_SignCommon": { "IsExternal": true },
                    "Qto_SignBaseQuantities": sign_qto(900.0, 300.0),
                    "AfroBIM_BaseQuantities": base_qto(900.0, 300.0)
                }
            },
            {
                "global_id": element_global_id(&project, f.ids[3]),
                "class": "IfcBuildingElementProxy",
                "name": "MK-004",
                "tag": f.ids[3].0.to_string(),
                "object_type": "marking",
                "predefined_type": "USERDEFINED",
                "description": "kind Arrow",
                "origin": [2800.0, 500.0],
                "width": 400.0,
                "height": 200.0,
                "groups": groups(3),
                "psets": {
                    "AfroBIM_Element": { "Mark": "MK-004", "Category": "marking", "Layer": "0", "Standard": standard, "Kind": "Arrow" },
                    "AfroBIM_BaseQuantities": base_qto(400.0, 200.0)
                }
            }
        ]
    })
}

#[test]
fn ifcopenshell_validates_the_export() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ifc = dir.join("afrobim-ifc-fixture.ifc");
    let expected = dir.join("afrobim-ifc-fixture.expected.json");
    let f = fixture();
    std::fs::write(&ifc, write_ifc(&f.document, &f.inputs(), &options())).unwrap();
    std::fs::write(&expected, serde_json::to_string_pretty(&expectations()).unwrap()).unwrap();

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
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("check_ifc.py");
    let out = Command::new(python).arg(&script).arg(&ifc).arg(&expected).output().expect("run check_ifc.py");
    let stdout = String::from_utf8_lossy(&out.stdout);
    println!("{stdout}");
    assert!(out.status.success(), "check_ifc.py failed on {}\n{stdout}\n{}", ifc.display(), String::from_utf8_lossy(&out.stderr));
}
