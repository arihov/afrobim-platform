//! Horizontal design: an alignment defined by PIs, each with a radius and optional spirals.
//!
//! Conventions are those of `afrobim-alignment`: metres, radians, X east, Y north, headings
//! counter-clockwise from +X, a left turn positive. Stations are internal stations.
//!
//! # Formulae and where they come from
//!
//! Let the tangents meet at a PI with deflection angle `Δ` (the change of heading, 0 < Δ < π),
//! circular radius `R`, and spiral lengths `Ls1` (in) and `Ls2` (out), either of which may be 0.
//!
//! **Plain circular curve (no spirals).** The radii `O-PC` and `O-PT` are square to the tangents,
//! so the angle at the centre `O` between them equals the deflection, `Δ`. In the right triangle
//! `O-PC-PI` the angle at `O` is `Δ/2` (the line `O-PI` bisects it), so
//!
//! ```text
//!   T  = R tan(Δ/2)             tangent length, PI to PC and PI to PT
//!   Lc = R Δ                    arc length (arc angle times radius)
//!   |O PI| = R / cos(Δ/2)       so  E = R (sec(Δ/2) - 1)   external distance, PI to the arc
//!   M  = R (1 - cos(Δ/2))       mid-ordinate, arc midpoint to the chord
//! ```
//!
//! **Clothoid (Euler spiral).** Curvature grows linearly with distance, `k(s) = s / (R Ls)`, so
//! the heading is `θ(s) = ∫k ds = s² / (2 R Ls)` and the spiral turns through
//! `θs = Ls / (2R)` by its end, where `k = 1/R`. Position follows by integrating the heading:
//! `x = ∫cos θ ds`, `y = ∫sin θ ds`. Expanding `cos θ` and `sin θ` in power series of
//! `θ = s²/(2 R Ls)` and integrating term by term gives, with `θ` then meaning `θs`,
//!
//! ```text
//!   x = Ls Σ (-1)^n θs^(2n)   / ((4n+1) (2n)!)  = Ls (1 - θs²/10 + θs⁴/216 - ...)
//!   y = Ls Σ (-1)^n θs^(2n+1) / ((4n+3) (2n+1)!) = Ls (θs/3 - θs³/42 + ...)
//! ```
//!
//! (the Fresnel integrals). `afrobim_alignment::clothoid_xy` sums this series; the alignment
//! itself is evaluated by Gauss-Legendre quadrature of the heading integral, exact to rounding
//! (see `afrobim-alignment`); `tests/clothoid_reference.rs` compares both with an arbitrary
//! precision Fresnel evaluation (mpmath) and states the agreement reached.
//!
//! **Spiral shift and extension.** With the spiral in, the circle is moved inward off the
//! tangent. The circle's centre in the frame of the TS (x along the tangent, y towards the
//! centre) is `(x - R sin θs, y + R cos θs)`. Its distance from the tangent is `R + p`, so
//!
//! ```text
//!   p = y - R (1 - cos θs)      shift of the circle off the tangent
//!   k = x - R sin θs            distance along the tangent from the TS to the foot of the
//!                               perpendicular from the circle centre (the "extension")
//! ```
//!
//! **Tangent lengths with spirals.** Drop perpendiculars from the circle centre to the two
//! tangents; they are `a = R + p1` and `b = R + p2` long and meet the tangents at the feet
//! `F1` (at `k1` from TS) and `F2` (at `k2` from ST). In the quadrilateral formed by the centre,
//! the two feet and the PI the angle at the PI is `π - Δ`, so the feet lie at
//! `PI-F1 = a tan(Δ/2) + (b - a)/sin Δ` and `PI-F2 = b tan(Δ/2) + (a - b)/sin Δ`.
//! Adding the extensions: `T1 = (R+p1) tan(Δ/2) + k1 + (p2 - p1)/sin Δ` and the mirror image for
//! `T2`. With equal spirals this is `Ts = (R+p) tan(Δ/2) + k`; with none it is `R tan(Δ/2)`.
//! `afrobim_alignment::PiCurve::solve` evaluates these; the tests here verify them independently
//! by laying the curve out from the TS and checking that it arrives on the second tangent at
//! `T2` from the PI, heading along it.
//!
//! The circular part has arc angle `Δ - θ1 - θ2`, so `Lc = R (Δ - θ1 - θ2)`. It is impossible
//! when that is negative, which is when `Ls1 + Ls2 > 2 R Δ`.
//!
//! **External distance with spirals.** The distance from the PI to the nearest point of the
//! circle, `|O PI| - R`, with `O` the circle centre found from the laid-out curve. For the
//! plain curve it equals `R (sec(Δ/2) - 1)`. The mid-ordinate reported is that of the circular
//! part alone, `R (1 - cos(Δc/2))` with `Δc = Δ - θ1 - θ2`.
//!
//! # Stations
//!
//! `TS` (or `PC`) is where the curve starts: the end of the previous curve plus the straight
//! between them. `SC = TS + Ls1`, `CS = SC + Lc`, `ST = CS + Ls2`. The PI station reported is
//! `TS + T1`, the station the PI would be given when stationed along the tangents.

