// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Vertical design: a profile defined by PVIs with parabolic vertical curves.
//!
//! Stations are internal stations (m), elevations metres, grades ratios (m/m, +0.02 is 2 % up
//! in the direction of increasing station).
//!
//! # Formulae
//!
//! At a PVI with grade in `g1` and grade out `g2` let `A = g2 - g1` (a ratio, algebraic; negative
//! for a crest, positive for a sag). A parabolic curve of horizontal length `L` is the curve whose
//! grade changes at the constant rate `r = A / L` per metre, so with `x` measured from the PVC
//!
//! ```text
//!   grade(x) = g1 + A x / L                       (the derivative of the elevation)
//!   z(x)     = z_pvc + g1 x + A x² / (2 L)        (integrating the grade)
//!   z_pvc    = z_pvi - g1 L / 2                   (the curve is symmetric about the PVI)
//! ```
//!
//! **K value.** `K = L / (100 |A|)`, the horizontal metres needed for each 1 % change of grade,
//! so `L = K · 100 |A|`. A curve entered by K is converted to a length at that rule; `K` and
//! `L` are held equivalent and both are reported.
//!
//! **Turning point.** The grade is zero where `g1 + A x / L = 0`, i.e. `x = -g1 L / A`. It lies
//! on the curve when `0 < x < L`, at elevation `z_pvc - g1² L / (2A)` (substituting back). It is
//! a high point on a crest (A < 0) and a low point on a sag (A > 0).
//!
//! **Offset at the PVI.** At `x = L/2` the curve is below or above the PVI by `|A| L / 8`.
//!
//! The profile that results is an `afrobim_alignment::Profile` of `Pvi`s with
//! `VerticalCurve::Parabola`, evaluated by that crate, so levels at any station are exactly what
//! the corridor and every other module will see.

use afrobim_alignment::{Profile, Pvi};
use serde::{Deserialize, Serialize};

use crate::error::{DesignError, Problem};

/// How the curve at a PVI is given.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CurveSpec {
    /// The grades meet at an angle.
    #[default]
    None,
    /// Horizontal length of the parabola, m.
    Length { length_m: f64 },
    /// K value: metres of curve per 1 % change of grade.
    K { k: f64 },
}

/// A PVI as entered.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VPvi {
    pub station_m: f64,
    pub elevation_m: f64,
    #[serde(default)]
    pub curve: CurveSpec,
}

impl VPvi {
    pub fn point(station_m: f64, elevation_m: f64) -> VPvi {
        VPvi { station_m, elevation_m, curve: CurveSpec::None }
    }
    pub fn with_length(station_m: f64, elevation_m: f64, length_m: f64) -> VPvi {
        VPvi { station_m, elevation_m, curve: CurveSpec::Length { length_m } }
    }
    pub fn with_k(station_m: f64, elevation_m: f64, k: f64) -> VPvi {
        VPvi { station_m, elevation_m, curve: CurveSpec::K { k } }
    }
}

/// The inputs of a vertical design.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct VerticalDesign {
    pub name: String,
    pub pvis: Vec<VPvi>,
}

/// Crest or sag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CurveKind {
    Crest,
    Sag,
}

/// A vertical curve, worked out.
#[derive(Debug, Clone, PartialEq)]
pub struct VCurveData {
    pub pvi_index: usize,
    pub kind: CurveKind,
    pub grade_in: f64,
    pub grade_out: f64,
    /// Algebraic difference of grades, ratio.
    pub grade_change: f64,
    pub length_m: f64,
    /// `L / (100 |A|)`, m per percent.
    pub k: f64,
    pub pvi_station_m: f64,
    pub pvi_elevation_m: f64,
    pub pvc_station_m: f64,
    pub pvc_elevation_m: f64,
    pub pvt_station_m: f64,
    pub pvt_elevation_m: f64,
    /// Offset of the curve from the PVI, `|A| L / 8`, m.
    pub offset_at_pvi_m: f64,
    /// The high point of a crest or low point of a sag when it lies on the curve: (station, elevation).
    pub turning_point: Option<(f64, f64)>,
}

/// The straight between two consecutive PVIs.
#[derive(Debug, Clone, PartialEq)]
pub struct GradeData {
    pub from_pvi: usize,
    pub grade: f64,
    pub start_station_m: f64,
    pub end_station_m: f64,
    /// Length of the grade line between the vertical curves at its two ends (the "tangent" in
    /// profile terms), m.
    pub tangent_length_m: f64,
}

