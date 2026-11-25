// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Contours of a TIN: minor and major, traced as polylines.
//!
//! Method: marching triangles, the two-dimensional member of the family introduced by
//! W. E. Lorensen and H. E. Cline, "Marching cubes: a high resolution 3D surface construction
//! algorithm", Computer Graphics (SIGGRAPH '87) 21(4), 1987. A contour at level `L` crosses a
//! triangle where its corners are not all on the same side of `L`; inside the plane of one
//! triangle it is a straight segment between the points where `L` meets two of the edges,
//! linear along each edge. Segments are chained by the identity of the edge they end on, not
//! by comparing coordinates, so the chains close exactly.
//!
//! A corner exactly at the level counts as on the upper side ("at or above"). This is the usual
//! symbolic tie-break; it keeps every contour a closed loop or a line ending on the surface edge,
//! and means a horizontal facet lying exactly at a contour level gets no contour line.
//!
//! Orientation: each contour runs with the higher ground on its left, so a closed contour
//! around a hill is counter-clockwise and around a pit clockwise.

use std::collections::{BTreeMap, HashMap, HashSet};

use afrobim_corridor::Tin;
use serde::{Deserialize, Serialize};

use crate::error::TerrainError;
use crate::geom::area2;

/// Minor or major contour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContourKind {
    Minor,
    Major,
}

/// A traced contour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contour {
    /// Elevation, m.
    pub level: f64,
    pub kind: ContourKind,
    /// Plan vertices `[x, y]`. A closed contour does not repeat its first vertex.
    pub points: Vec<[f64; 2]>,
    pub closed: bool,
    /// True when the vertices come from [`smooth`]: cosmetic, no longer on the TIN.
    pub smoothed: bool,
}

/// Which contours to trace.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ContourOptions {
    /// Spacing of minor contours, m.
    pub minor_interval: f64,
    /// Spacing of major contours, m: a whole multiple of the minor interval. Contours at a
    /// multiple of it are major. `None` makes every contour minor.
    pub major_interval: Option<f64>,
    /// Elevation of the contour with index zero, m (levels are `base + k * minor_interval`).
    pub base: f64,
}

impl ContourOptions {
    pub fn new(minor_interval: f64, major_interval: Option<f64>) -> ContourOptions {
        ContourOptions { minor_interval, major_interval, base: 0.0 }
    }
}

/// The contours of a surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContourSet {
    pub options: ContourOptions,
    /// Ordered by level, then by the order they were found.
    pub contours: Vec<Contour>,
}

#[derive(Clone, Copy)]
struct Seg {
    from: (u32, u32),
    to: (u32, u32),
    p: [f64; 2],
    q: [f64; 2],
}

