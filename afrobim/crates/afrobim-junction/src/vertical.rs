// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Vertical part of a junction: crossfall, the secondary profile locked to the primary, edge
//! levels, the blend of a return's profile from its two edges, and low points.
//!
//! # Conventions
//!
//! Cross-fall is in percent, **positive where the surface rises away from the centreline** (a
//! crowned road is negative on both sides), as in `afrobim-alignment` and `afrobim-corridor`.
//! The surface level at a plan point `p` of a road with a profile is
//!
//! ```text
//!   z(p) = z_profile(s) + (crossfall_side(s) / 100) * |offset|
//! ```
//!
//! where `(s, offset)` is the station and offset of `p` on the road alignment, and `side` is the
//! side of the alignment `p` is on. It is a plane on each side of the crown, through the profile
//! level on the alignment.
//!
//! # The secondary profile locked to the primary
//!
//! With the primary road's crown maintained, the secondary road's centreline level across the
//! primary carriageway is the primary road's surface along the secondary centreline. Let the
//! secondary centreline cross the primary's left and right edge alignments at secondary stations
//! `lo` and `hi`, and the primary's centreline at `sc`. The locked profile has PVIs (grade breaks,
//! no vertical curves) at `lo`, `sc` and `hi`, with levels `z(p)` above, and optionally at a regular
//! spacing between them (the primary surface along a skewed line is piecewise planar only if
//! the primary grade is constant; the extra PVIs follow a vertical curve of the primary). The
//! entered tie-in length `T` is the distance outside `lo` (and `hi`) at which the secondary returns
//! to its own profile: the PVIs of the original profile between `lo - T` and `hi + T` are replaced
//! by PVIs at `lo - T` and `hi + T` (at the original profile's level there) and the generated
//! ones, so the grade across the tie-in is `(z(lo) - z_orig(lo - T)) / T`, which is reported and
//! judged against nothing. An original PVI outside the tie-in keeps its vertical curve; if that
//! curve runs into the tie-in the profile is refused.
//!
//! # Blending a return's profile from its two edges (derivation)
//!
//! At the tangent points the return runs parallel to its edge, so it has the edge's level and the
//! edge's grade (sign-flipped where the return runs against the edge's stationing): levels `z0`,
//! `z1` and grades `g0`, `g1` along the return of length `L`. Two methods:
//!
//! 1. **One vertical curve (asymmetric parabola).** The tangent lines at the two ends,
//!    `z0 + g0 s` and `z1 + g1 (s - L)`, meet at `s* = (z1 - z0 - g1 L) / (g0 - g1)`. If
//!    `0 < s* < L` the profile has one PVI at `s*` (level `z0 + g0 s*`) carrying an asymmetric
//!    parabola from the first end to the last, `L1 = s*` and `L2 = L - s*`. That curve passes
//!    through both end levels with both end grades exactly (see `afrobim_alignment::Profile`).
//! 2. **Cubic (Hermite).** When the tangent lines meet outside the return (the profile has an
//!    inflection, or the grades are equal but the levels do not agree with them) no single curve
//!    of method 1 exists; the profile is then the cubic `z(t) = z0 h00 + L g0 h10 + z1 h01 + L g1 h11`,
//!    `t = s / L`, `h00 = 2t^3 - 3t^2 + 1`, `h10 = t^3 - 2t^2 + t`, `h01 = -2t^3 + 3t^2`,
//!    `h11 = t^3 - t^2`, which also meets both levels and both grades. The `Profile` type cannot
//!    hold a cubic, so it is given as PVIs at the entered spacing; the difference from the cubic is
//!    at most `|z''|max h^2 / 8` for spacing `h`.
//!
//! The straight case (`g0 = g1` and the levels on the line) is a two-PVI profile.
//!
//! # Low points
//!
//! The level along a return (or an edge) is scanned at 400 stations; each place the level falls
//! then rises is refined by golden-section search to 1e-9 m. A return with none has its low
//! point at the lower end, which is reported as `lowest_end`.

use afrobim_alignment::{Alignment, Profile, ProfileError, Pvi};
use afrobim_corridor::{Assembly, Side, SubKind};
use serde::{Deserialize, Serialize};

use crate::crossing::{find_crossings, Crossing};
use crate::error::JunctionError;
use crate::geom::{pt_at, Pt};

