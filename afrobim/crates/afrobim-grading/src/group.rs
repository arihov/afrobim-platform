// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Grading groups: several gradings and infills forming one surface, with rebuild data,
//! volumes, raise and lower, and auto-balance.
//!
//! # The group surface
//!
//! The faces of every grading and the triangles of every infill are welded into one [`Tin`]
//! (vertices equal to a micrometre in plan and elevation are one vertex). Overlaps between
//! gradings (two projections that cover the same ground) are not detected, as with the `Tin` type
//! itself; the caller sees them as a volume that does not close. [`GroupBuild::to_definition`]
//! also gives the surface as an `afrobim_terrain` definition: every vertex as a point, every edge
//! of the mesh as a standard breakline (so a rebuild by the terrain engine reproduces the same
//! triangles), the outer ring of the mesh as an outer boundary and inner rings as hide boundaries.
//!
//! # Rebuild data
//!
//! A build records a fingerprint of every footprint it used, of the ground surface, and of the
//! group's own settings ([`RebuildData`]). [`GradingGroup::stale_reasons`] compares a stored
//! record with the current site and ground and says what changed, so the caller refreshes the
//! group only when needed. Fingerprints are FNV-1a over the exact bits of the numbers.
//!
//! # Raise and lower, auto-balance
//!
//! `shift` raises (positive) or lowers (negative) the footprints of the whole group by that many
//! metres before projecting, so the daylight lines move as well (this is not a vertical shift of
//! the finished surface). The net volume (cut minus fill against a base surface, with the
//! shrink and swell factors the caller gives) is a continuous, decreasing function of the shift.
//! Auto-balance finds the shift that brings it to a target by a bracketed root search, the
//! **Illinois** variant of regula falsi (Dowell and Jarratt, BIT 11, 1971): a secant step inside
//! a bracket that always keeps a sign change, with the stale end's function value halved when
//! the same end is kept twice, which restores superlinear convergence. Each evaluation rebuilds
//! the group. The search stops when `|net - target|` is within the tolerance, the bracket is
//! narrower than a micrometre, or the iteration limit is reached; every evaluation is reported.

use std::collections::{HashMap, HashSet};

use afrobim_corridor::Tin;
use afrobim_terrain::{stage_storage, volume_between, BoundaryKind, Breakline, BreaklineKind, Operation, StagePoint, SurfaceDefinition, SurfacePoint, VolumeResult};
use serde::{Deserialize, Serialize};

use crate::error::{finite, GradingError, Result};
use crate::geom::*;
use crate::grading::{infill_triangles, Grading, GradingResult};
use crate::rays::{nodes, ProjectionParams};
use crate::site::Site;

/// What fills the inside of a ring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Infill {
    /// The inside of a closed footprint, from the footprint's own elevations.
    Footprint { footprint: String },
    /// The inside of the closed daylight line of grading number `grading` (a pond floor).
    Daylight { grading: usize },
}

/// A group of gradings and infills.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradingGroup {
    pub name: String,
    pub gradings: Vec<Grading>,
    pub infills: Vec<Infill>,
    /// Raise (positive) or lower (negative) of all footprints, m. Start at 0.
    pub shift: f64,
    pub params: ProjectionParams,
}

/// Fingerprints that tell whether a stored build is out of date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RebuildData {
    pub footprints: Vec<(String, u64)>,
    pub ground: Option<u64>,
    pub settings: u64,
}

/// Why a build is out of date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaleReason {
    FootprintChanged(String),
    FootprintMissing(String),
    GroundChanged,
    SettingsChanged,
}

/// The built group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupBuild {
    pub name: String,
    pub tin: Tin,
    pub gradings: Vec<GradingResult>,
    pub infill_triangles: Vec<[[f64; 3]; 3]>,
    pub warnings: Vec<String>,
    pub rebuild: RebuildData,
}

/// The terrain definition of a built group, with notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupDefinition {
    pub definition: SurfaceDefinition,
    pub notes: Vec<String>,
}

