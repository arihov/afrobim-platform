// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Hydrology and hydraulics for side drains and pipe culverts, by design basis.
//!
//! # Structure
//!
//! * [`hydraulics`] (re-exported here): physics that belongs to no basis. Section geometry,
//!   Manning's equation, normal depth, Froude number, critical depth. Every coefficient is an
//!   argument.
//! * [`DesignBasis`]: the governing documents the engineer has chosen. It supplies the citations
//!   of the shared open-channel check ([`check_channel`]) and the culvert layout limits
//!   ([`DesignBasis::culvert_layout_limits`]) the culvert module raises notes against.
//! * [`mowt`]: Uganda MoWT *Road Design Manual Volume 2: Drainage Design* (January 2010):
//!   design recurrence intervals, the Rational method, time of concentration, permissible
//!   velocities, culvert outlet control.
//! * [`dmrb`]: UK *Design Manual for Roads and Bridges*: CG 501, CD 521, CD 522 and CD 529.
//!   Design return periods, Manning's n for surface water channels, the channel flow equations
//!   of CD 521, culvert size rules.
//!
//! **Which basis governs a project is the engineer's choice.** Nothing here picks one from a
//! country or a client, and nothing mixes them silently: a calculation names its basis on its
//! first line and cites only that basis' documents (the shared physics is cited to the chosen
//! basis where it states the relation, and as plain physics where it does not).
//!
//! Every calculation is a `Vec<String>`, one statement per line with its reference, whichever
//! basis produced it.
//!
//! # Adding a basis
//!
//! A further basis (the SANRAL *Drainage Manual* for South Africa is the expected next one) is
//! a new sibling module with its own tables and methods, a new [`DesignBasis`] variant, and one
//! arm in each of that enum's methods. The element definitions, the shared hydraulics and
//! [`check_channel`] need no change. No South African value is held here yet.
//!
//! The result of anything in this module is a calculation for an engineer to check, not a
//! certified design.

pub mod culvert_check;
pub mod dmrb;
pub mod hydraulics;
pub mod inlet;
pub mod mowt;
pub mod sanral;

pub use hydraulics::{circular_critical_depth_m, froude_number, manning_velocity_m_s, normal_depth_m, ChannelGeometry, GRAVITY_M_S2};

use serde::{Deserialize, Serialize};

use crate::{positive, DrainageError};

/// The governing documents of a calculation or a set of layout checks. The engineer chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DesignBasis {
    /// Uganda Ministry of Works and Transport, *Road Design Manual Volume 2: Drainage Design*,
    /// January 2010. See [`mowt`].
    Mowt,
    /// UK *Design Manual for Roads and Bridges*: CG 501 version 2.2.0, CD 521 version 1.2.0,
    /// CD 522 version 0.1.0 and CD 529 version 1.1.0. See [`dmrb`].
    Dmrb,
    /// South African National Roads Agency (SANRAL) *Drainage Manual*, sixth edition, September
    /// 2013. See [`sanral`].
    Sanral,
}

/// A limit a basis states, with where it states it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limit {
    pub value: f64,
    /// The clause and its wording, ready to quote in a note.
    pub citation: &'static str,
}

/// A rule that culverts longer than a length need a larger diameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LongCulvertRule {
    pub longer_than_m: f64,
    pub minimum_diameter_m: f64,
    pub citation: &'static str,
}

/// What a basis says about the layout of a pipe culvert, each item with its citation. `None`
/// means the documents held for that basis state no such limit; it does not mean there is none
/// in documents this crate does not hold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CulvertLayoutLimits {
    /// Smallest internal diameter, m.
    pub minimum_diameter_m: Option<Limit>,
    pub long_culvert: Option<LongCulvertRule>,
    /// Largest skew from the perpendicular to the centreline, degrees.
    pub maximum_skew_deg: Option<Limit>,
    /// Gradient below which the culvert is not to be used, m/m.
    pub gradient_not_below: Option<Limit>,
    /// Gradient below which the culvert is to be avoided if possible, m/m.
    pub gradient_avoid_below: Option<Limit>,
    /// Gradient above which anchors are called for, m/m.
    pub gradient_anchors_above: Option<Limit>,
    /// Gradient above which special anchoring is called for, m/m.
    pub gradient_special_anchors_above: Option<Limit>,
    /// Internal diameter above which the culvert is classed as a structure, m.
    pub structure_above_diameter_m: Option<Limit>,
}

