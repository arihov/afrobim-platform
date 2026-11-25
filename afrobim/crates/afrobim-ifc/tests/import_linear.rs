// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The importer's reading of `IfcLinearPlacement` on a file this crate did not write.
//!
//! The buildingSMART sample `linear-placement-of-signal.ifc` (IFC4.3.x-sample-models,
//! `models/alignment-geometries-and-linear-positioning/`) has an alignment with lines, clothoids
//! and circles, a gradient curve with circular vertical curves, and products and referents
//! placed on it by `IfcLinearPlacement`. The file is not part of this repository: point
//! `IFC_LINEAR_SAMPLE` at a copy (default `/home/user/ifcsamples/files/linear-placement-of-signal.ifc`)
//! and the test compares the importer's world bounding box of every shaped product with
//! IfcOpenShell's. Without the file, or without IfcOpenShell, it prints why and passes.

mod common;

use std::path::PathBuf;
use std::process::Command;

use afrobim_ifc::import::{import_ifc, ImportOptions};
use common::python_with_ifcopenshell;

#[test]
fn the_importer_places_a_buildingsmart_sample_where_ifcopenshell_does() {
    let sample = PathBuf::from(std::env::var("IFC_LINEAR_SAMPLE").unwrap_or_else(|_| "/home/user/ifcsamples/files/linear-placement-of-signal.ifc".into()));
    if !sample.exists() {
        eprintln!("SKIPPED: no sample file at {}", sample.display());
        return;
    }
    let python = match python_with_ifcopenshell() {
        Ok(p) => p,
        Err(why) => {
            eprintln!("SKIPPED: IfcOpenShell not available ({why})");
            return;
        }
    };
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("import-linear-boxes.json");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("check_import_linear.py");
    let run = Command::new(python).arg(&script).arg(&sample).arg(&out).output().expect("run check_import_linear.py");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let boxes: serde_json::Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    let model = import_ifc(&sample, &ImportOptions::default()).expect("the sample imports");
    let r = &model.report;
    println!("linear placements evaluated {}, fallback {}, unresolved {:?}", r.linear_placements_resolved, r.linear_placements_fallback, r.placements_unresolved);
    assert!(r.linear_placements_resolved > 0);
    assert_eq!(r.linear_placements_fallback, 0, "{:?}", r.warnings);
    assert!(!r.placements_unresolved.contains_key("IFCLINEARPLACEMENT"));
    let (mut compared, mut worst) = (0, 0.0f64);
    for p in &model.products {
        let Some((lo, hi)) = p.bounds else { continue };
        let Some(expected) = boxes.get(&p.global_id) else { continue };
        let (elo, ehi) = (&expected[0], &expected[1]);
        for i in 0..3 {
            worst = worst.max((lo[i] - elo[i].as_f64().unwrap()).abs()).max((hi[i] - ehi[i].as_f64().unwrap()).abs());
        }
        compared += 1;
    }
    println!("{compared} products compared, worst difference {worst:.3e} mm");
    assert!(compared >= 20, "{compared}");
    assert!(worst < 0.05, "{worst}");
}
