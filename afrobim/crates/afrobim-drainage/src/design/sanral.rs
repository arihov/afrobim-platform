//! The South African (SANRAL) design basis for side drains, channels and culverts.
//!
//! # Design basis
//!
//! South African National Roads Agency SOC Ltd (SANRAL), *Drainage Manual*, **sixth edition,
//! September 2013** (fully revised and extended; ISBN 978-0-620-55428-2; PDF "Drainage Manual 6th
//! Edition Sept 2013", 468 PDF pages, with Addendum 1 of 29 May 2013), cited as "SDM" with its
//! printed page numbers (chapter-page, for example 3-18). Worked examples come from the companion
//! *Drainage Manual Application Guide*, September 2013 (ISBN 978-0-620-55429-9, 296 PDF pages),
//! cited as "SAG". Both were fetched from www.nra.co.za (SANRAL's current site, which carries the
//! downloads of www.sanral.co.za); see the README for URLs and dates.
//!
//! Every number in this module names its page, table, figure or equation. Nothing is taken from
//! MoWT or DMRB, and nothing here may be mixed into a MoWT or DMRB calculation: the basis is
//! chosen with [`super::DesignBasis::Sanral`] and a calculation names it on its first line.
//!
//! # What is here
//!
//! * **Flood by the rational method** as SDM Section 3.5.1 gives it: `Q = C·I·A / 3.6` with A in
//!   km² (Eq. 3.8); the run-off coefficient built up from Table 3.7 (rural `C1 = Cs + Cp + Cv` by
//!   the shares of the catchment in each class, urban `C2` by the shares of each land use at a
//!   factor the engineer picks within the printed range, lakes `C3` entered), the dolomite
//!   adjustment (p. 3-19, with the formula as the SAG prints it on p. 3-20), the return period
//!   factor `Ft` of Table 3.8, and the combined coefficient `CT = α·C1T + β·C2 + γ·C3` of SAG
//!   Eq. 3.12; the time of concentration by Kerby (Eq. 3.11, Table 3.10), by the US Soil
//!   Conservation Service formula (Eq. 3.12) and the 10-85 slope (Eq. 3.13), summed over reaches
//!   as p. 3-29 says; the area reduction factor of Eq. 3.10; and the point intensity `P/Tc` with
//!   the 15-minute floor of p. 3-20.
//! * **Design return period** by road class and the 20-year flood, from the piecewise linear
//!   lines of Figure 8.2 (p. 8-7), checked against SAG Example 7.1 (Class 3, Q20 = 85 m³/s gives
//!   15 years).
//! * **Channels**: the freeboard of Table 5.3 and the extra freeboard at bends of Table 5.4
//!   (p. 5-22), the permissible velocities of Table 5.6 for grass covers (p. 5-24), the maximum
//!   side slopes of Table 5.5 (p. 5-23), the minimum slopes of median drains (p. 5-23) and the
//!   velocity limit over concrete linings with joints (p. 5-25). Capacity is the shared Manning
//!   check ([`super::check_channel`]); Manning's equation is SDM Eq. 4.27 (p. 4-14).
//! * **Culverts**: inlet control by the relations of Table 7.2 (p. 7-8), outlet control by
//!   Eq. 7.4 (p. 7-7) with Manning friction, or with Darcy friction as SAG Example 7.1 does it,
//!   the limits `H1/D = 1.2` at the design flood and `min(2D, SBP)` at twice the return period
//!   (Section 8.3.2, p. 8-7), the free-flow freeboard of Figure 8.3 (p. 8-8), minimum sizes
//!   (7.2.2.5, p. 7-4) and the slope and velocity advice of 7.2.2.2 (p. 7-4).
//!
//! # Inputs, not defaults
//!
//! * **Rainfall.** The manual's rainfall routes are a depth-duration-frequency diagram
//!   (Figure 3.8), the Hershfield equation fed by TR102 station data and a thunder-days map
//!   (Eq. 3.9, Figure 3.9) and the Design Rainfall Estimation software. All three are site data.
//!   No South African map, region table or curve is embedded: the engineer enters the **point
//!   rainfall depth in mm for the time of concentration and return period** ([`RainfallInput`])
//!   from whichever source the project uses, and says which.
//! * **Roughness.** SDM gives ks and Manning's n in Figures 4.8 to 4.10, which are graphs on
//!   which a material is matched to a scale by eye. They cannot be read to a reliable value, so
//!   roughness stays an input. Only single values the text prints are held
//!   ([`CONCRETE_SIDE_DRAIN_KS_M`], [`CONCRETE_CULVERT_MANNING_N`]).
//! * **Inlet and outlet loss coefficients** `Kin`, `Kout` (Eq. 7.4): the manual names them and
//!   prints no table. SAG Example 7.1 uses 0.5 and 1.0.
//! * **Downstream energy level** `H2` and the shoulder break point.
//! * **Which slope unit Table 7.2 means.** See [`SlopeUnit`].
//!
//! # What is not here
//!
//! The Hershfield equation, the DDF diagram, the Standard Design Flood, unit hydrograph, SCS-SA,
//! empirical (Midgley and Pitman, Kovacs regions), statistical flood methods and the Rational
//! method's SDF variant with Table 3.13 are **not implemented**. Nor are Figure 4.8 to 4.10
//! roughness, the soil permissible velocity graph (Figure 5.15), kerb inlets, grid and side
//! outlets, low-level crossings, bridges and scour, stormwater systems, culvert inlet
//! improvement and outlet velocity charts (Figures 7.4 and 7.6, which are charts of the same
//! relations), erosion protection, subsurface drainage and the assessment of existing structures.
//!
//! The result is a calculation for an engineer to check, not a certified design.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use super::{ChannelCheck, ChannelInput, CulvertLayoutLimits, DesignBasis, Limit, LongCulvertRule};
use crate::{fixed, positive, DrainageError};

/// The first line of every calculation to this basis.
pub const BASIS: &str = "Design basis: SANRAL Drainage Manual, 6th edition (September 2013, Addendum 1 of 29 May 2013), cited as SDM; worked examples of the SANRAL Drainage Manual Application Guide (September 2013), cited as SAG.";

/// Gravitational acceleration as the SAG uses it in its culvert example (SAG Example 7.1,
/// pages 7-72 and 7-75: `2·9,81`).
pub const GRAVITY_M_S2: f64 = 9.81;

/// Single value printed under SDM Photograph 4.5 (p. 4-17): a concrete side drain, "Estimated
/// roughness = 0,002 m". Not a table: the roughness of other surfaces stays an input.
pub const CONCRETE_SIDE_DRAIN_KS_M: f64 = 0.002;
/// The Manning n printed beside it under Photograph 4.5 (p. 4-17): "0,016 s/m^1/3".
pub const CONCRETE_SIDE_DRAIN_MANNING_N: f64 = 0.016;
/// SDM Section 7.3.2.1 (p. 7-7): calculations for Figure 7.4 "have been performed with a value of
/// n = 0,016 s/m^1/3, which is a realistic design value for concrete culverts".
pub const CONCRETE_CULVERT_MANNING_N: f64 = 0.016;

/// Largest catchment the rational method is generally recommended for, km² (SDM 3.5.1.1, p. 3-18:
/// "generally recommended that the method should only be applied to catchments smaller than 15 km²").
pub const RATIONAL_AREA_RECOMMENDED_KM2: f64 = 15.0;
/// Shortest time of concentration used for the intensity, hours (SDM 3.5.1.3, p. 3-20, 15 minutes;
/// SAG Step 7a, p. 3-10: "If TC <= 0,25 hours divide by 0,25 hours").
pub const MINIMUM_INTENSITY_DURATION_H: f64 = 0.25;

/// Submergence limit of an inlet or outlet controlled structure at the design flood, `H1/D`
/// (SDM 8.3.2, p. 8-7: "usually limited to a ratio of H1/D = 1,2"; also 7.2.2.4, p. 7-4).
pub const SUBMERGENCE_RATIO_LIMIT: f64 = 1.2;
/// At twice the return period the upstream energy level is limited to the smaller of `2D` and the
/// shoulder break point (SDM 8.3.2, p. 8-7).
pub const SUBMERGENCE_RATIO_AT_2T: f64 = 2.0;
/// Minimum freeboard where significant debris is anticipated, m (SDM 8.4.1, p. 8-9 and Figure 8.3).
pub const DEBRIS_FREEBOARD_M: f64 = 0.3;
/// Smallest slope of an unpaved median drain, m/m (SDM 5.4.3, p. 5-23: "not less than 0,5%").
pub const MEDIAN_DRAIN_MIN_SLOPE_UNPAVED: f64 = 0.005;
/// Smallest slope of a paved median drain, m/m (SDM 5.4.3, p. 5-23: "not less than 0,25%").
pub const MEDIAN_DRAIN_MIN_SLOPE_PAVED: f64 = 0.0025;
/// Velocity over a concrete lining with joints or cracks of 60 mm thickness, m/s (SDM 5.4.6.1,
/// p. 5-25: "should not be higher than about 2,5 m/s").
pub const CONCRETE_LINING_JOINTED_VELOCITY_M_S: f64 = 2.5;

fn invalid<T>(message: impl Into<String>) -> Result<T, DrainageError> {
    Err(DrainageError::InvalidInput(message.into()))
}

fn ok(pass: bool) -> &'static str {
    if pass {
        "OK"
    } else {
        "FAILS"
    }
}

// ---------------------------------------------------------------------------------------------
// Run-off coefficient
// ---------------------------------------------------------------------------------------------

/// The three mean annual rainfall columns of SDM Table 3.7 (p. 3-18): "< 600", "600 - 900", "> 900".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapBand {
    Below600,
    From600To900,
    Above900,
}

impl MapBand {
    /// The column for a mean annual precipitation in mm. 600 and 900 fall in the middle column.
    pub fn from_map_mm(map_mm: f64) -> MapBand {
        if map_mm < 600.0 {
            MapBand::Below600
        } else if map_mm <= 900.0 {
            MapBand::From600To900
        } else {
            MapBand::Above900
        }
    }

    fn index(self) -> usize {
        match self {
            MapBand::Below600 => 0,
            MapBand::From600To900 => 1,
            MapBand::Above900 => 2,
        }
    }
}

/// Surface slope classes of Table 3.7 (`Cs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuralSlope {
    /// Vleis and pans (< 3 %).
    VleisAndPans,
    /// Flat areas (3 to 10 %).
    Flat,
    /// Hilly (10 to 30 %).
    Hilly,
    /// Steep areas (> 30 %).
    Steep,
}

impl RuralSlope {
    /// `Cs` of SDM Table 3.7 (p. 3-18) for the three rainfall columns.
    pub fn cs(self, band: MapBand) -> f64 {
        let row = match self {
            RuralSlope::VleisAndPans => [0.01, 0.03, 0.05],
            RuralSlope::Flat => [0.06, 0.08, 0.11],
            RuralSlope::Hilly => [0.12, 0.16, 0.20],
            RuralSlope::Steep => [0.22, 0.26, 0.30],
        };
        row[band.index()]
    }

    /// The dolomite reduction factor `Dfactor` (SDM p. 3-19; SAG Table 3C.4 on p. 3C-3 of the SDM forms).
    pub fn dolomite_factor(self) -> f64 {
        match self {
            RuralSlope::Steep => 0.50,
            RuralSlope::Hilly => 0.35,
            RuralSlope::Flat => 0.20,
            RuralSlope::VleisAndPans => 0.10,
        }
    }
}

/// Permeability classes of Table 3.7 (`Cp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permeability {
    VeryPermeable,
    Permeable,
    SemiPermeable,
    Impermeable,
}

impl Permeability {
    /// `Cp` of SDM Table 3.7 (p. 3-18).
    pub fn cp(self, band: MapBand) -> f64 {
        let row = match self {
            Permeability::VeryPermeable => [0.03, 0.04, 0.05],
            Permeability::Permeable => [0.06, 0.08, 0.10],
            Permeability::SemiPermeable => [0.12, 0.16, 0.20],
            Permeability::Impermeable => [0.21, 0.26, 0.30],
        };
        row[band.index()]
    }
}

/// Vegetation classes of Table 3.7 (`Cv`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Vegetation {
    ThickBushAndPlantation,
    LightBushAndFarmLands,
    Grasslands,
    NoVegetation,
}

impl Vegetation {
    /// `Cv` of SDM Table 3.7 (p. 3-18).
    pub fn cv(self, band: MapBand) -> f64 {
        let row = match self {
            Vegetation::ThickBushAndPlantation => [0.03, 0.04, 0.05],
            Vegetation::LightBushAndFarmLands => [0.07, 0.11, 0.15],
            Vegetation::Grasslands => [0.17, 0.21, 0.25],
            Vegetation::NoVegetation => [0.26, 0.28, 0.30],
        };
        row[band.index()]
    }
}

/// Urban land uses of Table 3.7 (`C2`), with the printed range of each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UrbanUse {
    LawnsSandyFlat,
    LawnsSandySteep,
    LawnsHeavySoilFlat,
    LawnsHeavySoilSteep,
    Houses,
    Flats,
    LightIndustry,
    HeavyIndustry,
    BusinessCityCentre,
    BusinessSuburban,
    BusinessStreets,
    MaximumFlood,
}

