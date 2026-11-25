// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Performance requirements of the road lighting classes of **EN 13201-2:2015**, and the class
//! selection procedure of **CEN/TR 13201-1:2014**, as reproduced in a secondary source.
//!
//! # The source, and what it is
//!
//! Every value here is read from:
//!
//! > Prof. Dr.-Ing. Axel Stockmar (University of Applied Sciences and Arts Hannover) for the
//! > Deutsche Lichttechnische Gesellschaft e.V. (LiTG), **Handbook about Interpretation of
//! > EN 13201**, INTERREG Central Europe project CE 452 "Dynamic Light", deliverable D.T4.1.4,
//! > October 2017, ISBN 978-3-947929-07-8 ([`HANDBOOK`]).
//!
//! The handbook interprets **EN 13201-2:2015** (performance requirements),
//! **EN 13201-5:2015** (energy performance indicators), **EN 13201-3:2015** and **-4:2015** (named
//! only) and **CEN/TR 13201-1:2014** (guidelines on selection of lighting classes). It does not
//! interpret the 2003 to 2004 edition, which it mentions only as the superseded first edition of
//! the technical report.
//!
//! **It is a secondary source.** Neither EN 13201-2:2015 nor CEN/TR 13201-1:2014 itself has been
//! read. The handbook gives its own table numbers (5.1 to 5.3 for the requirements, 4.1 to 4.3
//! for the selection) and **no EN table number**, so the citations below say "as reproduced in"
//! and give the handbook's table and printed page. The handbook says of its Table 5.3 that it "is
//! a corrected and modified version of the respective table in the European Standard EN 13201-2"
//! with recommendations for glare added; the threshold increment column of the P classes is that
//! addition and is labelled as such. **The owner should confirm every value against EN 13201-2:2015
//! and CEN/TR 13201-1:2014 themselves before relying on it** ([`SECONDARY_NOTICE`]).
//!
//! # What is here
//!
//! | Item | Handbook |
//! |---|---|
//! | [`TABLE_5_1_M`]: M1 to M6, luminance, uniformities, wet uniformity, threshold increment, edge illuminance ratio | Table 5.1, page 7 |
//! | [`TABLE_5_2_C`]: C0 to C5, illuminance, uniformity, threshold increment by speed | Table 5.2, page 9 |
//! | [`TABLE_5_3_P`]: P1 to P6 (P7 not determined), illuminances, threshold increment, facial recognition additions, and [`P_E_AV_MAX_FACTOR`] | Table 5.3, page 10 |
//! | [`lookup`]: a label such as "M3" to its requirements | |
//! | [`selection`]: the weighting parameters and "6 minus the sum" rule, as a suggestion | Tables 4.1 to 4.3, pages 4 to 6 |
//!
//! # What is not here
//!
//! The handbook has no table for the HS, SC or EV classes, and no formula or worked example for
//! the power density indicator or the annual energy consumption indicator of EN 13201-5 (section
//! 9 describes energy efficiency measures in words, with names that are not those of the
//! standard), so none is held. Table 5.4 (classes of comparable level), the figures (flicker,
//! stopping distance, threshold increment against adaptation luminance), the speed ranges given
//! only as examples (section 7.1), the alternative selection method of CEN/TR 13201-1 Annex B
//! (described, not tabulated) and the adaptive lighting discussion are not taken. See the README.
//!
//! # How the values are read
//!
//! The handbook does not word the direction of each limit. They are read as: luminance,
//! illuminance, uniformity and edge illuminance ratio are minimum maintained values; threshold
//! increment is a maximum; the one limit worded in the handbook ("shall not exceed 1.5 times")
//! is applied as worded. Entered results are compared without rounding.

/// The secondary source every value of this module comes from.
pub const HANDBOOK: &str =
    "Stockmar, Handbook about Interpretation of EN 13201 (LiTG, INTERREG Central Europe CE 452 Dynamic Light, D.T4.1.4, October 2017)";

/// The warning that goes with anything taken from this module.
pub const SECONDARY_NOTICE: &str = "The EN 13201-2:2015 class values are taken from a secondary source (Stockmar, Handbook about Interpretation of EN 13201, LiTG, 2017), not from the standard: the owner should confirm them against BS EN 13201-2:2015 itself.";

/// Citation of [`TABLE_5_1_M`].
pub const CITATION_M: &str = "EN 13201-2:2015 as reproduced in Stockmar, Handbook about Interpretation of EN 13201, Table 5.1, p. 7";
/// Citation of [`TABLE_5_2_C`].
pub const CITATION_C: &str = "EN 13201-2:2015 as reproduced in Stockmar, Handbook about Interpretation of EN 13201, Table 5.2, p. 9";
/// Citation of [`TABLE_5_3_P`].
pub const CITATION_P: &str = "EN 13201-2:2015 as reproduced in Stockmar, Handbook about Interpretation of EN 13201, Table 5.3, p. 10";

