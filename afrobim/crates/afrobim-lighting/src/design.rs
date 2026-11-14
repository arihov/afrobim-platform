//! Requirement checks for a lighting element, set out line by line with the clause each comes
//! from.
//!
//! # Basis
//!
//! - **DMRB TD 501 Road lighting design, Version 0.1.0, September 2025** (National Highways;
//!   supersedes Revision 0 of March 2020). Governing.
//! - **DMRB TA 501 Road lighting appraisal, Revision 0, March 2020**: the need for lighting.
//! - **DMRB CD 109 Highway link design, Version 1.1.0, October 2026**, Table 2.10: stopping
//!   sight distance, which TD 501 clause 3.11 calls on.
//! - **DMRB CD 377 Requirements for road restraint systems, Version 5.0.1, February 2026**,
//!   clauses 2.2, 2.3 and 2.6.1: lighting columns as roadside hazards, which TD 501 clause 3.4
//!   calls on.
//! - **MoWT Road Design Manual Volume 1: Geometric Design, January 2010**, section 12.8
//!   (pages 259 and 260). A local reference, reported beside the UK lines and never a reason to
//!   fail: where it differs, the UK documents govern.
//! - **BS 5489-1:2003** (the owner's copy; **a superseded edition**, replaced in 2013 and 2020),
//!   through [`crate::bs5489_2003`]: Table 2 column clearances, clause 5.2.1.2 vertical
//!   clearance, clause 5.2.3.3 bracket projection, Tables B.2 and B.3 class selection and
//!   Table D.1 luminaire maintenance factors. Every line that uses it says so, and the report
//!   opens with a warning. A mismatch with its informative annexes or recommendations is
//!   reported as a difference, not a failure; the Table 2 and 5.7 m clearances can fail.
//! - **TII DN-LHT-03038, February 2012** (Ireland; the NRA addendum to the withdrawn UK
//!   TD 34/07), paragraphs 2.9 and 2.15. A secondary reference like the MoWT manual.
//!
//! # What TD 501 does and does not give
//!
//! TD 501 is short on numbers. It hands the lighting class, the performance values, the
//! maintenance factor, conflict areas and the column set-back to BS 5489-1, BS EN 13201-2, ILP
//! PLG02, ILP TR30 and BS EN 12767 and reproduces none of them. The current BS 5489-1 and
//! BS EN 13201-2 have not been read. So this crate holds **no luminance, illuminance,
//! uniformity, threshold increment or spacing-to-height value**, and the only lighting class
//! selection it holds is that of the superseded 2003 edition. The lighting class remains a
//! label the engineer enters, and where a check needs a current figure the engineer enters it
//! and the report says whose figure it is, or reports that the input is needed.
//!
//! What is checked exactly from the DMRB documents:
//!
//! | Check | Source |
//! |---|---|
//! | The need for lighting has been established by appraisal | TD 501 cl. 1.1; TA 501 cl. 1.1 |
//! | Luminaire design tilt is zero | TD 501 cl. 2.5 |
//! | Luminous intensity class is G4, G5 or G6 | TD 501 cl. 2.5, 6.54 item 2 |
//! | Light source is LED | TD 501 cl. 3.14 |
//! | Photobiological risk group is RG0 or RG1 | TD 501 cl. 1.4, 3.15 |
//! | An unlit gap between lit sections is at least 4 × stopping sight distance | TD 501 cl. 3.11; CD 109 Table 2.10 |
//! | Columns not certified to BS EN 12767 are hazards for the RRRAP; which CD 377 process applies | TD 501 cl. 3.4, 3.5; CD 377 cl. 2.2, 2.3, 2.6.1 |
//! | Supply capacity is the largest load plus 20 % | TD 501 cl. 5.8 |
//! | One lighting class per zone; unique references | TD 501 cl. 6.48, 6.49, 6.52 |
//! | Which design outputs of cl. 6.53 and 6.54 have been entered | TD 501 cl. 6.53, 6.54 |
//!
//! # Readings that the engineer should confirm
//!
//! - Clause 3.11 reads "There shall not be an unlit gap of less than four times the stopping
//!   sight distance between lit sections." It is applied as: an unlit length between two lit
//!   sections must be at least 4 × SSD (otherwise the lighting should be continuous).
//! - Its note takes the SSD from CD 109 "using the one step below desirable minimum distances".
//!   The row of that name in Table 2.10 is read at the road's design speed. Only the six design
//!   speeds of the table are held; there is no interpolation.
//! - Clause 3.15 prints "RG0 or RG01"; with clause 1.4 ("RG0 or RG1") it is read as RG0 or RG1.
//! - "Mounting height" is defined in TD 501 as above the road surface, while clause 6.53 item 2
//!   asks for the height "above adjacent ground level". This crate has one entered mounting
//!   height, above the profile level at the column's station.
//! - TD 501 gives the power density indicator and the annual energy consumption indicator the
//!   same unit (W lx⁻¹ m⁻²) and abbreviates the second "ANCI". Neither is computed here.
//! - CD 377 clause 2.6.1 is a "should" (advice) list; the check reports it as such.
//! - The MoWT manual gives a distance behind the kerb for design speeds of 50 km/h or less and
//!   80 km/h or more and nothing between; the set-back entered here is from the carriageway
//!   edge, which is the kerb face only on a kerbed road.
//!
//! The report is for an engineer to check. It is not a lighting design, and passing every line
//! says nothing about lighting performance, which needs a photometric calculation.

use afrobim_kernel::{Note, Severity};

use crate::bs5489_2003::{self, ClassSelection, IpMinimum, Pollution, RoadType};
use crate::model::{Arrangement, LightingData, LightingDefinition};
use crate::{fixed, note};

/// CD 109 Version 1.1.0, Table 2.10 "Design speed related parameters", stopping sight distance
/// rows: (design speed km/h, desirable minimum m, one step below desirable minimum m).
pub const CD109_TABLE_2_10_SSD: [(f64, f64, f64); 6] =
    [(120.0, 295.0, 215.0), (100.0, 215.0, 160.0), (85.0, 160.0, 120.0), (70.0, 120.0, 90.0), (60.0, 90.0, 70.0), (50.0, 70.0, 50.0)];

/// TD 501 Version 0.1.0 clause 3.11: the multiple of the stopping sight distance below which an
/// unlit gap between lit sections is not allowed.
pub const TD501_UNLIT_GAP_SSD_MULTIPLE: f64 = 4.0;

/// TD 501 Version 0.1.0 clause 5.8: the allowance for future capacity on the largest load, as a
/// fraction (20 %).
pub const TD501_SPARE_SUPPLY_CAPACITY: f64 = 0.20;

/// TD 501 Version 0.1.0 clauses 2.5 and 6.54 item 2: the luminous intensity classes allowed.
pub const TD501_LUMINOUS_INTENSITY_CLASSES: [&str; 3] = ["G4", "G5", "G6"];

