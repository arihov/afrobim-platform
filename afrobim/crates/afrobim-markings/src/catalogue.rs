//! The longitudinal lines of Traffic Signs Manual Chapter 5, as data.
//!
//! Every number in this file was read from the edition named by [`EDITION`] and carries the
//! figure, table or paragraph it came from. Nothing here is from memory: a marking whose
//! dimensions could not be read in that edition is not in the catalogue.
//!
//! Two tables:
//!
//! - [`catalogue`]: one [`LineSpec`] per line type. What the diagram *is*: its mark and gap, the
//!   widths prescribed for it, its TSRGD reference. Source: the dimensioned figures.
//! - [`applications`]: one [`Application`] per row of the manual's tables. Which diagram and
//!   width the manual gives for a speed limit and a road layout. Source: Tables 2-1, 2-2, 2-3
//!   and 2-5, and paragraph 3.2.1.
//!
//! Speed limits are in miles per hour, as the manual writes them. [`kmh`] and [`speed_label`]
//! convert for display only; no threshold is restated in km/h.

use serde::{Deserialize, Serialize};

/// The edition every dimension in this crate was read from.
pub const EDITION: &str = "Traffic Signs Manual, Chapter 5: Road Markings, 2018 (TSO, first published 2019, ISBN 978 0 11 553208 5), for TSRGD 2016";

/// Where the edition was obtained.
pub const EDITION_URL: &str = "https://assets.publishing.service.gov.uk/media/5c4ace6ded915d38a0611abc/traffic-signs-manual-chapter-05.pdf";

/// A longitudinal line type of the catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LineType {
    /// Centre line, diagram 1008.
    Centre1008,
    /// Centre line, diagram 1008.1.
    Centre1008_1,
    /// Lane line, diagram 1005.
    Lane1005,
    /// Lane line, diagram 1005.1.
    Lane1005_1,
    /// Warning line, diagram 1004.
    Warning1004,
    /// Warning line, diagram 1004.1.
    Warning1004_1,
    /// Edge of carriageway, diagram 1009A (short broken line).
    Edge1009A,
    /// Edge of carriageway, diagram 1010 (broken line at junctions, lay-bys, lane-drops).
    Edge1010,
    /// Edge of carriageway, diagram 1012.1 (continuous line).
    Edge1012_1,
    /// Double white lines, diagram 1013.1 version A: both lines continuous.
    Double1013_1Continuous,
    /// Double white lines, diagram 1013.1 version D, the left line (looking up-station) broken.
    Double1013_1BrokenLeft,
    /// Double white lines, diagram 1013.1 version D, the right line (looking up-station) broken.
    Double1013_1BrokenRight,
}

impl LineType {
    pub const ALL: [LineType; 12] = [
        LineType::Centre1008,
        LineType::Centre1008_1,
        LineType::Lane1005,
        LineType::Lane1005_1,
        LineType::Warning1004,
        LineType::Warning1004_1,
        LineType::Edge1009A,
        LineType::Edge1010,
        LineType::Edge1012_1,
        LineType::Double1013_1Continuous,
        LineType::Double1013_1BrokenLeft,
        LineType::Double1013_1BrokenRight,
    ];

    /// The identifier stored in a marking element: the diagram number, with a suffix for the
    /// three forms of the double line.
    pub fn id(self) -> &'static str {
        spec(self).id
    }

    /// The line type with this identifier, ignoring case and surrounding space.
    pub fn from_id(id: &str) -> Option<LineType> {
        let id = id.trim();
        LineType::ALL.into_iter().find(|t| t.id().eq_ignore_ascii_case(id))
    }
}

/// The families the manual's section 2 and 3 divide longitudinal lines into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LineGroup {
    Centre,
    Lane,
    Warning,
    Edge,
    DoubleWhite,
}

/// How a line is laid along its length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pattern {
    Continuous,
    /// A mark followed by a gap, repeated. One mark and one gap are a module.
    Broken { mark_mm: u32, gap_mm: u32 },
}

impl Pattern {
    /// Mark plus gap, mm; `None` for a continuous line.
    pub fn module_mm(self) -> Option<u32> {
        match self {
            Pattern::Continuous => None,
            Pattern::Broken { mark_mm, gap_mm } => Some(mark_mm + gap_mm),
        }
    }

    /// `"2000 mm mark, 4000 mm gap"` or `"continuous"`.
    pub fn label(self) -> String {
        match self {
            Pattern::Continuous => "continuous".to_string(),
            Pattern::Broken { mark_mm, gap_mm } => format!("{mark_mm} mm mark, {gap_mm} mm gap"),
        }
    }
}

/// The colours Chapter 5 prescribes for longitudinal lines (1.8.1 and 12.5.1 also prescribe red,
/// for red routes, which is not in this crate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum MarkingColour {
    #[default]
    White,
    Yellow,
}

impl MarkingColour {
    pub fn label(self) -> &'static str {
        match self {
            MarkingColour::White => "white",
            MarkingColour::Yellow => "yellow",
        }
    }
}

/// A speed-limit condition exactly as a column or row of the manual's tables states it, mph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpeedBand {
    /// The table gives the same value at every speed limit.
    Any,
    /// "40 or less".
    AtMost40,
    /// "Over 40".
    Over40,
    /// "50" (Table 2-3).
    Is50,
    /// "60" (Table 2-3).
    Is60,
    /// "50 or 60" (Table 2-5).
    Is50Or60,
    /// "70".
    Is70,
}

impl SpeedBand {
    /// Whether a speed limit in mph falls in the band, read literally: "50 or 60" holds 50 and
    /// 60 and nothing between, because the table has no column for anything between.
    pub fn contains(self, mph: u32) -> bool {
        match self {
            SpeedBand::Any => true,
            SpeedBand::AtMost40 => mph <= 40,
            SpeedBand::Over40 => mph > 40,
            SpeedBand::Is50 => mph == 50,
            SpeedBand::Is60 => mph == 60,
            SpeedBand::Is50Or60 => mph == 50 || mph == 60,
            SpeedBand::Is70 => mph == 70,
        }
    }