impl UrbanUse {
    pub const ALL: [UrbanUse; 12] = [
        UrbanUse::LawnsSandyFlat,
        UrbanUse::LawnsSandySteep,
        UrbanUse::LawnsHeavySoilFlat,
        UrbanUse::LawnsHeavySoilSteep,
        UrbanUse::Houses,
        UrbanUse::Flats,
        UrbanUse::LightIndustry,
        UrbanUse::HeavyIndustry,
        UrbanUse::BusinessCityCentre,
        UrbanUse::BusinessSuburban,
        UrbanUse::BusinessStreets,
        UrbanUse::MaximumFlood,
    ];

    pub fn label(self) -> &'static str {
        match self {
            UrbanUse::LawnsSandyFlat => "Lawns, sandy, flat (<2%)",
            UrbanUse::LawnsSandySteep => "Lawns, sandy, steep (>7%)",
            UrbanUse::LawnsHeavySoilFlat => "Lawns, heavy soil, flat (<2%)",
            UrbanUse::LawnsHeavySoilSteep => "Lawns, heavy soil, steep (>7%)",
            UrbanUse::Houses => "Residential, houses",
            UrbanUse::Flats => "Residential, flats",
            UrbanUse::LightIndustry => "Industry, light",
            UrbanUse::HeavyIndustry => "Industry, heavy",
            UrbanUse::BusinessCityCentre => "Business, city centre",
            UrbanUse::BusinessSuburban => "Business, suburban",
            UrbanUse::BusinessStreets => "Business, streets",
            UrbanUse::MaximumFlood => "Maximum flood",
        }
    }

    /// The printed range (low, high) of SDM Table 3.7 (p. 3-18). The engineer picks the factor.
    pub fn range(self) -> (f64, f64) {
        match self {
            UrbanUse::LawnsSandyFlat => (0.05, 0.10),
            UrbanUse::LawnsSandySteep => (0.15, 0.20),
            UrbanUse::LawnsHeavySoilFlat => (0.13, 0.17),
            UrbanUse::LawnsHeavySoilSteep => (0.25, 0.35),
            UrbanUse::Houses => (0.30, 0.50),
            UrbanUse::Flats => (0.50, 0.70),
            UrbanUse::LightIndustry => (0.50, 0.80),
            UrbanUse::HeavyIndustry => (0.60, 0.90),
            UrbanUse::BusinessCityCentre => (0.70, 0.95),
            UrbanUse::BusinessSuburban => (0.50, 0.70),
            UrbanUse::BusinessStreets => (0.70, 0.95),
            UrbanUse::MaximumFlood => (1.0, 1.0),
        }
    }
}

/// The return period of a run-off coefficient adjustment: a tabulated year of Table 3.8 or the
/// maximum flood.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReturnPeriod {
    /// 2, 5, 10, 20, 50 or 100 years. Table 3.8 gives no other year and the manual gives no
    /// interpolation, so any other year is refused.
    Years(u32),
    /// The probable maximum flood: `C1 = Cs + Cpmax + Cvmax`, `C2 = 1`, `Ft = 1` (p. 3-19).
    MaximumFlood,
}

/// Which row of Table 3.8 applies: the manual's two catchment kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FtClass {
    SteepAndImpermeable,
    FlatAndPermeable,
}

/// The adjustment factor `Ft` of SDM Table 3.8 (p. 3-19).
pub fn return_period_factor(class: FtClass, period: ReturnPeriod) -> Result<f64, DrainageError> {
    let years = match period {
        ReturnPeriod::MaximumFlood => return Ok(1.0),
        ReturnPeriod::Years(y) => y,
    };
    let (steep, flat) = match years {
        2 => (0.75, 0.50),
        5 => (0.80, 0.55),
        10 => (0.85, 0.60),
        20 => (0.90, 0.67),
        50 => (0.95, 0.83),
        100 => (1.00, 1.00),
        other => return invalid(format!("SDM Table 3.8 (p. 3-19) gives the factor Ft for 2, 5, 10, 20, 50 and 100 years only, and no interpolation; {other} years is not tabulated")),
    };
    Ok(match class {
        FtClass::SteepAndImpermeable => steep,
        FtClass::FlatAndPermeable => flat,
    })
}

/// The rural part of a catchment: the share of its area (percent) in each class of Table 3.7.
/// Each of the three lists must add up to 100.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuralCatchment {
    /// Mean annual precipitation of the catchment, mm (selects the column of Table 3.7).
    pub map_mm: f64,
    pub slope: Vec<(RuralSlope, f64)>,
    pub permeability: Vec<(Permeability, f64)>,
    pub vegetation: Vec<(Vegetation, f64)>,
    /// Fraction of the catchment that is dolomitic, 0 to 1 (`D%` of the manual).
    pub dolomite_fraction: f64,
    pub ft_class: FtClass,
}

/// One land use of the urban part of a catchment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UrbanPart {
    pub land_use: UrbanUse,
    /// Share of the urban area, percent. The parts must add up to 100.
    pub percent: f64,
    /// The factor chosen within the printed range of [`UrbanUse::range`].
    pub factor: f64,
}

/// Everything the run-off coefficient needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunoffInput {
    pub rural: Option<RuralCatchment>,
    pub urban: Vec<UrbanPart>,
    /// Run-off coefficient for lakes, 0 to 1 (`C3`); an input, the manual prints no value.
    pub lakes_c3: f64,
    /// Area distribution factors: rural, urban, lakes (`α`, `β`, `γ`); they add up to 1.
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
    pub return_period: ReturnPeriod,
}

/// The result of [`runoff_coefficient`].
#[derive(Debug, Clone, PartialEq)]
pub struct RunoffCoefficient {
    pub c1: f64,
    pub c1d: f64,
    pub ft: f64,
    pub c1t: f64,
    pub c2: f64,
    pub c3: f64,
    pub ct: f64,
    lines: Vec<String>,
}

impl RunoffCoefficient {
    pub fn calculation(&self) -> Vec<String> {
        self.lines.clone()
    }
}

fn shares_total<T>(name: &str, parts: &[(T, f64)]) -> Result<f64, DrainageError> {
    if parts.is_empty() {
        return invalid(format!("the {name} shares of the rural catchment are missing"));
    }
    let total: f64 = parts.iter().map(|p| p.1).sum();
    if parts.iter().any(|p| !(p.1.is_finite() && p.1 >= 0.0)) || (total - 100.0).abs() > 0.01 {
        return invalid(format!("the {name} shares must not be negative and must add up to 100 percent (they add up to {total})"));
    }
    Ok(total)
}

/// The combined run-off coefficient `CT = α·C1T + β·C2 + γ·C3` of SAG Eq. 3.12 (p. 3-20), with
/// `C1 = Cs + Cp + Cv` (SDM 3.5.1.2.2, p. 3-18; SAG Eq. 3.10), the dolomite adjustment
/// `C1D = C1·(1 − D) + C1·D·Σ(Dfactor × share of Cs class)` (SAG p. 3-20; the SDM forms print the
/// same, p. 3C-2) and `C1T = C1D × Ft` (SAG Eq. 3.11, Table 3.8).
pub fn runoff_coefficient(input: &RunoffInput) -> Result<RunoffCoefficient, DrainageError> {
    let (a, b, g) = (input.alpha, input.beta, input.gamma);
    if ![a, b, g].iter().all(|v| v.is_finite() && *v >= 0.0) || (a + b + g - 1.0).abs() > 1e-6 {
        return invalid(format!("the area distribution factors alpha, beta and gamma must not be negative and must add up to 1 (they add up to {})", a + b + g));
    }
    if !(input.lakes_c3.is_finite() && (0.0..=1.0).contains(&input.lakes_c3)) {
        return invalid(format!("the lakes coefficient C3 must be between 0 and 1 (got {})", input.lakes_c3));
    }
    let max_flood = input.return_period == ReturnPeriod::MaximumFlood;
    let mut lines = vec![BASIS.to_string()];
    let (mut c1, mut c1d, mut ft, mut c1t) = (0.0, 0.0, 1.0, 0.0);
    if a > 0.0 {
        let Some(r) = &input.rural else { return invalid("alpha is above zero, so the rural catchment must be given") };
        if !positive(r.map_mm) {
            return invalid(format!("the mean annual precipitation must be greater than zero (got {})", r.map_mm));
        }
        if !(r.dolomite_fraction.is_finite() && (0.0..=1.0).contains(&r.dolomite_fraction)) {
            return invalid(format!("the dolomite fraction must be between 0 and 1 (got {})", r.dolomite_fraction));
        }
        shares_total("surface slope", &r.slope)?;
        shares_total("permeability", &r.permeability)?;
        shares_total("vegetation", &r.vegetation)?;
        let band = MapBand::from_map_mm(r.map_mm);
        let cs: f64 = r.slope.iter().map(|(c, p)| p / 100.0 * c.cs(band)).sum();
        let cp: f64 = if max_flood {
            Permeability::Impermeable.cp(band)
        } else {
            r.permeability.iter().map(|(c, p)| p / 100.0 * c.cp(band)).sum()
        };
        let cv: f64 = if max_flood {
            Vegetation::NoVegetation.cv(band)
        } else {
            r.vegetation.iter().map(|(c, p)| p / 100.0 * c.cv(band)).sum()
        };
        c1 = cs + cp + cv;
        lines.push(format!(
            "Rural: MAP {:.1} mm, column {} of Table 3.7 (SDM p. 3-18). Cs = {:.4}, Cp = {:.4}, Cv = {:.4}{}; C1 = Cs + Cp + Cv = {:.4} (SAG Eq. 3.10, p. 3-19).",
            r.map_mm,
            match band {
                MapBand::Below600 => "< 600",
                MapBand::From600To900 => "600 - 900",
                MapBand::Above900 => "> 900",
            },
            cs,
            cp,
            cv,
            if max_flood { " (maximum flood: Cpmax and Cvmax, SDM p. 3-19)" } else { "" },
            c1
        ));
        let dsum: f64 = r.slope.iter().map(|(c, p)| c.dolomite_factor() * p / 100.0).sum();
        c1d = c1 * (1.0 - r.dolomite_fraction) + c1 * r.dolomite_fraction * dsum;
        if r.dolomite_fraction > 0.0 {
            lines.push(format!(
                "Dolomite {:.1} %: C1D = C1·(1 - D) + C1·D·Σ(Dfactor × Cs share) = {:.4} × {:.3} + {:.4} × {:.3} × {:.4} = {:.4} (SDM p. 3-19 for the factors 0,50 / 0,35 / 0,20 / 0,10 by slope class; formula as printed in SAG p. 3-20).",
                r.dolomite_fraction * 100.0,
                c1,
                1.0 - r.dolomite_fraction,
                c1,
                r.dolomite_fraction,
                dsum,
                c1d
            ));
        } else {
            lines.push(format!("No dolomite: C1D = C1 = {c1d:.4}."));
        }
        ft = return_period_factor(r.ft_class, input.return_period)?;
        c1t = c1d * ft;
        lines.push(format!(
            "Return period factor Ft = {ft} ({} catchment, Table 3.8, SDM p. 3-19; {}); C1T = C1D × Ft = {:.4}.",
            match r.ft_class {
                FtClass::SteepAndImpermeable => "steep and impermeable",
                FtClass::FlatAndPermeable => "flat and permeable",
            },
            match input.return_period {
                ReturnPeriod::Years(y) => format!("{y} years"),
                ReturnPeriod::MaximumFlood => "maximum flood, Ft = 1".to_string(),
            },
            c1t
        ));
    }
    let mut c2 = 0.0;
    if b > 0.0 {
        if input.urban.is_empty() {
            return invalid("beta is above zero, so the urban land uses must be given");
        }
        let total: f64 = input.urban.iter().map(|u| u.percent).sum();
        if input.urban.iter().any(|u| !(u.percent.is_finite() && u.percent >= 0.0)) || (total - 100.0).abs() > 0.01 {
            return invalid(format!("the urban land use shares must not be negative and must add up to 100 percent (they add up to {total})"));
        }
        for u in &input.urban {
            let (lo, hi) = u.land_use.range();
            if !(u.factor.is_finite() && u.factor >= lo - 1e-9 && u.factor <= hi + 1e-9) {
                return invalid(format!("the factor {} for '{}' is outside the range {lo} to {hi} printed in SDM Table 3.7 (p. 3-18)", u.factor, u.land_use.label()));
            }
        }
        c2 = if max_flood { 1.0 } else { input.urban.iter().map(|u| u.percent / 100.0 * u.factor).sum() };
        lines.push(format!(
            "Urban: C2 = Σ(share × factor) over {} land use(s) within the ranges of Table 3.7 (SDM p. 3-18) = {:.4}{}.",
            input.urban.len(),
            c2,
            if max_flood { " (maximum flood: C2 = 1, SDM p. 3-19)" } else { "" }
        ));
    }
    let c3 = input.lakes_c3;
    let ct = a * c1t + b * c2 + g * c3;
    lines.push(format!("CT = α·C1T + β·C2 + γ·C3 = {a} × {c1t:.4} + {b} × {c2:.4} + {g} × {c3} = {ct:.4} (SAG Eq. 3.12, p. 3-20)."));
    Ok(RunoffCoefficient { c1, c1d, ft, c1t, c2, c3, ct, lines })
}