/// TD 501 Version 0.1.0 clauses 1.4 and 3.15: the photobiological risk groups allowed.
pub const TD501_RISK_GROUPS: [&str; 2] = ["RG0", "RG1"];

/// CD 377 Version 5.0.1 clauses 2.2 and 2.3: the speed limit (mph) and two-way flow (AADT) from
/// which the RRRAP is used on motorways and all-purpose trunk roads.
pub const CD377_RRRAP_SPEED_LIMIT_MPH: f64 = 50.0;
/// See [`CD377_RRRAP_SPEED_LIMIT_MPH`].
pub const CD377_RRRAP_AADT: f64 = 5000.0;

/// TII DN-LHT-03038 (February 2012; the NRA addendum to TD 34/07) paragraph 2.15: bracket
/// projections "should not exceed 1.5 metres for mounting heights up to and including 8 metres,
/// and 2 metres for all other mounting heights": (mounting height limit m, projection m,
/// projection above that height m).
pub const TII_BRACKET_PROJECTION: (f64, f64, f64) = (8.0, 1.5, 2.0);
/// TII DN-LHT-03038 paragraph 2.15: brackets "inclined slightly upwards at an angle not
/// exceeding 5°".
pub const TII_MAX_BRACKET_INCLINATION_DEG: f64 = 5.0;
/// TII DN-LHT-03038 paragraph 2.9: lighting "should normally comply with luminous intensity
/// class G6".
pub const TII_LUMINOUS_INTENSITY_CLASS: &str = "G6";

/// MoWT Geometric Design Manual (January 2010) section 12.8: "luminaries should be mounted at a
/// height of at least 9 meters".
pub const MOWT_MIN_MOUNTING_HEIGHT_M: f64 = 9.0;
/// MoWT section 12.8: on median installations "12-15 meter mounting heights are favored".
pub const MOWT_MEDIAN_MOUNTING_HEIGHT_M: (f64, f64) = (12.0, 15.0);
/// MoWT section 12.8: columns behind vertical kerbs, "0.5m behind the kerb for roads with a
/// design speed of 50 km/h or less".
pub const MOWT_SETBACK_LOW_SPEED: (f64, f64) = (50.0, 0.5);
/// MoWT section 12.8: "1.2m or greater for roads with a design speed of 80 km/h or greater".
pub const MOWT_SETBACK_HIGH_SPEED: (f64, f64) = (80.0, 1.2);

/// The "one step below desirable minimum" stopping sight distance of CD 109 Version 1.1.0
/// Table 2.10 for a design speed, m. `None` for a speed that is not a column of the table: the
/// table is not interpolated.
pub fn cd109_ssd_one_step_below_m(design_speed_kph: f64) -> Option<f64> {
    CD109_TABLE_2_10_SSD.iter().find(|r| r.0 == design_speed_kph).map(|r| r.2)
}

/// The distance behind the kerb the MoWT manual (section 12.8) gives for a design speed, m, and
/// whether it is a minimum ("or greater"). `None` between 50 and 80 km/h, where the manual says
/// nothing.
pub fn mowt_setback_behind_kerb_m(design_speed_kph: f64) -> Option<(f64, bool)> {
    if design_speed_kph <= MOWT_SETBACK_LOW_SPEED.0 {
        Some((MOWT_SETBACK_LOW_SPEED.1, false))
    } else if design_speed_kph >= MOWT_SETBACK_HIGH_SPEED.0 {
        Some((MOWT_SETBACK_HIGH_SPEED.1, true))
    } else {
        None
    }
}

/// An unlit length between this installation and another lit section.
#[derive(Debug, Clone, PartialEq)]
pub struct UnlitGap {
    /// What the gap is, e.g. "to the junction lighting at A109".
    pub description: String,
    pub length_m: f64,
}

/// What the engineer enters for the checks, beyond the element itself. Every field is optional:
/// a missing one gives an "input needed" line, never an assumed value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CheckInputs {
    /// Whether the need for lighting has been established by a TA 501 appraisal (or the
    /// Overseeing Organisation has agreed that none is warranted, TA 501 clause 1.2).
    pub appraisal_done: Option<bool>,
    /// Design speed of the road, km/h.
    pub design_speed_kph: Option<f64>,
    /// Stopping sight distance to use for clause 3.11, m, when the design speed is not a column
    /// of CD 109 Table 2.10 or another figure governs. Takes precedence over the table.
    pub stopping_sight_distance_m: Option<f64>,
    /// Unlit lengths to adjacent lit sections.
    pub unlit_gaps: Vec<UnlitGap>,
    /// The minimum set-back the engineer has taken from CD 377 and BS 5489-1, m, and where from.
    pub required_setback_m: Option<f64>,
    pub required_setback_source: String,
    /// Whether the columns are certified passively safe to BS EN 12767, and the class as entered.
    pub passively_safe: Option<bool>,
    pub passive_safety_class: String,
    /// Speed limit, mph, and two-way flow, AADT, for the CD 377 process.
    pub speed_limit_mph: Option<f64>,
    pub aadt: Option<f64>,
    /// The maintenance factor the engineer has determined to BS 5489-1.
    pub maintenance_factor: Option<f64>,
    pub maintenance_factor_source: String,
    /// The row of BS 5489-1:2003 Table B.2 the road falls in, and its average daily traffic
    /// (both directions), to report the class that superseded edition suggests.
    pub road_type: Option<RoadType>,
    pub class_adt: Option<f64>,
    /// For a conflict area: the lighting class of the approaching traffic routes (the highest of
    /// them), to report the BS 5489-1:2003 Table B.3 class.
    pub conflict_approach_class: Option<String>,
    /// Luminaire sealing, pollution category and cleaning interval, to report the typical
    /// luminaire maintenance factor of BS 5489-1:2003 Table D.1.
    pub luminaire_ip: Option<IpMinimum>,
    pub pollution: Option<Pollution>,
    pub cleaning_interval_months: Option<u32>,
    /// Lamp (light source) flux maintenance factor from the manufacturer's data.
    pub lamp_flux_maintenance_factor: Option<f64>,
}

/// How one line came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The requirement is met.
    Ok,
    /// The requirement is not met.
    Fails,
    /// The check cannot be made until the engineer enters something.
    InputNeeded,
    /// A figure or statement for the engineer; nothing to pass or fail.
    Information,
    /// A reference that does not govern (a secondary document, informative guidance or a
    /// recommendation of the superseded BS 5489-1:2003) differs from the entered design. Not a
    /// failure.
    Differs,
}

