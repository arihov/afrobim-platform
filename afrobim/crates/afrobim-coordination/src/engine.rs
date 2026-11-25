// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Broad phase (sweep and prune on boxes) and narrow phase (triangle tests) for one clash test.

use crate::geom::*;
use crate::results::{clash_id, Clash, Status};
use crate::rules::{ClashKind, ClashTest};
use crate::scene::{Part, Scene};

/// What a run did, for display and tuning.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunStats {
    /// Bodies in either selection.
    pub bodies_in_scope: usize,
    /// Pairs whose (inflated) boxes met.
    pub broad_pairs: usize,
    /// Pairs that survived the ignore rules and went to triangle tests.
    pub narrow_pairs: usize,
    pub clashes: usize,
}

struct Hit {
    depth: Option<f64>,
    distance: Option<f64>,
    bbox: Aabb,
    containment: bool,
}

/// Most boundary contacts validated by a probe per mesh pair.
const MAX_PROBES: usize = 128;

impl Scene {
    /// Runs one test. Clashes come back sorted by element ids, status `new`.
    pub fn run(&self, test: &ClashTest) -> (Vec<Clash>, RunStats) {
        let n = self.bodies.len();
        let in_a: Vec<bool> = self.bodies.iter().map(|b| test.a.matches(b)).collect();
        let in_b: Vec<bool> = self.bodies.iter().map(|b| test.b.matches(b)).collect();
        let scope: Vec<usize> = (0..n).filter(|&i| (in_a[i] || in_b[i]) && !self.boxes[i].is_empty()).collect();
        let mut stats = RunStats { bodies_in_scope: scope.len(), ..RunStats::default() };
        let eps = test.epsilon_mm.max(0.0);
        let margin = match test.kind {
            ClashKind::Hard => eps,
            ClashKind::Clearance => test.tolerance_mm.max(0.0),
        };
        if test.kind == ClashKind::Clearance && test.tolerance_mm <= 0.0 {
            return (Vec::new(), stats);
        }

        // Sweep along the axis on which the boxes are most spread out.
        let axis = (0..3)
            .max_by(|&p, &q| {
                let spread = |k: usize| {
                    let (lo, hi) = scope.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &i| (lo.min(self.boxes[i].min[k]), hi.max(self.boxes[i].max[k])));
                    hi - lo
                };
                spread(p).total_cmp(&spread(q))
            })
            .unwrap_or(0);
        let mut order = scope.clone();
        order.sort_by(|&i, &j| self.boxes[i].min[axis].total_cmp(&self.boxes[j].min[axis]));

