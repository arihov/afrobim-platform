// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Polygon triangulation (ear clipping with hole bridging) and the solids built from profiles.

use super::xform::{cross, dot, unit, V3};
#[cfg(test)]
use super::xform::sub;

pub type P2 = [f64; 2];

fn area2(r: &[P2]) -> f64 {
    let mut s = 0.0;
    for i in 0..r.len() {
        let (a, b) = (r[i], r[(i + 1) % r.len()]);
        s += a[0] * b[1] - b[0] * a[1];
    }
    s * 0.5
}

fn cross2(o: P2, a: P2, b: P2) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn in_tri(a: P2, b: P2, c: P2, p: P2, eps: f64) -> bool {
    // a, b, c counter-clockwise; true when p is inside or on the boundary.
    cross2(a, b, p) >= -eps && cross2(b, c, p) >= -eps && cross2(c, a, p) >= -eps
}

/// Triangulates `outer` with `holes`. Returns triangles as indices into the concatenation
/// `outer ++ holes[0] ++ holes[1] ...`, wound counter-clockwise (positive area). Orientation of the
/// input rings does not matter. Rings with fewer than 3 points are ignored; degenerate input
/// yields fewer triangles, never a panic. Complexity is quadratic in the ring size.
pub fn triangulate(outer: &[P2], holes: &[Vec<P2>]) -> Vec<[u32; 3]> {
    if outer.len() < 3 {
        return Vec::new();
    }
    let mut pts: Vec<P2> = outer.to_vec();
    let mut ring: Vec<usize> = (0..outer.len()).collect();
    if area2(outer) < 0.0 {
        ring.reverse();
    }
    // Hole rings, clockwise, with the index of their rightmost vertex.
    let mut hole_rings: Vec<Vec<usize>> = Vec::new();
    for h in holes {
        if h.len() < 3 {
            continue;
        }
        let base = pts.len();
        pts.extend_from_slice(h);
        let mut r: Vec<usize> = (base..base + h.len()).collect();
        if area2(h) > 0.0 {
            r.reverse();
        }
        hole_rings.push(r);
    }
    let size = pts.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]);
    let diag = (size[2] - size[0]).hypot(size[3] - size[1]).max(1e-12);
    let eps = diag * diag * 1e-14;

    // Bridge each hole to the ring along the shortest segment that crosses no edge and leaves both
    // ends into the polygon's interior (see `bridge`). A hole no bridge can be found for is left
    // out; the caller's area check then reports it.
    let outer_len = outer.len();
    let originals: Vec<Vec<usize>> = std::iter::once((0..outer_len).collect()).chain(hole_rings.iter().cloned()).collect();
    let mut pending: Vec<Vec<usize>> = hole_rings;
    while let Some(hole) = pending.pop() {
        if let Some(next) = bridge(&pts, &ring, &hole, &pending, &originals, eps) {
            ring = next;
        }
    }

    // Ear clipping on the (possibly bridged) ring.
    let n = ring.len();
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(n.saturating_sub(2));
    let mut left = n;
    let mut cur = 0usize;
    let mut stall = 0usize;
    while left > 3 {
        let (p, c, nx) = (prev[cur], cur, next[cur]);
        let (a, b, d) = (pts[ring[p]], pts[ring[c]], pts[ring[nx]]);
        let convex = cross2(a, b, d) > eps;
        let mut ear = convex;
        if ear {
            let mut j = next[nx];
            while j != p {
                let q = pts[ring[j]];
                let coincident = q == a || q == b || q == d;
                if !coincident && in_tri(a, b, d, q, 0.0) {
                    ear = false;
                    break;
                }
                j = next[j];
            }
        }
        // A zero-area corner (collinear or a bridge tip) is dropped without a triangle.
        let flat = cross2(a, b, d).abs() <= eps;
        if ear || flat || stall > left {
            if ear || (stall > left && cross2(a, b, d) > 0.0) {
                tris.push([ring[p] as u32, ring[c] as u32, ring[nx] as u32]);
            }
            next[p] = nx;
            prev[nx] = p;
            left -= 1;
            cur = nx;
            stall = 0;
        } else {
            cur = nx;
            stall += 1;
        }
    }
    if left == 3 {
        let (p, c, nx) = (prev[cur], cur, next[cur]);
        if cross2(pts[ring[p]], pts[ring[c]], pts[ring[nx]]) > eps {
            tris.push([ring[p] as u32, ring[c] as u32, ring[nx] as u32]);
        }
    }
    tris
}