use afrobim_alignment::{clothoid_xy, wrap_pi, Alignment, AlignmentElement, Pi, PiCurve, Turn};
use serde::{Deserialize, Serialize};

use crate::error::{DesignError, Problem};

/// The inputs of a horizontal design: PIs in order, the first and last being the ends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HorizontalDesign {
    pub name: String,
    /// Station at the first PI, m.
    #[serde(default)]
    pub start_station_m: f64,
    /// The PIs. The `radius`, `spiral_in` and `spiral_out` of the first and last are ignored.
    pub pis: Vec<Pi>,
}

/// Everything computed for the curve at one PI.
#[derive(Debug, Clone, PartialEq)]
pub struct CurveData {
    /// Index into [`HorizontalDesign::pis`].
    pub pi_index: usize,
    pub turn: Turn,
    pub radius_m: f64,
    pub spiral_in_m: f64,
    pub spiral_out_m: f64,
    /// Deflection angle, radians (positive).
    pub deflection_rad: f64,
    /// Spiral angles θs = Ls / 2R, radians.
    pub theta_in_rad: f64,
    pub theta_out_rad: f64,
    /// Spiral shift p off the tangent, m.
    pub shift_in_m: f64,
    pub shift_out_m: f64,
    /// Spiral extension k, m.
    pub extension_in_m: f64,
    pub extension_out_m: f64,
    /// Spiral end coordinates (x along the tangent, y towards the centre) in the frame of the TS / ST, m.
    pub spiral_in_xy: (f64, f64),
    pub spiral_out_xy: (f64, f64),
    /// PI to TS and PI to ST, m.
    pub tangent_in_m: f64,
    pub tangent_out_m: f64,
    /// Length of the circular part, m.
    pub arc_length_m: f64,
    /// Length of the whole curve, m.
    pub total_length_m: f64,
    /// External distance PI to the circle, m.
    pub external_m: f64,
    /// Mid-ordinate of the circular part, m.
    pub mid_ordinate_m: f64,
    /// Stations (internal) of the TS, SC, CS, ST. Without spirals TS = SC and CS = ST.
    pub ts_station_m: f64,
    pub sc_station_m: f64,
    pub cs_station_m: f64,
    pub st_station_m: f64,
    pub pi_station_m: f64,
    /// Plan points of TS, SC, CS, ST and the circle centre.
    pub ts: (f64, f64),
    pub sc: (f64, f64),
    pub cs: (f64, f64),
    pub st: (f64, f64),
    pub centre: (f64, f64),
}

/// The tangent (straight) on the leg between two consecutive PIs.
#[derive(Debug, Clone, PartialEq)]
pub struct TangentData {
    /// The leg runs from PI `from_pi` to PI `from_pi + 1`.
    pub from_pi: usize,
    /// Length of straight left between the end of one curve and the start of the next, m. Zero
    /// when the curves meet; the leg length minus the tangent lengths used.
    pub straight_m: f64,
    /// Distance between the two PIs, m.
    pub leg_m: f64,
    /// When there is a curve at both ends of the leg: `Some(true)` if they turn opposite ways
    /// (reverse curves), `Some(false)` if the same way (broken-back curves).
    pub reverse: Option<bool>,
    /// Station at the start of the straight, m.
    pub start_station_m: f64,
}