/// A lighting class M (roads for motorised traffic): one row of [`TABLE_5_1_M`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MClass {
    pub label: &'static str,
    /// Maintained average road surface luminance, dry, `L_av`, cd/m2.
    pub l_av_cd_m2: f64,
    /// Overall uniformity of road surface luminance, dry, `U_o`.
    pub u_o: f64,
    /// Longitudinal uniformity of road surface luminance, dry, `U_l`.
    pub u_l: f64,
    /// Overall uniformity, wet. Applicable in addition to the dry condition where road surfaces
    /// are wet for a substantial part of the hours of darkness and appropriate road surface
    /// reflectance data are available (footnote 1 of the table).
    pub u_o_wet: f64,
    /// Threshold increment `TI`, maximum, per cent.
    pub ti_max_percent: f64,
    /// Edge illuminance ratio `EIR`, minimum.
    pub eir_min: f64,
}

/// Table 5.1 "Requirements for lighting classes M for motorized traffic", page 7 of the handbook
/// (EN 13201-2:2015 as reproduced there). Columns: `L_av` in cd/m2, `U_o`, `U_l`, wet `U_o`,
/// `TI` in %, `EIR`.
pub const TABLE_5_1_M: [MClass; 6] = [
    MClass { label: "M1", l_av_cd_m2: 2.00, u_o: 0.40, u_l: 0.70, u_o_wet: 0.15, ti_max_percent: 10.0, eir_min: 0.35 },
    MClass { label: "M2", l_av_cd_m2: 1.50, u_o: 0.40, u_l: 0.70, u_o_wet: 0.15, ti_max_percent: 10.0, eir_min: 0.35 },
    MClass { label: "M3", l_av_cd_m2: 1.00, u_o: 0.40, u_l: 0.60, u_o_wet: 0.15, ti_max_percent: 15.0, eir_min: 0.30 },
    MClass { label: "M4", l_av_cd_m2: 0.75, u_o: 0.40, u_l: 0.60, u_o_wet: 0.15, ti_max_percent: 15.0, eir_min: 0.30 },
    MClass { label: "M5", l_av_cd_m2: 0.50, u_o: 0.35, u_l: 0.40, u_o_wet: 0.15, ti_max_percent: 15.0, eir_min: 0.30 },
    MClass { label: "M6", l_av_cd_m2: 0.30, u_o: 0.35, u_l: 0.40, u_o_wet: 0.15, ti_max_percent: 20.0, eir_min: 0.30 },
];

/// A lighting class C (conflict areas): one row of [`TABLE_5_2_C`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CClass {
    pub label: &'static str,
    /// Maintained average illuminance over the whole of the used surface, `E_av`, lx.
    pub e_av_lx: f64,
    /// Uniformity of illuminance `U_o(E)`.
    pub u_o: f64,
    /// Threshold increment, maximum, per cent, for very high, high or moderate speed. The handbook
    /// calls the threshold increment an informative criterion for the C classes.
    pub ti_max_high_speed_percent: f64,
    /// Threshold increment, maximum, per cent, for low or very low speed.
    pub ti_max_low_speed_percent: f64,
}

/// Table 5.2 "Requirements for lighting classes C for conflict areas", page 9 of the handbook
/// (EN 13201-2:2015 as reproduced there). Footnote 1: the threshold increment is applicable where
/// the visual tasks usually considered for the lighting of roads for motorised traffic are of
/// importance. The handbook adds that EN 13201-2:2015 recommends the limits for lower design
/// speeds.
pub const TABLE_5_2_C: [CClass; 6] = [
    CClass { label: "C0", e_av_lx: 50.0, u_o: 0.40, ti_max_high_speed_percent: 10.0, ti_max_low_speed_percent: 15.0 },
    CClass { label: "C1", e_av_lx: 30.0, u_o: 0.40, ti_max_high_speed_percent: 10.0, ti_max_low_speed_percent: 15.0 },
    CClass { label: "C2", e_av_lx: 20.0, u_o: 0.40, ti_max_high_speed_percent: 10.0, ti_max_low_speed_percent: 15.0 },
    CClass { label: "C3", e_av_lx: 15.0, u_o: 0.40, ti_max_high_speed_percent: 15.0, ti_max_low_speed_percent: 20.0 },
    CClass { label: "C4", e_av_lx: 10.0, u_o: 0.40, ti_max_high_speed_percent: 15.0, ti_max_low_speed_percent: 20.0 },
    CClass { label: "C5", e_av_lx: 7.5, u_o: 0.40, ti_max_high_speed_percent: 15.0, ti_max_low_speed_percent: 20.0 },
];

/// A lighting class P (pedestrian and low speed areas): one row of [`TABLE_5_3_P`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PClass {
    pub label: &'static str,
    /// Maintained average horizontal illuminance `E_h,av`, lx.
    pub e_h_av_lx: f64,
    /// Maintained minimum horizontal illuminance `E_h,min`, lx.
    pub e_h_min_lx: f64,
    /// Threshold increment, maximum, per cent. **Added by the handbook** to its "corrected and
    /// modified" Table 5.3 as a recommendation (it cites CIE 115:2010 annex D); not a requirement
    /// of EN 13201-2:2015 as the handbook reproduces it.
    pub ti_max_percent: f64,
    /// Additional requirement if facial recognition is necessary: minimum vertical illuminance
    /// `E_v,min`, lx.
    pub e_v_min_lx: f64,
    /// Additional requirement if facial recognition is necessary: minimum semi-cylindrical
    /// illuminance `E_sc,min`, lx. The handbook prints "1,5" for P3 (decimal comma); read as 1.5,
    /// which also fits the other rows.
    pub e_sc_min_lx: f64,
}

