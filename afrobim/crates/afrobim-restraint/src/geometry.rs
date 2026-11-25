// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! A run resolved against its alignment: the offset line, its length, the posts, the plan
//! drawing and the schematic solids.
//!
//! # Length along the offset line
//!
//! A line at constant offset `o` (positive right) from a centreline of curvature `κ` (positive
//! turning left) has, per metre of centreline, `1 + κ·o` metres of its own. Over a run from
//! station `s0` to `s1`
//!
//! ```text
//!   L = ∫ (1 + κ·o) ds = (s1 − s0) + o · Δθ        Δθ = ∫ κ ds, the angle turned
//! ```
//!
//! which is exact for straights, circular arcs and clothoids alike (their curvature is linear
//! in distance, so `Δθ` is a trapezium per element; no chords are involved). On a right-hand
//! curve a barrier on the right is on the inside and comes out shorter than the centreline; on
//! a left-hand curve, longer. The offset must stay on the centreline's side of the centre of
//! curvature (`1 + κ·o > 0` throughout), or the run is refused.
//!
//! Where two alignment elements meet at an angle (not a smooth alignment) the offset line has a
//! gap or an overlap there. It is not measured: `Δθ` counts curvature only.

use afrobim_alignment::{Alignment, ElementKind};
use afrobim_draw::{Drawing, Extents, Mesh3, Primitive, Pt2};
use serde::{Deserialize, Serialize};

use crate::{BarrierRun, EndTreatment, RestraintError, TrafficFace};

/// Greatest distance allowed between a drawn chord and the true offset line, m. Affects the
/// drawing and the mesh only, never a quantity.
const CHORD_SAG_M: f64 = 0.005;

/// A bay shorter than this is not given its own post: the closing post is the end post.
pub const POST_TOLERANCE_M: f64 = 0.001;

/// More posts than this in one run is taken to be a mistyped spacing.
pub const MAX_POSTS: usize = 200_000;

/// DISPLAY ONLY: size of the square post of the schematic, mm. From no standard and no product.
pub const SCHEMATIC_POST_SIZE_MM: f64 = 100.0;
/// DISPLAY ONLY: depth (vertical) of the schematic beam, mm. From no standard and no product.
pub const SCHEMATIC_BEAM_DEPTH_MM: f64 = 300.0;
/// DISPLAY ONLY: thickness (across the barrier) of the schematic beam, mm.
pub const SCHEMATIC_BEAM_THICKNESS_MM: f64 = 50.0;
/// DISPLAY ONLY: length of a post tick on the plan, m.
pub const PLAN_POST_TICK_M: f64 = 0.3;

/// A point of the run, on the offset line.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RunPoint {
    /// Internal station of the centreline point square to this one, m.
    pub station_m: f64,
    /// Plan easting and northing, m.
    pub x: f64,
    pub y: f64,
    /// Level, m: the alignment's profile level at the station, or 0 (see
    /// [`ResolvedRun::levels_from_profile`]).
    pub z: f64,
    /// Direction of increasing station there, radians counter-clockwise from +X.
    pub heading_rad: f64,
}

/// A run worked out against its alignment. Stored with the element so that the module can draw
/// it without the alignment; [`crate::sync_barrier_runs`] refreshes it when the alignment
/// changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedRun {
    /// Station labels of the two ends through the alignment's station equations.
    pub start_label: String,
    pub end_label: String,
    /// `end station − start station`, m.
    pub centreline_length_m: f64,
    /// Angle the centreline turns over the run, radians, positive to the left.
    pub turned_rad: f64,
    /// Length along the offset line, m: `centreline length + offset × turned`. See the module
    /// documentation.
    pub length_m: f64,
    /// True when `z` values are the alignment's profile levels. False when the alignment has no
    /// profile or its profile does not cover the whole run: every `z` is then 0.
    ///
    /// A profile level is the **centreline** level at the station. No cross-fall, verge slope
    /// or level difference to the barrier's own position is applied.
    pub levels_from_profile: bool,
    /// The offset line for drawing: straights as their two ends, curves and spirals as chords
    /// within 5 mm of the true line.
    pub line: Vec<RunPoint>,
    /// One point per post, in order; empty for a run without posts.
    pub posts: Vec<RunPoint>,
}

