//! Snap points for object snap on an underlay.
//!
//! Endpoint, midpoint, centre, quadrant and node points come out of the same expansion that draws
//! the underlay, so they sit exactly on what is drawn (an arc's centre is the arc's own centre,
//! not a polyline estimate). Intersections are computed between straight segments (lines, the
//! straight and flattened parts of polylines, and arcs flattened to `curve_tolerance_mm`), found with a grid so
//! the work is near linear in the number of segments, and limited by a budget:
//! `max_pair_tests` candidate pairs and `max_intersection_points` results. When a budget stops the
//! search the set says so; the points found before that remain valid.

use std::collections::HashMap;

use afrobim_draw::Pt2;

use super::expand::{Expanded, Shape, Snap, SnapKind};
use super::geom::{arc_points, sweep};

#[derive(Debug, Clone)]
pub struct SnapOptions {
    /// Most endpoint, midpoint, centre, quadrant and node points returned. Default 2 000 000.
    pub max_points: usize,
    /// Compute intersections. Default true.
    pub intersections: bool,
    /// Most intersection points returned, counted apart from `max_points`. Default 100 000.
    pub max_intersection_points: usize,
    /// Most candidate segment pairs tested. Default 5 000 000.
    pub max_pair_tests: u64,
    /// Most segments entered into the intersection search; above this no intersections are
    /// computed and the set says so. Default 2 000 000.
    pub max_segments: usize,
    /// Chord tolerance used when curves are flattened for intersections, millimetres. Intersections
    /// with a curve are those of its flattening, so they can differ from the true ones by about
    /// this much. Default 2.
    pub curve_tolerance_mm: f64,
}

