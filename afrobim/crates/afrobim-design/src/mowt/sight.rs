// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Sight distance: stopping (Tables 6.19, 6.20), passing (Tables 6.21, 6.22) and meeting sight distance, with
//! the assumptions the manual states (reaction time, friction, eye and object heights).
//!
//! The manual numbers these tables "6.19" to "6.22" with a point; the other tables of section 6 use a hyphen.
//! The names are kept as printed.

use serde::Serialize;

use super::{refuse, speed_index, Cited, Entry, Source};
use crate::error::DesignError;

/// The inputs behind the sight distances, each as the manual states it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SightDistanceInputs {
    /// Perception-brake reaction time, s: "The AASHTO brake reaction time for stopping has been set at 2,5 s ...
    /// This value has been adopted for Uganda case as well" (section 5.3, printed p. 55, PDF p. 64); also section
    /// 6.7.2 (p. 128).
    pub reaction_time_s: Cited<f64>,
    /// Eye height above the road, m: 1.07 (section 6.7.2, p. 128; 6.7.3, p. 130).
    pub eye_height_m: Cited<f64>,
    /// Object height for stopping sight distance, m: 0.15 (section 6.7.2, p. 128).
    pub object_height_stopping_m: Cited<f64>,
    /// Object height for passing sight distance, m: 1.3 (section 6.7.3, p. 130).
    pub object_height_passing_m: Cited<f64>,
    /// Headlight height, m: 0.60 (section 6.3.2, printed p. 110, PDF p. 119).
    pub headlight_height_m: Cited<f64>,
    /// Upward divergence of the headlight beam, degrees: 1 (section 6.3.2, p. 110).
    pub headlight_beam_deg: Cited<f64>,
    /// Safety distance added in meeting sight distance, m: 10.0 (section 6.7.5, printed p. 133, PDF p. 142).
    pub meeting_safety_distance_m: Cited<f64>,
}

pub fn inputs() -> SightDistanceInputs {
    let ssd = |p: u16| Source::new("text", "6.7.2", p, p + 9);
    SightDistanceInputs {
        reaction_time_s: Cited::new(2.5, "s", Source::new("text", "5.3 and 6.7.2", 55, 64)),
        eye_height_m: Cited::new(1.07, "m", ssd(128)),
        object_height_stopping_m: Cited::new(0.15, "m", ssd(128)),
        object_height_passing_m: Cited::new(1.3, "m", Source::new("text", "6.7.3", 130, 139)),
        headlight_height_m: Cited::new(0.6, "m", Source::new("text", "6.3.2", 110, 119)),
        headlight_beam_deg: Cited::new(1.0, "degree", Source::new("text", "6.3.2", 110, 119)),
        meeting_safety_distance_m: Cited::new(10.0, "m", Source::new("text", "6.7.5", 133, 142)),
    }
}

/// One row of Table 6.19, stopping sight distance on level ground for wet pavement.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoppingSightRow {
    pub design_speed_kmh: f64,
    /// "Assumed speed for condition", km/h: low and high.
    pub assumed_speed_kmh: (f64, f64),
    pub reaction_time_s: f64,
    /// Brake reaction distance, m: low and high.
    pub reaction_distance_m: (f64, f64),
    /// Coefficient of friction for wet pavement condition.
    pub friction: f64,
    /// Braking distance on level, m: low and high.
    pub braking_distance_m: (f64, f64),
    /// "Stopping sight distance for design", m: low and high. The class tables 4-3 to 4-9 carry the low end
    /// (rounded up to a multiple of 5 m except one cell).
    pub stopping_sight_distance_m: (f64, f64),
}

