//! Requirement checks on a gantry's geometry against the UK DMRB.
//!
//! **Passing these checks does not mean the gantry is structurally adequate.** A gantry is a
//! highway structure: it needs Eurocode analysis, fatigue assessment and technical approval
//! (CD 365 clauses 3.1, 6.1, 6.10), none of which is done here. Nothing in this module sizes a
//! member or computes a wind load, a deflection or a foundation.
//!
//! # Documents held (downloaded 14 November 2026 from standardsforhighways.co.uk)
//!
//! * **CD 127 Version 1.0.1** "Cross-sections and headrooms", published 29 July 2021. Served
//!   without a superseded mark. Headroom values come from its Tables 4.1 and 4.3.
//! * **CD 365 Revision 1** "Portal and cantilever signs/signals gantries", published March 2020.
//!   **The copy served is watermarked SUPERSEDED.** Its successor (Version 1.1.0, listed by
//!   third-party indexes as September 2025) could not be obtained, so every CD 365 clause number
//!   and requirement below is from Revision 1 and must be confirmed against the current version
//!   before it is relied on. The calculation report says so on its first line.
//! * **CD 377 Revision 4** "Requirements for road restraint systems", published January 2021,
//!   also watermarked SUPERSEDED (successor Version 5.0.x not obtained). **No value is taken
//!   from CD 377.** CD 365 clause 4.1 sends the designer to it for vehicle restraint; that is
//!   reported as a requirement for the engineer, with no set-back or working-width number.
//!
//! # What is checked
//!
//! | Check | Source |
//! |---|---|
//! | Headroom to the lowest point of boom, signs and entered equipment, after the entered SLS deflection, with sag compensation | CD 127 v1.0.1 cl. 4.1, 4.2, 4.3, 4.9, 4.11, Tables 4.1, 4.3; CD 365 r1 cl. 3.19 |
//! | Structure free zone: distance from the edge of the paved width to each leg (reported) | CD 127 v1.0.1 cl. 4.5, 4.6 |
//! | Dual carriageway: a non-passively-safe portal spans both carriageways with no leg in the central reserve | CD 365 r1 cl. 3.21 |
//! | Portal: levels at the ends of the beam are the same | CD 365 r1 cl. 3.22 |
//! | Distance from an over-bridge at least twice the gantry's maximum height | CD 365 r1 cl. 3.23 |
//! | Listed requirements for the engineer (technical approval, type selection, vehicle restraint, maintenance access, foundations, working life) | CD 365 r1 cl. 2.1, 3.1, 3.2, 3.16, 3.18, 3.20, 4.1, 6.1, 6.23, 7.28.1, 9.1 |
//!
//! CD 365 Revision 1 states no limit on span or outreach, and neither it nor CD 127 states a
//! minimum set-back of a leg from the carriageway as a number, so none is checked. See the
//! README for the ambiguities met and how each was read.

use afrobim_alignment::Alignment;
use serde::{Deserialize, Serialize};

use crate::geometry::SettingOut;
use crate::{fixed, GantryDefinition, GantryError, GantryType};

/// The CD 127 document the headroom values were read from.
pub const CD127_REVISION: &str = "CD 127 Version 1.0.1 (published 29 July 2021)";
/// The CD 365 document the gantry clauses were read from, with its status when downloaded.
pub const CD365_REVISION: &str = "CD 365 Revision 1 (published March 2020; marked SUPERSEDED when downloaded on 14 November 2026)";

/// CD 127 Version 1.0.1, Table 4.1 "Minimum standard headroom at structures", row "Footbridges,
/// sign and signal gantries, and other structures vulnerable to vehicular impact", column "New
/// construction headroom": 5.70 m (+ S).
pub const CD127_T41_GANTRY_NEW_CONSTRUCTION_MM: f64 = 5700.0;
/// CD 127 Version 1.0.1, Table 4.1, the same row, column "Maintained headroom": 5.41 m (+ S).
pub const CD127_T41_GANTRY_MAINTAINED_MM: f64 = 5410.0;
/// CD 127 Version 1.0.1, Table 4.1, row "All permanent structures over high load routes (not
/// applicable in NI)", column "New construction headroom": 6.45 m (+ S).
pub const CD127_T41_HIGH_LOAD_NEW_CONSTRUCTION_MM: f64 = 6450.0;
/// CD 127 Version 1.0.1, Table 4.1, the same row, column "Maintained headroom": 6.18 m (+ S).
pub const CD127_T41_HIGH_LOAD_MAINTAINED_MM: f64 = 6180.0;

