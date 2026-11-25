// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The whole build: corridor description fed to `build_model`, serde, CSV, dependencies, islands.

mod common;

use afrobim_alignment::Pvi;
use afrobim_corridor::{build_model, library, Assembly, Frequency, Layer, OffsetTarget, Side, Slope, Tin};
use afrobim_junction::*;
use common::*;

fn roads() -> (afrobim_alignment::Alignment, afrobim_alignment::Alignment) {
    let p = with_profile(primary(), vec![Pvi::point(0.0, 100.0), Pvi::point(600.0, 106.0)]);
    let s = with_profile(secondary(), vec![Pvi::point(0.0, 99.0), Pvi::point(600.0, 101.0)]);
    (p, s)
}

fn slot_assembly(name: &str) -> Assembly {
    Assembly::symmetric(name, vec![library::lane_to_offset("lane", OffsetTarget::Polyline(vec![]), Slope::Percent(-2.5), vec![Layer::new("Pave1", 0.1)])])
}

fn corridor_inputs() -> CorridorInputs {
    let at_ends = Frequency { at_geometry_points: true, ..Default::default() };
    CorridorInputs {
        name: None,
        library: vec![slot_assembly("approach"), slot_assembly("through"), slot_assembly("arm"), slot_assembly("mouth"), slot_assembly("return")],
        primary_approach_assembly: "approach".into(),
        primary_through_assembly: "through".into(),
        secondary_approach_assembly: "arm".into(),
        secondary_mouth_assembly: "mouth".into(),
        return_assembly: "return".into(),
        approach_frequency: at_ends.clone(),
        through_frequency: at_ends.clone(),
        mouth_frequency: Frequency { extra_stations: vec![304.0], at_geometry_points: true, ..Default::default() },
        return_frequency: Frequency { tangent_m: Some(1.0), curve_m: Some(1.0), spiral_m: Some(1.0), at_geometry_points: true, extra_stations: vec![9.0 * std::f64::consts::FRAC_PI_4], ..Default::default() },
        target_step_m: 0.5,
        end_inset_m: 0.005,
    }
}

fn built() -> (IntersectionResult, Vec<afrobim_alignment::Alignment>) {
    let (p, s) = roads();
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp.vertical = Some(vertical_inputs());
    inp.corridor = Some(corridor_inputs());
    let res = build_intersection(&inp, &[p.clone(), s.clone()]).unwrap();
    (res, vec![p, s])
}

#[test]
fn corridor_model_builds_and_reaches_the_edges() {
    let (res, _) = built();
    let c = res.corridor.as_ref().unwrap();
    // primary + two secondary arms + four returns
    assert_eq!(c.model.baselines.len(), 7);
    // Offset slots only: primary 3 regions x 2 sides, two secondary arms 2 regions x 2 sides, four returns x 1 side.
    assert_eq!(c.bindings.len(), 6 + 8 + 4);
    assert!(c.missing.iter().any(|m| m.contains("offset target and an elevation target together")));
    let ground = Tin::from_grid(E0 - 400.0, N0 - 400.0, 100.0, 100.0, 9, 9, |_, _| 90.0).unwrap();
    let built = build_model(&c.model, &c.alignments, &ground).unwrap();
    // Primary approach: the lanes end on the edges, 3.5 m left and 3.0 m right of the centreline.
    let primary = built.baselines.iter().find(|b| b.name.ends_with("primary")).unwrap();
    let sec = &primary.sections[1];
    let etw: Vec<_> = sec.design.points_with_code("ETW").collect();
    assert_eq!(etw.len(), 2);
    let left = etw.iter().find(|p| p.side == Some(Side::Left)).unwrap();
    let right = etw.iter().find(|p| p.side == Some(Side::Right)).unwrap();
    near(left.offset_m, -3.5, 1e-6, "left lane edge");
    near(right.offset_m, 3.0, 1e-6, "right lane edge");
    // Secondary mouth at station 304, 0.5 m past the primary's left edge crossing (303.5): the
    // secondary's lanes run to its own edges, 2.8 m right and 3.2 m left.
    let ahead = built.baselines.iter().find(|b| b.name.ends_with("secondary ahead")).unwrap();
    let mouth = ahead.sections.iter().find(|s| (s.section.station_m - 304.0).abs() < 1e-6).expect("section at 304");
    let r = mouth.design.points_with_code("ETW").find(|p| p.side == Some(Side::Right)).unwrap();
    near(r.offset_m, 2.8, 1e-6, "right lane edge in the mouth");
    let l = mouth.design.points_with_code("ETW").find(|p| p.side == Some(Side::Left)).unwrap();
    near(l.offset_m, -3.2, 1e-6, "left lane edge in the mouth");
    // Returns: each is a baseline with one side only.
    // The return P+S+ (R = 9, right angle): at the middle of the arc, 45 degrees round, the normal runs
    // through the centre to the corner, so the sliver is |centre - corner| - R = R (sqrt 2 - 1) wide.
    let ret = built.baselines.iter().find(|b| b.name.ends_with("return P+S+")).unwrap();
    assert!(ret.sections.len() > 5);
    let mid = ret.sections.iter().find(|s| (s.section.station_m - 9.0 * std::f64::consts::FRAC_PI_4).abs() < 1e-6).expect("section at the middle of the arc");
    let e: Vec<_> = mid.design.points_with_code("ETW").collect();
    assert_eq!(e.len(), 1);
    near(e[0].offset_m.abs(), 9.0 * (2.0f64.sqrt() - 1.0), 5e-3, "width of the sliver at mid arc");
}