// ---------------------------------------------------------------------------------------------
// Time of concentration, area reduction, rational method
// ---------------------------------------------------------------------------------------------

/// Surfaces of SDM Table 3.10 (p. 3-28), the Kerby roughness `r`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KerbySurface {
    PavedAreas,
    CleanCompactedSoilNoStones,
    SparseGrassOverFairlyRoughSurface,
    MediumGrassCover,
    ThickGrassCover,
}

impl KerbySurface {
    /// The recommended `r` of Table 3.10.
    pub fn r(self) -> f64 {
        match self {
            KerbySurface::PavedAreas => 0.02,
            KerbySurface::CleanCompactedSoilNoStones => 0.1,
            KerbySurface::SparseGrassOverFairlyRoughSurface => 0.3,
            KerbySurface::MediumGrassCover => 0.4,
            KerbySurface::ThickGrassCover => 0.8,
        }
    }
}

/// Time of concentration of overland flow by Kerby, hours: `Tc = 0,604·(r·L / S^0,5)^0,467`
/// (SDM Eq. 3.11, p. 3-27), L the hydraulic length in km and S = H/(1000 L) in m/m.
pub fn kerby_tc_h(r: f64, length_km: f64, slope_m_per_m: f64) -> f64 {
    0.604 * (r * length_km / slope_m_per_m.sqrt()).powf(0.467)
}

/// Time of concentration of a defined watercourse, hours: `Tc = (0,87·L² / (1000·Sav))^0,385`
/// (SDM Eq. 3.12, p. 3-28; the US Soil Conservation Service formula), L in km, Sav in m/m.
pub fn scs_tc_h(length_km: f64, average_slope_m_per_m: f64) -> f64 {
    (0.87 * length_km * length_km / (1000.0 * average_slope_m_per_m)).powf(0.385)
}

/// The 10-85 average slope, m/m: `Sav = (H0,85L - H0,10L) / (1000·0,75·L)` (SDM Eq. 3.13, p. 3-29),
/// the heights in m and L in km.
pub fn slope_10_85(h_at_10_percent_m: f64, h_at_85_percent_m: f64, length_km: f64) -> f64 {
    (h_at_85_percent_m - h_at_10_percent_m) / (1000.0 * 0.75 * length_km)
}

/// One reach of the flow path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TcReach {
    /// Overland flow by Kerby (Eq. 3.11): roughness `r`, hydraulic length km, slope m/m.
    Overland { r: f64, length_km: f64, slope_m_per_m: f64 },
    /// Defined watercourse by Eq. 3.12: length km, average slope m/m.
    Channel { length_km: f64, slope_m_per_m: f64 },
    /// Urban: flow velocity from the Chezy or Manning equation for a representative cross-section
    /// (SDM 3.5.1.4 (iii), p. 3-29), entered. Length in m, velocity in m/s.
    Urban { length_m: f64, velocity_m_s: f64 },
}

/// The result of [`time_of_concentration`].
#[derive(Debug, Clone, PartialEq)]
pub struct TimeOfConcentration {
    pub reach_hours: Vec<f64>,
    pub total_h: f64,
    /// Total length of the flow path, km.
    pub length_km: f64,
    pub lines: Vec<String>,
}

/// Time of concentration as the sum of the flow times of the reaches, as SDM 3.5.1.4 (p. 3-29)
/// says "in small catchments it may be necessary to determine TC as the sum of the flow times,
/// for overland and channel flow". Warns when the mean velocity is outside 0,1 to 4 m/s (p. 3-29).
pub fn time_of_concentration(reaches: &[TcReach]) -> Result<TimeOfConcentration, DrainageError> {
    if reaches.is_empty() {
        return invalid("at least one reach is needed for the time of concentration");
    }
    let mut hours = Vec::new();
    let mut lines = vec![BASIS.to_string()];
    let mut length_km = 0.0;
    for (i, r) in reaches.iter().enumerate() {
        let n = i + 1;
        match *r {
            TcReach::Overland { r, length_km: l, slope_m_per_m: s } => {
                if !positive(r) || !positive(l) || !positive(s) {
                    return invalid(format!("reach {n}: r, the length and the slope must be greater than zero"));
                }
                let t = kerby_tc_h(r, l, s);
                lines.push(format!("Reach {n}, overland (Kerby, SDM Eq. 3.11, p. 3-27): Tc = 0,604·(r·L/S^0,5)^0,467 with r = {r}, L = {l} km, S = {s}: = {t:.3} h."));
                length_km += l;
                hours.push(t);
            }
            TcReach::Channel { length_km: l, slope_m_per_m: s } => {
                if !positive(l) || !positive(s) {
                    return invalid(format!("reach {n}: the length and the slope must be greater than zero"));
                }
                let t = scs_tc_h(l, s);
                lines.push(format!("Reach {n}, defined watercourse (SDM Eq. 3.12, p. 3-28): Tc = (0,87·L²/(1000·Sav))^0,385 with L = {l} km, Sav = {s}: = {t:.3} h."));
                length_km += l;
                hours.push(t);
            }
            TcReach::Urban { length_m, velocity_m_s } => {
                if !positive(length_m) || !positive(velocity_m_s) {
                    return invalid(format!("reach {n}: the length and the velocity must be greater than zero"));
                }
                let t = length_m / velocity_m_s / 3600.0;
                lines.push(format!("Reach {n}, urban (SDM 3.5.1.4 (iii), p. 3-29): Tc = L/v = {length_m} m / {velocity_m_s} m/s = {t:.3} h."));
                length_km += length_m / 1000.0;
                hours.push(t);
            }
        }
    }
    let total_h: f64 = hours.iter().sum();
    lines.push(format!("Time of concentration = sum of the reaches = {total_h:.3} h = {:.1} min.", total_h * 60.0));
    let v = length_km * 1000.0 / (total_h * 3600.0);
    if !(0.1..=4.0).contains(&v) {
        lines.push(format!("Check: the mean velocity L/Tc = {v:.2} m/s is outside the typical 0,1 to 4 m/s (SDM p. 3-29); review the formulae and slope used."));
    }
    Ok(TimeOfConcentration { reach_hours: hours, total_h, length_km, lines })
}

/// The area reduction factor, percent, by SDM Eq. 3.10 (p. 3-25):
/// `ARF = (90000 - 12800·ln A + 9830·ln(60·Tc))^0,4` with A in km² and Tc in hours. The manual says
/// it "should be less than 100%", so the value is capped at 100.
pub fn area_reduction_factor_percent(area_km2: f64, tc_h: f64) -> Result<f64, DrainageError> {
    if !positive(area_km2) || !positive(tc_h) {
        return invalid(format!("the catchment area and the time of concentration must be greater than zero (got {area_km2} km², {tc_h} h)"));
    }
    let base = 90000.0 - 12800.0 * area_km2.ln() + 9830.0 * (60.0 * tc_h).ln();
    if base <= 0.0 {
        return invalid("Equation 3.10 is outside its range for this area and duration (the bracket is not positive)");
    }
    Ok(base.powf(0.4).min(100.0))
}

/// How the area reduction factor is taken.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AreaReduction {
    /// SDM Eq. 3.10.
    Equation,
    /// A value in percent read from Figure 3.11 or the SDM's other curves (entered, labelled).
    Entered(f64),
    /// None applied (ARF = 100 %), as SAG Example 3.1 does for its Alternative 3 (Steps 7c, 8c).
    NotApplied,
}

/// Rainfall for the rational method: site data, always entered.
#[derive(Debug, Clone, PartialEq)]
pub struct RainfallInput {
    /// Point rainfall depth in mm for the time of concentration (not less than 15 minutes) and the
    /// return period, from the DDF diagram, the Hershfield equation or the Design Rainfall
    /// Estimation software; the crate holds none of them.
    pub point_rainfall_mm: f64,
    /// Where the depth comes from, quoted in the calculation.
    pub source: String,
    pub area_reduction: AreaReduction,
}

/// What [`rational_method`] needs.
#[derive(Debug, Clone, PartialEq)]
pub struct RationalInput {
    pub runoff: RunoffInput,
    pub area_km2: f64,
    pub time_of_concentration_h: f64,
    pub rainfall: RainfallInput,
}

/// The result of [`rational_method`].
#[derive(Debug, Clone, PartialEq)]
pub struct RationalResult {
    pub runoff: RunoffCoefficient,
    /// Duration used for the point intensity, h: Tc, but not less than 0,25.
    pub intensity_duration_h: f64,
    /// Point intensity `PiT = PT / Tc`, mm/h.
    pub point_intensity_mm_h: f64,
    pub area_reduction_percent: f64,
    /// Average intensity over the catchment `IT = PiT × ARF/100`, mm/h.
    pub intensity_mm_h: f64,
    pub peak_flow_m3_s: f64,
    lines: Vec<String>,
}

/// Peak flow `Q = C·I·A / 3,6` in m³/s, I in mm/h, A in km² (SDM Eq. 3.8, p. 3-17).
pub fn rational_peak_flow_m3_s(c: f64, intensity_mm_h: f64, area_km2: f64) -> f64 {
    c * intensity_mm_h * area_km2 / 3.6
}

/// The rational method as SDM 3.5.1 gives it, with the rainfall entered by the engineer.
pub fn rational_method(input: &RationalInput) -> Result<RationalResult, DrainageError> {
    if !positive(input.area_km2) || !positive(input.time_of_concentration_h) || !positive(input.rainfall.point_rainfall_mm) {
        return invalid("the area, the time of concentration and the point rainfall must be greater than zero");
    }
    let runoff = runoff_coefficient(&input.runoff)?;
    let tc = input.time_of_concentration_h;
    let duration = tc.max(MINIMUM_INTENSITY_DURATION_H);
    let pi = input.rainfall.point_rainfall_mm / duration;
    let (arf, arf_text) = match input.rainfall.area_reduction {
        AreaReduction::Equation => {
            let v = area_reduction_factor_percent(input.area_km2, tc)?;
            (v, format!("ARF = (90000 - 12800·ln A + 9830·ln(60·Tc))^0,4 = {v:.1} % (SDM Eq. 3.10, p. 3-25)"))
        }
        AreaReduction::Entered(v) => {
            if !(v.is_finite() && v > 0.0 && v <= 100.0) {
                return invalid(format!("an entered area reduction factor must be above 0 and not above 100 percent (got {v})"));
            }
            (v, format!("ARF = {v} % (entered, read by the engineer from SDM Figure 3.11 or another curve of the manual)"))
        }
        AreaReduction::NotApplied => (100.0, "ARF = 100 % (no area reduction applied)".to_string()),
    };
    let intensity = pi * arf / 100.0;
    let q = rational_peak_flow_m3_s(runoff.ct, intensity, input.area_km2);
    let mut lines = runoff.calculation();
    lines.push(format!("Catchment area A = {} km²; time of concentration Tc = {tc:.3} h.", fixed(input.area_km2, 1, 3)));
    lines.push(format!(
        "Point rainfall PT = {} mm for Tc and the return period ({}, entered; the crate holds no rainfall data). PiT = PT/Tc = {} / {duration:.3} = {pi:.2} mm/h (SAG Eq. 3.3, p. 3-10; SDM p. 3-20: Tc below 15 minutes is taken as 0,25 h).",
        input.rainfall.point_rainfall_mm, input.rainfall.source, input.rainfall.point_rainfall_mm
    ));
    lines.push(format!("{arf_text}; IT = PiT × ARF/100 = {intensity:.2} mm/h (SAG Eq. 3.4, p. 3-12)."));
    lines.push(format!("Q = C·I·A/3,6 = {:.4} × {intensity:.2} × {} / 3,6 = {q:.2} m³/s (SDM Eq. 3.8, p. 3-17).", runoff.ct, fixed(input.area_km2, 1, 3)));
    if input.area_km2 > RATIONAL_AREA_RECOMMENDED_KM2 {
        lines.push(format!("Check: the catchment is {} km², above the 15 km² SDM 3.5.1.1 (p. 3-18) generally recommends for the rational method; it may be used by experienced engineers for larger catchments.", fixed(input.area_km2, 1, 3)));
    }
    Ok(RationalResult { runoff, intensity_duration_h: duration, point_intensity_mm_h: pi, area_reduction_percent: arf, intensity_mm_h: intensity, peak_flow_m3_s: q, lines })
}

impl RationalResult {
    pub fn calculation(&self) -> Vec<String> {
        self.lines.clone()
    }
}

// ---------------------------------------------------------------------------------------------
// Design return period and free-flow freeboard
// ---------------------------------------------------------------------------------------------

/// Road classes of SDM Section 8.2 (p. 8-3 to 8-6), which follow TRH 26. Figure 8.2 draws classes
/// 4 and 5 as one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoadClass {
    Class1,
    Class2,
    Class3,
    Class4Or5,
    Class6,
}

