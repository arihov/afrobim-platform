// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Flexible pavement design by the *AASHTO Guide for Design of Pavement Structures* (1993):
//! the empirical design equation solved for the structural number, and the layered structural
//! number check of an entered structure.
//!
//! # Sources
//!
//! The 1993 Guide itself is not free and was not read. Everything here is taken from two
//! documents that reproduce it, and each item says which:
//!
//! - **FHWA**: Federal Highway Administration, *Geotechnical Aspects of Pavements Reference
//!   Manual*, publication FHWA NHI-05-037 (2006), Appendix C "1993 AASHTO Design Method"
//!   (`https://www.fhwa.dot.gov/engineering/geotech/pubs/05037/ac.cfm`, page dated 06/27/2017).
//! - **Notes**: T. H. Fadhil and T. M. Ahmed, *Flexible Pavement AASHTO 1993*, postgraduate
//!   lecture notes, Department of Civil Engineering, University of Anbar (30 pages), supplied
//!   by the owner. They are lecture notes, not the Guide.
//!
//! The design equation (FHWA Equation C.2, Notes Equation 4.1), the structural number
//! (FHWA C.3, Notes 4.15) and the table of standard normal deviates (FHWA Table C-3, which the
//! Notes reproduce) were compared between the two and are identical, term for term and value
//! for value.
//!
//! # What this covers
//!
//! | Step | Here | Source |
//! |---|---|---|
//! | `log10 W18 = ZR·S0 + 9.36 log10(SN+1) − 0.20 + log10(ΔPSI/(4.2−1.5)) / (0.40 + 1094/(SN+1)^5.19) + 2.32 log10 MR − 8.07` | [`log10_w18`] | FHWA Eq. C.2; Notes Eq. 4.1 |
//! | The same solved for SN by bisection | [`required_structural_number`] | FHWA C.2 and C.4 ("must be solved implicitly") |
//! | Reliability to ZR | [`standard_normal_deviate`] | FHWA Table C-3 |
//! | `SN = a1·D1 + a2·D2·m2 + a3·D3·m3`, D in inches | [`structural_number`] | FHWA Eq. C.3; Notes Eq. 4.15 |
//! | SN required above each unbound layer, from that layer's modulus | [`design`] | FHWA Appendix C, procedure step 8 |
//! | Design lane ESALs `W18 = 365 · Σ(AADT·LEF) · G · DD · DL` | [`design_lane_esals`] | Notes Eq. 4.5 |
//! | Effective roadbed modulus from relative damage `uf = 1.18×10⁸ · MR^−2.32` | [`effective_roadbed_modulus_psi`] | Notes, "Equation 7.8" and the steps before it |
//! | Stage reliability `R_stage = R_overall^(1/n)` | [`stage_reliability_percent`] | Notes, section 4.1.7 |
//!
//! # What stays an input
//!
//! W18 (or the counts, load factors, growth factor and distribution factors it is made from),
//! the reliability or ZR, S0, the serviceability loss ΔPSI, the roadbed resilient modulus, and
//! for every layer its thickness, its layer coefficient `a` and its drainage coefficient `m`.
//! **No default is supplied for any of them.** The [`suggestions`] module holds values the two
//! sources print, each labelled with where it was read; an application may show them as
//! suggestions "as reproduced from the AASHTO 1993 Guide", and the entered value governs.
//! The Guide's charts for layer coefficients (asphalt, cement-treated and bituminous-treated
//! bases) are nomographs: those coefficients are inputs only.
//!
//! # What it does not do
//!
//! Rigid pavement design; overlay design; the serviceability loss from swelling and frost
//! heave (enter the ΔPSI left for traffic); minimum layer thicknesses and rounding rules of the
//! Guide; choosing thicknesses (many combinations satisfy one SN; the engineer enters a
//! structure and this checks it, reporting the thickness each layer would need); conversion of
//! CBR to resilient modulus; load equivalency factors by axle type and SN.
//!
//! # Differences and slips found in the sources
//!
//! - The equation and the ZR table agree between FHWA and the Notes (see above).
//! - FHWA Equation C.5 is printed `ΔPSI = pt − po = 4.2 − 2.5 = 1.7`; it is `po − pt`.
//! - Notes, Example 1: the table gives a load factor of 2.24 for 6-axle trucks; the calculation
//!   line uses 2.44, and the printed answer (5.35 × 10⁶) follows from 2.44.
//! - **FHWA Appendix C, procedure step 8.** The required SN over the subgrade, 5.07, is
//!   reproduced exactly. The SN required above the granular base (EBS = 40 000 psi) is printed
//!   as 2.62; the printed Equation C.2 with the printed inputs gives 2.74 (at SN = 2.62 it
//!   carries 8.8 million ESALs, not the 11.6 million of the example). This module follows the
//!   equation, so it asks for 158 mm of asphalt where the example's 2.62 leads to 6 inches.
//! - Notes, Appendix H example: the seasonal moduli are given for 5, 5.5, 1.5 and 1 months,
//!   which add up to 13, and the effective modulus is printed as 5.7 × 10³ psi. The relative
//!   damage expression printed in the same Notes gives 5 470 psi for those seasons (5 290 psi
//!   if the frozen month is left out), so the printed value is not reproduced. The roadbed
//!   modulus is an input here either way.
//! - Notes Table 4.6 (drainage coefficients) is a scan in which the decimal points are missing
//!   ("1 40–1 35"); the values in [`suggestions`] put them back.
//! - The two nomograph examples in the Notes are chart readings: SN = 5.0, where the equation
//!   gives 4.98, and SN = 5.6 (Appendix H), where it gives 5.55.
//! - The units are those of the Guide: MR in psi and D in inches. Thicknesses are entered here
//!   in millimetres and divided by 25.4.

