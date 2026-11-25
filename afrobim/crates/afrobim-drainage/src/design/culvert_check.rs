// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The complete pipe culvert check under the MoWT basis: inlet control, outlet control, the
//! greater of the two, and the limits.
//!
//! * **Outlet control** is [`super::mowt::outlet_control`], unchanged (DDM Section 9.4.2 and
//!   Section 9.5.1 Step 8).
//! * **Inlet control** is [`super::inlet::inlet_control`] (FHWA HDS-5 Appendix A equations in SI;
//!   DDM gives inlet control only as nomographs, see that module for the reasoning and the
//!   transition zone).
//! * **Governing headwater** is "the higher of HWi and HWo" (DDM Section 9.5.1 Step 9: "HWc = HWi
//!   > HWoi"; HDS-5 Design Guideline 1, Step 5). Both are depths above the inlet invert.
//! * **Limits**: the allowable headwater is the engineer's input. DDM's own limits are applied to
//!   the governing headwater: `HW/D <= 1.5` (Section 9.3, with 1.2 recommended) and, when the
//!   shoulder break point level and road class are given, the freeboard of Section 9.6.
//!
//! The first lines of [`CulvertCheck::calculation`] are the lines [`OutletControl::calculation`]
//! prints today, unchanged. Everything this module adds follows them, in
//! [`CulvertCheck::additional_lines`], so that an app which already prints the outlet control
//! calculation can append these lines and leave the rest as it is.

use super::inlet::{inlet_control, InletControl, InletEdge, InletGeometry, InletInput};
use super::mowt::{outlet_control, OutletControl, OutletControlInput, CULVERT_FREEBOARD_MAJOR_ROAD_M, CULVERT_FREEBOARD_MINOR_ROAD_M, HEADWATER_RATIO_LIMIT, HEADWATER_RATIO_RECOMMENDED};
use crate::{positive, DrainageError};

/// Which control governs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Inlet,
    Outlet,
}

impl Control {
    pub fn label(self) -> &'static str {
        match self {
            Control::Inlet => "inlet control",
            Control::Outlet => "outlet control",
        }
    }
}

/// The inputs of the DDM Section 9.6 freeboard check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreeboardInput {
    /// Level of the shoulder break point at the low point of the road, m above datum.
    pub shoulder_break_level_m: f64,
    /// Level of the inlet invert, m above datum.
    pub inlet_invert_level_m: f64,
    /// True for a major road (250 mm freeboard), false for a minor road (100 mm), DDM Section 9.6.
    pub major_road: bool,
}

/// What [`culvert_check`] needs.
#[derive(Debug, Clone, PartialEq)]
pub struct CulvertCheckInput {
    /// The outlet control inputs: flow, barrels, diameter, length, fall, n, Ke, tailwater.
    pub outlet: OutletControlInput,
    /// The inlet configuration, a circular row of HDS-5 Table A.1.
    pub inlet_edge: InletEdge,
    /// The largest headwater depth above the inlet invert the engineer will accept, m.
    pub allowable_headwater_m: f64,
    pub freeboard: Option<FreeboardInput>,
}

/// The result of [`culvert_check`].
#[derive(Debug, Clone, PartialEq)]
pub struct CulvertCheck {
    pub input: CulvertCheckInput,
    pub inlet: InletControl,
    pub outlet: OutletControl,
    pub governing: Control,
    /// The greater of the two headwaters, above the inlet invert, m.
    pub headwater_m: f64,
    /// Governing headwater over the diameter.
    pub headwater_ratio: f64,
    pub within_allowable: bool,
    /// HW/D within the limit of DDM Section 9.3.
    pub within_ratio_limit: bool,
    /// Freeboard of DDM Section 9.6 and whether it is met, when asked for.
    pub freeboard: Option<(f64, bool)>,
}

