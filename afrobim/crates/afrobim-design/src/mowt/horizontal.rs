// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Horizontal alignment: minimum radius with `e` and `f` (Table 6-1), relative gradient and runoff
//! (Tables 6-2, 6-3), superelevation rates and minimum runoff lengths (Tables 6-4 to 6-9), widening
//! on curves (Table 6-10), transition (spiral) rules, the rules the text states, and sight distance
//! on horizontal curves (Figure 6-21).
//!
//! The manual gives **no** minimum length of a horizontal curve, no minimum tangent length between
//! curves and no maximum deflection angle without a curve; sections 6.2.2 and 6.2.3 give only
//! the maximum tangent length (4.0 km) and qualitative advice (short tangents between curves "should
//! be avoided", compound curve radius ratios). Those that are numbers are in [`HorizontalRules`].

use serde::Serialize;

use super::classes::RoadClass;
use super::{refuse, speed_index, Cited, Source};
use crate::error::DesignError;

pub use super::se::{superelevation, superelevation_neighbours, table_rmin, tabulated_radii, SuperelevationCell, SuperelevationRate};

/// The maximum superelevations for which Table 6-1 gives radii, percent.
pub const EMAX_OPTIONS_PERCENT: [f64; 4] = [4.0, 6.0, 7.0, 8.0];

/// The constant of the manual's radius relation `R = V^2 / (127 (e + f))` (section 6.2.3, p. 79).
/// [`crate::physics::min_radius_m`] uses the unrounded `12.96 g = 127.0966`, which gives radii about
/// 0.07 % smaller; the limits in this module are the manual's printed table values.
pub const MANUAL_RADIUS_CONSTANT: f64 = 127.0;

/// The manual's relation for the minimum radius, `V^2 / (127 (e + f))`, with `e` as a ratio.
pub fn manual_min_radius_m(speed_kmh: f64, e_ratio: f64, f: f64) -> f64 {
    speed_kmh * speed_kmh / (MANUAL_RADIUS_CONSTANT * (e_ratio + f))
}

/// One row of Table 6-1 (printed p. 80, PDF p. 89).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MinRadiusRow {
    pub speed_kmh: f64,
    /// Maximum e, percent.
    pub emax_percent: f64,
    /// Limiting value of the side friction factor f.
    pub limiting_f: f64,
    /// `e/100 + f`.
    pub e_plus_f: f64,
    /// The "Calculated Radius" column, m.
    pub calculated_radius_m: f64,
    /// The "Rounded Radius" column, m: the value used as the limit.
    pub rounded_radius_m: f64,
}

const T61_F: [f64; 10] = [0.17, 0.17, 0.16, 0.15, 0.14, 0.14, 0.13, 0.12, 0.11, 0.09];
const T61_CALC: [[f64; 10]; 4] = [
    [33.7, 60.0, 98.4, 149.2, 214.3, 280.0, 375.2, 492.1, 635.2, 872.2],
    [30.8, 54.8, 89.5, 135.0, 192.9, 252.0, 335.7, 437.4, 560.4, 755.9],
    [29.5, 52.5, 85.6, 128.8, 183.7, 240.0, 318.9, 414.4, 529.3, 708.7],
    [28.3, 50.4, 82.0, 123.2, 175.4, 229.1, 303.7, 393.7, 501.5, 667.0],
];
const T61_ROUND: [[f64; 10]; 4] = [
    [35.0, 60.0, 100.0, 150.0, 215.0, 280.0, 375.0, 490.0, 635.0, 870.0],
    [30.0, 55.0, 90.0, 135.0, 195.0, 250.0, 335.0, 440.0, 560.0, 755.0],
    [30.0, 55.0, 85.0, 130.0, 185.0, 240.0, 320.0, 415.0, 530.0, 710.0],
    [30.0, 50.0, 80.0, 125.0, 175.0, 230.0, 305.0, 400.0, 500.0, 670.0],
];

fn emax_index(emax_percent: f64) -> Result<usize, DesignError> {
    EMAX_OPTIONS_PERCENT.iter().position(|&e| (e - emax_percent).abs() < 1e-9).ok_or_else(|| {
        DesignError::Input(format!("Table 6-1 gives radii for a maximum superelevation of 4, 6, 7 and 8 %; {emax_percent} % is not one of them and nothing is interpolated."))
    })
}

