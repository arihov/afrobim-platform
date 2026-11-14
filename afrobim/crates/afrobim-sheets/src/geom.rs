//! Plane geometry for composing sheets: affine maps, rectangles and clipping.
//!
//! Everything here is in one consistent unit chosen by the caller (paper millimetres in practice).

use afrobim_draw::Pt2;
use serde::{Deserialize, Serialize};

/// An axis-aligned rectangle, `x0 <= x1` and `y0 <= y1`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    pub const fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect { x0, y0, x1, y1 }
    }
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }
    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }
    pub fn centre(&self) -> Pt2 {
        Pt2::new((self.x0 + self.x1) / 2.0, (self.y0 + self.y1) / 2.0)
    }
    pub fn contains(&self, p: Pt2) -> bool {
        p.x >= self.x0 && p.x <= self.x1 && p.y >= self.y0 && p.y <= self.y1
    }
    /// The rectangle moved in by `d` on every side.
    pub fn inset(&self, d: f64) -> Rect {
        Rect::new(self.x0 + d, self.y0 + d, self.x1 - d, self.y1 - d)
    }
    pub fn corners(&self) -> [Pt2; 4] {
        [Pt2::new(self.x0, self.y0), Pt2::new(self.x1, self.y0), Pt2::new(self.x1, self.y1), Pt2::new(self.x0, self.y1)]
    }
    /// The part of this rectangle given by fractions of its width and height (0 to 1 from the
    /// lower left), for placing a view in a share of the drawing area.
    pub fn fraction(&self, f: &Fraction) -> Rect {
        Rect::new(self.x0 + f.x0 * self.width(), self.y0 + f.y0 * self.height(), self.x0 + f.x1 * self.width(), self.y0 + f.y1 * self.height())
    }
}

/// A sub-area as fractions of a parent rectangle, from its lower left corner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Fraction {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Fraction {
    /// All of the parent.
    pub const FULL: Fraction = Fraction { x0: 0.0, y0: 0.0, x1: 1.0, y1: 1.0 };
}

impl Default for Fraction {
    fn default() -> Self {
        Fraction::FULL
    }
}

/// A 2D affine map: `x' = a x + c y + e`, `y' = b x + d y + f`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aff {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Aff {
    pub const IDENTITY: Aff = Aff { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn translate(x: f64, y: f64) -> Aff {
        Aff { e: x, f: y, ..Aff::IDENTITY }
    }

    /// Uniform scale `k` after a counter-clockwise rotation by `deg`.
    pub fn scale_rotate(k: f64, deg: f64) -> Aff {
        let (s, c) = deg.to_radians().sin_cos();
        Aff { a: k * c, b: k * s, c: -k * s, d: k * c, e: 0.0, f: 0.0 }
    }

    pub fn apply(&self, p: Pt2) -> Pt2 {
        Pt2::new(self.a * p.x + self.c * p.y + self.e, self.b * p.x + self.d * p.y + self.f)
    }

    /// The map that applies `self` first and then `next`.
    pub fn then(&self, next: &Aff) -> Aff {
        Aff {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            e: next.a * self.e + next.c * self.f + next.e,
            f: next.b * self.e + next.d * self.f + next.f,
        }
    }

    /// The length scale of the map (exact for a similarity).
    pub fn scale(&self) -> f64 {
        (self.a * self.a + self.b * self.b).sqrt()
    }

    /// The angle in degrees the map turns the +X direction through.
    pub fn rotation_deg(&self) -> f64 {
        self.b.atan2(self.a).to_degrees()
    }
}

/// The part of the segment `a`-`b` inside `r` (Liang-Barsky), or `None` when it misses.
pub fn clip_segment(a: Pt2, b: Pt2, r: &Rect) -> Option<(Pt2, Pt2)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-dx, a.x - r.x0), (dx, r.x1 - a.x), (-dy, a.y - r.y0), (dy, r.y1 - a.y)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                if t > t1 {
                    return None;
                }
                t0 = t0.max(t);
            } else {
                if t < t0 {
                    return None;
                }
                t1 = t1.min(t);
            }
        }
    }
    Some((Pt2::new(a.x + t0 * dx, a.y + t0 * dy), Pt2::new(a.x + t1 * dx, a.y + t1 * dy)))
}