impl RoadClass {
    pub fn label(self) -> &'static str {
        match self {
            RoadClass::Class1 => "Class 1 (R1 rural principal arterial, U1 urban principal arterial)",
            RoadClass::Class2 => "Class 2 (R2 rural major arterial, U2 urban major arterial)",
            RoadClass::Class3 => "Class 3 (R3 rural minor arterial, U3 urban minor arterial)",
            RoadClass::Class4Or5 => "Class 4 and 5 (collector, local)",
            RoadClass::Class6 => "Class 6 (walkway)",
        }
    }

    /// The vertices of the line of SDM Figure 8.2 (p. 8-7): the return period at `Q20 <= 20` and at
    /// `Q20 >= 150` m³/s, read from the dots printed on the figure and confirmed by SAG Example 7.1
    /// (Class 3, Q20 = 85 m³/s: 15 years, which is the midpoint 10 to 20).
    pub fn return_period_vertices_years(self) -> (f64, f64) {
        match self {
            RoadClass::Class1 => (50.0, 100.0),
            RoadClass::Class2 => (20.0, 50.0),
            RoadClass::Class3 => (10.0, 20.0),
            RoadClass::Class4Or5 => (10.0, 10.0),
            RoadClass::Class6 => (5.0, 5.0),
        }
    }

    /// Classes for which the afflux at twice the return period must stay below the shoulder break
    /// point (SDM 8.3.2, p. 8-7: "shall generally only apply to Road Classes 1, 2 and 3").
    pub fn sbp_limit_applies(self) -> bool {
        matches!(self, RoadClass::Class1 | RoadClass::Class2 | RoadClass::Class3)
    }
}

/// The design flood frequency in years from the road class and the 20-year flood (the "indicator
/// flood", SDM 8.3.1, p. 8-6), by Figure 8.2: flat at the first vertex to `Q20 = 20` m³/s, a straight
/// line to the second vertex at 150 m³/s, then flat. The figure is drawn to 200 m³/s; beyond that
/// it is refused. The result is not rounded.
pub fn design_return_period_years(class: RoadClass, q20_m3_s: f64) -> Result<f64, DrainageError> {
    if !positive(q20_m3_s) {
        return invalid(format!("the 20-year flood must be greater than zero (got {q20_m3_s})"));
    }
    if q20_m3_s > 200.0 {
        return invalid(format!("SDM Figure 8.2 (p. 8-7) is drawn only to Q20 = 200 m³/s; {q20_m3_s} m³/s is beyond it"));
    }
    let (lo, hi) = class.return_period_vertices_years();
    Ok(if q20_m3_s <= 20.0 {
        lo
    } else if q20_m3_s >= 150.0 {
        hi
    } else {
        lo + (hi - lo) * (q20_m3_s - 20.0) / 130.0
    })
}

/// Required freeboard of a free-flow structure below the soffit, m, from the line printed in
/// SDM Figure 8.3 (p. 8-8): `F = 0,78·log10(QT) - 1,26`, not below zero. The figure draws the
/// line from about 41 m³/s (where it is zero). With severe debris at least [`DEBRIS_FREEBOARD_M`]
/// applies (printed in the figure and in 8.4.1).
pub fn free_flow_freeboard_m(design_flood_m3_s: f64) -> Result<f64, DrainageError> {
    if !positive(design_flood_m3_s) {
        return invalid(format!("the design flood must be greater than zero (got {design_flood_m3_s})"));
    }
    Ok((0.78 * design_flood_m3_s.log10() - 1.26).max(0.0))
}

// ---------------------------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------------------------

/// The channel sections of SDM Tables 5.3 and 5.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Rectangular,
    Trapezoidal,
}

/// Minimum freeboard of a straight channel, m, by SDM Table 5.3 (p. 5-22): subcritical (Fr < 1)
/// rectangular `0,15·E`, trapezoidal `0,20·E`; supercritical (Fr > 1) rectangular `0,25·y`,
/// trapezoidal `0,30·y`, with `E = y + v²/2g` (Eq. 5.6) and `y` the depth at the deepest point.
/// At Fr = 1 exactly the supercritical column is used (the larger). Between about 0,8 and 1,2 the
/// manual calls the flow unstable and asks for conservative depth and freeboard; the caller is told
/// by [`channel_freeboard_note`].
pub fn channel_freeboard_m(kind: ChannelKind, froude: f64, depth_m: f64, velocity_m_s: f64) -> f64 {
    let e = depth_m + velocity_m_s * velocity_m_s / (2.0 * GRAVITY_M_S2);
    match (kind, froude >= 1.0) {
        (ChannelKind::Rectangular, false) => 0.15 * e,
        (ChannelKind::Trapezoidal, false) => 0.20 * e,
        (ChannelKind::Rectangular, true) => 0.25 * depth_m,
        (ChannelKind::Trapezoidal, true) => 0.30 * depth_m,
    }
}

/// The caution SDM 5.4.2 (p. 5-22) gives when `0,8 < Fr < 1,2`, or None.
pub fn channel_freeboard_note(froude: f64) -> Option<&'static str> {
    (froude > 0.8 && froude < 1.2).then_some("SDM 5.4.2 (p. 5-22): if the Froude number lies between about 0,8 and 1,2 the flow is unstable and both the normal depth and the freeboard should be calculated conservatively.")
}

/// Additional freeboard at a bend, m, by SDM Table 5.4 (p. 5-22): `v` the mean velocity in the
/// straight channel, `b` the bed width, `r` the centreline radius, `k` the cotangent of the side
/// slope (horizontal per vertical; 0 for a rectangle), `y` the depth in the straight channel.
/// Rectangular: `3v²b/(4gr)` (Fr < 1), `1,2v²b/(gr)` (Fr > 1). Trapezoidal:
/// `v²(b + 2Ky)/(2(gr - 2Kv²))` (Fr < 1), `v²(b + 2Ky)/(gr - 2Kv²)` (Fr > 1). SDM asks for `r` of at least
/// three times the surface width; a smaller radius is refused.
#[allow(clippy::too_many_arguments)]
pub fn bend_additional_freeboard_m(kind: ChannelKind, froude: f64, velocity_m_s: f64, bed_width_m: f64, radius_m: f64, side_slope_h_per_v: f64, depth_m: f64) -> Result<f64, DrainageError> {
    if !positive(velocity_m_s) || !positive(radius_m) || !positive(depth_m) || !(bed_width_m.is_finite() && bed_width_m >= 0.0) {
        return invalid("the velocity, the radius and the depth must be greater than zero and the bed width not negative");
    }
    let k = match kind {
        ChannelKind::Rectangular => 0.0,
        ChannelKind::Trapezoidal => side_slope_h_per_v,
    };
    let surface = bed_width_m + 2.0 * k * depth_m;
    if radius_m < 3.0 * surface {
        return invalid(format!("SDM Table 5.4 (p. 5-23) requires a centreline radius of at least three times the surface width ({:.2} m); {radius_m} m is smaller", 3.0 * surface));
    }
    let (v2, g) = (velocity_m_s * velocity_m_s, GRAVITY_M_S2);
    let sub = froude < 1.0;
    Ok(match (kind, sub) {
        (ChannelKind::Rectangular, true) => 3.0 * v2 * bed_width_m / (4.0 * g * radius_m),
        (ChannelKind::Rectangular, false) => 1.2 * v2 * bed_width_m / (g * radius_m),
        (ChannelKind::Trapezoidal, true) => v2 * (bed_width_m + 2.0 * k * depth_m) / (2.0 * (g * radius_m - 2.0 * k * v2)),
        (ChannelKind::Trapezoidal, false) => v2 * (bed_width_m + 2.0 * k * depth_m) / (g * radius_m - 2.0 * k * v2),
    })
}

/// The mean annual rainfall columns of SDM Table 5.6 (p. 5-24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrassRainfall {
    Below600,
    From600To700,
    Above700,
}

/// The clay content columns of Table 5.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClayContent {
    Over15,
    From6To15,
    Under6,
}

/// The grasses of Table 5.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grass {
    Kikuyu,
    Nk37,
    K11,
    Rhodes,
    ECurvula,
    BlueBuffalo,
    PaspalumDidatum,
}

/// Permissible maximum velocity over a grass cover, m/s, from SDM Table 5.6 (p. 5-24). "No data"
/// cells are refused. The values hold "for a depth of flow of 0,3 m and on slopes of less than 3%"
/// and the velocity of soil with grass "must be limited to a maximum of 1,3 times the allowable
/// velocity for unprotected soil" (5.4.5, p. 5-24), which needs Figure 5.15 and stays an input.
pub fn grass_permissible_velocity_m_s(grass: Grass, rainfall: GrassRainfall, clay: ClayContent) -> Result<f64, DrainageError> {
    let c = match clay {
        ClayContent::Over15 => 0,
        ClayContent::From6To15 => 1,
        ClayContent::Under6 => 2,
    };
    // Rows: [<600 (3), 600-700 (3), >700 (3)]; None is "No data" in the table.
    let n = f64::NAN;
    let row: [f64; 9] = match grass {
        Grass::Kikuyu => [n, n, n, 1.8, 1.5, 0.8, 2.5, 2.0, 1.2],
        Grass::Nk37 => [n, n, n, 2.0, 1.5, 0.8, 2.0, 1.5, 1.0],
        Grass::K11 => [n, n, n, 1.5, 0.8, 0.6, 2.0, 1.5, 1.0],
        Grass::Rhodes => [n, n, n, 1.2, 0.8, 0.6, 1.5, 1.0, 0.8],
        Grass::ECurvula => [1.0, 0.8, 0.8, 1.2, 0.8, 0.6, 1.5, 1.0, 0.8],
        Grass::BlueBuffalo => [1.0, 0.8, 0.8, 1.2, 0.8, 0.6, n, n, n],
        Grass::PaspalumDidatum => [n, n, n, 1.2, 0.8, 0.6, 2.0, 1.5, 1.0],
    };
    let r = match rainfall {
        GrassRainfall::Below600 => 0,
        GrassRainfall::From600To700 => 1,
        GrassRainfall::Above700 => 2,
    };
    let v = row[r * 3 + c];
    if v.is_nan() {
        return invalid("SDM Table 5.6 (p. 5-24) gives 'No data' for this grass, rainfall and clay content");
    }
    Ok(v)
}

/// Materials of SDM Table 5.5 (p. 5-23), "Acceptable maximum side slopes for unlined channels".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideSlopeMaterial {
    Rock,
    StiffClayOrSoilWithConcreteLining,
    SoilWithStonePitchingOrLargeEarthChannels,
    FirmClayOrSmallEarthChannels,
    LooseSandySoil,
    SandyClayOrPorousClay,
    GrassedChannels,
}

/// The steepest side slope of Table 5.5 as horizontal per one vertical (the table prints
/// vertical : horizontal), with the flattest end of a printed range. `None` for rock, printed as
/// "Almost vertical". Grassed channels: 1:3 to 1:4, never steeper than 1:3 and for maintenance not
/// steeper than 1:4 (note to the table).
pub fn steepest_side_slope_h_per_v(material: SideSlopeMaterial) -> (Option<f64>, Option<f64>) {
    match material {
        SideSlopeMaterial::Rock => (None, None),
        SideSlopeMaterial::StiffClayOrSoilWithConcreteLining => (Some(1.0), Some(2.0)),
        SideSlopeMaterial::SoilWithStonePitchingOrLargeEarthChannels => (Some(1.0), None),
        SideSlopeMaterial::FirmClayOrSmallEarthChannels => (Some(1.5), None),
        SideSlopeMaterial::LooseSandySoil => (Some(2.5), None),
        SideSlopeMaterial::SandyClayOrPorousClay => (Some(3.0), None),
        SideSlopeMaterial::GrassedChannels => (Some(3.0), Some(4.0)),
    }
}

/// The result of [`check_channel_sanral`]: the shared Manning check and SDM's freeboard.
#[derive(Debug, Clone, PartialEq)]
pub struct SanralChannelCheck {
    pub check: ChannelCheck,
    pub kind: ChannelKind,
    /// Table 5.3 freeboard plus the bend freeboard where a radius was given, m.
    pub required_freeboard_m: f64,
    pub bend_freeboard_m: Option<f64>,
    pub freeboard_ok: bool,
}

/// The shared open-channel check with `basis = DesignBasis::Sanral`, then SDM's freeboard
/// (Table 5.3, and Table 5.4 when `bend_radius_m` is given) compared with `depth - normal depth`.
/// The input's own `required_freeboard_m` is replaced by the SDM value. The permissible velocity
/// is whatever the caller entered (for instance from [`grass_permissible_velocity_m_s`]).
pub fn check_channel_sanral(input: &ChannelInput, kind: ChannelKind, bend_radius_m: Option<f64>) -> Result<SanralChannelCheck, DrainageError> {
    if input.basis != DesignBasis::Sanral {
        return invalid("check_channel_sanral cites the SANRAL basis: set ChannelInput::basis to DesignBasis::Sanral");
    }
    let mut first = input.clone();
    first.required_freeboard_m = None;
    let probe = super::check_channel(&first)?;
    let table = channel_freeboard_m(kind, probe.froude, probe.normal_depth_m, probe.velocity_m_s);
    let bend = match bend_radius_m {
        Some(r) => {
            let slope = input.geometry.left_side_slope.max(input.geometry.right_side_slope);
            Some(bend_additional_freeboard_m(kind, probe.froude, probe.velocity_m_s, input.geometry.bottom_width_m, r, slope, probe.normal_depth_m)?)
        }
        None => None,
    };
    let required = table + bend.unwrap_or(0.0);
    let mut second = input.clone();
    second.required_freeboard_m = Some(required);
    let check = super::check_channel(&second)?;
    let freeboard_ok = check.freeboard_m >= required;
    Ok(SanralChannelCheck { check, kind, required_freeboard_m: required, bend_freeboard_m: bend, freeboard_ok })
}