const ASSUMED: [(f64, f64); 10] = [(30.0, 30.0), (40.0, 40.0), (47.0, 50.0), (55.0, 60.0), (63.0, 70.0), (70.0, 80.0), (77.0, 90.0), (85.0, 100.0), (91.0, 110.0), (98.0, 120.0)];
const REACTION: [(f64, f64); 10] = [(20.8, 20.8), (27.8, 27.8), (32.6, 34.7), (38.2, 41.7), (43.8, 48.6), (48.6, 55.6), (53.5, 62.5), (59.0, 69.4), (63.2, 76.4), (68.1, 83.3)];
const FRICTION: [f64; 10] = [0.40, 0.38, 0.35, 0.33, 0.31, 0.30, 0.30, 0.29, 0.28, 0.28];
const BRAKING: [(f64, f64); 10] = [(8.9, 8.9), (16.6, 16.6), (24.8, 28.1), (36.1, 42.9), (50.4, 62.2), (64.3, 84.0), (77.8, 106.3), (98.1, 135.8), (116.4, 170.1), (135.0, 202.5)];
const SSD: [(f64, f64); 10] = [(29.7, 29.7), (44.4, 44.4), (57.5, 62.8), (74.3, 84.6), (94.2, 110.8), (112.9, 139.5), (131.3, 168.8), (157.0, 205.2), (179.6, 246.5), (203.1, 285.8)];

/// Table 6.19 (printed p. 129, PDF p. 138).
pub fn stopping_sight_distance(speed_kmh: f64) -> Result<Cited<StoppingSightRow>, DesignError> {
    let i = speed_index(speed_kmh)?;
    Ok(Cited::new(
        StoppingSightRow {
            design_speed_kmh: speed_kmh,
            assumed_speed_kmh: ASSUMED[i],
            reaction_time_s: 2.5,
            reaction_distance_m: REACTION[i],
            friction: FRICTION[i],
            braking_distance_m: BRAKING[i],
            stopping_sight_distance_m: SSD[i],
        },
        "m",
        Source::new("Table 6.19", "6.7.2", 129, 138),
    ))
}

/// Section 6.7.2 (printed p. 128, PDF p. 137): `SSD = 0.278 x prt x V + V^2 / (254 (f + G))` with `prt` the
/// perception reaction time (s), V the speed (km/h), f the coefficient of longitudinal friction and G the
/// grade as a ratio (positive uphill, negative downhill). `None` unless `f + G > 0`.
pub fn stopping_sight_distance_formula_m(reaction_time_s: f64, speed_kmh: f64, friction: f64, grade_ratio: f64) -> Option<f64> {
    (friction + grade_ratio > 0.0).then(|| 0.278 * reaction_time_s * speed_kmh + speed_kmh * speed_kmh / (254.0 * (friction + grade_ratio)))
}

/// Direction of a grade in Table 6.20.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GradeDirection {
    Downgrade,
    Upgrade,
}

const DOWN: [[f64; 3]; 10] = [
    [30.4, 30.7, 31.6],
    [45.8, 46.3, 48.1],
    [65.5, 66.5, 69.9],
    [88.9, 90.6, 96.2],
    [117.5, 120.1, 129.0],
    [148.9, 152.5, 165.2],
    [180.7, 185.2, 201.2],
    [220.9, 227.0, 248.5],
    [267.0, 274.9, 303.3],
    [310.2, 319.6, 353.4],
];
const UP: [[f64; 3]; 10] = [
    [29.1, 28.7, 28.4],
    [43.2, 42.4, 41.8],
    [55.6, 54.4, 53.4],
    [71.6, 69.6, 68.0],
    [89.7, 87.2, 84.9],
    [107.1, 103.8, 100.8],
    [124.2, 120.2, 116.6],
    [148.0, 142.7, 138.1],
    [168.4, 162.0, 156.4],
    [190.1, 182.7, 176.1],
];