use super::{num, DesignError, DesignMethod, DesignResult};
use crate::model::{Layer, Material};

/// Millimetres per inch (exact by definition).
const MM_PER_INCH: f64 = 25.4;

/// The right-hand side of the 1993 AASHTO flexible design equation (FHWA NHI-05-037 Equation
/// C.2; Notes Equation 4.1): log10 of the 18-kip ESALs a structure of structural number `sn`
/// carries.
///
/// ```text
/// log10 W18 = ZR·S0 + 9.36·log10(SN + 1) − 0.20
///           + log10(ΔPSI / (4.2 − 1.5)) / (0.40 + 1094 / (SN + 1)^5.19)
///           + 2.32·log10(MR) − 8.07                         MR in psi
/// ```
pub fn log10_w18(zr: f64, s0: f64, sn: f64, delta_psi: f64, mr_psi: f64) -> f64 {
    zr * s0 + 9.36 * (sn + 1.0).log10() - 0.20 + (delta_psi / (4.2 - 1.5)).log10() / (0.40 + 1094.0 / (sn + 1.0).powf(5.19)) + 2.32 * mr_psi.log10() - 8.07
}

/// The structural number that makes the design equation carry `w18`, by bisection between 0 and
/// 30. `None` when the inputs are not positive numbers or no SN in that range does it.
pub fn required_structural_number(w18: f64, zr: f64, s0: f64, delta_psi: f64, mr_psi: f64) -> Option<f64> {
    if !(w18 > 0.0 && delta_psi > 0.0 && mr_psi > 0.0 && s0 >= 0.0) || !zr.is_finite() || !w18.is_finite() {
        return None;
    }
    let target = w18.log10();
    let f = |sn: f64| log10_w18(zr, s0, sn, delta_psi, mr_psi) - target;
    let (mut lo, mut hi) = (0.0, 30.0);
    if !(f(lo) < 0.0 && f(hi) > 0.0) {
        return None;
    }
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        if f(mid) < 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(0.5 * (lo + hi))
}