/// ∫ κ ds from the start of the alignment to `station_m`: exact, by a trapezium per element.
fn turned_to(alignment: &Alignment, station_m: f64) -> f64 {
    let mut start = alignment.start_station_m;
    let mut sum = 0.0;
    for e in &alignment.elements {
        let local = (station_m - start).clamp(0.0, e.length);
        sum += 0.5 * (e.curvature_at(0.0) + e.curvature_at(local)) * local;
        start += e.length;
    }
    sum
}

/// Number of bays for a length and spacing: whole spacings, plus a short closing bay when what
/// is left over is more than [`POST_TOLERANCE_M`]. At least 1.
pub fn bay_count(length_m: f64, spacing_m: f64) -> usize {
    (((length_m - POST_TOLERANCE_M) / spacing_m).ceil().max(1.0)) as usize
}

/// Number of posts for a length and spacing: one at the start, one every `spacing_m` along the
/// barrier, and one at the end (so `bays + 1`; the last bay may be short).
pub fn post_count(length_m: f64, spacing_m: f64) -> usize {
    bay_count(length_m, spacing_m) + 1
}

/// Works a run out against its alignment.
pub fn resolve(run: &BarrierRun, alignment: &Alignment) -> Result<ResolvedRun, RestraintError> {
    run.check()?;
    let (s0, s1, o) = (run.start_station_m, run.end_station_m, run.offset_m);
    let (a0, a1) = (alignment.start_station_m, alignment.end_station_m());
    if alignment.locate(s0).is_none() || alignment.locate(s1).is_none() {
        return Err(RestraintError::OffAlignment { start_m: s0, end_m: s1, alignment: alignment.name.clone(), from_m: a0, to_m: a1 });
    }

    // Stations to visit: the ends, every element joint inside, and chords on curved elements.
    let rmin = alignment.min_radius().map_or(f64::INFINITY, |r| (r - o.abs()).max(1.0));
    let max_chord = if rmin.is_finite() { (8.0 * rmin * CHORD_SAG_M).sqrt().clamp(0.25, 20.0) } else { f64::INFINITY };
    let mut stations = vec![s0];
    let mut elem_start = a0;
    for e in &alignment.elements {
        let (from, to) = (elem_start.max(s0), (elem_start + e.length).min(s1));
        elem_start += e.length;
        if to <= from {
            continue;
        }
        // 1 + κ·o is linear in an element, so its least value is at one end of the part used.
        for s in [from, to] {
            let k = e.curvature_at(s - (elem_start - e.length));
            if 1.0 + k * o <= 1e-9 {
                return Err(RestraintError::OffsetBeyondCurveCentre { offset_m: o, station_m: s });
            }
        }
        let n = if e.kind() == ElementKind::Line || !max_chord.is_finite() { 1 } else { ((to - from) / max_chord).ceil().max(1.0) as usize };
        for i in 1..=n {
            stations.push(from + (to - from) * i as f64 / n as f64);
        }
    }
    stations.dedup_by(|b, a| (*b - *a).abs() < 1e-9);
    if let Some(last) = stations.last_mut() {
        *last = s1;
    }

    let t0 = turned_to(alignment, s0);
    let turned_rad = turned_to(alignment, s1) - t0;
    let centreline_length_m = s1 - s0;
    let length_m = centreline_length_m + o * turned_rad;
    // Distance along the offset line from the run's start to the point square to station s.
    let along = |s: f64| (s - s0) + o * (turned_to(alignment, s) - t0);

    let mut post_stations = Vec::new();
    if let Some(spacing) = run.post_spacing_m {
        let bays = bay_count(length_m, spacing);
        if bays + 1 > MAX_POSTS {
            return Err(RestraintError::TooManyPosts { posts: bays + 1, spacing_m: spacing });
        }
        post_stations.push(s0);
        for k in 1..bays {
            // `along` rises with station, so bisection finds the station of each post.
            let target = k as f64 * spacing;
            let (mut lo, mut hi) = (s0, s1);
            for _ in 0..80 {
                let mid = 0.5 * (lo + hi);
                if along(mid) < target {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            post_stations.push(0.5 * (lo + hi));
        }
        post_stations.push(s1);
    }

    let profile = alignment.profile.as_ref();
    let level = |s: f64| profile.and_then(|p| p.elevation_at(s).ok());
    let levels_from_profile = profile.is_some() && stations.iter().chain(post_stations.iter()).all(|&s| level(s).is_some());
    let point = |s: f64| -> Result<RunPoint, RestraintError> {
        let p = alignment.point_at(s).ok_or(RestraintError::OffAlignment { start_m: s0, end_m: s1, alignment: alignment.name.clone(), from_m: a0, to_m: a1 })?;
        let (x, y) = p.offset(o);
        let z = if levels_from_profile { level(s).unwrap_or(0.0) } else { 0.0 };
        Ok(RunPoint { station_m: s, x, y, z, heading_rad: p.heading_rad })
    };
    Ok(ResolvedRun {
        start_label: alignment.station_label(s0, 3),
        end_label: alignment.station_label(s1, 3),
        centreline_length_m,
        turned_rad,
        length_m,
        levels_from_profile,
        line: stations.iter().map(|&s| point(s)).collect::<Result<_, _>>()?,
        posts: post_stations.iter().map(|&s| point(s)).collect::<Result<_, _>>()?,
    })
}

/// The plan of a run with what is needed to put it on the map, as `afrobim-alignment` does for
/// an alignment: drawing units are millimetres, Y up (north), 1 m on the ground = 1000 units,
/// and the drawing origin is the plan point (`origin_x_m`, `origin_y_m`).
#[derive(Debug, Clone, PartialEq)]
pub struct RestraintPlan {
    pub drawing: Drawing,
    pub origin_x_m: f64,
    pub origin_y_m: f64,
}

/// Unit vector to the right of a heading.
fn right_of(heading_rad: f64) -> (f64, f64) {
    (heading_rad.sin(), -heading_rad.cos())
}

/// Draws a run in plan.
///
/// Neither CD 377 5.0.1 nor the MoWT Geometric Design Manual defines a plan symbol for a safety
/// barrier (their figures use ad-hoc keys), so the run is drawn plainly:
///
/// * the barrier line as hairlines (`Primitive::Line`, width 0, black) along the offset line;
/// * a tick [`PLAN_POST_TICK_M`] long at each post, square to the line, on the side away from
///   the traffic (centred on the line for a double sided barrier);
/// * at an end with a terminal, a bar twice as long centred on the line.
pub fn plan_drawing(run: &BarrierRun, resolved: &ResolvedRun) -> RestraintPlan {
    let mut segments: Vec<[(f64, f64); 2]> = resolved.line.windows(2).map(|w| [(w[0].x, w[0].y), (w[1].x, w[1].y)]).collect();
    let (from, to) = match run.traffic_face {
        TrafficFace::Left => (0.0, PLAN_POST_TICK_M),
        TrafficFace::Right => (-PLAN_POST_TICK_M, 0.0),
        TrafficFace::Both => (-PLAN_POST_TICK_M / 2.0, PLAN_POST_TICK_M / 2.0),
    };
    let across = |p: &RunPoint, a: f64, b: f64| {
        let (rx, ry) = right_of(p.heading_rad);
        [(p.x + a * rx, p.y + a * ry), (p.x + b * rx, p.y + b * ry)]
    };
    segments.extend(resolved.posts.iter().map(|p| across(p, from, to)));
    for (end, p) in [(&run.start_end, resolved.line.first()), (&run.end_end, resolved.line.last())] {
        if let (EndTreatment::Terminal { .. }, Some(p)) = (end, p) {
            segments.push(across(p, -PLAN_POST_TICK_M, PLAN_POST_TICK_M));
        }
    }
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &(x, y) in segments.iter().flatten() {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    if !min_x.is_finite() {
        (min_x, min_y, max_x, max_y) = (0.0, 0.0, 0.0, 0.0);
    }
    // Subtract in metres before scaling, so large map coordinates lose nothing.
    let mm = |p: (f64, f64)| Pt2::new((p.0 - min_x) * 1000.0, (p.1 - min_y) * 1000.0);
    let primitives = segments.iter().map(|s| Primitive::Line { from: mm(s[0]), to: mm(s[1]), width: 0.0, colour: None }).collect();
    RestraintPlan {
        drawing: Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: mm((max_x, max_y)) } },
        origin_x_m: min_x,
        origin_y_m: min_y,
    }
}

/// A prism swept along `points`: at each point a rectangle from `a` to `b` across (mm, positive
/// to the right of the heading) and from `bottom` to `top` above the point's level (mm).
/// Coordinates are millimetres from `origin` (m).
fn sweep(mesh: &mut Mesh3, points: &[(RunPoint, f64)], origin: [f64; 3], a: f64, b: f64, bottom: f64, top: f64) {
    if points.len() < 2 {
        return;
    }
    let base = mesh.vertices.len() as u32;
    for (p, along_mm) in points {
        let (rx, ry) = right_of(p.heading_rad);
        let (fx, fy) = (p.heading_rad.cos(), p.heading_rad.sin());
        let (cx, cy, cz) = ((p.x - origin[0]) * 1000.0 + along_mm * fx, (p.y - origin[1]) * 1000.0 + along_mm * fy, (p.z - origin[2]) * 1000.0);
        for (u, w) in [(a, bottom), (b, bottom), (b, top), (a, top)] {
            mesh.vertices.push([cx + u * rx, cy + u * ry, cz + w]);
        }
    }
    let n = points.len() as u32;
    for k in 0..n - 1 {
        let (q, r) = (base + 4 * k, base + 4 * (k + 1));
        mesh.triangles.extend([
            [q, r + 1, q + 1], [q, r, r + 1], // bottom
            [q + 3, q + 2, r + 2], [q + 3, r + 2, r + 3], // top
            [q + 1, r + 1, r + 2], [q + 1, r + 2, q + 2], // right
            [r, q, q + 3], [r, q + 3, r + 3], // left
        ]);
    }
    let last = base + 4 * (n - 1);
    mesh.triangles.extend([[base, base + 1, base + 2], [base, base + 2, base + 3]]);
    mesh.triangles.extend([[last + 1, last, last + 3], [last + 1, last + 3, last + 2]]);
}

/// The frame the solids of a run are in: plan origin as [`RestraintPlan`], level origin the
/// lowest level of the run. All metres.
pub fn solids_origin(resolved: &ResolvedRun, plan: &RestraintPlan) -> [f64; 3] {
    let z = resolved.line.iter().chain(resolved.posts.iter()).map(|p| p.z).fold(f64::INFINITY, f64::min);
    [plan.origin_x_m, plan.origin_y_m, if z.is_finite() { z } else { 0.0 }]
}

/// The run as solids: one mesh for all the posts (when it has posts) and one for the beam.
///
/// **A schematic, not a product model.** The beam is a plain rectangular strip
/// [`SCHEMATIC_BEAM_DEPTH_MM`] deep and [`SCHEMATIC_BEAM_THICKNESS_MM`] thick with its centre at
/// the entered beam height and its traffic face on the run's line; the posts are plain square
/// prisms of [`SCHEMATIC_POST_SIZE_MM`] behind it at the entered spacing, from the ground to
/// the top of the beam. A run without posts is drawn as a strip from the ground to the entered
/// height. No rail profile, post section, spacer, footing, terminal or transition is shown, and
/// no quantity is taken from these meshes.
///
/// Millimetres from `origin` (see [`solids_origin`]); ground at each point is the point's `z`.
pub fn solids(run: &BarrierRun, resolved: &ResolvedRun, origin: [f64; 3]) -> Vec<Mesh3> {
    let (t, p) = (SCHEMATIC_BEAM_THICKNESS_MM, SCHEMATIC_POST_SIZE_MM);
    // Across the barrier, positive right: the beam away from the traffic, the posts behind it.
    let (beam_a, beam_b, post_centre) = match run.traffic_face {
        TrafficFace::Left => (0.0, t, t + p / 2.0),
        TrafficFace::Right => (-t, 0.0, -(t + p / 2.0)),
        TrafficFace::Both => (-t / 2.0, t / 2.0, 0.0),
    };
    let centre = run.beam_height_m * 1000.0;
    let top = centre + SCHEMATIC_BEAM_DEPTH_MM / 2.0;
    let bottom = if run.post_spacing_m.is_some() { (centre - SCHEMATIC_BEAM_DEPTH_MM / 2.0).max(0.0) } else { 0.0 };
    let top = if run.post_spacing_m.is_some() { top } else { centre };

    let mut out = Vec::new();
    if !resolved.posts.is_empty() {
        let mut posts = Mesh3 { name: "Barrier posts (schematic)".into(), vertices: Vec::new(), triangles: Vec::new(), colour: [0x80, 0x84, 0x88] };
        for post in &resolved.posts {
            sweep(&mut posts, &[(*post, -p / 2.0), (*post, p / 2.0)], origin, post_centre - p / 2.0, post_centre + p / 2.0, 0.0, top);
        }
        out.push(posts);
    }
    let mut beam = Mesh3 { name: "Barrier beam (schematic)".into(), vertices: Vec::new(), triangles: Vec::new(), colour: [0xB8, 0xBC, 0xC0] };
    let line: Vec<(RunPoint, f64)> = resolved.line.iter().map(|p| (*p, 0.0)).collect();
    sweep(&mut beam, &line, origin, beam_a, beam_b, bottom, top);
    out.push(beam);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::sample_run;
    use afrobim_alignment::{AlignmentBuilder, Profile, Pvi, Turn};
    use std::f64::consts::{FRAC_PI_2, PI};

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    /// 1000 m due east from station 0 at the plan origin.
    fn straight() -> Alignment {
        AlignmentBuilder::new("A1", 0.0, 0.0, 0.0, 0.0).line(1000.0).build().unwrap()
    }

    /// 100 m straight east, then a quarter circle of radius 200 m to the given hand.
    fn curve(turn: Turn) -> Alignment {
        AlignmentBuilder::new("A1", 0.0, 0.0, 0.0, 0.0).line(100.0).arc(200.0, 100.0 * PI, turn).build().unwrap()
    }

    #[test]
    fn post_counts() {
        // 100 m at 2 m: 50 bays, 51 posts.
        assert_eq!(post_count(100.0, 2.0), 51);
        // 30.48 m at 1.905 m: exactly 16 bays, 17 posts, whatever the last bit of the division.
        assert_eq!(post_count(30.48, 1.905), 17);
        // 10 m at 4 m: posts at 0, 4, 8 and a closing post at 10.
        assert_eq!(post_count(10.0, 4.0), 4);
        // Less than a millimetre over a whole number of bays adds no post; more does.
        assert_eq!(post_count(8.0005, 4.0), 3);
        assert_eq!(post_count(8.002, 4.0), 4);
        // Shorter than one bay: the two end posts.
        assert_eq!(post_count(1.0, 4.0), 2);
    }

    #[test]
    fn on_a_straight_the_run_is_the_centreline_shifted_right() {
        let r = resolve(&sample_run(), &straight()).unwrap();
        assert_eq!(r.centreline_length_m, 100.0);
        assert_eq!(r.turned_rad, 0.0);
        assert_eq!(r.length_m, 100.0);
        assert!(!r.levels_from_profile);
        // Heading east: right is south, so y = −5 and x runs from 100 to 200.
        assert_eq!(r.line.len(), 2);
        assert!(close(r.line[0].x, 100.0, 1e-9) && close(r.line[0].y, -5.0, 1e-9) && r.line[0].z == 0.0);
        assert!(close(r.line[1].x, 200.0, 1e-9) && close(r.line[1].y, -5.0, 1e-9));
        // Posts every 2 m: 51 of them, at x = 100, 102, ..., 200.
        assert_eq!(r.posts.len(), 51);
        for (k, p) in r.posts.iter().enumerate() {
            assert!(close(p.x, 100.0 + 2.0 * k as f64, 1e-6), "post {k} at {}", p.x);
            assert!(close(p.y, -5.0, 1e-9));
        }
        assert_eq!(r.start_label, "0+100.000");
        assert_eq!(r.end_label, "0+200.000");
    }

    #[test]
    fn on_a_right_hand_curve_a_barrier_on_the_right_is_on_the_inside_and_shorter() {
        // Quarter circle, R = 200 m, centreline 100π m. Barrier 5 m right = inside, radius 195 m:
        // length 195 · π/2 = 97.5π m. Formula: 100π + 5 · (−π/2).
        let mut run = sample_run();
        run.start_station_m = 100.0;
        run.end_station_m = 100.0 + 100.0 * PI;
        run.post_spacing_m = Some(3.0);
        let r = resolve(&run, &curve(Turn::Right)).unwrap();
        assert!(close(r.turned_rad, -FRAC_PI_2, 1e-12));
        assert!(close(r.length_m, 97.5 * PI, 1e-9));
        // Every drawn point is 195 m from the curve centre (100, −200).
        for p in &r.line {
            assert!(close(((p.x - 100.0).powi(2) + (p.y + 200.0).powi(2)).sqrt(), 195.0, 1e-6));
        }
        // 97.5π = 306.305 m at 3 m: 103 bays (102 whole and one of 0.305 m), 104 posts.
        assert_eq!(r.posts.len(), 104);
        // Consecutive posts are 3 m apart along the 195 m arc: chord 2 · 195 · sin(3 / 390).
        let chord = 2.0 * 195.0 * (3.0_f64 / 390.0).sin();
        for w in r.posts[..103].windows(2) {
            assert!(close(((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt(), chord, 1e-6));
        }
        // The drawn chords stay within 5 mm of the arc: their total is within a hair of it.
        let drawn: f64 = r.line.windows(2).map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt()).sum();
        assert!(drawn < r.length_m && drawn > r.length_m - 0.01);
    }

    #[test]
    fn on_a_left_hand_curve_the_same_barrier_is_on_the_outside_and_longer() {
        let mut run = sample_run();
        run.start_station_m = 100.0;
        run.end_station_m = 100.0 + 100.0 * PI;
        let r = resolve(&run, &curve(Turn::Left)).unwrap();
        // Radius 205 m over a quarter circle: 102.5π m.
        assert!(close(r.length_m, 102.5 * PI, 1e-9));
        // A barrier on the left (offset −5) of the same curve is the inside one: 97.5π m.
        run.offset_m = -5.0;
        assert!(close(resolve(&run, &curve(Turn::Left)).unwrap().length_m, 97.5 * PI, 1e-9));
    }

    #[test]
    fn a_run_over_a_straight_and_part_of_a_curve() {
        // Stations 50 to 150 on straight + R200 right curve: 50 m straight, 50 m of curve.
        // Inside at 5 m: 50 + 50 · 195/200 = 98.75 m.
        let mut run = sample_run();
        run.start_station_m = 50.0;
        run.end_station_m = 150.0;
        let r = resolve(&run, &curve(Turn::Right)).unwrap();
        assert!(close(r.length_m, 98.75, 1e-9));
        // Post k = 30 is 60 m along: 50 m of straight, then 10 m on the 195 m arc.
        let p = r.posts[30];
        let a = 10.0_f64 / 195.0;
        assert!(close(p.x, 100.0 + 195.0 * a.sin(), 1e-6) && close(p.y, -200.0 + 195.0 * a.cos(), 1e-6));
    }

    #[test]
    fn a_clothoid_run_uses_the_angle_turned() {
        // Spiral from straight to R = 100 m over 60 m turns L/2R = 0.3 rad to the left.
        let a = AlignmentBuilder::new("S", 0.0, 0.0, 0.0, 0.0).spiral(60.0, None, Some(100.0), Turn::Left).build().unwrap();
        let mut run = sample_run();
        run.alignment = "S".into();
        run.start_station_m = 0.0;
        run.end_station_m = 60.0;
        let r = resolve(&run, &a).unwrap();
        assert!(close(r.turned_rad, 0.3, 1e-12));
        assert!(close(r.length_m, 60.0 + 5.0 * 0.3, 1e-12));
    }

    #[test]
    fn refusals() {
        let mut run = sample_run();
        run.end_station_m = 1200.0;
        assert!(matches!(resolve(&run, &straight()), Err(RestraintError::OffAlignment { .. })));
        // 250 m to the right of a right-hand curve of 200 m radius is beyond its centre.
        let mut run = sample_run();
        run.offset_m = 250.0;
        run.end_station_m = 300.0;
        assert!(matches!(resolve(&run, &curve(Turn::Right)), Err(RestraintError::OffsetBeyondCurveCentre { .. })));
        // ... but fine on the straight part, and fine on the outside.
        run.end_station_m = 100.0;
        run.start_station_m = 10.0;
        assert!(resolve(&run, &curve(Turn::Right)).is_ok());
        let mut run = sample_run();
        run.post_spacing_m = Some(0.0001);
        assert!(matches!(resolve(&run, &straight()), Err(RestraintError::TooManyPosts { .. })));
        let mut run = sample_run();
        run.beam_height_m = 0.0;
        assert!(matches!(resolve(&run, &straight()), Err(RestraintError::InvalidRun(_))));
    }

    #[test]
    fn levels_come_from_the_profile_when_it_covers_the_run() {
        // A 2 % rise from level 10 m at station 0.
        let mut a = straight();
        a.profile = Some(Profile::new("P", vec![Pvi::point(0.0, 10.0), Pvi::point(1000.0, 30.0)]).unwrap());
        let r = resolve(&sample_run(), &a).unwrap();
        assert!(r.levels_from_profile);
        assert!(close(r.line[0].z, 12.0, 1e-9) && close(r.line[1].z, 14.0, 1e-9));
        assert!(close(r.posts[25].z, 13.0, 1e-9));
        // A profile that stops short of the run: all levels 0, and the flag says so.
        a.profile = Some(Profile::new("P", vec![Pvi::point(0.0, 10.0), Pvi::point(150.0, 13.0)]).unwrap());
        let r = resolve(&sample_run(), &a).unwrap();
        assert!(!r.levels_from_profile);
        assert!(r.line.iter().chain(r.posts.iter()).all(|p| p.z == 0.0));
    }

    #[test]
    fn plan_has_the_line_a_tick_per_post_and_a_bar_per_terminal() {
        let mut run = sample_run();
        run.post_spacing_m = Some(25.0);
        let r = resolve(&run, &straight()).unwrap();
        let plan = plan_drawing(&run, &r);
        // 1 line segment + 5 post ticks + 2 terminal bars.
        assert_eq!(plan.drawing.primitives.len(), 8);
        // Extent: x 100..200, y from the far end of the bars and ticks (−5.3) to −4.7.
        assert!(close(plan.origin_x_m, 100.0, 1e-9) && close(plan.origin_y_m, -5.3, 1e-9));
        assert!(close(plan.drawing.extents.max.x, 100_000.0, 1e-6) && close(plan.drawing.extents.max.y, 600.0, 1e-6));
        let lines: Vec<(Pt2, Pt2)> =
            plan.drawing.primitives.iter().filter_map(|p| if let Primitive::Line { from, to, .. } = p { Some((*from, *to)) } else { None }).collect();
        // The barrier line, 0.3 m above the drawing's lower edge.
        assert!(close(lines[0].0.y, 300.0, 1e-6) && close(lines[0].1.x, 100_000.0, 1e-6));
        // Traffic on the left (north): the ticks go right (south), from the line down 0.3 m.
        assert!(close(lines[2].0.x, 25_000.0, 1e-6) && close(lines[2].0.y, 300.0, 1e-6) && close(lines[2].1.y, 0.0, 1e-6));
        // Without terminals there are no bars.
        run.start_end = EndTreatment::Unspecified;
        run.end_end = EndTreatment::Unspecified;
        assert_eq!(plan_drawing(&run, &r).drawing.primitives.len(), 6);
    }

    #[test]
    fn solids_are_closed_outward_and_at_the_entered_height() {
        let mut run = sample_run();
        run.post_spacing_m = Some(25.0);
        let r = resolve(&run, &straight()).unwrap();
        let plan = plan_drawing(&run, &r);
        let s = solids(&run, &r, solids_origin(&r, &plan));
        assert_eq!(s.len(), 2);
        // Beam: 100 m × 50 mm × 300 mm; posts: 5 × 100 × 100 × (600 + 150) mm.
        assert!(close(s[1].volume(), 100_000.0 * 50.0 * 300.0, 1.0));
        assert!(close(s[0].volume(), 5.0 * 100.0 * 100.0 * 750.0, 1.0));
        let zs = |m: &Mesh3| (m.vertices.iter().map(|v| v[2]).fold(f64::INFINITY, f64::min), m.vertices.iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max));
        assert_eq!(zs(&s[1]), (450.0, 750.0));
        assert_eq!(zs(&s[0]), (0.0, 750.0));
        // Traffic on the left: the beam's traffic face is on the line (y = 300 mm in the frame)
        // and the beam and posts are to the right of it (smaller y).
        let ys = |m: &Mesh3| (m.vertices.iter().map(|v| v[1]).fold(f64::INFINITY, f64::min), m.vertices.iter().map(|v| v[1]).fold(f64::NEG_INFINITY, f64::max));
        let (lo, hi) = ys(&s[1]);
        assert!(close(lo, 250.0, 1e-6) && close(hi, 300.0, 1e-6));
        let (lo, hi) = ys(&s[0]);
        assert!(close(lo, 150.0, 1e-6) && close(hi, 250.0, 1e-6));

        // No posts: one strip from the ground to the entered height.
        run.post_spacing_m = None;
        let r = resolve(&run, &straight()).unwrap();
        let s = solids(&run, &r, solids_origin(&r, &plan));
        assert_eq!(s.len(), 1);
        assert_eq!(zs(&s[0]), (0.0, 600.0));
    }

    #[test]
    fn solids_follow_the_curve_and_the_profile() {
        let mut a = curve(Turn::Right);
        a.profile = Some(Profile::new("P", vec![Pvi::point(0.0, 100.0), Pvi::point(500.0, 110.0)]).unwrap());
        let mut run = sample_run();
        run.start_station_m = 100.0;
        run.end_station_m = 100.0 + 100.0 * PI;
        run.traffic_face = TrafficFace::Both;
        let r = resolve(&run, &a).unwrap();
        let plan = plan_drawing(&run, &r);
        let origin = solids_origin(&r, &plan);
        // Lowest level is at the start: 100 + 0.02 × 100 = 102 m.
        assert!(close(origin[2], 102.0, 1e-9));
        let s = solids(&run, &r, origin);
        // Beam centred on the 195 m arc: volume ≈ section × arc length (chords are a hair short).
        let exact = 50.0 * 300.0 * 97.5 * PI * 1000.0;
        assert!(s[1].volume() > exact * 0.9999 && s[1].volume() <= exact * 1.0000001);
        // The far end is 100π m further on at 2 %: 2π m = 6283 mm higher.
        let top = s[1].vertices.iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
        assert!(close(top, 2000.0 * PI + 750.0, 1e-3));
    }
}
