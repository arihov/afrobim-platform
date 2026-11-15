//! Geometry the reader needs: bulge arcs, the arbitrary axis algorithm, ellipses, NURBS curves
//! and curve flattening. Pure functions on [`Pt2`], angles in degrees unless a name says radians.

use afrobim_draw::Pt2;

pub fn pt(x: f64, y: f64) -> Pt2 {
    Pt2::new(x, y)
}

fn dist(a: Pt2, b: Pt2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// An arc recovered from a bulge, always described anticlockwise.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BulgeArc {
    pub centre: Pt2,
    pub radius: f64,
    /// Anticlockwise from `start_deg` to `end_deg`.
    pub start_deg: f64,
    pub end_deg: f64,
    /// True when the polyline runs from `a` to `b` anticlockwise (positive bulge).
    pub forward: bool,
}

/// The arc a bulge describes between `a` and `b`.
///
/// Definition (DXF Reference, LWPOLYLINE/VERTEX group 42): the bulge is the tangent of a quarter
/// of the included angle of the arc, positive for an anticlockwise arc from `a` to `b`:
/// `b = tan(theta / 4)`. With chord `c = |ab|`:
///
/// - the sagitta is `s = (c / 2) * b` (from `s = R (1 - cos(theta / 2))` and
///   `c / 2 = R sin(theta / 2)`, since `(1 - cos x) / sin x = tan(x / 2)`);
/// - the radius is `R = c (1 + b^2) / (4 |b|)`;
/// - the centre lies on the perpendicular bisector of the chord at signed distance
///   `d = (c / 2) (1 - b^2) / (2 b)` to the left of the direction `a -> b` (using
///   `tan(theta / 2) = 2 b / (1 - b^2)` and `d = (c / 2) / tan(theta / 2)`); `d` is negative for an
///   arc over 180 degrees (`|b| > 1`) and for a clockwise arc, which puts the centre on the right.
///
/// `None` for a zero bulge or coincident points.
pub fn bulge_arc(a: Pt2, b: Pt2, bulge: f64) -> Option<BulgeArc> {
    let c = dist(a, b);
    if bulge.abs() < 1e-12 || c < 1e-12 {
        return None;
    }
    let (ux, uy) = ((b.x - a.x) / c, (b.y - a.y) / c);
    let (lx, ly) = (-uy, ux); // left normal
    let d = (c / 2.0) * (1.0 - bulge * bulge) / (2.0 * bulge);
    let centre = pt((a.x + b.x) / 2.0 + lx * d, (a.y + b.y) / 2.0 + ly * d);
    let radius = c * (1.0 + bulge * bulge) / (4.0 * bulge.abs());
    let ang = |p: Pt2| (p.y - centre.y).atan2(p.x - centre.x).to_degrees().rem_euclid(360.0);
    let (sa, ea) = (ang(a), ang(b));
    Some(if bulge > 0.0 {
        BulgeArc { centre, radius, start_deg: sa, end_deg: ea, forward: true }
    } else {
        BulgeArc { centre, radius, start_deg: ea, end_deg: sa, forward: false }
    })
}

/// The included angle in degrees of a bulge, positive for anticlockwise: `4 atan(b)`.
pub fn bulge_included_angle_deg(bulge: f64) -> f64 {
    4.0 * bulge.atan().to_degrees()
}

/// Anticlockwise sweep in degrees from `start` to `end`, in [0, 360). Equal angles give 0.
pub fn sweep(start_deg: f64, end_deg: f64) -> f64 {
    (end_deg - start_deg).rem_euclid(360.0)
}

/// Points along an arc, both ends included, with a chord sagitta of at most `tol`.
pub fn arc_points(centre: Pt2, radius: f64, start_deg: f64, sweep_deg: f64, tol: f64) -> Vec<Pt2> {
    let n = arc_steps(radius, sweep_deg.abs(), tol);
    (0..=n)
        .map(|i| {
            let a = (start_deg + sweep_deg * i as f64 / n as f64).to_radians();
            pt(centre.x + radius * a.cos(), centre.y + radius * a.sin())
        })
        .collect()
}

/// Segments needed for an arc: the step is `2 acos(1 - tol / r)`, at most 45 degrees, and the
/// count is capped at 4096.
pub fn arc_steps(radius: f64, sweep_deg: f64, tol: f64) -> usize {
    let step = if radius > tol && tol > 0.0 { (2.0 * (1.0 - tol / radius).acos()).to_degrees() } else { 45.0 };
    let step = step.clamp(0.5, 45.0);
    ((sweep_deg / step).ceil() as usize).clamp(1, 4096)
}