impl DesignBasis {
    /// The two bases held before the SANRAL basis was added, in menu order. Kept unchanged so that
    /// existing code that counts or maps over it still compiles; use [`DesignBasis::EVERY`] for
    /// all three.
    pub const ALL: [DesignBasis; 2] = [DesignBasis::Mowt, DesignBasis::Dmrb];

    /// Every basis held, including [`DesignBasis::Sanral`].
    pub const EVERY: [DesignBasis; 3] = [DesignBasis::Mowt, DesignBasis::Dmrb, DesignBasis::Sanral];

    /// A short name for a menu.
    pub fn name(self) -> &'static str {
        match self {
            DesignBasis::Mowt => "MoWT Uganda (2010)",
            DesignBasis::Dmrb => "UK DMRB",
            DesignBasis::Sanral => "SANRAL South Africa (6th ed. 2013)",
        }
    }

    /// The first line of every calculation to this basis.
    pub fn basis_line(self) -> &'static str {
        match self {
            DesignBasis::Mowt => mowt::BASIS,
            DesignBasis::Dmrb => dmrb::BASIS,
            DesignBasis::Sanral => sanral::BASIS,
        }
    }

    /// Where the basis states Manning's equation.
    pub fn manning_reference(self) -> &'static str {
        match self {
            DesignBasis::Mowt => "DDM Example 4-1, Section 9.5.1 Step 12, Appendix 10.4",
            DesignBasis::Dmrb => "CD 521 Equation 5.25",
            DesignBasis::Sanral => "SDM Equation 4.27 (p. 4-14)",
        }
    }

    /// Where the basis states the Froude number, or that it does not.
    pub fn froude_reference(self) -> &'static str {
        match self {
            DesignBasis::Mowt => "DDM Appendix 10.4 Col. 11, on the hydraulic depth",
            DesignBasis::Dmrb => "general hydraulics; not a requirement of the DMRB documents held",
            DesignBasis::Sanral => "SDM Equation 5.5 (p. 5-22), Fr = (Q²B/gA³)^0,5, the same on the hydraulic depth",
        }
    }

    /// What the basis says about the freeboard of an open drain.
    pub fn open_drain_freeboard_statement(self) -> &'static str {
        match self {
            DesignBasis::Mowt => "DDM states no freeboard for open drains",
            DesignBasis::Dmrb => "the DMRB documents held (CG 501, CD 521, CD 522, CD 529) state no freeboard for ditches; ditch design is in CD 532, which is not held",
            DesignBasis::Sanral => "SDM Table 5.3 (p. 5-22) gives a minimum freeboard (0,15E to 0,30y by section and Froude number); enter it, or use sanral::check_channel_sanral which computes it",
        }
    }

    /// The layout limits the basis states for pipe culverts.
    pub fn culvert_layout_limits(self) -> CulvertLayoutLimits {
        match self {
            DesignBasis::Mowt => mowt::culvert_layout_limits(),
            DesignBasis::Dmrb => dmrb::culvert_layout_limits(),
            DesignBasis::Sanral => sanral::culvert_layout_limits(),
        }
    }
}

fn ok(pass: bool) -> &'static str {
    if pass {
        "OK"
    } else {
        "FAILS"
    }
}

fn invalid<T>(message: impl Into<String>) -> Result<T, DrainageError> {
    Err(DrainageError::InvalidInput(message.into()))
}

