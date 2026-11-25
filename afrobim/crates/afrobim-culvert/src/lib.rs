// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! AfroBIM culvert engine: a parametric culvert designer that turns user data into geometry,
//! quantities and a bar bending schedule. Phase 1 is the engine only; an app window drives it
//! through [`design`].
//!
//! See the README for every formula, the defaults and what is matched to the Km 39+136 drawing.

mod bbs;
mod drawings;
mod error;
mod layout;
mod model;
mod quantities;
mod road;
mod setout;

use afrobim_drainage::design::culvert_check::{culvert_check, CulvertCheckInput};
use afrobim_drainage::design::inlet::InletEdge;
use afrobim_drainage::design::mowt::OutletControlInput;
use serde::{Deserialize, Serialize};

pub use bbs::{
    check_member_counts, min_bend_radius_mm, raw_cutting_length_mm, schedule, shape_defs,
    unit_mass_kg_per_m, Applies, BarRule, Basis, Bbs, BbsRow, Count, DimExpr, DimRule,
    ElementCount, ElementKind, MemberCounts, ReinforcementTable, ScheduleLine, Series, ShapeDef, Q,
};
pub use drawings::{draw_all, CulvertDrawings};
pub use error::CulvertError;
pub use layout::{derive, Check, EndLayout, Layout};
pub use model::{
    default_notes, pipe_catalogue, ApronSpec, Barrel, Bedding, BoxSpec, ConcreteClasses,
    CulvertInput, DefaultNote, DefaultSource, EndStructure, InvertSpec, LengthRule,
    PipeCatalogueEntry, PipeSpec, QuantityOptions, SettingOutInput, Thresholds, ToeSpec, WeepSpec,
    WingSpec,
};
pub use quantities::{measure, EndQuantities, Quantities, QuantityLine};
pub use road::{
    KerbWalkway, Ogl, RoadSection, RoadSource, RoadTemplate, SectionPoint, Side, SideTemplate, Toe,
};
pub use setout::{axis_bearing_deg, axis_point, setting_out, SettingOutPoint};

/// The element category the app uses for culverts of this engine.
pub const CATEGORY: &str = "culvert";

/// A finished design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CulvertDesign {
    pub name: String,
    pub layout: Layout,
    pub bbs: Bbs,
    pub quantities: Quantities,
    pub drawings: CulvertDrawings,
    /// Empty when no setting-out position was given.
    pub setting_out: Vec<SettingOutPoint>,
    /// Layout checks plus the plan overlap check.
    pub checks: Vec<Check>,
}

impl CulvertDesign {
    /// Warnings where an imported or typed schedule's member counts disagree with this design's
    /// geometry, naming the marks (see [`check_member_counts`]).
    pub fn check_schedule(
        &self,
        input: &CulvertInput,
        lines: &[ScheduleLine],
    ) -> Result<Vec<String>, CulvertError> {
        check_member_counts(input, &self.layout, lines)
    }
}

/// Designs a culvert: validates the input, derives the geometry, schedules the reinforcement,
/// measures the quantities and draws the sheets.
pub fn design(input: &CulvertInput) -> Result<CulvertDesign, CulvertError> {
    let layout = derive(input)?;
    let bbs = schedule(input, &layout)?;
    let quantities = measure(input, &layout, &bbs);
    let points = input
        .setting_out
        .map(|s| setting_out(&layout, &s))
        .unwrap_or_default();
    let drawings = draw_all(input, &layout, &points);
    let mut checks = layout.checks.clone();
    let overlaps = plan_overlaps(&layout);
    checks.push(Check {
        name: "plan overlaps".to_string(),
        value: overlaps.len() as f64,
        limit: 0.0,
        unit: "nr".to_string(),
        passed: overlaps.is_empty(),
        message: if overlaps.is_empty() {
            "no end-structure elements overlap in plan".to_string()
        } else {
            format!("these overlap in plan: {}", overlaps.join("; "))
        },
    });
    Ok(CulvertDesign {
        name: input.name.clone(),
        layout,
        bbs,
        quantities,
        drawings,
        setting_out: points,
        checks,
    })
}

/// The concrete elements of both end structures as convex plan polygons (millimetres, axis frame:
/// x along the culvert from the left end to the right end, y to the left of that direction).
pub fn plan_solids(layout: &Layout) -> Vec<(String, Vec<(f64, f64)>)> {
    let mut out = Vec::new();
    for (tag, e) in [("left", &layout.left), ("right", &layout.right)] {
        let sd = e.side;
        let xy = |o: f64, t: f64| match sd {
            Side::Left => (-layout.left_len_mm - o, t),
            Side::Right => (layout.right_len_mm + o, t),
        };
        let s = &e.spec;
        let hw = e.headwall_width_mm / 2.0;
        out.push((
            format!("{tag} headwall"),
            vec![
                xy(0.0, -hw),
                xy(0.0, hw),
                xy(-s.headwall_thickness_mm, hw),
                xy(-s.headwall_thickness_mm, -hw),
            ],
        ));
        let a = s.apron.length_mm;
        out.push((
            format!("{tag} apron"),
            vec![
                xy(0.0, -hw),
                xy(0.0, hw),
                xy(a, e.apron_width_at_mm(a) / 2.0),
                xy(a, -e.apron_width_at_mm(a) / 2.0),
            ],
        ));
        let (sn, cs) = (e.splay_rad.sin(), e.splay_rad.cos());
        let th = s.wing.thickness_mm;
        for (name, k) in [("upper", 1.0), ("lower", -1.0)] {
            out.push((
                format!("{tag} {name} wing"),
                vec![
                    xy(0.0, k * hw),
                    xy(e.wing_reach_mm, k * (hw + e.wing_spread_mm)),
                    xy(
                        e.wing_reach_mm - th * sn,
                        k * (hw + e.wing_spread_mm + th * cs),
                    ),
                    xy(-th * sn, k * (hw + th * cs)),
                ],
            ));
        }
    }
    out
}

