// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Grading objects: projection from a footprint feature line to a daylight line.
//!
//! A [`Grading`] projects from a footprint (the whole line, or a station range) on one side by
//! one criterion, or by several criteria in station regions with transitions between them.
//!
//! # Method
//!
//! 1. The footprint is cut into nodes and rays (see the `rays` module): normal rays on straight
//!    runs and arcs, a miter ray at an interior corner, a radial fan at an exterior corner.
//! 2. For each ray the criterion gives a perpendicular distance `t` and an end elevation `z`.
//!    The daylight point is `foot + dir miter t` at `z`.
//! 3. **Regions.** Each region covers a station interval with one criterion (`Uniform`) or a
//!    blend of two (`Transition`). In a transition the daylight of a ray is
//!    `(1 - w) day(start) + w day(end)` (distance and elevation both), with `w = (s - from) /
//!    (to - from)` the station fraction. Where two regions meet at a station, the rays of the
//!    node are emitted once per region, so a jump between criteria becomes a triangle fan face.
//! 4. **Clean-up.** First, a normal ray beside an interior corner whose projection line crosses the
//!    corner's miter ray is dropped (it would end on the other wall's side of the bisector), so on
//!    flat ground each wall's daylight runs straight into the miter point. Second, the daylight polyline of each run of rays is tested for self-crossings
//!    (O(n^2) segment pairs). A loop (a pair of non-adjacent segments that cross) is removed by
//!    moving all the daylight points between them onto the crossing point, which is the usual trim
//!    of the projection lines at an interior corner. The rays concerned are marked `Trimmed`.
//! 5. **Faces.** Between consecutive rays the quad `foot_i, foot_i+1, day_i+1, day_i` is split on
//!    its shorter diagonal (a triangle where the feet or the daylight points coincide). Triangles
//!    are returned counter-clockwise in plan. A ray with no daylight breaks the faces beside it
//!    and a warning is recorded.

use afrobim_corridor::Tin;
use afrobim_terrain::{BoundaryKind, Breakline, BreaklineKind, Operation, SurfaceDefinition, SurfacePoint};
use serde::{Deserialize, Serialize};

use crate::criteria::Criterion;
use crate::error::{GradingError, Result};
use crate::featureline::FeatureLine;
use crate::geom::*;
use crate::rays::{ray_geometry, ProjectionParams, RayKind, Side};

/// The criterion of a region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "region", rename_all = "snake_case")]
pub enum RegionKind {
    Uniform { criterion: Criterion },
    /// Blend from `start` at the region's `from` station to `end` at its `to` station (both
    /// bounds are required for a transition).
    Transition { start: Criterion, end: Criterion },
}

/// A station interval of the footprint with its criterion. `None` bounds are open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub from: Option<f64>,
    pub to: Option<f64>,
    #[serde(flatten)]
    pub kind: RegionKind,
}

/// A grading object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grading {
    pub name: String,
    /// Name of the footprint feature line in the site.
    pub footprint: String,
    /// Station range of the footprint; `None` for the whole line (a closed footprint is then a ring).
    pub range: Option<(f64, f64)>,
    pub side: Side,
    pub regions: Vec<Region>,
}

/// What happened to a ray.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RayStatus {
    Ok,
    /// No daylight was found (the ground ends, or no crossing within the maximum distance).
    NoDaylight,
    /// The daylight point was moved onto a crossing by the loop clean-up.
    Trimmed,
}

/// The outcome of one ray.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RayResult {
    pub node: usize,
    pub region: usize,
    pub station: f64,
    pub kind: RayKind,
    pub foot: [f64; 3],
    /// Perpendicular distance from the footprint to the daylight point, m.
    pub distance: Option<f64>,
    pub daylight: Option<[f64; 3]>,
    pub status: RayStatus,
}

/// A daylight polyline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaylightLine {
    pub points: Vec<[f64; 3]>,
    /// True when the line closes on itself (the first point is not repeated).
    pub closed: bool,
}

