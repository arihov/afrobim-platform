//! Design traffic and flexible pavement requirements of the UK *Design Manual for Roads and
//! Bridges*: CD 224 "Traffic assessment" and CD 226 "Design for new pavement construction".
//!
//! # Revisions used, and a caution
//!
//! | Document | Revision read | Published | Status when read (November 2026) |
//! |---|---|---|---|
//! | CD 224 Traffic assessment | Revision 0 | March 2020 | Superseded by Version 0.1.0 (2025) |
//! | CD 226 Design for new pavement construction | Version 0.1.0 | 29 November 2021 | Superseded by Version 0.2.0 (2025) |
//!
//! Both were downloaded from `standardsforhighways.co.uk`. **The current revisions could not be
//! obtained** (the site serves its documents through a search application, and only the
//! attachment addresses of the earlier revisions could be found), so everything here is from
//! the revisions in the table and every calculation says so. Before this method is relied on,
//! the tables and equations below must be compared with the current documents.
//!
//! # What this covers
//!
//! | Step | Here | Source |
//! |---|---|---|
//! | Vehicle classes and categories | [`VehicleGroup`] | CD 224 Table 2.4 |
//! | Growth factor G for future traffic | [`growth_factor`] | CD 224 Table 2.12, clauses 2.13 and 2.17 |
//! | Flow after i years, `Fi = F·(1 + R)^i` | [`flow_after_years`] | CD 224 Equation 2.14, Table 2.12 Note 2 |
//! | Wear factor W, maintenance and new | [`wear_factor`] | CD 224 Table 2.18 |
//! | Percentage of commercial vehicles in the heaviest loaded lane, P | [`heaviest_lane_percent`] | CD 224 Table 2.21 |
//! | Design traffic `T = Σ(365·F·G·W·10⁻⁶)·Y·P` | [`design_traffic`] | CD 224 Equations 2.23a and 2.23b |
//! | Limits on foundation class and material by traffic | [`design`] | CD 226 clauses 2.4, 2.5, 2.7, 2.17, 2.19, 2.22 |
//! | Total asphalt over an HBGM base, `H = −16.05(log T)² + 101 log T + 45.8` | [`asphalt_over_hbgm_mm`] | CD 226 Equation 2.24 and its notes |
//!
//! # What it does not do, and why
//!
//! - **The total asphalt thickness of a flexible pavement with an asphalt base, and the
//!   thickness of an HBGM base.** In CD 226 Version 0.1.0 both exist only as curves on the
//!   nomograph of Figure 2.20, with no equation or table. They are not read off the figure here.
//!   The result therefore names no complete structure and [`DesignResult::layers`] is `None`.
//! - Foundation design (CD 225), surface course design (CD 236), rigid pavements, widening and
//!   maintenance design (CD 227), analytical design (CD 226 section 4), whole-life cost.
//!
//! # What stays an input
//!
//! The commercial vehicle flows by class or category (AADF, one direction, at opening), the
//! number of lanes, the design period, whether the scheme is new or maintenance, the
//! foundation class (designed to CD 225) and the pavement type and materials.
//!
//! # Readings of the documents that a user should know about
//!
//! - **Table 2.12** gives G only for design periods of 5, 10 ... 40 years. For any other period
//!   G must be entered; nothing is interpolated. "All commercial vehicles" has a G only at 40
//!   years (1.45), as clause 2.13 requires.
//! - **Table 2.4 to Table 2.12.** The growth rows are "OGV1 + PSV" and "OGV2"; a flow entered
//!   by class takes the row of its category in Table 2.4 (buses and coaches, 2 and 3-axle
//!   rigid: OGV1 + PSV; 4-axle rigid and all articulated: OGV2), which is what the worked
//!   example of Table 2.23.1N1 does. Table 2.4 lists 3-axle and 4-axle articulated separately;
//!   Table 2.18 has one wear factor for "3 and 4-axle articulated".
//! - **Table 2.21** is for "2 or more lanes in one direction". With one lane P is taken as 100
//!   per cent, which CD 224 does not state. In the "4 or more" lanes rows the two expressions
//!   do not meet at 10 500 cv/day (62.2 and 62.4 per cent); each is used in its printed range.
//! - **Equation 2.24 and Appendix A1.1.** At 60 msa the equation gives 174.6 mm, 175 mm when
//!   rounded up to 5 mm as clause 2.24 says. The worked example A1.1 reads 180 mm for 60 msa
//!   from the nomograph, whose scale is in 10 mm bands. This module returns the equation's
//!   175 mm and prints the difference.
//! - **CD 224 Table 2.23.1N1** prints 0.93 msa for the 6-axle articulated line of its worked
//!   example; its own factors give 0.95, which is also what its printed total of 2.87 needs.
//! - The equation's `log` is taken as the logarithm to base 10, which is the only reading that
//!   meets the stated limits (180 mm at 80 msa).

