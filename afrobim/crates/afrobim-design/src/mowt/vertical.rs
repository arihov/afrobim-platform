// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Vertical alignment: `K` values of crest and sag curves (Tables 6-11, 6-12, 6-13) and the formulas the
//! manual gives for the minimum length of a vertical curve, maximum gradients (Tables 6-14, 6-15),
//! the minimum gradient, critical length of gradient (Table 6-16) and climbing lanes (Tables 6-17, 6-18).
//!
//! The manual gives **no** absolute minimum length of vertical curve in metres: Figures 6-24 to 6-27 plot
//! minimum length against the algebraic difference of grades and show a cut-off at small differences that
//! cannot be read to a stated value, so none is built in. The rule of thumb "L = 30 G, K = 30" for
//! "general and good appearance" (p. 111) is a general rule, not a stated minimum, and is given here as a
//! constant only.

use serde::Serialize;

use super::classes::Terrain;
use super::{refuse, speed_index, Cited, Entry, Source};
use crate::error::DesignError;

/// One row of Tables 6-11 (crest, stopping) or 6-13 (sag, stopping): `K`, the length in metres per percent
/// of algebraic difference of grades. The tables print a range for each design speed, from the assumed speed
/// for condition (lower end) to the design speed (upper end).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KRow {
    pub design_speed_kmh: f64,
    /// "Assumed Speed for Condition", km/h: low and high.
    pub assumed_speed_kmh: (f64, f64),
    /// Coefficient of friction f.
    pub friction: f64,
    /// "Stopping sight distance for Design", m: low and high.
    pub stopping_sight_distance_m: (f64, f64),
    /// "Computed" K, low and high.
    pub k_computed: (f64, f64),
    /// "Rounded for Design" K, low and high. The class tables (4-3 to 4-9) carry the **low** end.
    pub k_rounded: (f64, f64),
}

// speed, assumed (lo, hi), f, ssd (lo, hi)
const ASSUMED: [(f64, f64); 10] = [(30.0, 30.0), (40.0, 40.0), (47.0, 50.0), (55.0, 60.0), (63.0, 70.0), (70.0, 80.0), (77.0, 90.0), (85.0, 100.0), (91.0, 110.0), (98.0, 120.0)];
const FRICTION: [f64; 10] = [0.40, 0.38, 0.35, 0.33, 0.31, 0.30, 0.30, 0.29, 0.28, 0.28];
const SSD: [(f64, f64); 10] = [(29.7, 29.7), (44.4, 44.4), (57.5, 62.8), (74.3, 84.6), (94.2, 110.8), (112.9, 139.5), (131.3, 168.8), (157.0, 205.2), (179.6, 246.5), (203.1, 285.8)];
const CREST_COMPUTED: [(f64, f64); 10] = [(2.18, 2.18), (4.88, 4.88), (8.18, 9.76), (13.66, 17.72), (21.96, 30.39), (31.55, 48.17), (42.67, 70.53), (61.01, 104.23), (79.84, 150.40), (102.10, 202.18)];
const CREST_ROUNDED: [(f64, f64); 10] = [(3.0, 3.0), (5.0, 5.0), (9.0, 10.0), (14.0, 18.0), (22.0, 31.0), (32.0, 49.0), (43.0, 71.0), (62.0, 105.0), (80.0, 151.0), (103.0, 203.0)];
const SAG_COMPUTED: [(f64, f64); 10] = [(3.94, 3.94), (7.16, 7.16), (10.29, 11.61), (14.53, 17.20), (19.73, 24.18), (24.74, 31.99), (29.75, 40.09), (36.82, 50.24), (43.09, 61.83), (49.65, 72.91)];
const SAG_ROUNDED: [(f64, f64); 10] = [(4.0, 4.0), (8.0, 8.0), (11.0, 12.0), (15.0, 18.0), (20.0, 25.0), (25.0, 32.0), (30.0, 41.0), (37.0, 51.0), (44.0, 62.0), (50.0, 73.0)];