/// Table 5.3 "Requirements for lighting classes P for pedestrian and low speed areas", page 10 of
/// the handbook. P7 is "performance not determined" and has no row (see [`ClassRequirements::P7`]).
pub const TABLE_5_3_P: [PClass; 6] = [
    PClass { label: "P1", e_h_av_lx: 15.0, e_h_min_lx: 3.0, ti_max_percent: 20.0, e_v_min_lx: 5.0, e_sc_min_lx: 3.0 },
    PClass { label: "P2", e_h_av_lx: 10.0, e_h_min_lx: 2.0, ti_max_percent: 25.0, e_v_min_lx: 3.0, e_sc_min_lx: 2.0 },
    PClass { label: "P3", e_h_av_lx: 7.5, e_h_min_lx: 1.5, ti_max_percent: 25.0, e_v_min_lx: 2.5, e_sc_min_lx: 1.5 },
    PClass { label: "P4", e_h_av_lx: 5.0, e_h_min_lx: 1.0, ti_max_percent: 30.0, e_v_min_lx: 1.5, e_sc_min_lx: 1.0 },
    PClass { label: "P5", e_h_av_lx: 3.0, e_h_min_lx: 0.6, ti_max_percent: 30.0, e_v_min_lx: 1.0, e_sc_min_lx: 0.6 },
    PClass { label: "P6", e_h_av_lx: 2.0, e_h_min_lx: 0.4, ti_max_percent: 35.0, e_v_min_lx: 0.6, e_sc_min_lx: 0.4 },
];

/// Note under Table 5.3, page 10: "To provide for uniformity the actual value of the maintained
/// average illuminance shall not exceed 1.5 times the value indicated for the class."
pub const P_E_AV_MAX_FACTOR: f64 = 1.5;

