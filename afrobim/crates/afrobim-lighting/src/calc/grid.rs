// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Calculation areas and their grids.
//!
//! The grids are **user-defined**. The calculation grid of BS EN 13201-3 (number and position of
//! points) is not held and no retrievable source states it, so none is offered for roads.
//! The only grid rule in the held documents is for car parks (BS 5489-1:2003 clause 10.7.5,
//! p. 26); see [`car_park_grid_limits`].
//!
//! A grid divides an area into equal cells and puts a point at the centre of each. The average
//! over the points is then the mean over the area. Along runs along the alignment, across
//! across it.

use afrobim_alignment::Alignment;
use serde::{Deserialize, Serialize};

use crate::Layout;

use super::CalcError;

/// How a length is divided into cells.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Division {
    /// This many equal cells (at least one).
    Count(usize),
    /// Equal cells no longer than this spacing, m: `ceil(length / spacing)` cells.
    Spacing(f64),
}

impl Division {
    pub(crate) fn cells(self, length_m: f64, what: &str) -> Result<usize, CalcError> {
        match self {
            Division::Count(n) if n >= 1 => Ok(n),
            Division::Count(_) => Err(CalcError::InvalidSettings(format!("the number of points {what} must be at least 1"))),
            Division::Spacing(d) if d.is_finite() && d > 0.0 => Ok(((length_m / d - 1e-9).ceil() as usize).max(1)),
            Division::Spacing(d) => Err(CalcError::InvalidSettings(format!("the point spacing {what} must be greater than zero (it is {d})"))),
        }
    }
}

/// What an area is. It decides which results feed which class criterion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AreaKind {
    /// The carriageway between its edges (the whole used surface for C classes when it is the
    /// conflict area; luminance is calculated only here).
    Carriageway,
    /// A footway, cycle path or other pedestrian area (P classes).
    Footway,
    /// A conflict area given by the user (C classes).
    Conflict,
    /// Any other area the user wants calculated.
    Other,
    /// One of the strips used for the edge illuminance ratio (generated, not entered).
    EdgeStrip,
}

impl AreaKind {
    pub fn label(self) -> &'static str {
        match self {
            AreaKind::Carriageway => "carriageway",
            AreaKind::Footway => "footway",
            AreaKind::Conflict => "conflict area",
            AreaKind::Other => "other",
            AreaKind::EdgeStrip => "edge strip",
        }
    }
}

/// A rectangle in the alignment's frame: stations `station_from_m` to `station_to_m` along, and
/// offsets `offset_from_m` to `offset_to_m` across (positive to the right looking up-station).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalcArea {
    pub name: String,
    pub kind: AreaKind,
    pub station_from_m: f64,
    pub station_to_m: f64,
    pub offset_from_m: f64,
    pub offset_to_m: f64,
    /// Division along the alignment.
    pub along: Division,
    /// Division across the alignment.
    pub across: Division,
}

impl CalcArea {
    pub(crate) fn check(&self) -> Result<(), CalcError> {
        let bad = |m: String| Err(CalcError::InvalidSettings(format!("area '{}': {m}", self.name)));
        if !(self.station_from_m.is_finite() && self.station_to_m.is_finite() && self.offset_from_m.is_finite() && self.offset_to_m.is_finite()) {
            return bad("stations and offsets must be numbers".to_string());
        }
        if self.station_to_m <= self.station_from_m {
            return bad("the end station must be greater than the start station".to_string());
        }
        if self.offset_to_m <= self.offset_from_m {
            return bad("the offset 'to' must be greater than the offset 'from' (offsets are positive to the right)".to_string());
        }
        Ok(())
    }
}

/// One calculation point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GridPoint {
    /// Index along (0 at the start station) and across (0 at the 'from' offset).
    pub i_along: usize,
    pub i_across: usize,
    pub station_m: f64,
    pub offset_m: f64,
    pub east_m: f64,
    pub north_m: f64,
}

/// How a station and offset become a plan position.
#[derive(Debug, Clone)]
pub enum GridFrame<'a> {
    /// The alignment itself: grid points follow it round curves.
    Alignment(&'a Alignment),
    /// A local straight approximation: the alignment taken as a straight line through
    /// (`origin_east_m`, `origin_north_m`) at station `station0_m` with the heading
    /// `heading_rad` (counter-clockwise from +x). Exact on a straight alignment; on a curve the
    /// grid leaves the carriageway by about `L^2 / (2 R)` over a length `L` (radius `R`), and
    /// [`GridFrame::from_layout`] warns when the heading of the columns changes by more than
    /// [`STRAIGHT_FRAME_HEADING_WARNING_DEG`].
    Straight { origin_east_m: f64, origin_north_m: f64, station0_m: f64, heading_rad: f64 },
}

/// Heading change across the columns (degrees) above which the straight frame is flagged. This
/// is this crate's own threshold, not a standard's.
pub const STRAIGHT_FRAME_HEADING_WARNING_DEG: f64 = 2.0;