fn k_row(speed_kmh: f64, computed: &[(f64, f64); 10], rounded: &[(f64, f64); 10], source: Source) -> Result<Cited<KRow>, DesignError> {
    let i = speed_index(speed_kmh)?;
    Ok(Cited::new(
        KRow { design_speed_kmh: speed_kmh, assumed_speed_kmh: ASSUMED[i], friction: FRICTION[i], stopping_sight_distance_m: SSD[i], k_computed: computed[i], k_rounded: rounded[i] },
        "K (m per % of G)",
        source,
    ))
}

/// Table 6-11 (printed p. 108, PDF p. 117): `K` of a crest vertical curve with stopping sight distance as
/// the control, eye height 1.07 m and object height 0.15 m.
pub fn crest_k_stopping(speed_kmh: f64) -> Result<Cited<KRow>, DesignError> {
    k_row(speed_kmh, &CREST_COMPUTED, &CREST_ROUNDED, Source::new("Table 6-11", "6.3.1", 108, 117))
}

/// Table 6-13 (printed p. 111, PDF p. 120): `K` of a sag vertical curve with the headlight sight distance
/// equal to the stopping sight distance (headlight height 0.6 m, upward beam divergence 1 degree).
pub fn sag_k(speed_kmh: f64) -> Result<Cited<KRow>, DesignError> {
    k_row(speed_kmh, &SAG_COMPUTED, &SAG_ROUNDED, Source::new("Table 6-13", "6.3.2", 111, 120))
}

/// One row of Table 6-12: `K` of a crest curve with passing sight distance as the control.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PassingKRow {
    pub design_speed_kmh: f64,
    /// "Minimum Passing sight distance for Design", m.
    pub passing_sight_distance_m: f64,
    pub k_computed: f64,
    pub k_rounded: f64,
}

/// Table 6-12 (printed p. 110, PDF p. 119), eye height 1.07 m, object height 1.3 m. The printed title says
/// "based on stopping sight distance" but the table is the passing one (slip).
pub fn crest_k_passing(speed_kmh: f64) -> Result<Cited<PassingKRow>, DesignError> {
    const PSD: [f64; 10] = [217.0, 285.0, 345.0, 407.0, 482.0, 541.0, 605.0, 670.0, 728.0, 792.0];
    const COMPUTED: [f64; 10] = [49.78, 85.86, 125.82, 175.10, 245.59, 309.39, 386.92, 474.52, 560.24, 663.07];
    const ROUNDED: [f64; 10] = [50.0, 86.0, 126.0, 176.0, 246.0, 310.0, 387.0, 475.0, 561.0, 664.0];
    let i = speed_index(speed_kmh)?;
    Ok(Cited::new(
        PassingKRow { design_speed_kmh: speed_kmh, passing_sight_distance_m: PSD[i], k_computed: COMPUTED[i], k_rounded: ROUNDED[i] },
        "K (m per % of G)",
        Source::new("Table 6-12", "6.3.1", 110, 119),
    ))
}

// ---- formulas ------------------------------------------------------------------------------

/// Eye height for stopping and passing sight distance, m (sections 6.3.1 and 6.7.2, pp. 107 and 128).
pub const EYE_HEIGHT_M: f64 = 1.07;
/// Object height for stopping sight distance on crests, m (pp. 107 and 128).
pub const OBJECT_HEIGHT_STOPPING_M: f64 = 0.15;
/// Object height for passing sight distance on crests, m (section 6.3.1, p. 109; section 6.7.3, p. 130).
pub const OBJECT_HEIGHT_PASSING_M: f64 = 1.3;
/// Headlight height for sag curves, m (section 6.3.2, p. 110).
pub const HEADLIGHT_HEIGHT_M: f64 = 0.6;
/// Upward divergence of the headlight beam, degrees (section 6.3.2, p. 110).
pub const HEADLIGHT_BEAM_DEG: f64 = 1.0;

fn positive(g: f64, s: f64) -> Result<(), DesignError> {
    if g > 0.0 && s > 0.0 {
        Ok(())
    } else {
        refuse("The algebraic difference of grades and the sight distance must be positive.")
    }
}

