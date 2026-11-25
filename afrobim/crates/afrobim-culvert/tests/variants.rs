// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Variants (1, 2, 3 cells; pipes and boxes; skew; asymmetric road), refusals and hydraulics.

use afrobim_culvert::*;
use afrobim_draw::Primitive;

fn pipe(id: f64, cells: u32) -> CulvertInput {
    let mut i = CulvertInput::reference();
    let spacing = if cells > 1 { 300.0 } else { 0.0 };
    i.barrel = Barrel::Pipe(PipeSpec::catalogue(id, cells, spacing).expect("catalogue size"));
    i.name = format!("{cells} x {id}");
    i
}

fn boxed(cells: u32) -> CulvertInput {
    let mut i = CulvertInput::reference();
    i.barrel = Barrel::Box(BoxSpec {
        clear_span_mm: 2000.0,
        clear_height_mm: 1500.0,
        cells,
        wall_mm: 250.0,
        top_slab_mm: 250.0,
        bottom_slab_mm: 250.0,
        haunch_mm: 150.0,
    });
    for e in [&mut i.left_end, &mut i.right_end] {
        e.headwall_height_mm = 2600.0;
        e.headwall_above_apron_mm = 2100.0;
        e.wing.height_at_headwall_mm = 2600.0;
    }
    // A higher headwall needs lower inverts to stay under the road edge.
    i.inverts = InvertSpec::Ends {
        inlet_m: 1107.1,
        outlet_m: 1106.85,
    };
    i.name = format!("{cells} cell box");
    i
}

fn row<'a>(d: &'a CulvertDesign, mark: &str) -> &'a BbsRow {
    d.bbs
        .rows
        .iter()
        .find(|r| r.mark == mark)
        .unwrap_or_else(|| panic!("no row {mark}"))
}

fn assert_sound(d: &CulvertDesign) {
    let failed: Vec<&Check> = d
        .checks
        .iter()
        .filter(|c| !c.passed && c.name == "plan overlaps")
        .collect();
    assert!(failed.is_empty(), "{}: {failed:?}", d.name);
    assert!(plan_overlaps(&d.layout).is_empty());
    assert!(d.quantities.surround_m3 > 0.0 || d.quantities.box_concrete_m3 > 0.0);
    assert!(d.quantities.end_structures_m3 > 0.0 && d.bbs.total_kg > 0.0);
    for dr in [
        &d.drawings.plan,
        &d.drawings.section_aa,
        &d.drawings.section_dd,
        &d.drawings.front_elevation,
        &d.drawings.wing_section_cc,
        &d.drawings.toe_section_bb,
    ] {
        assert!(!dr.primitives.is_empty(), "{}: an empty drawing", d.name);
        assert!(dr.extents.width() > 0.0 && dr.extents.height() > 0.0);
    }
}

#[test]
fn pipe_variants_widen_the_headwall_and_the_schedule_follows() {
    for id in [900.0, 1200.0] {
        for cells in 1..=3u32 {
            let i = pipe(id, cells);
            let d = design(&i).unwrap_or_else(|e| panic!("{} cells of {id}: {e}", cells));
            assert_sound(&d);
            let Barrel::Pipe(p) = i.barrel else {
                unreachable!()
            };
            let od = p.outside_diameter_mm();
            let want =
                cells as f64 * od + (cells as f64 - 1.0) * p.clear_spacing_mm + 2.0 * 200.0 + 50.0;
            assert!(
                (d.layout.left.headwall_width_mm - want).abs() < 1e-9,
                "{cells} x {id}: headwall {} against {want}",
                d.layout.left.headwall_width_mm
            );
            // The bars follow the geometry.
            let overall = d.layout.left.overall_width_mm;
            assert_eq!(row(&d, "13").dims[1], Some((overall - 80.0).round()));
            assert_eq!(
                row(&d, "13").dims[1],
                Some(
                    (d.layout.left.headwall_width_mm + 2.0 * (2310.0 + 141.4213562373095) - 80.0)
                        .round()
                )
            );
            assert_eq!(row(&d, "15").bars_per_element, 4 * cells);
            assert_eq!(
                row(&d, "15").dims[0],
                Some((std::f64::consts::PI * (od / 2.0 + 14.6)).round())
            );
            assert_eq!(row(&d, "08a").bars_per_element, 2 * cells);
            let n07 = ((d.layout.left.headwall_width_mm - 80.0) / 250.0).floor() as u32 + 1;
            assert_eq!(row(&d, "07").bars_per_element, n07);
            // Pipe length per cell is the barrel length; one line per cell.
            let lines = d
                .quantities
                .lines
                .iter()
                .filter(|l| l.category == "culvert.pipe_length")
                .count();
            assert_eq!(lines as u32, cells);
        }
    }
    // Wider culverts need more concrete and steel.
    let one = design(&pipe(1200.0, 1)).expect("design");
    let three = design(&pipe(1200.0, 3)).expect("design");
    assert!(three.quantities.end_structures_m3 > one.quantities.end_structures_m3);
    assert!(three.bbs.total_kg > one.bbs.total_kg);
    assert!(three.quantities.surround_m3 > one.quantities.surround_m3);
}

