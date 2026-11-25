// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Culvert inlet control by the equations of FHWA HDS-5, in SI.
//!
//! # Sources
//!
//! * **FHWA HDS-5**, *Hydraulic Design of Highway Culverts*, third edition, April 2012
//!   (FHWA-HIF-12-026), cited as "HDS-5". The equations are those of Appendix A, Section A.2
//!   (Equations A.1, A.2 and A.3, printed pages A.1 and A.2), the constants are those of
//!   Table A.1 (printed page A.8, PDF page 197), the slope correction is that of Appendix A
//!   Section A.2.1 and of Section 3.3.4 (printed page 3.25).
//! * **MoWT *Road Design Manual Volume 2: Drainage Design*, January 2010** (cited as "DDM"),
//!   Section 9.4.1 and Appendices 9.2 to 9.5: the manual gives inlet control only as nomographs
//!   (and a figure of the three flow zones), "adapted from publications by the U.S. Bureau of
//!   Public Roads, and from AASHTO model drainage manual". It prints no inlet control equation
//!   and no table of coefficients. HDS-5 Appendix A states that its equations are those "used to
//!   develop the inlet control nomographs" from the same Bureau of Public Roads (National Bureau
//!   of Standards) research. This module therefore takes the equations and every coefficient
//!   from HDS-5, and the culvert check ([`super::culvert_check`]) uses them beside DDM's own
//!   outlet control and DDM's own limits. Which document each number came from is stated at
//!   each constant. DDM's printed example (Section 9.5.1 Step 7, a 2135 mm by 1830 mm box) is
//!   reproduced in the tests.
//!
//! # The equations (HDS-5 Appendix A)
//!
//! With `x = Q / (A·D^0.5)` in SI (Q in m³/s, A the full barrel area in m², D the barrel height
//! or diameter in m) and the unit conversion `Ku = 1.811` for SI (printed under Equation A.2):
//!
//! ```text
//! unsubmerged, Form 1 (A.1)   HWi/D = Hc/D + K·(Ku·x)^M + Ks·S
//! unsubmerged, Form 2 (A.2)   HWi/D = K·(Ku·x)^M
//! submerged           (A.3)   HWi/D = c·(Ku·x)^2 + Y + Ks·S
//! ```
//!
//! `Hc` is the specific head at critical depth (dc + Vc²/2g), `S` the barrel slope (m/m), `Ks`
//! the slope correction, -0.5, or +0.7 for mitred inlets. HWi is measured from the invert of
//! the inlet control section.
//!
//! * **Zones**: the unsubmerged equations apply "up to about Q/AD^0.5 = 3.5 (1.93 SI)" (A.2.1)
//!   and the submerged equation "above about Q/AD^0.5 = 4.0 (2.21 SI)" (A.2.2).
//! * **Transition**: between those two values HDS-5 gives no equation. Appendix A (A.2) says the
//!   zone "is defined empirically by drawing a curve between and tangent to the curves defined by
//!   the unsubmerged and submerged equations". A curve drawn by hand cannot be computed. This
//!   module joins the unsubmerged curve at x = 1.93 to the submerged curve at x = 2.21 by a
//!   straight line, which is continuous with both. That line is **this crate's reading**, not an
//!   HDS-5 equation, and the result says so. The two end values are returned beside it.
//! * **Slope**: HDS-5 prints `Ks·S` in Equations A.1 and A.3 and **not** in Equation A.2 (Form 2),
//!   although the legend gives `Ks` for all of them. This module follows the print, so Form 2
//!   headwaters have no slope term (a little higher than with one, which errs on the safe side
//!   for every inlet except the mitred one, which is Form 1).
//! * **Accuracy**: HDS-5 Section A.4 (printed page A.7) says the nomographs are within 10 percent
//!   of the equation values. A nomograph reading and this module's value can differ by that much.
//! * **g** is 9.8 m/s² as HDS-5 states for SI (Appendix B, under Equation B.1).
//!
//! # Range and refusals
//!
//! * The headwater ratio is refused above the highest graduation of the headwater scale of the
//!   SI nomograph (Charts 1A, 2A, 3A, 8A, 9A, 10A: read from the charts, see [`InletEdge::chart_limit_hw_over_d`]).
//!   Nothing is extrapolated.
//! * A culvert whose outlet invert is more than D/2 above its inlet invert is refused
//!   (HDS-5 Section 3.3.4, printed page 3.25: the horizontal and adverse slope adjustment "should
//!   not be used if the exit invert is more than D/2 higher than the entrance invert"). Within
//!   that limit the signed slope in `Ks·S` gives the same adjustment the section prints.
//! * Tapered inlets, skewed headwalls, offset wingwalls (Charts 11 to 13, 55 to 59), embedded
//!   and open-bottom shapes, pipe-arches and arches are not implemented.
//!
//! The result is a calculation for an engineer to check, not a certified design.

use std::f64::consts::PI;

use crate::{positive, DrainageError};

/// Unit conversion `Ku` for SI, HDS-5 Section A.2.1 (printed page A.2): "1.0 (1.811 SI)".
pub const KU_SI: f64 = 1.811;
/// Upper end of the unsubmerged zone, `Q/AD^0.5` in SI, HDS-5 Section A.2.1 (printed page A.1):
/// "up to about Q/AD^0.5 = 3.5 (1.93 SI)".
pub const UNSUBMERGED_LIMIT_SI: f64 = 1.93;
/// Lower end of the submerged zone, `Q/AD^0.5` in SI, HDS-5 Section A.2.2 (printed page A.2):
/// "above about Q/AD^0.5 = 4.0 (2.21 SI)".
pub const SUBMERGED_LIMIT_SI: f64 = 2.21;
/// Acceleration due to gravity, m/s², as HDS-5 gives it for SI (Appendix B, under Equation B.1).
pub const GRAVITY_HDS5_M_S2: f64 = 9.8;
/// The slope the nomographs were drawn for, HDS-5 Section A.4 (printed page A.6): "A constant
/// slope value of 2 percent (0.02) was used for the development of these design curves."
pub const NOMOGRAPH_SLOPE: f64 = 0.02;
/// Slope correction for every inlet but the mitred one, HDS-5 Section A.2.1: "-0.5".
pub const KS_STANDARD: f64 = -0.5;
/// Slope correction for mitred inlets, HDS-5 Section A.2.1: "mitered inlets +0.7".
pub const KS_MITRED: f64 = 0.7;
/// Lower end of the headwater range HDS-5 says its fitted equations cover, `0.5 D`
/// (Chapter 3, Hydraulics of Inlet Control, printed page 3.4: "within the headwater range of
/// 0.5D to 3.0D").
pub const HW_OVER_D_LOWER_REFERENCE: f64 = 0.5;