/// The result of projecting a grading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradingResult {
    pub name: String,
    pub footprint: String,
    /// True when the footprint run is a whole closed ring.
    pub ring: bool,
    pub rays: Vec<RayResult>,
    pub projection_lines: Vec<[[f64; 3]; 2]>,
    pub daylight_lines: Vec<DaylightLine>,
    /// Face triangles, counter-clockwise in plan, source coordinates.
    pub triangles: Vec<[[f64; 3]; 3]>,
    /// Loops removed by the clean-up.
    pub loops_removed: usize,
    /// Normal rays dropped because they overshoot the miter ray of an interior corner.
    pub rays_dropped: usize,
    /// True when every ray found daylight.
    pub complete: bool,
    pub warnings: Vec<String>,
}

impl Grading {
    /// A grading with one criterion along the whole footprint.
    pub fn uniform(name: impl Into<String>, footprint: impl Into<String>, side: Side, criterion: Criterion) -> Grading {
        Grading { name: name.into(), footprint: footprint.into(), range: None, side, regions: vec![Region { from: None, to: None, kind: RegionKind::Uniform { criterion } }] }
    }

    /// Limits the grading to a station range of the footprint.
    pub fn with_range(mut self, from: f64, to: f64) -> Grading {
        self.range = Some((from, to));
        self
    }

    /// Replaces the regions (for criteria that change along the footprint).
    pub fn with_regions(mut self, regions: Vec<Region>) -> Grading {
        self.regions = regions;
        self
    }

    /// Stations where regions start or end.
    pub fn boundaries(&self) -> Vec<f64> {
        self.regions.iter().flat_map(|r| r.from.into_iter().chain(r.to)).collect()
    }

    fn validate(&self) -> Result<()> {
        if self.regions.is_empty() {
            return Err(GradingError::Invalid("a grading needs at least one region".into()));
        }
        for r in &self.regions {
            match &r.kind {
                RegionKind::Uniform { criterion } => criterion.validate()?,
                RegionKind::Transition { start, end } => {
                    start.validate()?;
                    end.validate()?;
                    match (r.from, r.to) {
                        (Some(a), Some(b)) if b > a => {}
                        _ => return Err(GradingError::Invalid("a transition needs from < to".into())),
                    }
                }
            }
        }
        Ok(())
    }

