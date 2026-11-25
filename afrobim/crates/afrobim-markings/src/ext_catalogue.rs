// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The second set of markings (UK TSM Chapter 5 2018, R2) and the Uganda manual's markings (R3),
//! as data.
//!
//! Every number carries the document, edition, table or figure and page it was read from. A
//! dimension that could not be read with confidence is not in a table: it is an input of the
//! element (for example the width of a hatched area) and the entry says so in `notes`.
//!
//! Sources, and the page numbers used in citations (printed page, as the page itself shows it):
//!
//! * **UK TSM Ch.5 2018**: Traffic Signs Manual Chapter 5 Road Markings, 2018 (TSO, ISBN 978 0 11
//!   553208 5). Printed pages. Tables 2-1 (p.12), 2-2 (p.15), 2-3 (p.17), 2-5 (p.20), 2-6 (p.24);
//!   Figures 2-8 (p.24), 2-16 (p.30), 2-17 (p.31), 9-1 (p.97), 9-5 (p.100); 11.2.1 (p.106),
//!   Table 11-2 (p.112).
//! * **TSRGD 2016**: The Traffic Signs Regulations and General Directions 2016, SI 2016/362,
//!   printed page numbers of the PDF: Schedule 7 Part 4 (p.116), Schedule 9 Part 6 (pp.147, 148),
//!   Schedule 11 Part 4 (p.223), Schedule 14 Part 2 (p.468). Used only where Chapter 5 refers to a
//!   diagram and does not dimension it.
//! * **Uganda TSM 2004**: Traffic Signs Manual Volume 1 (section E, printed pages 34 to 40) and
//!   Volume 2 Appendix 3 section 7 sign pages (the sign pages carry no page number; they are
//!   cited by sign number with the page of the PDF `Traffic Signs Manual Vol. 2 Complete.pdf`).

use serde::Serialize;

use crate::basis::Basis;
use crate::catalogue::{MarkingColour, Pattern};

/// Document labels used in citations.
pub const UK: &str = "UK TSM Chapter 5 (2018)";
pub const TSRGD: &str = "TSRGD 2016 (SI 2016/362)";
pub const UG1: &str = "Uganda TSM Vol.1 (2004)";
pub const UG2: &str = "Uganda TSM Vol.2 (2004) App.3 sign page";

/// What kind of marking a family is, for asking a basis whether it defines the family at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Family {
    DividingLine,
    LaneLine,
    WarningLine,
    EdgeLine,
    DoubleWhiteLine,
    WaitingRestriction,
    ParkingBay,
    StopLine,
    GiveWayLine,
    GiveWayTriangle,
    PedestrianCrossing,
    LaneArrow,
    DeflectionArrow,
    HatchedMarking,
    ChevronMarking,
    IslandMarking,
    ZigZagLine,
    WordedMarking,
    RoadStuds,
}

/// A sign of speed: which speeds a size or module applies to, in the basis's own unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Sel {
    Any,
    AtMost(u32),
    Over(u32),
    /// From the first value to the second, both included.
    Between(u32, u32),
    /// Exactly these values (a table column that names a value, such as "50 or 60").
    OneOf(&'static [u32]),
}

impl Sel {
    pub fn contains(self, v: u32) -> bool {
        match self {
            Sel::Any => true,
            Sel::AtMost(n) => v <= n,
            Sel::Over(n) => v > n,
            Sel::Between(lo, hi) => v >= lo && v <= hi,
            Sel::OneOf(list) => list.contains(&v),
        }
    }
}

/// How far the geometry of an entry is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GeometryStatus {
    /// Drawn from dimensions read from the source.
    Full,
    /// Drawn as a bounding block only, because the source does not dimension the outline. The
    /// string says why. Painted area is not reported for such an item, only the block's area.
    Placeholder(&'static str),
    /// Dimensions are catalogued; nothing is drawn. The string says why.
    CatalogueOnly(&'static str),
}

/// Where lines of a `Lines` entry are measured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Reference {
    /// The element's offset is the centre of the line (a double line: the middle of the pair).
    Centre,
    /// The element's offset is the edge of carriageway or kerb. The line lies inward of it (toward
    /// the alignment), with `clear_mm` between the edge and the near edge of the nearest line.
    Edge { clear_mm: u32 },
}

/// One line of a `Lines` entry: its pattern in an urban and in a rural setting (equal where the
/// manual does not distinguish).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StrandDef {
    pub urban: Pattern,
    pub rural: Pattern,
}

const fn same(p: Pattern) -> StrandDef {
    StrandDef { urban: p, rural: p }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct LinesDef {
    pub reference: Reference,
    /// One strand: a single line. Two: a double line, the first on the left looking up-station
    /// for `Reference::Centre`, the first nearest the edge for `Reference::Edge`.
    pub strands: &'static [StrandDef],
    pub widths_mm: &'static [u32],
    /// Allowed gaps between the two lines of a double line, mm; empty for a single line.
    pub separations_mm: &'static [u32],
    /// True where the manual distinguishes urban and rural marks.
    pub by_setting: bool,
}

/// A line laid across the road (stop and give way lines).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct TransverseDef {
    /// Pattern along the line, across the road.
    pub pattern: Pattern,
    /// Rows of the pattern: 2 for diagram 1003A.
    pub rows: u32,
    /// Gap between rows, mm.
    pub row_gap_mm: u32,
    /// Width of each row (along the direction of travel), mm: the alternatives.
    pub widths_mm: &'static [u32],
}

/// A hollow triangle (give way triangle).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct TriangleDef {
    pub base_mm: u32,
    pub height_mm: u32,
    /// Width of the border along each slanting side, mm.
    pub side_border_mm: u32,
    /// Depth of the border along the base, mm (the "600" of the figure; see notes).
    pub base_border_mm: u32,
}

/// One size of an arrow.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ArrowSize {
    pub sel: Sel,
    pub length_mm: u32,
    /// Overall width across the road, mm, where the source gives it.
    pub width_mm: Option<u32>,
    /// Further dimensions of the size, shown in the derived values (name, mm).
    pub aux: &'static [(&'static str, u32)],
}

/// The tapered straight arrow of the Uganda manual (M41): a tapered shaft and a triangular head.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct TaperedArrow {
    /// Head length at the base size, mm; scaled with the length for the larger size.
    pub head_length_mm: u32,
    pub head_width_mm: u32,
    pub tail_width_mm: u32,
    /// Shaft width where it meets the head.
    pub neck_width_mm: u32,
    /// The length the dimensions above are given for, mm.
    pub base_length_mm: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ArrowDef {
    pub sizes: &'static [ArrowSize],
    pub tapered: Option<TaperedArrow>,
}

/// A worded marking drawn as a bounding block.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct WordDef {
    pub text: &'static str,
    /// Length along the road (the character height, elongated) by speed.
    pub heights: &'static [(Sel, u32)],
    /// Width across the road, mm.
    pub width_mm: u32,
}

