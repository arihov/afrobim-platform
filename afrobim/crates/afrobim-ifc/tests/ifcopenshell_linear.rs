//! Alignment geometry and linear placement, checked by an independent IFC toolkit.
//!
//! Exports the model of `tests/common/linear.rs` twice, with `AlignmentExport::Linear` and with
//! `AlignmentExport::Geometry` (the same placements written absolutely), works out what the
//! alignment crate says about the alignment (curve points, levels, station mapping, the layout
//! segments), and runs `tests/check_alignment_ifc.py`, which opens both files with IfcOpenShell,
//! validates them, evaluates the curves and the linear placements with the toolkit's kernel and
//! compares everything. A missing `python3` or `ifcopenshell` makes the test print why and pass,
//! unless `AFROBIM_REQUIRE_IFCOPENSHELL` is set.
//!
//! The same is done for the road-module model (`tests/common/roads.rs`) with every root element
//! given a station on its alignment, so that every kind of geometry the exporter writes is
//! re-expressed in a linear placement's frame and compared with the absolute file.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use afrobim_alignment::{Alignment, AlignmentElement, ElementKind, VerticalCurve};
use afrobim_ifc::{element_global_id, export_ifc_model, global_id, AlignmentExport, IfcOptions};
use serde_json::{json, Value};

use common::linear::{linear_fixture, LinearFixture};
use common::roads::road_fixture;
use common::{options, python_with_ifcopenshell};

fn with(mode: AlignmentExport) -> IfcOptions {
    IfcOptions { alignment: mode, ..options() }
}

fn heading_at(a: &Alignment, station: f64) -> f64 {
    a.point_at(station).unwrap().heading_rad
}

