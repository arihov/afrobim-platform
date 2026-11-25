// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Roundabout geometry against closed forms. All dimensions are arbitrary test values.
//!
//! Centre (E0, N0), inscribed diameter 40 (radius 20), circulatory width 7, apron 2, so the carriageway
//! inner edge has radius 13, the central island edge radius 11, and the circulatory centreline radius
//! 16.5. Clockwise circulation.

mod common;

use afrobim_alignment::{Alignment, AlignmentElement};
use afrobim_junction::*;
use common::*;
use std::f64::consts::{FRAC_PI_2, PI};

const RO: f64 = 20.0;

fn arm(name: &str, alignment: &str, splitter: Vec<(f64, f64)>, nose: f64) -> ArmInput {
    ArmInput {
        name: name.into(),
        alignment: alignment.into(),
        approach_half_width_m: 3.5,
        flare_length_m: 30.0,
        entry_width_m: 7.0,
        entry_radius_m: 15.0,
        exit_width_m: 6.0,
        exit_radius_m: 20.0,
        departure_half_width_m: 3.5,
        splitter: SplitterInput { points: splitter.into_iter().map(|(d, h)| SplitterPoint { distance_m: d, half_width_m: h }).collect(), nose_radius_m: nose },
        path_offsets: None,
    }
}

fn inputs(arms: Vec<ArmInput>) -> RoundaboutInputs {
    RoundaboutInputs { name: "RB".into(), centre: (E0, N0), inscribed_diameter_m: 40.0, circulatory_width_m: 7.0, apron_width_m: 2.0, circulation: Circulation::Clockwise, arms }
}

/// Centre of an arc element: start plus the radius along the normal on the side it turns to.
fn arc_centre(e: &AlignmentElement) -> (f64, f64) {
    let r = 1.0 / e.curvature_start.abs();
    let side = e.curvature_start.signum();
    (e.x - side * r * e.heading_rad.sin(), e.y + side * r * e.heading_rad.cos())
}

fn south_arm() -> Alignment {
    // From 150 m south, heading north, ending on the inscribed circle at (E0, N0 - 20).
    line("south", E0, N0 - 150.0, FRAC_PI_2, 130.0, 0.0)
}

fn four_arms() -> Vec<Alignment> {
    vec![
        south_arm(),
        line("west", E0 - 150.0, N0, 0.0, 130.0, 0.0),
        line("north", E0, N0 + 150.0, -FRAC_PI_2, 130.0, 0.0),
        line("east", E0 + 150.0, N0, PI, 130.0, 0.0),
    ]
}

#[test]
fn circles_have_the_defined_radii_and_turn_the_way_of_circulation() {
    let res = build_roundabout(&inputs(vec![arm("S", "south", vec![(0.0, 2.0), (10.0, 0.0)], 0.5)]), &[south_arm()]).unwrap();
    for (a, r) in [(&res.inscribed_circle, 20.0), (&res.carriageway_inner_edge, 13.0), (&res.central_island_edge, 11.0), (&res.circulatory_centreline, 16.5)] {
        near(a.length(), 2.0 * PI * r, 1e-9, "circumference");
        assert!(a.elements.iter().all(|e| (e.min_radius().unwrap() - r).abs() < 1e-12 && e.curvature_start < 0.0), "clockwise arcs turn right");
        assert!(a.discontinuities(1e-9, 1e-9).is_empty());
        let end = a.point_at(a.length()).unwrap();
        near(end.x, E0 + r, 1e-7, "closes on the start x");
        near(end.y, N0, 1e-7, "closes on the start y");
    }
    let mut ccw = inputs(vec![arm("S", "south", vec![(0.0, 2.0), (10.0, 0.0)], 0.5)]);
    ccw.circulation = Circulation::Anticlockwise;
    let r = build_roundabout(&ccw, &[south_arm()]).unwrap();
    assert!(r.inscribed_circle.elements.iter().all(|e| e.curvature_start > 0.0));
}

