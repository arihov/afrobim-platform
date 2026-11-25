// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! South African design basis: design traffic, traffic classes, road categories, design
//! reliability, climatic class and subgrade preparation, as set out in SAPEM Chapter 10 and the
//! TRH4 values it reproduces.
//!
//! # Documents actually read
//!
//! | Document | Edition | Obtained from | Fetched |
//! |---|---|---|---|
//! | SANRAL, *South African Pavement Engineering Manual*, Chapter 10 "Pavement Design" | Second edition, 2014 (title on the flip-book copy: "SAPEM-Chapter-10-2nd-edition-2014", 140 pages) | `online.flipbuilder.com/jaru/pkqe/` (a third-party copy; its pages were read as text and the cited tables were checked against the rendered page images) | 2026-11-15 |
//! | Theyse and Muthen, "Pavement analysis and design software (PADS) based on the South African mechanistic-empirical design method", 2000 South African Transport Conference | 2000 | `repository.up.ac.za/bitstream/handle/2263/8322/75 Theyse.pdf` | 2026-11-15 |
//!
//! **TRH4 itself (Draft TRH4, 1996, "Structural design of flexible pavements for inter-urban and
//! rural roads", Committee of Land Transport Officials, Pretoria) was not obtained as a document
//! whose tables could be read.** The official copy at `transport.gov.za` could not be fetched
//! from the build environment (its server fails certificate validation through the egress
//! proxy, and the Internet Archive copy was refused). A third-party OCR text of the draft was
//! found, but its table pages, including every sheet of the pavement design catalogue, are
//! unreadable in that text, and no page images were obtainable. Nothing is taken from that OCR
//! text. **The TRH4 pavement design catalogue is therefore not implemented**, no layer stack is
//! returned, and the material code definitions of TRH14 and the TRH4 subgrade and selected
//! layer tables (TRH4 Tables 13, 16, 22) are not embedded.
//!
//! SAPEM Chapter 10 states that its traffic classes (its Table 11) are "the TRH4 traffic
//! classes" and that its road categories are those of TRH4 (and TRH12 and cncPAVE), and Table
//! 39 is headed "after TRH4, 1996". Those values are used here with SAPEM as the cited source.
//!
//! # What this covers
//!
//! | Step | Here | Source |
//! |---|---|---|
//! | Road category, reliability, design period, paved shoulder | [`RoadCategory`] | SAPEM Ch. 10 Table 2 (p. 8), section 3.2 (pp. 9, 10), Table 6 (p. 24) |
//! | Load equivalency `LEF = (P/80)^n`, and for a load interval | [`load_equivalency`], [`load_equivalency_interval`] | Equations 1 and 2 (pp. 30, 32) |
//! | Damage exponent by base and subbase | [`DamageExponent`] | Table 10 (p. 30), "(TRH4)" |
//! | E80 per heavy vehicle, weighted average | [`weighted_e80_per_hv`] | Figure 20 (p. 34) |
//! | Lane distribution factors | [`lane_distribution_factor`] | Table 13 (p. 39) |
//! | AADE growth, projection, cumulative equivalent traffic | [`combined_growth_rate`], [`project_aade`], [`cumulative_equivalent_traffic`] | Equations 13, 14, 15 (pp. 44, 45) |
//! | Traffic class ES0.003 to ES100 | [`TrafficClass`] | Table 11 (p. 34) |
//! | Climatic class from the Weinert N value or Thornthwaite index | [`ClimaticRegion`] | Tables 39 (p. 90) and 5 (p. 22) |
//! | Subgrade action by design CBR | [`subgrade_action`] | Table 4 (p. 22) |
//!
//! Page numbers are the printed page numbers of the chapter (the flip-book page is the printed
//! page plus 11).
//!
//! # Two definitions of a standard axle: do not move a number between methods
//!
//! In South Africa the standard axle is an 80 kN dual-wheel single axle, and an equivalent
//! standard axle (E80, ESA) is found with the power law of Equation 1 with an exponent n of 4
//! or 4.2, or from Table 10 for the pavement type (SAPEM p. 30). The other methods in this crate
//! define their own: the MoWT manual by its Equation 2 and Tables 2.2 and 2.3, AASHTO 1993 by
//! 18-kip ESALs with load equivalency factors that depend on axle type and structural number,
//! and the DMRB by wear factors per vehicle class. The same traffic gives a different number
//! under each. The traffic computed here is in E80 as SAPEM defines it, and **nothing in this
//! module converts it to or from another method's axles**. If the engineer wants to enter a
//! number from another method, it is entered as an input and the result says so.
//!
//! # What stays an input
//!
//! The base year heavy vehicle volume per lane and the E80 per heavy vehicle (or the daily E80
//! per lane directly), the two growth rates, the years from survey to opening, the structural
//! design period, the climatic class (or the Weinert N value or Thornthwaite index; no map is
//! embedded), the design CBR of the subgrade and the choice of base type and materials.
//!
//! # What it does not do
//!
//! - **The catalogue of structures** (TRH4 section 8.4 and the sheets at its pages 97 to 101):
//!   not readable, see above. [`DesignResult::layers`] is always `None`.
//! - **The South African mechanistic design method (SAMDM / SAPDM)** of SAPEM section 7.1: it
//!   needs transfer functions and a layered elastic analysis and is not implemented.
//! - The pavement number method, DCP-DSN design, structural number method, rehabilitation
//!   design, rigid and block pavements, adjustments for seasonal and exceptional traffic
//!   (SAPEM Tables 14, 15), E80/HV and growth tables (SAPEM Tables 17, 18, which are therefore
//!   inputs here).
//!
//! # Readings and slips
//!
//! - Table 11 prints the classes as ranges (`0.003 - 0.01`) without saying which end belongs
//!   to the class. The catalogue designs carry the upper value of the class, so a traffic that
//!   equals a limit exactly is placed in the class whose upper value it is, and the result says
//!   when this has happened.
//! - Table 39 prints the Weinert N ranges as "< 2", "2 to 5" and "> 5"; Table 5 prints
//!   Thornthwaite ranges that share their end points (for example "-40 to -20" and "-20 to 0").
//!   A value exactly on a shared limit takes the drier class and a warning is given.
//! - Table 4 prints "7 to 15" and "3 to 7", so a CBR of exactly 7 is in both rows; it takes the
//!   weaker row, which is this crate's choice and is flagged. 3 and 15 are not ambiguous
//!   ("< 3" and "> 15" exclude them).
//! - Worked example of Table 22 (p. 52): the printed 18 748 635 E80 is Equation 15 evaluated
//!   with the AADE at opening rounded to 1171 (1023 x 1.07^2 is 1171.23); with the unrounded
//!   AADE the result is 18 752 360. Both are 18 million to the printed precision.
//! - Figure 20 prints weighted E80/HV of 0.63, 0.88 and 0.59 summing to 2.09; the unrounded
//!   terms are 0.625, 0.875 and 0.5875 and the unrounded total is 2.0875.
//! - Page 32 of the chapter says a 100 kN axle is "equivalent to 2.4 single axle loads"; the
//!   figure is (100/80)^4 = 2.44.

