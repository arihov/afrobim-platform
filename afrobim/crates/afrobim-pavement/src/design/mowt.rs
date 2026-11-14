//! Flexible pavement structure by the catalogue of the Uganda MoWT *Road Design Manual
//! Volume 3: Pavement Design, Part I: Flexible Pavements* (January 2010), called "Part I" below.
//! Part I adopts the SATCC Draft Code of Practice for the Design of Road Pavements (1998,
//! reprinted 2001) and says so in its section 1.1.
//!
//! # What this covers
//!
//! The five steps of Part I, section 1.3, as far as they are arithmetic or a table:
//!
//! | Step | Here | Part I |
//! |---|---|---|
//! | Cumulative vehicles per category, one direction | [`cumulative_traffic`] | Equation 1, section 2.2 (a)(iii) |
//! | Axle load to equivalent standard axles | [`load_equivalency`], [`table_2_3`] | Equation 2, Tables 2.2 and 2.3 |
//! | Design traffic loading from the two directions | [`RoadType`] | Table 2.4 |
//! | Traffic class T1 to T8 | [`TrafficClass`] | Table 2.5 |
//! | Subgrade class S1 to S6 | [`SubgradeClass`] | Table 3.1 |
//! | Depth over which the subgrade strength must be available | [`SubgradeClass::minimum_depth_mm`] | Table 3.3 |
//! | Dry or wet set of charts | [`Climate`] | Section 4.2, Table 4.1 |
//! | The structure for the class pair | [`catalogue_structure`] | Appendix C, charts D1 to D5 and W1 to W5 |
//! | Design life guidance, design moisture content | [`design_life_guidance_years`], [`design_moisture_content_percent`] | Table 2.1, Equation 3 |
//!
//! # What stays an input
//!
//! Part I leaves these to the engineer and so does this module: the traffic counts and their
//! split into categories and directions; the growth rate; the design period; the damage factor
//! of each vehicle category (Part I gives none: "it is considered injudicious to propose
//! values", section 2.2 (a)(iv)) or the axle loads to compute it from; the design subgrade CBR
//! (sections 3.2 and 3.3 describe how it is tested and chosen); whether the region is dry or
//! wet and how good drainage and maintenance will be (section 4); and which base and subbase
//! combination suits the materials at hand (sections 5.2 and 5.9).
//!
//! # What it does not do
//!
//! - Traffic above 30 million ESAs and subgrade CBR below 2 per cent: Part I excludes both
//!   (sections 5.6 and 5.5). The result then has no structure.
//! - The structural number route of Table 5.2 (`DSN = a1·h1 + a2·h2 + a3·h3`): Part I gives the
//!   layer coefficients but not the structural number required, for which it refers to the 1989
//!   SATCC guide. Not implemented.
//! - Surfacing design (section 6: spray rates, Otta seals, asphalt mixes), materials selection
//!   (Appendix A), drainage (Appendix B), cost comparison, sensitivity analysis.
//! - Rigid pavements, gravel roads and rehabilitation (Parts II, III and IV of Volume 3).
//!
//! # Readings of Part I that a user should know about
//!
//! - **Equation 1** is printed without its brackets. It is implemented as
//!   `DT = T · 365 · ((1 + r/100)^p − 1) / (r/100)`, which reproduces the 1.95 × 10⁶ ESA of the
//!   Appendix D example. With r = 0 the expression is 0/0; its limit `T · 365 · p` is used.
//! - **Class boundaries.** Table 2.5 prints ranges that share their end values (and "0.3-07"
//!   for 0.3 to 0.7). T1 is "< 0.3", so 0.3 is taken as T2 and every range as including its
//!   lower end; 30 is taken as T8. Table 3.1 prints whole numbers (2, 3-4, 5-7, 8-14, 15-29,
//!   30+). Section 3.3 (b) treats a CBR of about 4.5 as class S2, so a class runs up to, but
//!   not including, the first number of the next: 4.9 is S2, 5.0 is S3.
//! - **Table 2.4** gives "up to twice" the two-way ESAs for paved widths of 4.5 m or less and
//!   says double should be used at 3.5 m or less. Between 3.5 and 4.5 m the factor is an input
//!   between 1 and 2; left out, it is 2, following the table's footnote. Widths of exactly 4.5
//!   and 6.0 m are read with the narrower band. On a dual carriageway exactly 2 000 commercial
//!   vehicles a day is in neither row; the 90 per cent row is used.
//! - **Cemented subbase strength.** The keys of charts D2, W2, D5 and W5 (and the Appendix D
//!   example) call the cemented subbase "7 day UCS 1.5 - 3 MPa"; Table 5.1 and the keys of D3
//!   and W3 give 0.75 - 1.5 MPa for a cemented subbase. The layer names returned here quote the
//!   key of the chart they come from.
//! - **Table 5.3** lists the granular base / cemented subbase charts as "D2 & W3"; the charts
//!   themselves are D2 and W2.
//! - **Table 2.3** is not what Equation 2 gives at the middle of each load band (at 8 000 kg
//!   and n = 4, Equation 2 gives 0.92; the table prints 0.95), and its kilogram and kilonewton
//!   halves differ. Both are offered: [`load_equivalency`] is Equation 2, [`table_2_3`] is the
//!   table as printed. A dash in the table is returned as `None`.
//! - **Appendix D** proposes a lime-treated subbase at traffic class T4, where Table 5.3 limits
//!   lime-treated subbases to class T2.
//! - The catalogue charts are raster images. How they were read and checked is described in
//!   the `mowt_catalogue` source; one cell (D3, S2/T4) is printed inconsistently and is flagged
//!   when selected.

use super::mowt_catalogue as cat;
use super::{num, DesignError, DesignMethod, DesignResult};
use crate::model::{Layer, Material};