/// What a class label resolves to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClassRequirements {
    M(&'static MClass),
    C(&'static CClass),
    P(&'static PClass),
    /// Class P7: the handbook's Table 5.3 says "performance not determined".
    P7,
}

impl ClassRequirements {
    /// The handbook citation of the table the class is in.
    pub fn citation(&self) -> &'static str {
        match self {
            ClassRequirements::M(_) => CITATION_M,
            ClassRequirements::C(_) => CITATION_C,
            ClassRequirements::P(_) | ClassRequirements::P7 => CITATION_P,
        }
    }
}

/// The requirements of a lighting class by its label: "M1" to "M6", "C0" to "C5", "P1" to "P7".
/// Case and surrounding spaces are ignored; anything else (for example the "ME3a" of
/// BS 5489-1:2003, or "M7") is `None`.
pub fn lookup(label: &str) -> Option<ClassRequirements> {
    let l = label.trim().to_uppercase();
    if l == "P7" {
        return Some(ClassRequirements::P7);
    }
    TABLE_5_1_M
        .iter()
        .find(|c| c.label == l)
        .map(ClassRequirements::M)
        .or_else(|| TABLE_5_2_C.iter().find(|c| c.label == l).map(ClassRequirements::C))
        .or_else(|| TABLE_5_3_P.iter().find(|c| c.label == l).map(ClassRequirements::P))
}

/// Class selection by the weighting parameters of **CEN/TR 13201-1:2014**, as reproduced in the
/// handbook (Tables 4.1, 4.2 and 4.3, pages 4 to 6).
///
/// **This is a suggestion from a technical report, never a requirement.** CEN/TR 13201-1 is
/// guidance, national bodies are not bound to adopt it (handbook, page 4 of the strategy part),
/// the handbook calls it "not written in a form which would allow national standardization
/// organizations to implement it" (page 22), and TD 501 hands the class selection to
/// BS 5489-1. The result is the engineer's to confirm.
///
/// Method (handbook, pages 5, 6 and 7): select one option per parameter, add the weighting
/// values, and the class number is **6 minus the sum**. The handbook says careful selection gives
/// class numbers 1 to 6 for M, 0 to 5 for C and 1 to 6 for P; a sum outside that gives no class
/// here (it is not clamped).
///
/// The handbook leaves out the descriptions and example values of the options ("without values
/// given as examples in the column of description"), and gives the speed ranges only as examples
/// (section 7.1). The options are therefore enumerations the engineer chooses, and no speed or
/// traffic figure is converted to an option here.
pub mod selection {
    /// The notice that goes with a suggested class.
    pub const SUGGESTION_NOTICE: &str = "Suggestion only, from CEN/TR 13201-1:2014 as reproduced in Stockmar, Handbook about Interpretation of EN 13201 (a technical report, not a requirement; not read in the original): the class is the engineer's to select and confirm.";

    /// Design speed options of Tables 4.1 and 4.2.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DesignSpeed {
        VeryHigh,
        High,
        Moderate,
        Low,
    }

    /// Traffic volume options of Tables 4.1 and 4.2.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TrafficVolume {
        High,
        Moderate,
        Low,
    }

    /// Traffic composition options of Tables 4.1 and 4.2.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TrafficComposition {
        MixedWithHighPercentageOfNonMotorized,
        Mixed,
        MotorizedOnly,
    }

    /// Junction density options of Table 4.1 (M only). The handbook (section 7.5, page 16) reports
    /// the technical report as rating it high above three intersections per kilometre.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum JunctionDensity {
        High,
        Moderate,
    }

    /// Ambient luminance options of Tables 4.1, 4.2 and 4.3.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AmbientLuminance {
        High,
        Moderate,
        Low,
    }

    /// Navigational task options of Tables 4.1 and 4.2.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum NavigationalTask {
        VeryDifficult,
        Difficult,
        Easy,
    }

    /// Design speed options of Table 4.3 (P).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PedestrianSpeed {
        Low,
        /// Walking speed.
        VeryLow,
    }

    /// Use intensity options of Table 4.3.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum UseIntensity {
        Busy,
        Normal,
        Quiet,
    }

    /// Traffic composition options of Table 4.3.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PedestrianTraffic {
        PedestriansCyclistsAndMotorized,
        PedestriansAndMotorized,
        PedestriansAndCyclistsOnly,
        PedestriansOnly,
        CyclistsOnly,
    }

    /// Weight of a design speed option in Table 4.1 (M), page 4: very high 2, high 1, moderate -1,
    /// low -2.
    pub fn weight_m_speed(v: DesignSpeed) -> i32 {
        match v {
            DesignSpeed::VeryHigh => 2,
            DesignSpeed::High => 1,
            DesignSpeed::Moderate => -1,
            DesignSpeed::Low => -2,
        }
    }

    /// Weight of a design speed option in Table 4.2 (C), page 5: very high 3, high 2, moderate 0,
    /// low -1.
    pub fn weight_c_speed(v: DesignSpeed) -> i32 {
        match v {
            DesignSpeed::VeryHigh => 3,
            DesignSpeed::High => 2,
            DesignSpeed::Moderate => 0,
            DesignSpeed::Low => -1,
        }
    }

    /// Traffic volume, Tables 4.1 and 4.2: high 1, moderate 0, low -1.
    pub fn weight_volume(v: TrafficVolume) -> i32 {
        match v {
            TrafficVolume::High => 1,
            TrafficVolume::Moderate => 0,
            TrafficVolume::Low => -1,
        }
    }

    /// Traffic composition, Tables 4.1 and 4.2: mixed with high percentage of non-motorized 2,
    /// mixed 1, motorized only 0.
    pub fn weight_composition(v: TrafficComposition) -> i32 {
        match v {
            TrafficComposition::MixedWithHighPercentageOfNonMotorized => 2,
            TrafficComposition::Mixed => 1,
            TrafficComposition::MotorizedOnly => 0,
        }
    }

    /// Separation of carriageway, Tables 4.1 and 4.2: no 1, yes 0.
    pub fn weight_separation(separated: bool) -> i32 {
        i32::from(!separated)
    }

    /// Junction density, Table 4.1: high 1, moderate 0.
    pub fn weight_junctions(v: JunctionDensity) -> i32 {
        match v {
            JunctionDensity::High => 1,
            JunctionDensity::Moderate => 0,
        }
    }

    /// Parked vehicles, Tables 4.1, 4.2 and 4.3: present 1, not present 0.
    pub fn weight_parked(present: bool) -> i32 {
        i32::from(present)
    }

    /// Ambient luminance, Tables 4.1, 4.2 and 4.3: high 1, moderate 0, low -1.
    pub fn weight_ambient(v: AmbientLuminance) -> i32 {
        match v {
            AmbientLuminance::High => 1,
            AmbientLuminance::Moderate => 0,
            AmbientLuminance::Low => -1,
        }
    }

    /// Navigational task, Tables 4.1 and 4.2: very difficult 2, difficult 1, easy 0.
    pub fn weight_navigation(v: NavigationalTask) -> i32 {
        match v {
            NavigationalTask::VeryDifficult => 2,
            NavigationalTask::Difficult => 1,
            NavigationalTask::Easy => 0,
        }
    }

    /// Design speed, Table 4.3 (P), page 6: low 1, very low (walking speed) 0.
    pub fn weight_p_speed(v: PedestrianSpeed) -> i32 {
        match v {
            PedestrianSpeed::Low => 1,
            PedestrianSpeed::VeryLow => 0,
        }
    }

    /// Use intensity, Table 4.3: busy 1, normal 0, quiet -1.
    pub fn weight_use_intensity(v: UseIntensity) -> i32 {
        match v {
            UseIntensity::Busy => 1,
            UseIntensity::Normal => 0,
            UseIntensity::Quiet => -1,
        }
    }

    /// Traffic composition, Table 4.3: pedestrians, cyclists and motorized traffic 2; pedestrians
    /// and motorized traffic 1; pedestrians and cyclists only 1; pedestrians only 0; cyclists
    /// only 0.
    pub fn weight_p_traffic(v: PedestrianTraffic) -> i32 {
        match v {
            PedestrianTraffic::PedestriansCyclistsAndMotorized => 2,
            PedestrianTraffic::PedestriansAndMotorized => 1,
            PedestrianTraffic::PedestriansAndCyclistsOnly => 1,
            PedestrianTraffic::PedestriansOnly => 0,
            PedestrianTraffic::CyclistsOnly => 0,
        }
    }

    /// The options chosen for a lighting class M (Table 4.1, page 4).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MSelection {
        pub design_speed: DesignSpeed,
        pub traffic_volume: TrafficVolume,
        pub traffic_composition: TrafficComposition,
        /// Whether the carriageways are separated.
        pub carriageway_separated: bool,
        pub junction_density: JunctionDensity,
        pub parked_vehicles_present: bool,
        pub ambient_luminance: AmbientLuminance,
        pub navigational_task: NavigationalTask,
    }

    /// The options chosen for a lighting class C (Table 4.2, page 5). There is no junction
    /// density parameter.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CSelection {
        pub design_speed: DesignSpeed,
        pub traffic_volume: TrafficVolume,
        pub traffic_composition: TrafficComposition,
        pub carriageway_separated: bool,
        pub parked_vehicles_present: bool,
        pub ambient_luminance: AmbientLuminance,
        pub navigational_task: NavigationalTask,
    }

    /// The options chosen for a lighting class P (Table 4.3, page 6).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PSelection {
        pub design_speed: PedestrianSpeed,
        pub use_intensity: UseIntensity,
        pub traffic: PedestrianTraffic,
        pub parked_vehicles_present: bool,
        pub ambient_luminance: AmbientLuminance,
        /// Facial recognition necessary: "additional requirements" (the vertical and
        /// semi-cylindrical illuminances of the class), not a weighting value.
        pub facial_recognition_necessary: bool,
    }

    /// The selection of any of the three families, for [`crate::design::CheckInputs`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ClassSelectionInput {
        M(MSelection),
        C(CSelection),
        P(PSelection),
    }

    impl ClassSelectionInput {
        pub fn suggest(&self) -> Suggestion {
            match self {
                ClassSelectionInput::M(s) => suggest_m(s),
                ClassSelectionInput::C(s) => suggest_c(s),
                ClassSelectionInput::P(s) => suggest_p(s),
            }
        }
    }

    /// A class suggested from the weighting values.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Suggestion {
        /// "M", "C" or "P".
        pub family: char,
        /// Sum of the weighting values.
        pub weight_sum: i32,
        /// 6 minus the sum.
        pub class_number: i32,
        /// The class label when the number is within the family's range, else `None`.
        pub class: Option<String>,
        /// For P: whether facial recognition is necessary and so the vertical and semi-cylindrical
        /// illuminances apply. `None` for M and C.
        pub facial_recognition_necessary: Option<bool>,
    }

    impl Suggestion {
        /// The suggestion in words, with [`SUGGESTION_NOTICE`].
        pub fn text(&self) -> String {
            let range = match self.family {
                'C' => "C0 to C5",
                'P' => "P1 to P6",
                _ => "M1 to M6",
            };
            let outcome = match &self.class {
                Some(c) => format!("suggests class {c}"),
                None => format!("gives class number {}, outside {range}: no class is suggested", self.class_number),
            };
            let facial = match self.facial_recognition_necessary {
                Some(true) => "; facial recognition is necessary, so the vertical and semi-cylindrical illuminances of the class apply",
                Some(false) => "; no additional facial recognition requirements",
                None => "",
            };
            format!("Weighting values sum to {}; 6 minus {} = {}: {outcome}{facial}. {SUGGESTION_NOTICE}", self.weight_sum, self.weight_sum, self.class_number)
        }
    }

    fn suggestion(family: char, weight_sum: i32, lowest: i32, highest: i32, facial: Option<bool>) -> Suggestion {
        let class_number = 6 - weight_sum;
        let class = (lowest..=highest).contains(&class_number).then(|| format!("{family}{class_number}"));
        Suggestion { family, weight_sum, class_number, class, facial_recognition_necessary: facial }
    }

    /// Suggests a class M: 6 minus the sum of the Table 4.1 weighting values, M1 to M6.
    pub fn suggest_m(s: &MSelection) -> Suggestion {
        let sum = weight_m_speed(s.design_speed)
            + weight_volume(s.traffic_volume)
            + weight_composition(s.traffic_composition)
            + weight_separation(s.carriageway_separated)
            + weight_junctions(s.junction_density)
            + weight_parked(s.parked_vehicles_present)
            + weight_ambient(s.ambient_luminance)
            + weight_navigation(s.navigational_task);
        suggestion('M', sum, 1, 6, None)
    }

    /// Suggests a class C: 6 minus the sum of the Table 4.2 weighting values, C0 to C5.
    pub fn suggest_c(s: &CSelection) -> Suggestion {
        let sum = weight_c_speed(s.design_speed)
            + weight_volume(s.traffic_volume)
            + weight_composition(s.traffic_composition)
            + weight_separation(s.carriageway_separated)
            + weight_parked(s.parked_vehicles_present)
            + weight_ambient(s.ambient_luminance)
            + weight_navigation(s.navigational_task);
        suggestion('C', sum, 0, 5, None)
    }

    /// Suggests a class P: 6 minus the sum of the Table 4.3 weighting values, P1 to P6.
    pub fn suggest_p(s: &PSelection) -> Suggestion {
        let sum = weight_p_speed(s.design_speed)
            + weight_use_intensity(s.use_intensity)
            + weight_p_traffic(s.traffic)
            + weight_parked(s.parked_vehicles_present)
            + weight_ambient(s.ambient_luminance);
        suggestion('P', sum, 1, 6, Some(s.facial_recognition_necessary))
    }
}

