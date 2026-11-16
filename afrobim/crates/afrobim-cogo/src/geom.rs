//! Plain 2D coordinates in metres. Easting is `e`, northing is `n`.
//!
//! Coordinates are stored exactly as given. Differences of two UTM-sized values (northing near
//! 10 000 000 m) are exact to the f64 spacing of about 2e-9 m there, so every routine here works on
//! differences from a local origin rather than on the large values.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Xy {
    pub e: f64,
    pub n: f64,
}

impl Xy {
    pub const fn new(e: f64, n: f64) -> Xy {
        Xy { e, n }
    }
    /// (de, dn) from `o` to self.
    pub fn sub(self, o: Xy) -> (f64, f64) {
        (self.e - o.e, self.n - o.n)
    }
    pub fn dist(self, o: Xy) -> f64 {
        let (a, b) = self.sub(o);
        a.hypot(b)
    }
    /// The point moved by (de, dn).
    pub fn add(self, de: f64, dn: f64) -> Xy {
        Xy { e: self.e + de, n: self.n + dn }
    }
}

/// Unit vector of a whole circle bearing (clockwise from north): (sin b, cos b) as (east, north).
pub fn dir(bearing_deg: f64) -> (f64, f64) {
    let r = bearing_deg.to_radians();
    (r.sin(), r.cos())
}

/// Whole circle bearing in [0, 360) of the vector (de, dn).
pub fn bearing_of(de: f64, dn: f64) -> f64 {
    crate::angle::normalize_deg(de.atan2(dn).to_degrees())
}

pub fn cross(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.0 * b.1 - a.1 * b.0
}
pub fn dot(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

/// Distance from p to the segment a-b and the clamped parameter t in [0,1] of the nearest point.
pub fn seg_nearest(p: Xy, a: Xy, b: Xy) -> (f64, f64) {
    let v = b.sub(a);
    let w = p.sub(a);
    let l2 = dot(v, v);
    let t = if l2 == 0.0 { 0.0 } else { (dot(w, v) / l2).clamp(0.0, 1.0) };
    let q = (v.0 * t - w.0, v.1 * t - w.1);
    (q.0.hypot(q.1), t)
}

/// Signed shoelace area of a ring (positive anticlockwise), computed from the first vertex.
pub fn ring_area(r: &[Xy]) -> f64 {
    if r.len() < 3 {
        return 0.0;
    }
    let o = r[0];
    let mut s = 0.0;
    for i in 0..r.len() {
        s += cross(r[i].sub(o), r[(i + 1) % r.len()].sub(o));
    }
    s / 2.0
}

/// Even-odd/winding point in ring test (strictly by crossing number; boundary not handled).
pub fn ring_winding(r: &[Xy], p: Xy) -> i32 {
    let mut w = 0;
    for i in 0..r.len() {
        let a = r[i];
        let b = r[(i + 1) % r.len()];
        let side = cross(b.sub(a), p.sub(a));
        if a.n <= p.n {
            if b.n > p.n && side > 0.0 {
                w += 1;
            }
        } else if b.n <= p.n && side < 0.0 {
            w -= 1;
        }
    }
    w
}