/// Traces the contours of `tin` at the intervals of `options`.
pub fn contours(tin: &Tin, options: &ContourOptions) -> Result<ContourSet, TerrainError> {
    let minor = options.minor_interval;
    if !(minor.is_finite() && minor > 0.0) || !options.base.is_finite() {
        return Err(TerrainError::InvalidInput("the contour interval must be a positive number and the base finite".into()));
    }
    let every = match options.major_interval {
        None => None,
        Some(m) => {
            let r = m / minor;
            if !(m.is_finite() && m > 0.0 && r >= 1.0 && (r - r.round()).abs() <= 1e-9 * r.max(1.0)) {
                return Err(TerrainError::InvalidInput("the major interval must be a whole multiple of the minor interval".into()));
            }
            Some(r.round() as i64)
        }
    };
    let b = tin.bounds();
    let span = (b.max_z - b.min_z) / minor;
    if span > 1.0e6 {
        return Err(TerrainError::InvalidInput(format!("{span:.0} contour levels were asked for; the interval is too small")));
    }
    let pts = tin.points();
    let level_of = |k: i64| options.base + k as f64 * minor;
    let mut by_level: BTreeMap<i64, Vec<Seg>> = BTreeMap::new();

    for t in tin.triangles() {
        let mut v = *t;
        if area2(pts[v[0] as usize], pts[v[1] as usize], pts[v[2] as usize]) < 0.0 {
            v.swap(1, 2);
        }
        let z = v.map(|i| pts[i as usize][2]);
        let (zlo, zhi) = (z[0].min(z[1]).min(z[2]), z[0].max(z[1]).max(z[2]));
        let k0 = ((zlo - options.base) / minor).ceil() as i64;
        let k1 = ((zhi - options.base) / minor).floor() as i64;
        for k in k0..=k1 {
            let level = level_of(k);
            let above = z.map(|zz| zz >= level);
            let n = above.iter().filter(|a| **a).count();
            if n == 0 || n == 3 {
                continue;
            }
            let lone_is_above = n == 1;
            let lone = (0..3).find(|&i| above[i] == lone_is_above).unwrap();
            let (l, m, nn) = (v[lone], v[(lone + 1) % 3], v[(lone + 2) % 3]);
            let key = |a: u32, b: u32| (a.min(b), a.max(b));
            let (e_lm, e_nl) = (key(l, m), key(nn, l));
            let (p_lm, p_nl) = (edge_point(pts, e_lm, level), edge_point(pts, e_nl, level));
            let seg = if lone_is_above { Seg { from: e_lm, to: e_nl, p: p_lm, q: p_nl } } else { Seg { from: e_nl, to: e_lm, p: p_nl, q: p_lm } };
            by_level.entry(k).or_default().push(seg);
        }
    }

    let mut out = Vec::new();
    for (k, segs) in by_level {
        let kind = match every {
            Some(n) if k.rem_euclid(n) == 0 => ContourKind::Major,
            _ => ContourKind::Minor,
        };
        trace_level(&segs, level_of(k), kind, &mut out);
    }
    Ok(ContourSet { options: *options, contours: out })
}

