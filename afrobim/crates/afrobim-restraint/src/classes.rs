// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The performance classes a road restraint system is specified by.
//!
//! Sources, named on every item:
//!
//! * **BS EN 1317-2:2010** *Road restraint systems, Part 2: Performance classes, impact test
//!   acceptance criteria and test methods for safety barriers including vehicle parapets*
//!   (EN 1317-2:2010 (E)), from the owner's copy: Table 1 (vehicle impact tests), Table 2
//!   (containment levels) with its notes, Table 3 (impact severity levels), Table 4 (levels of
//!   normalised working width), Table 5 (levels of normalised vehicle intrusion). The 2010
//!   edition is the one read; whether it is still current is for the owner to confirm.
//! * **DD ENV 1317-4:2002** (the official English version of ENV 1317-4:2001; a Draft for
//!   Development, "not to be regarded as a British Standard"), Table 1 and clause 5.1: terminal
//!   performance classes P1 to P4.
//! * **CD 377** = DMRB CD 377 *Requirements for road restraint systems*, Version 5.0.1
//!   (February 2026), which names the classes it requires but reproduces none of these tables.
//!   Its terminal classes T80/3, T110 and T110/1 are "as defined by PD CEN/TS 1317-7", which is
//!   **not held**: they are carried as names only.
//!
//! The values of Tables 2 to 5 were also compared with their reproduction in Transport
//! Infrastructure Ireland DN-REQ-03034 *Safety Barriers* (November 2015), Tables 5/1 to 5/4,
//! and agree.

use serde::{Deserialize, Serialize};

/// The kind of vehicle of an impact test (BS EN 1317-2:2010 Table 1, "Type of vehicle").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TestVehicle {
    Car,
    RigidHgv,
    Bus,
    ArticulatedHgv,
}

/// A vehicle impact test of BS EN 1317-2:2010 Table 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ImpactTest {
    TB11,
    TB21,
    TB22,
    TB31,
    TB32,
    TB41,
    TB42,
    TB51,
    TB61,
    TB71,
    TB81,
}

impl ImpactTest {
    pub const ALL: [ImpactTest; 11] = {
        use ImpactTest::*;
        [TB11, TB21, TB22, TB31, TB32, TB41, TB42, TB51, TB61, TB71, TB81]
    };

    pub fn label(self) -> &'static str {
        use ImpactTest::*;
        match self {
            TB11 => "TB 11",
            TB21 => "TB 21",
            TB22 => "TB 22",
            TB31 => "TB 31",
            TB32 => "TB 32",
            TB41 => "TB 41",
            TB42 => "TB 42",
            TB51 => "TB 51",
            TB61 => "TB 61",
            TB71 => "TB 71",
            TB81 => "TB 81",
        }
    }

    /// Impact speed (km/h), impact angle (degrees), total mass (kg) and type of vehicle:
    /// BS EN 1317-2:2010 Table 1.
    pub fn description(self) -> (f64, f64, f64, TestVehicle) {
        use ImpactTest::*;
        use TestVehicle::*;
        match self {
            TB11 => (100.0, 20.0, 900.0, Car),
            TB21 => (80.0, 8.0, 1300.0, Car),
            TB22 => (80.0, 15.0, 1300.0, Car),
            TB31 => (80.0, 20.0, 1500.0, Car),
            TB32 => (110.0, 20.0, 1500.0, Car),
            TB41 => (70.0, 8.0, 10_000.0, RigidHgv),
            TB42 => (70.0, 15.0, 10_000.0, RigidHgv),
            TB51 => (70.0, 20.0, 13_000.0, Bus),
            TB61 => (80.0, 20.0, 16_000.0, RigidHgv),
            TB71 => (65.0, 20.0, 30_000.0, RigidHgv),
            TB81 => (65.0, 20.0, 38_000.0, ArticulatedHgv),
        }
    }
}

/// The groups of BS EN 1317-2:2010 Table 2. CD 377 uses the last three ("normal", "higher" and
/// "very high containment level", e.g. clauses 3.14, 3.17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContainmentGroup {
    /// "Low angle containment levels are intended to be used only for temporary safety
    /// barriers" (Table 2 NOTE 1).
    LowAngle,
    Normal,
    Higher,
    VeryHigh,
}