/// FHWA NHI-05-037 Table C-3 "Standard normal deviates for various levels of reliability"
/// (reproduced unchanged in the Notes): (reliability in per cent, ZR).
pub const STANDARD_NORMAL_DEVIATES: [(f64, f64); 18] = [
    (50.0, 0.000),
    (60.0, -0.253),
    (70.0, -0.524),
    (75.0, -0.674),
    (80.0, -0.841),
    (85.0, -1.037),
    (90.0, -1.282),
    (91.0, -1.340),
    (92.0, -1.405),
    (93.0, -1.476),
    (94.0, -1.555),
    (95.0, -1.645),
    (96.0, -1.751),
    (97.0, -1.881),
    (98.0, -2.054),
    (99.0, -2.327),
    (99.9, -3.090),
    (99.99, -3.750),
];

/// ZR for a reliability listed in FHWA Table C-3. `None` for a reliability the table does not
/// list: nothing is interpolated, enter ZR instead.
pub fn standard_normal_deviate(reliability_percent: f64) -> Option<f64> {
    STANDARD_NORMAL_DEVIATES.iter().find(|(r, _)| (r - reliability_percent).abs() < 1e-9).map(|(_, z)| *z)
}

/// Reliability of each stage of a staged strategy (Notes, section 4.1.7):
/// `R_stage = R_overall^(1/n)`, in per cent.
pub fn stage_reliability_percent(overall_percent: f64, stages: u32) -> f64 {
    100.0 * (overall_percent / 100.0).powf(1.0 / f64::from(stages.max(1)))
}

/// Design lane ESALs over the period (Notes Equation 4.5):
/// `W18 = 365 · Σ(AADT_i · LEF_i) · G · DD · DL`.
///
/// `vehicles` are (two-way AADT of a vehicle type, its load factor in ESALs per vehicle);
/// `growth_factor` is G (the Notes' 25.54 for 20 years at 2.5 % is `((1 + r)^t − 1)/r`);
/// `directional` and `lane` are the distribution factors DD and DL as fractions. All entered.
pub fn design_lane_esals(vehicles: &[(f64, f64)], growth_factor: f64, directional: f64, lane: f64) -> f64 {
    365.0 * vehicles.iter().map(|(aadt, lef)| aadt * lef).sum::<f64>() * growth_factor * directional * lane
}

/// Relative damage of a roadbed modulus (Notes, "Equation 7.8"): `uf = 1.18×10⁸ · MR^−2.32`,
/// MR in psi.
pub fn relative_damage(mr_psi: f64) -> f64 {
    1.18e8 * mr_psi.powf(-2.32)
}

/// Effective roadbed resilient modulus, psi (Notes, "Steps how to find effective roadbed
/// Resilient Modulus"): the modulus whose relative damage is the average of the seasonal
/// relative damages. `seasons` are (MR in psi, duration in any one unit); the average is
/// weighted by duration. `None` for an empty list or a modulus that is not positive.
pub fn effective_roadbed_modulus_psi(seasons: &[(f64, f64)]) -> Option<f64> {
    let total: f64 = seasons.iter().map(|s| s.1).sum();
    if seasons.is_empty() || !(total > 0.0) || seasons.iter().any(|s| !(s.0 > 0.0) || !(s.1 >= 0.0)) {
        return None;
    }
    let mean = seasons.iter().map(|(mr, t)| relative_damage(*mr) * t).sum::<f64>() / total;
    Some((mean / 1.18e8).powf(-1.0 / 2.32))
}

/// One layer of the structure being checked.
#[derive(Debug, Clone, PartialEq)]
pub struct SnLayer {
    pub name: String,
    pub material: Material,
    pub thickness_mm: f64,
    /// Structural layer coefficient `a`, per inch, as entered.
    pub layer_coefficient: f64,
    /// Drainage coefficient `m`, as entered; 1 for bound layers (the drainage modification is
    /// for untreated base and subbase only).
    pub drainage_coefficient: f64,
    /// Resilient modulus of this layer, psi, for an unbound layer. When given, the structural
    /// number required above this layer is computed with it in place of the roadbed modulus and
    /// compared with what the layers above provide (FHWA Appendix C, procedure step 8).
    pub resilient_modulus_psi: Option<f64>,
}