/// Shrink and swell factors. Both are required: there is no default.
///
/// The adjusted volumes are `cut * cut_factor` (the usable fill a cut volume gives, so for
/// excavated rock or soil that swells and is then compacted the factor reflects both) and
/// `fill * fill_factor` (the bank volume that a fill volume demands). Pass 1.0 and 1.0 to use the
/// geometric volumes unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShrinkSwell {
    pub cut_factor: f64,
    pub fill_factor: f64,
}

/// Cut, fill and net after the factors.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AdjustedVolume {
    pub cut: f64,
    pub fill: f64,
    /// `cut - fill` after the factors, m3.
    pub net: f64,
}

impl ShrinkSwell {
    pub fn apply(&self, v: &VolumeResult) -> Result<AdjustedVolume> {
        finite("cut_factor", self.cut_factor)?;
        finite("fill_factor", self.fill_factor)?;
        if self.cut_factor <= 0.0 || self.fill_factor <= 0.0 {
            return Err(GradingError::Invalid("shrink and swell factors must be positive".into()));
        }
        let (cut, fill) = (v.cut * self.cut_factor, v.fill * self.fill_factor);
        Ok(AdjustedVolume { cut, fill, net: cut - fill })
    }
}

/// Inputs of the auto-balance. Every field is required.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BalanceOptions {
    /// The net volume wanted (cut minus fill after the factors), m3. Zero for a balanced site.
    pub target_net: f64,
    pub factors: ShrinkSwell,
    /// The search interval for the shift, m. The target must lie between the nets at its ends.
    pub min_shift: f64,
    pub max_shift: f64,
    /// Stop when `|net - target|` is not more than this, m3.
    pub tolerance: f64,
    pub max_iterations: usize,
}

/// One evaluation of the search.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BalanceEvaluation {
    pub shift: f64,
    pub cut: f64,
    pub fill: f64,
    pub net: f64,
}

/// The outcome of an auto-balance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceResult {
    pub shift: f64,
    pub net: f64,
    pub converged: bool,
    /// Number of secant steps (the two end evaluations are not counted).
    pub iterations: usize,
    /// Every evaluation in order, the two interval ends first.
    pub history: Vec<BalanceEvaluation>,
}

fn hash_tin(t: &Tin) -> u64 {
    let mut h = Fnv::new();
    for p in t.points() {
        p.iter().for_each(|v| h.f64(*v));
    }
    for tr in t.triangles() {
        tr.iter().for_each(|v| h.bytes(&v.to_le_bytes()));
    }
    h.0
}

impl GradingGroup {
    pub fn new(name: impl Into<String>, params: ProjectionParams) -> GradingGroup {
        GradingGroup { name: name.into(), gradings: vec![], infills: vec![], shift: 0.0, params }
    }

    /// Raises (positive) or lowers (negative) the whole group by a step, m. The change adds to
    /// the current shift; rebuild to see the new surface.
    pub fn raise_lower(&mut self, step: f64) -> Result<()> {
        finite("step", step)?;
        self.shift += step;
        Ok(())
    }

    fn footprint_names(&self) -> Vec<String> {
        let mut names: Vec<String> = vec![];
        for n in self.gradings.iter().map(|g| g.footprint.clone()).chain(self.infills.iter().filter_map(|i| if let Infill::Footprint { footprint } = i { Some(footprint.clone()) } else { None })) {
            if !names.contains(&n) {
                names.push(n);
            }
        }
        names
    }

    /// The fingerprints of the current inputs.
    pub fn rebuild_data(&self, site: &Site, ground: Option<&Tin>) -> Result<RebuildData> {
        let mut footprints = vec![];
        for n in self.footprint_names() {
            let fl = site.get(&n).ok_or_else(|| GradingError::NotFound(format!("footprint {n}")))?;
            footprints.push((n, fl.fingerprint()));
        }
        let mut h = Fnv::new();
        h.bytes(serde_json::to_string(self).map_err(|e| GradingError::Invalid(e.to_string()))?.as_bytes());
        Ok(RebuildData { footprints, ground: ground.map(hash_tin), settings: h.0 })
    }