/// What the alignment crate says, by its own arithmetic, for one alignment.
pub fn alignment_expectations(a: &Alignment, positioned: Option<Vec<String>>, with_stations: bool) -> Value {
    let start = a.start_station_m;
    // Plan samples: along every element at fractions of its length, labelled by kind.
    let mut samples = Vec::new();
    let mut at = start;
    for el in &a.elements {
        for f in [0.0, 0.05, 0.1, 0.25, 0.4, 0.5, 0.6, 0.75, 0.9, 0.95] {
            let station = at + f * el.length;
            let p = a.point_at(station).unwrap();
            let kind = match el.kind() {
                ElementKind::Line => "line",
                ElementKind::Arc => "arc",
                ElementKind::Spiral => "spiral",
            };
            let z = a.profile.as_ref().and_then(|p| p.elevation_at(station).ok()).map(|z| z * 1000.0);
            samples.push(json!({ "kind": kind, "d_m": station - start, "x_mm": p.x * 1000.0, "y_mm": p.y * 1000.0, "heading": heading_at(a, station), "z_mm": z }));
        }
        at += el.length;
    }
    let end = a.point_at(a.end_station_m()).unwrap();
    samples.push(json!({ "kind": "end", "d_m": a.length(), "x_mm": end.x * 1000.0, "y_mm": end.y * 1000.0, "heading": end.heading_rad, "z_mm": a.profile.as_ref().and_then(|p| p.elevation_at(a.end_station_m()).ok()).map(|z| z * 1000.0) }));
    // Vertical samples: every 5 m, and either side of each curve end.
    let mut vertical_samples = Vec::new();
    let mut vertical = Value::Null;
    if let Some(profile) = &a.profile {
        let (s0, s1) = profile.range().unwrap();
        let (lo, hi) = (s0.max(start), s1.min(a.end_station_m()));
        let mut stations: Vec<f64> = (0..).map(|i| lo + 5.0 * i as f64).take_while(|s| *s < hi).collect();
        stations.push(hi);
        for ex in profile.curve_extents().unwrap() {
            for s in [ex.start_station_m, ex.end_station_m, 0.5 * (ex.start_station_m + ex.end_station_m)] {
                if s >= lo && s <= hi {
                    stations.push(s);
                }
            }
        }
        for s in stations {
            let p = profile.point_at(s).unwrap();
            vertical_samples.push(json!({ "d_m": s - start, "z_mm": p.elevation_m * 1000.0, "grade": p.grade }));
        }
        // The segments: straights, curves and halves of asymmetric curves, from the PVIs.
        let mut bounds: Vec<(&str, f64, f64)> = Vec::new();
        let mut pos = s0;
        for ex in profile.curve_extents().unwrap() {
            if ex.start_station_m > pos + 1e-9 {
                bounds.push(("CONSTANTGRADIENT", pos, ex.start_station_m));
            }
            match profile.pvis[ex.pvi].curve {
                VerticalCurve::Parabola { .. } => bounds.push(("PARABOLICARC", ex.start_station_m, ex.end_station_m)),
                VerticalCurve::AsymmetricParabola { .. } => {
                    bounds.push(("PARABOLICARC", ex.start_station_m, profile.pvis[ex.pvi].station_m));
                    bounds.push(("PARABOLICARC", profile.pvis[ex.pvi].station_m, ex.end_station_m));
                }
                VerticalCurve::Circular { .. } => bounds.push(("CIRCULARARC", ex.start_station_m, ex.end_station_m)),
                VerticalCurve::None => {}
            }
            pos = ex.end_station_m;
        }
        if s1 > pos + 1e-9 {
            bounds.push(("CONSTANTGRADIENT", pos, s1));
        }
        let mut details = Vec::new();
        let mut kinds = Vec::new();
        for (kind, from, to) in bounds {
            let (p0, p1) = (profile.point_at(from).unwrap(), profile.point_at(to).unwrap());
            let length = to - from;
            let radius = match kind {
                "PARABOLICARC" => Some(length / (p1.grade - p0.grade) * 1000.0),
                "CIRCULARARC" => {
                    let r = length / (p1.grade.atan().sin() - p0.grade.atan().sin()).abs();
                    Some(if p1.grade > p0.grade { r } else { -r } * 1000.0)
                }
                _ => None,
            };
            kinds.push(kind);
            details.push(json!({ "d0_mm": (from - start) * 1000.0, "length_mm": length * 1000.0, "z0_mm": p0.elevation_m * 1000.0, "g0": p0.grade, "g1": if kind == "CONSTANTGRADIENT" { p0.grade } else { p1.grade }, "radius_mm": radius }));
        }
        vertical = json!({ "segments": kinds.len(), "kinds": kinds, "segments_detail": details });
    }
    // The horizontal segments, from the elements.
    let radius = |k: f64| if k == 0.0 { 0.0 } else { 1000.0 / k };
    let detail = |el: &AlignmentElement| {
        let (r0, r1) = match el.kind() {
            ElementKind::Line => (0.0, 0.0),
            _ => (radius(el.curvature_start), radius(el.curvature_end)),
        };
        json!({ "x_mm": el.x * 1000.0, "y_mm": el.y * 1000.0, "heading": afrobim_alignment::wrap_pi(el.heading_rad), "r0_mm": r0, "r1_mm": r1, "length_mm": el.length * 1000.0 })
    };
    let kinds: Vec<&str> = a.elements.iter().map(|e| match e.kind() {
        ElementKind::Line => "LINE",
        ElementKind::Arc => "CIRCULARARC",
        ElementKind::Spiral => "CLOTHOID",
    }).collect();
    // Station equations and displayed stations.
    let mut equations = Vec::new();
    let mut increasing = true;
    for (i, eq) in a.station_equations.equations.iter().enumerate() {
        let now = eq.direction == afrobim_alignment::StationDirection::Increasing;
        let has = if now != increasing { Value::Bool(now) } else { Value::Null };
        increasing = now;
        equations.push(json!({ "ahead_mm": eq.ahead_m * 1000.0, "back_mm": a.station_equations.back_station(i).unwrap() * 1000.0, "has_increasing": has, "distance_mm": (eq.internal_m - start) * 1000.0 }));
    }
    let mut displayed = Vec::new();
    // Displayed stations to try: the alignment's ends, and for every equation points either side
    // of its back and ahead stations.
    let mut tries: Vec<f64> = Vec::new();
    if with_stations {
        let end = a.end_station_m();
        tries.extend([start, 0.5 * (start + end), end]);
        for (i, eq) in a.station_equations.equations.iter().enumerate() {
            let back = a.station_equations.back_station(i).unwrap();
            tries.extend([back - 40.0, back - 1.0, back, back + 1.0, eq.ahead_m - 10.0, eq.ahead_m - 1.0, eq.ahead_m, eq.ahead_m + 1.0, eq.ahead_m + 10.0, eq.ahead_m + 33.0]);
        }
    }
    for s in tries {
        let candidates: Vec<f64> = a.internal_station_candidates(s).iter().map(|c| (c.internal_m - start) * 1000.0).collect();
        displayed.push(json!({ "displayed_mm": s * 1000.0, "candidates_mm": candidates }));
    }
    // Superelevation events, in profile-frame sign: positive for a curve to the right.
    let mut events = Vec::new();
    if let Some(sup) = &a.superelevation {
        for section in &sup.sections {
            for (i, p) in section.points.iter().enumerate() {
                let (l, r) = (p.left_pct.map(|v| v / 100.0), p.right_pct.map(|v| -v / 100.0));
                let sides: Vec<(&str, f64)> = match (l, r) {
                    (Some(l), Some(r)) if (l - r).abs() < 1e-12 => vec![("BOTH", l)],
                    (l, r) => l.map(|v| ("LEFT", v)).into_iter().chain(r.map(|v| ("RIGHT", v))).collect(),
                };
                for (side, value) in sides {
                    events.push(json!({ "station_m": p.station_m, "side": side, "value": value, "transition": i > 0 }));
                }
            }
        }
    }
    json!({
        "name": a.name,
        "start_station_m": start,
        "length_mm": a.length() * 1000.0,
        "decreasing": a.station_equations.equations.iter().any(|e| e.direction == afrobim_alignment::StationDirection::Decreasing),
        "vertical": vertical,
        "horizontal": { "segments": a.elements.len(), "kinds": kinds, "segments_detail": a.elements.iter().map(detail).collect::<Vec<_>>() },
        "samples": samples,
        "vertical_samples": vertical_samples,
        "equations": equations,
        "displayed": displayed,
        "superelevation": events,
        "positioned_global_ids": positioned,
    })
}