impl GridFrame<'static> {
    /// The straight frame of a stored layout: the alignment point of the first column and the
    /// heading there. The second element is a warning when the columns' headings differ by more
    /// than the threshold (the alignment bends, and the straight approximation is poor).
    pub fn from_layout(layout: &Layout) -> Result<(GridFrame<'static>, Option<String>), CalcError> {
        let first = layout.columns.first().ok_or_else(|| CalcError::InvalidSettings("the element has no columns".to_string()))?;
        let h = first.heading_rad;
        // The column stands at offset_m to the right of the alignment: right normal (sin h, -cos h).
        let origin_east_m = first.easting_m - first.offset_m * h.sin();
        let origin_north_m = first.northing_m + first.offset_m * h.cos();
        let spread_deg = layout
            .columns
            .iter()
            .map(|c| {
                let d = (c.heading_rad - h).rem_euclid(std::f64::consts::TAU);
                d.min(std::f64::consts::TAU - d).to_degrees()
            })
            .fold(0.0, f64::max);
        let warning = (spread_deg > STRAIGHT_FRAME_HEADING_WARNING_DEG).then(|| {
            format!(
                "The alignment turns by {spread_deg:.1} degrees along the columns; the straight local frame leaves the carriageway there. Pass the alignment (GridFrame::Alignment) to follow it."
            )
        });
        Ok((GridFrame::Straight { origin_east_m, origin_north_m, station0_m: first.station_m, heading_rad: h }, warning))
    }
}

impl GridFrame<'_> {
    /// Plan position of a station and offset (offset positive to the right), or `None` off the
    /// alignment.
    pub fn locate(&self, station_m: f64, offset_m: f64) -> Option<(f64, f64)> {
        match self {
            GridFrame::Alignment(a) => a.point_at_offset(station_m, offset_m),
            GridFrame::Straight { origin_east_m, origin_north_m, station0_m, heading_rad } => {
                let (s, c) = heading_rad.sin_cos();
                let along = station_m - station0_m;
                Some((origin_east_m + along * c + offset_m * s, origin_north_m + along * s - offset_m * c))
            }
        }
    }

    /// Heading (radians counter-clockwise from +x) at a station.
    pub fn heading_at(&self, station_m: f64) -> Option<f64> {
        match self {
            GridFrame::Alignment(a) => a.point_at(station_m).map(|p| p.heading_rad),
            GridFrame::Straight { heading_rad, .. } => Some(*heading_rad),
        }
    }
}

/// The points of an area: `n_along` by `n_across`, stored along-major (`index = i_along *
/// n_across + i_across`).
#[derive(Debug, Clone, PartialEq)]
pub struct AreaGrid {
    pub n_along: usize,
    pub n_across: usize,
    pub points: Vec<GridPoint>,
}

pub(crate) fn build_grid(frame: &GridFrame, area: &CalcArea) -> Result<AreaGrid, CalcError> {
    area.check()?;
    let n_along = area.along.cells(area.station_to_m - area.station_from_m, "along the alignment")?;
    let n_across = area.across.cells(area.offset_to_m - area.offset_from_m, "across the alignment")?;
    if n_along.saturating_mul(n_across) > 2_000_000 {
        return Err(CalcError::InvalidSettings(format!("area '{}' would have {} points; the limit is 2 000 000", area.name, n_along * n_across)));
    }
    let ds = (area.station_to_m - area.station_from_m) / n_along as f64;
    let dof = (area.offset_to_m - area.offset_from_m) / n_across as f64;
    let mut points = Vec::with_capacity(n_along * n_across);
    for i in 0..n_along {
        let station_m = area.station_from_m + (i as f64 + 0.5) * ds;
        for j in 0..n_across {
            let offset_m = area.offset_from_m + (j as f64 + 0.5) * dof;
            let (east_m, north_m) = frame
                .locate(station_m, offset_m)
                .ok_or_else(|| CalcError::InvalidSettings(format!("area '{}': station {station_m:.3} m is off the alignment", area.name)))?;
            points.push(GridPoint { i_along: i, i_across: j, station_m, offset_m, east_m, north_m });
        }
    }
    Ok(AreaGrid { n_along, n_across, points })
}

/// The only grid rule found in the held documents, for **car parks**: BS 5489-1:2003 (superseded)
/// clause 10.7.5, p. 26: the first grid points no more than 0.5 m (enclosed) or 1.0 m (outdoor)
/// from the wall or perimeter, a grid spacing of no more than 1.0 m (enclosed) or 5.0 m (outdoor),
/// and a minimum of 36 points per grid. Returned as (first point from perimeter m, spacing m,
/// minimum points), for the engineer to apply by hand: it is not applied automatically and it is
/// not the road grid of BS EN 13201-3.
pub fn car_park_grid_limits(enclosed: bool) -> (f64, f64, usize) {
    if enclosed {
        (0.5, 1.0, 36)
    } else {
        (1.0, 5.0, 36)
    }
}