impl Default for SnapOptions {
    fn default() -> Self {
        SnapOptions { max_points: 2_000_000, intersections: true, max_intersection_points: 100_000, max_pair_tests: 5_000_000, max_segments: 2_000_000, curve_tolerance_mm: 2.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SnapSet {
    /// Model millimetres.
    pub points: Vec<Snap>,
    /// `max_points` cut the list.
    pub truncated_points: bool,
    /// A budget ended the intersection search early.
    pub truncated_intersections: bool,
    pub segments: usize,
    pub pair_tests: u64,
}

struct Seg {
    a: Pt2,
    b: Pt2,
    chain: u32,
    idx: u32,
    /// Number of segments in the chain when it is closed (to catch the wrap-around neighbour), else 0.
    closed_n: u32,
}

pub fn collect(ex: &Expanded, k: f64, offset: Pt2, o: &SnapOptions) -> SnapSet {
    let tx = |p: Pt2| Pt2::new(p.x * k + offset.x, p.y * k + offset.y);
    let mut set = SnapSet::default();
    for s in &ex.snaps {
        if set.points.len() >= o.max_points {
            set.truncated_points = true;
            break;
        }
        set.points.push(Snap { p: tx(s.p), kind: s.kind });
    }
    if !o.intersections {
        return set;
    }
    // Segments in model millimetres.
    let mut segs: Vec<Seg> = Vec::new();
    let mut chain = 0u32;
    let tol_file = o.curve_tolerance_mm / k.max(1e-12);
    for it in &ex.items {
        let (pts, closed): (Vec<Pt2>, bool) = match &it.shape {
            Shape::Line(a, b) => (vec![*a, *b], false),
            Shape::Poly { pts, closed } => (pts.clone(), *closed),
            Shape::Arc { c, r, a0, a1 } => (arc_points(*c, *r, *a0, sweep(*a0, *a1), tol_file), false),
            Shape::Circle { c, r } => (arc_points(*c, *r, 0.0, 360.0, tol_file), true),
            _ => continue,
        };
        let n = pts.len();
        let count = if closed { n } else { n.saturating_sub(1) };
        if segs.len() + count > o.max_segments {
            set.truncated_intersections = true;
            set.segments = segs.len() + count;
            return set;
        }
        for i in 0..count {
            segs.push(Seg { a: tx(pts[i]), b: tx(pts[(i + 1) % n]), chain, idx: i as u32, closed_n: if closed { count as u32 } else { 0 } });
        }
        chain += 1;
    }
    set.segments = segs.len();
    if segs.len() < 2 {
        return set;
    }
    // Grid.
    let (mut lo, mut hi) = (Pt2::new(f64::INFINITY, f64::INFINITY), Pt2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    let mut total = 0.0;
    for s in &segs {
        for p in [s.a, s.b] {
            lo = Pt2::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Pt2::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        total += (s.a.x - s.b.x).hypot(s.a.y - s.b.y);
    }
    let extent = (hi.x - lo.x).max(hi.y - lo.y).max(1e-9);
    let cell = ((total / segs.len() as f64) * 2.0).max(extent / 1024.0).max(1e-9);
    let key = |p: Pt2| (((p.x - lo.x) / cell).floor() as i32, ((p.y - lo.y) / cell).floor() as i32);
    let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        let (ka, kb) = (key(s.a), key(s.b));
        let (x0, x1, y0, y1) = (ka.0.min(kb.0), ka.0.max(kb.0), ka.1.min(kb.1), ka.1.max(kb.1));
        if (x1 - x0 + 1) as i64 * (y1 - y0 + 1) as i64 > 4096 {
            set.truncated_intersections = true; // a segment spanning thousands of cells is left out
            continue;
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                grid.entry((x, y)).or_default().push(i as u32);
            }
        }
    }
    let mut found = 0usize;
    'cells: for (cell_key, members) in &grid {
        for (m, &i) in members.iter().enumerate() {
            for &j in &members[m + 1..] {
                let (s, t) = (&segs[i as usize], &segs[j as usize]);
                if s.chain == t.chain {
                    let d = s.idx.abs_diff(t.idx);
                    if d <= 1 || (s.closed_n > 0 && d + 1 == s.closed_n) {
                        continue;
                    }
                }
                set.pair_tests += 1;
                if set.pair_tests > o.max_pair_tests {
                    set.truncated_intersections = true;
                    break 'cells;
                }
                if let Some(p) = intersect(s, t) {
                    // Count a pair once: in the cell that holds the intersection.
                    if key(p) != *cell_key {
                        continue;
                    }
                    if found >= o.max_intersection_points {
                        set.truncated_intersections = true;
                        break 'cells;
                    }
                    set.points.push(Snap { p, kind: SnapKind::Intersection });
                    found += 1;
                }
            }
        }
    }
    set
}

/// The crossing point of two segments, excluding a meeting at an end point of both.
fn intersect(s: &Seg, t: &Seg) -> Option<Pt2> {
    let (r, q) = ((s.b.x - s.a.x, s.b.y - s.a.y), (t.b.x - t.a.x, t.b.y - t.a.y));
    let den = r.0 * q.1 - r.1 * q.0;
    let scale = (r.0.hypot(r.1) * q.0.hypot(q.1)).max(1e-300);
    if den.abs() < 1e-12 * scale {
        return None; // parallel or degenerate
    }
    let w = (t.a.x - s.a.x, t.a.y - s.a.y);
    let u = (w.0 * q.1 - w.1 * q.0) / den;
    let v = (w.0 * r.1 - w.1 * r.0) / den;
    let e = 1e-9;
    if !(-e..=1.0 + e).contains(&u) || !(-e..=1.0 + e).contains(&v) {
        return None;
    }
    let end = |x: f64| x < e || x > 1.0 - e;
    if end(u) && end(v) {
        return None; // shared end point: already an endpoint snap
    }
    Some(Pt2::new(s.a.x + u * r.0, s.a.y + u * r.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(a: (f64, f64), b: (f64, f64)) -> Seg {
        Seg { a: Pt2::new(a.0, a.1), b: Pt2::new(b.0, b.1), chain: 0, idx: 0, closed_n: 0 }
    }

    #[test]
    fn the_grid_finds_exactly_the_crossings_a_brute_force_search_finds() {
        use crate::reader::expand::{Expanded, Item, Resolved};
        // A deterministic scatter of 400 segments (a small linear congruential generator).
        let mut x = 12345u64;
        let mut rnd = move || {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((x >> 33) as f64) / (1u64 << 31) as f64
        };
        let mut items = Vec::new();
        for _ in 0..400 {
            let (px, py) = (rnd() * 1000.0, rnd() * 1000.0);
            let (qx, qy) = (px + (rnd() - 0.5) * 120.0, py + (rnd() - 0.5) * 120.0);
            items.push(Item { layer: 0, colour: Resolved { rgb: [0; 3], aci: None }, kind: "LINE", shape: Shape::Line(Pt2::new(px, py), Pt2::new(qx, qy)) });
        }
        let mut brute = 0;
        for i in 0..items.len() {
            for j in i + 1..items.len() {
                if let (Shape::Line(a, b), Shape::Line(c, d)) = (&items[i].shape, &items[j].shape) {
                    let (s, t) = (Seg { a: *a, b: *b, chain: 0, idx: 0, closed_n: 0 }, Seg { a: *c, b: *d, chain: 1, idx: 0, closed_n: 0 });
                    if intersect(&s, &t).is_some() {
                        brute += 1;
                    }
                }
            }
        }
        let ex = Expanded { items, ..Default::default() };
        let set = collect(&ex, 1.0, Pt2::new(0.0, 0.0), &SnapOptions::default());
        let found = set.points.iter().filter(|p| p.kind == SnapKind::Intersection).count();
        assert!(brute > 50, "the scatter should cross a good deal: {brute}");
        assert_eq!(found, brute);
        assert!(!set.truncated_intersections);
    }

    #[test]
    fn crossing_segments_meet_at_the_expected_point() {
        let p = intersect(&seg((0.0, 0.0), (10.0, 10.0)), &seg((0.0, 10.0), (10.0, 0.0))).unwrap();
        assert!((p.x - 5.0).abs() < 1e-12 && (p.y - 5.0).abs() < 1e-12);
        // A T: the end of one on the middle of the other counts; two ends meeting do not.
        assert!(intersect(&seg((0.0, 0.0), (10.0, 0.0)), &seg((5.0, 0.0), (5.0, 5.0))).is_some());
        assert!(intersect(&seg((0.0, 0.0), (5.0, 0.0)), &seg((5.0, 0.0), (5.0, 5.0))).is_none());
        // Parallel and disjoint.
        assert!(intersect(&seg((0.0, 0.0), (10.0, 0.0)), &seg((0.0, 1.0), (10.0, 1.0))).is_none());
        assert!(intersect(&seg((0.0, 0.0), (1.0, 1.0)), &seg((5.0, 0.0), (6.0, -3.0))).is_none());
    }
}