/// Table 6-1: minimum radius for a design speed and a maximum superelevation, with the limiting side
/// friction factor. Table 6-1 is printed "for non-urban roads" (30 to 120 km/h).
pub fn min_radius(speed_kmh: f64, emax_percent: f64) -> Result<Cited<MinRadiusRow>, DesignError> {
    let si = speed_index(speed_kmh)?;
    let ei = emax_index(emax_percent)?;
    let f = T61_F[si];
    Ok(Cited::new(
        MinRadiusRow {
            speed_kmh,
            emax_percent,
            limiting_f: f,
            e_plus_f: ((emax_percent / 100.0 + f) * 100.0).round() / 100.0,
            calculated_radius_m: T61_CALC[ei][si],
            rounded_radius_m: T61_ROUND[ei][si],
        },
        "m",
        Source::new("Table 6-1", "6.2.3", 80, 89),
    ))
}

/// The limiting side friction factor `f` of Table 6-1 for a design speed (the same for every emax).
pub fn limiting_side_friction(speed_kmh: f64) -> Result<Cited<f64>, DesignError> {
    let si = speed_index(speed_kmh)?;
    Ok(Cited::new(T61_F[si], "", Source::new("Table 6-1", "6.2.3", 80, 89)))
}

/// Section 6.2.3 (p. 79): "the results of empirical studies have indicated 0.22 as a value of f above
/// which passengers experience some discomfort."
pub fn comfort_side_friction() -> Cited<f64> {
    Cited::new(0.22, "", Source::new("text", "6.2.3", 79, 88))
}

/// Table 6-2 (printed p. 92, PDF p. 101): maximum relative gradient (slope) ratio between the edges of
/// a two lane carriageway and the centreline, printed `133:1` and so on; the value returned is the 133.
/// The column heading prints the unit as `[%]`, which is a slip: the cells are ratios.
pub fn max_relative_gradient_ratio(speed_kmh: f64) -> Result<Cited<f64>, DesignError> {
    const RATIO: [f64; 10] = [133.0, 143.0, 150.0, 167.0, 182.0, 200.0, 210.0, 210.0, 238.0, 250.0];
    let si = speed_index(speed_kmh)?;
    Ok(Cited::new(RATIO[si], ":1", Source::new("Table 6-2", "6.2.3 vii", 92, 101)))
}

/// Table 6-3 (printed p. 93, PDF p. 102): length of superelevation runoff of a multi lane road as a
/// multiple of the two lane length: 2 lanes 1.0, 3 lanes 1.2, 4 lanes 1.5.
pub fn runoff_length_factor(lanes: u8) -> Result<Cited<f64>, DesignError> {
    let v = match lanes {
        2 => 1.0,
        3 => 1.2,
        4 => 1.5,
        n => return refuse(format!("Table 6-3 gives the runoff length factor for 2, 3 and 4 lanes only, not {n}; nothing is extrapolated.")),
    };
    Ok(Cited::new(v, "times the two lane length", Source::new("Table 6-3", "6.2.3 vii", 93, 102)))
}

/// The distance travelled in 2 seconds at the design speed, m: the manual's minimum length of runoff
/// ("approximately", section 6.2.3 vii, p. 92).
pub fn two_second_distance_m(speed_kmh: f64) -> f64 {
    speed_kmh / 3.6 * 2.0
}

/// A superelevation runoff length worked from the manual's formula.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunoffLength {
    /// `i x Lw x ratio` (times the Table 6-3 factor for more than two lanes), m.
    pub by_relative_gradient_m: f64,
    /// Distance travelled in 2 seconds at the design speed, m.
    pub two_second_minimum_m: f64,
    /// The greater of the two, m.
    pub governing_m: f64,
}

/// Section 6.2.3 vii (p. 92): `L = i x Lw x ratio`, where `i` is the superelevation slope (`e/100`), `Lw`
/// the lane width and `ratio` the maximum relative gradient ratio of Table 6-2; not less than the distance
/// travelled in 2 seconds. For more than two lanes the length is multiplied by the Table 6-3 factor.
/// Tables 6-4 to 6-9 print integer lengths computed this way for a 3.5 m lane (checked in the tests).
pub fn runoff_length(speed_kmh: f64, superelevation_percent: f64, lane_width_m: f64, lanes: u8) -> Result<Cited<RunoffLength>, DesignError> {
    let ratio = max_relative_gradient_ratio(speed_kmh)?;
    let factor = runoff_length_factor(lanes)?;
    if !(superelevation_percent >= 0.0) || !(lane_width_m > 0.0) {
        return refuse("The superelevation and the lane width must be positive.");
    }
    let by_grad = superelevation_percent / 100.0 * lane_width_m * ratio.value * factor.value;
    let two = two_second_distance_m(speed_kmh);
    Ok(Cited::new(RunoffLength { by_relative_gradient_m: by_grad, two_second_minimum_m: two, governing_m: by_grad.max(two) }, "m", Source::new("text", "6.2.3 vii", 92, 101)))
}