/// Hatched, chevron and island markings between boundary lines.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct HatchModule {
    pub boundary_mark_mm: u32,
    pub boundary_gap_mm: u32,
    /// Spacing of the diagonal marks or chevrons along the road, mm.
    pub spacing_mm: u32,
    /// Width of each diagonal mark or chevron arm, perpendicular to its length, mm.
    pub mark_width_mm: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct HatchDef {
    /// The module when the speed limit is at most `split` (UK 1040, 1041: 40 mph).
    pub short: Option<HatchModule>,
    pub long: HatchModule,
    pub split: Option<u32>,
    pub boundary_widths_mm: &'static [u32],
    /// Boundary lines continuous (1042, M53) rather than broken.
    pub boundary_continuous: bool,
    pub chevrons: bool,
    /// Smallest and greatest width of the area between the boundary lines, mm.
    pub strip_min_mm: Option<u32>,
    pub strip_max_mm: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ZigZagDef {
    pub width_mm: u32,
    pub mark_mm: u32,
    pub gap_mm: u32,
    pub amplitudes_mm: &'static [u32],
}

/// Pedestrian crossing markings that run in rows across the carriageway.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct CrossingDef {
    /// True: white stripes (M33); false: two broken side lines (M34).
    pub stripes: bool,
    pub min_width_mm: u32,
    pub max_width_mm: u32,
    /// Allowed stripe depth (M33) or fixed mark length (M34), mm.
    pub mark_min_mm: u32,
    pub mark_max_mm: u32,
    pub gap_min_mm: u32,
    pub gap_max_mm: u32,
    /// Width of each side line, mm (M34).
    pub line_width_mm: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum Kind {
    Lines(LinesDef),
    Transverse(TransverseDef),
    Triangle(TriangleDef),
    Arrow(ArrowDef),
    Word(WordDef),
    Hatch(HatchDef),
    ZigZag(ZigZagDef),
    Crossing(CrossingDef),
    /// Catalogued only (see [`GeometryStatus::CatalogueOnly`]).
    None,
}

/// One marking type of the second set or of the Uganda manual.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ExtSpec {
    /// Identifier stored in elements: the UK diagram number, or the Uganda sign number.
    pub id: &'static str,
    pub basis: Basis,
    pub family: Family,
    pub name: &'static str,
    /// Diagram number (UK) or sign number (Uganda).
    pub diagram: &'static str,
    /// TSRGD 2016 Schedule-Part-Item reference as Chapter 5 or TSRGD prints it; empty for Uganda.
    pub tsrgd: &'static str,
    pub colour: MarkingColour,
    pub kind: Kind,
    pub geometry: GeometryStatus,
    /// Where the manual applies it.
    pub applies: &'static str,
    /// Where the dimensions were read.
    pub citation: &'static str,
    /// Interpretations made reading the figure, and what is an input of the element.
    pub notes: &'static str,
}

const fn pat(mark_mm: u32, gap_mm: u32) -> Pattern {
    Pattern::Broken { mark_mm, gap_mm }
}

const CONT: StrandDef = same(Pattern::Continuous);

macro_rules! lines {
    ($reference:expr, [$($s:expr),+], $widths:expr, $seps:expr, $by_setting:expr) => {
        Kind::Lines(LinesDef { reference: $reference, strands: &[$($s),+], widths_mm: $widths, separations_mm: $seps, by_setting: $by_setting })
    };
}

const URBAN_RURAL_BROKEN: StrandDef = StrandDef { urban: pat(3000, 6000), rural: pat(4000, 8000) };

/// Figure 2-8 / 2-16: the two modules by speed limit (40 mph or less / over 40 mph).
const HATCH_SHORT: HatchModule = HatchModule { boundary_mark_mm: 4000, boundary_gap_mm: 2000, spacing_mm: 3000, mark_width_mm: 150 };
const HATCH_LONG: HatchModule = HatchModule { boundary_mark_mm: 6000, boundary_gap_mm: 3000, spacing_mm: 2000, mark_width_mm: 200 };