/// The cross-section family of the barrel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InletShape {
    Circular,
    Box,
}

/// Which of HDS-5's two unsubmerged equation forms a table row uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquationForm {
    /// Equation A.1, with `Hc/D` and the slope term.
    One,
    /// Equation A.2, a power law with no slope term as printed.
    Two,
}

/// The constants of one row of HDS-5 Table A.1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InletCoefficients {
    /// Chart number of HDS-5 Appendix C the row belongs to.
    pub chart: u8,
    /// Nomograph scale of that chart ("1", "2", "3", "A" or "B").
    pub scale: &'static str,
    pub shape: InletShape,
    pub form: EquationForm,
    pub k: f64,
    pub m: f64,
    pub c: f64,
    pub y: f64,
    /// `Ks`: -0.5, or +0.7 for the mitred inlet.
    pub ks: f64,
}

/// The inlet rows of HDS-5 Table A.1 this module implements: circular concrete pipe (Chart 1),
/// circular corrugated metal pipe (Chart 2), circular pipe with a beveled ring (Chart 3), and
/// concrete box culverts (Charts 8, 9 and 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InletEdge {
    /// Chart 1 scale 1.
    ConcreteSquareEdgeHeadwall,
    /// Chart 1 scale 2.
    ConcreteGrooveEndHeadwall,
    /// Chart 1 scale 3.
    ConcreteGrooveEndProjecting,
    /// Chart 2 scale 1.
    CorrugatedMetalHeadwall,
    /// Chart 2 scale 2.
    CorrugatedMetalMiteredToSlope,
    /// Chart 2 scale 3.
    CorrugatedMetalProjecting,
    /// Chart 3 scale A.
    CircularBeveledRing45,
    /// Chart 3 scale B.
    CircularBeveledRing33_7,
    /// Chart 8 scale 1.
    BoxWingwallFlare30To75,
    /// Chart 8 scale 2.
    BoxWingwallFlare90And15,
    /// Chart 8 scale 3.
    BoxWingwallFlare0,
    /// Chart 9 scale 1.
    BoxWingwallFlare45Offset043D,
    /// Chart 9 scale 2.
    BoxWingwallFlare18To33_7Offset083D,
    /// Chart 10 scale 1.
    BoxHeadwallChamfers,
    /// Chart 10 scale 2.
    BoxHeadwallBevel45,
    /// Chart 10 scale 3.
    BoxHeadwallBevel33_7,
}

impl InletEdge {
    pub const ALL: [InletEdge; 16] = [
        InletEdge::ConcreteSquareEdgeHeadwall,
        InletEdge::ConcreteGrooveEndHeadwall,
        InletEdge::ConcreteGrooveEndProjecting,
        InletEdge::CorrugatedMetalHeadwall,
        InletEdge::CorrugatedMetalMiteredToSlope,
        InletEdge::CorrugatedMetalProjecting,
        InletEdge::CircularBeveledRing45,
        InletEdge::CircularBeveledRing33_7,
        InletEdge::BoxWingwallFlare30To75,
        InletEdge::BoxWingwallFlare90And15,
        InletEdge::BoxWingwallFlare0,
        InletEdge::BoxWingwallFlare45Offset043D,
        InletEdge::BoxWingwallFlare18To33_7Offset083D,
        InletEdge::BoxHeadwallChamfers,
        InletEdge::BoxHeadwallBevel45,
        InletEdge::BoxHeadwallBevel33_7,
    ];