    /// What changed since `data` was recorded. Empty means the stored build is current.
    pub fn stale_reasons(&self, data: &RebuildData, site: &Site, ground: Option<&Tin>) -> Vec<StaleReason> {
        let mut out = vec![];
        for (n, fp) in &data.footprints {
            match site.get(n) {
                None => out.push(StaleReason::FootprintMissing(n.clone())),
                Some(f) if f.fingerprint() != *fp => out.push(StaleReason::FootprintChanged(n.clone())),
                _ => {}
            }
        }
        for n in self.footprint_names() {
            if !data.footprints.iter().any(|(m, _)| *m == n) {
                out.push(match site.get(&n) {
                    None => StaleReason::FootprintMissing(n),
                    Some(_) => StaleReason::FootprintChanged(n),
                });
            }
        }
        if ground.map(hash_tin) != data.ground {
            out.push(StaleReason::GroundChanged);
        }
        if let Ok(now) = self.rebuild_data(site, ground) {
            if now.settings != data.settings {
                out.push(StaleReason::SettingsChanged);
            }
        }
        out
    }

    /// Builds the group surface.
    pub fn build(&self, site: &Site, ground: Option<&Tin>) -> Result<GroupBuild> {
        self.params.validate()?;
        finite("shift", self.shift)?;
        let shifted = |name: &str| -> Result<crate::featureline::FeatureLine> {
            let mut fl = site.get(name).ok_or_else(|| GradingError::NotFound(format!("footprint {name}")))?.clone();
            fl.raise_lower(self.shift)?;
            Ok(fl)
        };
        let mut results = Vec::new();
        let mut warnings = Vec::new();
        for g in &self.gradings {
            let fl = shifted(&g.footprint)?;
            let r = g.project(&fl, ground, &self.params)?;
            warnings.extend(r.warnings.iter().map(|w| format!("{}: {w}", g.name)));
            results.push(r);
        }
        let mut infill = Vec::new();
        for inf in &self.infills {
            match inf {
                Infill::Footprint { footprint } => {
                    let fl = shifted(footprint)?;
                    if !fl.closed {
                        return Err(GradingError::Invalid(format!("footprint {footprint} is not closed; it cannot be infilled")));
                    }
                    let extra: Vec<f64> = self.gradings.iter().filter(|g| g.footprint == *footprint).flat_map(|g| g.boundaries()).collect();
                    let (ns, _) = nodes(&fl, None, &self.params, &extra)?;
                    let ring: Vec<[f64; 3]> = ns.iter().map(|n| n.foot).collect();
                    infill.extend(infill_triangles(&ring)?);
                }
                Infill::Daylight { grading } => {
                    let r = results.get(*grading).ok_or_else(|| GradingError::NotFound(format!("grading {grading}")))?;
                    let line = r.daylight_lines.iter().find(|l| l.closed).ok_or_else(|| GradingError::Geometry(format!("grading {} has no closed daylight line to infill", r.name)))?;
                    infill.extend(infill_triangles(&line.points)?);
                }
            }
        }
        let mut all: Vec<[[f64; 3]; 3]> = results.iter().flat_map(|r| r.triangles.iter().cloned()).collect();
        all.extend(infill.iter().cloned());
        let tin = weld(&all)?;
        Ok(GroupBuild { name: self.name.clone(), tin, gradings: results, infill_triangles: infill, warnings, rebuild: self.rebuild_data(site, ground)? })
    }

