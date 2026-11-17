//! Horizontal alignment geometry: tangents, circular arcs and clothoid spirals.
//!
//! Everything here is in metres and radians, in plan coordinates X east / Y north. Headings are
//! measured counter-clockwise from +X in the direction of increasing station. Curvature is signed:
//! positive turns left (counter-clockwise), negative turns right.
//!
//! # The one curve behind all three element types
//!
//! Every element has a curvature that varies linearly with distance `s` along it, from
//! `curvature_start` (k0) to `curvature_end` (k1) over its length L:
//!
//! ```text
//!   k(s)     = k0 + (k1 - k0) s / L
//!   theta(s) = theta0 + k0 s + (k1 - k0) s^2 / (2 L)          heading
//!   x(s)     = x0 + integral_0^s cos(theta(t)) dt
//!   y(s)     = y0 + integral_0^s sin(theta(t)) dt
//! ```
//!
//! k0 = k1 = 0 is a straight, k0 = k1 != 0 a circular arc, anything else a clothoid (Euler spiral):
//! by definition the curve whose curvature is proportional to distance. A spiral between a tangent
//! and a curve has k0 = 0 or k1 = 0; one with both non-zero joins two curves (a compound spiral).
//!
//! Straights and arcs are evaluated in closed form. For spirals the two integrals are Fresnel
//! integrals with no closed form; they are evaluated by Gauss-Legendre quadrature (8 points per
//! panel, panels no longer than 0.5 rad of heading change). The integrand is entire and the rule is
//! exact for polynomials of degree 15, so the truncation error per panel is below 1e-16 of the
//! panel length; the result is accurate to rounding, and the tests hold it to 1e-12 L against the
//! series below and to 1e-6 m against brute-force integration of the heading.
//!
//! # The standard clothoid relations
//!
//! For a spiral of length L from a tangent (k = 0) to a curve of radius R, in the frame of its
//! start (x along the tangent, y towards the centre of the curve):
//!
//! ```text
//!   theta_s = L / (2 R)                                        total spiral angle
//!   x = L (1 - theta^2/10 + theta^4/216 - theta^6/9360 + ...)  = L sum (-1)^n theta^(2n)   / ((4n+1) (2n)!)
//!   y = L (theta/3 - theta^3/42 + theta^5/1320 - ...)          = L sum (-1)^n theta^(2n+1) / ((4n+3) (2n+1)!)
//!   p = y - R (1 - cos theta_s)                                shift of the circle off the tangent
//!   k = x - R sin theta_s                                      distance of the shifted curve's tangent point from TS
//!   Ts = (R + p) tan(Delta / 2) + k                            tangent length, equal spirals, deflection Delta
//!   Lc = R (Delta - 2 theta_s)                                 length of the circular part
//! ```
//!
//! The two series are the term-by-term integrals of the cosine and sine power series, i.e. the
//! Fresnel integrals (Abramowitz & Stegun, Handbook of Mathematical Functions, 7.3.11 and 7.3.13);
//! the remaining relations are the usual highway transition-curve formulae (e.g. Hickerson, Route
//! Location and Design, ch. 5; Meyer & Gibson, Route Surveying and Design, ch. 5). [`clothoid_xy`]
//! implements the series; it is used to cross-check the quadrature, not to evaluate alignments.

use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

use crate::profile::Profile;
use crate::stationing::{DisplayedStation, StationCandidate, StationEquationError, StationEquations};
use crate::superelevation::Superelevation;

/// Direction of a curve looking up-station.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Turn {
    /// Counter-clockwise (LandXML `rot="ccw"`); positive curvature.
    Left,
    /// Clockwise (LandXML `rot="cw"`); negative curvature.
    Right,
}

impl Turn {
    fn sign(self) -> f64 {
        match self {
            Turn::Left => 1.0,
            Turn::Right => -1.0,
        }
    }
}

/// What an element is, derived from its curvatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Line,
    Arc,
    Spiral,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AlignmentError {
    #[error("an alignment needs at least two PIs")]
    TooFewPis,
    #[error("PI {index} coincides with the PI before it")]
    CoincidentPis { index: usize },
    #[error("PI {index}: {message}")]
    BadPi { index: usize, message: String },
    #[error("the curves at PI {index} and PI {next} overlap by {overlap_m:.3} m: the tangent between them is too short")]
    CurvesOverlap { index: usize, next: usize, overlap_m: f64 },
    #[error("element {index}: {message}")]
    BadElement { index: usize, message: String },
}

/// A point on an alignment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StationPoint {
    /// Easting, m.
    pub x: f64,
    /// Northing, m.
    pub y: f64,
    /// Direction of increasing station, radians counter-clockwise from +X, in [0, 2π).
    pub heading_rad: f64,
}

