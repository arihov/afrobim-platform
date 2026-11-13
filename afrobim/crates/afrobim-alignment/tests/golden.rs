//! The placement rules against the C# reference.
//!
//! `tools/GoldenPlacement` runs `SignPlacement.Resolve` and `LocationReport.Write` from
//! `src/SignCore.Cli/SignPlacement.cs` and stores what they return in `tests/golden/`. Every case
//! is compared here: sides exactly, offsets and angles within 1e-12, warning and error text and
//! the CSV byte for byte.

use std::path::PathBuf;

use afrobim_alignment::{location_report, resolve, PlacedSign, SignLocation, SignSide, TrafficSide};
use serde_json::Value;

fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden").join(name)
}

fn json(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(golden(name)).unwrap()).unwrap()
}

fn side(v: &Value) -> Option<SignSide> {
    match v.as_str() {
        None => None,
        Some("left") => Some(SignSide::Left),
        Some("right") => Some(SignSide::Right),
        Some("both") => Some(SignSide::Both),
        Some(other) => panic!("unknown side {other}"),
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * b.abs().max(1.0)
}

#[test]
fn resolve_matches_the_reference_on_every_case() {
    let doc = json("placement.json");
    let cases = doc["cases"].as_array().unwrap();
    assert!(cases.len() > 4000, "the grid is {} cases", cases.len());
    let (mut plans, mut errors, mut warnings, mut pairs) = (0, 0, 0, 0);
    for (i, c) in cases.iter().enumerate() {
        let location = SignLocation { alignment: Some("A".into()), station: Some(100.0), offset: c["offset"].as_f64(), side: side(&c["side"]) };
        let drive = if c["drive"] == "left" { TrafficSide::Left } else { TrafficSide::Right };
        let got = resolve(&location, c["tangent_rad"].as_f64().unwrap(), drive);
        let what = format!("case {i}: {c}");
        if let Some(message) = c.get("error") {
            errors += 1;
            assert_eq!(got.expect_err(&what).to_string(), message.as_str().unwrap(), "{what}");
            continue;
        }
        plans += 1;
        let plan = got.unwrap_or_else(|e| panic!("{what}: {e}"));
        let want = c["points"].as_array().unwrap();
        assert_eq!(plan.points.len(), want.len(), "{what}");
        pairs += usize::from(want.len() == 2);
        for (p, w) in plan.points.iter().zip(want) {
            assert_eq!(Some(p.side), side(&w["side"]), "{what}");
            assert!(close(p.offset, w["offset"].as_f64().unwrap()), "{what}: offset {}", p.offset);
            assert!(close(p.rotation_rad, w["rotation_rad"].as_f64().unwrap()), "{what}: rotation {}", p.rotation_rad);
            assert!(close(p.rotation_deg(), w["rotation_deg"].as_f64().unwrap()), "{what}: rotation {}°", p.rotation_deg());
        }
        warnings += usize::from(plan.warning.is_some());
        assert_eq!(plan.warning.as_deref(), c["warning"].as_str(), "{what}");
    }
    // The grid really does exercise each outcome.
    assert!(plans > 3000 && errors > 50 && warnings > 500 && pairs > 500, "{plans} plans, {errors} errors, {warnings} warnings, {pairs} pairs");
}

#[test]
fn location_report_matches_the_reference_byte_for_byte() {
    let doc = json("location_reports.json");
    assert_eq!(doc["header"], location_report::HEADER);
    let reports = doc["reports"].as_array().unwrap();
    assert_eq!(reports.len(), 5);
    for r in reports {
        let signs: Vec<PlacedSign> = r["signs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| PlacedSign {
                code: s["code"].as_str().unwrap().to_string(),
                alignment: s["alignment"].as_str().unwrap().to_string(),
                station: s["station"].as_f64().unwrap(),
                offset: s["offset"].as_f64().unwrap(),
                side: side(&s["side"]).unwrap(),
                x: s["x"].as_f64().unwrap(),
                y: s["y"].as_f64().unwrap(),
                rotation_deg: s["rotation_deg"].as_f64().unwrap(),
            })
            .collect();
        let want = std::fs::read(golden(r["csv"].as_str().unwrap())).unwrap();
        let got = location_report::write(&signs);
        assert_eq!(got.as_bytes(), want.as_slice(), "report '{}':\n{got}\n-- expected --\n{}", r["name"], String::from_utf8_lossy(&want));
    }
}
