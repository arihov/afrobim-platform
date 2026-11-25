// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Illuminance (and, with an r-table, luminance) on calculation grids from the luminaires of a
//! lighting element.
//!
//! # What is calculated, and from what
//!
//! All formulae here are first-principles photometry, not rules of a standard. For a point
//! source of intensity `I(C, gamma)` at distance `d` from a point, with `u` the unit vector from
//! the point to the source:
//!
//! - horizontal illuminance `E_h = I * u_z / d^2`. With the luminaire at height `h` above the
//!   plane `u_z = h / d`, which is `E_h = I * cos^3(gamma_w) / h^2` where `gamma_w` is the angle of
//!   the ray from the vertical;
//! - illuminance on a vertical plane with horizontal unit normal `n`: `E_v = I * max(0, u . n) /
//!   d^2`;
//! - semi-cylindrical illuminance, defined here as the mean illuminance over the curved surface
//!   of an infinitesimal vertical half-cylinder whose flat side faces away from a reference
//!   direction `n0` (area weighting uniform in azimuth): with `w` the horizontal vector from the
//!   point to the source and `rho = |w|`, `E_sc = I * (rho + w . n0) / (pi * d^3)`. Derivation:
//!   a surface element with normal at azimuth `psi` receives `I cos(eps) cos(psi - beta) / d^2`
//!   for `|psi - beta| < 90 deg` (`eps` the elevation of the source, `beta` its azimuth); the
//!   mean over `psi` within 90 degrees of `n0` of `max(0, cos(psi - beta))` is `(1 + cos(beta -
//!   psi0)) / pi`. Tested against numerical integration;
//! - luminance `L = I * r / h^2`, with `r` from a user-supplied r-table ([`super::RTable`]).
//!
//! Contributions add. The maintained value is the initial value times the maintenance factor
//! (the user's input; BS 5489-1:2003 clause 6.4.4, p. 12: the maintenance factor is the product of
//! the lamp and luminaire maintenance factors).
//!
//! # Limits
//!
//! The calculation plane is horizontal, at `plane_level_m` (default: the mean column level, zero
//! without a profile); gradient and cross-fall are not applied. Luminaires are points at the
//! mounting height above their column's level, `outreach` along the bracket. There is no
//! obstruction, reflection from surfaces, or inter-reflection. Only luminaires within
//! `contribution_distance_m` in plan of a point contribute to it.

use serde::{Deserialize, Serialize};

use crate::{LightingData, Photometry};

use super::frame::{luminaire_instances, photometric_angles, LuminaireInstance, Orientation};
use super::grid::{build_grid, AreaGrid, AreaKind, CalcArea, Division, GridFrame, GridPoint};
use super::rtable::RTable;
use super::CalcError;

/// Which directions the vertical and semi-cylindrical illuminances face.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Facing {
    /// Both along the alignment: towards up-station and towards down-station.
    AlongAlignment,
    /// These whole-circle bearings, degrees clockwise from north, for the normal of the vertical
    /// plane and the reference direction `n0` of the half-cylinder.
    Bearings(Vec<f64>),
}

/// Vertical and semi-cylindrical illuminance at one height.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerticalSettings {
    /// Height above the calculation plane, m (the user's choice).
    pub height_m: f64,
    pub facing: Facing,
}

/// Where an observer stands (for luminance).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ObserverPosition {
    Plan { east_m: f64, north_m: f64 },
    StationOffset { station_m: f64, offset_m: f64 },
}

/// An observer for luminance. Observer positions of the standard are not held, so the user
/// enters them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObserverSpec {
    pub name: String,
    pub position: ObserverPosition,
    /// Eye height above the calculation plane, m. It does not enter the formula for `r`: `beta`
    /// and `tan(gamma)` do not depend on it; it is recorded for the report only.
    pub height_m: f64,
}

