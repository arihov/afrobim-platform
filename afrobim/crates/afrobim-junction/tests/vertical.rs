// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The vertical part against hand calculation.

mod common;

use afrobim_alignment::Pvi;
use afrobim_corridor::{library, Assembly, Side, Slope};
use afrobim_junction::*;
use common::*;

fn roads() -> (afrobim_alignment::Alignment, afrobim_alignment::Alignment) {
    // Primary rises 1 % (100 m at station 0 ... 106 m at 600; 103 m at the crossing, station 300).
    let p = with_profile(primary(), vec![Pvi::point(0.0, 100.0), Pvi::point(600.0, 106.0)]);
    // Secondary falls then rises: 99 m at 0, 101 m at 600, so 100.0 at the crossing (station 300).
    let s = with_profile(secondary(), vec![Pvi::point(0.0, 99.0), Pvi::point(600.0, 101.0)]);
    (p, s)
}

#[test]
fn secondary_profile_is_locked_to_the_primary_crown_and_crossfall() {
    // Right-angle cross, so every point of the secondary centreline has the primary station 300
    // and the primary surface there is 103.0 on the crown and falls 2.5 % either side.
    //   secondary meets the primary's left edge (offset 3.5) at station 303.5: 103 - 0.025 * 3.5 = 102.9125
    //   secondary meets the primary's right edge (offset 3.0) at station 297.0: 103 - 0.025 * 3.0 = 102.925
    //   tie-in 20 m outside each edge, at the secondary's own profile: z(s) = 99 + s / 300
    let (p, s) = roads();
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp.vertical = Some(vertical_inputs());
    let res = build_intersection(&inp, &[p, s]).unwrap();
    let lock = res.lock.as_ref().unwrap();
    near(lock.crown_level_m, 103.0, 1e-9, "crown");
    near(lock.original_level_at_crossing_m, 100.0, 1e-9, "own level at the crossing");
    near(lock.level_shift_m, 3.0, 1e-9, "shift");
    near(lock.region_start_station_m, 297.0, 1e-9, "lo");
    near(lock.region_end_station_m, 303.5, 1e-9, "hi");
    let pr = &lock.profile;
    near(pr.elevation_at(297.0).unwrap(), 102.925, 1e-9, "right edge");
    near(pr.elevation_at(300.0).unwrap(), 103.0, 1e-9, "crown");
    near(pr.elevation_at(303.5).unwrap(), 102.9125, 1e-9, "left edge");
    near(pr.elevation_at(277.0).unwrap(), 99.0 + 277.0 / 300.0, 1e-9, "tie-in start");
    near(pr.elevation_at(323.5).unwrap(), 99.0 + 323.5 / 300.0, 1e-9, "tie-in end");
    near(pr.elevation_at(100.0).unwrap(), 99.0 + 100.0 / 300.0, 1e-9, "original beyond");
    near(lock.tie_in_grade_start.unwrap(), (102.925 - (99.0 + 277.0 / 300.0)) / 20.0, 1e-9, "tie-in grade start");
    near(lock.tie_in_grade_end.unwrap(), ((99.0 + 323.5 / 300.0) - 102.9125) / 20.0, 1e-9, "tie-in grade end");
    // PVIs: 2 kept below, 5 generated (tie, edge, crown, edge, tie) ... plus the ends of the original.
    assert_eq!(pr.pvis.len(), 7);
    // The locked alignment is in the list for the corridor.
    assert!(res.alignments.iter().any(|a| a.name == pr.name));
}

#[test]
fn all_crowns_maintained_leaves_the_secondary_alone_and_reports_the_difference() {
    let (p, s) = roads();
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp.corridor_type = CorridorType::AllCrownsMaintained;
    inp.vertical = Some(vertical_inputs());
    let res = build_intersection(&inp, &[p, s]).unwrap();
    assert!(res.lock.is_none());
    let f = res.findings.iter().find(|f| f.code == "profile.crowns").unwrap();
    let d = f.values.iter().find(|(k, _)| k == "difference_m").unwrap().1;
    near(d, 100.0 - 103.0, 1e-9, "secondary minus primary at the crossing");
}

