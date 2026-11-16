//! Hydraulic and energy grade lines through a network from a tailwater level.
//!
//! # Method
//!
//! Working from each outlet upstream (steady flow, the flows of [`PipeNetwork::accumulate_flows`]):
//!
//! * At an outlet structure the hydraulic grade line (HGL) is the **tailwater level** (an input).
//! * For a pipe whose downstream-end water level `H` is at or above its downstream crown, or whose
//!   flow exceeds its full-bore discharge, the pipe is taken as flowing full and under pressure:
//!   `HGL_up = H + Sf L` with `Sf` the friction slope at the full-bore velocity `V = Q / A_full`,
//!   from the Manning or Colebrook-White law of [`crate::hydraulics::friction_slope`], and `L` the
//!   length along the pipe's centreline. (The energy equation between the two ends with the
//!   friction head loss `hf = Sf L`; Chow 1959, or any hydraulics text.)
//! * Otherwise the flow has a free surface. The water level is the uniform-flow depth above the
//!   invert at each end: `HGL = invert + y_n`.
//! * The energy grade line (EGL) is `HGL + V^2 / 2g` with `V = Q / A` for the flow area `A` in
//!   that pipe (g = 9.81 m/s2 from `afrobim_drainage`).
//!
//! # Structure losses
//!
//! Only if the structure carries a loss coefficient `K` (an input, `loss_coefficient`) does the
//! grade line rise across it by `K V^2 / (2g)`, `V` the velocity in the pipe leaving the structure
//! (the same form as LandXML's per-structure `lossIn` / `lossOut`). **This crate holds no method
//! for choosing K** (no inlet, bend, junction or benching loss formulae are cited), so unless the
//! caller enters K the result is **friction only**. [`GradeReport::friction_only`] says which.
//!
//! # Limits
//!
//! No gradually varied flow (backwater and drawdown) is computed. When the tailwater is above the
//! uniform-flow depth but below the crown, the backwater that would raise the upstream level is
//! ignored and an information finding says so. No hydraulic jump, no entrance or exit losses beyond
//! the K above. A branch (a structure with several pipes leaving) takes the highest downstream level.

use std::collections::BTreeMap;

use afrobim_drainage::design::GRAVITY_M_S2;
use serde::{Deserialize, Serialize};