use super::{num, DesignError, DesignMethod, DesignResult};
use crate::model::Layer;

const REVISIONS: &str = "CD 224 Revision 0 (March 2020) and CD 226 Version 0.1.0 (November 2021)";

/// Whether the wear factors for new road schemes or for maintenance apply (CD 224 clauses 2.19
/// and 2.20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// New road construction including widening: WN.
    New,
    /// Maintenance including re-alignment: WM.
    Maintenance,
}

/// A commercial vehicle class, a category, or all commercial vehicles together (CD 224 Tables
/// 2.4 and 2.18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VehicleGroup {
    BusesAndCoaches,
    Rigid2Axle,
    Rigid3Axle,
    Rigid4Axle,
    Articulated3And4Axle,
    Articulated5Axle,
    Articulated6Axle,
    /// The category OGV1 + PSV, when flows are not split by class.
    Ogv1PlusPsv,
    /// The category OGV2.
    Ogv2,
    /// No information on classes or categories (clause 2.13).
    AllCommercialVehicles,
}

impl VehicleGroup {
    pub const ALL: [VehicleGroup; 10] = [
        VehicleGroup::BusesAndCoaches,
        VehicleGroup::Rigid2Axle,
        VehicleGroup::Rigid3Axle,
        VehicleGroup::Rigid4Axle,
        VehicleGroup::Articulated3And4Axle,
        VehicleGroup::Articulated5Axle,
        VehicleGroup::Articulated6Axle,
        VehicleGroup::Ogv1PlusPsv,
        VehicleGroup::Ogv2,
        VehicleGroup::AllCommercialVehicles,
    ];

    pub fn label(self) -> &'static str {
        match self {
            VehicleGroup::BusesAndCoaches => "Buses and coaches (PSV)",
            VehicleGroup::Rigid2Axle => "2-axle rigid",
            VehicleGroup::Rigid3Axle => "3-axle rigid",
            VehicleGroup::Rigid4Axle => "4-axle rigid",
            VehicleGroup::Articulated3And4Axle => "3 and 4-axle articulated",
            VehicleGroup::Articulated5Axle => "5-axle articulated",
            VehicleGroup::Articulated6Axle => "6-axle articulated",
            VehicleGroup::Ogv1PlusPsv => "OGV1 + PSV",
            VehicleGroup::Ogv2 => "OGV2",
            VehicleGroup::AllCommercialVehicles => "All commercial vehicles",
        }
    }

    /// The row of the growth table the group takes: 0 = OGV1 + PSV, 1 = OGV2, 2 = all commercial
    /// vehicles (CD 224 Table 2.4 for the classes).
    fn growth_row(self) -> usize {
        match self {
            VehicleGroup::BusesAndCoaches | VehicleGroup::Rigid2Axle | VehicleGroup::Rigid3Axle | VehicleGroup::Ogv1PlusPsv => 0,
            VehicleGroup::Rigid4Axle | VehicleGroup::Articulated3And4Axle | VehicleGroup::Articulated5Axle | VehicleGroup::Articulated6Axle | VehicleGroup::Ogv2 => 1,
            VehicleGroup::AllCommercialVehicles => 2,
        }
    }

    /// True for OGV2 and its classes.
    pub fn is_ogv2(self) -> bool {
        self.growth_row() == 1
    }
}