/// Length of the tangent runout, m, as the worked examples compute it (pp. 94 and 96): the adverse crown
/// `NC` (percent) is removed at the rate of the runoff, `Lra = NC x Ls / FSE`, where `Ls` is the runoff
/// length and `FSE` the full superelevation (percent). Section 6.2.3 vii (p. 93) says the rate of removal
/// "should preferably be the same as the rate used to effect the superelevation runoff".
pub fn tangent_runout_length(normal_crown_percent: f64, runoff_length_m: f64, full_superelevation_percent: f64) -> Result<Cited<f64>, DesignError> {
    if !(full_superelevation_percent > 0.0) {
        return refuse("The full superelevation must be positive to work out a tangent runout.");
    }
    Ok(Cited::new(normal_crown_percent * runoff_length_m / full_superelevation_percent, "m", Source::new("worked example 1", "6.2.3 vii", 94, 103)))
}

/// Stations of a superelevated curve without spirals, laid out as the manual's worked example 1
/// (printed pp. 94 and 95): two thirds of the runoff `Ls` on the tangent before the PC (and after the PT),
/// one third on the curve; the tangent runout `Lra` before the start of the runoff.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SimpleCurveSuperelevationStations {
    pub start_of_superelevation: f64,
    pub start_of_runoff: f64,
    pub start_of_equal_superelevation: f64,
    pub start_of_full_superelevation: f64,
    pub end_of_full_superelevation: f64,
    pub end_of_runoff: f64,
    pub end_of_equal_superelevation: f64,
    pub end_of_superelevation: f64,
}

pub fn simple_curve_superelevation_stations(pc: f64, pt: f64, runoff_m: f64, runout_m: f64) -> Cited<SimpleCurveSuperelevationStations> {
    let two_thirds = 2.0 / 3.0 * runoff_m;
    let one_third = 1.0 / 3.0 * runoff_m;
    let start_of_runoff = pc - two_thirds;
    let end_of_runoff = pt + two_thirds;
    Cited::new(
        SimpleCurveSuperelevationStations {
            start_of_superelevation: pc - two_thirds - runout_m,
            start_of_runoff,
            start_of_equal_superelevation: start_of_runoff + runout_m,
            start_of_full_superelevation: pc + one_third,
            end_of_full_superelevation: pt - one_third,
            end_of_runoff,
            end_of_equal_superelevation: end_of_runoff - runout_m,
            end_of_superelevation: pt + two_thirds + runout_m,
        },
        "station, m",
        Source::new("worked example 1", "6.2.3 vii", 94, 103),
    )
}

/// Stations of a superelevated curve with spirals, laid out as worked example 2 (printed pp. 95 and 96):
/// the runoff is the whole spiral; the tangent runout `Lra` lies before the TS and after the ST.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpiralCurveSuperelevationStations {
    pub start_of_superelevation: f64,
    pub start_of_equal_superelevation: f64,
    pub start_of_full_superelevation: f64,
    pub end_of_full_superelevation: f64,
    pub end_of_equal_superelevation: f64,
    pub end_of_superelevation: f64,
}

pub fn spiral_curve_superelevation_stations(ts: f64, sc: f64, cs: f64, st: f64, runout_m: f64) -> Cited<SpiralCurveSuperelevationStations> {
    Cited::new(
        SpiralCurveSuperelevationStations {
            start_of_superelevation: ts - runout_m,
            start_of_equal_superelevation: ts + runout_m,
            start_of_full_superelevation: sc,
            end_of_full_superelevation: cs,
            end_of_equal_superelevation: st - runout_m,
            end_of_superelevation: st + runout_m,
        },
        "station, m",
        Source::new("worked example 2", "6.2.3 vii", 95, 104),
    )
}

// ---- Table 6-10: widening -------------------------------------------------------------------

/// Number of lanes for Table 6-10 curve widening.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WideningLanes {
    Single,
    Two,
}