/// A solved vertical design.
#[derive(Debug, Clone, PartialEq)]
pub struct VerticalSolution {
    pub profile: Profile,
    pub grades: Vec<GradeData>,
    pub curves: Vec<VCurveData>,
    pub warnings: Vec<String>,
}

impl VerticalSolution {
    /// Level at a station, m. Error outside the first and last PVI.
    pub fn level_at(&self, station_m: f64) -> Result<f64, DesignError> {
        self.profile.elevation_at(station_m).map_err(|e| DesignError::BadEdit(e.to_string()))
    }

    /// Grade at a station.
    pub fn grade_at(&self, station_m: f64) -> Result<f64, DesignError> {
        self.profile.grade_at(station_m).map_err(|e| DesignError::BadEdit(e.to_string()))
    }

    /// High points (crest turning points) as (station, elevation).
    pub fn high_points(&self) -> Vec<(f64, f64)> {
        self.curves.iter().filter(|c| c.kind == CurveKind::Crest).filter_map(|c| c.turning_point).collect()
    }

    /// Low points (sag turning points) as (station, elevation).
    pub fn low_points(&self) -> Vec<(f64, f64)> {
        self.curves.iter().filter(|c| c.kind == CurveKind::Sag).filter_map(|c| c.turning_point).collect()
    }

    /// Greatest absolute grade anywhere on the profile (grade lines and the ends of curves).
    pub fn max_abs_grade(&self) -> f64 {
        let lines = self.grades.iter().map(|g| g.grade.abs());
        let curves = self.curves.iter().flat_map(|c| [c.grade_in.abs(), c.grade_out.abs()]);
        lines.chain(curves).fold(0.0, f64::max)
    }

    /// Total rise and total fall along the profile, m (both positive), exact: the profile is
    /// monotone between PVC, PVT and turning points, which are all used.
    pub fn rise_and_fall(&self) -> (f64, f64) {
        let mut stations: Vec<f64> = self.profile.pvis.iter().map(|p| p.station_m).collect();
        for c in &self.curves {
            stations.extend([c.pvc_station_m, c.pvt_station_m]);
            if let Some((s, _)) = c.turning_point {
                stations.push(s);
            }
        }
        stations.sort_by(f64::total_cmp);
        let (mut rise, mut fall) = (0.0, 0.0);
        let mut last: Option<f64> = None;
        for s in stations {
            if let Ok(z) = self.profile.elevation_at(s) {
                if let Some(l) = last {
                    if z > l {
                        rise += z - l;
                    } else {
                        fall += l - z;
                    }
                }
                last = Some(z);
            }
        }
        (rise, fall)
    }
}

impl VerticalDesign {
    pub fn new(name: &str, pvis: Vec<VPvi>) -> VerticalDesign {
        VerticalDesign { name: name.to_string(), pvis }
    }

    fn grade(&self, i: usize) -> f64 {
        (self.pvis[i + 1].elevation_m - self.pvis[i].elevation_m) / (self.pvis[i + 1].station_m - self.pvis[i].station_m)
    }

    /// Length of the curve at PVI `i`, from its spec; `None` when it has no curve. An error
    /// message when K was entered and the grades are equal.
    fn length_of(&self, i: usize, warnings: &mut Vec<String>) -> Result<Option<f64>, String> {
        if i == 0 || i + 1 == self.pvis.len() {
            return match self.pvis[i].curve {
                CurveSpec::None => Ok(None),
                _ => Err(format!("PVI {i} is the {} point of the profile and cannot carry a vertical curve.", if i == 0 { "first" } else { "last" })),
            };
        }
        let a = self.grade(i) - self.grade(i - 1);
        match self.pvis[i].curve {
            CurveSpec::None => Ok(None),
            CurveSpec::Length { length_m } => {
                if !(length_m.is_finite() && length_m > 0.0) {
                    return Err(format!("PVI {i}: the curve length must be greater than zero (got {length_m} m)."));
                }
                if a.abs() < 1e-12 {
                    warnings.push(format!("PVI {i}: the grades on both sides are equal, so the curve entered there has no effect."));
                    return Ok(None);
                }
                Ok(Some(length_m))
            }
            CurveSpec::K { k } => {
                if !(k.is_finite() && k > 0.0) {
                    return Err(format!("PVI {i}: K must be greater than zero (got {k})."));
                }
                if a.abs() < 1e-12 {
                    warnings.push(format!("PVI {i}: the grades on both sides are equal, so K = {k} gives a curve of zero length."));
                    return Ok(None);
                }
                Ok(Some(k * 100.0 * a.abs()))
            }
        }
    }