// ---------------------------------------------------------------------------------------------
// Traffic
// ---------------------------------------------------------------------------------------------

/// Part I, Table 2.2 "Recommended relative damage exponents, n", by base / subbase type. The
/// table's cemented base on granular subbase is not used in the guide and has no chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PavementType {
    GranularBaseGranularSubbase,
    GranularBaseCementedSubbase,
    CementedBaseCementedSubbase,
    BituminousBaseGranularSubbase,
    BituminousBaseCementedSubbase,
}

impl PavementType {
    pub const ALL: [PavementType; 5] = [
        PavementType::GranularBaseGranularSubbase,
        PavementType::GranularBaseCementedSubbase,
        PavementType::CementedBaseCementedSubbase,
        PavementType::BituminousBaseGranularSubbase,
        PavementType::BituminousBaseCementedSubbase,
    ];

    /// Relative damage exponent n, Part I Table 2.2.
    pub fn damage_exponent(self) -> f64 {
        match self {
            PavementType::GranularBaseGranularSubbase => 4.0,
            PavementType::GranularBaseCementedSubbase => 3.0,
            PavementType::CementedBaseCementedSubbase => 4.5,
            PavementType::BituminousBaseGranularSubbase => 4.0,
            PavementType::BituminousBaseCementedSubbase => 4.0,
        }
    }

    /// The chart number shared by the dry and wet sets (Appendix C.1): 1 to 5.
    pub fn chart_number(self) -> u8 {
        match self {
            PavementType::GranularBaseGranularSubbase => 1,
            PavementType::GranularBaseCementedSubbase => 2,
            PavementType::CementedBaseCementedSubbase => 3,
            PavementType::BituminousBaseGranularSubbase => 4,
            PavementType::BituminousBaseCementedSubbase => 5,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PavementType::GranularBaseGranularSubbase => "granular base / granular subbase",
            PavementType::GranularBaseCementedSubbase => "granular base / cemented subbase",
            PavementType::CementedBaseCementedSubbase => "cemented base / cemented subbase",
            PavementType::BituminousBaseGranularSubbase => "bituminous base / granular subbase",
            PavementType::BituminousBaseCementedSubbase => "bituminous base / cemented subbase",
        }
    }
}

/// Part I, Equation 1: cumulative traffic of one vehicle category in one direction over the
/// design period.
///
/// ```text
///   DT = T · 365 · ((1 + r/100)^p − 1) / (r/100)
/// ```
///
/// `T` is the average daily traffic of the category in the first year, one direction; `r` the
/// growth rate, per cent per annum; `p` the design period, years. At r = 0 the limit
/// `T · 365 · p` is returned.
pub fn cumulative_traffic(daily_first_year: f64, growth_percent: f64, period_years: u32) -> f64 {
    let (r, p) = (growth_percent / 100.0, f64::from(period_years));
    let factor = if r == 0.0 { p } else { ((1.0 + r).powf(p) - 1.0) / r };
    daily_first_year * 365.0 * factor
}

/// Part I, Equation 2 for an axle load in kilonewtons: `F = (P / 80)^n`.
pub fn load_equivalency(axle_load_kn: f64, exponent: f64) -> f64 {
    (axle_load_kn / 80.0).powf(exponent)
}

/// Part I, Equation 2 for an axle load in kilograms: `F = (P / 8160)^n`.
pub fn load_equivalency_kg(axle_load_kg: f64, exponent: f64) -> f64 {
    (axle_load_kg / 8160.0).powf(exponent)
}

/// The unit of an axle load looked up in [`table_2_3`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadUnit {
    Kilograms,
    Kilonewtons,
}

/// Part I, Table 2.3, kilogram half: first load of each band (the band runs to just below the
/// next), then the factors for n = 3, 4 and 4.5. Negative = the table prints a dash.
const TABLE_2_3_KG: [(f64, [f64; 3]); 20] = [
    (1500.0, [0.02, -1.0, -1.0]),
    (2500.0, [0.05, 0.02, 0.01]),
    (3500.0, [0.12, 0.06, 0.05]),
    (4500.0, [0.24, 0.15, 0.12]),
    (5500.0, [0.41, 0.30, 0.26]),
    (6500.0, [0.64, 0.56, 0.52]),
    (7500.0, [0.95, 0.95, 0.94]),
    (8500.0, [1.35, 1.51, 1.59]),
    (9500.0, [1.85, 2.29, 2.55]),
    (10500.0, [2.46, 3.34, 3.90]),
    (11500.0, [3.20, 4.72, 5.75]),
    (12500.0, [4.06, 6.50, 8.22]),
    (13500.0, [5.07, 8.73, 11.46]),
    (14500.0, [6.23, 11.49, 15.61]),
    (15500.0, [7.56, 14.87, 20.85]),
    (16500.0, [9.06, 18.93, 27.37]),
    (17500.0, [10.76, 23.78, 35.37]),
    (18500.0, [12.65, 29.51, 45.09]),
    (19500.0, [14.75, 36.22, 56.77]),
    (20500.0, [-1.0, -1.0, -1.0]), // end of the table: 19500-20499 is the last band
];

/// Part I, Table 2.3, kilonewton half, laid out as [`TABLE_2_3_KG`].
const TABLE_2_3_KN: [(f64, [f64; 3]); 20] = [
    (15.0, [0.02, -1.0, -1.0]),
    (25.0, [0.05, 0.02, 0.01]),
    (35.0, [0.13, 0.06, 0.05]),
    (45.0, [0.24, 0.15, 0.12]),
    (55.0, [0.42, 0.32, 0.28]),
    (65.0, [0.66, 0.58, 0.55]),
    (75.0, [0.99, 0.99, 1.00]),
    (85.0, [1.41, 1.59, 1.69]),
    (95.0, [1.94, 2.42, 2.71]),
    (105.0, [2.58, 3.55, 4.16]),
    (115.0, [3.35, 5.02, 6.15]),
    (125.0, [4.26, 6.92, 8.82]),
    (135.0, [5.32, 9.3, 12.31]),
    (145.0, [6.54, 12.26, 16.79]),
    (155.0, [7.94, 15.88, 22.45]),
    (165.0, [9.53, 20.24, 29.50]),
    (175.0, [11.32, 25.44, 38.15]),
    (185.0, [13.31, 31.59, 48.67]),
    (195.0, [15.53, 38.79, 61.32]),
    (205.0, [-1.0, -1.0, -1.0]), // end of the table: 195-204 is the last band
];