        let mut clashes = Vec::new();
        for (oi, &i) in order.iter().enumerate() {
            let reach = self.boxes[i].max[axis] + margin;
            for &j in &order[oi + 1..] {
                if self.boxes[j].min[axis] > reach {
                    break;
                }
                if !self.boxes[i].overlaps(&self.boxes[j], margin) {
                    continue;
                }
                stats.broad_pairs += 1;
                if !((in_a[i] && in_b[j]) || (in_a[j] && in_b[i])) {
                    continue;
                }
                let (ia, ib) = (self.bodies[i].id, self.bodies[j].id);
                if ia == ib || test.is_approved(ia, ib) || (test.ignore_host_chain && self.host_related(ia, ib)) {
                    continue;
                }
                stats.narrow_pairs += 1;
                if let Some(hit) = self.narrow(i, j, test, eps) {
                    clashes.push(self.record(i, j, test, hit));
                }
            }
        }
        clashes.sort_by(|p, q| (p.element_a, p.element_b, &p.id).cmp(&(q.element_a, q.element_b, &q.id)));
        stats.clashes = clashes.len();
        (clashes, stats)
    }

    /// Runs several tests; the result of each is independent.
    pub fn run_all(&self, tests: &[ClashTest]) -> (Vec<Clash>, Vec<RunStats>) {
        let mut all = Vec::new();
        let mut stats = Vec::new();
        for t in tests {
            let (c, s) = self.run(t);
            all.extend(c);
            stats.push(s);
        }
        (all, stats)
    }

    fn record(&self, i: usize, j: usize, test: &ClashTest, hit: Hit) -> Clash {
        let (i, j) = if self.bodies[i].id <= self.bodies[j].id { (i, j) } else { (j, i) };
        let (a, b) = (&self.bodies[i], &self.bodies[j]);
        Clash {
            id: clash_id(&test.name, a.id, b.id),
            test: test.name.clone(),
            element_a: a.id,
            element_b: b.id,
            mark_a: a.mark.clone(),
            mark_b: b.mark.clone(),
            category_a: a.category.clone(),
            category_b: b.category.clone(),
            kind: test.kind,
            depth_mm: hit.depth,
            distance_mm: hit.distance,
            containment: hit.containment,
            point: hit.bbox.centre(),
            overlap_box: hit.bbox,
            status: Status::New,
            comment: String::new(),
            assignee: String::new(),
        }
    }

    /// Narrow phase for two bodies: the worst hit over all their mesh pairs.
    fn narrow(&self, i: usize, j: usize, test: &ClashTest, eps: f64) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        for pa in &self.parts[i] {
            for pb in &self.parts[j] {
                let hit = match test.kind {
                    ClashKind::Hard => hard_parts(pa, pb, eps, test.touching_counts),
                    ClashKind::Clearance => clearance_parts(pa, pb, test.tolerance_mm, eps),
                };
                if let Some(h) = hit {
                    best = Some(match best {
                        None => h,
                        Some(prev) => Hit {
                            depth: match (prev.depth, h.depth) {
                                (Some(a), Some(b)) => Some(a.max(b)),
                                (a, b) => a.or(b),
                            },
                            distance: match (prev.distance, h.distance) {
                                (Some(a), Some(b)) => Some(a.min(b)),
                                (a, b) => a.or(b),
                            },
                            bbox: prev.bbox.union(&h.bbox),
                            containment: prev.containment && h.containment,
                        },
                    });
                }
            }
        }
        best
    }
}

/// Shortest axis move that carries `inner` wholly out of `outer`'s box.
fn exit_depth(inner: &Aabb, outer: &Aabb) -> f64 {
    (0..3).map(|k| (inner.max[k] - outer.min[k]).min(outer.max[k] - inner.min[k])).fold(f64::INFINITY, f64::min)
}