impl StationPoint {
    /// The point `offset_m` square to the alignment: positive to the right looking up-station.
    pub fn offset(&self, offset_m: f64) -> (f64, f64) {
        (self.x + offset_m * self.heading_rad.sin(), self.y - offset_m * self.heading_rad.cos())
    }
}

/// One piece of an alignment: a straight, a circular arc or a clothoid spiral.
///
/// The element carries its own start point and start heading, so an alignment read from a file
/// keeps exactly the coordinates it was given; [`Alignment::joints`] reports any gaps.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AlignmentElement {
    /// Start easting, m.
    pub x: f64,
    /// Start northing, m.
    pub y: f64,
    /// Heading at the start, radians counter-clockwise from +X.
    pub heading_rad: f64,
    /// Length along the element, m.
    pub length: f64,
    /// Curvature at the start, 1/m: 0 on a tangent, +1/R turning left, −1/R turning right.
    #[serde(default)]
    pub curvature_start: f64,
    /// Curvature at the end, 1/m.
    #[serde(default)]
    pub curvature_end: f64,
}

impl AlignmentElement {
    pub fn line(x: f64, y: f64, heading_rad: f64, length: f64) -> Self {
        Self { x, y, heading_rad, length, curvature_start: 0.0, curvature_end: 0.0 }
    }

    pub fn arc(x: f64, y: f64, heading_rad: f64, radius: f64, length: f64, turn: Turn) -> Self {
        let k = turn.sign() / radius;
        Self { x, y, heading_rad, length, curvature_start: k, curvature_end: k }
    }

    /// A clothoid from `radius_start` to `radius_end`; `None` is an infinite radius (the tangent
    /// end of the spiral). Both radii turn the same way.
    pub fn spiral(x: f64, y: f64, heading_rad: f64, length: f64, radius_start: Option<f64>, radius_end: Option<f64>, turn: Turn) -> Self {
        let k = |r: Option<f64>| r.map_or(0.0, |r| turn.sign() / r);
        Self { x, y, heading_rad, length, curvature_start: k(radius_start), curvature_end: k(radius_end) }
    }

    pub fn kind(&self) -> ElementKind {
        if self.curvature_start != self.curvature_end {
            ElementKind::Spiral
        } else if self.curvature_start == 0.0 {
            ElementKind::Line
        } else {
            ElementKind::Arc
        }
    }

    /// Radius at the start, m; `None` when the curvature there is zero.
    pub fn radius_start(&self) -> Option<f64> {
        (self.curvature_start != 0.0).then(|| 1.0 / self.curvature_start.abs())
    }

    /// Radius at the end, m; `None` when the curvature there is zero.
    pub fn radius_end(&self) -> Option<f64> {
        (self.curvature_end != 0.0).then(|| 1.0 / self.curvature_end.abs())
    }

    /// The smallest radius on the element, m; `None` for a straight.
    pub fn min_radius(&self) -> Option<f64> {
        let k = self.curvature_start.abs().max(self.curvature_end.abs());
        (k > 0.0).then(|| 1.0 / k)
    }

    /// Which way the element turns overall; `None` for a straight.
    pub fn turn(&self) -> Option<Turn> {
        let k = self.curvature_start + self.curvature_end;
        if k > 0.0 {
            Some(Turn::Left)
        } else if k < 0.0 {
            Some(Turn::Right)
        } else {
            None
        }
    }

    fn curvature_rate(&self) -> f64 {
        if self.length > 0.0 {
            (self.curvature_end - self.curvature_start) / self.length
        } else {
            0.0
        }
    }

    /// Curvature at distance `s` from the element's start, 1/m.
    pub fn curvature_at(&self, s: f64) -> f64 {
        self.curvature_start + self.curvature_rate() * s
    }

    /// Heading at distance `s` from the element's start, radians (not normalised).
    pub fn heading_at(&self, s: f64) -> f64 {
        self.heading_rad + self.curvature_start * s + 0.5 * self.curvature_rate() * s * s
    }

    /// Position at distance `s` from the element's start.
    pub fn point_at(&self, s: f64) -> (f64, f64) {
        let (dx, dy) = integrate_heading(self.heading_rad, self.curvature_start, self.curvature_rate(), s);
        (self.x + dx, self.y + dy)
    }

    /// Position and (un-normalised) heading at the element's end.
    pub fn end(&self) -> (f64, f64, f64) {
        let (x, y) = self.point_at(self.length);
        (x, y, self.heading_at(self.length))
    }