    /// The band in the manual's words.
    pub fn label(self) -> &'static str {
        match self {
            SpeedBand::Any => "any speed limit",
            SpeedBand::AtMost40 => "40 mph or less",
            SpeedBand::Over40 => "over 40 mph",
            SpeedBand::Is50 => "50 mph",
            SpeedBand::Is60 => "60 mph",
            SpeedBand::Is50Or60 => "50 or 60 mph",
            SpeedBand::Is70 => "70 mph",
        }
    }
}

/// The two lines of a double white line system (diagram 1013.1), from paragraph 3.2.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoubleLine {
    /// The line on the left looking up-station.
    pub left: Pattern,
    /// The line on the right looking up-station.
    pub right: Pattern,
    /// "The Regulations require each line to be a minimum of 100 mm wide".
    pub min_width_mm: u32,
    /// "but 150 mm should normally be used".
    pub normal_width_mm: u32,
    /// "The two lines are normally spaced 175 mm apart".
    pub normal_separation_mm: u32,
    /// "(minimum 90 mm)".
    pub min_separation_mm: u32,
    /// "The overall maximum width of both lines and the gap between them must not exceed 900 mm".
    pub max_overall_mm: u32,
}

/// One line type: what the diagram is.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LineSpec {
    pub line_type: LineType,
    /// Identifier stored in elements, e.g. `"1008.1"`.
    pub id: &'static str,
    /// Name for a dialog, e.g. `"Centre line"`.
    pub name: &'static str,
    pub group: LineGroup,
    /// TSRGD 2016 diagram number.
    pub diagram: &'static str,
    /// TSRGD 2016 Schedule-Part-Item reference as the manual prints it.
    pub tsrgd: &'static str,
    /// The pattern of a single line. For a double line see `double`; this is then `Continuous`.
    pub pattern: Pattern,
    /// The alternative widths prescribed for the diagram, mm, as dimensioned on its figure. For
    /// a double line: the minimum and the normal width of each line (any width from the minimum
    /// up is allowed within the overall limit).
    pub widths_mm: &'static [u32],
    /// The speed limits the manual assigns the diagram to.
    pub speed: SpeedBand,
    pub colour: MarkingColour,
    pub double: Option<DoubleLine>,
    /// Where the dimensions were read.
    pub citation: &'static str,
}

const BROKEN_1_5: Pattern = Pattern::Broken { mark_mm: 1000, gap_mm: 5000 };

const fn double(left: Pattern, right: Pattern) -> Option<DoubleLine> {
    Some(DoubleLine { left, right, min_width_mm: 100, normal_width_mm: 150, normal_separation_mm: 175, min_separation_mm: 90, max_overall_mm: 900 })
}

