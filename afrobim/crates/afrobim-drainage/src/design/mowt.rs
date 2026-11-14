//! The MoWT (Uganda) design basis for side drains and pipe culverts.
//!
//! # Design basis
//!
//! Ministry of Works and Transport (Uganda), *Road Design Manual Volume 2: Drainage Design*,
//! January 2010, cited below as "DDM". Every number in this module that comes from the manual
//! names its section, table or equation where it is defined.
//!
//! * **Design recurrence interval**: DDM Table 3.2 (side ditches and culverts up to 6 m total
//!   opening), [`design_recurrence_years`].
//! * **Peak flow**: the Rational method, `Q = C·i·A / 360` (DDM Eqn. 4.10; Q in m³/s, i in
//!   mm/h, A in ha), with the area-weighted coefficient of Eqn. 4.6, the runoff coefficients of
//!   Tables 4.7(a) and 4.7(b), the recurrence multipliers of Table 4.8 and the storage factor
//!   `Cs = 2tc / (2tc + td)` of Eqn. 4.7 (Modified Rational method, Eqn. 4.11).
//! * **Time of concentration**: the two formulae of DDM Table 4.1 that the manual's own Example
//!   4-1 exercises: sheet flow (No. 8, HEC 22) with the roughness of Table 4-3, and the velocity
//!   method (No. 7, HDS 2) with the intercepts of Table 4-2; plus flow time in a channel or pipe
//!   as length over velocity (Section 4.2.1 d) and the minimum times of Table 4.4.
//! * **Open channels** (through [`super::check_channel`] with [`super::DesignBasis::Mowt`]):
//!   Manning's equation `Q = (1/n)·A·R^(2/3)·S^(1/2)` (the form used in DDM
//!   Example 4-1, Section 9.5.1 Step 12 and Appendix 10.4) with area, wetted perimeter and
//!   hydraulic radius in closed form for the trapezoid family, normal depth by bisection, the
//!   permissible velocities of DDM Table 9.1 and the Froude number.
//! * **Pipe culverts, outlet control only**: barrel losses `H = [1 + Ke + 19.63 n² L / R^1.33]
//!   · V²/2g` (DDM Eqn. 9.6), headwater `HW = H + ho − So·L` with `ho` the larger of the
//!   tailwater and `(dc + D)/2` (Eqns. 9.9 and 9.11), entrance loss coefficients of Appendix
//!   9.6, Manning's n ranges of Appendix 9.7, the headwater limit `HW/D ≤ 1.5` of Section 9.3
//!   and the freeboard of Section 9.6.
//!
//! # Inputs, not defaults
//!
//! * **Rainfall intensity.** The manual gives the form `i = a / (b + td)^c` (Eqn. 4.1,
//!   [`idf_intensity_mm_h`]) and tables of `a` and `c` for East African stations from TRRL
//!   Laboratory Report 623 (Tables 4.5 and 4.6). Those tables are not held here: both are
//!   headed "Group I", they give different constants for Entebbe, Kampala and Gulu without
//!   saying which applies, they stop at 10 years where Table 3.2 asks for up to 100, and the
//!   manual's working route to a design storm (Section 4.3) goes through the rainfall maps of
//!   Appendices 4.3 to 4.5, which are charts. The engineer enters the intensity and its source.
//! * **Manning's n for an open drain.** Table 9.1 prints a "roughness coefficient" of 25 to 60
//!   beside each permissible velocity without naming the formula it belongs to. Values of that
//!   size are the Strickler form (K = 1/n), not Manning's n; that reading is this module's, not
//!   the manual's, so n is entered and [`ChannelMaterial::manning_n_if_strickler`] is offered
//!   as a labelled conversion only.
//! * **Freeboard of an open drain.** The manual states none (Section 9.6 is about the headwater
//!   at a drainage structure). The check uses a required freeboard only when one is entered.
//! * **Tailwater** at a culvert, and the level of the shoulder break point.
//!
//! # What is not here
//!
//! * Culvert **inlet control**: the manual gives it only as nomographs (Appendices 9.3 to 9.5).
//!   The governing headwater is the higher of inlet and outlet control (Section 9.5.1 Step 9),
//!   so [`outlet_control`] alone does not size a culvert.
//! * Backwater profiles, flow over the road, performance curves, the approximate capacity
//!   tables of Appendices 9.19 to 9.21, box and slab culverts, bridges, energy dissipators and
//!   scour.
//! * The other time-of-concentration formulae of Table 4.1 (Nos. 1 to 6): the manual has no
//!   worked example to verify a transcription against. Eqns. 4.2 and 4.3 are left out because
//!   they contradict Table 4-2 (see the README).
//! * The SCS, unit hydrograph and TRRL East African flood models, flood routing, gutters,
//!   inlets, storm sewers and subsurface drains.
//! * The critical depth in a pipe is solved from `Q²·T / (g·A³) = 1` instead of being read
//!   from the chart of Appendix 9.10.
//!
//! The result is a calculation for an engineer to check, not a certified design.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use super::hydraulics::circular_critical_depth_m;
use super::{CulvertLayoutLimits, Limit};
use crate::{positive, DrainageError};

/// The first line of every calculation to this basis.
pub const BASIS: &str = "Design basis: MoWT Road Design Manual Vol. 2: Drainage Design (January 2010), cited as DDM.";

/// Acceleration due to gravity as DDM defines it for culvert velocity heads (under Eqn. 9.4):
/// "9.8 m/s2". The manual is not consistent: its Appendix 10.4 uses 9.81 in the Froude number
/// and the 19.63 of Eqn. 9.5b is 2 × 9.815.
pub const GRAVITY_CULVERT_M_S2: f64 = 9.8;
/// The constant of the friction term of DDM Eqn. 9.5b, as printed.
pub const FRICTION_CONSTANT: f64 = 19.63;
/// The exponent of the hydraulic radius in DDM Eqn. 9.5b, as printed (4/3 rounded).
pub const FRICTION_RADIUS_EXPONENT: f64 = 1.33;

fn invalid<T>(message: impl Into<String>) -> Result<T, DrainageError> {
    Err(DrainageError::InvalidInput(message.into()))
}

// ---------------------------------------------------------------------------------------------
// Design recurrence interval
// ---------------------------------------------------------------------------------------------

/// The columns of DDM Table 3.2, with the headings exactly as printed.
///
/// The printed headings are ambiguous: "PIII" appears in two columns and "PII", which the
/// table's legend defines, in none. The second column is presumably Paved II with Gravel A, but
/// the manual does not say so; choose the column, not a road class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DesignStandard {
    /// Column 1, printed "PIa, PIb".
    PavedIaIb,
    /// Column 2, printed "PIII Gravel A" (see the type's note).
    Column2PavedIiiGravelA,
    /// Column 3, printed "PIII, Gravel B".
    Column3PavedIiiGravelB,
    /// Column 4, printed "Gravel C".
    GravelC,
}

impl DesignStandard {
    /// The column heading as printed.
    pub fn printed_heading(self) -> &'static str {
        match self {
            DesignStandard::PavedIaIb => "PIa, PIb",
            DesignStandard::Column2PavedIiiGravelA => "PIII Gravel A",
            DesignStandard::Column3PavedIiiGravelB => "PIII, Gravel B",
            DesignStandard::GravelC => "Gravel C",
        }
    }
}

/// The rows of DDM Table 3.2 that this crate's elements fall under. The gutter, ford and bridge
/// rows are not held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructureType {
    /// "Side Ditches".
    SideDitch,
    /// "Culvert, pipe (see Note) Span < 2m".
    PipeCulvertSpanUnder2m,
    /// "Culvert, 2m < span < 6m".
    CulvertSpan2To6m,
}

impl StructureType {
    pub fn label(self) -> &'static str {
        match self {
            StructureType::SideDitch => "Side ditches",
            StructureType::PipeCulvertSpanUnder2m => "Culvert, pipe, span < 2 m",
            StructureType::CulvertSpan2To6m => "Culvert, 2 m < span < 6 m",
        }
    }
}

/// The design average recurrence interval, years, of DDM Table 3.2 ("could be used for initial
/// planning, design and analysis", Section 3.2.3).
pub fn design_recurrence_years(structure: StructureType, standard: DesignStandard) -> u32 {
    // DDM Table 3.2, columns in the order of `DesignStandard`.
    let row: [u32; 4] = match structure {
        StructureType::SideDitch => [10, 10, 5, 5],
        StructureType::PipeCulvertSpanUnder2m => [25, 10, 5, 5],
        StructureType::CulvertSpan2To6m => [50, 25, 10, 10],
    };
    row[match standard {
        DesignStandard::PavedIaIb => 0,
        DesignStandard::Column2PavedIiiGravelA => 1,
        DesignStandard::Column3PavedIiiGravelB => 2,
        DesignStandard::GravelC => 3,
    }]
}

/// The Table 3.2 row for a culvert of `barrels` pipes of the given internal diameter. Note 1 to
/// the table makes the span "the total clear-opening length of a structure", so two 1.2 m pipes
/// are a 2.4 m span.
///
/// The table's rows are "< 2 m" and "2 m < span < 6 m", so a span of exactly 2 m is in neither;
/// here it takes the longer-interval row. `None` at 6 m and over (the bridge rows, not held).
pub fn culvert_structure_type(barrels: u32, internal_diameter_m: f64) -> Option<StructureType> {
    let span = barrels as f64 * internal_diameter_m;
    if !(span > 0.0) {
        None
    } else if span < 2.0 {
        Some(StructureType::PipeCulvertSpanUnder2m)
    } else if span < 6.0 {
        Some(StructureType::CulvertSpan2To6m)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// Runoff coefficients
// ---------------------------------------------------------------------------------------------

/// The rows of DDM Table 4.7(a), "Typical Values of Rational Coefficients for Urban Areas".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UrbanLandUse {
    LawnSandySoilFlat,
    LawnSandySoilSteep,
    LawnHeavySoilFlat,
    LawnHeavySoilSteep,
    ResidentialSingleFamily,
    ResidentialApartments,
    IndustrialLight,
    IndustrialHeavy,
    BusinessDowntown,
    BusinessNeighbourhood,
    StreetAsphaltic,
    StreetConcrete,
    StreetBricks,
    Roofs,
}

impl UrbanLandUse {
    pub const ALL: [UrbanLandUse; 14] = [
        UrbanLandUse::LawnSandySoilFlat,
        UrbanLandUse::LawnSandySoilSteep,
        UrbanLandUse::LawnHeavySoilFlat,
        UrbanLandUse::LawnHeavySoilSteep,
        UrbanLandUse::ResidentialSingleFamily,
        UrbanLandUse::ResidentialApartments,
        UrbanLandUse::IndustrialLight,
        UrbanLandUse::IndustrialHeavy,
        UrbanLandUse::BusinessDowntown,
        UrbanLandUse::BusinessNeighbourhood,
        UrbanLandUse::StreetAsphaltic,
        UrbanLandUse::StreetConcrete,
        UrbanLandUse::StreetBricks,
        UrbanLandUse::Roofs,
    ];

    /// The row as printed.
    pub fn label(self) -> &'static str {
        match self {
            UrbanLandUse::LawnSandySoilFlat => "Lawns, sandy soil, flat < 2%",
            UrbanLandUse::LawnSandySoilSteep => "Lawns, sandy soil, steep > 7%",
            UrbanLandUse::LawnHeavySoilFlat => "Lawns, heavy soil, flat < 2%",
            UrbanLandUse::LawnHeavySoilSteep => "Lawns, heavy soil, steep > 7%",
            UrbanLandUse::ResidentialSingleFamily => "Residential, single family areas",
            UrbanLandUse::ResidentialApartments => "Residential, apartment dwelling areas",
            UrbanLandUse::IndustrialLight => "Industrial, light areas",
            UrbanLandUse::IndustrialHeavy => "Industrial, heavy areas",
            UrbanLandUse::BusinessDowntown => "Business, downtown areas",
            UrbanLandUse::BusinessNeighbourhood => "Business, neighbourhood areas",
            UrbanLandUse::StreetAsphaltic => "Streets, asphaltic",
            UrbanLandUse::StreetConcrete => "Streets, concrete",
            UrbanLandUse::StreetBricks => "Streets, bricks",
            UrbanLandUse::Roofs => "Roofs",
        }
    }

    /// The range of C the table gives, (low, high). The table gives ranges only; the value
    /// within the range is the engineer's choice. It has no rows for lawns between 2% and 7%.
    pub fn c_range(self) -> (f64, f64) {
        // DDM Table 4.7(a).
        match self {
            UrbanLandUse::LawnSandySoilFlat => (0.05, 0.10),
            UrbanLandUse::LawnSandySoilSteep => (0.15, 0.20),
            UrbanLandUse::LawnHeavySoilFlat => (0.13, 0.17),
            UrbanLandUse::LawnHeavySoilSteep => (0.25, 0.35),
            UrbanLandUse::ResidentialSingleFamily => (0.30, 0.50),
            UrbanLandUse::ResidentialApartments => (0.50, 0.70),
            UrbanLandUse::IndustrialLight => (0.50, 0.80),
            UrbanLandUse::IndustrialHeavy => (0.60, 0.90),
            UrbanLandUse::BusinessDowntown => (0.70, 0.95),
            UrbanLandUse::BusinessNeighbourhood => (0.50, 0.70),
            UrbanLandUse::StreetAsphaltic => (0.70, 0.95),
            UrbanLandUse::StreetConcrete => (0.80, 0.95),
            UrbanLandUse::StreetBricks => (0.70, 0.85),
            UrbanLandUse::Roofs => (0.75, 0.95),
        }
    }
}

