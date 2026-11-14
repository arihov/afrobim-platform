//! Vertical profile: PVIs joined by straight grades, with a vertical curve at any interior PVI.
//!
//! Stations here are **internal** stations (the alignment's continuous stations, before any
//! station equation) in metres; elevations are in metres; a grade is a ratio (m/m, +0.02 = 2 % up
//! in the direction of increasing station).
//!
//! How each quantity is computed. At a PVI at station `s`, elevation `z`, with grade `g1` in and
//! `g2` out, `A = g2 − g1`:
//!
//! - **Grade line.** `z(x) = z_i + g_i (x − s_i)` between PVI `i` and `i + 1`,
//!   `g_i = (z_{i+1} − z_i) / (s_{i+1} − s_i)`.
//! - **Symmetric parabola**, length `L` (horizontal, half each side of the PVI). From the PVC at
//!   `s − L/2`, `z_pvc = z − g1 L/2`, with `x` measured from the PVC:
//!   `y = z_pvc + g1 x + A x² / (2L)`, grade `g1 + A x / L`.
//! - **Asymmetric parabola**, lengths `L1` in and `L2` out, `L = L1 + L2`: two parabolas that meet
//!   at the PVI station with a common grade. Asking for grade `g1` at the PVC, `g2` at the PVT,
//!   the tangent elevations at both, and one grade and one elevation where they meet gives the
//!   rates of change of grade `r1 = A L2 / (L L1)` (first) and `r2 = A L1 / (L L2)` (second):
//!   `y = z_pvc + g1 x + r1 x²/2` for `0 ≤ x ≤ L1`, then from the meeting point
//!   (`z_m = z + A L1 L2 / (2L)`, `g_m = g1 + A L2 / L`) `y = z_m + g_m x' + r2 x'²/2`.
//! - **Circular curve**, radius `R`: the arc of a circle tangent to both grade lines, in the
//!   station/elevation plane at 1:1. With `a1 = atan g1`, `a2 = atan g2`, the tangent length
//!   along each grade line is `T = R tan(|a2 − a1| / 2)`, so the curve runs from `s − T cos a1` to
//!   `s + T cos a2`. Its centre is `R` from the PVC square to the grade (above for a sag, below
//!   for a crest), and `z(x) = z_c ∓ √(R² − (x − x_c)²)`.
//!
//! What is not done: no check of any design criterion (K values, sight distance, minimum or
//! maximum grade), no curves other than the three above, and no elevation outside the first and
//! last PVI: a station outside that range is an error, never an extrapolation.

use serde::{Deserialize, Serialize};

use crate::geometry::format_station;

/// The vertical curve at a PVI.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VerticalCurve {
    /// The grades meet at an angle.
    #[default]
    None,
    /// Symmetric parabola of horizontal length `length_m`, half each side of the PVI.
    Parabola { length_m: f64 },
    /// Two parabolas of horizontal lengths `length_in_m` before and `length_out_m` after the PVI.
    AsymmetricParabola { length_in_m: f64, length_out_m: f64 },
    /// Circular arc of radius `radius_m`, tangent to both grades.
    Circular { radius_m: f64 },
}

impl VerticalCurve {
    fn is_none(&self) -> bool {
        matches!(self, VerticalCurve::None)
    }
}

/// A point of vertical intersection: internal station and elevation in metres, and the curve there.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pvi {
    pub station_m: f64,
    pub elevation_m: f64,
    #[serde(default, skip_serializing_if = "VerticalCurve::is_none")]
    pub curve: VerticalCurve,
}

impl Pvi {
    /// A PVI where the grades meet without a curve (also the first and the last point).
    pub fn point(station_m: f64, elevation_m: f64) -> Self {
        Pvi { station_m, elevation_m, curve: VerticalCurve::None }
    }

    pub fn parabola(station_m: f64, elevation_m: f64, length_m: f64) -> Self {
        Pvi { station_m, elevation_m, curve: VerticalCurve::Parabola { length_m } }
    }