static CATALOGUE: [LineSpec; 12] = [
    LineSpec {
        line_type: LineType::Centre1008,
        id: "1008",
        name: "Centre line",
        group: LineGroup::Centre,
        diagram: "1008",
        tsrgd: "S11-4-6",
        pattern: Pattern::Broken { mark_mm: 2000, gap_mm: 4000 },
        widths_mm: &[100, 150],
        speed: SpeedBand::AtMost40,
        colour: MarkingColour::White,
        double: None,
        citation: "Chapter 5 (2018) Figure 2-1, Table 2-1, para 2.2.1; TSRGD 2016 diagram 1008",
    },
    LineSpec {
        line_type: LineType::Centre1008_1,
        id: "1008.1",
        name: "Centre line",
        group: LineGroup::Centre,
        diagram: "1008.1",
        tsrgd: "S11-4-7",
        pattern: Pattern::Broken { mark_mm: 3000, gap_mm: 6000 },
        widths_mm: &[100, 150],
        speed: SpeedBand::Over40,
        colour: MarkingColour::White,
        double: None,
        citation: "Chapter 5 (2018) Figure 2-1, Table 2-1, para 2.2.1; TSRGD 2016 diagram 1008.1",
    },
    LineSpec {
        line_type: LineType::Lane1005,
        id: "1005",
        name: "Lane line",
        group: LineGroup::Lane,
        diagram: "1005",
        tsrgd: "S11-4-4",
        pattern: BROKEN_1_5,
        widths_mm: &[100, 150],
        speed: SpeedBand::AtMost40,
        colour: MarkingColour::White,
        double: None,
        citation: "Chapter 5 (2018) Figure 2-3, Table 2-2; TSRGD 2016 diagram 1005",
    },
    LineSpec {
        line_type: LineType::Lane1005_1,
        id: "1005.1",
        name: "Lane line",
        group: LineGroup::Lane,
        diagram: "1005.1",
        tsrgd: "S11-4-5",
        pattern: Pattern::Broken { mark_mm: 2000, gap_mm: 7000 },
        widths_mm: &[100, 150],
        speed: SpeedBand::Over40,
        colour: MarkingColour::White,
        double: None,
        citation: "Chapter 5 (2018) Figure 2-3, Table 2-2; TSRGD 2016 diagram 1005.1",
    },
    LineSpec {
        line_type: LineType::Warning1004,
        id: "1004",
        name: "Warning line",
        group: LineGroup::Warning,
        diagram: "1004",
        tsrgd: "S11-4-2",
        pattern: Pattern::Broken { mark_mm: 4000, gap_mm: 2000 },
        widths_mm: &[100, 150],
        speed: SpeedBand::AtMost40,
        colour: MarkingColour::White,
        double: None,
        citation: "Chapter 5 (2018) Figure 2-4, Table 2-3, paras 2.4.3 and 2.7.1; TSRGD 2016 diagram 1004",
    },
    LineSpec {
        line_type: LineType::Warning1004_1,
        id: "1004.1",
        name: "Warning line",
        group: LineGroup::Warning,
        diagram: "1004.1",
        tsrgd: "S11-4-3",
        pattern: Pattern::Broken { mark_mm: 6000, gap_mm: 3000 },
        widths_mm: &[100, 150],
        speed: SpeedBand::Over40,
        colour: MarkingColour::White,
        double: None,
        citation: "Chapter 5 (2018) Figure 2-4, Table 2-3, paras 2.4.3 and 2.7.1; TSRGD 2016 diagram 1004.1",
    },
    LineSpec {
        line_type: LineType::Edge1009A,
        id: "1009A",
        name: "Edge of carriageway line (short broken)",
        group: LineGroup::Edge,
        diagram: "1009A",
        tsrgd: "S11-4-8",
        pattern: Pattern::Broken { mark_mm: 600, gap_mm: 300 },
        widths_mm: &[100, 150, 200],
        speed: SpeedBand::Any,
        colour: MarkingColour::White,
        double: None,
        citation: "Chapter 5 (2018) Figure 2-6, Table 2-5, para 2.5.3; TSRGD 2016 diagram 1009A",
    },
    LineSpec {
        line_type: LineType::Edge1010,
        id: "1010",
        name: "Edge of carriageway line (broken)",
        group: LineGroup::Edge,
        diagram: "1010",
        tsrgd: "S11-4-10",
        pattern: Pattern::Broken { mark_mm: 1000, gap_mm: 1000 },
        widths_mm: &[100, 150, 200, 250, 300],
        speed: SpeedBand::Any,
        colour: MarkingColour::White,
        double: None,
        citation: "Chapter 5 (2018) Figure 2-6, Table 2-5, para 2.5.4; TSRGD 2016 diagram 1010",
    },
    LineSpec {
        line_type: LineType::Edge1012_1,
        id: "1012.1",
        name: "Edge of carriageway line (continuous)",
        group: LineGroup::Edge,
        diagram: "1012.1",
        tsrgd: "S11-4-11",
        pattern: Pattern::Continuous,
        widths_mm: &[100, 150, 200],
        speed: SpeedBand::Any,
        colour: MarkingColour::White,
        double: None,
        citation: "Chapter 5 (2018) Figure 2-6, Table 2-5, para 2.5.6; TSRGD 2016 diagram 1012.1",
    },
    LineSpec {
        line_type: LineType::Double1013_1Continuous,
        id: "1013.1A",
        name: "Double white lines, both continuous",
        group: LineGroup::DoubleWhite,
        diagram: "1013.1",
        tsrgd: "S9-6-23",
        pattern: Pattern::Continuous,
        widths_mm: &[100, 150],
        speed: SpeedBand::Any,
        colour: MarkingColour::White,
        double: double(Pattern::Continuous, Pattern::Continuous),
        citation: "Chapter 5 (2018) Figure 3-1 version A, para 3.2.1; TSRGD 2016 diagram 1013.1",
    },
    LineSpec {
        line_type: LineType::Double1013_1BrokenLeft,
        id: "1013.1D-L",
        name: "Double white lines, left line broken",
        group: LineGroup::DoubleWhite,
        diagram: "1013.1",
        tsrgd: "S9-6-23",
        pattern: Pattern::Continuous,
        widths_mm: &[100, 150],
        speed: SpeedBand::Any,
        colour: MarkingColour::White,
        double: double(BROKEN_1_5, Pattern::Continuous),
        citation: "Chapter 5 (2018) Figure 3-1 version D, para 3.2.1; TSRGD 2016 diagram 1013.1",
    },
    LineSpec {
        line_type: LineType::Double1013_1BrokenRight,
        id: "1013.1D-R",
        name: "Double white lines, right line broken",
        group: LineGroup::DoubleWhite,
        diagram: "1013.1",
        tsrgd: "S9-6-23",
        pattern: Pattern::Continuous,
        widths_mm: &[100, 150],
        speed: SpeedBand::Any,
        colour: MarkingColour::White,
        double: double(Pattern::Continuous, BROKEN_1_5),
        citation: "Chapter 5 (2018) Figure 3-1 version D, para 3.2.1; TSRGD 2016 diagram 1013.1",
    },
];

/// Every line type, in the order of [`LineType::ALL`].
pub fn catalogue() -> &'static [LineSpec] {
    &CATALOGUE
}

/// The specification of one line type.
pub fn spec(line_type: LineType) -> &'static LineSpec {
    CATALOGUE.iter().find(|s| s.line_type == line_type).expect("every line type is in the catalogue")
}

/// The road layout or use a table row is for. Each value is one "Description" or "Purpose" cell
/// of Tables 2-1, 2-2, 2-3 or 2-5; [`Application::description`] carries the manual's own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoadContext {
    /// Table 2-1 "Two-lane roads, not less than 5.5 m in width"; Table 2-3 "two-lane roads less
    /// than 10 m wide".
    TwoLane,
    /// "Two-lane roads 10 m or more in width" (Tables 2-1 and 2-3).
    TwoLane10mOrWider,
    /// Table 2-1 "Three-lanes marked as two in one direction and one in the other".
    ThreeLaneTwoPlusOne,
    /// Table 2-1 "Four-lanes or more"; Table 2-3 "four or six-lane single carriageway roads".
    FourLanesOrMore,
    /// Table 2-2: a lane line between lanes in the same direction.
    LaneLine,
    /// Table 2-3: a warning line used as a lane line on the approach to a roundabout or
    /// signal-controlled junction, or on the minor road approaching a priority junction.
    LaneLineOnApproach,
    /// Table 2-5, diagram 1009A: field entrance or exit from a private drive.
    FieldEntranceOrPrivateDrive,
    /// Table 2-5, diagram 1010: lane diverge or merge, private drive, lay-by.
    JunctionDriveOrLayby,
    /// Table 2-5, diagram 1010: lane-drop.
    LaneDrop,
    /// Table 2-5, diagram 1010: emergency refuge area on a smart motorway.
    EmergencyRefugeArea,
    /// Table 2-5, diagram 1012.1: all-purpose road with no hard shoulder or hard strip.
    NoHardShoulderOrStrip,
    /// Table 2-5, diagram 1012.1: all-purpose road with a hard shoulder or hard strip.
    HardShoulderOrStrip,
    /// Table 2-5, diagram 1012.1: motorway.
    Motorway,
    /// Paragraph 3.2.1: a double white line system.
    DoubleWhiteLines,
}