/// CD 127 Version 1.0.1, Table 4.3 "Sag curve compensation": (sag radius in metres, additional
/// clearance S in millimetres) for the six tabulated radii. The table's first row is "≤ 1000:
/// 80" and its last "> 6000: 0".
pub const CD127_T43_SAG_COMPENSATION: [(f64, f64); 6] = [(1000.0, 80.0), (1200.0, 70.0), (1500.0, 55.0), (2000.0, 45.0), (3000.0, 25.0), (6000.0, 15.0)];

/// CD 365 Revision 1 clause 3.23: "Gantries shall not be located less than two times their
/// maximum height clear of any over-bridge unless the interaction between the two structures
/// with respect to wind turbulence is included in the design."
pub const CD365_3_23_OVERBRIDGE_HEIGHT_FACTOR: f64 = 2.0;

/// Which column of CD 127 Table 4.1 applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadroomBasis {
    /// A new structure (CD 127 clause 4.9).
    #[default]
    NewConstruction,
    /// An existing structure (CD 127 clause 4.11).
    Maintained,
}

/// What the engineer enters for the checks. Stored in the gantry's definition. Nothing here is
/// computed by this crate: deflection in particular comes from the structural analysis.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CheckInputs {
    pub headroom_basis: HeadroomBasis,
    /// The road is a high load route (CD 127 Table 4.1, last row; not applicable in NI).
    pub high_load_route: bool,
    /// Radius of the sag curve the road is on under the gantry, m, "measured along the
    /// carriageway over a 25 metres chord" (CD 127 clause 4.4). `None`: not on a sag curve.
    pub sag_radius_m: Option<f64>,
    /// Maximum downward deflection of the superstructure at the serviceability limit state, mm,
    /// from the structural analysis (CD 127 clause 4.2). `None`: not yet known; the headroom
    /// check is then reported as incomplete.
    pub sls_deflection_mm: Option<f64>,
    /// Height of the highest point of the surface under the gantry (paved width, verges, central
    /// reserve) above the reference level, mm. The model's road line is level at the centreline
    /// profile level; enter the rise due to cross-fall or a raised verge here.
    pub surface_rise_mm: f64,
    /// Cross-fall of the carriageway under the lowest point, percent (either sign). Used only to
    /// turn the vertical clearance into one square to the surface.
    pub cross_fall_pct: f64,
    /// Depth below the boom's underside of any equipment other than the carried signs, mm.
    pub equipment_below_boom_mm: f64,
    /// The gantry is a passively safe gantry (CD 365 Section 8).
    pub passively_safe: bool,
    /// A leg in the central reserve of a dual carriageway has been agreed with the Technical
    /// Approval Authority (CD 365 clause 3.21).
    pub central_reserve_support_agreed_with_taa: bool,
    /// Clear distance to the nearest over-bridge, m. `None`: no over-bridge nearby, or not entered.
    pub distance_to_overbridge_m: Option<f64>,
    /// The wind interaction between the gantry and the over-bridge is included in the structural
    /// design (CD 365 clause 3.23).
    pub wind_interaction_in_design: bool,
}

/// The outcome of one line of the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The stated requirement is met by the entered geometry.
    Ok,
    Fails,
    /// The geometry entered so far does not fail, but an input the requirement depends on is
    /// missing, so compliance is not shown.
    Incomplete,
    /// A requirement of the standard that this crate cannot check: for the engineer.
    Requirement,
    /// A value reported for the engineer's use, with no pass or fail.
    Reported,
    NotApplicable,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::Fails => "FAILS",
            Status::Incomplete => "INCOMPLETE",
            Status::Requirement => "REQUIREMENT",
            Status::Reported => "REPORTED",
            Status::NotApplicable => "NOT APPLICABLE",
        }
    }
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckLine {
    /// Document and clause, e.g. "CD 127 v1.0.1 cl. 4.1, Table 4.1".
    pub citation: String,
    pub status: Status,
    pub text: String,
}

/// The headroom check, with every figure it used. Levels are mm above the reference level.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadroomCheck {
    /// Table 4.1 value before sag compensation, mm.
    pub table_value_mm: f64,
    /// Sag compensation S from Table 4.3, mm.
    pub sag_compensation_mm: f64,
    /// Table value + S, mm.
    pub required_mm: f64,
    pub lowest_point_mm: f64,
    pub surface_rise_mm: f64,
    /// The entered SLS deflection, 0 when not entered.
    pub deflection_mm: f64,
    pub deflection_entered: bool,
    /// Lowest point − surface rise − deflection, mm.
    pub vertical_clearance_mm: f64,
    /// Longitudinal grade used (m/m): the profile's at the station, 0 without a profile.
    pub grade: f64,
    /// Cross-fall used (m/m).
    pub cross_fall: f64,
    /// Vertical clearance ÷ √(1 + grade² + cross-fall²): the clearance square to a plane
    /// surface, as CD 127 defines headroom.
    pub headroom_mm: f64,
    pub status: Status,
}