impl SnLayer {
    /// This layer's contribution `a · D · m`, D in inches.
    pub fn structural_number(&self) -> f64 {
        self.layer_coefficient * self.thickness_mm / MM_PER_INCH * self.drainage_coefficient
    }
}

/// `SN = Σ a_i · D_i · m_i` (FHWA Equation C.3 generalised as the text under it allows; Notes
/// Equation 4.15), D in inches.
pub fn structural_number(layers: &[SnLayer]) -> f64 {
    layers.iter().map(SnLayer::structural_number).sum()
}

/// How the reliability is given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reliability {
    /// A reliability listed in FHWA Table C-3, per cent.
    Percent(f64),
    /// The standard normal deviate itself (negative for reliabilities above 50 %).
    StandardNormalDeviate(f64),
}

/// Everything the method needs. Nothing has a default.
#[derive(Debug, Clone, PartialEq)]
pub struct Aashto93Input {
    /// Predicted 18-kip ESALs on the design lane over the performance period.
    pub w18: f64,
    pub reliability: Reliability,
    /// Overall standard deviation S0.
    pub overall_standard_deviation: f64,
    /// Serviceability loss ΔPSI allowed for traffic (initial minus terminal, less any loss
    /// assigned to swelling or frost heave).
    pub delta_psi: f64,
    /// Effective roadbed resilient modulus MR, psi.
    pub roadbed_modulus_psi: f64,
    /// The structure to check, top first. May be empty: the required SN is then all that is
    /// computed.
    pub layers: Vec<SnLayer>,
}

/// The check at the top of one layer.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceCheck {
    /// Name of the supporting layer, or "roadbed".
    pub support: String,
    pub support_modulus_psi: f64,
    /// SN required above the support.
    pub required_sn: f64,
    /// SN provided by the layers above it.
    pub provided_sn: f64,
    /// Thickness the layer directly above would need for `provided = required`, the other
    /// layers staying as entered, mm (not rounded). `None` when that layer's `a·m` is zero.
    pub thickness_needed_mm: Option<f64>,
}

impl InterfaceCheck {
    pub fn passes(&self) -> bool {
        self.provided_sn >= self.required_sn - 1e-9
    }
}

/// The result.
#[derive(Debug, Clone, PartialEq)]
pub struct Aashto93Design {
    pub input: Aashto93Input,
    pub zr: f64,
    /// SN required over the roadbed.
    pub required_sn: f64,
    /// SN of the entered structure.
    pub provided_sn: f64,
    /// One check per unbound layer with a modulus, top down, then the roadbed. Empty when no
    /// layers were entered.
    pub checks: Vec<InterfaceCheck>,
    pub warnings: Vec<String>,
}

impl Aashto93Design {
    /// True when layers were entered and every check passes.
    pub fn adequate(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(InterfaceCheck::passes)
    }
}

