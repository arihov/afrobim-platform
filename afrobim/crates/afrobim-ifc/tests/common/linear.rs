// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! A model for the linear placement: one alignment with spirals both ways, a compound spiral,
//! a profile with every kind of vertical curve, station equations (a gap, an overlap, a
//! decreasing run) and stored superelevation, and elements placed on it at stations and offsets:
//! a sign on two posts, a sign on one post, a sign without supports, a post without a sign, an
//! element of an unknown category, and three elements whose placement must stay absolute (no
//! station, a station off the alignment, and none at all).
//!
//! Every dimension is test data, not an engineering value.

use std::collections::BTreeMap;

use afrobim_alignment::{
    Alignment, AlignmentBuilder, CrossFallPoint, Profile, Pvi, StationEquation, StationEquations, Superelevation, SuperelevationSection, Turn,
};
use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use afrobim_ifc::IfcElementInput;
use afrobim_kernel::{Document, ElementId, Generated, Module, Pt, SitePlacement};
use afrobim_supports::{add_supports, FoundationDefinition, FoundationModule, FoundationShape, PostDefinition, PostModule, Section, SupportArrangement};
use serde_json::json;

pub struct LinearFixture {
    pub document: Document,
    pub generated: BTreeMap<ElementId, Generated>,
    pub alignment: Alignment,
    /// A second alignment with a horizontal layout only, and station equations without a
    /// decreasing run.
    pub plain: Alignment,
}

impl LinearFixture {
    pub fn inputs(&self) -> Vec<IfcElementInput<'_>> {
        self.document.elements().iter().map(|element| IfcElementInput { element, generated: self.generated.get(&element.id) }).collect()
    }
}

/// 690 m from station 2+000: tangent, left curve with spirals, tangent, right curve with a
/// compound spiral to a flatter right curve, tangent.
pub fn alignment() -> Alignment {
    let mut a = AlignmentBuilder::new("LN01", 2000.0, 3000.0, 1500.0, 0.2)
        .line(120.0)
        .spiral(50.0, None, Some(250.0), Turn::Left)
        .arc(250.0, 90.0, Turn::Left)
        .spiral(50.0, Some(250.0), None, Turn::Left)
        .line(60.0)
        .spiral(40.0, None, Some(180.0), Turn::Right)
        .arc(180.0, 70.0, Turn::Right)
        .spiral(30.0, Some(180.0), Some(400.0), Turn::Right)
        .arc(400.0, 60.0, Turn::Right)
        .spiral(40.0, Some(400.0), None, Turn::Right)
        .line(80.0)
        .build()
        .expect("the test alignment");
    a.profile = Some(
        Profile::new(
            "FG",
            vec![Pvi::point(2000.0, 100.0), Pvi::parabola(2150.0, 104.0, 100.0), Pvi::asymmetric(2300.0, 101.0, 40.0, 80.0), Pvi::circular(2450.0, 106.0, 800.0), Pvi::point(2690.0, 103.0)],
        )
        .expect("the test profile"),
    );
    a.station_equations = StationEquations::new(vec![
        // A gap: 2+200 is shown as 3+000.
        StationEquation::new(2200.0, 3000.0),
        // An overlap: back station 3+200, ahead 3+150.
        StationEquation::new(2400.0, 3150.0),
        // Stations run down from 2+550 and up again from 2+620.
        StationEquation { internal_m: 2550.0, back_m: None, ahead_m: 3100.0, direction: afrobim_alignment::StationDirection::Decreasing },
        StationEquation::new(2620.0, 5000.0),
    ])
    .expect("the test station equations");
    a.superelevation = Some(
        Superelevation::new(vec![
            SuperelevationSection {
                points: vec![
                    CrossFallPoint::new(2120.0, Some(-2.0), Some(-2.0)),
                    CrossFallPoint::new(2180.0, Some(-6.0), Some(6.0)),
                    CrossFallPoint::new(2240.0, Some(-6.0), Some(6.0)),
                    CrossFallPoint::new(2290.0, Some(-2.0), Some(-2.0)),
                ],
            },
            // One side only, and no stated transition from the section before.
            SuperelevationSection { points: vec![CrossFallPoint::new(2400.0, None, Some(-2.5)), CrossFallPoint::new(2440.0, None, Some(-3.5))] },
        ])
        .expect("the test superelevation"),
    );
    a
}