    /// The row's label as HDS-5 Table A.1 prints it, with the shape and material.
    pub fn label(self) -> &'static str {
        match self {
            InletEdge::ConcreteSquareEdgeHeadwall => "Circular concrete, square edge with headwall",
            InletEdge::ConcreteGrooveEndHeadwall => "Circular concrete, groove end with headwall",
            InletEdge::ConcreteGrooveEndProjecting => "Circular concrete, groove end projecting",
            InletEdge::CorrugatedMetalHeadwall => "Circular corrugated metal, headwall",
            InletEdge::CorrugatedMetalMiteredToSlope => "Circular corrugated metal, mitered to slope",
            InletEdge::CorrugatedMetalProjecting => "Circular corrugated metal, projecting",
            InletEdge::CircularBeveledRing45 => "Circular, beveled ring, 45° bevels",
            InletEdge::CircularBeveledRing33_7 => "Circular, beveled ring, 33.7° bevels",
            InletEdge::BoxWingwallFlare30To75 => "Rectangular concrete box, 30° to 75° wingwall flares",
            InletEdge::BoxWingwallFlare90And15 => "Rectangular concrete box, 90° and 15° wingwall flares",
            InletEdge::BoxWingwallFlare0 => "Rectangular concrete box, 0° wingwall flares",
            InletEdge::BoxWingwallFlare45Offset043D => "Rectangular concrete box, 45° wingwall flare, d = 0.043D",
            InletEdge::BoxWingwallFlare18To33_7Offset083D => "Rectangular concrete box, 18° to 33.7° wingwall flare, d = 0.083D",
            InletEdge::BoxHeadwallChamfers => "Rectangular concrete box, 90° headwall with 3/4 in chamfers",
            InletEdge::BoxHeadwallBevel45 => "Rectangular concrete box, 90° headwall with 45° bevels",
            InletEdge::BoxHeadwallBevel33_7 => "Rectangular concrete box, 90° headwall with 33.7° bevels",
        }
    }

    /// The row of HDS-5 Table A.1 (printed page A.8, PDF page 197), exactly as printed.
    pub fn coefficients(self) -> InletCoefficients {
        use EquationForm::{One, Two};
        use InletShape::{Box as B, Circular as C};
        let row = |chart, scale, shape, form, k, m, c, y, ks| InletCoefficients { chart, scale, shape, form, k, m, c, y, ks };
        match self {
            InletEdge::ConcreteSquareEdgeHeadwall => row(1, "1", C, One, 0.0098, 2.0, 0.0398, 0.67, KS_STANDARD),
            InletEdge::ConcreteGrooveEndHeadwall => row(1, "2", C, One, 0.0018, 2.0, 0.0292, 0.74, KS_STANDARD),
            InletEdge::ConcreteGrooveEndProjecting => row(1, "3", C, One, 0.0045, 2.0, 0.0317, 0.69, KS_STANDARD),
            InletEdge::CorrugatedMetalHeadwall => row(2, "1", C, One, 0.0078, 2.0, 0.0379, 0.69, KS_STANDARD),
            InletEdge::CorrugatedMetalMiteredToSlope => row(2, "2", C, One, 0.0210, 1.33, 0.0463, 0.75, KS_MITRED),
            InletEdge::CorrugatedMetalProjecting => row(2, "3", C, One, 0.0340, 1.50, 0.0553, 0.54, KS_STANDARD),
            InletEdge::CircularBeveledRing45 => row(3, "A", C, One, 0.0018, 2.50, 0.0300, 0.74, KS_STANDARD),
            InletEdge::CircularBeveledRing33_7 => row(3, "B", C, One, 0.0018, 2.50, 0.0243, 0.83, KS_STANDARD),
            InletEdge::BoxWingwallFlare30To75 => row(8, "1", B, One, 0.026, 1.0, 0.0347, 0.81, KS_STANDARD),
            InletEdge::BoxWingwallFlare90And15 => row(8, "2", B, One, 0.061, 0.75, 0.0400, 0.80, KS_STANDARD),
            InletEdge::BoxWingwallFlare0 => row(8, "3", B, One, 0.061, 0.75, 0.0423, 0.82, KS_STANDARD),
            InletEdge::BoxWingwallFlare45Offset043D => row(9, "1", B, Two, 0.510, 0.667, 0.0309, 0.80, KS_STANDARD),
            InletEdge::BoxWingwallFlare18To33_7Offset083D => row(9, "2", B, Two, 0.486, 0.667, 0.0249, 0.83, KS_STANDARD),
            InletEdge::BoxHeadwallChamfers => row(10, "1", B, Two, 0.515, 0.667, 0.0375, 0.79, KS_STANDARD),
            InletEdge::BoxHeadwallBevel45 => row(10, "2", B, Two, 0.495, 0.667, 0.0314, 0.82, KS_STANDARD),
            InletEdge::BoxHeadwallBevel33_7 => row(10, "3", B, Two, 0.486, 0.667, 0.0252, 0.865, KS_STANDARD),
        }
    }

    /// The highest graduation of the HW/D scale of this row on the SI nomograph of HDS-5
    /// Appendix C (Chart 1A to 3A, 8A to 10A), read from the printed charts: 6 for Charts 1 and 2;
    /// 3.6 (scale A) and 3.0 (scale B) for Chart 3; 8, 9 and 10 for Chart 8 scales 1 to 3;
    /// 4 for both scales of Chart 9; 10, 9 and 8 for Chart 10 scales 1 to 3. Beyond it the
    /// nomograph, and so the validated range of the equations, ends.
    pub fn chart_limit_hw_over_d(self) -> f64 {
        match self {
            InletEdge::ConcreteSquareEdgeHeadwall
            | InletEdge::ConcreteGrooveEndHeadwall
            | InletEdge::ConcreteGrooveEndProjecting
            | InletEdge::CorrugatedMetalHeadwall
            | InletEdge::CorrugatedMetalMiteredToSlope
            | InletEdge::CorrugatedMetalProjecting => 6.0,
            InletEdge::CircularBeveledRing45 => 3.6,
            InletEdge::CircularBeveledRing33_7 => 3.0,
            InletEdge::BoxWingwallFlare30To75 => 8.0,
            InletEdge::BoxWingwallFlare90And15 => 9.0,
            InletEdge::BoxWingwallFlare0 => 10.0,
            InletEdge::BoxWingwallFlare45Offset043D | InletEdge::BoxWingwallFlare18To33_7Offset083D => 4.0,
            InletEdge::BoxHeadwallChamfers => 10.0,
            InletEdge::BoxHeadwallBevel45 => 9.0,
            InletEdge::BoxHeadwallBevel33_7 => 8.0,
        }
    }

    /// Where the row is in HDS-5, for the calculation lines.
    pub fn citation(self) -> String {
        let c = self.coefficients();
        format!("HDS-5 Table A.1 (PDF p. 197), Chart {} scale {}, {}", c.chart, c.scale, self.label())
    }
}

/// The barrel section. Dimensions in metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InletGeometry {
    Circular { diameter_m: f64 },
    Box { width_m: f64, height_m: f64 },
}

impl InletGeometry {
    /// Barrel height or diameter D, m.
    pub fn height_m(&self) -> f64 {
        match *self {
            InletGeometry::Circular { diameter_m } => diameter_m,
            InletGeometry::Box { height_m, .. } => height_m,
        }
    }

    /// Full barrel area A, m².
    pub fn area_m2(&self) -> f64 {
        match *self {
            InletGeometry::Circular { diameter_m } => PI * diameter_m * diameter_m / 4.0,
            InletGeometry::Box { width_m, height_m } => width_m * height_m,
        }
    }

    pub fn shape(&self) -> InletShape {
        match self {
            InletGeometry::Circular { .. } => InletShape::Circular,
            InletGeometry::Box { .. } => InletShape::Box,
        }
    }
}

