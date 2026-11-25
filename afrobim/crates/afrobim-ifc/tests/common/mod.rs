// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! A hand-built document shared by the integration tests: three signs (one without a drawing)
//! and one element of a category the exporter does not know.

#![allow(dead_code)]

pub mod cases;
pub mod linear;
pub mod roads;
pub mod site;
pub mod step;
pub mod synth;

use std::collections::BTreeMap;

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use afrobim_ifc::{IfcElementInput, IfcOptions};
use afrobim_kernel::{Document, ElementId, Generated, Pt};
use serde_json::json;

pub const TIMESTAMP: i64 = 1_794_528_000; // 2026-11-13T00:00:00Z
pub const THICKNESS: f64 = 3.0;
pub const OFFSET: f64 = 0.1;

/// The area the exporter must produce for one colour on one paint layer.
pub struct Group {
    pub colour: Colour,
    pub layer: u32,
    pub area: f64,
}

pub struct Fixture {
    pub document: Document,
    pub ids: Vec<ElementId>,
    /// Parallel to `ids`.
    pub generated: Vec<Option<Generated>>,
    /// Parallel to `ids`: the expected artwork, ordered by layer then first appearance.
    pub groups: Vec<Vec<Group>>,
}

impl Fixture {
    pub fn inputs(&self) -> Vec<IfcElementInput<'_>> {
        self.ids
            .iter()
            .zip(&self.generated)
            .map(|(id, generated)| IfcElementInput { element: self.document.find(*id).unwrap(), generated: generated.as_ref() })
            .collect()
    }
}

pub fn options() -> IfcOptions {
    IfcOptions {
        project_name: "Kampala Northern Bypass — signs".to_string(),
        author: "A. N. O'Engineer".to_string(),
        organization: "AfroBIM".to_string(),
        application: "AfroBIM test".to_string(),
        timestamp: Some(TIMESTAMP),
        plate_thickness_mm: THICKNESS,
        artwork_offset_mm: OFFSET,
        ..IfcOptions::default()
    }
}

/// A Python interpreter that can import ifcopenshell, if there is one.
pub fn python_with_ifcopenshell() -> Result<&'static str, String> {
    let mut problems = Vec::new();
    for python in ["python3", "python"] {
        match std::process::Command::new(python).args(["-c", "import ifcopenshell, ifcopenshell.validate"]).output() {
            Ok(out) if out.status.success() => return Ok(python),
            Ok(out) => problems.push(format!("{python}: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("cannot import ifcopenshell"))),
            Err(error) => problems.push(format!("{python}: {error}")),
        }
    }
    Err(problems.join("; "))
}

pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt2> {
    vec![Pt2::new(x0, y0), Pt2::new(x1, y0), Pt2::new(x1, y1), Pt2::new(x0, y1)]
}

/// Area of a simple polygon (shoelace formula).
pub fn polygon_area(points: &[Pt2]) -> f64 {
    let n = points.len();
    (0..n).map(|i| points[i].x * points[(i + 1) % n].y - points[(i + 1) % n].x * points[i].y).sum::<f64>().abs() / 2.0
}

fn drawing(primitives: Vec<Primitive>, width: f64, height: f64) -> Drawing {
    Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(width, height) } }
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64, width: f64, colour: Option<Colour>) -> Primitive {
    Primitive::Line { from: Pt2::new(x0, y0), to: Pt2::new(x1, y1), width, colour }
}