    pub fn asymmetric(station_m: f64, elevation_m: f64, length_in_m: f64, length_out_m: f64) -> Self {
        Pvi { station_m, elevation_m, curve: VerticalCurve::AsymmetricParabola { length_in_m, length_out_m } }
    }

    pub fn circular(station_m: f64, elevation_m: f64, radius_m: f64) -> Self {
        Pvi { station_m, elevation_m, curve: VerticalCurve::Circular { radius_m } }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProfileError {
    #[error("a profile needs at least two PVIs")]
    TooFewPvis,
    #[error("PVI {index}: {message}")]
    BadPvi { index: usize, message: String },
    #[error("PVI {index} is not at a higher station than the PVI before it")]
    StationsNotIncreasing { index: usize },
    #[error("PVI {index} is the {which} point of the profile and cannot carry a vertical curve")]
    CurveAtEnd { index: usize, which: &'static str },
    #[error("the vertical curves at PVI {index} and PVI {next} overlap by {overlap_m:.3} m")]
    CurvesOverlap { index: usize, next: usize, overlap_m: f64 },
    #[error("station {} is outside the profile, which runs from {} to {}", format_station(*station_m, 3), format_station(*start_m, 3), format_station(*end_m, 3))]
    OutOfRange { station_m: f64, start_m: f64, end_m: f64 },
}

/// Elevation (m) and grade (m/m) of the profile at a station.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProfilePoint {
    pub elevation_m: f64,
    pub grade: f64,
}

/// Where the vertical curve at a PVI starts (PVC) and ends (PVT), as internal stations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerticalCurveExtent {
    /// Index of the PVI in [`Profile::pvis`].
    pub pvi: usize,
    pub start_station_m: f64,
    pub end_station_m: f64,
}

/// A vertical profile along an alignment. Stations are internal stations.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub name: String,
    pub pvis: Vec<Pvi>,
}

/// Curves may touch (PVT on the next PVC); this much overlap is put down to rounding.
const OVERLAP_TOLERANCE_M: f64 = 1e-6;
const GRACE_M: f64 = 1e-9;

impl Profile {
    /// A validated profile. See [`Profile::validate`].
    pub fn new(name: &str, pvis: Vec<Pvi>) -> Result<Profile, ProfileError> {
        let profile = Profile { name: name.to_string(), pvis };
        profile.validate()?;
        Ok(profile)
    }

    /// Checks that there are at least two PVIs, every number is finite, stations increase
    /// strictly, the first and last PVI carry no curve, curve lengths and radii are positive, and
    /// no curve runs into its neighbour or past the PVI before or after it. A profile read from
    /// JSON has not been through [`Profile::new`]; the evaluation methods run this check first.
    pub fn validate(&self) -> Result<(), ProfileError> {
        self.resolved().map(|_| ())
    }

    /// The first and last station the profile gives an elevation for.
    pub fn range(&self) -> Option<(f64, f64)> {
        match (self.pvis.first(), self.pvis.last()) {
            (Some(a), Some(b)) if self.pvis.len() >= 2 => Some((a.station_m, b.station_m)),
            _ => None,
        }
    }

    /// The grade (m/m) of the straight from PVI `index` to the next one.
    pub fn grade_between(&self, index: usize) -> Option<f64> {
        let (a, b) = (self.pvis.get(index)?, self.pvis.get(index + 1)?);
        Some((b.elevation_m - a.elevation_m) / (b.station_m - a.station_m))
    }

    /// The start and end station of each vertical curve, in PVI order.
    pub fn curve_extents(&self) -> Result<Vec<VerticalCurveExtent>, ProfileError> {
        Ok(self
            .resolved()?
            .iter()
            .enumerate()
            .filter(|(_, c)| !matches!(c.shape, Shape::None))
            .map(|(pvi, c)| VerticalCurveExtent { pvi, start_station_m: c.start, end_station_m: c.end })
            .collect())
    }

