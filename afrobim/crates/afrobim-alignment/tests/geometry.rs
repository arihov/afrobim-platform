//! Alignment geometry against answers known independently of the code under test.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use afrobim_alignment::{clothoid_xy, format_station, wrap_pi, Alignment, AlignmentBuilder, AlignmentError, ElementKind, Pi, PiCurve, Turn};

fn assert_close(got: f64, want: f64, tol: f64, what: &str) {
    assert!((got - want).abs() <= tol, "{what}: got {got}, want {want} (off by {:e})", got - want);
}

/// End of the alignment found the slow way: stepping along it and adding up
/// (cos heading, sin heading) ds with the midpoint rule. Uses only the heading from the crate.
fn walk(a: &Alignment, steps_per_element: usize) -> (f64, f64) {
    let first = a.point_at(a.start_station_m).unwrap();
    // Add up the displacement from zero, so large map coordinates do not swamp the small steps.
    let (mut x, mut y) = (0.0, 0.0);
    let mut s = a.start_station_m;
    for e in &a.elements {
        let ds = e.length / steps_per_element as f64;
        for i in 0..steps_per_element {
            let h = e.heading_at((i as f64 + 0.5) * ds);
            x += ds * h.cos();
            y += ds * h.sin();
        }
        s += e.length;
    }
    assert_close(s, a.end_station_m(), 1e-9, "stations add up");
    (first.x + x, first.y + y)
}

/// Every element type, both hands, a reverse and a compound spiral, on awkward coordinates.
fn everything() -> Alignment {
    AlignmentBuilder::new("everything", 1234.5, 457_200.0, 31_250.0, 0.7)
        .line(80.0)
        .spiral(60.0, None, Some(300.0), Turn::Left)
        .arc(300.0, 110.0, Turn::Left)
        .spiral(60.0, Some(300.0), None, Turn::Left)
        .spiral(50.0, None, Some(200.0), Turn::Right) // reverse, no tangent between
        .arc(200.0, 90.0, Turn::Right)
        .spiral(45.0, Some(200.0), Some(450.0), Turn::Right) // compound
        .arc(450.0, 70.0, Turn::Right)
        .spiral(55.0, Some(450.0), None, Turn::Right)
        .line(65.0)
        .arc(150.0, 400.0, Turn::Left) // more than a quarter turn
        .build()
        .unwrap()
}

#[test]
fn a_straight() {
    let a = Alignment::from_pis("s", 100.0, &[Pi::point(10.0, 20.0), Pi::point(310.0, 420.0)]).unwrap();
    assert_eq!(a.elements.len(), 1);
    assert_eq!(a.elements[0].kind(), ElementKind::Line);
    assert_close(a.length(), 500.0, 1e-12, "length");
    assert_close(a.end_station_m(), 600.0, 1e-12, "end station");
    let p = a.point_at(350.0).unwrap();
    assert_close(p.x, 160.0, 1e-10, "x");
    assert_close(p.y, 220.0, 1e-10, "y");
    assert_close(p.heading_rad, (4.0f64).atan2(3.0), 1e-14, "heading");
    assert_eq!(a.min_radius(), None);
    // Off the ends.
    assert!(a.point_at(99.99).is_none() && a.point_at(600.01).is_none() && a.point_at(f64::NAN).is_none());
    // Offset to the right of a line heading up and to the right is below and to the right of it.
    let (x, y) = a.point_at_offset(350.0, 5.0).unwrap();
    assert_close(x, 160.0 + 5.0 * 0.8, 1e-10, "offset x");
    assert_close(y, 220.0 - 5.0 * 0.6, 1e-10, "offset y");
}

