// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Verification of the IFC importer against IfcOpenShell's geometry iterator.
//!
//! For each sample file (directory in `IFC_SAMPLES_DIR`, default `/home/user/ifcsamples/files`) and
//! for the crate's own exported road model, the importer's products are written to JSON and
//! `tests/check_import.py` compares them with what IfcOpenShell builds: product counts by type,
//! the world bounding box of each product's geometry, and which products each side has geometry
//! for. The test prints a coverage table. It passes without IfcOpenShell (printing why) unless
//! `AFROBIM_REQUIRE_IFCOPENSHELL` is set.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use afrobim_ifc::import::*;
use afrobim_ifc::write_ifc_model;
use serde_json::{json, Value};

use common::cases::cases;
use common::roads::road_fixture;
use common::{options, python_with_ifcopenshell};

/// Bounding box deviation allowed, in mm, besides a relative part of the extent: circles are
/// polygons (32 segments, inscribed) here and exact in IfcOpenShell. A circle of radius r misses
/// the true bounding box by at most r (1 - cos(pi / 32)) = 0.48 % of r on each side.
const ABS_TOL_MM: f64 = 0.5;
const REL_TOL: f64 = 0.005;

fn volume(m: &afrobim_draw::Mesh3) -> f64 {
    m.triangles
        .iter()
        .map(|t| {
            let (a, b, c) = (m.vertices[t[0] as usize], m.vertices[t[1] as usize], m.vertices[t[2] as usize]);
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
        })
        .sum()
}

fn dump(model: &ReferenceModel) -> Value {
    Value::Array(
        model
            .products
            .iter()
            .map(|p| {
                json!({
                    "step_id": p.step_id,
                    "global_id": p.global_id,
                    "entity": p.entity,
                    "status": format!("{:?}", p.geometry),
                    "bounds": p.bounds.map(|(a, b)| json!([a, b])),
                    "triangles": p.meshes.iter().map(|m| m.triangles.len()).sum::<usize>(),
                    "volume_mm3": if p.meshes.is_empty() { Value::Null } else { json!(p.meshes.iter().map(volume).sum::<f64>()) },
                    "unread": p.unread_items,
                })
            })
            .collect(),
    )
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("afrobim-import-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

struct Outcome {
    label: String,
    schema: String,
    products_ours: usize,
    products_theirs: usize,
    counts_equal: bool,
    compared: usize,
    within: usize,
    outside: Vec<String>,
    theirs_only: BTreeMap<String, usize>,
    ours_only: usize,
    read_status: BTreeMap<String, usize>,
    volume_off: Vec<String>,
}

fn compare(python: &str, label: &str, path: &Path, model: &ReferenceModel, map: bool) -> Outcome {
    let json_path = scratch(&format!("{label}.json"));
    std::fs::write(&json_path, serde_json::to_vec(&dump(model)).unwrap()).unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/check_import.py");
    let result_path = scratch(&format!("{label}.result.json"));
    let _ = std::fs::remove_file(&result_path);
    let mut cmd = Command::new(python);
    cmd.arg("-I").arg(&script).arg(path).arg(&json_path).arg(&result_path);
    if map {
        cmd.arg("--map");
    }
    let out = cmd.output().expect("python runs");
    assert!(out.status.success(), "{label}: check_import.py failed: {}", String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&std::fs::read(&result_path).expect("result file")).unwrap_or_else(|e| panic!("{label}: bad result {e}"));
    // Product counts by type: every IfcOpenShell product type must have the same count here.
    let theirs = v["products_ifcopenshell"].as_object().unwrap();
    let mut ours: BTreeMap<String, usize> = BTreeMap::new();
    for p in &model.products {
        *ours.entry(p.entity.to_uppercase()).or_default() += 1;
    }
    let theirs_up: BTreeMap<String, usize> = theirs.iter().map(|(k, n)| (k.to_uppercase(), n.as_u64().unwrap() as usize)).collect();
    let counts_equal = ours == theirs_up;
    if !counts_equal {
        eprintln!("{label}: product counts differ\n  ours   {ours:?}\n  theirs {theirs_up:?}");
    }
    let mut within = 0;
    let mut outside = Vec::new();
    let mut volume_off = Vec::new();
    let compared = v["compared"].as_array().unwrap();
    let mut read_status: BTreeMap<String, usize> = BTreeMap::new();
    for c in compared {
        let dev = c["max_bbox_deviation_mm"].as_f64().unwrap();
        let ext = c["extent_mm"].as_f64().unwrap();
        let status = c["status"].as_str().unwrap().to_string();
        if status == "Read" {
            *read_status.entry("Read".into()).or_default() += 1;
        } else {
            *read_status.entry(status.clone()).or_default() += 1;
        }
        // Only fully read products are held to the tolerance; partial ones and placeholders are
        // counted separately.
        if status == "Read" {
            if dev <= ABS_TOL_MM + REL_TOL * ext {
                within += 1;
            } else {
                outside.push(format!("#{} {} dev {:.3} mm extent {:.0} mm", c["step_id"], c["entity"].as_str().unwrap(), dev, ext));
            }
            if let Some(r) = c["volume_ratio"].as_f64() {
                // Volumes: polygonal circles are inscribed, so a little under 1.
                if !(0.98..=1.02).contains(&r) {
                    volume_off.push(format!("#{} {} volume ratio {:.4}", c["step_id"], c["entity"].as_str().unwrap(), r));
                }
            }
        }
    }
    let mut theirs_only: BTreeMap<String, usize> = BTreeMap::new();
    for t in v["theirs_only"].as_array().unwrap() {
        let why = t["ours_unread"].as_array().unwrap().iter().filter_map(|s| s.as_str()).collect::<Vec<_>>().join("+");
        let key = format!("{} [{}{}]", t["entity"].as_str().unwrap(), t["ours_status"].as_str().unwrap(), if why.is_empty() { String::new() } else { format!(": {why}") });
        *theirs_only.entry(key).or_default() += 1;
    }
    Outcome {
        label: label.to_string(),
        schema: v["schema"].as_str().unwrap().to_string(),
        products_ours: model.products.len(),
        products_theirs: theirs_up.values().sum(),
        counts_equal,
        compared: compared.len(),
        within,
        outside,
        theirs_only,
        ours_only: v["ours_only"].as_array().unwrap().len(),
        read_status,
        volume_off,
    }
}

fn report(o: &Outcome) {
    eprintln!(
        "{:<44} {:<11} products {:>4}/{:<4} counts_equal {:<5} bbox compared {:>4} (full reads within tol {:>4}, outside {:>2}) ours_only {} theirs_only {}",
        o.label,
        o.schema,
        o.products_ours,
        o.products_theirs,
        o.counts_equal,
        o.compared,
        o.within,
        o.outside.len(),
        o.ours_only,
        o.theirs_only.values().sum::<usize>()
    );
    for x in &o.outside {
        eprintln!("    OUTSIDE {x}");
    }
    for x in &o.volume_off {
        eprintln!("    VOLUME {x}");
    }
    if !o.theirs_only.is_empty() {
        eprintln!("    geometry only in IfcOpenShell: {:?}", o.theirs_only);
    }
    if o.read_status.keys().any(|k| k != "Read") {
        eprintln!("    status of compared products: {:?}", o.read_status);
    }
}

fn sample_files() -> Vec<PathBuf> {
    let d = PathBuf::from(std::env::var("IFC_SAMPLES_DIR").unwrap_or_else(|_| "/home/user/ifcsamples/files".into()));
    let Ok(rd) = std::fs::read_dir(&d) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "ifc")).collect();
    v.sort();
    v
}