/// The "minimum number of marks on each approach" columns of Table 2-3. The table has columns
/// for 30 mph, 40 mph and over 40 mph and no others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MinMarks {
    pub at_30: Option<u32>,
    pub at_40: Option<u32>,
    pub over_40: Option<u32>,
}

impl MinMarks {
    /// The minimum for a speed limit, or `None` where the table has no column for it (for
    /// example 20 mph) or shows a dash.
    pub fn for_speed(self, mph: u32) -> Option<u32> {
        match mph {
            30 => self.at_30,
            40 => self.at_40,
            m if m > 40 => self.over_40,
            _ => None,
        }
    }
}

/// One row of the manual's tables: the diagram and width it gives for a speed limit and a road
/// layout.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Application {
    pub line_type: LineType,
    pub speed: SpeedBand,
    pub road: RoadContext,
    /// The width the row gives, mm. `None` where the table gives none (diagram 1009A: "See 2.5.3").
    pub width_mm: Option<u32>,
    /// A second width the row allows: the bracketed "(150)" of Table 2-2.
    pub alternative_width_mm: Option<u32>,
    /// The row's description in the manual's words.
    pub description: &'static str,
    /// Table 2-3 only.
    pub min_marks: Option<MinMarks>,
    /// Set when the row applies only under a condition stated in a table note; the condition.
    pub exception: Option<&'static str>,
    pub citation: &'static str,
}

impl Application {
    /// Whether the row gives or allows this width.
    pub fn offers(&self, width_mm: u32) -> bool {
        self.width_mm == Some(width_mm) || self.alternative_width_mm == Some(width_mm)
    }
}

const T21: &str = "Chapter 5 (2018) Table 2-1";
const T22: &str = "Chapter 5 (2018) Table 2-2";
const T23: &str = "Chapter 5 (2018) Table 2-3";
const T25: &str = "Chapter 5 (2018) Table 2-5";

const D_TWO_LANE: &str = "Two-lane roads, not less than 5.5 m in width";
const D_TWO_LANE_WIDE: &str = "Two-lane roads 10 m or more in width";
const D_THREE_LANE: &str = "Three-lanes marked as two in one direction and one in the other (see 2.2.2)";
const D_FOUR_LANE: &str = "Four-lanes or more (see 2.2.2)";
const D_WARN_TWO_LANE: &str = "Central warning line on two-lane roads less than 10 m wide";
const D_WARN_TWO_LANE_WIDE: &str = "Central warning line on two-lane roads 10 m or more wide";
const D_WARN_FOUR_LANE: &str = "Central warning line on four or six-lane single carriageway roads";
const D_WARN_LANE: &str =
    "Lane line on all roads on approach to roundabouts or signal controlled junctions, and the minor road approaching a priority junction";
const D_1010_JUNCTION: &str = "Edge of carriageway at a road junction when a Give Way or Stop marking is not appropriate (i.e. at a lane diverge or merge), or exit from a private drive onto a public road, or at a lay-by";
const D_1010_LANE_DROP: &str = "Division between the main carriageway and a traffic lane which leaves at a junction ahead (lane-drop)";
const D_1010_ERA: &str = "Boundary between emergency refuge area and an actively managed hard shoulder on a smart motorway";
const D_1012_NO_STRIP: &str = "All-purpose roads with no hard shoulder or hard strip";
const D_1012_STRIP: &str = "All-purpose roads with hard shoulder or hard strip";
const D_1012_MOTORWAY: &str = "Motorways";

const fn row(line_type: LineType, speed: SpeedBand, road: RoadContext, width_mm: u32, description: &'static str, citation: &'static str) -> Application {
    Application { line_type, speed, road, width_mm: Some(width_mm), alternative_width_mm: None, description, min_marks: None, exception: None, citation }
}

#[allow(clippy::too_many_arguments)]
const fn warning(
    line_type: LineType,
    speed: SpeedBand,
    road: RoadContext,
    width_mm: u32,
    description: &'static str,
    at_30: Option<u32>,
    at_40: Option<u32>,
    over_40: Option<u32>,
) -> Application {
    Application {
        line_type,
        speed,
        road,
        width_mm: Some(width_mm),
        alternative_width_mm: None,
        description,
        min_marks: Some(MinMarks { at_30, at_40, over_40 }),
        exception: None,
        citation: T23,
    }
}

use LineType as T;
use RoadContext as R;
use SpeedBand as S;

