use std::path::{Path, PathBuf};

use afrobim_fabrication::*;
use afrobim_signs::{SignDefinition, SignEngine};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    definition: SignDefinition,
    quantity: Option<u32>,
    location: Option<String>,
}

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn engine() -> SignEngine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    SignEngine::load(&root.join("packs/UK-TSM-2019.json"), &root.join("fonts")).unwrap()
}

fn fixtures() -> Vec<Fixture> {
    serde_json::from_str(&std::fs::read_to_string(golden().join("signs.json")).unwrap()).unwrap()
}

fn first_difference(a: &str, b: &str) -> String {
    for (n, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x != y {
            return format!("line {}: reference `{x}`, ours `{y}`", n + 1);
        }
    }
    format!("lengths differ: reference {} lines, ours {} lines", a.lines().count(), b.lines().count())
}

#[test]
fn quantities() {
    let e = engine();
    let rows: Vec<SignQuantity> = fixtures().iter().map(|f| measure(&e, &f.definition, &e.layout(&f.definition).unwrap())).collect();
    let reference = std::fs::read_to_string(golden().join("quantities.csv")).unwrap();
    let ours = to_csv(&rows);
    assert!(ours == reference, "{}", first_difference(&reference, &ours));
    let reference = std::fs::read_to_string(golden().join("quantities.summary.csv")).unwrap();
    let ours = summary_to_csv(&summarise(&rows));
    assert!(ours == reference, "{}", first_difference(&reference, &ours));
}

#[test]
fn dxf() {
    let e = engine();
    for (n, f) in fixtures().iter().enumerate() {
        let _ = (&f.quantity, &f.location);
        let layout = e.layout(&f.definition).unwrap();
        let info = SheetInfo { project: "A109 Kasese road – signing (Lot 2)".into(), drawn_by: "GoldenFab".into(), revision: "B".into(), date: "2026-01-15".into(), drawing_number: format!("FS-{:03}", n + 1) };
        let sheet = compose(&e, &f.definition, &layout, &info);
        let name = f.definition.code.replace('/', "_");
        let reference = std::fs::read_to_string(golden().join(format!("{name}-sheet.dxf"))).unwrap();
        let ours = write_sheet_dxf(&sheet);
        assert!(ours == reference, "{name}: {}", first_difference(&reference, &ours));
    }
}