/// Solves the design equation for the required SN and checks the entered structure.
pub fn design(input: &Aashto93Input) -> Result<Aashto93Design, DesignError> {
    let bad = |m: String| Err(DesignError::InvalidInput(m));
    let zr = match input.reliability {
        Reliability::StandardNormalDeviate(z) if z.is_finite() => z,
        Reliability::StandardNormalDeviate(z) => return bad(format!("The standard normal deviate cannot be {z}.")),
        Reliability::Percent(r) => match standard_normal_deviate(r) {
            Some(z) => z,
            None => return bad(format!("A reliability of {r} per cent is not in FHWA NHI-05-037 Table C-3; enter the standard normal deviate.")),
        },
    };
    if !(input.overall_standard_deviation.is_finite() && input.overall_standard_deviation >= 0.0) {
        return bad(format!("The overall standard deviation cannot be {}.", input.overall_standard_deviation));
    }
    for l in &input.layers {
        if !(l.thickness_mm.is_finite() && l.thickness_mm >= 0.0 && l.layer_coefficient.is_finite() && l.layer_coefficient >= 0.0 && l.drainage_coefficient.is_finite() && l.drainage_coefficient >= 0.0)
            || l.resilient_modulus_psi.is_some_and(|e| !(e > 0.0))
        {
            return bad(format!("Layer '{}' has a thickness, coefficient or modulus that cannot be used.", l.name));
        }
    }
    let solve = |mr: f64, what: &str| {
        required_structural_number(input.w18, zr, input.overall_standard_deviation, input.delta_psi, mr)
            .ok_or_else(|| DesignError::InvalidInput(format!("The design equation has no structural number between 0 and 30 for these inputs ({what}).")))
    };
    let required_sn = solve(input.roadbed_modulus_psi, "over the roadbed")?;
    let provided_sn = structural_number(&input.layers);

    let mut checks = Vec::new();
    let mut above = 0.0;
    for (i, layer) in input.layers.iter().enumerate() {
        if let (Some(modulus), true) = (layer.resilient_modulus_psi, i > 0) {
            let need = solve(modulus, &format!("above '{}'", layer.name))?;
            checks.push(interface(&layer.name, modulus, need, above, &input.layers[i - 1]));
        }
        above += layer.structural_number();
    }
    if let Some(last) = input.layers.last() {
        checks.push(interface("roadbed", input.roadbed_modulus_psi, required_sn, provided_sn, last));
    }

    let mut warnings = vec![
        "The equation and tables are from FHWA NHI-05-037 Appendix C and the University of Anbar lecture notes, which reproduce the AASHTO 1993 Guide; the Guide itself was not read.".to_string(),
        "Layer coefficients, drainage coefficients, S0, reliability and ΔPSI are the entered values. The Guide's minimum thicknesses and its allowance for swelling and frost heave are not applied.".to_string(),
    ];
    if input.delta_psi >= 4.2 - 1.5 {
        warnings.push("ΔPSI is 2.7 or more: the serviceability term of the equation is then zero or positive, outside the range of AASHO Road Test serviceability loss (4.2 to 1.5).".to_string());
    }
    for c in checks.iter().filter(|c| !c.passes()) {
        warnings.push(format!("The structure above the {} provides SN = {} where {} is required.", c.support, num(c.provided_sn, 2), num(c.required_sn, 2)));
    }
    Ok(Aashto93Design { input: input.clone(), zr, required_sn, provided_sn, checks, warnings })
}

fn interface(support: &str, modulus: f64, required: f64, provided: f64, layer_above: &SnLayer) -> InterfaceCheck {
    let per_mm = layer_above.layer_coefficient * layer_above.drainage_coefficient / MM_PER_INCH;
    InterfaceCheck {
        support: support.to_string(),
        support_modulus_psi: modulus,
        required_sn: required,
        provided_sn: provided,
        thickness_needed_mm: (per_mm > 0.0).then(|| ((required - (provided - layer_above.structural_number())) / per_mm).max(0.0)),
    }
}

impl DesignResult for Aashto93Design {
    fn method(&self) -> DesignMethod {
        DesignMethod::Aashto1993
    }