/// Table 6-10 (printed p. 104, PDF p. 113), "Curve widening": by the radius of the curve. The printed
/// radius bands are 20-40, 41-60, 61-120, 121-250 and over 250 m; a radius below 20 m is refused (no
/// value is printed, and section 6.2.3 iii says radii of 20 m or less are outside the standards) and so is
/// a radius that falls in the gap between two bands (40 to 41, 60 to 61, 120 to 121 m). The widening is
/// applied on the inside of the curve (text, p. 104).
pub fn curve_widening(radius_m: f64, lanes: WideningLanes) -> Result<Cited<f64>, DesignError> {
    let src = Source::new("Table 6-10", "6.2.4", 104, 113);
    let (single, two) = if radius_m < 20.0 {
        return refuse(format!("Table 6-10 starts at a radius of 20 m; {radius_m} m is below it and the manual gives no widening there (switchbacks of 20 m or less are outside the standards, section 6.2.3 iii, p. 87)."));
    } else if radius_m <= 40.0 {
        (0.6, 1.5)
    } else if radius_m < 41.0 {
        return refuse(format!("{radius_m} m falls between the printed bands 20-40 m and 41-60 m of Table 6-10; the manual does not say which applies."));
    } else if radius_m <= 60.0 {
        (0.6, 1.2)
    } else if radius_m < 61.0 {
        return refuse(format!("{radius_m} m falls between the printed bands 41-60 m and 61-120 m of Table 6-10; the manual does not say which applies."));
    } else if radius_m <= 120.0 {
        (0.0, 0.9)
    } else if radius_m < 121.0 {
        return refuse(format!("{radius_m} m falls between the printed bands 61-120 m and 121-250 m of Table 6-10; the manual does not say which applies."));
    } else if radius_m <= 250.0 {
        (0.0, 0.6)
    } else {
        (0.0, 0.0)
    };
    Ok(Cited::new(if lanes == WideningLanes::Single { single } else { two }, "m", src))
}

/// Table 6-10, "Fill widening", by the height of fill measured from the edge of the shoulder to the toe of
/// the slope: 0.0-3.0 m 0.3 m, 3.0-6.0 m 0.6 m, 6.0-9.0 m 0.9 m, over 9.0 m 0.9 m. Required "for design
/// classes I, II, III, A and B" (text, p. 104), on both sides of the road; Class C is refused. A height of
/// exactly 3.0 or 6.0 m is printed in two rows with different widenings and is refused.
pub fn fill_widening(class: RoadClass, height_of_fill_m: f64) -> Result<Cited<f64>, DesignError> {
    let src = Source::new("Table 6-10", "6.2.4", 104, 113);
    if class == RoadClass::C {
        return refuse("Section 6.2.4 (p. 104) requires widening at high fills for design classes I, II, III, A and B; it names no widening for Class C Gravel.");
    }
    if !(height_of_fill_m >= 0.0) {
        return refuse("The height of fill cannot be negative.");
    }
    let v = if height_of_fill_m < 3.0 {
        0.3
    } else if height_of_fill_m < 6.0 && height_of_fill_m > 3.0 {
        0.6
    } else if height_of_fill_m > 6.0 && height_of_fill_m <= 9.0 {
        0.9
    } else if height_of_fill_m > 9.0 {
        0.9
    } else {
        return refuse(format!("A height of fill of {height_of_fill_m} m is printed in two rows of Table 6-10 (0.0-3.0 and 3.0-6.0, or 3.0-6.0 and 6.0-9.0) with different widenings; the manual does not say which applies."));
    };
    Ok(Cited::new(v, "m", src))
}

// ---- Transition (spiral) rules ---------------------------------------------------------------

/// Section 6.2.3 iv (printed p. 88, PDF p. 97): "Transition curves are required if `R < V^3 / 432`"
/// (V in km/h, R in m); in all other cases a transition curve is not required. Returns the threshold
/// radius `V^3 / 432` in the result's value and whether the curve radius is below it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransitionRequirement {
    pub threshold_radius_m: f64,
    pub required: bool,
}

pub fn transition_required(speed_kmh: f64, radius_m: f64) -> Cited<TransitionRequirement> {
    let t = speed_kmh.powi(3) / 432.0;
    Cited::new(TransitionRequirement { threshold_radius_m: t, required: radius_m < t }, "m", Source::new("text", "6.2.3 iv", 88, 97))
}