/// CD 224 Revision 0, Table 2.18 "Wear factors for commercial vehicle classes and categories":
/// (maintenance WM, new WN), in the order of [`VehicleGroup::ALL`].
const WEAR_FACTORS: [(f64, f64); 10] = [(2.6, 3.9), (0.4, 0.6), (2.3, 3.4), (3.0, 4.6), (1.7, 2.5), (2.9, 4.4), (3.7, 5.6), (1.3, 1.9), (3.2, 4.9), (2.7, 4.0)];

/// Wear factor W, standard axles per vehicle (CD 224 Revision 0, Table 2.18).
pub fn wear_factor(group: VehicleGroup, scheme: Scheme) -> f64 {
    let i = VehicleGroup::ALL.iter().position(|g| *g == group).expect("every group is listed");
    match scheme {
        Scheme::Maintenance => WEAR_FACTORS[i].0,
        Scheme::New => WEAR_FACTORS[i].1,
    }
}

/// CD 224 Revision 0, Table 2.12 "Growth factors (G) for future traffic": design periods of 5 to
/// 40 years in steps of 5; rows OGV1 + PSV and OGV2.
const GROWTH_FACTORS: [[f64; 8]; 2] = [[1.02, 1.05, 1.08, 1.11, 1.14, 1.17, 1.21, 1.24], [1.04, 1.10, 1.16, 1.23, 1.30, 1.37, 1.46, 1.54]];
/// The same table: all commercial vehicles, 40 years only (and clause 2.13).
const GROWTH_FACTOR_ALL_40_YEARS: f64 = 1.45;

/// Growth factor G for future traffic (CD 224 Revision 0, Table 2.12). `None` where the table
/// has no value: a design period that is not 5, 10 ... 40 years, or all commercial vehicles at
/// anything but 40 years.
pub fn growth_factor(group: VehicleGroup, design_period_years: u32) -> Option<f64> {
    if design_period_years == 0 || design_period_years % 5 != 0 || design_period_years > 40 {
        return None;
    }
    match group.growth_row() {
        2 => (design_period_years == 40).then_some(GROWTH_FACTOR_ALL_40_YEARS),
        row => Some(GROWTH_FACTORS[row][(design_period_years / 5 - 1) as usize]),
    }
}

/// Equivalent annual growth rate R, per cent (CD 224 Revision 0, Table 2.12 Note 2): 1.07 for
/// OGV1, 2.10 for OGV2, 1.54 for all commercial vehicles.
pub fn annual_growth_rate_percent(group: VehicleGroup) -> f64 {
    [1.07, 2.10, 1.54][group.growth_row()]
}

/// CD 224 Revision 0, Equation 2.14: the flow after `years`, `Fi = F × (1 + R)^i`, for uses such
/// as choosing a polished stone value. Not used in the design traffic.
pub fn flow_after_years(flow: f64, group: VehicleGroup, years: u32) -> f64 {
    flow * (1.0 + annual_growth_rate_percent(group) / 100.0).powi(years as i32)
}

/// Percentage of commercial vehicles in the heaviest loaded lane, P (CD 224 Revision 0, Table
/// 2.21), from the number of lanes in one direction and the total flow F in cv/day. One lane
/// gives 100 (see the module documentation).
pub fn heaviest_lane_percent(lanes_one_direction: u32, flow_cv_per_day: f64) -> f64 {
    let f = flow_cv_per_day;
    match lanes_one_direction {
        0 | 1 => 100.0,
        2 | 3 => {
            if f <= 5000.0 {
                100.0 - 0.0036 * f
            } else if f <= 25_000.0 {
                89.0 - 0.0014 * f
            } else {
                54.0
            }
        }
        _ => {
            if f <= 10_500.0 {
                100.0 - 0.0036 * f
            } else if f <= 25_000.0 {
                75.0 - 0.0012 * f
            } else {
                45.0
            }
        }
    }
}