pub(crate) fn pe(e: ProfileError) -> JunctionError {
    JunctionError::Profile(e.to_string())
}

/// Where cross-fall comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossfallInput {
    /// Entered, percent, rising away from the centreline positive.
    Fixed { left_pct: f64, right_pct: f64 },
    /// The alignment's stored superelevation at the station (an error where it stores none).
    FromAlignment,
    /// The slope of the first lane on each side of this assembly.
    FromAssembly(Assembly),
}

/// The slope of the first `Lane` subassembly on the left and on the right of an assembly, percent.
pub fn crossfall_from_assembly(a: &Assembly) -> Result<(f64, f64), JunctionError> {
    let first = |list: &[afrobim_corridor::Subassembly], side: &str| {
        list.iter()
            .find_map(|s| match &s.kind {
                SubKind::Lane(strip) => Some(strip.slope.rise_per_m() * 100.0),
                _ => None,
            })
            .ok_or_else(|| JunctionError::input(format!("assembly '{}' has no lane on its {side} side to take a crossfall from", a.name)))
    };
    Ok((first(&a.left, "left")?, first(&a.right, "right")?))
}

impl CrossfallInput {
    /// The cross-fall on a side at a station of `road`, percent.
    pub fn at(&self, road: &Alignment, station_m: f64, side: Side) -> Result<f64, JunctionError> {
        match self {
            CrossfallInput::Fixed { left_pct, right_pct } => Ok(if side == Side::Left { *left_pct } else { *right_pct }),
            CrossfallInput::FromAssembly(a) => {
                let (l, r) = crossfall_from_assembly(a)?;
                Ok(if side == Side::Left { l } else { r })
            }
            CrossfallInput::FromAlignment => {
                let se = road.superelevation.as_ref().ok_or_else(|| JunctionError::input(format!("alignment '{}' stores no crossfall", road.name)))?;
                let f = se.cross_fall_at(station_m);
                let v = if side == Side::Left { f.left_pct } else { f.right_pct };
                v.ok_or_else(|| JunctionError::input(format!("alignment '{}' stores no crossfall on its {:?} side at station {station_m:.3}", road.name, side)))
            }
        }
    }
}

/// The surface of a road: its profile and cross-fall.
#[derive(Clone, Copy)]
pub struct SurfaceLevel<'a> {
    pub road: &'a Alignment,
    pub crossfall: &'a CrossfallInput,
}

impl SurfaceLevel<'_> {
    /// Level of the surface plane at a plan point, m. See the module documentation.
    pub fn at_point(&self, p: Pt) -> Result<f64, JunctionError> {
        let (s, o) = crate::geom::station_offset(self.road, p)?;
        self.at_station_offset(s, o)
    }
    pub fn at_station_offset(&self, s: f64, o: f64) -> Result<f64, JunctionError> {
        let prof = self.road.profile.as_ref().ok_or_else(|| JunctionError::NoProfile(self.road.name.clone()))?;
        let z = prof.elevation_at(s).map_err(pe)?;
        let side = if o >= 0.0 { Side::Right } else { Side::Left };
        Ok(z + self.crossfall.at(self.road, s, side)? / 100.0 * o.abs())
    }
}

/// Level along an edge alignment, as a function of the edge's station.
pub struct EdgeLevel<'a> {
    pub edge: &'a Alignment,
    pub surface: SurfaceLevel<'a>,
}

impl EdgeLevel<'_> {
    pub fn level(&self, s: f64) -> Result<f64, JunctionError> {
        self.surface.at_point(pt_at(self.edge, s)?)
    }
    /// Grade along the edge (m/m) by a central difference over 0.1 m (one-sided at the ends).
    pub fn grade(&self, s: f64) -> Result<f64, JunctionError> {
        let (lo, hi) = (self.edge.start_station_m, self.edge.end_station_m());
        let h = 0.05;
        let (a, b) = ((s - h).max(lo), (s + h).min(hi));
        if b - a < 1e-9 {
            return Ok(0.0);
        }
        Ok((self.level(b)? - self.level(a)?) / (b - a))
    }
}