use super::{num, DesignError, DesignMethod, DesignResult};
use crate::model::Layer;

/// The document that governs the values, for calculation lines.
pub const SOURCE: &str = "SANRAL South African Pavement Engineering Manual, Chapter 10 Pavement Design, second edition 2014 (SAPEM Ch. 10)";

const TRH4_NOTE: &str = "TRH4 is a 1996 draft (Draft TRH4, Committee of Land Transport Officials, Pretoria, 1996); its traffic classes, road categories and Weinert climatic classes are taken from SAPEM Ch. 10, which reproduces them, because the TRH4 document itself could not be read in this build";

// ---------------------------------------------------------------------------------------------
// Road categories

/// The four road categories (SAPEM Ch. 10 Table 2, p. 8; originally TRH4, TRH12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoadCategory {
    A,
    B,
    C,
    D,
}

impl RoadCategory {
    pub fn all() -> [RoadCategory; 4] {
        [RoadCategory::A, RoadCategory::B, RoadCategory::C, RoadCategory::D]
    }

    /// Description row of Table 2.
    pub fn description(self) -> &'static str {
        match self {
            RoadCategory::A => "Major inter-urban freeways and major rural roads",
            RoadCategory::B => "Inter-urban collectors and rural roads",
            RoadCategory::C => "Lightly trafficked rural roads, strategic roads",
            RoadCategory::D => "Rural access roads",
        }
    }

    /// Importance row of Table 2.
    pub fn importance(self) -> &'static str {
        match self {
            RoadCategory::A => "Very important",
            RoadCategory::B => "Important",
            RoadCategory::C | RoadCategory::D => "Less important",
        }
    }

    /// Level of service row of Table 2.
    pub fn level_of_service(self) -> &'static str {
        match self {
            RoadCategory::A => "Very high",
            RoadCategory::B => "High",
            RoadCategory::C | RoadCategory::D => "Moderate",
        }
    }

    /// Approximate design reliability, per cent (Table 2). Table 2 note 1: for Category D the
    /// 50 per cent "essentially implies designing for an average situation".
    pub fn design_reliability_percent(self) -> f64 {
        match self {
            RoadCategory::A => 95.0,
            RoadCategory::B => 90.0,
            RoadCategory::C => 80.0,
            RoadCategory::D => 50.0,
        }
    }

    /// Length of road exceeding the terminal distress condition at the end of the structural
    /// design life, per cent (Table 2).
    pub fn length_exceeding_terminal_percent(self) -> f64 {
        match self {
            RoadCategory::A => 5.0,
            RoadCategory::B => 10.0,
            RoadCategory::C => 20.0,
            RoadCategory::D => 50.0,
        }
    }

    /// The "typical pavement class" row of Table 2, as printed. A typical range, not a limit.
    pub fn typical_class_text(self) -> &'static str {
        match self {
            RoadCategory::A => "ES10 - ES100",
            RoadCategory::B => "ES1 - ES10",
            RoadCategory::C => "< ES0.03 - ES3",
            RoadCategory::D => "ES0.003 - ES1",
        }
    }

    /// The typical class range of Table 2 as (lowest, highest) classes. For Category C the
    /// printed lower end is "< ES0.03", read as ES0.03 and below, so the lowest class is
    /// `ES0.003`.
    pub fn typical_class_range(self) -> (TrafficClass, TrafficClass) {
        match self {
            RoadCategory::A => (TrafficClass::Es10, TrafficClass::Es100),
            RoadCategory::B => (TrafficClass::Es1, TrafficClass::Es10),
            RoadCategory::C => (TrafficClass::Es0_003, TrafficClass::Es3),
            RoadCategory::D => (TrafficClass::Es0_003, TrafficClass::Es1),
        }
    }

    /// Recommended structural design period in years, with the range the text allows
    /// (section 3.2, pp. 9 and 10): Category A 25 (15 to 30), Category B 20 (15 to 25),
    /// Category C 10, or 20 where structural rehabilitation will be difficult. Category D gets
    /// no number: the text says only that a short period is advantageous.
    pub fn design_period_guidance(self) -> &'static str {
        match self {
            RoadCategory::A => "25 years, ranging between 15 and 30 (SAPEM Ch. 10 section 3.2)",
            RoadCategory::B => "20 years, ranging between 15 and 25 (SAPEM Ch. 10 section 3.2)",
            RoadCategory::C => "a shortened 10 years where finances dictate, 20 years where structural rehabilitation will be difficult (SAPEM Ch. 10 section 3.2)",
            RoadCategory::D => "a short period (no number given; SAPEM Ch. 10 section 3.2)",
        }
    }

    /// Recommended minimum paved shoulder width, mm (Table 6, p. 24). `None` for Category D,
    /// for which the table gives no width.
    pub fn minimum_paved_shoulder_mm(self) -> Option<f64> {
        match self {
            RoadCategory::A => Some(1200.0),
            RoadCategory::B => Some(1000.0),
            RoadCategory::C => Some(800.0),
            RoadCategory::D => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Traffic classes

/// The ten TRH4 pavement classes (SAPEM Ch. 10 Table 11, p. 34).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TrafficClass {
    Es0_003,
    Es0_01,
    Es0_03,
    Es0_1,
    Es0_3,
    Es1,
    Es3,
    Es10,
    Es30,
    Es100,
}

impl TrafficClass {
    pub fn all() -> [TrafficClass; 10] {
        use TrafficClass::*;
        [Es0_003, Es0_01, Es0_03, Es0_1, Es0_3, Es1, Es3, Es10, Es30, Es100]
    }

    /// The class name as printed.
    pub fn name(self) -> &'static str {
        match self {
            TrafficClass::Es0_003 => "ES0.003",
            TrafficClass::Es0_01 => "ES0.01",
            TrafficClass::Es0_03 => "ES0.03",
            TrafficClass::Es0_1 => "ES0.1",
            TrafficClass::Es0_3 => "ES0.3",
            TrafficClass::Es1 => "ES1",
            TrafficClass::Es3 => "ES3",
            TrafficClass::Es10 => "ES10",
            TrafficClass::Es30 => "ES30",
            TrafficClass::Es100 => "ES100",
        }
    }

    /// Upper end of the structural capacity range, million E80 per lane (Table 11).
    pub fn upper_million_e80(self) -> f64 {
        match self {
            TrafficClass::Es0_003 => 0.003,
            TrafficClass::Es0_01 => 0.01,
            TrafficClass::Es0_03 => 0.03,
            TrafficClass::Es0_1 => 0.1,
            TrafficClass::Es0_3 => 0.3,
            TrafficClass::Es1 => 1.0,
            TrafficClass::Es3 => 3.0,
            TrafficClass::Es10 => 10.0,
            TrafficClass::Es30 => 30.0,
            TrafficClass::Es100 => 100.0,
        }
    }

    /// Lower end of the range, million E80 per lane. `None` for ES0.003, printed "< 0.003".
    pub fn lower_million_e80(self) -> Option<f64> {
        let all = TrafficClass::all();
        let i = all.iter().position(|c| *c == self)?;
        if i == 0 {
            None
        } else {
            Some(all[i - 1].upper_million_e80())
        }
    }

    /// The structural capacity column as printed.
    pub fn capacity_text(self) -> &'static str {
        match self {
            TrafficClass::Es0_003 => "< 0.003",
            TrafficClass::Es0_01 => "0.003 - 0.01",
            TrafficClass::Es0_03 => "0.01 - 0.03",
            TrafficClass::Es0_1 => "0.03 - 0.1",
            TrafficClass::Es0_3 => "0.1 - 0.3",
            TrafficClass::Es1 => "0.3 - 1",
            TrafficClass::Es3 => "1 - 3",
            TrafficClass::Es10 => "3 - 10",
            TrafficClass::Es30 => "10 - 30",
            TrafficClass::Es100 => "30 - 100",
        }
    }

    /// The approximate vehicles-per-day column as printed (Table 11 notes 3 and 4: per lane
    /// per day for the lighter classes; for ES10 to ES100 the total per direction with 20 per
    /// cent heavy vehicles and 2 E80 per heavy vehicle).
    pub fn approximate_vpd_text(self) -> &'static str {
        match self {
            TrafficClass::Es0_003 => "< 3",
            TrafficClass::Es0_01 => "3 - 10",
            TrafficClass::Es0_03 => "10 - 20",
            TrafficClass::Es0_1 => "20 - 75",
            TrafficClass::Es0_3 => "75 - 220",
            TrafficClass::Es1 => "220 - 700",
            TrafficClass::Es3 => "> 700",
            TrafficClass::Es10 => "> 700 (per direction)",
            TrafficClass::Es30 => "> 2200 (per direction)",
            TrafficClass::Es100 => "> 6500 (per direction)",
        }
    }

    /// Description column of Table 11.
    pub fn description(self) -> &'static str {
        use TrafficClass::*;
        match self {
            Es0_003 | Es0_01 | Es0_03 | Es0_1 | Es0_3 => "Very lightly trafficked roads with very few heavy vehicles. Includes roads transitioning from gravel to paved roads and may incorporate semi-permanent and/or all weather surfacing layers",
            Es1 => "Lightly trafficked roads carrying mainly cars, light delivery and agricultural vehicles with very few heavy vehicles",
            Es3 => "Medium traffic volume roads with a few heavy vehicles",
            Es10 => "High traffic volume roads or roads with many heavy vehicles",
            Es30 | Es100 => "Very high traffic volume roads with a high proportion of fully laden heavy vehicles",
        }
    }

    /// The class that carries `million_e80`, the cumulative design traffic per lane in million
    /// E80. A value equal to a class's upper value is in that class (the catalogue designs are
    /// for the upper value). `None` above 100 million, which no class covers.
    pub fn from_million_e80(million_e80: f64) -> Option<TrafficClass> {
        TrafficClass::all().into_iter().find(|c| million_e80 <= c.upper_million_e80())
    }

    /// Whether the value lies exactly on the upper limit of its class.
    pub fn is_on_limit(million_e80: f64) -> bool {
        TrafficClass::all().iter().any(|c| (million_e80 - c.upper_million_e80()).abs() <= 1e-12 * c.upper_million_e80())
    }
}