    /// Finds the shift that brings the net volume against `base` to the target; see the module
    /// documentation. On success the group's `shift` is set to the result. The search interval
    /// must bracket the target or the call fails with `NotBracketed`.
    pub fn auto_balance(&mut self, site: &Site, ground: Option<&Tin>, base: &Tin, region: Option<&[[f64; 2]]>, opts: &BalanceOptions) -> Result<BalanceResult> {
        for (n, v) in [("target_net", opts.target_net), ("min_shift", opts.min_shift), ("max_shift", opts.max_shift), ("tolerance", opts.tolerance)] {
            finite(n, v)?;
        }
        if opts.min_shift >= opts.max_shift || opts.tolerance < 0.0 || opts.max_iterations == 0 {
            return Err(GradingError::Invalid("need min_shift < max_shift, tolerance >= 0 and max_iterations > 0".into()));
        }
        let mut history: Vec<BalanceEvaluation> = vec![];
        let eval = |shift: f64, history: &mut Vec<BalanceEvaluation>| -> Result<f64> {
            let mut g = self.clone();
            g.shift = shift;
            let b = g.build(site, ground)?;
            let v = volume_between(base, &b.tin, region)?;
            let a = opts.factors.apply(&v)?;
            history.push(BalanceEvaluation { shift, cut: a.cut, fill: a.fill, net: a.net });
            Ok(a.net - opts.target_net)
        };
        let (mut a, mut b) = (opts.min_shift, opts.max_shift);
        let mut fa = eval(a, &mut history)?;
        let mut fb = eval(b, &mut history)?;
        let done = |f: f64| f.abs() <= opts.tolerance;
        let (best, iterations, converged) = if done(fa) {
            (a, 0, true)
        } else if done(fb) {
            (b, 0, true)
        } else {
            if fa * fb > 0.0 {
                return Err(GradingError::NotBracketed { low_net: fa + opts.target_net, high_net: fb + opts.target_net, target: opts.target_net });
            }
            let mut side = 0i8;
            let mut iter = 0;
            let mut result = None;
            while iter < opts.max_iterations {
                let c = (a * fb - b * fa) / (fb - fa);
                let fc = eval(c, &mut history)?;
                iter += 1;
                if done(fc) {
                    result = Some(c);
                    break;
                }
                if fc * fb > 0.0 {
                    b = c;
                    fb = fc;
                    if side == -1 {
                        fa /= 2.0;
                    }
                    side = -1;
                } else {
                    a = c;
                    fa = fc;
                    if side == 1 {
                        fb /= 2.0;
                    }
                    side = 1;
                }
                if (b - a).abs() < 1e-6 {
                    result = Some(c);
                    break;
                }
            }
            match result {
                Some(c) => (c, iter, true),
                None => {
                    let last = history.last().unwrap().shift;
                    (last, iter, false)
                }
            }
        };
        let net = history.iter().find(|h| h.shift == best).map(|h| h.net).unwrap_or(0.0);
        self.shift = best;
        Ok(BalanceResult { shift: best, net, converged, iterations, history })
    }
}

/// Welds a triangle soup into a `Tin`. Vertices closer than a micrometre in all three
/// coordinates are one vertex; triangles with no plan area are dropped.
pub(crate) fn weld(tris: &[[[f64; 3]; 3]]) -> Result<Tin> {
    let mut o = [f64::INFINITY, f64::INFINITY, f64::INFINITY];
    for t in tris {
        for p in t {
            for k in 0..3 {
                o[k] = o[k].min(p[k]);
            }
        }
    }
    let key = |p: &[f64; 3]| ((((p[0] - o[0]) * 1e6).round()) as i64, (((p[1] - o[1]) * 1e6).round()) as i64, (((p[2] - o[2]) * 1e6).round()) as i64);
    let mut map: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut points: Vec<[f64; 3]> = vec![];
    let mut idx: Vec<[u32; 3]> = vec![];
    for t in tris {
        let a2 = (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]);
        if a2.abs() < 1e-9 {
            continue;
        }
        let mut tri = [0u32; 3];
        for (k, p) in t.iter().enumerate() {
            tri[k] = *map.entry(key(p)).or_insert_with(|| {
                points.push(*p);
                (points.len() - 1) as u32
            });
        }
        if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] {
            continue;
        }
        idx.push(tri);
    }
    Ok(Tin::new(points, idx)?)
}