/// The requirement checks of one gantry.
#[derive(Debug, Clone, PartialEq)]
pub struct GantryChecks {
    pub mark: String,
    pub headroom: HeadroomCheck,
    pub lines: Vec<CheckLine>,
}

/// CD 127 Version 1.0.1 Table 4.3: the additional clearance S (mm) for a sag radius (m).
///
/// The table gives S at six radii, "≤ 1000" and "> 6000", and does not say what to take between
/// tabulated radii. This function does **not** interpolate: a radius between two rows takes the
/// S of the smaller tabulated radius (the larger S), which is never less than an interpolated
/// value. `None` (not on a sag curve) gives 0.
pub fn sag_compensation_mm(sag_radius_m: Option<f64>) -> f64 {
    let Some(r) = sag_radius_m else { return 0.0 };
    let table = CD127_T43_SAG_COMPENSATION;
    if r > table[table.len() - 1].0 {
        return 0.0;
    }
    // The last row whose radius does not exceed r; below the first row, the first ("≤ 1000").
    table.iter().rev().find(|&&(radius, _)| radius <= r).map_or(table[0].1, |&(_, s)| s)
}

/// CD 127 Version 1.0.1 Table 4.1: the minimum standard headroom for a sign gantry before sag
/// compensation, mm.
pub fn table_headroom_mm(basis: HeadroomBasis, high_load_route: bool) -> f64 {
    match (basis, high_load_route) {
        (HeadroomBasis::NewConstruction, false) => CD127_T41_GANTRY_NEW_CONSTRUCTION_MM,
        (HeadroomBasis::Maintained, false) => CD127_T41_GANTRY_MAINTAINED_MM,
        (HeadroomBasis::NewConstruction, true) => CD127_T41_HIGH_LOAD_NEW_CONSTRUCTION_MM,
        (HeadroomBasis::Maintained, true) => CD127_T41_HIGH_LOAD_MAINTAINED_MM,
    }
}

/// Tolerance on comparisons, mm: far below any tolerance of construction, it only absorbs
/// floating-point rounding so that a headroom entered exactly at the limit passes.
const EPS_MM: f64 = 1e-6;

