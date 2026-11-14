//! Requirement checks for a permanent safety barrier run.
//!
//! # Basis
//!
//! * **CD 377** = DMRB CD 377 *Requirements for road restraint systems*, Version 5.0.1
//!   (National Highways, published 27 February 2026), core document. It applies BS EN 1317.
//! * **BS EN 1317-2:2010** for the class tables (see the `classes` module source) and
//!   **DD ENV 1317-4:2002** for the ordering of terminal classes P1 to P4.
//! * **CD 127** = DMRB CD 127 *Cross-sections and headrooms*, Version 1.0.1 (29 July 2021),
//!   which CD 377 clause 2.37 makes the source of set-back for permanent barriers.
//! * **MoWT** = Uganda Ministry of Works and Transport, *Road Design Manual Volume 1: Geometric
//!   Design* (January 2010), Section 12.3. A local reference only: its figures are reported
//!   beside the UK ones when asked for and never decide a result. UK governs.
//!
//! # What is checked, and from which clause
//!
//! | Check | Clause |
//! |---|---|
//! | Minimum containment level by speed limit: N2 at 50 mph or more, N1 below; levels compared by the notes to BS EN 1317-2:2010 Table 2 | CD 377 3.6, 3.7 |
//! | A higher level found by the site specific risk assessment (entered, not computed) | CD 377 3.9 |
//! | Minimum containment in front of a structural support: N2; H4a or H1 for an existing support in CS 453 Group 1 or 2 | CD 377 3.47, Table 3.48 |
//! | Minimum containment at each end of a vehicle parapet: N2 | CD 377 3.64 |
//! | Impact severity level A or B | CD 377 3.12, 3.12.1 |
//! | No hazard within the working width class; widest class the clearance allows | CD 377 3.13, 3.14, 3.15; BS EN 1317-2:2010 Table 4 |
//! | For higher and very high containment, no hazard within the vehicle intrusion class | CD 377 3.13, 3.17; BS EN 1317-2:2010 Table 5 |
//! | Crest or toe of a slope over 200 mm high not within the working width class | CD 377 3.39, 3.41 |
//! | Set-back against the desirable minimum and the relaxations | CD 127 Table 2.24, 2.25, 2.26 |
//! | Rate of change of set-back (taper, flare) | CD 377 2.38, 2.39 |
//! | Length of need and minimum total length, as arithmetic on entered lengths | CD 377 3.24 to 3.31, Figure 3.24, Appendix C |
//! | Terminal performance class and energy absorption by speed limit and direction | CD 377 6.1, 6.3 to 6.5, 6.7 |
//! | A transition where the run joins a parapet or a structure; its containment and ISL | CD 377 3.59, 7.2, 7.10 to 7.12 |
//! | Gap to the next barrier installation | CD 377 3.35, 3.36 |
//!
//! # What this is not
//!
//! **It is not the risk assessment.** CD 377 2.3 requires the Road Restraint Risk Assessment
//! Process (RRRAP) on motorways and all-purpose trunk roads of 50 mph or more and 5,000 AADT or
//! more, and a local risk assessment below that (2.2). Whether a barrier is needed at all, the
//! lengths in advance of and beyond a hazard (3.26, 3.27) and any containment level above the
//! minimum (3.9) come out of that process. Version 5 of CD 377 gives **no fixed lengths**: the
//! lengths B to C and D to E are inputs here, and nothing in this module imitates or replaces
//! the RRRAP.
//!
//! Also not done: new structural supports (3.49 refers to PD 6688-1-7,
//! not held); central reserve provision and the national annexes (3.77 to 3.88, E/3.4 etc.);
//! where a hazard sits within a run (only lengths are compared); terminal lateral displacement
//! and exit box classes (6.15 to 6.19); everything the clauses leave to "the manufacturer's
//! product description and associated installation instructions".
//!
//! # Inputs, not defaults
//!
//! The speed limit, the lengths from the risk assessment, clearances, the set-back, any
//! manufacturer's declared values and the risk-assessed containment level are all entered. A
//! check whose input is absent is reported as NOT CHECKED, never assumed.
//!
//! The result is a list of checks for an engineer to review, not a certified design.

use crate::{
    fixed, BarrierRun, ConnectedSystem, ContainmentGroup, ContainmentLevel, Coverage, EndTreatment, ImpactSeverityLevel, ResolvedRun,
    TerminalClass, WorkingWidthClass,
};

/// Kilometres in a mile: the definition of the international mile (1 yard = 0.9144 m exactly).
pub const KM_PER_MILE: f64 = 1.609344;

/// The speed limit at which CD 377 steps its requirements, mph ("a speed limit of 50 mph or
/// more": clauses 3.6, 6.3, 6.4, 6.7; "less than 50 mph": 3.7, 6.5).
pub const SPEED_THRESHOLD_MPH: f64 = 50.0;

/// The two-way flow below which CD 377 calls for a local risk assessment instead of its full
/// application, annual average daily traffic (clauses 1.3, 2.2, 2.3).
pub const AADT_THRESHOLD: f64 = 5000.0;

/// A speed limit in the unit it is posted in.
///
/// CD 377 is written in miles per hour. A limit in km/h is converted exactly, which puts a
/// posted 80 km/h (49.7 mph) **below** the 50 mph step. Whether a metric 80 km/h road should be
/// treated as the 50 mph class is a policy question for the road authority; the report flags
/// it instead of deciding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpeedLimit {
    Mph(f64),
    Kmh(f64),
}

impl SpeedLimit {
    pub fn mph(self) -> f64 {
        match self {
            SpeedLimit::Mph(v) => v,
            SpeedLimit::Kmh(v) => v / KM_PER_MILE,
        }
    }

    /// "50 mph or more" (with 1e-9 of grace so that 80.4672 km/h is exactly 50 mph).
    pub fn is_50_mph_or_more(self) -> bool {
        self.mph() >= SPEED_THRESHOLD_MPH - 1e-9
    }

    pub fn label(self) -> String {
        match self {
            SpeedLimit::Mph(v) => format!("{} mph", fixed(v, 0, 1)),
            SpeedLimit::Kmh(v) => format!("{} km/h ({} mph)", fixed(v, 0, 1), fixed(self.mph(), 1, 1)),
        }
    }
}

/// Minimum containment level of a permanent safety barrier from the speed limit alone:
/// N2 at 50 mph or more (CD 377 3.6), N1 below (CD 377 3.7). A site specific risk assessment
/// may require more (3.6 to 3.9).
pub fn minimum_containment(speed: SpeedLimit) -> ContainmentLevel {
    if speed.is_50_mph_or_more() {
        ContainmentLevel::N2
    } else {
        ContainmentLevel::N1
    }
}

/// Categorisation of an existing structural support according to CS 453, as CD 377 uses it
/// (Table 3.48: "Group 1a or 1b (particularly at risk)", "Group 2 (at risk)"; 7.11: "Group 3
/// (low risk)"). The categorisation itself is an input from the CS 453 assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cs453Group {
    /// Group 1a or 1b.
    Group1,
    Group2,
    Group3,
}

/// A structural support the barrier stands in front of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralSupport {
    /// An existing support, with its CS 453 group when it has been assessed.
    Existing { cs453_group: Option<Cs453Group> },
    /// A new support designed to BS EN 1991-1-7: CD 377 3.49 sends the containment level to
    /// PD 6688-1-7, which is not held here.
    New,
}

/// Minimum containment level of a safety barrier in front of an existing structural support:
/// H4a for CS 453 Group 1a or 1b, H1 for Group 2 (CD 377 Table 3.48); otherwise the N2 of
/// clause 3.47, which applies in front of any structural support.
pub fn minimum_containment_at_support(group: Option<Cs453Group>) -> ContainmentLevel {
    match group {
        Some(Cs453Group::Group1) => ContainmentLevel::H4a,
        Some(Cs453Group::Group2) => ContainmentLevel::H1,
        Some(Cs453Group::Group3) | None => ContainmentLevel::N2,
    }
}

/// Where the barrier stands, in the terms of CD 127 Table 2.24.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetBackLocation {
    /// "In verges with no adjacent hard strip or hard shoulder".
    VergeWithoutHardStrip,
    /// "In verges with an adjacent hard strip or hard shoulder".
    VergeWithHardStrip,
    CentralReserve,
}

impl SetBackLocation {
    /// Desirable minimum set-back, m. CD 127 Table 2.24: 1200 mm in verges with no adjacent
    /// hard strip or hard shoulder, 600 mm in verges with one, 1200 mm in central reserves.
    pub fn desirable_minimum_m(self) -> f64 {
        match self {
            SetBackLocation::VergeWithoutHardStrip | SetBackLocation::CentralReserve => 1.2,
            SetBackLocation::VergeWithHardStrip => 0.6,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SetBackLocation::VergeWithoutHardStrip => "verge with no adjacent hard strip or hard shoulder",
            SetBackLocation::VergeWithHardStrip => "verge with an adjacent hard strip or hard shoulder",
            SetBackLocation::CentralReserve => "central reserve",
        }
    }
}