#[test]
fn return_profile_matches_the_edges_at_both_tangent_points() {
    // P+S+ with R = 9. T1 on the primary's left edge at primary station 300 + 2.8 + 9 = 311.8,
    // 3.5 m left of the crown: level 100 + 0.01 * 311.8 - 0.025 * 3.5 = 103.118 - 0.0875 = 103.0305.
    // The return runs against the edge's stationing there (heading -X): start grade -0.01.
    // T2 on the secondary's right edge, 2.8 m from the secondary centreline, secondary station
    // 300 + 3.5 + 9 = 312.5, inside the tie-in: centreline level
    //   102.9125 + g * (312.5 - 303.5), g = ((99 + 323.5/300) - 102.9125) / 20, minus 0.03 * 2.8.
    // It runs along the edge's stationing there: end grade g.
    let (p, s) = roads();
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp.vertical = Some(vertical_inputs());
    let res = build_intersection(&inp, &[p, s]).unwrap();
    let rv = return_of(&res, "P+S+").vertical.as_ref().unwrap();
    let g_tie = ((99.0 + 323.5 / 300.0) - 102.9125) / 20.0;
    near(rv.start_level_m, 100.0 + 0.01 * 311.8 - 0.025 * 3.5, 1e-9, "start level");
    near(rv.start_grade, -0.01, 1e-9, "start grade");
    near(rv.end_level_m, 102.9125 + g_tie * 9.0 - 0.03 * 2.8, 1e-9, "end level");
    near(rv.end_grade, g_tie, 1e-7, "end grade");
    // The profile hits both levels and both grades.
    let l = return_of(&res, "P+S+").geometry.length_m;
    near(rv.profile.elevation_at(0.0).unwrap(), rv.start_level_m, 1e-9, "profile start");
    near(rv.profile.elevation_at(l).unwrap(), rv.end_level_m, 1e-9, "profile end");
    near(rv.profile.grade_at(1e-9).unwrap(), rv.start_grade, 1e-6, "profile start grade");
    near(rv.profile.grade_at(l - 1e-9).unwrap(), rv.end_grade, 1e-6, "profile end grade");
    assert_eq!(rv.method, BlendMethod::SingleVerticalCurve);
    // It is on the alignment that the corridor gets.
    assert!(return_of(&res, "P+S+").geometry.alignment.profile.is_some());
}

#[test]
fn single_vertical_curve_blend_has_the_hand_computed_low_point() {
    // z0 = 10, g0 = -0.02, z1 = 9.9, g1 = +0.01, L = 40.
    //   tangent lines meet at s* = (z1 - z0 - g1 L) / (g0 - g1) = (-0.1 - 0.4) / (-0.03) = 16.6667
    //   asymmetric parabola with L1 = 16.6667, L2 = 23.3333, A = g1 - g0 = 0.03:
    //   second piece: g_m = g0 + A L2 / L = -0.0025 at s*, rate r2 = A L1 / (L L2) = 0.000535714
    //   grade zero at s* + 0.0025 / r2 = 16.6667 + 4.6667 = 21.3333
    //   PVI level z0 + g0 s* = 9.66667, z_m = PVI + A L1 L2 / (2 L) = 9.8125,
    //   low level z_m - g_m^2 / (2 r2) = 9.8125 - 0.0058333 = 9.806667.
    let rv = blend_return_profile("t", 40.0, 10.0, -0.02, 9.9, 0.01, 1.0, &|s| (s, 0.0)).unwrap();
    assert_eq!(rv.method, BlendMethod::SingleVerticalCurve);
    near(rv.pvi_station_m.unwrap(), 50.0 / 3.0, 1e-9, "PVI station");
    assert_eq!(rv.low_points.len(), 1);
    near(rv.low_points[0].station_m, 64.0 / 3.0, 1e-9, "low point station");
    near(rv.low_points[0].level_m, 9.8125 - 0.000_006_25 / (2.0 * 0.03 * (50.0 / 3.0) / (40.0 * (70.0 / 3.0))), 1e-9, "low point level");
    near(rv.low_points[0].level_m, 9.806_666_666_67, 1e-8, "low point level, decimal");
    near(rv.low_points[0].x, rv.low_points[0].station_m, 1e-12, "position from the mapping");
    // End levels and grades.
    assert_eq!(rv.profile.pvis.len(), 3);
    near(rv.profile.elevation_at(40.0).unwrap(), 9.9, 1e-9, "end level");
}

