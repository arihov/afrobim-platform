//! The performance classes a road restraint system is specified by.
//!
//! BS EN 1317 itself is not held in this repository. Every value here is taken from a freely
//! published official document that reproduces it, named on the item:
//!
//! * **CD 377** = DMRB CD 377 *Requirements for road restraint systems*, Version 5.0.1
//!   (National Highways, February 2026). It names the classes it requires (N1, N2, H1, H2, H4a;
//!   ISL A, B, C; "W1, W2, W3, etc"; terminal classes P1, P4, T80/3, T110, T110/1) but does not
//!   reproduce the BS EN 1317-2 tables.
//! * **TII DN-REQ-03034** = Transport Infrastructure Ireland, *Safety Barriers* DN-REQ-03034
//!   (November 2015, including Amendment No. 1 of January 2016; formerly NRA TD 19/15). Its
//!   Tables 5/1, 5/2 and 5/3 reproduce the containment levels, impact severity levels and
//!   working width classes of EN 1317-2 and say so ("Source: IS EN 1317-2").

use serde::{Deserialize, Serialize};

/// The three groups CD 377 speaks of ("normal", "higher" and "very high containment level";
/// e.g. clauses 3.14, 3.17) and under which TII DN-REQ-03034 Table 5/1 lists the levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContainmentGroup {
    Normal,
    Higher,
    VeryHigh,
}

/// Containment level of a vehicle restraint system (EN 1317-2), as listed in
/// TII DN-REQ-03034 Table 5/1. CD 377 5.0.1 itself names N1, N2 (3.6, 3.7), H1 and H4a
/// (Table 3.48) and H2 (7.10 NOTE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContainmentLevel {
    N1,
    N2,
    H1,
    L1,
    H2,
    L2,
    H3,
    L3,
    H4a,
    H4b,
    L4a,
    L4b,
}

impl ContainmentLevel {
    /// Every level, in the order of TII DN-REQ-03034 Table 5/1.
    pub const ALL: [ContainmentLevel; 12] = {
        use ContainmentLevel::*;
        [N1, N2, H1, L1, H2, L2, H3, L3, H4a, H4b, L4a, L4b]
    };

    pub fn label(self) -> &'static str {
        use ContainmentLevel::*;
        match self {
            N1 => "N1",
            N2 => "N2",
            H1 => "H1",
            L1 => "L1",
            H2 => "H2",
            L2 => "L2",
            H3 => "H3",
            L3 => "L3",
            H4a => "H4a",
            H4b => "H4b",
            L4a => "L4a",
            L4b => "L4b",
        }
    }

    /// The group the level is listed under in TII DN-REQ-03034 Table 5/1.
    pub fn group(self) -> ContainmentGroup {
        use ContainmentLevel::*;
        match self {
            N1 | N2 => ContainmentGroup::Normal,
            H1 | L1 | H2 | L2 | H3 | L3 => ContainmentGroup::Higher,
            H4a | H4b | L4a | L4b => ContainmentGroup::VeryHigh,
        }
    }

    /// Position in the listing of Table 5/1: N1 = 1, N2 = 2, H1/L1 = 3, H2/L2 = 4, H3/L3 = 5,
    /// the four very high levels = 6.
    fn step(self) -> u8 {
        use ContainmentLevel::*;
        match self {
            N1 => 1,
            N2 => 2,
            H1 | L1 => 3,
            H2 | L2 => 4,
            H3 | L3 => 5,
            H4a | H4b | L4a | L4b => 6,
        }
    }

    fn is_l(self) -> bool {
        use ContainmentLevel::*;
        matches!(self, L1 | L2 | L3 | L4a | L4b)
    }

    fn is_b(self) -> bool {
        matches!(self, ContainmentLevel::H4b | ContainmentLevel::L4b)
    }

    /// Whether a barrier of this level satisfies a requirement for `required`.
    ///
    /// **This ordering is a stated convention, not a table copied from a standard.** Neither
    /// CD 377 nor the TII reproduction prints the hierarchy of EN 1317-2. What they do give:
    ///
    /// * CD 377 treats the levels as ordered ("a minimum containment level greater than N2",
    ///   3.6; an N2 barrier joined to an H2 parapet takes a transition of "N2 or greater",
    ///   7.10 NOTE; H1 and H4a as minima above N2, Table 3.48). So a level further down the
    ///   listing of Table 5/1 is taken to satisfy one further up.
    /// * An L level is its H level's test plus the car test TB 32 (TII Table 5/1), and the
    ///   CD 377 Scotland annex S/3.1.1 allows an L level in place of an H level. So `Ln`
    ///   satisfies `Hn`; `Hn` does not satisfy `Ln`.
    /// * H4a and H4b are tested with different vehicles (30 t rigid, 38 t articulated). No
    ///   source held here ranks them, so an "a" level against a "b" level gives `None`.
    ///
    /// `None` means "not comparable from the sources held: the engineer decides".
    pub fn meets(self, required: ContainmentLevel) -> Option<bool> {
        if self == required {
            return Some(true);
        }
        if self.step() != required.step() {
            return Some(self.step() > required.step());
        }
        if self.step() == 6 && self.is_b() != required.is_b() {
            return None;
        }
        Some(self.is_l() && !required.is_l())
    }
}