// ---------------------------------------------------------------------------------------------
// Load equivalency

/// Damage exponent n of Equation 1: "4.2 from the AASHO Road Test, although a value of 4 is
/// often used" (p. 30).
pub const EXPONENT_AASHO: f64 = 4.2;
/// The commonly used value of 4 (p. 30).
pub const EXPONENT_FOURTH_POWER: f64 = 4.0;
/// The standard axle load, kN (80 kN dual-wheel single axle; p. 30).
pub const STANDARD_AXLE_KN: f64 = 80.0;

/// Equation 1 (p. 30): `LEF = (P / 80)^n`, in E80, for an axle load `p_kn` in kN.
pub fn load_equivalency(p_kn: f64, n: f64) -> Result<f64, DesignError> {
    if !(p_kn.is_finite() && p_kn >= 0.0) {
        return Err(DesignError::InvalidInput("axle load must be zero or more, in kN".into()));
    }
    if !(n.is_finite() && n > 0.0) {
        return Err(DesignError::InvalidInput("damage exponent must be above zero".into()));
    }
    Ok((p_kn / STANDARD_AXLE_KN).powf(n))
}

/// Equation 2 (p. 32): the load equivalency of a load interval, the mean of the values at its
/// lower and upper limits, `LEF = ((Pl/80)^n + (Pu/80)^n) / 2`.
pub fn load_equivalency_interval(lower_kn: f64, upper_kn: f64, n: f64) -> Result<f64, DesignError> {
    if upper_kn < lower_kn {
        return Err(DesignError::InvalidInput("the upper limit of a load interval must not be below its lower limit".into()));
    }
    Ok((load_equivalency(lower_kn, n)? + load_equivalency(upper_kn, n)?) / 2.0)
}