/// One line of the traffic table.
#[derive(Debug, Clone, PartialEq)]
pub struct Flow {
    pub group: VehicleGroup,
    /// F: annual average daily flow in one direction at scheme opening (or the current flow of
    /// an existing road), cv/day.
    pub aadf: f64,
    /// G entered by the engineer, for a design period Table 2.12 does not list. `None` takes
    /// the table.
    pub growth_factor: Option<f64>,
}

impl Flow {
    pub fn new(group: VehicleGroup, aadf: f64) -> Flow {
        Flow { group, aadf, growth_factor: None }
    }
}

/// Traffic for CD 224.
#[derive(Debug, Clone, PartialEq)]
pub struct TrafficInput {
    pub flows: Vec<Flow>,
    pub scheme: Scheme,
    /// Y, years. CD 226 clause 2.17 sets 40 for a new carriageway.
    pub design_period_years: u32,
    pub lanes_one_direction: u32,
    /// True when past traffic is being calculated, for which G = 1.0 (clause 2.17).
    pub past_traffic: bool,
}

/// One line worked through.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowTraffic {
    pub group: VehicleGroup,
    pub aadf: f64,
    pub growth_factor: f64,
    pub wear_factor: f64,
    /// Tc, msa per year (Equation 2.23a).
    pub weighted_annual_msa: f64,
}

/// The design traffic and how it was reached.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignTraffic {
    pub input: TrafficInput,
    pub flows: Vec<FlowTraffic>,
    /// Total daily flow, cv/day.
    pub total_flow: f64,
    /// ΣTc, msa per year.
    pub total_weighted_annual_msa: f64,
    /// P, per cent.
    pub heaviest_lane_percent: f64,
    /// T, msa (Equation 2.23b).
    pub design_traffic_msa: f64,
    pub warnings: Vec<String>,
}

/// CD 224 Revision 0, Equations 2.23a and 2.23b.
pub fn design_traffic(input: &TrafficInput) -> Result<DesignTraffic, DesignError> {
    let bad = |m: String| Err(DesignError::InvalidInput(m));
    if input.flows.is_empty() {
        return bad("There are no commercial vehicle flows.".to_string());
    }
    if input.design_period_years == 0 {
        return bad("The design period must be at least one year.".to_string());
    }
    if input.lanes_one_direction == 0 {
        return bad("There must be at least one lane.".to_string());
    }
    let all = input.flows.iter().any(|f| f.group == VehicleGroup::AllCommercialVehicles);
    if all && input.flows.len() > 1 {
        return bad("\"All commercial vehicles\" cannot be combined with flows by class or category.".to_string());
    }
    let mut warnings = vec![format!(
        "Calculated to {REVISIONS}. Both have since been superseded (CD 224 Version 0.1.0 and CD 226 Version 0.2.0, 2025), and those could not be obtained: compare the factors and equations with the current documents before relying on this."
    )];
    let mut flows = Vec::new();
    for f in &input.flows {
        if !(f.aadf.is_finite() && f.aadf >= 0.0) {
            return bad(format!("The flow of {} cannot be {}.", f.group.label(), f.aadf));
        }
        let growth = match (f.growth_factor, input.past_traffic) {
            (Some(g), _) if !(g.is_finite() && g > 0.0) => return bad(format!("The growth factor of {} cannot be {g}.", f.group.label())),
            (Some(g), _) => {
                warnings.push(format!("The growth factor of {} is the entered value {}, not a value of CD 224 Table 2.12.", f.group.label(), num(g, 3)));
                g
            }
            (None, true) => 1.0, // clause 2.17
            (None, false) => match growth_factor(f.group, input.design_period_years) {
                Some(g) => g,
                None => {
                    return bad(format!(
                        "CD 224 Table 2.12 has no growth factor for {} at a design period of {} years; enter one.",
                        f.group.label(),
                        input.design_period_years
                    ))
                }
            },
        };
        let wear = wear_factor(f.group, input.scheme);
        flows.push(FlowTraffic { group: f.group, aadf: f.aadf, growth_factor: growth, wear_factor: wear, weighted_annual_msa: 365.0 * f.aadf * growth * wear * 1.0e-6 });
    }
    let total_flow: f64 = flows.iter().map(|f| f.aadf).sum();
    let total_weighted: f64 = flows.iter().map(|f| f.weighted_annual_msa).sum();
    let p = heaviest_lane_percent(input.lanes_one_direction, total_flow);
    if input.lanes_one_direction == 1 {
        warnings.push("With one lane in one direction P is taken as 100 per cent; CD 224 Table 2.21 covers 2 or more lanes only.".to_string());
    }
    // Clause 2.7: for new road schemes the percentage of OGV2 shall not be less than 70.
    if input.scheme == Scheme::New && !all && total_flow > 0.0 {
        let ogv2 = flows.iter().filter(|f| f.group.is_ogv2()).map(|f| f.aadf).sum::<f64>() / total_flow * 100.0;
        if ogv2 < 70.0 {
            warnings.push(format!("OGV2 is {} per cent of the commercial vehicles. For new road schemes it shall not be less than 70 per cent (CD 224 clause 2.7); the flows are used as entered.", num(ogv2, 1)));
        }
    }
    Ok(DesignTraffic {
        input: input.clone(),
        flows,
        total_flow,
        total_weighted_annual_msa: total_weighted,
        heaviest_lane_percent: p,
        design_traffic_msa: total_weighted * f64::from(input.design_period_years) * p / 100.0,
        warnings,
    })
}