#[test]
fn box_culverts_of_1_2_and_3_cells() {
    let mut last_width = 0.0;
    for cells in 1..=3u32 {
        let i = boxed(cells);
        let d = design(&i).unwrap_or_else(|e| panic!("{cells} cell box: {e}"));
        assert_sound(&d);
        let want_outer = cells as f64 * 2000.0 + (cells as f64 + 1.0) * 250.0;
        assert_eq!(i.barrel.outer_width_mm(), want_outer);
        assert_eq!(d.layout.left.headwall_width_mm, want_outer + 400.0 + 50.0);
        assert!(d.layout.left.headwall_width_mm > last_width);
        last_width = d.layout.left.headwall_width_mm;
        // Box concrete: outer rectangle less the openings, plus the haunches, over the length.
        let area = want_outer / 1000.0 * 2.0 - cells as f64 * (2.0 * 1.5 - 4.0 * 0.5 * 0.15 * 0.15);
        assert!(
            (d.quantities.box_concrete_m3 - area * d.layout.barrel_len_mm / 1000.0).abs() < 1e-6
        );
        // The barrel is scheduled and the pipe-only half-round bars are not.
        assert!(d.bbs.rows.iter().any(|r| r.mark == "BX1"));
        assert!(!d.bbs.rows.iter().any(|r| r.mark == "15"));
        assert!(d.bbs.element_kg(ElementKind::BoxBarrel) > 0.0);
        assert!(d
            .quantities
            .lines
            .iter()
            .any(|l| l.category == "culvert.box_concrete"));
        let wall_rows = row(&d, "BX4");
        assert_eq!(wall_rows.elements, 2 * (cells + 1));
    }
}

#[test]
fn skewed_culvert_scales_the_length_by_the_skew() {
    let square = design(&pipe(1200.0, 1)).expect("design").layout;
    let mut i = pipe(1200.0, 1);
    i.skew_deg = 60.0;
    i.setting_out = Some(SettingOutInput {
        cl_easting_m: 1000.0,
        cl_northing_m: 2000.0,
        road_bearing_deg: 30.0,
    });
    let d = design(&i).expect("skewed design");
    assert_sound(&d);
    let k = 60f64.to_radians().sin();
    assert!((d.layout.left_len_mm - square.left_len_mm / k).abs() < 1e-6);
    assert!((d.layout.right_len_mm - square.right_len_mm / k).abs() < 1e-6);
    // The axis bearing is the road bearing plus the skew; point 2 is the right end.
    assert!((axis_bearing_deg(30.0, 60.0) - 90.0).abs() < 1e-12);
    let (p1, p2) = (&d.setting_out[0], &d.setting_out[1]);
    let dist =
        ((p2.easting_m - p1.easting_m).powi(2) + (p2.northing_m - p1.northing_m).powi(2)).sqrt();
    assert!((dist * 1000.0 - d.layout.barrel_len_mm).abs() < 1e-6);
    // Axis due east: northing is the same at both ends.
    assert!((p2.northing_m - p1.northing_m).abs() < 1e-9 && p2.easting_m > p1.easting_m);
}

