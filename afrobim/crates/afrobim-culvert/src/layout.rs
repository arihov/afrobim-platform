// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Derived geometry: barrel length from the road section, invert levels, end-structure extents
//! and the design checks. Everything here is a function of [`CulvertInput`]; nothing is stored.
//!
//! Axis frame: `s` is the distance along the culvert axis from the point where it crosses the road
//! centreline, mm, positive towards the right-hand end. A road offset is `s * sin(skew)`.
//! At each end `o` is the distance outwards from the outer face of the headwall and `t` is across
//! the culvert, mm.

use serde::{Deserialize, Serialize};

use crate::error::{invalid, CulvertError};
use crate::model::{CulvertInput, EndStructure, InvertSpec, LengthRule};
use crate::road::{Ogl, RoadSection, Side, Toe};

/// One design check and its outcome. A failed check does not stop the design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub value: f64,
    pub limit: f64,
    pub unit: String,
    pub passed: bool,
    pub message: String,
}

/// Derived dimensions of one end structure, mm unless stated.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EndLayout {
    pub side: Side,
    pub spec: EndStructure,
    pub headwall_width_mm: f64,
    /// Barrel invert at the outer face of this headwall, m.
    pub invert_m: f64,
    pub apron_top_m: f64,
    pub headwall_top_m: f64,
    pub headwall_base_m: f64,
    pub splay_rad: f64,
    /// Distance along the culvert axis that the water face of the wing reaches, mm.
    pub wing_reach_mm: f64,
    /// Distance across the culvert that the water face of the wing reaches, mm.
    pub wing_spread_mm: f64,
    /// Distance outwards of the outer corner of the wing end, mm.
    pub wing_outer_corner_o_mm: f64,
    /// Half width across the outer corners of the wing ends, mm.
    pub wing_outer_corner_t_mm: f64,
    /// Width across the outer corners of the wings, mm (6753 in the reference).
    pub overall_width_mm: f64,
    /// Apron plus toe beam, mm (2310 in the reference).
    pub apron_and_toe_mm: f64,
    /// Width of the apron at its end, between the water faces of the wings, mm.
    pub apron_end_width_mm: f64,
    /// Depth of the toe beam below the top of the apron, mm.
    pub toe_total_depth_mm: f64,
    /// Depth of the headwall and wings below the top of the apron, mm.
    pub below_apron_mm: f64,
}

/// The derived geometry of a culvert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub skew_deg: f64,
    /// Barrel length from the centreline to the outer face of the left headwall, mm.
    pub left_len_mm: f64,
    pub right_len_mm: f64,
    /// Total barrel length between the outer faces of the headwalls, mm.
    pub barrel_len_mm: f64,
    /// +1 when the water enters at the left end and falls towards +s, else -1.
    pub flow_dir: f64,
    /// Barrel gradient, m/m, falling from inlet to outlet.
    pub gradient: f64,
    pub invert_cl_m: f64,
    pub invert_left_m: f64,
    pub invert_right_m: f64,
    pub frl_cl_m: f64,
    pub ogl_cl_m: f64,
    /// Width of the headwalls (the same at both ends only when the inputs are), mm.
    pub left: EndLayout,
    pub right: EndLayout,
    /// Width of the surround or of the bedding, mm.
    pub surround_width_mm: f64,
    /// Cover from the top of the barrel to the finished road at the centreline, mm.
    pub cover_cl_mm: f64,
    /// Smallest cover under the road surface along the barrel, mm.
    pub min_cover_mm: f64,
    pub toe_left: Toe,
    pub toe_right: Toe,
    pub checks: Vec<Check>,
}

impl EndLayout {
    fn new(
        spec: &EndStructure,
        barrel_width_mm: f64,
        surround_side_mm: f64,
        side: Side,
        invert_m: f64,
    ) -> EndLayout {
        let hw = spec
            .headwall_width_mm
            .unwrap_or(barrel_width_mm + 2.0 * surround_side_mm + spec.headwall_extra_width_mm);
        let phi = spec.wing.splay_deg.to_radians();
        let (sin, cos) = (phi.sin(), phi.cos());
        let reach = spec.wing.length_mm * cos;
        let spread = spec.wing.length_mm * sin;
        let apron_top = invert_m - spec.apron_top_below_invert_mm / 1000.0;
        let below = spec.headwall_height_mm - spec.headwall_above_apron_mm;
        EndLayout {
            side,
            spec: *spec,
            headwall_width_mm: hw,
            invert_m,
            apron_top_m: apron_top,
            headwall_top_m: apron_top + spec.headwall_above_apron_mm / 1000.0,
            headwall_base_m: apron_top - below / 1000.0,
            splay_rad: phi,
            wing_reach_mm: reach,
            wing_spread_mm: spread,
            wing_outer_corner_o_mm: reach - spec.wing.thickness_mm * sin,
            wing_outer_corner_t_mm: hw / 2.0 + spread + spec.wing.thickness_mm * cos,
            overall_width_mm: hw + 2.0 * (spread + spec.wing.thickness_mm * cos),
            apron_and_toe_mm: spec.apron.length_mm + spec.toe.width_mm,
            apron_end_width_mm: hw + 2.0 * spec.apron.length_mm * phi.tan(),
            toe_total_depth_mm: spec.apron.thickness_mm + spec.toe.depth_mm,
            below_apron_mm: below,
        }
    }