impl SanralChannelCheck {
    pub fn passes(&self) -> bool {
        self.check.passes() && self.freeboard_ok
    }

    pub fn calculation(&self) -> Vec<String> {
        let c = &self.check;
        let mut out = c.calculation();
        let kind = match self.kind {
            ChannelKind::Rectangular => "rectangular",
            ChannelKind::Trapezoidal => "trapezoidal",
        };
        let e = c.normal_depth_m + c.velocity_m_s.powi(2) / (2.0 * GRAVITY_M_S2);
        let table = self.required_freeboard_m - self.bend_freeboard_m.unwrap_or(0.0);
        out.push(format!(
            "SDM Table 5.3 (p. 5-22), {kind} channel, Fr = {:.2}: E = y + v²/2g = {e:.3} m; required freeboard = {} = {table:.3} m.",
            c.froude,
            match (self.kind, c.froude >= 1.0) {
                (ChannelKind::Rectangular, false) => "0,15·E",
                (ChannelKind::Trapezoidal, false) => "0,20·E",
                (ChannelKind::Rectangular, true) => "0,25·y",
                (ChannelKind::Trapezoidal, true) => "0,30·y",
            }
        ));
        if let Some(b) = self.bend_freeboard_m {
            out.push(format!("Additional freeboard at the bend, SDM Table 5.4 (p. 5-22): {b:.3} m; required in total {:.3} m.", self.required_freeboard_m));
        }
        if let Some(note) = channel_freeboard_note(c.froude) {
            out.push(format!("Check: {note}"));
        }
        out.push(format!("Freeboard d - y = {:.3} m against {:.3} m required by SDM: {}.", c.freeboard_m, self.required_freeboard_m, ok(self.freeboard_ok)));
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Culverts
// ---------------------------------------------------------------------------------------------

/// The unit of `S0` in the round culvert relations of SDM Table 7.2.
///
/// The table prints `(S0 / 0,4)^0,05` and defines `S0` only as "slope of culvert bed with slight
/// effect on capacity", with no unit. In metres per metre 0,4 would be a 40 % slope; in per cent it
/// is 0,4 %. The manual does not say which, and the factor differs by about 20 % between the two
/// readings for a 0,15 % slope. The crate does not pick one: the caller must say, and the result
/// repeats the choice. SAG Example 7.1 uses only the orifice relation, which has no slope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlopeUnit {
    MetrePerMetre,
    Percent,
}

/// Inlet kinds of SDM Table 7.2 for rectangular culverts: `CB = 1,0` and `Ch = 0,8` for rounded
/// inlets (r > 0,1B), `CB = 0,9` and `Ch = 0,6` for square inlets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RectInlet {
    Rounded,
    Square,
}

impl RectInlet {
    fn cb(self) -> f64 {
        match self {
            RectInlet::Rounded => 1.0,
            RectInlet::Square => 0.9,
        }
    }
    fn ch(self) -> f64 {
        match self {
            RectInlet::Rounded => 0.8,
            RectInlet::Square => 0.6,
        }
    }
}

/// Discharge coefficient of the orifice relation, "CD ≈ 0,6" (SDM Table 7.2 note, p. 7-8).
pub const ORIFICE_CD: f64 = 0.6;

/// `(S0/0,4)^0,05` with the slope given in m/m and read in the unit the caller chose.
fn slope_factor(slope_m_per_m: f64, unit: SlopeUnit) -> Result<f64, DrainageError> {
    if !positive(slope_m_per_m) {
        return invalid(format!("the culvert slope must be greater than zero for Table 7.2 (got {slope_m_per_m})"));
    }
    let s0 = match unit {
        SlopeUnit::MetrePerMetre => slope_m_per_m,
        SlopeUnit::Percent => slope_m_per_m * 100.0,
    };
    Ok((s0 / 0.4).powf(0.05))
}

/// Discharge of a round culvert under inlet control at an upstream energy level `h1_m` above the
/// inlet invert, m³/s, by SDM Table 7.2 (p. 7-8):
/// `Q/(D²·√(gD)) = 0,48·(S0/0,4)^0,05·(H1/D)^1,9` for `0 < H1/D < 0,8`;
/// `0,44·(S0/0,4)^0,05·(H1/D)^1,5` for `0,8 <= H1/D <= 1,2`; above 1,2 the orifice relation
/// `Q = CD·A·√(2g(H1 - D/2))` with `CD = 0,6`. `s0` is the culvert slope in m/m, read in `unit` (see [`SlopeUnit`]).
pub fn round_inlet_flow_m3_s(h1_m: f64, diameter_m: f64, s0: f64, unit: SlopeUnit) -> Result<f64, DrainageError> {
    if !positive(h1_m) || !positive(diameter_m) {
        return invalid("the energy head and the diameter must be greater than zero");
    }
    let f = slope_factor(s0, unit)?;
    let d = diameter_m;
    let r = h1_m / d;
    let unit_flow = d * d * (GRAVITY_M_S2 * d).sqrt();
    Ok(if r < 0.8 {
        0.48 * f * r.powf(1.9) * unit_flow
    } else if r <= SUBMERGENCE_RATIO_LIMIT {
        0.44 * f * r.powf(1.5) * unit_flow
    } else {
        ORIFICE_CD * PI * d * d / 4.0 * (2.0 * GRAVITY_M_S2 * (h1_m - d / 2.0)).sqrt()
    })
}

/// The upstream energy level `H1` above the inlet invert, m, that a round culvert needs to pass
/// `flow_m3_s` under inlet control: [`round_inlet_flow_m3_s`] inverted. The relations are not
/// continuous at `H1/D = 1,2` (the weir relation gives `Q/(D²√(gD)) = 0,44·1,2^1,5·f`, the orifice
/// 0,557): for a flow just above the weir relation's flow at `1,2·D` the orifice relation would
/// give less than `1,2·D`, so the result is never below `1,2·D` there. See the README.
pub fn round_inlet_h1_m(flow_m3_s: f64, diameter_m: f64, s0: f64, unit: SlopeUnit) -> Result<f64, DrainageError> {
    if !positive(flow_m3_s) || !positive(diameter_m) {
        return invalid("the flow and the diameter must be greater than zero");
    }
    let f = slope_factor(s0, unit)?;
    let d = diameter_m;
    let unit_flow = d * d * (GRAVITY_M_S2 * d).sqrt();
    let n = flow_m3_s / unit_flow;
    let at_08 = 0.48 * f * 0.8f64.powf(1.9);
    let at_12 = 0.44 * f * SUBMERGENCE_RATIO_LIMIT.powf(1.5);
    if n < at_08 {
        Ok(d * (n / (0.48 * f)).powf(1.0 / 1.9))
    } else if n <= at_12 {
        Ok(d * (n / (0.44 * f)).powf(1.0 / 1.5))
    } else {
        let a = PI * d * d / 4.0;
        let orifice = d / 2.0 + (flow_m3_s / (ORIFICE_CD * a)).powi(2) / (2.0 * GRAVITY_M_S2);
        Ok(orifice.max(SUBMERGENCE_RATIO_LIMIT * d))
    }
}

/// Discharge of a rectangular culvert under inlet control, m³/s, by SDM Table 7.2 (p. 7-8):
/// for `H1/D <= 1,2` the weir relation `Q = (2/3)·CB·B·H1·√((2/3)·g·H1)`; above 1,2 the orifice
/// relation `Q = Ch·B·D·√(2g(H1 - Ch·D))`.
pub fn rect_inlet_flow_m3_s(h1_m: f64, width_m: f64, height_m: f64, inlet: RectInlet) -> Result<f64, DrainageError> {
    if !positive(h1_m) || !positive(width_m) || !positive(height_m) {
        return invalid("the energy head, the width and the height must be greater than zero");
    }
    Ok(if h1_m / height_m <= SUBMERGENCE_RATIO_LIMIT {
        2.0 / 3.0 * inlet.cb() * width_m * h1_m * (2.0 / 3.0 * GRAVITY_M_S2 * h1_m).sqrt()
    } else {
        inlet.ch() * width_m * height_m * (2.0 * GRAVITY_M_S2 * (h1_m - inlet.ch() * height_m)).sqrt()
    })
}

/// The `H1` a rectangular culvert needs for a flow under inlet control, m: [`rect_inlet_flow_m3_s`]
/// inverted, never below `1,2·D` when the flow exceeds the weir relation's flow at `1,2·D` (the two
/// relations differ by about 2 % there for square inlets).
pub fn rect_inlet_h1_m(flow_m3_s: f64, width_m: f64, height_m: f64, inlet: RectInlet) -> Result<f64, DrainageError> {
    if !positive(flow_m3_s) || !positive(width_m) || !positive(height_m) {
        return invalid("the flow, the width and the height must be greater than zero");
    }
    let k = 2.0 / 3.0 * inlet.cb() * width_m * (2.0 / 3.0 * GRAVITY_M_S2).sqrt();
    let weir = (flow_m3_s / k).powf(2.0 / 3.0);
    if weir / height_m <= SUBMERGENCE_RATIO_LIMIT {
        return Ok(weir);
    }
    let ch = inlet.ch();
    let orifice = ch * height_m + (flow_m3_s / (ch * width_m * height_m)).powi(2) / (2.0 * GRAVITY_M_S2);
    Ok(orifice.max(SUBMERGENCE_RATIO_LIMIT * height_m))
}

/// Friction in the barrel for outlet control.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BarrelFriction {
    /// Manning, SDM Eq. 7.4 (p. 7-7): `hf = v²·n²·L / R^(4/3)`.
    Manning { n: f64 },
    /// Darcy-Weisbach with the rough turbulent friction factor, as SAG Example 7.1 does
    /// (Eq. 7.6 and 7.7, p. 7-76): `hf = λ·L·v²/(2gD)`, `1/√λ = 2·log10(3,7D/ks)`. Round barrels only.
    Colebrook { ks_m: f64 },
}

/// The barrel of a lesser culvert.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SdmBarrel {
    Round { diameter_m: f64 },
    Rectangular { width_m: f64, height_m: f64, inlet: RectInlet },
}

impl SdmBarrel {
    /// D, the inside height or diameter.
    pub fn height_m(&self) -> f64 {
        match *self {
            SdmBarrel::Round { diameter_m } => diameter_m,
            SdmBarrel::Rectangular { height_m, .. } => height_m,
        }
    }
    fn area_m2(&self) -> f64 {
        match *self {
            SdmBarrel::Round { diameter_m } => PI * diameter_m * diameter_m / 4.0,
            SdmBarrel::Rectangular { width_m, height_m, .. } => width_m * height_m,
        }
    }
    fn hydraulic_radius_m(&self) -> f64 {
        match *self {
            SdmBarrel::Round { diameter_m } => diameter_m / 4.0,
            SdmBarrel::Rectangular { width_m, height_m, .. } => width_m * height_m / (2.0 * (width_m + height_m)),
        }
    }
}

/// Upstream energy level `H1` above the inlet invert for full-bore flow, m, by SDM Eq. 7.2 to 7.4
/// (p. 7-6, 7-7), as SAG Example 7.1 applies them (p. 7-75): `H1 = H2 + (Kin + Kout)·v²/2g + hf - S0·L`.
/// `h2_m` is the downstream energy level above the outlet invert (normal depth plus velocity head,
/// Chapter 4), an input; `fall_m` is `S0·L`.
#[allow(clippy::too_many_arguments)]
pub fn outlet_control_h1_m(flow_m3_s: f64, barrel: &SdmBarrel, length_m: f64, fall_m: f64, k_in: f64, k_out: f64, friction: BarrelFriction, h2_m: f64) -> Result<f64, DrainageError> {
    if !positive(flow_m3_s) || !positive(length_m) || !(h2_m.is_finite() && h2_m >= 0.0) || !fall_m.is_finite() {
        return invalid("the flow and the length must be greater than zero, the downstream energy level not negative and the fall a number");
    }
    if !(k_in.is_finite() && k_in >= 0.0 && k_out.is_finite() && k_out >= 0.0) {
        return invalid("the loss coefficients must not be negative");
    }
    let a = barrel.area_m2();
    let v = flow_m3_s / a;
    let velocity_head = v * v / (2.0 * GRAVITY_M_S2);
    let hf = match friction {
        BarrelFriction::Manning { n } => {
            if !positive(n) {
                return invalid(format!("Manning's n must be greater than zero (got {n})"));
            }
            v * v * n * n * length_m / barrel.hydraulic_radius_m().powf(4.0 / 3.0)
        }
        BarrelFriction::Colebrook { ks_m } => {
            let SdmBarrel::Round { diameter_m: d } = *barrel else {
                return invalid("the Darcy friction of SAG Eq. 7.6 and 7.7 is given for round barrels; use Manning for a rectangular barrel");
            };
            if !positive(ks_m) || d <= ks_m {
                return invalid(format!("the absolute roughness must be greater than zero and below the diameter (got {ks_m} m)"));
            }
            let lambda = 1.0 / (2.0 * (3.7 * d / ks_m).log10()).powi(2);
            lambda * length_m * v * v / (2.0 * GRAVITY_M_S2 * d)
        }
    };
    Ok(h2_m + (k_in + k_out) * velocity_head + hf - fall_m)
}