/// 330 m, horizontal only, from station 500: a right curve with spirals between tangents, and a
/// gap and an overlap in the stationing.
pub fn plain_alignment() -> Alignment {
    let mut a = AlignmentBuilder::new("LN02", 500.0, -2000.0, 4000.0, -0.6)
        .line(60.0)
        .spiral(40.0, None, Some(220.0), Turn::Right)
        .arc(220.0, 100.0, Turn::Right)
        .spiral(40.0, Some(220.0), None, Turn::Right)
        .line(90.0)
        .build()
        .expect("the plain test alignment");
    a.station_equations = StationEquations::new(vec![StationEquation::new(600.0, 900.0), StationEquation::new(700.0, 850.0)]).expect("the plain station equations");
    a
}

fn face(width: f64, height: f64, colour: Colour) -> Generated {
    let outline = vec![Pt2::new(0.0, 0.0), Pt2::new(width, 0.0), Pt2::new(width, height), Pt2::new(0.0, height)];
    Generated {
        drawing: Drawing { primitives: vec![Primitive::Fill { outline, colour }], extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(width, height) } },
        ..Generated::default()
    }
}

fn post(section: Section, embedment_mm: f64) -> PostDefinition {
    PostDefinition { section, length_mm: 1.0, embedment_mm, material: "steel (test text)".into(), finish: String::new(), placeholder: false }
}

fn block(length_mm: f64, width_mm: f64, depth_mm: f64) -> FoundationDefinition {
    FoundationDefinition {
        shape: FoundationShape::Block { length_mm, width_mm, depth_mm },
        top_below_ground_mm: 100.0,
        concrete: "concrete (test text)".into(),
        blinding_mm: None,
        socket: None,
        placeholder: false,
    }
}

fn arrangement(mounting_height_mm: f64, post_count: u32) -> SupportArrangement {
    SupportArrangement { mounting_height_mm, post_count, edge_inset_mm: 300.0, top_offset_mm: 0.0, setback_mm: 60.0, shared_foundation: false }
}

/// Where a sign stands at a station and offset, as the sign schedule puts it: on the profile's
/// level, plus `lift_mm`, turned to face the traffic.
pub fn placed(a: &Alignment, station_m: f64, offset_m: f64, lift_mm: f64) -> SitePlacement {
    let name = a.name.clone();
    let at = a.point_at(station_m).expect("a station on the alignment");
    let (x, y) = at.offset(offset_m);
    let z = a.profile.as_ref().map_or(0.0, |p| p.elevation_at(station_m).unwrap() * 1000.0);
    SitePlacement {
        x: x * 1000.0,
        y: y * 1000.0,
        z: z + lift_mm,
        rotation_deg: at.heading_rad.to_degrees() - 90.0,
        alignment: Some(name),
        station_m: Some(station_m),
        offset_m: Some(offset_m),
    }
}