    /// Wing height at distance `u` along the water face from the headwall, mm.
    pub fn wing_height_mm(&self, u: f64) -> f64 {
        let w = &self.spec.wing;
        let f = (u / w.length_mm).clamp(0.0, 1.0);
        w.height_at_headwall_mm + (w.height_at_end_mm - w.height_at_headwall_mm) * f
    }

    /// Width of the apron between the water faces of the wings at `o` from the headwall, mm.
    pub fn apron_width_at_mm(&self, o: f64) -> f64 {
        self.headwall_width_mm + 2.0 * o * self.splay_rad.tan()
    }
}

/// Where the embankment side slope meets the level `top_m`, as a road offset (m, signed), for the
/// barrel end on `side`.
fn slope_offset_m(section: &RoadSection, side: Side, top_m: f64) -> Result<f64, CulvertError> {
    let e = section.edge(side);
    let name = if side == Side::Left { "left" } else { "right" };
    if top_m > e.level_m + 1e-9 {
        return invalid(format!(
            "the top of the {name} headwall ({top_m:.3} m) is above the {name} road edge ({:.3} m): the barrel would end inside the road. Lower the headwall, lower the inverts or give the barrel length",
            e.level_m
        ));
    }
    let d = section.embankment_slope_n * (e.level_m - top_m);
    Ok(if side == Side::Left {
        e.offset_m - d
    } else {
        e.offset_m + d
    })
}

/// Invert at the left end, at the right end, the gradient (m/m) and the invert at the centreline
/// for the given end lengths.
fn inverts_for(
    spec: &InvertSpec,
    inlet_left: bool,
    flow_dir: f64,
    left_len: f64,
    right_len: f64,
) -> (f64, f64, f64, f64) {
    match *spec {
        InvertSpec::Ends { inlet_m, outlet_m } => {
            let (l, r) = if inlet_left {
                (inlet_m, outlet_m)
            } else {
                (outlet_m, inlet_m)
            };
            let total = left_len + right_len;
            let g = if total > 0.0 {
                (inlet_m - outlet_m) / (total / 1000.0)
            } else {
                0.0
            };
            // invert(s) = cl - flow_dir * g * s, so at the left end s = -left_len.
            (l, r, g, l - flow_dir * g * left_len / 1000.0)
        }
        InvertSpec::CentreGradient {
            centre_m,
            gradient_pct,
        } => {
            let g = gradient_pct / 100.0;
            (
                centre_m + flow_dir * g * left_len / 1000.0,
                centre_m - flow_dir * g * right_len / 1000.0,
                g,
                centre_m,
            )
        }
    }
}

