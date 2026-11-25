// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Two dimensional least squares adjustment of a survey network: directions (with one orientation
//! unknown per setup), distances, known azimuths, GNSS baseline components (E and N), and weighted
//! control coordinates. Heights are adjusted separately (`level.rs`); a 3D adjustment is not delivered.
//!
//! OBSERVATION EQUATIONS (derived here; the linearisation step is Teunissen, *Network Quality Control*,
//! 2024, section 2.4.2, eq. 2.45). For a station S and a target T with dE = E_T - E_S, dN = N_T - N_S,
//! s^2 = dE^2 + dN^2 and azimuth a = atan2(dE, dN) (clockwise from north):
//!
//! * distance          f = s:                       df/dE_T = dE/s,  df/dN_T = dN/s, and the negatives for S;
//! * azimuth           f = a:                       da/dE_T = dN/s^2, da/dN_T = -dE/s^2, negatives for S
//!   (from a = atan2(dE, dN): da/d(dE) = dN/s^2 and da/d(dN) = -dE/s^2);
//! * circle reading    f = g (a - w):               g = +1 for readings increasing clockwise, -1 for
//!   anticlockwise; w is the orientation unknown of the setup (the azimuth of the circle zero), so
//!   df/dw = -g and the coordinate coefficients are g times those of the azimuth;
//! * baseline component, constrained coordinate: linear.
//!
//! Angles measured at a station are differences of two readings of the same setup, so they are
//! contained exactly (with the right correlation) in the readings with their orientation unknown. A
//! setup whose backsight is a point with coordinates contributes one more reading, the backsight
//! reading (0 for horizontal angles turned from the backsight; the stored backsight circle reading
//! for circle readings), unless the backsight is observed explicitly. A setup whose backsight is only an
//! azimuth contributes an observation of its orientation w = az - g r with the standard error of one
//! reading of the equipment (an assumption stated here, change the equipment record to change it).
//!
//! STANDARD ERRORS (propagation of independent errors; derived here). For the mean of n rows of a set
//! at horizontal distance D with equipment standard errors:
//!   direction:  sigma^2 = (sp^2 + sr^2) / n + (ei^2 + et^2) / D^2   (radians)
//!   distance:   sigma^2 = (a^2 + (b D)^2) / n + (ei^2 + et^2)        (metres)
//! with sp, sr the pointing and reading standard errors, a, b the EDM constant and proportional parts,
//! and ei, et the standard deviation of the instrument and target centring in each horizontal
//! coordinate. Derivation of the centring terms: an offset of the instrument or target by a vector with
//! independent components of standard deviation e changes the direction by (component perpendicular to
//! the line)/D, so by e/D radians, and the distance by the component along the line, so by e metres;
//! the two instrument and target contributions add in variance. Centring errors are systematic over
//! the repeated rows of one setup, so they are not divided by n. The centring terms are included only
//! when `include_centring` is set. These are the standard textbook error propagation, not a standard
//! prescribed by any survey regulation.
//!
//! CONVERGENCE: Gauss-Newton iteration stops when dx' N dx <= `convergence` (dimensionless) or after
//! `max_iterations` (then `converged` is false). Both are inputs.
//!
//! TESTS: global test of the variance factor, w-test per observation and the error ellipse are
//! described in `lsq.rs` and `stats.rs`.

use std::collections::HashMap;

use afrobim_cogo::angle::normalize_deg;
use serde::{Deserialize, Serialize};

use crate::error::{Result, SurveyError};
use crate::linalg::Matrix;
use crate::lsq::{self, Row};
use crate::model::*;
use crate::reduce::{orientation, reduce_setup};
use crate::stats::*;
use crate::units::HzDirection;

const ARCSEC_PER_RAD: f64 = 206_264.806_247_096_36; // 180 * 3600 / pi

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdjustOptions {
    pub max_iterations: usize,
    /// Convergence criterion on dx' N dx.
    pub convergence: f64,
    /// Probability content of the error ellipses, for example 0.95 (an input).
    pub ellipse_confidence: f64,
    /// Significance level of the global test of the variance factor (two sided).
    pub global_alpha: f64,
    /// An observation is flagged when |w| exceeds this threshold (an input; `normal_two_sided_critical`
    /// computes the value for a chosen significance level).
    pub blunder_threshold: f64,
    /// Scale covariances by the a posteriori variance factor and use the F based ellipse factor.
    pub scale_by_posteriori: bool,
    pub include_centring: bool,
    /// Equipment used for setups that name none.
    pub default_equipment: Option<String>,
    /// Observation keys left out of the adjustment (for example after data snooping).
    pub excluded: Vec<String>,
}