#[test]
fn asymmetric_road_with_a_walkway_on_one_side() {
    let side = |kerb| SideTemplate {
        carriageway_width_m: 3.5,
        carriageway_crossfall_pct: 2.5,
        shoulder_width_m: 1.5,
        shoulder_crossfall_pct: 4.0,
        kerb,
    };
    let walkway = KerbWalkway {
        width_m: 1.8,
        height_step_m: 0.15,
        crossfall_pct: 2.0,
    };
    let template = RoadTemplate {
        frl_m: 1110.195,
        left: side(None),
        right: side(Some(walkway)),
        pavement_depth_m: 0.5,
        embankment_slope_n: 1.77,
        cut_slope_n: None,
    };
    let mut i = pipe(1200.0, 1);
    i.road = RoadSource::Template(template.clone());
    let d = design(&i).expect("design with a walkway");
    assert_sound(&d);
    // The two sides differ in width and in the level of the edge, so the lengths differ.
    let section = template.to_section().expect("section");
    assert!((section.edge(Side::Right).offset_m - 6.8).abs() < 1e-9);
    assert!((section.edge(Side::Left).offset_m + 5.0).abs() < 1e-9);
    assert!((d.layout.left_len_mm - d.layout.right_len_mm).abs() > 500.0);
    // The same section handed in as points or as a corridor section gives the same culvert.
    let pts: Vec<(f64, f64)> = section
        .points
        .iter()
        .map(|p| (p.offset_m, p.level_m))
        .collect();
    let mut a = i.clone();
    a.road = RoadSource::Points(RoadSection::from_points(&pts, 0.5, 1.77, None).expect("points"));
    let mut b = i.clone();
    b.road = RoadSource::Corridor(
        RoadSection::from_corridor_section(&pts, 0.5, 1.77, None).expect("corridor"),
    );
    assert_eq!(design(&a).expect("a").layout, d.layout);
    assert_eq!(design(&b).expect("b").layout, d.layout);
}

#[test]
fn length_rules() {
    let mut i = pipe(1200.0, 1);
    i.length = LengthRule::Fixed { total_mm: 16000.0 };
    let d = design(&i).expect("fixed");
    assert_eq!(
        (d.layout.left_len_mm, d.layout.right_len_mm),
        (8000.0, 8000.0)
    );
    i.length = LengthRule::LeftRight {
        left_mm: 7000.0,
        right_mm: 9000.0,
    };
    let d = design(&i).expect("left right");
    assert_eq!(d.layout.barrel_len_mm, 16000.0);
    // Inverts by centre level and gradient: the ends follow.
    i.inverts = InvertSpec::CentreGradient {
        centre_m: 1107.47,
        gradient_pct: 1.5,
    };
    i.length = LengthRule::FromEmbankment;
    let d = design(&i).expect("gradient");
    assert!((d.layout.gradient - 0.015).abs() < 1e-12);
    assert!(
        (d.layout.invert_left_m - (1107.47 + 0.015 * d.layout.left_len_mm / 1000.0)).abs() < 1e-9
    );
    assert!(
        (d.layout.invert_right_m - (1107.47 - 0.015 * d.layout.right_len_mm / 1000.0)).abs() < 1e-9
    );
    // The inlet on the right reverses the fall.
    i.inlet_side = Side::Right;
    let d = design(&i).expect("inlet right");
    assert!(d.layout.invert_right_m > d.layout.invert_left_m);
}

#[test]
fn checks_report_cover_and_gradient_thresholds() {
    let mut i = pipe(1200.0, 1);
    i.thresholds = Thresholds {
        min_cover_mm: 5000.0,
        min_gradient_pct: 2.0,
    };
    let d = design(&i).expect("design");
    let failed: Vec<&str> = d
        .checks
        .iter()
        .filter(|c| !c.passed)
        .map(|c| c.name.as_str())
        .collect();
    assert!(
        failed.contains(&"minimum cover") && failed.contains(&"self-cleansing gradient"),
        "{failed:?}"
    );
    // Failed checks do not stop the design.
    assert!(d.bbs.total_kg > 0.0);
}

fn refused(i: &CulvertInput, words: &str) {
    match design(i) {
        Ok(_) => panic!("expected a refusal mentioning {words:?}"),
        Err(e) => assert!(
            e.to_string().contains(words),
            "message {:?} should mention {words:?}",
            e.to_string()
        ),
    }
}

