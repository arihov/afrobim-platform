// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! A test road: an alignment with a profile and a station equation, a ground surface, a corridor
//! and two road markings.

#![allow(dead_code)]

use afrobim_alignment::{AlignmentBuilder, AlignmentModule, Profile, Pvi, StationEquation, StationEquations, Turn};
use afrobim_corridor::{
    add_corridor, add_surface, CorridorDefinition, CorridorModule, Daylight, Link, Slope, SurfaceModule, Template, Tin,
};
use afrobim_kernel::{Document, ElementId, ModuleRegistry, Pt};
use afrobim_markings::{add_marking, LineType, MarkingDefinition, MarkingModule};
use afrobim_sheets::{ProjectInfo, ProjectInfoModule, Revision, SheetsModule, UserField};

pub const X0: f64 = 450_000.0;
pub const Y0: f64 = 100_000.0;

pub struct Fixture {
    pub doc: Document,
    pub registry: ModuleRegistry,
    pub alignment: ElementId,
}

pub fn project() -> ProjectInfo {
    ProjectInfo {
        project: "Test road, rehabilitation and upgrading".into(),
        client: "Test client".into(),
        contract_number: "TEST/2026/001".into(),
        drawn_by: "AB".into(),
        checked_by: "CD".into(),
        approved_by: "EF".into(),
        date: "2026-11-14".into(),
        revisions: vec![
            Revision { rev: "A".into(), date: "2026-10-01".into(), description: "First issue".into(), by: "AB".into(), checked: "CD".into() },
            Revision { rev: "B".into(), date: "2026-11-14".into(), description: "Alignment revised at curve 2".into(), by: "AB".into(), checked: "CD".into() },
        ],
        user_fields: vec![UserField { label: "Phase".into(), value: "Detailed design".into() }],
        logo_text: "LOGO".into(),
    }
}

pub fn fixture() -> Fixture {
    // 300 m straight, 250 m left curve R400, 400 m straight, 200 m right curve R500, 350 m straight.
    let mut alignment = AlignmentBuilder::new("MC1", 0.0, X0, Y0, 0.35)
        .line(300.0)
        .arc(400.0, 250.0, Turn::Left)
        .line(400.0)
        .arc(500.0, 200.0, Turn::Right)
        .line(350.0)
        .build()
        .unwrap();
    alignment.profile = Some(
        Profile::new(
            "MC1 profile",
            vec![Pvi::point(0.0, 1100.0), Pvi::parabola(400.0, 1106.0, 120.0), Pvi::parabola(900.0, 1098.0, 160.0), Pvi::point(1500.0, 1104.0)],
        )
        .unwrap(),
    );
    // Displayed stations jump from 1000 to 1050 at internal 1000.
    alignment.station_equations = StationEquations::new(vec![StationEquation::new(1000.0, 1050.0)]).unwrap();

    let mut doc = Document::default();
    let id = doc.add(afrobim_alignment::CATEGORY, "MC1", Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&alignment).unwrap());

    // Ground: a grid over the alignment's box, a gentle cross-fall and a ripple.
    let pts = alignment.polyline(5.0);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in &pts {
        x0 = x0.min(p.0);
        y0 = y0.min(p.1);
        x1 = x1.max(p.0);
        y1 = y1.max(p.1);
    }
    let (gx, gy, d) = (x0 - 120.0, y0 - 120.0, 20.0);
    let (nx, ny) = (((x1 - x0 + 240.0) / d) as usize + 2, ((y1 - y0 + 240.0) / d) as usize + 2);
    let tin = Tin::from_grid(gx, gy, d, d, nx, ny, |x, y| 1101.0 + 0.002 * (x - X0) + 2.5 * ((x - X0) / 90.0).sin() + 0.01 * (y - Y0)).unwrap();
    let surface = add_surface(&mut doc, "Ground", &tin, None).unwrap();

    let template = Template::symmetric(
        "Typical",
        vec![Link::new("lane", 3.5, Slope::Percent(-2.5)).paved(), Link::new("shoulder", 1.5, Slope::Percent(-4.0))],
        Daylight { cut_run_per_rise: 2.0, fill_run_per_rise: 3.0 },
        0.5,
    );
    add_corridor(&mut doc, id, surface, CorridorDefinition::single("Road", template, 50.0)).unwrap();

    let centre = MarkingDefinition::new(LineType::Centre1008, 0.0, 1500.0, 0.0, 30).with_width(100);
    add_marking(&mut doc, id, "RM-001", &centre).unwrap();
    let edge = MarkingDefinition::new(LineType::Edge1012_1, 0.0, 1500.0, 3.5, 30).with_width(100);
    add_marking(&mut doc, id, "RM-002", &edge).unwrap();

    afrobim_sheets::set_project_info(&mut doc, &project());

    let mut registry = ModuleRegistry::default();
    registry.register(Box::new(AlignmentModule));
    registry.register(Box::new(MarkingModule));
    registry.register(Box::new(CorridorModule));
    registry.register(Box::new(SurfaceModule::default()));
    registry.register(Box::new(SheetsModule));
    registry.register(Box::new(ProjectInfoModule));
    Fixture { doc, registry, alignment: id }
}