/// Signed angle test: does the direction `t` from `cur` lie strictly inside the wedge of a ring
/// whose interior is on the left of its direction of travel (counter-clockwise outer ring, or a
/// clockwise hole ring)? The wedge sweeps counter-clockwise from the outgoing edge to the reversed
/// incoming edge.
fn in_wedge(prev: P2, cur: P2, next: P2, t: P2) -> bool {
    let ang = |v: P2| v[1].atan2(v[0]);
    let a_out = ang([next[0] - cur[0], next[1] - cur[1]]);
    let a_in = ang([prev[0] - cur[0], prev[1] - cur[1]]);
    let a_t = ang([t[0] - cur[0], t[1] - cur[1]]);
    let tau = std::f64::consts::TAU;
    let sweep = (a_in - a_out).rem_euclid(tau);
    let at = (a_t - a_out).rem_euclid(tau);
    // A reflex or straight vertex has a sweep of 180 degrees or more; a zero sweep (spike) is
    // taken as the full turn.
    let sweep = if sweep < 1e-12 { tau } else { sweep };
    at > 1e-9 && at < sweep - 1e-9
}

/// True when segments `ab` and `cd` meet anywhere, ends included.
fn segments_meet(a: P2, b: P2, c: P2, d: P2, eps: f64) -> bool {
    let o1 = cross2(a, b, c);
    let o2 = cross2(a, b, d);
    let o3 = cross2(c, d, a);
    let o4 = cross2(c, d, b);
    if ((o1 > eps && o2 < -eps) || (o1 < -eps && o2 > eps)) && ((o3 > eps && o4 < -eps) || (o3 < -eps && o4 > eps)) {
        return true;
    }
    let on = |p: P2, q: P2, r: P2, o: f64| o.abs() <= eps && r[0] >= p[0].min(q[0]) - 1e-12 && r[0] <= p[0].max(q[0]) + 1e-12 && r[1] >= p[1].min(q[1]) - 1e-12 && r[1] <= p[1].max(q[1]) + 1e-12;
    on(a, b, c, o1) || on(a, b, d, o2) || on(c, d, a, o3) || on(c, d, b, o4)
}

fn inside_parity(pts: &[P2], rings: &[Vec<usize>], p: P2) -> bool {
    let mut inside = false;
    for r in rings {
        for k in 0..r.len() {
            let (a, b) = (pts[r[k]], pts[r[(k + 1) % r.len()]]);
            if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
                inside = !inside;
            }
        }
    }
    inside
}

/// Splices `hole` (clockwise) into `ring` (counter-clockwise) along the shortest valid bridge.
fn bridge(pts: &[P2], ring: &[usize], hole: &[usize], pending: &[Vec<usize>], originals: &[Vec<usize>], eps: f64) -> Option<Vec<usize>> {
    // Every edge a bridge must not cross: the ring (earlier bridges included), this hole, and the
    // holes still to come.
    let mut edges: Vec<(P2, P2)> = Vec::new();
    for r in std::iter::once(&ring.to_vec()).chain(std::iter::once(&hole.to_vec())).chain(pending.iter()) {
        for k in 0..r.len() {
            edges.push((pts[r[k]], pts[r[(k + 1) % r.len()]]));
        }
    }
    let mut cands: Vec<(f64, usize, usize)> = Vec::with_capacity(ring.len() * hole.len());
    for (ap, &a) in ring.iter().enumerate() {
        for (bp, &b) in hole.iter().enumerate() {
            let (pa, pb) = (pts[a], pts[b]);
            cands.push(((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2), ap, bp));
        }
    }
    cands.sort_by(|x, y| x.0.total_cmp(&y.0));
    for (_, ap, bp) in cands {
        let (a, b) = (ring[ap], hole[bp]);
        let (pa, pb) = (pts[a], pts[b]);
        let rn = ring.len();
        let hn = hole.len();
        if !in_wedge(pts[ring[(ap + rn - 1) % rn]], pa, pts[ring[(ap + 1) % rn]], pb) || !in_wedge(pts[hole[(bp + hn - 1) % hn]], pb, pts[hole[(bp + 1) % hn]], pa) {
            continue;
        }
        let blocked = edges.iter().any(|&(c, d)| {
            let touches = |p: P2| p == pa || p == pb;
            if touches(c) && touches(d) {
                return false; // the edge is the bridge itself
            }
            if touches(c) || touches(d) {
                // Shares one end with the bridge: it only blocks if it runs back along it.
                let (shared, other) = if touches(c) { (c, d) } else { (d, c) };
                let far = if shared == pa { pb } else { pa };
                return cross2(shared, far, other).abs() <= eps && (other[0] - shared[0]) * (far[0] - shared[0]) + (other[1] - shared[1]) * (far[1] - shared[1]) > 0.0;
            }
            segments_meet(pa, pb, c, d, eps)
        });
        if blocked {
            continue;
        }
        let mid = [(pa[0] + pb[0]) * 0.5, (pa[1] + pb[1]) * 0.5];
        if !inside_parity(pts, originals, mid) {
            continue;
        }
        let mut next: Vec<usize> = ring[..=ap].to_vec();
        for s in 0..=hn {
            next.push(hole[(bp + s) % hn]);
        }
        next.push(a);
        next.extend_from_slice(&ring[ap + 1..]);
        return Some(next);
    }
    None
}