#[test]
fn entry_and_exit_arcs_are_tangent_to_the_inscribed_circle_and_to_the_flare_lines() {
    let res = build_roundabout(&inputs(vec![arm("S", "south", vec![(0.0, 2.0), (10.0, 0.0)], 0.5)]), &[south_arm()]).unwrap();
    let a = &res.arms[0];
    near(a.give_way_point.0, E0, 1e-9, "give-way x");
    near(a.give_way_point.1, N0 - RO, 1e-9, "give-way y");
    near(a.azimuth_rad, -FRAC_PI_2, 1e-12, "azimuth");
    // Entry kerb: a flare line then an arc. The line starts at the flare start, 30 m upstream and
    // (splitter half-width 0 there) 3.5 m to the entry side, the west.
    let k = &a.entry_kerb;
    assert_eq!(k.elements.len(), 2);
    near(k.elements[0].x, E0 - 3.5, 1e-9, "flare start x");
    near(k.elements[0].y, N0 - RO - 30.0, 1e-9, "flare start y");
    assert!(k.discontinuities(1e-6, 1e-9).is_empty(), "line and arc meet smoothly: {:?}", k.joints());
    // The flare line passes through the entry width point on the give-way line: 2.0 + 7.0 west of the axis.
    let line0 = &k.elements[0];
    let (dx, dy) = (line0.heading_rad.cos(), line0.heading_rad.sin());
    let p = (E0 - 9.0, N0 - RO);
    let off = (p.0 - line0.x) * dy - (p.1 - line0.y) * dx;
    near(off, 0.0, 1e-9, "the flare line passes through the entry width point");
    // The arc: radius 15, ends on the inscribed circle heading along the circulation (tangent).
    let arc = &k.elements[1];
    near(1.0 / arc.curvature_start.abs(), 15.0, 1e-9, "entry radius");
    let (ex, ey, eh) = arc.end();
    near(((ex - E0).powi(2) + (ey - N0).powi(2)).sqrt(), RO, 1e-8, "arc ends on the inscribed circle");
    let c = arc_centre(arc);
    near(((c.0 - E0).powi(2) + (c.1 - N0).powi(2)).sqrt(), RO + 15.0, 1e-8, "centre is outside the circle by the radius (external tangency)");
    // Clockwise circulation: heading at the point of the circle is the azimuth minus a quarter turn.
    let az = (ey - N0).atan2(ex - E0);
    near(afrobim_alignment::wrap_pi(eh - (az - FRAC_PI_2)), 0.0, 1e-8, "tangent to the circle");
    // Exit: the arc starts on the circle along the circulation and ends on a flare line to 3.5 m + 0 from the axis.
    let x = &a.exit_kerb;
    assert!(x.discontinuities(1e-6, 1e-9).is_empty(), "{:?}", x.joints());
    let xarc = &x.elements[0];
    near(1.0 / xarc.curvature_start.abs(), 20.0, 1e-9, "exit radius");
    let az0 = (xarc.y - N0).atan2(xarc.x - E0);
    near(((xarc.x - E0).powi(2) + (xarc.y - N0).powi(2)).sqrt(), RO, 1e-8, "exit arc starts on the circle");
    near(afrobim_alignment::wrap_pi(xarc.heading_rad - (az0 - FRAC_PI_2)), 0.0, 1e-8, "exit arc leaves along the circulation");
    let xl = x.elements.last().unwrap();
    let (lx, ly, _) = xl.end();
    near(lx, E0 + 3.5, 1e-9, "exit flare end x (east of the axis)");
    near(ly, N0 - RO - 30.0, 1e-9, "exit flare end y");
    // The two kerbs are on opposite sides of the arm.
    assert!(k.elements[0].x < E0 && lx > E0);
    // Reported figures.
    let m = &a.measures;
    near(m.entry_angle_rad, FRAC_PI_2, 1e-12, "radial arm: entry angle");
    near(m.flare_ratio, (7.0 - 3.5) / 30.0, 1e-12, "flare ratio");
    near(m.angle_to_next_arm_rad, 2.0 * PI, 1e-12, "a single arm");
}

