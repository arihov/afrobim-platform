//! Checks. Each returns a list of [`Finding`]s with element ids and plain sentences.
//!
//! No check has a default limit: a limit is checked only when the rule set gives it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use afrobim_drainage::design::circular_critical_depth_m;

use crate::finding::{Finding, Severity};
use crate::ground::Ground;
use crate::hydraulics::{normal_flow, HydraulicSettings, NormalFlow};
use crate::model::*;
use crate::rules::RuleSet;

/// Per-pipe hydraulic results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipeHydraulics {
    pub pipe: PipeId,
    pub flow_m3_s: f64,
    /// Fall over plan length, m/m.
    pub slope: f64,
    /// `None` when the pipe could not be computed (the findings say why).
    pub flow: Option<NormalFlow>,
    /// Flow over full-bore discharge.
    pub capacity_ratio: Option<f64>,
    /// Critical depth of the flow in a circular pipe, m (`afrobim_drainage::circular_critical_depth_m`).
    pub critical_depth_m: Option<f64>,
}

/// Hydraulic results and findings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HydraulicReport {
    pub pipes: Vec<PipeHydraulics>,
    pub findings: Vec<Finding>,
}

impl PipeNetwork {
    /// Check cover against a ground surface: the ground level minus the outside top of the
    /// barrel (crown plus wall thickness), at both ends and, if `spacing_m` is given, along the
    /// pipe at that spacing. Only `min_cover_m` and `max_cover_m` are checked. At most one
    /// finding per pipe per limit is made, for the worst place.
    pub fn check_cover(&self, ground: &dyn Ground, rules: &RuleSet, spacing_m: Option<f64>) -> Vec<Finding> {
        let mut out = Vec::new();
        if rules.min_cover_m.is_none() && rules.max_cover_m.is_none() {
            return out;
        }
        for p in self.pipes() {
            let Ok(path) = self.pipe_path(p.id) else { continue };
            let l = path.length_m;
            let mut stations = vec![0.0, l];
            if let Some(sp) = spacing_m.filter(|s| *s > 0.0 && l > 0.0) {
                let mut c = sp;
                while c < l {
                    stations.push(c);
                    c += sp;
                }
            }
            stations.sort_by(|a, b| a.partial_cmp(b).unwrap());
            stations.dedup();
            let down = p.wall_thickness_m.unwrap_or(0.0) + p.shape.height_m();
            let mut worst_min: Option<(f64, f64)> = None;
            let mut worst_max: Option<(f64, f64)> = None;
            let mut off_surface = false;
            for c in stations {
                let pt = path.point_at(c);
                let Some(g) = ground.level_m(pt[0], pt[1]) else {
                    off_surface = true;
                    continue;
                };
                let t = if l > 0.0 { c / l } else { 0.0 };
                let invert = p.start_invert_m + (p.end_invert_m - p.start_invert_m) * t;
                let cover = g - (invert + down);
                if rules.min_cover_m.is_some_and(|m| cover < m) && worst_min.is_none_or(|w| cover < w.0) {
                    worst_min = Some((cover, c));
                }
                if rules.max_cover_m.is_some_and(|m| cover > m) && worst_max.is_none_or(|w| cover > w.0) {
                    worst_max = Some((cover, c));
                }
            }
            if off_surface {
                out.push(Finding::new("cover", Severity::Warning, vec![ElementRef::Pipe(p.id)], format!("pipe {} runs off the ground surface, so its cover was not checked there", p.name)));
            }
            if let (Some((cover, c)), Some(min)) = (worst_min, rules.min_cover_m) {
                out.push(Finding::new("cover", Severity::Warning, vec![ElementRef::Pipe(p.id)], format!("pipe {} has {cover:.3} m of cover at {c:.3} m from its start, less than the minimum {min:.3} m", p.name)));
            }
            if let (Some((cover, c)), Some(max)) = (worst_max, rules.max_cover_m) {
                out.push(Finding::new("cover", Severity::Warning, vec![ElementRef::Pipe(p.id)], format!("pipe {} has {cover:.3} m of cover at {c:.3} m from its start, more than the maximum {max:.3} m", p.name)));
            }
        }
        out
    }