/// Part I, Table 2.3 "Load equivalency factors for different axle load groups, in ESAs", as
/// printed. `exponent` must be 3, 4 or 4.5. `None` when the table prints a dash (light axles),
/// when the load is above the last band (20 499 kg, 204 kN) or the exponent is not tabulated.
pub fn table_2_3(axle_load: f64, unit: LoadUnit, exponent: f64) -> Option<f64> {
    let column = [3.0, 4.0, 4.5].iter().position(|n| *n == exponent)?;
    let table = match unit {
        LoadUnit::Kilograms => &TABLE_2_3_KG,
        LoadUnit::Kilonewtons => &TABLE_2_3_KN,
    };
    let row = table.iter().rev().find(|(from, _)| axle_load >= *from)?;
    Some(row.1[column]).filter(|f| *f >= 0.0)
}

/// How the equivalent standard axles of one vehicle are obtained.
#[derive(Debug, Clone, PartialEq)]
pub enum DamageFactor {
    /// The average damage factor of the category from an axle load survey, ESAs per vehicle,
    /// as entered.
    EsaPerVehicle(f64),
    /// The axle loads of a representative vehicle, kN; Equation 2 is applied to each axle with
    /// the exponent of the chosen pavement type (Table 2.2) and the results are added. No
    /// allowance is made for tandem or tridem groups, as section 2.2 (a)(iv) recommends.
    AxleLoadsKn(Vec<f64>),
    /// The same with the loads in kilograms.
    AxleLoadsKg(Vec<f64>),
}

impl DamageFactor {
    /// ESAs per vehicle for the given exponent.
    pub fn esa_per_vehicle(&self, exponent: f64) -> f64 {
        match self {
            DamageFactor::EsaPerVehicle(f) => *f,
            DamageFactor::AxleLoadsKn(loads) => loads.iter().map(|p| load_equivalency(*p, exponent)).sum(),
            DamageFactor::AxleLoadsKg(loads) => loads.iter().map(|p| load_equivalency_kg(*p, exponent)).sum(),
        }
    }
}

/// One vehicle category in one direction.
#[derive(Debug, Clone, PartialEq)]
pub struct VehicleClass {
    pub name: String,
    /// `T` of Equation 1: average daily number in the first year, this direction.
    pub daily_first_year: f64,
    /// Growth rate of this category, per cent per annum; `None` takes the input's general rate.
    pub growth_percent: Option<f64>,
    pub damage: DamageFactor,
}

impl VehicleClass {
    pub fn new(name: &str, daily_first_year: f64, esa_per_vehicle: f64) -> VehicleClass {
        VehicleClass { name: name.to_string(), daily_first_year, growth_percent: None, damage: DamageFactor::EsaPerVehicle(esa_per_vehicle) }
    }
}

/// The road, for Part I Table 2.4 "Factors for design traffic loading".
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RoadType {
    SingleCarriageway {
        paved_width_m: f64,
        /// For paved widths over 3.5 m and up to 4.5 m only: the multiple of the two-way ESAs to
        /// design for, between 1 and 2 ("up to twice"). `None` takes 2, as the table's
        /// footnote advises.
        narrow_road_factor: Option<f64>,
    },
    DualCarriageway {
        /// Commercial vehicles per day in one direction, which selects the 90 or 80 per cent row.
        commercial_vehicles_per_day_one_direction: f64,
    },
}

/// Traffic for the design.
#[derive(Debug, Clone, PartialEq)]
pub struct TrafficInput {
    /// The vehicle categories of one direction.
    pub direction_a: Vec<VehicleClass>,
    /// The other direction. `None` means it carries the same as `direction_a`.
    pub direction_b: Option<Vec<VehicleClass>>,
    /// Growth rate, per cent per annum, for categories that do not carry their own.
    pub growth_percent: f64,
    /// Design period `p`, years.
    pub design_period_years: u32,
    pub road: RoadType,
}

/// Part I, Table 2.5 "Traffic classes".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TrafficClass {
    T1,
    T2,
    T3,
    T4,
    T5,
    T6,
    T7,
    T8,
}

impl TrafficClass {
    pub const ALL: [TrafficClass; 8] =
        [TrafficClass::T1, TrafficClass::T2, TrafficClass::T3, TrafficClass::T4, TrafficClass::T5, TrafficClass::T6, TrafficClass::T7, TrafficClass::T8];

    /// Upper limits of T1 to T8, million ESAs: Part I Table 2.5 (and the heading of every chart
    /// of Appendix C).
    const UPPER_MILLION_ESA: [f64; 8] = [0.3, 0.7, 1.5, 3.0, 6.0, 10.0, 17.0, 30.0];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        ["T1", "T2", "T3", "T4", "T5", "T6", "T7", "T8"][self.index()]
    }

    /// The range of the class, million ESAs: (from, to). T1 starts at zero.
    pub fn range_million_esa(self) -> (f64, f64) {
        let i = self.index();
        (if i == 0 { 0.0 } else { Self::UPPER_MILLION_ESA[i - 1] }, Self::UPPER_MILLION_ESA[i])
    }

    /// The class for a design traffic loading in million ESAs. A value on a boundary is given
    /// the higher class, except 30, which is T8 (see the module documentation). `None` above
    /// 30 million, which Part I does not cover (section 5.6), or for a negative or non-number.
    pub fn for_million_esa(million_esa: f64) -> Option<TrafficClass> {
        if !(million_esa >= 0.0) || million_esa > 30.0 {
            return None;
        }
        Some(Self::ALL.into_iter().find(|c| million_esa < c.range_million_esa().1).unwrap_or(TrafficClass::T8))
    }
}