#[test]
fn a_quarter_circle_each_way() {
    let r = 100.0;
    let quarter = PI * r / 2.0;
    let left = AlignmentBuilder::new("q", 0.0, 0.0, 0.0, 0.0).arc(r, quarter, Turn::Left).build().unwrap();
    assert_close(left.length(), PI * r / 2.0, 1e-12, "length");
    let end = left.point_at(quarter).unwrap();
    assert_close(end.x, 100.0, 1e-11, "x");
    assert_close(end.y, 100.0, 1e-11, "y");
    assert_close(end.heading_rad, FRAC_PI_2, 1e-13, "heading");
    let mid = left.point_at(quarter / 2.0).unwrap();
    assert_close(mid.x, r * (PI / 4.0).sin(), 1e-11, "mid x");
    assert_close(mid.y, r * (1.0 - (PI / 4.0).cos()), 1e-11, "mid y");
    assert_eq!(left.min_radius(), Some(100.0));
    assert_eq!(left.elements[0].turn(), Some(Turn::Left));

    let right = AlignmentBuilder::new("q", 0.0, 0.0, 0.0, 0.0).arc(r, quarter, Turn::Right).build().unwrap();
    let end = right.point_at(quarter).unwrap();
    assert_close(end.x, 100.0, 1e-11, "x");
    assert_close(end.y, -100.0, 1e-11, "y");
    assert_close(end.heading_rad, 3.0 * FRAC_PI_2, 1e-13, "heading");
    assert_eq!(right.elements[0].turn(), Some(Turn::Right));

    // A very flat arc does not lose precision (radius 10 000 km over 1 m).
    let flat = AlignmentBuilder::new("f", 0.0, 0.0, 0.0, 0.0).arc(1e7, 1.0, Turn::Left).build().unwrap();
    let end = flat.point_at(1.0).unwrap();
    assert_close(end.x, 1.0 - 1.0 / 6e14, 1e-16, "flat x = R sin(1/R)");
    assert_close(end.y, 0.5e-7, 1e-20, "flat y");
}

#[test]
fn clothoid_quadrature_agrees_with_the_series_and_with_brute_force() {
    for (length, radius) in [(60.0, 300.0), (50.0, 200.0), (100.0, 100.0), (30.0, 1000.0), (200.0, 80.0), (0.5, 15.0), (300.0, 50.0)] {
        let a = AlignmentBuilder::new("c", 0.0, 0.0, 0.0, 0.0).spiral(length, None, Some(radius), Turn::Left).build().unwrap();
        let end = a.point_at(length).unwrap();
        // The standard series.
        let (sx, sy) = clothoid_xy(length, radius);
        assert_close(end.x, sx, 1e-12 * length, &format!("x against the series, L={length} R={radius}"));
        assert_close(end.y, sy, 1e-12 * length, &format!("y against the series, L={length} R={radius}"));
        assert_close(end.heading_rad, length / (2.0 * radius), 1e-13, "spiral angle L/2R");
        // Fine-step integration of the heading.
        let (wx, wy) = walk(&a, 400_000);
        assert_close(end.x, wx, 1e-6, "x against the walk");
        assert_close(end.y, wy, 1e-6, "y against the walk");
        // The mirror image turns right.
        let m = AlignmentBuilder::new("c", 0.0, 0.0, 0.0, 0.0).spiral(length, None, Some(radius), Turn::Right).build().unwrap();
        let mend = m.point_at(length).unwrap();
        assert_close(mend.x, end.x, 1e-12 * length, "mirror x");
        assert_close(mend.y, -end.y, 1e-12 * length, "mirror y");
        // A spiral out is the same curve run backwards: from its end, looking back, it is a spiral in.
        let out = AlignmentBuilder::new("c", 0.0, 0.0, 0.0, 0.0).spiral(length, Some(radius), None, Turn::Left).build().unwrap();
        let oend = out.point_at(length).unwrap();
        let (bx, by) = (-oend.heading_rad.cos(), -oend.heading_rad.sin()); // looking back from the tangent end
        let (dx, dy) = (-oend.x, -oend.y); // the curve end as seen from there
        assert_close(dx * bx + dy * by, sx, 1e-11 * length, "spiral out, along");
        assert_close(dx * by - dy * bx, sy, 1e-11 * length, "spiral out, across (to the right looking back)");
    }
    // The textbook leading terms: x ≈ L(1 − θ²/10), y ≈ L(θ/3 − θ³/42), for L = 60, R = 200 (θ = 0.15).
    let (x, y) = clothoid_xy(60.0, 200.0);
    assert_close(x, 59.8651, 1e-4, "Xs");
    assert_close(y, 2.9952, 1e-4, "Ys");
}