/// Mean annual precipitation band of DDM Table 4.7(b), mm. The table does not say which band
/// exactly 300 mm or 600 mm belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RainfallBand {
    Under300,
    From300To600,
    Over600,
}

/// "Average slope of hillsides in catchment", factor Cs of DDM Table 4.7(b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HillSlope {
    /// Printed "3.5% flat" (read as under 3.5%).
    Flat,
    /// "3.5%-11% soft to moderate".
    SoftToModerate,
    /// "11%-35% steep".
    Steep,
    /// ">35% very steep".
    VerySteep,
}

/// "Permeability of soil", factor Ck of DDM Table 4.7(b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SoilPermeability {
    VeryPermeable,
    Permeable,
    SemiPermeable,
    Impermeable,
}

/// "Vegetation", factor Cv of DDM Table 4.7(b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Vegetation {
    DenseForestOrVeryLooseDeposits,
    CultivatedLandOrThinForest,
    Grassland,
    BareRock,
}

/// The multipliers of the Note under DDM Table 4.7(b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuralAdjustment {
    /// No note applies: C = Cs + Ck + Cv.
    None,
    /// Note 1, contour cultivated lands: × 0.80.
    ContourCultivated,
    /// Note 2, 100% dense wood, flat < 3.5%: × 0.60.
    DenseWoodFlat,
    /// Note 2, 100% dense wood, steep > 11%: × 0.80. (The note gives nothing between 3.5% and 11%.)
    DenseWoodSteep,
    /// Note 3, lakes, swamps and dams: × 1.00.
    LakesSwampsDams,
}

/// The rural runoff coefficient of DDM Table 4.7(b): `C = factor × (Cs + Ck + Cv)`.
pub fn rural_runoff_coefficient(band: RainfallBand, slope: HillSlope, soil: SoilPermeability, vegetation: Vegetation, adjustment: RuralAdjustment) -> f64 {
    let col = match band {
        RainfallBand::Under300 => 0,
        RainfallBand::From300To600 => 1,
        RainfallBand::Over600 => 2,
    };
    // DDM Table 4.7(b), MAP columns < 300, 300-600, > 600 mm.
    let cs: [f64; 3] = match slope {
        HillSlope::Flat => [0.01, 0.02, 0.03],
        HillSlope::SoftToModerate => [0.04, 0.06, 0.08],
        HillSlope::Steep => [0.09, 0.12, 0.16],
        HillSlope::VerySteep => [0.18, 0.22, 0.26],
    };
    let ck: [f64; 3] = match soil {
        SoilPermeability::VeryPermeable => [0.02, 0.03, 0.04],
        SoilPermeability::Permeable => [0.04, 0.06, 0.08],
        SoilPermeability::SemiPermeable => [0.08, 0.12, 0.16],
        SoilPermeability::Impermeable => [0.15, 0.21, 0.26],
    };
    let cv: [f64; 3] = match vegetation {
        Vegetation::DenseForestOrVeryLooseDeposits => [0.02, 0.03, 0.04],
        Vegetation::CultivatedLandOrThinForest => [0.04, 0.07, 0.11],
        Vegetation::Grassland => [0.13, 0.17, 0.21],
        Vegetation::BareRock => [0.24, 0.26, 0.28],
    };
    // Note under DDM Table 4.7(b).
    let factor = match adjustment {
        RuralAdjustment::None | RuralAdjustment::LakesSwampsDams => 1.00,
        RuralAdjustment::ContourCultivated | RuralAdjustment::DenseWoodSteep => 0.80,
        RuralAdjustment::DenseWoodFlat => 0.60,
    };
    factor * (cs[col] + ck[col] + cv[col])
}

/// The area-weighted runoff coefficient of DDM Eqn. 4.6, `Cw = Σ(Ai·Ci) / A`, from
/// (area, C) pairs in any one unit of area. `None` when the total area is not positive.
pub fn weighted_runoff_coefficient(parts: &[(f64, f64)]) -> Option<f64> {
    let area: f64 = parts.iter().map(|p| p.0).sum();
    if positive(area) {
        Some(parts.iter().map(|p| p.0 * p.1).sum::<f64>() / area)
    } else {
        None
    }
}

/// The multiplier of DDM Table 4.8 ("Multipliers for Higher Recurrence Interval") for the
/// recurrence intervals the table lists: 2 to 10 years 1.0, 25 years 1.1, 50 years 1.2,
/// 100 years 1.25. `None` for any other interval; the table does not interpolate.
///
/// The manual does not say in so many words what the multiplier is applied to (it follows a
/// paragraph on how C grows with rainfall intensity), does not limit the product with C to 1,
/// and its own 25-year Example 4.3 does not apply it. It is therefore never applied
/// automatically: pass it as [`RationalInput::frequency_multiplier`] if it is wanted.
pub fn recurrence_multiplier(years: u32) -> Option<f64> {
    match years {
        2..=10 => Some(1.0),
        25 => Some(1.1),
        50 => Some(1.2),
        100 => Some(1.25),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Rainfall intensity
// ---------------------------------------------------------------------------------------------

/// The intensity-duration-frequency form of DDM Eqn. 4.1, `i = a / (b + td)^c`, with the storm
/// duration `td` and `b` in hours and `i` in mm/h (the units of the manual's Appendix 4.1
/// example). The constants are inputs: see the module documentation for why the station tables
/// are not held.
pub fn idf_intensity_mm_h(a: f64, b_hours: f64, c: f64, duration_hours: f64) -> f64 {
    a / (b_hours + duration_hours).powf(c)
}

// ---------------------------------------------------------------------------------------------
// Time of concentration
// ---------------------------------------------------------------------------------------------

/// The rows of DDM Table 4-2, "Intercept Coefficients for Velocity vs. Slope Relationship".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LandCover {
    ForestHeavyLitterOrHayMeadow,
    TrashFallowMinimumTillageContourStripCroppedWoodland,
    ShortGrassPasture,
    CultivatedStraightRow,
    NearlyBareAndUntilled,
    GrassedWaterway,
    Unpaved,
    PavedAreaOrSmallUplandGullies,
}

impl LandCover {
    /// The intercept coefficient k of DDM Table 4-2, for `V = k · Sp^0.5` with V in m/s and the
    /// slope Sp in per cent.
    pub fn intercept_k(self) -> f64 {
        match self {
            LandCover::ForestHeavyLitterOrHayMeadow => 0.076,
            LandCover::TrashFallowMinimumTillageContourStripCroppedWoodland => 0.152,
            LandCover::ShortGrassPasture => 0.213,
            LandCover::CultivatedStraightRow => 0.274,
            LandCover::NearlyBareAndUntilled => 0.305,
            LandCover::GrassedWaterway => 0.457,
            LandCover::Unpaved => 0.491,
            LandCover::PavedAreaOrSmallUplandGullies => 0.619,
        }
    }
}

/// The rows of DDM Table 4-3, "Manning's Roughness Coefficient (n) for Overland Sheet Flow".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SheetFlowSurface {
    SmoothAsphalt,
    SmoothConcrete,
    OrdinaryConcreteLining,
    GoodWood,
    BrickWithCementMortar,
    VitrifiedClay,
    CastIron,
    CorrugatedMetalPipe,
    CementRubbleSurface,
    FallowNoResidue,
    CultivatedResidueUnder20Percent,
    CultivatedResidueOver20Percent,
    RangeNatural,
    ShortGrassPrairie,
    DenseGrasses,
    BermudaGrass,
    WoodsLightUnderbrush,
    WoodsDenseUnderbrush,
}

impl SheetFlowSurface {
    /// Manning's n for overland sheet flow, DDM Table 4-3. These are sheet-flow values; they
    /// are not channel roughnesses.
    pub fn manning_n(self) -> f64 {
        match self {
            SheetFlowSurface::SmoothAsphalt => 0.011,
            SheetFlowSurface::SmoothConcrete => 0.012,
            SheetFlowSurface::OrdinaryConcreteLining => 0.013,
            SheetFlowSurface::GoodWood => 0.014,
            SheetFlowSurface::BrickWithCementMortar => 0.014,
            SheetFlowSurface::VitrifiedClay => 0.015,
            SheetFlowSurface::CastIron => 0.015,
            SheetFlowSurface::CorrugatedMetalPipe => 0.024,
            SheetFlowSurface::CementRubbleSurface => 0.024,
            SheetFlowSurface::FallowNoResidue => 0.05,
            SheetFlowSurface::CultivatedResidueUnder20Percent => 0.06,
            SheetFlowSurface::CultivatedResidueOver20Percent => 0.17,
            SheetFlowSurface::RangeNatural => 0.13,
            SheetFlowSurface::ShortGrassPrairie => 0.15,
            SheetFlowSurface::DenseGrasses => 0.24,
            SheetFlowSurface::BermudaGrass => 0.41,
            SheetFlowSurface::WoodsLightUnderbrush => 0.40,
            SheetFlowSurface::WoodsDenseUnderbrush => 0.80,
        }
    }
}

/// DDM Section 4.2.1(e): the flow travel time to the point of entry to the drainage network
/// "shall not less than 5 minutes" (sic) (also Table 4.4, road inlet pits).
pub const MINIMUM_TC_MIN: f64 = 5.0;
/// DDM Table 4.4: standard minimum time of concentration for small areas under 0.4 ha, minutes.
pub const MINIMUM_TC_SMALL_AREA_MIN: f64 = 10.0;

/// Sheet flow travel time, minutes: DDM Table 4.1 No. 8 (HEC 22),
/// `Tti = (6.943 / i^0.4) · (n·L / √S)^0.6`, with i in mm/h, L in m and S in m/m.
pub fn sheet_flow_time_min(manning_n: f64, length_m: f64, slope_m_per_m: f64, intensity_mm_h: f64) -> f64 {
    6.943 / intensity_mm_h.powf(0.4) * (manning_n * length_m / slope_m_per_m.sqrt()).powf(0.6)
}

/// Velocity of concentrated flow, m/s: DDM Table 4.1 No. 7 (HDS 2), `V = k · Sp^0.5` with the
/// slope in **per cent** and k from Table 4-2.
pub fn concentrated_flow_velocity_m_s(intercept_k: f64, slope_percent: f64) -> f64 {
    intercept_k * slope_percent.sqrt()
}