    /// Check slope. A pipe that rises in the flow direction is always reported (a gravity pipe
    /// must fall); `min_slope` and `max_slope` are checked when set.
    pub fn check_slope(&self, rules: &RuleSet) -> Vec<Finding> {
        let mut out = Vec::new();
        for p in self.pipes() {
            let Ok(s) = self.slope(p.id) else { continue };
            if !s.is_finite() {
                out.push(Finding::new("slope", Severity::Error, vec![ElementRef::Pipe(p.id)], format!("pipe {} has zero length, so it has no slope", p.name)));
                continue;
            }
            if s < 0.0 {
                out.push(Finding::new("slope", Severity::Warning, vec![ElementRef::Pipe(p.id)], format!("pipe {} rises {:.3} m in the flow direction ({:.4} m/m)", p.name, -s * self.plan_length_m(p.id).unwrap_or(0.0), s)));
                continue;
            }
            if let Some(min) = rules.min_slope.filter(|m| s < *m) {
                out.push(Finding::new("slope", Severity::Warning, vec![ElementRef::Pipe(p.id)], format!("pipe {} has slope {s:.5} m/m, less than the minimum {min:.5} m/m", p.name)));
            }
            if let Some(max) = rules.max_slope.filter(|m| s > *m) {
                out.push(Finding::new("slope", Severity::Warning, vec![ElementRef::Pipe(p.id)], format!("pipe {} has slope {s:.5} m/m, more than the maximum {max:.5} m/m", p.name)));
            }
        }
        out
    }

    /// Check pipe length (structure centre to structure centre, along the plan path) against
    /// `max_pipe_length_m`.
    pub fn check_length(&self, rules: &RuleSet) -> Vec<Finding> {
        let mut out = Vec::new();
        let Some(max) = rules.max_pipe_length_m else { return out };
        for p in self.pipes() {
            if let Ok(l) = self.plan_length_m(p.id) {
                if l > max {
                    out.push(Finding::new("length", Severity::Warning, vec![ElementRef::Pipe(p.id)], format!("pipe {} is {l:.3} m long, more than the maximum {max:.3} m", p.name)));
                }
            }
        }
        out
    }

    /// Check the drop and level match at structures against the rule set: the matched level of
    /// each pipe leaving must be `structure_drop_m` below that of the lowest pipe entering
    /// (tolerance 1 mm), and no pipe may leave a structure above a pipe entering it.
    pub fn check_structure_drops(&self, rules: &RuleSet) -> Vec<Finding> {
        let mut out = Vec::new();
        let Some(mode) = rules.match_mode else { return out };
        let drop = rules.structure_drop_m.unwrap_or(0.0);
        for s in self.structures() {
            let ins = self.incoming(s.id);
            let outs = self.outgoing(s.id);
            if ins.is_empty() || outs.is_empty() {
                continue;
            }
            let lowest_in = ins.iter().filter_map(|i| self.pipe(*i).ok()).map(|p| mode.pipe_end_level_m(p, false)).fold(f64::INFINITY, f64::min);
            for o in outs {
                let Ok(op) = self.pipe(o) else { continue };
                let lvl = mode.pipe_end_level_m(op, true);
                let actual = lowest_in - lvl;
                if (actual - drop).abs() > 1e-3 {
                    let mut el = vec![ElementRef::Structure(s.id), ElementRef::Pipe(o)];
                    el.extend(ins.iter().map(|i| ElementRef::Pipe(*i)));
                    out.push(Finding::new("drop", Severity::Warning, el, format!("at structure {} the drop to pipe {} is {actual:.3} m, the rule says {drop:.3} m", s.name, op.name)));
                }
            }
        }
        out
    }