/// How one containment level stands against a required one. See [`ContainmentLevel::covers`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// The level is the required one, or is considered to have met it (Table 2 NOTE 2).
    Covers,
    /// The level stands further down Table 2 than the required one, but NOTE 2 excludes it:
    /// "N1 and N2 do not include T3, H-Levels do not include L-Levels and ... H1, ..., H4b do
    /// not include N2".
    HigherButNotIncluded,
    /// The level is lower than the required one (or is the H level where its L level is
    /// required).
    Below,
    /// One is an "a" and the other a "b" very high level: "no hierarchy is given between them"
    /// (Table 2 NOTE 3).
    NotRanked,
}

/// Containment level of a safety barrier or vehicle parapet: BS EN 1317-2:2010 Table 2.
/// CD 377 5.0.1 itself names N1, N2 (3.6, 3.7), H1 and H4a (Table 3.48) and H2 (7.10 NOTE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContainmentLevel {
    T1,
    T2,
    T3,
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
    /// Every level, in the order of BS EN 1317-2:2010 Table 2.
    pub const ALL: [ContainmentLevel; 15] = {
        use ContainmentLevel::*;
        [T1, T2, T3, N1, N2, H1, L1, H2, L2, H3, L3, H4a, H4b, L4a, L4b]
    };

    pub fn label(self) -> &'static str {
        use ContainmentLevel::*;
        match self {
            T1 => "T1",
            T2 => "T2",
            T3 => "T3",
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

    /// The group the level is listed under in BS EN 1317-2:2010 Table 2.
    pub fn group(self) -> ContainmentGroup {
        use ContainmentLevel::*;
        match self {
            T1 | T2 | T3 => ContainmentGroup::LowAngle,
            N1 | N2 => ContainmentGroup::Normal,
            H1 | L1 | H2 | L2 | H3 | L3 => ContainmentGroup::Higher,
            H4a | H4b | L4a | L4b => ContainmentGroup::VeryHigh,
        }
    }

    /// The acceptance tests of the level: BS EN 1317-2:2010 Table 2, "Acceptance test".
    pub fn acceptance_tests(self) -> &'static [ImpactTest] {
        use ContainmentLevel::*;
        use ImpactTest::*;
        match self {
            T1 => &[TB21],
            T2 => &[TB22],
            T3 => &[TB41, TB21],
            N1 => &[TB31],
            N2 => &[TB32, TB11],
            H1 => &[TB42, TB11],
            L1 => &[TB42, TB32, TB11],
            H2 => &[TB51, TB11],
            L2 => &[TB51, TB32, TB11],
            H3 => &[TB61, TB11],
            L3 => &[TB61, TB32, TB11],
            H4a => &[TB71, TB11],
            H4b => &[TB81, TB11],
            L4a => &[TB71, TB32, TB11],
            L4b => &[TB81, TB32, TB11],
        }
    }

    /// Row of Table 2 counted from the top, H and L of one number sharing a step and the four
    /// very high levels sharing the last.
    fn step(self) -> u8 {
        use ContainmentLevel::*;
        match self {
            T1 => 1,
            T2 => 2,
            T3 => 3,
            N1 => 4,
            N2 => 5,
            H1 | L1 => 6,
            H2 | L2 => 7,
            H3 | L3 => 8,
            H4a | H4b | L4a | L4b => 9,
        }
    }

    fn is_h(self) -> bool {
        use ContainmentLevel::*;
        matches!(self, H1 | H2 | H3 | H4a | H4b)
    }

    fn is_l(self) -> bool {
        use ContainmentLevel::*;
        matches!(self, L1 | L2 | L3 | L4a | L4b)
    }

    fn is_b(self) -> bool {
        matches!(self, ContainmentLevel::H4b | ContainmentLevel::L4b)
    }

    /// How a barrier of this level stands against a requirement for `required`, by the notes
    /// to BS EN 1317-2:2010 Table 2:
    ///
    /// * NOTE 2: "A successfully tested barrier at a given containment level should be
    ///   considered as having met the containment requirements of any lower level, except that
    ///   N1 and N2 do not include T3, H-Levels do not include L-Levels and that H1, ..., H4b do
    ///   not include N2."
    /// * NOTE 3: "The two containment levels H4a and H4b should not be regarded as equivalent
    ///   and no hierarchy is given between them. The same holds for ... L4a and L4b."
    /// * NOTE 4: the L classes are the corresponding H classes with test TB 32 added, so `Ln`
    ///   covers `Hn` and `Hn` is below `Ln`.
    ///
    /// "Lower" is read as higher up Table 2. An "a" level against a "b" level is `NotRanked`,
    /// whichever letters they carry, except that an H level never includes an L level.
    pub fn covers(self, required: ContainmentLevel) -> Coverage {
        use ContainmentLevel::*;
        if self == required {
            return Coverage::Covers;
        }
        if self.step() < required.step() {
            return Coverage::Below;
        }
        if self.is_h() && required.is_l() {
            return if self.step() == required.step() { Coverage::Below } else { Coverage::HigherButNotIncluded };
        }
        if self.step() == required.step() {
            // Only the very high step holds more than an H and its L.
            return if self.is_b() != required.is_b() { Coverage::NotRanked } else { Coverage::Covers };
        }
        if (matches!(self, N1 | N2) && required == T3) || (self.is_h() && required == N2) {
            return Coverage::HigherButNotIncluded;
        }
        Coverage::Covers
    }
}

