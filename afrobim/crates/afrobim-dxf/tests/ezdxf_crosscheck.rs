//! Cross-check of the DXF reader against ezdxf.
//!
//! `tests/ezdxf_gen.py` builds a DXF with ezdxf covering each entity type (lines, arcs, circles,
//! mirrored extrusions, bulged polylines, ellipses, splines, hatches, text, solids, blocks with
//! scale, rotation, mirror, arrays and nesting, and a rendered dimension) in several DXF versions,
//! and records what ezdxf's own entity code says each one is. The reader reads the file and the
//! two are compared geometrically. The tests are skipped (with a printed note) when Python 3 or
//! ezdxf is not available.

use std::path::{Path, PathBuf};
use std::process::Command;

use afrobim_dxf::reader;
use serde_json::Value;

fn ezdxf_available() -> bool {
    Command::new("python3").args(["-I", "-c", "import ezdxf"]).output().map(|o| o.status.success()).unwrap_or(false)
}

fn generate(version: &str) -> Option<PathBuf> {
    if !ezdxf_available() {
        eprintln!("SKIPPED: python3 with ezdxf is not available");
        return None;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("ezdxf_{version}"));
    std::fs::create_dir_all(&dir).unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("ezdxf_gen.py");
    let out = Command::new("python3").arg("-I").arg(&script).arg(&dir).arg(version).output().expect("python runs");
    assert!(out.status.success(), "generator failed: {}", String::from_utf8_lossy(&out.stderr));
    Some(dir)
}

type Poly = Vec<(f64, f64)>;

/// Distance from `p` to the polyline `b`.
fn dist_to(p: (f64, f64), b: &Poly) -> f64 {
    if b.len() == 1 {
        return (p.0 - b[0].0).hypot(p.1 - b[0].1);
    }
    b.windows(2)
        .map(|w| {
            let (a, c) = (w[0], w[1]);
            let (dx, dy) = (c.0 - a.0, c.1 - a.1);
            let l2 = dx * dx + dy * dy;
            let t = if l2 < 1e-300 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) };
            (p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))
        })
        .fold(f64::INFINITY, f64::min)
}

/// Largest distance of any vertex of one polyline from the other, both ways.
fn hausdorff(a: &Poly, b: &Poly) -> f64 {
    let d1 = a.iter().map(|p| dist_to(*p, b)).fold(0.0, f64::max);
    let d2 = b.iter().map(|p| dist_to(*p, a)).fold(0.0, f64::max);
    d1.max(d2)
}

fn closed(p: &Poly) -> Poly {
    let mut c = p.clone();
    c.push(p[0]);
    c
}

fn bbox(p: &Poly) -> (f64, f64, f64, f64) {
    p.iter().fold((f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), |a, q| (a.0.min(q.0), a.1.min(q.1), a.2.max(q.0), a.3.max(q.1)))
}