pub fn linear_fixture() -> LinearFixture {
    let a = alignment();
    let mut d = Document::default();
    d.standard = "uganda-tsm-2004".to_string();
    d.add("alignment", "LN01", Pt::new(20000.0, 0.0), 300000.0, 200000.0, "Alignments", serde_json::to_value(&a).unwrap());

    // A. Two round posts on block foundations, on the first parabola, to the right.
    let s = d.add("sign", "ADS-201", Pt::new(0.0, 0.0), 2400.0, 1200.0, "Signs", json!({ "kind": "StackType", "catalog_key": "GFS-B1" }));
    d.find_mut(s).unwrap().site = Some(placed(&a, 2180.0, 4.5, 0.0));
    add_supports(&mut d, s, &arrangement(2100.0, 2), &post(Section::Chs { outside_diameter_mm: 100.0, wall_mm: 10.0 }, 1000.0), &block(600.0, 600.0, 900.0)).unwrap();

    // B. One RHS post on the asymmetric parabola, to the left, raised 120 mm (the cross-fall of 6 m at 2 %).
    let s = d.add("sign", "REG-202", Pt::new(4000.0, 0.0), 600.0, 600.0, "Signs", json!({ "kind": "Regulatory" }));
    d.find_mut(s).unwrap().site = Some(placed(&a, 2330.0, -6.0, 120.0));
    add_supports(
        &mut d,
        s,
        &arrangement(1800.0, 1),
        &post(Section::Rhs { width_mm: 80.0, depth_mm: 80.0, wall_mm: 5.0, outer_corner_radius_mm: 10.0, inner_corner_radius_mm: 5.0 }, 600.0),
        &block(600.0, 400.0, 900.0),
    )
    .unwrap();

    // C. A sign with no supports, on the circular vertical curve, in the compound curve.
    let s = d.add("sign", "MKR-203", Pt::new(6000.0, 0.0), 300.0, 900.0, "Signs", json!({ "kind": "Marker" }));
    d.find_mut(s).unwrap().site = Some(placed(&a, 2460.0, 3.0, 800.0));

    // D. A post no sign hosts, on the first tangent.
    let l = d.add(
        "post",
        "LOOSE-P2",
        Pt::new(8000.0, 0.0),
        80.0,
        2000.0,
        "Supports",
        PostDefinition { section: Section::SolidRect { width_mm: 80.0, depth_mm: 40.0 }, length_mm: 2000.0, embedment_mm: 500.0, material: String::new(), finish: String::new(), placeholder: true }.to_value(),
    );
    d.find_mut(l).unwrap().site = Some(placed(&a, 2050.0, 8.0, 0.0));

    // E. A category without a mapping, in the last tangent.
    let m = d.add("bollard", "MK-204", Pt::new(9000.0, 500.0), 400.0, 200.0, "0", json!({ "kind": "Arrow" }));
    d.find_mut(m).unwrap().site = Some(placed(&a, 2650.0, -2.0, 0.0));

    // F. A sign whose recorded station is wrong: it stands at 2+560 and says 2+500.
    let s = d.add("sign", "WRN-205", Pt::new(10000.0, 0.0), 600.0, 800.0, "Signs", json!({ "kind": "Warning" }));
    let mut p = placed(&a, 2560.0, 2.0, 0.0);
    p.station_m = Some(2500.0);
    d.find_mut(s).unwrap().site = Some(p);

    // G. Absolute placement stays: no station, a station beyond the end, no site placement.
    let s = d.add("sign", "REG-206", Pt::new(11000.0, 0.0), 600.0, 600.0, "Signs", json!({ "kind": "Regulatory" }));
    d.find_mut(s).unwrap().site = Some(SitePlacement { x: 3100.0e3, y: 1600.0e3, z: 100.0, rotation_deg: 33.0, alignment: Some("LN01".into()), station_m: None, offset_m: None });
    let s = d.add("sign", "REG-207", Pt::new(12000.0, 0.0), 600.0, 600.0, "Signs", json!({ "kind": "Regulatory" }));
    let mut off = placed(&a, 2400.0, 1.0, 0.0);
    off.station_m = Some(9000.0);
    d.find_mut(s).unwrap().site = Some(off);
    d.add("sign", "REG-208", Pt::new(13000.0, 0.0), 600.0, 600.0, "Signs", json!({ "kind": "Regulatory" }));

    // H. On the second alignment, which has no profile: placed on its 2D curve.
    let plain = plain_alignment();
    d.add("alignment", "LN02", Pt::new(20000.0, 5000.0), 300000.0, 200000.0, "Alignments", serde_json::to_value(&plain).unwrap());
    let s = d.add("sign", "REG-209", Pt::new(14000.0, 0.0), 600.0, 600.0, "Signs", json!({ "kind": "Regulatory" }));
    d.find_mut(s).unwrap().site = Some(placed(&plain, 660.0, 3.0, 0.0));

    let mut generated = BTreeMap::new();
    for element in d.elements() {
        let g = match element.category.as_str() {
            "sign" => Some(face(element.width, element.height, Colour::Green)),
            "bollard" => Some(face(element.width, element.height, Colour::Yellow)),
            "post" => Some(PostModule::default().generate(element).unwrap()),
            "foundation" => Some(FoundationModule::default().generate(element).unwrap()),
            _ => None,
        };
        if let Some(g) = g {
            generated.insert(element.id, g);
        }
    }
    LinearFixture { document: d, generated, alignment: a, plain }
}