#[test]
fn an_assembly_with_no_slot_is_noted() {
    let (p, s) = roads();
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp.vertical = Some(vertical_inputs());
    let mut ci = corridor_inputs();
    ci.library[0] = Assembly::symmetric("approach", vec![library::lane("lane", 3.5, Slope::Percent(-2.5), vec![])]);
    inp.corridor = Some(ci);
    let res = build_intersection(&inp, &[p, s]).unwrap();
    let c = res.corridor.unwrap();
    assert!(c.notes.iter().any(|n| n.contains("'approach'") && n.contains("no empty target slot")));
    // A name that is not in the library is an error.
    let mut ci2 = corridor_inputs();
    ci2.return_assembly = "nope".into();
    let (p, s) = roads();
    let mut inp2 = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp2.vertical = Some(vertical_inputs());
    inp2.corridor = Some(ci2);
    assert!(build_intersection(&inp2, &[p, s]).is_err());
}

#[test]
fn everything_round_trips_through_json() {
    let (res, _) = built();
    let json = serde_json::to_string(&res).unwrap();
    let back: IntersectionResult = serde_json::from_str(&json).unwrap();
    // serde_json's default float parsing can differ in the last digit, so compare as JSON values.
    assert!(json_close(&serde_json::to_value(&res).unwrap(), &serde_json::to_value(&back).unwrap()));
    let (p, s) = roads();
    let mut inp = cross_inputs(ReturnKind::Compound { radius1_m: 20.0, radius2_m: 8.0, radius3_m: 30.0, offset1_m: 1.0, offset3_m: 0.5 });
    inp.vertical = Some(vertical_inputs());
    inp.corridor = Some(corridor_inputs());
    let j = serde_json::to_string(&inp).unwrap();
    let inp_back: IntersectionInputs = serde_json::from_str(&j).unwrap();
    assert_eq!(inp_back, inp, "inputs hold short decimals, which round-trip exactly");
    // The rebuild from the round-tripped inputs is the same.
    let a = build_intersection(&inp, &[p.clone(), s.clone()]).unwrap();
    let b = build_intersection(&inp_back, &[p, s]).unwrap();
    assert_eq!(a, b);
    // The alignments and profiles themselves read back from JSON.
    let al_json = serde_json::to_string(&a.alignments).unwrap();
    let al_back: Vec<afrobim_alignment::Alignment> = serde_json::from_str(&al_json).unwrap();
    assert_eq!(al_back.len(), a.alignments.len());
}

#[test]
fn dependency_record_goes_stale_when_an_alignment_profile_or_assembly_changes() {
    let (res, alignments) = built();
    let lib = corridor_inputs().library;
    assert!(res.dependencies.stale(&alignments, &lib).is_empty());
    assert!(res.dependencies.items.iter().any(|d| d.kind == DependencyKind::Profile && d.name == "secondary"));
    assert!(res.dependencies.items.iter().any(|d| d.kind == DependencyKind::Assembly && d.name == "return"));
    // Move the secondary profile.
    let mut moved = alignments.clone();
    moved[1].profile = Some(afrobim_alignment::Profile::new("p", vec![Pvi::point(0.0, 99.5), Pvi::point(600.0, 101.0)]).unwrap());
    assert_eq!(res.dependencies.stale(&moved, &lib), vec!["Profile secondary".to_string()]);
    // Move the primary in plan.
    let mut moved = alignments.clone();
    moved[0].elements[0].x += 0.01;
    assert_eq!(res.dependencies.stale(&moved, &lib), vec!["Alignment primary".to_string()]);
    // Change an assembly.
    let mut lib2 = lib.clone();
    lib2[4].baseline_elevation_m = 0.05;
    assert_eq!(res.dependencies.stale(&alignments, &lib2), vec!["Assembly return".to_string()]);
    // A missing alignment is stale too.
    assert_eq!(res.dependencies.stale(&alignments[..1], &lib).len(), 2);
}