/// Inlet control, outlet control, the greater of the two with the controlling condition named,
/// and the limits. See the module documentation.
pub fn culvert_check(input: &CulvertCheckInput) -> Result<CulvertCheck, DrainageError> {
    if !positive(input.allowable_headwater_m) {
        return Err(DrainageError::InvalidInput(format!("the allowable headwater must be greater than zero (got {})", input.allowable_headwater_m)));
    }
    let o = &input.outlet;
    let outlet = outlet_control(o)?;
    let inlet = inlet_control(&InletInput {
        geometry: InletGeometry::Circular { diameter_m: o.diameter_m },
        edge: input.inlet_edge,
        flow_m3_s: outlet.flow_per_barrel_m3_s,
        slope_m_per_m: o.fall_m / o.length_m,
        length_m: o.length_m,
    })?;
    let governing = if inlet.headwater_m >= outlet.headwater_m { Control::Inlet } else { Control::Outlet };
    let headwater_m = inlet.headwater_m.max(outlet.headwater_m);
    let headwater_ratio = headwater_m / o.diameter_m;
    let freeboard = input.freeboard.map(|f| {
        let free = f.shoulder_break_level_m - (f.inlet_invert_level_m + headwater_m);
        let need = if f.major_road { CULVERT_FREEBOARD_MAJOR_ROAD_M } else { CULVERT_FREEBOARD_MINOR_ROAD_M };
        (free, free >= need)
    });
    Ok(CulvertCheck {
        input: input.clone(),
        inlet,
        outlet,
        governing,
        headwater_m,
        headwater_ratio,
        within_allowable: headwater_m <= input.allowable_headwater_m,
        within_ratio_limit: headwater_ratio <= HEADWATER_RATIO_LIMIT,
        freeboard,
    })
}

fn ok(pass: bool) -> &'static str {
    if pass {
        "OK"
    } else {
        "FAILS"
    }
}

impl CulvertCheck {
    /// True when the governing headwater is within the allowable headwater and within DDM's
    /// `HW/D` limit, and the freeboard is met where it was asked for.
    pub fn passes(&self) -> bool {
        self.within_allowable && self.within_ratio_limit && self.freeboard.map_or(true, |(_, pass)| pass)
    }

    /// The lines of the outlet control calculation exactly as [`OutletControl::calculation`] prints them.
    pub fn outlet_lines(&self) -> Vec<String> {
        self.outlet.calculation()
    }

    /// The lines this module adds: a note on the outlet control warning, the inlet control
    /// calculation, the greater-of rule with the controlling condition, and the limits.
    pub fn additional_lines(&self) -> Vec<String> {
        let d = self.input.outlet.diameter_m;
        let mut out = vec![
            "Note: the line above that says inlet control is not computed refers to the outlet control calculation alone. Inlet control is computed below (FHWA HDS-5 equations; DDM Section 9.4.1 gives it only as nomographs).".to_string(),
        ];
        out.extend(self.inlet.calculation());
        out.push(format!(
            "Controlling headwater (DDM Section 9.5.1 Step 9, the higher of the two): HWi = {:.3} m (inlet), HWoi = {:.3} m (outlet); HWc = {:.3} m, {} governs; HWc/D = {:.2}.",
            self.inlet.headwater_m, self.outlet.headwater_m, self.headwater_m, self.governing.label(), self.headwater_ratio
        ));
        out.push(format!(
            "Allowable headwater {:.3} m (entered): HWc = {:.3} m: {}.",
            self.input.allowable_headwater_m,
            self.headwater_m,
            ok(self.within_allowable)
        ));
        out.push(format!("DDM Section 9.3: HW/D should not exceed {HEADWATER_RATIO_LIMIT}: HWc/D = {:.2}: {}.", self.headwater_ratio, ok(self.within_ratio_limit)));
        if self.within_ratio_limit && self.headwater_ratio > HEADWATER_RATIO_RECOMMENDED {
            out.push(format!("Check: HWc/D = {:.2} exceeds {HEADWATER_RATIO_RECOMMENDED}, the ratio DDM Section 9.3 recommends where the flooding effect of the headwater cannot be predicted.", self.headwater_ratio));
        }
        if let (Some(f), Some((free, pass))) = (self.input.freeboard, self.freeboard) {
            let need = if f.major_road { CULVERT_FREEBOARD_MAJOR_ROAD_M } else { CULVERT_FREEBOARD_MINOR_ROAD_M };
            out.push(format!(
                "Freeboard (DDM Section 9.6) = shoulder break level {:.3} m less headwater level {:.3} m = {:.3} m against {need} m for a {} road: {}.",
                f.shoulder_break_level_m,
                f.inlet_invert_level_m + self.headwater_m,
                free,
                if f.major_road { "major" } else { "minor" },
                ok(pass)
            ));
        }
        out.push(format!("D = {d:.3} m; inlet and outlet control headwaters are both measured above the inlet invert."));
        out
    }