static CATALOGUE: &[ExtSpec] = &[
    // ---------------------------------------------------------------- UK: yellow lines
    ExtSpec {
        id: "1017",
        basis: Basis::Uk,
        family: Family::WaitingRestriction,
        name: "Waiting restriction, single yellow line",
        diagram: "1017",
        tsrgd: "S7-4-2",
        colour: MarkingColour::Yellow,
        kind: lines!(Reference::Edge { clear_mm: 150 }, [CONT], &[50, 75, 100], &[], false),
        geometry: GeometryStatus::Full,
        applies: "Waiting of vehicles prohibited for a time that is not continuous throughout the year (TSRGD Sch.7 Pt.4 item 2). Chapter 5 10.2.4 and 10.6.3 refer to it; its dimensions are in TSRGD.",
        citation: "TSRGD 2016 Schedule 7 Part 4 item 2, diagram 1017, p.116",
        notes: "Width 50, 75 or 100 mm as the figure lists. 150 mm is the dimension from the edge of the carriageway to the line, read as the clear distance from the edge to the near edge of the line (the figure is small). The second 150 at the foot of the figure (the end mark) is not modelled.",
    },
    ExtSpec {
        id: "1018.1",
        basis: Basis::Uk,
        family: Family::WaitingRestriction,
        name: "Waiting restriction, double yellow lines",
        diagram: "1018.1",
        tsrgd: "S7-4-1",
        colour: MarkingColour::Yellow,
        kind: lines!(Reference::Edge { clear_mm: 150 }, [CONT, CONT], &[50, 75, 100], &[50, 75, 100], false),
        geometry: GeometryStatus::Full,
        applies: "Waiting of vehicles prohibited at all times, or stopping in a lay-by prohibited except in emergency (TSRGD Sch.7 Pt.4 item 1).",
        citation: "TSRGD 2016 Schedule 7 Part 4 item 1, diagram 1018.1, p.116",
        notes: "Each line 50, 75 or 100 mm; the gap between the lines 50, 75 or 100 mm (the figure lists three alternatives for each and does not say whether they must match, so the gap is an input). 150 mm from the edge as for 1017. The transverse end mark is not modelled.",
    },
    // ---------------------------------------------------------------- UK: transverse lines
    ExtSpec {
        id: "1001",
        basis: Basis::Uk,
        family: Family::StopLine,
        name: "Stop line at light signals",
        diagram: "1001",
        tsrgd: "S14-2-46",
        colour: MarkingColour::White,
        kind: Kind::Transverse(TransverseDef { pattern: Pattern::Continuous, rows: 1, row_gap_mm: 0, widths_mm: &[200, 300] }),
        geometry: GeometryStatus::Full,
        applies: "Vehicular traffic not to proceed beyond the line when required to stop by light signals, a constable or a traffic warden (TSRGD Sch.14 Pt.2 item 46); Chapter 5 6.5 (signalised roundabouts) refers to it.",
        citation: "TSRGD 2016 Schedule 14 Part 2 item 46, diagram 1001, p.468",
        notes: "Continuous line 200 or 300 mm wide.",
    },
    ExtSpec {
        id: "1002.1",
        basis: Basis::Uk,
        family: Family::StopLine,
        name: "Stop line at a stop sign",
        diagram: "1002.1",
        tsrgd: "S9-6-1",
        colour: MarkingColour::White,
        kind: Kind::Transverse(TransverseDef { pattern: Pattern::Continuous, rows: 1, row_gap_mm: 0, widths_mm: &[400] }),
        geometry: GeometryStatus::Full,
        applies: "Vehicular traffic must not proceed beyond the line when required to stop by the stop sign (TSRGD Sch.9 Pt.6 item 1).",
        citation: "TSRGD 2016 Schedule 9 Part 6 item 1, diagram 1002.1, p.147",
        notes: "Continuous line 400 mm wide.",
    },
    ExtSpec {
        id: "1003.1",
        basis: Basis::Uk,
        family: Family::GiveWayLine,
        name: "Give way line at a roundabout (advisory)",
        diagram: "1003.1",
        tsrgd: "S11-4-1",
        colour: MarkingColour::White,
        kind: Kind::Transverse(TransverseDef { pattern: pat(1000, 1000), rows: 1, row_gap_mm: 0, widths_mm: &[200, 300] }),
        geometry: GeometryStatus::Full,
        applies: "Vehicles approaching a roundabout give way at or beyond the line (TSRGD Sch.11 Pt.4 item 1). Width 300 where the warning line is 1004.1 or a hatched boundary is 6 m mark and 3 m gap, otherwise normally 200 (Chapter 5 6.2.2).",
        citation: "TSRGD 2016 Schedule 11 Part 4 item 1, diagram 1003.1, p.223; UK TSM Chapter 5 (2018) 6.2.2, p.62",
        notes: "Marks 1000 mm, gaps 1000 mm (the two 1000 dimensions of the figure), width 200 or 300 mm.",
    },
    ExtSpec {
        id: "1003A",
        basis: Basis::Uk,
        family: Family::GiveWayLine,
        name: "Give way line, double broken (mandatory)",
        diagram: "1003A",
        tsrgd: "S9-6-3",
        colour: MarkingColour::White,
        kind: Kind::Transverse(TransverseDef { pattern: pat(600, 300), rows: 2, row_gap_mm: 300, widths_mm: &[200] }),
        geometry: GeometryStatus::Full,
        applies: "Vehicular traffic must give way (TSRGD Sch.9 Pt.6 item 3); used with the triangle 1023A (Chapter 5 6.3.6, 11.2.2).",
        citation: "TSRGD 2016 Schedule 9 Part 6 item 3, diagram 1003A, p.148",
        notes: "Two rows of broken line, each 200 mm wide, 300 mm apart; marks 600 mm and gaps 300 mm, read from the figure's 600 and 300 dimensions and its proportions. Diagram 1003.3 (mini-roundabouts) is not read.",
    },
    ExtSpec {
        id: "1023A",
        basis: Basis::Uk,
        family: Family::GiveWayTriangle,
        name: "Give way triangle",
        diagram: "1023A",
        tsrgd: "S9-6-4",
        colour: MarkingColour::White,
        kind: Kind::Triangle(TriangleDef { base_mm: 1250, height_mm: 3750, side_border_mm: 150, base_border_mm: 600 }),
        geometry: GeometryStatus::Full,
        applies: "Approach to a road junction, level crossing or road narrowing at which the marking at item 3 or 6 is placed (TSRGD Sch.9 Pt.6 item 4).",
        citation: "TSRGD 2016 Schedule 9 Part 6 item 4, diagram 1023A, p.148",
        notes: "Base 1250, height 3750, border 150 along the sides. The figure's 600 is read as the depth of the border along the base (the hollow starts 600 below the base in the figure); this reading is not stated in words. Drawn as in the figure, base toward +y; use rotation 180 to turn it.",
    },
    ExtSpec {
        id: "1022",
        basis: Basis::Uk,
        family: Family::WordedMarking,
        name: "STOP (worded marking)",
        diagram: "1022",
        tsrgd: "S9-6-2",
        colour: MarkingColour::White,
        kind: Kind::Word(WordDef { text: "STOP", heights: &[(Sel::Any, 1600), (Sel::Any, 2800)], width_mm: 548 + 436 + 624 + 520 }),
        geometry: GeometryStatus::Placeholder("letter shapes are not drawn: the chapter gives tile widths and the two character heights only"),
        applies: "Approach to a junction at which a stop sign is placed (TSRGD Sch.9 Pt.6 item 2).",
        citation: "UK TSM Chapter 5 (2018) 11.7.1 to 11.7.3, Table 11-2 (S 548, T 436, O 624, P 520 mm), Table 11-3, p.112; TSRGD 2016 Schedule 9 Part 6 item 2, p.147 (heights 1600 and 2800)",
        notes: "Placeholder block: width = sum of the elongated tile widths (2128 mm), length along the road 1600 or 2800 mm (an input `size_mm`). Painted area is not reported.",
    },
    ExtSpec {
        id: "1024",
        basis: Basis::Uk,
        family: Family::WordedMarking,
        name: "SLOW (worded marking)",
        diagram: "1024",
        tsrgd: "S11-4-15",
        colour: MarkingColour::White,
        kind: Kind::Word(WordDef { text: "SLOW", heights: &[(Sel::AtMost(40), 1600), (Sel::Over(40), 2800)], width_mm: 548 + 428 + 624 + 732 }),
        geometry: GeometryStatus::Placeholder("letter shapes are not drawn: the chapter gives tile widths and the two character heights only"),
        applies: "1600 mm where the speed limit is 40 mph or less, 2800 mm where it is more than 40 mph (11.2.1).",
        citation: "UK TSM Chapter 5 (2018) 11.2.1 and Figure 11-1 (p.106), Table 11-2 (S 548, L 428, O 624, W 732 mm, p.112)",
        notes: "Placeholder block 2332 mm wide (sum of tile widths) by 1600 or 2800 mm long. Painted area is not reported.",
    },
    // ---------------------------------------------------------------- UK: arrows
    ExtSpec {
        id: "1038-ahead",
        basis: Basis::Uk,
        family: Family::LaneArrow,
        name: "Lane arrow, ahead",
        diagram: "1038",
        tsrgd: "S11-4-20",
        colour: MarkingColour::White,
        kind: Kind::Arrow(ArrowDef { sizes: LANE_SIZES_500, tapered: None }),
        geometry: GeometryStatus::Placeholder("Chapter 5 and TSRGD give the length and one width per alternative; the outline is on working drawings that were not available"),
        applies: "Approach to busy multi-lane junctions, two or three in sequence in each lane (9.1.1); length by speed limit and siting from Table 9-1.",
        citation: "UK TSM Chapter 5 (2018) Figure 9-1 and Table 9-1, p.97; TSRGD 2016 Schedule 11 Part 4 item 20, p.237",
        notes: "Panel of the figure with the 300 and 500 dimensions (shaft 300, head 500). Length 4000, 6000 or 9000 mm by Table 9-1.",
    },
    ExtSpec {
        id: "1038-turn",
        basis: Basis::Uk,
        family: Family::LaneArrow,
        name: "Lane arrow, turn",
        diagram: "1038",
        tsrgd: "S11-4-20",
        colour: MarkingColour::White,
        kind: Kind::Arrow(ArrowDef { sizes: LANE_SIZES_750, tapered: None }),
        geometry: GeometryStatus::Placeholder("outline not dimensioned in the sources held"),
        applies: "As 1038-ahead.",
        citation: "UK TSM Chapter 5 (2018) Figure 9-1 and Table 9-1, p.97; TSRGD 2016 Schedule 11 Part 4 item 20, p.237",
        notes: "Panel of the figure with the 750 dimension (bent arrow). Width 750 mm is the only transverse dimension read. Mirror panels (the other direction) have the same size.",
    },
    ExtSpec {
        id: "1038-ahead-and-turn",
        basis: Basis::Uk,
        family: Family::LaneArrow,
        name: "Lane arrow, ahead and turn",
        diagram: "1038",
        tsrgd: "S11-4-20",
        colour: MarkingColour::White,
        kind: Kind::Arrow(ArrowDef { sizes: LANE_SIZES_850, tapered: None }),
        geometry: GeometryStatus::Placeholder("outline not dimensioned in the sources held"),
        applies: "As 1038-ahead.",
        citation: "UK TSM Chapter 5 (2018) Figure 9-1 and Table 9-1, p.97; TSRGD 2016 Schedule 11 Part 4 item 20, p.237",
        notes: "Panel of the figure with the 850 dimension (ahead arrow with a side branch).",
    },
    ExtSpec {
        id: "1038-two-way",
        basis: Basis::Uk,
        family: Family::LaneArrow,
        name: "Lane arrow, ahead with branches both sides",
        diagram: "1038",
        tsrgd: "S11-4-20",
        colour: MarkingColour::White,
        kind: Kind::Arrow(ArrowDef { sizes: LANE_SIZES_1200, tapered: None }),
        geometry: GeometryStatus::Placeholder("outline not dimensioned in the sources held"),
        applies: "As 1038-ahead.",
        citation: "UK TSM Chapter 5 (2018) Figure 9-1 and Table 9-1, p.97; TSRGD 2016 Schedule 11 Part 4 item 20, p.237",
        notes: "Panel of the figure with the 1200 dimension.",
    },
    ExtSpec {
        id: "1014",
        basis: Basis::Uk,
        family: Family::DeflectionArrow,
        name: "Deflection arrow",
        diagram: "1014",
        tsrgd: "S11-4-14",
        colour: MarkingColour::White,
        kind: Kind::Arrow(ArrowDef { sizes: DEFLECTION_SIZES, tapered: None }),
        geometry: GeometryStatus::Placeholder("the bend of the shaft and the barbs of the head are not dimensioned beyond the lengths and widths of Figure 9-5"),
        applies: "Direction in which traffic should pass a marking to diagram 1049B, 1049A, 1013.1, 1040, 1040.3 or 1040.4, an obstruction, or a reduction in lanes (9.5.1). May be reversed (9.5.2). Siting by Table 2-6.",
        citation: "UK TSM Chapter 5 (2018) Figure 9-5, p.100; Table 2-6, p.24",
        notes: "Three sizes: length 4500, 6000, 9000; shaft width 150, 200, 300; lateral extent of the head 525, 700, 1050; head length 1500, 2000, 3000 (the figure lists them in that order). The block is length by lateral extent.",
    },
    // ---------------------------------------------------------------- UK: hatching and chevrons
    ExtSpec {
        id: "1040",
        basis: Basis::Uk,
        family: Family::HatchedMarking,
        name: "Hatched marking, centre of a two-way road (parallel part)",
        diagram: "1040",
        tsrgd: "S11-4-23",
        colour: MarkingColour::White,
        kind: Kind::Hatch(HatchDef {
            short: Some(HATCH_SHORT),
            long: HATCH_LONG,
            split: Some(40),
            boundary_widths_mm: &[100, 150],
            boundary_continuous: false,
            chevrons: false,
            strip_min_mm: Some(800),
            strip_max_mm: Some(8000),
        }),
        geometry: GeometryStatus::Full,
        applies: "Divides opposing flows on two-way roads where the warning line is insufficient (2.7.3). Tapers (Table 2-6, 2-7), the 400 mm and 100 mm minimum dimensions of the taper and the refuge approach are not modelled.",
        citation: "UK TSM Chapter 5 (2018) Figure 2-8 (p.24), 2.7.1 (p.23), Table 2-6 (p.24)",
        notes: "Boundary lines 100 or 150 mm wide, marks 4000 and gaps 2000 mm at 40 mph or less, 6000 and 3000 mm over 40 mph (2.7.1). Diagonals at 45 degrees, 150 mm wide at 3000 mm spacing with the short module, 200 mm wide at 2000 mm spacing with the long module (2.7.1: closer spacing and wider diagonal go with the longer boundary marks). The area between the boundary lines is 800 mm minimum, 8000 mm maximum; which edges that is measured between is not stated, and it is read here as the clear width between the inner edges of the boundary lines. The \"400 min\" dimension is not used. The lean of the diagonals and where the first one starts are not dimensioned: the first diagonal starts at the start station and the lean is an input.",
    },
    ExtSpec {
        id: "1041",
        basis: Basis::Uk,
        family: Family::ChevronMarking,
        name: "Chevron marking (parallel part)",
        diagram: "1041",
        tsrgd: "S11-4-27",
        colour: MarkingColour::White,
        kind: Kind::Hatch(HatchDef {
            short: Some(HATCH_SHORT),
            long: HATCH_LONG,
            split: Some(40),
            boundary_widths_mm: &[100, 150],
            boundary_continuous: false,
            chevrons: true,
            strip_min_mm: Some(800),
            strip_max_mm: Some(8000),
        }),
        geometry: GeometryStatus::Full,
        applies: "Separates streams travelling in the same direction, approach to a junction or refuge in a one-way street (2.11.1). Chevrons point toward approaching traffic (2.11.3). Studs red (4.4.1).",
        citation: "UK TSM Chapter 5 (2018) Figure 2-16 (p.30), 2.11.1 to 2.11.3 (pp.30 to 31), 2.7.1 (p.23)",
        notes: "As 1040 with chevrons of 90 degrees included angle (each arm at 45 degrees to the road) in place of diagonals; the same widths and spacings as Figure 2-16 repeats. The tapered version is not modelled.",
    },
    ExtSpec {
        id: "1042",
        basis: Basis::Uk,
        family: Family::ChevronMarking,
        name: "Chevron marking, mandatory (parallel-sided)",
        diagram: "1042",
        tsrgd: "S9-6-22",
        colour: MarkingColour::White,
        kind: Kind::Hatch(HatchDef {
            short: None,
            long: HatchModule { boundary_mark_mm: 0, boundary_gap_mm: 0, spacing_mm: 3400, mark_width_mm: 150 },
            split: None,
            boundary_widths_mm: &[150, 200],
            boundary_continuous: true,
            chevrons: true,
            strip_min_mm: Some(500),
            strip_max_mm: Some(8000),
        }),
        geometry: GeometryStatus::Full,
        applies: "Areas of carriageway drivers must not enter except in an emergency; the parallel-sided version marks a segregated left-turn lane (2.11.2, 6.8). Studs red (4.4.1).",
        citation: "UK TSM Chapter 5 (2018) Figure 2-17 (p.31), 2.11.2 and 2.11.3 (pp.30 to 31)",
        notes: "Continuous boundary lines 150 or 200 mm wide (2.11.2: generally 200, 150 at roundabouts and on dual carriageways without hard shoulders); chevrons of 90 degrees included angle, arm width 150 or 200 mm, pitch 3400 mm (the figure's 3400, read as apex to apex). Width between the lines 500 mm minimum, 8000 mm maximum. The figure's \"1000\" dimension is not used. Tapered versions are not modelled.",
    },
    // ---------------------------------------------------------------- Uganda: dividing lines
    ExtSpec {
        id: "M01",
        basis: Basis::Uganda,
        family: Family::DividingLine,
        name: "Continuous dividing line",
        diagram: "M01",
        tsrgd: "",
        colour: MarkingColour::Yellow,
        kind: lines!(Reference::Centre, [CONT], &[100], &[], false),
        geometry: GeometryStatus::Full,
        applies: "Rural roads 5.5 to 6.0 m wide where overtaking is prohibited in one or both directions; not less than 100 m long; also at least 50 m on the approach to M31 and M32 lines; in towns where essential. Sight distance below which overtaking is prohibited: 150 m at 50 km/h, 180 at 60, 240 at 80, 300 at 100 (measured from eye height 1.05 m to object height 1.3 m).",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M01 (PDF p.161); Vol.1 section E.2 (p.35)",
        notes: "100 mm yellow.",
    },
    ExtSpec {
        id: "M02",
        basis: Basis::Uganda,
        family: Family::DividingLine,
        name: "Double continuous dividing line",
        diagram: "M02",
        tsrgd: "",
        colour: MarkingColour::Yellow,
        kind: lines!(Reference::Centre, [CONT, CONT], &[100], &[130], false),
        geometry: GeometryStatus::Full,
        applies: "Single carriageway rural roads wider than 6.0 m where overtaking is prohibited in both directions; not normally in urban areas except wide multi-lane roads with no median.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M02 (PDF p.162); Vol.1 section E.2 (p.35)",
        notes: "Two 100 mm yellow lines 130 mm apart.",
    },
    ExtSpec {
        id: "M03",
        basis: Basis::Uganda,
        family: Family::DividingLine,
        name: "Broken dividing line",
        diagram: "M03",
        tsrgd: "",
        colour: MarkingColour::Yellow,
        kind: lines!(Reference::Centre, [URBAN_RURAL_BROKEN], &[100], &[], true),
        geometry: GeometryStatus::Full,
        applies: "Separates opposing flows on rural roads where overtaking is not prohibited, and in towns where necessary.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M03 (PDF p.163); Vol.1 section E.2 (p.35)",
        notes: "100 mm yellow; urban 3.0 m mark, 6.0 m gap; rural 4.0 m mark, 8.0 m gap.",
    },
    ExtSpec {
        id: "M04-L",
        basis: Basis::Uganda,
        family: Family::DividingLine,
        name: "Continuous and broken dividing line, left line broken",
        diagram: "M04",
        tsrgd: "",
        colour: MarkingColour::Yellow,
        kind: lines!(Reference::Centre, [URBAN_RURAL_BROKEN, CONT], &[100], &[130], true),
        geometry: GeometryStatus::Full,
        applies: "Rural roads wider than 6.0 m where overtaking is prohibited in one direction; do not change from M02 to M04 when the M04 length would be shorter than the sight distance standard.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M04 (PDF p.164); Vol.1 section E.2 (p.35)",
        notes: "One M01 and one M03 line 130 mm apart. The manual does not say which side the broken line is on; the two versions (-L, -R, looking up-station) are the caller's choice.",
    },
    ExtSpec {
        id: "M04-R",
        basis: Basis::Uganda,
        family: Family::DividingLine,
        name: "Continuous and broken dividing line, right line broken",
        diagram: "M04",
        tsrgd: "",
        colour: MarkingColour::Yellow,
        kind: lines!(Reference::Centre, [CONT, URBAN_RURAL_BROKEN], &[100], &[130], true),
        geometry: GeometryStatus::Full,
        applies: "As M04-L.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M04 (PDF p.164); Vol.1 section E.2 (p.35)",
        notes: "As M04-L.",
    },
    ExtSpec {
        id: "M05",
        basis: Basis::Uganda,
        family: Family::ZigZagLine,
        name: "Zig-zag dividing line",
        diagram: "M05",
        tsrgd: "",
        colour: MarkingColour::Yellow,
        kind: Kind::ZigZag(ZigZagDef { width_mm: 100, mark_mm: 2000, gap_mm: 150, amplitudes_mm: &[500, 300] }),
        geometry: GeometryStatus::Full,
        applies: "Instead of a dividing line on the approach to a pedestrian crossing, normally over 20 to 40 m; not within signalised intersections.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M05 (PDF p.165); Vol.1 section E.2 (p.35)",
        notes: "100 mm yellow; marks 2000 mm along the road, gaps 150 mm; lateral extent 500 mm (300 mm as the alternative). The figure's 500 is read as the overall lateral extent of a mark including its width, so the centre line moves 500 minus 100 mm.",
    },
    // ---------------------------------------------------------------- Uganda: lane and edge lines
    ExtSpec {
        id: "M11",
        basis: Basis::Uganda,
        family: Family::LaneLine,
        name: "Continuous lane line",
        diagram: "M11",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: lines!(Reference::Centre, [CONT], &[100], &[], false),
        geometry: GeometryStatus::Full,
        applies: "Lanes for traffic in the same direction at intersections, sharp bends and where changing lane is hazardous; minimum length 18 m at intersections, 27 m preferable at multi-lane approaches.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M11 (PDF p.166); Vol.1 section E.2 (p.35)",
        notes: "100 mm white.",
    },
    ExtSpec {
        id: "M12",
        basis: Basis::Uganda,
        family: Family::LaneLine,
        name: "Broken lane line",
        diagram: "M12",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: lines!(Reference::Centre, [URBAN_RURAL_BROKEN], &[100], &[], true),
        geometry: GeometryStatus::Full,
        applies: "Lanes for traffic in the same direction where changing lane is permissible.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M12 (PDF p.167); Vol.1 section E.2 (p.35)",
        notes: "100 mm white; urban 3.0 m mark, 6.0 m gap; rural 4.0 m mark, 8.0 m gap.",
    },
    ExtSpec {
        id: "M21",
        basis: Basis::Uganda,
        family: Family::EdgeLine,
        name: "Standard edge line",
        diagram: "M21",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: lines!(Reference::Edge { clear_mm: 100 }, [CONT], &[100], &[], false),
        geometry: GeometryStatus::Full,
        applies: "Edge of the traffic lane on rural roads; always where there are paved shoulders; not normally on roads narrower than 6 m. Marked 100 mm from the edge of the carriageway where there is no paved shoulder; at the edge of the shoulder where there is one.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M21 (PDF p.168); Vol.1 section E.2 (p.36)",
        notes: "100 mm white. The element's offset is the edge of the carriageway (or of the shoulder); the 100 mm is read as the clear distance from that edge to the near edge of the line. With a paved shoulder the manual puts the line at the edge of the shoulder: give that edge as the offset and read the 100 mm clear distance as still applying, or place the line as a Centre-referenced marking by hand.",
    },
    ExtSpec {
        id: "M22",
        basis: Basis::Uganda,
        family: Family::EdgeLine,
        name: "Continuity line",
        diagram: "M22",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: lines!(Reference::Centre, [same(pat(1000, 1000))], &[100, 200], &[], false),
        geometry: GeometryStatus::Full,
        applies: "Instead of the standard edge line on rural roads at intersections and access roads (100 mm), at lay-bys and bus stops (200 mm), between a through lane and a climbing lane (200 mm); not normally on carriageways narrower than 6 m.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M22 (PDF p.169); Vol.1 section E.2 (p.36)",
        notes: "White, 100 mm or 200 mm wide, 1.0 m marks and 1.0 m gaps urban and rural.",
    },
    ExtSpec {
        id: "M23",
        basis: Basis::Uganda,
        family: Family::WaitingRestriction,
        name: "No parking line",
        diagram: "M23",
        tsrgd: "",
        colour: MarkingColour::Yellow,
        kind: lines!(Reference::Edge { clear_mm: 250 }, [CONT], &[100], &[], false),
        geometry: GeometryStatus::Full,
        applies: "Where parking must be prohibited, normally urban areas near intersections. Recommended on the kerb face; otherwise about 250 mm out from the kerb.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M23 (PDF p.170); Vol.1 section E.2 (p.36)",
        notes: "100 mm yellow. \"About 250 mm out from the kerb\" is read as the clear distance from the kerb to the near edge of the line. The kerb-face option is not modelled.",
    },
    // ---------------------------------------------------------------- Uganda: transverse markings
    ExtSpec {
        id: "M31",
        basis: Basis::Uganda,
        family: Family::StopLine,
        name: "Stop line",
        diagram: "M31",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::Transverse(TransverseDef { pattern: Pattern::Continuous, rows: 1, row_gap_mm: 0, widths_mm: &[400] }),
        geometry: GeometryStatus::Full,
        applies: "Wherever there is an R61 Stop sign, a TS03 railway crossing signal, or signal-controlled intersection or pedestrian crossing. Normally from the left edge of the road to the centre line; across the full width if that distance is under 2.75 m. At signals normally 1 m in advance of the signal column. Never closer than 600 mm to the path of vehicles on the main road.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M31 (PDF p.171); Vol.1 section E.3 (p.37)",
        notes: "Continuous 400 mm white line at right angles to the flow.",
    },
    ExtSpec {
        id: "M32",
        basis: Basis::Uganda,
        family: Family::GiveWayLine,
        name: "Give way line",
        diagram: "M32",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::Transverse(TransverseDef { pattern: pat(600, 300), rows: 1, row_gap_mm: 0, widths_mm: &[400] }),
        geometry: GeometryStatus::Full,
        applies: "Wherever there is an R62 Give Way sign; alone at minor intersections without signs and at M33 crossings. Same extent rule as M31.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M32 (PDF p.172); Vol.1 section E.3 (p.37)",
        notes: "Broken 400 mm white line. The figure gives 300 mm and 600 mm; which is the mark is read from its proportions as mark 600 mm, gap 300 mm.",
    },
    ExtSpec {
        id: "M33",
        basis: Basis::Uganda,
        family: Family::PedestrianCrossing,
        name: "Uncontrolled pedestrian crossing (zebra)",
        diagram: "M33",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::Crossing(CrossingDef { stripes: true, min_width_mm: 3000, max_width_mm: 5000, mark_min_mm: 500, mark_max_mm: 700, gap_min_mm: 500, gap_max_mm: 700, line_width_mm: 0 }),
        geometry: GeometryStatus::Full,
        applies: "Only where there is a strong need; standard width 4 m, 3 m may do on low-speed roads. Give way line M32 where drivers must stop; M05 on the approach.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M33 (PDF p.173); Vol.1 section E.3 (p.37)",
        notes: "Stripes 500 to 700 mm deep, gaps 500 to 700 mm, crossing width 3.0 to 5.0 m. The figure also shows 500 to 1300 mm at the ends (edge to first stripe); not modelled. Number of stripes is an input.",
    },
    ExtSpec {
        id: "M34",
        basis: Basis::Uganda,
        family: Family::PedestrianCrossing,
        name: "Signal-controlled pedestrian crossing",
        diagram: "M34",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::Crossing(CrossingDef { stripes: false, min_width_mm: 2500, max_width_mm: 5000, mark_min_mm: 500, mark_max_mm: 500, gap_min_mm: 500, gap_max_mm: 500, line_width_mm: 100 }),
        geometry: GeometryStatus::Full,
        applies: "Controlled by TS02 pedestrian signals; stop line M31 where drivers must stop. Width generally 2.5 m, up to 5.0 m for a busy crossing.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M34 (PDF p.174); Vol.1 section E.3 (p.37)",
        notes: "Two 100 mm broken white lines at right angles to the traffic flow, marks 500 mm, gaps 500 mm, 2.5 to 5.0 m apart (read as between the inner edges). The end spacing of 500 to 700 mm is not modelled. Length along the road is an input.",
    },
    ExtSpec {
        id: "M35",
        basis: Basis::Uganda,
        family: Family::PedestrianCrossing,
        name: "Hump marking (chequerboard)",
        diagram: "M35",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::None,
        geometry: GeometryStatus::CatalogueOnly("the number of squares and the hump length are site inputs; the chequerboard is not laid out"),
        applies: "All humps, three-row chequerboard on both sides of the hump across its width.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M35 (PDF p.175)",
        notes: "Squares normally 500 mm on a side, at least 300 mm.",
    },
    // ---------------------------------------------------------------- Uganda: arrows
    ExtSpec {
        id: "M41",
        basis: Basis::Uganda,
        family: Family::LaneArrow,
        name: "Lane arrow, ahead only",
        diagram: "M41",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::Arrow(ArrowDef {
            sizes: &[
                ArrowSize { sel: Sel::AtMost(60), length_mm: 4000, width_mm: Some(500), aux: &[("head length", 1575), ("shaft length", 2425), ("tail width", 300), ("neck width", 120)] },
                ArrowSize { sel: Sel::Over(60), length_mm: 6000, width_mm: Some(500), aux: &[("head length", 1575), ("shaft length", 2425), ("tail width", 300), ("neck width", 120)] },
            ],
            tapered: Some(TaperedArrow { head_length_mm: 1575, head_width_mm: 500, tail_width_mm: 300, neck_width_mm: 120, base_length_mm: 4000 }),
        }),
        geometry: GeometryStatus::Full,
        applies: "Lane approaching an intersection; the 6 m arrow where traffic speed exceeds 60 km/h. Usually two per lane: the first 15 m from the stop or give way line, the second a further 30 m; distances increased by 50 per cent for 6 m arrows.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M41 (PDF p.176); Vol.1 section E.4 (p.37)",
        notes: "Outline from the figure: length 4000, head length 1575, head width 500, shaft 300 wide at the tail tapering to 120 at the head, shaft length 2425. The 6 m arrow \"is extended proportionally and has the same width dimensions\": read as head and shaft lengths scaled by 6000/4000, all widths unchanged.",
    },
    ExtSpec {
        id: "M42",
        basis: Basis::Uganda,
        family: Family::LaneArrow,
        name: "Lane arrow, left or right turn only",
        diagram: "M42",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::Arrow(ArrowDef {
            sizes: &[
                ArrowSize { sel: Sel::AtMost(60), length_mm: 4000, width_mm: Some(600), aux: &[("shaft length to bend", 2425), ("tail width", 300), ("neck width", 120)] },
                ArrowSize { sel: Sel::Over(60), length_mm: 6000, width_mm: Some(600), aux: &[("shaft length to bend", 2425), ("tail width", 300), ("neck width", 120)] },
            ],
            tapered: None,
        }),
        geometry: GeometryStatus::Placeholder("the bend of the shaft is not dimensioned (no radius); bounding block only"),
        applies: "Lane approaching an intersection in which traffic must turn; 6 m arrow above 60 km/h; siting as M41.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M42 (PDF p.177); Vol.1 section E.4 (p.37)",
        notes: "Width 600 mm is the dimension at the head of the figure. Left and right versions are mirror images.",
    },
    ExtSpec {
        id: "M43",
        basis: Basis::Uganda,
        family: Family::LaneArrow,
        name: "Lane arrow, ahead or left/right turn",
        diagram: "M43",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::Arrow(ArrowDef {
            sizes: &[
                ArrowSize { sel: Sel::AtMost(60), length_mm: 4000, width_mm: None, aux: &[("head length", 1575), ("head width", 500), ("tail width", 300), ("neck width", 120)] },
                ArrowSize { sel: Sel::Over(60), length_mm: 6000, width_mm: None, aux: &[("head length", 1575), ("head width", 500), ("tail width", 300), ("neck width", 120)] },
            ],
            tapered: None,
        }),
        geometry: GeometryStatus::Placeholder("the overall width across the side branch cannot be told from the figure's dimensions; width is an input; the branch is not drawn"),
        applies: "Lane in which traffic can go straight on or turn; 6 m arrow above 60 km/h; siting as M41.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M43 (PDF p.178); Vol.1 section E.4 (p.37)",
        notes: "Pass `width_mm` for the block width; the figure shows 850, 500, 190, 575, 425, 350 and 1850 mm without saying which is the overall width.",
    },
    ExtSpec {
        id: "M44",
        basis: Basis::Uganda,
        family: Family::LaneArrow,
        name: "Diverge arrow",
        diagram: "M44",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::Arrow(ArrowDef {
            sizes: &[
                ArrowSize { sel: Sel::AtMost(60), length_mm: 8000, width_mm: Some(2100), aux: &[("line width", 200)] },
                ArrowSize { sel: Sel::Over(60), length_mm: 16000, width_mm: Some(2100), aux: &[("line width", 200)] },
            ],
            tapered: None,
        }),
        geometry: GeometryStatus::Placeholder("outline of the splitting line and two heads is not fully dimensioned; bounding block only"),
        applies: "Start of a special lane on the approach to an intersection; on the through lane alongside the start of the special lane; 16 m version above 60 km/h.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M44 (PDF p.179); Vol.1 section E.4 (p.37)",
        notes: "Width 2100 mm is read as the overall transverse extent (the figure's 2100 at the right end); the 16 m arrow is extended proportionally with the same width dimensions.",
    },
    ExtSpec {
        id: "M45",
        basis: Basis::Uganda,
        family: Family::DeflectionArrow,
        name: "Deflecting arrow",
        diagram: "M45",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::Arrow(ArrowDef {
            sizes: &[
                ArrowSize { sel: Sel::AtMost(60), length_mm: 5000, width_mm: Some(800), aux: &[("head width", 600), ("head length", 1800), ("tail width", 300)] },
                ArrowSize { sel: Sel::Over(60), length_mm: 8000, width_mm: Some(800), aux: &[("head width", 600), ("head length", 1800), ("tail width", 300)] },
            ],
            tapered: None,
        }),
        geometry: GeometryStatus::Placeholder("the curve of the shaft is not dimensioned; bounding block only"),
        applies: "End of a lane (acceleration lane, climbing lane or ordinary lane); two arrows, the last about 50 m before the end of the lane and the other 50 to 100 m before that; 8 m arrow above 60 km/h.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M45 (PDF p.180); Vol.1 section E.4 (p.37)",
        notes: "Overall width 800 mm is the figure's 800; the 8 m arrow is extended proportionally with the same width dimensions.",
    },
    ExtSpec {
        id: "M51",
        basis: Basis::Uganda,
        family: Family::ChevronMarking,
        name: "Chevron marking, merging traffic",
        diagram: "M51",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::None,
        geometry: GeometryStatus::CatalogueOnly("tapered nose geometry (600 mm tip widening to 8000 mm maximum) is not parametric enough to lay out from the figure"),
        applies: "Nose of a channelising island where two traffic streams merge.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M51 (PDF p.181); Vol.1 section E.4 (p.38)",
        notes: "Chevrons bordered by a 150 mm line; figure dimensions 150, 600, 450 min, 2000, 400, 8000 max, 45 degrees.",
    },
    ExtSpec {
        id: "M52",
        basis: Basis::Uganda,
        family: Family::ChevronMarking,
        name: "Chevron marking, diverging traffic",
        diagram: "M52",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::None,
        geometry: GeometryStatus::CatalogueOnly("as M51"),
        applies: "Nose of a channelising island where a stream divides; also alone to separate lanes.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M52 (PDF p.182); Vol.1 section E.4 (p.38)",
        notes: "Same dimensions as M51.",
    },
    ExtSpec {
        id: "M53",
        basis: Basis::Uganda,
        family: Family::IslandMarking,
        name: "Traffic island marking, between opposing directions (parallel part)",
        diagram: "M53",
        tsrgd: "",
        colour: MarkingColour::Yellow,
        kind: Kind::Hatch(HatchDef {
            short: None,
            long: HatchModule { boundary_mark_mm: 0, boundary_gap_mm: 0, spacing_mm: 2000, mark_width_mm: 200 },
            split: None,
            boundary_widths_mm: &[150],
            boundary_continuous: true,
            chevrons: false,
            strip_min_mm: None,
            strip_max_mm: None,
        }),
        geometry: GeometryStatus::Full,
        applies: "Guides vehicles past a central island, or forms a ghost island; at bends to discourage encroachment. Starts from a dividing line and develops by a taper (1:20 at up to 80 km/h, 1:30 above 80 km/h, 1:40 or 1:50 when the island is 1.5 m wide or less); the taper is not modelled.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M53 (PDF p.183); Vol.1 section E.4 (p.38)",
        notes: "Yellow diagonal bars 200 mm wide at 45 degrees, 2000 mm apart, bordered by 150 mm continuous lines. The width between the borders is not given by the manual: an input. 2000 mm is read as the spacing of the bars along the road.",
    },
    ExtSpec {
        id: "M61",
        basis: Basis::Uganda,
        family: Family::GiveWayTriangle,
        name: "Give way marking (hollow triangle)",
        diagram: "M61",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::Triangle(TriangleDef { base_mm: 1250, height_mm: 3750, side_border_mm: 150, base_border_mm: 600 }),
        geometry: GeometryStatus::Full,
        applies: "Extra emphasis for the M32 give way line, never alone; base about 3 m (up to 15 m) in advance of the line; one in the centre of each lane.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M61 (PDF p.184); Vol.1 section E.4 (p.38)",
        notes: "Dimensions 1250, 3750, 150, 600 as the UK 1023A; the 600 is read as for 1023A (depth of the border along the base).",
    },
    ExtSpec {
        id: "M62",
        basis: Basis::Uganda,
        family: Family::WordedMarking,
        name: "STOP (text marking)",
        diagram: "M62",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::Word(WordDef { text: "STOP", heights: &[(Sel::AtMost(60), 1600), (Sel::Over(60), 2800)], width_mm: 2050 }),
        geometry: GeometryStatus::Placeholder("letter shapes are not given"),
        applies: "Extra emphasis for the R61 Stop sign; not with signals or railway crossings or alone. Top of the word about 3 m (up to 15 m) in advance of the stop line. Other messages (SLOW, BUS STOP, TAXI) are variations.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M62 (PDF p.185); Vol.1 section E.4 (p.38)",
        notes: "Width 2050 mm (the same for both sizes); length along the road 1600 or 2800 mm. Which size goes with which speed is not stated; the split at 60 km/h follows the arrows' rule and is an input choice (`size_mm`).",
    },
    ExtSpec {
        id: "M63",
        basis: Basis::Uganda,
        family: Family::WordedMarking,
        name: "Speed limit marking",
        diagram: "M63",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::None,
        geometry: GeometryStatus::CatalogueOnly("elongated roundel and numerals: the figure's dimensions (1900, 50, 4500, 9000, 1500, 3000) cannot be assigned to features with confidence"),
        applies: "Emphasis for an R49 speed limit sign; in the centre of the lane, not more than 50 m beyond the sign; may be repeated to 500 m.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M63 (PDF p.186); Vol.1 section E.4 (p.38)",
        notes: "Elongated in the direction of travel.",
    },
    ExtSpec {
        id: "M71",
        basis: Basis::Uganda,
        family: Family::ParkingBay,
        name: "Parking bay",
        diagram: "M71",
        tsrgd: "",
        colour: MarkingColour::White,
        kind: Kind::None,
        geometry: GeometryStatus::CatalogueOnly("the manual varies the bay by site (parallel or angled); only minima are given"),
        applies: "Roads with a speed limit of 50 km/h or less where parking is needed; not within 10 m of intersections, major accesses or pedestrian crossings. Need not be reflectorised.",
        citation: "Uganda TSM Vol.2 (2004) App.3 sign page M71 (PDF p.187); Vol.1 section E.4 (p.38)",
        notes: "Continuous 100 mm white line outlining the bays; minimum 2500 mm wide and 5000 mm long; 500 mm dimension also shown.",
    },
];

