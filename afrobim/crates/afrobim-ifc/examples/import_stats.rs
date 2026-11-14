//! Imports one IFC file and prints the coverage report, the time taken and the peak memory.
//!
//!     cargo run -p afrobim-ifc --example import_stats -- model.ifc [--map]
//!
//! Peak memory is `VmHWM` from `/proc/self/status` (Linux).

use std::path::Path;
use std::time::Instant;

use afrobim_ifc::import::*;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: import_stats <file.ifc> [--map]");
        std::process::exit(2);
    };
    let options = ImportOptions { apply_map_conversion: args.iter().any(|a| a == "--map"), ..ImportOptions::default() };
    let t = Instant::now();
    let model = match import_ifc(Path::new(path), &options) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let secs = t.elapsed().as_secs_f64();
    let r = &model.report;
    println!("file: {path} ({:.1} MB), schema {}, {} instances", r.file_bytes as f64 / 1e6, r.schema, r.instances);
    println!("length unit: {} ({} mm)", r.length_unit, r.length_unit_mm);
    println!(
        "products {} ({} spatial, left out below): read {}, partial {}, bounding box only {}, not read {}, no representation {}, placement unresolved {}",
        r.products, r.products_spatial, r.products_read, r.products_partial, r.products_placeholder, r.products_not_read, r.products_no_representation, r.products_placement_unresolved
    );
    println!("triangles {}", r.triangles);
    println!("by body representation type (read / not read): {:?}", r.by_representation.iter().map(|(k, v)| (k.as_str(), v.read, v.not_read)).collect::<Vec<_>>());
    println!("geometry items read: {:?}", r.items_read);
    println!("geometry not read: {:?}", r.items_not_read);
    println!("placements not resolved: {:?}", r.placements_unresolved);
    println!("properties not read: {:?}", r.properties_not_read);
    for w in &r.warnings {
        println!("warning: {w}");
    }
    let hwm = std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find(|l| l.starts_with("VmHWM:")).map(str::to_string));
    println!("time {:.2} s ({}), {}", secs, if cfg!(debug_assertions) { "debug build" } else { "release build" }, hwm.unwrap_or_else(|| "peak memory unavailable".into()));
}
