//! Pure geometry: vectors, boxes and triangle tests. Millimetres throughout.

use serde::{Deserialize, Serialize};

pub type V3 = [f64; 3];
pub type Tri = [V3; 3];

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub fn len(a: V3) -> f64 {
    dot(a, a).sqrt()
}
pub fn dist(a: V3, b: V3) -> f64 {
    len(sub(a, b))
}
pub fn mid(a: V3, b: V3) -> V3 {
    scale(add(a, b), 0.5)
}

/// Unit normal of a triangle (counter-clockwise seen from outside), `None` if degenerate.
pub fn unit_normal(t: &Tri) -> Option<V3> {
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let l = len(n);
    (l > 1e-9).then(|| scale(n, 1.0 / l))
}

/// An axis-aligned box in world millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub min: V3,
    pub max: V3,
}

impl Aabb {
    pub fn empty() -> Aabb {
        Aabb { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] }
    }
    pub fn of_points<'a>(pts: impl IntoIterator<Item = &'a V3>) -> Aabb {
        let mut b = Aabb::empty();
        for p in pts {
            b.grow(*p);
        }
        b
    }
    pub fn is_empty(&self) -> bool {
        (0..3).any(|k| self.min[k] > self.max[k])
    }
    pub fn grow(&mut self, p: V3) {
        for (k, x) in p.iter().enumerate() {
            self.min[k] = self.min[k].min(*x);
            self.max[k] = self.max[k].max(*x);
        }
    }
    pub fn union(&self, o: &Aabb) -> Aabb {
        let mut r = *self;
        if !o.is_empty() {
            r.grow(o.min);
            r.grow(o.max);
        }
        r
    }
    /// The common box, possibly empty (min greater than max).
    pub fn intersection(&self, o: &Aabb) -> Aabb {
        let mut r = *self;
        for k in 0..3 {
            r.min[k] = r.min[k].max(o.min[k]);
            r.max[k] = r.max[k].min(o.max[k]);
        }
        r
    }
    /// True when the gap between the boxes is at most `m` along every axis.
    pub fn overlaps(&self, o: &Aabb, m: f64) -> bool {
        (0..3).all(|k| self.min[k] - m <= o.max[k] && o.min[k] - m <= self.max[k])
    }
    /// Euclidean distance between the boxes, zero when they overlap.
    pub fn gap(&self, o: &Aabb) -> f64 {
        let mut s = 0.0;
        for k in 0..3 {
            let g = (self.min[k] - o.max[k]).max(o.min[k] - self.max[k]).max(0.0);
            s += g * g;
        }
        s.sqrt()
    }
    pub fn contains(&self, o: &Aabb, m: f64) -> bool {
        (0..3).all(|k| self.min[k] - m <= o.min[k] && o.max[k] <= self.max[k] + m)
    }
    pub fn contains_point(&self, p: V3, m: f64) -> bool {
        (0..3).all(|k| self.min[k] - m <= p[k] && p[k] <= self.max[k] + m)
    }
    pub fn centre(&self) -> V3 {
        mid(self.min, self.max)
    }
    pub fn extent(&self) -> V3 {
        sub(self.max, self.min)
    }
}

pub fn tri_box(t: &Tri) -> Aabb {
    Aabb::of_points(t.iter())
}

/// Where the segment `p`..`q` strictly crosses the plane of `t` (both ends more than `eps` off
/// it, on opposite sides) and the crossing lies in the triangle (edges inclusive, within
/// `eps`). The flag says whether the point is also more than `eps` inside every edge.
pub fn seg_tri_cross(p: V3, q: V3, t: &Tri, eps: f64) -> Option<(V3, bool)> {
    let nn = unit_normal(t)?;
    let dp = dot(nn, sub(p, t[0]));
    let dq = dot(nn, sub(q, t[0]));
    if !((dp > eps && dq < -eps) || (dp < -eps && dq > eps)) {
        return None;
    }
    let x = add(p, scale(sub(q, p), dp / (dp - dq)));
    let mut interior = true;
    for (u, v) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
        let e = sub(v, u);
        let side = dot(cross(e, sub(x, u)), nn);
        let l = len(e);
        if side < -eps * l {
            return None;
        }
        if side <= eps * l {
            interior = false;
        }
    }
    Some((x, interior))
}

/// Inclusive piercing test used for distance: an edge that meets the other triangle at all.
fn seg_tri_touch(p: V3, q: V3, t: &Tri) -> Option<V3> {
    let nn = unit_normal(t)?;
    let dp = dot(nn, sub(p, t[0]));
    let dq = dot(nn, sub(q, t[0]));
    if dp * dq > 0.0 || (dp == 0.0 && dq == 0.0) {
        return None;
    }
    let x = add(p, scale(sub(q, p), dp / (dp - dq)));
    for (u, v) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
        if dot(cross(sub(v, u), sub(x, u)), nn) < -1e-12 * len(sub(v, u)) {
            return None;
        }
    }
    Some(x)
}