#[test]
fn a_symmetric_spiral_curve_spiral_matches_the_standard_relations() {
    // Tangents meeting at 60°, R = 200, Ls = 60 at both ends.
    let (r, ls, delta) = (200.0, 60.0, 60f64.to_radians());
    let (d1, d2) = (500.0, 400.0);
    let pis = [Pi::point(0.0, 0.0), Pi::spiral_curve(d1, 0.0, r, ls, ls), Pi::point(d1 + d2 * delta.cos(), d2 * delta.sin())];
    let a = Alignment::from_pis("scs", 0.0, &pis).unwrap();

    // Standard relations, from the series.
    let theta = ls / (2.0 * r);
    let (xs, ys) = clothoid_xy(ls, r);
    let p = ys - r * (1.0 - theta.cos());
    let k = xs - r * theta.sin();
    let ts = (r + p) * (delta / 2.0).tan() + k;
    let lc = r * (delta - 2.0 * theta);
    assert_close(p, 0.7493, 1e-3, "shift p ≈ Ls²/24R = 0.75");
    assert_close(k, 29.977, 1e-2, "k ≈ Ls/2 = 30");

    let kinds: Vec<ElementKind> = a.elements.iter().map(|e| e.kind()).collect();
    assert_eq!(kinds, [ElementKind::Line, ElementKind::Spiral, ElementKind::Arc, ElementKind::Spiral, ElementKind::Line]);
    assert_close(a.elements[0].length, d1 - ts, 1e-9, "tangent to TS");
    assert_close(a.elements[2].length, lc, 1e-9, "circular arc");
    assert_close(a.elements[4].length, d2 - ts, 1e-9, "ST to end");
    assert_close(a.length(), d1 + d2 - 2.0 * ts + 2.0 * ls + lc, 1e-9, "total length");
    assert_eq!(a.min_radius(), Some(r));

    // SC: the end of the spiral in.
    let sc = a.point_at(d1 - ts + ls).unwrap();
    assert_close(sc.x, d1 - ts + xs, 1e-9, "SC x");
    assert_close(sc.y, ys, 1e-9, "SC y");
    assert_close(sc.heading_rad, theta, 1e-12, "SC heading");
    // CS: by symmetry about the bisector of the tangents.
    let cs = a.point_at(d1 - ts + ls + lc).unwrap();
    assert_close(cs.heading_rad, delta - theta, 1e-12, "CS heading");
    // The middle of the curve lies on the bisector through the PI, (R + p)/cos(Δ/2) − R from it.
    let mid = a.point_at(d1 - ts + ls + lc / 2.0).unwrap();
    let external = (r + p) / (delta / 2.0).cos() - r;
    let bis = FRAC_PI_2 + delta / 2.0; // from the PI towards the centre
    assert_close(mid.x, d1 + external * bis.cos(), 1e-9, "mid x");
    assert_close(mid.y, external * bis.sin(), 1e-9, "mid y");
    assert_close(mid.heading_rad, delta / 2.0, 1e-12, "mid heading");
    // The alignment arrives exactly at the last PI, on the outgoing tangent.
    let end = a.point_at(a.end_station_m()).unwrap();
    assert_close(end.x, pis[2].x, 1e-9, "end x");
    assert_close(end.y, pis[2].y, 1e-9, "end y");
    assert_close(end.heading_rad, delta, 1e-12, "end heading");
    // And the slow walk along the heading ends in the same place.
    let (wx, wy) = walk(&a, 200_000);
    assert_close(end.x, wx, 1e-6, "end x against the walk");
    assert_close(end.y, wy, 1e-6, "end y against the walk");
    assert!(a.discontinuities(1e-9, 1e-12).is_empty(), "{:?}", a.joints());
}

