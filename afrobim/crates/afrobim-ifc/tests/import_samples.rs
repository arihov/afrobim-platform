// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Reads the sample files (directory in `IFC_SAMPLES_DIR`, default `/home/user/ifcsamples/files`)
//! and prints coverage. Skipped when the directory is missing.

use std::path::PathBuf;
use std::time::Instant;

use afrobim_ifc::import::*;

fn dir() -> Option<PathBuf> {
    let d = PathBuf::from(std::env::var("IFC_SAMPLES_DIR").unwrap_or_else(|_| "/home/user/ifcsamples/files".into()));
    d.exists().then_some(d)
}

#[test]
fn every_sample_file_imports_and_reports_coverage() {
    let Some(d) = dir() else {
        eprintln!("SKIPPED: no sample directory");
        return;
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "ifc")).collect();
    files.sort();
    for f in files {
        let t = Instant::now();
        let r = import_ifc(&f, &ImportOptions::default());
        match r {
            Ok(m) => {
                let r = &m.report;
                eprintln!(
                    "{:<44} {:<12} products {:>5} read {:>5} partial {:>4} bbox {:>4} notread {:>4} norep {:>4} unplaced {:>3} tris {:>8} {:>6.0} ms",
                    f.file_name().unwrap().to_string_lossy(),
                    r.schema,
                    r.products,
                    r.products_read,
                    r.products_partial,
                    r.products_placeholder,
                    r.products_not_read,
                    r.products_no_representation,
                    r.products_placement_unresolved,
                    r.triangles,
                    t.elapsed().as_secs_f64() * 1000.0
                );
                if !r.items_not_read.is_empty() {
                    eprintln!("    not read: {:?}", r.items_not_read);
                }
                if !r.warnings.is_empty() {
                    eprintln!("    warnings: {:?}", r.warnings);
                }
            }
            Err(e) => panic!("{}: {e}", f.display()),
        }
    }
}