impl GantryChecks {
    /// Runs the checks. `alignment` supplies the longitudinal grade at the station (for the
    /// headroom square to the surface); without it, or without a profile, the grade is 0.
    pub fn run(mark: &str, def: &GantryDefinition, alignment: Option<&Alignment>) -> Result<GantryChecks, GantryError> {
        def.check()?;
        let setting: Option<SettingOut> = match alignment {
            Some(a) => Some(def.setting_out(a)?),
            None => None,
        };
        let grade = setting.as_ref().and_then(|s| s.grade);
        let c = &def.checks;
        let mut lines = Vec::new();
        let mut push = |citation: &str, status: Status, text: String| lines.push(CheckLine { citation: citation.to_string(), status, text });

        // ---- Headroom ----
        let table_value = table_headroom_mm(c.headroom_basis, c.high_load_route);
        let sag = sag_compensation_mm(c.sag_radius_m);
        let required = table_value + sag;
        let lowest = def.lowest_point_mm();
        let deflection = c.sls_deflection_mm.unwrap_or(0.0).max(0.0);
        let vertical = lowest - c.surface_rise_mm - deflection;
        let (g, cf) = (grade.unwrap_or(0.0), c.cross_fall_pct / 100.0);
        let headroom = vertical / (1.0 + g * g + cf * cf).sqrt();
        let headroom_status = if headroom < required - EPS_MM {
            Status::Fails
        } else if c.sls_deflection_mm.is_none() {
            Status::Incomplete
        } else {
            Status::Ok
        };
        let basis = match c.headroom_basis {
            HeadroomBasis::NewConstruction => "new construction headroom",
            HeadroomBasis::Maintained => "maintained headroom",
        };
        let row = if c.high_load_route { "all permanent structures over high load routes" } else { "sign and signal gantries" };
        push(
            "CD 127 v1.0.1 cl. 4.1, 4.3, Tables 4.1, 4.3",
            Status::Reported,
            format!(
                "Minimum standard headroom ({basis}, {row}) = {} + S = {} + {} = {} mm{}.",
                fixed(table_value, 0, 1),
                fixed(table_value, 0, 1),
                fixed(sag, 0, 1),
                fixed(required, 0, 1),
                match c.sag_radius_m {
                    Some(r) => format!(" (sag radius {} m entered; Table 4.3 is not interpolated, the next smaller tabulated radius is used)", fixed(r, 0, 1)),
                    None => " (no sag curve entered: S = 0)".to_string(),
                }
            ),
        );
        push(
            "CD 127 v1.0.1 cl. 4.2, Terms and definitions \"Headroom\"; CD 365 r1 cl. 3.19",
            Status::Reported,
            format!(
                "Lowest point of boom, signs and entered equipment = {} mm above the reference level; highest surface under the gantry = {} mm; SLS deflection = {}; vertical clearance = {} mm.",
                fixed(lowest, 0, 1),
                fixed(c.surface_rise_mm, 0, 1),
                match c.sls_deflection_mm {
                    Some(d) => format!("{} mm (entered)", fixed(d, 0, 1)),
                    None => "NOT ENTERED (taken as 0)".to_string(),
                },
                fixed(vertical, 0, 1)
            ),
        );
        push(
            "CD 127 v1.0.1 Terms and definitions \"Headroom\"",
            Status::Reported,
            format!(
                "Headroom square to the surface = {} / sqrt(1 + {}^2 + {}^2) = {} mm (longitudinal grade {}, cross-fall as entered).",
                fixed(vertical, 0, 1),
                fixed(g, 0, 4),
                fixed(cf, 0, 4),
                fixed(headroom, 0, 1),
                if grade.is_some() { "from the profile" } else { "0: no profile supplied" }
            ),
        );
        push(
            "CD 127 v1.0.1 cl. 4.1, 4.2, 4.9, 4.11",
            headroom_status,
            format!(
                "Headroom {} mm against {} mm: {}.{}",
                fixed(headroom, 0, 1),
                fixed(required, 0, 1),
                headroom_status.label(),
                if headroom_status == Status::Incomplete {
                    " The SLS deflection of the superstructure has not been entered, so compliance after deflection is not shown."
                } else {
                    ""
                }
            ),
        );
        let headroom_check = HeadroomCheck {
            table_value_mm: table_value,
            sag_compensation_mm: sag,
            required_mm: required,
            lowest_point_mm: lowest,
            surface_rise_mm: c.surface_rise_mm,
            deflection_mm: deflection,
            deflection_entered: c.sls_deflection_mm.is_some(),
            vertical_clearance_mm: vertical,
            grade: g,
            cross_fall: cf,
            headroom_mm: headroom,
            status: headroom_status,
        };

        // ---- Structure free zone: distance from the paved width to each leg ----
        if def.carriageways.is_empty() {
            push("CD 127 v1.0.1 cl. 4.5, 4.6", Status::Incomplete, "No carriageway is entered, so the distance from the paved width to the legs cannot be reported.".into());
        }
        for (i, _) in def.legs.iter().enumerate() {
            let (a, b) = def.leg_faces_offset_m(i);
            // The nearest paved edge to this leg, and the leg's faces measured from it.
            let nearest = def
                .carriageways
                .iter()
                .flat_map(|cw| {
                    let (l, r) = cw.paved_edges_m();
                    [l, r]
                })
                .min_by(|x, y| (x - (a + b) / 2.0).abs().total_cmp(&(y - (a + b) / 2.0).abs()));
            if let Some(edge) = nearest {
                let (near, far) = ((a - edge).abs().min((b - edge).abs()), (a - edge).abs().max((b - edge).abs()));
                push(
                    "CD 127 v1.0.1 cl. 4.5, 4.6",
                    Status::Reported,
                    format!(
                        "Leg {}: near face {} m and far face {} m from the edge of the paved width (offset {} m). Clause 4.6(2) takes the structure free zone to the outermost face of the support, or the full verge or central reserve width if less; the headroom above applies over it.",
                        i + 1,
                        fixed(near, 3, 3),
                        fixed(far, 3, 3),
                        fixed(edge, 3, 3)
                    ),
                );
            }
        }

        // ---- CD 365 3.21: central reserve ----
        if def.gantry_type == GantryType::Portal && def.carriageways.len() >= 2 {
            let first = def.carriageways.first().expect("two carriageways").paved_edges_m();
            let last = def.carriageways.last().expect("two carriageways").paved_edges_m();
            let between = def.legs.iter().filter(|l| l.offset_m > first.1 && l.offset_m < last.0).count();
            let (status, text) = if c.passively_safe {
                (Status::NotApplicable, "The gantry is entered as passively safe; clause 3.21 applies to non-passively safe portal gantries.".to_string())
            } else if between == 0 {
                (Status::Ok, "The portal spans the carriageways entered with no leg between them.".to_string())
            } else if c.central_reserve_support_agreed_with_taa {
                (Status::Ok, format!("{between} leg(s) stand between the carriageways; entered as agreed with the TAA."))
            } else {
                (Status::Fails, format!("{between} leg(s) stand between the carriageways: a non-passively safe portal shall span both carriageways without support in the central reserve unless agreed with the TAA."))
            };
            push("CD 365 r1 cl. 3.21", status, text);
        } else {
            push(
                "CD 365 r1 cl. 3.21",
                Status::NotApplicable,
                "Applies to a portal on a dual carriageway (two carriageways entered).".into(),
            );
        }

        // ---- CD 365 3.22 ----
        match def.gantry_type {
            GantryType::Portal => push("CD 365 r1 cl. 3.22", Status::Ok, "The levels at the ends of the beam are the same: the model has one boom level.".into()),
            GantryType::Cantilever => push("CD 365 r1 cl. 3.22", Status::NotApplicable, "Applies to portal type gantries.".into()),
        }

        // ---- CD 365 3.23: over-bridge ----
        let lowest_base = def.legs.iter().map(|l| l.base_above_reference_mm).fold(f64::INFINITY, f64::min);
        let max_height_m = (def.highest_point_mm() - lowest_base) / 1000.0;
        let clear = CD365_3_23_OVERBRIDGE_HEIGHT_FACTOR * max_height_m;
        match c.distance_to_overbridge_m {
            None => push(
                "CD 365 r1 cl. 3.23",
                Status::Requirement,
                format!("No over-bridge distance entered. A gantry shall not be less than 2 x its maximum height = 2 x {} = {} m clear of any over-bridge unless the wind interaction is included in the design.", fixed(max_height_m, 3, 3), fixed(clear, 3, 3)),
            ),
            Some(d) => {
                let status = if d >= clear - EPS_MM / 1000.0 || c.wind_interaction_in_design { Status::Ok } else { Status::Fails };
                push(
                    "CD 365 r1 cl. 3.23",
                    status,
                    format!(
                        "Distance to the over-bridge {} m against 2 x maximum height = 2 x {} = {} m{}: {}.",
                        fixed(d, 3, 3),
                        fixed(max_height_m, 3, 3),
                        fixed(clear, 3, 3),
                        if c.wind_interaction_in_design { " (wind interaction entered as included in the design)" } else { "" },
                        status.label()
                    ),
                );
            }
        }

        // ---- Requirements this crate cannot check ----
        for (citation, text) in LISTED_REQUIREMENTS {
            push(citation, Status::Requirement, text.to_string());
        }

        Ok(GantryChecks { mark: mark.to_string(), headroom: headroom_check, lines })
    }