use crate::finding::{Finding, Severity};
use crate::hydraulics::{friction_slope, normal_flow, HydraulicSettings};
use crate::model::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipeGrade {
    pub pipe: PipeId,
    pub flow_m3_s: f64,
    pub velocity_m_s: f64,
    /// `true` when the pipe was solved as full and under pressure.
    pub pressure_flow: bool,
    /// Friction head lost along the pipe, m (zero for free-surface flow).
    pub friction_loss_m: f64,
    pub hgl_start_m: f64,
    pub hgl_end_m: f64,
    pub egl_start_m: f64,
    pub egl_end_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructureGrade {
    pub structure: StructureId,
    pub hgl_m: f64,
    pub egl_m: f64,
    /// Loss `K V^2 / 2g` applied at this structure, m (zero without a K).
    pub loss_m: f64,
    pub rim_level_m: f64,
    pub above_rim: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradeReport {
    pub tailwater_level_m: f64,
    pub pipes: Vec<PipeGrade>,
    pub structures: Vec<StructureGrade>,
    /// No structure had a loss coefficient: only pipe friction is in the grade lines.
    pub friction_only: bool,
    pub method: String,
    pub findings: Vec<Finding>,
}

impl PipeNetwork {
    /// HGL and EGL through the network from a tailwater level at the outlets, m. See the module
    /// documentation for the method and its limits.
    pub fn grade_lines(&self, tailwater_level_m: f64, settings: &HydraulicSettings) -> GradeReport {
        let fr = self.accumulate_flows();
        let mut findings = fr.findings.clone();
        let topo = self.topology();
        let mut pipe_rows: BTreeMap<PipeId, PipeGrade> = BTreeMap::new();
        let mut struct_rows: BTreeMap<StructureId, StructureGrade> = BTreeMap::new();
        let mut any_k = false;
        for sid in topo.order.iter().rev() {
            let Ok(s) = self.structure(*sid) else { continue };
            let outs = self.outgoing(*sid);
            // the water level in the structure
            let mut h = f64::NEG_INFINITY;
            let mut loss = 0.0;
            let mut egl_s = f64::NEG_INFINITY;
            if outs.is_empty() {
                h = tailwater_level_m;
                egl_s = h;
            }
            for o in &outs {
                if let Some(pg) = pipe_rows.get(o) {
                    let l = s.loss_coefficient.map(|k| k * pg.velocity_m_s * pg.velocity_m_s / (2.0 * GRAVITY_M_S2)).unwrap_or(0.0);
                    if s.loss_coefficient.is_some() {
                        any_k = true;
                    }
                    if pg.hgl_start_m + l > h {
                        h = pg.hgl_start_m + l;
                        loss = l;
                        egl_s = pg.egl_start_m + l;
                    }
                }
            }
            if !h.is_finite() {
                continue;
            }
            struct_rows.insert(*sid, StructureGrade { structure: *sid, hgl_m: h, egl_m: egl_s, loss_m: loss, rim_level_m: s.rim_level_m, above_rim: h > s.rim_level_m });
            if h > s.rim_level_m {
                findings.push(Finding::new("hgl", Severity::Warning, vec![ElementRef::Structure(*sid)], format!("the hydraulic grade line at structure {} is {h:.3} m, above its rim level {:.3} m", s.name, s.rim_level_m)));
            }
            // the pipes entering this structure
            for pid in self.incoming(*sid) {
                let Ok(p) = self.pipe(pid) else { continue };
                let q = fr.flows.get(&pid).copied().unwrap_or(0.0);
                let el = vec![ElementRef::Pipe(pid)];
                let Some(rough) = p.roughness else {
                    findings.push(Finding::new("data", Severity::Error, el, format!("pipe {} has no roughness, so the grade line through it was not computed", p.name)));
                    continue;
                };
                let Some(full_area) = p.shape.full_area_m2() else {
                    findings.push(Finding::new("data", Severity::Error, el, format!("pipe {} has no usable section, so the grade line through it was not computed", p.name)));
                    continue;
                };
                let full_per = p.shape.full_perimeter_m().unwrap_or(f64::NAN);
                let r_full = full_area / full_per;
                let l3 = self.sloping_length_m(pid).unwrap_or(0.0);
                let crown_down = p.end_invert_m + p.shape.height_m();
                let slope = self.slope(pid).unwrap_or(f64::NAN);
                let nf = if slope > 0.0 { normal_flow(&p.shape, &rough, slope, q, settings).ok() } else { None };
                let pressure = h >= crown_down || nf.is_none_or(|n| n.exceeds_capacity);
                let row = if pressure {
                    let v = q / full_area;
                    match friction_slope(&rough, v, r_full, settings) {
                        Ok(sf) => {
                            let hf = sf * l3;
                            let base = h.max(crown_down);
                            let vh = v * v / (2.0 * GRAVITY_M_S2);
                            PipeGrade { pipe: pid, flow_m3_s: q, velocity_m_s: v, pressure_flow: true, friction_loss_m: hf, hgl_start_m: base + hf, hgl_end_m: base, egl_start_m: base + hf + vh, egl_end_m: base + vh }
                        }
                        Err(e) => {
                            findings.push(Finding::new("data", Severity::Error, el, format!("pipe {}: {e}", p.name)));
                            continue;
                        }
                    }
                } else if let Some(n) = nf {
                    let up = p.start_invert_m + n.depth_m;
                    let down = p.end_invert_m + n.depth_m;
                    if h > down + 1e-3 {
                        findings.push(Finding::new(
                            "hgl",
                            Severity::Info,
                            el,
                            format!("pipe {}: the water level {h:.3} m at its downstream end is above the uniform-flow level {down:.3} m but below the crown; backwater is not computed, so the upstream level is the uniform-flow level", p.name),
                        ));
                    }
                    let vh = n.velocity_m_s * n.velocity_m_s / (2.0 * GRAVITY_M_S2);
                    PipeGrade { pipe: pid, flow_m3_s: q, velocity_m_s: n.velocity_m_s, pressure_flow: false, friction_loss_m: 0.0, hgl_start_m: up, hgl_end_m: down.max(h.min(crown_down)), egl_start_m: up + vh, egl_end_m: down.max(h.min(crown_down)) + vh }
                } else {
                    continue;
                };
                pipe_rows.insert(pid, row);
            }
        }
        let method = if any_k {
            "steady flow from the tailwater; full-bore friction (Manning or Colebrook-White) where the downstream end is at or above the crown, uniform-flow depth otherwise; structure loss K V^2/2g only where the structure has a loss coefficient K (an input); no backwater".to_string()
        } else {
            "steady flow from the tailwater; full-bore friction (Manning or Colebrook-White) where the downstream end is at or above the crown, uniform-flow depth otherwise; friction only: no structure loss coefficients were entered and no loss method is included; no backwater".to_string()
        };
        GradeReport { tailwater_level_m, pipes: pipe_rows.into_values().collect(), structures: struct_rows.into_values().collect(), friction_only: !any_k, method, findings }
    }
}
