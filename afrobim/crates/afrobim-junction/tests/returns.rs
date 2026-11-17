//! Kerb returns against closed forms.
//!
//! Right-angle cross, primary along +X, secondary along +Y, crossing at (E0, N0). Edge offsets
//! (arbitrary test values): primary left 3.5, right 3.0; secondary left 3.2, right 2.8. For the
//! quadrant P+S+ the edges are the primary's left edge `y = N0 + 3.5` and the secondary's right
//! edge `x = E0 + 2.8`. With a simple return of radius R:
//!
//!   corner            (E0 + 2.8,     N0 + 3.5)
//!   centre            (E0 + 2.8 + R, N0 + 3.5 + R)
//!   tangent points    (E0 + 2.8 + R, N0 + 3.5) on the primary edge, (E0 + 2.8, N0 + 3.5 + R) on the secondary
//!   arc length        R * pi / 2          tangent lengths R and R
//!
//! The other three quadrants are the mirror images about the two centrelines, using the offset of
//! the side each edge is on.

mod common;

use afrobim_junction::*;
use common::*;
use std::f64::consts::{FRAC_PI_2, PI};

fn tangent_points(r: f64) -> [(&'static str, (f64, f64), (f64, f64)); 4] {
    [
        ("P+S+", (E0 + 2.8 + r, N0 + 3.5), (E0 + 2.8, N0 + 3.5 + r)),
        ("P-S+", (E0 - 3.2 - r, N0 + 3.5), (E0 - 3.2, N0 + 3.5 + r)),
        ("P-S-", (E0 - 3.2 - r, N0 - 3.0), (E0 - 3.2, N0 - 3.0 - r)),
        ("P+S-", (E0 + 2.8 + r, N0 - 3.0), (E0 + 2.8, N0 - 3.0 - r)),
    ]
}

#[test]
fn simple_returns_of_a_right_angle_cross_have_the_known_tangent_points() {
    let r = 9.0;
    let res = build_intersection(&cross_inputs(ReturnKind::Simple { radius_m: r }), &[primary(), secondary()]).unwrap();
    assert_eq!(res.returns.len(), 4);
    near(res.crossing.angle_rad, FRAC_PI_2, 1e-12, "angle");
    for (label, t1, t2) in tangent_points(r) {
        let g = &return_of(&res, label).geometry;
        near(g.primary_point.0, t1.0, 1e-7, &format!("{label} T1 x"));
        near(g.primary_point.1, t1.1, 1e-7, &format!("{label} T1 y"));
        near(g.secondary_point.0, t2.0, 1e-7, &format!("{label} T2 x"));
        near(g.secondary_point.1, t2.1, 1e-7, &format!("{label} T2 y"));
        near(g.length_m, r * PI / 2.0, 1e-7, &format!("{label} arc length"));
        near(g.primary_tangent_length_m, r, 1e-7, &format!("{label} tangent length 1"));
        near(g.secondary_tangent_length_m, r, 1e-7, &format!("{label} tangent length 2"));
        near(g.turn_rad, FRAC_PI_2, 1e-9, &format!("{label} turn"));
        // The end of the arc, from the alignment itself, is the second tangent point.
        let end = g.alignment.point_at(g.length_m).unwrap();
        near(end.x, t2.0, 1e-7, &format!("{label} end x"));
        near(end.y, t2.1, 1e-7, &format!("{label} end y"));
    }
    // The centre of P+S+ is R from both edges.
    let c = return_of(&res, "P+S+").geometry.centres[0];
    near(c.0, E0 + 2.8 + r, 1e-7, "centre x");
    near(c.1, N0 + 3.5 + r, 1e-7, "centre y");
    // Four returns, none overlapping: no warnings or errors about them.
    assert!(res.findings.iter().all(|f| f.code != "return.overlap" && f.code != "return.does_not_fit"));
}

#[test]
fn simple_return_on_a_skewed_cross_has_tangent_length_r_over_tan_half_angle() {
    // Secondary at 60 degrees to the primary through the same point. The quadrant between the
    // primary ahead and the secondary ahead has an interior angle of 60 degrees; the tangent
    // length of a fillet in a corner of interior angle theta is R / tan(theta / 2). The edges are
    // offset by different amounts, which moves the corner but not the angle.
    let h = std::f64::consts::FRAC_PI_3;
    let s = line("secondary", E0 - 300.0 * h.cos(), N0 - 300.0 * h.sin(), h, 600.0, 0.0);
    let r = 12.0;
    let res = build_intersection(&cross_inputs(ReturnKind::Simple { radius_m: r }), &[primary(), s]).unwrap();
    near(res.crossing.skew_rad.to_degrees(), 30.0, 1e-9, "skew");
    let acute = &return_of(&res, "P+S+").geometry;
    near(acute.primary_tangent_length_m, r / (h / 2.0).tan(), 1e-7, "acute tangent length 1");
    near(acute.secondary_tangent_length_m, r / (h / 2.0).tan(), 1e-7, "acute tangent length 2");
    near(acute.turn_rad, PI - h, 1e-9, "turn = pi - interior angle");
    near(acute.length_m, r * (PI - h), 1e-7, "arc length");
    let obtuse = &return_of(&res, "P-S+").geometry;
    near(obtuse.primary_tangent_length_m, r / ((PI - h) / 2.0).tan(), 1e-7, "obtuse tangent length");
    near(obtuse.turn_rad, h, 1e-9, "obtuse turn");
}

#[test]
fn simple_returns_on_a_curved_primary_are_tangent_to_both_edges() {
    // Primary on an arc of radius 400 turning left (centre to the left); the secondary crosses it
    // square. Whatever the curve, the centre must be R from each edge and the tangent points are
    // the feet of the perpendiculars.
    use afrobim_alignment::{AlignmentBuilder, Turn};
    let p = AlignmentBuilder::new("primary", 0.0, E0 - 300.0, N0, 0.0).arc(400.0, 600.0, Turn::Left).build().unwrap();
    let mid = p.point_at(300.0).unwrap();
    let s = line("secondary", mid.x + 300.0 * (mid.heading_rad + FRAC_PI_2).cos() * -1.0, mid.y - 300.0 * (mid.heading_rad + FRAC_PI_2).sin(), mid.heading_rad + FRAC_PI_2, 600.0, 0.0);
    let r = 10.0;
    let res = build_intersection(&cross_inputs(ReturnKind::Simple { radius_m: r }), &[p.clone(), s.clone()]).unwrap();
    assert_eq!(res.returns.len(), 4);
    for rr in &res.returns {
        let g = &rr.geometry;
        let c = g.centres[0];
        let e1 = res.edges.iter().find(|e| e.road == RoadId::Primary && e.side == g.primary_side).unwrap();
        let e2 = res.edges.iter().find(|e| e.road == RoadId::Secondary && e.side == g.secondary_side).unwrap();
        let (_, o1) = e1.alignment.station_offset(c.0, c.1).unwrap();
        let (_, o2) = e2.alignment.station_offset(c.0, c.1).unwrap();
        near(o1.abs(), r, 1e-6, "centre to primary edge");
        near(o2.abs(), r, 1e-6, "centre to secondary edge");
        near(((g.primary_point.0 - c.0).powi(2) + (g.primary_point.1 - c.1).powi(2)).sqrt(), r, 1e-6, "T1 on the circle");
        // The tangent point is on the edge itself.
        let (_, t1) = e1.alignment.station_offset(g.primary_point.0, g.primary_point.1).unwrap();
        near(t1, 0.0, 1e-6, "T1 on the edge");
    }
    // The primary is on a curve at the crossing, which is reported.
    assert!(res.findings.iter().any(|f| f.code == "junction.on_curve" && f.subject.ends_with("primary") && f.message.contains("radius 400")));
}

#[test]
fn compound_return_with_equal_radii_is_the_simple_curve() {
    // Three arcs of radius R each turning 30 degrees are one 90 degree arc of radius R. The
    // offset of the end of the first arc from the edge is R (1 - cos 30 deg).
    let r = 9.0;
    let o = r * (1.0 - (PI / 6.0).cos());
    let kind = ReturnKind::Compound { radius1_m: r, radius2_m: r, radius3_m: r, offset1_m: o, offset3_m: o };
    let res = build_intersection(&cross_inputs(kind), &[primary(), secondary()]).unwrap();
    for (label, t1, t2) in tangent_points(r) {
        let g = &return_of(&res, label).geometry;
        near(g.primary_point.0, t1.0, 1e-7, &format!("{label} T1 x"));
        near(g.primary_point.1, t1.1, 1e-7, &format!("{label} T1 y"));
        near(g.secondary_point.0, t2.0, 1e-7, &format!("{label} T2 x"));
        near(g.secondary_point.1, t2.1, 1e-7, &format!("{label} T2 y"));
        near(g.length_m, r * PI / 2.0, 1e-7, &format!("{label} length"));
        assert_eq!(g.alignment.elements.len(), 3);
        assert!(g.alignment.discontinuities(1e-9, 1e-9).is_empty(), "arcs are tangent to each other");
    }
}

#[test]
fn compound_return_with_different_radii_closes_on_both_edges() {
    // R1 = 20, R2 = 8, R3 = 30 with offsets 1.0 and 0.5: no closed form for the tangent lengths,
    // so check the conditions that define the curve: starts on the primary edge heading along it,
    // ends on the secondary edge heading along it, arcs tangent to each other, and the first
    // arc's end is o1 from the edge.
    let kind = ReturnKind::Compound { radius1_m: 20.0, radius2_m: 8.0, radius3_m: 30.0, offset1_m: 1.0, offset3_m: 0.5 };
    let res = build_intersection(&cross_inputs(kind), &[primary(), secondary()]).unwrap();
    let g = &return_of(&res, "P+S+").geometry;
    let al = &g.alignment;
    assert_eq!(al.elements.len(), 3);
    assert!(al.discontinuities(1e-9, 1e-9).is_empty());
    let start = al.point_at(0.0).unwrap();
    near(start.y, N0 + 3.5, 1e-9, "starts on the primary edge");
    near(start.heading_rad, PI, 1e-9, "heading towards the corner, along the edge");
    let end = al.point_at(al.length()).unwrap();
    near(end.x, E0 + 2.8, 1e-9, "ends on the secondary edge");
    near(end.heading_rad, FRAC_PI_2, 1e-9, "heading along the secondary edge");
    // End of the first arc.
    let first = &al.elements[0];
    let (_, y1, _) = first.end();
    near(y1 - (N0 + 3.5), 1.0, 1e-9, "offset of the end of the first arc from the edge");
    let last = &al.elements[2];
    near(last.x - (E0 + 2.8), 0.5, 1e-9, "offset of the start of the last arc from the edge");
    // The tangent lengths follow from the closure, and the total turn is the right angle.
    near(g.turn_rad, FRAC_PI_2, 1e-12, "turn");
    assert!(g.primary_tangent_length_m > 0.0 && g.secondary_tangent_length_m > 0.0);
}

#[test]
fn compound_return_that_cannot_fit_is_reported_not_built() {
    // Offsets that use up more than the 90 degrees: acos(1 - 8/9) * 2 = 2 * 83.6 deg.
    let kind = ReturnKind::Compound { radius1_m: 9.0, radius2_m: 9.0, radius3_m: 9.0, offset1_m: 8.0, offset3_m: 8.0 };
    let res = build_intersection(&cross_inputs(kind), &[primary(), secondary()]).unwrap();
    assert!(res.returns.is_empty());
    let fails: Vec<_> = res.findings.iter().filter(|f| f.code == "return.does_not_fit").collect();
    assert_eq!(fails.len(), 4);
    assert!(fails[0].message.contains("too large for the angle"), "{}", fails[0].message);
}

#[test]
fn chamfer_with_three_and_four_metre_legs_is_five_metres_long() {
    let kind = ReturnKind::Chamfer { primary_leg_m: 4.0, secondary_leg_m: 3.0 };
    let res = build_intersection(&cross_inputs(kind), &[primary(), secondary()]).unwrap();
    let g = &return_of(&res, "P+S+").geometry;
    near(g.primary_point.0, E0 + 2.8 + 4.0, 1e-9, "T1 x");
    near(g.primary_point.1, N0 + 3.5, 1e-9, "T1 y");
    near(g.secondary_point.0, E0 + 2.8, 1e-9, "T2 x");
    near(g.secondary_point.1, N0 + 3.5 + 3.0, 1e-9, "T2 y");
    near(g.length_m, 5.0, 1e-9, "length");
    // And in the opposite quadrant, along the other directions.
    let g = &return_of(&res, "P-S-").geometry;
    near(g.primary_point.0, E0 - 3.2 - 4.0, 1e-9, "P-S- T1 x");
    near(g.secondary_point.1, N0 - 3.0 - 3.0, 1e-9, "P-S- T2 y");
}

#[test]
fn a_radius_too_large_for_the_extent_is_a_finding_and_the_others_still_build() {
    // Extents of 100 m: a 400 m radius puts the tangent points 400 m out.
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp.returns[0].kind = ReturnKind::Simple { radius_m: 400.0 };
    let res = build_intersection(&inp, &[primary(), secondary()]).unwrap();
    assert_eq!(res.returns.len(), 3);
    let f = res.findings.iter().find(|f| f.code == "return.does_not_fit").unwrap();
    assert!(f.message.contains("too large"), "{}", f.message);
    assert_eq!(f.severity, Severity::Error);
}

#[test]
fn overlap_test_finds_crossing_polylines_and_not_touching_ones() {
    let a = vec![(E0, N0), (E0 + 10.0, N0 + 10.0)];
    let b = vec![(E0, N0 + 10.0), (E0 + 10.0, N0)];
    assert!(polylines_intersect(&a, &b));
    // Meeting at an end is touching, not crossing.
    let c = vec![(E0 + 10.0, N0 + 10.0), (E0 + 20.0, N0)];
    assert!(!polylines_intersect(&a, &c));
    // Four returns of a right-angle cross lie in four separate corners: none overlaps another.
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 60.0 });
    inp.primary_edges = edges(0.5, 0.5);
    inp.secondary_edges = edges(0.5, 0.5);
    let res = build_intersection(&inp, &[primary(), secondary()]).unwrap();
    assert_eq!(res.returns.len(), 4);
    assert!(res.findings.iter().all(|f| f.code != "return.overlap"));
}