#[test]
fn impossible_inputs_are_refused_with_a_message() {
    let mut i = pipe(1200.0, 1);
    i.ogl = vec![(0.0, 1111.0)];
    refused(&i, "original ground");
    let mut i = pipe(1200.0, 1);
    i.length = LengthRule::Fixed { total_mm: 17000.0 };
    i.inverts = InvertSpec::Ends {
        inlet_m: 1110.5,
        outlet_m: 1110.3,
    };
    refused(&i, "cover");
    let mut i = pipe(1200.0, 1);
    i.left_end.wing.length_mm = 2000.0;
    refused(&i, "shorter than the apron");
    let mut i = pipe(1200.0, 1);
    i.inverts = InvertSpec::Ends {
        inlet_m: 1107.0,
        outlet_m: 1107.2,
    };
    refused(&i, "above the outlet");
    let mut i = pipe(1200.0, 3);
    i.barrel = Barrel::Pipe(PipeSpec {
        internal_diameter_mm: 1200.0,
        wall_mm: 100.0,
        cells: 3,
        clear_spacing_mm: 0.0,
    });
    refused(&i, "clear spacing");
    let mut i = pipe(1200.0, 1);
    i.skew_deg = 0.0;
    refused(&i, "skew");
    let mut i = pipe(1200.0, 1);
    i.length = LengthRule::Fixed { total_mm: 3000.0 };
    refused(&i, "overlap");
    let mut i = pipe(1200.0, 1);
    i.left_end.headwall_height_mm = 1000.0;
    refused(&i, "above the apron");
    let mut i = pipe(1200.0, 1);
    i.left_end.headwall_above_apron_mm = 1000.0;
    refused(&i, "below the top of the barrel");
    let mut i = pipe(1200.0, 1);
    i.left_end.headwall_above_apron_mm = 3500.0;
    i.left_end.headwall_height_mm = 4000.0;
    refused(&i, "above the left road edge");
    assert!(PipeSpec::catalogue(1000.0, 1, 0.0).is_err());
    assert!(RoadSection::from_points(&[(1.0, 5.0), (2.0, 5.0)], 0.3, 2.0, None).is_err());
    // The ground above the edge needs a cut slope.
    let s =
        RoadSection::from_points(&[(-5.0, 10.0), (5.0, 10.0)], 0.3, 2.0, None).expect("section");
    assert!(s
        .toe(&Ogl::from_points(&[(0.0, 12.0)]).expect("ogl"), Side::Left)
        .is_err());
}