/// Travel time, minutes: DDM Table 4.1 No. 7, `Tt = L / (60·V)`.
pub fn travel_time_min(length_m: f64, velocity_m_s: f64) -> f64 {
    length_m / (60.0 * velocity_m_s)
}

/// One stretch of the flow path to the point of interest.
#[derive(Debug, Clone, PartialEq)]
pub enum FlowSegment {
    /// Overland sheet flow (DDM Table 4.1 No. 8) with n from Table 4-3.
    Sheet { manning_n: f64, length_m: f64, slope_m_per_m: f64 },
    /// Shallow concentrated or gully flow (Table 4.1 No. 7) with k from Table 4-2.
    Concentrated { intercept_k: f64, length_m: f64, slope_m_per_m: f64 },
    /// Flow in an open channel or pipe at a known velocity (Section 4.2.1 d), e.g. from
    /// [`super::manning_velocity_m_s`].
    Conduit { length_m: f64, velocity_m_s: f64 },
}

/// The travel time of one segment.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentTime {
    pub segment: FlowSegment,
    /// None for sheet flow, whose formula gives the time directly.
    pub velocity_m_s: Option<f64>,
    pub minutes: f64,
}

/// A time of concentration built up from flow segments.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeOfConcentration {
    /// The rainfall intensity used in the sheet flow formula, mm/h.
    pub intensity_mm_h: f64,
    pub segments: Vec<SegmentTime>,
    /// Sum of the segment times, minutes.
    pub total_min: f64,
    /// The minimum the caller asked to be applied, minutes.
    pub minimum_min: f64,
    /// The larger of the total and the minimum.
    pub adopted_min: f64,
}

/// Sums the travel times of the segments of one flow path (DDM Section 4.2.1, Example 4-1) and
/// applies a minimum ([`MINIMUM_TC_MIN`] or [`MINIMUM_TC_SMALL_AREA_MIN`]).
///
/// The sheet flow time depends on the rainfall intensity, which itself is read at the time of
/// concentration, so the two are found by trial: this function does one pass with the intensity
/// given. Choosing the longest of alternative flow paths and checking the partial area effect
/// (Section 4.2.1 f) are left to the engineer.
pub fn time_of_concentration(segments: &[FlowSegment], intensity_mm_h: f64, minimum_min: f64) -> Result<TimeOfConcentration, DrainageError> {
    if segments.is_empty() {
        return invalid("a time of concentration needs at least one flow segment");
    }
    if !(minimum_min.is_finite() && minimum_min >= 0.0) {
        return invalid(format!("the minimum time must not be negative (got {minimum_min})"));
    }
    let mut out = Vec::with_capacity(segments.len());
    for (i, s) in segments.iter().enumerate() {
        let bad = |what: &str| invalid::<()>(format!("flow segment {}: {what} must be greater than zero", i + 1));
        let (velocity, minutes) = match *s {
            FlowSegment::Sheet { manning_n, length_m, slope_m_per_m } => {
                if !positive(manning_n) || !positive(length_m) || !positive(slope_m_per_m) {
                    bad("n, length and slope")?;
                }
                if !positive(intensity_mm_h) {
                    return invalid("sheet flow needs a rainfall intensity greater than zero");
                }
                (None, sheet_flow_time_min(manning_n, length_m, slope_m_per_m, intensity_mm_h))
            }
            FlowSegment::Concentrated { intercept_k, length_m, slope_m_per_m } => {
                if !positive(intercept_k) || !positive(length_m) || !positive(slope_m_per_m) {
                    bad("k, length and slope")?;
                }
                let v = concentrated_flow_velocity_m_s(intercept_k, slope_m_per_m * 100.0);
                (Some(v), travel_time_min(length_m, v))
            }
            FlowSegment::Conduit { length_m, velocity_m_s } => {
                if !positive(length_m) || !positive(velocity_m_s) {
                    bad("length and velocity")?;
                }
                (Some(velocity_m_s), travel_time_min(length_m, velocity_m_s))
            }
        };
        out.push(SegmentTime { segment: s.clone(), velocity_m_s: velocity, minutes });
    }
    let total_min: f64 = out.iter().map(|s| s.minutes).sum();
    Ok(TimeOfConcentration { intensity_mm_h, segments: out, total_min, minimum_min, adopted_min: total_min.max(minimum_min) })
}