/// Everything entered for a calculation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalcSettings {
    /// The maintenance factor, a user input in (0, 1].
    pub maintenance_factor: f64,
    /// Luminaires further than this in plan from a point do not contribute to it, m.
    pub contribution_distance_m: f64,
    #[serde(default)]
    pub orientation: Orientation,
    pub areas: Vec<CalcArea>,
    /// Level of the calculation plane, m. `None`: the mean of the column levels, zero when none.
    #[serde(default)]
    pub plane_level_m: Option<f64>,
    #[serde(default)]
    pub vertical: Option<VerticalSettings>,
    /// Observers for luminance; used only when an r-table is passed.
    #[serde(default)]
    pub observers: Vec<ObserverSpec>,
    /// Width of the edge strips for the edge illuminance ratio, m: one driving lane (handbook p. 8).
    #[serde(default)]
    pub eir_lane_width_m: Option<f64>,
}

/// Average, minimum, maximum and the two uniformities of a set of values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub average: f64,
    pub minimum: f64,
    pub maximum: f64,
    /// Overall uniformity: minimum over average.
    pub uniformity_min_avg: f64,
    /// Minimum over maximum.
    pub uniformity_min_max: f64,
}

impl Stats {
    pub fn of(values: &[f64]) -> Stats {
        let n = values.len().max(1) as f64;
        let average = values.iter().sum::<f64>() / n;
        let minimum = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let maximum = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let ratio = |a: f64, b: f64| if b > 0.0 { a / b } else { 0.0 };
        Stats { average, minimum, maximum, uniformity_min_avg: ratio(minimum, average), uniformity_min_max: ratio(minimum, maximum) }
    }
}

/// Vertical and semi-cylindrical illuminance for one facing direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectionResult {
    pub label: String,
    /// Whole-circle bearing of the facing direction, degrees.
    pub bearing_deg: f64,
    pub ev_lx: Vec<f64>,
    pub esc_lx: Vec<f64>,
    pub ev: Stats,
    pub esc: Stats,
}

/// Luminance for one observer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LuminanceResult {
    pub observer: String,
    pub l_cd_m2: Vec<f64>,
    pub stats: Stats,
    /// Luminaire contributions that fell beyond the last row of the r-table and were taken as zero.
    pub outside_rtable: usize,
}

/// Results for one area. Values are maintained values (initial times the maintenance factor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AreaResult {
    pub name: String,
    pub kind: AreaKind,
    pub n_along: usize,
    pub n_across: usize,
    pub points: Vec<GridPoint>,
    /// Maintained horizontal illuminance at each point, lx (index as in [`AreaGrid`]).
    pub e_h_lx: Vec<f64>,
    pub e_h: Stats,
    pub vertical: Vec<DirectionResult>,
    pub luminance: Vec<LuminanceResult>,
}

/// The edge illuminance ratio (handbook p. 8): average horizontal illuminance on a strip just
/// outside the carriageway edge over that on a strip inside the edge of the same width, per side;
/// the minimum of the two sides is the value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EirResult {
    pub lane_width_m: f64,
    pub left: f64,
    pub right: f64,
    pub value: f64,
}

/// The result of a calculation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalcResult {
    pub areas: Vec<AreaResult>,
    pub eir: Option<EirResult>,
    pub luminaire_count: usize,
    pub maintenance_factor: f64,
    pub contribution_distance_m: f64,
    pub plane_level_m: f64,
    pub warnings: Vec<String>,
}

impl CalcResult {
    pub fn area(&self, name: &str) -> Option<&AreaResult> {
        self.areas.iter().find(|a| a.name == name)
    }
}

fn bearing_to_dir(bearing_deg: f64) -> [f64; 2] {
    let b = bearing_deg.to_radians();
    [b.sin(), b.cos()]
}

fn dir_to_bearing_deg(heading_rad: f64) -> f64 {
    (90.0 - heading_rad.to_degrees()).rem_euclid(360.0)
}

struct Ctx<'a> {
    phot: &'a Photometry,
    insts: &'a [LuminaireInstance],
    orientation: Orientation,
    distance_m: f64,
    mf: f64,
}