/// Closest point on a triangle to `p` (Ericson, Real-Time Collision Detection).
pub fn closest_pt_tri(p: V3, t: &Tri) -> V3 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add(a, scale(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, scale(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return add(b, scale(sub(c, b), w));
    }
    let denom = 1.0 / (va + vb + vc);
    add(a, add(scale(ab, vb * denom), scale(ac, vc * denom)))
}

/// Closest points between segments `p1`..`q1` and `p2`..`q2` (Ericson).
pub fn closest_seg_seg(p1: V3, q1: V3, p2: V3, q2: V3) -> (V3, V3) {
    const E: f64 = 1e-12;
    let d1 = sub(q1, p1);
    let d2 = sub(q2, p2);
    let r = sub(p1, p2);
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    let (s, t);
    if a <= E && e <= E {
        return (p1, p2);
    }
    if a <= E {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = dot(d1, r);
        if e <= E {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            let mut s0 = if denom > E * a * e { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let mut t0 = (b * s0 + f) / e;
            if t0 < 0.0 {
                t0 = 0.0;
                s0 = (-c / a).clamp(0.0, 1.0);
            } else if t0 > 1.0 {
                t0 = 1.0;
                s0 = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = s0;
            t = t0;
        }
    }
    (add(p1, scale(d1, s)), add(p2, scale(d2, t)))
}

/// Minimum distance between two triangles with the closest pair of points. Exact for any
/// configuration: zero (with a point on both) when they intersect, otherwise the least of the
/// six vertex-face and nine edge-edge distances.
pub fn tri_closest(t1: &Tri, t2: &Tri) -> (f64, V3, V3) {
    for (u, v) in [(t1[0], t1[1]), (t1[1], t1[2]), (t1[2], t1[0])] {
        if let Some(x) = seg_tri_touch(u, v, t2) {
            return (0.0, x, x);
        }
    }
    for (u, v) in [(t2[0], t2[1]), (t2[1], t2[2]), (t2[2], t2[0])] {
        if let Some(x) = seg_tri_touch(u, v, t1) {
            return (0.0, x, x);
        }
    }
    let mut best = (f64::INFINITY, t1[0], t2[0]);
    let mut consider = |p: V3, q: V3| {
        let d = dist(p, q);
        if d < best.0 {
            best = (d, p, q);
        }
    };
    for v in t1 {
        consider(*v, closest_pt_tri(*v, t2));
    }
    for v in t2 {
        consider(closest_pt_tri(*v, t1), *v);
    }
    for (a, b) in [(t1[0], t1[1]), (t1[1], t1[2]), (t1[2], t1[0])] {
        for (c, d) in [(t2[0], t2[1]), (t2[1], t2[2]), (t2[2], t2[0])] {
            let (p, q) = closest_seg_seg(a, b, c, d);
            consider(p, q);
        }
    }
    best
}

/// Overlap of two coplanar triangles that face the same way (so their solids lie on the same
/// side): the clipped polygon, if its area is positive.
pub fn coplanar_same_side_overlap(t1: &Tri, t2: &Tri, eps: f64) -> Option<Vec<V3>> {
    let n1 = unit_normal(t1)?;
    let n2 = unit_normal(t2)?;
    if dot(n1, n2) < 1.0 - 1e-9 || dot(n1, sub(t2[0], t1[0])).abs() > eps {
        return None;
    }
    let mut poly: Vec<V3> = t1.to_vec();
    for (u, v) in [(t2[0], t2[1]), (t2[1], t2[2]), (t2[2], t2[0])] {
        let inward = cross(n2, sub(v, u));
        let side = |p: V3| dot(inward, sub(p, u));
        let mut out = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            let (sp, sq) = (side(p), side(q));
            if sp >= 0.0 {
                out.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                out.push(add(p, scale(sub(q, p), sp / (sp - sq))));
            }
        }
        poly = out;
        if poly.len() < 3 {
            return None;
        }
    }
    let mut area2 = [0.0; 3];
    for i in 1..poly.len() - 1 {
        let c = cross(sub(poly[i], poly[0]), sub(poly[i + 1], poly[0]));
        for k in 0..3 {
            area2[k] += c[k];
        }
    }
    (0.5 * len(area2) > 1e-6).then_some(poly)
}

/// Moller-Trumbore: does the ray from `o` along `d` hit the triangle ahead of `o`?
pub fn ray_hits_tri(o: V3, d: V3, t: &Tri) -> bool {
    let e1 = sub(t[1], t[0]);
    let e2 = sub(t[2], t[0]);
    let h = cross(d, e2);
    let det = dot(e1, h);
    if det.abs() < 1e-14 {
        return false;
    }
    let f = 1.0 / det;
    let s = sub(o, t[0]);
    let u = f * dot(s, h);
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = cross(s, e1);
    let v = f * dot(d, q);
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    f * dot(e2, q) > 1e-9
}