#[test]
fn entry_angle_follows_the_arm_direction_against_the_circulation() {
    // An arm 15 m west of the centre running north, meeting the circle at (-15, -sqrt(175)): the
    // clockwise flow there has tangent (r_y, -r_x) = (-0.6614, 0.75) (r the unit radial (-0.75, -0.6614)),
    // so the angle to the axis (0, 1) is acos(0.75). 15 m east it is acos(-0.75).
    let sq = (RO * RO - 15.0 * 15.0f64).sqrt();
    let west = line("w", E0 - 15.0, N0 - 150.0, FRAC_PI_2, 150.0 - sq, 0.0);
    let east = line("e", E0 + 15.0, N0 - 150.0, FRAC_PI_2, 150.0 - sq, 0.0);
    let mut a1 = arm("W", "w", vec![(0.0, 2.0), (10.0, 0.0)], 0.5);
    // The flare lines must come within the inscribed radius of the centre for a fillet to exist.
    a1.entry_width_m = 2.5;
    a1.exit_width_m = 1.0;
    a1.entry_radius_m = 3.0;
    a1.exit_radius_m = 3.0;
    let r1 = build_roundabout(&inputs(vec![a1]), &[west]).unwrap();
    near(r1.arms[0].measures.entry_angle_rad, 0.75f64.acos(), 1e-9, "west of centre");
    let mut a2 = arm("E", "e", vec![(0.0, 2.0), (10.0, 0.0)], 0.5);
    a2.entry_width_m = 2.5;
    a2.exit_width_m = 1.0;
    a2.entry_radius_m = 3.0;
    a2.exit_radius_m = 3.0;
    let r2 = build_roundabout(&inputs(vec![a2]), &[east]).unwrap();
    near(r2.arms[0].measures.entry_angle_rad, (-0.75f64).acos(), 1e-9, "east of centre");
}

#[test]
fn splitter_nose_is_rounded_with_the_closed_form_tangent_length() {
    // Sides from (0, 3) to (3, 0): the half-angle at the nose is 45 degrees, the tangent length r_n / tan(45) = r_n = 0.5,
    // the arc turns pi - 2 * 45 deg = 90 degrees, so its length is 0.5 * pi / 2.
    let res = build_roundabout(&inputs(vec![arm("S", "south", vec![(0.0, 3.0), (3.0, 0.0)], 0.5)]), &[south_arm()]).unwrap();
    let s = &res.arms[0].splitter;
    let nose_arc = s.alignment.elements.iter().find(|e| e.kind() == afrobim_alignment::ElementKind::Arc).unwrap();
    near(1.0 / nose_arc.curvature_start.abs(), 0.5, 1e-12, "nose radius");
    near(nose_arc.length, 0.5 * FRAC_PI_2, 1e-12, "nose arc length");
    // Tangent point: tip (a = -3, w = 0) pulled back along the side by r_n: (-3 + 0.5 / sqrt 2, 0.5 / sqrt 2) in (along, toward west).
    let t = 0.5 / 2.0f64.sqrt();
    near(nose_arc.x, E0 - t, 1e-9, "tangent point x (west is the entry side)");
    near(nose_arc.y, N0 - RO - 3.0 + t, 1e-9, "tangent point y");
    // The outline is closed and its area is the triangle 6 x 3 / 2 = 9 less the rounded tip:
    // the tip area removed is r_n^2 (1/tan(a) - (pi/2 - a)) with a = 45 deg: 0.25 * (1 - 0.785398) = 0.053650.
    near(s.area_m2, 9.0 - 0.25 * (1.0 - PI / 4.0), 2e-3, "area");
    near(s.nose.0, E0, 1e-9, "nose x");
    near(s.nose.1, N0 - RO - 3.0, 1e-9, "nose y");
    // A nose radius too large for the side is left sharp and reported.
    let big = build_roundabout(&inputs(vec![arm("S", "south", vec![(0.0, 3.0), (3.0, 0.0)], 5.0)]), &[south_arm()]).unwrap();
    assert!(big.findings.iter().any(|f| f.code == "roundabout.nose_radius"));
}