#[test]
fn drawings_carry_text_dimensions_and_setting_out_points() {
    let mut i = pipe(1200.0, 1);
    i.setting_out = Some(SettingOutInput {
        cl_easting_m: 526020.8,
        cl_northing_m: 94387.9,
        road_bearing_deg: 99.857,
    });
    let d = design(&i).expect("design");
    let texts = |dr: &afrobim_draw::Drawing| -> Vec<String> {
        dr.primitives
            .iter()
            .filter_map(|p| {
                if let Primitive::Glyphs { text, .. } = p {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect()
    };
    let plan = texts(&d.drawings.plan);
    for n in 1..=6 {
        assert!(plan.contains(&n.to_string()), "plan lacks point {n}");
    }
    assert!(
        plan.iter().any(|t| t == "PLAN")
            && plan.iter().any(|t| t == "FLOW")
            && plan.iter().any(|t| t.contains("KM 39+136"))
    );
    assert!(plan.iter().any(|t| t.starts_with("BARREL")));
    let aa = texts(&d.drawings.section_aa);
    assert!(
        aa.iter().any(|t| t.starts_with("INV 1107.595"))
            && aa.iter().any(|t| t.starts_with("GRADIENT 1.50")),
        "{aa:?}"
    );
    assert!(
        aa.iter().any(|t| t.starts_with("SECTION A-A"))
            && aa.iter().any(|t| t.starts_with("FRL 1110.195"))
    );
    let dd = texts(&d.drawings.section_dd);
    for w in ["200", "100", "1200", "COVER 1424"] {
        assert!(dd.iter().any(|t| t == w), "section D-D lacks {w}: {dd:?}");
    }
    let fe = texts(&d.drawings.front_elevation);
    assert!(
        fe.contains(&"2310".to_string())
            && fe.contains(&"6753".to_string())
            && fe.iter().any(|t| t == "HEADWALL 1850"),
        "{fe:?}"
    );
    assert!(texts(&d.drawings.wing_section_cc).contains(&"3267".to_string()));
    assert!(texts(&d.drawings.toe_section_bb).contains(&"500".to_string()));
    // Every Glyphs primitive has outlines, and every Line has a finite length.
    for dr in [
        &d.drawings.plan,
        &d.drawings.section_aa,
        &d.drawings.section_dd,
    ] {
        for p in &dr.primitives {
            match p {
                Primitive::Glyphs { contours, text, .. } => {
                    assert!(text.trim().is_empty() || !contours.is_empty())
                }
                Primitive::Line { from, to, .. } => assert!(from.distance_to(*to).is_finite()),
                _ => {}
            }
        }
    }
}

#[test]
fn quantity_lines_use_the_culvert_categories() {
    let d = design(&pipe(1200.0, 2)).expect("design");
    let units: std::collections::BTreeMap<&str, &str> = d
        .quantities
        .lines
        .iter()
        .map(|l| (l.category.as_str(), l.unit.as_str()))
        .collect();
    for (cat, unit) in [
        ("culvert.pipe_length", "m"),
        ("culvert.surround", "m3"),
        ("culvert.bedding", "m3"),
        ("culvert.blinding_barrel", "m3"),
        ("culvert.blinding_end", "m3"),
        ("culvert.headwall", "m3"),
        ("culvert.wing", "m3"),
        ("culvert.apron", "m3"),
        ("culvert.toe", "m3"),
        ("culvert.end_structures", "m3"),
        ("culvert.excavation", "m3"),
        ("culvert.backfill", "m3"),
        ("culvert.weep_holes", "m"),
        ("culvert.weep_holes_nr", "nr"),
        ("culvert.steel", "kg"),
    ] {
        assert_eq!(units.get(cat), Some(&unit), "{cat}");
    }
    let c = &CulvertInput::reference().concrete;
    let class_of = |cat: &str| {
        d.quantities
            .find(cat)
            .and_then(|l| l.attributes.get("class"))
            .cloned()
    };
    assert_eq!(class_of("culvert.surround"), Some(c.surround.clone()));
    assert_eq!(class_of("culvert.headwall"), Some(c.headwall.clone()));
    // Concrete classes are text inputs.
    let mut i = pipe(1200.0, 1);
    i.concrete.surround = "C20".to_string();
    assert_eq!(
        design(&i)
            .expect("design")
            .quantities
            .find("culvert.surround")
            .and_then(|l| l.attributes.get("class"))
            .map(String::as_str),
        Some("C20")
    );
    // Steel lines sum to the schedule.
    let kg: f64 = d
        .quantities
        .lines
        .iter()
        .filter(|l| l.category == "culvert.steel")
        .map(|l| l.quantity)
        .sum();
    assert!((kg - d.bbs.total_kg).abs() < 1e-6);
    assert!(d.quantities.excavation_m3 > 0.0 && d.quantities.backfill_m3 >= 0.0);
}

#[test]
fn ends_can_differ_and_the_schedule_marks_the_right_end() {
    let mut i = pipe(1200.0, 1);
    i.right_end.apron.length_mm = 2400.0;
    i.right_end.wing.length_mm = 3500.0;
    let d = design(&i).expect("design");
    assert_sound(&d);
    assert!(d.bbs.rows.iter().any(|r| r.mark == "13"));
    assert!(d.bbs.rows.iter().any(|r| r.mark == "13R"));
    assert_eq!(row(&d, "13").elements, 1);
    assert_ne!(row(&d, "13").dims, row(&d, "13R").dims);
}

#[test]
fn auto_select_uses_the_drainage_hydraulics() {
    use afrobim_drainage::design::inlet::InletEdge;
    let h = |q: f64| HydraulicsInput {
        design_flow_m3_s: q,
        manning_n: 0.013,
        entrance_loss_ke: 0.5,
        tailwater_m: 0.3,
        inlet_edge: InletEdge::ConcreteSquareEdgeHeadwall,
        allowable_headwater_m: 2.5,
    };
    let i = pipe(1200.0, 1);
    let small = select_pipe(&i, &h(0.3)).expect("small flow");
    let large = select_pipe(&i, &h(3.0)).expect("large flow");
    assert!(pipe_catalogue()
        .iter()
        .any(|e| e.internal_diameter_mm == small.internal_diameter_mm));
    assert!(large.internal_diameter_mm >= small.internal_diameter_mm);
    assert_eq!(large.cells, 1);
    assert!(select_pipe(&i, &h(500.0)).is_err());
    assert!(matches!(
        select_box(&i, &h(1.0)),
        Err(CulvertError::NotImplemented(_))
    ));
}

#[test]
fn defaults_are_labelled_and_the_input_round_trips() {
    let notes = default_notes();
    assert!(notes
        .iter()
        .any(|n| n.field.contains("min_cover") && n.source == DefaultSource::Assumed));
    assert!(notes
        .iter()
        .any(|n| n.field.contains("min_gradient") && n.source == DefaultSource::Drawing));
    let i = CulvertInput::reference();
    let text = serde_json::to_string(&i).expect("serialise");
    let back: CulvertInput = serde_json::from_str(&text).expect("deserialise");
    assert_eq!(back, i);
    assert_eq!(
        pipe_catalogue()
            .iter()
            .map(|e| e.internal_diameter_mm)
            .collect::<Vec<_>>(),
        vec![450.0, 600.0, 750.0, 900.0, 1050.0, 1200.0, 1350.0, 1500.0, 1800.0]
    );
}