impl AdjustOptions {
    /// All numerical choices are explicit; there are no defaults.
    pub fn new(max_iterations: usize, convergence: f64, ellipse_confidence: f64, global_alpha: f64, blunder_threshold: f64) -> AdjustOptions {
        AdjustOptions {
            max_iterations,
            convergence,
            ellipse_confidence,
            global_alpha,
            blunder_threshold,
            scale_by_posteriori: false,
            include_centring: true,
            default_equipment: None,
            excluded: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObsKind {
    Direction,
    Distance,
    Azimuth,
    BaselineE,
    BaselineN,
    CoordinateE,
    CoordinateN,
    Orientation,
    HeightDifference,
    BaselineU,
    CoordinateZ,
}

impl ObsKind {
    pub fn is_angular(self) -> bool {
        matches!(self, ObsKind::Direction | ObsKind::Azimuth | ObsKind::Orientation)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObsResult {
    pub key: String,
    pub kind: ObsKind,
    pub from: String,
    pub to: String,
    /// Degrees for angular kinds, metres otherwise.
    pub observed: f64,
    pub adjusted: f64,
    /// Residual (adjusted minus observed) in arc seconds for angular kinds, metres otherwise.
    pub residual: f64,
    /// Standard error in arc seconds or metres.
    pub sigma: f64,
    /// Standard deviation of the residual, same units.
    pub sigma_residual: f64,
    pub redundancy: f64,
    /// w statistic; `None` for an uncontrolled observation (redundancy number near zero).
    pub std_residual: Option<f64>,
    pub flagged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ellipse {
    pub semi_major: f64,
    pub semi_minor: f64,
    /// Azimuth of the major axis, degrees clockwise from north, in [0, 180).
    pub azimuth_major_deg: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdjustedPoint {
    pub name: String,
    pub e: f64,
    pub n: f64,
    pub sigma_e: f64,
    pub sigma_n: f64,
    pub covariance_en: f64,
    pub ellipse: Ellipse,
    pub fixed: bool,
    /// Adjusted minus approximate coordinates.
    pub shift: (f64, f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GlobalTest {
    pub statistic: f64,
    pub dof: usize,
    pub alpha: f64,
    /// Critical values of chi-square at alpha/2 and 1 - alpha/2 (computed, see `stats::chi2_quantile`).
    pub lower_critical: f64,
    pub upper_critical: f64,
    /// One sided critical value at 1 - alpha (Teunissen eq. 3.44 tests T > this).
    pub one_sided_critical: f64,
    /// Statistic between the two sided limits.
    pub passes: bool,
    /// Statistic above the one sided limit (model or stochastic model too optimistic, or a blunder).
    pub exceeds_one_sided: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrientationResult {
    pub setup_id: u64,
    pub station: String,
    pub orientation_deg: f64,
    pub sigma_arcsec: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Adjustment {
    pub network: String,
    pub iterations: usize,
    pub converged: bool,
    pub last_step_norm: f64,
    pub n_observations: usize,
    pub n_unknowns: usize,
    pub dof: isize,
    /// v' W v with weights from the stated standard errors (a priori variance factor 1).
    pub vtpv: f64,
    pub sigma0_sq_posteriori: Option<f64>,
    pub global_test: Option<GlobalTest>,
    pub points: Vec<AdjustedPoint>,
    pub orientations: Vec<OrientationResult>,
    pub observations: Vec<ObsResult>,
    pub scaled_by_posteriori: bool,
    pub ellipse_scale: f64,
    pub ellipse_confidence: f64,
    pub blunder_threshold: f64,
    /// Covariance matrix of the unknowns (scaled as stated by `scaled_by_posteriori`) and the index of
    /// each point's E and N in it.
    pub covariance: Matrix,
    pub index: Vec<(String, Option<usize>, Option<usize>)>,
}

/// Precision of the vector between two points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RelativePrecision {
    pub distance: f64,
    pub azimuth_deg: f64,
    pub sigma_distance: f64,
    pub sigma_azimuth_arcsec: f64,
    /// distance / sigma_distance: the "1 in n" relative precision, `None` if sigma is zero.
    pub ratio: Option<f64>,
    pub ellipse: Ellipse,
}

impl Adjustment {
    pub fn point(&self, name: &str) -> Option<&AdjustedPoint> {
        self.points.iter().find(|p| p.name == name)
    }

    /// Relative precision between two points from the covariance matrix of the adjustment:
    /// the covariance of the vector (dE, dN) = (E_b - E_a, N_b - N_a) is S_bb + S_aa - S_ab - S_ba
    /// (variance of a difference). The standard deviation of the distance s follows by propagation
    /// with u = (dE, dN)/s: sigma_s^2 = u' S u; that of the azimuth: sigma_a^2 = (dN, -dE) S (dN, -dE)'/s^4.
    pub fn relative_precision(&self, a: &str, b: &str) -> Result<RelativePrecision> {
        let pa = self.point(a).ok_or_else(|| SurveyError::NoPoint(a.into()))?;
        let pb = self.point(b).ok_or_else(|| SurveyError::NoPoint(b.into()))?;
        let ia = self.index.iter().find(|x| x.0 == a).unwrap();
        let ib = self.index.iter().find(|x| x.0 == b).unwrap();
        let c = |i: Option<usize>, j: Option<usize>| match (i, j) {
            (Some(i), Some(j)) => self.covariance.get(i, j),
            _ => 0.0,
        };
        let s_ee = c(ia.1, ia.1) + c(ib.1, ib.1) - 2.0 * c(ia.1, ib.1);
        let s_nn = c(ia.2, ia.2) + c(ib.2, ib.2) - 2.0 * c(ia.2, ib.2);
        let s_en = c(ia.1, ia.2) + c(ib.1, ib.2) - c(ia.1, ib.2) - c(ib.1, ia.2);
        let (de, dn) = (pb.e - pa.e, pb.n - pa.n);
        let s = de.hypot(dn);
        if s == 0.0 {
            return Err(SurveyError::Invalid("the two points coincide".into()));
        }
        let (ue, un) = (de / s, dn / s);
        let var_s = ue * ue * s_ee + 2.0 * ue * un * s_en + un * un * s_nn;
        let var_a = (dn * dn * s_ee - 2.0 * dn * de * s_en + de * de * s_nn) / s.powi(4);
        let ellipse = ellipse_from(s_ee, s_nn, s_en, self.ellipse_scale, self.ellipse_confidence);
        let sd = var_s.max(0.0).sqrt();
        Ok(RelativePrecision {
            distance: s,
            azimuth_deg: normalize_deg(de.atan2(dn).to_degrees()),
            sigma_distance: sd,
            sigma_azimuth_arcsec: var_a.max(0.0).sqrt() * ARCSEC_PER_RAD,
            ratio: if sd > 0.0 { Some(s / sd) } else { None },
            ellipse,
        })
    }
}

/// Ellipse of a 2x2 covariance: eigenvalues l = (a + c)/2 +- sqrt(((a - c)/2)^2 + b^2); the major axis
/// has azimuth theta with tan(2 theta) = 2b / (c - a) (measured from north, MIT 12.215 lecture 14, the
/// relation tan 2phi = 2 s_ne / (s_n^2 - s_e^2)); semi-axes are k sqrt(l).
pub fn ellipse_from(var_e: f64, var_n: f64, cov_en: f64, k: f64, confidence: f64) -> Ellipse {
    let m = 0.5 * (var_e + var_n);
    let d = (0.25 * (var_e - var_n).powi(2) + cov_en * cov_en).sqrt();
    let l1 = (m + d).max(0.0);
    let l2 = (m - d).max(0.0);
    let theta = 0.5 * (2.0 * cov_en).atan2(var_n - var_e);
    let mut az = theta.to_degrees();
    while az < 0.0 {
        az += 180.0;
    }
    while az >= 180.0 {
        az -= 180.0;
    }
    Ellipse { semi_major: k * l1.sqrt(), semi_minor: k * l2.sqrt(), azimuth_major_deg: az, confidence }
}

fn wrap_pi(x: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    let mut y = x % two_pi;
    if y > std::f64::consts::PI {
        y -= two_pi;
    } else if y <= -std::f64::consts::PI {
        y += two_pi;
    }
    y
}

#[derive(Clone)]
struct Pt {
    name: String,
    e0: f64,
    n0: f64,
    ie: Option<usize>,
    inn: Option<usize>,
    fixed: bool,
}

enum Eq {
    Reading { from: usize, to: usize, omega: usize, g: f64 },
    Distance { from: usize, to: usize },
    Azimuth { from: usize, to: usize },
    BaseE { from: usize, to: usize },
    BaseN { from: usize, to: usize },
    Coord { pt: usize, north: bool },
    Orient { omega: usize },
}

struct Ob {
    key: String,
    kind: ObsKind,
    from: String,
    to: String,
    value: f64,
    sigma: f64,
    eq: Eq,
}

pub(crate) fn pick_equipment<'a>(db: &'a SurveyDatabase, setup: &Setup, opts: &AdjustOptions) -> Result<&'a Equipment> {
    let name = setup.equipment.as_ref().or(opts.default_equipment.as_ref()).ok_or_else(|| {
        SurveyError::Invalid(format!("setup {} names no equipment and no default equipment is given: standard errors are inputs", setup.station))
    })?;
    db.equipment(name).ok_or_else(|| SurveyError::Invalid(format!("equipment '{name}' is not in the database")))
}

/// Adjust one network in plan.
pub fn adjust_network_2d(db: &SurveyDatabase, network: &str, opts: &AdjustOptions) -> Result<Adjustment> {
    let net = db.network(network)?;
    let sgn = if db.settings.hz_direction == HzDirection::Right { 1.0 } else { -1.0 };
    let mut pts: Vec<Pt> = Vec::new();
    let mut pmap: HashMap<String, usize> = HashMap::new();
    let mut x0: Vec<f64> = Vec::new();
    let mut ensure = |name: &str, pts: &mut Vec<Pt>, x0: &mut Vec<f64>| -> Result<usize> {
        if let Some(&i) = pmap.get(name) {
            return Ok(i);
        }
        let p = net.point(name).ok_or_else(|| SurveyError::NoPoint(name.into()))?;
        let (e, n) = p.e.zip(p.n).ok_or_else(|| SurveyError::Invalid(format!("point {name} has no approximate coordinates: compute them by radiation first")))?;
        let fixed_e = p.control && p.fix_e;
        let fixed_n = p.control && p.fix_n;
        let ie = if fixed_e {
            None
        } else {
            x0.push(e);
            Some(x0.len() - 1)
        };
        let inn = if fixed_n {
            None
        } else {
            x0.push(n);
            Some(x0.len() - 1)
        };
        pts.push(Pt { name: name.into(), e0: e, n0: n, ie, inn, fixed: fixed_e && fixed_n });
        pmap.insert(name.into(), pts.len() - 1);
        Ok(pts.len() - 1)
    };

    let mut obs: Vec<Ob> = Vec::new();
    let mut omegas: Vec<(u64, String, usize)> = Vec::new(); // setup id, station, param index
    let excluded = |k: &str| opts.excluded.iter().any(|e| e == k);
    let dist_approx = |a: &Pt, b: &Pt| (b.e0 - a.e0).hypot(b.n0 - a.n0);

    for setup in &net.setups {
        let sets = reduce_setup(db, net, setup)?;
        if !sets.iter().any(|s| s.hz_deg.is_some() || s.horizontal_distance.is_some()) {
            continue;
        }
        let eq = pick_equipment(db, setup, opts)?;
        let st = ensure(&setup.station, &mut pts, &mut x0)?;
        let has_dirs = sets.iter().any(|s| s.hz_deg.is_some());
        let mut omega_idx = None;
        let mut om_val = 0.0;
        let mut r_bs = 0.0;
        if has_dirs {
            let o = orientation(net, setup, &sets)?;
            let any_angle = sets.iter().any(|s| s.hz_kind == HzKind::Angle && s.hz_deg.is_some());
            r_bs = if any_angle { 0.0 } else { o.backsight_reading_deg };
            om_val = (o.backsight_azimuth_deg - sgn * r_bs).to_radians();
            x0.push(om_val);
            omega_idx = Some(x0.len() - 1);
            omegas.push((setup.id, setup.station.clone(), x0.len() - 1));
        }
        let sig_reading = |n: usize, d: f64| {
            let n = n.max(1) as f64;
            let mut var = (eq.sigma_pointing_arcsec.powi(2) + eq.sigma_reading_arcsec.powi(2)) / ARCSEC_PER_RAD.powi(2) / n;
            if opts.include_centring && d > 0.0 {
                var += (eq.centring_instrument_m.powi(2) + eq.centring_target_m.powi(2)) / (d * d);
            }
            var.sqrt()
        };
        let sig_dist = |n: usize, d: f64| {
            let n = n.max(1) as f64;
            let mut var = (eq.sigma_distance_const_m.powi(2) + (eq.sigma_distance_ppm * 1e-6 * d).powi(2)) / n;
            if opts.include_centring {
                var += eq.centring_instrument_m.powi(2) + eq.centring_target_m.powi(2);
            }
            var.sqrt()
        };
        let mut backsight_observed = false;
        for s in &sets {
            let to = ensure(&s.target, &mut pts, &mut x0)?;
            if Some(&s.target) == setup.backsight_point.as_ref() && s.hz_deg.is_some() {
                backsight_observed = true;
            }
            let d_approx = dist_approx(&pts[st], &pts[to]);
            if let (Some(h), Some(w)) = (s.hz_deg, omega_idx) {
                let key = format!("S{}:{}:dir", setup.id, s.target);
                if !excluded(&key) {
                    let hrad = h.to_radians();
                    let n = s.hz_spread.map(|x| x.n).unwrap_or(1);
                    obs.push(Ob { key, kind: ObsKind::Direction, from: setup.station.clone(), to: s.target.clone(), value: hrad, sigma: sig_reading(n, d_approx), eq: Eq::Reading { from: st, to, omega: w, g: sgn } });
                }
            }
            if let Some(d) = s.horizontal_distance {
                let key = format!("S{}:{}:dist", setup.id, s.target);
                if !excluded(&key) {
                    let n = s.distance_spread.map(|x| x.n).unwrap_or(1);
                    obs.push(Ob { key, kind: ObsKind::Distance, from: setup.station.clone(), to: s.target.clone(), value: d, sigma: sig_dist(n, d), eq: Eq::Distance { from: st, to } });
                }
            }
        }
        if let (Some(w), true) = (omega_idx, has_dirs) {
            let _ = om_val;
            match (&setup.backsight_point, backsight_observed) {
                (Some(b), false) if net.point(b).and_then(|p| p.e.zip(p.n)).is_some() => {
                    let bi = ensure(b, &mut pts, &mut x0)?;
                    let key = format!("S{}:{}:bsdir", setup.id, b);
                    if !excluded(&key) {
                        let d = dist_approx(&pts[st], &pts[bi]);
                        obs.push(Ob { key, kind: ObsKind::Direction, from: setup.station.clone(), to: b.clone(), value: r_bs.to_radians(), sigma: sig_reading(1, d), eq: Eq::Reading { from: st, to: bi, omega: w, g: sgn } });
                    }
                }
                (Some(_), true) => {}
                _ => {
                    // azimuth only (or backsight without coordinates): orientation observation
                    let key = format!("S{}:orient", setup.id);
                    if !excluded(&key) {
                        let s1 = ((eq.sigma_pointing_arcsec.powi(2) + eq.sigma_reading_arcsec.powi(2)).sqrt()) / ARCSEC_PER_RAD;
                        obs.push(Ob { key, kind: ObsKind::Orientation, from: setup.station.clone(), to: String::new(), value: om_val, sigma: s1, eq: Eq::Orient { omega: w } });
                    }
                }
            }
        }
    }
    for (i, d) in net.directions.iter().enumerate() {
        if let Some(sa) = d.sigma_arcsec {
            let key = format!("AZ{}:{}>{}", i, d.from, d.to);
            if excluded(&key) {
                continue;
            }
            let a = ensure(&d.from, &mut pts, &mut x0)?;
            let b = ensure(&d.to, &mut pts, &mut x0)?;
            obs.push(Ob { key, kind: ObsKind::Azimuth, from: d.from.clone(), to: d.to.clone(), value: d.azimuth_deg.to_radians(), sigma: sa / ARCSEC_PER_RAD, eq: Eq::Azimuth { from: a, to: b } });
        }
    }
    for (i, b) in net.baselines.iter().enumerate() {
        let a = ensure(&b.from, &mut pts, &mut x0)?;
        let t = ensure(&b.to, &mut pts, &mut x0)?;
        let ke = format!("BL{}:{}>{}:E", i, b.from, b.to);
        let kn = format!("BL{}:{}>{}:N", i, b.from, b.to);
        if !excluded(&ke) {
            obs.push(Ob { key: ke, kind: ObsKind::BaselineE, from: b.from.clone(), to: b.to.clone(), value: b.de, sigma: b.sigma_e, eq: Eq::BaseE { from: a, to: t } });
        }
        if !excluded(&kn) {
            obs.push(Ob { key: kn, kind: ObsKind::BaselineN, from: b.from.clone(), to: b.to.clone(), value: b.dn, sigma: b.sigma_n, eq: Eq::BaseN { from: a, to: t } });
        }
    }
    // weighted control coordinates (free components with a stated sigma)
    let used: Vec<String> = pts.iter().map(|p| p.name.clone()).collect();
    for name in used {
        let p = net.point(&name).unwrap();
        if !p.control {
            continue;
        }
        let i = pmap[&name];
        if !p.fix_e {
            if let (Some(s), Some(e)) = (p.sigma_e, p.e) {
                let key = format!("PT:{name}:E");
                if !excluded(&key) {
                    obs.push(Ob { key, kind: ObsKind::CoordinateE, from: name.clone(), to: String::new(), value: e, sigma: s, eq: Eq::Coord { pt: i, north: false } });
                }
            }
        }
        if !p.fix_n {
            if let (Some(s), Some(n)) = (p.sigma_n, p.n) {
                let key = format!("PT:{name}:N");
                if !excluded(&key) {
                    obs.push(Ob { key, kind: ObsKind::CoordinateN, from: name.clone(), to: String::new(), value: n, sigma: s, eq: Eq::Coord { pt: i, north: true } });
                }
            }
        }
    }
    if obs.is_empty() {
        return Err(SurveyError::Invalid("the network has no observations to adjust".into()));
    }
    // parameters that no observation uses (a free point with only excluded observations) would make
    // the normal matrix singular; the Cholesky error names the unknown index in that case.
    let sigmas: Vec<f64> = obs.iter().map(|o| o.sigma).collect();
    let pts_ref = pts.clone();
    let coord = |x: &[f64], i: usize| -> (f64, f64) {
        let p = &pts_ref[i];
        (p.ie.map(|k| x[k]).unwrap_or(p.e0), p.inn.map(|k| x[k]).unwrap_or(p.n0))
    };
    let mut lin = |x: &[f64]| -> Result<Vec<Row>> {
        let mut rows = Vec::with_capacity(obs.len());
        for o in &obs {
            let mut cols: Vec<(usize, f64)> = Vec::new();
            let mut push = |i: Option<usize>, v: f64| {
                if let Some(i) = i {
                    cols.push((i, v));
                }
            };
            let l = match o.eq {
                Eq::Reading { from, to, omega, g } => {
                    let (es, ns) = coord(x, from);
                    let (et, nt) = coord(x, to);
                    let (de, dn) = (et - es, nt - ns);
                    let s2 = de * de + dn * dn;
                    let a = de.atan2(dn);
                    push(pts_ref[to].ie, g * dn / s2);
                    push(pts_ref[to].inn, -g * de / s2);
                    push(pts_ref[from].ie, -g * dn / s2);
                    push(pts_ref[from].inn, g * de / s2);
                    push(Some(omega), -g);
                    wrap_pi(o.value - g * (a - x[omega]))
                }
                Eq::Azimuth { from, to } => {
                    let (es, ns) = coord(x, from);
                    let (et, nt) = coord(x, to);
                    let (de, dn) = (et - es, nt - ns);
                    let s2 = de * de + dn * dn;
                    push(pts_ref[to].ie, dn / s2);
                    push(pts_ref[to].inn, -de / s2);
                    push(pts_ref[from].ie, -dn / s2);
                    push(pts_ref[from].inn, de / s2);
                    wrap_pi(o.value - de.atan2(dn))
                }
                Eq::Distance { from, to } => {
                    let (es, ns) = coord(x, from);
                    let (et, nt) = coord(x, to);
                    let (de, dn) = (et - es, nt - ns);
                    let s = de.hypot(dn);
                    push(pts_ref[to].ie, de / s);
                    push(pts_ref[to].inn, dn / s);
                    push(pts_ref[from].ie, -de / s);
                    push(pts_ref[from].inn, -dn / s);
                    o.value - s
                }
                Eq::BaseE { from, to } => {
                    let (es, _) = coord(x, from);
                    let (et, _) = coord(x, to);
                    push(pts_ref[to].ie, 1.0);
                    push(pts_ref[from].ie, -1.0);
                    o.value - (et - es)
                }
                Eq::BaseN { from, to } => {
                    let (_, ns) = coord(x, from);
                    let (_, nt) = coord(x, to);
                    push(pts_ref[to].inn, 1.0);
                    push(pts_ref[from].inn, -1.0);
                    o.value - (nt - ns)
                }
                Eq::Coord { pt, north } => {
                    let (e, n) = coord(x, pt);
                    if north {
                        push(pts_ref[pt].inn, 1.0);
                        o.value - n
                    } else {
                        push(pts_ref[pt].ie, 1.0);
                        o.value - e
                    }
                }
                Eq::Orient { omega } => {
                    push(Some(omega), 1.0);
                    wrap_pi(o.value - x[omega])
                }
            };
            rows.push(Row { l, cols });
        }
        Ok(rows)
    };
    let sol = lsq::solve(x0.clone(), &sigmas, &mut lin, opts.max_iterations, opts.convergence)?;

    let dof = sol.dof;
    let s0sq = if dof > 0 { Some(sol.vtpv / dof as f64) } else { None };
    let scaled = opts.scale_by_posteriori && s0sq.is_some();
    let scale = if scaled { s0sq.unwrap() } else { 1.0 };
    let k = if scaled { ellipse_scale_estimated_variance(opts.ellipse_confidence, dof as f64) } else { ellipse_scale_known_variance(opts.ellipse_confidence) };
    let mut cov = sol.qx.clone();
    cov.data.iter_mut().for_each(|v| *v *= scale);
    let mut points = Vec::new();
    let mut index = Vec::new();
    for p in &pts {
        let e = p.ie.map(|i| sol.x[i]).unwrap_or(p.e0);
        let n = p.inn.map(|i| sol.x[i]).unwrap_or(p.n0);
        let c = |i: Option<usize>, j: Option<usize>| match (i, j) {
            (Some(i), Some(j)) => cov.get(i, j),
            _ => 0.0,
        };
        let (ve, vn, ven) = (c(p.ie, p.ie), c(p.inn, p.inn), c(p.ie, p.inn));
        points.push(AdjustedPoint {
            name: p.name.clone(),
            e,
            n,
            sigma_e: ve.max(0.0).sqrt(),
            sigma_n: vn.max(0.0).sqrt(),
            covariance_en: ven,
            ellipse: ellipse_from(ve, vn, ven, k, opts.ellipse_confidence),
            fixed: p.fixed,
            shift: (e - p.e0, n - p.n0),
        });
        index.push((p.name.clone(), p.ie, p.inn));
    }
    let orientations = omegas
        .iter()
        .map(|(id, st, i)| OrientationResult { setup_id: *id, station: st.clone(), orientation_deg: normalize_deg(sol.x[*i].to_degrees()), sigma_arcsec: cov.get(*i, *i).max(0.0).sqrt() * ARCSEC_PER_RAD })
        .collect();
    let observations: Vec<ObsResult> = obs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let ang = o.kind.is_angular();
            let u = if ang { ARCSEC_PER_RAD } else { 1.0 };
            let adj = o.value + sol.v[i];
            let w = sol.w[i];
            ObsResult {
                key: o.key.clone(),
                kind: o.kind,
                from: o.from.clone(),
                to: o.to.clone(),
                observed: if ang { o.value.to_degrees() } else { o.value },
                adjusted: if ang { adj.to_degrees() } else { adj },
                residual: sol.v[i] * u,
                sigma: o.sigma * u,
                sigma_residual: sol.sigma_v[i] * u,
                redundancy: sol.redundancy[i],
                std_residual: w,
                flagged: w.map(|w| w.abs() > opts.blunder_threshold).unwrap_or(false),
            }
        })
        .collect();
    let global_test = if dof > 0 {
        let d = dof as f64;
        let lo = chi2_quantile(opts.global_alpha / 2.0, d);
        let hi = chi2_quantile(1.0 - opts.global_alpha / 2.0, d);
        Some(GlobalTest {
            statistic: sol.vtpv,
            dof: dof as usize,
            alpha: opts.global_alpha,
            lower_critical: lo,
            upper_critical: hi,
            one_sided_critical: chi2_quantile(1.0 - opts.global_alpha, d),
            passes: sol.vtpv >= lo && sol.vtpv <= hi,
            exceeds_one_sided: sol.vtpv > chi2_quantile(1.0 - opts.global_alpha, d),
        })
    } else {
        None
    };
    Ok(Adjustment {
        network: network.into(),
        iterations: sol.iterations,
        converged: sol.converged,
        last_step_norm: sol.last_step_norm,
        n_observations: obs.len(),
        n_unknowns: x0.len(),
        dof,
        vtpv: sol.vtpv,
        sigma0_sq_posteriori: s0sq,
        global_test,
        points,
        orientations,
        observations,
        scaled_by_posteriori: scaled,
        ellipse_scale: k,
        ellipse_confidence: opts.ellipse_confidence,
        blunder_threshold: opts.blunder_threshold,
        covariance: cov,
        index,
    })
}

/// Result of data snooping: observations are removed one at a time, the one with the largest |w| above
/// the threshold first, and the network is adjusted again, until no observation is flagged or
/// `max_removals` is reached or the next removal would make the adjustment fail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnoopResult {
    pub adjustment: Adjustment,
    /// (key, w statistic at removal) in the order removed.
    pub removed: Vec<(String, f64)>,
    pub stopped_reason: String,
}

/// One-at-a-time data snooping (Teunissen 2024, section 3.4.2, "data snooping": the observation with the
/// largest |w_i| above the critical value is the suspected blunder). Removing it and repeating is the
/// usual practice; the rule and its threshold (`opts.blunder_threshold`) are stated, not hidden.
pub fn adjust_with_snooping(db: &SurveyDatabase, network: &str, opts: &AdjustOptions, max_removals: usize) -> Result<SnoopResult> {
    let mut o = opts.clone();
    let mut removed = Vec::new();
    let mut adj = adjust_network_2d(db, network, &o)?;
    loop {
        let worst = adj.observations.iter().filter(|x| x.flagged).max_by(|a, b| a.std_residual.unwrap().abs().partial_cmp(&b.std_residual.unwrap().abs()).unwrap());
        let Some(w) = worst else {
            return Ok(SnoopResult { adjustment: adj, removed, stopped_reason: "no observation exceeds the threshold".into() });
        };
        if removed.len() >= max_removals {
            return Ok(SnoopResult { adjustment: adj, removed, stopped_reason: "maximum number of removals reached".into() });
        }
        let (key, wv) = (w.key.clone(), w.std_residual.unwrap());
        let mut trial = o.clone();
        trial.excluded.push(key.clone());
        match adjust_network_2d(db, network, &trial) {
            Ok(next) => {
                removed.push((key, wv));
                o = trial;
                adj = next;
            }
            Err(e) => {
                return Ok(SnoopResult { adjustment: adj, removed, stopped_reason: format!("removing {key} fails: {e}") });
            }
        }
    }
}