impl TimeOfConcentration {
    /// The calculation set out line by line with its references.
    pub fn calculation(&self) -> Vec<String> {
        let mut out = vec![BASIS.to_string()];
        for (i, s) in self.segments.iter().enumerate() {
            let k = i + 1;
            out.push(match s.segment {
                FlowSegment::Sheet { manning_n, length_m, slope_m_per_m } => format!(
                    "Segment {k}, sheet flow (DDM Table 4.1 No. 8): Tt = (6.943 / i^0.4)·(n·L/√S)^0.6 = (6.943 / {:.1}^0.4)·({manning_n} × {length_m} / √{slope_m_per_m})^0.6 = {:.1} min.",
                    self.intensity_mm_h, s.minutes
                ),
                FlowSegment::Concentrated { intercept_k, length_m, slope_m_per_m } => format!(
                    "Segment {k}, concentrated flow (DDM Table 4.1 No. 7, k from Table 4-2): V = k·Sp^0.5 = {intercept_k} × {:.3}^0.5 = {:.2} m/s; Tt = L/(60·V) = {length_m} / (60 × {:.2}) = {:.1} min.",
                    slope_m_per_m * 100.0,
                    s.velocity_m_s.unwrap_or(0.0),
                    s.velocity_m_s.unwrap_or(0.0),
                    s.minutes
                ),
                FlowSegment::Conduit { length_m, velocity_m_s } => format!(
                    "Segment {k}, channel or pipe flow (DDM Section 4.2.1 d): Tt = L/(60·V) = {length_m} / (60 × {velocity_m_s:.2}) = {:.1} min.",
                    s.minutes
                ),
            });
        }
        out.push(format!("tc = sum of travel times = {:.1} min.", self.total_min));
        if self.adopted_min > self.total_min {
            out.push(format!("Minimum time of concentration {:.0} min governs (DDM Section 4.2.1 e, Table 4.4): tc = {:.1} min.", self.minimum_min, self.adopted_min));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Rational method
// ---------------------------------------------------------------------------------------------

/// Peak flow by the Rational method, m³/s: DDM Eqn. 4.10, `Q = C·i·A / 360`, with i in mm/h and
/// A in hectares.
pub fn rational_peak_flow_m3_s(runoff_coefficient: f64, intensity_mm_h: f64, area_ha: f64) -> f64 {
    runoff_coefficient * intensity_mm_h * area_ha / 360.0
}

/// The storage (peak reduction) factor of the Modified Rational method: DDM Eqn. 4.7,
/// `Cs = 2·tc / (2·tc + td)`, with tc the time of concentration and td the time of flow in the
/// drain, in the same unit.
pub fn storage_reduction_factor(tc: f64, td: f64) -> f64 {
    2.0 * tc / (2.0 * tc + td)
}

/// One part of a catchment with a uniform land use.
#[derive(Debug, Clone, PartialEq)]
pub struct SubCatchment {
    pub description: String,
    pub area_ha: f64,
    /// Rational coefficient C, from [`UrbanLandUse::c_range`], [`rural_runoff_coefficient`] or
    /// the engineer's own source.
    pub runoff_coefficient: f64,
}

/// What the Rational method needs.
#[derive(Debug, Clone, PartialEq)]
pub struct RationalInput {
    pub sub_catchments: Vec<SubCatchment>,
    /// Design rainfall intensity for a duration equal to the time of concentration, mm/h. An
    /// input: see the module documentation.
    pub intensity_mm_h: f64,
    /// Where the intensity comes from, quoted in the calculation.
    pub intensity_source: String,
    /// Multiplier for a higher recurrence interval ([`recurrence_multiplier`], DDM Table 4.8).
    /// 1.0 for none.
    pub frequency_multiplier: f64,
    /// The storage factor Cs of the Modified Rational method ([`storage_reduction_factor`]),
    /// or None for the basic method.
    pub storage_factor: Option<f64>,
}

/// The result of [`rational_method`].
#[derive(Debug, Clone, PartialEq)]
pub struct RationalResult {
    pub input: RationalInput,
    pub area_ha: f64,
    /// DDM Eqn. 4.6.
    pub weighted_c: f64,
    /// DDM Eqn. 4.10, times the frequency multiplier, times Cs when given (Eqn. 4.11).
    pub peak_flow_m3_s: f64,
    /// Limits of the method that the inputs go beyond.
    pub warnings: Vec<String>,
}

/// Peak flow of a small catchment by the Rational method (DDM Section 4.4.2) or, with a storage
/// factor, the Modified Rational method (Section 4.4.3).
pub fn rational_method(input: &RationalInput) -> Result<RationalResult, DrainageError> {
    if input.sub_catchments.is_empty() {
        return invalid("the catchment needs at least one area");
    }
    for s in &input.sub_catchments {
        if !positive(s.area_ha) {
            return invalid(format!("the area of '{}' must be greater than zero (got {})", s.description, s.area_ha));
        }
        if !(s.runoff_coefficient > 0.0 && s.runoff_coefficient <= 1.0) {
            return invalid(format!("the runoff coefficient of '{}' must be between 0 and 1 (got {})", s.description, s.runoff_coefficient));
        }
    }
    if !positive(input.intensity_mm_h) {
        return invalid(format!("the rainfall intensity must be greater than zero (got {})", input.intensity_mm_h));
    }
    if !positive(input.frequency_multiplier) {
        return invalid(format!("the frequency multiplier must be greater than zero (got {}); use 1.0 for none", input.frequency_multiplier));
    }
    if let Some(cs) = input.storage_factor {
        if !(cs > 0.0 && cs <= 1.0) {
            return invalid(format!("the storage factor Cs must be between 0 and 1 (got {cs})"));
        }
    }
    let parts: Vec<(f64, f64)> = input.sub_catchments.iter().map(|s| (s.area_ha, s.runoff_coefficient)).collect();
    let area_ha: f64 = parts.iter().map(|p| p.0).sum();
    let weighted_c = weighted_runoff_coefficient(&parts).expect("the areas are positive");
    let peak_flow_m3_s =
        input.storage_factor.unwrap_or(1.0) * input.frequency_multiplier * rational_peak_flow_m3_s(weighted_c, input.intensity_mm_h, area_ha);

    let mut warnings = Vec::new();
    // DDM Section 4.4.2 (area not greater than 80 ha) and Section 4.4.3 (80 to 120 ha).
    if area_ha > 120.0 {
        warnings.push(format!(
            "The catchment is {area_ha:.1} ha. DDM Section 4.4.3: the Modified Rational method 'could safely be applied up to 120 hectares' and beyond that 'should be employed with caution'."
        ));
    } else if area_ha > 80.0 && input.storage_factor.is_none() {
        warnings.push(format!(
            "The catchment is {area_ha:.1} ha. DDM Section 4.4.2 recommends the Rational method for catchments not greater than 80 ha; Section 4.4.3 recommends the Modified Rational method (storage factor Cs) above that."
        ));
    }
    if weighted_c * input.frequency_multiplier > 1.0 {
        warnings.push(format!(
            "C × multiplier = {:.2} is greater than 1. DDM Table 4.8 states no upper limit; decide whether to cap it.",
            weighted_c * input.frequency_multiplier
        ));
    }
    Ok(RationalResult { input: input.clone(), area_ha, weighted_c, peak_flow_m3_s, warnings })
}

impl RationalResult {
    /// The calculation set out line by line with its references.
    pub fn calculation(&self) -> Vec<String> {
        let i = &self.input;
        let mut out = vec![BASIS.to_string()];
        for s in &i.sub_catchments {
            out.push(format!("{}: A = {:.2} ha, C = {:.2}, C·A = {:.3}.", s.description, s.area_ha, s.runoff_coefficient, s.area_ha * s.runoff_coefficient));
        }
        out.push(format!(
            "Weighted C = Σ(Ci·Ai) / A = {:.3} / {:.2} = {:.3} (DDM Eqn. 4.6).",
            self.weighted_c * self.area_ha,
            self.area_ha,
            self.weighted_c
        ));
        out.push(format!("Design rainfall intensity i = {:.1} mm/h ({}).", i.intensity_mm_h, i.intensity_source));
        let basic = rational_peak_flow_m3_s(self.weighted_c, i.intensity_mm_h, self.area_ha);
        out.push(format!(
            "Q = C·i·A / 360 = {:.3} × {:.1} × {:.2} / 360 = {:.3} m³/s (DDM Eqn. 4.10).",
            self.weighted_c, i.intensity_mm_h, self.area_ha, basic
        ));
        if i.frequency_multiplier != 1.0 {
            out.push(format!("Recurrence interval multiplier {:.2} (DDM Table 4.8): Q = {:.3} m³/s.", i.frequency_multiplier, basic * i.frequency_multiplier));
        }
        if let Some(cs) = i.storage_factor {
            out.push(format!("Storage factor Cs = {cs:.3} (DDM Eqn. 4.7); Q = Cs·C·i·A / 360 = {:.3} m³/s (DDM Eqn. 4.11).", self.peak_flow_m3_s));
        }
        out.push(format!("Design peak flow Q = {:.3} m³/s.", self.peak_flow_m3_s));
        out.extend(self.warnings.iter().map(|w| format!("Check: {w}")));
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Open channels
// ---------------------------------------------------------------------------------------------

/// The rows of DDM Table 9.1, "Permissible velocity & roughness coefficient" ("Maximum
/// permissible velocities in erodible ditches and corresponding roughness coefficients",
/// Section 9.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelMaterial {
    SandLoamFineGravelVolcanicAsh,
    StiffClay,
    /// Printed "Course Gravel".
    CoarseGravel,
    ConglomerateHardShaleSoftRock,
    HardRock,
    Masonry,
    Concrete,
}

impl ChannelMaterial {
    pub const ALL: [ChannelMaterial; 7] = [
        ChannelMaterial::SandLoamFineGravelVolcanicAsh,
        ChannelMaterial::StiffClay,
        ChannelMaterial::CoarseGravel,
        ChannelMaterial::ConglomerateHardShaleSoftRock,
        ChannelMaterial::HardRock,
        ChannelMaterial::Masonry,
        ChannelMaterial::Concrete,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ChannelMaterial::SandLoamFineGravelVolcanicAsh => "Sand, loam, fine gravel, volcanic ash",
            ChannelMaterial::StiffClay => "Stiff clay",
            ChannelMaterial::CoarseGravel => "Coarse gravel",
            ChannelMaterial::ConglomerateHardShaleSoftRock => "Conglomerate, hard shale, soft rock",
            ChannelMaterial::HardRock => "Hard rock",
            ChannelMaterial::Masonry => "Masonry",
            ChannelMaterial::Concrete => "Concrete",
        }
    }

    /// Maximum permissible velocity, m/s: DDM Table 9.1.
    pub fn permissible_velocity_m_s(self) -> f64 {
        match self {
            ChannelMaterial::SandLoamFineGravelVolcanicAsh => 0.6,
            ChannelMaterial::StiffClay => 1.1,
            ChannelMaterial::CoarseGravel => 1.5,
            ChannelMaterial::ConglomerateHardShaleSoftRock => 2.0,
            ChannelMaterial::HardRock | ChannelMaterial::Masonry | ChannelMaterial::Concrete => 3.0,
        }
    }

    /// The "roughness coefficient" column of DDM Table 9.1, exactly as printed. The table does
    /// not name the formula it belongs to or give a unit.
    pub fn roughness_coefficient_as_printed(self) -> f64 {
        match self {
            ChannelMaterial::SandLoamFineGravelVolcanicAsh => 45.0,
            ChannelMaterial::StiffClay => 50.0,
            ChannelMaterial::CoarseGravel | ChannelMaterial::Masonry => 40.0,
            ChannelMaterial::ConglomerateHardShaleSoftRock | ChannelMaterial::HardRock => 25.0,
            ChannelMaterial::Concrete => 60.0,
        }
    }

    /// Manning's n **if** the printed roughness coefficient is read as a Strickler coefficient
    /// K = 1/n (m^(1/3)/s): `n = 1 / K`. That reading is this crate's interpretation of an
    /// unlabelled column, not a statement of the manual; nothing here uses it by default.
    pub fn manning_n_if_strickler(self) -> f64 {
        1.0 / self.roughness_coefficient_as_printed()
    }
}

// ---------------------------------------------------------------------------------------------
// Pipe culverts: outlet control
// ---------------------------------------------------------------------------------------------

/// DDM Section 9.6: freeboard below the road shoulder break point "is usually set at 250 mm for
/// major roads", m.
pub const CULVERT_FREEBOARD_MAJOR_ROAD_M: f64 = 0.25;
/// DDM Section 9.6: "... and 100 mm for minor roads", m. ("On low-cost low traffic roads no
/// freeboard is required.")
pub const CULVERT_FREEBOARD_MINOR_ROAD_M: f64 = 0.10;
/// DDM Section 9.3: the allowable headwater is limited by, among others, "an Hw/D no greater
/// than 1.5".
pub const HEADWATER_RATIO_LIMIT: f64 = 1.5;
/// DDM Section 9.3: Hw/D "equal to 1.2 is recommended for cases where insufficient data is
/// available to predict the flooding effect from headwater depth".
pub const HEADWATER_RATIO_RECOMMENDED: f64 = 1.2;
/// DDM Section 9.10: "The minimum diameter of a pipe culvert under a road should be 600 mm", m.
pub const MINIMUM_PIPE_CULVERT_DIAMETER_M: f64 = 0.6;
/// DDM Section 9.8: "The culvert skew shall not exceed 45°" from the perpendicular to the
/// centreline "without the approval of the concerned Authority".
pub const MAXIMUM_CULVERT_SKEW_DEG: f64 = 45.0;
/// DDM Section 9.9.1: "Gradients less than 1 percent should be avoided, if possible."
pub const CULVERT_GRADIENT_AVOID_BELOW: f64 = 0.01;
/// DDM Section 9.9.1: "Gradients less than 0.5 percent should not be used".
pub const CULVERT_GRADIENT_MINIMUM: f64 = 0.005;
/// DDM Section 9.9.2: above 10 per cent, concrete pipes "should be provided with anchors".
pub const CULVERT_GRADIENT_ANCHORS_ABOVE: f64 = 0.10;
/// DDM Section 9.9.2: above 20 per cent, "special anchoring devices ... for all types of pipe culvert".
pub const CULVERT_GRADIENT_SPECIAL_ANCHORS_ABOVE: f64 = 0.20;
/// DDM Section 9.7: culverts "should be laid to grades that produce a non-silting or a
/// non-erosive velocity, ideally between 1 and 3.5 m/s".
pub const CULVERT_VELOCITY_IDEAL_M_S: (f64, f64) = (1.0, 3.5);

/// The pipe culvert layout limits DDM states, for [`super::DesignBasis::culvert_layout_limits`].
pub fn culvert_layout_limits() -> CulvertLayoutLimits {
    CulvertLayoutLimits {
        minimum_diameter_m: Some(Limit {
            value: MINIMUM_PIPE_CULVERT_DIAMETER_M,
            citation: "DDM Section 9.10: 'The minimum diameter of a pipe culvert under a road should be 600 mm'",
        }),
        long_culvert: None,
        maximum_skew_deg: Some(Limit {
            value: MAXIMUM_CULVERT_SKEW_DEG,
            citation: "DDM Section 9.8: the skew 'shall not exceed 45°' from the perpendicular to the centreline without the approval of the Authority",
        }),
        gradient_not_below: Some(Limit {
            value: CULVERT_GRADIENT_MINIMUM,
            citation: "DDM Section 9.9.1: 'Gradients less than 0.5 percent should not be used' because of siltation",
        }),
        gradient_avoid_below: Some(Limit {
            value: CULVERT_GRADIENT_AVOID_BELOW,
            citation: "DDM Section 9.9.1: 'Gradients less than 1 percent should be avoided, if possible.'",
        }),
        gradient_anchors_above: Some(Limit {
            value: CULVERT_GRADIENT_ANCHORS_ABOVE,
            citation: "DDM Section 9.9.2: above 10 percent concrete pipes 'should be provided with anchors'",
        }),
        gradient_special_anchors_above: Some(Limit {
            value: CULVERT_GRADIENT_SPECIAL_ANCHORS_ABOVE,
            citation: "DDM Section 9.9.2: above 20 percent 'special anchoring devices should be incorporated for all types of pipe culvert'",
        }),
        structure_above_diameter_m: None,
    }
}

/// Entrances of pipe culverts in DDM Appendix 9.6, "Entrance Loss Coefficient (Outlet Control,
/// Full or Partially Full)". The box culvert rows are not held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipeEntrance {
    ConcreteMitredToFillSlope,
    ConcreteEndSectionConformingToFillSlope,
    ConcreteProjectingSquareCutEnd,
    ConcreteHeadwallSquareEdge,
    ConcreteHeadwallRounded,
    ConcreteHeadwallSocketEnd,
    ConcreteProjectingSocketEnd,
    ConcreteBevelledEdges,
    ConcreteSideOrSlopeTaperedInlet,
    CorrugatedMetalProjectingNoHeadwall,
    CorrugatedMetalMitredToFillSlope,
    CorrugatedMetalHeadwallSquareEdge,
    CorrugatedMetalEndSectionConformingToFillSlope,
    CorrugatedMetalBevelledEdges,
    CorrugatedMetalSideOrSlopeTaperedInlet,
}

impl PipeEntrance {
    pub fn label(self) -> &'static str {
        match self {
            PipeEntrance::ConcreteMitredToFillSlope => "Pipe, concrete: mitred to conform to fill slope",
            PipeEntrance::ConcreteEndSectionConformingToFillSlope => "Pipe, concrete: end-section conforming to fill slope",
            PipeEntrance::ConcreteProjectingSquareCutEnd => "Pipe, concrete: projecting from fill, square cut end",
            PipeEntrance::ConcreteHeadwallSquareEdge => "Pipe, concrete: headwall or headwall and wingwalls, square-edge",
            PipeEntrance::ConcreteHeadwallRounded => "Pipe, concrete: headwall or headwall and wingwalls, rounded (radius = 1/12 D)",
            PipeEntrance::ConcreteHeadwallSocketEnd => "Pipe, concrete: headwall or headwall and wingwalls, socket end of pipe (groove-end)",
            PipeEntrance::ConcreteProjectingSocketEnd => "Pipe, concrete: projecting from fill, socket end (groove-end)",
            PipeEntrance::ConcreteBevelledEdges => "Pipe, concrete: bevelled edges, 33.7° or 45° bevels",
            PipeEntrance::ConcreteSideOrSlopeTaperedInlet => "Pipe, concrete: side- or slope-tapered inlet",
            PipeEntrance::CorrugatedMetalProjectingNoHeadwall => "Pipe or pipe-arch, corrugated metal: projecting from fill (no headwall)",
            PipeEntrance::CorrugatedMetalMitredToFillSlope => "Pipe or pipe-arch, corrugated metal: mitred to conform to fill slope",
            PipeEntrance::CorrugatedMetalHeadwallSquareEdge => "Pipe or pipe-arch, corrugated metal: headwall or headwall and wingwalls, square-edge",
            PipeEntrance::CorrugatedMetalEndSectionConformingToFillSlope => "Pipe or pipe-arch, corrugated metal: end-section conforming to fill slope",
            PipeEntrance::CorrugatedMetalBevelledEdges => "Pipe or pipe-arch, corrugated metal: bevelled edges, 33.7° or 45° bevels",
            PipeEntrance::CorrugatedMetalSideOrSlopeTaperedInlet => "Pipe or pipe-arch, corrugated metal: side- or slope-tapered inlet",
        }
    }

    /// The entrance loss coefficient Ke of DDM Appendix 9.6.
    pub fn ke(self) -> f64 {
        match self {
            PipeEntrance::CorrugatedMetalProjectingNoHeadwall => 0.9,
            PipeEntrance::ConcreteMitredToFillSlope | PipeEntrance::CorrugatedMetalMitredToFillSlope => 0.7,
            PipeEntrance::ConcreteEndSectionConformingToFillSlope
            | PipeEntrance::ConcreteProjectingSquareCutEnd
            | PipeEntrance::ConcreteHeadwallSquareEdge
            | PipeEntrance::CorrugatedMetalHeadwallSquareEdge
            | PipeEntrance::CorrugatedMetalEndSectionConformingToFillSlope => 0.5,
            PipeEntrance::ConcreteHeadwallRounded
            | PipeEntrance::ConcreteHeadwallSocketEnd
            | PipeEntrance::ConcreteProjectingSocketEnd
            | PipeEntrance::ConcreteBevelledEdges
            | PipeEntrance::ConcreteSideOrSlopeTaperedInlet
            | PipeEntrance::CorrugatedMetalBevelledEdges
            | PipeEntrance::CorrugatedMetalSideOrSlopeTaperedInlet => 0.2,
        }
    }
}

/// The pipe rows of DDM Appendix 9.7, "Recommended Manning's n Values for Pipes". (The "Concrete
/// Boxes" row is not held.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipeWall {
    ConcreteSmooth,
    CorrugatedMetal68x13,
    CorrugatedMetal150x25,
    CorrugatedMetal125x25,
    CorrugatedMetal75x25,
    StructuralPlate150x50,
    StructuralPlate230x64,
    CorrugatedMetalHelical68x13FullCircularFlow,
    SpiralRibMetalSmooth,
}