impl Outcome {
    pub fn label(self) -> &'static str {
        match self {
            Outcome::Ok => "OK",
            Outcome::Fails => "FAILS",
            Outcome::InputNeeded => "INPUT NEEDED",
            Outcome::Information => "INFORMATION",
            Outcome::Differs => "DIFFERS (reference only, does not govern)",
        }
    }
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckLine {
    /// Document, revision and clause, e.g. "TD 501 v0.1.0 cl. 3.11".
    pub clause: String,
    /// What the clause requires, in brief.
    pub requirement: String,
    /// What was found.
    pub finding: String,
    pub outcome: Outcome,
}

/// The checks of one lighting element.
#[derive(Debug, Clone, PartialEq)]
pub struct LightingChecks {
    pub reference: String,
    pub lines: Vec<CheckLine>,
}

const TD501: &str = "TD 501 v0.1.0";
const MOWT: &str = "MoWT GDM 2010 s. 12.8";
const TII: &str = "TII DN-LHT-03038 (2012)";
const BS: &str = bs5489_2003::EDITION;

impl LightingChecks {
    /// True when no line fails. Lines that need input or only inform do not count either way, so
    /// this is not a statement that the design complies.
    pub fn passes(&self) -> bool {
        !self.lines.iter().any(|l| l.outcome == Outcome::Fails)
    }

    pub fn count(&self, outcome: Outcome) -> usize {
        self.lines.iter().filter(|l| l.outcome == outcome).count()
    }

    /// The report set out line by line with its references, for the engineer to check.
    pub fn calculation(&self) -> Vec<String> {
        let mut out = vec![
            format!("Road lighting requirement checks for '{}'.", self.reference),
            "Basis: DMRB TD 501 Version 0.1.0 (September 2025) with TA 501 Revision 0, CD 109 Version 1.1.0 and CD 377 Version 5.0.1; BS 5489-1:2003 where a line cites it; MoWT Geometric Design Manual (January 2010) section 12.8 and TII DN-LHT-03038 (February 2012) as secondary references only. TD 501 governs.".to_string(),
            format!("WARNING: {}", bs5489_2003::SUPERSEDED_NOTICE),
        ];
        out.extend(self.lines.iter().map(|l| format!("[{}] {} {}: {}.", l.clause, l.requirement, l.finding, l.outcome.label())));
        out.push(format!(
            "Summary: {} OK, {} fail, {} need input, {} for information, {} differ from a reference that does not govern.",
            self.count(Outcome::Ok),
            self.count(Outcome::Fails),
            self.count(Outcome::InputNeeded),
            self.count(Outcome::Information),
            self.count(Outcome::Differs)
        ));
        out.push("For an engineer to check. Not a certified design: no photometric, structural or electrical design has been done.".to_string());
        out
    }

    /// The failing lines as Error notes, lines needing input as Warnings and local differences
    /// as Info, for showing on the element.
    pub fn notes(&self) -> Vec<Note> {
        self.lines
            .iter()
            .filter_map(|l| {
                let severity = match l.outcome {
                    Outcome::Fails => Severity::Error,
                    Outcome::InputNeeded => Severity::Warning,
                    Outcome::Differs => Severity::Info,
                    Outcome::Ok | Outcome::Information => return None,
                };
                Some(note(severity, "LIGHTING_CHECK", format!("[{}] {} {}", l.clause, l.requirement, l.finding)))
            })
            .collect()
    }
}

