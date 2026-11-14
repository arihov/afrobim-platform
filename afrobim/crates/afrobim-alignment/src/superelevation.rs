//! Superelevation as stored data: cross-fall on each side of the centreline at stated stations.
//!
//! **This is data handling only.** Nothing here works out a superelevation rate from a speed and
//! a radius, and there is no table of design rates, maximum rates or runoff lengths. A value that
//! is not in the data is absent (`None`), never assumed: in particular a normal crown is not
//! assumed anywhere.
//!
//! Conventions:
//!
//! - Stations are **internal** stations in metres.
//! - A cross-fall is in **percent**, one for each side of the centreline (left and right looking
//!   up-station), **positive where the surface rises away from the centreline** and negative
//!   where it falls. A crowned road is negative on both sides; a superelevated left-hand curve
//!   (surface sloping towards the inside, the left) is negative on the left and positive on the
//!   right.
//! - The data is held in **sections**. Inside a section each side is interpolated linearly
//!   between the two stated stations around the station asked for, and only when both carry a
//!   value for that side. Between sections, and outside them, nothing is known.

use serde::{Deserialize, Serialize};

/// Which of the critical stations of a superelevation transition a point is, when the source
/// says. The names are those of LandXML 1.2, whose schema describes them as noted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriticalStation {
    /// `BeginRunoutSta`: start of tangent runout entering the curve (normal crown).
    BeginRunout,
    /// `BeginRunoffSta`: end of tangent runout and start of runoff entering the curve (half crown).
    BeginRunoff,
    /// `FullSuperSta`: where full superelevation begins.
    FullSuper,
    /// `RunoffSta`: end of full superelevation and start of runoff leaving the curve.
    Runoff,
    /// `StartofRunoutSta`: end of runoff and start of tangent runout leaving the curve (half crown).
    StartOfRunout,
    /// `EndofRunoutSta`: end of tangent runout leaving the curve (full crown).
    EndOfRunout,
}

/// The cross-fall stated at one station. A side that the source does not give is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CrossFallPoint {
    pub station_m: f64,
    /// Left side, percent, positive rising away from the centreline.
    #[serde(default)]
    pub left_pct: Option<f64>,
    /// Right side, percent, positive rising away from the centreline.
    #[serde(default)]
    pub right_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<CriticalStation>,
}

impl CrossFallPoint {
    pub fn new(station_m: f64, left_pct: Option<f64>, right_pct: Option<f64>) -> Self {
        CrossFallPoint { station_m, left_pct, right_pct, kind: None }
    }
}

/// A run of stated stations between which the cross-fall may be interpolated.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SuperelevationSection {
    pub points: Vec<CrossFallPoint>,
}

/// The cross-fall on each side at a station, percent, positive rising away from the centreline.
/// `None` where the data does not say.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CrossFall {
    pub left_pct: Option<f64>,
    pub right_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SuperelevationError {
    #[error("superelevation section {section} has no stations")]
    EmptySection { section: usize },
    #[error("superelevation section {section}, station {index}: {message}")]
    BadPoint { section: usize, index: usize, message: String },
    #[error("superelevation section {section}: station {index} is not at a higher station than the one before it")]
    StationsNotIncreasing { section: usize, index: usize },
    #[error("superelevation section {section} starts before section {previous} ends")]
    SectionsOverlap { previous: usize, section: usize },
}

/// Stored cross-fall along an alignment. See the module documentation for the conventions.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Superelevation {
    pub sections: Vec<SuperelevationSection>,
}

impl Superelevation {
    /// Validated superelevation data. See [`Superelevation::validate`].
    pub fn new(sections: Vec<SuperelevationSection>) -> Result<Superelevation, SuperelevationError> {
        let s = Superelevation { sections };
        s.validate()?;
        Ok(s)
    }

    /// One section holding all the points.
    pub fn from_points(points: Vec<CrossFallPoint>) -> Result<Superelevation, SuperelevationError> {
        Superelevation::new(vec![SuperelevationSection { points }])
    }

    /// Checks that every section has at least one station, every number is finite, stations
    /// increase strictly inside a section, and sections follow one another without overlapping
    /// (they may share an end station).
    pub fn validate(&self) -> Result<(), SuperelevationError> {
        let mut previous_end: Option<f64> = None;
        for (section, s) in self.sections.iter().enumerate() {
            if s.points.is_empty() {
                return Err(SuperelevationError::EmptySection { section });
            }
            for (index, p) in s.points.iter().enumerate() {
                let finite = p.station_m.is_finite() && p.left_pct.is_none_or(f64::is_finite) && p.right_pct.is_none_or(f64::is_finite);
                if !finite {
                    return Err(SuperelevationError::BadPoint { section, index, message: "a value is not a finite number".into() });
                }
                if index > 0 && p.station_m <= s.points[index - 1].station_m {
                    return Err(SuperelevationError::StationsNotIncreasing { section, index });
                }
            }
            if previous_end.is_some_and(|end| s.points[0].station_m < end) {
                return Err(SuperelevationError::SectionsOverlap { previous: section - 1, section });
            }
            previous_end = Some(s.points[s.points.len() - 1].station_m);
        }
        Ok(())
    }

    /// The first and last stated station; `None` when there is no data.
    pub fn range(&self) -> Option<(f64, f64)> {
        let first = self.sections.first()?.points.first()?.station_m;
        let last = self.sections.last()?.points.last()?.station_m;
        Some((first, last))
    }

    /// The cross-fall at an internal station: for each side, linear interpolation between the
    /// stated stations either side of it in the same section, when both give that side; the
    /// stated value at a stated station; otherwise `None`. Outside every section both sides are
    /// `None`. Where two sections share a station, the later one answers.
    pub fn cross_fall_at(&self, station_m: f64) -> CrossFall {
        for s in self.sections.iter().rev() {
            let (Some(first), Some(last)) = (s.points.first(), s.points.last()) else { continue };
            if !(station_m >= first.station_m && station_m <= last.station_m) {
                continue;
            }
            // The last stated station at or before the one asked for.
            let i = s.points.iter().rposition(|p| p.station_m <= station_m).unwrap_or(0);
            let a = &s.points[i];
            if a.station_m == station_m || i + 1 == s.points.len() {
                return CrossFall { left_pct: a.left_pct, right_pct: a.right_pct };
            }
            let b = &s.points[i + 1];
            let t = (station_m - a.station_m) / (b.station_m - a.station_m);
            let between = |a: Option<f64>, b: Option<f64>| Some(a? + (b? - a?) * t);
            return CrossFall { left_pct: between(a.left_pct, b.left_pct), right_pct: between(a.right_pct, b.right_pct) };
        }
        CrossFall::default()
    }
}