#[test]
fn a_tee_has_two_returns_and_a_missing_definition_is_refused() {
    let s = line("secondary", E0, N0, FRAC_PI_2, 300.0, 0.0);
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    // Only the quadrants with both arms need a definition: the secondary has only its ahead arm.
    inp.returns.retain(|r| r.quadrant.secondary_arm == Arm::Ahead);
    let res = build_intersection(&inp, &[primary(), s.clone()]).unwrap();
    assert_eq!(res.returns.len(), 2);
    assert_eq!(res.crossing.kind, CrossingKind::Tee);
    // A return given for a quadrant that does not exist is an error.
    let bad = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    let e = build_intersection(&bad, &[primary(), s.clone()]).unwrap_err();
    assert!(e.to_string().contains("does not exist"), "{e}");
    // A quadrant with no definition is an error: there is no default radius.
    let mut inp2 = cross_inputs(ReturnKind::Simple { radius_m: 9.0 });
    inp2.returns.pop();
    let e = build_intersection(&inp2, &[primary(), secondary()]).unwrap_err();
    assert!(e.to_string().contains("needs a return definition"), "{e}");
    // The kerb lines of a tee: the primary's right edge is whole, its left edge is in two pieces.
    let lines: Vec<_> = res.kerb_lines.iter().filter(|k| k.road == RoadId::Primary).collect();
    assert_eq!(lines.len(), 3);
}