#[cfg(test)]
mod tests {
    use super::selection::*;
    use super::*;

    #[test]
    fn table_5_1_m_values_are_those_of_the_handbook_page_7() {
        // (class, Lav cd/m2, Uo, Ul, wet Uo, TI %, EIR), read from the rendered page.
        let rows = [
            ("M1", 2.00, 0.40, 0.70, 0.15, 10.0, 0.35),
            ("M2", 1.50, 0.40, 0.70, 0.15, 10.0, 0.35),
            ("M3", 1.00, 0.40, 0.60, 0.15, 15.0, 0.30),
            ("M4", 0.75, 0.40, 0.60, 0.15, 15.0, 0.30),
            ("M5", 0.50, 0.35, 0.40, 0.15, 15.0, 0.30),
            ("M6", 0.30, 0.35, 0.40, 0.15, 20.0, 0.30),
        ];
        for (c, r) in TABLE_5_1_M.iter().zip(rows) {
            assert_eq!((c.label, c.l_av_cd_m2, c.u_o, c.u_l, c.u_o_wet, c.ti_max_percent, c.eir_min), r, "{}", r.0);
        }
        assert!(CITATION_M.contains("Table 5.1") && CITATION_M.contains("p. 7") && CITATION_M.contains("EN 13201-2:2015"));
    }