// Lane arrow sizes by Table 9-1: 40 or less 4 m, 50 or 60 6 m, 70 9 m.
const L40: Sel = Sel::AtMost(40);
const L50_60: Sel = Sel::OneOf(&[50, 60]);
const L70: Sel = Sel::OneOf(&[70]);

static LANE_SIZES_500: &[ArrowSize] = &[
    ArrowSize { sel: L40, length_mm: 4000, width_mm: Some(500), aux: &[("shaft width", 300)] },
    ArrowSize { sel: L50_60, length_mm: 6000, width_mm: Some(500), aux: &[("shaft width", 300)] },
    ArrowSize { sel: L70, length_mm: 9000, width_mm: Some(500), aux: &[("shaft width", 300)] },
];
static LANE_SIZES_750: &[ArrowSize] = &[
    ArrowSize { sel: L40, length_mm: 4000, width_mm: Some(750), aux: &[] },
    ArrowSize { sel: L50_60, length_mm: 6000, width_mm: Some(750), aux: &[] },
    ArrowSize { sel: L70, length_mm: 9000, width_mm: Some(750), aux: &[] },
];
static LANE_SIZES_850: &[ArrowSize] = &[
    ArrowSize { sel: L40, length_mm: 4000, width_mm: Some(850), aux: &[] },
    ArrowSize { sel: L50_60, length_mm: 6000, width_mm: Some(850), aux: &[] },
    ArrowSize { sel: L70, length_mm: 9000, width_mm: Some(850), aux: &[] },
];
static LANE_SIZES_1200: &[ArrowSize] = &[
    ArrowSize { sel: L40, length_mm: 4000, width_mm: Some(1200), aux: &[] },
    ArrowSize { sel: L50_60, length_mm: 6000, width_mm: Some(1200), aux: &[] },
    ArrowSize { sel: L70, length_mm: 9000, width_mm: Some(1200), aux: &[] },
];