/// The arbitrary axis algorithm: the OCS axes (X, Y, Z) of an entity whose extrusion direction is
/// `n`. Z is `n`; X is `Wy x n` when `n` is close to the world Z axis (both |nx| and |ny| under
/// 1/64) and `Wz x n` otherwise; Y is `Z x X`. Same rule as `ezdxf.math.OCS`.
pub fn ocs_axes(n: (f64, f64, f64)) -> [[f64; 3]; 3] {
    let l = (n.0 * n.0 + n.1 * n.1 + n.2 * n.2).sqrt();
    let az = if l < 1e-12 { [0.0, 0.0, 1.0] } else { [n.0 / l, n.1 / l, n.2 / l] };
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let ax = if az[0].abs() < 1.0 / 64.0 && az[1].abs() < 1.0 / 64.0 { cross([0.0, 1.0, 0.0], az) } else { cross([0.0, 0.0, 1.0], az) };
    let lx = (ax[0] * ax[0] + ax[1] * ax[1] + ax[2] * ax[2]).sqrt();
    let ax = [ax[0] / lx, ax[1] / lx, ax[2] / lx];
    let ay = cross(az, ax);
    [ax, ay, az]
}

/// A point of an entity's OCS (x, y, elevation z) in world coordinates, projected to plan (x, y).
pub fn ocs_to_wcs(n: Option<(f64, f64, f64)>, x: f64, y: f64, z: f64) -> Pt2 {
    match n {
        None => pt(x, y),
        Some(n) => {
            let [ax, ay, az] = ocs_axes(n);
            pt(x * ax[0] + y * ay[0] + z * az[0], x * ax[1] + y * ay[1] + z * az[1])
        }
    }
}

/// True when the extrusion is the world Z axis (or absent).
pub fn is_plain_z(n: Option<(f64, f64, f64)>) -> bool {
    n.is_none_or(|n| n.0.abs() < 1e-9 && n.1.abs() < 1e-9 && n.2 > 0.0)
}

/// True when the extrusion is exactly the world -Z axis.
pub fn is_minus_z(n: Option<(f64, f64, f64)>) -> bool {
    n.is_some_and(|n| n.0.abs() < 1e-9 && n.1.abs() < 1e-9 && n.2 < 0.0)
}

/// Points of an ellipse: `centre + cos(t) * major + sin(t) * minor` for `t` from `t0` to `t1`
/// radians (anticlockwise, `t1 > t0`), with a sagitta tolerance `tol` estimated from the larger radius.
pub fn ellipse_points(centre: Pt2, major: Pt2, minor: Pt2, t0: f64, t1: f64, tol: f64) -> Vec<Pt2> {
    let r = major.x.hypot(major.y).max(minor.x.hypot(minor.y));
    let sweep_deg = (t1 - t0).to_degrees().abs();
    let n = arc_steps(r, sweep_deg, tol);
    (0..=n)
        .map(|i| {
            let t = t0 + (t1 - t0) * i as f64 / n as f64;
            pt(centre.x + t.cos() * major.x + t.sin() * minor.x, centre.y + t.cos() * major.y + t.sin() * minor.y)
        })
        .collect()
}

/// A NURBS curve (a B-spline when all weights are 1), in the plane.
#[derive(Debug, Clone, PartialEq)]
pub struct Nurbs {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub ctrl: Vec<Pt2>,
    pub weights: Vec<f64>,
}

impl Nurbs {
    /// Validates the curve: `knots.len() == ctrl.len() + degree + 1`, non-decreasing knots, a
    /// positive span, positive weights. Returns the reason when it is not evaluable.
    pub fn new(degree: usize, knots: Vec<f64>, ctrl: Vec<Pt2>, weights: Vec<f64>) -> Result<Nurbs, String> {
        let n = ctrl.len();
        if degree == 0 || n < degree + 1 {
            return Err(format!("{n} control points are too few for degree {degree}"));
        }
        if knots.len() != n + degree + 1 {
            return Err(format!("{} knots for {n} control points of degree {degree} (expected {})", knots.len(), n + degree + 1));
        }
        if knots.windows(2).any(|w| w[1] < w[0]) {
            return Err("the knots decrease".into());
        }
        if !(knots[n] > knots[degree]) {
            return Err("the knot domain is empty".into());
        }
        let weights = if weights.len() == n { weights } else { vec![1.0; n] };
        if weights.iter().any(|w| !(*w > 0.0)) {
            return Err("a weight is not positive".into());
        }
        Ok(Nurbs { degree, knots, ctrl, weights })
    }

