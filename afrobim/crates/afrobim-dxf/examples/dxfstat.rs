// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Reads a DXF and prints what was read, how long it took and the peak memory of the process.
//!
//! `cargo run -p afrobim-dxf --example dxfstat -- FILE.dxf [--drawing] [--snaps]`

use std::time::Instant;

use afrobim_dxf::reader::{self, DisplayOptions, SnapOptions};

fn peak_rss_mb() -> f64 {
    // VmHWM in /proc/self/status is the peak resident set, in kB (Linux).
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmHWM:")).and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse::<f64>().ok())))
        .map(|kb| kb / 1024.0)
        .unwrap_or(f64::NAN)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: dxfstat FILE.dxf [--drawing] [--snaps]");
        std::process::exit(2);
    };
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let t = Instant::now();
    let u = match reader::read_file(std::path::Path::new(path)) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let read_s = t.elapsed().as_secs_f64();
    println!("file: {path} ({:.1} MB)", size as f64 / 1e6);
    println!("read: {read_s:.2} s ({:.1} MB/s), peak memory {:.0} MB", size as f64 / 1e6 / read_s.max(1e-9), peak_rss_mb());
    println!("version: {:?}, units: {} ({} mm per unit{})", u.report.dxf_version, u.units.name, u.unit_scale_mm, if u.units.assumed { ", assumed" } else { "" });
    println!("layers: {}, blocks: {}, model entities: {}, records: {}", u.layers.len(), u.blocks.len(), u.entities.len(), u.report.records);
    println!("read by type: {:?}", u.report.read);
    println!("not read: {:?}, paper space skipped: {}", u.report.unread, u.report.paper_space_skipped);
    if let Some((lo, hi)) = u.bounds {
        println!("bounds: ({:.3}, {:.3}) to ({:.3}, {:.3}) file units", lo.x, lo.y, hi.x, hi.y);
    }
    for w in u.report.warnings.iter().take(10) {
        println!("warning: {w}");
    }
    if args.iter().any(|a| a == "--drawing") {
        let t = Instant::now();
        let d = u.to_drawing(&DisplayOptions::default());
        println!("drawing: {} primitives, {} labels in {:.2} s, peak memory {:.0} MB", d.drawing.primitives.len(), d.labels.len(), t.elapsed().as_secs_f64(), peak_rss_mb());
    }
    if args.iter().any(|a| a == "--snaps") {
        let t = Instant::now();
        let s = u.snap_points(&DisplayOptions::default(), &SnapOptions::default());
        println!(
            "snaps: {} points ({} segments, {} pair tests, truncated points {}, truncated intersections {}) in {:.2} s, peak memory {:.0} MB",
            s.points.len(),
            s.segments,
            s.pair_tests,
            s.truncated_points,
            s.truncated_intersections,
            t.elapsed().as_secs_f64(),
            peak_rss_mb()
        );
    }
}