#[test]
fn pi_construction_reproduces_the_tangent_lengths() {
    // Plain circular curves, one to the left and one to the right: T = R tan(Δ/2), L = RΔ.
    let pis = [Pi::point(0.0, 0.0), Pi::curve(400.0, 0.0, 250.0), Pi::curve(700.0, 400.0, 180.0), Pi::point(1200.0, 400.0)];
    let a = Alignment::from_pis("pi", 0.0, &pis).unwrap();
    let d1 = (4.0f64).atan2(3.0); // left at PI 1, right by the same angle at PI 2
    let (t1, t2) = (250.0 * (d1 / 2.0).tan(), 180.0 * (d1 / 2.0).tan());
    assert_close(t1, 125.0, 1e-9, "tan(Δ/2) = 1/2 for a 3-4-5 deflection");
    let lengths: Vec<f64> = a.elements.iter().map(|e| e.length).collect();
    let want = [400.0 - t1, 250.0 * d1, 500.0 - t1 - t2, 180.0 * d1, 500.0 - t2];
    assert_eq!(lengths.len(), want.len());
    for (i, (g, w)) in lengths.iter().zip(want).enumerate() {
        assert_close(*g, w, 1e-9, &format!("element {i}"));
    }
    assert_eq!(a.elements[1].turn(), Some(Turn::Left));
    assert_eq!(a.elements[3].turn(), Some(Turn::Right));
    // PC and PT stand T back from and T beyond the PI.
    let pc = a.point_at(400.0 - t1).unwrap();
    assert_close(pc.x, 400.0 - t1, 1e-9, "PC x");
    let pt = a.point_at(400.0 - t1 + 250.0 * d1).unwrap();
    assert_close(pt.x, 400.0 + t1 * 0.6, 1e-9, "PT x");
    assert_close(pt.y, t1 * 0.8, 1e-9, "PT y");
    let end = a.point_at(a.end_station_m()).unwrap();
    assert_close(end.x, 1200.0, 1e-9, "end x");
    assert_close(end.y, 400.0, 1e-9, "end y");
    assert!(a.discontinuities(1e-9, 1e-12).is_empty());

    // Unequal spirals: the solver's tangents close the figure, and equal ones give the textbook Ts.
    let c = PiCurve::solve(d1, 250.0, 70.0, 40.0).unwrap();
    assert!(c.tangent_in != c.tangent_out);
    let pis = [Pi::point(0.0, 0.0), Pi::spiral_curve(400.0, 0.0, 250.0, 70.0, 40.0), Pi::point(700.0, 400.0)];
    let a = Alignment::from_pis("uneq", 0.0, &pis).unwrap();
    assert_close(a.elements[0].length, 400.0 - c.tangent_in, 1e-9, "tangent in");
    assert_close(a.elements[4].length, 500.0 - c.tangent_out, 1e-9, "tangent out");
    let end = a.point_at(a.end_station_m()).unwrap();
    assert_close(end.x, 700.0, 1e-9, "end x");
    assert_close(end.y, 400.0, 1e-9, "end y");
    assert!(a.discontinuities(1e-9, 1e-12).is_empty(), "{:?}", a.joints());

    let c = PiCurve::solve(-d1, 250.0, 70.0, 70.0).unwrap();
    let (xs, ys) = clothoid_xy(70.0, 250.0);
    let theta: f64 = 70.0 / 500.0;
    let ts = (250.0 + ys - 250.0 * (1.0 - theta.cos())) * (d1 / 2.0).tan() + xs - 250.0 * theta.sin();
    assert_close(c.tangent_in, ts, 1e-10, "Ts");
    assert_close(c.tangent_out, ts, 1e-10, "Ts");
    assert_close(c.arc_length, 250.0 * (d1 - 2.0 * theta), 1e-10, "Lc");
    assert_close(c.total_length(70.0, 70.0), 250.0 * d1 + 70.0, 1e-10, "L = RΔ + Ls");
}