/// What the inlet control calculation needs.
#[derive(Debug, Clone, PartialEq)]
pub struct InletInput {
    pub geometry: InletGeometry,
    pub edge: InletEdge,
    /// Discharge of one barrel, m³/s.
    pub flow_m3_s: f64,
    /// Barrel slope, m/m, positive falling to the outlet.
    pub slope_m_per_m: f64,
    /// Barrel length, m. Used only to apply the adverse slope limit of HDS-5 Section 3.3.4.
    pub length_m: f64,
}

/// The flow zone of the inlet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InletRegime {
    Unsubmerged,
    /// Between `Q/AD^0.5` = 1.93 and 2.21 (SI): the straight-line reading of this module.
    Transition,
    Submerged,
}

impl InletRegime {
    pub fn label(self) -> &'static str {
        match self {
            InletRegime::Unsubmerged => "unsubmerged (weir)",
            InletRegime::Transition => "transition",
            InletRegime::Submerged => "submerged (orifice)",
        }
    }
}

/// The result of [`inlet_control`].
#[derive(Debug, Clone, PartialEq)]
pub struct InletControl {
    pub input: InletInput,
    /// Full barrel area A, m².
    pub area_m2: f64,
    /// Barrel height or diameter D, m.
    pub height_m: f64,
    /// `x = Q/(A·D^0.5)` in SI.
    pub flow_parameter_si: f64,
    pub regime: InletRegime,
    /// The unsubmerged equation at `x` (in the transition: at x = 1.93), HW/D. None when submerged.
    pub hw_over_d_unsubmerged: Option<f64>,
    /// The submerged equation at `x` (in the transition: at x = 2.21), HW/D. None when unsubmerged.
    pub hw_over_d_submerged: Option<f64>,
    /// HW/D used.
    pub hw_over_d: f64,
    /// Headwater depth above the inlet invert, HWi, m.
    pub headwater_m: f64,
    /// Specific head at critical depth Hc, m (Form 1 only; None otherwise).
    pub critical_specific_head_m: Option<f64>,
    pub warnings: Vec<String>,
}

fn invalid<T>(message: impl Into<String>) -> Result<T, DrainageError> {
    Err(DrainageError::InvalidInput(message.into()))
}

/// Critical depth and specific head at critical depth in a circular pipe, for a discharge, with g
/// as given. Returns `(dc, Hc)` in m, with `Hc = dc + Vc²/2g = dc + A/(2T)` (HDS-5 Equation A.4).
fn circular_critical(flow_m3_s: f64, d: f64, g: f64) -> (f64, f64) {
    let geometry = |y: f64| {
        let theta = 2.0 * (1.0 - 2.0 * y / d).clamp(-1.0, 1.0).acos();
        (d * d / 8.0 * (theta - theta.sin()), d * (theta / 2.0).sin())
    };
    let f = |y: f64| {
        let (a, t) = geometry(y);
        a.powi(3) / t - flow_m3_s * flow_m3_s / g
    };
    let (mut lo, mut hi) = (1e-9 * d, d * (1.0 - 1e-12));
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if f(mid) < 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let dc = 0.5 * (lo + hi);
    let (a, t) = geometry(dc);
    (dc, dc + a / (2.0 * t))
}

/// Critical depth and specific head of a rectangular box: `dc = (q²/g)^(1/3)`, `Hc = 1.5 dc`, with
/// the critical depth held at the barrel height as HDS-5 Section 3.3.3 says (it "cannot exceed D").
fn box_critical(flow_m3_s: f64, b: f64, d: f64, g: f64) -> (f64, f64) {
    let q = flow_m3_s / b;
    let dc = (q * q / g).cbrt().min(d);
    (dc, 1.5 * dc)
}

struct Evaluator {
    coefficients: InletCoefficients,
    geometry: InletGeometry,
    slope: f64,
}

impl Evaluator {
    fn hc(&self, x_si: f64) -> f64 {
        let a = self.geometry.area_m2();
        let d = self.geometry.height_m();
        let flow = x_si * a * d.sqrt();
        match self.geometry {
            InletGeometry::Circular { diameter_m } => circular_critical(flow, diameter_m, GRAVITY_HDS5_M_S2).1,
            InletGeometry::Box { width_m, height_m } => box_critical(flow, width_m, height_m, GRAVITY_HDS5_M_S2).1,
        }
    }

    /// Equation A.1 or A.2 at `x` (SI).
    fn unsubmerged(&self, x_si: f64) -> f64 {
        let c = &self.coefficients;
        let power = c.k * (KU_SI * x_si).powf(c.m);
        match c.form {
            EquationForm::One => self.hc(x_si) / self.geometry.height_m() + power + c.ks * self.slope,
            EquationForm::Two => power,
        }
    }

    /// Equation A.3 at `x` (SI).
    fn submerged(&self, x_si: f64) -> f64 {
        let c = &self.coefficients;
        c.c * (KU_SI * x_si).powi(2) + c.y + c.ks * self.slope
    }
}