/// Checks a lighting element against the requirements listed in the module documentation.
pub fn check_lighting(data: &LightingData, inputs: &CheckInputs) -> LightingChecks {
    let spec = data.definition.column();
    let lum = &spec.luminaire;
    let mut lines: Vec<CheckLine> = Vec::new();
    let mut line = |clause: String, requirement: &str, finding: String, outcome: Outcome| {
        lines.push(CheckLine { clause, requirement: requirement.to_string(), finding, outcome });
    };
    let td = |c: &str| format!("{TD501} cl. {c}");
    let m = |v: f64| fixed(v, 0, 3);

    // Need for lighting.
    let (finding, outcome) = match inputs.appraisal_done {
        Some(true) => ("The appraisal is recorded as done".to_string(), Outcome::Ok),
        Some(false) => ("No appraisal has been done".to_string(), Outcome::Fails),
        None => ("Not stated whether an appraisal has been done".to_string(), Outcome::InputNeeded),
    };
    line(format!("{TD501} cl. 1.1; TA 501 rev 0 cl. 1.1"), "The need for lighting is established by a TA 501 appraisal before design.", finding, outcome);

    // Lighting class and zone.
    let class = data.definition.lighting_class().trim();
    line(
        td("3.6, 6.48, 6.50"),
        "The lighting class is selected from BS 5489-1 on road hierarchy and traffic data, one class per road lighting zone.",
        if class.is_empty() {
            "No lighting class is entered".to_string()
        } else {
            format!("Class '{class}' as entered for the whole element; the selection is the engineer's (BS 5489-1 is not held)")
        },
        if class.is_empty() { Outcome::InputNeeded } else { Outcome::Information },
    );

    // References.
    let mut marks: Vec<&str> = data.layout.columns.iter().map(|c| c.mark.as_str()).collect();
    marks.sort_unstable();
    let duplicates = marks.windows(2).filter(|w| w[0] == w[1]).count();
    let reference = data.definition.reference().trim();
    line(
        td("6.49, 6.52"),
        "Each zone and each luminaire has a unique reference.",
        if reference.is_empty() {
            "The element has no reference".to_string()
        } else if duplicates > 0 {
            format!("{duplicates} repeated column mark(s) in '{reference}'")
        } else {
            format!("'{reference}' with {} distinct column mark(s); uniqueness across the scheme is not checked here", marks.len())
        },
        if reference.is_empty() || duplicates > 0 { Outcome::Fails } else { Outcome::Ok },
    );

    // Tilt and luminous intensity class.
    line(
        td("2.5"),
        "Luminaire design tilt shall be zero degrees.",
        format!("Tilt entered: {}°", fixed(spec.tilt_deg, 0, 2)),
        if spec.tilt_deg == 0.0 { Outcome::Ok } else { Outcome::Fails },
    );
    let (finding, outcome) = match lum.luminous_intensity_class.as_deref().map(|s| s.trim().to_uppercase()) {
        Some(g) if TD501_LUMINOUS_INTENSITY_CLASSES.contains(&g.as_str()) => (format!("{g} as entered"), Outcome::Ok),
        Some(g) if !g.is_empty() => (format!("'{g}' as entered"), Outcome::Fails),
        _ => ("No luminous intensity class is entered".to_string(), Outcome::InputNeeded),
    };
    line(td("2.5, 6.54(2)"), "The luminous intensity class is G4, G5 or G6.", finding, outcome);

    // Light source.
    let (finding, outcome) = match lum.led {
        Some(true) => ("LED as entered".to_string(), Outcome::Ok),
        Some(false) => ("Entered as not LED".to_string(), Outcome::Fails),
        None => ("The light source type is not entered".to_string(), Outcome::InputNeeded),
    };
    line(td("3.14"), "Luminaires shall contain an LED light source.", finding, outcome);
    let (finding, outcome) = match lum.risk_group.as_deref().map(|s| s.trim().to_uppercase().replace(' ', "")) {
        Some(g) if TD501_RISK_GROUPS.contains(&g.as_str()) => (format!("{g} as entered"), Outcome::Ok),
        Some(g) if !g.is_empty() => (format!("'{g}' as entered"), Outcome::Fails),
        _ => ("No risk group is entered".to_string(), Outcome::InputNeeded),
    };
    line(td("1.4, 3.15"), "The photobiological risk group to BS EN 62471 is RG0 or RG1 (3.15 prints 'RG01').", finding, outcome);

    // Set-back.
    let setback = match &data.definition {
        LightingDefinition::Installation(i) => Some(i.setback_m),
        LightingDefinition::Column(_) => None,
    };
    let requirement = "The off-set of columns from the carriageway is in accordance with CD 377 and BS 5489-1 (TD 501 gives no figure).";
    match (setback, inputs.required_setback_m) {
        (Some(s), Some(r)) => {
            let source = if inputs.required_setback_source.trim().is_empty() { "source not stated" } else { inputs.required_setback_source.trim() };
            line(
                td("3.4"),
                requirement,
                format!("Set-back {} m against {} m required as entered by the engineer ({source})", m(s), m(r)),
                if s >= r { Outcome::Ok } else { Outcome::Fails },
            );
        }
        (Some(s), None) => line(td("3.4"), requirement, format!("Set-back {} m; the required minimum has not been entered", m(s)), Outcome::InputNeeded),
        (None, _) => line(
            td("3.4"),
            requirement,
            "A single column has no entered carriageway edge; check its clearance from the carriageway on the drawing".to_string(),
            Outcome::InputNeeded,
        ),
    }

    // Passive safety and road restraint.
    let (finding, outcome) = match inputs.passively_safe {
        Some(true) => {
            let class = inputs.passive_safety_class.trim();
            (
                format!(
                    "Columns entered as certified to BS EN 12767{}; the class and deployment to ILP TR30 are the engineer's (neither document is held)",
                    if class.is_empty() { String::new() } else { format!(", class {class}") }
                ),
                Outcome::Information,
            )
        }
        Some(false) => (
            "Columns are not certified to BS EN 12767: CD 377 v5.0.1 cl. 2.6.1 item 16 lists them among the hazards that should be included in the RRRAP".to_string(),
            Outcome::Information,
        ),
        None => ("Not stated whether the columns are passively safe".to_string(), Outcome::InputNeeded),
    };
    line(
        format!("{TD501} cl. 3.5; CD 377 v5.0.1 cl. 2.6.1(16)"),
        "Passively safe lighting structures are selected and deployed to BS EN 12767 and ILP TR30.",
        finding,
        outcome,
    );
    let (finding, outcome) = match (inputs.speed_limit_mph, inputs.aadt) {
        (Some(v), Some(q)) if v >= CD377_RRRAP_SPEED_LIMIT_MPH && q >= CD377_RRRAP_AADT => (
            format!("Speed limit {} mph and {} AADT: cl. 2.3 applies, the RRRAP shall be used to record the hazards, the columns among them", fixed(v, 0, 1), fixed(q, 0, 0)),
            Outcome::Information,
        ),
        (Some(v), Some(q)) => (
            format!("Speed limit {} mph and {} AADT: cl. 2.2 applies, a local risk assessment shall determine whether a road restraint system is necessary", fixed(v, 0, 1), fixed(q, 0, 0)),
            Outcome::Information,
        ),
        _ => ("The speed limit and two-way AADT are not both entered".to_string(), Outcome::InputNeeded),
    };
    line(
        format!("{TD501} cl. 3.4; CD 377 v5.0.1 cl. 2.2, 2.3"),
        "Roadside hazards are assessed by the RRRAP at 50 mph or more with 5,000 AADT or more, otherwise by local risk assessment (motorways and all-purpose trunk roads).",
        finding,
        outcome,
    );

    // Maintenance factor.
    let (finding, outcome) = match inputs.maintenance_factor {
        Some(f) => {
            let source = if inputs.maintenance_factor_source.trim().is_empty() { "source not stated" } else { inputs.maintenance_factor_source.trim() };
            (format!("{} as entered by the engineer ({source}); not verified and not used in any calculation here", fixed(f, 0, 3)), Outcome::Information)
        }
        None => ("No maintenance factor is entered".to_string(), Outcome::InputNeeded),
    };
    line(td("3.8"), "Maintenance factors are in accordance with BS 5489-1.", finding, outcome);

    // Unlit gaps.
    let ssd = match (inputs.stopping_sight_distance_m, inputs.design_speed_kph) {
        (Some(d), _) => Some((d, "as entered by the engineer".to_string())),
        (None, Some(v)) => cd109_ssd_one_step_below_m(v).map(|d| (d, format!("CD 109 v1.1.0 Table 2.10, one step below desirable minimum at {} km/h", fixed(v, 0, 1)))),
        (None, None) => None,
    };
    let clause = format!("{TD501} cl. 3.11; CD 109 v1.1.0 Table 2.10");
    let requirement = "An unlit gap between lit sections is not less than four times the stopping sight distance.";
    match (&ssd, inputs.unlit_gaps.is_empty()) {
        (None, _) => line(
            clause,
            requirement,
            match inputs.design_speed_kph {
                Some(v) => format!("{} km/h is not a design speed of Table 2.10 (120, 100, 85, 70, 60, 50); enter the stopping sight distance", fixed(v, 0, 1)),
                None => "Neither a design speed nor a stopping sight distance is entered".to_string(),
            },
            Outcome::InputNeeded,
        ),
        (Some((d, source)), true) => line(
            clause,
            requirement,
            format!("SSD = {} m ({source}); minimum unlit gap = 4 × {} = {} m. No gaps to adjacent lit sections are entered", m(*d), m(*d), m(TD501_UNLIT_GAP_SSD_MULTIPLE * d)),
            Outcome::Information,
        ),
        (Some((d, source)), false) => {
            let least = TD501_UNLIT_GAP_SSD_MULTIPLE * d;
            for gap in &inputs.unlit_gaps {
                line(
                    clause.clone(),
                    requirement,
                    format!("SSD = {} m ({source}); minimum 4 × {} = {} m; gap '{}' is {} m", m(*d), m(*d), m(least), gap.description, m(gap.length_m)),
                    if gap.length_m >= least { Outcome::Ok } else { Outcome::Fails },
                );
            }
        }
    }

    // Supply capacity.
    let load = data.connected_load_w();
    line(
        td("5.7, 5.8"),
        "The supply meets the largest load under normal operation plus 20 % for future capacity, on manufacturers' data.",
        format!(
            "Connected load of this element = {} luminaire(s) × {} W = {} W; with 20 % = {} W. Luminaires only: other equipment on the supply and any electrical design are not included",
            data.luminaire_count(),
            fixed(lum.power_w, 0, 1),
            fixed(load, 0, 1),
            fixed(load * (1.0 + TD501_SPARE_SUPPLY_CAPACITY), 0, 1)
        ),
        Outcome::Information,
    );

    // Design outputs.
    let mut missing: Vec<&str> = Vec::new();
    if lum.mass_kg.is_none() {
        missing.push("maximum weight (6.53 item 4)");
    }
    if lum.windage_m2.is_none() {
        missing.push("maximum windage area (6.53 item 5)");
    }
    if lum.lumen_output_lm <= 0.0 {
        missing.push("lumen output (6.54 item 1)");
    }
    if lum.cct_k.is_none() {
        missing.push("correlated colour temperature (6.54 item 3)");
    }
    if lum.cri.is_none() {
        missing.push("minimum colour rendering index (6.54 item 4)");
    }
    if lum.power_w <= 0.0 {
        missing.push("maximum power (6.54 item 6)");
    }
    line(
        td("6.53, 6.54"),
        "The design outputs define each luminaire's support, mounting height, weight, windage, attitude and characteristics.",
        if missing.is_empty() {
            "Mounting height, design attitude, weight, windage, lumen output, CCT, CRI and power are entered; driver current, control socket, IP, IK, electrical class, surge protection and corrosivity category are not held in this model".to_string()
        } else {
            format!("Not entered: {}. Driver current, control socket, IP, IK, electrical class, surge protection and corrosivity category are not held in this model", missing.join(", "))
        },
        if missing.is_empty() { Outcome::Information } else { Outcome::InputNeeded },
    );

    // What is not verified.
    line(
        td("2.1, 4.2, 6.50"),
        "Lighting performance fulfils BS 5489-1 and BS EN 13201-2; energy performance indicators to BS EN 13201-5.",
        "Not verified: no photometric calculation is made and the photometric file is a reference only. The power density and annual energy indicators are not computed".to_string(),
        Outcome::Information,
    );
    line(
        td("3.19"),
        "Columns are in accordance with CD 354 and CC 481.",
        "Not verified: column and foundation structural design is outside this crate".to_string(),
        Outcome::Information,
    );

    // BS 5489-1:2003 (superseded edition): what it states exactly.
    let superseded = "superseded edition: confirm against the current BS 5489-1";
    let entered_class = data.definition.lighting_class().trim().to_uppercase();
    if let Some(road) = inputs.road_type {
        let clause = format!("{TD501} cl. 3.6; {BS} Table B.2 (informative)");
        let requirement = "The lighting class for a motorway or traffic route by road type and average daily traffic.";
        match bs5489_2003::traffic_route_class(road, inputs.class_adt) {
            ClassSelection::Classes(classes) => {
                let flow = inputs.class_adt.map_or(String::new(), |q| format!(", ADT {}", fixed(q, 0, 0)));
                let matches = classes.iter().any(|c| c.to_uppercase() == entered_class);
                line(
                    clause,
                    requirement,
                    format!(
                        "{}{flow}: {}; entered class '{}' ({superseded}; later editions use the M, C and P class names)",
                        road.label(),
                        classes.join(" or "),
                        data.definition.lighting_class().trim()
                    ),
                    if matches { Outcome::Ok } else { Outcome::Differs },
                );
            }
            ClassSelection::FlowNeeded => line(clause, requirement, format!("{}: the average daily traffic is not entered", road.label()), Outcome::InputNeeded),
            ClassSelection::NotCovered => line(
                clause,
                requirement,
                format!("{}: the table gives '<15 000' and '>15 000' and has no entry for exactly 15 000", road.label()),
                Outcome::InputNeeded,
            ),
        }
    }
    if let Some(approach) = inputs.conflict_approach_class.as_deref() {
        let clause = format!("{TD501} cl. 3.10; {BS} Table B.3 (informative)");
        let requirement = "The lighting class of a conflict area follows from the class of the approaching traffic routes.";
        match bs5489_2003::conflict_area_class(approach) {
            Some(ce) => line(
                clause,
                requirement,
                format!("Approach class {}: conflict area class {ce}; entered class '{}' ({superseded})", approach.trim(), data.definition.lighting_class().trim()),
                if ce == entered_class { Outcome::Ok } else { Outcome::Differs },
            ),
            None => line(clause, requirement, format!("'{}' is not one of ME1 to ME5, the classes of the table", approach.trim()), Outcome::InputNeeded),
        }
    }
    if let (LightingDefinition::Installation(i), Some(v)) = (&data.definition, inputs.design_speed_kph) {
        if i.arrangement != Arrangement::TwinCentral {
            let clause = format!("{TD501} cl. 3.4; {BS} Table 2");
            let requirement = "Recommended minimum clearance from the edge of the carriageway to the face of a lighting column, by design speed.";
            let clearance = bs5489_2003::min_clearance_m(v);
            match clearance {
                Some(c) => line(
                    clause,
                    requirement,
                    format!(
                        "{} km/h: {} m; set-back to the column axis {} m (the face is nearer by half the column width, which is not modelled; where a safety barrier protects the column its design governs, note 3) ({superseded})",
                        fixed(v, 0, 1),
                        m(c),
                        m(i.setback_m)
                    ),
                    if i.setback_m >= c { Outcome::Ok } else { Outcome::Fails },
                ),
                None => line(
                    clause,
                    requirement,
                    format!("{} km/h is not a design speed of Table 2 (50, 80, 100, 120); the table is not interpolated", fixed(v, 0, 1)),
                    Outcome::InputNeeded,
                ),
            }
            // Vertical clearance where the luminaire is over the carriageway or within the clearance.
            let within = i.overhang_m() >= 0.0 || clearance.is_some_and(|c| i.setback_m - spec.bracket_outreach_m < c);
            if within {
                let least = bs5489_2003::VERTICAL_CLEARANCE_OVER_CARRIAGEWAY_M;
                line(
                    format!("{BS} cl. 5.2.1.2"),
                    "The lowest point of a luminaire or bracket over the carriageway, or within the Table 2 clearance, should be at least 5.7 m above the carriageway surface.",
                    format!(
                        "Mounting height {} m against {} m; the mounting height is to the luminaire, so the lowest point of the bracket and luminaire is for the engineer to confirm ({superseded})",
                        m(spec.mounting_height_m),
                        m(least)
                    ),
                    if spec.mounting_height_m >= least { Outcome::Ok } else { Outcome::Fails },
                );
            }
        }
    }
    let quarter = bs5489_2003::BRACKET_PROJECTION_MAX_FRACTION_OF_HEIGHT * spec.mounting_height_m;
    line(
        format!("{BS} cl. 5.2.3.3"),
        "Bracket projection is recommended not to exceed one quarter of the mounting height.",
        format!("Outreach {} m against {} m / 4 = {} m ({superseded})", m(spec.bracket_outreach_m), m(spec.mounting_height_m), m(quarter)),
        if spec.bracket_outreach_m <= quarter { Outcome::Ok } else { Outcome::Differs },
    );
    if let (Some(ip), Some(pollution), Some(months)) = (inputs.luminaire_ip, inputs.pollution, inputs.cleaning_interval_months) {
        let clause = format!("{TD501} cl. 3.8; {BS} cl. 6.4.4, Table D.1 (informative)");
        let requirement = "The maintenance factor is the product of the lamp flux maintenance factor (manufacturer's data) and the luminaire maintenance factor (typical values in Table D.1).";
        match bs5489_2003::luminaire_maintenance_factor(ip, pollution, months) {
            Some(f) => {
                let product = match inputs.lamp_flux_maintenance_factor {
                    Some(l) => format!(
                        "; with the entered lamp flux maintenance factor {} the maintenance factor is {} × {} = {}",
                        fixed(l, 0, 3),
                        fixed(l, 0, 3),
                        fixed(f, 2, 2),
                        fixed(l * f, 0, 3)
                    ),
                    None => "; the lamp flux maintenance factor is not entered".to_string(),
                };
                let entered = inputs.maintenance_factor.map_or(String::new(), |e| format!("; maintenance factor entered by the engineer {}", fixed(e, 0, 3)));
                line(
                    clause,
                    requirement,
                    format!(
                        "Typical luminaire maintenance factor for {ip:?}, {pollution:?} pollution, cleaning every {months} months: {}{product}{entered}. The table predates LED luminaires ({superseded})",
                        fixed(f, 2, 2)
                    ),
                    Outcome::Information,
                );
            }
            None => line(
                clause,
                requirement,
                format!("{months} months is not a cleaning interval of Table D.1 (12, 18, 24, 36); the table is not interpolated"),
                Outcome::InputNeeded,
            ),
        }
    }
    line(
        BS.to_string(),
        "Performance requirements of the lighting classes.",
        "Not held: BS 5489-1:2003 refers to BS EN 13201-2 for luminance, uniformity, threshold increment and illuminance and does not reproduce them".to_string(),
        Outcome::Information,
    );

    // Secondary reference: TII (Ireland).
    let limit = if spec.mounting_height_m <= TII_BRACKET_PROJECTION.0 { TII_BRACKET_PROJECTION.1 } else { TII_BRACKET_PROJECTION.2 };
    line(
        format!("{TII} para. 2.15"),
        "Bracket projections should not exceed 1.5 m for mounting heights up to and including 8 m and 2 m for all other mounting heights.",
        format!("Outreach {} m against {} m at a mounting height of {} m", m(spec.bracket_outreach_m), m(limit), m(spec.mounting_height_m)),
        if spec.bracket_outreach_m <= limit { Outcome::Ok } else { Outcome::Differs },
    );
    line(
        format!("{TII} para. 2.15"),
        "Brackets should be inclined slightly upwards at an angle not exceeding 5°.",
        format!("Tilt {}°; TD 501 cl. 2.5 requires zero tilt and governs", fixed(spec.tilt_deg, 0, 2)),
        if (0.0..=TII_MAX_BRACKET_INCLINATION_DEG).contains(&spec.tilt_deg) { Outcome::Ok } else { Outcome::Differs },
    );
    if let Some(g) = lum.luminous_intensity_class.as_deref().map(|s| s.trim().to_uppercase()).filter(|g| !g.is_empty()) {
        line(
            format!("{TII} para. 2.9"),
            "Lighting should normally comply with luminous intensity class G6.",
            format!("{g} as entered; TD 501 cl. 2.5 allows G4 to G6 and governs"),
            if g == TII_LUMINOUS_INTENSITY_CLASS { Outcome::Ok } else { Outcome::Differs },
        );
    }

    // Local reference: MoWT.
    let h = spec.mounting_height_m;
    line(
        MOWT.to_string(),
        "Luminaires should be mounted at a height of at least 9 m.",
        format!("Mounting height {} m", m(h)),
        if h >= MOWT_MIN_MOUNTING_HEIGHT_M { Outcome::Ok } else { Outcome::Differs },
    );
    if let LightingDefinition::Installation(i) = &data.definition {
        if i.arrangement == Arrangement::TwinCentral {
            let (lo, hi) = MOWT_MEDIAN_MOUNTING_HEIGHT_M;
            line(
                MOWT.to_string(),
                "On median installations dual mast arms should be used, for which 12 to 15 m mounting heights are favoured; the columns should be protected by a safety barrier.",
                format!("Twin central arrangement at {} m; the barrier is not modelled", m(h)),
                if h >= lo && h <= hi { Outcome::Ok } else { Outcome::Differs },
            );
        } else {
            let requirement = "Columns behind vertical kerbs: 0.5 m behind the kerb at a design speed of 50 km/h or less, 1.2 m or greater at 80 km/h or greater.";
            match inputs.design_speed_kph.map(|v| (v, mowt_setback_behind_kerb_m(v))) {
                Some((v, Some((d, at_least)))) => line(
                    MOWT.to_string(),
                    requirement,
                    format!(
                        "Design speed {} km/h: {} m{}; set-back from the carriageway edge {} m (the kerb face only on a kerbed road)",
                        fixed(v, 0, 1),
                        m(d),
                        if at_least { " or greater" } else { "" },
                        m(i.setback_m)
                    ),
                    if i.setback_m >= d { Outcome::Ok } else { Outcome::Differs },
                ),
                Some((v, None)) => line(
                    MOWT.to_string(),
                    requirement,
                    format!("The manual gives no distance for {} km/h (between 50 and 80 km/h); set-back {} m", fixed(v, 0, 1), m(i.setback_m)),
                    Outcome::Information,
                ),
                None => line(MOWT.to_string(), requirement, "No design speed is entered".to_string(), Outcome::InputNeeded),
            }
        }
    }
    line(
        MOWT.to_string(),
        "Columns within the clear zone should have a frangible impact attenuation feature, except in densely populated areas with footways.",
        "The clear zone is not modelled; for the engineer to assess with the passive safety line above".to_string(),
        Outcome::Information,
    );

    LightingChecks { reference: data.definition.reference().to_string(), lines }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_installation;
    use crate::layout::tests::{installation, straight};
    use crate::model::LightingData;

    fn data(arrangement: Arrangement) -> LightingData {
        let def = installation(arrangement, 0.0, 300.0, 50.0);
        let layout = layout_installation(&straight(), &def).unwrap();
        LightingData { definition: LightingDefinition::Installation(def), layout }
    }

    fn find<'a>(c: &'a LightingChecks, clause: &str) -> Vec<&'a CheckLine> {
        c.lines.iter().filter(|l| l.clause.contains(clause)).collect()
    }

    #[test]
    fn stopping_sight_distances_are_those_of_cd_109_table_2_10() {
        // Read from CD 109 Version 1.1.0, Table 2.10, row "One step below desirable minimum".
        for (v, d) in [(120.0, 215.0), (100.0, 160.0), (85.0, 120.0), (70.0, 90.0), (60.0, 70.0), (50.0, 50.0)] {
            assert_eq!(cd109_ssd_one_step_below_m(v), Some(d));
        }
        // Row "Desirable minimum", and the step structure of the table: each one-step-below
        // value is the desirable minimum of the next design speed down.
        assert_eq!(CD109_TABLE_2_10_SSD.map(|r| r.1), [295.0, 215.0, 160.0, 120.0, 90.0, 70.0]);
        for w in CD109_TABLE_2_10_SSD.windows(2) {
            assert_eq!(w[0].2, w[1].1);
        }
        // No interpolation and no extrapolation.
        assert_eq!(cd109_ssd_one_step_below_m(80.0), None);
        assert_eq!(cd109_ssd_one_step_below_m(40.0), None);
    }

    #[test]
    fn mowt_values_are_those_of_section_12_8() {
        assert_eq!(MOWT_MIN_MOUNTING_HEIGHT_M, 9.0);
        assert_eq!(MOWT_MEDIAN_MOUNTING_HEIGHT_M, (12.0, 15.0));
        assert_eq!(mowt_setback_behind_kerb_m(30.0), Some((0.5, false)));
        assert_eq!(mowt_setback_behind_kerb_m(50.0), Some((0.5, false)));
        assert_eq!(mowt_setback_behind_kerb_m(60.0), None);
        assert_eq!(mowt_setback_behind_kerb_m(80.0), Some((1.2, true)));
        assert_eq!(mowt_setback_behind_kerb_m(110.0), Some((1.2, true)));
    }

    #[test]
    fn with_nothing_entered_the_report_asks_and_assumes_nothing() {
        let c = check_lighting(&data(Arrangement::SingleRight), &CheckInputs::default());
        // Tilt zero and references are checkable from the element alone; the MoWT height too.
        assert!(c.passes());
        assert_eq!(find(&c, "cl. 2.5")[0].outcome, Outcome::Ok);
        assert_eq!(find(&c, "6.49")[0].outcome, Outcome::Ok);
        for clause in ["cl. 1.1", "2.5, 6.54", "cl. 3.14", "1.4, 3.15", "cl. 3.4", "cl. 3.5", "cl. 3.8", "cl. 3.11", "6.53"] {
            assert!(find(&c, clause).iter().any(|l| l.outcome == Outcome::InputNeeded), "{clause}");
        }
        assert_eq!(c.count(Outcome::Fails), 0);
        let text = c.calculation();
        assert!(text[1].contains("TD 501 Version 0.1.0"));
        assert!(text.iter().any(|l| l.contains("no photometric calculation")));
        assert!(text.last().unwrap().contains("Not a certified design"));
        assert!(c.notes().iter().all(|n| n.severity == Severity::Warning));
    }

    #[test]
    fn a_fully_entered_installation() {
        let mut d = data(Arrangement::SingleRight);
        if let LightingDefinition::Installation(i) = &mut d.definition {
            let l = &mut i.column.luminaire;
            l.luminous_intensity_class = Some("g4".to_string());
            l.led = Some(true);
            l.risk_group = Some("RG1".to_string());
            l.cct_k = Some(3000.0);
            l.cri = Some(70.0);
            l.mass_kg = Some(9.5);
            l.windage_m2 = Some(0.06);
        }
        let inputs = CheckInputs {
            appraisal_done: Some(true),
            design_speed_kph: Some(100.0),
            unlit_gaps: vec![UnlitGap { description: "north".to_string(), length_m: 640.0 }, UnlitGap { description: "south".to_string(), length_m: 639.0 }],
            required_setback_m: Some(1.5),
            required_setback_source: "CD 377 RRRAP output ref 12".to_string(),
            passively_safe: Some(false),
            speed_limit_mph: Some(60.0),
            aadt: Some(12000.0),
            maintenance_factor: Some(0.85),
            ..Default::default()
        };
        let c = check_lighting(&d, &inputs);
        // 4 × 160 = 640 m: a 640 m gap is allowed, a 639 m one is not.
        let gaps = find(&c, "cl. 3.11");
        assert_eq!(gaps.len(), 2);
        assert_eq!((gaps[0].outcome, gaps[1].outcome), (Outcome::Ok, Outcome::Fails));
        assert!(gaps[0].finding.contains("SSD = 160 m") && gaps[0].finding.contains("= 640 m"));
        assert!(!c.passes());
        assert_eq!(c.count(Outcome::Fails), 1);
        assert_eq!(c.count(Outcome::InputNeeded), 0);
        assert_eq!(find(&c, "cl. 3.4;")[0].outcome, Outcome::Information);
        assert!(find(&c, "cl. 3.4;")[0].finding.contains("cl. 2.3 applies"));
        assert!(find(&c, "cl. 3.5")[0].finding.contains("item 16"));
        let setback = c.lines.iter().find(|l| l.clause.ends_with("cl. 3.4")).unwrap();
        assert_eq!(setback.outcome, Outcome::Ok);
        assert!(setback.finding.contains("RRRAP output ref 12"));
        // 7 luminaires of 100 W: 700 W, 840 W with 20 %.
        let supply = find(&c, "5.7, 5.8")[0];
        assert!(supply.finding.contains("= 700 W") && supply.finding.contains("= 840 W"));
        // MoWT: 10 m ≥ 9 m and 1.5 m ≥ 1.2 m at 100 km/h.
        let mowt = find(&c, "MoWT");
        assert_eq!((mowt[0].outcome, mowt[1].outcome), (Outcome::Ok, Outcome::Ok));
        assert!(c.notes().iter().any(|n| n.severity == Severity::Error && n.message.contains("south")));
    }

    #[test]
    fn failures_and_local_differences() {
        let mut d = data(Arrangement::TwinCentral);
        if let LightingDefinition::Installation(i) = &mut d.definition {
            i.column.tilt_deg = 5.0;
            i.column.mounting_height_m = 8.0;
            i.lighting_class = String::new();
            i.column.luminaire.luminous_intensity_class = Some("G2".to_string());
            i.column.luminaire.led = Some(false);
            i.column.luminaire.risk_group = Some("RG2".to_string());
        }
        d.layout.columns[1].mark = d.layout.columns[0].mark.clone();
        let inputs = CheckInputs {
            appraisal_done: Some(false),
            required_setback_m: Some(2.0),
            stopping_sight_distance_m: Some(100.0),
            design_speed_kph: Some(80.0),
            speed_limit_mph: Some(30.0),
            aadt: Some(20000.0),
            ..Default::default()
        };
        let c = check_lighting(&d, &inputs);
        for clause in ["cl. 1.1", "6.49", "cl. 2.5", "2.5, 6.54", "cl. 3.14", "1.4, 3.15"] {
            assert!(find(&c, clause).iter().any(|l| l.outcome == Outcome::Fails), "{clause}");
        }
        // Set-back 1.5 m against 2.0 m entered.
        assert_eq!(c.lines.iter().find(|l| l.clause.ends_with("cl. 3.4")).unwrap().outcome, Outcome::Fails);
        assert_eq!(find(&c, "3.6, 6.48")[0].outcome, Outcome::InputNeeded);
        assert!(find(&c, "cl. 3.4;")[0].finding.contains("cl. 2.2 applies"));
        // The entered SSD is used and no gap is entered: 400 m for information.
        let gap = find(&c, "cl. 3.11")[0];
        assert_eq!(gap.outcome, Outcome::Information);
        assert!(gap.finding.contains("= 400 m") && gap.finding.contains("as entered"));
        // MoWT: 8 m is under 9 m and outside 12 to 15 m; these differ but do not fail.
        let mowt = find(&c, "MoWT");
        assert_eq!((mowt[0].outcome, mowt[1].outcome), (Outcome::Differs, Outcome::Differs));
        assert_eq!(c.count(Outcome::Differs), 3);
        assert!(c.calculation().iter().any(|l| l.contains("does not govern")));

        // A design speed between the table's columns asks for the SSD.
        let c = check_lighting(&data(Arrangement::SingleLeft), &CheckInputs { design_speed_kph: Some(65.0), ..Default::default() });
        assert_eq!(find(&c, "cl. 3.11")[0].outcome, Outcome::InputNeeded);
        assert!(find(&c, "MoWT")[1].finding.contains("gives no distance"));
    }

    #[test]
    fn bs_5489_1_2003_and_tii_lines() {
        let mut d = data(Arrangement::SingleRight);
        if let LightingDefinition::Installation(i) = &mut d.definition {
            i.lighting_class = "ME3a".to_string();
            i.setback_m = 1.2;
            i.column.bracket_outreach_m = 2.6;
            i.column.tilt_deg = 5.0;
            i.column.luminaire.luminous_intensity_class = Some("G6".to_string());
        }
        let mut inputs = CheckInputs {
            design_speed_kph: Some(100.0),
            road_type: Some(RoadType::MainDistributorSingle),
            class_adt: Some(12_000.0),
            luminaire_ip: Some(IpMinimum::Ip6x),
            pollution: Some(Pollution::Medium),
            cleaning_interval_months: Some(36),
            lamp_flux_maintenance_factor: Some(0.9),
            maintenance_factor: Some(0.78),
            ..Default::default()
        };
        let c = check_lighting(&d, &inputs);
        let text = c.calculation();
        assert!(text[2].starts_with("WARNING: BS 5489-1:2003 is a superseded edition"));
        // Table B.2: main distributor at 12 000 ADT is ME3a, as entered.
        let class = find(&c, "Table B.2")[0];
        assert_eq!(class.outcome, Outcome::Ok);
        assert!(class.finding.contains("ME3a") && class.finding.contains("superseded edition"));
        // Table 2: 1.5 m at 100 km/h against a 1.2 m set-back.
        let clearance = find(&c, "2003 Table 2")[0];
        assert_eq!(clearance.outcome, Outcome::Fails);
        assert!(clearance.finding.contains("100 km/h: 1.5 m"));
        // The luminaire overhangs the carriageway by 1.4 m: 10 m clears 5.7 m.
        assert_eq!(find(&c, "cl. 5.2.1.2")[0].outcome, Outcome::Ok);
        // A quarter of 10 m is 2.5 m: 2.6 m differs, as it does from TII's 2 m.
        assert_eq!(find(&c, "cl. 5.2.3.3")[0].outcome, Outcome::Differs);
        let tii = find(&c, "TII");
        assert_eq!((tii[0].outcome, tii[1].outcome, tii[2].outcome), (Outcome::Differs, Outcome::Ok, Outcome::Ok));
        assert!(tii[0].finding.contains("against 2 m"));
        // Table D.1: IP6X, medium pollution, 36 months is 0.87; 0.9 × 0.87 = 0.783.
        let mf = find(&c, "Table D.1")[0];
        assert_eq!(mf.outcome, Outcome::Information);
        assert!(mf.finding.contains(": 0.87;") && mf.finding.contains("0.9 × 0.87 = 0.783") && mf.finding.contains("0.78"));
        // TD 501 still fails the 5° tilt that TII allows.
        assert_eq!(find(&c, "cl. 2.5")[0].outcome, Outcome::Fails);

        // A different class differs without failing; missing flow and odd intervals ask.
        inputs.class_adt = Some(20_000.0);
        inputs.cleaning_interval_months = Some(30);
        inputs.design_speed_kph = Some(60.0);
        inputs.conflict_approach_class = Some("ME2".to_string());
        let c = check_lighting(&d, &inputs);
        assert_eq!(find(&c, "Table B.2")[0].outcome, Outcome::Differs);
        assert!(find(&c, "Table B.2")[0].finding.contains("ME2"));
        assert_eq!(find(&c, "Table D.1")[0].outcome, Outcome::InputNeeded);
        assert_eq!(find(&c, "2003 Table 2")[0].outcome, Outcome::InputNeeded);
        let conflict = find(&c, "Table B.3")[0];
        assert_eq!(conflict.outcome, Outcome::Differs);
        assert!(conflict.finding.contains("CE1"));
        inputs.road_type = Some(RoadType::StrategicDual);
        inputs.class_adt = Some(15_000.0);
        assert!(find(&check_lighting(&d, &inputs), "Table B.2")[0].finding.contains("no entry for exactly 15 000"));
        inputs.class_adt = None;
        assert_eq!(find(&check_lighting(&d, &inputs), "Table B.2")[0].outcome, Outcome::InputNeeded);

        // A low luminaire over the carriageway fails the 5.7 m clearance; a set-back one is not checked.
        if let LightingDefinition::Installation(i) = &mut d.definition {
            i.column.mounting_height_m = 5.0;
        }
        inputs.design_speed_kph = Some(50.0);
        assert_eq!(find(&check_lighting(&d, &inputs), "cl. 5.2.1.2")[0].outcome, Outcome::Fails);
        if let LightingDefinition::Installation(i) = &mut d.definition {
            i.setback_m = 3.0;
            i.column.bracket_outreach_m = 1.0;
        }
        assert!(find(&check_lighting(&d, &inputs), "cl. 5.2.1.2").is_empty());
    }
}