#[test]
fn tables_and_csv() {
    let (res, _) = built();
    let t = res.returns_table();
    assert_eq!(t.rows.len(), 4);
    let csv = t.to_csv();
    assert!(csv.starts_with("quadrant,kind,t_primary_x"));
    assert_eq!(csv.lines().count(), 5);
    assert!(csv.contains("simple R=9"));
    let c = res.crossings_table().to_csv();
    assert!(c.contains("0+300.000,0+300.000,90.0000,0.0000,Cross,true"), "{c}");
    assert_eq!(res.edges_table().rows.len(), 4);
    let f = res.findings_table().to_csv();
    assert!(f.starts_with("code,severity,subject,message,values"));
    // A comma in a cell is quoted.
    assert!(f.contains('"'));
    // Plain geometry for drawing.
    let d = res.drawing(0.5);
    assert_eq!(d.layers.len(), 5);
    assert_eq!(d.layers[1].polylines.len(), 4);
    assert!(d.layers[1].polylines[0].len() > 10);
}

#[test]
fn island_outline_by_offsets_has_the_rectangle_area() {
    // A rectangle on the secondary: stations 320 to 340, offsets 1 to 5 to its right.
    // Area 20 x 4 = 80 m2, perimeter 48 m, computed from points at UTM magnitude.
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp.islands = vec![IslandInput {
        name: "refuge".into(),
        road: RoadId::Secondary,
        points: vec![
            IslandPoint { station_m: 320.0, offset_m: 1.0 },
            IslandPoint { station_m: 340.0, offset_m: 1.0 },
            IslandPoint { station_m: 340.0, offset_m: 5.0 },
            IslandPoint { station_m: 320.0, offset_m: 5.0 },
        ],
    }];
    let res = build_intersection(&inp, &[primary(), secondary()]).unwrap();
    let i = &res.islands[0];
    near(i.area_m2, 80.0, 1e-6, "area");
    near(i.perimeter_m, 48.0, 1e-6, "perimeter");
    near(i.outline[0].0, E0 + 1.0, 1e-9, "first point x (right of +Y is +X)");
    near(i.outline[0].1, N0 + 20.0, 1e-9, "first point y");
    assert_eq!(i.outline.first(), i.outline.last());
    near(i.alignment.length(), 48.0, 1e-6, "alignment length");
}

#[test]
fn crossing_selection_and_errors() {
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp.crossing_index = 3;
    let e = build_intersection(&inp, &[primary(), secondary()]).unwrap_err();
    assert!(matches!(e, JunctionError::NoCrossing { found: 1, index: 3, .. }), "{e}");
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp.secondary = "missing".into();
    assert!(matches!(build_intersection(&inp, &[primary(), secondary()]), Err(JunctionError::NoAlignment(_))));
    // No profile: the vertical part needs one.
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp.vertical = Some(vertical_inputs());
    assert!(matches!(build_intersection(&inp, &[primary(), secondary()]), Err(JunctionError::NoProfile(_))));
    // Zero offset on an edge is refused.
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp.primary_edges = edges(0.0, 3.0);
    assert!(build_intersection(&inp, &[primary(), secondary()]).is_err());
}

#[test]
fn skew_and_grade_are_reported_with_values_and_no_limit() {
    let (res, _) = built();
    let skew = res.findings.iter().find(|f| f.code == "junction.skew").unwrap();
    assert_eq!(skew.severity, Severity::Info);
    assert!(skew.message.contains("no limit"));
    let grade = res.findings.iter().find(|f| f.code == "junction.on_grade").unwrap();
    let pg = grade.values.iter().find(|(k, _)| k == "primary_grade").unwrap().1;
    near(pg, 0.01, 1e-12, "primary grade");
    // Nothing in the findings is an error or warning for a clean junction.
    assert!(res.findings.iter().all(|f| f.severity == Severity::Info), "{:?}", res.findings.iter().filter(|f| f.severity != Severity::Info).collect::<Vec<_>>());
}

#[test]
fn widening_moves_the_return_tangent_point_and_is_reported() {
    // A left widening of 2 m on the primary from station 300 to 340 (taper 10 m up, 10 m down) puts the
    // edge 5.5 m out beside the secondary arm; the corner of P+S+ moves out with it.
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp.primary_edges.left_widening.push(Widening { start_station_m: 280.0, taper_length_m: 10.0, length_m: 60.0, exit_taper_length_m: 10.0, width_m: 2.0 });
    let res = build_intersection(&inp, &[primary(), secondary()]).unwrap();
    let g = &return_of(&res, "P+S+").geometry;
    near(g.primary_point.1, N0 + 5.5, 1e-7, "tangent point is on the widened edge");
    near(g.corner.1, N0 + 5.5, 1e-7, "corner moves with it");
    assert!(res.findings.iter().any(|f| f.code == "return.on_widening"));
    // The side without widening is unchanged.
    near(return_of(&res, "P+S-").geometry.primary_point.1, N0 - 3.0, 1e-7, "right side");
    let _ = Side::Left;
}

fn json_close(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value::*;
    match (a, b) {
        (Number(x), Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            (x - y).abs() <= 1e-12 * x.abs().max(y.abs()).max(1.0)
        }
        (Array(x), Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| json_close(p, q)),
        (Object(x), Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| json_close(v, w))),
        _ => a == b,
    }
}