static APPLICATIONS: [Application; 45] = [
    // Table 2-1 Centre lines on single carriageway roads.
    row(T::Centre1008, S::AtMost40, R::TwoLane, 100, D_TWO_LANE, T21),
    row(T::Centre1008, S::AtMost40, R::TwoLane10mOrWider, 150, D_TWO_LANE_WIDE, T21),
    row(T::Centre1008, S::AtMost40, R::ThreeLaneTwoPlusOne, 150, D_THREE_LANE, T21),
    row(T::Centre1008, S::AtMost40, R::FourLanesOrMore, 150, D_FOUR_LANE, T21),
    Application {
        exception: Some("where the 85th percentile speed is generally below 40 mph; used consistently and not interchanged with diagram 1008.1 at frequent intervals (Table 2-1 NOTE 1, paras 2.1 and 2.2.1)"),
        ..row(T::Centre1008, S::Over40, R::TwoLane, 100, D_TWO_LANE, "Chapter 5 (2018) Table 2-1 and NOTE 1")
    },
    row(T::Centre1008_1, S::Over40, R::TwoLane, 100, D_TWO_LANE, T21),
    row(T::Centre1008_1, S::Over40, R::TwoLane10mOrWider, 150, D_TWO_LANE_WIDE, T21),
    row(T::Centre1008_1, S::Over40, R::ThreeLaneTwoPlusOne, 150, D_THREE_LANE, T21),
    row(T::Centre1008_1, S::Over40, R::FourLanesOrMore, 150, D_FOUR_LANE, T21),
    // Table 2-2 Lane lines: "100 (150)".
    Application { alternative_width_mm: Some(150), ..row(T::Lane1005, S::AtMost40, R::LaneLine, 100, "Lane line", T22) },
    Application { alternative_width_mm: Some(150), ..row(T::Lane1005_1, S::Over40, R::LaneLine, 100, "Lane line", T22) },
    // Table 2-3 Warning lines.
    warning(T::Warning1004, S::AtMost40, R::TwoLane, 100, D_WARN_TWO_LANE, Some(5), Some(7), None),
    warning(T::Warning1004_1, S::Over40, R::TwoLane, 100, D_WARN_TWO_LANE, None, None, Some(7)),
    warning(T::Warning1004, S::AtMost40, R::TwoLane10mOrWider, 150, D_WARN_TWO_LANE_WIDE, Some(5), Some(7), None),
    warning(T::Warning1004_1, S::Over40, R::TwoLane10mOrWider, 150, D_WARN_TWO_LANE_WIDE, None, None, Some(7)),
    warning(T::Warning1004, S::AtMost40, R::FourLanesOrMore, 150, D_WARN_FOUR_LANE, Some(7), Some(10), None),
    warning(T::Warning1004_1, S::Over40, R::FourLanesOrMore, 150, D_WARN_FOUR_LANE, None, None, Some(10)),
    warning(T::Warning1004, S::AtMost40, R::LaneLineOnApproach, 100, D_WARN_LANE, Some(5), Some(7), None),
    warning(T::Warning1004_1, S::Is50, R::LaneLineOnApproach, 100, D_WARN_LANE, None, None, Some(7)),
    warning(T::Warning1004_1, S::Is60, R::LaneLineOnApproach, 100, D_WARN_LANE, None, None, Some(7)),
    warning(T::Warning1004_1, S::Is70, R::LaneLineOnApproach, 150, D_WARN_LANE, None, None, Some(7)),
    // Table 2-3 NOTE 1 and NOTE 2: diagram 1004 where the speed limit is over 40 mph.
    Application {
        exception: Some("where the speed limit is over 40 mph but traffic speeds are generally below 40 mph, using a minimum of 7 marks (Table 2-3 NOTE 1, para 2.1)"),
        citation: "Chapter 5 (2018) Table 2-3 NOTE 1",
        ..warning(T::Warning1004, S::Over40, R::TwoLane, 100, D_WARN_TWO_LANE, None, None, Some(7))
    },
    Application {
        exception: Some("on the minor road approach to a priority junction where the speed limit is over 40 mph (Table 2-3 NOTE 2, paras 2.4.11 and 2.4.13)"),
        min_marks: None,
        citation: "Chapter 5 (2018) Table 2-3 NOTE 2",
        ..warning(T::Warning1004, S::Over40, R::LaneLineOnApproach, 100, D_WARN_LANE, None, None, None)
    },
    // Table 2-5 Edge of carriageway markings.
    Application {
        line_type: T::Edge1009A,
        speed: S::Any,
        road: R::FieldEntranceOrPrivateDrive,
        width_mm: None,
        alternative_width_mm: None,
        description: "Edge of carriageway at a field entrance or exit from a private drive onto a public road",
        min_marks: None,
        exception: None,
        citation: "Chapter 5 (2018) Table 2-5 (width column: \"See 2.5.3\")",
    },
    row(T::Edge1010, S::AtMost40, R::JunctionDriveOrLayby, 100, D_1010_JUNCTION, T25),
    row(T::Edge1010, S::Is50Or60, R::JunctionDriveOrLayby, 150, D_1010_JUNCTION, T25),
    row(T::Edge1010, S::Is70, R::JunctionDriveOrLayby, 200, D_1010_JUNCTION, T25),
    row(T::Edge1010, S::AtMost40, R::LaneDrop, 100, D_1010_LANE_DROP, T25),
    row(T::Edge1010, S::Is50Or60, R::LaneDrop, 150, D_1010_LANE_DROP, T25),
    row(T::Edge1010, S::Is70, R::LaneDrop, 200, D_1010_LANE_DROP, T25),
    row(T::Edge1010, S::AtMost40, R::EmergencyRefugeArea, 200, D_1010_ERA, T25),
    row(T::Edge1010, S::Is50Or60, R::EmergencyRefugeArea, 200, D_1010_ERA, T25),
    row(T::Edge1010, S::Is70, R::EmergencyRefugeArea, 200, D_1010_ERA, T25),
    row(T::Edge1012_1, S::AtMost40, R::NoHardShoulderOrStrip, 100, D_1012_NO_STRIP, T25),
    row(T::Edge1012_1, S::Is50Or60, R::NoHardShoulderOrStrip, 100, D_1012_NO_STRIP, T25),
    row(T::Edge1012_1, S::Is70, R::NoHardShoulderOrStrip, 150, D_1012_NO_STRIP, T25),
    row(T::Edge1012_1, S::AtMost40, R::HardShoulderOrStrip, 150, D_1012_STRIP, T25),
    row(T::Edge1012_1, S::Is50Or60, R::HardShoulderOrStrip, 150, D_1012_STRIP, T25),
    row(T::Edge1012_1, S::Is70, R::HardShoulderOrStrip, 200, D_1012_STRIP, T25),
    row(T::Edge1012_1, S::AtMost40, R::Motorway, 200, D_1012_MOTORWAY, T25),
    row(T::Edge1012_1, S::Is50Or60, R::Motorway, 200, D_1012_MOTORWAY, T25),
    row(T::Edge1012_1, S::Is70, R::Motorway, 200, D_1012_MOTORWAY, T25),
    // Paragraph 3.2.1 Double white lines: 150 mm "should normally be used", 100 mm the minimum.
    Application {
        alternative_width_mm: Some(100),
        ..row(T::Double1013_1Continuous, S::Any, R::DoubleWhiteLines, 150, "Double white lines, both continuous", "Chapter 5 (2018) para 3.2.1")
    },
    Application {
        alternative_width_mm: Some(100),
        ..row(T::Double1013_1BrokenLeft, S::Any, R::DoubleWhiteLines, 150, "Double white lines, one line broken", "Chapter 5 (2018) para 3.2.1")
    },
    Application {
        alternative_width_mm: Some(100),
        ..row(T::Double1013_1BrokenRight, S::Any, R::DoubleWhiteLines, 150, "Double white lines, one line broken", "Chapter 5 (2018) para 3.2.1")
    },
];