/// Derives the layout. Refuses inputs that cannot be built.
pub fn derive(input: &CulvertInput) -> Result<Layout, CulvertError> {
    input.validate()?;
    let section = input.road.section()?;
    let ogl = Ogl::from_points(&input.ogl)?;
    let sin_skew = input.skew_deg.to_radians().sin();
    let frl_cl = section.frl_m();
    let ogl_cl = ogl.level_at(0.0);
    if ogl_cl > frl_cl {
        return invalid(format!("the original ground at the centreline ({ogl_cl:.3} m) is above the finished road level ({frl_cl:.3} m): a culvert in cut is not handled, check the levels"));
    }
    let width = input.barrel.outer_width_mm();
    let inlet_left = input.inlet_side == Side::Left;
    let flow_dir = if inlet_left { 1.0 } else { -1.0 };

    // Invert at each barrel end as a function of the two end lengths.
    let lengths_fixed: Option<(f64, f64)> = match input.length {
        LengthRule::FromEmbankment => None,
        LengthRule::Fixed { total_mm } => Some((total_mm / 2.0, total_mm / 2.0)),
        LengthRule::LeftRight { left_mm, right_mm } => Some((left_mm, right_mm)),
    };
    let end_height_m =
        |e: &EndStructure| (e.headwall_above_apron_mm - e.apron_top_below_invert_mm) / 1000.0;
    let (mut left_len, mut right_len) = lengths_fixed.unwrap_or((0.0, 0.0));
    let mut iterations = 0;
    let (inv_l, inv_r, gradient, inv_cl) = loop {
        let inv = inverts_for(&input.inverts, inlet_left, flow_dir, left_len, right_len);
        if lengths_fixed.is_some() {
            break inv;
        }
        // Lengths from the embankment slope at the headwall tops.
        let top_l = inv.0 + end_height_m(&input.left_end);
        let top_r = inv.1 + end_height_m(&input.right_end);
        let x_l = slope_offset_m(&section, Side::Left, top_l)?;
        let x_r = slope_offset_m(&section, Side::Right, top_r)?;
        let (new_l, new_r) = (-x_l * 1000.0 / sin_skew, x_r * 1000.0 / sin_skew);
        let moved = (new_l - left_len).abs().max((new_r - right_len).abs());
        left_len = new_l;
        right_len = new_r;
        iterations += 1;
        // Inverts given at the ends do not depend on the lengths, so one pass settles them.
        if moved < 1e-6 || (matches!(input.inverts, InvertSpec::Ends { .. }) && iterations >= 2) {
            break inverts_for(&input.inverts, inlet_left, flow_dir, left_len, right_len);
        }
        if iterations > 200 {
            return invalid("the barrel length did not settle: check the inverts, the side slope and the headwall height");
        }
    };
    if !(left_len > 0.0 && right_len > 0.0) {
        return invalid(format!("the barrel lengths from the centreline must both be greater than zero (left {left_len:.0}, right {right_len:.0} mm)"));
    }

    let left = EndLayout::new(
        &input.left_end,
        width,
        input.bedding.surround_side_mm,
        Side::Left,
        inv_l,
    );
    let right = EndLayout::new(
        &input.right_end,
        width,
        input.bedding.surround_side_mm,
        Side::Right,
        inv_r,
    );
    if left_len + right_len < left.apron_and_toe_mm + right.apron_and_toe_mm {
        return invalid(format!(
            "the barrel is {:.0} mm long but the two end structures need {:.0} mm: they would overlap",
            left_len + right_len,
            left.apron_and_toe_mm + right.apron_and_toe_mm
        ));
    }

    for e in [&left, &right] {
        let crown = e.invert_m + input.barrel.crown_above_invert_mm() / 1000.0;
        if e.headwall_top_m < crown {
            return invalid(format!(
                "the {} headwall top ({:.3} m) is below the top of the barrel ({crown:.3} m): make the headwall higher or lower the apron",
                if e.side == Side::Left { "left" } else { "right" },
                e.headwall_top_m
            ));
        }
    }
    let crown_cl = inv_cl + input.barrel.crown_above_invert_mm() / 1000.0;
    let cover_cl_mm = (frl_cl - crown_cl) * 1000.0;
    if cover_cl_mm < 0.0 {
        return invalid(format!(
            "the cover over the barrel at the road centreline is negative ({cover_cl_mm:.0} mm): the top of the barrel ({crown_cl:.3} m) is above the finished road ({frl_cl:.3} m). Lower the inverts"
        ));
    }
    // Smallest cover under the road surface: at every section point inside the barrel.
    let mut min_cover = cover_cl_mm;
    for p in &section.points {
        let s = p.offset_m * 1000.0 / sin_skew;
        if s >= -left_len && s <= right_len {
            let invert = inv_cl - flow_dir * gradient * s / 1000.0;
            let cover =
                (p.level_m - invert - input.barrel.crown_above_invert_mm() / 1000.0) * 1000.0;
            min_cover = min_cover.min(cover);
        }
    }

    let mut checks = Vec::new();
    let gpct = gradient * 100.0;
    checks.push(Check {
        name: "minimum cover".to_string(),
        value: min_cover,
        limit: input.thresholds.min_cover_mm,
        unit: "mm".to_string(),
        passed: min_cover >= input.thresholds.min_cover_mm,
        message: format!(
            "the smallest cover under the road is {min_cover:.0} mm against a minimum of {:.0} mm",
            input.thresholds.min_cover_mm
        ),
    });
    checks.push(Check {
        name: "self-cleansing gradient".to_string(),
        value: gpct,
        limit: input.thresholds.min_gradient_pct,
        unit: "%".to_string(),
        passed: gpct + 1e-9 >= input.thresholds.min_gradient_pct,
        message: format!(
            "the barrel gradient is {gpct:.2} % against a minimum of {:.2} %",
            input.thresholds.min_gradient_pct
        ),
    });

    Ok(Layout {
        skew_deg: input.skew_deg,
        left_len_mm: left_len,
        right_len_mm: right_len,
        barrel_len_mm: left_len + right_len,
        flow_dir,
        gradient,
        invert_cl_m: inv_cl,
        invert_left_m: inv_l,
        invert_right_m: inv_r,
        frl_cl_m: frl_cl,
        ogl_cl_m: ogl_cl,
        left,
        right,
        surround_width_mm: width + 2.0 * input.bedding.surround_side_mm,
        cover_cl_mm,
        min_cover_mm: min_cover,
        toe_left: section.toe(&ogl, Side::Left)?,
        toe_right: section.toe(&ogl, Side::Right)?,
        checks,
    })
}

impl Layout {
    /// Barrel invert at axis position `s_mm`, m.
    pub fn invert_at_m(&self, s_mm: f64) -> f64 {
        self.invert_cl_m - self.flow_dir * self.gradient * s_mm / 1000.0
    }

    /// The end layout on a side.
    pub fn end(&self, side: Side) -> &EndLayout {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }
}