    /// Elevation in metres at an internal station. An error outside [`Profile::range`].
    pub fn elevation_at(&self, station_m: f64) -> Result<f64, ProfileError> {
        self.point_at(station_m).map(|p| p.elevation_m)
    }

    /// Grade (m/m) at an internal station. At a PVI without a curve the grade changes abruptly:
    /// the grade ahead is returned, except at the last PVI. An error outside [`Profile::range`].
    pub fn grade_at(&self, station_m: f64) -> Result<f64, ProfileError> {
        self.point_at(station_m).map(|p| p.grade)
    }

    /// Elevation and grade at an internal station. An error outside [`Profile::range`].
    pub fn point_at(&self, station_m: f64) -> Result<ProfilePoint, ProfileError> {
        let curves = self.resolved()?;
        let (start_m, end_m) = (self.pvis[0].station_m, self.pvis[self.pvis.len() - 1].station_m);
        if !(station_m >= start_m - GRACE_M && station_m <= end_m + GRACE_M) {
            return Err(ProfileError::OutOfRange { station_m, start_m, end_m });
        }
        let s = station_m.clamp(start_m, end_m);
        // The straight that holds s: from PVI i to PVI i + 1 (a station on a PVI belongs to the one ahead).
        let last = self.pvis.len() - 2;
        let i = (0..=last).rev().find(|&i| self.pvis[i].station_m <= s).unwrap_or(0);
        if s < curves[i].end {
            return Ok(curves[i].at(s));
        }
        if s > curves[i + 1].start {
            return Ok(curves[i + 1].at(s));
        }
        let grade = (self.pvis[i + 1].elevation_m - self.pvis[i].elevation_m) / (self.pvis[i + 1].station_m - self.pvis[i].station_m);
        Ok(ProfilePoint { elevation_m: self.pvis[i].elevation_m + grade * (s - self.pvis[i].station_m), grade })
    }

