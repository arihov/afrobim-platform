// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! LandXML export: round trip through the `afrobim-alignment` importer.
//!
//! Stated tolerance: after export and import, every element's start point, heading, length and
//! curvatures, and every PVI and vertical curve length, agree to 1e-6 (metres, radians, 1/m); the
//! test measures the worst difference and prints it. Points go out with full `f64` precision
//! and the importer re-derives arc radii, centres and spiral headings from them, so the
//! difference is rounding, not the file format.

use afrobim_alignment::{parse_landxml, Alignment, VerticalCurve};
use afrobim_design::*;

fn design() -> AlignmentDesign {
    let h = HorizontalDesign::new(
        "Option A",
        12_000.0,
        vec![
            Pi::point(450_000.0, 100_000.0),
            Pi::spiral_curve(450_800.0, 100_100.0, 400.0, 90.0, 90.0),
            Pi::curve(451_500.0, 100_900.0, 350.0),
            Pi::spiral_curve(452_100.0, 100_300.0, 500.0, 70.0, 100.0),
            Pi::point(452_900.0, 100_500.0),
        ],
    );
    let v = VerticalDesign::new(
        "Option A profile",
        vec![VPvi::point(12_000.0, 1_150.0), VPvi::with_length(12_500.0, 1_165.0, 240.0), VPvi::with_k(13_100.0, 1_150.0, 30.0), VPvi::point(13_700.0, 1_158.0)],
    );
    AlignmentDesign::new(h, Some(v))
}

fn compare(a: &Alignment, b: &Alignment) -> f64 {
    assert_eq!(a.elements.len(), b.elements.len());
    let mut worst = 0.0f64;
    for (p, q) in a.elements.iter().zip(&b.elements) {
        worst = worst
            .max((p.x - q.x).abs())
            .max((p.y - q.y).abs())
            .max((p.length - q.length).abs())
            .max((afrobim_alignment::wrap_pi(p.heading_rad - q.heading_rad)).abs())
            .max((p.curvature_start - q.curvature_start).abs())
            .max((p.curvature_end - q.curvature_end).abs());
        assert_eq!(p.kind(), q.kind());
    }
    worst
}

#[test]
fn export_then_import_gives_the_same_geometry() {
    let built = design().build().unwrap();
    let xml = design().to_landxml(&LandXmlOptions { date: "2026-11-14".into(), time: "09:00:00".into(), project: "Test".into() }).unwrap();
    let imp = parse_landxml(&xml).unwrap();
    assert_eq!(imp.alignments.len(), 1, "{:?}", imp.notes);
    // The importer's notes must not report a disagreement between attributes and geometry.
    assert!(imp.notes.iter().all(|n| !n.contains("but")), "{:?}", imp.notes);
    let back = &imp.alignments[0];
    assert_eq!(back.name, "Option A");
    assert_eq!(back.start_station_m, 12_000.0);
    let worst = compare(&built.alignment, back);
    eprintln!("LandXML round trip, worst geometry difference: {worst:.3e}");
    assert!(worst < 1e-6, "{worst}");
    assert!((back.length() - built.alignment.length()).abs() < 1e-6);
    // Profile.
    let (p, q) = (built.alignment.profile.as_ref().unwrap(), back.profile.as_ref().unwrap());
    assert_eq!(p.pvis.len(), q.pvis.len());
    for (a, b) in p.pvis.iter().zip(&q.pvis) {
        assert!((a.station_m - b.station_m).abs() < 1e-9 && (a.elevation_m - b.elevation_m).abs() < 1e-9);
        match (a.curve, b.curve) {
            (VerticalCurve::None, VerticalCurve::None) => {}
            (VerticalCurve::Parabola { length_m: x }, VerticalCurve::Parabola { length_m: y }) => assert!((x - y).abs() < 1e-9),
            other => panic!("{other:?}"),
        }
    }
    // Levels at stations agree.
    for s in [12_000.0, 12_380.0, 12_500.0, 13_000.0, 13_100.0, 13_699.0] {
        assert!((p.elevation_at(s).unwrap() - q.elevation_at(s).unwrap()).abs() < 1e-9);
    }
    // Plan positions agree along the alignment.
    for s in [12_000.0, 12_500.0, 13_000.0, 13_500.0] {
        let (a, b) = (built.alignment.point_at(s).unwrap(), back.point_at(s).unwrap());
        assert!((a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6);
    }
}

#[test]
fn names_follow_the_landxml_schema() {
    let xml = design().to_landxml(&LandXmlOptions::default()).unwrap();
    for needle in [
        "<LandXML ", "version=\"1.2\"", "<Units>", "<Metric ", "linearUnit=\"meter\"", "<Alignments>", "<Alignment name=", "staStart=", "<CoordGeom>", "<Line ", "<Spiral ",
        "spiType=\"clothoid\"", "radiusStart=\"INF\"", "<Curve ", "rot=\"cw\"", "<Center>", "<Profile name=", "<ProfAlign name=", "<PVI>", "<ParaCurve length=",
    ] {
        assert!(xml.contains(needle), "missing {needle}");
    }
    // Every Spiral has Start, PI and End (the schema requires all three).
    let blocks: Vec<&str> = xml.split("<Spiral ").skip(1).collect();
    assert_eq!(blocks.len(), 4);
    for b in blocks {
        let body = &b[..b.find("</Spiral>").unwrap()];
        assert!(body.contains("<Start>") && body.contains("<PI>") && body.contains("<End>"));
    }
}

/// Writes the sample export to the path in `AFROBIM_DESIGN_SAMPLE_OUT`, if set, so it can be
/// validated against the schema: `xmllint --noout --schema LandXML-1.2.xsd sample.xml`.
#[test]
fn write_sample_for_schema_validation() {
    if let Ok(path) = std::env::var("AFROBIM_DESIGN_SAMPLE_OUT") {
        std::fs::write(path, design().to_landxml(&LandXmlOptions::default()).unwrap()).unwrap();
    }
}

#[test]
fn a_horizontal_only_design_exports_without_a_profile() {
    let mut d = design();
    d.vertical = None;
    let xml = d.to_landxml(&LandXmlOptions::default()).unwrap();
    assert!(!xml.contains("<Profile"));
    assert!(parse_landxml(&xml).unwrap().alignments[0].profile.is_none());
}

#[test]
fn station_equations_survive() {
    use afrobim_alignment::{StationDirection, StationEquation};
    let mut a = design().build().unwrap().alignment;
    a.station_equations.equations.push(StationEquation { internal_m: 12_800.0, back_m: None, ahead_m: 5_000.0, direction: StationDirection::Increasing });
    let xml = write_landxml(&a, &LandXmlOptions::default());
    let back = &parse_landxml(&xml).unwrap().alignments[0];
    assert_eq!(back.station_equations.equations.len(), 1);
    assert_eq!(back.station_equations.equations[0].ahead_m, 5_000.0);
}