    /// Distances along the element at which `(px, py)` is locally nearest to it, as
    /// `(distance, along)`: `along` is the component of the vector from the nearest point to the
    /// target along the tangent there: zero at a perpendicular foot, negative when the target lies
    /// behind the element's start, positive when it lies beyond its end.
    fn nearest(&self, px: f64, py: f64) -> Vec<(f64, f64)> {
        let len = self.length;
        let g = |s: f64| {
            let (x, y) = self.point_at(s);
            let h = self.heading_at(s);
            (px - x) * h.cos() + (py - y) * h.sin()
        };
        let mut out = Vec::new();
        let g0 = g(0.0);
        if len <= 0.0 {
            out.push((0.0, g0));
            return out;
        }
        if self.kind() == ElementKind::Line {
            out.push(if g0 < 0.0 {
                (0.0, g0)
            } else if g0 > len {
                (len, g0 - len)
            } else {
                (g0, 0.0)
            });
            return out;
        }
        // Along a curve the tangential component g changes from + to − where the distance has a
        // local minimum. Sample finely enough that the heading turns at most 0.1 rad per step,
        // then close in on each sign change.
        let turned = self.curvature_start.abs().max(self.curvature_end.abs()) * len;
        let n = ((turned / 0.1).ceil() as usize).clamp(16, 4096);
        let mut a = 0.0;
        let mut ga = g0;
        if g0 <= 0.0 {
            out.push((0.0, g0));
        }
        for i in 1..=n {
            let b = len * i as f64 / n as f64;
            let gb = g(b);
            if ga > 0.0 && gb <= 0.0 {
                out.push((self.refine(px, py, a, b, &g), 0.0));
            }
            a = b;
            ga = gb;
        }
        if ga > 0.0 {
            out.push((len, ga));
        }
        out
    }

    /// Root of g in [a, b], where g(a) ≥ 0 ≥ g(b): Newton steps, bisecting whenever one leaves
    /// the bracket. g'(s) = −1 + k(s) · (offset to the left), from T' = k N.
    ///
    /// A position cannot be told apart below the spacing of the coordinates that hold it (about
    /// 2e-9 m at a UTM northing of 10 000 km), so the tolerance on `s` is never finer than a few
    /// units of that spacing; with a finer one the Newton steps stall on rounding noise and the
    /// loop ran out with the midpoint of the bracket, which can lie metres from the root. The
    /// best `s` seen is what an unfinished loop returns.
    fn refine(&self, px: f64, py: f64, mut a: f64, mut b: f64, g: &dyn Fn(f64) -> f64) -> f64 {
        let resolution = 4.0 * f64::EPSILON * px.abs().max(py.abs()).max(self.x.abs()).max(self.y.abs()).max(1.0);
        let tolerance = (1e-13 * self.length.max(1.0)).max(resolution);
        let mut s = 0.5 * (a + b);
        let mut best = (f64::INFINITY, s);
        for _ in 0..200 {
            let gs = g(s);
            if gs == 0.0 {
                return s;
            }
            if gs.abs() < best.0 {
                best = (gs.abs(), s);
            }
            if gs > 0.0 {
                a = s;
            } else {
                b = s;
            }
            let (x, y) = self.point_at(s);
            let h = self.heading_at(s);
            let left = -(px - x) * h.sin() + (py - y) * h.cos();
            let slope = -1.0 + self.curvature_at(s) * left;
            let newton = s - gs / slope;
            if slope < 0.0 && newton >= a && newton <= b {
                if (newton - s).abs() <= tolerance {
                    return newton;
                }
                s = newton;
            } else {
                if b - a <= tolerance {
                    break;
                }
                s = 0.5 * (a + b);
            }
        }
        best.1
    }
}

/// sin(u)/u, accurate near zero.
fn sinc(u: f64) -> f64 {
    if u.abs() < 1e-4 {
        1.0 - u * u / 6.0
    } else {
        u.sin() / u
    }
}

const GL8_NODES: [f64; 4] = [0.183_434_642_495_649_8, 0.525_532_409_916_329, 0.796_666_477_413_626_7, 0.960_289_856_497_536_3];
const GL8_WEIGHTS: [f64; 4] = [0.362_683_783_378_362, 0.313_706_645_877_887_3, 0.222_381_034_453_374_5, 0.101_228_536_290_376_3];