/// Unit normal of a 3D polygon by Newell's method (the direction a counter-clockwise ring faces).
pub fn newell(ring: &[V3]) -> Option<V3> {
    let mut n = [0.0; 3];
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    unit(n)
}

/// Triangulates a planar 3D polygon with holes. `outer` is wound counter-clockwise seen from the
/// face's front; triangles keep that facing. Returns triangles as indices into
/// `outer ++ holes...`, and whether their total area equals the outline's area minus the holes'
/// (it does not for self-intersecting or overlapping loops, and for a bridge the algorithm could
/// not make). Faces with a degenerate outer ring give nothing (and `true`).
pub fn triangulate_3d(outer: &[V3], holes: &[Vec<V3>]) -> (Vec<[u32; 3]>, bool) {
    let Some(n) = newell(outer) else { return (Vec::new(), true) };
    // In-plane basis (u, v) with u x v = n.
    let helper = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = unit(cross(helper, n)).unwrap_or([1.0, 0.0, 0.0]);
    let v = cross(n, u);
    let flat = |r: &[V3]| -> Vec<P2> { r.iter().map(|p| [dot(*p, u), dot(*p, v)]).collect() };
    let o2 = flat(outer);
    let h2: Vec<Vec<P2>> = holes.iter().map(|h| flat(h)).collect();
    let tris = triangulate(&o2, &h2);
    let mut pts: Vec<P2> = o2.clone();
    for h in &h2 {
        pts.extend_from_slice(h);
    }
    let got: f64 = tris.iter().map(|t| cross2(pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]) * 0.5).sum();
    let want = area2(&o2).abs() - h2.iter().map(|h| area2(h).abs()).sum::<f64>();
    (tris, (got - want).abs() <= 1e-6 * area2(&o2).abs().max(1e-12))
}

/// Signed volume of a triangle mesh (positive when the triangles face outward).
pub fn signed_volume(verts: &[V3], tris: &[[u32; 3]]) -> f64 {
    tris.iter()
        .map(|t| {
            let (a, b, c) = (verts[t[0] as usize], verts[t[1] as usize], verts[t[2] as usize]);
            dot(a, cross(b, c)) / 6.0
        })
        .sum()
}