#[test]
fn pi_construction_handles_reverse_curves_angle_points_and_bad_input() {
    // A reverse curve whose two halves meet with no tangent between them.
    let t = 200.0 * (FRAC_PI_2 / 2.0).tan();
    let pis = [Pi::point(0.0, 0.0), Pi::curve(300.0, 0.0, 200.0), Pi::curve(300.0, 2.0 * t, 200.0), Pi::point(700.0, 2.0 * t)];
    let a = Alignment::from_pis("rev", 0.0, &pis).unwrap();
    let kinds: Vec<ElementKind> = a.elements.iter().map(|e| e.kind()).collect();
    assert_eq!(kinds, [ElementKind::Line, ElementKind::Arc, ElementKind::Arc, ElementKind::Line]);
    assert_eq!((a.elements[1].turn(), a.elements[2].turn()), (Some(Turn::Left), Some(Turn::Right)));
    assert!(a.discontinuities(1e-9, 1e-12).is_empty());

    // One metre less between the PIs and the curves overlap.
    let pis2 = [pis[0], pis[1], Pi::curve(300.0, 2.0 * t - 1.0, 200.0), Pi::point(700.0, 2.0 * t - 1.0)];
    match Alignment::from_pis("rev", 0.0, &pis2) {
        Err(AlignmentError::CurvesOverlap { index: 1, next: 2, overlap_m }) => assert_close(overlap_m, 1.0, 1e-9, "overlap"),
        other => panic!("{other:?}"),
    }
    // Spirals that turn more than the deflection.
    let e = Alignment::from_pis("x", 0.0, &[Pi::point(0.0, 0.0), Pi::spiral_curve(500.0, 0.0, 100.0, 80.0, 80.0), Pi::point(900.0, 100.0)]);
    assert!(matches!(e, Err(AlignmentError::BadPi { index: 1, .. })), "{e:?}");
    assert_eq!(Alignment::from_pis("x", 0.0, &[Pi::point(0.0, 0.0)]), Err(AlignmentError::TooFewPis));
    assert_eq!(Alignment::from_pis("x", 0.0, &[Pi::point(0.0, 0.0), Pi::point(0.0, 0.0)]), Err(AlignmentError::CoincidentPis { index: 1 }));

    // Radius 0 leaves an angle point; a PI on the straight adds nothing but a joint.
    let a = Alignment::from_pis("kink", 0.0, &[Pi::point(0.0, 0.0), Pi::point(100.0, 0.0), Pi::point(200.0, 0.0), Pi::point(200.0, 100.0)]).unwrap();
    assert_eq!(a.elements.len(), 3);
    let breaks = a.discontinuities(1e-9, 1e-9);
    assert_eq!(breaks.len(), 1);
    assert_close(breaks[0].station_m, 200.0, 1e-12, "kink station");
    assert_close(breaks[0].heading_change_rad, FRAC_PI_2, 1e-12, "kink angle");
}

#[test]
fn position_and_heading_are_continuous_at_every_joint() {
    let a = everything();
    assert_eq!(a.elements.len(), 11);
    for j in a.joints() {
        assert!(j.gap_m < 1e-9 && j.heading_change_rad.abs() < 1e-12, "{j:?}");
        // Either side of the joint, through the public lookup.
        let (before, at, after) = (a.point_at(j.station_m - 1e-6).unwrap(), a.point_at(j.station_m).unwrap(), a.point_at(j.station_m + 1e-6).unwrap());
        for p in [before, after] {
            assert!((p.x - at.x).hypot(p.y - at.y) < 1.0001e-6, "{j:?}");
            assert!(wrap_pi(p.heading_rad - at.heading_rad).abs() < 1e-7, "{j:?}");
        }
    }
    // The whole chain agrees with the slow walk.
    let end = a.point_at(a.end_station_m()).unwrap();
    let (wx, wy) = walk(&a, 200_000);
    assert_close(end.x, wx, 1e-6, "end x against the walk");
    assert_close(end.y, wy, 1e-6, "end y against the walk");
    // Headings stay in [0, 2π).
    for i in 0..=1000 {
        let h = a.point_at(a.start_station_m + a.length() * i as f64 / 1000.0).unwrap().heading_rad;
        assert!((0.0..TAU).contains(&h));
    }
    assert_eq!(a.min_radius(), Some(150.0));
}