fn edge_point(pts: &[[f64; 3]], e: (u32, u32), level: f64) -> [f64; 2] {
    let (a, b) = (pts[e.0 as usize], pts[e.1 as usize]);
    let t = (level - a[2]) / (b[2] - a[2]);
    [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
}

fn trace_level(segs: &[Seg], level: f64, kind: ContourKind, out: &mut Vec<Contour>) {
    let mut starts: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    let mut ends: HashSet<(u32, u32)> = HashSet::new();
    for (i, s) in segs.iter().enumerate() {
        starts.entry(s.from).or_default().push(i);
        ends.insert(s.to);
    }
    let mut used = vec![false; segs.len()];
    let follow = |first: usize, used: &mut Vec<bool>, closing_edge: Option<(u32, u32)>| -> (Vec<[f64; 2]>, bool) {
        used[first] = true;
        let mut pts = vec![segs[first].p, segs[first].q];
        let mut at = segs[first].to;
        let mut closed = false;
        loop {
            if Some(at) == closing_edge {
                closed = true;
                pts.pop();
                break;
            }
            let next = starts.get(&at).and_then(|l| l.iter().copied().find(|&j| !used[j]));
            match next {
                Some(j) => {
                    used[j] = true;
                    pts.push(segs[j].q);
                    at = segs[j].to;
                }
                None => break,
            }
        }
        // Corners exactly on the level give zero-length segments: drop repeated vertices.
        pts.dedup();
        if closed && pts.len() > 1 && pts.first() == pts.last() {
            pts.pop();
        }
        (pts, closed)
    };
    // Open lines first: they start on an edge no segment ends on.
    for i in 0..segs.len() {
        if !used[i] && !ends.contains(&segs[i].from) {
            let (points, closed) = follow(i, &mut used, None);
            if points.len() >= 2 {
                out.push(Contour { level, kind, points, closed, smoothed: false });
            }
        }
    }
    for i in 0..segs.len() {
        if !used[i] {
            let (points, closed) = follow(i, &mut used, Some(segs[i].from));
            // A closed contour needs three distinct vertices; fewer is a level that only touches.
            if points.len() >= if closed { 3 } else { 2 } {
                out.push(Contour { level, kind, points, closed, smoothed: false });
            }
        }
    }
}

/// Smooths a contour by Chaikin corner cutting.
///
/// **Cosmetic.** The result no longer lies on the TIN, can move a contour across a neighbouring
/// one where lines are close, and is not used by any analysis or volume in this crate. It only
/// makes a plotted contour look less angular. Open contours keep their end points. Each
/// iteration replaces every segment `P Q` by the points at one quarter and three quarters along
/// it (G. M. Chaikin, "An algorithm for high speed curve generation", Computer Graphics and
/// Image Processing 3(4), 1974), so the number of vertices roughly doubles; `iterations` is
/// limited to 6.
pub fn smooth(contour: &Contour, iterations: u32) -> Contour {
    let mut pts = contour.points.clone();
    for _ in 0..iterations.min(6) {
        let n = pts.len();
        if n < 3 && contour.closed || n < 2 {
            break;
        }
        let cut = |p: [f64; 2], q: [f64; 2]| ([0.75 * p[0] + 0.25 * q[0], 0.75 * p[1] + 0.25 * q[1]], [0.25 * p[0] + 0.75 * q[0], 0.25 * p[1] + 0.75 * q[1]]);
        let mut next = Vec::with_capacity(2 * n);
        if contour.closed {
            for i in 0..n {
                let (a, b) = cut(pts[i], pts[(i + 1) % n]);
                next.push(a);
                next.push(b);
            }
        } else {
            next.push(pts[0]);
            for i in 0..n - 1 {
                let (a, b) = cut(pts[i], pts[i + 1]);
                if i > 0 {
                    next.push(a);
                }
                if i < n - 2 {
                    next.push(b);
                }
            }
            next.push(pts[n - 1]);
        }
        pts = next;
    }
    Contour { level: contour.level, kind: contour.kind, points: pts, closed: contour.closed, smoothed: true }
}

impl ContourSet {
    /// The contours of one kind.
    pub fn of_kind(&self, kind: ContourKind) -> impl Iterator<Item = &Contour> {
        self.contours.iter().filter(move |c| c.kind == kind)
    }

    /// Total plan length of all contours of a kind, m.
    pub fn length(&self, kind: ContourKind) -> f64 {
        self.of_kind(kind).map(|c| c.length()).sum()
    }
}

impl Contour {
    /// Plan length, m (including the closing segment of a closed contour).
    pub fn length(&self) -> f64 {
        let n = self.points.len();
        let segs = if self.closed { n } else { n.saturating_sub(1) };
        (0..segs).map(|i| (self.points[(i + 1) % n][0] - self.points[i][0]).hypot(self.points[(i + 1) % n][1] - self.points[i][1])).sum()
    }

    /// Signed plan area of a closed contour (positive counter-clockwise, so positive around a
    /// hill); zero for an open one.
    pub fn signed_area(&self) -> f64 {
        if !self.closed {
            return 0.0;
        }
        crate::geom::polygon_area2(&self.points) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cone(n: usize) -> Tin {
        // Square pyramid on a 40 m square, apex 10 m at the centre, built from 4 triangles.
        let _ = n;
        let pts = vec![[0.0, 0.0, 0.0], [40.0, 0.0, 0.0], [40.0, 40.0, 0.0], [0.0, 40.0, 0.0], [20.0, 20.0, 10.0]];
        Tin::new(pts, vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]]).unwrap()
    }

    #[test]
    fn closed_contours_of_a_pyramid_have_the_exact_area() {
        let set = contours(&cone(0), &ContourOptions::new(2.0, Some(4.0))).unwrap();
        // Levels 2, 4, 6, 8 (0 is at the base: all corners "at or above", no line; 10 is the apex).
        let levels: Vec<f64> = set.contours.iter().map(|c| c.level).collect();
        assert_eq!(levels, vec![2.0, 4.0, 6.0, 8.0]);
        for c in &set.contours {
            assert!(c.closed);
            assert_eq!(c.points.len(), 4);
            // Square of side 40 * (1 - level / 10).
            let side = 40.0 * (1.0 - c.level / 10.0);
            assert!((c.signed_area() - side * side).abs() < 1e-9, "{} {}", c.level, c.signed_area());
            assert!(c.signed_area() > 0.0, "counter-clockwise around a hill");
        }
        let kinds: Vec<(f64, ContourKind)> = set.contours.iter().map(|c| (c.level, c.kind)).collect();
        assert!(kinds.contains(&(4.0, ContourKind::Major)));
        assert!(kinds.contains(&(8.0, ContourKind::Major)));
        assert!(kinds.contains(&(2.0, ContourKind::Minor)));
    }

    #[test]
    fn pit_contours_run_clockwise() {
        let pts = vec![[0.0, 0.0, 10.0], [40.0, 0.0, 10.0], [40.0, 40.0, 10.0], [0.0, 40.0, 10.0], [20.0, 20.0, 0.0]];
        let tin = Tin::new(pts, vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]]).unwrap();
        let set = contours(&tin, &ContourOptions::new(5.0, None)).unwrap();
        // Level 5 around the pit, and level 10 along the rim (the surface edge, a closed loop).
        assert_eq!(set.contours.len(), 2);
        let c = &set.contours[0];
        assert!(c.level == 5.0 && c.closed && c.signed_area() < 0.0);
        assert!((c.signed_area().abs() - 400.0).abs() < 1e-9);
    }

    #[test]
    fn plane_contours_are_open_straight_lines() {
        let tin = Tin::from_grid(500_000.0, 9_000_000.0, 10.0, 10.0, 6, 6, |x, _| 0.1 * (x - 500_000.0)).unwrap();
        let set = contours(&tin, &ContourOptions { minor_interval: 1.0, major_interval: Some(2.0), base: 0.0 }).unwrap();
        // Levels 1, 2, 3, 4 (0 at the left edge: at-or-above everywhere there; 5 at the right edge).
        assert_eq!(set.contours.len(), 5);
        for c in &set.contours {
            assert!(!c.closed);
            assert_eq!(c.points.len(), 6, "{c:?}");
            assert!((c.length() - 50.0).abs() < 1e-6);
            let x = 500_000.0 + c.level * 10.0;
            assert!(c.points.iter().all(|p| (p[0] - x).abs() < 1e-6));
            // Higher ground on the left: for a plane rising with x the line runs south (-y).
            assert!(c.points[0][1] > c.points[5][1]);
        }
    }

    #[test]
    fn a_level_that_only_touches_a_vertex_gives_no_line() {
        let set = contours(&cone(0), &ContourOptions::new(10.0, None)).unwrap();
        // Level 10 touches only the apex; level 0 is the flat base, at or above everywhere.
        assert!(set.contours.is_empty(), "{:?}", set.contours);
    }

    #[test]
    fn base_offset_and_validation() {
        let set = contours(&cone(0), &ContourOptions { minor_interval: 4.0, major_interval: None, base: 1.0 }).unwrap();
        let levels: Vec<f64> = set.contours.iter().map(|c| c.level).collect();
        assert_eq!(levels, vec![1.0, 5.0, 9.0]);
        assert!(contours(&cone(0), &ContourOptions::new(0.0, None)).is_err());
        assert!(contours(&cone(0), &ContourOptions::new(2.0, Some(3.0))).is_err());
        assert!(contours(&cone(0), &ContourOptions::new(2.0, Some(1.0))).is_err());
        assert!(contours(&cone(0), &ContourOptions::new(1e-9, None)).is_err());
    }

    #[test]
    fn smoothing_is_marked_and_keeps_ends() {
        let tin = Tin::from_grid(0.0, 0.0, 10.0, 10.0, 4, 4, |x, y| (x * 0.3).sin() * 3.0 + y * 0.1).unwrap();
        let set = contours(&tin, &ContourOptions::new(0.5, None)).unwrap();
        let open = set.contours.iter().find(|c| !c.closed && c.points.len() > 3).unwrap();
        let s = smooth(open, 2);
        assert!(s.smoothed && !open.smoothed);
        assert_eq!(s.points.first(), open.points.first());
        assert_eq!(s.points.last(), open.points.last());
        assert!(s.points.len() > open.points.len());
        let cl = contours(&cone(0), &ContourOptions::new(2.0, None)).unwrap().contours.remove(0);
        let sc = smooth(&cl, 1);
        assert_eq!(sc.points.len(), 8);
        assert!(sc.closed && sc.smoothed);
    }
}