/// ∫₀ˢ (cos θ(t), sin θ(t)) dt for θ(t) = θ₀ + k₀ t + ½ c t²: the displacement along an element
/// with start heading θ₀, start curvature k₀ and curvature rate c. See the module documentation
/// for the method and its accuracy.
pub(crate) fn integrate_heading(theta0: f64, k0: f64, rate: f64, s: f64) -> (f64, f64) {
    if rate == 0.0 || s == 0.0 {
        // Straight or circular arc: the chord has length s·sinc(k₀s/2) on heading θ₀ + k₀s/2.
        let half = 0.5 * k0 * s;
        let chord = s * sinc(half);
        return (chord * (theta0 + half).cos(), chord * (theta0 + half).sin());
    }
    let turned = (k0 * s).abs() + 0.5 * (rate * s * s).abs();
    let panels = ((turned / 0.5).ceil() as usize).max(1);
    let h = s / panels as f64;
    let (mut x, mut y) = (0.0, 0.0);
    for p in 0..panels {
        let mid = (p as f64 + 0.5) * h;
        for (node, weight) in GL8_NODES.iter().zip(GL8_WEIGHTS) {
            for t in [mid - 0.5 * h * node, mid + 0.5 * h * node] {
                let theta = theta0 + k0 * t + 0.5 * rate * t * t;
                x += weight * theta.cos();
                y += weight * theta.sin();
            }
        }
    }
    (0.5 * h * x, 0.5 * h * y)
}

/// End coordinates of a clothoid of the given length running from a tangent to a curve of the
/// given radius, in the frame of its start (x along the tangent, y towards the centre), from the
/// Fresnel power series quoted in the module documentation. Summed until the terms vanish; meant
/// for spiral angles up to a few radians (highway spirals are well under one).
pub fn clothoid_xy(length: f64, radius: f64) -> (f64, f64) {
    let theta = length / (2.0 * radius);
    let (mut x, mut y) = (0.0, 0.0);
    // term_x = (−1)ⁿ θ²ⁿ / (2n)!, term_y = (−1)ⁿ θ²ⁿ⁺¹ / (2n+1)!
    let mut term_x = 1.0;
    let mut term_y = theta;
    for n in 0..60 {
        let nf = n as f64;
        x += term_x / (4.0 * nf + 1.0);
        y += term_y / (4.0 * nf + 3.0);
        if term_x.abs() < 1e-18 && term_y.abs() < 1e-18 {
            break;
        }
        term_x *= -theta * theta / ((2.0 * nf + 1.0) * (2.0 * nf + 2.0));
        term_y *= -theta * theta / ((2.0 * nf + 2.0) * (2.0 * nf + 3.0));
    }
    (length * x, length * y)
}

/// A point of intersection of two tangents, with the curve to fit there.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pi {
    pub x: f64,
    pub y: f64,
    /// Radius of the circular curve, m. 0 leaves an angle point with no curve. Ignored at the
    /// first and last PI.
    #[serde(default)]
    pub radius: f64,
    /// Length of the spiral into the curve, m; 0 for none.
    #[serde(default)]
    pub spiral_in: f64,
    /// Length of the spiral out of the curve, m; 0 for none.
    #[serde(default)]
    pub spiral_out: f64,
}

impl Pi {
    /// A PI with no curve: the start or end of the alignment, or an angle point.
    pub fn point(x: f64, y: f64) -> Self {
        Self { x, y, radius: 0.0, spiral_in: 0.0, spiral_out: 0.0 }
    }

    /// A PI with a plain circular curve.
    pub fn curve(x: f64, y: f64, radius: f64) -> Self {
        Self { x, y, radius, spiral_in: 0.0, spiral_out: 0.0 }
    }

    /// A PI with a spiral-curve-spiral.
    pub fn spiral_curve(x: f64, y: f64, radius: f64, spiral_in: f64, spiral_out: f64) -> Self {
        Self { x, y, radius, spiral_in, spiral_out }
    }
}

/// The parts of the curve fitted at one PI (see the relations in the module documentation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PiCurve {
    /// Deflection angle Δ between the tangents, radians, always positive.
    pub deflection_rad: f64,
    /// Spiral angles θs = Ls / 2R of the spiral in and the spiral out, radians.
    pub theta_in: f64,
    pub theta_out: f64,
    /// Shifts p of the circle off each tangent, m.
    pub shift_in: f64,
    pub shift_out: f64,
    /// Distances k along each tangent from the spiral's tangent end to the shifted curve's tangent point, m.
    pub k_in: f64,
    pub k_out: f64,
    /// PI to the start of the curve (TS, or PC without a spiral), m.
    pub tangent_in: f64,
    /// PI to the end of the curve (ST, or PT without a spiral), m.
    pub tangent_out: f64,
    /// Length of the circular part, m.
    pub arc_length: f64,
}