/// A polyline cut to `r`: the pieces that remain, each at least two points. A closed outline is
/// passed with its first point repeated at the end.
pub fn clip_polyline(points: &[Pt2], r: &Rect) -> Vec<Vec<Pt2>> {
    let mut out: Vec<Vec<Pt2>> = Vec::new();
    let mut current: Vec<Pt2> = Vec::new();
    for w in points.windows(2) {
        match clip_segment(w[0], w[1], r) {
            Some((p, q)) => {
                let continues = current.last().is_some_and(|l| l.distance_to(p) < 1e-9);
                if !continues {
                    if current.len() >= 2 {
                        out.push(std::mem::take(&mut current));
                    } else {
                        current.clear();
                    }
                    current.push(p);
                }
                if q.distance_to(p) > 1e-12 || current.len() < 2 {
                    current.push(q);
                }
            }
            None => {
                if current.len() >= 2 {
                    out.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            }
        }
    }
    if current.len() >= 2 {
        out.push(current);
    }
    out
}

/// A closed polygon cut to `r` (Sutherland-Hodgman). Empty when nothing remains.
pub fn clip_polygon(points: &[Pt2], r: &Rect) -> Vec<Pt2> {
    type Inside = fn(Pt2, &Rect) -> bool;
    type Cross = fn(Pt2, Pt2, &Rect) -> Pt2;
    let edges: [(Inside, Cross); 4] = [
        (|p, r| p.x >= r.x0, |a, b, r| Pt2::new(r.x0, a.y + (b.y - a.y) * (r.x0 - a.x) / (b.x - a.x))),
        (|p, r| p.x <= r.x1, |a, b, r| Pt2::new(r.x1, a.y + (b.y - a.y) * (r.x1 - a.x) / (b.x - a.x))),
        (|p, r| p.y >= r.y0, |a, b, r| Pt2::new(a.x + (b.x - a.x) * (r.y0 - a.y) / (b.y - a.y), r.y0)),
        (|p, r| p.y <= r.y1, |a, b, r| Pt2::new(a.x + (b.x - a.x) * (r.y1 - a.y) / (b.y - a.y), r.y1)),
    ];
    let mut poly: Vec<Pt2> = points.to_vec();
    for (inside, cross) in edges {
        if poly.is_empty() {
            break;
        }
        let input = std::mem::take(&mut poly);
        let mut prev = input[input.len() - 1];
        for &cur in &input {
            match (inside(cur, r), inside(prev, r)) {
                (true, true) => poly.push(cur),
                (true, false) => {
                    poly.push(cross(prev, cur, r));
                    poly.push(cur);
                }
                (false, true) => poly.push(cross(prev, cur, r)),
                (false, false) => {}
            }
            prev = cur;
        }
    }
    poly
}

/// An arc as chords that stay within `max_sag` of the true curve. Angles in degrees anticlockwise.
pub fn arc_points(centre: Pt2, radius: f64, start_deg: f64, end_deg: f64, max_sag: f64) -> Vec<Pt2> {
    let mut sweep = end_deg - start_deg;
    while sweep <= 0.0 {
        sweep += 360.0;
    }
    // Sagitta of a chord spanning angle t on radius R is R (1 - cos(t/2)).
    let step = if radius > max_sag { 2.0 * (1.0 - max_sag / radius).acos().to_degrees() } else { 90.0 };
    let n = ((sweep / step.clamp(1.0, 90.0)).ceil() as usize).max(1);
    (0..=n)
        .map(|i| {
            let a = (start_deg + sweep * i as f64 / n as f64).to_radians();
            Pt2::new(centre.x + radius * a.cos(), centre.y + radius * a.sin())
        })
        .collect()
}

/// A straight segment cut into dashes of `dash` with gaps of `gap`, starting with a dash.
pub fn dash_segment(a: Pt2, b: Pt2, dash: f64, gap: f64) -> Vec<(Pt2, Pt2)> {
    let len = a.distance_to(b);
    if len <= 0.0 || dash <= 0.0 {
        return Vec::new();
    }
    let (ux, uy) = ((b.x - a.x) / len, (b.y - a.y) / len);
    let at = |t: f64| Pt2::new(a.x + ux * t, a.y + uy * t);
    let mut out = Vec::new();
    let mut t = 0.0;
    while t < len {
        out.push((at(t), at((t + dash).min(len))));
        t += dash + gap.max(0.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Rect = Rect::new(0.0, 0.0, 10.0, 10.0);

    #[test]
    fn a_segment_is_cut_where_it_crosses_the_edge() {
        // From (-5, 5) to (5, 5) the part with x >= 0 is (0,5)-(5,5): the cut is at t = 0.5.
        let (p, q) = clip_segment(Pt2::new(-5.0, 5.0), Pt2::new(5.0, 5.0), &R).unwrap();
        assert_eq!((p, q), (Pt2::new(0.0, 5.0), Pt2::new(5.0, 5.0)));
        // A diagonal (-5,-5)-(15,15) enters at (0,0) and leaves at (10,10).
        let (p, q) = clip_segment(Pt2::new(-5.0, -5.0), Pt2::new(15.0, 15.0), &R).unwrap();
        assert!(p.distance_to(Pt2::new(0.0, 0.0)) < 1e-12 && q.distance_to(Pt2::new(10.0, 10.0)) < 1e-12);
        assert!(clip_segment(Pt2::new(-5.0, 11.0), Pt2::new(15.0, 11.0), &R).is_none());
    }

    #[test]
    fn a_polyline_that_leaves_and_returns_is_two_pieces() {
        let pts = [Pt2::new(1.0, 5.0), Pt2::new(5.0, 5.0), Pt2::new(5.0, 15.0), Pt2::new(7.0, 15.0), Pt2::new(7.0, 5.0)];
        let pieces = clip_polyline(&pts, &R);
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0].last().copied(), Some(Pt2::new(5.0, 10.0)));
        assert_eq!(pieces[1].first().copied(), Some(Pt2::new(7.0, 10.0)));
    }

    #[test]
    fn a_polygon_is_cut_to_the_rectangle_with_the_right_area() {
        // The square (5,5)-(15,15) overlaps R in (5,5)-(10,10): area 25.
        let sq = [Pt2::new(5.0, 5.0), Pt2::new(15.0, 5.0), Pt2::new(15.0, 15.0), Pt2::new(5.0, 15.0)];
        let c = clip_polygon(&sq, &R);
        let area: f64 = (0..c.len()).map(|i| c[i].x * c[(i + 1) % c.len()].y - c[(i + 1) % c.len()].x * c[i].y).sum::<f64>() / 2.0;
        assert!((area - 25.0).abs() < 1e-9);
        assert!(clip_polygon(&[Pt2::new(20.0, 20.0), Pt2::new(30.0, 20.0), Pt2::new(30.0, 30.0)], &R).is_empty());
    }

    #[test]
    fn affine_maps_compose_in_order() {
        // Rotate (1,0) by 90 degrees to (0,1), scale by 2 to (0,2), then move by (10, 20).
        let m = Aff::scale_rotate(2.0, 90.0).then(&Aff::translate(10.0, 20.0));
        let p = m.apply(Pt2::new(1.0, 0.0));
        assert!((p.x - 10.0).abs() < 1e-12 && (p.y - 22.0).abs() < 1e-12);
        assert!((m.scale() - 2.0).abs() < 1e-12 && (m.rotation_deg() - 90.0).abs() < 1e-9);
    }

    #[test]
    fn arcs_stay_within_the_sag() {
        let pts = arc_points(Pt2::new(0.0, 0.0), 100.0, 0.0, 90.0, 0.05);
        assert!(pts.len() > 8);
        for w in pts.windows(2) {
            let mid = Pt2::new((w[0].x + w[1].x) / 2.0, (w[0].y + w[1].y) / 2.0);
            assert!(100.0 - mid.distance_to(Pt2::new(0.0, 0.0)) <= 0.05 + 1e-9);
        }
    }

    #[test]
    fn dashes_start_with_a_dash_and_end_on_the_line() {
        let d = dash_segment(Pt2::new(0.0, 0.0), Pt2::new(10.0, 0.0), 3.0, 1.0);
        assert_eq!(d.len(), 3); // 0-3, 4-7, 8-10
        assert_eq!(d[2].1, Pt2::new(10.0, 0.0));
    }
}