impl Ctx<'_> {
    /// Calls `f(inst, intensity_cd, vector_from_point_to_luminaire, distance)` for each luminaire
    /// within the contribution distance of the point.
    fn each(&self, p: [f64; 3], mut f: impl FnMut(&LuminaireInstance, f64, [f64; 3], f64)) {
        for inst in self.insts {
            let (dx, dy) = (inst.x_m - p[0], inst.y_m - p[1]);
            if dx * dx + dy * dy > self.distance_m * self.distance_m {
                continue;
            }
            let dz = inst.z_m - p[2];
            let d = (dx * dx + dy * dy + dz * dz).sqrt();
            if d < 1e-9 {
                continue;
            }
            // Direction from the luminaire to the point is the negative of the vector.
            let to_point = [-dx / d, -dy / d, -dz / d];
            let (c, g) = photometric_angles(inst, &self.orientation, self.phot.measured_tilt_deg, to_point);
            let i = self.phot.intensity(c, g);
            if i > 0.0 {
                f(inst, i, [dx, dy, dz], d);
            }
        }
    }

    fn e_h(&self, p: [f64; 3]) -> f64 {
        let mut e = 0.0;
        self.each(p, |_, i, v, d| {
            if v[2] > 0.0 {
                e += i * v[2] / (d * d * d);
            }
        });
        e * self.mf
    }

    fn e_v(&self, p: [f64; 3], n: [f64; 2]) -> f64 {
        let mut e = 0.0;
        self.each(p, |_, i, v, d| {
            let cos = (v[0] * n[0] + v[1] * n[1]) / d;
            if cos > 0.0 {
                e += i * cos / (d * d);
            }
        });
        e * self.mf
    }

    fn e_sc(&self, p: [f64; 3], n0: [f64; 2]) -> f64 {
        let mut e = 0.0;
        self.each(p, |_, i, v, d| {
            let rho = (v[0] * v[0] + v[1] * v[1]).sqrt();
            let term = rho + v[0] * n0[0] + v[1] * n0[1];
            if term > 0.0 {
                e += i * term / (std::f64::consts::PI * d * d * d);
            }
        });
        e * self.mf
    }

    fn luminance(&self, p: [f64; 3], observer: [f64; 2], table: &RTable, outside: &mut usize) -> f64 {
        let mut l = 0.0;
        self.each(p, |_, i, v, _| {
            let h = v[2];
            if h <= 0.0 {
                return;
            }
            let rho = (v[0] * v[0] + v[1] * v[1]).sqrt();
            let (ox, oy) = (observer[0] - p[0], observer[1] - p[1]);
            let on = (ox * ox + oy * oy).sqrt();
            // beta: the angle between the vertical planes through the point and the luminaire, and
            // through the point and the observer, 0 to 180 degrees.
            let beta = if rho < 1e-9 || on < 1e-9 { 0.0 } else { ((v[0] * ox + v[1] * oy) / (rho * on)).clamp(-1.0, 1.0).acos().to_degrees() };
            match table.r(beta, rho / h) {
                Some(r) => l += i * r / (h * h),
                None => *outside += 1,
            }
        });
        l * self.mf
    }
}

fn validate(settings: &CalcSettings) -> Result<(), CalcError> {
    let bad = |m: &str| Err(CalcError::InvalidSettings(m.to_string()));
    if !(settings.maintenance_factor.is_finite() && settings.maintenance_factor > 0.0 && settings.maintenance_factor <= 1.0) {
        return bad("the maintenance factor must be greater than 0 and not greater than 1");
    }
    if !(settings.contribution_distance_m.is_finite() && settings.contribution_distance_m > 0.0) {
        return bad("the contribution distance must be greater than zero");
    }
    if settings.areas.is_empty() {
        return bad("no calculation area is given");
    }
    if let Some(v) = &settings.vertical {
        if !(v.height_m.is_finite() && v.height_m >= 0.0) {
            return bad("the height of the vertical illuminance must be a number not less than zero");
        }
    }
    if let Some(w) = settings.eir_lane_width_m {
        if !(w.is_finite() && w > 0.0) {
            return bad("the edge strip width must be greater than zero");
        }
    }
    Ok(())
}