/// A solved horizontal design.
#[derive(Debug, Clone, PartialEq)]
pub struct HorizontalSolution {
    /// The alignment, ready for every other module. No profile yet.
    pub alignment: Alignment,
    pub curves: Vec<CurveData>,
    /// One per leg between PIs, in order.
    pub tangents: Vec<TangentData>,
    /// Matters that are not errors: angle points, curves that meet with no tangent.
    pub warnings: Vec<String>,
}

impl HorizontalSolution {
    /// Curve data for PI `index`, if there is a curve there.
    pub fn curve_at_pi(&self, index: usize) -> Option<&CurveData> {
        self.curves.iter().find(|c| c.pi_index == index)
    }
}

fn deg(r: f64) -> f64 {
    r.to_degrees()
}

impl HorizontalDesign {
    pub fn new(name: &str, start_station_m: f64, pis: Vec<Pi>) -> HorizontalDesign {
        HorizontalDesign { name: name.to_string(), start_station_m, pis }
    }

    fn dir(&self, i: usize) -> f64 {
        (self.pis[i + 1].y - self.pis[i].y).atan2(self.pis[i + 1].x - self.pis[i].x)
    }

    fn leg(&self, i: usize) -> f64 {
        (self.pis[i + 1].x - self.pis[i].x).hypot(self.pis[i + 1].y - self.pis[i].y)
    }