/// CD 127 Table 2.24 Note 1: relaxation to 600 mm "for roads of speed limit 50mph or less"
/// (verges without a hard strip, and central reserves), m.
pub const SET_BACK_RELAXATION_LOW_SPEED_M: f64 = 0.6;
/// CD 127 Table 2.24 Note 2: relaxation to 1000 mm "at existing roads with physical constraints
/// (e.g. a structure)" (verges without a hard strip, and central reserves), m.
pub const SET_BACK_RELAXATION_CONSTRAINT_M: f64 = 1.0;
/// CD 127 Table 2.24 Note 3: relaxation to 450 mm "where it is considered necessary to position
/// the VRS away from the edge of an existing embankment in order to provide support to the
/// foundation" (verges with a hard strip or hard shoulder), m.
pub const SET_BACK_RELAXATION_EMBANKMENT_M: f64 = 0.45;

/// How a set-back stands against CD 127 Table 2.24.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SetBackOutcome {
    /// At or above the desirable minimum.
    Desirable,
    /// Below the desirable minimum but within the relaxation of the numbered note.
    Relaxed { to_m: f64, note: u8 },
    /// Below everything that applies; `least_m` is the smallest value that would be accepted.
    Below { least_m: f64 },
}

/// Set-back against CD 127 Table 2.24 and its notes.
///
/// * `speed`: Note 1 applies at "50mph or less" (so at exactly 50 mph both the N2 of CD 377 3.6
///   and this relaxation apply).
/// * `existing_road_with_physical_constraint`: the engineer's statement that Note 2 applies.
/// * `away_from_existing_embankment_edge`: the engineer's statement that Note 3 applies.
///
/// A relaxation is permitted by the table, not automatic: the report marks it for the
/// engineer's justification (CD 127 2.25 NOTE 2 lists what to weigh).
pub fn check_set_back(
    location: SetBackLocation,
    set_back_m: f64,
    speed: SpeedLimit,
    existing_road_with_physical_constraint: bool,
    away_from_existing_embankment_edge: bool,
) -> SetBackOutcome {
    const TOL: f64 = 1e-9;
    if set_back_m >= location.desirable_minimum_m() - TOL {
        return SetBackOutcome::Desirable;
    }
    let mut relaxations: Vec<(f64, u8)> = Vec::new();
    match location {
        SetBackLocation::VergeWithoutHardStrip | SetBackLocation::CentralReserve => {
            if existing_road_with_physical_constraint {
                relaxations.push((SET_BACK_RELAXATION_CONSTRAINT_M, 2));
            }
            if speed.mph() <= SPEED_THRESHOLD_MPH + TOL {
                relaxations.push((SET_BACK_RELAXATION_LOW_SPEED_M, 1));
            }
        }
        SetBackLocation::VergeWithHardStrip => {
            if away_from_existing_embankment_edge {
                relaxations.push((SET_BACK_RELAXATION_EMBANKMENT_M, 3));
            }
        }
    }
    // The first relaxation the set-back satisfies, trying the least relaxed first.
    relaxations.sort_by(|a, b| b.0.total_cmp(&a.0));
    match relaxations.iter().find(|r| set_back_m >= r.0 - TOL) {
        Some(&(to_m, note)) => SetBackOutcome::Relaxed { to_m, note },
        None => SetBackOutcome::Below { least_m: relaxations.last().map_or(location.desirable_minimum_m(), |r| r.0) },
    }
}

/// CD 127 2.26: the sole double sided deformable barrier of an unobstructed central reserve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoubleSidedDeformable {
    /// "The working width of the safety barrier", m, from the product declaration.
    pub working_width_m: f64,
    /// "The actual width of the safety barrier", m.
    pub barrier_width_m: f64,
}

/// What the set-back check needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetBackInput {
    pub location: SetBackLocation,
    pub existing_road_with_physical_constraint: bool,
    pub away_from_existing_embankment_edge: bool,
    pub double_sided_deformable: Option<DoubleSidedDeformable>,
}

/// What limits the rate of change of set-back.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaperBasis {
    /// The rate "declared in the manufacturer's product description and associated installation
    /// instructions" (CD 377 2.38), as 1 in `one_in`.
    ManufacturerDeclared { one_in: f64 },
    /// A non-proprietary deformable safety barrier: 1 in 16 (CD 377 2.39).
    NonProprietaryDeformable,
    /// A non-proprietary rigid safety barrier: 1 in 20 (CD 377 2.39).
    NonProprietaryRigid,
    /// A proprietary system whose declared rate has not been entered: cannot be checked.
    ProprietaryNotDeclared,
}

/// CD 377 2.39: greatest rate of change of set-back for a non-proprietary deformable safety
/// barrier, as 1 in this.
pub const TAPER_NON_PROPRIETARY_DEFORMABLE_ONE_IN: f64 = 16.0;
/// CD 377 2.39: the same for a non-proprietary rigid safety barrier.
pub const TAPER_NON_PROPRIETARY_RIGID_ONE_IN: f64 = 20.0;

impl TaperBasis {
    /// The limit as "1 in n": the taper must be no steeper. `None` when it is not known.
    pub fn limit_one_in(self) -> Option<f64> {
        match self {
            TaperBasis::ManufacturerDeclared { one_in } => Some(one_in),
            TaperBasis::NonProprietaryDeformable => Some(TAPER_NON_PROPRIETARY_DEFORMABLE_ONE_IN),
            TaperBasis::NonProprietaryRigid => Some(TAPER_NON_PROPRIETARY_RIGID_ONE_IN),
            TaperBasis::ProprietaryNotDeclared => None,
        }
    }
}

/// A change of set-back along the barrier (a taper or flare).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaperInput {
    /// Change in set-back, m.
    pub change_in_set_back_m: f64,
    /// Length along the road over which it happens, m.
    pub length_m: f64,
    pub basis: TaperBasis,
}

impl TaperInput {
    /// The taper as "1 in n": length over change. Infinite when the set-back does not change.
    pub fn one_in(&self) -> f64 {
        if self.change_in_set_back_m.abs() < 1e-12 {
            f64::INFINITY
        } else {
            self.length_m / self.change_in_set_back_m.abs()
        }
    }
}

/// The lengths of CD 377 Figure 3.24. All of them are inputs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LengthOfNeedInput {
    /// B to C: minimum length of full containment barrier in advance of the hazard, from the
    /// RRRAP (CD 377 3.26), m.
    pub advance_m: f64,
    /// C to D: length of the hazard(s), m.
    pub hazard_m: f64,
    /// D to E: minimum length of full containment barrier beyond the hazard, from the RRRAP
    /// (CD 377 3.27), m.
    pub beyond_m: f64,
    /// A length of need the designer adopts above the sum, for local factors (CD 377 3.28;
    /// Appendix C Example 2), m.
    pub adopted_length_of_need_m: Option<f64>,
    /// A to B: additional length declared by the manufacturer to reach full containment at B
    /// (CD 377 3.29), m.
    pub approach_to_full_containment_m: f64,
    /// E to F: the same after the length of need, m.
    pub departure_to_full_containment_m: f64,
    /// A minimum length of need declared by the manufacturer, if any (CD 377 3.31), m.
    pub manufacturer_min_length_of_need_m: Option<f64>,
    /// A minimum total length declared by the manufacturer, if any (CD 377 3.31), m.
    pub manufacturer_min_total_length_m: Option<f64>,
}

/// The lengths worked out from a [`LengthOfNeedInput`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LengthOfNeed {
    /// B to C + C to D + D to E (CD 377 3.25), m.
    pub sum_m: f64,
    /// The length of need used: the largest of the sum, the adopted value and the
    /// manufacturer's minimum (CD 377 3.28, 3.31), m.
    pub length_of_need_m: f64,
    /// A to F: the length of need plus the two additional lengths, or the manufacturer's
    /// minimum total if longer (CD 377 3.29, 3.31), m.
    pub total_m: f64,
    /// False when the adopted length of need is less than the sum, which 3.25 does not allow.
    pub adopted_is_sufficient: bool,
}

impl LengthOfNeedInput {
    pub fn lengths(&self) -> LengthOfNeed {
        let sum_m = self.advance_m + self.hazard_m + self.beyond_m;
        let adopted_is_sufficient = self.adopted_length_of_need_m.is_none_or(|a| a >= sum_m - 1e-9);
        let length_of_need_m = [self.adopted_length_of_need_m, self.manufacturer_min_length_of_need_m].into_iter().flatten().fold(sum_m, f64::max);
        let total = self.approach_to_full_containment_m + length_of_need_m + self.departure_to_full_containment_m;
        LengthOfNeed { sum_m, length_of_need_m, total_m: self.manufacturer_min_total_length_m.map_or(total, |m| m.max(total)), adopted_is_sufficient }
    }
}

/// How a terminal's class stands against CD 377 6.3 to 6.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalVerdict {
    /// One of the classes the clause names.
    Meets,
    /// P2, P3 or P4 where the clause asks for a minimum of P1: a higher class is "considered as
    /// having met conditions of lower classes" (DD ENV 1317-4:2002, 5.1).
    MeetsAsHigherPClass,
    /// T110 or T110/1 where the clause asks for a minimum of T80/3. PD CEN/TS 1317-7, which
    /// orders the T classes, is not held; they are taken as above the minimum because CD 377
    /// 6.3 requires them for the more onerous case. An assumption, stated on the report line.
    MeetsAsHigherTClass,
    Fails,
    /// A class neither CD 377 nor DD ENV 1317-4:2002 names: compare it against PD CEN/TS 1317-7.
    NotNamed,
}