/// The strips of the edge illuminance ratio for `carriageway`.
fn eir_areas(carriageway: &CalcArea, n_across_cells: usize, lane_width_m: f64) -> Result<Vec<CalcArea>, CalcError> {
    let width = carriageway.offset_to_m - carriageway.offset_from_m;
    if 2.0 * lane_width_m > width + 1e-9 {
        return Err(CalcError::InvalidSettings(format!(
            "the edge strip width ({lane_width_m} m) is more than half the carriageway width ({width} m): the inner strips would overlap"
        )));
    }
    let spacing = width / n_across_cells as f64;
    let n = ((lane_width_m / spacing - 1e-9).ceil() as usize).max(1);
    let make = |name: &str, from: f64, to: f64| CalcArea {
        name: name.to_string(),
        kind: AreaKind::EdgeStrip,
        station_from_m: carriageway.station_from_m,
        station_to_m: carriageway.station_to_m,
        offset_from_m: from,
        offset_to_m: to,
        along: carriageway.along,
        across: Division::Count(n),
    };
    let (a, b) = (carriageway.offset_from_m, carriageway.offset_to_m);
    Ok(vec![
        make("EIR outer left", a - lane_width_m, a),
        make("EIR inner left", a, a + lane_width_m),
        make("EIR inner right", b - lane_width_m, b),
        make("EIR outer right", b, b + lane_width_m),
    ])
}