impl PiCurve {
    /// Solves the curve for a deflection (radians, sign ignored), a radius and two spiral lengths.
    ///
    /// With unequal spirals the tangent lengths are
    /// `T_in = (R + p_in) tan(Δ/2) + k_in + (p_out − p_in) / sin Δ` and the mirror image for
    /// `T_out`; with equal spirals both reduce to `Ts = (R + p) tan(Δ/2) + k`, and with none to
    /// `T = R tan(Δ/2)`.
    pub fn solve(deflection_rad: f64, radius: f64, spiral_in: f64, spiral_out: f64) -> Result<PiCurve, String> {
        let delta = deflection_rad.abs();
        if !(radius.is_finite() && radius > 0.0) {
            return Err(format!("radius {radius} m is not a positive length"));
        }
        if !(spiral_in.is_finite() && spiral_out.is_finite() && spiral_in >= 0.0 && spiral_out >= 0.0) {
            return Err("spiral lengths must be zero or positive".to_string());
        }
        if !(delta > 0.0 && delta < PI - 1e-9) {
            return Err(format!("deflection of {:.4}° cannot take a curve", delta.to_degrees()));
        }
        let part = |ls: f64| {
            let theta = ls / (2.0 * radius);
            let (x, y) = if ls > 0.0 { integrate_heading(0.0, 0.0, 1.0 / (radius * ls), ls) } else { (0.0, 0.0) };
            (theta, y - radius * (1.0 - theta.cos()), x - radius * theta.sin())
        };
        let (theta_in, shift_in, k_in) = part(spiral_in);
        let (theta_out, shift_out, k_out) = part(spiral_out);
        let arc_angle = delta - theta_in - theta_out;
        if arc_angle < -1e-12 {
            return Err(format!(
                "the spirals turn {:.4}°, more than the deflection of {:.4}°: shorten them or reduce the radius",
                (theta_in + theta_out).to_degrees(),
                delta.to_degrees()
            ));
        }
        let half = (0.5 * delta).tan();
        Ok(PiCurve {
            deflection_rad: delta,
            theta_in,
            theta_out,
            shift_in,
            shift_out,
            k_in,
            k_out,
            tangent_in: (radius + shift_in) * half + k_in + (shift_out - shift_in) / delta.sin(),
            tangent_out: (radius + shift_out) * half + k_out + (shift_in - shift_out) / delta.sin(),
            arc_length: radius * arc_angle.max(0.0),
        })
    }

    /// Length of the whole curve, spirals included, m.
    pub fn total_length(&self, spiral_in: f64, spiral_out: f64) -> f64 {
        spiral_in + self.arc_length + spiral_out
    }
}

/// A break in position or direction between two consecutive elements.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Joint {
    /// Index of the element that starts at the joint.
    pub index: usize,
    pub station_m: f64,
    /// Distance between the end of one element and the start of the next, m.
    pub gap_m: f64,
    /// Heading of the next element minus heading at the end of the previous one, radians in (−π, π].
    pub heading_change_rad: f64,
}

/// An alignment: a chain of horizontal elements with a station at its start, and optionally a
/// vertical profile, stored superelevation and station equations.
///
/// **Internal stations** run continuously from `start_station_m` by distance along the elements.
/// Every method that takes a station takes an internal one unless it says otherwise; the profile
/// and the superelevation are stored against internal stations too. `station_equations` only
/// changes how a station is *named* ([`Alignment::displayed_station`],
/// [`Alignment::internal_station`], [`Alignment::station_label`]).
///
/// The three optional parts are left out of the JSON when absent, and default to absent when
/// reading, so alignments saved before they existed load unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alignment {
    pub name: String,
    #[serde(default)]
    pub start_station_m: f64,
    pub elements: Vec<AlignmentElement>,
    /// Vertical profile (centreline levels), against internal stations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<Profile>,
    /// Stored cross-fall, against internal stations. Data only: see [`Superelevation`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superelevation: Option<Superelevation>,
    /// Station equations; empty when displayed stations are the internal ones.
    #[serde(default, skip_serializing_if = "StationEquations::is_empty")]
    pub station_equations: StationEquations,
}

/// Wraps an angle to (−π, π].
pub fn wrap_pi(a: f64) -> f64 {
    let r = a.rem_euclid(TAU);
    if r > PI {
        r - TAU
    } else {
        r
    }
}

impl Alignment {
    /// An alignment from finished elements. Each must have a finite start, heading and curvature
    /// and a length that is not negative; continuity is not required (see [`Alignment::joints`]).
    pub fn from_elements(name: &str, start_station_m: f64, elements: Vec<AlignmentElement>) -> Result<Alignment, AlignmentError> {
        for (index, e) in elements.iter().enumerate() {
            let finite = [e.x, e.y, e.heading_rad, e.length, e.curvature_start, e.curvature_end].iter().all(|v| v.is_finite());
            if !finite {
                return Err(AlignmentError::BadElement { index, message: "has a value that is not a finite number".into() });
            }
            if e.length < 0.0 {
                return Err(AlignmentError::BadElement { index, message: format!("has a negative length ({} m)", e.length) });
            }
        }
        Ok(Alignment { name: name.to_string(), start_station_m, elements, profile: None, superelevation: None, station_equations: StationEquations::default() })
    }