/// A closed prism: `profile` (counter-clockwise or not) in the XY plane at z = 0, extruded by
/// `offset` (a full 3D vector, `direction * depth`). Triangles face outward; a profile with
/// holes is supported.
pub fn prism(profile: &[P2], holes: &[Vec<P2>], offset: V3) -> (Vec<V3>, Vec<[u32; 3]>) {
    let mut ring: Vec<P2> = profile.to_vec();
    if area2(&ring) < 0.0 {
        ring.reverse();
    }
    let hole_rings: Vec<Vec<P2>> = holes
        .iter()
        .map(|h| {
            let mut r = h.clone();
            if area2(&r) > 0.0 {
                r.reverse();
            }
            r
        })
        .collect();
    let cap = triangulate(&ring, &hole_rings);
    let rings: Vec<&Vec<P2>> = std::iter::once(&ring).chain(hole_rings.iter()).collect();
    let total: usize = rings.iter().map(|r| r.len()).sum();
    let mut verts: Vec<V3> = Vec::with_capacity(total * 2);
    for r in &rings {
        verts.extend(r.iter().map(|p| [p[0], p[1], 0.0]));
    }
    for r in &rings {
        verts.extend(r.iter().map(|p| [p[0] + offset[0], p[1] + offset[1], offset[2]]));
    }
    let mut tris: Vec<[u32; 3]> = Vec::new();
    // Caps: bottom faces away from the extrusion (reverse), top faces along it.
    for t in &cap {
        tris.push([t[0], t[2], t[1]]);
        tris.push([t[0] + total as u32, t[1] + total as u32, t[2] + total as u32]);
    }
    let mut base = 0u32;
    for r in &rings {
        let m = r.len() as u32;
        for i in 0..m {
            let j = (i + 1) % m;
            let (a, b) = (base + i, base + j);
            tris.push([a, b, b + total as u32]);
            tris.push([a, b + total as u32, a + total as u32]);
        }
        base += m;
    }
    // The construction assumes the offset leaves the profile plane upwards; if it goes down the
    // whole shell is inside out.
    if signed_volume(&verts, &tris) < 0.0 {
        for t in &mut tris {
            t.swap(1, 2);
        }
    }
    (verts, tris)
}