impl PipeWall {
    /// The range of Manning's n the appendix recommends, (low, high). The value within the
    /// range is the engineer's choice. (Section 9.5.2 separately records that its approximate
    /// capacity tables used 0.015 for reinforced concrete pipe.)
    pub fn manning_n_range(self) -> (f64, f64) {
        // DDM Appendix 9.7.
        match self {
            PipeWall::ConcreteSmooth => (0.010, 0.013),
            PipeWall::CorrugatedMetal68x13 => (0.022, 0.027),
            PipeWall::CorrugatedMetal150x25 => (0.022, 0.025),
            PipeWall::CorrugatedMetal125x25 => (0.025, 0.026),
            PipeWall::CorrugatedMetal75x25 => (0.027, 0.028),
            PipeWall::StructuralPlate150x50 => (0.033, 0.035),
            PipeWall::StructuralPlate230x64 => (0.033, 0.037),
            PipeWall::CorrugatedMetalHelical68x13FullCircularFlow => (0.012, 0.024),
            PipeWall::SpiralRibMetalSmooth => (0.012, 0.013),
        }
    }
}

/// Losses through a full culvert barrel, m: DDM Eqn. 9.6,
/// `H = [1 + Ke + 19.63·n²·L / R^1.33] · V²/2g`, with V = Q/A (Eqn. 9.3), the constants as
/// printed and g = 9.8 m/s² as the manual defines it under Eqn. 9.4. `area_m2` and
/// `hydraulic_radius_m` are those of the full barrel (for a pipe, πD²/4 and D/4).
pub fn barrel_head_loss_m(flow_m3_s: f64, area_m2: f64, hydraulic_radius_m: f64, manning_n: f64, length_m: f64, ke: f64) -> f64 {
    let velocity_head = (flow_m3_s / area_m2).powi(2) / (2.0 * GRAVITY_CULVERT_M_S2);
    (1.0 + ke + FRICTION_CONSTANT * manning_n * manning_n * length_m / hydraulic_radius_m.powf(FRICTION_RADIUS_EXPONENT)) * velocity_head
}

/// What the outlet-control calculation needs.
#[derive(Debug, Clone, PartialEq)]
pub struct OutletControlInput {
    /// Total design flow at the culvert, m³/s. It is divided equally between the barrels
    /// (DDM Section 9.5.1 Step 6 c).
    pub design_flow_m3_s: f64,
    pub barrels: u32,
    /// Internal diameter D, m.
    pub diameter_m: f64,
    /// Length of the barrel L, m.
    pub length_m: f64,
    /// Drop from the inlet invert to the outlet invert, So·L, m.
    pub fall_m: f64,
    /// Manning's n of the barrel, e.g. from [`PipeWall::manning_n_range`].
    pub manning_n: f64,
    /// Entrance loss coefficient Ke, e.g. [`PipeEntrance::ke`].
    pub entrance_loss_ke: f64,
    /// Tailwater depth above the outlet invert, TW, m. An input: the manual takes it from the
    /// normal depth of the downstream channel or a water surface profile (Step 8 a).
    pub tailwater_m: f64,
}

/// The result of [`outlet_control`].
#[derive(Debug, Clone, PartialEq)]
pub struct OutletControl {
    pub input: OutletControlInput,
    pub flow_per_barrel_m3_s: f64,
    /// Full barrel area πD²/4, m².
    pub area_m2: f64,
    /// Full barrel hydraulic radius D/4, m.
    pub hydraulic_radius_m: f64,
    /// Full barrel velocity V = Q/A (DDM Eqn. 9.3), m/s.
    pub velocity_m_s: f64,
    /// V²/2g with g = 9.8 (DDM Eqn. 9.4), m.
    pub velocity_head_m: f64,
    /// Barrel losses H (DDM Eqn. 9.6), m.
    pub head_loss_m: f64,
    /// Critical depth dc, m ([`circular_critical_depth_m`]).
    pub critical_depth_m: f64,
    /// The larger of TW and (dc + D)/2 (DDM Eqn. 9.9), m.
    pub ho_m: f64,
    /// Outlet-control headwater depth above the inlet invert, HWoi = H + ho − So·L
    /// (DDM Eqn. 9.11), m.
    pub headwater_m: f64,
    /// HWoi / D.
    pub headwater_ratio: f64,
    /// Limits of the method and of the manual's criteria that the result meets or goes beyond.
    pub warnings: Vec<String>,
}

/// Outlet-control headwater of a pipe culvert flowing full, by DDM Section 9.4.2 and Section
/// 9.5.1 Step 8: `HWoi = H + ho − So·L`, approach and downstream velocity heads neglected.
///
/// **This is one half of a culvert check.** The governing headwater is the higher of the inlet-
/// and outlet-control values (Step 9), and inlet control is available in the manual only as
/// nomographs, which are not implemented. Every result carries that warning.
pub fn outlet_control(input: &OutletControlInput) -> Result<OutletControl, DrainageError> {
    let i = input;
    if i.barrels == 0 {
        return invalid("a culvert needs at least one barrel");
    }
    for (name, v) in [("design flow", i.design_flow_m3_s), ("diameter", i.diameter_m), ("length", i.length_m), ("Manning's n", i.manning_n)] {
        if !positive(v) {
            return invalid(format!("the {name} must be greater than zero (got {v})"));
        }
    }
    for (name, v) in [("entrance loss coefficient", i.entrance_loss_ke), ("tailwater depth", i.tailwater_m)] {
        if !(v.is_finite() && v >= 0.0) {
            return invalid(format!("the {name} must not be negative (got {v})"));
        }
    }
    if !i.fall_m.is_finite() {
        return invalid("the fall between the inverts must be a number");
    }
    let d = i.diameter_m;
    let flow_per_barrel_m3_s = i.design_flow_m3_s / i.barrels as f64;
    let area_m2 = PI * d * d / 4.0;
    let hydraulic_radius_m = d / 4.0;
    let velocity_m_s = flow_per_barrel_m3_s / area_m2;
    let velocity_head_m = velocity_m_s * velocity_m_s / (2.0 * GRAVITY_CULVERT_M_S2);
    let head_loss_m = barrel_head_loss_m(flow_per_barrel_m3_s, area_m2, hydraulic_radius_m, i.manning_n, i.length_m, i.entrance_loss_ke);
    let critical_depth_m = circular_critical_depth_m(flow_per_barrel_m3_s, d);
    let ho_m = i.tailwater_m.max((critical_depth_m + d) / 2.0);
    let headwater_m = head_loss_m + ho_m - i.fall_m;
    let headwater_ratio = headwater_m / d;

    let mut warnings = vec![
        "Inlet control is not computed: DDM gives it only as nomographs (Appendices 9.3 and 9.4). The governing headwater is the higher of the inlet- and outlet-control values (Section 9.5.1 Step 9), so this result alone does not size the culvert.".to_string(),
    ];
    // DDM Section 9.5.1 Step 8 g and the text under Eqn. 9.9.
    if headwater_m < 0.75 * d {
        warnings.push(format!(
            "HWoi = {headwater_m:.2} m is below 0.75·D = {:.2} m: 'the approximate method shall not be used' (DDM Section 9.5.1 Step 8 g); backwater calculations are required.",
            0.75 * d
        ));
    } else if headwater_m < 1.2 * d {
        warnings.push(format!(
            "HWoi = {headwater_m:.2} m is below 1.2·D = {:.2} m: the barrel may flow partly full and 'backwater calculations should be used to check the result' (DDM Section 9.5.1 Step 8 g).",
            1.2 * d
        ));
    }
    if headwater_ratio > HEADWATER_RATIO_LIMIT {
        warnings.push(format!("HWoi/D = {headwater_ratio:.2} exceeds the limit of {HEADWATER_RATIO_LIMIT} (DDM Section 9.3)."));
    } else if headwater_ratio > HEADWATER_RATIO_RECOMMENDED {
        warnings.push(format!(
            "HWoi/D = {headwater_ratio:.2} exceeds {HEADWATER_RATIO_RECOMMENDED}, the ratio DDM Section 9.3 recommends where the flooding effect of the headwater cannot be predicted."
        ));
    }
    let (v_lo, v_hi) = CULVERT_VELOCITY_IDEAL_M_S;
    if velocity_m_s < v_lo || velocity_m_s > v_hi {
        warnings.push(format!("Full-barrel velocity {velocity_m_s:.2} m/s is outside the 'ideally between {v_lo} and {v_hi} m/s' of DDM Section 9.7."));
    }
    Ok(OutletControl {
        input: input.clone(),
        flow_per_barrel_m3_s,
        area_m2,
        hydraulic_radius_m,
        velocity_m_s,
        velocity_head_m,
        head_loss_m,
        critical_depth_m,
        ho_m,
        headwater_m,
        headwater_ratio,
        warnings,
    })
}

impl OutletControl {
    /// Freeboard by DDM Section 9.6: the level of the road shoulder break point at the low
    /// point of the road less the headwater level, m. The headwater level is the inlet invert
    /// level plus the **outlet-control** headwater, so this is an upper bound on the freeboard
    /// until inlet control has been checked by other means. Compare with
    /// [`CULVERT_FREEBOARD_MAJOR_ROAD_M`] or [`CULVERT_FREEBOARD_MINOR_ROAD_M`].
    pub fn freeboard_m(&self, inlet_invert_level_m: f64, shoulder_break_level_m: f64) -> f64 {
        shoulder_break_level_m - (inlet_invert_level_m + self.headwater_m)
    }

