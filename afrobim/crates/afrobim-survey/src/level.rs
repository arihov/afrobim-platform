// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! One dimensional (height) least squares adjustment of levelling runs.
//!
//! Observation equation of a height difference from `from` to `to`: dh = H_to - H_from + noise,
//! linear in the unknown heights (Teunissen, *Network Quality Control*, 2024, section 4.2.4, eq. 4.40,
//! the levelling network example). Standard error of a leg (propagation, derived here):
//! sigma^2 = a^2 + b^2 L_km with a the constant part and b the part per square root of kilometre of
//! the equipment record (both inputs); L_km is the length of the leg in kilometres.
//!
//! Other height observations that are included: the vertical component of GNSS baselines (with their
//! stated sigma) and weighted control heights (a control point with Z not fixed and a sigma). Total
//! station trigonometric heights are not combined here.
//!
//! The estimation, the global test, the w-test and the redundancy numbers are those of `lsq.rs`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::adjust::{AdjustOptions, GlobalTest, ObsKind, ObsResult};
use crate::error::{Result, SurveyError};
use crate::lsq::{self, Row};
use crate::model::*;
use crate::stats::chi2_quantile;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdjustedHeight {
    pub name: String,
    pub z: f64,
    pub sigma: f64,
    pub fixed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LevelAdjustment {
    pub network: String,
    pub iterations: usize,
    pub converged: bool,
    pub n_observations: usize,
    pub n_unknowns: usize,
    pub dof: isize,
    pub vtpv: f64,
    pub sigma0_sq_posteriori: Option<f64>,
    pub global_test: Option<GlobalTest>,
    pub heights: Vec<AdjustedHeight>,
    pub observations: Vec<ObsResult>,
}

pub fn adjust_levelling(db: &SurveyDatabase, network: &str, opts: &AdjustOptions) -> Result<LevelAdjustment> {
    let net = db.network(network)?;
    let mut names: Vec<String> = Vec::new();
    let mut map: HashMap<String, usize> = HashMap::new();
    let mut x0: Vec<f64> = Vec::new();
    // heights: every point name that occurs; fixed ones have no unknown
    let mut fixed_val: HashMap<String, f64> = HashMap::new();
    let mut ensure = |n: &str, names: &mut Vec<String>, x0: &mut Vec<f64>| -> Result<usize> {
        if let Some(&i) = map.get(n) {
            return Ok(i);
        }
        let p = net.point(n);
        let fixed = p.map(|p| p.control && p.fix_z && p.z.is_some()).unwrap_or(false);
        let z0 = p.and_then(|p| p.z).unwrap_or(0.0);
        names.push(n.to_string());
        let idx = names.len() - 1;
        map.insert(n.to_string(), idx);
        if fixed {
            fixed_val.insert(n.to_string(), z0);
        } else {
            x0.push(z0);
        }
        Ok(idx)
    };
    // parameter index of each point (None if fixed): built after all points are known
    struct Raw {
        key: String,
        kind: ObsKind,
        from: String,
        to: String,
        value: f64,
        sigma: f64,
        from_pt: Option<usize>,
        to_pt: usize,
    }
    let mut raws: Vec<Raw> = Vec::new();
    for run in &net.level_runs {
        let en = run.equipment.as_ref().or(opts.default_equipment.as_ref()).ok_or_else(|| SurveyError::Invalid(format!("levelling run {} names no equipment and no default is given", run.name)))?;
        let eq = db.equipment(en).ok_or_else(|| SurveyError::Invalid(format!("equipment '{en}' is not in the database")))?;
        for (i, leg) in run.legs.iter().enumerate() {
            let key = format!("LV{}:{}:{}>{}", run.id, i, leg.from, leg.to);
            if opts.excluded.contains(&key) {
                continue;
            }
            let a = ensure(&leg.from, &mut names, &mut x0)?;
            let b = ensure(&leg.to, &mut names, &mut x0)?;
            let var = eq.level_sigma_const_m.powi(2) + eq.level_sigma_per_sqrt_km_m.powi(2) * leg.length_m / 1000.0;
            raws.push(Raw { key, kind: ObsKind::HeightDifference, from: leg.from.clone(), to: leg.to.clone(), value: leg.dh, sigma: var.sqrt(), from_pt: Some(a), to_pt: b });
        }
    }
    for (i, b) in net.baselines.iter().enumerate() {
        let key = format!("BL{}:{}>{}:U", i, b.from, b.to);
        if opts.excluded.contains(&key) {
            continue;
        }
        let a = ensure(&b.from, &mut names, &mut x0)?;
        let t = ensure(&b.to, &mut names, &mut x0)?;
        raws.push(Raw { key, kind: ObsKind::BaselineU, from: b.from.clone(), to: b.to.clone(), value: b.du, sigma: b.sigma_u, from_pt: Some(a), to_pt: t });
    }
    for p in &net.points {
        if let (true, false, Some(s), Some(z)) = (p.control, p.fix_z, p.sigma_z, p.z) {
            if map.contains_key(&p.name) {
                let key = format!("PT:{}:Z", p.name);
                if !opts.excluded.contains(&key) {
                    raws.push(Raw { key, kind: ObsKind::CoordinateZ, from: p.name.clone(), to: String::new(), value: z, sigma: s, from_pt: None, to_pt: map[&p.name] });
                }
            }
        }
    }
    if raws.is_empty() {
        return Err(SurveyError::Invalid("no height observations".into()));
    }
    // parameter index per point
    let mut pidx: Vec<Option<usize>> = Vec::new();
    let mut k = 0;
    for n in &names {
        if fixed_val.contains_key(n) {
            pidx.push(None);
        } else {
            pidx.push(Some(k));
            k += 1;
        }
    }
    let obs = raws;
    let zfix = |n: usize| fixed_val.get(&names[n]).copied();
    let sigmas: Vec<f64> = obs.iter().map(|o| o.sigma).collect();
    let mut lin = |x: &[f64]| -> Result<Vec<Row>> {
        let mut rows = Vec::new();
        for o in &obs {
            let get = |pt: usize| match pidx[pt] {
                Some(i) => x[i],
                None => zfix(pt).unwrap(),
            };
            let mut cols = Vec::new();
            if let Some(i) = pidx[o.to_pt] {
                cols.push((i, 1.0));
            }
            let f = match o.from_pt {
                Some(fp) => {
                    if let Some(i) = pidx[fp] {
                        cols.push((i, -1.0));
                    }
                    get(o.to_pt) - get(fp)
                }
                None => get(o.to_pt),
            };
            rows.push(Row { l: o.value - f, cols });
        }
        Ok(rows)
    };
    let sol = lsq::solve(x0.clone(), &sigmas, &mut lin, opts.max_iterations, opts.convergence)?;
    let dof = sol.dof;
    let s0 = if dof > 0 { Some(sol.vtpv / dof as f64) } else { None };
    let heights = names
        .iter()
        .enumerate()
        .map(|(i, n)| match pidx[i] {
            Some(k) => AdjustedHeight { name: n.clone(), z: sol.x[k], sigma: sol.qx.get(k, k).max(0.0).sqrt() * if opts.scale_by_posteriori { s0.unwrap_or(1.0).sqrt() } else { 1.0 }, fixed: false },
            None => AdjustedHeight { name: n.clone(), z: fixed_val[n], sigma: 0.0, fixed: true },
        })
        .collect();
    let observations = obs
        .iter()
        .enumerate()
        .map(|(i, o)| ObsResult {
            key: o.key.clone(),
            kind: o.kind,
            from: o.from.clone(),
            to: o.to.clone(),
            observed: o.value,
            adjusted: o.value + sol.v[i],
            residual: sol.v[i],
            sigma: o.sigma,
            sigma_residual: sol.sigma_v[i],
            redundancy: sol.redundancy[i],
            std_residual: sol.w[i],
            flagged: sol.w[i].map(|w| w.abs() > opts.blunder_threshold).unwrap_or(false),
        })
        .collect();
    let global_test = if dof > 0 {
        let d = dof as f64;
        let lo = chi2_quantile(opts.global_alpha / 2.0, d);
        let hi = chi2_quantile(1.0 - opts.global_alpha / 2.0, d);
        let one = chi2_quantile(1.0 - opts.global_alpha, d);
        Some(GlobalTest { statistic: sol.vtpv, dof: dof as usize, alpha: opts.global_alpha, lower_critical: lo, upper_critical: hi, one_sided_critical: one, passes: sol.vtpv >= lo && sol.vtpv <= hi, exceeds_one_sided: sol.vtpv > one })
    } else {
        None
    };
    Ok(LevelAdjustment { network: network.into(), iterations: sol.iterations, converged: sol.converged, n_observations: obs.len(), n_unknowns: x0.len(), dof, vtpv: sol.vtpv, sigma0_sq_posteriori: s0, global_test, heights, observations })
}