impl std::fmt::Display for ContainmentLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Class of normalised working width level, W1 to W8: BS EN 1317-2:2010 Table 4.
/// CD 377 requires it as the "normalised working width class" (3.13 to 3.18).
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

    /// The level of the class: the normalised working width is not more than this, in metres.
    /// BS EN 1317-2:2010 Table 4: W1 ≤ 0,6; W2 ≤ 0,8; W3 ≤ 1,0; W4 ≤ 1,3; W5 ≤ 1,7; W6 ≤ 2,1;
    /// W7 ≤ 2,5; W8 ≤ 3,5. (NOTE 1: "In specific cases, a class of working width level less
    /// than W1 may be specified"; such a value is not a class here.)
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

/// Class of normalised vehicle intrusion level, VI1 to VI9: BS EN 1317-2:2010 Table 5.
/// CD 377 requires it for higher and very high containment barriers (3.13, 3.17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VehicleIntrusionClass {
    VI1,
    VI2,
    VI3,
    VI4,
    VI5,
    VI6,
    VI7,
    VI8,
    VI9,
}

impl VehicleIntrusionClass {
    pub const ALL: [VehicleIntrusionClass; 9] = {
        use VehicleIntrusionClass::*;
        [VI1, VI2, VI3, VI4, VI5, VI6, VI7, VI8, VI9]
    };

    pub fn label(self) -> &'static str {
        use VehicleIntrusionClass::*;
        match self {
            VI1 => "VI1",
            VI2 => "VI2",
            VI3 => "VI3",
            VI4 => "VI4",
            VI5 => "VI5",
            VI6 => "VI6",
            VI7 => "VI7",
            VI8 => "VI8",
            VI9 => "VI9",
        }
    }

    /// The level of the class: the normalised vehicle intrusion is not more than this, in
    /// metres. BS EN 1317-2:2010 Table 5: VI1 ≤ 0,6; VI2 ≤ 0,8; VI3 ≤ 1,0; VI4 ≤ 1,3;
    /// VI5 ≤ 1,7; VI6 ≤ 2,1; VI7 ≤ 2,5; VI8 ≤ 3,5. `None` for VI9, which is "> 3,5" and so has
    /// no upper bound.
    pub fn level_m(self) -> Option<f64> {
        use VehicleIntrusionClass::*;
        match self {
            VI1 => Some(0.6),
            VI2 => Some(0.8),
            VI3 => Some(1.0),
            VI4 => Some(1.3),
            VI5 => Some(1.7),
            VI6 => Some(2.1),
            VI7 => Some(2.5),
            VI8 => Some(3.5),
            VI9 => None,
        }
    }
}

impl std::fmt::Display for VehicleIntrusionClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Theoretical head impact velocity limit common to the three impact severity levels, km/h:
/// BS EN 1317-2:2010 Table 3, "THIV ≤ 33 km/h".
pub const THIV_LIMIT_KMH: f64 = 33.0;

/// Impact severity level: BS EN 1317-2:2010 Table 3 (CD 377 clauses 3.12, 6.14, 7.11, 7.12).
/// "Impact severity level A affords a greater level of safety for the occupant of an errant car
/// than level B, and level B greater than level C" (BS EN 1317-2:2010, 3.3).
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

    /// The acceleration severity index the level allows: BS EN 1317-2:2010 Table 3, A: ASI ≤
    /// 1,0; B: ASI ≤ 1,4; C: ASI ≤ 1,9. Informative here: no check in this crate uses it, since
    /// ASI is a test result of the product.
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

/// Performance class of a terminal.
///
/// * P1 to P4: DD ENV 1317-4:2002 Table 1. Clause 5.1: "A successfully tested installation at
///   a given performance class, shall normally be considered as having met conditions of lower
///   classes", the classes being "classified according to an increasing containment capacity".
/// * T80/3, T110, T110/1: named by CD 377 5.0.1 clauses 6.3 to 6.5 "as defined by
///   PD CEN/TS 1317-7", which is not held here. They are names only.
/// * Any other class is carried as text and cannot be checked.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TerminalClass {
    P1,
    P2,
    P3,
    P4,
    #[serde(rename = "T80/3")]
    T80_3,
    T110,
    #[serde(rename = "T110/1")]
    T110_1,
    /// A class neither document above names, as written on the product declaration.
    Other(String),
}

