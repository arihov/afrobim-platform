//! The Rust DXF writer must produce exactly the text the C# `DxfWriter` produces for the same
//! primitives. `tools/GoldenDxf` (C#) reads primitive lists (the sign engine's golden fixtures
//! plus a synthetic one covering bands, hairlines, arcs and symbol placeholders), runs the C#
//! writer and stores the DXF next to this test.

use std::path::{Path, PathBuf};

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use serde_json::Value;

fn pt(v: &Value) -> Pt2 {
    Pt2::new(v[0].as_f64().unwrap(), v[1].as_f64().unwrap())
}

fn colour(v: &Value) -> Option<Colour> {
    Some(match v.as_str()? {
        "White" => Colour::White,
        "Black" => Colour::Black,
        "Green" => Colour::Green,
        "Blue" => Colour::Blue,
        "Brown" => Colour::Brown,
        "Yellow" => Colour::Yellow,
        "Red" => Colour::Red,
        "Grey" => Colour::Grey,
        other => panic!("unknown colour {other}"),
    })
}

fn drawing(fixture: &Value) -> Drawing {
    let primitives = fixture["primitives"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| match p["type"].as_str().unwrap() {
            "Fill" => Primitive::Fill {
                outline: p["outline"].as_array().unwrap().iter().map(pt).collect(),
                colour: colour(&p["colour"]).unwrap(),
            },
            "Line" => Primitive::Line {
                from: pt(&p["from"]),
                to: pt(&p["to"]),
                width: p["width"].as_f64().unwrap(),
                colour: colour(&p["colour"]),
            },
            "Arc" => Primitive::Arc {
                centre: pt(&p["centre"]),
                radius: p["radius"].as_f64().unwrap(),
                start_deg: p["start_deg"].as_f64().unwrap(),
                end_deg: p["end_deg"].as_f64().unwrap(),
                colour: colour(&p["colour"]),
            },
            "Glyphs" => Primitive::Glyphs {
                contours: p["contours"].as_array().unwrap().iter().map(|c| c.as_array().unwrap().iter().map(pt).collect()).collect(),
                text: p["text"].as_str().unwrap().to_string(),
                ink: colour(&p["ink"]),
            },
            "Symbol" => Primitive::Symbol {
                origin: pt(&p["origin"]),
                id: p["id"].as_str().unwrap().to_string(),
                size: p["size"].as_f64().unwrap(),
                rotation_deg: p["rotation_deg"].as_f64().unwrap(),
            },
            other => panic!("unknown primitive {other}"),
        })
        .collect();
    Drawing { primitives, extents: Extents::default() }
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
fn every_dxf_matches_the_reference_writer() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let golden = here.join("tests/golden");
    let sign_fixtures = here.join("../afrobim-signs/tests/golden");

    let mut checked = 0;
    for entry in std::fs::read_dir(&golden).unwrap() {
        let dxf_path = entry.unwrap().path();
        if dxf_path.extension().and_then(|e| e.to_str()) != Some("dxf") {
            continue;
        }
        let stem = dxf_path.file_stem().unwrap().to_string_lossy().to_string();
        // The source primitives sit beside the DXF (synthetic cases) or among the sign fixtures.
        let local: PathBuf = golden.join(format!("{stem}.json"));
        let source = if local.exists() { local } else { sign_fixtures.join(format!("{stem}.json")) };
        let fixture: Value = serde_json::from_str(&std::fs::read_to_string(&source).unwrap_or_else(|e| panic!("{}: {e}", source.display()))).unwrap();
        let sw_mm = fixture["sw_mm"].as_f64().unwrap();

        let ours = afrobim_dxf::write(&drawing(&fixture), sw_mm);
        let reference = std::fs::read_to_string(&dxf_path).unwrap();
        assert!(ours == reference, "{stem}: {}", first_difference(&reference, &ours));
        checked += 1;
    }
    assert!(checked >= 4, "expected the reference DXFs to be present, found {checked}");
}