/// A circle as a polygon of `segments` vertices, the first on the +X axis.
pub fn circle(radius: f64, segments: u32) -> Vec<P2> {
    let n = segments.max(3);
    (0..n)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            [radius * a.cos(), radius * a.sin()]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area_of(pts: &[P2], tris: &[[u32; 3]]) -> f64 {
        tris.iter().map(|t| cross2(pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]) * 0.5).sum()
    }

    fn all_ccw(pts: &[P2], tris: &[[u32; 3]]) -> bool {
        tris.iter().all(|t| cross2(pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]) > 0.0)
    }

    fn pool(outer: &[P2], holes: &[Vec<P2>]) -> Vec<P2> {
        let mut p = outer.to_vec();
        for h in holes {
            p.extend_from_slice(h);
        }
        p
    }

    #[test]
    fn square_and_l_shape_keep_their_area() {
        let sq = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let t = triangulate(&sq, &[]);
        assert_eq!(t.len(), 2);
        assert!((area_of(&sq, &t) - 16.0).abs() < 1e-12 && all_ccw(&sq, &t));
        let l = [[0.0, 0.0], [4.0, 0.0], [4.0, 1.0], [1.0, 1.0], [1.0, 4.0], [0.0, 4.0]];
        let t = triangulate(&l, &[]);
        assert_eq!(t.len(), 4);
        assert!((area_of(&l, &t) - 7.0).abs() < 1e-12 && all_ccw(&l, &t));
        // Clockwise input gives the same area and counter-clockwise triangles.
        let cw: Vec<P2> = l.iter().rev().copied().collect();
        let t = triangulate(&cw, &[]);
        assert!((area_of(&cw, &t) - 7.0).abs() < 1e-12);
    }

    #[test]
    fn collinear_points_and_a_closing_duplicate_do_not_break_it() {
        let r = [[0.0, 0.0], [2.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let t = triangulate(&r, &[]);
        assert!((area_of(&r, &t) - 16.0).abs() < 1e-12);
        assert!(triangulate(&[[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]], &[]).is_empty());
        assert!(triangulate(&[[0.0, 0.0], [1.0, 0.0]], &[]).is_empty());
    }

    #[test]
    fn one_and_two_holes() {
        let outer = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let h1 = vec![[2.0, 2.0], [4.0, 2.0], [4.0, 4.0], [2.0, 4.0]];
        let t = triangulate(&outer, std::slice::from_ref(&h1));
        let p = pool(&outer, std::slice::from_ref(&h1));
        assert!((area_of(&p, &t) - (100.0 - 4.0)).abs() < 1e-9, "{}", area_of(&p, &t));
        let h2 = vec![[6.0, 5.0], [8.0, 5.0], [8.0, 8.0], [6.0, 8.0]];
        let t = triangulate(&outer, &[h1.clone(), h2.clone()]);
        let p = pool(&outer, &[h1, h2]);
        assert!((area_of(&p, &t) - (100.0 - 4.0 - 6.0)).abs() < 1e-9, "{}", area_of(&p, &t));
        assert!(all_ccw(&p, &t));
    }

    #[test]
    fn two_side_by_side_holes_sharing_a_scanline() {
        // A window wall: two glazing openings whose bottom edges lie on the same horizontal line
        // (taken from a real faceted brep of a window frame).
        let outer = [[1.0, 0.0], [1.0, 1.2], [-1.0, 1.2], [-1.0, 0.0]];
        let h1 = vec![[0.93, 0.07], [0.035, 0.07], [0.035, 1.13], [0.93, 1.13]];
        let h2 = vec![[-0.035, 0.07], [-0.93, 0.07], [-0.93, 1.13], [-0.035, 1.13]];
        let holes = [h1, h2];
        let t = triangulate(&outer, &holes);
        let p = pool(&outer, &holes);
        let want = 2.4 - 2.0 * (0.895 * 1.06);
        assert!((area_of(&p, &t) - want).abs() < 1e-9, "{} vs {want}", area_of(&p, &t));
        assert!(all_ccw(&p, &t));
    }

    #[test]
    fn many_holes_in_a_grid_of_cells() {
        // A 100 x 100 plate with up to nine star-shaped holes, one in each of a random subset of
        // the cells of a 3 x 3 grid, with random turns and sizes: rows and columns of holes share
        // scanlines and tangent directions as often as not.
        let mut seed = 777u64;
        let mut rnd = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as f64 / (1u64 << 31) as f64
        };
        for case in 0..300 {
            let outer = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
            let mut holes: Vec<Vec<P2>> = Vec::new();
            for cell in 0..9 {
                if rnd() < 0.5 {
                    continue;
                }
                let (cx, cy) = ((cell % 3) as f64 * 33.3 + 16.6 + 1.0, (cell / 3) as f64 * 33.3 + 16.6 + 1.0);
                let n = 3 + (rnd() * 6.0) as usize;
                let r = 4.0 + 10.0 * rnd();
                let turn = rnd() * std::f64::consts::TAU;
                let rect = case % 3 == 0;
                let mut h: Vec<P2> = if rect {
                    vec![[cx - r, cy - r * 0.6], [cx + r, cy - r * 0.6], [cx + r, cy + r * 0.6], [cx - r, cy + r * 0.6]]
                } else {
                    (0..n).map(|k| {
                        let a = turn + std::f64::consts::TAU * k as f64 / n as f64;
                        let rr = r * (0.6 + 0.4 * rnd());
                        [cx + rr * a.cos(), cy + rr * a.sin()]
                    }).collect()
                };
                if case % 2 == 0 {
                    h.reverse();
                }
                holes.push(h);
            }
            let want = 10000.0 - holes.iter().map(|h| area2(h).abs()).sum::<f64>();
            let t = triangulate(&outer, &holes);
            let p = pool(&outer, &holes);
            assert!((area_of(&p, &t) - want).abs() < 1e-6 * want, "case {case}: {} vs {want} ({} holes)", area_of(&p, &t), holes.len());
            assert!(all_ccw(&p, &t), "case {case}");
        }
    }

    #[test]
    fn the_window_wall_in_every_orientation() {
        // The layout of the previous test turned by multiples of 90 degrees and mirrored: which
        // vertex is rightmost, and which scanline edges collide, changes in each.
        let base_outer = [[1.0, 0.0], [1.0, 1.2], [-1.0, 1.2], [-1.0, 0.0]];
        let h1 = [[0.93, 0.07], [0.035, 0.07], [0.035, 1.13], [0.93, 1.13]];
        let h2 = [[-0.035, 0.07], [-0.93, 0.07], [-0.93, 1.13], [-0.035, 1.13]];
        let want = 2.4 - 2.0 * (0.895 * 1.06);
        for mirror in [false, true] {
            for quarter in 0..4 {
                let tf = |p: &[f64; 2]| {
                    let (mut x, y) = (p[0], p[1]);
                    if mirror {
                        x = -x;
                    }
                    match quarter {
                        0 => [x, y],
                        1 => [-y, x],
                        2 => [-x, -y],
                        _ => [y, -x],
                    }
                };
                let outer: Vec<P2> = base_outer.iter().map(tf).collect();
                let holes: Vec<Vec<P2>> = vec![h1.iter().map(tf).collect(), h2.iter().map(tf).collect()];
                let t = triangulate(&outer, &holes);
                let p = pool(&outer, &holes);
                if (area_of(&p, &t) - want).abs() >= 1e-9 {
                    for tri in &t {
                        eprintln!("{:?} {:?} {:?}", p[tri[0] as usize], p[tri[1] as usize], p[tri[2] as usize]);
                    }
                }
                assert!((area_of(&p, &t) - want).abs() < 1e-9, "mirror {mirror} quarter {quarter}: {} vs {want}", area_of(&p, &t));
                assert!(all_ccw(&p, &t), "mirror {mirror} quarter {quarter}");
            }
        }
    }

    #[test]
    fn random_star_polygons_with_a_hole() {
        let mut seed = 12345u64;
        let mut rnd = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as f64 / (1u64 << 31) as f64
        };
        for case in 0..200 {
            let n = 5 + (case % 40);
            let outer: Vec<P2> = (0..n)
                .map(|k| {
                    let a = std::f64::consts::TAU * k as f64 / n as f64;
                    let r = 20.0 + 10.0 * rnd();
                    [r * a.cos(), r * a.sin()]
                })
                .collect();
            let hole = if case % 2 == 0 {
                let m = 3 + case % 7;
                Some((0..m).map(|k| {
                    let a = -std::f64::consts::TAU * k as f64 / m as f64;
                    let r = 3.0 + 6.0 * rnd();
                    [r * a.cos() + 2.0, r * a.sin() - 1.0]
                }).collect::<Vec<P2>>())
            } else {
                None
            };
            let holes: Vec<Vec<P2>> = hole.iter().cloned().collect();
            let want = area2(&outer).abs() - hole.as_ref().map(|h| area2(h).abs()).unwrap_or(0.0);
            let t = triangulate(&outer, &holes);
            let p = pool(&outer, &holes);
            let got = area_of(&p, &t);
            assert!((got - want).abs() < 1e-6 * want, "case {case}: {got} vs {want}");
            assert!(all_ccw(&p, &t), "case {case}");
        }
    }

    #[test]
    fn a_planar_3d_face_keeps_its_facing() {
        // A square in the XZ plane, counter-clockwise seen from -Y, so facing -Y.
        let f = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 0.0, 2.0], [0.0, 0.0, 2.0]];
        let n = newell(&f).unwrap();
        assert!((n[1] + 1.0).abs() < 1e-12);
        let (t, ok) = triangulate_3d(&f, &[]);
        assert!(ok);
        assert_eq!(t.len(), 2);
        for tri in &t {
            let (a, b, c) = (f[tri[0] as usize], f[tri[1] as usize], f[tri[2] as usize]);
            assert!(dot(cross(sub(b, a), sub(c, a)), n) > 0.0);
        }
    }

    #[test]
    fn a_prism_is_closed_with_outward_triangles_for_either_direction() {
        let sq = [[0.0, 0.0], [2.0, 0.0], [2.0, 3.0], [0.0, 3.0]];
        for off in [[0.0, 0.0, 5.0], [0.0, 0.0, -5.0], [1.0, 0.0, 5.0]] {
            let (v, t) = prism(&sq, &[], off);
            assert!((signed_volume(&v, &t) - 30.0).abs() < 1e-9, "{off:?}");
        }
        let hole = vec![[0.5, 0.5], [1.5, 0.5], [1.5, 1.5], [0.5, 1.5]];
        let (v, t) = prism(&sq, &[hole], [0.0, 0.0, 2.0]);
        assert!((signed_volume(&v, &t) - 10.0).abs() < 1e-9);
        // Every edge is shared by exactly two triangles after welding bridge duplicates.
        let (v, t) = prism(&circle(1.0, 16), &[], [0.0, 0.0, 1.0]);
        let vol = signed_volume(&v, &t);
        assert!((vol - 0.5 * 16.0 * (std::f64::consts::TAU / 16.0).sin()).abs() < 1e-9);
    }
}