    /// Solves the design, or lists every reason it is impossible, in plain words.
    pub fn solve(&self) -> Result<HorizontalSolution, DesignError> {
        let n = self.pis.len();
        let mut problems: Vec<Problem> = Vec::new();
        if n < 2 {
            return Err(DesignError::Impossible(vec![Problem::whole("An alignment needs at least two PIs, a start and an end.")]));
        }
        for (i, p) in self.pis.iter().enumerate() {
            if !(p.x.is_finite() && p.y.is_finite()) {
                problems.push(Problem::at(i, format!("PI {i} has coordinates that are not finite numbers.")));
            }
            if !self.start_station_m.is_finite() {
                problems.push(Problem::whole("The start station is not a finite number."));
            }
        }
        if !problems.is_empty() {
            return Err(DesignError::Impossible(problems));
        }
        for i in 1..n {
            if self.pis[i].x == self.pis[i - 1].x && self.pis[i].y == self.pis[i - 1].y {
                problems.push(Problem::at(i, format!("PI {i} is at the same point as PI {}; move one of them.", i - 1)));
            }
        }
        if !problems.is_empty() {
            return Err(DesignError::Impossible(problems));
        }

        // The curve at each interior PI.
        let mut solved: Vec<Option<PiCurve>> = vec![None; n];
        let mut warnings: Vec<String> = Vec::new();
        #[allow(clippy::needless_range_loop)]
        for i in 1..n - 1 {
            let p = self.pis[i];
            let delta = wrap_pi(self.dir(i) - self.dir(i - 1));
            if p.radius == 0.0 && p.spiral_in == 0.0 && p.spiral_out == 0.0 {
                if delta.abs() > 1e-9 {
                    warnings.push(format!("PI {i} is an angle point: the alignment changes direction by {:.3} degrees with no curve.", deg(delta.abs())));
                }
                continue;
            }
            if delta.abs() < 1e-9 {
                warnings.push(format!("PI {i} lies on a straight line; the curve entered there has no effect."));
                continue;
            }
            if !(p.radius.is_finite() && p.radius > 0.0) {
                problems.push(Problem::at(i, format!("PI {i}: a curve needs a radius greater than zero (got {} m).", p.radius)));
                continue;
            }
            if !(p.spiral_in.is_finite() && p.spiral_out.is_finite() && p.spiral_in >= 0.0 && p.spiral_out >= 0.0) {
                problems.push(Problem::at(i, format!("PI {i}: spiral lengths must be zero or positive.")));
                continue;
            }
            if delta.abs() > std::f64::consts::PI - 1e-6 {
                problems.push(Problem::at(i, format!("PI {i}: the alignment doubles back on itself ({:.2} degrees of deflection); no single curve can do that.", deg(delta.abs()))));
                continue;
            }
            let spirals = p.spiral_in + p.spiral_out;
            if spirals / (2.0 * p.radius) > delta.abs() + 1e-12 {
                let max_total = 2.0 * p.radius * delta.abs();
                let min_r = spirals / (2.0 * delta.abs());
                problems.push(Problem::at(
                    i,
                    format!(
                        "PI {i}: the radius ({:.1} m) is too small for the deflection ({:.3} degrees) and the spiral lengths ({:.1} m + {:.1} m). The spirals alone would turn the road {:.3} degrees, more than the deflection, leaving no circular arc. Either shorten the spirals to {:.1} m in total or less for this radius, or raise the radius to {:.1} m or more for these spirals.",
                        p.radius,
                        deg(delta.abs()),
                        p.spiral_in,
                        p.spiral_out,
                        deg(spirals / (2.0 * p.radius)),
                        max_total,
                        min_r
                    ),
                ));
                continue;
            }
            match PiCurve::solve(delta, p.radius, p.spiral_in, p.spiral_out) {
                Ok(c) => solved[i] = Some(c),
                Err(m) => problems.push(Problem::at(i, format!("PI {i}: {m}"))),
            }
        }
        if !problems.is_empty() {
            return Err(DesignError::Impossible(problems));
        }

        // Tangents: each leg must hold the tangent lengths that use it.
        let t_out = |i: usize| solved[i].map_or(0.0, |c| c.tangent_out);
        let t_in = |i: usize| solved[i].map_or(0.0, |c| c.tangent_in);
        let mut straights = Vec::with_capacity(n - 1);
        for j in 0..n - 1 {
            let leg = self.leg(j);
            let (a, b) = (t_out(j), t_in(j + 1));
            let straight = leg - a - b;
            if straight < -1e-9 {
                let what = if j == 0 {
                    format!("The curve at PI 1 needs {b:.3} m of tangent but the first leg (PI 0 to PI 1) is only {leg:.3} m long.")
                } else if j + 1 == n - 1 {
                    format!("The curve at PI {j} needs {a:.3} m of tangent but the last leg (PI {j} to PI {}) is only {leg:.3} m long.", j + 1)
                } else {
                    format!(
                        "The curves at PI {j} and PI {} overlap by {:.3} m: they need {a:.3} m + {b:.3} m = {:.3} m of tangent but PIs {j} and {} are only {leg:.3} m apart. The tangent between them is too short; reduce a radius or spiral, or move the PIs further apart.",
                        j + 1,
                        -straight,
                        a + b,
                        j + 1
                    )
                };
                problems.push(Problem::at(j + 1, what));
            }
            straights.push(straight.max(0.0));
        }
        if !problems.is_empty() {
            return Err(DesignError::Impossible(problems));
        }

        let alignment = Alignment::from_pis(&self.name, self.start_station_m, &self.pis).map_err(|e| DesignError::Impossible(vec![Problem::whole(e.to_string())]))?;

        // Curve and tangent records, walking the stations.
        let mut curves = Vec::new();
        let mut tangents = Vec::new();
        let mut cursor = self.start_station_m; // station at the end of the previous curve
        for j in 0..n - 1 {
            let reverse = match (solved[j], solved[j + 1]) {
                (Some(_), Some(_)) => {
                    let d0 = wrap_pi(self.dir(j) - if j > 0 { self.dir(j - 1) } else { self.dir(j) });
                    let d1 = wrap_pi(self.dir(j + 1) - self.dir(j));
                    Some(d0 * d1 < 0.0)
                }
                _ => None,
            };
            tangents.push(TangentData { from_pi: j, straight_m: straights[j], leg_m: self.leg(j), reverse, start_station_m: cursor });
            if let Some(r) = reverse {
                if straights[j] < 1e-6 {
                    warnings.push(format!(
                        "The curves at PI {j} and PI {} meet with no tangent between them ({}).",
                        j + 1,
                        if r { "a reverse curve" } else { "a broken-back curve" }
                    ));
                }
            }
            let ts_station = cursor + straights[j];
            if j + 1 == n - 1 {
                break;
            }
            let i = j + 1;
            let Some(c) = solved[i] else {
                cursor = ts_station;
                continue;
            };
            let p = self.pis[i];
            let heading = self.dir(i - 1);
            let out_heading = self.dir(i);
            let delta = wrap_pi(out_heading - heading);
            let turn = if delta > 0.0 { Turn::Left } else { Turn::Right };
            let u = |h: f64| (h.cos(), h.sin());
            let (ux, uy) = u(heading);
            let ts = (p.x - c.tangent_in * ux, p.y - c.tangent_in * uy);
            let mut pos = (ts.0, ts.1, heading);
            let step = |e: AlignmentElement, pos: &mut (f64, f64, f64)| {
                let end = e.end();
                *pos = end;
                (end.0, end.1)
            };
            let sc = if p.spiral_in > 0.0 { step(AlignmentElement::spiral(pos.0, pos.1, pos.2, p.spiral_in, None, Some(p.radius), turn), &mut pos) } else { ts };
            let cs = if c.arc_length > 1e-12 { step(AlignmentElement::arc(pos.0, pos.1, pos.2, p.radius, c.arc_length, turn), &mut pos) } else { sc };
            let st = if p.spiral_out > 0.0 { step(AlignmentElement::spiral(pos.0, pos.1, pos.2, p.spiral_out, Some(p.radius), None, turn), &mut pos) } else { cs };
            // Centre: R to the inside of the circle's heading at SC.
            let h_sc = heading + turn_sign(turn) * c.theta_in;
            let centre = (sc.0 - turn_sign(turn) * p.radius * h_sc.sin(), sc.1 + turn_sign(turn) * p.radius * h_sc.cos());
            let external = (p.x - centre.0).hypot(p.y - centre.1) - p.radius;
            let arc_angle = c.arc_length / p.radius;
            let total = p.spiral_in + c.arc_length + p.spiral_out;
            let xy = |ls: f64| if ls > 0.0 { clothoid_xy(ls, p.radius) } else { (0.0, 0.0) };
            let sc_station = ts_station + p.spiral_in;
            let cs_station = sc_station + c.arc_length;
            let st_station = cs_station + p.spiral_out;
            curves.push(CurveData {
                pi_index: i,
                turn,
                radius_m: p.radius,
                spiral_in_m: p.spiral_in,
                spiral_out_m: p.spiral_out,
                deflection_rad: c.deflection_rad,
                theta_in_rad: c.theta_in,
                theta_out_rad: c.theta_out,
                shift_in_m: c.shift_in,
                shift_out_m: c.shift_out,
                extension_in_m: c.k_in,
                extension_out_m: c.k_out,
                spiral_in_xy: xy(p.spiral_in),
                spiral_out_xy: xy(p.spiral_out),
                tangent_in_m: c.tangent_in,
                tangent_out_m: c.tangent_out,
                arc_length_m: c.arc_length,
                total_length_m: total,
                external_m: external,
                mid_ordinate_m: p.radius * (1.0 - (0.5 * arc_angle).cos()),
                ts_station_m: ts_station,
                sc_station_m: sc_station,
                cs_station_m: cs_station,
                st_station_m: st_station,
                pi_station_m: ts_station + c.tangent_in,
                ts,
                sc,
                cs,
                st,
                centre,
            });
            cursor = st_station;
        }
        Ok(HorizontalSolution { alignment, curves, tangents, warnings })
    }