/// A profile of PVIs without curves at a regular spacing from `s0` to `s1`, levels from `f`.
pub fn sample_profile(name: &str, s0: f64, s1: f64, spacing_m: f64, f: &dyn Fn(f64) -> Result<f64, JunctionError>) -> Result<Profile, JunctionError> {
    if !(spacing_m.is_finite() && spacing_m > 0.0) {
        return Err(JunctionError::input(format!("the profile sample spacing must be greater than zero (got {spacing_m})")));
    }
    let n = (((s1 - s0) / spacing_m).ceil() as usize).clamp(1, 200_000);
    let mut pvis = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let s = if i == n { s1 } else { s0 + (s1 - s0) * i as f64 / n as f64 };
        pvis.push(Pvi::point(s, f(s)?));
    }
    Profile::new(name, pvis).map_err(pe)
}

/// A place where the level has a local minimum.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LowPoint {
    pub station_m: f64,
    pub level_m: f64,
    pub x: f64,
    pub y: f64,
}

/// Interior local minima of a level function over `[s0, s1]` (station, level), found as the places
/// where its grade `g` changes from negative to not negative: `g` is sampled at 400 stations and
/// each sign change is bisected to 1e-12 m. (Finding the minimum of the level itself would be
/// limited to about the square root of the rounding error in station.)
pub fn local_minima(level: &dyn Fn(f64) -> f64, grade: &dyn Fn(f64) -> f64, s0: f64, s1: f64) -> Vec<(f64, f64)> {
    const N: usize = 400;
    let st = |i: usize| s0 + (s1 - s0) * i as f64 / N as f64;
    let g: Vec<f64> = (0..=N).map(|i| grade(st(i))).collect();
    let mut out = Vec::new();
    for i in 1..=N {
        if g[i - 1] < 0.0 && g[i] >= 0.0 {
            let (mut a, mut b) = (st(i - 1), st(i));
            for _ in 0..200 {
                let m = 0.5 * (a + b);
                if grade(m) < 0.0 {
                    a = m;
                } else {
                    b = m;
                }
                if b - a < 1e-12 {
                    break;
                }
            }
            let s = 0.5 * (a + b);
            // A flat run (grade exactly zero over a length) is one low point, not many.
            if out.last().is_none_or(|(p, _): &(f64, f64)| (s - *p).abs() > 1e-6) {
                out.push((s, level(s)));
            }
        }
    }
    out
}

/// How the profile of a return was formed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendMethod {
    /// Two PVIs: the grade of the line through both ends.
    StraightGrade,
    /// One PVI with an asymmetric parabola over the whole return.
    SingleVerticalCurve,
    /// A cubic through both ends and both grades, given as sampled PVIs.
    CubicSampled,
}

/// The vertical definition of a return.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReturnVertical {
    pub method: BlendMethod,
    pub profile: Profile,
    pub start_level_m: f64,
    pub start_grade: f64,
    pub end_level_m: f64,
    pub end_grade: f64,
    /// Station of the PVI of the single curve, when that was the method.
    pub pvi_station_m: Option<f64>,
    /// Interior low points (the drainage low points of the return).
    pub low_points: Vec<LowPoint>,
    /// The lower end of the return.
    pub lowest_end: LowPoint,
    /// Largest grade magnitude along the return (m/m).
    pub max_abs_grade: f64,
}

enum Blend {
    Line { z0: f64, g: f64 },
    Asym(Profile),
    Cubic { z0: f64, z1: f64, g0: f64, g1: f64, l: f64 },
}

impl Blend {
    fn level(&self, s: f64) -> f64 {
        match self {
            Blend::Line { z0, g } => z0 + g * s,
            Blend::Asym(p) => p.elevation_at(s).unwrap_or(f64::NAN),
            Blend::Cubic { z0, z1, g0, g1, l } => {
                let t = (s / l).clamp(0.0, 1.0);
                let (t2, t3) = (t * t, t * t * t);
                z0 * (2.0 * t3 - 3.0 * t2 + 1.0) + l * g0 * (t3 - 2.0 * t2 + t) + z1 * (-2.0 * t3 + 3.0 * t2) + l * g1 * (t3 - t2)
            }
        }
    }
    fn grade(&self, s: f64) -> f64 {
        match self {
            Blend::Line { g, .. } => *g,
            Blend::Asym(p) => p.grade_at(s).unwrap_or(f64::NAN),
            Blend::Cubic { z0, z1, g0, g1, l } => {
                let t = (s / l).clamp(0.0, 1.0);
                let t2 = t * t;
                (z0 * (6.0 * t2 - 6.0 * t) + l * g0 * (3.0 * t2 - 4.0 * t + 1.0) + z1 * (-6.0 * t2 + 6.0 * t) + l * g1 * (3.0 * t2 - 2.0 * t)) / l
            }
        }
    }
}