fn overlap_depth(a: &[(f64, f64)], b: &[(f64, f64)]) -> f64 {
    // Separating-axis test on convex polygons: the smallest overlap over all edge normals.
    let mut least = f64::MAX;
    for poly in [a, b] {
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            let (nx, ny) = (-(q.1 - p.1), q.0 - p.0);
            let len = (nx * nx + ny * ny).sqrt();
            if len < 1e-9 {
                continue;
            }
            let proj = |pts: &[(f64, f64)]| {
                pts.iter()
                    .map(|v| (v.0 * nx + v.1 * ny) / len)
                    .fold((f64::MAX, f64::MIN), |(lo, hi), d| (lo.min(d), hi.max(d)))
            };
            let (a_lo, a_hi) = proj(a);
            let (b_lo, b_hi) = proj(b);
            least = least.min(a_hi.min(b_hi) - a_lo.max(b_lo));
        }
    }
    least
}

/// Names of the pairs of end-structure elements that overlap in plan by more than 1 mm. Wings
/// stand on the toe beam, so the toe beams are not tested against them.
pub fn plan_overlaps(layout: &Layout) -> Vec<String> {
    let solids = plan_solids(layout);
    let mut out = Vec::new();
    for i in 0..solids.len() {
        for j in i + 1..solids.len() {
            if overlap_depth(&solids[i].1, &solids[j].1) > 1.0 {
                out.push(format!("{} and {}", solids[i].0, solids[j].0));
            }
        }
    }
    out
}

/// What the drainage crate's culvert hydraulics need besides the geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HydraulicsInput {
    pub design_flow_m3_s: f64,
    pub manning_n: f64,
    pub entrance_loss_ke: f64,
    pub tailwater_m: f64,
    pub inlet_edge: InletEdge,
    /// The largest headwater above the inlet invert the engineer accepts, m.
    pub allowable_headwater_m: f64,
}

/// The smallest catalogue pipe, for the cell count of `input`, whose governing headwater (inlet
/// or outlet control, from `afrobim-drainage`) is within the allowable headwater and the
/// manual's HW/D limit. The barrel length and fall are those of the current layout.
pub fn select_pipe(input: &CulvertInput, h: &HydraulicsInput) -> Result<PipeSpec, CulvertError> {
    let cells = input.barrel.cells();
    let spacing = match input.barrel {
        Barrel::Pipe(p) => p.clear_spacing_mm,
        Barrel::Box(_) => 300.0,
    };
    // The length and fall do not depend on which pipe is chosen when the inverts are at the ends;
    // start from the current barrel.
    let layout = derive(input)?;
    let fall_m = (layout.invert_left_m - layout.invert_right_m).abs();
    let length_m = layout.barrel_len_mm / 1000.0;
    let mut last_refusal = None;
    for entry in pipe_catalogue() {
        let result = culvert_check(&CulvertCheckInput {
            outlet: OutletControlInput {
                design_flow_m3_s: h.design_flow_m3_s,
                barrels: cells,
                diameter_m: entry.internal_diameter_mm / 1000.0,
                length_m,
                fall_m,
                manning_n: h.manning_n,
                entrance_loss_ke: h.entrance_loss_ke,
                tailwater_m: h.tailwater_m,
            },
            inlet_edge: h.inlet_edge,
            allowable_headwater_m: h.allowable_headwater_m,
            freeboard: None,
        });
        match result {
            Ok(r) if r.passes() => {
                return PipeSpec::catalogue(entry.internal_diameter_mm, cells, spacing)
            }
            Ok(_) => {}
            // A pipe too small for the flow can fall outside the charts the drainage crate
            // implements: that is a pipe that does not pass, not a failure of the search.
            Err(e) => last_refusal = Some(e.to_string()),
        }
    }
    if let Some(r) =
        last_refusal.filter(|_| h.design_flow_m3_s <= 0.0 || !h.design_flow_m3_s.is_finite())
    {
        return Err(CulvertError::Hydraulics(r));
    }
    Err(CulvertError::Invalid(format!(
        "no catalogue pipe of {cells} cell(s) passes {} m3/s within a headwater of {} m: add cells or allow a higher headwater",
        h.design_flow_m3_s, h.allowable_headwater_m
    )))
}

/// TODO hook: box culvert hydraulics are not in `afrobim-drainage` (its culvert check covers
/// circular barrels), and this crate does not invent them. Choose the box by hand.
pub fn select_box(_input: &CulvertInput, _h: &HydraulicsInput) -> Result<BoxSpec, CulvertError> {
    Err(CulvertError::NotImplemented("box culvert sizing: afrobim-drainage has no box hydraulics yet; enter the span and height by hand".to_string()))
}