impl DesignTraffic {
    fn lines(&self) -> Vec<String> {
        let i = &self.input;
        let w = if i.scheme == Scheme::New { "WN, new road schemes" } else { "WM, maintenance" };
        let mut out = vec![format!(
            "Design traffic to CD 224 Revision 0 (March 2020): Tc = 365 × F × G × W × 10⁻⁶ msa (Equation 2.23a); T = ΣTc × Y × P (Equation 2.23b). Wear factors {w} (Table 2.18); growth factors {} (Table 2.12).",
            if i.past_traffic { "1.0 for past traffic, clause 2.17".to_string() } else { format!("for {} years", i.design_period_years) }
        )];
        for f in &self.flows {
            out.push(format!(
                "{}: F = {} cv/day, G = {}, W = {}, Tc = {} msa.",
                f.group.label(),
                num(f.aadf, 1),
                num(f.growth_factor, 3),
                num(f.wear_factor, 1),
                num(f.weighted_annual_msa, 3)
            ));
        }
        out.push(format!("Total daily flow = {} cv/day; total weighted annual traffic ΣTc = {} msa.", num(self.total_flow, 1), num(self.total_weighted_annual_msa, 3)));
        out.push(format!(
            "Heaviest loaded lane, {} lane(s) in one direction: P = {}% (Table 2.21).",
            i.lanes_one_direction,
            num(self.heaviest_lane_percent, 1)
        ));
        out.push(format!(
            "Design traffic T = {} × {} × {}% = {} msa.",
            num(self.total_weighted_annual_msa, 3),
            i.design_period_years,
            num(self.heaviest_lane_percent, 1),
            num(self.design_traffic_msa, 1)
        ));
        out
    }
}

// ---------------------------------------------------------------------------------------------
// CD 226
// ---------------------------------------------------------------------------------------------

/// CD 226 Version 0.1.0, Equation 2.24 with its notes: total thickness of asphalt (surface
/// course, binder course and base where present) of a flexible pavement with an HBGM base, mm.
///
/// `H = −16.05 (log T)² + 101 log T + 45.8`, T in msa; not less than 100 mm nor more than
/// 180 mm (Note 2); 180 mm from 80 msa (Note 3); rounded up to the nearest 5 mm (clause 2.24).
/// `None` above the maximum design traffic of 400 msa (Note 1) or for a traffic that is not
/// positive.
pub fn asphalt_over_hbgm_mm(design_traffic_msa: f64) -> Option<f64> {
    let t = design_traffic_msa;
    if !(t > 0.0) || t > 400.0 {
        return None;
    }
    let h = if t >= 80.0 { 180.0 } else { equation_2_24(t).clamp(100.0, 180.0) };
    // Rounded up to 5 mm; a hair's breadth above a multiple of 5 is rounding error, not thickness.
    Some(((h - 1e-9) / 5.0).ceil() * 5.0)
}