/// Section 6.3.1 (printed p. 107, PDF p. 116): minimum length of a crest curve for stopping sight distance
/// `S` (m) and algebraic difference of grades `G` (%), with eye height 1.07 m and object height 0.15 m, in the
/// manual's simplified form: `L = G S^2 / 404` when S is less than L, `L = 2 S - 404 / G` when S is greater
/// than L. (The general form has `100 (sqrt(2 h1) + sqrt(2 h2))^2` = 404.2 in place of 404.)
pub fn crest_min_length_stopping_m(sight_distance_m: f64, g_percent: f64) -> Result<Cited<f64>, DesignError> {
    positive(g_percent, sight_distance_m)?;
    let l1 = g_percent * sight_distance_m * sight_distance_m / 404.0;
    let l = if sight_distance_m <= l1 { l1 } else { 2.0 * sight_distance_m - 404.0 / g_percent };
    Ok(Cited::new(l, "m", Source::new("text", "6.3.1", 107, 116)))
}

/// The manual's general crest formulas (p. 107): `L = G S^2 / (100 (sqrt(2 h1) + sqrt(2 h2))^2)` when S is
/// less than L, `L = 2 S - 200 (sqrt h1 + sqrt h2)^2 / G` when S is greater than L; heights in m.
pub fn crest_min_length_general_m(sight_distance_m: f64, g_percent: f64, eye_height_m: f64, object_height_m: f64) -> Result<Cited<f64>, DesignError> {
    positive(g_percent, sight_distance_m)?;
    let c = (2.0 * eye_height_m).sqrt() + (2.0 * object_height_m).sqrt();
    let l1 = g_percent * sight_distance_m * sight_distance_m / (100.0 * c * c);
    let l = if sight_distance_m <= l1 { l1 } else { 2.0 * sight_distance_m - 200.0 * (eye_height_m.sqrt() + object_height_m.sqrt()).powi(2) / g_percent };
    Ok(Cited::new(l, "m", Source::new("text", "6.3.1", 107, 116)))
}

/// Section 6.3.1 (printed p. 109, PDF p. 118): minimum length of a crest curve for passing sight distance `S`,
/// eye height 1.07 m and object height 1.3 m: `L = G S^2 / 946` when S is less than L, `L = 2 S - 946 / G`
/// when S is greater than L.
pub fn crest_min_length_passing_m(sight_distance_m: f64, g_percent: f64) -> Result<Cited<f64>, DesignError> {
    positive(g_percent, sight_distance_m)?;
    let l1 = g_percent * sight_distance_m * sight_distance_m / 946.0;
    let l = if sight_distance_m <= l1 { l1 } else { 2.0 * sight_distance_m - 946.0 / g_percent };
    Ok(Cited::new(l, "m", Source::new("text", "6.3.1", 109, 118)))
}

/// Section 6.3.2 (printed p. 111, PDF p. 120): minimum length of a sag curve for the headlight sight
/// distance `S` (the stopping sight distance), headlight height 0.6 m and beam divergence 1 degree, in the
/// manual's simplified form: `L = G S^2 / (120 + 3.5 S)` when S is less than L, `L = 2 S - (120 + 3.5 S) / G`
/// when S is greater than L.
pub fn sag_min_length_m(sight_distance_m: f64, g_percent: f64) -> Result<Cited<f64>, DesignError> {
    positive(g_percent, sight_distance_m)?;
    let d = 120.0 + 3.5 * sight_distance_m;
    let l1 = g_percent * sight_distance_m * sight_distance_m / d;
    let l = if sight_distance_m <= l1 { l1 } else { 2.0 * sight_distance_m - d / g_percent };
    Ok(Cited::new(l, "m", Source::new("text", "6.3.2", 111, 120)))
}

/// The manual's general sag formulas (printed p. 110, PDF p. 119): `L = G S^2 / (200 (h1 + S tan(theta)))`
/// when S is less than L, `L = 2 S - 200 (h1 + S tan(theta)) / G` when S is greater than L.
pub fn sag_min_length_general_m(sight_distance_m: f64, g_percent: f64, headlight_height_m: f64, beam_deg: f64) -> Result<Cited<f64>, DesignError> {
    positive(g_percent, sight_distance_m)?;
    let d = 200.0 * (headlight_height_m + sight_distance_m * beam_deg.to_radians().tan());
    let l1 = g_percent * sight_distance_m * sight_distance_m / d;
    let l = if sight_distance_m <= l1 { l1 } else { 2.0 * sight_distance_m - d / g_percent };
    Ok(Cited::new(l, "m", Source::new("text", "6.3.2", 110, 119)))
}

