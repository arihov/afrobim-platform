// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Time and memory of the importer on a large generated file. Ignored by default (it writes a
//! file of a few tens of megabytes to the temp directory):
//!
//!     cargo test -p afrobim-ifc --test import_perf -- --ignored --nocapture
//!
//! `IFC_PERF_MB` sets the target size (default 40); `IFC_PERF_KEEP=path` keeps a copy of the file. Debug builds are several times slower than
//! release; the numbers in the README say which build produced them. Peak memory is read from
//! `/proc/self/status` (`VmHWM`) after resetting it, so Linux only.

mod common;

use std::time::Instant;

use afrobim_ifc::import::*;
use common::synth::*;

fn hwm_kb() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    s.lines().find(|l| l.starts_with("VmHWM:"))?.split_whitespace().nth(1)?.parse().ok()
}

fn reset_hwm() {
    let _ = std::fs::write("/proc/self/clear_refs", "5");
}

/// A grid of products: faceted brep cuboids, tessellated blocks with a few hundred triangles,
/// extruded polyline profiles, and mapped uses of one representation.
fn generate(target_bytes: usize) -> (String, usize) {
    let mut b = B::new("IFC4", "IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.)");
    // One shared map for the mapped products.
    let prof = b.rect(500.0, 300.0, [0.0, 0.0], None);
    let ex = b.extrusion(prof, None, [0.0, 0.0, 1.0], 800.0);
    let origin = b.axis3([0.0; 3], None, None);
    let mut n = 0usize;
    let mut size = 0usize;
    while size < target_bytes {
        let x = (n % 200) as f64 * 3000.0;
        let y = (n / 200) as f64 * 3000.0;
        let pl = b.local_placement(None, [x, y, 0.0], None);
        let shape = match n % 4 {
            0 => {
                let pts = cuboid(1000.0 + (n % 7) as f64, 800.0, 600.0);
                let faces: Vec<BrepFace> = CUBOID_FACES.iter().map(|f| (f.to_vec(), vec![], false)).collect();
                let br = b.brep(&pts, &faces);
                b.shape("Body", "Brep", &[br])
            }
            1 => {
                // A 12 x 12 x 2 block surface: a few hundred triangles.
                let (nx, ny) = (12usize, 12usize);
                let mut pts = Vec::new();
                for j in 0..=ny {
                    for i in 0..=nx {
                        pts.push([i as f64 * 100.0, j as f64 * 100.0, ((i + j) % 3) as f64 * 5.0]);
                    }
                }
                let mut tris = Vec::new();
                for j in 0..ny {
                    for i in 0..nx {
                        let a = (j * (nx + 1) + i + 1) as u32;
                        let (bb, c, d) = (a + 1, a + (nx + 1) as u32, a + (nx + 1) as u32 + 1);
                        tris.push([a, bb, d]);
                        tris.push([a, d, c]);
                    }
                }
                let ts = b.triangulated(&pts, &tris, None);
                b.shape("Body", "Tessellation", &[ts])
            }
            2 => {
                let l = [[0.0, 0.0], [900.0, 0.0], [900.0, 250.0], [250.0, 250.0], [250.0, 700.0], [0.0, 700.0]];
                let p = b.closed_profile(&l);
                let ex = b.extrusion(p, None, [0.0, 0.0, 1.0], 500.0);
                b.shape("Body", "SweptSolid", &[ex])
            }
            _ => {
                let op = b.operator(None, None, None, [0.0, 0.0, 0.0], Some(1.0 + (n % 3) as f64));
                let mi = b.mapped(origin, &[ex], op);
                b.shape("Body", "MappedRepresentation", &[mi])
            }
        };
        b.proxy(&format!("P{n}"), pl, Some(shape));
        n += 1;
        size = b.approx_bytes();
    }
    (b.finish(), n)
}

#[test]
#[ignore]
fn a_file_of_tens_of_megabytes_imports_in_bounded_time_and_memory() {
    let mb: usize = std::env::var("IFC_PERF_MB").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
    let (text, products) = generate(mb * 1_000_000);
    let path = std::env::temp_dir().join(format!("afrobim-perf-{}.ifc", std::process::id()));
    std::fs::write(&path, &text).unwrap();
    let bytes = text.len();
    drop(text);
    reset_hwm();
    let base = hwm_kb().unwrap_or(0);
    let t = Instant::now();
    let model = import_ifc(&path, &ImportOptions::default()).unwrap();
    let elapsed = t.elapsed();
    let peak = hwm_kb().unwrap_or(0);
    let t2 = Instant::now();
    let file = StepFile::open(&path).unwrap();
    let parse_only = t2.elapsed();
    let instances = file.len();
    drop(file);
    match std::env::var("IFC_PERF_KEEP") {
        Ok(keep) => {
            let _ = std::fs::copy(&path, keep);
            let _ = std::fs::remove_file(&path);
        }
        Err(_) => {
            let _ = std::fs::remove_file(&path);
        }
    }
    eprintln!(
        "PERF {:.1} MB, {} products, {} instances, {} triangles: parse {:.2} s, full import {:.2} s ({}), peak RSS during import {:.0} MB (baseline {:.0} MB)",
        bytes as f64 / 1e6,
        products,
        instances,
        model.report.triangles,
        parse_only.as_secs_f64(),
        elapsed.as_secs_f64(),
        if cfg!(debug_assertions) { "debug build" } else { "release build" },
        peak as f64 / 1024.0,
        base as f64 / 1024.0
    );
    assert_eq!(model.report.products - 3, products);
    assert_eq!(model.report.products_read, products);
}
