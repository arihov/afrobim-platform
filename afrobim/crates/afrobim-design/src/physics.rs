// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Physics relations used by the design checks, each derived here from first principles.
//!
//! These contain **no standard's values**. Friction factors, reaction times, eye and object
//! heights, headlight height and beam angle, superelevation limits and design speeds are
//! arguments. The only constant is standard gravity, `g = 9.80665 m/s²` (the conventional value
//! fixed by the CGPM in 1901).
//!
//! # Horizontal curve: `e + f = V² / (127 R)`
//!
//! A vehicle at speed `v` (m/s) round a curve of radius `R` (m) needs the centripetal
//! acceleration `v²/R`. A road banked at cross-fall `e` (a ratio, `tan` of the bank angle, taken
//! equal to `sin` for the small angles of roads) supplies `g e` of it by gravity; the rest,
//! `v²/R - g e`, must come from side friction, `f g` with `f` the side-friction factor demanded:
//!
//! ```text
//!   v² / R = g (e + f)           so   e + f = v² / (g R)
//! ```
//!
//! With the speed in km/h, `V = 3.6 v`, `v² = V² / 12.96`, hence `e + f = V² / (12.96 g R)`,
//! and `12.96 × 9.80665 = 127.10`. The familiar "127" is this number rounded; the code uses the
//! unrounded value, which differs by under 0.1 %.
//!
//! # Stopping distance
//!
//! Distance covered during the reaction time `t` at constant speed, `v t`, plus the braking
//! distance. Braking at deceleration `a = g (f + G)` on a grade `G` (ratio, positive uphill,
//! `f` the longitudinal friction factor) stops from `v` in `v² / (2a)`:
//!
//! ```text
//!   S = v t + v² / (2 g (f + G))     with V in km/h:  S = V t / 3.6 + V² / (254.2 (f + G))
//! ```
//!
//! (`2 × 12.96 × 9.80665 = 254.19`; the familiar 254 is this rounded.)
//!
//! # Crest curve: sight distance and K
//!
//! Take the parabola's tangent at the vertex as horizontal with the curve falling away,
//! `y = -A x² / (2L)` for a grade change `A` (a ratio, magnitude). A straight line of sight that
//! just touches the curve at abscissa `xt` lies above the curve by `A (x - xt)² / (2L)` at `x`.
//! The eye is `h1` above the road at distance `a` before `xt` and the object `h2` above it at
//! `b` beyond, so `A (a)² / (2L) = h1` and `A (b)² / (2L) = h2` (measuring from `xt`), giving
//! `a = √(2 L h1 / A)`, `b = √(2 L h2 / A)` and the sight distance
//!
//! ```text
//!   S = a + b = √(2L/A) (√h1 + √h2)        so   L = A S² / (2 (√h1 + √h2)²)
//! ```
//!
//! independent of where `xt` is. In terms of `K = L / (100 A)` (A as a ratio, so `100 A` is the
//! percentage change): `K = S² / (200 (√h1 + √h2)²)`. This holds while `S ≤ L`; for longer sight
//! distances the line of sight touches the curve beyond its ends and the relation differs. This
//! crate therefore uses the closed form only to *size* a curve for a stated `S`, and
//! checks real curves by the numerical sight line test ([`crest_sight_clear`]), which has no such
//! restriction.
//!
//! # Sag curve: headlight sight distance and K
//!
//! The beam leaves a point `H` above the road at an upward angle `α` to the vehicle's grade. Shear
//! the picture so the grade line at the vehicle is horizontal: the road then rises ahead of it as
//! `A x² / (2L)` and the beam is `H + x tan α`. The beam meets the road at `x = S` where
//! `A S² / (2L) = H + S tan α`, so
//!
//! ```text
//!   L = A S² / (2 (H + S tan α))     K = S² / (200 (H + S tan α))
//! ```
//!
//! again for `S ≤ L`, with the small-angle simplification that shearing leaves the beam angle
//! relative to the grade unchanged. The numerical test [`sag_sight_distance`] has neither limit.

use afrobim_alignment::Profile;

/// Conventional standard gravity, m/s².
pub const STANDARD_GRAVITY: f64 = 9.80665;

fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

/// Smallest radius for speed `v_kmh`, superelevation `e` and side-friction factor `f` (both
/// ratios), `R = V² / (12.96 g (e + f))`. `None` unless `e + f > 0` and the speed is positive.
pub fn min_radius_m(v_kmh: f64, e: f64, f: f64) -> Option<f64> {
    (positive(v_kmh) && (e + f) > 0.0 && (e + f).is_finite()).then(|| v_kmh * v_kmh / (12.96 * STANDARD_GRAVITY * (e + f)))
}

/// The side-friction factor demanded on a curve of radius `r_m` at `v_kmh` with superelevation `e`:
/// `f = V² / (12.96 g R) - e`.
pub fn side_friction_demand(v_kmh: f64, r_m: f64, e: f64) -> Option<f64> {
    (positive(v_kmh) && positive(r_m)).then(|| v_kmh * v_kmh / (12.96 * STANDARD_GRAVITY * r_m) - e)
}

/// Stopping distance, m: reaction time `t_s` seconds at `v_kmh`, then braking with longitudinal
/// friction factor `f` on a grade `grade` (ratio, positive uphill). `None` if `f + grade <= 0`.
pub fn stopping_distance_m(v_kmh: f64, t_s: f64, f: f64, grade: f64) -> Option<f64> {
    let v = v_kmh / 3.6;
    (v_kmh.is_finite() && v >= 0.0 && t_s >= 0.0 && (f + grade) > 0.0).then(|| v * t_s + v * v / (2.0 * STANDARD_GRAVITY * (f + grade)))
}