/// One flood at a culvert: the flow of all barrels and the downstream energy level for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CulvertFlood {
    pub flow_m3_s: f64,
    /// Downstream energy level above the outlet invert, m (input).
    pub h2_m: f64,
}

/// What [`culvert_check_sanral`] needs.
#[derive(Debug, Clone, PartialEq)]
pub struct SanralCulvertInput {
    pub barrel: SdmBarrel,
    pub barrels: u32,
    pub length_m: f64,
    /// Slope of the culvert bed, m/m, positive falling to the outlet.
    pub slope_m_per_m: f64,
    /// For a round barrel: how `slope_m_per_m` is read in Table 7.2 (see [`SlopeUnit`]). The
    /// manual does not say, so there is no default.
    pub slope_unit: SlopeUnit,
    pub k_in: f64,
    pub k_out: f64,
    pub friction: BarrelFriction,
    /// The design flood `QT`.
    pub design: CulvertFlood,
    /// The flood of twice the return period, `Q2T`, for the second limit.
    pub twice: Option<CulvertFlood>,
    /// Height of the shoulder break point above the inlet invert, m.
    pub shoulder_break_height_m: Option<f64>,
    pub road_class: RoadClass,
}

/// The result of [`culvert_check_sanral`].
#[derive(Debug, Clone, PartialEq)]
pub struct SanralCulvertCheck {
    pub input: SanralCulvertInput,
    pub inlet_h1_m: f64,
    pub outlet_h1_m: f64,
    /// The greater of the two.
    pub h1_m: f64,
    pub inlet_governs: bool,
    pub h1_over_d: f64,
    pub design_ok: bool,
    /// At twice the return period: (H1, allowed, ok).
    pub twice: Option<(f64, f64, bool)>,
    lines: Vec<String>,
}

impl SanralCulvertCheck {
    pub fn passes(&self) -> bool {
        self.design_ok && self.twice.map_or(true, |t| t.2)
    }

    pub fn calculation(&self) -> Vec<String> {
        self.lines.clone()
    }
}

fn controlling_h1(input: &SanralCulvertInput, flood: &CulvertFlood) -> Result<(f64, f64, f64), DrainageError> {
    if input.barrels == 0 {
        return invalid("a culvert needs at least one barrel");
    }
    let per_barrel = flood.flow_m3_s / input.barrels as f64;
    let inlet = match input.barrel {
        SdmBarrel::Round { diameter_m } => round_inlet_h1_m(per_barrel, diameter_m, input.slope_m_per_m, input.slope_unit)?,
        SdmBarrel::Rectangular { width_m, height_m, inlet } => rect_inlet_h1_m(per_barrel, width_m, height_m, inlet)?,
    };
    let outlet = outlet_control_h1_m(per_barrel, &input.barrel, input.length_m, input.slope_m_per_m * input.length_m, input.k_in, input.k_out, input.friction, flood.h2_m)?;
    Ok((inlet, outlet, per_barrel))
}

/// Inlet and outlet control of a lesser culvert as SDM Chapter 7 presents them: "the larger
/// calculated upstream required energy H1 value ... reflects the applicable hydraulic control"
/// (7.4.1, p. 7-10), against `H1/D <= 1,2` at the design flood and `min(2D, SBP)` at the flood of
/// twice the return period (8.3.2, p. 8-7). The SBP limit is a requirement for road classes 1 to 3.
pub fn culvert_check_sanral(input: &SanralCulvertInput) -> Result<SanralCulvertCheck, DrainageError> {
    let d = input.barrel.height_m();
    let (inlet, outlet, per_barrel) = controlling_h1(input, &input.design)?;
    let h1 = inlet.max(outlet);
    let ratio = h1 / d;
    let design_ok = ratio <= SUBMERGENCE_RATIO_LIMIT;
    let mut lines = vec![
        BASIS.to_string(),
        format!(
            "Culvert: {} barrel(s) of {}, L = {} m, S0 = {} m/m. Design flood QT = {} m³/s, {:.3} m³/s per barrel.",
            input.barrels,
            match input.barrel {
                SdmBarrel::Round { diameter_m } => format!("round D = {diameter_m} m"),
                SdmBarrel::Rectangular { width_m, height_m, .. } => format!("rectangular {width_m} m by {height_m} m"),
            },
            input.length_m,
            input.slope_m_per_m,
            input.design.flow_m3_s,
            per_barrel
        ),
    ];
    match input.barrel {
        SdmBarrel::Round { .. } => lines.push(format!(
            "Inlet control, Table 7.2 round culverts (SDM p. 7-8), S0 read as {} in (S0/0,4)^0,05 (the manual does not state the unit): H1 = {inlet:.3} m.",
            match input.slope_unit {
                SlopeUnit::MetrePerMetre => format!("{} m/m", input.slope_m_per_m),
                SlopeUnit::Percent => format!("{} per cent", input.slope_m_per_m * 100.0),
            }
        )),
        SdmBarrel::Rectangular { inlet: i, .. } => lines.push(format!(
            "Inlet control, Table 7.2 rectangular culverts (SDM p. 7-8), {} inlet (CB = {}, Ch = {}): H1 = {inlet:.3} m.",
            if i == RectInlet::Rounded { "rounded" } else { "square" },
            i.cb(),
            i.ch()
        )),
    }
    lines.push(match input.friction {
        BarrelFriction::Manning { n } => format!(
            "Outlet control, Eq. 7.4 with Manning friction (SDM p. 7-7): H1 = H2 + (Kin + Kout)·v²/2g + v²·n²·L/R^(4/3) - S0·L with H2 = {} m, Kin = {}, Kout = {}, n = {n}: H1 = {outlet:.3} m.",
            input.design.h2_m, input.k_in, input.k_out
        ),
        BarrelFriction::Colebrook { ks_m } => format!(
            "Outlet control, Eq. 7.4 with Darcy friction as SAG Example 7.1 (p. 7-75, 7-76; Eq. 7.6, 7.7): H1 = H2 + (Kin + Kout)·v²/2g + λ·L·v²/(2gD) - S0·L with H2 = {} m, Kin = {}, Kout = {}, ks = {ks_m} m: H1 = {outlet:.3} m.",
            input.design.h2_m, input.k_in, input.k_out
        ),
    });
    lines.push(format!(
        "Governing upstream energy level (SDM 7.4.1, p. 7-10: the larger): H1 = {h1:.3} m, {} governs; H1/D = {ratio:.2} against {SUBMERGENCE_RATIO_LIMIT} at QT (SDM 8.3.2, p. 8-7): {}.",
        if inlet >= outlet { "inlet control" } else { "outlet control" },
        ok(design_ok)
    ));
    let mut twice = None;
    if let Some(t) = &input.twice {
        let (i2, o2, _) = controlling_h1(input, t)?;
        let h12 = i2.max(o2);
        let mut allowed = SUBMERGENCE_RATIO_AT_2T * d;
        let mut text = format!("2D = {allowed:.3} m");
        if let Some(sbp) = input.shoulder_break_height_m {
            allowed = allowed.min(sbp);
            text = format!("the smaller of 2D = {:.3} m and the shoulder break point {sbp:.3} m", SUBMERGENCE_RATIO_AT_2T * d);
        }
        let pass = h12 <= allowed;
        lines.push(format!(
            "Twice the return period, Q2T = {} m³/s: H1 = {h12:.3} m (inlet {i2:.3}, outlet {o2:.3}) against {text} (SDM 8.3.2, p. 8-7): {}.{}",
            t.flow_m3_s,
            ok(pass),
            if input.road_class.sbp_limit_applies() { "" } else { " (The shoulder break point requirement generally applies to road classes 1 to 3 only.)" }
        ));
        twice = Some((h12, allowed, pass));
    }
    let area = input.barrel.area_m2();
    let v = per_barrel / area;
    lines.push(format!(
        "Full-barrel velocity {v:.2} m/s; SDM 7.2.2.2 (p. 7-4): velocities through culverts should not be less than 1 m/s and the slope not less than about 1 %.{}",
        if v < 1.0 || input.slope_m_per_m < 0.01 { " Check: below that advice." } else { "" }
    ));
    Ok(SanralCulvertCheck { input: input.clone(), inlet_h1_m: inlet, outlet_h1_m: outlet, h1_m: h1, inlet_governs: inlet >= outlet, h1_over_d: ratio, design_ok, twice, lines })
}