/// Every table row, in the manual's order.
pub fn applications() -> impl Iterator<Item = &'static Application> {
    APPLICATIONS.iter()
}

/// The rows for one line type at a speed limit (mph), in table order. Rows that apply only
/// under a table note have [`Application::exception`] set.
pub fn applications_for(line_type: LineType, speed_limit_mph: u32) -> Vec<&'static Application> {
    applications().filter(|a| a.line_type == line_type && a.speed.contains(speed_limit_mph)).collect()
}

/// The manual's choice for a family of line, a speed limit (mph) and a road layout: every row
/// that matches, the general rows first and the rows conditional on a table note after them.
///
/// More than one general row means the manual itself offers alternatives; none means it has no
/// row for that combination. Neither case is resolved here.
pub fn select(group: LineGroup, speed_limit_mph: u32, road: RoadContext) -> Vec<&'static Application> {
    let mut rows: Vec<&'static Application> =
        applications().filter(|a| spec(a.line_type).group == group && a.road == road && a.speed.contains(speed_limit_mph)).collect();
    rows.sort_by_key(|a| a.exception.is_some());
    rows
}

/// Kilometres per hour for a speed in miles per hour (1 mile = 1.609344 km exactly, the
/// international mile). For display only: the manual's thresholds are in mph and are not
/// restated in km/h anywhere in this crate.
pub fn kmh(mph: u32) -> f64 {
    mph as f64 * 1.609344
}

