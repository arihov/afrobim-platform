//! Annotation output through the DXF writer, read back by ezdxf and by this workspace's own
//! reader. Skipped (with a printed note) when Python 3 or ezdxf is missing. No DXF written here
//! has been opened in AutoCAD.

use std::path::Path;
use std::process::Command;

use afrobim_annotate::*;
use afrobim_dxf::{reader, DxfDocument};
use afrobim_kernel::Document;
use serde_json::Value;

fn pt(x: f64, y: f64) -> Pt {
    afrobim_draw::Pt2::new(x, y)
}

fn sample() -> Document {
    let mut doc = Document::default();
    let mut s = AnnotationSettings { annotation_scale: 100.0, ..Default::default() };
    s.layers.insert("DIMS".into(), LayerProps { colour: [255, 0, 0], lineweight_mm: 0.25 });
    settings::store(&mut doc, &s);
    let st = s.current_dim();
    add_dimension(&mut doc, DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(12_500.0, 3_000.0), line_pt: pt(0.0, 4_000.0), orient: LinearOrient::Aligned }, &st), "DIMS").unwrap();
    add_dimension(&mut doc, DimData::new(DimKind::Angular { vertex: pt(0.0, 0.0), p1: pt(5000.0, 0.0), p2: pt(0.0, 5000.0), arc_pt: pt(3000.0, 3000.0) }, &st), "DIMS").unwrap();
    add_dimension(&mut doc, DimData::new(DimKind::Radius { centre: pt(20_000.0, 0.0), radius: 4000.0, at: pt(22_000.0, 2_000.0) }, &st), "DIMS").unwrap();
    add_dimension(&mut doc, DimData::new(DimKind::ArcLength { centre: pt(0.0, -20_000.0), radius: 9000.0, start_deg: 20.0, end_deg: 80.0, offset: 600.0 }, &st), "DIMS").unwrap();
    let mut t = TextData::new("Existing kerb\nto be removed", pt(1000.0, 8000.0), &s.current_text(), 100.0);
    t.rotation_deg = 20.0;
    t.width_factor = 0.8;
    t.justify = Justify::MiddleCentre;
    add_text(&mut doc, t, "NOTES").unwrap();
    add_leader(&mut doc, LeaderData::new(vec![pt(500.0, 500.0), pt(3000.0, 2500.0)], LeaderText::Literal("R61 \u{b0}".into()), &st), "NOTES").unwrap();
    doc
}

fn run_ezdxf(dxf: &Path) -> Option<Value> {
    let probe = Command::new("python3").args(["-I", "-c", "import ezdxf"]).output().ok()?;
    if !probe.status.success() {
        eprintln!("SKIPPED: python3 with ezdxf is not available");
        return None;
    }
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("read_dxf.py");
    let out = Command::new("python3").arg("-I").arg(script).arg(dxf).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    Some(serde_json::from_slice(&out.stdout).unwrap())
}

#[test]
fn exported_annotations_pass_the_ezdxf_audit_and_carry_the_text() {
    let doc = sample();
    let mut dxf = DxfDocument::new();
    let report = export_dxf(&doc, &mut dxf);
    assert_eq!(report.annotations, 6);
    let text = dxf.finish();
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("annotations.dxf");
    std::fs::write(&path, &text).unwrap();

    // This workspace's reader reads every entity it wrote.
    let u = reader::read_bytes(text.as_bytes()).unwrap();
    assert_eq!(u.report.total_unread(), 0, "{:?}", u.report.unread);

    let Some(j) = run_ezdxf(&path) else { return };
    assert!(j["errors"].as_array().unwrap().is_empty(), "ezdxf audit: {:?}", j["errors"]);
    let ents = j["entities"].as_array().unwrap();
    let of = |t: &str| ents.iter().filter(|e| e["type"] == t).collect::<Vec<_>>();

    // Text: every label is one TEXT with the same string, height and rotation, on its element's layer.
    let (items, _) = annotation_items(&doc);
    let labels: Vec<&Label> = items.iter().flat_map(|i| i.geometry.labels.iter()).collect();
    let texts = of("TEXT");
    assert_eq!(texts.len(), labels.len());
    for l in &labels {
        let t = texts.iter().find(|t| t["text"] == l.text.as_str() && (t["height"].as_f64().unwrap() - l.height).abs() < 1e-3).unwrap_or_else(|| panic!("no TEXT for {:?}", l.text));
        let at = if t["halign"].as_i64().unwrap() != 0 { &t["align_point"] } else { &t["insert"] };
        assert!((at[0].as_f64().unwrap() - l.at.x).abs() < 1e-3 && (at[1].as_f64().unwrap() - l.at.y).abs() < 1e-3, "{:?} at {at}", l.text);
        assert!((t["rotation"].as_f64().unwrap() - l.rotation_deg).abs() < 1e-3);
    }
    // The narrowed text carries its width factor.
    assert!(texts.iter().any(|t| (t["width"].as_f64().unwrap() - 0.8).abs() < 1e-9 && t["text"] == "Existing kerb"));
    // Arrow heads are SOLIDs, one per filled arrow.
    let arrows: usize = items.iter().map(|i| i.geometry.fills.iter().filter(|f| f.kind == FillKind::Arrow).count()).sum();
    assert_eq!(of("SOLID").len(), arrows);
    // Straight strokes, arcs: counts agree with the geometry.
    let (mut lines, mut arcs) = (0, 0);
    for i in &items {
        for s in &i.geometry.segs {
            match s {
                Seg::Line { .. } => lines += 1,
                Seg::Arc { .. } => arcs += 1,
                Seg::Poly { .. } => {}
            }
        }
    }
    assert_eq!(of("LINE").len(), lines);
    assert_eq!(of("ARC").len(), arcs);
    // The DIMS layer takes the colour of its layer properties (red is ACI 1).
    assert!(ents.iter().filter(|e| e["layer"] == "DIMS").all(|e| e["color"] == 1));
}

#[test]
fn the_arrow_solid_has_its_corners_in_the_order_a_solid_stores_them() {
    let mut doc = Document::default();
    let st = DimStyle { text_height_mm: 10.0, arrow_size_mm: 10.0, ..DimStyle::default() };
    add_dimension(&mut doc, DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(1000.0, 0.0), line_pt: pt(0.0, 100.0), orient: LinearOrient::Horizontal }, &st), "D").unwrap();
    let mut dxf = DxfDocument::new();
    export_dxf(&doc, &mut dxf);
    let text = dxf.finish();
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("arrow.dxf");
    std::fs::write(&path, &text).unwrap();
    let Some(j) = run_ezdxf(&path) else { return };
    let solids: Vec<&Value> = j["entities"].as_array().unwrap().iter().filter(|e| e["type"] == "SOLID").collect();
    assert_eq!(solids.len(), 2);
    // The first arrow has its tip at (0, 100) and its base 10 to the right of it, 1.67 either side.
    let v = &solids[0]["vertices"];
    assert_eq!((v[0][0].as_f64().unwrap(), v[0][1].as_f64().unwrap()), (0.0, 100.0));
    // A triangle: the fourth vertex repeats the third.
    assert_eq!(v[2], v[3]);
}