impl std::fmt::Display for ContainmentLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Working width class (EN 1317-2), W1 to W8, with the levels of TII DN-REQ-03034 Table 5/3.
///
/// CD 377 5.0.1 requires the "normalised working width class" (3.13 to 3.18) and defines the
/// class as "the designation W1, W2, W3, etc" but prints no widths. Its clauses 3.15 and 3.18
/// use 0.6 m as the smallest class value, which agrees with W1 here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WorkingWidthClass {
    W1,
    W2,
    W3,
    W4,
    W5,
    W6,
    W7,
    W8,
}

impl WorkingWidthClass {
    pub const ALL: [WorkingWidthClass; 8] = {
        use WorkingWidthClass::*;
        [W1, W2, W3, W4, W5, W6, W7, W8]
    };

    pub fn label(self) -> &'static str {
        use WorkingWidthClass::*;
        match self {
            W1 => "W1",
            W2 => "W2",
            W3 => "W3",
            W4 => "W4",
            W5 => "W5",
            W6 => "W6",
            W7 => "W7",
            W8 => "W8",
        }
    }

    /// The level of the class: the working width is not more than this, in metres.
    /// TII DN-REQ-03034 Table 5/3 (source IS EN 1317-2): W1 ≤ 0.6, W2 ≤ 0.8, W3 ≤ 1.0,
    /// W4 ≤ 1.3, W5 ≤ 1.7, W6 ≤ 2.1, W7 ≤ 2.5, W8 ≤ 3.5.
    pub fn level_m(self) -> f64 {
        use WorkingWidthClass::*;
        match self {
            W1 => 0.6,
            W2 => 0.8,
            W3 => 1.0,
            W4 => 1.3,
            W5 => 1.7,
            W6 => 2.1,
            W7 => 2.5,
            W8 => 3.5,
        }
    }

    /// The widest class whose level fits within `available_m`, measured from the traffic face
    /// of the barrier; `None` when even W1 does not fit (CD 377 3.15: the value itself then
    /// becomes the design output).
    pub fn widest_within(available_m: f64) -> Option<WorkingWidthClass> {
        Self::ALL.iter().rev().copied().find(|c| c.level_m() <= available_m + 1e-9)
    }
}

impl std::fmt::Display for WorkingWidthClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Theoretical head impact velocity limit common to the three impact severity levels, km/h.
/// TII DN-REQ-03034 Table 5/2 (source IS EN 1317-2).
pub const THIV_LIMIT_KMH: f64 = 33.0;

/// Impact severity level (CD 377 Terms and definitions; clauses 3.12, 6.14, 7.11, 7.12).
/// A is the least severe for the occupants (CD 377 Terms, Notes 8 and 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ImpactSeverityLevel {
    A,
    B,
    C,
}

impl ImpactSeverityLevel {
    pub fn label(self) -> &'static str {
        match self {
            ImpactSeverityLevel::A => "A",
            ImpactSeverityLevel::B => "B",
            ImpactSeverityLevel::C => "C",
        }
    }

    /// The acceleration severity index the level allows: A ≤ 1.0, B ≤ 1.4, C ≤ 1.9.
    /// TII DN-REQ-03034 Table 5/2 (source IS EN 1317-2). Informative only: no check in this
    /// crate uses it, since ASI is a test result of the product.
    pub fn asi_limit(self) -> f64 {
        match self {
            ImpactSeverityLevel::A => 1.0,
            ImpactSeverityLevel::B => 1.4,
            ImpactSeverityLevel::C => 1.9,
        }
    }
}

impl std::fmt::Display for ImpactSeverityLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Performance class of a terminal, as CD 377 5.0.1 clauses 6.3 to 6.5 name them: P1 and P4
/// "as defined by DD ENV 1317-4:2002", T80/3, T110 and T110/1 "as defined by
/// PD CEN/TS 1317-7". Neither document is held here, so any other class is carried as text and
/// cannot be checked.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TerminalClass {
    P1,
    P4,
    #[serde(rename = "T80/3")]
    T80_3,
    T110,
    #[serde(rename = "T110/1")]
    T110_1,
    /// A class CD 377 does not name, as written on the product declaration.
    Other(String),
}