/// The layout limits SDM states for culverts, for [`DesignBasis::culvert_layout_limits`]:
/// minimum diameter 600 mm up to 30 m long and 900 mm longer (7.2.2.5, p. 7-4: "600 mm in diameter
/// or a rectangular section of 750 mm (wide) x 450 mm (high)", "900 mm" over 30 m), and the slope
/// "not less than about 1%" (7.2.2.2, p. 7-4). No skew, no cover and no anchor limit is stated in
/// the chapter on culverts; SDM 9.3.4 (p. 9-18) gives a minimum cover of 0,9 m for storm drains
/// "for highway applications ... where possible", which is a rule for storm drains and is not
/// applied to culverts here.
pub fn culvert_layout_limits() -> CulvertLayoutLimits {
    CulvertLayoutLimits {
        minimum_diameter_m: Some(Limit { value: 0.6, citation: "SDM 7.2.2.5 (p. 7-4): the minimum recommended practical size for a culvert up to 30 m long is 600 mm in diameter" }),
        long_culvert: Some(LongCulvertRule {
            longer_than_m: 30.0,
            minimum_diameter_m: 0.9,
            citation: "SDM 7.2.2.5 (p. 7-4): for a culvert longer than 30 m a minimum diameter of 900 mm 'should be considered' to allow maintenance",
        }),
        maximum_skew_deg: None,
        gradient_not_below: None,
        gradient_avoid_below: Some(Limit { value: 0.01, citation: "SDM 7.2.2.2 (p. 7-4): 'the slope of a culvert should accordingly not be less than about 1%' so that velocities stay at or above 1 m/s" }),
        gradient_anchors_above: None,
        gradient_special_anchors_above: None,
        structure_above_diameter_m: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::ChannelGeometry;

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!((actual - expected).abs() <= tolerance, "expected {expected} ± {tolerance}, got {actual}");
    }

    /// SAG Example 3.1 (Moretele Spruit, pages 3-5 to 3-21), the rural and urban inputs of
    /// Tables 3.4 and 3.5, in the shares it prints.
    fn moretele(period: ReturnPeriod) -> RunoffInput {
        RunoffInput {
            rural: Some(RuralCatchment {
                map_mm: 746.6,
                slope: vec![(RuralSlope::VleisAndPans, 20.0), (RuralSlope::Flat, 70.0), (RuralSlope::Hilly, 10.0), (RuralSlope::Steep, 0.0)],
                permeability: vec![(Permeability::VeryPermeable, 0.0), (Permeability::Permeable, 50.0), (Permeability::SemiPermeable, 50.0), (Permeability::Impermeable, 0.0)],
                vegetation: vec![(Vegetation::ThickBushAndPlantation, 0.0), (Vegetation::LightBushAndFarmLands, 45.0), (Vegetation::Grasslands, 50.0), (Vegetation::NoVegetation, 5.0)],
                dolomite_fraction: 0.10,
                ft_class: FtClass::FlatAndPermeable,
            }),
            urban: vec![
                UrbanPart { land_use: UrbanUse::LawnsSandyFlat, percent: 20.0, factor: 0.10 },
                UrbanPart { land_use: UrbanUse::LawnsSandySteep, percent: 10.0, factor: 0.20 },
                UrbanPart { land_use: UrbanUse::Houses, percent: 40.0, factor: 0.50 },
                UrbanPart { land_use: UrbanUse::Flats, percent: 5.0, factor: 0.70 },
                UrbanPart { land_use: UrbanUse::LightIndustry, percent: 5.0, factor: 0.80 },
                UrbanPart { land_use: UrbanUse::BusinessSuburban, percent: 10.0, factor: 0.70 },
                UrbanPart { land_use: UrbanUse::BusinessStreets, percent: 10.0, factor: 0.95 },
            ],
            lakes_c3: 0.0,
            alpha: 0.4,
            beta: 0.6,
            gamma: 0.0,
            return_period: period,
        }
    }

    #[test]
    fn table_3_7_and_3_8_as_printed() {
        assert_eq!(MapBand::from_map_mm(599.9), MapBand::Below600);
        assert_eq!(MapBand::from_map_mm(600.0), MapBand::From600To900);
        assert_eq!(MapBand::from_map_mm(900.0), MapBand::From600To900);
        assert_eq!(MapBand::from_map_mm(900.1), MapBand::Above900);
        assert_eq!(RuralSlope::Steep.cs(MapBand::Above900), 0.30);
        assert_eq!(Permeability::Impermeable.cp(MapBand::Below600), 0.21);
        assert_eq!(Vegetation::Grasslands.cv(MapBand::From600To900), 0.21);
        assert_eq!(UrbanUse::BusinessStreets.range(), (0.70, 0.95));
        assert_eq!(return_period_factor(FtClass::FlatAndPermeable, ReturnPeriod::Years(20)).unwrap(), 0.67);
        assert_eq!(return_period_factor(FtClass::SteepAndImpermeable, ReturnPeriod::Years(2)).unwrap(), 0.75);
        assert_eq!(return_period_factor(FtClass::FlatAndPermeable, ReturnPeriod::MaximumFlood).unwrap(), 1.0);
        let e = return_period_factor(FtClass::FlatAndPermeable, ReturnPeriod::Years(25)).unwrap_err().to_string();
        assert!(e.contains("25 years is not tabulated"));
    }

    /// SAG Example 3.1, Steps 10 and 11: C1 = 0,3665, C1D = 0,337, C1(20) = 0,2258, C1(50) = 0,2797,
    /// C2 = 0,48, C20 = 0,3783, C50 = 0,3999.
    #[test]
    fn sag_example_3_1_runoff_coefficients() {
        let r20 = runoff_coefficient(&moretele(ReturnPeriod::Years(20))).unwrap();
        close(r20.c1, 0.3665, 0.00005);
        close(r20.c1d, 0.337, 0.0005);
        close(r20.c1t, 0.2258, 0.0001);
        close(r20.c2, 0.48, 1e-9);
        close(r20.ct, 0.3783, 0.0001);
        let r50 = runoff_coefficient(&moretele(ReturnPeriod::Years(50))).unwrap();
        close(r50.c1t, 0.2797, 0.0001);
        close(r50.ct, 0.3999, 0.0001);
        let text = r20.calculation().join("\n");
        assert!(text.starts_with("Design basis: SANRAL Drainage Manual") && text.contains("Table 3.7 (SDM p. 3-18)") && !text.contains("DDM") && !text.contains("DMRB"));
    }

    #[test]
    fn runoff_refuses_what_the_tables_do_not_print() {
        let mut bad = moretele(ReturnPeriod::Years(20));
        bad.urban[0].factor = 0.2; // outside 0,05 - 0,10
        assert!(runoff_coefficient(&bad).unwrap_err().to_string().contains("outside the range"));
        let mut shares = moretele(ReturnPeriod::Years(20));
        shares.rural.as_mut().unwrap().slope[0].1 = 25.0;
        assert!(runoff_coefficient(&shares).unwrap_err().to_string().contains("add up to 100"));
        let mut alpha = moretele(ReturnPeriod::Years(20));
        alpha.gamma = 0.1;
        assert!(runoff_coefficient(&alpha).is_err());
        // Maximum flood: Cpmax, Cvmax, C2 = 1, Ft = 1. Hand worked, column 600-900: Cs = 0,0016 + ... see below.
        let mf = runoff_coefficient(&moretele(ReturnPeriod::MaximumFlood)).unwrap();
        // Cs = 0,2×0,03 + 0,7×0,08 + 0,1×0,16 = 0,078; Cpmax = 0,26; Cvmax = 0,28; C1 = 0,618.
        close(mf.c1, 0.618, 1e-9);
        close(mf.c2, 1.0, 1e-9);
        close(mf.ft, 1.0, 1e-9);
    }

    /// SAG Example 3.1, Steps 3 and 4: Sav = 0,02146 from H0,10L = 1 412,1 m and H0,85L = 1 528,8 m,
    /// L = 7,25 km; Tc = 1,338 h. Kerby hand worked: r = 0,3, L = 0,2 km, S = 0,02:
    /// 0,604·(0,06/0,141421)^0,467 = 0,604 × 0,6701 = 0,4047 h.
    #[test]
    fn time_of_concentration_formulae() {
        let s = slope_10_85(1412.1, 1528.8, 7.25);
        close(s, 0.02146, 0.00001);
        close(scs_tc_h(7.25, 0.02146), 1.338, 0.001);
        close(kerby_tc_h(0.3, 0.2, 0.02), 0.4047, 0.0002);
        assert_eq!(KerbySurface::ThickGrassCover.r(), 0.8);
        let t = time_of_concentration(&[TcReach::Channel { length_km: 7.25, slope_m_per_m: s }]).unwrap();
        close(t.total_h, 1.338, 0.001);
        assert!(t.lines.iter().any(|l| l.contains("Eq. 3.12")));
        let sum = time_of_concentration(&[TcReach::Overland { r: 0.3, length_km: 0.2, slope_m_per_m: 0.02 }, TcReach::Urban { length_m: 360.0, velocity_m_s: 1.0 }]).unwrap();
        close(sum.total_h, 0.4047 + 0.1, 0.0003);
        assert!(time_of_concentration(&[]).is_err());
    }

    /// SAG Example 3.1 Alternative 2, Step 9b: ARF = 96 % for A = 28,5 km² and Tc = 1,338 h (Eq. 3.7
    /// of the guide, Eq. 3.10 of the manual).
    #[test]
    fn area_reduction_factor_equation() {
        close(area_reduction_factor_percent(28.5, 1.338).unwrap(), 96.0, 0.1);
        // Hand worked for A = 100 km², Tc = 2 h: (90000 - 58947 + 47060)^0,4 = 78113^0,4 = 90,6.
        close(area_reduction_factor_percent(100.0, 2.0).unwrap(), 90.6, 0.1);
        // Capped at 100 for a small, quick catchment.
        assert_eq!(area_reduction_factor_percent(1.0, 1.0).unwrap(), 100.0);
        assert!(area_reduction_factor_percent(0.0, 1.0).is_err());
    }

    fn peak(period: u32, p_mm: f64, arf: AreaReduction) -> RationalResult {
        rational_method(&RationalInput {
            runoff: moretele(ReturnPeriod::Years(period)),
            area_km2: 28.5,
            time_of_concentration_h: 1.338,
            rainfall: RainfallInput { point_rainfall_mm: p_mm, source: "SAG Example 3.1".into(), area_reduction: arf },
        })
        .unwrap()
    }

    /// SAG Example 3.1, Step 11 (p. 3-21). Alternative 1: P20 = 78 mm, P50 = 104 mm, ARF 94 % and 91 %
    /// read from a figure: Pi 58,3 and 77,7 mm/h, I 54,79 and 70,71 mm/h, Q 164 and 224 m³/s.
    /// Alternative 2: Pt20 = 85,55 and Pt50 = 107,10 mm, ARF 96 %: I 61,4 and 76,9 mm/h, Q 184 and
    /// 243 m³/s. Alternative 3: 57 and 70 mm with no area reduction: Q 128 and 166 m³/s.
    #[test]
    fn sag_example_3_1_peak_flows() {
        let a20 = peak(20, 78.0, AreaReduction::Entered(94.0));
        close(a20.point_intensity_mm_h, 58.3, 0.02);
        close(a20.intensity_mm_h, 54.79, 0.02);
        close(a20.peak_flow_m3_s, 164.0, 0.5);
        let a50 = peak(50, 104.0, AreaReduction::Entered(91.0));
        close(a50.point_intensity_mm_h, 77.7, 0.05);
        close(a50.intensity_mm_h, 70.71, 0.05);
        close(a50.peak_flow_m3_s, 224.0, 0.5);
        let b20 = peak(20, 85.55, AreaReduction::Equation);
        close(b20.point_intensity_mm_h, 63.93, 0.02);
        close(b20.area_reduction_percent, 96.0, 0.1);
        close(b20.intensity_mm_h, 61.4, 0.1);
        close(b20.peak_flow_m3_s, 184.0, 0.7);
        let b50 = peak(50, 107.10, AreaReduction::Equation);
        close(b50.intensity_mm_h, 76.9, 0.1);
        close(b50.peak_flow_m3_s, 243.0, 0.8);
        let c20 = peak(20, 57.0, AreaReduction::NotApplied);
        close(c20.point_intensity_mm_h, 42.6, 0.02);
        close(c20.peak_flow_m3_s, 128.0, 0.5);
        let c50 = peak(50, 70.0, AreaReduction::NotApplied);
        close(c50.peak_flow_m3_s, 166.0, 0.5);
        // The catchment is above 15 km².
        assert!(a20.calculation().iter().any(|l| l.contains("above the 15 km²")));
        // A Tc under 15 minutes is held at 0,25 h for the intensity.
        let quick = rational_method(&RationalInput {
            runoff: moretele(ReturnPeriod::Years(20)),
            area_km2: 1.0,
            time_of_concentration_h: 0.1,
            rainfall: RainfallInput { point_rainfall_mm: 20.0, source: "entered".into(), area_reduction: AreaReduction::NotApplied },
        })
        .unwrap();
        close(quick.point_intensity_mm_h, 80.0, 1e-9);
        assert_eq!(quick.intensity_duration_h, 0.25);
    }

    /// SAG Example 7.1 (p. 7-69): Class 3, Q20 = 85 m³/s gives T = 15 years from the figure.
    #[test]
    fn return_period_by_road_class() {
        close(design_return_period_years(RoadClass::Class3, 85.0).unwrap(), 15.0, 1e-9);
        close(design_return_period_years(RoadClass::Class1, 20.0).unwrap(), 50.0, 1e-9);
        close(design_return_period_years(RoadClass::Class1, 150.0).unwrap(), 100.0, 1e-9);
        close(design_return_period_years(RoadClass::Class1, 185.0).unwrap(), 100.0, 1e-9);
        close(design_return_period_years(RoadClass::Class2, 85.0).unwrap(), 35.0, 1e-9);
        close(design_return_period_years(RoadClass::Class3, 5.0).unwrap(), 10.0, 1e-9);
        close(design_return_period_years(RoadClass::Class4Or5, 120.0).unwrap(), 10.0, 1e-9);
        close(design_return_period_years(RoadClass::Class6, 120.0).unwrap(), 5.0, 1e-9);
        assert!(design_return_period_years(RoadClass::Class1, 250.0).unwrap_err().to_string().contains("beyond"));
        assert!(RoadClass::Class3.sbp_limit_applies() && !RoadClass::Class4Or5.sbp_limit_applies());
    }

    #[test]
    fn figure_8_3_freeboard_line() {
        close(free_flow_freeboard_m(1000.0).unwrap(), 1.08, 1e-9);
        close(free_flow_freeboard_m(100000.0).unwrap(), 2.64, 1e-9);
        assert_eq!(free_flow_freeboard_m(20.0).unwrap(), 0.0);
        close(free_flow_freeboard_m(41.25).unwrap(), 0.0, 0.002);
    }

    /// Table 5.3 hand worked: trapezoidal, y = 0,5 m, v = 1,2 m/s, subcritical:
    /// E = 0,5 + 1,44/19,62 = 0,57339, freeboard 0,20·E = 0,11468 m. Supercritical: 0,30·y = 0,15.
    /// Table 5.4 hand worked: v = 1,2, b = 1, K = 1,5, y = 0,5, r = 20: subcritical trapezoid
    /// 1,44 × 2,5 / (2 × (196,2 - 4,32)) = 3,6/383,76 = 0,009381; rectangular 3·1,44·1/(4·9,81·20) = 0,005505.
    #[test]
    fn tables_5_3_and_5_4() {
        close(channel_freeboard_m(ChannelKind::Trapezoidal, 0.5, 0.5, 1.2), 0.11468, 0.00001);
        close(channel_freeboard_m(ChannelKind::Rectangular, 0.5, 0.5, 1.2), 0.15 * 0.57339, 0.00001);
        close(channel_freeboard_m(ChannelKind::Trapezoidal, 1.5, 0.5, 1.2), 0.15, 1e-12);
        close(channel_freeboard_m(ChannelKind::Rectangular, 1.0, 0.5, 1.2), 0.125, 1e-12);
        assert!(channel_freeboard_note(0.9).is_some() && channel_freeboard_note(0.5).is_none());
        close(bend_additional_freeboard_m(ChannelKind::Trapezoidal, 0.5, 1.2, 1.0, 20.0, 1.5, 0.5).unwrap(), 0.009381, 0.000001);
        close(bend_additional_freeboard_m(ChannelKind::Rectangular, 0.5, 1.2, 1.0, 20.0, 0.0, 0.5).unwrap(), 0.005505, 0.000001);
        close(bend_additional_freeboard_m(ChannelKind::Rectangular, 1.5, 1.2, 1.0, 20.0, 0.0, 0.5).unwrap(), 0.008807, 0.000001);
        // r under three times the surface width (b + 2Ky = 2,5 m, so under 7,5 m) is refused.
        assert!(bend_additional_freeboard_m(ChannelKind::Trapezoidal, 0.5, 1.2, 1.0, 7.0, 1.5, 0.5).is_err());
    }

    #[test]
    fn table_5_6_and_5_5() {
        assert_eq!(grass_permissible_velocity_m_s(Grass::Kikuyu, GrassRainfall::From600To700, ClayContent::Over15).unwrap(), 1.8);
        assert_eq!(grass_permissible_velocity_m_s(Grass::ECurvula, GrassRainfall::Below600, ClayContent::Under6).unwrap(), 0.8);
        assert_eq!(grass_permissible_velocity_m_s(Grass::PaspalumDidatum, GrassRainfall::Above700, ClayContent::From6To15).unwrap(), 1.5);
        assert_eq!(grass_permissible_velocity_m_s(Grass::Kikuyu, GrassRainfall::Above700, ClayContent::Over15).unwrap(), 2.5);
        assert!(grass_permissible_velocity_m_s(Grass::Kikuyu, GrassRainfall::Below600, ClayContent::Over15).unwrap_err().to_string().contains("No data"));
        assert!(grass_permissible_velocity_m_s(Grass::BlueBuffalo, GrassRainfall::Above700, ClayContent::Under6).is_err());
        assert_eq!(steepest_side_slope_h_per_v(SideSlopeMaterial::GrassedChannels), (Some(3.0), Some(4.0)));
        assert_eq!(steepest_side_slope_h_per_v(SideSlopeMaterial::Rock), (None, None));
        assert_eq!(steepest_side_slope_h_per_v(SideSlopeMaterial::LooseSandySoil).0, Some(2.5));
    }

    fn channel_input() -> ChannelInput {
        ChannelInput {
            basis: DesignBasis::Sanral,
            geometry: ChannelGeometry::trapezoid(1.0, 1.5),
            depth_m: 0.9,
            manning_n: CONCRETE_SIDE_DRAIN_MANNING_N,
            roughness_source: "SDM p. 4-17, Photograph 4.5 (concrete side drain)".into(),
            gradient_m_per_m: 0.004,
            design_flow_m3_s: 1.0,
            permissible_velocity_m_s: Some(CONCRETE_LINING_JOINTED_VELOCITY_M_S),
            permissible_source: "SDM 5.4.6.1, p. 5-25".into(),
            required_freeboard_m: None,
        }
    }

    #[test]
    fn channel_check_names_the_sanral_basis_and_applies_table_5_3() {
        let c = check_channel_sanral(&channel_input(), ChannelKind::Trapezoidal, None).unwrap();
        let y = c.check.normal_depth_m;
        let e = y + c.check.velocity_m_s.powi(2) / (2.0 * 9.81);
        assert!(c.check.froude < 1.0);
        close(c.required_freeboard_m, 0.20 * e, 1e-9);
        assert_eq!(c.freeboard_ok, c.check.freeboard_m >= c.required_freeboard_m);
        let text = c.calculation().join("\n");
        assert!(text.starts_with("Design basis: SANRAL Drainage Manual") && text.contains("SDM Table 5.3") && text.contains("SDM Equation 4.27"));
        assert!(!text.contains("DDM") && !text.contains("CD 521"));
        // With a bend.
        let bend = check_channel_sanral(&channel_input(), ChannelKind::Trapezoidal, Some(40.0)).unwrap();
        assert!(bend.required_freeboard_m > c.required_freeboard_m && bend.bend_freeboard_m.is_some());
        // Another basis is refused.
        let mut other = channel_input();
        other.basis = DesignBasis::Mowt;
        assert!(check_channel_sanral(&other, ChannelKind::Trapezoidal, None).is_err());
    }

    /// SAG Example 7.1 (p. 7-72): D = 1,8 m, H1 = 2,5 m, orifice relation, Q = 8,55 m³/s per culvert.
    /// Box 1,8 m by 1,8 m, square inlet, H1 = 2,4 m: Q = 9,893 m³/s (p. 7-73).
    #[test]
    fn sag_example_7_1_inlet_control() {
        let q = round_inlet_flow_m3_s(2.5, 1.8, 0.0015, SlopeUnit::MetrePerMetre).unwrap();
        close(q, 8.55, 0.01);
        close(44.5 / q, 5.20, 0.01);
        // The orifice relation has no slope: the same result whichever unit is chosen.
        close(round_inlet_flow_m3_s(2.5, 1.8, 0.0015, SlopeUnit::Percent).unwrap(), q, 1e-12);
        close(round_inlet_h1_m(q, 1.8, 0.0015, SlopeUnit::MetrePerMetre).unwrap(), 2.5, 1e-9);
        let b = rect_inlet_flow_m3_s(2.4, 1.8, 1.8, RectInlet::Square).unwrap();
        close(b, 9.893, 0.001);
        close(rect_inlet_h1_m(b, 1.8, 1.8, RectInlet::Square).unwrap(), 2.4, 1e-9);
        // The Figure 7.4 reading in the example (H1 = 2,4 m for 8,9 m³/s) is not what the relation
        // gives: the orifice relation gives H1 = 1,08 + (8,9/1,944)²/19,62 = 2,148 m, which is below
        // 1,2 D = 2,16 m, where the weir relation holds, so the result is held at 1,2 D.
        close(rect_inlet_h1_m(8.9, 1.8, 1.8, RectInlet::Square).unwrap(), 2.16, 1e-9);
        close(rect_inlet_h1_m(9.5, 1.8, 1.8, RectInlet::Square).unwrap(), 1.08 + (9.5f64 / 1.944).powi(2) / 19.62, 1e-9);
    }

    /// Table 7.2 hand worked, round, weir zones. D = 1 m, S0 = 0,004 m/m read in m/m:
    /// f = (0,004/0,4)^0,05 = 0,01^0,05 = 0,79433. H1 = 0,6 m (H1/D = 0,6):
    /// Q = 0,48 × 0,79433 × 0,6^1,9 × 1 × √9,81 = 0,38128 × 0,37888 × 3,13209 = 0,45246 m³/s.
    /// H1 = 1,0 m (H1/D = 1,0): Q = 0,44 × 0,79433 × 1 × 3,13209 = 1,09469 m³/s.
    #[test]
    fn table_7_2_round_weir_relations_and_the_slope_unit() {
        close(round_inlet_flow_m3_s(0.6, 1.0, 0.004, SlopeUnit::MetrePerMetre).unwrap(), 0.45246, 0.0001);
        close(round_inlet_flow_m3_s(1.0, 1.0, 0.004, SlopeUnit::MetrePerMetre).unwrap(), 1.09469, 0.0003);
        // The same culvert with S0 read in per cent (0,4 %) has f = 1: 0,44 × 3,13209 = 1,37812.
        close(round_inlet_flow_m3_s(1.0, 1.0, 0.004, SlopeUnit::Percent).unwrap(), 1.37812, 0.0003);
        // The two weir relations meet at H1/D = 0,8 (0,3140 and 0,3148 in Q/(D²√(gD)) with f = 1).
        let below = round_inlet_flow_m3_s(0.7999, 1.0, 0.004, SlopeUnit::Percent).unwrap();
        let above = round_inlet_flow_m3_s(0.8001, 1.0, 0.004, SlopeUnit::Percent).unwrap();
        close(above / below, 1.0, 0.004);
        // Inversion in every zone.
        for h in [0.3, 0.6, 0.79, 0.85, 1.1, 1.19] {
            let q = round_inlet_flow_m3_s(h, 1.0, 0.004, SlopeUnit::MetrePerMetre).unwrap();
            close(round_inlet_h1_m(q, 1.0, 0.004, SlopeUnit::MetrePerMetre).unwrap(), h, 1e-9);
        }
        assert!(round_inlet_flow_m3_s(1.0, 1.0, 0.0, SlopeUnit::MetrePerMetre).is_err());
        // Rectangular weir hand worked: B = 2, H1 = 1 m, D = 1,2 m, rounded inlet:
        // (2/3)·1·2·1·√(6,54) = 1,3333 × 2,5573 = 3,4097 m³/s.
        close(rect_inlet_flow_m3_s(1.0, 2.0, 1.2, RectInlet::Rounded).unwrap(), 3.4097, 0.0005);
        // The discontinuity at 1,2 is not hidden: the inverse never returns less than 1,2 D above the weir flow.
        let weir_max = rect_inlet_flow_m3_s(1.2, 1.0, 1.0, RectInlet::Square).unwrap();
        let h = rect_inlet_h1_m(weir_max * 1.001, 1.0, 1.0, RectInlet::Square).unwrap();
        assert!(h >= 1.2);
    }

    /// SAG Example 7.1, outlet control (p. 7-75, 7-76): six 1,8 m pipes, L = 25 m, S0 = 0,0015,
    /// ks = 0,002 m, Kin = 0,5, Kout = 1, H2 = 2,0 + 0,175 m: hl = 0,649, hf = 0,121, H1 = 2,908 m.
    /// Seven pipes: H1 = 2,704 m.
    #[test]
    fn sag_example_7_1_outlet_control() {
        let barrel = SdmBarrel::Round { diameter_m: 1.8 };
        let h1 = outlet_control_h1_m(44.5 / 6.0, &barrel, 25.0, 0.0015 * 25.0, 0.5, 1.0, BarrelFriction::Colebrook { ks_m: 0.002 }, 2.175).unwrap();
        close(h1, 2.908, 0.002);
        let h7 = outlet_control_h1_m(44.5 / 7.0, &barrel, 25.0, 0.0015 * 25.0, 0.5, 1.0, BarrelFriction::Colebrook { ks_m: 0.002 }, 2.175).unwrap();
        close(h7, 2.704, 0.002);
        // Manning (Eq. 7.4) hand worked, one 1,8 m pipe, Q = 5 m³/s, n = 0,016, L = 25 m:
        // A = 2,54469, v = 1,96488, v²/2g = 0,19677; R = 0,45, R^(4/3) = 0,34504; hf = 3,86075 × 0,000256 × 25 / 0,34504 = 0,07161;
        // H1 = 1,5 + (1,5 × 0,19677) + 0,07161 - 0,0375 = 1,5 + 0,29516 + 0,03411 = 1,82927.
        let m = outlet_control_h1_m(5.0, &barrel, 25.0, 0.0375, 0.5, 1.0, BarrelFriction::Manning { n: CONCRETE_CULVERT_MANNING_N }, 1.5).unwrap();
        close(m, 1.8293, 0.001);
        assert!(outlet_control_h1_m(5.0, &SdmBarrel::Rectangular { width_m: 2.0, height_m: 1.0, inlet: RectInlet::Square }, 25.0, 0.0, 0.5, 1.0, BarrelFriction::Colebrook { ks_m: 0.002 }, 1.0).is_err());
    }

    fn culvert(barrels: u32, q: f64) -> SanralCulvertInput {
        SanralCulvertInput {
            barrel: SdmBarrel::Round { diameter_m: 1.8 },
            barrels,
            length_m: 25.0,
            slope_m_per_m: 0.0015,
            slope_unit: SlopeUnit::MetrePerMetre,
            k_in: 0.5,
            k_out: 1.0,
            friction: BarrelFriction::Colebrook { ks_m: 0.002 },
            design: CulvertFlood { flow_m3_s: q, h2_m: 2.175 },
            twice: None,
            shoulder_break_height_m: Some(2.5),
            road_class: RoadClass::Class3,
        }
    }

    /// The greater-of rule on SAG Example 7.1: with six pipes outlet control gives 2,908 m and
    /// governs (the guide concludes "outlet control will be maintained"); inlet control gives
    /// 0,6-orifice H1 of 2,5 m only for 8,55 m³/s, here less because each pipe carries 7,417.
    #[test]
    fn culvert_check_takes_the_greater_and_names_it() {
        let c = culvert_check_sanral(&culvert(6, 44.5)).unwrap();
        assert!(!c.inlet_governs);
        close(c.outlet_h1_m, 2.908, 0.002);
        assert!(c.inlet_h1_m < c.outlet_h1_m);
        close(c.h1_m, c.outlet_h1_m, 0.0);
        close(c.h1_over_d, 2.908 / 1.8, 0.002);
        assert!(!c.design_ok && !c.passes()); // 1,62 > 1,2
        let text = c.calculation().join("\n");
        assert!(text.starts_with("Design basis: SANRAL Drainage Manual") && text.contains("outlet control governs") && text.contains("SDM 8.3.2"));
        // Twice the return period against min(2D, SBP).
        let mut twice = culvert(8, 44.5);
        twice.twice = Some(CulvertFlood { flow_m3_s: 60.0, h2_m: 2.3 });
        let c2 = culvert_check_sanral(&twice).unwrap();
        let (_, allowed, _) = c2.twice.unwrap();
        close(allowed, 2.5, 1e-12); // the smaller of 2D = 3,6 and SBP = 2,5
        assert!(c2.calculation().iter().any(|l| l.contains("Twice the return period")));
        assert!(culvert_check_sanral(&culvert(0, 10.0)).is_err());
    }

    #[test]
    fn layout_limits_are_sanrals() {
        let l = culvert_layout_limits();
        assert_eq!(l.minimum_diameter_m.unwrap().value, 0.6);
        let long = l.long_culvert.unwrap();
        assert_eq!((long.longer_than_m, long.minimum_diameter_m), (30.0, 0.9));
        assert_eq!(l.gradient_avoid_below.unwrap().value, 0.01);
        assert!(l.maximum_skew_deg.is_none() && l.gradient_not_below.is_none());
        assert_eq!(DesignBasis::Sanral.culvert_layout_limits(), l);
        assert_eq!(DesignBasis::Sanral.basis_line(), BASIS);
    }
}
