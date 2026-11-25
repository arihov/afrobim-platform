// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Plan geometry helpers. Arithmetic that mixes many points is done relative to a local origin
//! (the first point of the data), and results are returned as `local + origin`, so true UTM
//! coordinates keep their precision.

pub type P2 = [f64; 2];

pub fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}
pub fn add(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}
pub fn scale(a: P2, k: f64) -> P2 {
    [a[0] * k, a[1] * k]
}
pub fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
pub fn cross(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
pub fn len(a: P2) -> f64 {
    a[0].hypot(a[1])
}
pub fn unit(a: P2) -> P2 {
    let l = len(a);
    if l == 0.0 {
        [0.0, 0.0]
    } else {
        [a[0] / l, a[1] / l]
    }
}
/// Rotates a vector by +90 degrees (counter-clockwise).
pub fn left(a: P2) -> P2 {
    [-a[1], a[0]]
}
pub fn rot(a: P2, ang: f64) -> P2 {
    let (s, c) = ang.sin_cos();
    [a[0] * c - a[1] * s, a[0] * s + a[1] * c]
}

/// Intersection of segments `a0a1` and `b0b1` (all in the same local frame): the parameters
/// `(t, u)` along each in `[0, 1]` (with a small length tolerance), or `None` when they are
/// parallel or miss. Derived from solving `a0 + t (a1 - a0) = b0 + u (b1 - b0)` by Cramer's rule.
pub fn seg_intersect(a0: P2, a1: P2, b0: P2, b1: P2, eps: f64) -> Option<(f64, f64)> {
    let r = sub(a1, a0);
    let s = sub(b1, b0);
    let den = cross(r, s);
    if den.abs() <= 1e-14 * len(r) * len(s) {
        return None;
    }
    let qp = sub(b0, a0);
    let t = cross(qp, s) / den;
    let u = cross(qp, r) / den;
    let (te, ue) = (eps / len(r).max(1e-300), eps / len(s).max(1e-300));
    if t >= -te && t <= 1.0 + te && u >= -ue && u <= 1.0 + ue {
        Some((t.clamp(0.0, 1.0), u.clamp(0.0, 1.0)))
    } else {
        None
    }
}

/// Intersection point of two infinite lines (point and direction), or `None` if parallel.
pub fn line_line(p: P2, d: P2, q: P2, e: P2) -> Option<P2> {
    let den = cross(d, e);
    if den.abs() <= 1e-12 * len(d) * len(e) {
        return None;
    }
    let t = cross(sub(q, p), e) / den;
    Some(add(p, scale(d, t)))
}

/// Intersections of the infinite line (point `p`, unit direction `d`) with the circle (centre
/// `c`, radius `r`).
pub fn line_circle(p: P2, d: P2, c: P2, r: f64) -> Vec<P2> {
    let f = sub(p, c);
    let b = dot(f, d);
    let disc = b * b - (dot(f, f) - r * r);
    if disc < 0.0 {
        return vec![];
    }
    let s = disc.sqrt();
    vec![add(p, scale(d, -b - s)), add(p, scale(d, -b + s))]
}

/// Intersections of two circles.
pub fn circle_circle(c0: P2, r0: f64, c1: P2, r1: f64) -> Vec<P2> {
    let dv = sub(c1, c0);
    let d = len(dv);
    if d == 0.0 || d > r0 + r1 || d < (r0 - r1).abs() {
        return vec![];
    }
    let a = (r0 * r0 - r1 * r1 + d * d) / (2.0 * d);
    let h = (r0 * r0 - a * a).max(0.0).sqrt();
    let m = add(c0, scale(dv, a / d));
    let o = scale(left(dv), h / d);
    vec![add(m, o), sub(m, o)]
}

/// Twice the signed area of a ring (positive counter-clockwise), shoelace formula, computed
/// relative to the first vertex.
pub fn area2(ring: &[P2]) -> f64 {
    if ring.len() < 3 {
        return 0.0;
    }
    let o = ring[0];
    let mut s = 0.0;
    for i in 0..ring.len() {
        let (a, b) = (sub(ring[i], o), sub(ring[(i + 1) % ring.len()], o));
        s += cross(a, b);
    }
    s
}

/// Perpendicular distance of `p` from the line through `a` and `b`.
pub fn dist_to_line(p: P2, a: P2, b: P2) -> f64 {
    let d = sub(b, a);
    let l = len(d);
    if l == 0.0 {
        return len(sub(p, a));
    }
    cross(d, sub(p, a)).abs() / l
}

/// FNV-1a hashing of numbers, deterministic across runs and platforms (used for rebuild data).
pub struct Fnv(pub u64);
impl Fnv {
    pub fn new() -> Fnv {
        Fnv(0xcbf29ce484222325)
    }
    pub fn bytes(&mut self, b: &[u8]) {
        for x in b {
            self.0 ^= *x as u64;
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
    pub fn f64(&mut self, v: f64) {
        self.bytes(&v.to_bits().to_le_bytes());
    }
}
impl Default for Fnv {
    fn default() -> Fnv {
        Fnv::new()
    }
}