/// K for a crest curve giving sight distance `s_m` with eye height `h1` and object height `h2`
/// (valid for `S ≤ L`): `K = S² / (200 (√h1 + √h2)²)`.
pub fn crest_k_for_sight(s_m: f64, h1_m: f64, h2_m: f64) -> Option<f64> {
    (positive(s_m) && positive(h1_m) && h2_m.is_finite() && h2_m >= 0.0).then(|| s_m * s_m / (200.0 * (h1_m.sqrt() + h2_m.sqrt()).powi(2)))
}

/// K for a sag curve giving headlight sight distance `s_m` with headlight height `h_m` and
/// upward beam angle `beam_deg` (valid for `S ≤ L`): `K = S² / (200 (H + S tan α))`.
pub fn sag_k_for_sight(s_m: f64, h_m: f64, beam_deg: f64) -> Option<f64> {
    (positive(s_m) && positive(h_m) && beam_deg.is_finite()).then(|| s_m * s_m / (200.0 * (h_m + s_m * beam_deg.to_radians().tan())))
}

/// True if a line of sight from an eye `h1` above the road at `eye_station` to a point `h2`
/// above it at `eye_station + s_m` clears the profile all the way, tested every `step_m`.
/// Stations outside the profile count as clear (the profile says nothing there).
pub fn crest_sight_clear(profile: &Profile, eye_station: f64, s_m: f64, h1_m: f64, h2_m: f64, step_m: f64) -> bool {
    let Some((lo, hi)) = profile.range() else { return true };
    let (a, b) = (eye_station, eye_station + s_m);
    let (Ok(za), Ok(zb)) = (profile.elevation_at(a.clamp(lo, hi)), profile.elevation_at(b.clamp(lo, hi))) else { return true };
    if a < lo || b > hi {
        return true;
    }
    let (ya, yb) = (za + h1_m, zb + h2_m);
    let n = ((s_m / step_m).ceil() as usize).max(1);
    (1..n).all(|i| {
        let t = i as f64 / n as f64;
        let s = a + t * s_m;
        profile.elevation_at(s).map_or(true, |z| z <= ya + t * (yb - ya) + 1e-9)
    })
}

/// The smallest sight distance over all eye positions between `from_station` and `to_station`
/// (every `step_m`), where for each eye position the distance is the longest object distance up
/// to `cap_m` (in steps of `step_m`) for which the sight line is clear. Returns
/// `(distance, eye_station)` of the worst case. A distance equal to `cap_m` means "at least `cap_m`".
///
/// The profile is read once on a grid of `step_m`. For an eye at `a` and height `h1`, a point
/// on the profile at `s_k` blocks an object at `s_j` when its slope seen from the eye,
/// `(z_k - (z_a + h1)) / (s_k - a)`, exceeds the slope of the line to the object,
/// `(z_j + h2 - (z_a + h1)) / (s_j - a)`; so one running maximum over `k` serves every `j`.
pub fn crest_available_sight_distance(profile: &Profile, from_station: f64, to_station: f64, h1_m: f64, h2_m: f64, cap_m: f64, step_m: f64) -> (f64, f64) {
    let Some((lo, hi)) = profile.range() else { return (cap_m, from_station) };
    let n_eyes = (((to_station - from_station) / step_m).floor().max(0.0) as usize) + 1;
    let n_obj = (cap_m / step_m).floor() as usize;
    let n = n_eyes + n_obj + 1;
    // Levels on the grid; None outside the profile.
    let z: Vec<Option<f64>> = (0..n)
        .map(|i| {
            let s = from_station + i as f64 * step_m;
            (s >= lo && s <= hi).then(|| profile.elevation_at(s).ok()).flatten()
        })
        .collect();
    let mut worst = (cap_m, from_station);
    for i in 0..n_eyes {
        let Some(za) = z[i] else { continue };
        let ya = za + h1_m;
        let mut max_slope = f64::NEG_INFINITY;
        let mut avail = cap_m;
        for j in 1..=n_obj {
            let Some(zj) = z.get(i + j).copied().flatten() else { break };
            let d = j as f64 * step_m;
            if max_slope > (zj + h2_m - ya) / d + 1e-12 {
                avail = (j - 1) as f64 * step_m;
                break;
            }
            max_slope = max_slope.max((zj - ya) / d);
        }
        if avail < worst.0 {
            worst = (avail, from_station + i as f64 * step_m);
        }
    }
    worst
}

/// Headlight sight distance at one eye station on a sag: the distance at which a beam from `h_m`
/// above the road, rising at `beam_deg` to the grade at that point, first meets the road surface,
/// searched out to `cap_m`. `cap_m` is returned when it does not meet the road within it.
pub fn sag_sight_distance(profile: &Profile, station: f64, h_m: f64, beam_deg: f64, cap_m: f64, step_m: f64) -> f64 {
    let Ok(p) = profile.point_at(station) else { return cap_m };
    let slope = p.grade + beam_deg.to_radians().tan();
    let mut s = step_m;
    let (_, hi) = profile.range().unwrap_or((0.0, 0.0));
    while s <= cap_m + 1e-9 {
        if station + s > hi {
            return cap_m;
        }
        let beam = p.elevation_m + h_m + slope * s;
        match profile.elevation_at(station + s) {
            Ok(z) if z >= beam => {
                // Interpolate between the last clear step and this one.
                let prev = s - step_m;
                let beam_prev = p.elevation_m + h_m + slope * prev;
                let z_prev = profile.elevation_at(station + prev).unwrap_or(beam_prev - 1.0);
                let (g0, g1) = (beam_prev - z_prev, beam - z);
                return if (g0 - g1).abs() < 1e-12 { s } else { prev + step_m * g0 / (g0 - g1) };
            }
            _ => {}
        }
        s += step_m;
    }
    cap_m
}