/// Compares the reader's geometry for `u` with ezdxf's `expected` and returns the number of paths
/// matched, or what did not match.
fn compare_paths(version: &str, u: &reader::Underlay, expected: &Value) -> Result<usize, String> {
    let tol = 0.01; // drawing units: both sides are flattened to 0.001
    let flat = u.flatten(0.001);
    let mut mine: Vec<(String, Poly, bool, bool)> = flat.iter().filter(|f| f.0 != "TEXT" && f.0 != "POINT").map(|f| (f.0.clone(), f.2.iter().map(|p| (p.x, p.y)).collect(), f.3, false)).collect();
    let exp: Vec<(String, Poly)> = expected["paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["kind"].as_str().unwrap().to_string(), p["pts"].as_array().unwrap().iter().map(|q| (q[0].as_f64().unwrap(), q[1].as_f64().unwrap())).collect()))
        .collect();
    let mut unmatched_expected = Vec::new();
    for (kind, e) in &exp {
        let be = bbox(e);
        let mut found = None;
        for (i, (_, m, is_closed, used)) in mine.iter().enumerate() {
            if *used {
                continue;
            }
            let bm = bbox(m);
            if (be.0 - bm.0).abs() > tol || (be.1 - bm.1).abs() > tol || (be.2 - bm.2).abs() > tol || (be.3 - bm.3).abs() > tol {
                continue;
            }
            let mm = if *is_closed { closed(m) } else { m.clone() };
            let d = [hausdorff(&mm, e), hausdorff(&mm, &closed(e)), hausdorff(m, e)].into_iter().fold(f64::INFINITY, f64::min);
            if d <= tol {
                found = Some(i);
                break;
            }
        }
        match found {
            Some(i) => mine[i].3 = true,
            None => unmatched_expected.push((kind.clone(), be)),
        }
    }
    let leftover: Vec<(String, (f64, f64, f64, f64))> = mine.iter().filter(|m| !m.3).map(|m| (m.0.clone(), bbox(&m.1))).collect();
    if !unmatched_expected.is_empty() || !leftover.is_empty() {
        return Err(format!("{version}: ezdxf geometry the reader does not produce: {unmatched_expected:?}\nreader geometry ezdxf does not have: {leftover:?}"));
    }
    Ok(exp.len())
}

fn crosscheck(version: &str) {
    let Some(dir) = generate(version) else { return };
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("cross.json")).unwrap()).unwrap();
    let u = reader::read_file(&dir.join("cross.dxf")).unwrap();
    let flat = u.flatten(0.001);
    match compare_paths(version, &u, &expected) {
        Ok(n) => eprintln!("{version}: {n} paths matched ezdxf"),
        Err(e) => panic!("{e}"),
    }

    // Texts of model space: same plain text, anchor, height and rotation.
    let labels: Vec<reader::TextGeom> = u.entities.iter().filter_map(|e| if let reader::Geom::Text(t) = &e.geom { Some(t.clone()) } else { None }).collect();
    let want = expected["texts"].as_array().unwrap();
    assert_eq!(labels.len(), want.len(), "{version}: number of texts");
    for (t, w) in labels.iter().zip(want) {
        assert_eq!(t.text, w["text"].as_str().unwrap(), "{version}");
        assert!((t.at.x - w["x"].as_f64().unwrap()).abs() < 1e-9 && (t.at.y - w["y"].as_f64().unwrap()).abs() < 1e-9, "{version}: position of {:?}", t.text);
        assert!((t.height - w["height"].as_f64().unwrap()).abs() < 1e-9);
        assert!((t.rotation_deg - w["rotation"].as_f64().unwrap()).abs() < 1e-9);
    }
    // The rendered dimension carries its measured text (25 m) in its block.
    if version != "R12" {
        let ex = u.expand(&Default::default());
        assert!(ex.items.iter().any(|it| matches!(&it.shape, reader::Shape::Text(t) if t.text.contains("25"))), "{version}: dimension text");
    }
    // Points.
    let pts: Vec<_> = flat.iter().filter(|f| f.0 == "POINT").collect();
    assert_eq!(pts.len(), expected["points"].as_array().unwrap().len());
}

#[test]
fn r12_matches_ezdxf() {
    crosscheck("R12");
}

#[test]
fn r2000_matches_ezdxf() {
    crosscheck("R2000");
}

#[test]
fn r2010_matches_ezdxf() {
    crosscheck("R2010");
}

#[test]
fn r2018_matches_ezdxf() {
    crosscheck("R2018");
}

#[test]
fn the_header_and_layers_match_ezdxf() {
    let Some(dir) = generate("R2018") else { return };
    let u = reader::read_file(&dir.join("cross.dxf")).unwrap();
    assert_eq!(u.report.dxf_version.as_deref(), Some("AC1032"));
    assert_eq!(u.units.insunits, Some(6));
    assert_eq!(u.unit_scale_mm, 1000.0);
    for (name, aci) in [("ROAD", 1u8), ("EDGE", 3), ("TEXT", 5), ("BLK", 2)] {
        let l = &u.layers[u.layer_index(name).unwrap()];
        assert_eq!(l.aci, Some(aci), "{name}");
        assert!(l.visible());
    }
}

/// Real-world files from AutoCAD and other CAD programs, kept outside the repository. Set
/// `AFROBIM_DXF_SAMPLES` to a directory of `.dxf` files; each is read here and by ezdxf and the
/// geometry compared. A file ezdxf itself cannot read is reported and skipped.
#[test]
fn real_world_samples_match_ezdxf() {
    let Ok(dir) = std::env::var("AFROBIM_DXF_SAMPLES") else {
        eprintln!("SKIPPED: AFROBIM_DXF_SAMPLES is not set");
        return;
    };
    if !ezdxf_available() {
        eprintln!("SKIPPED: python3 with ezdxf is not available");
        return;
    }
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("ezdxf_gen.py");
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let mut failures = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "dxf")).collect();
    files.sort();
    for f in files {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let json = tmp.join(format!("{name}.json"));
        let out = Command::new("python3").arg("-I").arg(&script).arg("--expect").arg(&f).arg(&json).output().unwrap();
        if !out.status.success() {
            eprintln!("{name}: ezdxf cannot read it, skipped");
            continue;
        }
        let u = match reader::read_file(&f) {
            Ok(u) => u,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        let expected: Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
        match compare_paths(&name, &u, &expected) {
            Ok(n) => eprintln!("{name}: {n} paths matched ezdxf; read {:?}; not read {:?}", u.report.read, u.report.unread),
            Err(e) => failures.push(e.chars().take(1500).collect()),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