/// Terminal performance class against the speed limit and whether the terminal faces oncoming
/// traffic.
///
/// * 50 mph or more, facing oncoming traffic: T110, T110/1 or P4 (CD 377 6.3).
/// * 50 mph or more, not facing oncoming traffic: minimum T80/3 or P1 (CD 377 6.4).
/// * Less than 50 mph: minimum T80/3 or P1 (CD 377 6.5).
pub fn check_terminal_class(class: &TerminalClass, speed: SpeedLimit, faces_oncoming_traffic: bool) -> TerminalVerdict {
    let onerous = speed.is_50_mph_or_more() && faces_oncoming_traffic;
    match class {
        TerminalClass::Other(_) => TerminalVerdict::NotNamed,
        TerminalClass::T110 | TerminalClass::T110_1 if onerous => TerminalVerdict::Meets,
        TerminalClass::T110 | TerminalClass::T110_1 => TerminalVerdict::MeetsAsHigherTClass,
        TerminalClass::T80_3 if onerous => TerminalVerdict::Fails,
        TerminalClass::T80_3 => TerminalVerdict::Meets,
        p => match (p.p_number(), onerous) {
            (Some(4), true) | (Some(1), false) => TerminalVerdict::Meets,
            (Some(_), true) => TerminalVerdict::Fails,
            (Some(_), false) => TerminalVerdict::MeetsAsHigherPClass,
            (None, _) => TerminalVerdict::NotNamed,
        },
    }
}

/// CD 377 3.35: a gap of this length or less between adjacent safety barrier installations
/// "shall be closed" (with the exceptions the clause lists), m.
pub const GAP_CLOSE_M: f64 = 50.0;
/// CD 377 3.36: a gap over 50 m and up to this "shall be closed" unless there are significant
/// cost or technical requirements (and the same exceptions), m.
pub const GAP_CLOSE_UNLESS_COST_M: f64 = 100.0;

/// MoWT Geometric Design Manual (January 2010) 12.3.4: "Generally you need at least 30 m of
/// steel beam strong post guardrail for it to perform satisfactorily", m. Local reference.
pub const MOWT_MIN_GUARDRAIL_LENGTH_M: f64 = 30.0;
/// MoWT 12.3.6: "Set the guardrail back from the shoulder edge (or carriageway edge if there is
/// no shoulder) by at least 600 mm", m. Local reference.
pub const MOWT_MIN_SET_BACK_M: f64 = 0.6;
/// MoWT 12.3.7: "Do not leave short (<80m) gaps in guardrail", m. Local reference.
pub const MOWT_SHORT_GAP_M: f64 = 80.0;

/// What is known about one end of the run beyond the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EndContext {
    /// A terminal here "faces oncoming traffic" (CD 377 6.3; on a two-way single carriageway
    /// both ends do).
    pub faces_oncoming_traffic: bool,
    /// A transition here connects into an existing structure in CS 453 Group 3, or a new
    /// structure designed for main and residual loads to BS EN 1991-1-7 (CD 377 7.11, 7.12).
    pub into_impact_resistant_structure: bool,
}

/// Everything the checks need beyond the run itself. `None` means "not entered": the check is
/// reported as not made.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignInput {
    pub speed_limit: SpeedLimit,
    /// Two-way annual average daily traffic, when known.
    pub two_way_aadt: Option<f64>,
    /// The containment level the site specific risk assessment found necessary, when it is
    /// above the minimum (CD 377 3.9).
    pub risk_assessed_containment: Option<ContainmentLevel>,
    /// The barrier stands in front of a structural support.
    pub structural_support: Option<StructuralSupport>,
    /// The barrier is the one at an end of a vehicle parapet (CD 377 3.64).
    pub at_parapet_end: bool,
    /// Distance from the traffic face of the barrier to the nearest hazard behind it, m.
    pub hazard_clearance_m: Option<f64>,
    /// Distance from the traffic face to the crest or toe of a slope more than 200 mm high, m.
    pub slope_clearance_m: Option<f64>,
    /// Set-back check; uses [`BarrierRun::set_back_m`].
    pub set_back: Option<SetBackInput>,
    pub taper: Option<TaperInput>,
    pub length_of_need: Option<LengthOfNeedInput>,
    pub start: EndContext,
    pub end: EndContext,
    /// Gap to the nearest adjacent barrier installation, terminal to terminal, m.
    pub gap_to_adjacent_barrier_m: Option<f64>,
    /// Add the MoWT local reference lines.
    pub include_mowt_reference: bool,
}

impl DesignInput {
    /// Only the speed limit; every other check is left unmade until its input is entered.
    pub fn new(speed_limit: SpeedLimit) -> DesignInput {
        DesignInput {
            speed_limit,
            two_way_aadt: None,
            risk_assessed_containment: None,
            structural_support: None,
            at_parapet_end: false,
            hazard_clearance_m: None,
            slope_clearance_m: None,
            set_back: None,
            taper: None,
            length_of_need: None,
            start: EndContext::default(),
            end: EndContext::default(),
            gap_to_adjacent_barrier_m: None,
            include_mowt_reference: false,
        }
    }
}

/// The outcome of one line of the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The requirement is met.
    Ok,
    /// Met only by a relaxation the standard permits; the engineer justifies it.
    Relaxation,
    /// The requirement is not met.
    Fails,
    /// The clause has exceptions or needs judgement: the engineer decides.
    Review,
    /// An input is missing or the source is not held: nothing was checked.
    NotChecked,
    /// Information, advice ("should") or a local reference; decides nothing.
    Note,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::Relaxation => "OK BY RELAXATION",
            Status::Fails => "FAILS",
            Status::Review => "REVIEW",
            Status::NotChecked => "NOT CHECKED",
            Status::Note => "NOTE",
        }
    }
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckLine {
    /// Document and clause, e.g. `CD 377 3.6`.
    pub reference: String,
    pub text: String,
    pub status: Status,
}

/// The checks of one run.
#[derive(Debug, Clone, PartialEq)]
pub struct RestraintDesign {
    pub mark: String,
    pub lines: Vec<CheckLine>,
}

impl RestraintDesign {
    /// True when no line FAILS. It does not mean the barrier is adequate: lines may be NOT
    /// CHECKED or for REVIEW, and the risk assessment is outside this module.
    pub fn passes(&self) -> bool {
        !self.lines.iter().any(|l| l.status == Status::Fails)
    }

    pub fn count(&self, status: Status) -> usize {
        self.lines.iter().filter(|l| l.status == status).count()
    }

    /// The checks set out line by line with their references, for the engineer to check.
    pub fn calculation(&self) -> Vec<String> {
        let mut out = vec![
            format!("Safety barrier run {}: requirement checks to DMRB CD 377 Version 5.0.1 and CD 127 Version 1.0.1.", self.mark),
            "These checks do not perform or replace the Road Restraint Risk Assessment Process (RRRAP) or a local risk assessment (CD 377 2.2 to 2.6); lengths and any higher containment level from that assessment are inputs.".to_string(),
        ];
        out.extend(self.lines.iter().map(|l| format!("{}: {}: {}.", l.reference, l.text, l.status.label())));
        out.push(format!(
            "Summary: {} OK, {} by relaxation, {} FAILS, {} for review, {} not checked.",
            self.count(Status::Ok),
            self.count(Status::Relaxation),
            self.count(Status::Fails),
            self.count(Status::Review),
            self.count(Status::NotChecked)
        ));
        out.push("For an engineer to check; not a certified design.".to_string());
        out
    }
}

fn m(v: f64) -> String {
    format!("{} m", fixed(v, 1, 3))
}

