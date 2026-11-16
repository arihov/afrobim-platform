//! Shared builders. Every number here is an arbitrary test value, not a design value.

#![allow(dead_code)]

use afrobim_alignment::{Alignment, AlignmentBuilder, Profile, Pvi};
use afrobim_corridor::Side;
use afrobim_junction::*;
use std::f64::consts::FRAC_PI_2;

/// Easting and northing at true UTM magnitude (zone 36 north, near the equator).
pub const E0: f64 = 450_000.0;
pub const N0: f64 = 9_960_000.0;

pub fn line(name: &str, x: f64, y: f64, heading: f64, length: f64, start_station: f64) -> Alignment {
    AlignmentBuilder::new(name, start_station, x, y, heading).line(length).build().unwrap()
}

/// Primary along +X through (E0, N0), 600 m, stations 0 to 600, crossing at 300.
pub fn primary() -> Alignment {
    line("primary", E0 - 300.0, N0, 0.0, 600.0, 0.0)
}

/// Secondary along +Y through (E0, N0), 600 m, stations 0 to 600, crossing at 300.
pub fn secondary() -> Alignment {
    line("secondary", E0, N0 - 300.0, FRAC_PI_2, 600.0, 0.0)
}

pub fn with_profile(mut a: Alignment, pvis: Vec<Pvi>) -> Alignment {
    a.profile = Some(Profile::new("profile", pvis).unwrap());
    a
}

pub fn edges(left: f64, right: f64) -> RoadEdges {
    RoadEdges { left_offset_m: left, right_offset_m: right, left_widening: vec![], right_widening: vec![] }
}

pub fn all_returns(kind: ReturnKind) -> Vec<ReturnSpec> {
    Quadrant::ALL.iter().map(|q| ReturnSpec { quadrant: *q, kind: kind.clone() }).collect()
}

pub fn cross_inputs(kind: ReturnKind) -> IntersectionInputs {
    IntersectionInputs {
        name: "J1".into(),
        primary: "primary".into(),
        secondary: "secondary".into(),
        crossing_index: 0,
        extend_m: 0.0,
        corridor_type: CorridorType::PrimaryCrownMaintained,
        primary_edges: edges(3.5, 3.0),
        secondary_edges: edges(3.2, 2.8),
        extents: JunctionExtents { primary_behind_m: 100.0, primary_ahead_m: 100.0, secondary_behind_m: 100.0, secondary_ahead_m: 100.0 },
        returns: all_returns(kind),
        islands: vec![],
        vertical: None,
        corridor: None,
    }
}

pub fn vertical_inputs() -> VerticalInputs {
    VerticalInputs {
        primary_crossfall: CrossfallInput::Fixed { left_pct: -2.5, right_pct: -2.5 },
        secondary_crossfall: CrossfallInput::Fixed { left_pct: -3.0, right_pct: -3.0 },
        tie_in_length_m: 20.0,
        lock_sample_spacing_m: None,
        edge_profile_spacing_m: 1.0,
        return_profile_spacing_m: 1.0,
    }
}

pub fn return_of<'a>(r: &'a IntersectionResult, label: &str) -> &'a ReturnResult {
    r.returns.iter().find(|x| x.geometry.quadrant.label() == label).unwrap_or_else(|| panic!("no return {label}"))
}

pub fn near(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} vs {b} (difference {:e}, tolerance {:e})", (a - b).abs(), tol);
}

pub fn side_name(s: Side) -> &'static str {
    if s == Side::Left {
        "left"
    } else {
        "right"
    }
}