/// Inlet control headwater of one barrel by HDS-5 Appendix A, in SI. See the module documentation
/// for the equations, the transition, the slope term and the refusals.
pub fn inlet_control(input: &InletInput) -> Result<InletControl, DrainageError> {
    let coefficients = input.edge.coefficients();
    if coefficients.shape != input.geometry.shape() {
        return invalid(format!(
            "the inlet '{}' belongs to a {} barrel, and HDS-5 says coefficients for rectangular shapes must not be used for non-rectangular shapes and vice versa (Section A.3, printed page A.2)",
            input.edge.label(),
            if coefficients.shape == InletShape::Box { "rectangular" } else { "circular" }
        ));
    }
    match input.geometry {
        InletGeometry::Circular { diameter_m } => {
            if !positive(diameter_m) {
                return invalid(format!("the diameter must be greater than zero (got {diameter_m})"));
            }
        }
        InletGeometry::Box { width_m, height_m } => {
            if !positive(width_m) || !positive(height_m) {
                return invalid(format!("the box width and height must be greater than zero (got {width_m} and {height_m})"));
            }
        }
    }
    if !positive(input.flow_m3_s) {
        return invalid(format!("the flow must be greater than zero (got {})", input.flow_m3_s));
    }
    if !input.slope_m_per_m.is_finite() || !(input.length_m.is_finite() && input.length_m >= 0.0) {
        return invalid("the slope and the length must be numbers, the length not negative");
    }
    let a = input.geometry.area_m2();
    let d = input.geometry.height_m();
    if input.slope_m_per_m < 0.0 && -input.slope_m_per_m * input.length_m > d / 2.0 {
        return invalid(format!(
            "the outlet invert is {:.3} m above the inlet invert, more than D/2 = {:.3} m; HDS-5 Section 3.3.4 (printed page 3.25) says the adverse slope adjustment should not be used then",
            -input.slope_m_per_m * input.length_m,
            d / 2.0
        ));
    }
    let x = input.flow_m3_s / (a * d.sqrt());
    let ev = Evaluator { coefficients, geometry: input.geometry, slope: input.slope_m_per_m };
    let (regime, unsub, sub, hw_over_d) = if x <= UNSUBMERGED_LIMIT_SI {
        let u = ev.unsubmerged(x);
        (InletRegime::Unsubmerged, Some(u), None, u)
    } else if x >= SUBMERGED_LIMIT_SI {
        let s = ev.submerged(x);
        (InletRegime::Submerged, None, Some(s), s)
    } else {
        let u = ev.unsubmerged(UNSUBMERGED_LIMIT_SI);
        let s = ev.submerged(SUBMERGED_LIMIT_SI);
        let line = u + (x - UNSUBMERGED_LIMIT_SI) / (SUBMERGED_LIMIT_SI - UNSUBMERGED_LIMIT_SI) * (s - u);
        (InletRegime::Transition, Some(u), Some(s), line)
    };
    let limit = input.edge.chart_limit_hw_over_d();
    if hw_over_d > limit {
        return invalid(format!(
            "HW/D = {hw_over_d:.2} is above {limit}, the highest graduation of the headwater scale of HDS-5 Chart {}A (scale {}); the equations are not extrapolated beyond the charts. Use a larger barrel.",
            coefficients.chart, coefficients.scale
        ));
    }
    let mut warnings = Vec::new();
    if hw_over_d < HW_OVER_D_LOWER_REFERENCE {
        warnings.push(format!(
            "HW/D = {hw_over_d:.2} is below 0.5, the lower end of the headwater range HDS-5 says its fitted equations cover (Chapter 3, Hydraulics of Inlet Control, printed page 3.4)."
        ));
    }
    if regime == InletRegime::Transition {
        warnings.push(format!(
            "x = Q/(A·D^0.5) = {x:.3} lies in the transition zone ({UNSUBMERGED_LIMIT_SI} to {SUBMERGED_LIMIT_SI}, SI), where HDS-5 gives no equation (Appendix A, A.2: the curve is drawn by hand). The headwater is this crate's straight line between the two equations, not an HDS-5 value; the two equation values are {:.3} and {:.3}.",
            unsub.unwrap_or(f64::NAN),
            sub.unwrap_or(f64::NAN)
        ));
    }
    warnings.push("HDS-5 Section A.4 (printed page A.7): the nomographs are within 10 percent of the equation values, so a nomograph reading can differ from this value by that much.".to_string());
    let critical_specific_head_m = (coefficients.form == EquationForm::One).then(|| ev.hc(x.min(UNSUBMERGED_LIMIT_SI)));
    Ok(InletControl { input: input.clone(), area_m2: a, height_m: d, flow_parameter_si: x, regime, hw_over_d_unsubmerged: unsub, hw_over_d_submerged: sub, hw_over_d, headwater_m: hw_over_d * d, critical_specific_head_m, warnings })
}

impl InletControl {
    /// The calculation set out line by line with its references.
    pub fn calculation(&self) -> Vec<String> {
        let i = &self.input;
        let c = i.edge.coefficients();
        let d = self.height_m;
        let mut out = vec![
            format!("Inlet control, FHWA HDS-5 (3rd ed., 2012) Appendix A equations in SI; DDM Section 9.4.1 gives the same family only as nomographs (Appendices 9.3 to 9.5). {}.", i.edge.citation()),
            match i.geometry {
                InletGeometry::Circular { diameter_m } => format!("Circular barrel D = {diameter_m:.3} m, A = πD²/4 = {:.4} m²; Q = {:.3} m³/s per barrel; S = {:.5} m/m.", self.area_m2, i.flow_m3_s, i.slope_m_per_m),
                InletGeometry::Box { width_m, height_m } => format!("Box barrel B = {width_m:.3} m by D = {height_m:.3} m, A = B·D = {:.4} m²; Q = {:.3} m³/s per barrel; S = {:.5} m/m.", self.area_m2, i.flow_m3_s, i.slope_m_per_m),
            },
            format!("Q/(A·D^0.5) = {:.3} (SI); Ku = {KU_SI} (HDS-5 A.2.1), so Ku·Q/(A·D^0.5) = {:.3}. Zone: {} (unsubmerged up to {UNSUBMERGED_LIMIT_SI}, submerged from {SUBMERGED_LIMIT_SI}, HDS-5 A.2.1 and A.2.2).", self.flow_parameter_si, KU_SI * self.flow_parameter_si, self.regime.label()),
        ];
        let slope_text = if c.ks > 0.0 { format!("+{}·S", c.ks) } else { format!("{}·S", c.ks) };
        if let Some(u) = self.hw_over_d_unsubmerged {
            let at = if self.regime == InletRegime::Transition { format!(" at x = {UNSUBMERGED_LIMIT_SI}") } else { String::new() };
            match c.form {
                EquationForm::One => out.push(format!(
                    "Unsubmerged, Equation A.1 (Form 1){at}: HWi/D = Hc/D + K·(Ku·x)^M + Ks·S with K = {}, M = {}, Ks = {}: = {:.3} (Hc = {:.3} m, g = {GRAVITY_HDS5_M_S2}; slope term {slope_text} = {:.4}).",
                    c.k,
                    c.m,
                    c.ks,
                    u,
                    self.critical_specific_head_m.unwrap_or(f64::NAN),
                    c.ks * i.slope_m_per_m
                )),
                EquationForm::Two => out.push(format!("Unsubmerged, Equation A.2 (Form 2){at}: HWi/D = K·(Ku·x)^M with K = {}, M = {}: = {:.3} (HDS-5 prints no slope term in Form 2).", c.k, c.m, u)),
            }
        }
        if let Some(s) = self.hw_over_d_submerged {
            let at = if self.regime == InletRegime::Transition { format!(" at x = {SUBMERGED_LIMIT_SI}") } else { String::new() };
            out.push(format!(
                "Submerged, Equation A.3{at}: HWi/D = c·(Ku·x)² + Y + Ks·S with c = {}, Y = {}, Ks = {}: = {:.3} (slope term {slope_text} = {:.4}).",
                c.c,
                c.y,
                c.ks,
                s,
                c.ks * i.slope_m_per_m
            ));
        }
        if self.regime == InletRegime::Transition {
            out.push(format!("Transition: straight line between the two values (this crate's reading of HDS-5 A.2, not an HDS-5 equation): HWi/D = {:.3}.", self.hw_over_d));
        }
        out.push(format!("HWi = (HWi/D)·D = {:.3} × {:.3} = {:.3} m above the inlet invert.", self.hw_over_d, d, self.headwater_m));
        out.extend(self.warnings.iter().map(|w| format!("Check: {w}")));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!((actual - expected).abs() <= tolerance, "expected {expected} ± {tolerance}, got {actual}");
    }