/// Table 6.20 (printed p. 129, PDF p. 138): stopping sight distance, m, on a 3, 5 or 7 percent downgrade or
/// upgrade. For upgrades the table uses the "assumed speed for condition" (the low end of Table 6.19), for
/// downgrades the design speed.
///
/// The **5 % downgrade** column is withheld: all ten printed values equal the manual's own formula
/// (section 6.7.2) for a 4 % downgrade (for example 120 km/h: 319.6 printed, 319.6 for 4 %, 329.9 for 5 %).
pub fn stopping_sight_distance_on_grade(speed_kmh: f64, grade_percent: f64, direction: GradeDirection) -> Result<Cited<f64>, DesignError> {
    let i = speed_index(speed_kmh)?;
    let j = match grade_percent {
        g if (g - 3.0).abs() < 1e-9 => 0,
        g if (g - 5.0).abs() < 1e-9 => 1,
        g if (g - 7.0).abs() < 1e-9 => 2,
        g => return refuse(format!("Table 6.20 gives grades of 3, 5 and 7 % only; {g} % is not a column and nothing is interpolated.")),
    };
    let src = Source::new("Table 6.20", "6.7.2", 129, 138);
    let e = match (direction, j) {
        (GradeDirection::Downgrade, 1) => Entry::Withheld("all ten printed values of the 5 % downgrade column equal the manual's own formula for a 4 % downgrade, not 5 %"),
        (GradeDirection::Downgrade, j) => Entry::V(DOWN[i][j]),
        (GradeDirection::Upgrade, j) => Entry::V(UP[i][j]),
    };
    let mut c = e.get(&format!("the stopping sight distance at {speed_kmh} km/h on a {grade_percent} % grade"), src)?;
    c.unit = "m";
    Ok(c)
}

/// One column of Table 6.21, elements of safe passing sight distance on two lane roads.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PassingElements {
    /// Speed group, km/h: low and high (50-65, 66-80, 81-95, 96-110).
    pub speed_group_kmh: (f64, f64),
    pub average_passing_speed_kmh: f64,
    /// Average acceleration a, km/h/s.
    pub acceleration_kmh_per_s: f64,
    /// Time of initial manoeuvre t1, s.
    pub t1_s: f64,
    /// Distance travelled in the initial manoeuvre d1, m.
    pub d1_m: f64,
    /// Time the passing vehicle occupies the right lane t2, s.
    pub t2_s: f64,
    pub d2_m: f64,
    /// Clearance length d3, m.
    pub d3_m: f64,
    /// Distance travelled by the opposing vehicle d4, m.
    pub d4_m: f64,
    /// Total d1 + d2 + d3 + d4, m.
    pub total_m: f64,
}

/// Table 6.21 (printed p. 132, PDF p. 141): the column whose speed group holds `passing_speed_kmh`. Speeds in the
/// gaps between groups (65 to 66, 80 to 81, 95 to 96) and below 50 or above 110 km/h are refused.
pub fn passing_elements(passing_speed_kmh: f64) -> Result<Cited<PassingElements>, DesignError> {
    let groups = [
        PassingElements { speed_group_kmh: (50.0, 65.0), average_passing_speed_kmh: 56.2, acceleration_kmh_per_s: 2.25, t1_s: 3.6, d1_m: 45.0, t2_s: 9.3, d2_m: 145.0, d3_m: 30.0, d4_m: 95.0, total_m: 315.0 },
        PassingElements { speed_group_kmh: (66.0, 80.0), average_passing_speed_kmh: 70.0, acceleration_kmh_per_s: 2.30, t1_s: 4.0, d1_m: 65.0, t2_s: 10.0, d2_m: 195.0, d3_m: 55.0, d4_m: 130.0, total_m: 445.0 },
        PassingElements { speed_group_kmh: (81.0, 95.0), average_passing_speed_kmh: 84.5, acceleration_kmh_per_s: 2.37, t1_s: 4.3, d1_m: 90.0, t2_s: 10.7, d2_m: 250.0, d3_m: 75.0, d4_m: 165.0, total_m: 580.0 },
        PassingElements { speed_group_kmh: (96.0, 110.0), average_passing_speed_kmh: 99.80, acceleration_kmh_per_s: 2.41, t1_s: 4.5, d1_m: 110.0, t2_s: 11.3, d2_m: 315.0, d3_m: 90.0, d4_m: 210.0, total_m: 725.0 },
    ];
    match groups.into_iter().find(|g| passing_speed_kmh >= g.speed_group_kmh.0 && passing_speed_kmh <= g.speed_group_kmh.1) {
        Some(g) => Ok(Cited::new(g, "m", Source::new("Table 6.21", "6.7.3", 132, 141))),
        None => refuse(format!("Table 6.21 has speed groups 50-65, 66-80, 81-95 and 96-110 km/h; {passing_speed_kmh} km/h is in none of them.")),
    }
}