    /// The calculation set out line by line with its references.
    pub fn calculation(&self) -> Vec<String> {
        let i = &self.input;
        let d = i.diameter_m;
        let mut out = vec![
            BASIS.to_string(),
            "Pipe culvert, outlet control, barrel flowing full (DDM Section 9.4.2 and Section 9.5.1 Step 8).".to_string(),
            format!(
                "Q = {:.3} m³/s over {} barrel(s): {:.3} m³/s per barrel (Step 6 c). D = {:.3} m, L = {:.2} m, So·L = {:.3} m, n = {}, Ke = {} (DDM Appendix 9.6), TW = {:.3} m.",
                i.design_flow_m3_s, i.barrels, self.flow_per_barrel_m3_s, d, i.length_m, i.fall_m, i.manning_n, i.entrance_loss_ke, i.tailwater_m
            ),
            format!("A = πD²/4 = {:.4} m²; R = D/4 = {:.4} m; V = Q/A = {:.2} m/s (DDM Eqn. 9.3); V²/2g = {:.4} m with g = 9.8 (Eqn. 9.4).", self.area_m2, self.hydraulic_radius_m, self.velocity_m_s, self.velocity_head_m),
            format!(
                "H = [1 + Ke + 19.63·n²·L / R^1.33]·V²/2g = [1 + {} + {:.4}] × {:.4} = {:.3} m (DDM Eqn. 9.6).",
                i.entrance_loss_ke,
                FRICTION_CONSTANT * i.manning_n * i.manning_n * i.length_m / self.hydraulic_radius_m.powf(FRICTION_RADIUS_EXPONENT),
                self.velocity_head_m,
                self.head_loss_m
            ),
            format!(
                "Critical depth dc = {:.3} m, solved from Q²·T/(g·A³) = 1 (DDM reads it from the chart of Appendix 9.10); (dc + D)/2 = {:.3} m.",
                self.critical_depth_m,
                (self.critical_depth_m + d) / 2.0
            ),
            format!("ho = the larger of TW and (dc + D)/2 = {:.3} m (DDM Eqn. 9.9).", self.ho_m),
            format!(
                "HWoi = H + ho − So·L = {:.3} + {:.3} − {:.3} = {:.3} m above the inlet invert (DDM Eqn. 9.11); HWoi/D = {:.2}.",
                self.head_loss_m, self.ho_m, i.fall_m, self.headwater_m, self.headwater_ratio
            ),
        ];
        out.extend(self.warnings.iter().map(|w| format!("Check: {w}")));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::{check_channel, froude_number, manning_velocity_m_s, normal_depth_m, ChannelGeometry, ChannelInput, DesignBasis};

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!((actual - expected).abs() <= tolerance, "expected {expected} ± {tolerance}, got {actual}");
    }

    #[test]
    fn table_3_2_recurrence_intervals() {
        use DesignStandard::*;
        use StructureType::*;
        // DDM Table 3.2, the three rows held, read across the four printed columns.
        let row = |s| [PavedIaIb, Column2PavedIiiGravelA, Column3PavedIiiGravelB, GravelC].map(|c| design_recurrence_years(s, c));
        assert_eq!(row(SideDitch), [10, 10, 5, 5]);
        assert_eq!(row(PipeCulvertSpanUnder2m), [25, 10, 5, 5]);
        assert_eq!(row(CulvertSpan2To6m), [50, 25, 10, 10]);
        // Note 1: "the span for a double 1.2-meter diameter pipe is 2.4 meters, and the design
        // storm frequency is therefore 'culvert, 2m < span < 6m'".
        assert_eq!(culvert_structure_type(2, 1.2), Some(CulvertSpan2To6m));
        assert_eq!(culvert_structure_type(1, 1.2), Some(PipeCulvertSpanUnder2m));
        assert_eq!(culvert_structure_type(2, 1.0), Some(CulvertSpan2To6m)); // exactly 2 m: see the function's note
        assert_eq!(culvert_structure_type(4, 1.5), None); // 6 m: a bridge row, not held
        assert_eq!(DesignStandard::Column2PavedIiiGravelA.printed_heading(), "PIII Gravel A");
    }

    /// DDM Example 4.2 (Section 4.3.3): weighted runoff coefficients.
    #[test]
    fn example_4_2_weighted_runoff_coefficient() {
        // Existing: (8.95 × 0.25 + 8.60 × 0.22) / 17.55 = 4.1295 / 17.55 = 0.2353. Manual: 0.235.
        close(weighted_runoff_coefficient(&[(8.95, 0.25), (8.60, 0.22)]).unwrap(), 0.235, 0.0005);
        // Proposed, from the example's table (paved 2.20 ha):
        // (2.20 × 0.90 + 0.66 × 0.15 + 7.52 × 0.25 + 7.17 × 0.22) / 17.55 = 5.5364 / 17.55 = 0.3155.
        // The manual's working line writes 2.22 for the paved area (its table says 2.20 and only
        // 2.20 makes the areas total 17.55) and prints 0.315; with 2.22 the result is 0.3165.
        let proposed = weighted_runoff_coefficient(&[(2.20, 0.90), (0.66, 0.15), (7.52, 0.25), (7.17, 0.22)]).unwrap();
        close(proposed, 0.3155, 0.0001);
        close(2.20 + 0.66 + 7.52 + 7.17, 17.55, 1e-9);
        assert_eq!(weighted_runoff_coefficient(&[]), None);
    }

    /// DDM Example 4-1 (Section 4.2.1, adopted from HEC 22): time of concentration over four
    /// flow segments at i = 60 mm/h. Manual: 14.6 + 4.8 + 3.7 + 1.4 = 24.5 min.
    #[test]
    fn example_4_1_time_of_concentration() {
        // Segment 4 is a 380 mm concrete pipe at 0.008 flowing full: R = D/4. The manual takes
        // n = 0.011 "from Table 4.3", where 0.011 is the smooth asphalt row (smooth concrete is
        // 0.012); its figure is reproduced here with the n it used.
        let pipe_v = manning_velocity_m_s(0.011, 0.38 / 4.0, 0.008);
        close(pipe_v, 1.7, 0.05);
        let segments = [
            FlowSegment::Sheet { manning_n: SheetFlowSurface::ShortGrassPrairie.manning_n(), length_m: 25.0, slope_m_per_m: 0.005 },
            FlowSegment::Concentrated { intercept_k: LandCover::ShortGrassPasture.intercept_k(), length_m: 43.0, slope_m_per_m: 0.005 },
            FlowSegment::Concentrated { intercept_k: LandCover::GrassedWaterway.intercept_k(), length_m: 79.0, slope_m_per_m: 0.006 },
            FlowSegment::Conduit { length_m: 146.0, velocity_m_s: pipe_v },
        ];
        let tc = time_of_concentration(&segments, 60.0, MINIMUM_TC_MIN).unwrap();
        let minutes: Vec<f64> = tc.segments.iter().map(|s| s.minutes).collect();
        close(minutes[0], 14.6, 0.05);
        close(minutes[1], 4.8, 0.05);
        close(minutes[2], 3.7, 0.05);
        close(minutes[3], 1.4, 0.05);
        close(tc.segments[1].velocity_m_s.unwrap(), 0.15, 0.005);
        close(tc.segments[2].velocity_m_s.unwrap(), 0.35, 0.005);
        close(tc.total_min, 24.5, 0.1);
        assert_eq!(tc.adopted_min, tc.total_min);
        let text = tc.calculation().join("\n");
        assert!(text.contains("Table 4.1 No. 8") && text.contains("Table 4-2") && text.contains("tc = sum of travel times = 24.5 min."));

        // A very short path takes the minimum of Table 4.4.
        let short = time_of_concentration(&[FlowSegment::Conduit { length_m: 30.0, velocity_m_s: 1.0 }], 60.0, MINIMUM_TC_SMALL_AREA_MIN).unwrap();
        close(short.total_min, 0.5, 1e-9);
        assert_eq!(short.adopted_min, 10.0);
        assert!(short.calculation().iter().any(|l| l.contains("Minimum time of concentration 10 min governs")));
        assert!(time_of_concentration(&[], 60.0, 5.0).is_err());
        assert!(time_of_concentration(&[FlowSegment::Conduit { length_m: 30.0, velocity_m_s: 0.0 }], 60.0, 5.0).is_err());
    }

    /// DDM Example 4.3 (Section 4.4.2, adopted from HDS 2): Rational method, 43.7 ha, 25 years.
    #[test]
    fn example_4_3_rational_method() {
        // Velocities of the example's Table 4.3.2 by the velocity method: short grass at 2 %,
        // grassed waterway at 2 % and at 1 %.
        close(concentrated_flow_velocity_m_s(LandCover::ShortGrassPasture.intercept_k(), 2.0), 0.30, 0.005);
        close(concentrated_flow_velocity_m_s(LandCover::GrassedWaterway.intercept_k(), 2.0), 0.65, 0.005);
        close(concentrated_flow_velocity_m_s(LandCover::GrassedWaterway.intercept_k(), 1.0), 0.46, 0.005);
        // Tc = 90/0.30 + 300/0.65 + 650/0.46 = 300 + 461.5 + 1413.0 = 2174.6 s = 36.2 min.
        // (The example's text says 30 m of short grass; its table and its sum use 90 m.)
        let tc = time_of_concentration(
            &[
                FlowSegment::Conduit { length_m: 90.0, velocity_m_s: 0.30 },
                FlowSegment::Conduit { length_m: 300.0, velocity_m_s: 0.65 },
                FlowSegment::Conduit { length_m: 650.0, velocity_m_s: 0.46 },
            ],
            85.0,
            MINIMUM_TC_MIN,
        )
        .unwrap();
        close(tc.total_min, 36.2, 0.05);

        let sub = |d: &str, a: f64, c: f64| SubCatchment { description: d.to_string(), area_ha: a, runoff_coefficient: c };
        let input = RationalInput {
            sub_catchments: vec![sub("Park", 21.8, 0.20), sub("Commercial (100% impervious)", 1.5, 0.95), sub("Single-family", 20.4, 0.40)],
            intensity_mm_h: 85.0,
            intensity_source: "IDF curve for the locality, as given in the example".to_string(),
            frequency_multiplier: 1.0,
            storage_factor: None,
        };
        let r = rational_method(&input).unwrap();
        // ΣCA = 4.36 + 1.425 + 8.16 = 13.945; C = 13.945 / 43.7 = 0.319. Manual: 13.95, 0.32.
        close(r.area_ha, 43.7, 1e-9);
        close(r.weighted_c, 0.32, 0.005);
        // Q = 0.319 × 85 × 43.7 / 360 = 3.29 m³/s. Manual: 3.3 (with C rounded to 0.32, 3.30).
        close(r.peak_flow_m3_s, 3.3, 0.05);
        close(rational_peak_flow_m3_s(0.32, 85.0, 43.7), 3.30, 0.005);
        assert!(r.warnings.is_empty());
        let text = r.calculation().join("\n");
        assert!(text.contains("DDM Eqn. 4.6") && text.contains("DDM Eqn. 4.10") && text.contains("as given in the example"));
        assert!(!text.contains("Table 4.8"));

        // The 25-year multiplier of Table 4.8, which the manual's own example leaves out.
        let with = rational_method(&RationalInput { frequency_multiplier: recurrence_multiplier(25).unwrap(), ..input.clone() }).unwrap();
        close(with.peak_flow_m3_s, 1.1 * r.peak_flow_m3_s, 1e-12);
        assert!(with.calculation().iter().any(|l| l.contains("Table 4.8")));
        // Modified Rational method: tc = 36.2 min, flow time in the drain 20 min:
        // Cs = 2 × 36.2 / (2 × 36.2 + 20) = 72.4 / 92.4 = 0.7835.
        let cs = storage_reduction_factor(36.2, 20.0);
        close(cs, 0.7835, 0.0001);
        let modified = rational_method(&RationalInput { storage_factor: Some(cs), ..input.clone() }).unwrap();
        close(modified.peak_flow_m3_s, cs * r.peak_flow_m3_s, 1e-12);
        assert!(modified.calculation().iter().any(|l| l.contains("DDM Eqn. 4.11")));
    }