    /// An alignment through a list of PIs. The first and last are its ends; each one between
    /// takes a circular curve of its `radius`, with clothoid spirals of `spiral_in` and
    /// `spiral_out` when those are not zero (tangent, spiral, curve, spiral, tangent). A PI with
    /// radius 0 is left as an angle point; a PI on a straight adds nothing.
    ///
    /// Consecutive curves may meet (a reverse or broken-back curve with no tangent between) but
    /// may not overlap.
    pub fn from_pis(name: &str, start_station_m: f64, pis: &[Pi]) -> Result<Alignment, AlignmentError> {
        if pis.len() < 2 {
            return Err(AlignmentError::TooFewPis);
        }
        for (index, p) in pis.iter().enumerate() {
            if !(p.x.is_finite() && p.y.is_finite()) {
                return Err(AlignmentError::BadPi { index, message: "coordinates are not finite numbers".into() });
            }
            if index > 0 && p.x == pis[index - 1].x && p.y == pis[index - 1].y {
                return Err(AlignmentError::CoincidentPis { index });
            }
        }
        let dir = |i: usize| (pis[i + 1].y - pis[i].y).atan2(pis[i + 1].x - pis[i].x);
        let dist = |i: usize| (pis[i + 1].x - pis[i].x).hypot(pis[i + 1].y - pis[i].y);

        let mut elements: Vec<AlignmentElement> = Vec::new();
        // Where the current tangent starts, and how much of it the previous curve has used.
        let (mut from_x, mut from_y) = (pis[0].x, pis[0].y);
        let mut used = 0.0;
        for i in 1..pis.len() {
            let heading = dir(i - 1);
            let leg = dist(i - 1);
            let last = i == pis.len() - 1;
            let p = pis[i];
            let delta = if last { 0.0 } else { wrap_pi(dir(i) - heading) };
            let curve = if last || p.radius == 0.0 || delta.abs() < 1e-12 {
                None
            } else {
                Some(PiCurve::solve(delta, p.radius, p.spiral_in, p.spiral_out).map_err(|message| AlignmentError::BadPi { index: i, message })?)
            };
            let tangent_in = curve.map_or(0.0, |c| c.tangent_in);
            let straight = leg - used - tangent_in;
            if straight < -1e-9 {
                return Err(AlignmentError::CurvesOverlap { index: i - 1, next: i, overlap_m: -straight });
            }
            if straight > 1e-9 {
                elements.push(AlignmentElement::line(from_x, from_y, heading, straight));
            }
            let Some(c) = curve else {
                (from_x, from_y, used) = (p.x, p.y, 0.0);
                continue;
            };
            let turn = if delta > 0.0 { Turn::Left } else { Turn::Right };
            let (mut x, mut y, mut h) = (p.x - c.tangent_in * heading.cos(), p.y - c.tangent_in * heading.sin(), heading);
            let mut push = |e: AlignmentElement, x: &mut f64, y: &mut f64, h: &mut f64| {
                (*x, *y, *h) = e.end();
                elements.push(e);
            };
            if p.spiral_in > 0.0 {
                push(AlignmentElement::spiral(x, y, h, p.spiral_in, None, Some(p.radius), turn), &mut x, &mut y, &mut h);
            }
            if c.arc_length > 1e-12 {
                push(AlignmentElement::arc(x, y, h, p.radius, c.arc_length, turn), &mut x, &mut y, &mut h);
            }
            if p.spiral_out > 0.0 {
                push(AlignmentElement::spiral(x, y, h, p.spiral_out, Some(p.radius), None, turn), &mut x, &mut y, &mut h);
            }
            let out = dir(i);
            (from_x, from_y, used) = (p.x + c.tangent_out * out.cos(), p.y + c.tangent_out * out.sin(), c.tangent_out);
        }
        Ok(Alignment { name: name.to_string(), start_station_m, elements, profile: None, superelevation: None, station_equations: StationEquations::default() })
    }

    /// Total length along the alignment, m.
    pub fn length(&self) -> f64 {
        self.elements.iter().map(|e| e.length).sum()
    }

    pub fn end_station_m(&self) -> f64 {
        self.start_station_m + self.length()
    }

    /// The smallest radius anywhere on the alignment, m; `None` when it is all straight.
    pub fn min_radius(&self) -> Option<f64> {
        self.elements.iter().filter_map(|e| e.min_radius()).reduce(f64::min)
    }

    /// The station at the start of each element.
    pub fn element_stations(&self) -> Vec<f64> {
        let mut s = self.start_station_m;
        self.elements
            .iter()
            .map(|e| {
                let at = s;
                s += e.length;
                at
            })
            .collect()
    }