// Figure 9-5: sizes in the order the figure lists them. The 85th percentile speed bands of
// Table 2-6 choose the length: up to 40 mph 4.5 m, 41 to 60 mph 6 m, 61 to 70 mph 9 m.
static DEFLECTION_SIZES: &[ArrowSize] = &[
    ArrowSize { sel: Sel::AtMost(40), length_mm: 4500, width_mm: Some(525), aux: &[("shaft width", 150), ("head length", 1500)] },
    ArrowSize { sel: Sel::Between(41, 60), length_mm: 6000, width_mm: Some(700), aux: &[("shaft width", 200), ("head length", 2000)] },
    ArrowSize { sel: Sel::Between(61, 70), length_mm: 9000, width_mm: Some(1050), aux: &[("shaft width", 300), ("head length", 3000)] },
];

/// Every entry of the second set and of the Uganda manual, in catalogue order.
pub fn ext_catalogue() -> &'static [ExtSpec] {
    CATALOGUE
}

/// The entries of one basis.
pub fn ext_catalogue_for(basis: Basis) -> impl Iterator<Item = &'static ExtSpec> {
    CATALOGUE.iter().filter(move |s| s.basis == basis)
}

/// The entry with an identifier (ignoring case), whatever its basis.
pub fn ext_spec(id: &str) -> Option<&'static ExtSpec> {
    let id = id.trim();
    CATALOGUE.iter().find(|s| s.id.eq_ignore_ascii_case(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_unique_and_every_entry_is_cited() {
        for (i, s) in CATALOGUE.iter().enumerate() {
            assert!(CATALOGUE[i + 1..].iter().all(|t| !t.id.eq_ignore_ascii_case(s.id)), "{}", s.id);
            assert!(!s.citation.is_empty() && !s.notes.is_empty() && !s.applies.is_empty(), "{}", s.id);
            match s.basis {
                Basis::Uk => assert!(s.citation.contains("TSM Chapter 5 (2018)") || s.citation.contains("TSRGD 2016"), "{}", s.id),
                Basis::Uganda => assert!(s.citation.starts_with("Uganda TSM Vol.2 (2004)"), "{}", s.id),
            }
        }
    }
}