    fn calculation(&self) -> Vec<String> {
        let i = &self.input;
        let reliability = match i.reliability {
            Reliability::Percent(r) => format!("R = {}%, ZR = {} (FHWA NHI-05-037 Table C-3)", num(r, 2), num(self.zr, 3)),
            Reliability::StandardNormalDeviate(_) => format!("ZR = {} (entered)", num(self.zr, 3)),
        };
        let mut out = vec![
            format!("Design basis: {}.", DesignMethod::Aashto1993.reference()),
            "log10 W18 = ZR·S0 + 9.36·log10(SN + 1) − 0.20 + log10(ΔPSI/(4.2 − 1.5)) / (0.40 + 1094/(SN + 1)^5.19) + 2.32·log10(MR) − 8.07 (FHWA Eq. C.2; lecture notes Eq. 4.1).".to_string(),
            format!(
                "Inputs: W18 = {} ESALs; {reliability}; S0 = {}; ΔPSI = {}; roadbed MR = {} psi.",
                num(i.w18, 0),
                num(i.overall_standard_deviation, 3),
                num(i.delta_psi, 2),
                num(i.roadbed_modulus_psi, 0)
            ),
            format!(
                "Required structural number over the roadbed, by bisection: SN = {} (the equation then gives log10 W18 = {} against {}).",
                num(self.required_sn, 2),
                num(log10_w18(self.zr, i.overall_standard_deviation, self.required_sn, i.delta_psi, i.roadbed_modulus_psi), 4),
                num(i.w18.log10(), 4)
            ),
        ];
        if i.layers.is_empty() {
            out.push("No structure was entered to check.".to_string());
        } else {
            out.push("Structure entered, SN = Σ a·D·m with D in inches (FHWA Eq. C.3; lecture notes Eq. 4.15):".to_string());
            for l in &i.layers {
                out.push(format!(
                    "  {}: D = {} mm = {} in, a = {}, m = {}: a·D·m = {}.",
                    l.name,
                    num(l.thickness_mm, 1),
                    num(l.thickness_mm / MM_PER_INCH, 2),
                    num(l.layer_coefficient, 3),
                    num(l.drainage_coefficient, 2),
                    num(l.structural_number(), 3)
                ));
            }
            for c in &self.checks {
                let need = c.thickness_needed_mm.map_or(String::new(), |t| format!(" The layer directly above would need {} mm ({} in) to meet it exactly.", num(t, 0), num(t / MM_PER_INCH, 2)));
                out.push(format!(
                    "Above the {} (MR = {} psi): SN required = {}, provided = {}: {}.{need}",
                    c.support,
                    num(c.support_modulus_psi, 0),
                    num(c.required_sn, 2),
                    num(c.provided_sn, 2),
                    if c.passes() { "OK" } else { "NOT ENOUGH" }
                ));
            }
        }
        out.extend(self.warnings.iter().map(|w| format!("Note: {w}")));
        out.push("This is a calculation for an engineer to check, not a certified design.".to_string());
        out
    }

    fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// The entered structure as a layer stack, when it passes every check.
    fn layers(&self) -> Option<Vec<Layer>> {
        self.adequate().then(|| self.input.layers.iter().map(|l| Layer::new(&l.name, l.material.clone(), l.thickness_mm)).collect())
    }
}

/// Values printed in the two sources, for an application to show as cited suggestions.
///
/// **None of these is a default.** Each is "as reproduced from the AASHTO 1993 Guide" by the
/// source named on it, which is not the Guide; the engineer's entered value governs.
pub mod suggestions {
    /// How a suggestion from the lecture notes must be labelled.
    pub const NOTES_SOURCE: &str =
        "Fadhil and Ahmed, Flexible Pavement AASHTO 1993, lecture notes, University of Anbar, as reproduced from the AASHTO 1993 Guide";
    /// How a suggestion from the FHWA manual must be labelled.
    pub const FHWA_SOURCE: &str = "FHWA NHI-05-037, Geotechnical Aspects of Pavements, Appendix C, as reproduced from the AASHTO 1993 Guide";