    /// Projects the grading from `footprint` (the feature line this grading names) against
    /// `ground` (needed by `TargetSurface` criteria).
    pub fn project(&self, footprint: &FeatureLine, ground: Option<&Tin>, params: &ProjectionParams) -> Result<GradingResult> {
        self.validate()?;
        let extra = self.boundaries();
        let set = ray_geometry(footprint, self.range, self.side, params, &extra)?;
        let o = [set.rays[0].foot[0], set.rays[0].foot[1]];
        let eps = 1e-9;
        let covers = |r: &Region, s: f64| r.from.map_or(true, |f| s >= f - eps) && r.to.map_or(true, |t| s <= t + eps);

        let mut rays: Vec<RayResult> = Vec::new();
        let mut dl_local: Vec<Option<[f64; 3]>> = Vec::new();
        let mut warnings = Vec::new();
        let mut i = 0;
        while i < set.rays.len() {
            let mut j = i;
            while j < set.rays.len() && set.rays[j].node == set.rays[i].node {
                j += 1;
            }
            let s = set.rays[i].station;
            let hits: Vec<usize> = (0..self.regions.len()).filter(|&k| covers(&self.regions[k], s)).collect();
            if hits.is_empty() {
                return Err(GradingError::Invalid(format!("no region covers station {s:.3}")));
            }
            for &k in &hits {
                for g in &set.rays[i..j] {
                    let hit = match &self.regions[k].kind {
                        RegionKind::Uniform { criterion } => criterion.daylight([g.foot[0], g.foot[1]], g.foot[2], g.dir, g.miter, ground)?,
                        RegionKind::Transition { start, end } => {
                            let (a, b) = (self.regions[k].from.unwrap(), self.regions[k].to.unwrap());
                            let w = ((s - a) / (b - a)).clamp(0.0, 1.0);
                            let (h1, h2) = (start.daylight([g.foot[0], g.foot[1]], g.foot[2], g.dir, g.miter, ground)?, end.daylight([g.foot[0], g.foot[1]], g.foot[2], g.dir, g.miter, ground)?);
                            match (h1, h2) {
                                (Some(h1), Some(h2)) => Some(crate::criteria::DaylightHit { t: (1.0 - w) * h1.t + w * h2.t, z: (1.0 - w) * h1.z + w * h2.z }),
                                _ => None,
                            }
                        }
                    };
                    let foot_local = [g.foot[0] - o[0], g.foot[1] - o[1]];
                    let day = hit.map(|h| {
                        let p = add(foot_local, scale(g.dir, g.miter * h.t));
                        [p[0], p[1], h.z]
                    });
                    if hit.is_none() {
                        warnings.push(format!("no daylight at station {s:.3} (region {k})"));
                    }
                    dl_local.push(day);
                    rays.push(RayResult { node: g.node, region: k, station: s, kind: g.kind, foot: g.foot, distance: hit.map(|h| h.t), daylight: None, status: if hit.is_some() { RayStatus::Ok } else { RayStatus::NoDaylight } });
                }
            }
            i = j;
        }

        // Clean-up 1, interior corners: a normal ray beside an interior corner can overshoot the
        // corner's miter ray (it ends on the other wall's side of the bisector). Such a ray is
        // dropped, so on each wall the daylight runs straight into the miter point.
        let last_node = rays.last().map(|r| r.node).unwrap_or(0);
        let mut keep = vec![true; rays.len()];
        for m in 0..rays.len() {
            let (rm, Some(dm)) = (&rays[m], dl_local[m]) else { continue };
            if rm.kind != RayKind::InteriorCorner {
                continue;
            }
            let (a0, a1) = ([rm.foot[0] - o[0], rm.foot[1] - o[1]], [dm[0], dm[1]]);
            for k in 0..rays.len() {
                let (rk, Some(dk)) = (&rays[k], dl_local[k]) else { continue };
                if !keep[k] || rk.kind != RayKind::Normal || rk.node == rm.node || (!set.ring && (rk.node == 0 || rk.node == last_node)) {
                    continue;
                }
                let (b0, b1) = ([rk.foot[0] - o[0], rk.foot[1] - o[1]], [dk[0], dk[1]]);
                if let Some((t, u)) = seg_intersect(a0, a1, b0, b1, 0.0) {
                    if t > 1e-9 && t < 1.0 - 1e-9 && u > 1e-9 && u < 1.0 - 1e-9 {
                        keep[k] = false;
                    }
                }
            }
        }
        let rays_dropped = keep.iter().filter(|k| !**k).count();
        if rays_dropped > 0 {
            let mut it = keep.iter();
            rays.retain(|_| *it.next().unwrap());
            let mut it = keep.iter();
            dl_local.retain(|_| *it.next().unwrap());
        }

        // Clean-up 2: loops in each run of rays that found daylight.
        let n = rays.len();
        let mut loops_removed = 0;
        let runs = runs_of(&dl_local, set.ring);
        for run in &runs {
            let closed = set.ring && run.len() == n;
            let mut pts: Vec<[f64; 3]> = run.iter().map(|&k| dl_local[k].unwrap()).collect();
            let before = pts.clone();
            loops_removed += remove_loops(&mut pts, closed);
            for (m, &k) in run.iter().enumerate() {
                if (pts[m][0] - before[m][0]).abs() > 1e-9 || (pts[m][1] - before[m][1]).abs() > 1e-9 || (pts[m][2] - before[m][2]).abs() > 1e-9 {
                    rays[k].status = RayStatus::Trimmed;
                    dl_local[k] = Some(pts[m]);
                }
            }
        }
        for (k, r) in rays.iter_mut().enumerate() {
            r.daylight = dl_local[k].map(|p| [o[0] + p[0], o[1] + p[1], p[2]]);
        }
        let complete = rays.iter().all(|r| r.daylight.is_some());

        // Faces.
        let mut triangles = Vec::new();
        let pairs = if set.ring { n } else { n.saturating_sub(1) };
        for k in 0..pairs {
            let (a, b) = (&rays[k], &rays[(k + 1) % n]);
            if let (Some(da), Some(db)) = (a.daylight, b.daylight) {
                face(a.foot, b.foot, da, db, &mut triangles);
            }
        }
        if !complete {
            warnings.push("some rays found no daylight; faces beside them are missing".into());
        }

        // Lines.
        let mut projection_lines = Vec::new();
        for r in &rays {
            if let Some(d) = r.daylight {
                if len([d[0] - r.foot[0], d[1] - r.foot[1]]) > 1e-9 || (d[2] - r.foot[2]).abs() > 1e-9 {
                    projection_lines.push([r.foot, d]);
                }
            }
        }
        let mut daylight_lines = Vec::new();
        for run in runs_of(&dl_local, set.ring) {
            let closed = set.ring && run.len() == n;
            let mut pts: Vec<[f64; 3]> = Vec::new();
            for &k in &run {
                let d = rays[k].daylight.unwrap();
                if pts.last().map_or(true, |l| (l[0] - d[0]).abs() > 1e-9 || (l[1] - d[1]).abs() > 1e-9 || (l[2] - d[2]).abs() > 1e-9) {
                    pts.push(d);
                }
            }
            if closed && pts.len() > 1 {
                let (f, l) = (pts[0], pts[pts.len() - 1]);
                if (f[0] - l[0]).abs() <= 1e-9 && (f[1] - l[1]).abs() <= 1e-9 && (f[2] - l[2]).abs() <= 1e-9 {
                    pts.pop();
                }
            }
            if pts.len() >= 2 {
                daylight_lines.push(DaylightLine { points: pts, closed });
            }
        }
        Ok(GradingResult { name: self.name.clone(), footprint: self.footprint.clone(), ring: set.ring, rays, projection_lines, daylight_lines, triangles, loops_removed, rays_dropped, complete, warnings })
    }
}