    /// Validates and works out each PVI's curve. One entry per PVI.
    fn resolved(&self) -> Result<Vec<Resolved>, ProfileError> {
        let n = self.pvis.len();
        if n < 2 {
            return Err(ProfileError::TooFewPvis);
        }
        for (index, p) in self.pvis.iter().enumerate() {
            if !(p.station_m.is_finite() && p.elevation_m.is_finite()) {
                return Err(ProfileError::BadPvi { index, message: "station or elevation is not a finite number".into() });
            }
            if index > 0 && p.station_m <= self.pvis[index - 1].station_m {
                return Err(ProfileError::StationsNotIncreasing { index });
            }
        }
        let grade = |i: usize| (self.pvis[i + 1].elevation_m - self.pvis[i].elevation_m) / (self.pvis[i + 1].station_m - self.pvis[i].station_m);
        let positive = |index: usize, what: &str, v: f64| {
            if v.is_finite() && v > 0.0 {
                Ok(())
            } else {
                Err(ProfileError::BadPvi { index, message: format!("{what} must be a positive number, not {v}") })
            }
        };
        let mut out = Vec::with_capacity(n);
        for (index, p) in self.pvis.iter().enumerate() {
            let (s, z) = (p.station_m, p.elevation_m);
            if index == 0 || index == n - 1 {
                if !p.curve.is_none() {
                    return Err(ProfileError::CurveAtEnd { index, which: if index == 0 { "first" } else { "last" } });
                }
                out.push(Resolved { start: s, end: s, shape: Shape::None });
                continue;
            }
            let (g1, g2) = (grade(index - 1), grade(index));
            let a = g2 - g1;
            out.push(match p.curve {
                VerticalCurve::None => Resolved { start: s, end: s, shape: Shape::None },
                VerticalCurve::Parabola { length_m } => {
                    positive(index, "the curve length", length_m)?;
                    let half = length_m / 2.0;
                    let first = Piece { x0: s - half, z0: z - g1 * half, g0: g1, rate: a / length_m };
                    Resolved { start: s - half, end: s + half, shape: Shape::Parabolas { first, second: first, meet: s + half } }
                }
                VerticalCurve::AsymmetricParabola { length_in_m: l1, length_out_m: l2 } => {
                    positive(index, "the curve length in", l1)?;
                    positive(index, "the curve length out", l2)?;
                    let l = l1 + l2;
                    let first = Piece { x0: s - l1, z0: z - g1 * l1, g0: g1, rate: a * l2 / (l * l1) };
                    let second = Piece { x0: s, z0: z + a * l1 * l2 / (2.0 * l), g0: g1 + a * l2 / l, rate: a * l1 / (l * l2) };
                    Resolved { start: s - l1, end: s + l2, shape: Shape::Parabolas { first, second, meet: s } }
                }
                VerticalCurve::Circular { radius_m } => {
                    positive(index, "the curve radius", radius_m)?;
                    let (a1, a2) = (g1.atan(), g2.atan());
                    let turn = a2 - a1;
                    if turn == 0.0 {
                        // The grades are in line: there is nothing to round.
                        Resolved { start: s, end: s, shape: Shape::None }
                    } else {
                        let t = radius_m * (turn.abs() / 2.0).tan();
                        let sign = turn.signum();
                        let (x_pvc, z_pvc) = (s - t * a1.cos(), z - t * a1.sin());
                        Resolved {
                            start: x_pvc,
                            end: s + t * a2.cos(),
                            shape: Shape::Circle { xc: x_pvc - sign * radius_m * a1.sin(), zc: z_pvc + sign * radius_m * a1.cos(), radius: radius_m, sign },
                        }
                    }
                }
            });
        }
        for i in 0..n - 1 {
            let overlap_m = out[i].end - out[i + 1].start;
            if overlap_m > OVERLAP_TOLERANCE_M {
                return Err(ProfileError::CurvesOverlap { index: i, next: i + 1, overlap_m });
            }
        }
        Ok(out)
    }
}

/// A parabola: elevation `z0` and grade `g0` at station `x0`, grade changing at `rate` per metre.
#[derive(Debug, Clone, Copy)]
struct Piece {
    x0: f64,
    z0: f64,
    g0: f64,
    rate: f64,
}

impl Piece {
    fn at(&self, s: f64) -> ProfilePoint {
        let x = s - self.x0;
        ProfilePoint { elevation_m: self.z0 + self.g0 * x + self.rate * x * x / 2.0, grade: self.g0 + self.rate * x }
    }
}

#[derive(Debug, Clone, Copy)]
enum Shape {
    None,
    /// `first` up to station `meet`, `second` after it.
    Parabolas { first: Piece, second: Piece, meet: f64 },
    /// Centre (`xc`, `zc`); `sign` +1 for a sag (centre above), −1 for a crest.
    Circle { xc: f64, zc: f64, radius: f64, sign: f64 },
}

/// The curve at one PVI: where it starts and ends, and its shape. Without a curve both are the PVI station.
#[derive(Debug, Clone, Copy)]
struct Resolved {
    start: f64,
    end: f64,
    shape: Shape,
}

impl Resolved {
    fn at(&self, s: f64) -> ProfilePoint {
        match self.shape {
            Shape::Parabolas { first, second, meet } => {
                if s <= meet {
                    first.at(s)
                } else {
                    second.at(s)
                }
            }
            Shape::Circle { xc, zc, radius, sign } => {
                let dx = s - xc;
                let h = (radius * radius - dx * dx).max(0.0).sqrt();
                ProfilePoint { elevation_m: zc - sign * h, grade: sign * dx / h }
            }
            // A PVI without a curve has no extent, so no station falls inside it; the PVI itself is the answer.
            Shape::None => ProfilePoint { elevation_m: f64::NAN, grade: f64::NAN },
        }
    }
}