    #[test]
    fn table_5_2_c_values_are_those_of_the_handbook_page_9() {
        // (class, Eav lx, Uo, TI % high/moderate speed, TI % low/very low speed).
        let rows = [("C0", 50.0, 0.40, 10.0, 15.0), ("C1", 30.0, 0.40, 10.0, 15.0), ("C2", 20.0, 0.40, 10.0, 15.0), ("C3", 15.0, 0.40, 15.0, 20.0), ("C4", 10.0, 0.40, 15.0, 20.0), ("C5", 7.5, 0.40, 15.0, 20.0)];
        for (c, r) in TABLE_5_2_C.iter().zip(rows) {
            assert_eq!((c.label, c.e_av_lx, c.u_o, c.ti_max_high_speed_percent, c.ti_max_low_speed_percent), r, "{}", r.0);
        }
        assert!(CITATION_C.contains("Table 5.2") && CITATION_C.contains("p. 9"));
    }

    #[test]
    fn table_5_3_p_values_are_those_of_the_handbook_page_10() {
        // (class, Eh,av lx, Eh,min lx, TI %, Ev,min lx, Esc,min lx).
        let rows = [
            ("P1", 15.0, 3.0, 20.0, 5.0, 3.0),
            ("P2", 10.0, 2.0, 25.0, 3.0, 2.0),
            ("P3", 7.5, 1.5, 25.0, 2.5, 1.5),
            ("P4", 5.0, 1.0, 30.0, 1.5, 1.0),
            ("P5", 3.0, 0.6, 30.0, 1.0, 0.6),
            ("P6", 2.0, 0.4, 35.0, 0.6, 0.4),
        ];
        for (c, r) in TABLE_5_3_P.iter().zip(rows) {
            assert_eq!((c.label, c.e_h_av_lx, c.e_h_min_lx, c.ti_max_percent, c.e_v_min_lx, c.e_sc_min_lx), r, "{}", r.0);
        }
        assert_eq!(P_E_AV_MAX_FACTOR, 1.5);
        assert!(CITATION_P.contains("Table 5.3") && CITATION_P.contains("p. 10"));
        assert_eq!(lookup("P7"), Some(ClassRequirements::P7));
    }

    #[test]
    fn lookup_resolves_entered_labels() {
        match lookup("M3") {
            Some(ClassRequirements::M(c)) => assert_eq!((c.label, c.l_av_cd_m2, c.u_l), ("M3", 1.0, 0.6)),
            other => panic!("{other:?}"),
        }
        assert!(matches!(lookup(" c0 "), Some(ClassRequirements::C(c)) if c.e_av_lx == 50.0));
        assert!(matches!(lookup("p6"), Some(ClassRequirements::P(c)) if c.e_h_min_lx == 0.4));
        assert!(lookup("M3").unwrap().citation().contains("Table 5.1"));
        assert!(lookup("C5").unwrap().citation().contains("Table 5.2"));
        assert!(lookup("P1").unwrap().citation().contains("Table 5.3"));
        for bad in ["", "M0", "M7", "C6", "P8", "ME3a", "M 3", "S1", "CE2"] {
            assert_eq!(lookup(bad), None, "{bad}");
        }
    }