/// Maximal runs of consecutive indices whose daylight exists. For a ring that is entirely present
/// the single run covers all indices; a ring with a gap is cut at the gap.
fn runs_of(dl: &[Option<[f64; 3]>], ring: bool) -> Vec<Vec<usize>> {
    let n = dl.len();
    let mut runs: Vec<Vec<usize>> = vec![];
    let mut cur: Vec<usize> = vec![];
    for k in 0..n {
        if dl[k].is_some() {
            cur.push(k);
        } else if !cur.is_empty() {
            runs.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        if ring && !runs.is_empty() && dl[0].is_some() {
            let first = runs.remove(0);
            cur.extend(first);
        }
        runs.push(cur);
    }
    runs
}

/// Removes loops of a polyline: for non-adjacent segments `i` and `j` that cross at a point
/// interior to both, all points `i+1..=j` move onto the crossing (elevation interpolated along
/// segment `i`). Returns the number of loops removed.
fn remove_loops(pts: &mut [[f64; 3]], closed: bool) -> usize {
    let n = pts.len();
    if n < 4 {
        return 0;
    }
    let nseg = if closed { n } else { n - 1 };
    let mut loops = 0;
    for _ in 0..=n {
        let mut found = None;
        'scan: for i in 0..nseg {
            for j in i + 2..nseg {
                if closed && i == 0 && j == nseg - 1 {
                    continue;
                }
                let (a0, a1, b0, b1) = (pts[i], pts[(i + 1) % n], pts[j], pts[(j + 1) % n]);
                if let Some((t, u)) = seg_intersect([a0[0], a0[1]], [a1[0], a1[1]], [b0[0], b0[1]], [b1[0], b1[1]], 0.0) {
                    if t > 1e-9 && t < 1.0 - 1e-9 && u > 1e-9 && u < 1.0 - 1e-9 {
                        found = Some((i, j, [a0[0] + t * (a1[0] - a0[0]), a0[1] + t * (a1[1] - a0[1]), a0[2] + t * (a1[2] - a0[2])]));
                        break 'scan;
                    }
                }
            }
        }
        match found {
            Some((i, j, x)) => {
                for p in pts.iter_mut().take(j + 1).skip(i + 1) {
                    *p = x;
                }
                loops += 1;
            }
            None => break,
        }
    }
    loops
}