/// Suggested damage exponents by base and subbase combination (Table 10, p. 30, "(TRH4)").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DamageExponent {
    GranularGranular,
    GranularCemented,
    CementedGranularPreCracked,
    CementedGranularPostCracked,
    CementedCementedPreCracked,
    CementedCementedPostCracked,
    BsmGranular,
    HotMixAsphaltCemented,
    Concrete,
}

impl DamageExponent {
    pub fn all() -> [DamageExponent; 9] {
        use DamageExponent::*;
        [GranularGranular, GranularCemented, CementedGranularPreCracked, CementedGranularPostCracked, CementedCementedPreCracked, CementedCementedPostCracked, BsmGranular, HotMixAsphaltCemented, Concrete]
    }

    /// The row as printed.
    pub fn name(self) -> &'static str {
        use DamageExponent::*;
        match self {
            GranularGranular => "Granular/granular",
            GranularCemented => "Granular/cemented",
            CementedGranularPreCracked => "Cemented/granular, pre-cracked",
            CementedGranularPostCracked => "Cemented/granular, post-cracked",
            CementedCementedPreCracked => "Cemented/cemented, pre-cracked",
            CementedCementedPostCracked => "Cemented/cemented, post-cracked",
            BsmGranular => "BSM/granular",
            HotMixAsphaltCemented => "Hot mix asphalt/cemented",
            Concrete => "Concrete",
        }
    }

    /// Range of values (low, high); `None` for concrete, for which only a recommended value is
    /// printed.
    pub fn range(self) -> Option<(f64, f64)> {
        use DamageExponent::*;
        match self {
            GranularGranular => Some((3.0, 6.0)),
            GranularCemented => Some((2.0, 4.0)),
            CementedGranularPreCracked => Some((4.0, 10.0)),
            CementedGranularPostCracked => Some((3.0, 6.0)),
            CementedCementedPreCracked => Some((3.0, 6.0)),
            CementedCementedPostCracked => Some((2.0, 5.0)),
            BsmGranular => Some((2.0, 6.0)),
            HotMixAsphaltCemented => Some((2.0, 5.0)),
            Concrete => None,
        }
    }

    /// The recommended value. The cemented/cemented rows print "(4 - 5)", a recommended range:
    /// the lower value is returned here, and [`DamageExponent::recommended_text`] gives the
    /// printed form.
    pub fn recommended(self) -> f64 {
        use DamageExponent::*;
        match self {
            GranularGranular => 4.0,
            GranularCemented => 3.0,
            CementedGranularPreCracked | CementedGranularPostCracked => 5.0,
            CementedCementedPreCracked | CementedCementedPostCracked => 4.0,
            BsmGranular => 4.0,
            HotMixAsphaltCemented => 4.0,
            Concrete => 4.5,
        }
    }

    pub fn recommended_text(self) -> &'static str {
        use DamageExponent::*;
        match self {
            GranularGranular | BsmGranular | HotMixAsphaltCemented => "4",
            GranularCemented => "3",
            CementedGranularPreCracked | CementedGranularPostCracked => "5",
            CementedCementedPreCracked | CementedCementedPostCracked => "4 - 5",
            Concrete => "4.5",
        }
    }
}