// ---------------------------------------------------------------------------------------------
// Subgrade and climate
// ---------------------------------------------------------------------------------------------

/// Part I, Table 3.1 "Subgrade classification".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SubgradeClass {
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
}

impl SubgradeClass {
    pub const ALL: [SubgradeClass; 6] = [SubgradeClass::S1, SubgradeClass::S2, SubgradeClass::S3, SubgradeClass::S4, SubgradeClass::S5, SubgradeClass::S6];

    /// First CBR of each class, per cent: Part I Table 3.1 (2, 3-4, 5-7, 8-14, 15-29, 30+).
    const FROM_CBR: [f64; 6] = [2.0, 3.0, 5.0, 8.0, 15.0, 30.0];

    /// Part I, Table 3.3 "Recommended minimum subgrade depth meeting design strength", mm.
    const MINIMUM_DEPTH_MM: [f64; 6] = [250.0, 250.0, 350.0, 450.0, 550.0, 650.0];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        ["S1", "S2", "S3", "S4", "S5", "S6"][self.index()]
    }

    /// The CBR range as Table 3.1 prints it.
    pub fn printed_range(self) -> &'static str {
        ["2", "3-4", "5-7", "8-14", "15-29", "30+"][self.index()]
    }

    /// The class for a design subgrade CBR in per cent. `None` below 2 per cent, which Part I
    /// does not cover (section 5.5). A class runs up to, but not including, the first value of
    /// the next (see the module documentation).
    pub fn for_cbr(cbr_percent: f64) -> Option<SubgradeClass> {
        if !(cbr_percent >= Self::FROM_CBR[0]) {
            return None;
        }
        Self::ALL.into_iter().rev().find(|c| cbr_percent >= Self::FROM_CBR[c.index()])
    }

    /// Depth below the pavement layers to which the engineer should confirm the design subgrade
    /// strength is available, mm (Part I Table 3.3). It is not a depth of reworking.
    pub fn minimum_depth_mm(self) -> f64 {
        Self::MINIMUM_DEPTH_MM[self.index()]
    }
}

/// Part I, Equation 3 (section 3.2 b): design moisture content for CBR testing in dry areas,
/// per cent, from the optimum moisture content of AASHTO T-99: `0.67 · OMC + 0.8`.
pub fn design_moisture_content_percent(omc_percent: f64) -> f64 {
    0.67 * omc_percent + 0.8
}

/// A two-level judgement used by Tables 2.1 and 4.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Low,
    High,
}

/// Part I, Table 2.1 "Pavement design life selection guidance": the range of design life in
/// years for the reliability of the design data and the importance or level of service of the
/// road. Section 2.1 adds that the life should most times be 15 or 20 years, with 10 the
/// recommended minimum and 20 the maximum.
pub fn design_life_guidance_years(data_reliability: Level, importance: Level) -> (u32, u32) {
    match (data_reliability, importance) {
        (Level::Low, Level::Low) => (10, 15),
        (Level::Low, Level::High) => (15, 15),
        (Level::High, Level::Low) => (10, 20),
        (Level::High, Level::High) => (15, 20),
    }
}

/// How well something is expected to be provided, for Part I Table 4.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provision {
    /// Drainage: "good, well planned, well constructed". Maintenance: "good, programmed,
    /// defects remedied timeously".
    Good,
    Deficient,
}

/// The engineer's statement of the operating climate (Part I, section 4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Climate {
    /// A predominantly dry region in the sense of section 4.2: annual rainfall below 250 mm
    /// with no likelihood of moisture ingress, or a higher, evenly spread rainfall that the
    /// Engineer deems dry. The section adds that regions with more than 500 mm a year are
    /// unlikely to be regarded as dry.
    PredominantlyDry,
    /// Anything else. Table 4.1 then chooses between the dry and wet charts from the expected
    /// drainage, maintenance and traffic class.
    PredominantlyWet { drainage: Provision, maintenance: Provision },
}

/// The set of charts to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChartSet {
    Dry,
    Wet,
}