    fn pipe(d: f64, edge: InletEdge, q: f64, s: f64) -> InletInput {
        InletInput { geometry: InletGeometry::Circular { diameter_m: d }, edge, flow_m3_s: q, slope_m_per_m: s, length_m: 30.0 }
    }

    fn boxed(b: f64, d: f64, edge: InletEdge, q: f64, s: f64) -> InletInput {
        InletInput { geometry: InletGeometry::Box { width_m: b, height_m: d }, edge, flow_m3_s: q, slope_m_per_m: s, length_m: 30.0 }
    }

    /// HDS-5 Table A.1 rows, spot checked against the printed table (PDF page 197).
    #[test]
    fn table_a_1_rows_as_printed() {
        let r = InletEdge::ConcreteGrooveEndHeadwall.coefficients();
        assert_eq!((r.chart, r.scale, r.k, r.m, r.c, r.y), (1, "2", 0.0018, 2.0, 0.0292, 0.74));
        let r = InletEdge::CorrugatedMetalMiteredToSlope.coefficients();
        assert_eq!((r.k, r.m, r.c, r.y, r.ks), (0.0210, 1.33, 0.0463, 0.75, 0.7));
        let r = InletEdge::BoxHeadwallBevel33_7.coefficients();
        assert_eq!((r.form, r.k, r.m, r.c, r.y), (EquationForm::Two, 0.486, 0.667, 0.0252, 0.865));
        let r = InletEdge::BoxWingwallFlare90And15.coefficients();
        assert_eq!((r.form, r.k, r.m, r.c, r.y), (EquationForm::One, 0.061, 0.75, 0.0400, 0.80));
        assert_eq!(InletEdge::ALL.len(), 16);
    }

    /// HDS-5 Section A.3.1 (printed pages A.3 and A.4) develops a curve from Equations A.1 and A.3
    /// with K = 0.0340, M = 1.5, c = 0.0496, Y = 0.53, S = 0.02 and a tabulated Hc/D. The
    /// arithmetic is reproduced with the table's Hc/D. The table prints 0.55 in the row
    /// Qc/AD^0.5 = 2.54, where its own columns (0.138 + 0.84 - 0.01) give 0.97: a slip.
    #[test]
    fn section_a_3_1_printed_table() {
        let unsub = |x: f64, hc: f64| hc + 0.0340 * x.powf(1.5) - 0.5 * 0.02;
        let printed = [(0.05, 0.12, 0.11), (0.3, 0.27, 0.27), (1.18, 0.55, 0.58), (4.6, 1.22, 1.54), (6.2, 1.53, 2.05)];
        for (x, hc, result) in printed {
            close(unsub(x, hc), result, 0.006);
        }
        close(unsub(2.54, 0.84), 0.968, 0.001); // the table prints 0.55
        let sub = |x: f64| 0.0496 * x * x + 0.53 - 0.5 * 0.02;
        for (x, result) in [(1.0, 0.57), (2.0, 0.72), (4.0, 1.31), (6.0, 2.31), (8.0, 3.69)] {
            close(sub(x), result, 0.006);
        }
    }

    /// HDS-5 Design Guideline 1.3 (circular, printed pages DG1.1 to DG1.4): Q = 200 ft³/s
    /// (5.663 m³/s), S = 0.01, 54 in (1.3716 m) concrete pipe, groove end. Printed nomograph
    /// HW/D = 1.77, HWi = 7.97 ft; HY-8 7.9 ft. The scale (headwall or projecting) is not stated.
    #[test]
    fn design_guideline_1_3_circular_concrete() {
        let r = inlet_control(&pipe(1.3716, InletEdge::ConcreteGrooveEndHeadwall, 5.663, 0.01)).unwrap();
        assert_eq!(r.regime, InletRegime::Submerged);
        close(r.hw_over_d, 1.76, 0.01);
        close(r.headwater_m / 0.3048, 7.9, 0.1); // HY-8: 7.9 ft; the nomograph 7.97 ft
        assert!((r.hw_over_d - 1.77).abs() / 1.77 < 0.10);
        let projecting = inlet_control(&pipe(1.3716, InletEdge::ConcreteGrooveEndProjecting, 5.663, 0.01)).unwrap();
        assert!((projecting.hw_over_d - 1.77).abs() / 1.77 < 0.10);
        // 60 in (1.524 m): printed HW/D = 1.36.
        let r60 = inlet_control(&pipe(1.524, InletEdge::ConcreteGrooveEndHeadwall, 5.663, 0.01)).unwrap();
        close(r60.hw_over_d, 1.34, 0.01);
        assert!((r60.hw_over_d - 1.36).abs() / 1.36 < 0.10);
        // 72 in (1.8288 m) corrugated metal, 45 degree bevels (Chart 3 scale A), unsubmerged:
        // printed HW/D = 0.96.
        let cm = inlet_control(&pipe(1.8288, InletEdge::CircularBeveledRing45, 5.663, 0.01)).unwrap();
        assert_eq!(cm.regime, InletRegime::Unsubmerged);
        assert!((cm.hw_over_d - 0.96).abs() / 0.96 < 0.10, "{}", cm.hw_over_d);
        // 60 in beveled ring, submerged: printed HW/D = 1.40.
        let cm60 = inlet_control(&pipe(1.524, InletEdge::CircularBeveledRing45, 5.663, 0.01)).unwrap();
        assert!((cm60.hw_over_d - 1.40).abs() / 1.40 < 0.10);
    }