    #[test]
    fn selection_weights_are_those_of_tables_4_1_to_4_3() {
        // Table 4.1 (page 4).
        assert_eq!([DesignSpeed::VeryHigh, DesignSpeed::High, DesignSpeed::Moderate, DesignSpeed::Low].map(weight_m_speed), [2, 1, -1, -2]);
        assert_eq!([TrafficVolume::High, TrafficVolume::Moderate, TrafficVolume::Low].map(weight_volume), [1, 0, -1]);
        assert_eq!(
            [TrafficComposition::MixedWithHighPercentageOfNonMotorized, TrafficComposition::Mixed, TrafficComposition::MotorizedOnly].map(weight_composition),
            [2, 1, 0]
        );
        assert_eq!((weight_separation(false), weight_separation(true)), (1, 0));
        assert_eq!([JunctionDensity::High, JunctionDensity::Moderate].map(weight_junctions), [1, 0]);
        assert_eq!((weight_parked(true), weight_parked(false)), (1, 0));
        assert_eq!([AmbientLuminance::High, AmbientLuminance::Moderate, AmbientLuminance::Low].map(weight_ambient), [1, 0, -1]);
        assert_eq!([NavigationalTask::VeryDifficult, NavigationalTask::Difficult, NavigationalTask::Easy].map(weight_navigation), [2, 1, 0]);
        // Table 4.2 (page 5): only the design speed differs.
        assert_eq!([DesignSpeed::VeryHigh, DesignSpeed::High, DesignSpeed::Moderate, DesignSpeed::Low].map(weight_c_speed), [3, 2, 0, -1]);
        // Table 4.3 (page 6).
        assert_eq!([PedestrianSpeed::Low, PedestrianSpeed::VeryLow].map(weight_p_speed), [1, 0]);
        assert_eq!([UseIntensity::Busy, UseIntensity::Normal, UseIntensity::Quiet].map(weight_use_intensity), [1, 0, -1]);
        assert_eq!(
            [
                PedestrianTraffic::PedestriansCyclistsAndMotorized,
                PedestrianTraffic::PedestriansAndMotorized,
                PedestrianTraffic::PedestriansAndCyclistsOnly,
                PedestrianTraffic::PedestriansOnly,
                PedestrianTraffic::CyclistsOnly
            ]
            .map(weight_p_traffic),
            [2, 1, 1, 0, 0]
        );
    }

    fn m() -> MSelection {
        MSelection {
            design_speed: DesignSpeed::High,
            traffic_volume: TrafficVolume::Moderate,
            traffic_composition: TrafficComposition::Mixed,
            carriageway_separated: true,
            junction_density: JunctionDensity::Moderate,
            parked_vehicles_present: true,
            ambient_luminance: AmbientLuminance::Moderate,
            navigational_task: NavigationalTask::Easy,
        }
    }

    #[test]
    fn six_minus_the_sum_gives_the_class() {
        // Hand-worked: high speed 1 + moderate volume 0 + mixed 1 + separated 0 + moderate
        // junctions 0 + parked 1 + moderate ambient 0 + easy 0 = 3; 6 - 3 = 3.
        let s = suggest_m(&m());
        assert_eq!((s.weight_sum, s.class_number, s.class.as_deref()), (3, 3, Some("M3")));
        let text = s.text();
        assert!(text.contains("6 minus 3 = 3") && text.contains("suggests class M3") && text.contains("Suggestion only") && text.contains("technical report"));
        // Moderate speed -1 alone (parked not present, mixed removed): sum 0 gives M6.
        let low = MSelection { design_speed: DesignSpeed::Moderate, traffic_composition: TrafficComposition::Mixed, parked_vehicles_present: false, ..m() };
        assert_eq!(suggest_m(&low).class.as_deref(), Some("M6"));
        // Most onerous: 2 + 1 + 2 + 1 + 1 + 1 + 1 + 2 = 11; 6 - 11 = -5: no class.
        let worst = MSelection {
            design_speed: DesignSpeed::VeryHigh,
            traffic_volume: TrafficVolume::High,
            traffic_composition: TrafficComposition::MixedWithHighPercentageOfNonMotorized,
            carriageway_separated: false,
            junction_density: JunctionDensity::High,
            parked_vehicles_present: true,
            ambient_luminance: AmbientLuminance::High,
            navigational_task: NavigationalTask::VeryDifficult,
        };
        let s = suggest_m(&worst);
        assert_eq!((s.weight_sum, s.class_number, s.class.clone()), (11, -5, None));
        assert!(s.text().contains("no class is suggested"));
        // Edges of the M range: sum 5 gives M1, sum 6 gives class number 0, which M does not have.
        let five = MSelection { design_speed: DesignSpeed::VeryHigh, traffic_volume: TrafficVolume::High, parked_vehicles_present: true, ..m() };
        // 2 + 1 + 1 + 0 + 0 + 1 + 0 + 0 = 5
        assert_eq!(suggest_m(&five).class.as_deref(), Some("M1"));
        let six = MSelection { traffic_composition: TrafficComposition::MixedWithHighPercentageOfNonMotorized, ..five };
        assert_eq!((suggest_m(&six).class_number, suggest_m(&six).class), (0, None));
        // Lowest: -2 - 1 + 0 + 0 + 0 + 0 - 1 + 0 = -4; class number 10: no class.
        let lowest = MSelection {
            design_speed: DesignSpeed::Low,
            traffic_volume: TrafficVolume::Low,
            traffic_composition: TrafficComposition::MotorizedOnly,
            carriageway_separated: true,
            junction_density: JunctionDensity::Moderate,
            parked_vehicles_present: false,
            ambient_luminance: AmbientLuminance::Low,
            navigational_task: NavigationalTask::Easy,
        };
        assert_eq!((suggest_m(&lowest).class_number, suggest_m(&lowest).class), (10, None));
    }