impl Climate {
    /// The chart set for a traffic class: section 4.2 for a dry region, Table 4.1 "Guide to
    /// selecting design conditions for predominantly wet regions" otherwise.
    pub fn chart_set(self, traffic: TrafficClass) -> ChartSet {
        let low_traffic = traffic <= TrafficClass::T2; // Table 4.1: "Low, class T1 or T2"; "High, class T3 and more"
        match self {
            Climate::PredominantlyDry => ChartSet::Dry,
            Climate::PredominantlyWet { drainage: Provision::Good, maintenance: Provision::Good } => ChartSet::Dry,
            Climate::PredominantlyWet { drainage: Provision::Deficient, maintenance: Provision::Deficient } => ChartSet::Wet,
            Climate::PredominantlyWet { .. } => {
                if low_traffic {
                    ChartSet::Dry
                } else {
                    ChartSet::Wet
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Catalogue
// ---------------------------------------------------------------------------------------------

/// The surfacing a chart shows on a structure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Surfacing {
    /// "SD": a surface dressing, with no thickness.
    SurfaceDressing,
    /// Hot mix asphalt of the thickness shown, mm.
    HotMixAsphalt(f64),
}

/// One structure of the Appendix C catalogue. A thickness of `None` means the layer is absent.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogueStructure {
    /// "D1" to "D5" or "W1" to "W5".
    pub chart: &'static str,
    pub pavement: PavementType,
    pub set: ChartSet,
    pub traffic: TrafficClass,
    pub subgrade: SubgradeClass,
    pub surfacing: Surfacing,
    pub base_mm: f64,
    /// The cemented upper subbase (7 day UCS 3 - 5 MPa in the chart key) of some D2 and W2
    /// structures.
    pub upper_subbase_mm: Option<f64>,
    pub subbase_mm: Option<f64>,
    pub selected_layer_mm: Option<f64>,
    /// A doubt about this cell as printed, if any.
    pub note: Option<&'static str>,
}

/// Name of the chart for a pavement type and set: "D1" ... "W5".
pub fn chart_name(pavement: PavementType, set: ChartSet) -> &'static str {
    let n = usize::from(pavement.chart_number()) - 1;
    match set {
        ChartSet::Dry => ["D1", "D2", "D3", "D4", "D5"][n],
        ChartSet::Wet => ["W1", "W2", "W3", "W4", "W5"][n],
    }
}

fn chart(pavement: PavementType, set: ChartSet) -> &'static cat::Chart {
    match (set, pavement.chart_number()) {
        (ChartSet::Dry, 1) => &cat::D1,
        (ChartSet::Dry, 2) => &cat::D2,
        (ChartSet::Dry, 3) => &cat::D3,
        (ChartSet::Dry, 4) => &cat::D4,
        (ChartSet::Dry, _) => &cat::D5,
        (ChartSet::Wet, 1) => &cat::W1,
        (ChartSet::Wet, 2) => &cat::W2,
        (ChartSet::Wet, 3) => &cat::W3,
        (ChartSet::Wet, 4) => &cat::W4,
        (ChartSet::Wet, _) => &cat::W5,
    }
}

/// The structure Appendix C gives for a pavement type, chart set, traffic class and subgrade
/// class. `None` where the chart is empty: D3 and W3 stop at T6, and D4, D5, W4 and W5 start
/// at T5.
pub fn catalogue_structure(pavement: PavementType, set: ChartSet, traffic: TrafficClass, subgrade: SubgradeClass) -> Option<CatalogueStructure> {
    let name = chart_name(pavement, set);
    let [surfacing, base, upper, subbase, selected] = chart(pavement, set)[subgrade.index()][traffic.index()]?;
    let some = |v: u16| (v > 0).then_some(f64::from(v));
    Some(CatalogueStructure {
        chart: name,
        pavement,
        set,
        traffic,
        subgrade,
        surfacing: if surfacing == 0 { Surfacing::SurfaceDressing } else { Surfacing::HotMixAsphalt(f64::from(surfacing)) },
        base_mm: f64::from(base),
        upper_subbase_mm: some(upper),
        subbase_mm: some(subbase),
        selected_layer_mm: some(selected),
        note: cat::cell_note(name, subgrade.index(), traffic.index()),
    })
}

impl CatalogueStructure {
    /// Sum of the layer thicknesses, mm (a surface dressing counts as zero).
    pub fn total_thickness_mm(&self) -> f64 {
        let surfacing = match self.surfacing {
            Surfacing::SurfaceDressing => 0.0,
            Surfacing::HotMixAsphalt(t) => t,
        };
        surfacing + self.base_mm + self.upper_subbase_mm.unwrap_or(0.0) + self.subbase_mm.unwrap_or(0.0) + self.selected_layer_mm.unwrap_or(0.0)
    }