/// CD 226 Version 0.1.0, Equation 2.24 itself, without its limits or rounding.
pub fn equation_2_24(design_traffic_msa: f64) -> f64 {
    let l = design_traffic_msa.log10();
    -16.05 * l * l + 101.0 * l + 45.8
}

/// The asphalt of the base and binder course (CD 226 Tables 2.10 and 2.13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsphaltMaterial {
    /// Dense and heavy-duty macadam, 40/60 binder, designed to Clause 929.
    Ac4060,
    /// EME2 to Clause 930.
    Eme2,
}

/// HBGM category of Figure 2.20 (examples of each are in CD 226 Table 2.23N2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HbgmCategory {
    A,
    B,
    C,
    D,
}

/// The flexible pavement types of CD 226 clause 2.9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexiblePavement {
    AsphaltBase { material: AsphaltMaterial },
    HbgmBase { category: HbgmCategory },
}

/// Everything the method needs.
#[derive(Debug, Clone, PartialEq)]
pub struct DmrbInput {
    pub traffic: TrafficInput,
    /// Foundation stiffness class 1 to 4, as designed to CD 225.
    pub foundation_class: u8,
    pub pavement: FlexiblePavement,
}

/// The result.
#[derive(Debug, Clone, PartialEq)]
pub struct DmrbDesign {
    pub input: DmrbInput,
    pub traffic: DesignTraffic,
    /// The traffic the pavement is designed for, msa: T, or 1 msa where T is less on a new road
    /// (CD 226 clause 2.19).
    pub pavement_traffic_msa: f64,
    /// Total asphalt thickness over an HBGM base, mm (Equation 2.24). `None` for an asphalt
    /// base, or above 400 msa.
    pub asphalt_over_hbgm_mm: Option<f64>,
    pub warnings: Vec<String>,
}

/// Design traffic to CD 224 and the flexible pavement requirements of CD 226 that are stated in
/// words, a table or an equation. See the module documentation for what is left out.
pub fn design(input: &DmrbInput) -> Result<DmrbDesign, DesignError> {
    if !(1..=4).contains(&input.foundation_class) {
        return Err(DesignError::InvalidInput(format!("The foundation class must be 1 to 4 (it is {}).", input.foundation_class)));
    }
    let traffic = design_traffic(&input.traffic)?;
    let mut warnings = traffic.warnings.clone();
    let new = input.traffic.scheme == Scheme::New;
    let t = traffic.design_traffic_msa;
    let pavement_traffic = if new { t.max(1.0) } else { t };
    if new && t < 1.0 {
        warnings.push("The design traffic is less than 1 msa; the minimum design traffic for new roads is 1 msa (CD 226 clause 2.19), which is used for the pavement.".to_string());
    }
    if new && input.traffic.design_period_years != 40 {
        warnings.push(format!("The design period is {} years. For a new carriageway the design life shall be 40 years (CD 226 clause 2.17).", input.traffic.design_period_years));
    }
    if new && input.foundation_class == 1 && pavement_traffic > 20.0 {
        warnings.push("Foundation class 1 shall only be used for design traffic of 20 msa or less on a new carriageway (CD 226 clause 2.4).".to_string());
    }
    if new && input.foundation_class == 2 && pavement_traffic > 80.0 {
        warnings.push("Foundation class 2 shall only be used for design traffic of 80 msa or less on a new carriageway (CD 226 clause 2.5).".to_string());
    }
    if new && input.pavement == (FlexiblePavement::AsphaltBase { material: AsphaltMaterial::Eme2 }) && input.foundation_class < 3 {
        warnings.push("An asphalt base using EME2 on a new carriageway shall use a class 3 or class 4 foundation, unless a class 2 foundation can be demonstrated to achieve a minimum stiffness of 100 MPa (CD 226 clause 2.7).".to_string());
    }
    if pavement_traffic > 80.0 {
        warnings.push("Where traffic exceeds 80 msa, the coarse aggregate in all the asphalt materials shall contain only crushed rock or slag (CD 226 clause 2.22).".to_string());
    }
    let mut asphalt = None;
    match input.pavement {
        FlexiblePavement::AsphaltBase { .. } => warnings.push(
            "The total asphalt thickness of a flexible pavement with an asphalt base is given by CD 226 Version 0.1.0 only as curves on the nomograph of Figure 2.20. It is not computed here: read it from the figure for this design traffic, foundation class and material.".to_string(),
        ),
        FlexiblePavement::HbgmBase { .. } => {
            asphalt = asphalt_over_hbgm_mm(pavement_traffic);
            if asphalt.is_none() {
                warnings.push("The design traffic is above 400 msa, the maximum of CD 226 Equation 2.24 (Note 1).".to_string());
            }
            warnings.push(
                "The thickness of the HBGM base is given by CD 226 Version 0.1.0 only as curves on the nomograph of Figure 2.20 (minimum 150 mm, clause 2.23). It is not computed here: read it from the figure for this design traffic, foundation class and HBGM category.".to_string(),
            );
            warnings.push(
                "Equation 2.24 and the nomograph differ by the nomograph's 10 mm bands: at 60 msa the equation gives 175 mm where worked example A1.1 reads 180 mm from Figure 2.20.".to_string(),
            );
        }
    }
    Ok(DmrbDesign { input: input.clone(), traffic, pavement_traffic_msa: pavement_traffic, asphalt_over_hbgm_mm: asphalt, warnings })
}