    /// Functional classification of FHWA Table C-2.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FunctionalClass {
        InterstateAndOtherFreeways,
        PrincipalArterials,
        Collectors,
        Local,
    }

    /// Suggested range of reliability, per cent: FHWA NHI-05-037 Table C-2 "Suggested levels of
    /// reliability for various functional classifications (AASHTO, 1993)" (Notes Table 4.4).
    pub fn reliability_range_percent(class: FunctionalClass, urban: bool) -> (f64, f64) {
        match (class, urban) {
            (FunctionalClass::InterstateAndOtherFreeways, true) => (85.0, 99.9),
            (FunctionalClass::InterstateAndOtherFreeways, false) => (80.0, 99.9),
            (FunctionalClass::PrincipalArterials, true) => (80.0, 99.0),
            (FunctionalClass::PrincipalArterials, false) => (75.0, 95.0),
            (FunctionalClass::Collectors, true) => (80.0, 95.0),
            (FunctionalClass::Collectors, false) => (75.0, 95.0),
            (FunctionalClass::Local, _) => (50.0, 80.0),
        }
    }

    /// FHWA Appendix C, "Reliability": for flexible pavements S0 values "typically range between
    /// 0.35 and 0.50, with a value of 0.45 commonly used for design". (low, high, commonly used)
    pub const S0_FLEXIBLE_FHWA: (f64, f64, f64) = (0.35, 0.50, 0.45);

    /// FHWA Appendix C, "Serviceability": a typical initial serviceability for flexible
    /// pavements is 4.2, and a terminal serviceability of 2.5 or higher is recommended for major
    /// highways. (initial, terminal)
    pub const SERVICEABILITY_FLEXIBLE_FHWA: (f64, f64) = (4.2, 2.5);

    /// Notes, "Structural Number (SN)": average layer coefficients of the materials of the AASHO
    /// Road Test: asphalt concrete surface course, crushed stone base course, sandy gravel
    /// subbase.
    pub const ROAD_TEST_LAYER_COEFFICIENTS: (f64, f64, f64) = (0.44, 0.14, 0.11);

    /// Notes Equation 4.17: layer coefficient of a granular base from its resilient modulus in
    /// psi, `a2 = 0.249·log10(EBS) − 0.977`.
    pub fn granular_base_coefficient(modulus_psi: f64) -> f64 {
        0.249 * modulus_psi.log10() - 0.977
    }

    /// Notes Equation 4.18: layer coefficient of a granular subbase from its resilient modulus
    /// in psi, `a3 = 0.227·log10(ESB) − 0.839`.
    pub fn granular_subbase_coefficient(modulus_psi: f64) -> f64 {
        0.227 * modulus_psi.log10() - 0.839
    }

    /// Quality of drainage, rows of Notes Table 4.6.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DrainageQuality {
        Excellent,
        Good,
        Fair,
        Poor,
        VeryPoor,
    }

    /// Per cent of time the pavement structure is exposed to moisture levels approaching
    /// saturation, columns of Notes Table 4.6.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MoistureExposure {
        LessThan1Percent,
        From1To5Percent,
        From5To25Percent,
        GreaterThan25Percent,
    }

    /// Notes Table 4.6 "Recommended mi values for modifying structural layer coefficients of
    /// untreated base and subbase materials in flexible pavements": the range printed, as
    /// (from, to). The last column prints a single value. The table is a scan without decimal
    /// points ("1 40-1 35"), which are restored here.
    pub fn drainage_coefficient_range(quality: DrainageQuality, exposure: MoistureExposure) -> (f64, f64) {
        const TABLE: [[(f64, f64); 4]; 5] = [
            [(1.40, 1.35), (1.35, 1.30), (1.30, 1.20), (1.20, 1.20)],
            [(1.35, 1.25), (1.25, 1.15), (1.15, 1.00), (1.00, 1.00)],
            [(1.25, 1.15), (1.15, 1.05), (1.00, 0.80), (0.80, 0.80)],
            [(1.15, 1.05), (1.05, 0.80), (0.80, 0.60), (0.60, 0.60)],
            [(1.05, 0.95), (0.95, 0.75), (0.75, 0.40), (0.40, 0.40)],
        ];
        TABLE[quality as usize][exposure as usize]
    }
}