    /// Solves the profile, or lists every reason it is impossible in plain words.
    pub fn solve(&self) -> Result<VerticalSolution, DesignError> {
        let n = self.pvis.len();
        let mut problems = Vec::new();
        if n < 2 {
            return Err(DesignError::Impossible(vec![Problem::whole("A profile needs at least two PVIs.")]));
        }
        for (i, p) in self.pvis.iter().enumerate() {
            if !(p.station_m.is_finite() && p.elevation_m.is_finite()) {
                problems.push(Problem::at(i, format!("PVI {i} has a station or elevation that is not a finite number.")));
            }
        }
        if !problems.is_empty() {
            return Err(DesignError::Impossible(problems));
        }
        for i in 1..n {
            if self.pvis[i].station_m <= self.pvis[i - 1].station_m {
                problems.push(Problem::at(i, format!("PVI {i} (station {:.3}) is not beyond PVI {} (station {:.3}); PVIs must run in increasing station order.", self.pvis[i].station_m, i - 1, self.pvis[i - 1].station_m)));
            }
        }
        if !problems.is_empty() {
            return Err(DesignError::Impossible(problems));
        }
        let mut warnings = Vec::new();
        let mut lengths: Vec<Option<f64>> = Vec::with_capacity(n);
        for i in 0..n {
            match self.length_of(i, &mut warnings) {
                Ok(l) => lengths.push(l),
                Err(m) => {
                    problems.push(Problem::at(i, m));
                    lengths.push(None);
                }
            }
        }
        if !problems.is_empty() {
            return Err(DesignError::Impossible(problems));
        }
        // Overlaps: a curve may reach its neighbour's curve or PVI but not beyond.
        let half = |i: usize| lengths[i].map_or(0.0, |l| l / 2.0);
        for i in 0..n - 1 {
            let gap = (self.pvis[i + 1].station_m - half(i + 1)) - (self.pvis[i].station_m + half(i));
            if gap < -1e-6 {
                let d = self.pvis[i + 1].station_m - self.pvis[i].station_m;
                let msg = if i == 0 {
                    format!("The vertical curve at PVI 1 (length {:.1} m) would start {:.3} m before the first PVI. The longest curve that fits there is {:.1} m.", lengths[1].unwrap_or(0.0), -gap, 2.0 * (d - half(2)).max(0.0))
                } else if i + 1 == n - 1 {
                    format!("The vertical curve at PVI {i} (length {:.1} m) would end {:.3} m beyond the last PVI. The longest curve that fits there is {:.1} m.", lengths[i].unwrap_or(0.0), -gap, 2.0 * (d - half(i - 1)).max(0.0))
                } else {
                    format!("The vertical curves at PVI {i} and PVI {} overlap by {:.3} m: their half-lengths ({:.1} m + {:.1} m) are more than the {:.1} m between the PVIs. Shorten one of them.", i + 1, -gap, half(i), half(i + 1), d)
                };
                problems.push(Problem::at(i + (i == 0) as usize, msg));
            }
        }
        if !problems.is_empty() {
            return Err(DesignError::Impossible(problems));
        }

        let pvis: Vec<Pvi> = self
            .pvis
            .iter()
            .enumerate()
            .map(|(i, p)| match lengths[i] {
                Some(l) => Pvi::parabola(p.station_m, p.elevation_m, l),
                None => Pvi::point(p.station_m, p.elevation_m),
            })
            .collect();
        let profile = Profile::new(&self.name, pvis).map_err(|e| DesignError::Impossible(vec![Problem::whole(e.to_string())]))?;

        let mut curves = Vec::new();
        #[allow(clippy::needless_range_loop)]
        for i in 1..n - 1 {
            let Some(l) = lengths[i] else { continue };
            let (g1, g2) = (self.grade(i - 1), self.grade(i));
            let a = g2 - g1;
            let (s, z) = (self.pvis[i].station_m, self.pvis[i].elevation_m);
            let z_pvc = z - g1 * l / 2.0;
            let x_turn = -g1 * l / a;
            let turning_point = (x_turn > 0.0 && x_turn < l).then(|| (s - l / 2.0 + x_turn, z_pvc - g1 * g1 * l / (2.0 * a)));
            curves.push(VCurveData {
                pvi_index: i,
                kind: if a < 0.0 { CurveKind::Crest } else { CurveKind::Sag },
                grade_in: g1,
                grade_out: g2,
                grade_change: a,
                length_m: l,
                k: l / (100.0 * a.abs()),
                pvi_station_m: s,
                pvi_elevation_m: z,
                pvc_station_m: s - l / 2.0,
                pvc_elevation_m: z_pvc,
                pvt_station_m: s + l / 2.0,
                pvt_elevation_m: z + g2 * l / 2.0,
                offset_at_pvi_m: a.abs() * l / 8.0,
                turning_point,
            });
        }
        let grades = (0..n - 1)
            .map(|i| {
                let start = self.pvis[i].station_m + half(i);
                let end = self.pvis[i + 1].station_m - half(i + 1);
                GradeData { from_pvi: i, grade: self.grade(i), start_station_m: start, end_station_m: end, tangent_length_m: (end - start).max(0.0) }
            })
            .collect();
        Ok(VerticalSolution { profile, grades, curves, warnings })
    }

