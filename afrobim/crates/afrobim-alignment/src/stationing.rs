//! Station equations: the station shown on drawings where it departs from the distance along
//! the alignment.
//!
//! Two kinds of station:
//!
//! - the **internal** station is the alignment's own: its start station plus the distance along
//!   it, continuous from end to end. All geometry, the profile and the superelevation use it.
//! - the **displayed** station is what the drawings and the schedule say. Without station
//!   equations the two are the same.
//!
//! A [`StationEquation`] sits at an internal station. Arriving there the displayed station is the
//! *back* station; leaving it, the displayed station restarts at the *ahead* station and then
//! runs up (or down, for a decreasing equation) metre for metre. The equations cut the alignment
//! into **regions**: region 0 before the first equation, region `i` after equation `i − 1`. In
//! region 0 the displayed station is the internal station.
//!
//! - Ahead greater than back leaves a **gap**: displayed stations between them are nowhere on the
//!   alignment.
//! - Ahead less than back makes an **overlap**: a displayed station there occurs in two regions.
//!   Such a station is ambiguous. [`StationEquations::to_internal`] refuses it with
//!   [`StationEquationError::Ambiguous`], which lists every candidate;
//!   [`StationEquations::to_internal_in_region`] takes the region and so has one answer;
//!   [`StationEquations::candidates`] returns them all.
//!
//! A displayed station on its own therefore does not always name a place: carry the region with
//! it ([`DisplayedStation`]) wherever an alignment has an overlap.

use serde::{Deserialize, Serialize};

use crate::geometry::format_station;

/// Whether displayed stations run up or down after an equation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StationDirection {
    #[default]
    Increasing,
    Decreasing,
}

/// One station equation, in metres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StationEquation {
    /// Where it is: the internal station.
    pub internal_m: f64,
    /// The displayed station arriving at it. It follows from the equations before this one, so it
    /// may be left out; when given it must agree with them to 1 mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_m: Option<f64>,
    /// The displayed station leaving it.
    pub ahead_m: f64,
    #[serde(default)]
    pub direction: StationDirection,
}

impl StationEquation {
    /// An increasing equation at `internal_m` where the displayed station becomes `ahead_m`.
    pub fn new(internal_m: f64, ahead_m: f64) -> Self {
        StationEquation { internal_m, back_m: None, ahead_m, direction: StationDirection::Increasing }
    }
}

/// A displayed station and the region it is in (0 before the first equation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayedStation {
    pub station_m: f64,
    pub region: usize,
}

/// One place a displayed station can mean: the region and the internal station there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StationCandidate {
    pub region: usize,
    pub internal_m: f64,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StationEquationError {
    #[error("station equation {index} has a value that is not a finite number")]
    NotFinite { index: usize },
    #[error("station equation {index} is not at a higher internal station than the one before it")]
    NotIncreasing { index: usize },
    #[error("station equation {index}: back station {} does not agree with the stationing before it, which arrives at {}", format_station(*back_m, 3), format_station(*expected_m, 3))]
    BackMismatch { index: usize, back_m: f64, expected_m: f64 },
    #[error("there is no station region {region}: the alignment has regions 0 to {last}")]
    NoSuchRegion { region: usize, last: usize },
    #[error("station {} is not in region {region}, which runs from {} to {}", format_station(*displayed_m, 3), format_station(*from_m, 3), format_station(*to_m, 3))]
    NotInRegion { displayed_m: f64, region: usize, from_m: f64, to_m: f64 },
    #[error("station {} is not on the alignment: it is beyond its ends or in a gap between station equations", format_station(*displayed_m, 3))]
    NotOnAlignment { displayed_m: f64 },
    #[error("station {} occurs {} times on the alignment (regions {}); say which region is meant", format_station(*displayed_m, 3), candidates.len(), regions(candidates))]
    Ambiguous { displayed_m: f64, candidates: Vec<StationCandidate> },
}

fn regions(candidates: &[StationCandidate]) -> String {
    candidates.iter().map(|c| c.region.to_string()).collect::<Vec<_>>().join(", ")
}

/// The station equations of an alignment, in order along it. Empty means none.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StationEquations {
    pub equations: Vec<StationEquation>,
}

/// A stated back station may differ from the derived one by this much.
const BACK_TOLERANCE_M: f64 = 1e-3;
const GRACE_M: f64 = 1e-9;

impl StationEquations {
    /// Validated equations. See [`StationEquations::validate`].
    pub fn new(equations: Vec<StationEquation>) -> Result<StationEquations, StationEquationError> {
        let e = StationEquations { equations };
        e.validate()?;
        Ok(e)
    }

