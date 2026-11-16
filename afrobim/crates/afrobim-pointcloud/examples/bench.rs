//! Measures indexing of synthetic points. Run each mode as its own process so the peak memory
//! (VmHWM, Linux) belongs to that mode:
//!
//! ```text
//! cargo run --release -p afrobim-pointcloud --example bench -- build 10000000
//! cargo run --release -p afrobim-pointcloud --example bench -- write 10000000 /path/cloud.las
//! cargo run --release -p afrobim-pointcloud --example bench -- open /path/cloud.las
//! ```
//!
//! `build`: generate points in memory, build the index, run queries. `write`: write a LAS file.
//! `open`: open the file through the cache twice (first builds, second reads the cache).

use afrobim_pointcloud::*;
use std::time::Instant;

fn mem() -> String {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let get = |k: &str| s.lines().find(|l| l.starts_with(k)).map(|l| l.split_whitespace().nth(1).unwrap_or("?").parse::<f64>().unwrap_or(0.0) / 1024.0).unwrap_or(0.0);
    format!("rss {:.0} MB, peak {:.0} MB", get("VmRSS:"), get("VmHWM:"))
}

/// A terrain-like cloud over a long corridor: 2 km x 200 m, northing near 9 960 000.
fn generate(n: usize) -> PointBuffer {
    let mut s = 88172645463325252u64;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut b = PointBuffer::with_capacity(n, false, false);
    for _ in 0..n {
        let (x, y) = (500000.0 + next() * 2000.0, 9960000.0 + next() * 200.0);
        let z = 100.0 + 8.0 * ((x - 500000.0) / 150.0).sin() + 0.03 * (next() - 0.5);
        b.push(&PointRecord { xyz: [x, y, z], class: if next() < 0.6 { 2 } else { 1 }, intensity: (next() * 4000.0) as u16, return_number: 1, number_of_returns: 1, ..Default::default() });
    }
    b
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    match args.first().map(String::as_str) {
        Some("build") => {
            let n: usize = args[1].parse().unwrap();
            let t = Instant::now();
            let buf = generate(n);
            println!("[{profile}] generate {n} points: {:.2} s ({})", t.elapsed().as_secs_f64(), mem());
            let t = Instant::now();
            let ix = PointIndex::build(buf, &IndexOptions::default()).unwrap();
            let secs = t.elapsed().as_secs_f64();
            println!("[{profile}] build index: {secs:.2} s ({} per point held, {:.1} bytes/point incl. key) ({})", ix.memory_bytes() / n, ix.memory_bytes() as f64 / n as f64, mem());
            let view = Bounds3::plan([500400.0, 9960050.0], [500600.0, 9960150.0]);
            let t = Instant::now();
            let hits = ix.query_box(&view);
            println!("[{profile}] box query (200 m x 100 m): {} points in {:.1} ms", hits.len(), t.elapsed().as_secs_f64() * 1e3);
            let t = Instant::now();
            let lod = ix.lod(&view, 100_000);
            println!("[{profile}] lod 100000 of that view: {} points (depth {}) in {:.1} ms", lod.indices.len(), lod.depth, t.elapsed().as_secs_f64() * 1e3);
            let t = Instant::now();
            let whole = ix.lod(&Bounds3::plan([500000.0, 9960000.0], [502000.0, 9960200.0]), 100_000);
            println!("[{profile}] lod 100000 of the whole cloud: {} points in {:.1} ms", whole.indices.len(), t.elapsed().as_secs_f64() * 1e3);
            let line: Vec<[f64; 2]> = (0..=20).map(|k| [500000.0 + k as f64 * 100.0, 9960100.0 + 30.0 * (k as f64 / 3.0).sin()]).collect();
            let t = Instant::now();
            let sw = ix.query_polyline(&line, 5.0);
            println!("[{profile}] corridor swath 2 km x 10 m: {} points in {:.1} ms", sw.len(), t.elapsed().as_secs_f64() * 1e3);
            let t = Instant::now();
            let mut acc = 0.0;
            for k in 0..1000 {
                let q = [500000.0 + (k as f64 * 1.9), 9960000.0 + (k as f64 * 0.19), 100.0];
                acc += ix.nearest(q, 8, false)[7].distance;
            }
            println!("[{profile}] 1000 x 8-nearest: {:.1} ms total ({:.1} us each) (check {acc:.3})", t.elapsed().as_secs_f64() * 1e3, t.elapsed().as_secs_f64() * 1e6 / 1000.0);
        }
        Some("write") => {
            let n: usize = args[1].parse().unwrap();
            let path = std::path::Path::new(&args[2]);
            let t = Instant::now();
            let buf = generate(n);
            let mut o = ExportOptions::with_scale([0.001; 3], LasVersion::V1_2, args[2].ends_with(".laz"));
            o.offset = Some([500000.0, 9960000.0, 0.0]);
            let rep = export_las(path, &buf, None, &o).unwrap();
            println!("[{profile}] wrote {} points to {} in {:.2} s ({}); max rounding {:.4} m", rep.points_written, path.display(), t.elapsed().as_secs_f64(), mem(), rep.max_rounding_error);
        }
        Some("open") => {
            let path = std::path::Path::new(&args[1]);
            let size = std::fs::metadata(path).unwrap().len() as f64 / 1e6;
            let t = Instant::now();
            let h = sha256_file(path).unwrap();
            println!("[{profile}] sha256 of {size:.0} MB: {:.2} s ({})", t.elapsed().as_secs_f64(), &h[..12]);
            let _ = std::fs::remove_file(cache_path_for(path));
            let t = Instant::now();
            let a = open_indexed(path, &SourceOptions::default(), &IndexOptions::default()).unwrap();
            println!("[{profile}] first open ({:?}): {:.2} s ({})", a.cache, t.elapsed().as_secs_f64(), mem());
            drop(a);
            let t = Instant::now();
            let b = open_indexed(path, &SourceOptions::default(), &IndexOptions::default()).unwrap();
            println!("[{profile}] second open ({:?}): {:.2} s, {} points ({}), cache {:.0} MB", b.cache, t.elapsed().as_secs_f64(), b.index.len(), mem(), std::fs::metadata(&b.cache_path).unwrap().len() as f64 / 1e6);
        }
        _ => eprintln!("usage: bench build N | bench write N path | bench open path"),
    }
}