    /// Applies `edit` to a copy; keeps it only if it solves.
    pub fn try_edit(&mut self, edit: impl FnOnce(&mut VerticalDesign) -> Result<(), DesignError>) -> Result<VerticalSolution, DesignError> {
        let mut copy = self.clone();
        edit(&mut copy)?;
        let solution = copy.solve()?;
        *self = copy;
        Ok(solution)
    }

    fn check_index(&self, index: usize) -> Result<(), DesignError> {
        if index < self.pvis.len() {
            Ok(())
        } else {
            Err(DesignError::BadEdit(format!("There is no PVI {index}; the profile has {} PVIs.", self.pvis.len())))
        }
    }

    /// Inserts a PVI as PVI `index`.
    pub fn insert_pvi(&mut self, index: usize, pvi: VPvi) -> Result<VerticalSolution, DesignError> {
        if index > self.pvis.len() {
            return Err(DesignError::BadEdit(format!("Cannot insert at {index}: the profile has {} PVIs.", self.pvis.len())));
        }
        self.try_edit(|d| {
            d.pvis.insert(index, pvi);
            Ok(())
        })
    }

    /// Inserts a PVI at `station_m` on the profile as it is now (no curve), at the profile's own
    /// level there, in the right place in the order. A convenient "click on the profile" operation.
    pub fn insert_pvi_at_station(&mut self, station_m: f64) -> Result<VerticalSolution, DesignError> {
        let now = self.solve()?;
        let z = now.level_at(station_m)?;
        let index = self.pvis.iter().position(|p| p.station_m > station_m).unwrap_or(self.pvis.len());
        self.insert_pvi(index, VPvi::point(station_m, z))
    }

    /// Moves PVI `index` to a new station and elevation, keeping its curve spec.
    pub fn move_pvi(&mut self, index: usize, station_m: f64, elevation_m: f64) -> Result<VerticalSolution, DesignError> {
        self.check_index(index)?;
        self.try_edit(|d| {
            d.pvis[index].station_m = station_m;
            d.pvis[index].elevation_m = elevation_m;
            Ok(())
        })
    }

    /// Deletes PVI `index`.
    pub fn delete_pvi(&mut self, index: usize) -> Result<VerticalSolution, DesignError> {
        self.check_index(index)?;
        self.try_edit(|d| {
            d.pvis.remove(index);
            Ok(())
        })
    }

    /// Sets the curve at PVI `index` to a length in metres.
    pub fn set_curve_length(&mut self, index: usize, length_m: f64) -> Result<VerticalSolution, DesignError> {
        self.set_curve(index, CurveSpec::Length { length_m })
    }

    /// Sets the curve at PVI `index` by K (m per percent).
    pub fn set_curve_k(&mut self, index: usize, k: f64) -> Result<VerticalSolution, DesignError> {
        self.set_curve(index, CurveSpec::K { k })
    }

    /// Sets or removes (`CurveSpec::None`) the curve at PVI `index`.
    pub fn set_curve(&mut self, index: usize, spec: CurveSpec) -> Result<VerticalSolution, DesignError> {
        self.check_index(index)?;
        self.try_edit(|d| {
            d.pvis[index].curve = spec;
            Ok(())
        })
    }
}