/// Section 6.2.3 iv (p. 88): minimum length of spiral `L = 0.0702 V^3 / (R C)` (L in m, V in km/h, R in m,
/// C the rate of increase of centripetal acceleration in m/s^3). The manual says the "value C = 1
/// generally is acceptable for railroad operation, but values ranging from 1 to 3 have been used for
/// roads"; a C outside 1 to 3 is refused. It adds that "a more practical control for the length of spiral is
/// that in which it equals the length required for superelevation runoff".
pub fn min_spiral_length_comfort(speed_kmh: f64, radius_m: f64, c_m_per_s3: f64) -> Result<Cited<f64>, DesignError> {
    if !(1.0..=3.0).contains(&c_m_per_s3) {
        return refuse(format!("The manual names values of C from 1 to 3 m/s^3 for roads (section 6.2.3 iv, p. 88); {c_m_per_s3} is outside that range."));
    }
    if !(radius_m > 0.0) || !(speed_kmh > 0.0) {
        return refuse("The speed and the radius must be positive.");
    }
    Ok(Cited::new(0.0702 * speed_kmh.powi(3) / (radius_m * c_m_per_s3), "m", Source::new("text", "6.2.3 iv", 88, 97)))
}

/// The standard value of `k`, the rate of increase of centripetal acceleration (m/s^3), of the clothoid
/// (section 6.2.3 v, printed p. 89, PDF p. 98): "K should normally be in the interval 0.3 to 0.6 with a
/// standard value of 0.45".
pub const CLOTHOID_K_STANDARD: f64 = 0.45;
pub const CLOTHOID_K_RANGE: (f64, f64) = (0.3, 0.6);

/// Clothoid parameter `A = sqrt(v^3 / k)` (m, v in m/s), p. 89. `k` outside 0.3 to 0.6 is refused.
/// (The text prints the defining relation as `A^2 = R - L`; Figure 6-13 prints `A^2 = R x L`, which is the
/// clothoid relation, so the dash on page 88 is a slip.)
pub fn clothoid_parameter(speed_kmh: f64, k_m_per_s3: f64) -> Result<Cited<f64>, DesignError> {
    if !(CLOTHOID_K_RANGE.0..=CLOTHOID_K_RANGE.1).contains(&k_m_per_s3) {
        return refuse(format!("The manual gives k from 0.3 to 0.6 m/s^3 (standard 0.45), section 6.2.3 v, p. 89; {k_m_per_s3} is outside that range."));
    }
    let v = speed_kmh / 3.6;
    Ok(Cited::new((v.powi(3) / k_m_per_s3).sqrt(), "m", Source::new("text", "6.2.3 v", 89, 98)))
}

/// Spiral length for the clothoid parameter at the standard or a chosen `k`: `L = A^2 / R = v^3 / (k R)`.
pub fn spiral_length_for_k(speed_kmh: f64, radius_m: f64, k_m_per_s3: f64) -> Result<Cited<f64>, DesignError> {
    let a = clothoid_parameter(speed_kmh, k_m_per_s3)?;
    if !(radius_m > 0.0) {
        return refuse("The radius must be positive.");
    }
    Ok(Cited::new(a.value * a.value / radius_m, "m", a.source))
}

/// Aesthetic limits of a simple clothoid for the final radius `R` (Figure 6-14 text, printed p. 90,
/// PDF p. 99): `R/3 < A < R` and `0.1 R < L < R`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClothoidAestheticLimits {
    pub a_min_m: f64,
    pub a_max_m: f64,
    pub l_min_m: f64,
    pub l_max_m: f64,
}

pub fn clothoid_aesthetic_limits(radius_m: f64) -> Cited<ClothoidAestheticLimits> {
    Cited::new(ClothoidAestheticLimits { a_min_m: radius_m / 3.0, a_max_m: radius_m, l_min_m: 0.1 * radius_m, l_max_m: radius_m }, "m", Source::new("text", "6.2.3 v", 90, 99))
}

/// "The circular displacement DR should be at least 0.25 m to avoid kinks" (p. 90, PDF p. 99).
pub fn min_circular_shift() -> Cited<f64> {
    Cited::new(0.25, "m", Source::new("text", "6.2.3 v", 90, 99))
}