impl DesignResult for DmrbDesign {
    fn method(&self) -> DesignMethod {
        DesignMethod::DmrbCd226
    }

    fn calculation(&self) -> Vec<String> {
        let mut out = vec![format!("Design basis: {}.", DesignMethod::DmrbCd226.reference())];
        out.extend(self.traffic.lines());
        out.push(format!("Foundation class {} (entered; designed to CD 225).", self.input.foundation_class));
        match self.input.pavement {
            FlexiblePavement::AsphaltBase { material } => out.push(format!(
                "Flexible pavement with an asphalt base, {} (CD 226 Table 2.10). Total asphalt thickness: not computed (Figure 2.20 only).",
                if material == AsphaltMaterial::Ac4060 { "AC 40/60" } else { "EME2" }
            )),
            FlexiblePavement::HbgmBase { category } => {
                out.push(format!("Flexible pavement with an HBGM base, category {category:?} (CD 226 clause 2.11, Table 2.23N2)."));
                let t = self.pavement_traffic_msa;
                match self.asphalt_over_hbgm_mm {
                    Some(h) if t >= 80.0 => out.push(format!("Total thickness of asphalt = {} mm: design traffic of 80 msa or more (CD 226 Equation 2.24, Note 3).", num(h, 0))),
                    Some(h) => out.push(format!(
                        "Total thickness of asphalt H = −16.05 (log T)² + 101 log T + 45.8 = {} mm for T = {} msa, limited to 100 to 180 mm and rounded up to 5 mm: {} mm (CD 226 Equation 2.24).",
                        num(equation_2_24(t), 1),
                        num(t, 1),
                        num(h, 0)
                    )),
                    None => out.push("Total thickness of asphalt: none; the traffic is outside Equation 2.24.".to_string()),
                }
                out.push("Thickness of the HBGM base: not computed (Figure 2.20 only).".to_string());
            }
        }
        out.extend(self.warnings.iter().map(|w| format!("Note: {w}")));
        out.push("This is a calculation for an engineer to check, not a certified design, and it is not a complete pavement design to CD 226.".to_string());
        out
    }

    fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Always `None`: CD 226 Version 0.1.0 gives part of every flexible structure only on a
    /// nomograph, so no complete stack is offered.
    fn layers(&self) -> Option<Vec<Layer>> {
        None
    }
}
