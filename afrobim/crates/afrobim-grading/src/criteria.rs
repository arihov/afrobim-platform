//! Grading criteria: how far and how steeply a projection from the footprint goes.
//!
//! Every number is an input; the crate has no default slope.
//!
//! # Slope format
//!
//! A [`Slope`] is a **magnitude** in one of two formats, always positive:
//!
//! * `Slope::Ratio(h)`: `h` metres horizontal for 1 metre vertical (3 means 3H:1V, a 33.3 % grade).
//! * `Slope::GradePercent(g)`: `g` metres of rise per 100 metres of run (50 means 2H:1V).
//!
//! The direction (rising or falling away from the footprint) is never part of the slope. It is
//! implied by the criterion (cut slopes rise, fill slopes fall) or given as a [`Direction`].
//!
//! # Criteria
//!
//! Let `z0` be the footprint elevation at a ray and `t` the perpendicular distance from the
//! footprint (see the `rays` module for how rays and miters work).
//!
//! * `TargetSurface`: project until the slope surface meets the ground. The side is chosen at the
//!   ray: where the ground at the footprint is above `z0` the **cut slope** rises away from the
//!   footprint, where it is below `z0` the **fill slope** falls away. The daylight is the first
//!   point along the ray where `ground(t) - (z0 + s t) = 0`, found exactly on the piecewise linear
//!   ground (the ray is cut by the triangles it crosses; between cuts both ground and slope are
//!   linear, so the root of each piece is a linear interpolation). `max_distance` limits the
//!   search; a ray that finds no daylight reports it.
//! * `TargetDistance`: stop at plan distance `distance` with the given slope and direction.
//! * `TargetElevation`: stop at elevation `elevation`. If it is above `z0` the **cut slope** is
//!   used (the ground a cut rises into), if below the **fill slope**; `t = |elevation - z0| / s`.
//! * `TargetRelativeElevation`: stop at `z0 + delta` with one slope, `t = |delta| / s`.

use afrobim_corridor::Tin;
use serde::{Deserialize, Serialize};

use crate::error::{finite, GradingError, Result};
use crate::geom::*;

/// A slope magnitude. See the module documentation for the two formats.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slope {
    /// `h` horizontal to 1 vertical.
    Ratio(f64),
    /// Rise per 100 of run, in percent.
    GradePercent(f64),
}

impl Slope {
    /// Rise over run (a fraction, positive).
    pub fn rise_per_run(&self) -> Result<f64> {
        let v = match *self {
            Slope::Ratio(h) => {
                finite("slope ratio", h)?;
                if h <= 0.0 {
                    return Err(GradingError::Invalid("a slope ratio must be positive (horizontal per 1 vertical)".into()));
                }
                1.0 / h
            }
            Slope::GradePercent(g) => {
                finite("slope grade", g)?;
                if g <= 0.0 {
                    return Err(GradingError::Invalid("a slope grade must be positive (the direction is given separately)".into()));
                }
                g / 100.0
            }
        };
        Ok(v)
    }
}

/// Whether a slope rises or falls away from the footprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Rise,
    Fall,
}

/// A grading criterion. See the module documentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "criterion", rename_all = "snake_case")]
pub enum Criterion {
    TargetSurface { cut_slope: Slope, fill_slope: Slope, max_distance: f64 },
    TargetDistance { distance: f64, slope: Slope, direction: Direction },
    TargetElevation { elevation: f64, cut_slope: Slope, fill_slope: Slope },
    TargetRelativeElevation { delta: f64, slope: Slope },
}

/// Where a ray meets its target: perpendicular distance `t` and elevation `z`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DaylightHit {
    pub t: f64,
    pub z: f64,
}