fn hard_parts(pa: &Part, pb: &Part, eps: f64, touch: bool) -> Option<Hit> {
    if !pa.aabb.overlaps(&pb.aabb, eps) {
        return None;
    }
    let region = pa.aabb.intersection(&pb.aabb);
    let ca = pa.tris_near(&region, eps);
    let cb = pb.tris_near(&region, eps);
    let mut pts: Vec<V3> = Vec::new();
    // Contacts on the boundary of a triangle of a closed target: (point, edge direction, target is b).
    let mut boundary: Vec<(V3, V3, bool)> = Vec::new();
    for &ia in &ca {
        let ta = &pa.tris[ia];
        for &ib in &cb {
            if !pa.boxes[ia].overlaps(&pb.boxes[ib], eps) {
                continue;
            }
            let tb = &pb.tris[ib];
            for (p, q) in [(ta[0], ta[1]), (ta[1], ta[2]), (ta[2], ta[0])] {
                if let Some((x, interior)) = seg_tri_cross(p, q, tb, eps) {
                    if interior || !pb.closed {
                        pts.push(x);
                    } else {
                        boundary.push((x, sub(q, p), true));
                    }
                }
            }
            for (p, q) in [(tb[0], tb[1]), (tb[1], tb[2]), (tb[2], tb[0])] {
                if let Some((x, interior)) = seg_tri_cross(p, q, ta, eps) {
                    if interior || !pa.closed {
                        pts.push(x);
                    } else {
                        boundary.push((x, sub(q, p), false));
                    }
                }
            }
            if pa.closed && pb.closed {
                if let Some(poly) = coplanar_same_side_overlap(ta, tb, eps) {
                    pts.extend(poly);
                }
            }
        }
    }
    // A boundary contact is a clash only if the edge runs into the target's interior on
    // one side of it; otherwise the bodies just graze.
    for (x, dir, target_is_b) in boundary.into_iter().take(MAX_PROBES) {
        let target = if target_is_b { pb } else { pa };
        let l = len(dir);
        if l <= 0.0 {
            continue;
        }
        let step = (100.0 * eps).max(0.01).min(0.25 * l);
        let u = scale(dir, 1.0 / l);
        if target.strictly_inside(add(x, scale(u, step)), eps) || target.strictly_inside(sub(x, scale(u, step)), eps) {
            pts.push(x);
        }
    }
    if !pts.is_empty() {
        let b = Aabb::of_points(pts.iter());
        let common = pa.aabb.intersection(&pb.aabb);
        let b = {
            let c = b.intersection(&common);
            if c.is_empty() {
                b
            } else {
                c
            }
        };
        let e = b.extent();
        return Some(Hit { depth: Some(e[0].min(e[1]).min(e[2])), distance: None, bbox: b, containment: false });
    }
    if touch {
        let mut best = (eps, None);
        for &ia in &ca {
            for &ib in &cb {
                if pa.boxes[ia].gap(&pb.boxes[ib]) > best.0 {
                    continue;
                }
                let (d, p, q) = tri_closest(&pa.tris[ia], &pb.tris[ib]);
                if d <= best.0 {
                    best = (d, Some((p, q)));
                }
            }
        }
        if let (_, Some((p, q))) = best {
            return Some(Hit { depth: Some(0.0), distance: None, bbox: Aabb::of_points([p, q].iter()), containment: false });
        }
    }
    containment(pa, pb, eps).map(|(bbox, depth)| Hit { depth: Some(depth), distance: None, bbox, containment: true })
}

/// One closed part wholly inside another closed part, with no surface contact (the caller
/// has already found no crossing). Returns the inner box and the exit depth.
fn containment(pa: &Part, pb: &Part, eps: f64) -> Option<(Aabb, f64)> {
    if !pa.closed || !pb.closed {
        return None;
    }
    for (inner, outer) in [(pa, pb), (pb, pa)] {
        if !outer.aabb.contains(&inner.aabb, eps) {
            continue;
        }
        // A vertex strictly inside decides it: the surfaces do not cross, so the inner body
        // is entirely on one side. Vertices on the outer surface are skipped.
        let near_surface = |v: &V3| {
            outer.tris_near(&Aabb { min: *v, max: *v }, eps).into_iter().any(|t| dist(closest_pt_tri(*v, &outer.tris[t]), *v) <= eps)
        };
        if let Some(v) = inner.verts.iter().take(64).find(|v| !near_surface(v)) {
            if outer.strictly_inside(*v, eps) {
                return Some((inner.aabb, exit_depth(&inner.aabb, &outer.aabb)));
            }
        }
    }
    None
}

fn clearance_parts(pa: &Part, pb: &Part, tol: f64, eps: f64) -> Option<Hit> {
    if pa.aabb.gap(&pb.aabb) >= tol {
        return None;
    }
    let ca = pa.tris_near(&pb.aabb, tol);
    let cb = pb.tris_near(&pa.aabb, tol);
    let mut best: (f64, Option<(V3, V3)>) = (tol, None);
    'outer: for &ia in &ca {
        for &ib in &cb {
            if pa.boxes[ia].gap(&pb.boxes[ib]) >= best.0 {
                continue;
            }
            let (d, p, q) = tri_closest(&pa.tris[ia], &pb.tris[ib]);
            if d < best.0 {
                best = (d, Some((p, q)));
                if d == 0.0 {
                    break 'outer;
                }
            }
        }
    }
    if let (d, Some((p, q))) = best {
        return Some(Hit { depth: None, distance: Some(d), bbox: Aabb::of_points([p, q].iter()), containment: false });
    }
    containment(pa, pb, eps).map(|(bbox, _)| Hit { depth: None, distance: Some(0.0), bbox, containment: true })
}