/// What the open-channel check needs.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelInput {
    /// The basis the calculation is cited to.
    pub basis: DesignBasis,
    pub geometry: ChannelGeometry,
    /// Depth of the drain from its top edges to the bed, m.
    pub depth_m: f64,
    /// Manning's n. An input under every basis; [`mowt::ChannelMaterial`] and
    /// [`dmrb::ChannelSurface`] say what each basis offers.
    pub manning_n: f64,
    /// Where n comes from, quoted in the calculation.
    pub roughness_source: String,
    /// Longitudinal gradient of the bed, m/m.
    pub gradient_m_per_m: f64,
    pub design_flow_m3_s: f64,
    /// Maximum permissible velocity, m/s, e.g. [`mowt::ChannelMaterial::permissible_velocity_m_s`].
    /// None leaves the velocity unchecked.
    pub permissible_velocity_m_s: Option<f64>,
    /// Where the permissible velocity comes from, quoted in the calculation.
    pub permissible_source: String,
    /// Required freeboard, m. Neither basis states one for open drains, so None leaves the
    /// freeboard reported but unchecked.
    pub required_freeboard_m: Option<f64>,
}

/// The result of [`check_channel`]. Section properties are at the normal depth.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelCheck {
    pub input: ChannelInput,
    /// Depth of uniform flow at the design flow, m (the section continued upward if it must be).
    pub normal_depth_m: f64,
    pub area_m2: f64,
    pub wetted_perimeter_m: f64,
    pub hydraulic_radius_m: f64,
    pub top_width_m: f64,
    pub velocity_m_s: f64,
    /// On the hydraulic depth A/T.
    pub froude: f64,
    /// Discharge with the drain flowing to its top edges, m³/s.
    pub full_capacity_m3_s: f64,
    pub full_velocity_m_s: f64,
    /// Drain depth less normal depth, m; negative when the drain overtops.
    pub freeboard_m: f64,
    /// The design flow is carried within the drain's depth.
    pub capacity_ok: bool,
    /// None when no permissible velocity was given.
    pub velocity_ok: Option<bool>,
    /// None when no required freeboard was given.
    pub freeboard_ok: Option<bool>,
}

/// Uniform flow in an open drain at the design flow: normal depth, velocity against the
/// permissible velocity, Froude number, capacity at full depth and freeboard. The arithmetic is
/// the same under every basis; the basis sets the citations.
///
/// Uniform flow is assumed along the whole drain: no backwater from a culvert or outfall, no
/// drawdown, no allowance for bends, silting or vegetation growth beyond the n entered.
pub fn check_channel(input: &ChannelInput) -> Result<ChannelCheck, DrainageError> {
    if !positive(input.depth_m) {
        return invalid(format!("the drain depth must be greater than zero (got {})", input.depth_m));
    }
    if !positive(input.design_flow_m3_s) {
        return invalid(format!("the design flow must be greater than zero (got {})", input.design_flow_m3_s));
    }
    if let Some(v) = input.permissible_velocity_m_s {
        if !positive(v) {
            return invalid(format!("the permissible velocity must be greater than zero (got {v})"));
        }
    }
    if let Some(f) = input.required_freeboard_m {
        if !(f.is_finite() && f >= 0.0) {
            return invalid(format!("the required freeboard must not be negative (got {f})"));
        }
    }
    let g = &input.geometry;
    let y = normal_depth_m(g, input.manning_n, input.gradient_m_per_m, input.design_flow_m3_s)?;
    let area_m2 = g.area_m2(y);
    let velocity_m_s = input.design_flow_m3_s / area_m2;
    let full_capacity_m3_s = g.discharge_m3_s(input.depth_m, input.manning_n, input.gradient_m_per_m);
    let freeboard_m = input.depth_m - y;
    Ok(ChannelCheck {
        input: input.clone(),
        normal_depth_m: y,
        area_m2,
        wetted_perimeter_m: g.wetted_perimeter_m(y),
        hydraulic_radius_m: g.hydraulic_radius_m(y),
        top_width_m: g.top_width_m(y),
        velocity_m_s,
        froude: froude_number(velocity_m_s, g.hydraulic_depth_m(y)),
        full_capacity_m3_s,
        full_velocity_m_s: full_capacity_m3_s / g.area_m2(input.depth_m),
        freeboard_m,
        capacity_ok: freeboard_m >= 0.0,
        velocity_ok: input.permissible_velocity_m_s.map(|limit| velocity_m_s <= limit),
        freeboard_ok: input.required_freeboard_m.map(|need| freeboard_m >= need),
    })
}