/// Figure 20 (p. 34): the weighted average E80 per heavy vehicle of vehicle types, from
/// (number of vehicles, E80 per vehicle) pairs. The E80 per vehicle is the sum of its axles'
/// E80.
pub fn weighted_e80_per_hv(types: &[(f64, f64)]) -> Result<f64, DesignError> {
    let total: f64 = types.iter().map(|(n, _)| n).sum();
    if types.is_empty() || !(total > 0.0) || types.iter().any(|(n, e)| !(n.is_finite() && *n >= 0.0 && e.is_finite() && *e >= 0.0)) {
        return Err(DesignError::InvalidInput("vehicle types need non-negative numbers of vehicles and E80 per vehicle, and at least one vehicle".into()));
    }
    Ok(types.iter().map(|(n, e)| e * n / total).sum())
}

// ---------------------------------------------------------------------------------------------
// Lane distribution

/// Which lane of a carriageway (Table 13, p. 39). "Lane 1 is the outer or slow lane."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    /// The surfaced slow shoulder: the same pavement design is used as the adjacent lane, and
    /// the table gives it the factor of lane 1.
    SurfacedSlowShoulder,
    Lane1,
    Lane2,
    Lane3,
}

/// Table 13 (p. 39): the E80 lane distribution factor Be, for the total number of lanes in both
/// directions (2, 4 or 6, dual carriageway above two). `None` where the table has a dash (a
/// lane that does not exist) or the number of lanes is not in the table.
pub fn lane_distribution_factor(total_lanes_both_directions: u32, lane: Lane) -> Option<f64> {
    match (total_lanes_both_directions, lane) {
        (2, Lane::SurfacedSlowShoulder | Lane::Lane1) => Some(1.00),
        (4, Lane::SurfacedSlowShoulder | Lane::Lane1) => Some(0.95),
        (4, Lane::Lane2) => Some(0.30),
        (6, Lane::SurfacedSlowShoulder | Lane::Lane1) => Some(0.70),
        (6, Lane::Lane2) => Some(0.60),
        (6, Lane::Lane3) => Some(0.25),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Growth and cumulative equivalent traffic

/// Equation 13 (p. 44): the yearly AADE growth rate as a decimal fraction,
/// `i = (1 + h)(1 + v) - 1`, from the heavy vehicle volume growth `h` and the E80/HV growth `v`,
/// both as decimal fractions.
pub fn combined_growth_rate(h: f64, v: f64) -> Result<f64, DesignError> {
    if !(h.is_finite() && v.is_finite() && h > -1.0 && v > -1.0) {
        return Err(DesignError::InvalidInput("growth rates must be decimal fractions above -1".into()));
    }
    Ok((1.0 + h) * (1.0 + v) - 1.0)
}

/// Equation 14 (p. 45): `AADE_O = AADE_S (1 + i)^n`, the AADE at opening from the AADE at the
/// survey, `n` years earlier.
pub fn project_aade(aade_survey: f64, i: f64, years: f64) -> Result<f64, DesignError> {
    if !(aade_survey.is_finite() && aade_survey >= 0.0 && i.is_finite() && i > -1.0 && years.is_finite() && years >= 0.0) {
        return Err(DesignError::InvalidInput("AADE and years must be zero or more and the growth rate above -1".into()));
    }
    Ok(aade_survey * (1.0 + i).powf(years))
}

/// Equation 15 (p. 45): the cumulative equivalent traffic in E80,
/// `ET = 365 · AADE_O · (1 + i) · ((1 + i)^y - 1) / i`, over `years` from opening. For a growth
/// rate of exactly zero the limit `365 · AADE_O · y` is used.
pub fn cumulative_equivalent_traffic(aade_opening: f64, i: f64, years: f64) -> Result<f64, DesignError> {
    if !(aade_opening.is_finite() && aade_opening >= 0.0 && i.is_finite() && i > -1.0 && years.is_finite() && years > 0.0) {
        return Err(DesignError::InvalidInput("AADE must be zero or more, the structural design period above zero and the growth rate above -1".into()));
    }
    if i == 0.0 {
        return Ok(365.0 * aade_opening * years);
    }
    Ok(365.0 * aade_opening * (1.0 + i) * ((1.0 + i).powf(years) - 1.0) / i)
}

// ---------------------------------------------------------------------------------------------
// Climate and subgrade

/// The moisture classes of Table 39 (p. 90, "Climate and Weinert N values, after TRH4, 1996")
/// and Table 5 (p. 22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClimaticRegion {
    Wet,
    Moderate,
    Dry,
}

impl ClimaticRegion {
    pub fn name(self) -> &'static str {
        match self {
            ClimaticRegion::Wet => "wet",
            ClimaticRegion::Moderate => "moderate",
            ClimaticRegion::Dry => "dry",
        }
    }

    /// The class of a Weinert N value, Table 39: wet below 2, moderate 2 to 5, dry above 5.
    /// The second element is true for a value exactly on a limit (2 or 5), which the table
    /// puts in the moderate row ("2 to 5").
    pub fn from_weinert_n(n: f64) -> Result<(ClimaticRegion, bool), DesignError> {
        if !n.is_finite() || n < 0.0 {
            return Err(DesignError::InvalidInput("Weinert N must be zero or more".into()));
        }
        let on_limit = n == 2.0 || n == 5.0;
        let c = if n < 2.0 {
            ClimaticRegion::Wet
        } else if n <= 5.0 {
            ClimaticRegion::Moderate
        } else {
            ClimaticRegion::Dry
        };
        Ok((c, on_limit))
    }

    /// The class of a Thornthwaite moisture index, Table 5: below -40 arid and -40 to -20
    /// semi-arid (both dry), -20 to 0 dry sub-humid and 0 to 20 moist sub-humid (both
    /// moderate), above 20 humid (wet). The second element is true for a value exactly on a
    /// shared limit (-40, -20, 0, 20), which takes the drier class.
    pub fn from_thornthwaite(index: f64) -> Result<(ClimaticRegion, bool), DesignError> {
        if !index.is_finite() {
            return Err(DesignError::InvalidInput("Thornthwaite index must be a number".into()));
        }
        let on_limit = [-40.0, -20.0, 0.0, 20.0].contains(&index);
        let c = if index <= -20.0 {
            ClimaticRegion::Dry
        } else if index <= 20.0 {
            ClimaticRegion::Moderate
        } else {
            ClimaticRegion::Wet
        };
        Ok((c, on_limit))
    }
}