    /// The outlet control calculation as it stands, then [`CulvertCheck::additional_lines`].
    pub fn calculation(&self) -> Vec<String> {
        let mut out = self.outlet_lines();
        out.extend(self.additional_lines());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!((actual - expected).abs() <= tolerance, "expected {expected} ± {tolerance}, got {actual}");
    }

    fn input(flow: f64, fall: f64, allowable: f64) -> CulvertCheckInput {
        CulvertCheckInput {
            outlet: OutletControlInput { design_flow_m3_s: flow, barrels: 1, diameter_m: 1.2, length_m: 20.0, fall_m: fall, manning_n: 0.013, entrance_loss_ke: 0.5, tailwater_m: 0.3 },
            inlet_edge: InletEdge::ConcreteSquareEdgeHeadwall,
            allowable_headwater_m: allowable,
            freeboard: None,
        }
    }

    /// Steep barrel: inlet control governs. D = 1.2 m, Q = 3.5 m³/s, S = 0.1 (fall 2 m over 20 m).
    /// Inlet: x = 2.8251, submerged, HW/D = 0.0398 × 26.176 + 0.67 - 0.05 = 1.6618, HWi = 1.994 m.
    /// Outlet: H + ho - fall with a 2 m fall is far below that.
    #[test]
    fn steep_culvert_is_inlet_controlled() {
        let c = culvert_check(&input(3.5, 2.0, 2.5)).unwrap();
        assert_eq!(c.governing, Control::Inlet);
        close(c.inlet.headwater_m, 1.994, 0.001);
        assert!(c.outlet.headwater_m < c.inlet.headwater_m);
        close(c.headwater_m, c.inlet.headwater_m, 0.0);
        assert!(c.within_allowable);
        close(c.headwater_ratio, 1.6618, 0.001);
        assert!(!c.within_ratio_limit); // 1.66 > 1.5 (DDM Section 9.3)
        assert!(!c.passes());
    }

    /// Flat barrel with a high tailwater: outlet control governs and the headwater is the greater.
    #[test]
    fn flat_culvert_with_high_tailwater_is_outlet_controlled() {
        let mut i = input(1.0, 0.02, 3.0);
        i.outlet.tailwater_m = 1.5;
        let c = culvert_check(&i).unwrap();
        assert_eq!(c.governing, Control::Outlet);
        assert!(c.outlet.headwater_m > c.inlet.headwater_m);
        close(c.headwater_m, c.outlet.headwater_m, 0.0);
        let text = c.calculation().join("\n");
        assert!(text.contains("outlet control governs"), "{text}");
    }

    /// The existing lines come first and unchanged; the new lines follow.
    #[test]
    fn existing_lines_are_unchanged() {
        let c = culvert_check(&input(3.5, 2.0, 2.5)).unwrap();
        let all = c.calculation();
        let outlet = c.outlet.calculation();
        assert_eq!(&all[..outlet.len()], &outlet[..]);
        assert_eq!(all.len(), outlet.len() + c.additional_lines().len());
        assert!(all[outlet.len()].starts_with("Note: the line above that says inlet control is not computed"));
        assert!(all.iter().any(|l| l.contains("Inlet control, FHWA HDS-5")));
    }

    #[test]
    fn allowable_headwater_and_freeboard_are_checked() {
        let tight = culvert_check(&input(3.5, 2.0, 1.5)).unwrap();
        assert!(!tight.within_allowable);
        let mut i = input(0.6, 0.2, 2.0);
        i.freeboard = Some(FreeboardInput { shoulder_break_level_m: 101.0, inlet_invert_level_m: 100.0, major_road: true });
        let c = culvert_check(&i).unwrap();
        let (free, pass) = c.freeboard.unwrap();
        close(free, 1.0 - c.headwater_m, 1e-9);
        assert_eq!(pass, free >= 0.25);
        assert!(c.additional_lines().iter().any(|l| l.contains("Freeboard (DDM Section 9.6)")));
        assert!(culvert_check(&input(1.0, 0.1, 0.0)).is_err());
    }

    #[test]
    fn refuses_outside_the_tabulated_range() {
        // A very small barrel for the flow: HW/D beyond the chart.
        let mut i = input(4.0, 0.2, 5.0);
        i.outlet.diameter_m = 0.6;
        assert!(culvert_check(&i).unwrap_err().to_string().contains("highest graduation"));
        // A box inlet on a pipe culvert.
        let mut b = input(1.0, 0.2, 5.0);
        b.inlet_edge = InletEdge::BoxHeadwallBevel45;
        assert!(culvert_check(&b).is_err());
    }
}