    /// Velocity, capacity and partial-flow depth at the design flow of each pipe.
    ///
    /// Flows come from [`PipeNetwork::accumulate_flows`] (a pipe's own design flow, or the
    /// structure inflows summed down the network). Each pipe is solved for uniform flow at its
    /// own slope with its own roughness; see [`crate::hydraulics`] for the method. The rules
    /// checked are `min_velocity_m_s`, `max_velocity_m_s` and `max_depth_ratio`. A flow above the
    /// full-bore discharge is always reported as `capacity`.
    pub fn check_hydraulics(&self, rules: &RuleSet, settings: &HydraulicSettings) -> HydraulicReport {
        let fr = self.accumulate_flows();
        let mut findings = fr.findings;
        let mut rows = Vec::new();
        for p in self.pipes() {
            let el = vec![ElementRef::Pipe(p.id)];
            let Some(&q) = fr.flows.get(&p.id) else { continue };
            let slope = self.slope(p.id).unwrap_or(f64::NAN);
            let mut row = PipeHydraulics { pipe: p.id, flow_m3_s: q, slope, flow: None, capacity_ratio: None, critical_depth_m: None };
            let Some(rough) = p.roughness else {
                findings.push(Finding::new("data", Severity::Error, el, format!("pipe {} has no roughness, so its hydraulics were not computed", p.name)));
                rows.push(row);
                continue;
            };
            if !(slope > 0.0) {
                findings.push(Finding::new("data", Severity::Warning, el, format!("pipe {} has slope {slope:.5} m/m; uniform flow needs a falling pipe, so its hydraulics were not computed", p.name)));
                rows.push(row);
                continue;
            }
            match normal_flow(&p.shape, &rough, slope, q, settings) {
                Err(e) => {
                    findings.push(Finding::new("data", Severity::Error, el, format!("pipe {}: {e}", p.name)));
                }
                Ok(nf) => {
                    row.capacity_ratio = Some(q / nf.full_discharge_m3_s);
                    if let PipeShape::Circular { diameter_m } = p.shape {
                        row.critical_depth_m = Some(circular_critical_depth_m(q, diameter_m));
                    }
                    if nf.exceeds_capacity {
                        findings.push(Finding::new("capacity", Severity::Warning, el.clone(), format!("pipe {} carries {q:.4} m3/s, more than its full-bore capacity of {:.4} m3/s", p.name, nf.full_discharge_m3_s)));
                    }
                    if q > 0.0 {
                        if let Some(min) = rules.min_velocity_m_s.filter(|m| nf.velocity_m_s < *m) {
                            findings.push(Finding::new("velocity", Severity::Warning, el.clone(), format!("pipe {} has velocity {:.3} m/s at {q:.4} m3/s, less than the minimum {min:.3} m/s", p.name, nf.velocity_m_s)));
                        }
                        if let Some(max) = rules.max_velocity_m_s.filter(|m| nf.velocity_m_s > *m) {
                            findings.push(Finding::new("velocity", Severity::Warning, el.clone(), format!("pipe {} has velocity {:.3} m/s at {q:.4} m3/s, more than the maximum {max:.3} m/s", p.name, nf.velocity_m_s)));
                        }
                        if let Some(max) = rules.max_depth_ratio.filter(|m| nf.depth_ratio > *m) {
                            findings.push(Finding::new("depth_ratio", Severity::Warning, el.clone(), format!("pipe {} flows at depth ratio {:.3} at {q:.4} m3/s, more than the maximum {max:.3}", p.name, nf.depth_ratio)));
                        }
                    } else if rules.min_velocity_m_s.is_some() || rules.max_velocity_m_s.is_some() || rules.max_depth_ratio.is_some() {
                        findings.push(Finding::new("data", Severity::Info, el, format!("pipe {} has zero design flow, so velocity and depth rules were not checked", p.name)));
                    }
                    row.flow = Some(nf);
                }
            }
            rows.push(row);
        }
        HydraulicReport { pipes: rows, findings }
    }

    /// All the geometric rule checks together: cover (when a ground is given), slope, length and
    /// structure drops.
    pub fn check_all_geometry(&self, rules: &RuleSet, ground: Option<&dyn Ground>, cover_spacing_m: Option<f64>) -> Vec<Finding> {
        let mut f = self.topology_findings();
        if let Some(g) = ground {
            f.extend(self.check_cover(g, rules, cover_spacing_m));
        }
        f.extend(self.check_slope(rules));
        f.extend(self.check_length(rules));
        f.extend(self.check_structure_drops(rules));
        f
    }

    /// Cover along each pipe at a spacing: `(pipe, [(distance from start, ground, cover)])`.
    pub fn cover_profile(&self, ground: &dyn Ground, spacing_m: f64) -> BTreeMap<PipeId, Vec<(f64, Option<f64>, Option<f64>)>> {
        let mut out = BTreeMap::new();
        for p in self.pipes() {
            let Ok(path) = self.pipe_path(p.id) else { continue };
            let l = path.length_m;
            let mut stations = vec![0.0];
            if spacing_m > 0.0 {
                let mut c = spacing_m;
                while c < l {
                    stations.push(c);
                    c += spacing_m;
                }
            }
            stations.push(l);
            let down = p.wall_thickness_m.unwrap_or(0.0) + p.shape.height_m();
            let rows = stations
                .into_iter()
                .map(|c| {
                    let pt = path.point_at(c);
                    let g = ground.level_m(pt[0], pt[1]);
                    let t = if l > 0.0 { c / l } else { 0.0 };
                    let inv = p.start_invert_m + (p.end_invert_m - p.start_invert_m) * t;
                    (c, g, g.map(|g| g - (inv + down)))
                })
                .collect();
            out.insert(p.id, rows);
        }
        out
    }
}