    /// True when no line fails. This says nothing about structural adequacy.
    pub fn no_failures(&self) -> bool {
        self.lines.iter().all(|l| l.status != Status::Fails)
    }

    /// The report, line by line, for printing with the gantry.
    pub fn calculation(&self) -> Vec<String> {
        let mut out = vec![
            format!("Gantry {}: requirement checks on entered geometry to {CD127_REVISION} and {CD365_REVISION}. Confirm every CD 365 clause against the current version before relying on it.", self.mark),
            NOT_A_STRUCTURAL_DESIGN.to_string(),
        ];
        out.extend(self.lines.iter().map(|l| format!("[{}] {} ({})", l.status.label(), l.text, l.citation)));
        out
    }
}

/// The sentence the report and the README carry.
pub const NOT_A_STRUCTURAL_DESIGN: &str = "This is not a structural design: passing these checks does not show that the gantry, its connections or its foundations are structurally adequate.";

/// Requirements of CD 365 Revision 1 that are reported for the engineer and not checked.
/// Each text paraphrases the clause cited; read the clause itself.
pub const LISTED_REQUIREMENTS: [(&str, &str); 11] = [
    ("CD 365 r1 cl. 3.1", "Technical approval of the design is required in accordance with CG 300, covering all structural elements including foundations, sign plates, sign support frames and fixings."),
    ("CD 365 r1 cl. 6.1, 6.10", "The gantry is to be designed to the Eurocode bridge parts and its fatigue performance verified. Not done here."),
    ("CD 365 r1 cl. 2.1, 2.2", "The type of gantry is to be selected by a risk assessment and a whole-life cost benefit analysis, including passively safe and non-passively safe types."),
    ("CD 365 r1 cl. 3.2, 3.3", "Design working life: 60 years for standard gantries and 30 years for smart motorway and passively safe gantries; minimum fatigue design life is the design working life plus 10 years."),
    ("CD 365 r1 cl. 4.1", "Vehicle restraint systems are to be provided in accordance with CD 377. No CD 377 value is checked here."),
    ("CD 365 r1 cl. 3.20", "Horizontal and vertical clearances between the structure and vehicle restraint systems are to be in accordance with CD 377 and CD 127. Not checked here."),
    ("CD 365 r1 cl. 7.28.1", "For non-passively safe gantries, supports in the verge should be set back from the edge of the carriageway. The clause gives no distance."),
    ("CD 365 r1 cl. 3.16, 3.18", "The gantry is to be designed for future maintenance, with access arrangements for the equipment it carries."),
    ("CD 365 r1 cl. 9.1", "Where the risk assessment identifies a need for permanent maintenance access, Section 9 (walkways, ladders, handrails) applies. Not modelled here."),
    ("CD 365 r1 cl. 6.23", "Foundations, holding down bolts, plinths and bases are to have greater reserves of structural resistance than the gantry they support. The foundation blocks here are entered sizes, not designed."),
    ("CD 365 r1 cl. 6.6, 6.7, Table 6.7", "Deformations at the serviceability limit state are limited by Table 6.7 and dead load deflection is to be countered by pre-camber. Deflections are not computed here."),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;
    use crate::{Carriageway, Direction, SignPosition};
    use afrobim_alignment::{AlignmentBuilder, Profile, Pvi};

    fn with(f: impl Fn(&mut GantryDefinition)) -> GantryChecks {
        let mut d = portal();
        d.checks.sls_deflection_mm = Some(0.0);
        f(&mut d);
        GantryChecks::run("G1", &d, None).unwrap()
    }

    #[test]
    fn table_4_1_values_are_those_of_cd_127_version_1_0_1() {
        // CD 127 Version 1.0.1 Table 4.1, row "Footbridges, sign and signal gantries...":
        // new construction 5.70 + S, maintained 5.41 + S.
        assert_eq!(table_headroom_mm(HeadroomBasis::NewConstruction, false), 5700.0);
        assert_eq!(table_headroom_mm(HeadroomBasis::Maintained, false), 5410.0);
        // Row "All permanent structures over high load routes": 6.45 + S and 6.18 + S.
        assert_eq!(table_headroom_mm(HeadroomBasis::NewConstruction, true), 6450.0);
        assert_eq!(table_headroom_mm(HeadroomBasis::Maintained, true), 6180.0);
    }

    #[test]
    fn table_4_3_sag_compensation() {
        // CD 127 Version 1.0.1 Table 4.3, every row.
        for (r, s) in [(1000.0, 80.0), (1200.0, 70.0), (1500.0, 55.0), (2000.0, 45.0), (3000.0, 25.0), (6000.0, 15.0)] {
            assert_eq!(sag_compensation_mm(Some(r)), s);
        }
        assert_eq!(sag_compensation_mm(Some(500.0)), 80.0); // "≤ 1000"
        assert_eq!(sag_compensation_mm(Some(6000.1)), 0.0); // "> 6000"
        assert_eq!(sag_compensation_mm(Some(20_000.0)), 0.0);
        assert_eq!(sag_compensation_mm(None), 0.0);
        // Between rows: the smaller tabulated radius, never interpolated.
        assert_eq!(sag_compensation_mm(Some(1100.0)), 80.0);
        assert_eq!(sag_compensation_mm(Some(1999.0)), 55.0);
        assert_eq!(sag_compensation_mm(Some(4500.0)), 25.0);
    }

    #[test]
    fn cd_365_values() {
        // CD 365 Revision 1 clause 3.23: "two times their maximum height".
        assert_eq!(CD365_3_23_OVERBRIDGE_HEIGHT_FACTOR, 2.0);
    }

    #[test]
    fn headroom_at_the_limit_and_either_side() {
        // Hand-worked: new gantry, no sag, level road, no deflection: 5700 mm required.
        let at = with(|d| d.boom.underside_above_reference_mm = 5700.0);
        assert_eq!((at.headroom.required_mm, at.headroom.headroom_mm, at.headroom.status), (5700.0, 5700.0, Status::Ok));
        let below = with(|d| d.boom.underside_above_reference_mm = 5699.0);
        assert_eq!((below.headroom.headroom_mm, below.headroom.status), (5699.0, Status::Fails));
        assert!(!below.no_failures());
        let above = with(|d| d.boom.underside_above_reference_mm = 5701.0);
        assert_eq!(above.headroom.status, Status::Ok);
        assert!(above.no_failures());
    }

    #[test]
    fn headroom_with_sag_deflection_surface_rise_and_a_low_sign() {
        // Sag radius 1500 m: S = 55; required 5755. Boom at 6000; a sign hangs 150 below: lowest
        // 5850. Surface rises 40 (cross-fall); SLS deflection 55: vertical 5850 − 40 − 55 = 5755.
        let set = |d: &mut GantryDefinition, deflection: f64| {
            d.checks.sag_radius_m = Some(1500.0);
            d.checks.surface_rise_mm = 40.0;
            d.checks.sls_deflection_mm = Some(deflection);
            d.signs.push(mount(1, SignPosition::Lane { index: 2 }, -150.0));
        };
        let at = with(|d| set(d, 55.0));
        assert_eq!((at.headroom.table_value_mm, at.headroom.sag_compensation_mm, at.headroom.required_mm), (5700.0, 55.0, 5755.0));
        assert_eq!((at.headroom.lowest_point_mm, at.headroom.vertical_clearance_mm, at.headroom.status), (5850.0, 5755.0, Status::Ok));
        assert_eq!(with(|d| set(d, 56.0)).headroom.status, Status::Fails);
        assert_eq!(with(|d| set(d, 54.0)).headroom.status, Status::Ok);
        // Maintained headroom on a high load route: 6180 + 55 = 6235 > 5755 available.
        let hl = with(|d| {
            set(d, 55.0);
            d.checks.high_load_route = true;
            d.checks.headroom_basis = HeadroomBasis::Maintained;
        });
        assert_eq!((hl.headroom.required_mm, hl.headroom.status), (6235.0, Status::Fails));
        // Other equipment 400 below the boom governs over the sign: 5600.
        assert_eq!(with(|d| d.checks.equipment_below_boom_mm = 400.0).headroom.lowest_point_mm, 5600.0);
    }

    #[test]
    fn headroom_without_a_deflection_is_incomplete_not_ok() {
        let mut d = portal();
        assert_eq!(d.checks.sls_deflection_mm, None);
        let c = GantryChecks::run("G1", &d, None).unwrap();
        assert_eq!(c.headroom.status, Status::Incomplete);
        assert!(!c.headroom.deflection_entered);
        assert!(c.calculation().iter().any(|l| l.contains("NOT ENTERED")));
        // A failure is still a failure.
        d.boom.underside_above_reference_mm = 5000.0;
        assert_eq!(GantryChecks::run("G1", &d, None).unwrap().headroom.status, Status::Fails);
    }

    #[test]
    fn headroom_is_square_to_the_surface_on_a_grade() {
        // Profile grade 6 %: headroom = 5700 / √(1 + 0.06²) = 5689.77 mm, which fails 5700;
        // 5711 / √1.0036 = 5700.75 passes.
        let mut a = AlignmentBuilder::new("A1", 0.0, 0.0, 0.0, 0.0).line(500.0).build().unwrap();
        a.profile = Some(Profile::new("P", vec![Pvi::point(0.0, 10.0), Pvi::point(500.0, 40.0)]).unwrap());
        let mut d = portal();
        d.checks.sls_deflection_mm = Some(0.0);
        d.boom.underside_above_reference_mm = 5700.0;
        let c = GantryChecks::run("G1", &d, Some(&a)).unwrap();
        assert!(close(c.headroom.grade, 0.06));
        assert!((c.headroom.headroom_mm - 5700.0 / 1.0036f64.sqrt()).abs() < 1e-9);
        assert!((c.headroom.headroom_mm - 5689.77).abs() < 0.01);
        assert_eq!(c.headroom.status, Status::Fails);
        d.boom.underside_above_reference_mm = 5711.0;
        assert_eq!(GantryChecks::run("G1", &d, Some(&a)).unwrap().headroom.status, Status::Ok);
        // Cross-fall 2.5 % as well: 5700 / √(1 + 0.0036 + 0.000625).
        d.boom.underside_above_reference_mm = 5700.0;
        d.checks.cross_fall_pct = -2.5;
        let c = GantryChecks::run("G1", &d, Some(&a)).unwrap();
        assert!((c.headroom.headroom_mm - 5700.0 / 1.004225f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn legs_are_reported_from_the_edge_of_the_paved_width() {
        // Paved edges at ±8.3 m; legs at ±9 m, 600 wide: faces 0.4 m and 1.0 m from the edge.
        let c = with(|_| {});
        let sfz: Vec<&CheckLine> = c.lines.iter().filter(|l| l.citation.contains("4.6") && l.status == Status::Reported).collect();
        assert_eq!(sfz.len(), 2);
        assert!(sfz[0].text.contains("Leg 1: near face 0.400 m and far face 1.000 m") && sfz[0].text.contains("offset -8.300 m"));
        assert!(sfz[1].text.contains("Leg 2: near face 0.400 m and far face 1.000 m"));
        let none = with(|d| d.carriageways.clear());
        assert!(none.lines.iter().any(|l| l.citation.contains("4.6") && l.status == Status::Incomplete));
    }

    fn dual(d: &mut GantryDefinition) {
        // Two 7.3 m carriageways either side of a 4 m central reserve (−2..2), legs outside.
        d.carriageways = vec![Carriageway::one_way(-9.3, 2, 3.65, Direction::UpStation), Carriageway::one_way(2.0, 2, 3.65, Direction::DownStation)];
        d.legs[0].offset_m = -11.0;
        d.legs[1].offset_m = 11.0;
    }

    #[test]
    fn central_reserve_support_cd_365_3_21() {
        let line = |c: &GantryChecks| c.lines.iter().find(|l| l.citation == "CD 365 r1 cl. 3.21").unwrap().clone();
        assert_eq!(line(&with(|_| {})).status, Status::NotApplicable); // single carriageway
        assert_eq!(line(&with(dual)).status, Status::Ok);
        let in_reserve = |d: &mut GantryDefinition| {
            dual(d);
            d.legs[1].offset_m = 0.0;
        };
        assert_eq!(line(&with(in_reserve)).status, Status::Fails);
        assert_eq!(line(&with(|d| { in_reserve(d); d.checks.central_reserve_support_agreed_with_taa = true; })).status, Status::Ok);
        assert_eq!(line(&with(|d| { in_reserve(d); d.checks.passively_safe = true; })).status, Status::NotApplicable);
    }

    #[test]
    fn overbridge_distance_cd_365_3_23() {
        let line = |c: &GantryChecks| c.lines.iter().find(|l| l.citation == "CD 365 r1 cl. 3.23").unwrap().clone();
        // Highest point: boom top 6800; lowest base −200: maximum height 7.0 m; 2 × 7.0 = 14.0 m.
        let none = line(&with(|_| {}));
        assert_eq!(none.status, Status::Requirement);
        assert!(none.text.contains("2 x 7.000 = 14.000 m"));
        assert_eq!(line(&with(|d| d.checks.distance_to_overbridge_m = Some(14.0))).status, Status::Ok);
        assert_eq!(line(&with(|d| d.checks.distance_to_overbridge_m = Some(13.99))).status, Status::Fails);
        assert_eq!(line(&with(|d| { d.checks.distance_to_overbridge_m = Some(5.0); d.checks.wind_interaction_in_design = true; })).status, Status::Ok);
        // A sign standing above the boom raises the maximum height: 6000 + 0 + 2000 = 8000 → 8.2 m.
        let tall = line(&with(|d| d.signs.push(mount(1, SignPosition::Lane { index: 1 }, 0.0))));
        assert!(tall.text.contains("2 x 8.200 = 16.400 m"));
    }

    #[test]
    fn the_report_says_what_it_is_and_cites_every_line() {
        let c = with(|_| {});
        let report = c.calculation();
        assert!(report[0].contains("CD 127 Version 1.0.1") && report[0].contains("CD 365 Revision 1") && report[0].contains("SUPERSEDED"));
        assert_eq!(report[1], NOT_A_STRUCTURAL_DESIGN);
        assert_eq!(report.len(), 2 + c.lines.len());
        assert!(c.lines.iter().all(|l| l.citation.starts_with("CD 127 v1.0.1") || l.citation.starts_with("CD 365 r1")));
        assert!(report.iter().any(|l| l.starts_with("[OK] Headroom 6000 mm against 5700 mm: OK.")));
        assert!(report.iter().any(|l| l.contains("CD 377") && l.starts_with("[REQUIREMENT]")));
        // Portal: beam ends level by construction; cantilever: not applicable.
        assert!(c.lines.iter().any(|l| l.citation == "CD 365 r1 cl. 3.22" && l.status == Status::Ok));
        let mut cant = cantilever();
        cant.checks.sls_deflection_mm = Some(10.0);
        let cc = GantryChecks::run("C1", &cant, None).unwrap();
        assert!(cc.lines.iter().any(|l| l.citation == "CD 365 r1 cl. 3.22" && l.status == Status::NotApplicable));
        assert_eq!(cc.headroom.headroom_mm, 5990.0);
        // An invalid definition is refused.
        let mut bad = portal();
        bad.legs.clear();
        assert!(GantryChecks::run("X", &bad, None).is_err());
    }
}