    /// HDS-5 Design Guideline 1.4 (box, printed pages DG1.8 to DG1.10): Q = 300 ft³/s (8.5 m³/s),
    /// S = 0.02. 6 ft by 5 ft box, square edge in a 90 degree headwall: printed HW/D = 1.57,
    /// HWi = 7.9 ft. 5 ft by 5 ft: 1.91 and 9.6 ft (HY-8 9.65 ft). 5 ft by 5 ft with 45 degree
    /// bevels: 1.71 and 8.55 ft; the SI result is printed as 33.10 m, 2.62 m above the invert.
    #[test]
    fn design_guideline_1_4_concrete_box() {
        let ft = 0.3048;
        let sq = boxed(6.0 * ft, 5.0 * ft, InletEdge::BoxWingwallFlare90And15, 8.5, 0.02);
        let r = inlet_control(&sq).unwrap();
        close(r.hw_over_d, 1.59, 0.01);
        assert!((r.hw_over_d - 1.57).abs() / 1.57 < 0.05);
        let r55 = inlet_control(&boxed(5.0 * ft, 5.0 * ft, InletEdge::BoxWingwallFlare90And15, 8.5, 0.02)).unwrap();
        close(r55.hw_over_d, 1.94, 0.01);
        close(r55.headwater_m / ft, 9.65, 0.15); // HY-8
        assert!((r55.hw_over_d - 1.91).abs() / 1.91 < 0.05);
        let bevel = inlet_control(&boxed(5.0 * ft, 5.0 * ft, InletEdge::BoxHeadwallBevel45, 8.5, 0.02)).unwrap();
        close(bevel.hw_over_d, 1.714, 0.002);
        close(bevel.headwater_m / ft, 8.55, 0.05);
        close(bevel.headwater_m, 2.62, 0.02); // 33.10 m less the invert 30.48 m
        assert_eq!(bevel.regime, InletRegime::Submerged);
    }

    /// HDS-5 Chart 1A (SI nomograph, PDF page 218) prints an example: D = 1050 mm, Q = 3.40 m³/s,
    /// HW/D = 2.5, 2.1, 2.2 for scales 1, 2, 3 (HW 2.68, 2.26, 2.35 m). The nomographs are drawn for
    /// S = 0.02, so S = 0.02 here. The printed HW column is 0.04 to 0.055 m above HW/D times D in
    /// every row (2.5 × 1.05 = 2.625, not 2.68): a slip or an unrounded reading.
    #[test]
    fn chart_1a_printed_si_example() {
        let printed = [(InletEdge::ConcreteSquareEdgeHeadwall, 2.5, 2.68), (InletEdge::ConcreteGrooveEndHeadwall, 2.1, 2.26), (InletEdge::ConcreteGrooveEndProjecting, 2.2, 2.35)];
        for (edge, hw_d, hw) in printed {
            let r = inlet_control(&pipe(1.05, edge, 3.40, NOMOGRAPH_SLOPE)).unwrap();
            assert_eq!(r.regime, InletRegime::Submerged);
            assert!((r.hw_over_d - hw_d).abs() / hw_d < 0.10, "{edge:?} {}", r.hw_over_d);
            assert!((r.headwater_m - hw).abs() / hw < 0.10);
        }
        close(inlet_control(&pipe(1.05, InletEdge::ConcreteGrooveEndHeadwall, 3.40, 0.02)).unwrap().hw_over_d, 2.136, 0.002);
        // Chart 8A (PDF page 232): 1500 mm by 600 mm box, Q = 2.12 m³/s, HW/D 1.75, 1.90, 2.05 for scales 1 to 3.
        for (edge, hw_d) in [(InletEdge::BoxWingwallFlare30To75, 1.75), (InletEdge::BoxWingwallFlare90And15, 1.90), (InletEdge::BoxWingwallFlare0, 2.05)] {
            let r = inlet_control(&boxed(1.5, 0.6, edge, 2.12, NOMOGRAPH_SLOPE)).unwrap();
            assert!((r.hw_over_d - hw_d).abs() / hw_d < 0.10, "{edge:?} {}", r.hw_over_d);
        }
    }

    /// DDM Section 9.5.1 Step 7 (printed page 98): 2135 mm by 1830 mm box, Q = 11.33 m³/s,
    /// "45° bevel" entrance (Appendix 9.5 nomograph, HW/D = 1.27, HWi = 2.32 m). The flow is in
    /// the transition zone: x = 11.33/(2.135 × 1.83 × 1.83^0.5) = 2.14, between 1.93 and 2.21.
    /// The example's culvert slope is So·L = (0.05)90 in Step 8 g, so S = 0.05.
    #[test]
    fn ddm_example_step_7_transition() {
        let r = inlet_control(&boxed(2.135, 1.83, InletEdge::BoxHeadwallBevel45, 11.33, 0.05)).unwrap();
        assert_eq!(r.regime, InletRegime::Transition);
        close(r.flow_parameter_si, 2.144, 0.002);
        let (u, s) = (r.hw_over_d_unsubmerged.unwrap(), r.hw_over_d_submerged.unwrap());
        close(u, 0.495 * (1.811f64 * 1.93).powf(0.667), 1e-9);
        close(s, 0.0314 * (1.811f64 * 2.21).powi(2) + 0.82 - 0.025, 1e-9);
        // Hand worked: line from (1.93, 1.1405) to (2.21, 1.2975) evaluated at 2.144.
        close(r.hw_over_d, 1.2604, 0.002);
        // DDM prints 1.27 from the nomograph, 2.32 m: within the 10 percent HDS-5 allows.
        assert!((r.hw_over_d - 1.27).abs() / 1.27 < 0.01);
        assert!((r.headwater_m - 2.32).abs() < 0.03);
        assert!(r.calculation().iter().any(|l| l.contains("straight line")));
        assert!(r.warnings.iter().any(|w| w.contains("transition zone")));
    }