    /// The structure as layers, top first. The names quote the key of the chart the structure
    /// comes from; a surface dressing is a layer of zero thickness.
    pub fn layers(&self) -> Vec<Layer> {
        let mut out = Vec::new();
        match self.surfacing {
            Surfacing::SurfaceDressing => out.push(Layer::new("Surface dressing", Material::SurfaceDressing, 0.0)),
            Surfacing::HotMixAsphalt(t) => out.push(Layer::new("Hot mix asphalt surfacing", Material::HotMixAsphalt, t)),
        }
        use PavementType::*;
        out.push(match self.pavement {
            GranularBaseGranularSubbase | GranularBaseCementedSubbase => Layer::new("Granular base (soaked CBR > 80%)", Material::GranularBase, self.base_mm),
            CementedBaseCementedSubbase => Layer::new("Cemented base (7 day UCS 1.5 - 3 MPa)", Material::CementedBase, self.base_mm),
            BituminousBaseGranularSubbase | BituminousBaseCementedSubbase => Layer::new("Bituminous base", Material::BituminousBase, self.base_mm),
        });
        if let Some(t) = self.upper_subbase_mm {
            out.push(Layer::new("Cemented upper subbase (7 day UCS 3 - 5 MPa)", Material::CementedUpperSubbase, t));
        }
        if let Some(t) = self.subbase_mm {
            out.push(match self.pavement {
                GranularBaseGranularSubbase | BituminousBaseGranularSubbase => Layer::new("Granular subbase (soaked CBR > 30%)", Material::GranularSubbase, t),
                CementedBaseCementedSubbase => Layer::new("Cemented subbase (7 day UCS 0.75 - 1.5 MPa)", Material::CementedSubbase, t),
                GranularBaseCementedSubbase | BituminousBaseCementedSubbase => Layer::new("Cemented subbase (7 day UCS 1.5 - 3 MPa)", Material::CementedSubbase, t),
            });
        }
        if let Some(t) = self.selected_layer_mm {
            out.push(Layer::new("Selected layer (soaked CBR > 15%)", Material::SelectedLayer, t));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// The design
// ---------------------------------------------------------------------------------------------

/// Everything the method needs.
#[derive(Debug, Clone, PartialEq)]
pub struct MowtInput {
    pub traffic: TrafficInput,
    /// Design subgrade CBR, per cent, tested and chosen as Part I sections 3.2 and 3.3 describe.
    pub subgrade_cbr_percent: f64,
    pub climate: Climate,
    /// The base and subbase combination the engineer is considering. It selects the chart and,
    /// where axle loads are entered, the damage exponent.
    pub pavement: PavementType,
}

/// One vehicle category worked through.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassTraffic {
    pub name: String,
    pub daily_first_year: f64,
    pub growth_percent: f64,
    /// `DT` of Equation 1.
    pub cumulative_vehicles: f64,
    pub esa_per_vehicle: f64,
    pub cumulative_esa: f64,
}

/// The result.
#[derive(Debug, Clone, PartialEq)]
pub struct MowtDesign {
    pub input: MowtInput,
    pub exponent: f64,
    /// Direction A, then direction B (a copy of A when the input has none).
    pub directions: [Vec<ClassTraffic>; 2],
    /// Cumulative ESAs of each direction.
    pub direction_esa: [f64; 2],
    /// The Table 2.4 rule applied, in words.
    pub loading_rule: String,
    /// Design traffic loading, ESAs.
    pub design_esa: f64,
    /// `None` above 30 million ESAs.
    pub traffic_class: Option<TrafficClass>,
    /// `None` below CBR 2 per cent.
    pub subgrade_class: Option<SubgradeClass>,
    pub chart_set: Option<ChartSet>,
    /// `None` when a class is missing or the chart is empty for the pair.
    pub structure: Option<CatalogueStructure>,
    pub warnings: Vec<String>,
}

fn finite_non_negative(v: f64) -> bool {
    v.is_finite() && v >= 0.0
}

fn work_direction(classes: &[VehicleClass], input: &TrafficInput, exponent: f64, which: &str) -> Result<Vec<ClassTraffic>, DesignError> {
    if classes.is_empty() {
        return Err(DesignError::InvalidInput(format!("Direction {which} has no vehicle categories.")));
    }
    classes
        .iter()
        .map(|c| {
            let growth = c.growth_percent.unwrap_or(input.growth_percent);
            let esa_per_vehicle = c.damage.esa_per_vehicle(exponent);
            if !finite_non_negative(c.daily_first_year) || !finite_non_negative(esa_per_vehicle) || !growth.is_finite() || growth <= -100.0 {
                return Err(DesignError::InvalidInput(format!("Vehicle category '{}' (direction {which}) has a count, growth rate or damage factor that cannot be used.", c.name)));
            }
            let cumulative_vehicles = cumulative_traffic(c.daily_first_year, growth, input.design_period_years);
            Ok(ClassTraffic {
                name: c.name.clone(),
                daily_first_year: c.daily_first_year,
                growth_percent: growth,
                cumulative_vehicles,
                esa_per_vehicle,
                cumulative_esa: cumulative_vehicles * esa_per_vehicle,
            })
        })
        .collect()
}

/// Part I, Table 2.4: the design traffic loading from the cumulative ESAs of the two
/// directions, and the rule in words.
fn design_loading(road: RoadType, a: f64, b: f64) -> Result<(f64, String), DesignError> {
    let (sum, heavier) = (a + b, a.max(b));
    match road {
        RoadType::SingleCarriageway { paved_width_m: w, narrow_road_factor } => {
            if !(w.is_finite() && w > 0.0) {
                return Err(DesignError::InvalidInput(format!("The paved width must be greater than zero (it is {w}).")));
            }
            if w <= 3.5 {
                Ok((2.0 * sum, format!("single carriageway, paved width {w} m (3.5 m or less): double the sum of the ESAs in each direction, for channelisation")))
            } else if w <= 4.5 {
                let f = narrow_road_factor.unwrap_or(2.0);
                if !(1.0..=2.0).contains(&f) {
                    return Err(DesignError::InvalidInput(format!("The factor for a paved width of 4.5 m or less must be between 1 and 2 (it is {f}).")));
                }
                let why = if narrow_road_factor.is_some() { "as entered" } else { "the table's footnote advises double" };
                Ok((f * sum, format!("single carriageway, paved width {w} m (4.5 m or less): {} × the sum of the ESAs in each direction (\"up to twice\"; {why})", num(f, 3))))
            } else if w <= 6.0 {
                Ok((0.8 * sum, format!("single carriageway, paved width {w} m (4.5 m to 6.0 m): 80% of the sum of the ESAs in each direction")))
            } else {
                Ok((heavier, format!("single carriageway, paved width {w} m (more than 6.0 m): total ESAs in the most heavily trafficked direction")))
            }
        }
        RoadType::DualCarriageway { commercial_vehicles_per_day_one_direction: cv } => {
            if !finite_non_negative(cv) {
                return Err(DesignError::InvalidInput(format!("The number of commercial vehicles per day cannot be {cv}.")));
            }
            if cv > 2000.0 {
                Ok((0.8 * heavier, format!("dual carriageway, {cv} commercial vehicles per day in one direction (more than 2 000): 80% of the total ESAs in the direction")))
            } else {
                Ok((0.9 * heavier, format!("dual carriageway, {cv} commercial vehicles per day in one direction (less than 2 000): 90% of the total ESAs in the direction")))
            }
        }
    }
}

/// Runs the Part I procedure. Fails only on input that cannot be used; traffic or subgrade
/// outside the range of Part I gives a result without a structure and a warning saying why.
pub fn design(input: &MowtInput) -> Result<MowtDesign, DesignError> {
    let t = &input.traffic;
    if t.design_period_years == 0 {
        return Err(DesignError::InvalidInput("The design period must be at least one year.".to_string()));
    }
    if !t.growth_percent.is_finite() || t.growth_percent <= -100.0 {
        return Err(DesignError::InvalidInput(format!("The growth rate cannot be {} per cent.", t.growth_percent)));
    }
    if !(input.subgrade_cbr_percent.is_finite() && input.subgrade_cbr_percent > 0.0) {
        return Err(DesignError::InvalidInput(format!("The subgrade CBR must be greater than zero (it is {}).", input.subgrade_cbr_percent)));
    }
    let exponent = input.pavement.damage_exponent();
    let a = work_direction(&t.direction_a, t, exponent, "A")?;
    let b = match &t.direction_b {
        Some(classes) => work_direction(classes, t, exponent, "B")?,
        None => a.clone(),
    };
    let direction_esa = [a.iter().map(|c| c.cumulative_esa).sum::<f64>(), b.iter().map(|c| c.cumulative_esa).sum::<f64>()];
    let (design_esa, loading_rule) = design_loading(t.road, direction_esa[0], direction_esa[1])?;
    let million = design_esa / 1.0e6;

    let mut warnings = Vec::new();
    // Section 2.1: maximum 20 years, minimum 10 recommended.
    if !(10..=20).contains(&t.design_period_years) {
        warnings.push(format!(
            "The design period is {} years. Part I recommends a maximum design life of 20 years and a minimum of 10 (section 2.1, Table 2.1).",
            t.design_period_years
        ));
    }
    // Section 2.2 (a)(iii): "Growth rates will normally be in the range of 2 to 15 per cent per annum".
    for c in a.iter().chain(t.direction_b.as_ref().map(|_| b.iter()).into_iter().flatten()) {
        if !(2.0..=15.0).contains(&c.growth_percent) {
            warnings.push(format!(
                "The growth rate of '{}' is {} per cent. Part I says growth rates will normally be in the range of 2 to 15 per cent per annum (section 2.2 (a)(iii)).",
                c.name,
                num(c.growth_percent, 3)
            ));
        }
    }
    if matches!(t.road, RoadType::DualCarriageway { commercial_vehicles_per_day_one_direction } if commercial_vehicles_per_day_one_direction == 2000.0) {
        warnings.push("Exactly 2 000 commercial vehicles per day is in neither row of Table 2.4 (\"less than\" and \"more than\" 2 000); the 90 per cent row is used.".to_string());
    }

    let traffic_class = TrafficClass::for_million_esa(million);
    match traffic_class {
        None => warnings.push(format!(
            "The design traffic loading is {} million ESAs. Part I is not for more than 30 million; other established design methods must be used (section 5.6). No structure is given.",
            num(million, 3)
        )),
        Some(_) if million < 0.1 => warnings.push(
            "The design traffic loading is less than 0.1 million ESAs. Part I recommends also considering alternative designs proven locally for such light trafficking (section 2.3).".to_string(),
        ),
        Some(_) => {}
    }
    let subgrade_class = SubgradeClass::for_cbr(input.subgrade_cbr_percent);
    if subgrade_class.is_none() {
        warnings.push(format!(
            "The subgrade CBR is {} per cent. Part I does not cover a subgrade CBR below 2 per cent; the subgrade must be treated and re-evaluated first (section 5.5). No structure is given.",
            num(input.subgrade_cbr_percent, 2)
        ));
    }
    let chart_set = traffic_class.map(|c| input.climate.chart_set(c));
    let mut structure = None;
    if let (Some(tc), Some(sc), Some(set)) = (traffic_class, subgrade_class, chart_set) {
        let name = chart_name(input.pavement, set);
        structure = catalogue_structure(input.pavement, set, tc, sc);
        match &structure {
            None => warnings.push(format!(
                "Chart {name} ({}) gives no structure for traffic class {}; choose another base and subbase combination.",
                input.pavement.label(),
                tc.label()
            )),
            Some(s) => {
                if let Some(doubt) = s.note {
                    warnings.push(doubt.to_string());
                }
                // Note printed under charts D1, D2 and D4.
                if matches!(name, "D1" | "D2" | "D4") && s.surfacing == Surfacing::HotMixAsphalt(50.0) {
                    warnings.push(format!("Chart {name} notes: \"50mm hot mix asphalt layer can be reduced to 40mm where local experience shows this to be adequate\"."));
                }
                // Note in Appendix C.1 on charts D2 and W2.
                if s.upper_subbase_mm.is_some() {
                    warnings.push(
                        "Appendix C notes for charts D2 and W2: considering the construction time of cemented subbase, the upper and lower cemented subbase may be merged into one layer, and if the thickness is more than 250 mm the remaining portion may be provided by increasing the granular base course.".to_string(),
                    );
                }
                if name == "W3" {
                    warnings.push(
                        "Chart W3 notes that its layer thicknesses are adapted from TRL ORN 31, Chart 8, since a flexible surfacing such as a double surface dressing is recommended when cemented material is used as a road base.".to_string(),
                    );
                }
                if matches!(input.pavement, PavementType::GranularBaseCementedSubbase | PavementType::BituminousBaseCementedSubbase) {
                    warnings.push(format!(
                        "The key of chart {name} gives the cemented subbase as 7 day UCS 1.5 - 3 MPa, where Table 5.1 gives 0.75 - 1.5 MPa for a cemented subbase. Confirm which strength is intended. Table 5.3 limits a lime-treated subbase to class T2 and a bitumen emulsion treated one to class T4."
                    ));
                }
                if input.pavement == PavementType::CementedBaseCementedSubbase {
                    warnings.push("Table 5.3: a bitumen emulsion treated base is permissible to class T3; a lime-treated subbase to class T2 and a bitumen emulsion treated subbase to class T4.".to_string());
                }
                // Section 5.9, construction note.
                if s.layers().iter().any(|l| l.thickness_mm > 200.0) {
                    warnings.push(
                        "The structure has a layer thicker than 200 mm. Construction lift thicknesses are to be defined by the Engineer; compacted lifts greater than 200 mm or less than 100 mm should generally be avoided (section 5.9, construction note).".to_string(),
                    );
                }
            }
        }
    }

    Ok(MowtDesign {
        input: input.clone(),
        exponent,
        directions: [a, b],
        direction_esa,
        loading_rule,
        design_esa,
        traffic_class,
        subgrade_class,
        chart_set,
        structure,
        warnings,
    })
}

impl DesignResult for MowtDesign {
    fn method(&self) -> DesignMethod {
        DesignMethod::MowtRdmVol3
    }

    fn calculation(&self) -> Vec<String> {
        let t = &self.input.traffic;
        let p = t.design_period_years;
        let mut out = vec![
            format!("Design basis: {}.", DesignMethod::MowtRdmVol3.reference()),
            format!("Pavement type considered: {} (the engineer's choice, sections 5.2 and 5.9).", self.input.pavement.label()),
            format!("Design period p = {p} years (section 2.1). Cumulative traffic by Equation 1: DT = T · 365 · ((1 + r/100)^p − 1) / (r/100)."),
        ];
        let uses_axles = [&t.direction_a].into_iter().chain(t.direction_b.as_ref()).flatten().any(|c| !matches!(c.damage, DamageFactor::EsaPerVehicle(_)));
        if uses_axles {
            out.push(format!(
                "Damage factors from axle loads by Equation 2, F = (P/80)^n in kN or (P/8160)^n in kg, with n = {} for {} (Table 2.2), summed over the axles.",
                num(self.exponent, 1),
                self.input.pavement.label()
            ));
        }
        let same = t.direction_b.is_none();
        for (i, classes) in self.directions.iter().enumerate() {
            if i == 1 && same {
                out.push(format!("Direction B is taken to carry the same as direction A: {} ESAs.", num(self.direction_esa[1], 0)));
                break;
            }
            let which = ["A", "B"][i];
            for c in classes {
                out.push(format!(
                    "Direction {which}, {}: T = {} per day, r = {}%, DT = {} vehicles; × {} ESA per vehicle = {} ESAs.",
                    c.name,
                    num(c.daily_first_year, 2),
                    num(c.growth_percent, 3),
                    num(c.cumulative_vehicles, 0),
                    num(c.esa_per_vehicle, 4),
                    num(c.cumulative_esa, 0)
                ));
            }
            out.push(format!("Direction {which}: sum = {} ESAs (section 2.2 (a)(v)).", num(self.direction_esa[i], 0)));
        }
        out.push(format!("Table 2.4, {}.", self.loading_rule));
        out.push(format!("Design traffic loading = {} ESAs = {} million ESAs.", num(self.design_esa, 0), num(self.design_esa / 1.0e6, 3)));
        match self.traffic_class {
            Some(c) => {
                let (from, to) = c.range_million_esa();
                out.push(format!(
                    "Traffic class {} ({} to {} million ESAs, Table 2.5). The loading is {} million above the lower limit and {} million below the upper; if that is close, review the forecast as section 2.3 asks.",
                    c.label(),
                    num(from, 1),
                    num(to, 1),
                    num(self.design_esa / 1.0e6 - from, 3),
                    num(to - self.design_esa / 1.0e6, 3)
                ));
            }
            None => out.push("Traffic class: none; the loading is above 30 million ESAs (Table 2.5, section 5.6).".to_string()),
        }
        match self.subgrade_class {
            Some(c) => out.push(format!(
                "Design subgrade CBR = {}% (entered): subgrade class {} (CBR {}%, Table 3.1). Confirm that this strength is available to at least {} mm below the pavement layers (Table 3.3).",
                num(self.input.subgrade_cbr_percent, 2),
                c.label(),
                c.printed_range(),
                num(c.minimum_depth_mm(), 0)
            )),
            None => out.push(format!("Design subgrade CBR = {}% (entered): below class S1 (Table 3.1, section 5.5).", num(self.input.subgrade_cbr_percent, 2))),
        }
        if let (Some(set), Some(tc)) = (self.chart_set, self.traffic_class) {
            let set_name = if set == ChartSet::Dry { "dry" } else { "wet" };
            let why = match self.input.climate {
                Climate::PredominantlyDry => "predominantly dry region as stated by the engineer (section 4.2)".to_string(),
                Climate::PredominantlyWet { drainage, maintenance } => format!(
                    "predominantly wet region, drainage {}, maintenance {}, traffic class {} (Table 4.1)",
                    if drainage == Provision::Good { "good" } else { "deficient" },
                    if maintenance == Provision::Good { "good" } else { "deficient" },
                    tc.label()
                ),
            };
            out.push(format!("Design conditions: {set_name} region charts; {why}."));
            out.push(format!("Chart {}: {} (Appendix C, Table 5.3).", chart_name(self.input.pavement, set), self.input.pavement.label()));
        }
        match &self.structure {
            Some(s) => {
                for l in s.layers() {
                    if l.thickness_mm > 0.0 {
                        out.push(format!("  {} mm  {}", num(l.thickness_mm, 0), l.name));
                    } else {
                        out.push(format!("  {}  (shown as \"SD\", no thickness)", l.name));
                    }
                }
                out.push(format!("Total thickness of the layers = {} mm (chart {}, {} / {}).", num(s.total_thickness_mm(), 0), s.chart, s.subgrade.label(), s.traffic.label()));
            }
            None => out.push("No catalogue structure; see the warnings.".to_string()),
        }
        out.extend(self.warnings.iter().map(|w| format!("Note: {w}")));
        out.push("This is a calculation for an engineer to check, not a certified design. Part I asks that the catalogue structure be reviewed against local conditions, materials and other design methods (preamble, section 5.9).".to_string());
        out
    }

    fn warnings(&self) -> &[String] {
        &self.warnings
    }

    fn layers(&self) -> Option<Vec<Layer>> {
        self.structure.as_ref().map(CatalogueStructure::layers)
    }
}