    #[test]
    fn rational_method_states_its_limits_and_refuses_bad_input() {
        let one = |area: f64, c: f64| RationalInput {
            sub_catchments: vec![SubCatchment { description: "all".into(), area_ha: area, runoff_coefficient: c }],
            intensity_mm_h: 50.0,
            intensity_source: "test".into(),
            frequency_multiplier: 1.0,
            storage_factor: None,
        };
        assert!(rational_method(&one(80.0, 0.5)).unwrap().warnings.is_empty());
        let w = rational_method(&one(100.0, 0.5)).unwrap().warnings;
        assert!(w.len() == 1 && w[0].contains("Section 4.4.2") && w[0].contains("80 ha"));
        // With the storage factor the 80 to 120 ha range is the Modified method's own.
        assert!(rational_method(&RationalInput { storage_factor: Some(0.8), ..one(100.0, 0.5) }).unwrap().warnings.is_empty());
        assert!(rational_method(&RationalInput { storage_factor: Some(0.8), ..one(150.0, 0.5) }).unwrap().warnings[0].contains("120 hectares"));
        let capped = rational_method(&RationalInput { frequency_multiplier: 1.25, ..one(10.0, 0.9) }).unwrap();
        assert!(capped.warnings[0].contains("greater than 1"));
        // Q = 0.9 × 1.25 × 50 × 10 / 360 = 1.5625: not capped silently.
        close(capped.peak_flow_m3_s, 1.5625, 1e-9);

        assert!(rational_method(&one(0.0, 0.5)).is_err());
        assert!(rational_method(&one(10.0, 1.2)).is_err());
        assert!(rational_method(&one(10.0, 0.0)).is_err());
        assert!(rational_method(&RationalInput { intensity_mm_h: 0.0, ..one(10.0, 0.5) }).is_err());
        assert!(rational_method(&RationalInput { sub_catchments: vec![], ..one(10.0, 0.5) }).is_err());
        assert!(rational_method(&RationalInput { storage_factor: Some(1.5), ..one(10.0, 0.5) }).is_err());
    }

    #[test]
    fn runoff_coefficient_tables() {
        // DDM Table 4.7(a).
        assert_eq!(UrbanLandUse::StreetAsphaltic.c_range(), (0.70, 0.95));
        assert_eq!(UrbanLandUse::LawnHeavySoilSteep.c_range(), (0.25, 0.35));
        assert_eq!(UrbanLandUse::ResidentialSingleFamily.c_range(), (0.30, 0.50));
        assert!(UrbanLandUse::ALL.iter().all(|u| u.c_range().0 < u.c_range().1 && !u.label().is_empty()));
        // DDM Table 4.7(b), MAP > 600 mm: steep 0.16 + semi-permeable 0.16 + grassland 0.21 = 0.53.
        let c = |b, a| rural_runoff_coefficient(b, HillSlope::Steep, SoilPermeability::SemiPermeable, Vegetation::Grassland, a);
        close(c(RainfallBand::Over600, RuralAdjustment::None), 0.53, 1e-12);
        // 300-600 mm: 0.12 + 0.12 + 0.17 = 0.41; < 300 mm: 0.09 + 0.08 + 0.13 = 0.30.
        close(c(RainfallBand::From300To600, RuralAdjustment::None), 0.41, 1e-12);
        close(c(RainfallBand::Under300, RuralAdjustment::None), 0.30, 1e-12);
        // Notes: contour cultivated × 0.80 = 0.424; lakes × 1.00.
        close(c(RainfallBand::Over600, RuralAdjustment::ContourCultivated), 0.424, 1e-12);
        close(c(RainfallBand::Over600, RuralAdjustment::LakesSwampsDams), 0.53, 1e-12);
        // Dense wood on flat ground: 0.60 × (0.03 + 0.04 + 0.04) = 0.066.
        let wood = rural_runoff_coefficient(RainfallBand::Over600, HillSlope::Flat, SoilPermeability::VeryPermeable, Vegetation::DenseForestOrVeryLooseDeposits, RuralAdjustment::DenseWoodFlat);
        close(wood, 0.066, 1e-12);
        // The largest value the table can give: 0.26 + 0.26 + 0.28 = 0.80.
        close(rural_runoff_coefficient(RainfallBand::Over600, HillSlope::VerySteep, SoilPermeability::Impermeable, Vegetation::BareRock, RuralAdjustment::None), 0.80, 1e-12);
        // DDM Table 4.8.
        assert_eq!([2, 5, 10, 25, 50, 100, 20].map(recurrence_multiplier), [Some(1.0), Some(1.0), Some(1.0), Some(1.1), Some(1.2), Some(1.25), None]);
    }

    /// DDM Appendix 4.1: the "Demonstration" station's 2-year equation i = 39.4 / (0.3 + td)^0.89
    /// and its tabulated intensity of 18.78 mm/h at 120 minutes.
    #[test]
    fn idf_form_reproduces_the_appendix_4_1_table() {
        // 2.3^0.89 = 2.0987; 39.4 / 2.0987 = 18.77.
        close(idf_intensity_mm_h(39.4, 0.3, 0.89, 2.0), 18.78, 0.02);
    }

    #[test]
    fn section_properties_in_closed_form() {
        // Trapezoid b = 1.0, z = 1.5 both sides, y = 0.5:
        // A = 0.5 × (1.0 + 3.0 × 0.5 / 2) = 0.875; P = 1.0 + 0.5 × 2 × √3.25 = 2.802776;
        // T = 1.0 + 3.0 × 0.5 = 2.5; R = 0.875 / 2.802776 = 0.312190; A/T = 0.35.
        let t = ChannelGeometry::trapezoid(1.0, 1.5);
        close(t.area_m2(0.5), 0.875, 1e-12);
        close(t.wetted_perimeter_m(0.5), 2.802776, 1e-6);
        close(t.top_width_m(0.5), 2.5, 1e-12);
        close(t.hydraulic_radius_m(0.5), 0.312190, 1e-6);
        close(t.hydraulic_depth_m(0.5), 0.35, 1e-12);
        // V with slopes 2 and 2, y = 0.4: A = 4 × 0.16 / 2 = 0.32; P = 0.4 × 2 × √5 = 1.788854; T = 1.6.
        let v = ChannelGeometry::v(2.0, 2.0);
        close(v.area_m2(0.4), 0.32, 1e-12);
        close(v.wetted_perimeter_m(0.4), 1.788854, 1e-6);
        close(v.top_width_m(0.4), 1.6, 1e-12);
        // Unequal V, slopes 3 and 1, y = 0.4: A = 4 × 0.16 / 2 = 0.32; P = 0.4 × (√10 + √2) = 1.830596.
        let u = ChannelGeometry::v(3.0, 1.0);
        close(u.area_m2(0.4), 0.32, 1e-12);
        close(u.wetted_perimeter_m(0.4), 1.830596, 1e-6);
        // Rectangle 1.0 wide, y = 0.5: A = 0.5, P = 2.0, R = 0.25.
        let r = ChannelGeometry::rectangle(1.0);
        close(r.area_m2(0.5), 0.5, 1e-12);
        close(r.wetted_perimeter_m(0.5), 2.0, 1e-12);
        close(r.hydraulic_radius_m(0.5), 0.25, 1e-12);
    }

    #[test]
    fn manning_capacity_of_a_known_trapezoid() {
        // b = 1.0 m, z = 1.5, y = 0.5 m, n = 0.025, S = 0.004.
        // R = 0.312190 m; R^(2/3) = 0.460202; √S = 0.0632456.
        // V = 0.460202 × 0.0632456 / 0.025 = 1.16422 m/s; Q = 0.875 × 1.16422 = 1.01870 m³/s.
        let t = ChannelGeometry::trapezoid(1.0, 1.5);
        close(manning_velocity_m_s(0.025, t.hydraulic_radius_m(0.5), 0.004), 1.16422, 2e-5);
        close(t.discharge_m3_s(0.5, 0.025, 0.004), 1.01870, 2e-5);
        // Rectangle 1.0 × 0.5, n = 0.015, S = 0.01: R = 0.25, R^(2/3) = 0.396850;
        // V = 0.396850 × 0.1 / 0.015 = 2.64567 m/s; Q = 1.32283 m³/s.
        close(ChannelGeometry::rectangle(1.0).discharge_m3_s(0.5, 0.015, 0.01), 1.32283, 2e-5);
        assert_eq!(t.discharge_m3_s(0.0, 0.025, 0.004), 0.0);
    }

    #[test]
    fn normal_depth_returns_the_depth_the_flow_was_computed_from() {
        for (g, n, s, y) in [
            (ChannelGeometry::trapezoid(1.0, 1.5), 0.025, 0.004, 0.5),
            (ChannelGeometry::v(2.0, 3.0), 0.03, 0.02, 0.35),
            (ChannelGeometry::rectangle(0.6), 0.013, 0.001, 0.45),
            (ChannelGeometry::trapezoid(3.0, 2.0), 0.035, 0.0005, 2.75),
        ] {
            let q = g.discharge_m3_s(y, n, s);
            close(normal_depth_m(&g, n, s, q).unwrap(), y, 1e-9);
        }
        let g = ChannelGeometry::trapezoid(1.0, 1.5);
        assert_eq!(normal_depth_m(&g, 0.025, 0.004, 0.0).unwrap(), 0.0);
        assert!(normal_depth_m(&g, 0.0, 0.004, 1.0).is_err());
        assert!(normal_depth_m(&g, 0.025, 0.0, 1.0).is_err());
        assert!(normal_depth_m(&g, 0.025, 0.004, -1.0).is_err());
        assert!(normal_depth_m(&ChannelGeometry::v(0.0, 0.0), 0.025, 0.004, 1.0).is_err());
    }

    /// DDM Section 9.5.1, example of culvert design, Step 12: normal depth in a rectangular
    /// barrel 2.13 m wide, n = 0.012, S = 0.05, Q = 11.33 m³/s.
    #[test]
    fn section_9_5_1_step_12_normal_depth() {
        // The manual reduces Manning to (2.13·dn)·[2.13·dn / (2.13 + 2·dn)]^(2/3) = 0.608 and, by
        // trial, "use dn = 0.55 m" where the left side is 0.596, not yet 0.608. The root is
        // 0.558 m: at 0.558, 1.18854 × (1.18854 / 3.246)^(2/3) = 1.18854 × 0.51181 = 0.6083.
        let g = ChannelGeometry::rectangle(2.13);
        close(11.33 * 0.012 / 0.05f64.sqrt(), 0.608, 0.0005);
        close(g.area_m2(0.55) * g.hydraulic_radius_m(0.55).powf(2.0 / 3.0), 0.596, 0.0005);
        let dn = normal_depth_m(&g, 0.012, 0.05, 11.33).unwrap();
        close(dn, 0.558, 0.001);
        // Outlet velocity: 11.33 / (2.13 × 0.558) = 9.53 m/s. The manual's Step 12 has 9.68 (from
        // dn = 0.55) and its Step 15 quotes "9.5 m/s".
        close(11.33 / g.area_m2(dn), 9.53, 0.02);
        close(11.33 / g.area_m2(0.55), 9.68, 0.01);
    }

    #[test]
    fn froude_number_of_a_rectangular_flow() {
        // V = 2.0 m/s at y = 0.4 m: F = 2.0 / √(9.81 × 0.4) = 2.0 / 1.98091 = 1.00964.
        close(froude_number(2.0, 0.4), 1.00964, 1e-5);
    }

    #[test]
    fn table_9_1_permissible_velocities() {
        use ChannelMaterial::*;
        assert_eq!(ChannelMaterial::ALL.map(|m| m.permissible_velocity_m_s()), [0.6, 1.1, 1.5, 2.0, 3.0, 3.0, 3.0]);
        assert_eq!(ChannelMaterial::ALL.map(|m| m.roughness_coefficient_as_printed()), [45.0, 50.0, 40.0, 25.0, 25.0, 40.0, 60.0]);
        // Read as Strickler K: concrete 1/60 = 0.0167, masonry 1/40 = 0.025.
        close(Concrete.manning_n_if_strickler(), 0.016667, 1e-6);
        close(Masonry.manning_n_if_strickler(), 0.025, 1e-12);
        assert_eq!(SandLoamFineGravelVolcanicAsh.label(), "Sand, loam, fine gravel, volcanic ash");
    }

    fn drain_input(flow: f64) -> ChannelInput {
        ChannelInput {
            basis: DesignBasis::Mowt,
            geometry: ChannelGeometry::trapezoid(1.0, 1.5),
            depth_m: 0.6,
            manning_n: 0.025,
            roughness_source: "entered".into(),
            gradient_m_per_m: 0.004,
            design_flow_m3_s: flow,
            permissible_velocity_m_s: Some(ChannelMaterial::StiffClay.permissible_velocity_m_s()),
            permissible_source: "DDM Table 9.1, stiff clay".into(),
            required_freeboard_m: Some(0.15),
        }
    }

