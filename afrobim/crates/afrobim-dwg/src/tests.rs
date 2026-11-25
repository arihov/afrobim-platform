// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
use super::*;
use afrobim_draw::Pt2;
use afrobim_dxf::reader::{self, Underlay};

/// A small drawing as AfroBIM's own exporter writes it (R12 ASCII DXF): lines, arcs, an open and a
/// closed polyline, text, on four layers.
fn exporter_dxf() -> String {
    let mut d = afrobim_dxf::DxfDocument::new();
    d.add_line("ROAD-EDGE", 1, Pt2::new(0.0, 0.0), Pt2::new(100_000.0, 0.0));
    d.add_line("ROAD-EDGE", 1, Pt2::new(0.0, 7_300.0), Pt2::new(100_000.0, 7_300.0));
    d.add_arc("KERB", 3, Pt2::new(50_000.0, 20_000.0), 12_500.0, 30.0, 150.0);
    d.add_polyline("CENTRELINE", 2, &[Pt2::new(0.0, 3_650.0), Pt2::new(40_000.0, 3_650.0), Pt2::new(100_000.0, 5_000.0)], false, 0.0);
    d.add_polyline("CENTRELINE", 2, &[Pt2::new(10.0, 10.0), Pt2::new(20.0, 10.0), Pt2::new(20.0, 20.0), Pt2::new(10.0, 20.0)], true, 0.0);
    d.add_text(&afrobim_dxf::DxfText::new("CH 0+000", Pt2::new(0.0, -2_000.0), 350.0, "TEXT"));
    d.finish()
}

fn read_dxf(text: &str) -> Underlay {
    reader::read_bytes(text.as_bytes()).expect("the afrobim reader reads the DXF")
}

fn count(u: &Underlay, kind: &str) -> usize {
    u.report.read.get(kind).copied().unwrap_or(0)
}

fn layer_names(u: &Underlay) -> Vec<String> {
    let mut v: Vec<String> = u.layers.iter().map(|l| l.name.to_ascii_uppercase()).collect();
    v.sort();
    v
}

#[test]
fn version_codes_are_recognised_and_named() {
    assert!(is_dwg(b"AC1032\0\0"));
    assert!(is_dwg(b"AC1015"));
    assert!(!is_dwg(b"  0\nSECTION"));
    assert!(!is_dwg(b"AC10"));
    assert!(!is_dwg(b"AC10XY"));
    assert_eq!(version_of(b"AC1027\0\0\0").as_deref(), Some("AC1027"));
    assert_eq!(version_of(b"0\nSECTION"), None);
    assert_eq!(DwgVersion::parse("ac1018"), Some(DwgVersion::R2004));
    assert_eq!(DwgVersion::parse("2018"), Some(DwgVersion::R2018));
    assert_eq!(DwgVersion::parse("R2013"), Some(DwgVersion::R2013));
    assert_eq!(DwgVersion::parse("2000"), None);
    for v in DwgVersion::ALL {
        assert_eq!(DwgVersion::parse(v.code()), Some(v));
        assert_eq!(release_name(v.code()), v.name());
    }
}

#[test]
fn oversize_input_is_refused_before_the_decoder_runs() {
    let small = Limits { dwg_bytes: 16, ..Limits::DEFAULT };
    let e = dwg_to_dxf_with(&[b'A'; 17], small).unwrap_err();
    assert!(e.contains("larger than"), "{e}");
    let e = dxf_to_dwg_with(&"0\n".repeat(20), DwgVersion::R2018, Limits { dxf_bytes: 16, ..Limits::DEFAULT }).unwrap_err();
    assert!(e.contains("more than"), "{e}");
    // A DXF that grows past its cap while being written is refused too.
    let dwg = dxf_to_dwg(&exporter_dxf(), DwgVersion::R2018).unwrap();
    let e = dwg_to_dxf_with(&dwg, Limits { dxf_bytes: 4096, ..Limits::DEFAULT }).unwrap_err();
    assert!(e.contains("more than"), "{e}");
}