#[test]
fn station_offset_inverts_point_at_offset_on_every_element_type() {
    let a = everything();
    let stations = a.element_stations();
    let mut checked = 0;
    for (i, e) in a.elements.iter().enumerate() {
        for step in 0..=40 {
            let station = stations[i] + e.length * step as f64 / 40.0;
            for offset in [-12.0, -3.5, -0.001, 0.0, 0.001, 2.25, 15.0] {
                let (x, y) = a.point_at_offset(station, offset).unwrap();
                let (s, d) = a.station_offset(x, y).unwrap_or_else(|| panic!("element {i} station {station} offset {offset}: no answer"));
                assert_close(s, station, 1e-7, &format!("station, element {i} ({:?}) offset {offset}", e.kind()));
                assert_close(d, offset, 1e-7, &format!("offset, element {i} ({:?}) station {station}", e.kind()));
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 11 * 41 * 7);

    // Right of a line heading east is south: positive offset.
    let line = AlignmentBuilder::new("l", 50.0, 0.0, 0.0, 0.0).line(100.0).build().unwrap();
    let (s, d) = line.station_offset(30.0, -4.0).unwrap();
    assert_close(s, 80.0, 1e-12, "station");
    assert_close(d, 4.0, 1e-12, "offset to the right");
    assert_close(line.station_offset(30.0, 4.0).unwrap().1, -4.0, 1e-12, "offset to the left");
    // Behind the start and beyond the end there is no perpendicular foot.
    assert_eq!(line.station_offset(-0.5, 1.0), None);
    assert_eq!(line.station_offset(100.5, 1.0), None);
    assert!(line.station_offset(0.0, 1.0).is_some() && line.station_offset(100.0, -1.0).is_some());
    // Inside a left-hand curve the offset is negative; outside, positive.
    let arc = AlignmentBuilder::new("a", 0.0, 0.0, 0.0, 0.0).arc(100.0, 100.0 * FRAC_PI_2, Turn::Left).build().unwrap();
    let (s, d) = arc.station_offset(100.0 - 90.0 * FRAC_PI_2.sin() * (PI / 4.0).cos(), 100.0 - 90.0 * (PI / 4.0).cos()).unwrap();
    let _ = s;
    assert!(d < 0.0);
    let (s, d) = arc.station_offset(110.0 * (PI / 4.0).sin(), 100.0 - 110.0 * (PI / 4.0).cos()).unwrap();
    assert_close(s, 100.0 * PI / 4.0, 1e-9, "station on the arc");
    assert_close(d, 10.0, 1e-9, "outside the arc");
    // Outside a corner the nearest point is the corner.
    let kink = Alignment::from_pis("k", 0.0, &[Pi::point(0.0, 0.0), Pi::point(100.0, 0.0), Pi::point(100.0, 100.0)]).unwrap();
    let (s, d) = kink.station_offset(103.0, -4.0).unwrap();
    assert_close(s, 100.0, 1e-9, "corner station");
    assert_close(d, 5.0, 1e-9, "distance to the corner, on the right");
    assert_eq!(Alignment::from_elements("empty", 0.0, vec![]).unwrap().station_offset(0.0, 0.0), None);
}

#[test]
fn polyline_keeps_joints_and_respects_the_chord_limit() {
    let a = everything();
    let pts = a.polyline(5.0);
    let start = a.point_at(a.start_station_m).unwrap();
    let end = a.point_at(a.end_station_m()).unwrap();
    assert_eq!(pts[0], (start.x, start.y));
    let last = pts[pts.len() - 1];
    assert!((last.0 - end.x).hypot(last.1 - end.y) < 1e-9);
    let straights: f64 = a.elements.iter().filter(|e| e.kind() == ElementKind::Line).map(|e| e.length).sum();
    let mut length = 0.0;
    for w in pts.windows(2) {
        let c = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
        assert!(c > 0.0 && (c <= 5.0 + 1e-9 || c > 60.0), "chord {c}"); // only the straights are longer
        length += c;
    }
    assert!(length < a.length() && length > a.length() - 0.05, "{length} against {}", a.length());
    // Every joint is a vertex.
    for j in a.joints() {
        let p = a.point_at(j.station_m).unwrap();
        assert!(pts.iter().any(|q| (q.0 - p.x).hypot(q.1 - p.y) < 1e-9), "{j:?}");
    }
    assert!(straights > 0.0);
    // A straight is two points whatever the chord.
    let line = AlignmentBuilder::new("l", 0.0, 0.0, 0.0, 0.0).line(1000.0).build().unwrap();
    assert_eq!(line.polyline(1.0).len(), 2);
}

#[test]
fn stations_are_written_as_kilometres_plus_metres() {
    assert_eq!(format_station(1250.0, 3), "1+250.000");
    assert_eq!(format_station(0.0, 3), "0+000.000");
    assert_eq!(format_station(50.5, 3), "0+050.500");
    assert_eq!(format_station(999.9996, 3), "1+000.000");
    assert_eq!(format_station(12345.678, 2), "12+345.68");
    assert_eq!(format_station(1234567.0, 0), "1234+567");
    assert_eq!(format_station(-50.0, 3), "-0+050.000");
    assert_eq!(format_station(-0.0001, 3), "0+000.000");
}

#[test]
fn elements_report_their_shape_and_bad_ones_are_refused() {
    let a = everything();
    let spiral = &a.elements[6];
    assert_eq!(spiral.kind(), ElementKind::Spiral);
    assert_close(spiral.radius_start().unwrap(), 200.0, 1e-9, "radius start");
    assert_close(spiral.radius_end().unwrap(), 450.0, 1e-9, "radius end");
    assert_eq!(spiral.turn(), Some(Turn::Right));
    assert_eq!(a.elements[1].radius_start(), None);
    assert_eq!(a.elements[0].turn(), None);
    assert_eq!(a.locate(a.start_station_m + 80.0), Some((1, 0.0)));
    assert_eq!(a.locate(a.end_station_m()).unwrap().0, 10);

    let mut bad = a.elements.clone();
    bad[3].length = -1.0;
    assert!(matches!(Alignment::from_elements("b", 0.0, bad), Err(AlignmentError::BadElement { index: 3, .. })));
    let mut bad = a.elements.clone();
    bad[2].x = f64::NAN;
    assert!(matches!(Alignment::from_elements("b", 0.0, bad), Err(AlignmentError::BadElement { index: 2, .. })));

    // The serialised form reads back the same alignment (serde_json may move a value by one
    // unit in the last place when it parses, so compare to that).
    let json = serde_json::to_string(&a).unwrap();
    let back: Alignment = serde_json::from_str(&json).unwrap();
    assert_eq!((back.name.as_str(), back.start_station_m, back.elements.len()), ("everything", 1234.5, 11));
    for (b, e) in back.elements.iter().zip(&a.elements) {
        for (got, want) in [(b.x, e.x), (b.y, e.y), (b.heading_rad, e.heading_rad), (b.length, e.length), (b.curvature_start, e.curvature_start), (b.curvature_end, e.curvature_end)] {
            assert!((got - want).abs() <= 4.0 * f64::EPSILON * want.abs(), "{got} against {want}");
        }
    }
    assert!(json.contains("\"curvature_start\"") && json.contains("\"heading_rad\""));
}