#[test]
fn the_importer_agrees_with_ifcopenshell_on_the_sample_files_and_the_own_export() {
    let python = match python_with_ifcopenshell() {
        Ok(p) => p,
        Err(why) => {
            assert!(std::env::var("AFROBIM_REQUIRE_IFCOPENSHELL").is_err(), "IfcOpenShell required but missing: {why}");
            eprintln!("SKIPPED: {why}");
            return;
        }
    };
    let mut outcomes = Vec::new();
    // The crate's own exported road model.
    let f = road_fixture();
    let own = write_ifc_model(&f.document, &f.inputs(), &options());
    let own_path = scratch("own-road-model.ifc");
    std::fs::write(&own_path, &own).unwrap();
    let model = import_ifc(&own_path, &ImportOptions::default()).expect("own export imports");
    outcomes.push(compare(python, "own-road-model", &own_path, &model, false));
    // The hand-written cases: slanted and mapped extrusions, holes, reversed bounds, deep chains.
    let synth_path = scratch("synthetic-cases.ifc");
    std::fs::write(&synth_path, cases(false)).unwrap();
    let model = import_ifc(&synth_path, &ImportOptions::default()).expect("cases import");
    outcomes.push(compare(python, "synthetic-cases", &synth_path, &model, false));
    // Independent files.
    for path in sample_files() {
        let label = path.file_name().unwrap().to_string_lossy().to_string();
        let model = match import_ifc(&path, &ImportOptions::default()) {
            Ok(m) => m,
            Err(e) => panic!("{label}: {e}"),
        };
        outcomes.push(compare(python, &label, &path, &model, false));
        if label.contains("mapconversion") {
            // The same file with IfcMapConversion applied, against IfcOpenShell's helmert transformation.
            let applied = import_ifc(&path, &ImportOptions { apply_map_conversion: true, ..ImportOptions::default() }).unwrap();
            outcomes.push(compare(python, &format!("{label} (map applied)"), &path, &applied, true));
        }
    }
    for o in &outcomes {
        report(o);
    }
    let total_compared: usize = outcomes.iter().map(|o| o.compared).sum();
    let total_within: usize = outcomes.iter().map(|o| o.within).sum();
    eprintln!("TOTAL: {} files, {} products compared by bounding box, {} fully read ones within tolerance", outcomes.len(), total_compared, total_within);
    assert!(outcomes.iter().all(|o| o.outside.is_empty()), "bounding boxes outside tolerance");
    assert!(outcomes.iter().all(|o| o.counts_equal), "product counts by type differ");
    assert!(outcomes.iter().all(|o| o.volume_off.is_empty()), "volumes differ");
}

#[test]
fn the_attribute_tables_match_the_installed_ifcopenshell_schemas() {
    let python = match python_with_ifcopenshell() {
        Ok(p) => p,
        Err(why) => {
            assert!(std::env::var("AFROBIM_REQUIRE_IFCOPENSHELL").is_err(), "IfcOpenShell required but missing: {why}");
            eprintln!("SKIPPED: {why}");
            return;
        }
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new(python).arg("-I").arg(root.join("tests/gen_import_tables.py")).arg("--check").arg(root.join("src/import/tables.rs")).output().expect("python runs");
    assert!(out.status.success(), "src/import/tables.rs is stale: {}", String::from_utf8_lossy(&out.stderr));
}