impl TerminalClass {
    pub fn label(&self) -> String {
        match self {
            TerminalClass::P1 => "P1".into(),
            TerminalClass::P2 => "P2".into(),
            TerminalClass::P3 => "P3".into(),
            TerminalClass::P4 => "P4".into(),
            TerminalClass::T80_3 => "T80/3".into(),
            TerminalClass::T110 => "T110".into(),
            TerminalClass::T110_1 => "T110/1".into(),
            TerminalClass::Other(s) => s.trim().to_string(),
        }
    }

    /// 1 to 4 for P1 to P4 (DD ENV 1317-4:2002 Table 1, increasing); `None` for the others.
    pub fn p_number(&self) -> Option<u8> {
        match self {
            TerminalClass::P1 => Some(1),
            TerminalClass::P2 => Some(2),
            TerminalClass::P3 => Some(3),
            TerminalClass::P4 => Some(4),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ContainmentLevel::*;

    #[test]
    fn impact_tests_are_bs_en_1317_2_2010_table_1() {
        use ImpactTest::*;
        use TestVehicle::*;
        // Test, impact speed km/h, impact angle °, total mass kg, type of vehicle.
        let table = [
            (TB11, 100.0, 20.0, 900.0, Car),
            (TB21, 80.0, 8.0, 1300.0, Car),
            (TB22, 80.0, 15.0, 1300.0, Car),
            (TB31, 80.0, 20.0, 1500.0, Car),
            (TB32, 110.0, 20.0, 1500.0, Car),
            (TB41, 70.0, 8.0, 10_000.0, RigidHgv),
            (TB42, 70.0, 15.0, 10_000.0, RigidHgv),
            (TB51, 70.0, 20.0, 13_000.0, Bus),
            (TB61, 80.0, 20.0, 16_000.0, RigidHgv),
            (TB71, 65.0, 20.0, 30_000.0, RigidHgv),
            (TB81, 65.0, 20.0, 38_000.0, ArticulatedHgv),
        ];
        assert_eq!(table.len(), ImpactTest::ALL.len());
        for (test, speed, angle, mass, vehicle) in table {
            assert_eq!(test.description(), (speed, angle, mass, vehicle), "{}", test.label());
        }
        assert_eq!(TB11.label(), "TB 11");
    }

    #[test]
    fn containment_levels_are_bs_en_1317_2_2010_table_2() {
        use ImpactTest::*;
        let labels: Vec<&str> = ContainmentLevel::ALL.iter().map(|l| l.label()).collect();
        assert_eq!(labels, ["T1", "T2", "T3", "N1", "N2", "H1", "L1", "H2", "L2", "H3", "L3", "H4a", "H4b", "L4a", "L4b"]);
        assert!([T1, T2, T3].iter().all(|l| l.group() == ContainmentGroup::LowAngle));
        assert!([N1, N2].iter().all(|l| l.group() == ContainmentGroup::Normal));
        assert!([H1, L1, H2, L2, H3, L3].iter().all(|l| l.group() == ContainmentGroup::Higher));
        assert!([H4a, H4b, L4a, L4b].iter().all(|l| l.group() == ContainmentGroup::VeryHigh));
        // Acceptance tests, row by row.
        let table: [(ContainmentLevel, &[ImpactTest]); 15] = [
            (T1, &[TB21]),
            (T2, &[TB22]),
            (T3, &[TB41, TB21]),
            (N1, &[TB31]),
            (N2, &[TB32, TB11]),
            (H1, &[TB42, TB11]),
            (L1, &[TB42, TB32, TB11]),
            (H2, &[TB51, TB11]),
            (L2, &[TB51, TB32, TB11]),
            (H3, &[TB61, TB11]),
            (L3, &[TB61, TB32, TB11]),
            (H4a, &[TB71, TB11]),
            (H4b, &[TB81, TB11]),
            (L4a, &[TB71, TB32, TB11]),
            (L4b, &[TB81, TB32, TB11]),
        ];
        for (level, tests) in table {
            assert_eq!(level.acceptance_tests(), tests, "{level}");
        }
    }

    #[test]
    fn coverage_follows_the_notes_to_table_2() {
        use Coverage::*;
        assert_eq!(N2.covers(N2), Covers);
        assert_eq!(N2.covers(N1), Covers);
        assert_eq!(N1.covers(N2), Below);
        assert_eq!(N1.covers(T2), Covers);
        assert_eq!(T3.covers(T1), Covers);
        // NOTE 2: N1 and N2 do not include T3.
        assert_eq!(N1.covers(T3), HigherButNotIncluded);
        assert_eq!(N2.covers(T3), HigherButNotIncluded);
        assert_eq!(H1.covers(T3), Covers);
        // NOTE 2: H1, ..., H4b do not include N2 (they do include N1).
        for h in [H1, H2, H3, H4a, H4b] {
            assert_eq!(h.covers(N2), HigherButNotIncluded, "{h}");
            assert_eq!(h.covers(N1), Covers, "{h}");
        }
        // NOTE 4: L levels add TB 32, so they include N2 and their H level.
        for l in [L1, L2, L3, L4a, L4b] {
            assert_eq!(l.covers(N2), Covers, "{l}");
        }
        assert_eq!(L2.covers(H2), Covers);
        assert_eq!(L3.covers(H1), Covers);
        assert_eq!(L3.covers(L1), Covers);
        // NOTE 2: H levels do not include L levels.
        assert_eq!(H2.covers(L2), Below);
        assert_eq!(H3.covers(L1), HigherButNotIncluded);
        assert_eq!(H4a.covers(L4b), Below);
        // Plain hierarchy among H levels.
        assert_eq!(H4a.covers(H1), Covers);
        assert_eq!(H4b.covers(H3), Covers);
        assert_eq!(H1.covers(H4a), Below);
        // NOTE 3: no hierarchy between a and b.
        assert_eq!(H4b.covers(H4a), NotRanked);
        assert_eq!(H4a.covers(H4b), NotRanked);
        assert_eq!(L4a.covers(L4b), NotRanked);
        assert_eq!(L4b.covers(H4a), NotRanked);
        assert_eq!(L4a.covers(H4a), Covers);
    }

    #[test]
    fn impact_severity_levels_are_bs_en_1317_2_2010_table_3() {
        assert_eq!(ImpactSeverityLevel::A.asi_limit(), 1.0);
        assert_eq!(ImpactSeverityLevel::B.asi_limit(), 1.4);
        assert_eq!(ImpactSeverityLevel::C.asi_limit(), 1.9);
        assert_eq!(THIV_LIMIT_KMH, 33.0);
    }

    #[test]
    fn working_width_levels_are_bs_en_1317_2_2010_table_4() {
        let table = [0.6, 0.8, 1.0, 1.3, 1.7, 2.1, 2.5, 3.5];
        for (class, level) in WorkingWidthClass::ALL.iter().zip(table) {
            assert_eq!(class.level_m(), level, "{class}");
        }
        assert_eq!(WorkingWidthClass::ALL.map(|c| c.label()), ["W1", "W2", "W3", "W4", "W5", "W6", "W7", "W8"]);
    }

    #[test]
    fn vehicle_intrusion_levels_are_bs_en_1317_2_2010_table_5() {
        let table = [Some(0.6), Some(0.8), Some(1.0), Some(1.3), Some(1.7), Some(2.1), Some(2.5), Some(3.5), None];
        for (class, level) in VehicleIntrusionClass::ALL.iter().zip(table) {
            assert_eq!(class.level_m(), level, "{class}");
        }
        assert_eq!(VehicleIntrusionClass::VI9.label(), "VI9");
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
    fn terminal_p_classes_are_dd_env_1317_4_2002_table_1() {
        use TerminalClass::*;
        assert_eq!([P1, P2, P3, P4].map(|c| c.p_number()), [Some(1), Some(2), Some(3), Some(4)]);
        assert_eq!([T80_3, T110, T110_1, Other("x".into())].map(|c| c.p_number()), [None; 4]);
    }

    #[test]
    fn classes_serialise_as_their_printed_names() {
        assert_eq!(serde_json::to_string(&H4a).unwrap(), "\"H4a\"");
        assert_eq!(serde_json::to_string(&VehicleIntrusionClass::VI3).unwrap(), "\"VI3\"");
        assert_eq!(serde_json::to_string(&TerminalClass::T80_3).unwrap(), "\"T80/3\"");
        assert_eq!(serde_json::to_string(&TerminalClass::T110_1).unwrap(), "\"T110/1\"");
        let other: TerminalClass = serde_json::from_str("{\"Other\":\"X9\"}").unwrap();
        assert_eq!(other.label(), "X9");
    }
}