impl GroupBuild {
    /// Volume between `base` (the existing ground) and the group surface, with the terrain
    /// engine's exact TIN-to-TIN method: `cut` where the base is above the group, `fill` where
    /// it is below. `region` limits the volume to a polygon.
    pub fn volume_against(&self, base: &Tin, region: Option<&[[f64; 2]]>) -> Result<VolumeResult> {
        Ok(volume_between(base, &self.tin, region)?)
    }

    /// Stage-storage of the group surface as a basin (a pond) inside `region`, through the
    /// terrain crate. Keep the levels below the spill level of the region.
    pub fn stage_storage(&self, region: Option<&[[f64; 2]]>, levels: &[f64]) -> Result<Vec<StagePoint>> {
        Ok(stage_storage(&self.tin, region, levels)?)
    }

    /// The group surface as a terrain definition. See the module documentation.
    pub fn to_definition(&self) -> GroupDefinition {
        let pts = self.tin.points();
        let mut notes = vec![];
        let mut def = SurfaceDefinition::new();
        def.push(Operation::AddPoints { group: self.name.clone(), points: pts.iter().enumerate().map(|(i, p)| SurfacePoint::new(format!("{i}"), p[0], p[1], p[2])).collect() });
        let mut seen: HashSet<(u32, u32)> = HashSet::new();
        let mut lines = vec![];
        for t in self.tin.triangles() {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                let e = (a.min(b), a.max(b));
                if seen.insert(e) {
                    lines.push(Breakline { name: format!("edge-{}-{}", e.0, e.1), kind: BreaklineKind::Standard, vertices: vec![pts[e.0 as usize], pts[e.1 as usize]] });
                }
            }
        }
        def.push(Operation::AddBreaklines { lines });
        // Boundary rings from the edges that belong to one triangle.
        let edges = self.tin.boundary_edges();
        let mut adj: HashMap<u32, Vec<u32>> = HashMap::new();
        for e in &edges {
            adj.entry(e[0]).or_default().push(e[1]);
            adj.entry(e[1]).or_default().push(e[0]);
        }
        if adj.values().any(|v| v.len() != 2) {
            notes.push("the mesh boundary touches itself at a vertex; no boundary was written".into());
        } else {
            let mut used: HashSet<(u32, u32)> = HashSet::new();
            let mut rings: Vec<Vec<[f64; 2]>> = vec![];
            for e in &edges {
                if used.contains(&(e[0].min(e[1]), e[0].max(e[1]))) {
                    continue;
                }
                let (start, mut cur, mut prev) = (e[0], e[1], e[0]);
                let mut ring = vec![start];
                used.insert((e[0].min(e[1]), e[0].max(e[1])));
                while cur != start {
                    ring.push(cur);
                    let next = adj[&cur].iter().cloned().find(|n| *n != prev).unwrap_or(start);
                    used.insert((cur.min(next), cur.max(next)));
                    prev = cur;
                    cur = next;
                }
                rings.push(ring.iter().map(|&i| [pts[i as usize][0], pts[i as usize][1]]).collect());
            }
            let (mut big, mut big_a) = (usize::MAX, -1.0);
            for (k, r) in rings.iter().enumerate() {
                if r.len() >= 3 && area2(r).abs() > big_a {
                    big_a = area2(r).abs();
                    big = k;
                }
            }
            for (k, r) in rings.into_iter().enumerate() {
                if r.len() < 3 {
                    continue;
                }
                let kind = if k == big { BoundaryKind::Outer } else { BoundaryKind::Hide };
                def.push(Operation::AddBoundary { name: format!("ring-{k}"), kind, vertices: r });
            }
        }
        GroupDefinition { definition: def, notes }
    }
}