    /// Hand-worked unsubmerged case, Form 1, box: B = 3 m, D = 2 m, Q = 6 m³/s, S = 0.01,
    /// Chart 8 scale 3 (K = 0.061, M = 0.75). q = 2 m²/s, dc = (4/9.8)^(1/3) = 0.7421 m,
    /// Hc = 1.5 dc = 1.1131 m. x = 6/(6 × 1.4142) = 0.7071 (unsubmerged). Ku·x = 1.2806.
    /// HW/D = 1.1131/2 + 0.061 × 1.2806^0.75 - 0.005 = 0.55655 + 0.07344 - 0.005 = 0.6250.
    #[test]
    fn hand_worked_unsubmerged_box() {
        let r = inlet_control(&boxed(3.0, 2.0, InletEdge::BoxWingwallFlare0, 6.0, 0.01)).unwrap();
        assert_eq!(r.regime, InletRegime::Unsubmerged);
        close(r.critical_specific_head_m.unwrap(), 1.1131, 0.0005);
        close(r.hw_over_d, 0.6250, 0.0005);
        close(r.headwater_m, 1.250, 0.001);
        assert!(r.hw_over_d_submerged.is_none());
    }

    /// Hand-worked submerged case, circular: D = 1.2 m, Q = 3.5 m³/s, S = 0.005, Chart 1 scale 1.
    /// A = 1.13097, x = 3.5/(1.13097 × 1.09545) = 2.8251 (submerged), Ku·x = 5.1163, squared 26.176.
    /// HW/D = 0.0398 × 26.176 + 0.67 - 0.0025 = 1.04180 + 0.6675 = 1.7093; HWi = 2.051 m.
    #[test]
    fn hand_worked_submerged_circular() {
        let r = inlet_control(&pipe(1.2, InletEdge::ConcreteSquareEdgeHeadwall, 3.5, 0.005)).unwrap();
        assert_eq!(r.regime, InletRegime::Submerged);
        close(r.flow_parameter_si, 2.8251, 0.0005);
        close(r.hw_over_d, 1.7093, 0.0005);
        close(r.headwater_m, 2.0512, 0.001);
    }

    /// Hand-worked Form 2 and mitred cases. Form 2 has no slope term as printed; a mitred inlet
    /// has Ks = +0.7.
    #[test]
    fn form_two_has_no_slope_term_and_mitred_has_plus_point_seven() {
        let a = inlet_control(&boxed(2.0, 1.0, InletEdge::BoxHeadwallChamfers, 1.5, 0.0)).unwrap();
        let b = inlet_control(&boxed(2.0, 1.0, InletEdge::BoxHeadwallChamfers, 1.5, 0.05)).unwrap();
        assert_eq!(a.hw_over_d, b.hw_over_d); // x = 0.75, unsubmerged: 0.515 (1.811 × 0.75)^0.667 = 0.515 × 1.2276
        close(a.hw_over_d, 0.515 * (1.811f64 * 0.75).powf(0.667), 1e-9);
        let flat = inlet_control(&pipe(1.0, InletEdge::CorrugatedMetalMiteredToSlope, 0.4, 0.0)).unwrap();
        let steep = inlet_control(&pipe(1.0, InletEdge::CorrugatedMetalMiteredToSlope, 0.4, 0.02)).unwrap();
        close(steep.hw_over_d - flat.hw_over_d, 0.7 * 0.02, 1e-9);
        let std_flat = inlet_control(&pipe(1.0, InletEdge::CorrugatedMetalHeadwall, 0.4, 0.0)).unwrap();
        let std_steep = inlet_control(&pipe(1.0, InletEdge::CorrugatedMetalHeadwall, 0.4, 0.02)).unwrap();
        close(std_steep.hw_over_d - std_flat.hw_over_d, -0.5 * 0.02, 1e-9);
    }

    #[test]
    fn refusals_have_plain_messages() {
        // Above the top of the SI chart scale: HW/D > 6 for Chart 1.
        let e = inlet_control(&pipe(0.6, InletEdge::ConcreteSquareEdgeHeadwall, 3.0, 0.01)).unwrap_err().to_string();
        assert!(e.contains("highest graduation") && e.contains("Use a larger barrel"), "{e}");
        // A box inlet on a pipe, and the reverse.
        assert!(inlet_control(&pipe(1.0, InletEdge::BoxHeadwallBevel45, 1.0, 0.01)).unwrap_err().to_string().contains("rectangular"));
        assert!(inlet_control(&boxed(1.0, 1.0, InletEdge::ConcreteSquareEdgeHeadwall, 1.0, 0.01)).unwrap_err().to_string().contains("circular"));
        // Adverse fall above D/2.
        let adverse = InletInput { length_m: 20.0, slope_m_per_m: -0.03, ..pipe(1.0, InletEdge::ConcreteSquareEdgeHeadwall, 0.5, 0.0) };
        assert!(inlet_control(&adverse).unwrap_err().to_string().contains("D/2"));
        let ok = InletInput { length_m: 10.0, slope_m_per_m: -0.03, ..adverse.clone() };
        assert!(inlet_control(&ok).is_ok());
        assert!(inlet_control(&pipe(1.0, InletEdge::ConcreteSquareEdgeHeadwall, 0.0, 0.01)).is_err());
        assert!(inlet_control(&pipe(0.0, InletEdge::ConcreteSquareEdgeHeadwall, 1.0, 0.01)).is_err());
    }
}