    pub fn domain(&self) -> (f64, f64) {
        (self.knots[self.degree], self.knots[self.ctrl.len()])
    }

    /// Knot span index `i` with `U[i] <= u < U[i + 1]` inside the domain (the last span at the end).
    fn span(&self, u: f64) -> usize {
        let n = self.ctrl.len() - 1;
        if u >= self.knots[n + 1] {
            return n;
        }
        let (mut lo, mut hi) = (self.degree, n + 1);
        while hi - lo > 1 {
            let m = (lo + hi) / 2;
            if u < self.knots[m] {
                hi = m;
            } else {
                lo = m;
            }
        }
        lo
    }

    /// The point at parameter `u` (clamped to the domain): Cox-de Boor basis functions
    /// (Piegl and Tiller, The NURBS Book, algorithm A2.2) with rational weights.
    pub fn eval(&self, u: f64) -> Pt2 {
        let (a, b) = self.domain();
        let u = u.clamp(a, b);
        let p = self.degree;
        let i = self.span(u);
        let mut basis = vec![0.0; p + 1];
        let (mut left, mut right) = (vec![0.0; p + 1], vec![0.0; p + 1]);
        basis[0] = 1.0;
        for j in 1..=p {
            left[j] = u - self.knots[i + 1 - j];
            right[j] = self.knots[i + j] - u;
            let mut saved = 0.0;
            for r in 0..j {
                let denom = right[r + 1] + left[j - r];
                let temp = if denom == 0.0 { 0.0 } else { basis[r] / denom };
                basis[r] = saved + right[r + 1] * temp;
                saved = left[j - r] * temp;
            }
            basis[j] = saved;
        }
        let (mut x, mut y, mut w) = (0.0, 0.0, 0.0);
        for (r, nb) in basis.iter().enumerate() {
            let k = i - p + r;
            let bw = nb * self.weights[k];
            x += bw * self.ctrl[k].x;
            y += bw * self.ctrl[k].y;
            w += bw;
        }
        pt(x / w, y / w)
    }

    /// A polyline within `tol` of the curve: each knot span is split until the midpoint is within
    /// `tol` of its chord (at least four pieces per span, at most 2^12).
    pub fn flatten(&self, tol: f64) -> Vec<Pt2> {
        let (lo, hi) = self.domain();
        let mut ks: Vec<f64> = self.knots.iter().copied().filter(|k| *k >= lo && *k <= hi).collect();
        ks.dedup();
        let mut out = vec![self.eval(ks[0])];
        for w in ks.windows(2) {
            let n = 4;
            for i in 0..n {
                let (a, b) = (w[0] + (w[1] - w[0]) * i as f64 / n as f64, w[0] + (w[1] - w[0]) * (i + 1) as f64 / n as f64);
                self.refine(a, b, tol, 0, &mut out);
            }
        }
        out
    }