/// How the climate is entered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Climate {
    /// The class entered directly.
    Region(ClimaticRegion),
    /// The Weinert N value of the site (Table 39).
    WeinertN(f64),
    /// The Thornthwaite moisture index of the site (Table 5).
    Thornthwaite(f64),
}

/// The action for an in situ subgrade of a design CBR (Table 4, p. 22, "CBR at 93 per cent
/// modified AASHTO density"). Returns the printed row label and its actions, and whether the
/// CBR is exactly 7, the one value printed in two rows ("7 to 15" and "3 to 7"), in which case
/// the weaker row is returned. 3 is only in "3 to 7" ("< 3" excludes it) and 15 only in
/// "7 to 15" ("> 15" excludes it).
pub fn subgrade_action(cbr_percent: f64) -> Result<(&'static str, &'static [&'static str], bool), DesignError> {
    if !(cbr_percent.is_finite() && cbr_percent > 0.0) {
        return Err(DesignError::InvalidInput("CBR must be above zero".into()));
    }
    let on_limit = cbr_percent == 7.0;
    Ok(if cbr_percent > 15.0 {
        ("> 15", &["In situ subgrade of a G7 standard and of sufficient strength to support structural layers.", "Rip and recompact to 93% of modified (mod.) AASHTO density."][..], on_limit)
    } else if cbr_percent > 7.0 {
        ("7 to 15", &["In situ subgrade of a G9 standard.", "Rip and recompact in situ material to 93% of mod. AASHTO density.", "Import a 150 mm thick layer of G7 standard material."][..], on_limit)
    } else if cbr_percent >= 3.0 {
        ("3 to 7", &["In situ subgrade of a G10 standard.", "Rip and recompact in situ material to 93% of mod. AASHTO density.", "Import a 150 mm thick layer of G9 standard material.", "Import a second 150 mm thick layer of G7 standard material."][..], on_limit)
    } else {
        ("< 3", &["Chemical/mechanical stabilization.", "Or, remove and import new material.", "Or, add additional cover to place poor quality in situ material below material depth."][..], on_limit)
    })
}

// ---------------------------------------------------------------------------------------------
// The design

/// The base year loading, entered one of two ways.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BaseLoading {
    /// Daily E80 per design lane at the survey date directly (the ADE of SAPEM section 4.3,
    /// already adjusted for lane distribution and for a full year).
    DailyE80PerLane(f64),
    /// Heavy vehicles per day in the design lane and the E80 per heavy vehicle:
    /// `ADE = E80/HV x HVV / (duration x lanes)` with duration and lanes both 1 (SAPEM section 4.3, p. 37, and the worked example of Table 22).
    HeavyVehicles { hvv_per_lane_per_day: f64, e80_per_hv: f64 },
}