    /// The element holding a station and the distance into it. A station on a joint belongs to
    /// the element that starts there, except at the very end. `None` off the alignment (with
    /// 1e-9 m of grace at each end).
    pub fn locate(&self, station_m: f64) -> Option<(usize, f64)> {
        const GRACE: f64 = 1e-9;
        let total = self.length();
        let d = station_m - self.start_station_m;
        if self.elements.is_empty() || d.is_nan() || d < -GRACE || d > total + GRACE {
            return None;
        }
        let d = d.clamp(0.0, total);
        let mut start = 0.0;
        for (i, e) in self.elements.iter().enumerate() {
            if d < start + e.length || i == self.elements.len() - 1 {
                return Some((i, (d - start).min(e.length)));
            }
            start += e.length;
        }
        None
    }

    /// Position and heading at a station; `None` when the station is off the alignment.
    pub fn point_at(&self, station_m: f64) -> Option<StationPoint> {
        let (i, s) = self.locate(station_m)?;
        let e = &self.elements[i];
        let (x, y) = e.point_at(s);
        Some(StationPoint { x, y, heading_rad: e.heading_at(s).rem_euclid(TAU) })
    }

    /// Curvature (1/m, positive turning left) at a station; `None` off the alignment. On a joint
    /// it is that of the element starting there.
    pub fn curvature_at(&self, station_m: f64) -> Option<f64> {
        let (i, s) = self.locate(station_m)?;
        Some(self.elements[i].curvature_at(s))
    }

    /// The displayed station of an internal station and the region it is in, through the station
    /// equations; `None` off the alignment. Without equations it is the internal station, region 0.
    /// A station exactly on an equation gets the ahead station.
    pub fn displayed_station(&self, internal_m: f64) -> Option<DisplayedStation> {
        self.locate(internal_m)?;
        Some(self.station_equations.to_displayed(internal_m))
    }

    /// The internal station of a displayed station.
    ///
    /// With `region: Some(r)` the station is looked for in region `r` only (0 before the first
    /// equation). With `None` it must occur exactly once on the alignment: a station in an overlap
    /// is refused with [`StationEquationError::Ambiguous`], which lists the candidates, and one in
    /// a gap or beyond the ends with [`StationEquationError::NotOnAlignment`].
    pub fn internal_station(&self, displayed_m: f64, region: Option<usize>) -> Result<f64, StationEquationError> {
        let (start, end) = (self.start_station_m, self.end_station_m());
        match region {
            Some(r) => self.station_equations.to_internal_in_region(displayed_m, r, start, end),
            None => self.station_equations.to_internal(displayed_m, start, end),
        }
    }

    /// Every place on the alignment that a displayed station names, in order along it.
    pub fn internal_station_candidates(&self, displayed_m: f64) -> Vec<StationCandidate> {
        self.station_equations.candidates(displayed_m, self.start_station_m, self.end_station_m())
    }

    /// The label of an internal station: its displayed station as `1+250.000`.
    pub fn station_label(&self, internal_m: f64, decimals: usize) -> String {
        self.station_equations.label(internal_m, decimals)
    }

    /// The plan position at a station and offset (positive to the right looking up-station).
    pub fn point_at_offset(&self, station_m: f64, offset_m: f64) -> Option<(f64, f64)> {
        self.point_at(station_m).map(|p| p.offset(offset_m))
    }

    /// Station and offset of a plan point: the nearest point of the alignment, and the signed
    /// distance from it, positive to the right looking up-station.
    ///
    /// `None` when the point lies behind the start or beyond the end (it has no perpendicular
    /// foot on the alignment), or the alignment is empty. Where elements meet at an angle, a
    /// point in the wedge outside the corner is nearest to the corner itself and is given the
    /// corner's station. A point as far from a curve as its centre has no unique answer.
    pub fn station_offset(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        const END_TOLERANCE: f64 = 1e-9;
        // (distance², element, distance along, tangential overshoot)
        let mut best: Option<(f64, usize, f64, f64)> = None;
        for (i, e) in self.elements.iter().enumerate() {
            for (s, along) in e.nearest(x, y) {
                let (cx, cy) = e.point_at(s);
                let d2 = (x - cx).powi(2) + (y - cy).powi(2);
                if best.is_none_or(|b| d2 < b.0) {
                    best = Some((d2, i, s, along));
                }
            }
        }
        let (d2, i, s, along) = best?;
        let last = self.elements.len() - 1;
        if (i == 0 && along < -END_TOLERANCE) || (i == last && along > END_TOLERANCE) {
            return None;
        }
        let e = &self.elements[i];
        let (cx, cy) = e.point_at(s);
        let h = e.heading_at(s);
        let right = (x - cx) * h.sin() - (y - cy) * h.cos();
        // At a corner the nearest point is not a perpendicular foot: keep the full distance.
        let offset = if along.abs() > END_TOLERANCE { d2.sqrt().copysign(right) } else { right };
        let station = self.start_station_m + self.elements[..i].iter().map(|e| e.length).sum::<f64>() + s;
        Some((station, offset))
    }