pub fn fixture() -> Fixture {
    let mut document = Document::default();
    document.standard = "uganda-tsm-2004".to_string();
    document.ensure_layer("Signs");
    let mut ids = Vec::new();
    let mut generated = Vec::new();
    let mut groups = Vec::new();

    // 1. A direction sign with every kind of primitive.
    let id = document.add(
        "sign",
        "ADS-001",
        Pt::new(0.0, 0.0),
        1200.0,
        800.0,
        "Signs",
        json!({ "kind": "StackType", "catalog_key": "GFS-B1", "speed_kph": 80, "destinations": ["Kampala", "Jinja"] }),
    );
    {
        let e = document.find_mut(id).unwrap();
        e.properties.insert("Material".into(), "Aluminium 3 mm".into());
        e.properties.insert("Drawing ref".into(), "O'Brien / Zürich №7 \\ rev B".into());
        e.properties.insert("Remarks".into(), "x".repeat(300));
    }
    let background = rect(0.0, 0.0, 1200.0, 800.0);
    // Concave: an arrow pointing right.
    let arrow = vec![
        Pt2::new(100.0, 300.0),
        Pt2::new(400.0, 300.0),
        Pt2::new(400.0, 200.0),
        Pt2::new(600.0, 400.0),
        Pt2::new(400.0, 600.0),
        Pt2::new(400.0, 500.0),
        Pt2::new(100.0, 500.0),
    ];
    // An "O": the counter is wound the same way as the outside, so only even-odd keeps it open.
    let o_outer = rect(700.0, 300.0, 1000.0, 600.0);
    let o_counter = rect(750.0, 350.0, 950.0, 550.0);
    let i_stem = rect(1040.0, 300.0, 1080.0, 600.0);
    let patch = rect(150.0, 350.0, 350.0, 450.0);
    let dot = rect(200.0, 380.0, 300.0, 420.0);
    let border = 2.0 * 1160.0 * 20.0 + 2.0 * 760.0 * 20.0;
    let white = border + polygon_area(&arrow) + polygon_area(&o_outer) - polygon_area(&o_counter) + polygon_area(&i_stem);
    // The arc becomes 30 quads between radii 49.5 and 50.5, each spanning 6 degrees.
    let arc = 30.0 * 0.5 * 6.0_f64.to_radians().sin() * (50.5 * 50.5 - 49.5 * 49.5);
    groups.push(vec![
        Group { colour: Colour::Green, layer: 0, area: polygon_area(&background) },
        Group { colour: Colour::White, layer: 1, area: white },
        Group { colour: Colour::Black, layer: 1, area: arc },
        Group { colour: Colour::Black, layer: 2, area: polygon_area(&patch) },
        Group { colour: Colour::Yellow, layer: 3, area: polygon_area(&dot) },
    ]);
    let mut derived = BTreeMap::new();
    derived.insert("x_height_mm".to_string(), "100".to_string());
    derived.insert("area_m2".to_string(), "0.96".to_string());
    generated.push(Some(Generated {
        solids: Vec::new(),
        drawing: drawing(
            vec![
                Primitive::Fill { outline: background, colour: Colour::Green },
                line(20.0, 30.0, 1180.0, 30.0, 20.0, Some(Colour::White)),
                line(20.0, 770.0, 1180.0, 770.0, 20.0, Some(Colour::White)),
                line(30.0, 20.0, 30.0, 780.0, 20.0, Some(Colour::White)),
                line(1170.0, 20.0, 1170.0, 780.0, 20.0, Some(Colour::White)),
                Primitive::Fill { outline: arrow, colour: Colour::White },
                Primitive::Glyphs { contours: vec![o_outer, o_counter, i_stem], text: "OI".into(), ink: None },
                Primitive::Fill { outline: patch, colour: Colour::Black },
                Primitive::Glyphs { contours: vec![dot], text: "-".into(), ink: Some(Colour::Yellow) },
                Primitive::Arc { centre: Pt2::new(800.0, 120.0), radius: 50.0, start_deg: 0.0, end_deg: 180.0, colour: None },
                Primitive::Symbol { origin: Pt2::new(900.0, 100.0), id: "airport".into(), size: 80.0, rotation_deg: 0.0 },
            ],
            1200.0,
            800.0,
        ),
        notes: Vec::new(),
        derived,
    }));
    ids.push(id);

    // 2. A triangular sign with a diagonal band.
    let id = document.add("sign", "REG-002 'Yield'", Pt::new(1700.0, 0.0), 600.0, 600.0, "Signs", json!({ "kind": "Regulatory" }));
    let triangle = vec![Pt2::new(0.0, 0.0), Pt2::new(600.0, 0.0), Pt2::new(300.0, 600.0)];
    groups.push(vec![
        Group { colour: Colour::Red, layer: 0, area: polygon_area(&triangle) },
        Group { colour: Colour::White, layer: 1, area: 200.0 * 2.0_f64.sqrt() * 30.0 },
    ]);
    generated.push(Some(Generated {
        solids: Vec::new(),
        drawing: drawing(
            vec![Primitive::Fill { outline: triangle, colour: Colour::Red }, line(200.0, 100.0, 400.0, 300.0, 30.0, Some(Colour::White))],
            600.0,
            600.0,
        ),
        notes: Vec::new(),
        derived: BTreeMap::new(),
    }));
    ids.push(id);

    // 3. A sign whose drawing was not generated: plate only.
    let id = document.add("sign", "BARE-003", Pt::new(0.0, 1300.0), 900.0, 300.0, "0", json!({}));
    groups.push(Vec::new());
    generated.push(None);
    ids.push(id);

    // 4. A category the exporter has no mapping for.
    let id = document.add("marking", "MK-004", Pt::new(2800.0, 500.0), 400.0, 200.0, "0", json!({ "kind": "Arrow" }));
    let slab = rect(0.0, 0.0, 400.0, 200.0);
    groups.push(vec![Group { colour: Colour::Yellow, layer: 0, area: polygon_area(&slab) }]);
    generated.push(Some(Generated {
        solids: Vec::new(),
        drawing: drawing(vec![Primitive::Fill { outline: slab, colour: Colour::Yellow }], 400.0, 200.0),
        notes: Vec::new(),
        derived: BTreeMap::new(),
    }));
    ids.push(id);

    Fixture { document, ids, generated, groups }
}