/// The profile of a return of length `length_m` from the level and grade at each end (grades along
/// the return). `spacing_m` is used only by the cubic fallback. `at` gives the plan point of a station
/// of the return, for the low point positions.
#[allow(clippy::too_many_arguments)]
pub fn blend_return_profile(
    name: &str,
    length_m: f64,
    z0: f64,
    g0: f64,
    z1: f64,
    g1: f64,
    spacing_m: f64,
    at: &dyn Fn(f64) -> Pt,
) -> Result<ReturnVertical, JunctionError> {
    let l = length_m;
    if !(l.is_finite() && l > 0.0) {
        return Err(JunctionError::geometry("a return of no length has no profile"));
    }
    let straight = (g0 - g1).abs() < 1e-12 && (z1 - z0 - g0 * l).abs() < 1e-9;
    let mut pvi_station = None;
    let (method, blend, profile) = if straight {
        (BlendMethod::StraightGrade, Blend::Line { z0, g: g0 }, Profile::new(name, vec![Pvi::point(0.0, z0), Pvi::point(l, z1)]).map_err(pe)?)
    } else {
        let s_star = if (g0 - g1).abs() > 1e-12 { Some((z1 - z0 - g1 * l) / (g0 - g1)) } else { None };
        match s_star {
            Some(s) if s > 1e-6 * l && s < l - 1e-6 * l => {
                pvi_station = Some(s);
                let p = Profile::new(name, vec![Pvi::point(0.0, z0), Pvi::asymmetric(s, z0 + g0 * s, s, l - s), Pvi::point(l, z1)]).map_err(pe)?;
                (BlendMethod::SingleVerticalCurve, Blend::Asym(p.clone()), p)
            }
            _ => {
                let b = Blend::Cubic { z0, z1, g0, g1, l };
                let p = sample_profile(name, 0.0, l, spacing_m, &|s| Ok(b.level(s)))?;
                (BlendMethod::CubicSampled, b, p)
            }
        }
    };
    let f = |s: f64| blend.level(s);
    let low_points = local_minima(&f, &|s| blend.grade(s), 0.0, l)
        .into_iter()
        .map(|(s, z)| {
            let p = at(s);
            LowPoint { station_m: s, level_m: z, x: p.0, y: p.1 }
        })
        .collect();
    let end = |s: f64, z: f64| {
        let p = at(s);
        LowPoint { station_m: s, level_m: z, x: p.0, y: p.1 }
    };
    let lowest_end = if z0 <= z1 { end(0.0, z0) } else { end(l, z1) };
    let max_abs_grade = (0..=400).map(|i| blend.grade(l * i as f64 / 400.0).abs()).fold(0.0, f64::max);
    Ok(ReturnVertical { method, profile, start_level_m: z0, start_grade: g0, end_level_m: z1, end_grade: g1, pvi_station_m: pvi_station, low_points, lowest_end, max_abs_grade })
}

/// What locking the secondary profile to the primary produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockReport {
    pub profile: Profile,
    /// Secondary stations where its centreline meets the primary's two edges (the lock region).
    pub region_start_station_m: f64,
    pub region_end_station_m: f64,
    /// Secondary station of the crossing.
    pub crossing_station_m: f64,
    /// Primary crown level at the crossing, m.
    pub crown_level_m: f64,
    /// The secondary's own profile level at the crossing before locking, m.
    pub original_level_at_crossing_m: f64,
    /// `crown_level_m - original_level_at_crossing_m`.
    pub level_shift_m: f64,
    /// Grade of the tie-in on the low-station side and the high-station side (m/m), when there is one.
    pub tie_in_grade_start: Option<f64>,
    pub tie_in_grade_end: Option<f64>,
}