/// Checks a run. `resolved` supplies the length along the barrier.
pub fn check_run(run: &BarrierRun, resolved: &ResolvedRun, input: &DesignInput) -> RestraintDesign {
    let mut lines: Vec<CheckLine> = Vec::new();
    let mut push = |reference: &str, text: String, status: Status| lines.push(CheckLine { reference: reference.to_string(), text, status });
    let speed = input.speed_limit;
    let fast = speed.is_50_mph_or_more();
    let band = if fast { "50 mph or more" } else { "less than 50 mph" };

    // Scope.
    if let SpeedLimit::Kmh(v) = speed {
        if !fast && v >= 80.0 - 1e-9 {
            push(
                "CD 377 3.6",
                format!("the limit of {} is just under the 50 mph (80.47 km/h) step; whether an 80 km/h road is to be treated as the 50 mph class is for the road authority", speed.label()),
                Status::Review,
            );
        }
    }
    let low_flow = input.two_way_aadt.is_some_and(|a| a < AADT_THRESHOLD);
    if !fast || low_flow {
        let flow = input.two_way_aadt.map(|a| format!(", {} AADT", fixed(a, 0, 0))).unwrap_or_default();
        push(
            "CD 377 2.2",
            format!("at {}{flow} (below 50 mph and/or 5,000 AADT) a local risk assessment determines whether a restraint system is necessary and its minimum performance", speed.label()),
            Status::Note,
        );
    }

    // Containment.
    let contain = |why: String, required: ContainmentLevel| -> (String, Status) {
        let (status, tail) = match run.containment.covers(required) {
            Coverage::Covers => (Status::Ok, ""),
            Coverage::Below => (Status::Fails, ""),
            // CD 377 itself asks for H1 or H4a where N2 is the general minimum (3.47 with Table
            // 3.48), so it counts an H level as above N2; the standard's note is reported.
            Coverage::HigherButNotIncluded if required == ContainmentLevel::N2 => (
                Status::Ok,
                " (CD 377 counts an H level as greater than N2; note that by BS EN 1317-2:2010 Table 2 NOTE 2 an H level does not include the N2 car test TB 32, which an L level does)",
            ),
            Coverage::HigherButNotIncluded => (Status::Review, " (a level further down BS EN 1317-2:2010 Table 2, but NOTE 2 to that table says it does not include the required one)"),
            Coverage::NotRanked => (Status::Review, " (BS EN 1317-2:2010 Table 2 NOTE 3 gives no hierarchy between the a and b levels)"),
        };
        (format!("{why}, minimum containment {required}; entered {}{tail}", run.containment), status)
    };
    macro_rules! containment {
        ($reference:expr, $why:expr, $required:expr) => {{
            let (text, status) = contain($why, $required);
            push($reference, text, status);
        }};
    }
    containment!(if fast { "CD 377 3.6" } else { "CD 377 3.7" }, format!("speed limit {} is {band}", speed.label()), minimum_containment(speed));
    if let Some(level) = input.risk_assessed_containment {
        containment!("CD 377 3.9", "the site specific risk assessment (entered) requires more than the minimum".into(), level);
    }
    match input.structural_support {
        Some(StructuralSupport::Existing { cs453_group }) => {
            containment!("CD 377 3.47", "in front of a structural support".into(), minimum_containment_at_support(None));
            match cs453_group {
                Some(g @ (Cs453Group::Group1 | Cs453Group::Group2)) => {
                    let name = if g == Cs453Group::Group1 { "Group 1a or 1b" } else { "Group 2" };
                    containment!("CD 377 Table 3.48", format!("existing structural support in CS 453 {name}"), minimum_containment_at_support(Some(g)));
                }
                Some(Cs453Group::Group3) => {}
                None => push("CD 377 3.48", "the existing structural support has no CS 453 group entered, so Table 3.48 is not applied".into(), Status::NotChecked),
            }
        }
        Some(StructuralSupport::New) => {
            containment!("CD 377 3.47", "in front of a structural support".into(), minimum_containment_at_support(None));
            push("CD 377 3.49", "new structural support: the minimum containment is to PD 6688-1-7, which this tool does not hold".into(), Status::NotChecked);
        }
        None => {}
    }
    if input.at_parapet_end {
        containment!("CD 377 3.64", "safety barrier at the end of a vehicle parapet".into(), ContainmentLevel::N2);
    }

    // Impact severity.
    match run.impact_severity {
        ImpactSeverityLevel::A => push("CD 377 3.12", "impact severity level shall be A or B; entered A".into(), Status::Ok),
        ImpactSeverityLevel::B => {
            push("CD 377 3.12", "impact severity level shall be A or B; entered B".into(), Status::Ok);
            push("CD 377 3.12.1", "ISL A should be selected over ISL B".into(), Status::Note);
        }
        ImpactSeverityLevel::C => push("CD 377 3.12", "impact severity level shall be A or B; entered C".into(), Status::Fails),
    }

    // Working width.
    let w = run.working_width;
    match input.hazard_clearance_m {
        Some(c) => {
            let ok = w.level_m() <= c + 1e-9;
            push(
                "CD 377 3.13",
                format!("no hazard within the working width class: {w} is up to {}, nearest hazard {} from the traffic face", m(w.level_m()), m(c)),
                if ok { Status::Ok } else { Status::Fails },
            );
            match WorkingWidthClass::widest_within(c) {
                Some(widest) => push("CD 377 3.14", format!("the widest class the clearance of {} allows is {widest}", m(c)), Status::Note),
                None => push(
                    "CD 377 3.15",
                    format!("the clearance of {} is less than 0.6 m, so no class fits and the value itself is the design output", m(c)),
                    Status::Note,
                ),
            }
        }
        None => push("CD 377 3.13", "no hazard clearance entered, so the working width class is not checked against a hazard".into(), Status::NotChecked),
    }
    if let Some(c) = input.slope_clearance_m {
        let ok = w.level_m() <= c + 1e-9;
        push(
            "CD 377 3.39, 3.41",
            format!(
                "crest or toe of a slope over 200 mm high not within the working width class (unless the manufacturer permits): {w} is up to {}, slope {} from the traffic face",
                m(w.level_m()),
                m(c)
            ),
            if ok { Status::Ok } else { Status::Fails },
        );
    }
    if matches!(run.containment.group(), ContainmentGroup::Higher | ContainmentGroup::VeryHigh) {
        match (run.vehicle_intrusion, input.hazard_clearance_m) {
            (Some(vi), Some(c)) => match vi.level_m() {
                Some(level) => push(
                    "CD 377 3.13, 3.17",
                    format!("{} barrier: no hazard within the vehicle intrusion class: {vi} is up to {}, nearest hazard {} from the traffic face", run.containment, m(level), m(c)),
                    if level <= c + 1e-9 { Status::Ok } else { Status::Fails },
                ),
                None => push(
                    "CD 377 3.13, 3.17",
                    format!("{} barrier: {vi} is more than 3.5 m with no upper bound (BS EN 1317-2:2010 Table 5); compare the declared normalised vehicle intrusion with the clearance of {}", run.containment, m(c)),
                    Status::Review,
                ),
            },
            (None, _) => push(
                "CD 377 3.13, 3.17",
                format!("{} is a higher or very high containment level: the vehicle intrusion class also applies and none is entered", run.containment),
                Status::NotChecked,
            ),
            (Some(_), None) => {}
        }
    }
    if run.containment.group() == ContainmentGroup::LowAngle {
        push(
            "BS EN 1317-2:2010 Table 2 NOTE 1",
            format!("{} is a low angle containment level, intended only for temporary safety barriers; these checks are for permanent barriers", run.containment),
            Status::Review,
        );
    }

    // Set-back.
    match (&input.set_back, run.set_back_m) {
        (Some(sb), Some(s)) => {
            let desirable = sb.location.desirable_minimum_m();
            let head = format!("{}: desirable minimum set-back {}, entered {}", sb.location.label(), m(desirable), m(s));
            match check_set_back(sb.location, s, speed, sb.existing_road_with_physical_constraint, sb.away_from_existing_embankment_edge) {
                SetBackOutcome::Desirable => push("CD 127 Table 2.24", head, Status::Ok),
                SetBackOutcome::Relaxed { to_m, note } => {
                    push("CD 127 Table 2.24", format!("{head}; within the relaxation to {} of Note {note}, to be justified", m(to_m)), Status::Relaxation)
                }
                SetBackOutcome::Below { least_m } => push("CD 127 Table 2.24", format!("{head}; the least that applies here is {}", m(least_m)), Status::Fails),
            }
            if let Some(d) = sb.double_sided_deformable {
                let need = d.working_width_m - d.barrier_width_m;
                push(
                    "CD 127 2.26",
                    format!("sole double sided deformable barrier in a central reserve: set-back not less than working width − barrier width = {} − {} = {}; entered {}", m(d.working_width_m), m(d.barrier_width_m), m(need), m(s)),
                    if s >= need - 1e-9 { Status::Ok } else { Status::Fails },
                );
            }
        }
        (Some(_), None) => push("CD 127 Table 2.24", "the run has no set-back entered".into(), Status::NotChecked),
        (None, _) => push("CD 127 Table 2.24", "no set-back location entered, so the set-back is not checked".into(), Status::NotChecked),
    }

    // Taper.
    if let Some(t) = &input.taper {
        let actual = t.one_in();
        let shown = if actual.is_finite() { format!("1 in {}", fixed(actual, 1, 1)) } else { "none".into() };
        let head = format!("set-back changes by {} over {}: {shown}", m(t.change_in_set_back_m.abs()), m(t.length_m));
        let (reference, what) = match t.basis {
            TaperBasis::ManufacturerDeclared { .. } => ("CD 377 2.38", "the manufacturer's declared rate (entered)"),
            TaperBasis::NonProprietaryDeformable => ("CD 377 2.39", "a non-proprietary deformable safety barrier"),
            TaperBasis::NonProprietaryRigid => ("CD 377 2.39", "a non-proprietary rigid safety barrier"),
            TaperBasis::ProprietaryNotDeclared => ("CD 377 2.38", ""),
        };
        match t.basis.limit_one_in() {
            Some(limit) => push(
                reference,
                format!("{head}; not to be steeper than 1 in {} for {what}", fixed(limit, 0, 1)),
                if actual >= limit - 1e-9 { Status::Ok } else { Status::Fails },
            ),
            None => push(reference, format!("{head}; the manufacturer's declared rate is not entered"), Status::NotChecked),
        }
    }

    // Length of need.
    let provided = resolved.length_m;
    match &input.length_of_need {
        Some(l) => {
            let r = l.lengths();
            push(
                "CD 377 3.25",
                format!("length of need B to E = {} + {} + {} = {} (B to C and D to E are inputs from the risk assessment)", m(l.advance_m), m(l.hazard_m), m(l.beyond_m), m(r.sum_m)),
                Status::Note,
            );
            if let Some(a) = l.adopted_length_of_need_m {
                push(
                    "CD 377 3.28",
                    format!("length of need adopted by the designer {} against the sum {}", m(a), m(r.sum_m)),
                    if r.adopted_is_sufficient { Status::Ok } else { Status::Fails },
                );
            }
            if let Some(min) = l.manufacturer_min_length_of_need_m {
                push("CD 377 3.31", format!("manufacturer's minimum length of need {} (entered); the longer governs: {}", m(min), m(r.length_of_need_m)), Status::Note);
            }
            let plain = l.approach_to_full_containment_m + r.length_of_need_m + l.departure_to_full_containment_m;
            push(
                "CD 377 3.29",
                format!(
                    "minimum total length A to F = {} + {} + {} = {} (A to B and E to F as declared by the manufacturer, entered)",
                    m(l.approach_to_full_containment_m),
                    m(r.length_of_need_m),
                    m(l.departure_to_full_containment_m),
                    m(plain)
                ),
                Status::Note,
            );
            if let Some(min) = l.manufacturer_min_total_length_m {
                push("CD 377 3.31", format!("manufacturer's minimum total length {} (entered); the longer governs: {}", m(min), m(r.total_m)), Status::Note);
            }
            push(
                "CD 377 3.29, 3.30",
                format!("length of the run along the barrier {} against the minimum total length {} (length only: where the hazard sits within the run is not checked)", m(provided), m(r.total_m)),
                if provided >= r.total_m - 1e-9 { Status::Ok } else { Status::Fails },
            );
        }
        None => push(
            "CD 377 3.24",
            "no length of need entered: the lengths in advance of and beyond the hazard come from the risk assessment and are not supplied by this tool".into(),
            Status::NotChecked,
        ),
    }

    // Ends.
    for (which, end, ctx) in [("start", &run.start_end, input.start), ("end", &run.end_end, input.end)] {
        match end {
            EndTreatment::Unspecified => push(
                "CD 377 6.1",
                format!("{which}: terminals shall be provided on the ends of a safety barrier; neither a terminal nor a connection is entered"),
                Status::Fails,
            ),
            EndTreatment::Terminal { performance_class, energy_absorbing, .. } => {
                let facing = if ctx.faces_oncoming_traffic { "facing oncoming traffic" } else { "not facing oncoming traffic" };
                let (reference, asked) = match (fast, ctx.faces_oncoming_traffic) {
                    (true, true) => ("CD 377 6.3", "T110, T110/1 or P4"),
                    (true, false) => ("CD 377 6.4", "a minimum of T80/3 or P1"),
                    (false, _) => ("CD 377 6.5", "a minimum of T80/3 or P1"),
                };
                let head = format!("{which} terminal, {band}, {facing}: {asked}; entered {}", performance_class.label());
                match check_terminal_class(performance_class, speed, ctx.faces_oncoming_traffic) {
                    TerminalVerdict::Meets => push(reference, head, Status::Ok),
                    TerminalVerdict::MeetsAsHigherPClass => push(reference, format!("{head}, a higher class than P1 (DD ENV 1317-4:2002, 5.1)"), Status::Ok),
                    TerminalVerdict::MeetsAsHigherTClass => push(
                        reference,
                        format!("{head}, taken as above the minimum (assumption: PD CEN/TS 1317-7, which orders the T classes, is not held)"),
                        Status::Ok,
                    ),
                    TerminalVerdict::Fails => push(reference, head, Status::Fails),
                    TerminalVerdict::NotNamed => push(
                        reference,
                        format!("{head}, a class not named in CD 377 or DD ENV 1317-4:2002: compare against PD CEN/TS 1317-7"),
                        Status::NotChecked,
                    ),
                }
                if fast && ctx.faces_oncoming_traffic {
                    let (text, status) = match energy_absorbing {
                        Some(true) => ("entered as energy absorbing", Status::Ok),
                        Some(false) => ("entered as not energy absorbing", Status::Fails),
                        None => ("not entered", Status::NotChecked),
                    };
                    push("CD 377 6.7", format!("{which} terminal, {band}, {facing}: shall be energy absorbing; {text}"), status);
                }
            }
            EndTreatment::ConnectsTo { system, name, transition: None } => match system {
                ConnectedSystem::Parapet => push(
                    "CD 377 3.59",
                    format!("{which}: a transition shall be provided between the safety barrier and vehicle parapet {}; none is entered", name.trim()),
                    Status::Fails,
                ),
                ConnectedSystem::Structure => push(
                    "CD 377 7.2",
                    format!("{which}: a transition shall be included between the barrier and structure {}; none is entered", name.trim()),
                    Status::Fails,
                ),
                ConnectedSystem::SafetyBarrier => push(
                    "CD 377 7.1",
                    format!("{which}: connects to safety barrier {} with no transition entered; one is required if the two differ in cross section, containment level or material, or by more than one working width class", name.trim()),
                    Status::Review,
                ),
            },
            EndTreatment::ConnectsTo { system, name, transition: Some(t) } => {
                let to = format!("{which} transition to {} {}", system.label(), name.trim());
                match (t.containment, t.connected_containment) {
                    (Some(tc), Some(other)) => {
                        // The lower of the two connected systems.
                        let lower = match run.containment.covers(other) {
                            Coverage::Covers | Coverage::HigherButNotIncluded => Some(other),
                            Coverage::Below => Some(run.containment),
                            Coverage::NotRanked => None,
                        };
                        match lower {
                            Some(lower) => {
                                let status = match tc.covers(lower) {
                                    Coverage::Covers => Status::Ok,
                                    Coverage::Below => Status::Fails,
                                    Coverage::HigherButNotIncluded if lower == ContainmentLevel::N2 => Status::Ok,
                                    Coverage::HigherButNotIncluded | Coverage::NotRanked => Status::Review,
                                };
                                push("CD 377 7.10", format!("{to}: containment not less than the lower of {} and {other}, which is {lower}; entered {tc}", run.containment), status);
                            }
                            None => push("CD 377 7.10", format!("{to}: {} and {other} are not ranked by BS EN 1317-2:2010; transition entered as {tc}", run.containment), Status::Review),
                        }
                    }
                    _ => push("CD 377 7.10", format!("{to}: the containment levels of the transition and of the connected system are not both entered"), Status::NotChecked),
                }
                match t.impact_severity {
                    Some(isl) => {
                        let (reference, limit) = if ctx.into_impact_resistant_structure { ("CD 377 7.12", ImpactSeverityLevel::C) } else { ("CD 377 7.11", ImpactSeverityLevel::B) };
                        push(reference, format!("{to}: impact severity level shall not exceed {limit}; entered {isl}"), if isl <= limit { Status::Ok } else { Status::Fails });
                    }
                    None => push("CD 377 7.11", format!("{to}: no impact severity level entered"), Status::NotChecked),
                }
            }
        }
    }

    // Gap.
    if let Some(g) = input.gap_to_adjacent_barrier_m {
        if g <= GAP_CLOSE_M + 1e-9 {
            push(
                "CD 377 3.35",
                format!("gap of {} to the adjacent barrier is 50 m or less: it shall be closed unless needed for access or maintenance or the offsets cannot be joined at the declared taper", m(g)),
                Status::Review,
            );
        } else if g <= GAP_CLOSE_UNLESS_COST_M + 1e-9 {
            push(
                "CD 377 3.36",
                format!("gap of {} to the adjacent barrier is over 50 m and up to 100 m: it shall be closed unless there are significant cost or technical requirements, or the exceptions of 3.35 apply", m(g)),
                Status::Review,
            );
        } else {
            push("CD 377 3.35, 3.36", format!("gap of {} to the adjacent barrier is more than 100 m", m(g)), Status::Ok);
        }
    }

    // Local reference.
    if input.include_mowt_reference {
        let compare = |ok: bool| if ok { "meets it" } else { "is below it" };
        push(
            "MoWT 12.3.4 (local reference, not governing)",
            format!("generally at least {} of steel beam strong post guardrail; the run is {} and {}", m(MOWT_MIN_GUARDRAIL_LENGTH_M), m(provided), compare(provided >= MOWT_MIN_GUARDRAIL_LENGTH_M - 1e-9)),
            Status::Note,
        );
        if let Some(s) = run.set_back_m {
            push(
                "MoWT 12.3.6 (local reference, not governing)",
                format!("guardrail at least {} back from the shoulder edge, or the carriageway edge if there is no shoulder; the entered set-back of {} {} (CD 127 measures set-back from a differently defined point)", m(MOWT_MIN_SET_BACK_M), m(s), compare(s >= MOWT_MIN_SET_BACK_M - 1e-9)),
                Status::Note,
            );
        }
        if let Some(g) = input.gap_to_adjacent_barrier_m {
            if g < MOWT_SHORT_GAP_M {
                push("MoWT 12.3.7 (local reference, not governing)", format!("gaps shorter than {} are not to be left; the gap is {}", m(MOWT_SHORT_GAP_M), m(g)), Status::Note);
            }
        }
    }

    RestraintDesign { mark: run.name.clone(), lines }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::sample_run;
    use crate::{resolve, Transition, VehicleIntrusionClass};
    use afrobim_alignment::AlignmentBuilder;
    use ContainmentLevel::*;

    fn resolved(run: &BarrierRun) -> ResolvedRun {
        let a = AlignmentBuilder::new("A1", 0.0, 0.0, 0.0, 0.0).line(1000.0).build().unwrap();
        resolve(run, &a).unwrap()
    }

    fn status_of(d: &RestraintDesign, reference: &str) -> Vec<Status> {
        d.lines.iter().filter(|l| l.reference == reference).map(|l| l.status).collect()
    }

    #[test]
    fn speed_limits_and_the_50_mph_step() {
        assert!(SpeedLimit::Mph(50.0).is_50_mph_or_more());
        assert!(!SpeedLimit::Mph(49.9).is_50_mph_or_more());
        // 80 km/h = 49.71 mph; 80.4672 km/h is exactly 50 mph.
        assert!((SpeedLimit::Kmh(80.0).mph() - 49.7097).abs() < 1e-4);
        assert!(!SpeedLimit::Kmh(80.0).is_50_mph_or_more());
        assert!(SpeedLimit::Kmh(80.4672).is_50_mph_or_more());
        assert!(SpeedLimit::Kmh(100.0).is_50_mph_or_more());
        assert_eq!(SpeedLimit::Mph(60.0).label(), "60 mph");
        assert_eq!(SpeedLimit::Kmh(80.0).label(), "80 km/h (49.7 mph)");
    }

    #[test]
    fn minimum_containment_is_cd_377_3_6_and_3_7() {
        assert_eq!(minimum_containment(SpeedLimit::Mph(50.0)), N2);
        assert_eq!(minimum_containment(SpeedLimit::Mph(70.0)), N2);
        assert_eq!(minimum_containment(SpeedLimit::Mph(40.0)), N1);
        assert_eq!(minimum_containment(SpeedLimit::Kmh(80.0)), N1);
        assert_eq!(minimum_containment(SpeedLimit::Kmh(100.0)), N2);
    }

    #[test]
    fn containment_at_structural_supports_is_cd_377_table_3_48() {
        assert_eq!(minimum_containment_at_support(Some(Cs453Group::Group1)), H4a);
        assert_eq!(minimum_containment_at_support(Some(Cs453Group::Group2)), H1);
        // Clause 3.47 for everything else.
        assert_eq!(minimum_containment_at_support(Some(Cs453Group::Group3)), N2);
        assert_eq!(minimum_containment_at_support(None), N2);
    }

    #[test]
    fn set_back_values_are_cd_127_table_2_24() {
        assert_eq!(SetBackLocation::VergeWithoutHardStrip.desirable_minimum_m(), 1.2);
        assert_eq!(SetBackLocation::VergeWithHardStrip.desirable_minimum_m(), 0.6);
        assert_eq!(SetBackLocation::CentralReserve.desirable_minimum_m(), 1.2);
        assert_eq!(SET_BACK_RELAXATION_LOW_SPEED_M, 0.6);
        assert_eq!(SET_BACK_RELAXATION_CONSTRAINT_M, 1.0);
        assert_eq!(SET_BACK_RELAXATION_EMBANKMENT_M, 0.45);
    }

    #[test]
    fn set_back_relaxations() {
        use SetBackLocation::*;
        use SetBackOutcome::*;
        let (fast, slow) = (SpeedLimit::Mph(70.0), SpeedLimit::Mph(40.0));
        assert_eq!(check_set_back(VergeWithoutHardStrip, 1.2, fast, false, false), Desirable);
        // 1.1 m at 70 mph: fails outright; with a physical constraint on an existing road, Note 2.
        assert_eq!(check_set_back(VergeWithoutHardStrip, 1.1, fast, false, false), Below { least_m: 1.2 });
        assert_eq!(check_set_back(VergeWithoutHardStrip, 1.1, fast, true, false), Relaxed { to_m: 1.0, note: 2 });
        assert_eq!(check_set_back(CentralReserve, 0.9, fast, true, false), Below { least_m: 1.0 });
        // Note 1 at 50 mph or less, to 600 mm. At exactly 50 mph it applies.
        assert_eq!(check_set_back(CentralReserve, 0.7, slow, false, false), Relaxed { to_m: 0.6, note: 1 });
        assert_eq!(check_set_back(VergeWithoutHardStrip, 0.6, SpeedLimit::Mph(50.0), false, false), Relaxed { to_m: 0.6, note: 1 });
        assert_eq!(check_set_back(VergeWithoutHardStrip, 0.6, SpeedLimit::Mph(51.0), false, false), Below { least_m: 1.2 });
        assert_eq!(check_set_back(VergeWithoutHardStrip, 0.5, slow, true, false), Below { least_m: 0.6 });
        // Both notes available: the less relaxed one that fits is named.
        assert_eq!(check_set_back(VergeWithoutHardStrip, 1.05, slow, true, false), Relaxed { to_m: 1.0, note: 2 });
        // Verge with a hard strip: 600 mm; Note 3 to 450 mm only; Notes 1 and 2 do not apply.
        assert_eq!(check_set_back(VergeWithHardStrip, 0.6, fast, false, false), Desirable);
        assert_eq!(check_set_back(VergeWithHardStrip, 0.5, slow, true, false), Below { least_m: 0.6 });
        assert_eq!(check_set_back(VergeWithHardStrip, 0.5, fast, false, true), Relaxed { to_m: 0.45, note: 3 });
        assert_eq!(check_set_back(VergeWithHardStrip, 0.4, fast, false, true), Below { least_m: 0.45 });
    }

    #[test]
    fn taper_limits_are_cd_377_2_39() {
        assert_eq!(TaperBasis::NonProprietaryDeformable.limit_one_in(), Some(16.0));
        assert_eq!(TaperBasis::NonProprietaryRigid.limit_one_in(), Some(20.0));
        assert_eq!(TaperBasis::ManufacturerDeclared { one_in: 30.0 }.limit_one_in(), Some(30.0));
        assert_eq!(TaperBasis::ProprietaryNotDeclared.limit_one_in(), None);
        // 0.6 m over 9.6 m is exactly 1 in 16.
        let t = TaperInput { change_in_set_back_m: -0.6, length_m: 9.6, basis: TaperBasis::NonProprietaryDeformable };
        assert!((t.one_in() - 16.0).abs() < 1e-12);
    }

    #[test]
    fn length_of_need_reproduces_cd_377_appendix_c() {
        // C1: gantry base 5.5 m long in the verge of a dual carriageway. B to C 20.0 m, D to E
        // 0 m: length of need 25.5 m. Full containment 12.5 m from the terminal: total 50.5 m.
        let c1 = LengthOfNeedInput {
            advance_m: 20.0,
            hazard_m: 5.5,
            beyond_m: 0.0,
            approach_to_full_containment_m: 12.5,
            departure_to_full_containment_m: 12.5,
            ..Default::default()
        };
        let r = c1.lengths();
        assert!((r.sum_m - 25.5).abs() < 1e-12 && (r.length_of_need_m - 25.5).abs() < 1e-12);
        assert!((r.total_m - 50.5).abs() < 1e-12);

        // C2: sign post 0.1 m in the verge of a single carriageway. 16.0 + 0.1 + 10.0 = 26.1 m;
        // the designer adopts 45.0 m; 17.4 m each end: total 79.8 m.
        let c2 = LengthOfNeedInput {
            advance_m: 16.0,
            hazard_m: 0.1,
            beyond_m: 10.0,
            adopted_length_of_need_m: Some(45.0),
            approach_to_full_containment_m: 17.4,
            departure_to_full_containment_m: 17.4,
            ..Default::default()
        };
        let r = c2.lengths();
        assert!((r.sum_m - 26.1).abs() < 1e-12);
        assert!((r.length_of_need_m - 45.0).abs() < 1e-12 && r.adopted_is_sufficient);
        assert!((r.total_m - 79.8).abs() < 1e-12);

        // 3.31: the manufacturer's values govern when longer; an adopted value below the sum
        // is flagged and not used.
        let r = LengthOfNeedInput { manufacturer_min_length_of_need_m: Some(40.0), manufacturer_min_total_length_m: Some(70.0), ..c1 }.lengths();
        assert_eq!((r.length_of_need_m, r.total_m), (40.0, 70.0));
        let r = LengthOfNeedInput { adopted_length_of_need_m: Some(20.0), ..c1 }.lengths();
        assert!(!r.adopted_is_sufficient && (r.length_of_need_m - 25.5).abs() < 1e-12);
    }

    #[test]
    fn terminal_classes_are_cd_377_6_3_to_6_5() {
        use TerminalClass::*;
        use TerminalVerdict::*;
        let (fast, slow) = (SpeedLimit::Mph(60.0), SpeedLimit::Mph(40.0));
        // 6.3: 50 mph or more, facing oncoming traffic: T110, T110/1 or P4 and nothing else.
        for c in [T110, T110_1, P4] {
            assert_eq!(check_terminal_class(&c, fast, true), Meets);
        }
        for c in [T80_3, P1, P2, P3] {
            assert_eq!(check_terminal_class(&c, fast, true), Fails);
        }
        // 6.4 and 6.5: minimum T80/3 or P1.
        for (speed, facing) in [(fast, false), (slow, true), (slow, false)] {
            for c in [T80_3, P1] {
                assert_eq!(check_terminal_class(&c, speed, facing), Meets);
            }
            // DD ENV 1317-4:2002 5.1: a higher P class has met the lower ones.
            for c in [P2, P3, P4] {
                assert_eq!(check_terminal_class(&c, speed, facing), MeetsAsHigherPClass);
            }
            // The T ordering is an assumption (PD CEN/TS 1317-7 not held).
            for c in [T110, T110_1] {
                assert_eq!(check_terminal_class(&c, speed, facing), MeetsAsHigherTClass);
            }
        }
        assert_eq!(check_terminal_class(&Other("X".into()), fast, true), NotNamed);
    }

    #[test]
    fn gap_and_local_reference_constants() {
        assert_eq!((GAP_CLOSE_M, GAP_CLOSE_UNLESS_COST_M), (50.0, 100.0)); // CD 377 3.35, 3.36
        assert_eq!((MOWT_MIN_GUARDRAIL_LENGTH_M, MOWT_MIN_SET_BACK_M, MOWT_SHORT_GAP_M), (30.0, 0.6, 80.0)); // MoWT 12.3.4, .6, .7
        assert_eq!((SPEED_THRESHOLD_MPH, AADT_THRESHOLD), (50.0, 5000.0)); // CD 377 1.3
    }

    /// A 100 m N2 W4 ISL A run on a 60 mph single carriageway, hand-checked line by line.
    fn worked_input() -> DesignInput {
        DesignInput {
            two_way_aadt: Some(12_000.0),
            hazard_clearance_m: Some(1.5),
            slope_clearance_m: Some(1.3),
            set_back: Some(SetBackInput {
                location: SetBackLocation::VergeWithoutHardStrip,
                existing_road_with_physical_constraint: false,
                away_from_existing_embankment_edge: false,
                double_sided_deformable: None,
            }),
            taper: Some(TaperInput { change_in_set_back_m: 0.5, length_m: 10.0, basis: TaperBasis::NonProprietaryDeformable }),
            length_of_need: Some(LengthOfNeedInput {
                advance_m: 16.0,
                hazard_m: 0.1,
                beyond_m: 10.0,
                adopted_length_of_need_m: Some(45.0),
                approach_to_full_containment_m: 17.4,
                departure_to_full_containment_m: 17.4,
                ..Default::default()
            }),
            start: EndContext { faces_oncoming_traffic: true, into_impact_resistant_structure: false },
            end: EndContext { faces_oncoming_traffic: true, into_impact_resistant_structure: false },
            gap_to_adjacent_barrier_m: Some(150.0),
            ..DesignInput::new(SpeedLimit::Mph(60.0))
        }
    }

    fn worked_run() -> BarrierRun {
        let mut run = sample_run();
        run.start_end = EndTreatment::Terminal { performance_class: TerminalClass::T110, energy_absorbing: Some(true), description: String::new() };
        run.end_end = EndTreatment::Terminal { performance_class: TerminalClass::P4, energy_absorbing: Some(true), description: String::new() };
        run
    }

    #[test]
    fn a_run_that_meets_every_check() {
        let run = worked_run();
        let d = check_run(&run, &resolved(&run), &worked_input());
        assert!(d.passes());
        // 60 mph: N2 (3.6). ISL A (3.12). W4 = 1.3 m against 1.5 m and 1.3 m (3.13, 3.39).
        // Set-back 1.2 m = desirable. Taper 1 in 20 against 1 in 16. Length 100 m against
        // 17.4 + 45.0 + 17.4 = 79.8 m. T110 and P4 facing traffic, energy absorbing. Gap 150 m.
        for reference in ["CD 377 3.6", "CD 377 3.12", "CD 377 3.13", "CD 377 3.39, 3.41", "CD 127 Table 2.24", "CD 377 2.39", "CD 377 3.28", "CD 377 3.29, 3.30", "CD 377 3.35, 3.36"] {
            assert_eq!(status_of(&d, reference), [Status::Ok], "{reference}");
        }
        assert_eq!(status_of(&d, "CD 377 6.3"), [Status::Ok, Status::Ok]);
        assert_eq!(status_of(&d, "CD 377 6.7"), [Status::Ok, Status::Ok]);
        assert_eq!((d.count(Status::Fails), d.count(Status::NotChecked), d.count(Status::Review), d.count(Status::Relaxation)), (0, 0, 0, 0));
        assert_eq!(d.count(Status::Ok), 13);

        let text = d.calculation();
        assert!(text[0].contains("VRS-001") && text[0].contains("CD 377 Version 5.0.1"));
        assert!(text[1].contains("do not perform or replace the Road Restraint Risk Assessment Process"));
        assert!(text.contains(&"CD 377 3.6: speed limit 60 mph is 50 mph or more, minimum containment N2; entered N2: OK.".to_string()));
        assert!(text.contains(&"CD 377 3.13: no hazard within the working width class: W4 is up to 1.3 m, nearest hazard 1.5 m from the traffic face: OK.".to_string()));
        assert!(text.contains(&"CD 377 3.14: the widest class the clearance of 1.5 m allows is W4: NOTE.".to_string()));
        assert!(text.contains(&"CD 127 Table 2.24: verge with no adjacent hard strip or hard shoulder: desirable minimum set-back 1.2 m, entered 1.2 m: OK.".to_string()));
        assert!(text.contains(&"CD 377 2.39: set-back changes by 0.5 m over 10.0 m: 1 in 20.0; not to be steeper than 1 in 16 for a non-proprietary deformable safety barrier: OK.".to_string()));
        assert!(text.contains(&"CD 377 3.25: length of need B to E = 16.0 m + 0.1 m + 10.0 m = 26.1 m (B to C and D to E are inputs from the risk assessment): NOTE.".to_string()));
        assert!(text.contains(&"CD 377 3.29: minimum total length A to F = 17.4 m + 45.0 m + 17.4 m = 79.8 m (A to B and E to F as declared by the manufacturer, entered): NOTE.".to_string()));
        assert!(text.contains(&"CD 377 6.3: start terminal, 50 mph or more, facing oncoming traffic: T110, T110/1 or P4; entered T110: OK.".to_string()));
        assert_eq!(text[text.len() - 2], "Summary: 13 OK, 0 by relaxation, 0 FAILS, 0 for review, 0 not checked.");
        assert_eq!(text[text.len() - 1], "For an engineer to check; not a certified design.");
        assert!(text.iter().all(|l| !l.contains("FAILS.")));
    }

    #[test]
    fn a_run_that_fails_each_check() {
        let mut run = worked_run();
        run.containment = N1; // below N2 at 60 mph
        run.impact_severity = ImpactSeverityLevel::C;
        run.working_width = WorkingWidthClass::W5; // 1.7 m against 1.5 m and 1.3 m
        run.set_back_m = Some(0.9); // below 1.2 m, no relaxation at 60 mph
        run.start_station_m = 100.0;
        run.end_station_m = 170.0; // 70 m against 79.8 m
        run.start_end = EndTreatment::terminal(TerminalClass::P1); // facing traffic at 60 mph
        run.end_end = EndTreatment::Unspecified;
        let mut input = worked_input();
        input.taper = Some(TaperInput { change_in_set_back_m: 1.0, length_m: 10.0, basis: TaperBasis::NonProprietaryRigid }); // 1 in 10 against 1 in 20
        input.length_of_need.as_mut().unwrap().adopted_length_of_need_m = Some(45.0);
        input.risk_assessed_containment = Some(H2);
        input.gap_to_adjacent_barrier_m = Some(40.0);
        let d = check_run(&run, &resolved(&run), &input);
        assert!(!d.passes());
        for reference in ["CD 377 3.6", "CD 377 3.9", "CD 377 3.12", "CD 377 3.13", "CD 377 3.39, 3.41", "CD 127 Table 2.24", "CD 377 2.39", "CD 377 3.29, 3.30", "CD 377 6.3", "CD 377 6.1"] {
            assert_eq!(status_of(&d, reference), [Status::Fails], "{reference}");
        }
        assert_eq!(status_of(&d, "CD 377 6.7"), [Status::NotChecked]);
        assert_eq!(status_of(&d, "CD 377 3.35"), [Status::Review]);
        assert_eq!(d.count(Status::Fails), 10);
        let text = d.calculation();
        assert!(text.contains(&"CD 127 Table 2.24: verge with no adjacent hard strip or hard shoulder: desirable minimum set-back 1.2 m, entered 0.9 m; the least that applies here is 1.2 m: FAILS.".to_string()));
        assert!(text.contains(&"CD 377 3.29, 3.30: length of the run along the barrier 70.0 m against the minimum total length 79.8 m (length only: where the hazard sits within the run is not checked): FAILS.".to_string()));
    }

    #[test]
    fn nothing_entered_means_not_checked_never_assumed() {
        let mut run = sample_run();
        run.set_back_m = None;
        let d = check_run(&run, &resolved(&run), &DesignInput::new(SpeedLimit::Mph(60.0)));
        assert!(d.passes());
        assert_eq!(status_of(&d, "CD 377 3.13"), [Status::NotChecked]);
        assert_eq!(status_of(&d, "CD 127 Table 2.24"), [Status::NotChecked]);
        assert_eq!(status_of(&d, "CD 377 3.24"), [Status::NotChecked]);
        assert!(d.calculation().iter().any(|l| l.contains("come from the risk assessment and are not supplied by this tool")));
        // Terminals default to "not facing oncoming traffic": T110 then counts as above T80/3.
        assert_eq!(status_of(&d, "CD 377 6.4"), [Status::Ok, Status::Ok]);
        assert!(d.lines.iter().any(|l| l.text.contains("taken as above the minimum")));
    }

    #[test]
    fn low_speed_metric_limit_and_relaxation() {
        // 80 km/h = 49.7 mph: N1 is the minimum, flagged for the authority's ruling; Note 1
        // relaxes the 1.2 m set-back to 0.6 m; T80/3 terminals are enough even facing traffic.
        let mut run = sample_run();
        run.containment = N1;
        run.set_back_m = Some(0.8);
        run.start_end = EndTreatment::terminal(TerminalClass::T80_3);
        let mut input = worked_input();
        input.speed_limit = SpeedLimit::Kmh(80.0);
        input.two_way_aadt = Some(3000.0);
        input.taper = None;
        let d = check_run(&run, &resolved(&run), &input);
        assert!(d.passes());
        assert_eq!(status_of(&d, "CD 377 3.6"), [Status::Review]);
        assert_eq!(status_of(&d, "CD 377 2.2"), [Status::Note]);
        assert_eq!(status_of(&d, "CD 377 3.7"), [Status::Ok]);
        assert_eq!(status_of(&d, "CD 127 Table 2.24"), [Status::Relaxation]);
        assert_eq!(status_of(&d, "CD 377 6.5"), [Status::Ok, Status::Ok]);
        assert!(status_of(&d, "CD 377 6.7").is_empty());
        let text = d.calculation();
        assert!(text.contains(&"CD 377 3.7: speed limit 80 km/h (49.7 mph) is less than 50 mph, minimum containment N1; entered N1: OK.".to_string()));
        assert!(text.iter().any(|l| l.ends_with("within the relaxation to 0.6 m of Note 1, to be justified: OK BY RELAXATION.")));
        // The same road posted at 100 km/h: N1 fails 3.6 and the relaxation is gone.
        input.speed_limit = SpeedLimit::Kmh(100.0);
        let d = check_run(&run, &resolved(&run), &input);
        assert_eq!(status_of(&d, "CD 377 3.6"), [Status::Fails]);
        assert_eq!(status_of(&d, "CD 127 Table 2.24"), [Status::Fails]);
    }

    #[test]
    fn structural_supports_parapets_and_transitions() {
        let mut run = worked_run();
        run.containment = H1;
        run.impact_severity = ImpactSeverityLevel::B;
        // End: transition to an H2 parapet. Lower of H1 and H2 is H1 (7.10): an N2 transition fails.
        let transition = |c, isl| EndTreatment::ConnectsTo {
            system: ConnectedSystem::Parapet,
            name: "BR-1".into(),
            transition: Some(Transition { description: String::new(), containment: Some(c), impact_severity: Some(isl), connected_containment: Some(H2) }),
        };
        run.end_end = transition(N2, ImpactSeverityLevel::C);
        let mut input = worked_input();
        input.structural_support = Some(StructuralSupport::Existing { cs453_group: Some(Cs453Group::Group1) });
        input.at_parapet_end = true;
        let d = check_run(&run, &resolved(&run), &input);
        assert_eq!(status_of(&d, "CD 377 3.6"), [Status::Ok]);
        assert_eq!(status_of(&d, "CD 377 3.47"), [Status::Ok]);
        assert_eq!(status_of(&d, "CD 377 Table 3.48"), [Status::Fails]); // H1 against H4a
        assert_eq!(status_of(&d, "CD 377 3.64"), [Status::Ok]);
        assert_eq!(status_of(&d, "CD 377 3.12.1"), [Status::Note]);
        assert_eq!(status_of(&d, "CD 377 3.13, 3.17"), [Status::NotChecked]);
        // H1 against the N2 of 3.6, 3.47 and 3.64: OK, with the standard's note reported.
        assert_eq!(d.lines.iter().filter(|l| l.text.contains("does not include the N2 car test TB 32")).count(), 3);
        assert_eq!(status_of(&d, "CD 377 7.10"), [Status::Fails]);
        assert_eq!(status_of(&d, "CD 377 7.11"), [Status::Fails]); // ISL C against B
        assert!(d.calculation().iter().any(|l| l == "CD 377 7.10: end transition to parapet BR-1: containment not less than the lower of H1 and H2, which is H1; entered N2: FAILS."));

        // Group 2 support, H1 transition of ISL C into an impact-resistant structure (7.12).
        input.structural_support = Some(StructuralSupport::Existing { cs453_group: Some(Cs453Group::Group2) });
        input.end.into_impact_resistant_structure = true;
        run.end_end = transition(H1, ImpactSeverityLevel::C);
        let d = check_run(&run, &resolved(&run), &input);
        assert_eq!(status_of(&d, "CD 377 Table 3.48"), [Status::Ok]);
        assert_eq!(status_of(&d, "CD 377 7.10"), [Status::Ok]);
        assert_eq!(status_of(&d, "CD 377 7.12"), [Status::Ok]);
        assert!(d.passes());

        // Vehicle intrusion (Table 5): VI4 = 1.3 m against the 1.5 m clearance; VI6 = 2.1 m fails;
        // VI9 has no upper bound.
        for (vi, status) in [(VehicleIntrusionClass::VI4, Status::Ok), (VehicleIntrusionClass::VI6, Status::Fails), (VehicleIntrusionClass::VI9, Status::Review)] {
            run.vehicle_intrusion = Some(vi);
            assert_eq!(status_of(&check_run(&run, &resolved(&run), &input), "CD 377 3.13, 3.17"), [status], "{vi}");
        }
        // An L level covers N2 outright; an H level against a required L level is for review.
        run.containment = L1;
        let d = check_run(&run, &resolved(&run), &input);
        assert!(d.lines.iter().all(|l| !l.text.contains("TB 32")));
        run.containment = H2;
        input.risk_assessed_containment = Some(L1);
        assert_eq!(status_of(&check_run(&run, &resolved(&run), &input), "CD 377 3.9"), [Status::Review]);
        input.risk_assessed_containment = None;

        // H4b against the H4a of Table 3.48 is not ranked: for review, not a pass or a fail.
        run.containment = H4b;
        input.structural_support = Some(StructuralSupport::Existing { cs453_group: Some(Cs453Group::Group1) });
        assert_eq!(status_of(&check_run(&run, &resolved(&run), &input), "CD 377 Table 3.48"), [Status::Review]);

        // New support: PD 6688-1-7 is not held. Unassessed existing support: Table 3.48 unmade.
        input.structural_support = Some(StructuralSupport::New);
        assert_eq!(status_of(&check_run(&run, &resolved(&run), &input), "CD 377 3.49"), [Status::NotChecked]);
        input.structural_support = Some(StructuralSupport::Existing { cs453_group: None });
        assert_eq!(status_of(&check_run(&run, &resolved(&run), &input), "CD 377 3.48"), [Status::NotChecked]);

        // Joining a parapet or a structure with no transition fails; another barrier is for review.
        let join = |system| EndTreatment::ConnectsTo { system, name: "X".into(), transition: None };
        run.end_end = join(ConnectedSystem::Parapet);
        assert_eq!(status_of(&check_run(&run, &resolved(&run), &input), "CD 377 3.59"), [Status::Fails]);
        run.end_end = join(ConnectedSystem::Structure);
        assert_eq!(status_of(&check_run(&run, &resolved(&run), &input), "CD 377 7.2"), [Status::Fails]);
        run.end_end = join(ConnectedSystem::SafetyBarrier);
        assert_eq!(status_of(&check_run(&run, &resolved(&run), &input), "CD 377 7.1"), [Status::Review]);
    }

    #[test]
    fn working_width_under_0_6_m_double_sided_reserve_and_gaps() {
        let mut run = worked_run();
        run.working_width = WorkingWidthClass::W1;
        run.set_back_m = Some(1.2);
        let mut input = worked_input();
        input.hazard_clearance_m = Some(0.5);
        input.slope_clearance_m = None;
        // CD 127 2.26: working width 1.9 m, barrier 0.4 m wide: set-back at least 1.5 m.
        input.set_back = Some(SetBackInput {
            location: SetBackLocation::CentralReserve,
            existing_road_with_physical_constraint: false,
            away_from_existing_embankment_edge: false,
            double_sided_deformable: Some(DoubleSidedDeformable { working_width_m: 1.9, barrier_width_m: 0.4 }),
        });
        input.gap_to_adjacent_barrier_m = Some(75.0);
        input.include_mowt_reference = true;
        let d = check_run(&run, &resolved(&run), &input);
        assert_eq!(status_of(&d, "CD 377 3.13"), [Status::Fails]);
        assert_eq!(status_of(&d, "CD 377 3.15"), [Status::Note]);
        assert_eq!(status_of(&d, "CD 127 Table 2.24"), [Status::Ok]);
        assert_eq!(status_of(&d, "CD 127 2.26"), [Status::Fails]);
        assert_eq!(status_of(&d, "CD 377 3.36"), [Status::Review]);
        // The local reference lines are notes and change nothing.
        let local: Vec<&CheckLine> = d.lines.iter().filter(|l| l.reference.starts_with("MoWT")).collect();
        assert_eq!(local.len(), 3);
        assert!(local.iter().all(|l| l.status == Status::Note && l.reference.ends_with("(local reference, not governing)")));
        assert!(local[0].text.contains("at least 30.0 m") && local[0].text.contains("the run is 100.0 m and meets it"));
        assert!(local[2].text.contains("shorter than 80.0 m"));
    }
}