#[test]
fn a_crest_has_no_low_point_and_the_lower_end_is_reported() {
    // z0 = 10, g0 = +0.02, z1 = 10.5, g1 = -0.01, L = 40: s* = 30, a crest.
    let rv = blend_return_profile("t", 40.0, 10.0, 0.02, 10.5, -0.01, 1.0, &|s| (s, 0.0)).unwrap();
    assert!(rv.low_points.is_empty());
    near(rv.lowest_end.level_m, 10.0, 1e-12, "lower end");
    near(rv.lowest_end.station_m, 0.0, 1e-12, "lower end station");
}

#[test]
fn straight_and_cubic_fallbacks() {
    // Straight: 0.5 m in 50 m at 1 %.
    let l = blend_return_profile("t", 50.0, 10.0, 0.01, 10.5, 0.01, 1.0, &|s| (s, 0.0)).unwrap();
    assert_eq!(l.method, BlendMethod::StraightGrade);
    assert_eq!(l.profile.pvis.len(), 2);
    // Equal grades but the levels do not agree with them: the tangent lines are parallel, an S curve.
    let c = blend_return_profile("t", 40.0, 10.0, 0.01, 10.0, 0.01, 1.0, &|s| (s, 0.0)).unwrap();
    assert_eq!(c.method, BlendMethod::CubicSampled);
    // Hermite cubic with equal end levels and equal end grades: z(t) = z0 + L g (t^3 - 2 t^2 + t) + L g (t^3 - t^2)
    // = z0 + L g (2 t^3 - 3 t^2 + t); at t = 0.5: z0 + 40 * 0.01 * (0.25 - 0.75 + 0.5) = z0.
    near(c.profile.elevation_at(20.0).unwrap(), 10.0, 1e-9, "mid level");
    // At t = 0.25: 40 * 0.01 * (2/64 - 3/16 + 1/4) = 0.4 * 0.09375 = 0.0375.
    near(c.profile.elevation_at(10.0).unwrap(), 10.0375, 1e-9, "quarter level");
    assert!(c.profile.pvis.len() > 10);
    assert!(blend_return_profile("t", 0.0, 10.0, 0.0, 10.0, 0.0, 1.0, &|s| (s, 0.0)).is_err());
}

#[test]
fn crossfall_can_come_from_the_assembly_or_the_alignment() {
    let mut asm = Assembly::new("A");
    asm.right.push(library::lane("lane", 3.5, Slope::Percent(-2.0), vec![]));
    asm.left.push(library::lane("lane", 3.5, Slope::Percent(-3.0), vec![]));
    let (l, r) = crossfall_from_assembly(&asm).unwrap();
    near(l, -3.0, 1e-12, "left");
    near(r, -2.0, 1e-12, "right");
    let (p, s) = roads();
    let cf = CrossfallInput::FromAssembly(asm);
    near(cf.at(&p, 300.0, Side::Left).unwrap(), -3.0, 1e-12, "left via input");
    // The alignment stores none: refused, not defaulted.
    assert!(CrossfallInput::FromAlignment.at(&s, 300.0, Side::Right).is_err());
    assert!(crossfall_from_assembly(&Assembly::new("empty")).is_err());
}

#[test]
fn edge_profiles_and_low_points_are_exact_on_a_vertical_curve() {
    // A sag in the primary: PVI at 300 with a 200 m parabola, from +1 % to ... use PVIs 0:105,
    // 300:100, 600:105 (grades -1.67 % / +1.67 %), a 200 m curve at 300 makes the lowest point at 300.
    let p = with_profile(primary(), vec![Pvi::point(0.0, 105.0), Pvi::parabola(300.0, 100.0, 200.0), Pvi::point(600.0, 105.0)]);
    let s = with_profile(secondary(), vec![Pvi::point(0.0, 99.0), Pvi::point(600.0, 101.0)]);
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp.vertical = Some(vertical_inputs());
    let res = build_intersection(&inp, &[p, s]).unwrap();
    let pl = res.edges.iter().find(|e| e.road == RoadId::Primary && e.side == Side::Left).unwrap();
    assert_eq!(pl.low_points.len(), 1);
    near(pl.low_points[0].y, N0 + 3.5, 1e-6, "low point on the left edge");
    near(pl.low_points[0].x, E0, 1e-4, "at the sag");
    // Vertex of the parabola: PVI level + A L / 8 = 100 + (2 * 5 / 300) * 200 / 8, less 2.5 % of 3.5 m.
    near(pl.low_points[0].level_m, 100.0 + (2.0 * 5.0 / 300.0) * 200.0 / 8.0 - 0.025 * 3.5, 1e-6, "level of the sag on the edge");
}