    /// The alignment as a polyline for drawing: every element joint, with curves and spirals
    /// divided into equal chords no longer than `max_chord_m`. Straights give their two ends only.
    pub fn polyline(&self, max_chord_m: f64) -> Vec<(f64, f64)> {
        let mut pts: Vec<(f64, f64)> = Vec::new();
        let mut push = |p: (f64, f64)| {
            if pts.last() != Some(&p) {
                pts.push(p);
            }
        };
        for e in &self.elements {
            let n = if e.kind() == ElementKind::Line || max_chord_m.is_nan() || max_chord_m <= 0.0 {
                1
            } else {
                ((e.length / max_chord_m).ceil() as usize).clamp(1, 1_000_000)
            };
            for i in 0..=n {
                push(e.point_at(e.length * i as f64 / n as f64));
            }
        }
        pts
    }

    /// Every joint between consecutive elements, with the gap and change of direction across it.
    /// A smooth alignment has both at rounding level.
    pub fn joints(&self) -> Vec<Joint> {
        let stations = self.element_stations();
        self.elements
            .windows(2)
            .enumerate()
            .map(|(i, pair)| {
                let (x, y, h) = pair[0].end();
                Joint {
                    index: i + 1,
                    station_m: stations[i + 1],
                    gap_m: (pair[1].x - x).hypot(pair[1].y - y),
                    heading_change_rad: wrap_pi(pair[1].heading_rad - h),
                }
            })
            .collect()
    }

    /// The joints that break by more than the given tolerances.
    pub fn discontinuities(&self, gap_tolerance_m: f64, heading_tolerance_rad: f64) -> Vec<Joint> {
        self.joints().into_iter().filter(|j| j.gap_m > gap_tolerance_m || j.heading_change_rad.abs() > heading_tolerance_rad).collect()
    }
}

/// Builds an alignment element by element, each starting where the last one ended. This is the
/// way to lay out compound curves (arcs of different radius in the same direction, with or
/// without a spiral between) and reverse curves.
#[derive(Debug, Clone)]
pub struct AlignmentBuilder {
    name: String,
    start_station_m: f64,
    x: f64,
    y: f64,
    heading_rad: f64,
    elements: Vec<AlignmentElement>,
}

impl AlignmentBuilder {
    pub fn new(name: &str, start_station_m: f64, x: f64, y: f64, heading_rad: f64) -> Self {
        Self { name: name.to_string(), start_station_m, x, y, heading_rad, elements: Vec::new() }
    }

    fn push(mut self, e: AlignmentElement) -> Self {
        (self.x, self.y, self.heading_rad) = e.end();
        self.elements.push(e);
        self
    }

    pub fn line(self, length: f64) -> Self {
        let e = AlignmentElement::line(self.x, self.y, self.heading_rad, length);
        self.push(e)
    }

    pub fn arc(self, radius: f64, length: f64, turn: Turn) -> Self {
        let e = AlignmentElement::arc(self.x, self.y, self.heading_rad, radius, length, turn);
        self.push(e)
    }

    /// A clothoid between two radii; `None` is the tangent end.
    pub fn spiral(self, length: f64, radius_start: Option<f64>, radius_end: Option<f64>, turn: Turn) -> Self {
        let e = AlignmentElement::spiral(self.x, self.y, self.heading_rad, length, radius_start, radius_end, turn);
        self.push(e)
    }

    pub fn build(self) -> Result<Alignment, AlignmentError> {
        Alignment::from_elements(&self.name, self.start_station_m, self.elements)
    }
}

/// A station in metres as kilometres + metres, e.g. `1250.0` → `1+250.000`, with the given
/// number of decimals. Negative stations read `-0+050.000`.
pub fn format_station(station_m: f64, decimals: usize) -> String {
    if !station_m.is_finite() {
        return station_m.to_string();
    }
    // Round first, so 999.9996 becomes 1+000.000 and not 0+1000.000.
    let text = format!("{:.*}", decimals, station_m.abs());
    let (whole, frac) = match text.split_once('.') {
        Some((w, f)) => (w, Some(f)),
        None => (text.as_str(), None),
    };
    let whole = format!("{whole:0>4}");
    let (km, m) = whole.split_at(whole.len() - 3);
    let negative = station_m < 0.0 && text.bytes().any(|b| (b'1'..=b'9').contains(&b));
    let mut out = format!("{}{km}+{m}", if negative { "-" } else { "" });
    if let Some(f) = frac {
        out.push('.');
        out.push_str(f);
    }
    out
}