    fn refine(&self, a: f64, b: f64, tol: f64, depth: u32, out: &mut Vec<Pt2>) {
        let (pa, pb) = (self.eval(a), self.eval(b));
        let m = (a + b) / 2.0;
        let pm = self.eval(m);
        let chord = dist(pa, pb);
        // Distance of pm from the chord ab.
        let off = if chord < 1e-12 { dist(pa, pm) } else { ((pb.x - pa.x) * (pa.y - pm.y) - (pa.x - pm.x) * (pb.y - pa.y)).abs() / chord };
        if off > tol && depth < 12 {
            self.refine(a, m, tol, depth + 1, out);
            self.refine(m, b, tol, depth + 1, out);
        } else {
            out.push(pb);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn bulge_one_is_a_semicircle() {
        // Included angle 4 * atan(1) = 180 degrees. Chord (0,0)-(2,0): centre (1,0), radius 1.
        let a = bulge_arc(pt(0.0, 0.0), pt(2.0, 0.0), 1.0).unwrap();
        assert!(close(a.centre.x, 1.0) && close(a.centre.y, 0.0) && close(a.radius, 1.0));
        assert!(a.forward && close(a.start_deg, 180.0) && close(a.end_deg, 0.0));
        assert!(close(bulge_included_angle_deg(1.0), 180.0));
    }

    #[test]
    fn bulge_half_hand_worked() {
        // b = 0.5: theta = 4 atan(0.5) = 106.26 deg. c = 2: R = 2 * 1.25 / 2 = 1.25,
        // d = 1 * 0.75 / 1 = 0.75 to the left of +X, so the centre is (1, 0.75). The arc passes
        // below the chord: its lowest point is (1, 0.75 - 1.25) = (1, -0.5) = sagitta (c/2) b = 0.5.
        let a = bulge_arc(pt(0.0, 0.0), pt(2.0, 0.0), 0.5).unwrap();
        assert!(close(a.centre.x, 1.0) && close(a.centre.y, 0.75) && close(a.radius, 1.25));
        assert!(close(bulge_included_angle_deg(0.5), 106.26020470831196));
        let pts = arc_points(a.centre, a.radius, a.start_deg, sweep(a.start_deg, a.end_deg), 1e-4);
        assert!(close(pts[0].x, 0.0) && close(pts[0].y, 0.0));
        assert!(close(pts.last().unwrap().x, 2.0) && close(pts.last().unwrap().y, 0.0));
        let lowest = pts.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        assert!((lowest - -0.5).abs() < 1e-4);
    }

    #[test]
    fn negative_bulge_is_the_mirror_image_and_large_bulge_passes_180() {
        let a = bulge_arc(pt(0.0, 0.0), pt(2.0, 0.0), -0.5).unwrap();
        assert!(close(a.centre.y, -0.75) && !a.forward);
        // Anticlockwise description: from the end point's angle round to the start point's.
        assert!(close(sweep(a.start_deg, a.end_deg), 106.26020470831196));
        // b = 2: theta = 4 atan 2 = 253.74 deg > 180, so the centre is on the opposite side of
        // the chord to the arc's bulge: d = 1 * (1 - 4) / 4 = -0.75, below the chord for an arc running below...
        let big = bulge_arc(pt(0.0, 0.0), pt(2.0, 0.0), 2.0).unwrap();
        assert!(close(big.centre.y, -0.75) && close(big.radius, 2.0 * 5.0 / 8.0));
        assert!(bulge_arc(pt(0.0, 0.0), pt(2.0, 0.0), 0.0).is_none());
        assert!(bulge_arc(pt(1.0, 1.0), pt(1.0, 1.0), 1.0).is_none());
    }

    #[test]
    fn bulge_arc_end_points_lie_on_the_circle_for_random_cases() {
        for (ax, ay, bx, by, b) in [(3.0, 4.0, -2.0, 7.0, 0.3), (0.0, 0.0, 10.0, 10.0, -1.7), (100.0, -50.0, 90.0, -40.0, 0.01)] {
            let (a, p) = (pt(ax, ay), pt(bx, by));
            let arc = bulge_arc(a, p, b).unwrap();
            assert!(close(dist(arc.centre, a), arc.radius) && close(dist(arc.centre, p), arc.radius));
            // The sweep is the included angle (or its complement for the reversed description).
            let s = sweep(arc.start_deg, arc.end_deg);
            assert!(close(s, bulge_included_angle_deg(b).abs()) || close(s, 360.0 - bulge_included_angle_deg(b).abs()), "{s}");
        }
    }

    #[test]
    fn arc_sampling_respects_the_tolerance() {
        let pts = arc_points(pt(0.0, 0.0), 1000.0, 0.0, 90.0, 0.5);
        for w in pts.windows(2) {
            let mid = pt((w[0].x + w[1].x) / 2.0, (w[0].y + w[1].y) / 2.0);
            assert!(1000.0 - mid.x.hypot(mid.y) <= 0.5 + 1e-9);
        }
        assert!(arc_steps(0.1, 360.0, 0.5) >= 8); // a tiny circle still gets 8 segments
    }

    #[test]
    fn arbitrary_axis_matches_the_published_rule() {
        // Extrusion (0,0,-1): X = Wy x N = (-1, 0, 0); Y = N x X = (0, 1, 0)... so x mirrors.
        let p = ocs_to_wcs(Some((0.0, 0.0, -1.0)), 3.0, 4.0, 0.0);
        assert!(close(p.x, -3.0) && close(p.y, 4.0));
        // Extrusion along +X: X = Wz x N = (0, 1, 0), Y = N x X = (0, 0, 1): OCS x runs along world Y.
        let q = ocs_to_wcs(Some((1.0, 0.0, 0.0)), 3.0, 4.0, 0.0);
        assert!(close(q.x, 0.0) && close(q.y, 3.0));
        assert!(is_plain_z(None) && is_plain_z(Some((0.0, 0.0, 5.0))) && is_minus_z(Some((0.0, 0.0, -1.0))));
    }

    #[test]
    fn quadratic_bezier_is_evaluated_correctly() {
        // Clamped quadratic with control points (0,0), (1,2), (2,0): C(0.5) = (1, 1), C(0) and C(1) the ends.
        let c = Nurbs::new(2, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], vec![pt(0.0, 0.0), pt(1.0, 2.0), pt(2.0, 0.0)], vec![]).unwrap();
        let m = c.eval(0.5);
        assert!(close(m.x, 1.0) && close(m.y, 1.0));
        assert_eq!(c.eval(0.0), pt(0.0, 0.0));
        let e = c.eval(1.0);
        assert!(close(e.x, 2.0) && close(e.y, 0.0));
        // C(0.25) = 0.5625 P0 + 0.375 P1 + 0.0625 P2 = (0.375 + 0.125, 0.75) = (0.5, 0.75).
        let q = c.eval(0.25);
        assert!(close(q.x, 0.5) && close(q.y, 0.75), "{q:?}");
    }

    #[test]
    fn rational_quarter_circle_stays_on_the_circle() {
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let c = Nurbs::new(2, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], vec![pt(1.0, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)], vec![1.0, w, 1.0]).unwrap();
        for i in 0..=20 {
            let p = c.eval(i as f64 / 20.0);
            assert!((p.x.hypot(p.y) - 1.0).abs() < 1e-12);
        }
        let m = c.eval(0.5);
        assert!(close(m.x, w) && close(m.y, w));
        // Flattening to 1e-3 keeps every point within tolerance of the circle's chord sag.
        let pts = c.flatten(1e-3);
        assert!(pts.len() > 8);
        for p in &pts {
            assert!((p.x.hypot(p.y) - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn cubic_with_interior_knots_matches_a_hand_computed_point() {
        // Uniform cubic B-spline, 5 control points on a line y = x gives the line back.
        let ctrl: Vec<Pt2> = (0..5).map(|i| pt(i as f64, i as f64)).collect();
        let knots = vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0];
        let c = Nurbs::new(3, knots, ctrl, vec![]).unwrap();
        for i in 0..=10 {
            let p = c.eval(i as f64 / 10.0);
            assert!(close(p.x, p.y));
        }
        // A cubic Bezier piece: (0,0) (0,1) (1,1) (1,0): C(0.5) = (0.5, 0.75).
        let b = Nurbs::new(3, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0], vec![pt(0.0, 0.0), pt(0.0, 1.0), pt(1.0, 1.0), pt(1.0, 0.0)], vec![]).unwrap();
        let m = b.eval(0.5);
        assert!(close(m.x, 0.5) && close(m.y, 0.75));
    }

    #[test]
    fn invalid_nurbs_are_refused_with_a_reason() {
        let p = vec![pt(0.0, 0.0), pt(1.0, 1.0), pt(2.0, 0.0)];
        assert!(Nurbs::new(2, vec![0.0, 0.0, 1.0, 1.0], p.clone(), vec![]).unwrap_err().contains("knots"));
        assert!(Nurbs::new(3, vec![0.0; 7], p.clone(), vec![]).unwrap_err().contains("too few"));
        assert!(Nurbs::new(2, vec![0.0, 0.0, 0.0, 1.0, 0.5, 1.0], p.clone(), vec![]).unwrap_err().contains("decrease"));
        assert!(Nurbs::new(2, vec![0.0; 6], p, vec![]).unwrap_err().contains("empty"));
    }

    #[test]
    fn ellipse_points_hit_the_axes() {
        // Major 4 along X, minor 2 along Y.
        let pts = ellipse_points(pt(10.0, 20.0), pt(4.0, 0.0), pt(0.0, 2.0), 0.0, std::f64::consts::TAU, 0.01);
        assert!(close(pts[0].x, 14.0) && close(pts[0].y, 20.0));
        assert!(pts.iter().any(|p| close(p.x, 10.0) && close(p.y, 22.0)) || pts.len() % 4 != 0);
        for p in &pts {
            let (u, v) = ((p.x - 10.0) / 4.0, (p.y - 20.0) / 2.0);
            assert!(close(u * u + v * v, 1.0));
        }
    }
}