impl Criterion {
    pub fn validate(&self) -> Result<()> {
        match self {
            Criterion::TargetSurface { cut_slope, fill_slope, max_distance } => {
                cut_slope.rise_per_run()?;
                fill_slope.rise_per_run()?;
                if !(*max_distance > 0.0 && max_distance.is_finite()) {
                    return Err(GradingError::Invalid("max_distance must be positive".into()));
                }
            }
            Criterion::TargetDistance { distance, slope, .. } => {
                slope.rise_per_run()?;
                if !(*distance >= 0.0 && distance.is_finite()) {
                    return Err(GradingError::Invalid("distance must be a non-negative number".into()));
                }
            }
            Criterion::TargetElevation { elevation, cut_slope, fill_slope } => {
                finite("elevation", *elevation)?;
                cut_slope.rise_per_run()?;
                fill_slope.rise_per_run()?;
            }
            Criterion::TargetRelativeElevation { delta, slope } => {
                finite("delta", *delta)?;
                slope.rise_per_run()?;
            }
        }
        Ok(())
    }

    pub fn needs_surface(&self) -> bool {
        matches!(self, Criterion::TargetSurface { .. })
    }

    /// The daylight of one ray: foot point `p` (source coordinates), elevation `z0`, unit plan
    /// direction `dir` and miter factor `miter` (the plan point at perpendicular distance `t` is
    /// `p + dir miter t`). `Ok(None)` means no daylight was found (the ground ends, or no
    /// crossing within the maximum distance).
    pub fn daylight(&self, p: [f64; 2], z0: f64, dir: [f64; 2], miter: f64, ground: Option<&Tin>) -> Result<Option<DaylightHit>> {
        match self {
            Criterion::TargetDistance { distance, slope, direction } => {
                let s = slope.rise_per_run()?;
                let z = match direction {
                    Direction::Rise => z0 + s * distance,
                    Direction::Fall => z0 - s * distance,
                };
                Ok(Some(DaylightHit { t: *distance, z }))
            }
            Criterion::TargetElevation { elevation, cut_slope, fill_slope } => {
                let dz = elevation - z0;
                if dz == 0.0 {
                    return Ok(Some(DaylightHit { t: 0.0, z: *elevation }));
                }
                let s = if dz > 0.0 { cut_slope.rise_per_run()? } else { fill_slope.rise_per_run()? };
                Ok(Some(DaylightHit { t: dz.abs() / s, z: *elevation }))
            }
            Criterion::TargetRelativeElevation { delta, slope } => {
                let s = slope.rise_per_run()?;
                Ok(Some(DaylightHit { t: delta.abs() / s, z: z0 + delta }))
            }
            Criterion::TargetSurface { cut_slope, fill_slope, max_distance } => {
                let tin = ground.ok_or_else(|| GradingError::Invalid("a target surface criterion needs a ground surface".into()))?;
                let Some(g0) = tin.elevation_at(p[0], p[1]) else { return Ok(None) };
                let f0 = g0 - z0;
                if f0.abs() < 1e-9 {
                    return Ok(Some(DaylightHit { t: 0.0, z: z0 }));
                }
                let (s, sign) = if f0 > 0.0 { (cut_slope.rise_per_run()?, 1.0) } else { (-fill_slope.rise_per_run()?, -1.0) };
                let end = add(p, scale(dir, max_distance * miter));
                let runs = tin.section((p[0], p[1]), (end[0], end[1]));
                let Some(run) = runs.iter().find(|r| r.first().map(|q| q.0 < 1e-6).unwrap_or(false)) else { return Ok(None) };
                // f(d) = ground(d) - (z0 + s d / miter); the daylight is the first sign change from f0's side.
                let f = |d: f64, g: f64| g - (z0 + s * d / miter);
                for w in run.windows(2) {
                    let (fa, fb) = (f(w[0].0, w[0].1), f(w[1].0, w[1].1));
                    if sign * fa > 0.0 && sign * fb <= 0.0 {
                        let u = fa / (fa - fb);
                        let d = w[0].0 + u * (w[1].0 - w[0].0);
                        let t = d / miter;
                        return Ok(Some(DaylightHit { t, z: z0 + s * t }));
                    }
                }
                Ok(None)
            }
        }
    }
}