/// Section 6.3.2 (printed p. 111, PDF p. 120): the sag length for rider comfort (centripetal acceleration not
/// above 0.3 m/s^2), `L = G V^2 / 395`, V the design speed in km/h, G in percent.
pub fn sag_comfort_length_m(speed_kmh: f64, g_percent: f64) -> Cited<f64> {
    Cited::new(g_percent * speed_kmh * speed_kmh / 395.0, "m", Source::new("text", "6.3.2", 111, 120))
}

/// "For general and good appearance, as a rule-of-thumb for length of vertical curve L = 30 G, and K = 30
/// could be used" (section 6.3.2, p. 111, PDF p. 120).
pub fn appearance_rule_of_thumb_k() -> Cited<f64> {
    Cited::new(30.0, "K", Source::new("text", "6.3.2", 111, 120))
}

/// "The criterion is providing a minimum grade of 0.30 percent within the 15 m of the level point"
/// (drainage of curbed sections, sag and crest curves; section 6.3.2, p. 111, PDF p. 120): `(grade %, distance m)`.
pub fn kerbed_drainage_minimum_grade() -> Cited<(f64, f64)> {
    Cited::new((0.30, 15.0), "% within m of the level point", Source::new("text", "6.3.2", 111, 120))
}

// ---- gradients -------------------------------------------------------------------------------

const GRADE_SPEEDS: [f64; 9] = [40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 110.0, 120.0];
const D: Entry = Entry::Dash;
const fn v(x: f64) -> Entry {
    Entry::V(x)
}

const DESIRABLE: [[Entry; 9]; 3] = [
    [D, D, v(5.0), v(4.0), v(4.0), v(3.5), v(3.0), v(3.0), v(3.0)],
    [D, v(7.0), v(6.0), v(5.5), v(5.0), v(4.5), v(4.5), D, D],
    [v(10.0), v(9.0), v(8.0), v(7.0), D, D, D, D, D],
];
const ABSOLUTE: [[Entry; 9]; 3] = [
    [D, D, D, v(6.0), v(6.0), v(5.5), v(5.0), v(4.0), v(4.0)],
    [D, v(9.0), v(8.0), v(7.5), v(7.0), v(6.5), D, D, D],
    [v(12.0), v(11.0), v(10.0), v(9.0), D, D, D, D, D],
];

fn grade_entry(table: &[[Entry; 9]; 3], terrain: Terrain, speed_kmh: f64, name: &'static str, printed: u16, pdf: u16) -> Result<Cited<f64>, DesignError> {
    let row = match terrain {
        Terrain::Flat => 0,
        Terrain::Rolling => 1,
        Terrain::Mountainous => 2,
        other => return refuse(format!("{name} has rows for flat, rolling and mountainous terrain only, not {}.", other.manual_name())),
    };
    let Some(col) = GRADE_SPEEDS.iter().position(|&s| (s - speed_kmh).abs() < 1e-9) else {
        return refuse(format!("{name} has columns for design speeds 40 to 120 km/h; {speed_kmh} km/h is not one of them."));
    };
    let src = Source::new(name, "6.5.1", printed, pdf);
    let mut c = table[row][col].get(&format!("the maximum gradient for {} terrain at {speed_kmh} km/h", terrain.manual_name()), src)?;
    c.unit = "%";
    Ok(c)
}

/// Table 6-14 (printed p. 122, PDF p. 131): maximum gradient (desirable), percent, by terrain and design speed.
/// A dash in the table is refused. Section 6.5.1 says the design standards of Tables 4-3 to 4-9 "include both
/// desirable and absolute maximum values for different terrain condition"; where those differ from Tables 6-14
/// and 6-15 the difference is listed in [`super::known_slips`].
pub fn max_gradient_desirable(terrain: Terrain, speed_kmh: f64) -> Result<Cited<f64>, DesignError> {
    grade_entry(&DESIRABLE, terrain, speed_kmh, "Table 6-14", 122, 131)
}