/// Generates the secondary profile through the junction from the primary's surface. See the
/// module documentation. `primary_edges` are the primary's left and right edge alignments.
#[allow(clippy::too_many_arguments)]
pub fn lock_secondary_profile(
    primary: &Alignment,
    primary_edges: [&Alignment; 2],
    secondary: &Alignment,
    crossing: &Crossing,
    primary_crossfall: &CrossfallInput,
    tie_in_m: f64,
    sample_spacing_m: Option<f64>,
    name: &str,
) -> Result<LockReport, JunctionError> {
    if !(tie_in_m.is_finite() && tie_in_m > 0.0) {
        return Err(JunctionError::input(format!("the tie-in length must be greater than zero (got {tie_in_m})")));
    }
    let orig = secondary.profile.as_ref().ok_or_else(|| JunctionError::NoProfile(secondary.name.clone()))?;
    let (p_lo, p_hi) = orig.range().ok_or_else(|| JunctionError::NoProfile(secondary.name.clone()))?;
    let sc = crossing.station_b_m;
    let surface = SurfaceLevel { road: primary, crossfall: primary_crossfall };
    let mut stations = vec![sc];
    for e in primary_edges {
        let hit = find_crossings(secondary, e, 0.0).into_iter().min_by(|a, b| (a.station_a_m - sc).abs().total_cmp(&(b.station_a_m - sc).abs()));
        if let Some(h) = hit {
            stations.push(h.station_a_m);
        }
    }
    let lo = stations.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = stations.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if !(hi - lo > 1e-6) {
        return Err(JunctionError::geometry(format!("the secondary '{}' does not reach either edge of the primary '{}'", secondary.name, primary.name)));
    }
    if lo < p_lo - 1e-9 || hi > p_hi + 1e-9 {
        return Err(JunctionError::Profile(format!("the profile of '{}' (stations {p_lo:.3} to {p_hi:.3}) does not cover the junction (stations {lo:.3} to {hi:.3})", secondary.name)));
    }
    let mut region = vec![lo, sc, hi];
    if let Some(sp) = sample_spacing_m {
        if !(sp.is_finite() && sp > 0.0) {
            return Err(JunctionError::input(format!("the lock sample spacing must be greater than zero (got {sp})")));
        }
        let n = ((hi - lo) / sp).ceil() as usize;
        region.extend((1..n).map(|i| lo + (hi - lo) * i as f64 / n as f64));
    }
    region.sort_by(f64::total_cmp);
    region.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let mut generated: Vec<Pvi> = Vec::new();
    for s in &region {
        generated.push(Pvi::point(*s, surface.at_point(pt_at(secondary, *s)?)?));
    }
    let crown = surface.at_point(pt_at(secondary, sc)?)?;
    let (t_lo, t_hi) = ((lo - tie_in_m).max(p_lo), (hi + tie_in_m).min(p_hi));
    let mut pvis: Vec<Pvi> = orig.pvis.iter().filter(|p| p.station_m < t_lo - 1e-9).cloned().collect();
    let mut grade_start = None;
    if lo - t_lo > 1e-6 {
        let z = orig.elevation_at(t_lo).map_err(pe)?;
        grade_start = Some((generated[0].elevation_m - z) / (lo - t_lo));
        pvis.push(Pvi::point(t_lo, z));
    }
    pvis.extend(generated.iter().cloned());
    let mut grade_end = None;
    if t_hi - hi > 1e-6 {
        let z = orig.elevation_at(t_hi).map_err(pe)?;
        grade_end = Some((z - generated.last().map(|p| p.elevation_m).unwrap_or(z)) / (t_hi - hi));
        pvis.push(Pvi::point(t_hi, z));
    }
    pvis.extend(orig.pvis.iter().filter(|p| p.station_m > t_hi + 1e-9).cloned());
    let profile = Profile::new(name, pvis).map_err(pe)?;
    let original_level_at_crossing_m = orig.elevation_at(sc).map_err(pe)?;
    Ok(LockReport {
        profile,
        region_start_station_m: lo,
        region_end_station_m: hi,
        crossing_station_m: sc,
        crown_level_m: crown,
        original_level_at_crossing_m,
        level_shift_m: crown - original_level_at_crossing_m,
        tie_in_grade_start: grade_start,
        tie_in_grade_end: grade_end,
    })
}