impl BaseLoading {
    pub fn ade(self) -> f64 {
        match self {
            BaseLoading::DailyE80PerLane(a) => a,
            BaseLoading::HeavyVehicles { hvv_per_lane_per_day, e80_per_hv } => hvv_per_lane_per_day * e80_per_hv,
        }
    }
}

/// The input of the South African design basis.
#[derive(Debug, Clone, PartialEq)]
pub struct Trh4Input {
    pub category: RoadCategory,
    /// Base year loading at the traffic survey date, in E80 as SAPEM defines it. Not a number
    /// from another method's axles unless the engineer has chosen to enter it so.
    pub loading: BaseLoading,
    /// Heavy vehicle volume growth rate per year as a decimal fraction (0.05 for 5 per cent).
    pub hv_growth: f64,
    /// E80 per heavy vehicle growth rate per year as a decimal fraction.
    pub e80_per_hv_growth: f64,
    /// Years from the traffic survey to the opening of the road.
    pub years_survey_to_opening: f64,
    /// Structural design period, years, from opening.
    pub structural_design_period_years: f64,
    pub climate: Climate,
    /// Design CBR of the in situ subgrade, per cent at 93 per cent modified AASHTO density.
    pub subgrade_design_cbr_percent: Option<f64>,
}

/// The result: the design traffic and what follows from it. No structure is given.
#[derive(Debug, Clone, PartialEq)]
pub struct Trh4Design {
    pub input: Trh4Input,
    pub ade: f64,
    pub combined_growth: f64,
    pub aade_at_opening: f64,
    /// Cumulative equivalent traffic over the design period, E80 per lane.
    pub design_traffic_e80: f64,
    /// `None` when the traffic is above 100 million E80, which no class covers.
    pub traffic_class: Option<TrafficClass>,
    pub climate: ClimaticRegion,
    warnings: Vec<String>,
    subgrade_lines: Vec<String>,
}

/// Runs the design traffic and classification.
pub fn design(input: &Trh4Input) -> Result<Trh4Design, DesignError> {
    let ade = input.loading.ade();
    if let BaseLoading::HeavyVehicles { hvv_per_lane_per_day, e80_per_hv } = input.loading {
        if !(hvv_per_lane_per_day.is_finite() && hvv_per_lane_per_day >= 0.0 && e80_per_hv.is_finite() && e80_per_hv >= 0.0) {
            return Err(DesignError::InvalidInput("heavy vehicles per day and E80 per heavy vehicle must be zero or more".into()));
        }
    }
    if !(ade.is_finite() && ade >= 0.0) {
        return Err(DesignError::InvalidInput("daily E80 per lane must be zero or more".into()));
    }
    let i = combined_growth_rate(input.hv_growth, input.e80_per_hv_growth)?;
    let aade_o = project_aade(ade, i, input.years_survey_to_opening)?;
    let et = cumulative_equivalent_traffic(aade_o, i, input.structural_design_period_years)?;
    let mut warnings = Vec::new();

    let traffic_class = TrafficClass::from_million_e80(et / 1.0e6);
    match traffic_class {
        None => warnings.push(format!("The design traffic of {} million E80 per lane is above ES100 (100 million, SAPEM Ch. 10 Table 11): no TRH4 traffic class covers it and the catalogue does not apply.", num(et / 1.0e6, 1))),
        Some(c) => {
            if TrafficClass::is_on_limit(et / 1.0e6) {
                warnings.push(format!("The design traffic equals the upper value of {} exactly; it is placed in {} (the catalogue designs carry the upper value).", c.name(), c.name()));
            }
            let (lo, hi) = input.category.typical_class_range();
            if c < lo || c > hi {
                warnings.push(format!(
                    "Class {} is outside the typical pavement class of Category {:?} ({}, SAPEM Ch. 10 Table 2). Table 2 gives typical ranges, not limits; check the road category and the design traffic.",
                    c.name(),
                    input.category,
                    input.category.typical_class_text()
                ));
            }
        }
    }

    let (climate, climate_on_limit) = match input.climate {
        Climate::Region(r) => (r, false),
        Climate::WeinertN(n) => ClimaticRegion::from_weinert_n(n)?,
        Climate::Thornthwaite(x) => ClimaticRegion::from_thornthwaite(x)?,
    };
    if climate_on_limit {
        warnings.push("The climate index is exactly on a limit shared by two printed ranges; the drier class is taken (the tables do not say which applies). Confirm the class.".to_string());
    }

    let mut subgrade_lines = Vec::new();
    if let Some(cbr) = input.subgrade_design_cbr_percent {
        let (row, actions, on_limit) = subgrade_action(cbr)?;
        subgrade_lines.push(format!("Subgrade design CBR {} % at 93 % mod. AASHTO density: row \"{row}\" of SAPEM Ch. 10 Table 4 (p. 22).", num(cbr, 1)));
        for a in actions {
            subgrade_lines.push(format!("  {a}"));
        }
        if on_limit {
            warnings.push("The design CBR of exactly 7 is printed in two rows of Table 4 (7 to 15 and 3 to 7); the weaker row is shown (the table does not say which applies).".to_string());
        }
        if cbr < 3.0 {
            warnings.push("Subgrade CBR below 3: Table 4 offers no in situ option; the subgrade must be stabilised, replaced or covered. No catalogue lookup is possible.".to_string());
        }
    }

    warnings.push("The TRH4 pavement design catalogue is not implemented (its sheets could not be read in this build): no layer stack is returned. Select the structure from the catalogue yourself and enter it where the application asks for layers.".to_string());
    warnings.push("The South African mechanistic design method (SAMDM) of SAPEM section 7.1 is not implemented.".to_string());
    warnings.push("Traffic here is in E80 as SAPEM Ch. 10 defines it (80 kN dual-wheel single axle). It is not the same quantity as the MoWT, AASHTO or DMRB design traffic; do not copy a number between methods.".to_string());

    Ok(Trh4Design { input: input.clone(), ade, combined_growth: i, aade_at_opening: aade_o, design_traffic_e80: et, traffic_class, climate, warnings, subgrade_lines })
}