/// Runs the calculation. `rtable` together with `settings.observers` gives luminance on the
/// carriageway areas; without an r-table no luminance is calculated.
pub fn calculate(phot: &Photometry, data: &LightingData, frame: &GridFrame, settings: &CalcSettings, rtable: Option<&RTable>) -> Result<CalcResult, CalcError> {
    validate(settings)?;
    let insts = luminaire_instances(data);
    if insts.is_empty() {
        return Err(CalcError::InvalidSettings("the element has no luminaires".to_string()));
    }
    let mut warnings: Vec<String> = Vec::new();
    let levels: Vec<f64> = data.layout.columns.iter().filter_map(|c| c.level_m).collect();
    let plane = settings.plane_level_m.unwrap_or_else(|| if levels.is_empty() { 0.0 } else { levels.iter().sum::<f64>() / levels.len() as f64 });
    if !levels.is_empty() && levels.len() != data.layout.columns.len() {
        warnings.push("Some columns have no profile level; they were taken at level zero.".to_string());
    }
    if settings.observers.is_empty() != rtable.is_none() {
        if rtable.is_some() {
            warnings.push("An r-table was given but no observer, so no luminance was calculated.".to_string());
        } else {
            return Err(CalcError::InvalidSettings("observers are given but no r-table: luminance needs an r-table file".to_string()));
        }
    }
    if let Some(t) = rtable {
        if t.missing_cells > 0 {
            warnings.push(format!("The r-table has {} cells marked '-', taken as zero.", t.missing_cells));
        }
    }
    let ctx = Ctx { phot, insts: &insts, orientation: settings.orientation, distance_m: settings.contribution_distance_m, mf: settings.maintenance_factor };

    // Areas: the entered ones, then the edge strips.
    let mut areas: Vec<CalcArea> = settings.areas.clone();
    if let Some(w) = settings.eir_lane_width_m {
        let carriageway = settings
            .areas
            .iter()
            .find(|a| a.kind == AreaKind::Carriageway)
            .ok_or_else(|| CalcError::InvalidSettings("an edge strip width is given but no area is a carriageway".to_string()))?;
        let cells = carriageway.across.cells(carriageway.offset_to_m - carriageway.offset_from_m, "across the alignment")?;
        areas.extend(eir_areas(carriageway, cells, w)?);
    }

    let mut out: Vec<AreaResult> = Vec::new();
    for area in &areas {
        let grid: AreaGrid = build_grid(frame, area)?;
        let e_h: Vec<f64> = grid.points.iter().map(|p| ctx.e_h([p.east_m, p.north_m, plane])).collect();
        let mut vertical = Vec::new();
        if let (Some(v), true) = (&settings.vertical, area.kind != AreaKind::EdgeStrip) {
            let mid = 0.5 * (area.station_from_m + area.station_to_m);
            let directions: Vec<(String, f64)> = match &v.facing {
                Facing::AlongAlignment => {
                    let h = frame.heading_at(mid).ok_or_else(|| CalcError::InvalidSettings("the area is off the alignment".to_string()))?;
                    vec![("towards up-station".to_string(), dir_to_bearing_deg(h)), ("towards down-station".to_string(), dir_to_bearing_deg(h + std::f64::consts::PI))]
                }
                Facing::Bearings(bs) => bs.iter().map(|b| (format!("bearing {:.1}", b.rem_euclid(360.0)), b.rem_euclid(360.0))).collect(),
            };
            for (label, bearing) in directions {
                let n = bearing_to_dir(bearing);
                let z = plane + v.height_m;
                let ev: Vec<f64> = grid.points.iter().map(|p| ctx.e_v([p.east_m, p.north_m, z], n)).collect();
                let esc: Vec<f64> = grid.points.iter().map(|p| ctx.e_sc([p.east_m, p.north_m, z], n)).collect();
                vertical.push(DirectionResult { label, bearing_deg: bearing, ev: Stats::of(&ev), esc: Stats::of(&esc), ev_lx: ev, esc_lx: esc });
            }
        }
        let mut luminance = Vec::new();
        if let (Some(table), AreaKind::Carriageway) = (rtable, area.kind) {
            for obs in &settings.observers {
                let (ox, oy) = match &obs.position {
                    ObserverPosition::Plan { east_m, north_m } => (*east_m, *north_m),
                    ObserverPosition::StationOffset { station_m, offset_m } => frame
                        .locate(*station_m, *offset_m)
                        .ok_or_else(|| CalcError::InvalidSettings(format!("observer '{}' is off the alignment", obs.name)))?,
                };
                let mut outside = 0usize;
                let l: Vec<f64> = grid.points.iter().map(|p| ctx.luminance([p.east_m, p.north_m, plane], [ox, oy], table, &mut outside)).collect();
                if outside > 0 {
                    warnings.push(format!("Observer '{}', area '{}': {outside} luminaire contributions lay beyond the r-table's last tan(gamma) and were taken as zero.", obs.name, area.name));
                }
                luminance.push(LuminanceResult { observer: obs.name.clone(), stats: Stats::of(&l), l_cd_m2: l, outside_rtable: outside });
            }
        }
        out.push(AreaResult { name: area.name.clone(), kind: area.kind, n_along: grid.n_along, n_across: grid.n_across, points: grid.points, e_h: Stats::of(&e_h), e_h_lx: e_h, vertical, luminance });
    }

    let eir = settings.eir_lane_width_m.and_then(|w| {
        let avg = |n: &str| out.iter().find(|a| a.name == n).map(|a| a.e_h.average);
        let side = |outer: &str, inner: &str| match (avg(outer), avg(inner)) {
            (Some(o), Some(i)) if i > 0.0 => Some(o / i),
            _ => None,
        };
        let (left, right) = (side("EIR outer left", "EIR inner left")?, side("EIR outer right", "EIR inner right")?);
        Some(EirResult { lane_width_m: w, left, right, value: left.min(right) })
    });
    if settings.eir_lane_width_m.is_some() && eir.is_none() {
        warnings.push("The edge illuminance ratio could not be formed: an inner strip has no light.".to_string());
    }
    let in_range = insts
        .iter()
        .filter(|l| out.iter().any(|a| a.points.iter().any(|p| (p.east_m - l.x_m).powi(2) + (p.north_m - l.y_m).powi(2) <= settings.contribution_distance_m.powi(2))))
        .count();
    if in_range < insts.len() {
        warnings.push(format!("{} of {} luminaires lie beyond the contribution distance of every point and do not contribute.", insts.len() - in_range, insts.len()));
    }
    Ok(CalcResult { areas: out, eir, luminaire_count: insts.len(), maintenance_factor: settings.maintenance_factor, contribution_distance_m: settings.contribution_distance_m, plane_level_m: plane, warnings })
}