/// `d2 = 0.278 v t2` (section 6.7.3, printed p. 131, PDF p. 140), v in km/h, t2 in s.
pub fn passing_d2_m(average_speed_kmh: f64, t2_s: f64) -> f64 {
    0.278 * average_speed_kmh * t2_s
}

/// `d4 = 2/3 d2` (section 6.7.3, p. 130; the formula on p. 132 is printed `d3 = 2/3 d2`, a slip in the letter).
pub fn passing_d4_m(d2_m: f64) -> f64 {
    2.0 / 3.0 * d2_m
}

/// `d1 = 0.278 t1 (v - m + a t1 / 2)` (p. 131): v average speed of the passing vehicle (km/h), m the speed
/// difference of the passed and passing vehicles (km/h), a the average acceleration (km/h/s), t1 in s. The
/// manual states no value of m.
pub fn passing_d1_m(average_speed_kmh: f64, speed_difference_kmh: f64, acceleration_kmh_per_s: f64, t1_s: f64) -> f64 {
    0.278 * t1_s * (average_speed_kmh - speed_difference_kmh + acceleration_kmh_per_s * t1_s / 2.0)
}

/// One row of Table 6.22.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PassingSightRow {
    pub design_speed_kmh: f64,
    pub passed_vehicle_speed_kmh: f64,
    pub passing_vehicle_speed_kmh: f64,
    /// "Minimum passing sight distance for design", m.
    pub passing_sight_distance_m: f64,
}

/// Table 6.22 (printed p. 132, PDF p. 141): minimum passing sight distance for design on a two lane road, by
/// design speed (eye height 1.07 m, object height 1.3 m).
pub fn minimum_passing_sight_distance(speed_kmh: f64) -> Result<Cited<PassingSightRow>, DesignError> {
    const PASSED: [f64; 10] = [29.0, 36.0, 44.0, 51.0, 59.0, 65.0, 73.0, 79.0, 85.0, 91.0];
    const PASSING: [f64; 10] = [44.0, 51.0, 59.0, 66.0, 74.0, 80.0, 88.0, 94.0, 100.0, 106.0];
    const PSD: [f64; 10] = [217.0, 285.0, 345.0, 407.0, 482.0, 541.0, 605.0, 670.0, 728.0, 792.0];
    let i = speed_index(speed_kmh)?;
    Ok(Cited::new(
        PassingSightRow { design_speed_kmh: speed_kmh, passed_vehicle_speed_kmh: PASSED[i], passing_vehicle_speed_kmh: PASSING[i], passing_sight_distance_m: PSD[i] },
        "m",
        Source::new("Table 6.22", "6.7.3", 132, 141),
    ))
}

/// Meeting sight distance (section 6.7.5, printed p. 133, PDF p. 142): "the sum of the respective stopping sight
/// distances for the two vehicles plus 10.0 m safety distance", for two vehicles in opposite directions on a two
/// lane rural road with insufficient width for passing.
pub fn meeting_sight_distance_m(stopping_sight_distance_1_m: f64, stopping_sight_distance_2_m: f64) -> Cited<f64> {
    Cited::new(stopping_sight_distance_1_m + stopping_sight_distance_2_m + 10.0, "m", Source::new("text", "6.7.5", 133, 142))
}