fn expectations(f: &LinearFixture) -> Value {
    let project = options().project_name;
    let assembly = |name: &str| {
        let id = f.document.elements().iter().find(|e| e.name == name).unwrap().id;
        global_id(&format!("{project}/element/{}/assembly", id.0))
    };
    let element = |name: &str| element_global_id(&project, f.document.elements().iter().find(|e| e.name == name).unwrap().id);
    // Everything whose site placement names the alignment.
    let positioned = vec![
        assembly("ADS-201"),
        assembly("REG-202"),
        element("MKR-203"),
        element("LOOSE-P2"),
        element("MK-204"),
        element("WRN-205"),
        element("REG-206"),
        element("REG-207"),
    ];
    let e = alignment_expectations(&f.alignment, Some(positioned), true);
    let plain = alignment_expectations(&f.plain, Some(vec![element("REG-209")]), true);
    json!({
        "alignments": [e, plain],
        "linear_names": ["ADS-201", "REG-202", "MKR-203", "LOOSE-P2", "MK-204", "WRN-205", "REG-209"],
        "tolerances": { "plan_mm": 0.01, "direction_rad": 1e-8, "level_mm": 0.01, "placement_mm": 0.001 },
    })
}

fn run_check(linear: &Path, absolute: &Path, expected: &Path, figures: &Path) {
    let python = match python_with_ifcopenshell() {
        Ok(p) => p,
        Err(why) => {
            let message = format!("SKIPPED: IfcOpenShell validation not run ({why})");
            if std::env::var_os("AFROBIM_REQUIRE_IFCOPENSHELL").is_some() {
                panic!("{message}");
            }
            eprintln!("{message}");
            return;
        }
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("check_alignment_ifc.py");
    let out = Command::new(python).arg(&script).arg(linear).arg(absolute).arg(expected).arg(figures).output().expect("run check_alignment_ifc.py");
    let stdout = String::from_utf8_lossy(&out.stdout);
    println!("{stdout}");
    assert!(out.status.success(), "check_alignment_ifc.py failed\n{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn ifcopenshell_checks_the_alignment_geometry_and_the_linear_placements() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let f = linear_fixture();
    let linear = dir.join("afrobim-ifc-alignment-linear.ifc");
    let absolute = dir.join("afrobim-ifc-alignment-absolute.ifc");
    let expected = dir.join("afrobim-ifc-alignment.expected.json");
    std::fs::write(&linear, export_ifc_model(&f.document, &f.inputs(), &with(AlignmentExport::Linear)).text).unwrap();
    std::fs::write(&absolute, export_ifc_model(&f.document, &f.inputs(), &with(AlignmentExport::Geometry)).text).unwrap();
    std::fs::write(&expected, serde_json::to_string_pretty(&expectations(&f)).unwrap()).unwrap();
    run_check(&linear, &absolute, &expected, &dir.join("afrobim-ifc-alignment.figures.json"));
}

/// The road-module model with every root element given the alignment, a station and an offset:
/// the geometry of markings, drains, culverts, gantries, columns, barriers, a pavement and a
/// corridor is written in the frame of a linear placement and must land where it did.
#[test]
fn ifcopenshell_finds_the_road_modules_where_they_were_when_placed_linearly() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mut f = road_fixture();
    let ids: Vec<_> = f.document.elements().iter().filter(|e| e.site.is_some() && e.category != "alignment" && e.category != "surface").map(|e| (e.id, e.name.clone())).collect();
    for (id, _) in &ids {
        let e = f.document.find_mut(*id).unwrap();
        let s = e.site.as_mut().unwrap();
        s.alignment = Some("MC10".into());
        s.station_m = Some(10.0);
        s.offset_m = Some(0.0);
    }
    let linear = dir.join("afrobim-ifc-roads-linear.ifc");
    let absolute = dir.join("afrobim-ifc-roads-absolute.ifc");
    let expected = dir.join("afrobim-ifc-roads-linear.expected.json");
    let e = export_ifc_model(&f.document, &f.inputs(), &with(AlignmentExport::Linear));
    std::fs::write(&linear, &e.text).unwrap();
    std::fs::write(&absolute, export_ifc_model(&f.document, &f.inputs(), &with(AlignmentExport::Geometry)).text).unwrap();
    // The road fixture's alignment has no equations or superelevation; the positioned products
    // are not listed here (the check is on placements and boxes).
    let x = alignment_expectations(&f.alignment, None, false);
    std::fs::write(
        &expected,
        serde_json::to_string_pretty(&json!({ "alignments": [x], "linear_names": null, "tolerances": { "plan_mm": 0.01, "direction_rad": 1e-8, "level_mm": 0.01, "placement_mm": 0.001 } })).unwrap(),
    )
    .unwrap();
    assert!(e.text.contains("IFCLINEARPLACEMENT"));
    run_check(&linear, &absolute, &expected, &dir.join("afrobim-ifc-roads-linear.figures.json"));
}