impl DesignResult for Trh4Design {
    fn method(&self) -> DesignMethod {
        DesignMethod::SaTrh4
    }

    fn calculation(&self) -> Vec<String> {
        let inp = &self.input;
        let mut out = vec![
            format!("Design basis: {}.", DesignMethod::SaTrh4.reference()),
            format!("Edition note: {TRH4_NOTE}."),
            format!("Road category {:?}: {} (SAPEM Ch. 10 Table 2, p. 8). Importance: {}; level of service: {}.", inp.category, inp.category.description(), inp.category.importance(), inp.category.level_of_service()),
            format!(
                "Approximate design reliability {} %; length of road exceeding the terminal distress condition at the end of the structural design life {} % (Table 2).",
                num(inp.category.design_reliability_percent(), 0),
                num(inp.category.length_exceeding_terminal_percent(), 0)
            ),
            format!("Structural design period guidance for Category {:?}: {}. Entered: {} years.", inp.category, inp.category.design_period_guidance(), num(inp.structural_design_period_years, 1)),
        ];
        match inp.category.minimum_paved_shoulder_mm() {
            Some(w) => out.push(format!("Recommended minimum paved shoulder {} mm (Table 6, p. 24).", num(w, 0))),
            None => out.push("Table 6 (p. 24) gives no paved shoulder width for Category D.".to_string()),
        }
        out.push("Standard axle: 80 kN dual-wheel single axle; E80 = (P/80)^n with n of 4 or 4.2 (Equation 1, p. 30). Not converted to or from any other method's axles.".to_string());
        match inp.loading {
            BaseLoading::DailyE80PerLane(a) => out.push(format!("ADE at the survey date: {} E80 per lane per day (entered).", num(a, 2))),
            BaseLoading::HeavyVehicles { hvv_per_lane_per_day, e80_per_hv } => out.push(format!("ADE = E80/HV x HVV = {} x {} = {} E80 per lane per day (as in the worked example, Table 22, p. 52).", num(e80_per_hv, 2), num(hvv_per_lane_per_day, 1), num(self.ade, 2))),
        }
        out.push(format!(
            "AADE growth i = (1 + h)(1 + v) - 1 = (1 + {}) x (1 + {}) - 1 = {} (Equation 13, p. 44).",
            num(inp.hv_growth, 4),
            num(inp.e80_per_hv_growth, 4),
            num(self.combined_growth, 4)
        ));
        out.push(format!("AADE at opening = {} x (1 + {})^{} = {} (Equation 14, p. 45).", num(self.ade, 2), num(self.combined_growth, 4), num(inp.years_survey_to_opening, 1), num(self.aade_at_opening, 1)));
        out.push(format!(
            "Cumulative equivalent traffic ET = 365 x AADE x (1 + i) x ((1 + i)^y - 1) / i, y = {} years: {} E80 per lane = {} million E80 (Equation 15, p. 45).",
            num(inp.structural_design_period_years, 1),
            num(self.design_traffic_e80, 0),
            num(self.design_traffic_e80 / 1.0e6, 3)
        ));
        match self.traffic_class {
            Some(c) => out.push(format!(
                "Traffic class {}: structural capacity {} million E80 per lane (Table 11, p. 34). {}.",
                c.name(),
                c.capacity_text(),
                c.description()
            )),
            None => out.push("Traffic class: none; above ES100 (Table 11, p. 34).".to_string()),
        }
        out.push(format!("Climatic class: {} (Table 39, p. 90: wet N < 2, moderate N = 2 to 5, dry N > 5; Table 5, p. 22, for the Thornthwaite index). The class is an input; no map is embedded. Weinert N is not defined in the documents held.", self.climate.name()));
        out.extend(self.subgrade_lines.iter().cloned());
        out.push("Structure: not selected. The TRH4 catalogue (Draft TRH4 1996, section 8.4 and the sheets at its pages 97 to 101) could not be read, so no layer stack is given.".to_string());
        out.extend(self.warnings.iter().map(|w| format!("Note: {w}")));
        out.push("This is a calculation for an engineer to check, not a certified design, and it is not a complete pavement design to TRH4.".to_string());
        out
    }

    fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Always `None`: the TRH4 catalogue could not be read, so no structure is offered.
    fn layers(&self) -> Option<Vec<Layer>> {
        None
    }
}