#[test]
fn garbage_is_refused_with_a_message_not_a_panic() {
    assert!(dwg_to_dxf(b"0\nSECTION\n").unwrap_err().contains("not a DWG"));
    // A header that claims to be a DWG and then nothing.
    let mut fake = b"AC1032".to_vec();
    fake.extend(std::iter::repeat_n(0u8, 4096));
    assert!(dwg_to_dxf(&fake).is_err());
    // Random bytes after a valid code, for every version written.
    let mut x: u32 = 0x1234_5678;
    for v in DwgVersion::ALL {
        let mut junk = v.code().as_bytes().to_vec();
        junk.extend((0..20_000).map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        }));
        let _ = dwg_to_dxf(&junk);
    }
    assert!(dxf_to_dwg("this is not a DXF", DwgVersion::R2018).is_err());
}

/// The exporter's R12 DXF goes to DWG and back, for every version written, and the afrobim
/// reader finds the same lines, arcs, polylines, text and layers.
#[test]
fn the_exporters_dxf_round_trips_through_every_dwg_version() {
    let original = read_dxf(&exporter_dxf());
    assert_eq!(count(&original, "LINE"), 2);
    assert_eq!(count(&original, "ARC"), 1);
    assert_eq!(count(&original, "TEXT"), 1);
    let polylines = count(&original, "POLYLINE") + count(&original, "LWPOLYLINE");
    assert_eq!(polylines, 2);
    for v in DwgVersion::ALL {
        let dwg = dxf_to_dwg(&exporter_dxf(), v).unwrap_or_else(|e| panic!("{}: {e}", v.code()));
        assert!(dwg.starts_with(v.code().as_bytes()), "{}", v.code());
        assert_eq!(version_of(&dwg).as_deref(), Some(v.code()));
        let back = dwg_to_dxf_with(&dwg, Limits::DEFAULT).unwrap_or_else(|e| panic!("{}: {e}", v.code()));
        assert!(back.notes.is_empty(), "{}: {:?}", v.code(), back.notes);
        let u = read_dxf(&back.dxf);
        assert_eq!(count(&u, "LINE"), 2, "{}: {:?}", v.code(), u.report.read);
        assert_eq!(count(&u, "ARC"), 1, "{}: {:?}", v.code(), u.report.read);
        assert_eq!(count(&u, "TEXT") + count(&u, "MTEXT"), 1, "{}: {:?}", v.code(), u.report.read);
        assert_eq!(count(&u, "POLYLINE") + count(&u, "LWPOLYLINE"), 2, "{}: {:?}", v.code(), u.report.read);
        for name in ["ROAD-EDGE", "KERB", "CENTRELINE", "TEXT"] {
            assert!(layer_names(&u).contains(&name.to_string()), "{}: layer {name} in {:?}", v.code(), layer_names(&u));
        }
        // The geometry lands where it was: the same bounds to the millimetre.
        let (a, b) = (original.bounds.unwrap(), u.bounds.unwrap());
        for (p, q) in [(a.0, b.0), (a.1, b.1)] {
            assert!((p.x - q.x).abs() < 1.0 && (p.y - q.y).abs() < 1.0, "{}: {a:?} against {b:?}", v.code());
        }
        // The text survives.
        assert!(back.dxf.contains("CH 0+000"), "{}", v.code());
    }
}