impl ChannelCheck {
    /// True when the flow is carried and every check that was asked for is met.
    pub fn passes(&self) -> bool {
        self.capacity_ok && self.velocity_ok.unwrap_or(true) && self.freeboard_ok.unwrap_or(true)
    }

    /// The calculation set out line by line with its references.
    pub fn calculation(&self) -> Vec<String> {
        let i = &self.input;
        let g = &i.geometry;
        let basis = i.basis;
        let mut out = vec![
            basis.basis_line().to_string(),
            format!(
                "Section: bed width b = {:.3} m, side slopes zl = {} and zr = {} (H:1V), depth d = {:.3} m.",
                g.bottom_width_m, g.left_side_slope, g.right_side_slope, i.depth_m
            ),
            format!("Manning's n = {} ({}); gradient S = {:.5} m/m; design flow Q = {:.3} m³/s.", i.manning_n, i.roughness_source, i.gradient_m_per_m, i.design_flow_m3_s),
            format!("Manning: Q = (1/n)·A·R^(2/3)·S^(1/2) ({}), uniform flow.", basis.manning_reference()),
            format!("Normal depth by bisection: y = {:.3} m.", self.normal_depth_m),
            format!(
                "A = y·(b + (zl + zr)·y/2) = {:.4} m²; P = b + y·(√(1+zl²) + √(1+zr²)) = {:.4} m; R = A/P = {:.4} m; T = b + (zl + zr)·y = {:.3} m.",
                self.area_m2, self.wetted_perimeter_m, self.hydraulic_radius_m, self.top_width_m
            ),
        ];
        let state = if self.froude > 1.0 {
            "supercritical"
        } else if self.froude < 1.0 {
            "subcritical"
        } else {
            "critical"
        };
        match (i.permissible_velocity_m_s, self.velocity_ok) {
            (Some(limit), Some(pass)) => out.push(format!(
                "V = Q/A = {:.2} m/s against a maximum permissible velocity of {limit} m/s ({}): {}.",
                self.velocity_m_s,
                i.permissible_source,
                ok(pass)
            )),
            _ => out.push(format!("V = Q/A = {:.2} m/s. No permissible velocity was given, so it is not checked.", self.velocity_m_s)),
        }
        out.push(format!("Froude number F = V/√(g·A/T) = {:.2} with g = 9.81 ({}): {state} flow.", self.froude, basis.froude_reference()));
        out.push(format!(
            "Capacity at full depth d = {:.3} m: Q = {:.3} m³/s at V = {:.2} m/s; design flow carried within the section: {}.",
            i.depth_m,
            self.full_capacity_m3_s,
            self.full_velocity_m_s,
            ok(self.capacity_ok)
        ));
        let statement = basis.open_drain_freeboard_statement();
        match (i.required_freeboard_m, self.freeboard_ok) {
            (Some(need), Some(pass)) => out.push(format!("Freeboard = d − y = {:.3} m against {need:.3} m required (entered; {statement}): {}.", self.freeboard_m, ok(pass))),
            _ => out.push(format!("Freeboard = d − y = {:.3} m. It is not checked: none was entered, and {statement}.", self.freeboard_m)),
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_basis_names_itself_and_cites_only_its_own_documents() {
        let input = |basis| ChannelInput {
            basis,
            geometry: ChannelGeometry::trapezoid(1.0, 1.5),
            depth_m: 0.6,
            manning_n: 0.025,
            roughness_source: "entered".into(),
            gradient_m_per_m: 0.004,
            design_flow_m3_s: 1.0,
            permissible_velocity_m_s: None,
            permissible_source: String::new(),
            required_freeboard_m: None,
        };
        let m = check_channel(&input(DesignBasis::Mowt)).unwrap();
        let d = check_channel(&input(DesignBasis::Dmrb)).unwrap();
        // The arithmetic does not depend on the basis.
        assert_eq!((m.normal_depth_m, m.velocity_m_s, m.froude, m.full_capacity_m3_s), (d.normal_depth_m, d.velocity_m_s, d.froude, d.full_capacity_m3_s));
        let (mt, dt) = (m.calculation().join("\n"), d.calculation().join("\n"));
        assert!(mt.starts_with("Design basis: MoWT Road Design Manual Vol. 2"));
        assert!(mt.contains("DDM Example 4-1") && !mt.contains("CD 521") && !mt.contains("DMRB"));
        assert!(dt.starts_with("Design basis: UK Design Manual for Roads and Bridges"));
        assert!(dt.contains("CD 521 Equation 5.25") && !dt.contains("DDM") && !dt.contains("MoWT"));
        assert!(dt.contains("CD 532, which is not held"));
        assert_eq!(m.calculation().len(), d.calculation().len());
        assert_eq!(DesignBasis::ALL.map(|b| b.name()), ["MoWT Uganda (2010)", "UK DMRB"]);
        // The basis is stored with an element by name.
        assert_eq!(serde_json::to_string(&DesignBasis::Dmrb).unwrap(), "\"Dmrb\"");
    }

    #[test]
    fn the_sanral_basis_cites_only_its_own_documents() {
        let input = ChannelInput {
            basis: DesignBasis::Sanral,
            geometry: ChannelGeometry::trapezoid(1.0, 1.5),
            depth_m: 0.6,
            manning_n: 0.025,
            roughness_source: "entered".into(),
            gradient_m_per_m: 0.004,
            design_flow_m3_s: 1.0,
            permissible_velocity_m_s: None,
            permissible_source: String::new(),
            required_freeboard_m: None,
        };
        let s = check_channel(&input).unwrap();
        let m = check_channel(&ChannelInput { basis: DesignBasis::Mowt, ..input.clone() }).unwrap();
        assert_eq!((s.normal_depth_m, s.velocity_m_s), (m.normal_depth_m, m.velocity_m_s));
        let t = s.calculation().join("\n");
        assert!(t.starts_with("Design basis: SANRAL Drainage Manual") && t.contains("SDM Equation 4.27") && t.contains("SDM Table 5.3"));
        assert!(!t.contains("DDM") && !t.contains("CD 521") && !t.contains("MoWT"));
        assert_eq!(serde_json::to_string(&DesignBasis::Sanral).unwrap(), "\"Sanral\"");
        assert_eq!(DesignBasis::EVERY.map(|b| b.name()), ["MoWT Uganda (2010)", "UK DMRB", "SANRAL South Africa (6th ed. 2013)"]);
    }

    #[test]
    fn culvert_layout_limits_differ_by_basis() {
        let m = DesignBasis::Mowt.culvert_layout_limits();
        let d = DesignBasis::Dmrb.culvert_layout_limits();
        assert_eq!(m.minimum_diameter_m.unwrap().value, 0.6);
        assert_eq!(d.minimum_diameter_m.unwrap().value, 0.45);
        assert!(m.long_culvert.is_none());
        let long = d.long_culvert.unwrap();
        assert_eq!((long.longer_than_m, long.minimum_diameter_m), (12.0, 1.2));
        assert_eq!(m.maximum_skew_deg.unwrap().value, 45.0);
        assert!(d.maximum_skew_deg.is_none() && d.gradient_not_below.is_none());
        assert_eq!(d.structure_above_diameter_m.unwrap().value, 0.9);
        assert!(m.structure_above_diameter_m.is_none());
    }
}