fn near3(a: [f64; 3], b: [f64; 3]) -> bool {
    (a[0] - b[0]).abs() <= 1e-9 && (a[1] - b[1]).abs() <= 1e-9 && (a[2] - b[2]).abs() <= 1e-9
}

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Pushes the triangles of the quad between two consecutive rays (counter-clockwise in plan).
pub(crate) fn face(fa: [f64; 3], fb: [f64; 3], da: [f64; 3], db: [f64; 3], out: &mut Vec<[[f64; 3]; 3]>) {
    let mut tris: Vec<[[f64; 3]; 3]> = vec![];
    if near3(fa, fb) {
        if !near3(da, db) {
            tris.push([fa, da, db]);
        }
    } else if near3(da, db) {
        tris.push([fa, fb, da]);
    } else if dist3(fa, db) <= dist3(fb, da) {
        tris.push([fa, fb, db]);
        tris.push([fa, db, da]);
    } else {
        tris.push([fa, fb, da]);
        tris.push([fb, db, da]);
    }
    for t in tris {
        push_ccw(t, out);
    }
}

/// Adds a triangle oriented counter-clockwise in plan, dropping one with no plan area.
pub(crate) fn push_ccw(t: [[f64; 3]; 3], out: &mut Vec<[[f64; 3]; 3]>) {
    let a2 = (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]);
    if a2.abs() < 1e-9 {
        return;
    }
    if a2 < 0.0 {
        out.push([t[0], t[2], t[1]]);
    } else {
        out.push(t);
    }
}

/// Triangulates the inside of a closed ring `[x, y, z]` (not repeating the first point) with the
/// terrain engine: constrained Delaunay on the ring's points with the ring as breakline and
/// outer boundary. The triangles use the ring's own coordinates; no point is added.
pub fn infill_triangles(ring: &[[f64; 3]]) -> Result<Vec<[[f64; 3]; 3]>> {
    if ring.len() < 3 {
        return Err(GradingError::Invalid("an infill ring needs at least three points".into()));
    }
    let mut closed: Vec<[f64; 3]> = ring.to_vec();
    closed.push(ring[0]);
    let def = SurfaceDefinition::new()
        .with(Operation::AddPoints { group: "infill".into(), points: ring.iter().enumerate().map(|(i, p)| SurfacePoint::new(format!("{i}"), p[0], p[1], p[2])).collect() })
        .with(Operation::AddBreaklines { lines: vec![Breakline { name: "ring".into(), kind: BreaklineKind::Standard, vertices: closed }] })
        .with(Operation::AddBoundary { name: "ring".into(), kind: BoundaryKind::Outer, vertices: ring.iter().map(|p| [p[0], p[1]]).collect() });
    let built = def.build()?;
    let (pts, tris) = (built.tin.points(), built.tin.triangles());
    let mut out = Vec::with_capacity(tris.len());
    for t in tris {
        push_ccw([pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]], &mut out);
    }
    Ok(out)
}