/// `"40 mph (64 km/h)"`: a speed limit with its rounded km/h equivalent, for display.
pub fn speed_label(mph: u32) -> String {
    format!("{mph} mph ({:.0} km/h)", kmh(mph))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn broken(t: LineType) -> (u32, u32) {
        match spec(t).pattern {
            Pattern::Broken { mark_mm, gap_mm } => (mark_mm, gap_mm),
            Pattern::Continuous => panic!("{t:?} is continuous"),
        }
    }

    #[test]
    fn figure_2_1_centre_lines() {
        assert_eq!(broken(T::Centre1008), (2000, 4000));
        assert_eq!(broken(T::Centre1008_1), (3000, 6000));
        for t in [T::Centre1008, T::Centre1008_1] {
            assert_eq!(spec(t).widths_mm, &[100, 150]);
        }
        assert_eq!((spec(T::Centre1008).diagram, spec(T::Centre1008).tsrgd), ("1008", "S11-4-6"));
        assert_eq!((spec(T::Centre1008_1).diagram, spec(T::Centre1008_1).tsrgd), ("1008.1", "S11-4-7"));
        assert_eq!((spec(T::Centre1008).speed, spec(T::Centre1008_1).speed), (S::AtMost40, S::Over40));
    }

    #[test]
    fn figure_2_3_lane_lines() {
        assert_eq!(broken(T::Lane1005), (1000, 5000));
        assert_eq!(broken(T::Lane1005_1), (2000, 7000));
        for t in [T::Lane1005, T::Lane1005_1] {
            assert_eq!(spec(t).widths_mm, &[100, 150]);
        }
        assert_eq!((spec(T::Lane1005).diagram, spec(T::Lane1005).tsrgd), ("1005", "S11-4-4"));
        assert_eq!((spec(T::Lane1005_1).diagram, spec(T::Lane1005_1).tsrgd), ("1005.1", "S11-4-5"));
        assert_eq!((spec(T::Lane1005).speed, spec(T::Lane1005_1).speed), (S::AtMost40, S::Over40));
    }

    #[test]
    fn figure_2_4_warning_lines_are_the_inverse_of_the_centre_lines() {
        assert_eq!(broken(T::Warning1004), (4000, 2000));
        assert_eq!(broken(T::Warning1004_1), (6000, 3000));
        // 2.4.3: "The two standard modules are 6 metres and 9 metres in length".
        assert_eq!(spec(T::Warning1004).pattern.module_mm(), Some(6000));
        assert_eq!(spec(T::Warning1004_1).pattern.module_mm(), Some(9000));
        // 2.2.3: the same module as the centre line of the same speed.
        assert_eq!(spec(T::Centre1008).pattern.module_mm(), Some(6000));
        assert_eq!(spec(T::Centre1008_1).pattern.module_mm(), Some(9000));
        for t in [T::Warning1004, T::Warning1004_1] {
            assert_eq!(spec(t).widths_mm, &[100, 150]);
        }
        assert_eq!((spec(T::Warning1004).diagram, spec(T::Warning1004).tsrgd), ("1004", "S11-4-2"));
        assert_eq!((spec(T::Warning1004_1).diagram, spec(T::Warning1004_1).tsrgd), ("1004.1", "S11-4-3"));
    }

    #[test]
    fn figure_2_6_edge_lines() {
        assert_eq!(broken(T::Edge1009A), (600, 300));
        assert_eq!(broken(T::Edge1010), (1000, 1000));
        assert_eq!(spec(T::Edge1012_1).pattern, Pattern::Continuous);
        assert_eq!(spec(T::Edge1009A).widths_mm, &[100, 150, 200]);
        assert_eq!(spec(T::Edge1010).widths_mm, &[100, 150, 200, 250, 300]);
        assert_eq!(spec(T::Edge1012_1).widths_mm, &[100, 150, 200]);
        assert_eq!((spec(T::Edge1009A).diagram, spec(T::Edge1009A).tsrgd), ("1009A", "S11-4-8"));
        assert_eq!((spec(T::Edge1010).diagram, spec(T::Edge1010).tsrgd), ("1010", "S11-4-10"));
        assert_eq!((spec(T::Edge1012_1).diagram, spec(T::Edge1012_1).tsrgd), ("1012.1", "S11-4-11"));
    }

    #[test]
    fn paragraph_3_2_1_double_white_lines() {
        for t in [T::Double1013_1Continuous, T::Double1013_1BrokenLeft, T::Double1013_1BrokenRight] {
            let s = spec(t);
            let d = s.double.unwrap();
            assert_eq!((s.diagram, s.tsrgd), ("1013.1", "S9-6-23"));
            assert_eq!((d.min_width_mm, d.normal_width_mm), (100, 150));
            assert_eq!((d.normal_separation_mm, d.min_separation_mm, d.max_overall_mm), (175, 90, 900));
        }
        let a = spec(T::Double1013_1Continuous).double.unwrap();
        assert_eq!((a.left, a.right), (Pattern::Continuous, Pattern::Continuous));
        let l = spec(T::Double1013_1BrokenLeft).double.unwrap();
        assert_eq!((l.left, l.right), (Pattern::Broken { mark_mm: 1000, gap_mm: 5000 }, Pattern::Continuous));
        let r = spec(T::Double1013_1BrokenRight).double.unwrap();
        assert_eq!((r.left, r.right), (Pattern::Continuous, Pattern::Broken { mark_mm: 1000, gap_mm: 5000 }));
    }

    #[test]
    fn every_line_is_white_and_has_an_id_that_reads_back() {
        for s in catalogue() {
            assert_eq!(s.colour, MarkingColour::White);
            assert_eq!(LineType::from_id(s.id), Some(s.line_type));
            assert!(s.citation.starts_with("Chapter 5 (2018) "));
            assert!(s.citation.contains(&format!("TSRGD 2016 diagram {}", s.diagram)));
        }
        assert_eq!(LineType::from_id(" 1009a "), Some(T::Edge1009A));
        assert_eq!(LineType::from_id("1012.2"), None);
        assert_eq!(catalogue().len(), LineType::ALL.len());
    }

    /// (width, alternative) of the general rows for a type, speed and road.
    fn widths(t: LineType, mph: u32, road: RoadContext) -> Vec<(Option<u32>, Option<u32>)> {
        applications_for(t, mph).into_iter().filter(|a| a.road == road && a.exception.is_none()).map(|a| (a.width_mm, a.alternative_width_mm)).collect()
    }

    #[test]
    fn table_2_1_centre_line_widths() {
        for mph in [20, 30, 40] {
            assert_eq!(widths(T::Centre1008, mph, R::TwoLane), [(Some(100), None)]);
            for road in [R::TwoLane10mOrWider, R::ThreeLaneTwoPlusOne, R::FourLanesOrMore] {
                assert_eq!(widths(T::Centre1008, mph, road), [(Some(150), None)]);
            }
            assert!(applications_for(T::Centre1008_1, mph).is_empty());
        }
        for mph in [50, 60, 70] {
            assert_eq!(widths(T::Centre1008_1, mph, R::TwoLane), [(Some(100), None)]);
            for road in [R::TwoLane10mOrWider, R::ThreeLaneTwoPlusOne, R::FourLanesOrMore] {
                assert_eq!(widths(T::Centre1008_1, mph, road), [(Some(150), None)]);
            }
            // Diagram 1008 over 40 mph only under NOTE 1, 100 mm, two-lane roads.
            let exceptions = applications_for(T::Centre1008, mph);
            assert_eq!(exceptions.len(), 1);
            assert_eq!((exceptions[0].width_mm, exceptions[0].road), (Some(100), R::TwoLane));
            assert!(exceptions[0].exception.unwrap().contains("85th percentile"));
        }
    }

    #[test]
    fn table_2_2_lane_line_widths() {
        assert_eq!(widths(T::Lane1005, 30, R::LaneLine), [(Some(100), Some(150))]);
        assert_eq!(widths(T::Lane1005, 40, R::LaneLine), [(Some(100), Some(150))]);
        assert_eq!(widths(T::Lane1005_1, 50, R::LaneLine), [(Some(100), Some(150))]);
        assert_eq!(widths(T::Lane1005_1, 70, R::LaneLine), [(Some(100), Some(150))]);
        assert!(applications_for(T::Lane1005, 50).is_empty());
        assert!(applications_for(T::Lane1005_1, 40).is_empty());
    }

    fn warning_row(t: LineType, mph: u32, road: RoadContext) -> (u32, Option<u32>) {
        let rows: Vec<_> = applications_for(t, mph).into_iter().filter(|a| a.road == road && a.exception.is_none()).collect();
        assert_eq!(rows.len(), 1, "{t:?} {mph} {road:?}");
        (rows[0].width_mm.unwrap(), rows[0].min_marks.unwrap().for_speed(mph))
    }

    #[test]
    fn table_2_3_warning_line_widths_and_minimum_marks() {
        // (road, width, marks at 30, at 40, over 40)
        for (road, width, m30, m40, m_over) in
            [(R::TwoLane, 100, 5, 7, 7), (R::TwoLane10mOrWider, 150, 5, 7, 7), (R::FourLanesOrMore, 150, 7, 10, 10)]
        {
            assert_eq!(warning_row(T::Warning1004, 30, road), (width, Some(m30)));
            assert_eq!(warning_row(T::Warning1004, 40, road), (width, Some(m40)));
            for mph in [50, 60, 70] {
                assert_eq!(warning_row(T::Warning1004_1, mph, road), (width, Some(m_over)));
            }
        }
        assert_eq!(warning_row(T::Warning1004, 30, R::LaneLineOnApproach), (100, Some(5)));
        assert_eq!(warning_row(T::Warning1004, 40, R::LaneLineOnApproach), (100, Some(7)));
        assert_eq!(warning_row(T::Warning1004_1, 50, R::LaneLineOnApproach), (100, Some(7)));
        assert_eq!(warning_row(T::Warning1004_1, 60, R::LaneLineOnApproach), (100, Some(7)));
        assert_eq!(warning_row(T::Warning1004_1, 70, R::LaneLineOnApproach), (150, Some(7)));
        // The table has no column for 20 mph.
        assert_eq!(warning_row(T::Warning1004, 20, R::TwoLane), (100, None));
        // NOTE 1 and NOTE 2: diagram 1004 over 40 mph, as exceptions only.
        let notes: Vec<_> = applications_for(T::Warning1004, 60);
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().all(|a| a.exception.is_some() && a.width_mm == Some(100)));
        assert_eq!(notes[0].min_marks.unwrap().for_speed(60), Some(7));
        assert_eq!(notes[1].min_marks, None);
    }

    #[test]
    fn table_2_5_edge_line_widths() {
        // (road, width at 40 or less, at 50 or 60, at 70)
        let table = [
            (T::Edge1010, R::JunctionDriveOrLayby, [100, 150, 200]),
            (T::Edge1010, R::LaneDrop, [100, 150, 200]),
            (T::Edge1010, R::EmergencyRefugeArea, [200, 200, 200]),
            (T::Edge1012_1, R::NoHardShoulderOrStrip, [100, 100, 150]),
            (T::Edge1012_1, R::HardShoulderOrStrip, [150, 150, 200]),
            (T::Edge1012_1, R::Motorway, [200, 200, 200]),
        ];
        for (t, road, [low, mid, high]) in table {
            for mph in [20, 30, 40] {
                assert_eq!(widths(t, mph, road), [(Some(low), None)], "{t:?} {road:?} {mph}");
            }
            for mph in [50, 60] {
                assert_eq!(widths(t, mph, road), [(Some(mid), None)], "{t:?} {road:?} {mph}");
            }
            assert_eq!(widths(t, 70, road), [(Some(high), None)], "{t:?} {road:?} 70");
            // No column between the three.
            assert!(widths(t, 45, road).is_empty());
            assert!(widths(t, 65, road).is_empty());
        }
        // 1009A: the table gives no width.
        assert_eq!(widths(T::Edge1009A, 30, R::FieldEntranceOrPrivateDrive), [(None, None)]);
    }

    #[test]
    fn the_table_holds_the_rows_of_the_manual_and_no_others() {
        let count = |t: LineType| applications().filter(|a| a.line_type == t).count();
        assert_eq!((count(T::Centre1008), count(T::Centre1008_1)), (5, 4));
        assert_eq!((count(T::Lane1005), count(T::Lane1005_1)), (1, 1));
        assert_eq!((count(T::Warning1004), count(T::Warning1004_1)), (6, 6));
        assert_eq!((count(T::Edge1009A), count(T::Edge1010), count(T::Edge1012_1)), (1, 9, 9));
        assert_eq!(applications().filter(|a| spec(a.line_type).group == LineGroup::DoubleWhite).count(), 3);
        assert_eq!(applications().count(), 45);
        // Every row's width is one prescribed for its diagram.
        for a in applications() {
            for w in [a.width_mm, a.alternative_width_mm].into_iter().flatten() {
                assert!(spec(a.line_type).widths_mm.contains(&w), "{a:?}");
            }
        }
    }

    #[test]
    fn selection_by_speed_and_road() {
        let pick = |g, mph, road| select(g, mph, road).into_iter().map(|a| (a.line_type, a.width_mm.unwrap(), a.exception.is_some())).collect::<Vec<_>>();
        assert_eq!(pick(LineGroup::Centre, 30, R::TwoLane), [(T::Centre1008, 100, false)]);
        assert_eq!(pick(LineGroup::Centre, 60, R::TwoLane), [(T::Centre1008_1, 100, false), (T::Centre1008, 100, true)]);
        assert_eq!(pick(LineGroup::Centre, 60, R::FourLanesOrMore), [(T::Centre1008_1, 150, false)]);
        assert_eq!(pick(LineGroup::Lane, 70, R::LaneLine), [(T::Lane1005_1, 100, false)]);
        assert_eq!(pick(LineGroup::Warning, 70, R::LaneLineOnApproach), [(T::Warning1004_1, 150, false), (T::Warning1004, 100, true)]);
        assert_eq!(pick(LineGroup::Edge, 60, R::HardShoulderOrStrip), [(T::Edge1012_1, 150, false)]);
        assert_eq!(pick(LineGroup::Edge, 70, R::LaneDrop), [(T::Edge1010, 200, false)]);
        // The manual has no row for a three-lane warning line (2.4.7 covers it in words only).
        assert!(select(LineGroup::Warning, 60, R::ThreeLaneTwoPlusOne).is_empty());
    }

    #[test]
    fn speed_bands_are_read_literally_and_kmh_is_display_only() {
        assert!(S::AtMost40.contains(40) && !S::AtMost40.contains(41));
        assert!(S::Over40.contains(41) && !S::Over40.contains(40));
        assert!(S::Is50Or60.contains(50) && S::Is50Or60.contains(60) && !S::Is50Or60.contains(55));
        assert!(S::Is70.contains(70) && !S::Is70.contains(60));
        assert!((kmh(40) - 64.37376).abs() < 1e-9);
        assert_eq!(speed_label(40), "40 mph (64 km/h)");
        assert_eq!(speed_label(60), "60 mph (97 km/h)");
    }
}
