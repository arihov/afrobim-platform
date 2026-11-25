// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Cross-section elements, as lookups for the corridor and pavement modules to use later: lane, carriageway
//! and shoulder widths and the right of way by design class (Table 4-2b), normal and shoulder cross fall
//! (class tables and section 7.5), edge strip and edge line widths (Table 7-3) and headroom additions at crests
//! (Table 7-1). Nothing here changes any other crate.

use serde::Serialize;

use super::classes::{self, ClassCrossSection, Parameter, RoadClass, Surface, Terrain};
use super::{refuse, Cited, Source};
use crate::error::DesignError;

/// Table 4-2b (printed p. 45, PDF p. 54): lane width, carriageway width, shoulders, median, right of way.
pub fn class_cross_section(class: RoadClass) -> Cited<ClassCrossSection> {
    classes::class_cross_section(class)
}

/// Lane width of a class, m (Table 4-2b). Section 7.3 (p. 136) adds that for paved roads the lane width "is the
/// net width excluding width of edge strip, edge line and center line markings".
pub fn lane_width_m(class: RoadClass) -> Cited<f64> {
    let c = classes::class_cross_section(class);
    Cited::new(c.value.lane_width_m, "m", c.source)
}

/// Width of each shoulder, m (Table 4-2b prints `2 x 2.5` and so on).
pub fn shoulder_width_m(class: RoadClass) -> Cited<f64> {
    let c = classes::class_cross_section(class);
    Cited::new(c.value.shoulder_width_each_side_m, "m", c.source)
}

/// Normal cross fall of the carriageway, percent, as the class tables 4-3 to 4-9 print it (2.5 for the paved
/// classes, 4 for the gravel classes; the same in every terrain column).
pub fn normal_cross_fall_percent(class: RoadClass) -> Result<Cited<f64>, DesignError> {
    classes::class_parameter(class, Terrain::Flat, Parameter::NormalCrossFall)
}

/// Shoulder cross fall, percent, as the class tables print it (4 for every class).
pub fn shoulder_cross_fall_percent(class: RoadClass) -> Result<Cited<f64>, DesignError> {
    classes::class_parameter(class, Terrain::Flat, Parameter::ShoulderCrossFall)
}

/// The cross fall rules of section 7.5 (printed p. 137, PDF p. 146).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrossFallRules {
    /// "The normal cross fall for paved carriageway on tangent sections ... shall be 2.5 percent maximum."
    pub paved_max_percent: Cited<f64>,
    /// "Higher values up to 3 percent could be used in areas of intense rainfall", undesirable for operation.
    pub paved_max_in_intense_rainfall_percent: Cited<f64>,
    /// "On high type two-lane carriageway the crown slope of as low as 2 percent is accepted for all other conditions."
    pub paved_high_type_min_percent: Cited<f64>,
    /// "The normal cross fall for unpaved roads should be 4 percent."
    pub unpaved_percent: Cited<f64>,
}

pub fn cross_fall_rules() -> CrossFallRules {
    let s = Source::new("text", "7.5", 137, 146);
    CrossFallRules {
        paved_max_percent: Cited::new(2.5, "%", s),
        paved_max_in_intense_rainfall_percent: Cited::new(3.0, "%", s),
        paved_high_type_min_percent: Cited::new(2.0, "%", s),
        unpaved_percent: Cited::new(4.0, "%", s),
    }
}

/// Shoulder slopes of section 7.4 (printed p. 137, PDF p. 146): "Paved shoulders normally have same slope as the
/// traffic lanes and should be sloped from 3 to 6 percent, gravel and crushed-rock shoulders from 4 to 6 percent
/// and turf shoulders up to 8 percent." Returns `(low, high)` percent; the paved and gravel ranges are lower and
/// upper limits as printed, turf has no lower figure (`None`).
pub fn shoulder_slope_range_percent(kind: ShoulderSurface) -> Cited<(Option<f64>, f64)> {
    let s = Source::new("text", "7.4", 137, 146);
    match kind {
        ShoulderSurface::Paved => Cited::new((Some(3.0), 6.0), "%", s),
        ShoulderSurface::GravelOrCrushedRock => Cited::new((Some(4.0), 6.0), "%", s),
        ShoulderSurface::Turf => Cited::new((None, 8.0), "%", s),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ShoulderSurface {
    Paved,
    GravelOrCrushedRock,
    Turf,
}

/// Width of the edge strip and the edge line, m (Table 7-3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EdgeStrip {
    pub edge_strip_width_m: f64,
    pub edge_line_width_m: f64,
}

/// Table 7-3 (printed p. 136, PDF p. 145): width of edge strip and edge line by design speed: 120 km/h 0.75 and
/// 0.15 m, 100 km/h 0.45 and 0.15 m, 80 km/h 0.45 and 0.10 m, 60 km/h 0.20 and 0.10 m, below 60 km/h a dash (no
/// value). Speeds between the printed rows (70, 90, 110) are refused. Only for paved roads.
pub fn edge_strip(class: RoadClass, speed_kmh: f64) -> Result<Cited<EdgeStrip>, DesignError> {
    if class.surface() != Surface::Paved {
        return refuse("Table 7-3 and its text (section 7.3, p. 136) concern the edge strip and edge line of paved roads; the manual gives no edge strip for gravel classes.");
    }
    let src = Source::new("Table 7-3", "7.3", 136, 145);
    let (strip, line) = match speed_kmh {
        s if (s - 120.0).abs() < 1e-9 => (0.75, 0.15),
        s if (s - 100.0).abs() < 1e-9 => (0.45, 0.15),
        s if (s - 80.0).abs() < 1e-9 => (0.45, 0.10),
        s if (s - 60.0).abs() < 1e-9 => (0.20, 0.10),
        s if s < 60.0 => return refuse(format!("Table 7-3 prints a dash for design speeds below 60 km/h ({speed_kmh} km/h): the manual gives no value.")),
        s => return refuse(format!("Table 7-3 has rows for 120, 100, 80 and 60 km/h and below 60 km/h; {s} km/h is not a row and nothing is interpolated.")),
    };
    Ok(Cited::new(EdgeStrip { edge_strip_width_m: strip, edge_line_width_m: line }, "m", src))
}

/// Table 7-1 (printed p. 134, PDF p. 143): additions to headroom at crests, m, by crest radius: 100 m 0.12,
/// 200 m 0.06, 300 m 0.04, 400 m 0.03, 500 to 700 m 0.02. The text says the addition is needed at crests with
/// radii below 700 m. A radius that is not one of the rows (and not 500 to 700) is refused; nothing is interpolated.
pub fn headroom_addition_at_crest_m(crest_radius_m: f64) -> Result<Cited<f64>, DesignError> {
    let src = Source::new("Table 7-1", "7.2", 134, 143);
    let v = match crest_radius_m {
        r if (r - 100.0).abs() < 1e-9 => 0.12,
        r if (r - 200.0).abs() < 1e-9 => 0.06,
        r if (r - 300.0).abs() < 1e-9 => 0.04,
        r if (r - 400.0).abs() < 1e-9 => 0.03,
        r if (500.0..=700.0).contains(&r) => 0.02,
        r => return refuse(format!("Table 7-1 gives additions to headroom for crest radii of 100, 200, 300, 400 and 500 to 700 m; {r} m is not a row and nothing is interpolated or extrapolated.")),
    };
    Ok(Cited::new(v, "m", src))
}