    /// The range of radii for which the curve at PI `index` can be fitted with its present spiral
    /// lengths, the other PIs unchanged: `(smallest, largest)` in metres.
    ///
    /// The smallest is `(Ls1 + Ls2) / (2Δ)` (the spirals then use the whole deflection). The
    /// largest is found by bisection on the overlap with the neighbouring curves, which only
    /// grows as the radius grows. `None` when the PI is not an interior PI with a deflection,
    /// or even the smallest radius does not fit. Meant for the sketching UI's drag limits.
    pub fn feasible_radius_range(&self, index: usize) -> Option<(f64, f64)> {
        if index == 0 || index + 1 >= self.pis.len() {
            return None;
        }
        let delta = wrap_pi(self.dir(index) - self.dir(index - 1)).abs();
        if !(1e-9..=std::f64::consts::PI - 1e-6).contains(&delta) {
            return None;
        }
        let p = self.pis[index];
        let lo = ((p.spiral_in + p.spiral_out) / (2.0 * delta)).max(1e-3) * (1.0 + 1e-9);
        let fits = |r: f64| {
            let mut d = self.clone();
            d.pis[index].radius = r;
            d.solve().is_ok()
        };
        if !fits(lo) {
            return None;
        }
        let (mut a, mut b) = (lo, 1.0e7);
        if fits(b) {
            return Some((lo, b));
        }
        for _ in 0..200 {
            let m = 0.5 * (a + b);
            if fits(m) {
                a = m;
            } else {
                b = m;
            }
        }
        Some((lo, a))
    }