    #[test]
    fn c_and_p_selection() {
        // C: very high 3 + high volume 1 + mixed 1 + not separated 1 + parked 1 + moderate 0 +
        // difficult 1 = 8; 6 - 8 = -2 (no class). With moderate speed 0: sum 5, C1.
        let c = CSelection {
            design_speed: DesignSpeed::VeryHigh,
            traffic_volume: TrafficVolume::High,
            traffic_composition: TrafficComposition::Mixed,
            carriageway_separated: false,
            parked_vehicles_present: true,
            ambient_luminance: AmbientLuminance::Moderate,
            navigational_task: NavigationalTask::Difficult,
        };
        assert_eq!((suggest_c(&c).weight_sum, suggest_c(&c).class), (8, None));
        let c = CSelection { design_speed: DesignSpeed::Moderate, ..c };
        assert_eq!((suggest_c(&c).weight_sum, suggest_c(&c).class.as_deref()), (5, Some("C1")));
        // Sum 6 gives C0, the class M does not have; sum 1 gives C5; sum 0 gives 6, none.
        let c0 = CSelection { traffic_composition: TrafficComposition::MixedWithHighPercentageOfNonMotorized, ..c };
        assert_eq!(suggest_c(&c0).class.as_deref(), Some("C0"));
        let c5 = CSelection {
            design_speed: DesignSpeed::Moderate,
            traffic_volume: TrafficVolume::Moderate,
            traffic_composition: TrafficComposition::Mixed,
            carriageway_separated: true,
            parked_vehicles_present: false,
            ambient_luminance: AmbientLuminance::Moderate,
            navigational_task: NavigationalTask::Easy,
        };
        assert_eq!(suggest_c(&c5).class.as_deref(), Some("C5"));
        assert_eq!(suggest_c(&CSelection { traffic_composition: TrafficComposition::MotorizedOnly, ..c5 }).class, None);

        // P: low 1 + normal 0 + pedestrians and cyclists 1 + parked 0 + moderate 0 = 2; P4.
        let p = PSelection {
            design_speed: PedestrianSpeed::Low,
            use_intensity: UseIntensity::Normal,
            traffic: PedestrianTraffic::PedestriansAndCyclistsOnly,
            parked_vehicles_present: false,
            ambient_luminance: AmbientLuminance::Moderate,
            facial_recognition_necessary: true,
        };
        let s = suggest_p(&p);
        assert_eq!((s.weight_sum, s.class.as_deref(), s.facial_recognition_necessary), (2, Some("P4"), Some(true)));
        assert!(s.text().contains("vertical and semi-cylindrical"));
        // Busy, all traffic, parked, high ambient: 1 + 1 + 2 + 1 + 1 = 6; class number 0: none.
        let busy = PSelection {
            use_intensity: UseIntensity::Busy,
            traffic: PedestrianTraffic::PedestriansCyclistsAndMotorized,
            parked_vehicles_present: true,
            ambient_luminance: AmbientLuminance::High,
            facial_recognition_necessary: false,
            ..p
        };
        assert_eq!((suggest_p(&busy).class_number, suggest_p(&busy).class), (0, None));
        // Quiet, walking speed, pedestrians only, low ambient: -1 - 1 = -2; class number 8: none; sum 5 is P1.
        let quiet = PSelection { design_speed: PedestrianSpeed::VeryLow, use_intensity: UseIntensity::Quiet, traffic: PedestrianTraffic::PedestriansOnly, parked_vehicles_present: false, ambient_luminance: AmbientLuminance::Low, ..p };
        assert_eq!((suggest_p(&quiet).class_number, suggest_p(&quiet).class), (8, None));
        let five = PSelection { use_intensity: UseIntensity::Busy, traffic: PedestrianTraffic::PedestriansCyclistsAndMotorized, parked_vehicles_present: true, ambient_luminance: AmbientLuminance::Moderate, ..p };
        // 1 + 1 + 2 + 1 + 0 = 5
        assert_eq!(suggest_p(&five).class.as_deref(), Some("P1"));
        assert_eq!(ClassSelectionInput::P(five).suggest(), suggest_p(&five));
    }
}