/// Every DWG under the project folders, read by the built-in reader and, when it is installed,
/// by ODA File Converter, with the afrobim reader's counts for each. The files are only read.
/// Run with `cargo test -p afrobim-dwg -- --ignored --nocapture real_project`.
#[test]
#[ignore = "reads the project's own DWG files, which are not in the repository"]
fn real_project_dwgs() {
    let roots = ["/Users/dtmengineering/_Venture/afrobim/dwg-trials", "/Users/dtmengineering/_Venture/afrobim/kafunjo"];
    let mut files = Vec::new();
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dwg")) {
                out.push(p);
            }
        }
    }
    for r in roots {
        walk(std::path::Path::new(r), &mut files);
    }
    files.sort();
    println!("{} DWG file(s) found", files.len());
    let oda = reader::dwg::find_converter();
    let cache = std::env::temp_dir().join(format!("afrobim-dwg-real-{}", std::process::id()));
    let summary = |u: &Underlay| {
        let mut top: Vec<(&String, &usize)> = u.report.read.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        let top: Vec<String> = top.iter().take(8).map(|(k, v)| format!("{k} {v}")).collect();
        let view = u.to_drawing(&reader::DisplayOptions::default());
        let points: usize = view
            .drawing
            .primitives
            .iter()
            .map(|p| match p {
                afrobim_draw::Primitive::Fill { outline, .. } => outline.len(),
                afrobim_draw::Primitive::Glyphs { contours, .. } => contours.iter().map(Vec::len).sum(),
                _ => 2,
            })
            .sum();
        let b = u.bounds.map(|(a, b)| format!("({:.1}, {:.1}) to ({:.1}, {:.1})", a.x, a.y, b.x, b.y)).unwrap_or_default();
        format!(
            "{} read, {} unread, {} layers; drawn: {} items, {} primitives, {} points, {} labels{}; bounds {b}; {} warnings; {}",
            u.report.total_read(),
            u.report.total_unread(),
            u.layers.len(),
            view.items,
            view.drawing.primitives.len(),
            points,
            view.labels.len(),
            if view.truncated { " (truncated)" } else { "" },
            u.report.warnings.len(),
            top.join(", ")
        )
    };
    let mut failures = 0;
    for f in &files {
        let bytes = std::fs::read(f).unwrap();
        let code = version_of(&bytes).unwrap_or_else(|| "not a DWG".into());
        println!("\n{} ({code}, {:.1} MB)", f.display(), bytes.len() as f64 / 1048576.0);
        let t = std::time::Instant::now();
        match dwg_to_dxf_with(&bytes, Limits::DEFAULT) {
            Ok(c) => {
                let dxf_mb = c.dxf.len() as f64 / 1048576.0;
                match reader::read_bytes(c.dxf.as_bytes()) {
                    Ok(u) => println!("  built-in: {:.1} s, {dxf_mb:.1} MB DXF, {}", t.elapsed().as_secs_f64(), summary(&u)),
                    Err(e) => {
                        failures += 1;
                        println!("  built-in: DXF written ({dxf_mb:.1} MB) but the afrobim reader refused it: {e}");
                    }
                }
                for n in &c.notes {
                    println!("  built-in note: {n}");
                }
            }
            Err(e) => {
                failures += 1;
                println!("  built-in: FAILED after {:.1} s: {e}", t.elapsed().as_secs_f64());
            }
        }
        if let Some(program) = &oda {
            let t = std::time::Instant::now();
            match reader::dwg::convert_with(program, f, &cache).map_err(|e| e.to_string()).and_then(|dxf| reader::read_file(&dxf).map_err(|e| e.to_string())) {
                Ok(u) => println!("  ODA:      {:.1} s, {}", t.elapsed().as_secs_f64(), summary(&u)),
                Err(e) => println!("  ODA:      failed: {e}"),
            }
        }
    }
    let _ = std::fs::remove_dir_all(&cache);
    println!("\n{failures} built-in failure(s) of {}", files.len());
}

/// The DWGs written for export, checked by an independent reader: ODA File Converter turns each
/// back into DXF and the afrobim reader counts it. Needs ODA File Converter installed.
#[test]
#[ignore = "needs ODA File Converter installed"]
fn exported_dwgs_open_in_oda_file_converter() {
    let program = reader::dwg::find_converter().expect("ODA File Converter is installed");
    let dir = std::env::temp_dir().join(format!("afrobim-dwg-oda-audit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for v in DwgVersion::ALL {
        let path = dir.join(format!("export-{}.dwg", v.code()));
        std::fs::write(&path, dxf_to_dwg(&exporter_dxf(), v).unwrap()).unwrap();
        let dxf = reader::dwg::convert_with(&program, &path, &dir.join("cache")).unwrap_or_else(|e| panic!("{}: ODA refused it: {e}", v.code()));
        let u = reader::read_file(&dxf).unwrap();
        println!("{}: ODA read {:?}", v.code(), u.report.read);
        assert_eq!(count(&u, "LINE"), 2, "{}", v.code());
        assert_eq!(count(&u, "ARC"), 1, "{}", v.code());
        assert_eq!(count(&u, "TEXT") + count(&u, "MTEXT"), 1, "{}", v.code());
        assert_eq!(count(&u, "POLYLINE") + count(&u, "LWPOLYLINE"), 2, "{}", v.code());
    }
    let _ = std::fs::remove_dir_all(&dir);
}