    /// Applies `edit` to a copy; keeps the result only if it solves. The design is unchanged on error.
    pub fn try_edit(&mut self, edit: impl FnOnce(&mut HorizontalDesign) -> Result<(), DesignError>) -> Result<HorizontalSolution, DesignError> {
        let mut copy = self.clone();
        edit(&mut copy)?;
        let solution = copy.solve()?;
        *self = copy;
        Ok(solution)
    }

    fn check_index(&self, index: usize) -> Result<(), DesignError> {
        if index < self.pis.len() {
            Ok(())
        } else {
            Err(DesignError::BadEdit(format!("There is no PI {index}; the alignment has {} PIs (0 to {}).", self.pis.len(), self.pis.len().saturating_sub(1))))
        }
    }

    /// Inserts a PI so that it becomes PI `index` (0 to the number of PIs). Refused, with the
    /// design unchanged, if the result is impossible.
    pub fn insert_pi(&mut self, index: usize, pi: Pi) -> Result<HorizontalSolution, DesignError> {
        if index > self.pis.len() {
            return Err(DesignError::BadEdit(format!("Cannot insert at {index}: the alignment has {} PIs.", self.pis.len())));
        }
        self.try_edit(|d| {
            d.pis.insert(index, pi);
            Ok(())
        })
    }

    /// Moves PI `index` to a new point, keeping its radius and spirals.
    pub fn move_pi(&mut self, index: usize, x: f64, y: f64) -> Result<HorizontalSolution, DesignError> {
        self.check_index(index)?;
        self.try_edit(|d| {
            d.pis[index].x = x;
            d.pis[index].y = y;
            Ok(())
        })
    }

    /// Deletes PI `index`; its neighbours become joined. At least two PIs must remain.
    pub fn delete_pi(&mut self, index: usize) -> Result<HorizontalSolution, DesignError> {
        self.check_index(index)?;
        self.try_edit(|d| {
            d.pis.remove(index);
            Ok(())
        })
    }

    /// Changes the radius of the curve at PI `index` (0 removes the curve).
    pub fn set_radius(&mut self, index: usize, radius_m: f64) -> Result<HorizontalSolution, DesignError> {
        self.check_index(index)?;
        self.try_edit(|d| {
            d.pis[index].radius = radius_m;
            Ok(())
        })
    }

    /// Changes the spiral lengths at PI `index`.
    pub fn set_spirals(&mut self, index: usize, spiral_in_m: f64, spiral_out_m: f64) -> Result<HorizontalSolution, DesignError> {
        self.check_index(index)?;
        self.try_edit(|d| {
            d.pis[index].spiral_in = spiral_in_m;
            d.pis[index].spiral_out = spiral_out_m;
            Ok(())
        })
    }
}

fn turn_sign(t: Turn) -> f64 {
    match t {
        Turn::Left => 1.0,
        Turn::Right => -1.0,
    }
}
