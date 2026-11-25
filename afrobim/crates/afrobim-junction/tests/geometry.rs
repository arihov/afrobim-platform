// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Crossings, offsets and edges, against closed forms. Coordinates are at true UTM magnitude.

mod common;

use afrobim_alignment::{Alignment, AlignmentBuilder, Pi, Turn};
use afrobim_corridor::Side;
use afrobim_junction::*;
use common::*;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_3};

#[test]
fn right_angle_cross_stations_and_angle() {
    // Primary stations 0..600 with the crossing at 300; give the primary a start station of 1000
    // to check the stations are internal stations, not distances.
    let p = line("p", E0 - 200.0, N0, 0.0, 400.0, 1000.0);
    let s = line("s", E0, N0 - 150.0, FRAC_PI_2, 300.0, 0.0);
    let c = find_crossings(&p, &s, 0.0);
    assert_eq!(c.len(), 1);
    near(c[0].x, E0, 1e-9, "x");
    near(c[0].y, N0, 1e-9, "y");
    near(c[0].station_a_m, 1200.0, 1e-9, "primary station");
    near(c[0].station_b_m, 150.0, 1e-9, "secondary station");
    near(c[0].angle_rad, FRAC_PI_2, 1e-12, "angle");
    near(c[0].skew_rad, 0.0, 1e-12, "skew");
    assert_eq!(c[0].kind, CrossingKind::Cross);
}

#[test]
fn skewed_cross_reports_acute_angle_and_skew() {
    // Secondary at 60 degrees to the primary, through the same point.
    let h = FRAC_PI_3;
    let s = line("s", E0 - 150.0 * h.cos(), N0 - 150.0 * h.sin(), h, 300.0, 0.0);
    let c = find_crossings(&primary(), &s, 0.0);
    assert_eq!(c.len(), 1);
    near(c[0].angle_rad, FRAC_PI_3, 1e-12, "angle");
    near(c[0].skew_rad.to_degrees(), 30.0, 1e-9, "skew");
    near(c[0].station_a_m, 300.0, 1e-9, "primary station");
    // The same lines the other way round give the same angle.
    let d = find_crossings(&s, &primary(), 0.0);
    near(d[0].angle_rad, FRAC_PI_3, 1e-12, "angle swapped");
    // A secondary at 120 degrees is also 60 degrees acute.
    let h2 = 2.0 * FRAC_PI_3;
    let s2 = line("s2", E0 - 150.0 * h2.cos(), N0 - 150.0 * h2.sin(), h2, 300.0, 0.0);
    near(find_crossings(&primary(), &s2, 0.0)[0].angle_rad, FRAC_PI_3, 1e-12, "angle 120");
}

#[test]
fn tee_is_found_at_an_end() {
    let s = line("s", E0, N0, FRAC_PI_2, 200.0, 0.0);
    let c = find_crossings(&primary(), &s, 0.0);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].kind, CrossingKind::Tee);
    near(c[0].station_b_m, 0.0, 1e-9, "at the secondary's start");
    assert!(!c[0].arms_b.behind && c[0].arms_b.ahead);
    // A secondary that stops 5 m short is found only by an extension, and flagged.
    let short = line("short", E0, N0 - 205.0, FRAC_PI_2, 200.0, 0.0);
    assert!(find_crossings(&primary(), &short, 0.0).is_empty());
    let c = find_crossings(&primary(), &short, 10.0);
    assert_eq!(c.len(), 1);
    assert!(c[0].on_extension_b && !c[0].on_extension_a);
    near(c[0].station_b_m, 205.0, 1e-9, "station on the extension");
}

#[test]
fn line_crossing_an_arc_matches_the_circle_equation() {
    // Arc of radius 500 about (E0, N0) from the east point, anticlockwise, a quarter circle; a
    // horizontal line 300 m above the centre meets it where x = sqrt(500^2 - 300^2) = 400.
    let arc = AlignmentBuilder::new("arc", 0.0, E0 + 500.0, N0, FRAC_PI_2).arc(500.0, 500.0 * FRAC_PI_2, Turn::Left).build().unwrap();
    let l = line("l", E0 - 400.0, N0 + 300.0, 0.0, 850.0, 0.0);
    let c = find_crossings(&arc, &l, 0.0);
    assert_eq!(c.len(), 1);
    near(c[0].x, E0 + 400.0, 1e-8, "x");
    near(c[0].y, N0 + 300.0, 1e-8, "y");
    near(c[0].station_a_m, 500.0 * (300.0f64).atan2(400.0), 1e-8, "arc station");
    near(c[0].station_b_m, 800.0, 1e-8, "line station");
    // Angle: the arc's tangent there is perpendicular to the radius (400, 300), the line is along +X.
    near(c[0].angle_rad, (400.0f64).atan2(300.0), 1e-9, "angle");
}