#[test]
fn four_arm_roundabout_reports_angles_and_the_entry_path_radius() {
    let mut arms = Vec::new();
    for (n, al) in [("S", "south"), ("W", "west"), ("N", "north"), ("E", "east")] {
        let mut a = arm(n, al, vec![(0.0, 2.0), (10.0, 0.0)], 0.5);
        a.path_offsets = Some(PathOffsets { from_entry_kerb_m: 1.0, from_splitter_m: 1.5, from_central_island_m: 2.0 });
        arms.push(a);
    }
    let res = build_roundabout(&inputs(arms), &four_arms()).unwrap();
    for a in &res.arms {
        near(a.measures.entry_angle_rad, FRAC_PI_2, 1e-9, &format!("{} entry angle", a.name));
        near(a.measures.angle_to_next_arm_rad, FRAC_PI_2, 1e-9, &format!("{} angle to next", a.name));
    }
    // South arm: P1 at the flare start 1.0 in from the kerb: lateral 0 + 3.5 - 1.0 = 2.5 west, 30 south of the circle.
    // P2 on the give-way line 1.5 out from the splitter (half-width 2.0): 3.5 west. P3 on the ray from
    // the centre at -135 degrees (half way to the west arm), at 11 + 2 = 13 from the centre.
    let p1 = (E0 - 2.5, N0 - RO - 30.0);
    let p2 = (E0 - 3.5, N0 - RO);
    let p3 = (E0 + 13.0 * (-0.75 * PI).cos(), N0 + 13.0 * (-0.75 * PI).sin());
    let (a, b, c) = (dist(p1, p2), dist(p2, p3), dist(p3, p1));
    let area = 0.5 * ((p2.0 - p1.0) * (p3.1 - p1.1) - (p2.1 - p1.1) * (p3.0 - p1.0)).abs();
    let expected = a * b * c / (4.0 * area);
    let m = &res.arms[0].measures;
    near(m.entry_path_radius_m.unwrap(), expected, 1e-6, "circumradius of the three points");
    let pts = m.entry_path_points.unwrap();
    near(pts[2].0, p3.0, 1e-9, "P3 x");
    near(pts[2].1, p3.1, 1e-9, "P3 y");
    // Without the offsets the radius is not computed.
    let plain = build_roundabout(&inputs(vec![arm("S", "south", vec![(0.0, 2.0), (10.0, 0.0)], 0.5)]), &[south_arm()]).unwrap();
    assert!(plain.arms[0].measures.entry_path_radius_m.is_none());
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[test]
fn roundabout_tables_drawing_serde_and_input_checks() {
    let mut arms = Vec::new();
    for (n, al) in [("S", "south"), ("W", "west"), ("N", "north"), ("E", "east")] {
        arms.push(arm(n, al, vec![(0.0, 2.0), (10.0, 0.0)], 0.5));
    }
    let inp = inputs(arms);
    let res = build_roundabout(&inp, &four_arms()).unwrap();
    let csv = res.arms_table().to_csv();
    assert!(csv.starts_with("arm,azimuth_deg,entry_angle_deg"));
    assert_eq!(csv.lines().count(), 5);
    let d = res.drawing(0.5);
    assert_eq!(d.layers[0].polylines.len(), 4);
    assert_eq!(d.layers[3].polylines.len(), 4);
    let json = serde_json::to_string(&res).unwrap();
    let back: RoundaboutResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back.arms.len(), 4);
    let j = serde_json::to_string(&inp).unwrap();
    assert_eq!(serde_json::from_str::<RoundaboutInputs>(&j).unwrap(), inp);
    assert_eq!(res.dependencies.items.len(), 4);
    // Inputs are checked, never defaulted.
    let mut bad = inp.clone();
    bad.apron_width_m = 30.0;
    assert!(build_roundabout(&bad, &four_arms()).is_err());
    let mut bad = inp.clone();
    bad.arms[0].entry_width_m = 0.0;
    assert!(build_roundabout(&bad, &four_arms()).is_err());
    let mut bad = inp.clone();
    bad.arms[0].splitter.points.last_mut().unwrap().half_width_m = 0.5;
    assert!(build_roundabout(&bad, &four_arms()).is_err());
    let mut bad = inp;
    bad.arms[0].alignment = "nope".into();
    assert!(matches!(build_roundabout(&bad, &four_arms()), Err(JunctionError::NoAlignment(_))));
}