    /// Checks that every number is finite, that the equations are in strictly increasing order of
    /// internal station, and that each stated back station agrees with the stationing before it.
    pub fn validate(&self) -> Result<(), StationEquationError> {
        for (index, e) in self.equations.iter().enumerate() {
            if !(e.internal_m.is_finite() && e.ahead_m.is_finite() && e.back_m.is_none_or(f64::is_finite)) {
                return Err(StationEquationError::NotFinite { index });
            }
            if index > 0 && e.internal_m <= self.equations[index - 1].internal_m {
                return Err(StationEquationError::NotIncreasing { index });
            }
            if let Some(back_m) = e.back_m {
                let expected_m = self.displayed_in(index, e.internal_m);
                if (back_m - expected_m).abs() > BACK_TOLERANCE_M {
                    return Err(StationEquationError::BackMismatch { index, back_m, expected_m });
                }
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.equations.is_empty()
    }

    /// The number of regions: one more than the number of equations.
    pub fn region_count(&self) -> usize {
        self.equations.len() + 1
    }

    /// The region an internal station is in. A station exactly on an equation is in the region
    /// ahead of it.
    pub fn region_of(&self, internal_m: f64) -> usize {
        self.equations.iter().take_while(|e| e.internal_m <= internal_m).count()
    }

    /// The displayed station arriving at equation `index`, worked out from the equations before it.
    pub fn back_station(&self, index: usize) -> Option<f64> {
        self.equations.get(index).map(|e| self.displayed_in(index, e.internal_m))
    }

    /// The displayed station of an internal station, and its region. A station exactly on an
    /// equation gets the ahead station. The alignment's ends are not known here: see
    /// [`crate::Alignment::displayed_station`].
    pub fn to_displayed(&self, internal_m: f64) -> DisplayedStation {
        let region = self.region_of(internal_m);
        DisplayedStation { station_m: self.displayed_in(region, internal_m), region }
    }

    /// The displayed station of `internal_m` by the stationing of `region`, whether or not the
    /// station is in that region.
    fn displayed_in(&self, region: usize, internal_m: f64) -> f64 {
        match region.checked_sub(1).and_then(|i| self.equations.get(i)) {
            None => internal_m,
            Some(e) => match e.direction {
                StationDirection::Increasing => e.ahead_m + (internal_m - e.internal_m),
                StationDirection::Decreasing => e.ahead_m - (internal_m - e.internal_m),
            },
        }
    }

    fn internal_in(&self, region: usize, displayed_m: f64) -> f64 {
        match region.checked_sub(1).and_then(|i| self.equations.get(i)) {
            None => displayed_m,
            Some(e) => match e.direction {
                StationDirection::Increasing => e.internal_m + (displayed_m - e.ahead_m),
                StationDirection::Decreasing => e.internal_m + (e.ahead_m - displayed_m),
            },
        }
    }

    /// The internal stations a region covers on an alignment running from `start_m` to `end_m`
    /// (internal); `None` when the region lies wholly off the alignment.
    fn region_span(&self, region: usize, start_m: f64, end_m: f64) -> Option<(f64, f64)> {
        let lo = region.checked_sub(1).map_or(start_m, |i| self.equations[i].internal_m.max(start_m));
        let hi = self.equations.get(region).map_or(end_m, |e| e.internal_m.min(end_m));
        (lo <= hi).then_some((lo, hi))
    }

    /// The internal station of a displayed station in a stated region, on an alignment whose
    /// internal stations run from `start_m` to `end_m`. Both ends of a region belong to it, so
    /// the back station of an equation is found in the region before it and the ahead station in
    /// the region after.
    pub fn to_internal_in_region(&self, displayed_m: f64, region: usize, start_m: f64, end_m: f64) -> Result<f64, StationEquationError> {
        if region >= self.region_count() {
            return Err(StationEquationError::NoSuchRegion { region, last: self.region_count() - 1 });
        }
        let Some((lo, hi)) = self.region_span(region, start_m, end_m) else {
            return Err(StationEquationError::NotOnAlignment { displayed_m });
        };
        let internal = self.internal_in(region, displayed_m);
        if internal >= lo - GRACE_M && internal <= hi + GRACE_M {
            Ok(internal.clamp(lo, hi))
        } else {
            Err(StationEquationError::NotInRegion {
                displayed_m,
                region,
                from_m: self.displayed_in(region, lo),
                to_m: self.displayed_in(region, hi),
            })
        }
    }

    /// Every place a displayed station occurs on an alignment whose internal stations run from
    /// `start_m` to `end_m`, in order along it: none in a gap or off the ends, two or more in an
    /// overlap. The same point reached from both sides of an equation (back equal to ahead) is
    /// given once, in the region ahead.
    pub fn candidates(&self, displayed_m: f64, start_m: f64, end_m: f64) -> Vec<StationCandidate> {
        let mut out: Vec<StationCandidate> = Vec::new();
        for region in 0..self.region_count() {
            if let Ok(internal_m) = self.to_internal_in_region(displayed_m, region, start_m, end_m) {
                if out.last().is_some_and(|c| (c.internal_m - internal_m).abs() <= GRACE_M) {
                    out.pop();
                }
                out.push(StationCandidate { region, internal_m });
            }
        }
        out
    }

    /// The internal station of a displayed station when it occurs exactly once on the alignment.
    /// [`StationEquationError::Ambiguous`] (with every candidate) when it occurs more than once,
    /// [`StationEquationError::NotOnAlignment`] when it does not occur.
    pub fn to_internal(&self, displayed_m: f64, start_m: f64, end_m: f64) -> Result<f64, StationEquationError> {
        let candidates = self.candidates(displayed_m, start_m, end_m);
        match candidates.as_slice() {
            [] => Err(StationEquationError::NotOnAlignment { displayed_m }),
            [one] => Ok(one.internal_m),
            _ => Err(StationEquationError::Ambiguous { displayed_m, candidates }),
        }
    }

    /// The displayed station of an internal station as text, e.g. `1+250.000`
    /// ([`format_station`]). In an overlap two places share a label; the region tells them apart.
    pub fn label(&self, internal_m: f64, decimals: usize) -> String {
        format_station(self.to_displayed(internal_m).station_m, decimals)
    }
}