#[test]
fn crossing_on_a_spiral_lies_on_both_alignments() {
    let pis = [Pi::point(E0 - 400.0, N0 - 50.0), Pi::spiral_curve(E0, N0 + 20.0, 300.0, 80.0, 80.0), Pi::point(E0 + 400.0, N0 - 40.0)];
    let a = Alignment::from_pis("a", 0.0, &pis).unwrap();
    let b = line("b", E0 + 30.0, N0 - 300.0, FRAC_PI_2, 600.0, 0.0);
    let c = find_crossings(&a, &b, 0.0);
    assert!(!c.is_empty());
    for x in &c {
        let pa = a.point_at(x.station_a_m).unwrap();
        let pb = b.point_at(x.station_b_m).unwrap();
        near(pa.x, pb.x, 1e-9, "x agree");
        near(pa.y, pb.y, 1e-9, "y agree");
    }
}

#[test]
fn offsetting_an_arc_changes_radius_and_length_exactly() {
    // Left-hand arc, radius 200, 150 m long. To the right is away from the centre: R + d.
    let a = AlignmentBuilder::new("a", 0.0, E0, N0, 0.3).arc(200.0, 150.0, Turn::Left).build().unwrap();
    let right = offset_alignment(&a, 5.0, "r").unwrap();
    near(right.elements[0].min_radius().unwrap(), 205.0, 1e-9, "radius right");
    near(right.length(), 150.0 * 205.0 / 200.0, 1e-9, "length right");
    let left = offset_alignment(&a, -5.0, "l").unwrap();
    near(left.elements[0].min_radius().unwrap(), 195.0, 1e-9, "radius left");
    near(left.length(), 150.0 * 195.0 / 200.0, 1e-9, "length left");
    // The offset start point is square to the heading.
    let p0 = a.point_at(0.0).unwrap();
    near(right.elements[0].x, p0.x + 5.0 * 0.3f64.sin(), 1e-9, "start x");
    near(right.elements[0].y, p0.y - 5.0 * 0.3f64.cos(), 1e-9, "start y");
    // An offset to the inside as large as the radius collapses.
    assert!(offset_alignment(&a, -200.0, "x").is_err());
}

#[test]
fn offsetting_a_spiral_stays_a_constant_distance_away() {
    let pis = [Pi::point(E0 - 300.0, N0), Pi::spiral_curve(E0, N0, 250.0, 90.0, 90.0), Pi::point(E0 + 100.0, N0 + 300.0)];
    let a = Alignment::from_pis("a", 0.0, &pis).unwrap();
    for d in [4.0, -4.0] {
        let o = offset_alignment(&a, d, "o").unwrap();
        assert!(max_joint_gap(&o) < 1e-4, "gap {}", max_joint_gap(&o));
        for p in o.polyline(2.0) {
            let (_, off) = a.station_offset(p.0, p.1).unwrap();
            near(off, d, 2e-4, "distance from the base alignment");
        }
    }
}

#[test]
fn widening_table_shapes_the_edge() {
    let road = line("road", E0, N0, 0.0, 300.0, 0.0);
    let mut e = edges(3.5, 3.0);
    // Entry taper 20 m from 100, 30 m at full width, exit taper 10 m, 3 m wide.
    e.left_widening.push(Widening { start_station_m: 100.0, taper_length_m: 20.0, length_m: 30.0, exit_taper_length_m: 10.0, width_m: 3.0 });
    near(e.offset_at(Side::Left, 99.0), 3.5, 1e-12, "before");
    near(e.offset_at(Side::Left, 110.0), 3.5 + 1.5, 1e-12, "half way up the taper");
    near(e.offset_at(Side::Left, 135.0), 6.5, 1e-12, "full width");
    near(e.offset_at(Side::Left, 155.0), 3.5 + 1.5, 1e-12, "half way down");
    near(e.offset_at(Side::Left, 170.0), 3.5, 1e-12, "after");
    let b = build_edge(&road, Side::Left, &e, 0.0, 300.0, "edge").unwrap();
    // Every vertex of the edge is at the offset the table gives at its road station.
    for p in b.alignment.polyline(1.0) {
        let (s, o) = road.station_offset(p.0, p.1).unwrap();
        near(-o, e.offset_at(Side::Left, s), 1e-9, "edge offset at a vertex");
    }
    assert_eq!(b.alignment.elements.len(), 5);
    near(b.max_taper_chord_deviation_m, 0.0, 1e-9, "straight road: tapers are exact");
    // The right edge has no widening.
    let r = build_edge(&road, Side::Right, &e, 0.0, 300.0, "edge").unwrap();
    assert_eq!(r.alignment.elements.len(), 1);
    // A widening on a curve is a chord, and the deviation is reported.
    let curved = AlignmentBuilder::new("c", 0.0, E0, N0, 0.0).arc(150.0, 300.0, Turn::Left).build().unwrap();
    let cb = build_edge(&curved, Side::Left, &e, 0.0, 300.0, "edge").unwrap();
    assert!(cb.max_taper_chord_deviation_m > 0.0);
}