/// The numbers the text of section 6.2 states as rules.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HorizontalRules {
    /// "The maximum length of a tangent section therefore should not exceed 4.0 kilometers" (6.2.2, p. 79).
    pub max_tangent_length_m: Cited<f64>,
    /// Compound curves, desirable maximum ratio of radii 1.5:1 (6.2.3 i, p. 84).
    pub compound_ratio_desirable: Cited<f64>,
    /// Compound curves, maximum ratio of radii 1.75:1 (6.2.3 i, p. 84).
    pub compound_ratio_maximum: Cited<f64>,
    /// Isolated curves: the minimum radius of Tables 4-3 to 4-9 "shall be increased by 50 percent" (p. 85).
    pub isolated_curve_radius_factor: Cited<f64>,
    /// Switchback curves: radius "of 20 m or less, with a minimum of 10 m", outside the standards (p. 87).
    pub switchback_radius_max_m: Cited<f64>,
    pub switchback_radius_min_m: Cited<f64>,
    /// S-curve of two clothoids with A < 200: ratio A1/A2 < 1.5 recommended (p. 91).
    pub s_curve_parameter_ratio: Cited<f64>,
}

pub fn rules() -> HorizontalRules {
    HorizontalRules {
        max_tangent_length_m: Cited::new(4000.0, "m", Source::new("text", "6.2.2", 79, 88)),
        compound_ratio_desirable: Cited::new(1.5, ":1", Source::new("text", "6.2.3 i", 84, 93)),
        compound_ratio_maximum: Cited::new(1.75, ":1", Source::new("text", "6.2.3 i", 84, 93)),
        isolated_curve_radius_factor: Cited::new(1.5, "times the tabulated minimum radius", Source::new("text", "6.2.3 ii", 85, 94)),
        switchback_radius_max_m: Cited::new(20.0, "m", Source::new("text", "6.2.3 iii", 87, 96)),
        switchback_radius_min_m: Cited::new(10.0, "m", Source::new("text", "6.2.3 iii", 87, 96)),
        s_curve_parameter_ratio: Cited::new(1.5, ":1", Source::new("text", "6.2.3 v", 91, 100)),
    }
}

// ---- Sight distance on horizontal curves (Figure 6-21) -----------------------------------------

/// Degree of curve by the arc definition, `D = 1145.916 / R` degrees (section 6.2.3, p. 82), the central
/// angle subtended by a 20 m arc.
pub fn degree_of_curve(radius_m: f64) -> Cited<f64> {
    Cited::new(1145.916 / radius_m, "degrees per 20 m arc", Source::new("text", "6.2.3", 82, 91))
}

/// Figure 6-21 (printed p. 105, PDF p. 114): line of sight `LS = 2 R sin(D/2)` for the deflection angle D
/// (degrees) subtended by the line of sight and the radius.
pub fn line_of_sight_m(radius_m: f64, deflection_deg: f64) -> Cited<f64> {
    Cited::new(2.0 * radius_m * (deflection_deg.to_radians() / 2.0).sin(), "m", Source::new("Figure 6-21", "6.2.5", 105, 114))
}

/// Figure 6-21, relation [a]: middle ordinate `M = R (1 - cos(D/2))`, m, for the deflection angle D (degrees).
pub fn middle_ordinate_m(radius_m: f64, deflection_deg: f64) -> Cited<f64> {
    Cited::new(radius_m * (1.0 - (deflection_deg.to_radians() / 2.0).cos()), "m", Source::new("Figure 6-21", "6.2.5", 105, 114))
}

/// Figure 6-21, relation [b]: the middle ordinate needed to see the stopping sight distance `S` (m) round a
/// curve of radius `R`, `M = R (1 - cos(28.65 S / R))` with the angle in degrees (28.65 = 90 / pi: the sight
/// distance is measured along the centreline of the inside lane, section 6.2.5).
///
/// Direction of the test: the manual's text says the ordinate of [b] "should not be less than that of
/// [a]", but its worked example concludes "11.85 >> 3.94 design is OK" with [a] the clearance available
/// and [b] the clearance required. The example is the sensible reading (the clearance available, [a], must
/// be at least the clearance required, [b]) and is the one used in the tests; the wording of the text is
/// recorded as a slip.
pub fn required_middle_ordinate_m(radius_m: f64, stopping_sight_distance_m: f64) -> Cited<f64> {
    let angle_deg = 28.65 * stopping_sight_distance_m / radius_m;
    Cited::new(radius_m * (1.0 - angle_deg.to_radians().cos()), "m", Source::new("Figure 6-21", "6.2.5", 105, 114))
}