/// Table 6-15 (printed p. 122, PDF p. 131): absolute maximum gradient, percent. "Employment of a gradient in
/// excess of the desirable maximum can only be authorized through the employment of a Departure from
/// Standard" (6.5.1, p. 123).
pub fn max_gradient_absolute(terrain: Terrain, speed_kmh: f64) -> Result<Cited<f64>, DesignError> {
    grade_entry(&ABSOLUTE, terrain, speed_kmh, "Table 6-15", 122, 131)
}

/// Section 6.5.2 (printed p. 123, PDF p. 132): "The minimum gradient in cuttings in order to avoid standing
/// water in the road side ditches is 0.3 - 0.5 percent." The class tables give 0.5 % ("Minimum Gradient in cut").
/// The rule is for cuttings; flat and level gradients on uncurbed paved roads are acceptable where the cross
/// slope and the carriageway level above the surrounding ground drain the surface.
pub fn min_gradient_in_cuttings_range_percent() -> Cited<(f64, f64)> {
    Cited::new((0.3, 0.5), "%", Source::new("text", "6.5.2", 123, 132))
}

/// Table 6-16 (printed p. 123, PDF p. 132): maximum desirable length of gradient, m, for the gradients 12, 11,
/// 10 and 9 percent (300, 350, 400, 450 m). Other gradients are refused: the table has no rule between its rows.
pub fn critical_length_of_gradient_m(gradient_percent: f64) -> Result<Cited<f64>, DesignError> {
    let v = match gradient_percent {
        g if (g - 12.0).abs() < 1e-9 => 300.0,
        g if (g - 11.0).abs() < 1e-9 => 350.0,
        g if (g - 10.0).abs() < 1e-9 => 400.0,
        g if (g - 9.0).abs() < 1e-9 => 450.0,
        g => return refuse(format!("Table 6-16 gives the maximum desirable length for gradients of 12, 11, 10 and 9 % only; {g} % is not a row and nothing is interpolated or extrapolated.")),
    };
    Ok(Cited::new(v, "m", Source::new("Table 6-16", "6.5.4", 123, 132)))
}

/// Table 6-17 (printed p. 125, PDF p. 134): the threshold combinations of average grade and length for which
/// a climbing lane is considered (design truck speed falling more than 20 km/h below 80 km/h): average grade
/// 2 % 1500 m, 3 % 500 m, 4 % 300 m. The printed title of the table, "Maximum Gradients (Desirable)", is a slip.
pub fn climbing_lane_threshold_length_m(average_grade_percent: f64) -> Result<Cited<f64>, DesignError> {
    let v = match average_grade_percent {
        g if (g - 2.0).abs() < 1e-9 => 1500.0,
        g if (g - 3.0).abs() < 1e-9 => 500.0,
        g if (g - 4.0).abs() < 1e-9 => 300.0,
        g => return refuse(format!("Table 6-17 gives average grades of 2, 3 and 4 % only; {g} % is not a row.")),
    };
    Ok(Cited::new(v, "m", Source::new("Table 6-17", "6.6", 125, 134)))
}

/// Table 6-18 (printed p. 125, PDF p. 134): entry and exit taper lengths of a climbing lane for a speed limit of
/// 80 km/h (150 m, 200 m) or 100 km/h (200 m, 300 m). The printed title, "Absolute Maximum Gradients", is a slip.
pub fn climbing_lane_tapers_m(speed_limit_kmh: f64) -> Result<Cited<(f64, f64)>, DesignError> {
    let v = match speed_limit_kmh {
        s if (s - 80.0).abs() < 1e-9 => (150.0, 200.0),
        s if (s - 100.0).abs() < 1e-9 => (200.0, 300.0),
        s => return refuse(format!("Table 6-18 gives tapers for speed limits of 80 and 100 km/h only; {s} km/h is not a row.")),
    };
    Ok(Cited::new(v, "m (entry, exit)", Source::new("Table 6-18", "6.6", 125, 134)))
}