impl TerminalClass {
    pub fn label(&self) -> String {
        match self {
            TerminalClass::P1 => "P1".into(),
            TerminalClass::P4 => "P4".into(),
            TerminalClass::T80_3 => "T80/3".into(),
            TerminalClass::T110 => "T110".into(),
            TerminalClass::T110_1 => "T110/1".into(),
            TerminalClass::Other(s) => s.trim().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ContainmentLevel::*;

    #[test]
    fn working_width_levels_are_those_of_tii_table_5_3() {
        // TII DN-REQ-03034 (Nov 2015) Table 5/3, read from the rendered page.
        let table = [0.6, 0.8, 1.0, 1.3, 1.7, 2.1, 2.5, 3.5];
        for (class, level) in WorkingWidthClass::ALL.iter().zip(table) {
            assert_eq!(class.level_m(), level, "{class}");
        }
        assert_eq!(WorkingWidthClass::ALL.map(|c| c.label()), ["W1", "W2", "W3", "W4", "W5", "W6", "W7", "W8"]);
    }

    #[test]
    fn widest_class_within_a_clearance() {
        assert_eq!(WorkingWidthClass::widest_within(0.59), None); // CD 377 3.15: less than 0.6 m
        assert_eq!(WorkingWidthClass::widest_within(0.6), Some(WorkingWidthClass::W1));
        assert_eq!(WorkingWidthClass::widest_within(1.29), Some(WorkingWidthClass::W3));
        assert_eq!(WorkingWidthClass::widest_within(1.3), Some(WorkingWidthClass::W4));
        assert_eq!(WorkingWidthClass::widest_within(10.0), Some(WorkingWidthClass::W8));
    }

    #[test]
    fn impact_severity_limits_are_those_of_tii_table_5_2() {
        assert_eq!(ImpactSeverityLevel::A.asi_limit(), 1.0);
        assert_eq!(ImpactSeverityLevel::B.asi_limit(), 1.4);
        assert_eq!(ImpactSeverityLevel::C.asi_limit(), 1.9);
        assert_eq!(THIV_LIMIT_KMH, 33.0);
    }

    #[test]
    fn containment_levels_and_groups_are_those_of_tii_table_5_1() {
        let labels: Vec<&str> = ContainmentLevel::ALL.iter().map(|l| l.label()).collect();
        assert_eq!(labels, ["N1", "N2", "H1", "L1", "H2", "L2", "H3", "L3", "H4a", "H4b", "L4a", "L4b"]);
        assert!([N1, N2].iter().all(|l| l.group() == ContainmentGroup::Normal));
        assert!([H1, L1, H2, L2, H3, L3].iter().all(|l| l.group() == ContainmentGroup::Higher));
        assert!([H4a, H4b, L4a, L4b].iter().all(|l| l.group() == ContainmentGroup::VeryHigh));
    }

    #[test]
    fn containment_ordering_convention() {
        assert_eq!(N2.meets(N2), Some(true));
        assert_eq!(N1.meets(N2), Some(false));
        assert_eq!(N2.meets(N1), Some(true));
        // CD 377 7.10 NOTE: N2 is the lower of N2 and H2.
        assert_eq!(H2.meets(N2), Some(true));
        assert_eq!(N2.meets(H2), Some(false));
        // CD 377 Table 3.48: H1 and H4a are minima above N2.
        assert_eq!(H1.meets(N2), Some(true));
        assert_eq!(H4a.meets(H1), Some(true));
        assert_eq!(H1.meets(H4a), Some(false));
        // L in place of H (TII Table 5/1 tests; CD 377 Scotland annex S/3.1.1), not the reverse.
        assert_eq!(L2.meets(H2), Some(true));
        assert_eq!(H2.meets(L2), Some(false));
        assert_eq!(L4a.meets(H4a), Some(true));
        // a against b: not ranked by any source held.
        assert_eq!(H4b.meets(H4a), None);
        assert_eq!(L4b.meets(H4a), None);
        assert_eq!(H4a.meets(L4b), None);
    }

    #[test]
    fn classes_serialise_as_their_printed_names() {
        assert_eq!(serde_json::to_string(&H4a).unwrap(), "\"H4a\"");
        assert_eq!(serde_json::to_string(&TerminalClass::T80_3).unwrap(), "\"T80/3\"");
        assert_eq!(serde_json::to_string(&TerminalClass::T110_1).unwrap(), "\"T110/1\"");
        let other: TerminalClass = serde_json::from_str("{\"Other\":\"P2\"}").unwrap();
        assert_eq!(other.label(), "P2");
    }
}
