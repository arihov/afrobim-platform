// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Small plan-geometry helpers shared by the modules. Not public API.

use robust::{orient2d, Coord};

/// Exact orientation of `c` against the directed line `a -> b`: positive when `a, b, c` turn
/// counter-clockwise, negative when clockwise, zero when collinear. Shewchuk's adaptive
/// predicate (J. R. Shewchuk, "Adaptive Precision Floating-Point Arithmetic and Fast Robust
/// Geometric Predicates", Discrete & Computational Geometry 18(3), 1997), so the sign is right
/// even for the nearly collinear triples that true eastings and northings produce.
pub(crate) fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    orient2d(Coord { x: a[0], y: a[1] }, Coord { x: b[0], y: b[1] }, Coord { x: c[0], y: c[1] })
}

/// Twice the signed plan area of a triangle with the same expression the corridor `Tin` uses to
/// reject degenerate triangles, so a triangle passing this test is accepted by `Tin::new`.
pub(crate) fn area2(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Even-odd point in polygon test (ray along +x). Points exactly on an edge may fall either way;
/// callers only test points that are not on an edge (triangle centroids of a triangulation that
/// honours the polygon's edges).
pub(crate) fn point_in_polygon(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0];
            if p[0] < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Signed area of a plan polygon (positive counter-clockwise), shoelace formula.
pub(crate) fn polygon_area2(poly: &[[f64; 2]]) -> f64 {
    let n = poly.len();
    let mut s = 0.0;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        s += a[0] * b[1] - b[0] * a[1];
    }
    s
}

/// True when the open segments `ab` and `cd` cross at a single interior point of both.
pub(crate) fn proper_crossing(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let (o1, o2) = (orient(a, b, c), orient(a, b, d));
    let (o3, o4) = (orient(c, d, a), orient(c, d, b));
    ((o1 > 0.0 && o2 < 0.0) || (o1 < 0.0 && o2 > 0.0)) && ((o3 > 0.0 && o4 < 0.0) || (o3 < 0.0 && o4 > 0.0))
}

/// A vertex of a clipped polygon: plan position and the value of the clipped linear field.
pub(crate) type Poly = Vec<[f64; 2]>;

/// Clips a convex polygon to the half plane to the left of (or on) the directed line `a -> b`
/// (Sutherland and Hodgman, "Reentrant polygon clipping", Communications of the ACM 17(1),
/// 1974). `poly` must be convex; the result is convex.
pub(crate) fn clip_left(poly: &[[f64; 2]], a: [f64; 2], b: [f64; 2]) -> Poly {
    let side = |p: [f64; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    clip_by(poly, side)
}

/// Clips a convex polygon to where the linear function `f` is `>= 0` (Sutherland and Hodgman,
/// as above). `f` must be affine in the plan position.
pub(crate) fn clip_by(poly: &[[f64; 2]], f: impl Fn([f64; 2]) -> f64) -> Poly {
    let n = poly.len();
    let mut out: Poly = Vec::with_capacity(n + 2);
    if n == 0 {
        return out;
    }
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let (fp, fq) = (f(p), f(q));
        if fp >= 0.0 {
            out.push(p);
        }
        if (fp >= 0.0) != (fq >= 0.0) {
            let t = fp / (fp - fq);
            out.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
        }
    }
    out
}

/// Area of a convex (or simple) polygon, absolute value.
pub(crate) fn poly_area(poly: &[[f64; 2]]) -> f64 {
    if poly.len() < 3 {
        0.0
    } else {
        polygon_area2(poly).abs() / 2.0
    }
}

/// The plane `z = c0 + cx * x + cy * y` through a triangle, in coordinates relative to the
/// triangle's first vertex `o` (so `z = o.z + gx * (x - o.x) + gy * (y - o.y)`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Plane {
    pub o: [f64; 3],
    pub gx: f64,
    pub gy: f64,
}

impl Plane {
    /// The plane of a triangle; `None` when it has no plan area.
    pub fn of(t: [[f64; 3]; 3]) -> Option<Plane> {
        let (e1, e2) = ([t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]], [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]]);
        let det = e1[0] * e2[1] - e1[1] * e2[0];
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let gx = (e1[2] * e2[1] - e2[2] * e1[1]) / det;
        let gy = (e2[2] * e1[0] - e1[2] * e2[0]) / det;
        Some(Plane { o: t[0], gx, gy })
    }

    pub fn z(&self, p: [f64; 2]) -> f64 {
        self.o[2] + self.gx * (p[0] - self.o[0]) + self.gy * (p[1] - self.o[1])
    }
}

/// Integral of an affine function over a polygon, given by fan triangulation: each triangle's
/// integral is its area times the mean of the function at its three corners, which is exact for
/// an affine function (the integral of an affine function over a triangle is the area times the
/// value at the centroid, and the centroid value is the mean of the corner values).
pub(crate) fn integrate_affine(poly: &[[f64; 2]], f: impl Fn([f64; 2]) -> f64) -> (f64, f64) {
    let mut area = 0.0;
    let mut integral = 0.0;
    if poly.len() < 3 {
        return (0.0, 0.0);
    }
    let o = poly[0];
    let fo = f(o);
    for w in poly[1..].windows(2) {
        let (b, c) = (w[0], w[1]);
        let a = ((b[0] - o[0]) * (c[1] - o[1]) - (b[1] - o[1]) * (c[0] - o[0])).abs() / 2.0;
        area += a;
        integral += a * (fo + f(b) + f(c)) / 3.0;
    }
    (area, integral)
}