    #[test]
    fn channel_check_at_a_flow_with_a_known_normal_depth() {
        // The trapezoid of `manning_capacity_of_a_known_trapezoid` carries 1.01870 m³/s at
        // y = 0.5 m and V = 1.16422 m/s.
        let c = check_channel(&drain_input(1.01870)).unwrap();
        close(c.normal_depth_m, 0.5, 1e-5);
        close(c.velocity_m_s, 1.16422, 2e-5);
        close(c.area_m2, 0.875, 2e-5);
        // F = 1.16422 / √(9.81 × 0.35) = 1.16422 / 1.85297 = 0.6283: subcritical.
        close(c.froude, 0.6283, 1e-4);
        // Freeboard 0.6 − 0.5 = 0.1 m < 0.15 m required; V = 1.164 > 1.1 m/s for stiff clay.
        close(c.freeboard_m, 0.1, 1e-5);
        assert!(c.capacity_ok);
        assert_eq!((c.velocity_ok, c.freeboard_ok), (Some(false), Some(false)));
        assert!(!c.passes());
        // Full depth 0.6: A = 0.6 × (1.0 + 0.9) = 1.14; P = 1.0 + 1.2 × √3.25 = 3.163331;
        // R = 0.360380; R^(2/3) = 0.506413; V = 0.506413 × 0.0632456 / 0.025 = 1.28114; Q = 1.46050.
        close(c.full_capacity_m3_s, 1.46050, 5e-5);
        close(c.full_velocity_m_s, 1.28114, 5e-5);
        let text = c.calculation().join("\n");
        assert!(text.contains("Normal depth by bisection: y = 0.500 m."));
        assert!(text.contains("against a maximum permissible velocity of 1.1 m/s (DDM Table 9.1, stiff clay): FAILS."));
        assert!(text.contains("subcritical flow"));
        assert!(text.contains("against 0.150 m required (entered; DDM states no freeboard for open drains): FAILS."));
    }

    #[test]
    fn channel_check_reports_overtopping_and_unchecked_items() {
        let over = check_channel(&drain_input(2.0)).unwrap();
        assert!(!over.capacity_ok && over.freeboard_m < 0.0 && over.normal_depth_m > 0.6 && !over.passes());
        assert!(over.calculation().iter().any(|l| l.contains("design flow carried within the section: FAILS")));

        let mut input = drain_input(0.3);
        input.permissible_velocity_m_s = None;
        input.required_freeboard_m = None;
        let c = check_channel(&input).unwrap();
        assert!(c.passes() && c.velocity_ok.is_none() && c.freeboard_ok.is_none());
        let text = c.calculation().join("\n");
        assert!(text.contains("No permissible velocity was given") && text.contains("none was entered, and DDM states no freeboard for open drains"));

        // A steep concrete channel is supercritical.
        let mut steep = drain_input(1.0);
        steep.manning_n = 0.015;
        steep.gradient_m_per_m = 0.05;
        let c = check_channel(&steep).unwrap();
        assert!(c.froude > 1.0);
        assert!(c.calculation().iter().any(|l| l.contains("supercritical flow")));

        assert!(check_channel(&ChannelInput { design_flow_m3_s: 0.0, ..drain_input(1.0) }).is_err());
        assert!(check_channel(&ChannelInput { depth_m: 0.0, ..drain_input(1.0) }).is_err());
        assert!(check_channel(&ChannelInput { manning_n: -0.02, ..drain_input(1.0) }).is_err());
    }

    #[test]
    fn appendix_9_6_and_9_7_values() {
        assert_eq!(PipeEntrance::ConcreteMitredToFillSlope.ke(), 0.7);
        assert_eq!(PipeEntrance::ConcreteProjectingSquareCutEnd.ke(), 0.5);
        assert_eq!(PipeEntrance::ConcreteHeadwallSquareEdge.ke(), 0.5);
        assert_eq!(PipeEntrance::ConcreteHeadwallSocketEnd.ke(), 0.2);
        assert_eq!(PipeEntrance::ConcreteProjectingSocketEnd.ke(), 0.2);
        assert_eq!(PipeEntrance::CorrugatedMetalProjectingNoHeadwall.ke(), 0.9);
        assert_eq!(PipeEntrance::CorrugatedMetalMitredToFillSlope.ke(), 0.7);
        assert_eq!(PipeEntrance::CorrugatedMetalHeadwallSquareEdge.ke(), 0.5);
        assert_eq!(PipeEntrance::CorrugatedMetalBevelledEdges.ke(), 0.2);
        assert!(PipeEntrance::ConcreteHeadwallRounded.label().contains("1/12 D"));
        assert_eq!(PipeWall::ConcreteSmooth.manning_n_range(), (0.010, 0.013));
        assert_eq!(PipeWall::CorrugatedMetal68x13.manning_n_range(), (0.022, 0.027));
        assert_eq!(PipeWall::StructuralPlate230x64.manning_n_range(), (0.033, 0.037));
        assert_eq!(PipeWall::CorrugatedMetalHelical68x13FullCircularFlow.manning_n_range(), (0.012, 0.024));
    }

    /// DDM Section 9.5.1, example of culvert design, Step 8: barrel losses of the 2135 × 1830 mm
    /// box, L = 90 m, n = 0.012, Ke = 0.2, Q = 11.33 m³/s, area 3.90 m².
    #[test]
    fn section_9_5_1_step_8_barrel_losses_by_equation_9_6() {
        // R = A/P = 3.90 / (2 × (2.135 + 1.83)) = 3.90 / 7.93 = 0.49180 m.
        // V = 11.33 / 3.90 = 2.9051 m/s; V²/2g = 8.4397 / 19.6 = 0.43060 m.
        // 19.63 × 0.012² × 90 / 0.49180^1.33 = 0.254405 / 0.38911 = 0.65381.
        // H = (1 + 0.2 + 0.65381) × 0.43060 = 0.7983 m.
        // The manual reads H = 0.85 m from the chart of Appendix 9.14 instead: about 6 % more
        // than its own Eqn. 9.6 gives.
        let h = barrel_head_loss_m(11.33, 3.90, 3.90 / 7.93, 0.012, 90.0, 0.2);
        close(h, 0.7983, 0.0005);
        // Step 8 g: HWoi = H + ho − So·L with ho = 1.63 and So·L = 0.05 × 90 = 4.5:
        // 0.798 + 1.63 − 4.5 = −2.07 m (manual, with the chart's 0.85: −2.02 m).
        close(h + 1.63 - 4.5, -2.07, 0.005);
    }

    #[test]
    fn critical_depth_in_a_pipe() {
        // At half depth of a 1.0 m pipe A = π/8 = 0.392699 m² and T = 1.0 m, so the flow that is
        // critical there is Q = √(g·A³/T) = √(9.81 × 0.0605577) = 0.77075 m³/s.
        close(circular_critical_depth_m(0.77075, 1.0), 0.5, 1e-4);
        // It rises with the flow and stays below the crown.
        let big = circular_critical_depth_m(50.0, 1.0);
        assert!(big > 0.99 && big < 1.0);
        assert!(circular_critical_depth_m(0.1, 1.0) < 0.5);
        assert_eq!(circular_critical_depth_m(0.0, 1.0), 0.0);
    }

    fn culvert(flow: f64, tailwater: f64) -> OutletControlInput {
        OutletControlInput {
            design_flow_m3_s: flow,
            barrels: 2,
            diameter_m: 1.0,
            length_m: 20.0,
            fall_m: 0.2,
            manning_n: 0.012,
            entrance_loss_ke: PipeEntrance::ConcreteHeadwallSquareEdge.ke(),
            tailwater_m: tailwater,
        }
    }

    #[test]
    fn outlet_control_headwater_by_hand() {
        // Two 1.0 m pipes, 4.0 m³/s in total, submerged outlet TW = 1.2 m.
        // Per barrel Q = 2.0; A = π/4 = 0.785398; V = 2.54648; V²/2g = 6.48456 / 19.6 = 0.330845.
        // R = 0.25; 0.25^1.33 = 0.158220; 19.63 × 0.000144 × 20 / 0.158220 = 0.357312.
        // H = (1 + 0.5 + 0.357312) × 0.330845 = 0.61448 m.
        // dc < D so (dc + D)/2 < 1.0 < TW: ho = 1.2. HWoi = 0.61448 + 1.2 − 0.2 = 1.61448 m.
        let r = outlet_control(&culvert(4.0, 1.2)).unwrap();
        close(r.flow_per_barrel_m3_s, 2.0, 1e-12);
        close(r.area_m2, 0.785398, 1e-6);
        close(r.velocity_m_s, 2.54648, 1e-5);
        close(r.velocity_head_m, 0.330845, 1e-6);
        close(r.head_loss_m, 0.61448, 2e-5);
        assert_eq!(r.ho_m, 1.2);
        close(r.headwater_m, 1.61448, 2e-5);
        close(r.headwater_ratio, 1.61448, 2e-5);
        // Shoulder break point 2.0 m above the inlet invert: freeboard 2.0 − 1.614 = 0.386 m.
        close(r.freeboard_m(100.0, 102.0), 0.38552, 2e-5);
        assert!(r.freeboard_m(100.0, 102.0) > CULVERT_FREEBOARD_MAJOR_ROAD_M);
        // The inlet-control caveat is always there; HW/D > 1.5 is reported.
        assert!(r.warnings[0].contains("Inlet control is not computed"));
        assert!(r.warnings.iter().any(|w| w.contains("exceeds the limit of 1.5 (DDM Section 9.3)")));
        let text = r.calculation().join("\n");
        assert!(text.contains("DDM Eqn. 9.6") && text.contains("DDM Eqn. 9.11") && text.contains("HWoi/D = 1.61") && text.contains("Check: Inlet control"));
    }

    #[test]
    fn outlet_control_with_a_free_outlet_uses_dc_plus_d_over_two() {
        // No tailwater: ho = (dc + D)/2, between D/2 and D.
        let r = outlet_control(&culvert(2.0, 0.0)).unwrap();
        close(r.ho_m, (r.critical_depth_m + 1.0) / 2.0, 1e-12);
        assert!(r.critical_depth_m > 0.0 && r.critical_depth_m < 1.0);
        // Per barrel Q = 1.0: V = 1.27324; V²/2g = 0.0827112; H = 1.857312 × 0.0827112 = 0.15362.
        close(r.head_loss_m, 0.15362, 2e-5);
        close(r.headwater_m, 0.15362 + r.ho_m - 0.2, 2e-5);
        // Below 1.2·D the method is flagged as approximate; a steep fall takes it below 0.75·D.
        let level = outlet_control(&OutletControlInput { fall_m: 0.0, ..culvert(2.0, 0.0) }).unwrap();
        assert!(level.headwater_m > 0.75 && level.headwater_m < 1.2);
        assert!(level.warnings.iter().any(|w| w.contains("below 1.2·D")));
        let steep = outlet_control(&OutletControlInput { fall_m: 1.0, ..culvert(2.0, 0.0) }).unwrap();
        assert!(steep.warnings.iter().any(|w| w.contains("shall not be used")));
        // A trickle is below the ideal velocity range of Section 9.7.
        let slow = outlet_control(&culvert(0.5, 1.5)).unwrap();
        assert!(slow.warnings.iter().any(|w| w.contains("Section 9.7")));

        assert!(outlet_control(&OutletControlInput { barrels: 0, ..culvert(2.0, 0.0) }).is_err());
        assert!(outlet_control(&OutletControlInput { diameter_m: 0.0, ..culvert(2.0, 0.0) }).is_err());
        assert!(outlet_control(&OutletControlInput { tailwater_m: -0.1, ..culvert(2.0, 0.0) }).is_err());
        assert!(outlet_control(&OutletControlInput { manning_n: 0.0, ..culvert(2.0, 0.0) }).is_err());
    }
}
