//! Catalogue lookup by basis, road stud rows, and the siting tables of Chapter 5 that go with
//! the second set (Table 9-1 lane arrows, Table 2-6 deflection arrows and tapers).

use serde::{Deserialize, Serialize};

use crate::basis::{Basis, Lookup, NotDefined};
use crate::catalogue::LineType;
use crate::ext_catalogue::{ext_spec, ExtSpec, Family, Sel};

/// What an identifier names.
#[derive(Debug, Clone, Copy)]
pub enum TypeRef {
    /// A longitudinal line of the first catalogue ([`LineType`]).
    Legacy(LineType),
    /// A second-set or Uganda entry.
    Ext(&'static ExtSpec),
    /// Road studs ([`stud_rows`]).
    Studs,
}

/// Identifier of road studs in an extension element.
pub const STUD_ID: &str = "STUD";

const UK_IDS: &[(Family, &[&str])] = &[
    (Family::DividingLine, &["1008", "1008.1"]),
    (Family::LaneLine, &["1005", "1005.1"]),
    (Family::WarningLine, &["1004", "1004.1"]),
    (Family::EdgeLine, &["1009A", "1010", "1012.1"]),
    (Family::DoubleWhiteLine, &["1013.1A", "1013.1D-L", "1013.1D-R"]),
    (Family::WaitingRestriction, &["1017", "1018.1"]),
    (Family::StopLine, &["1001", "1002.1"]),
    (Family::GiveWayLine, &["1003.1", "1003A"]),
    (Family::GiveWayTriangle, &["1023A"]),
    (Family::LaneArrow, &["1038-ahead", "1038-turn", "1038-ahead-and-turn", "1038-two-way"]),
    (Family::DeflectionArrow, &["1014"]),
    (Family::HatchedMarking, &["1040"]),
    (Family::ChevronMarking, &["1041", "1042"]),
    (Family::WordedMarking, &["1022", "1024"]),
    (Family::RoadStuds, &[STUD_ID]),
];

const UG_IDS: &[(Family, &[&str])] = &[
    (Family::DividingLine, &["M01", "M02", "M03", "M04-L", "M04-R"]),
    (Family::LaneLine, &["M11", "M12"]),
    (Family::EdgeLine, &["M21", "M22"]),
    (Family::WaitingRestriction, &["M23"]),
    (Family::StopLine, &["M31"]),
    (Family::GiveWayLine, &["M32"]),
    (Family::GiveWayTriangle, &["M61"]),
    (Family::PedestrianCrossing, &["M33", "M34", "M35"]),
    (Family::LaneArrow, &["M41", "M42", "M43", "M44"]),
    (Family::DeflectionArrow, &["M45"]),
    (Family::ChevronMarking, &["M51", "M52"]),
    (Family::IslandMarking, &["M53"]),
    (Family::ZigZagLine, &["M05"]),
    (Family::WordedMarking, &["M62", "M63"]),
    (Family::ParkingBay, &["M71"]),
    (Family::RoadStuds, &[STUD_ID]),
];

fn ids(basis: Basis, family: Family) -> &'static [&'static str] {
    let table = match basis {
        Basis::Uk => UK_IDS,
        Basis::Uganda => UG_IDS,
    };
    table.iter().find(|(f, _)| *f == family).map_or(&[][..], |(_, ids)| ids)
}

/// The identifiers of a family under a basis. A family the basis does not define is
/// [`Lookup::NotDefined`]; nothing falls back to the other basis.
///
/// ```
/// use afrobim_markings::{lookup_family, Basis, Family};
/// let warning = lookup_family(Basis::Uganda, Family::WarningLine);
/// assert!(warning.clone().into_result().unwrap_err().message().contains("not defined in the Uganda manual"));
/// assert!(lookup_family(Basis::Uk, Family::WarningLine).is_defined());
/// ```
pub fn lookup_family(basis: Basis, family: Family) -> Lookup<&'static [&'static str]> {
    let found = ids(basis, family);
    if !found.is_empty() {
        return Lookup::Found(found);
    }
    let other = match basis {
        Basis::Uk => Basis::Uganda,
        Basis::Uganda => Basis::Uk,
    };
    let elsewhere = ids(other, family);
    let what = if elsewhere.is_empty() {
        format!("{family:?}")
    } else {
        format!("{family:?} ({} {})", other.label(), elsewhere.join(", "))
    };
    Lookup::NotDefined(NotDefined { basis, what, catalogued_elsewhere: basis == Basis::Uk })
}

/// What an identifier names under a basis. A UK type asked for under the Uganda basis (and the
/// reverse) is not defined there; the caller who wants the other basis's value says so on the
/// element (`uk_fallback`).
pub fn lookup_type(basis: Basis, id: &str) -> Lookup<TypeRef> {
    let id = id.trim();
    let (found, owner) = if id.eq_ignore_ascii_case(STUD_ID) {
        (Some(TypeRef::Studs), None)
    } else if let Some(spec) = ext_spec(id) {
        (Some(TypeRef::Ext(spec)), Some(spec.basis))
    } else if let Some(t) = LineType::from_id(id) {
        (Some(TypeRef::Legacy(t)), Some(Basis::Uk))
    } else {
        (None, None)
    };
    match (found, owner) {
        (Some(t), None) => Lookup::Found(t),
        (Some(t), Some(owner)) if owner == basis => Lookup::Found(t),
        (Some(_), Some(owner)) => Lookup::NotDefined(NotDefined { basis, what: format!("{id} ({})", owner.label()), catalogued_elsewhere: basis == Basis::Uk }),
        (None, _) => Lookup::NotDefined(NotDefined { basis, what: format!("'{id}' is not a marking type of this catalogue"), catalogued_elsewhere: true }),
    }
}

// ------------------------------------------------------------------------------------ studs

/// Colour of a road stud's reflector (UK Chapter 5 4.4.1; Uganda Table E.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StudColour {
    White,
    Red,
    Amber,
    Green,
}

impl StudColour {
    pub fn label(self) -> &'static str {
        match self {
            StudColour::White => "white",
            StudColour::Red => "red",
            StudColour::Amber => "amber",
            StudColour::Green => "green",
        }
    }
}

/// One row of the stud tables: the spacing of studs along a line type.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StudRow {
    pub id: &'static str,
    pub basis: Basis,
    /// The line the studs go with: a UK diagram number, a catalogue id, or a Uganda sign number.
    pub line: &'static str,
    /// Width of the line, mm, where the row depends on it.
    pub width_mm: Option<u32>,
    /// Speed limit the row applies to (mph for UK).
    pub speed: Sel,
    /// Spacing in an urban and a rural setting, m; `None` where the manual gives none.
    pub urban_m: Option<f64>,
    pub rural_m: Option<f64>,
    /// For the UK double white line the manual gives a range: smallest and greatest, m.
    pub range_m: Option<(f64, f64)>,
    pub colour: StudColour,
    /// The manual's words where no number is given ("Not used", "See 7.4").
    pub text: &'static str,
    pub description: &'static str,
    pub citation: &'static str,
}

const fn row(id: &'static str, line: &'static str, width_mm: Option<u32>, speed: Sel, spacing_m: f64, colour: StudColour, description: &'static str, citation: &'static str) -> StudRow {
    StudRow { id, basis: Basis::Uk, line, width_mm, speed, urban_m: Some(spacing_m), rural_m: Some(spacing_m), range_m: None, colour, text: "", description, citation }
}

const T21: &str = "UK TSM Chapter 5 (2018) Table 2-1, p.12";
const T22: &str = "UK TSM Chapter 5 (2018) Table 2-2, p.15";
const T23: &str = "UK TSM Chapter 5 (2018) Table 2-3, p.17";
const T25: &str = "UK TSM Chapter 5 (2018) Table 2-5, p.20";
const E1: &str = "Uganda TSM Vol.1 (2004) Table E.1, p.40";

use StudColour::{Green, Red, White};

static STUDS: &[StudRow] = &[
    // UK Table 2-1 centre lines
    row("UK-2-1-a", "1008", Some(100), Sel::AtMost(40), 12.0, White, "Two-lane roads not less than 5.5 m wide", T21),
    row("UK-2-1-b", "1008", Some(150), Sel::AtMost(40), 6.0, White, "Two-lane roads 10 m or more; three lanes 2+1; four lanes or more", T21),
    row("UK-2-1-c", "1008", Some(100), Sel::Over(40), 12.0, White, "Diagram 1008 where the 85th percentile speed is generally below 40 mph (Table 2-1 note 1)", T21),
    row("UK-2-1-d", "1008.1", Some(100), Sel::Over(40), 18.0, White, "Two-lane roads not less than 5.5 m wide", T21),
    row("UK-2-1-e", "1008.1", Some(150), Sel::Over(40), 9.0, White, "Two-lane roads 10 m or more; three lanes 2+1; four lanes or more", T21),
    // Table 2-2 lane lines (any width)
    row("UK-2-2-a", "1005", None, Sel::AtMost(40), 12.0, White, "Lane line", T22),
    row("UK-2-2-b", "1005.1", None, Sel::Over(40), 18.0, White, "Lane line", T22),
    // Table 2-3 warning lines
    row("UK-2-3-a", "1004", Some(100), Sel::AtMost(40), 6.0, White, "Central warning line, two-lane roads less than 10 m wide; lane line on approach to roundabouts and signals", T23),
    row("UK-2-3-b", "1004.1", Some(100), Sel::Over(40), 9.0, White, "Central warning line, two-lane roads less than 10 m wide; lane line on approach at 50 and 60 mph", T23),
    row("UK-2-3-c", "1004", Some(150), Sel::AtMost(40), 6.0, White, "Central warning line, two-lane roads 10 m or more; four or six-lane single carriageway", T23),
    row("UK-2-3-d", "1004.1", Some(150), Sel::Over(40), 9.0, White, "Central warning line, two-lane roads 10 m or more; four or six-lane single carriageway; lane line on approach at 70 mph", T23),
    // Table 2-5 edge lines
    row("UK-2-5-a", "1010", None, Sel::Any, 8.0, Green, "Edge at a road junction (lane diverge or merge), private drive exit, lay-by; green reflectors at lay-bys, acceleration and deceleration lanes (4.4.1)", T25),
    StudRow { id: "UK-2-5-b", basis: Basis::Uk, line: "1010", width_mm: None, speed: Sel::Any, urban_m: None, rural_m: None, range_m: None, colour: Green, text: "See 7.4", description: "Lane-drop: spacing given in section 7.4 of the chapter, not read", citation: T25 },
    StudRow { id: "UK-2-5-c", basis: Basis::Uk, line: "1010", width_mm: Some(200), speed: Sel::Any, urban_m: None, rural_m: None, range_m: None, colour: Red, text: "Not used", description: "Boundary of an emergency refuge area on a smart motorway", citation: T25 },
    StudRow { id: "UK-2-5-d", basis: Basis::Uk, line: "1009A", width_mm: None, speed: Sel::Any, urban_m: None, rural_m: None, range_m: None, colour: Green, text: "Not used", description: "Field entrance or private drive exit (4.4.1: green studs must not be used with 1009A)", citation: T25 },
    StudRow { id: "UK-2-5-e", basis: Basis::Uk, line: "1012.1", width_mm: None, speed: Sel::Any, urban_m: None, rural_m: None, range_m: None, colour: Red, text: "Not normally used", description: "All-purpose roads with no hard shoulder or hard strip", citation: T25 },
    row("UK-2-5-f", "1012.1", Some(150), Sel::Any, 18.0, Red, "All-purpose roads with hard shoulder or hard strip (red on the nearside edge, amber next to a central reservation, 4.4.1)", T25),
    row("UK-2-5-g", "1012.1", Some(200), Sel::Any, 18.0, Red, "Motorways, and 70 mph roads with hard shoulder (red nearside, amber at the central reservation, 4.4.1)", T25),
    // 3.3.1 double white lines
    StudRow {
        id: "UK-3-3-1",
        basis: Basis::Uk,
        line: "1013.1",
        width_mm: None,
        speed: Sel::Any,
        urban_m: Some(4.5),
        rural_m: Some(4.5),
        range_m: Some((3.0, 4.5)),
        colour: White,
        text: "",
        description: "Double white lines: a single row of white bi-directional studs between the lines at centres between 3 m and 4.5 m; 4.5 m recommended for new installations; 4 m at level crossings (Chapter 6, not read)",
        citation: "UK TSM Chapter 5 (2018) 3.3.1, p.36",
    },
    // 4.5.2 chevrons
    row("UK-4-5-2", "1041", None, Sel::Any, 3.0, Red, "Red studs adjacent to chevron markings are closed up to 3 m centres (4.5.2); studs must be red on both sides of the marking (4.4.1)", "UK TSM Chapter 5 (2018) 4.5.2 and 4.4.1, pp.48 to 49"),
    // Uganda Table E.1
    StudRow { id: "UG-E1-M01", basis: Basis::Uganda, line: "M01", width_mm: None, speed: Sel::Any, urban_m: Some(6.0), rural_m: Some(6.0), range_m: None, colour: White, text: "", description: "Continuous dividing line", citation: E1 },
    StudRow { id: "UG-E1-M02", basis: Basis::Uganda, line: "M02", width_mm: None, speed: Sel::Any, urban_m: Some(6.0), rural_m: Some(6.0), range_m: None, colour: White, text: "", description: "Double dividing line (the table's M02 M04 row)", citation: E1 },
    StudRow { id: "UG-E1-M04", basis: Basis::Uganda, line: "M04-L", width_mm: None, speed: Sel::Any, urban_m: Some(6.0), rural_m: Some(6.0), range_m: None, colour: White, text: "", description: "Double dividing line (the table's M02 M04 row), left line broken", citation: E1 },
    StudRow { id: "UG-E1-M04R", basis: Basis::Uganda, line: "M04-R", width_mm: None, speed: Sel::Any, urban_m: Some(6.0), rural_m: Some(6.0), range_m: None, colour: White, text: "", description: "Double dividing line (the table's M02 M04 row), right line broken", citation: E1 },
    StudRow { id: "UG-E1-M03", basis: Basis::Uganda, line: "M03", width_mm: None, speed: Sel::Any, urban_m: Some(18.0), rural_m: Some(24.0), range_m: None, colour: White, text: "", description: "Broken dividing line", citation: E1 },
    StudRow { id: "UG-E1-M12", basis: Basis::Uganda, line: "M12", width_mm: None, speed: Sel::Any, urban_m: Some(18.0), rural_m: Some(24.0), range_m: None, colour: White, text: "", description: "Broken lane line", citation: E1 },
    StudRow { id: "UG-E1-M21", basis: Basis::Uganda, line: "M21", width_mm: None, speed: Sel::Any, urban_m: Some(18.0), rural_m: Some(24.0), range_m: None, colour: Red, text: "amber on edge lines next to medians", description: "Edge line; red, amber on edge lines next to medians; one-sided (uni-directional) studs normally", citation: E1 },
    StudRow { id: "UG-E1-M22", basis: Basis::Uganda, line: "M22", width_mm: None, speed: Sel::Any, urban_m: Some(3.0), rural_m: Some(3.0), range_m: None, colour: Green, text: "", description: "Continuity line", citation: E1 },
];

/// Every stud row of both bases.
pub fn stud_catalogue() -> &'static [StudRow] {
    STUDS
}

/// The stud rows of a basis for a line type. `speed` is in the basis's own unit; rows that do
/// not depend on speed always match. `width_mm` narrows rows that depend on the line's width.
///
/// An empty result means the basis gives no stud spacing for that line.
pub fn stud_rows(basis: Basis, line: &str, width_mm: Option<u32>, speed: Option<u32>) -> Vec<&'static StudRow> {
    STUDS
        .iter()
        .filter(|r| r.basis == basis && (r.line.eq_ignore_ascii_case(line.trim()) || (r.line == "1013.1" && line.trim().starts_with("1013.1"))))
        .filter(|r| match (r.width_mm, width_mm) {
            (Some(rw), Some(w)) => rw == w,
            _ => true,
        })
        .filter(|r| match (r.speed, speed) {
            (Sel::Any, _) => true,
            (sel, Some(v)) => sel.contains(v),
            (_, None) => true,
        })
        .collect()
}

/// The 4.5.3 reduction: the standard 18 m spacing is reduced to 9 m on bends with a radius of
/// curvature less than 450 m (UK TSM Chapter 5 (2018) 4.5.3, p.49). Any other spacing is
/// returned as it is, since the paragraph speaks of the 18 m standard only.
pub fn uk_stud_spacing_on_bend(spacing_m: f64, radius_m: f64) -> f64 {
    if (spacing_m - 18.0).abs() < 1e-9 && radius_m < 450.0 {
        9.0
    } else {
        spacing_m
    }
}

/// Siting of the Uganda lane arrows M41 to M43 on the approach to a stop or give way line: the
/// first arrow 15 m from the line and the second a further 30 m; "where 6 metre arrows are used
/// the distances should be increased by 50%" (Uganda TSM Vol.2 (2004) App.3 sign pages M41 to
/// M43, PDF pp.176 to 178). Returns (first from the line, second from the first), m, for an
/// arrow length of 4 or 6 m; `None` for other lengths.
pub fn uganda_lane_arrow_siting(length_m: u32) -> Option<(f64, f64)> {
    match length_m {
        4 => Some((15.0, 30.0)),
        6 => Some((22.5, 45.0)),
        _ => None,
    }
}

/// The sight distance below which overtaking is prohibited, m, for a traffic speed in km/h
/// (Uganda TSM Vol.2 (2004) App.3 sign page M01, PDF p.161): 50 km/h 150 m, 60 km/h 180 m,
/// 80 km/h 240 m, 100 km/h 300 m. `None` for any other speed, which the table does not give.
pub fn uganda_overtaking_sight_distance_m(speed_kmh: u32) -> Option<u32> {
    match speed_kmh {
        50 => Some(150),
        60 => Some(180),
        80 => Some(240),
        100 => Some(300),
        _ => None,
    }
}

// ------------------------------------------------------------------------- arrow siting tables

/// One row of UK Table 9-1 (Location of lane arrows).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LaneArrowSiting {
    pub speed_limit_mph: &'static str,
    pub arrow_length_m: u32,
    /// First arrow from the Stop or Give Way line, m: smallest and greatest.
    pub first_from_line_m: (f64, f64),
    pub second_from_first_m: (f64, f64),
    pub third_from_second_m: (f64, f64),
}

/// Table 9-1 (UK TSM Chapter 5 (2018) p.97) for a speed limit in mph: 40 or less 4 m, 50 or 60
/// 6 m, 70 9 m. The 9 m row is "up to 1.5 times the above distances": the distances returned
/// for 70 mph are the table's distances with the upper bounds multiplied by 1.5 (the lower
/// bounds as the table, which says "up to"). `None` where the table has no row (for example 45).
pub fn lane_arrow_siting(speed_limit_mph: u32) -> Option<LaneArrowSiting> {
    let base = |label: &'static str, len: u32, k: f64| LaneArrowSiting {
        speed_limit_mph: label,
        arrow_length_m: len,
        first_from_line_m: (15.0, 25.0 * k),
        second_from_first_m: (30.0, 50.0 * k),
        third_from_second_m: (30.0, 50.0 * k),
    };
    match speed_limit_mph {
        v if v <= 40 => Some(base("40 or less", 4, 1.0)),
        50 | 60 => Some(base("50 or 60", 6, 1.0)),
        70 => Some(base("70", 9, 1.5)),
        _ => None,
    }
}

/// One row of UK Table 2-6 (taper and arrow details for diagrams 1040, 1040.3 and 1040.4).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DeflectionRow {
    pub speed85_label: &'static str,
    /// The taper is 1 in this number.
    pub taper_one_in: u32,
    pub arrow_length_m: f64,
    /// Distance from the tip of the arrow to the start of the taper: first, second, third arrow.
    pub tip_to_taper_m: [f64; 3],
}

static TABLE_2_6: [(u32, u32, DeflectionRow); 5] = [
    (0, 30, DeflectionRow { speed85_label: "Up to 30", taper_one_in: 40, arrow_length_m: 4.5, tip_to_taper_m: [13.75, 43.75, 79.75] }),
    (31, 40, DeflectionRow { speed85_label: "31 to 40", taper_one_in: 40, arrow_length_m: 4.5, tip_to_taper_m: [19.75, 55.75, 109.75] }),
    (41, 50, DeflectionRow { speed85_label: "41 to 50", taper_one_in: 45, arrow_length_m: 6.0, tip_to_taper_m: [21.0, 66.0, 138.0] }),
    (51, 60, DeflectionRow { speed85_label: "51 to 60", taper_one_in: 50, arrow_length_m: 6.0, tip_to_taper_m: [30.0, 84.0, 165.0] }),
    (61, 70, DeflectionRow { speed85_label: "61 to 70", taper_one_in: 55, arrow_length_m: 9.0, tip_to_taper_m: [37.5, 91.5, 190.5] }),
];

/// Table 2-6 (UK TSM Chapter 5 (2018) p.24) for the 85th percentile speed of private cars, mph.
/// `None` above 70 mph, where the table has no row.
pub fn deflection_row(speed85_mph: u32) -> Option<&'static DeflectionRow> {
    TABLE_2_6.iter().find(|(lo, hi, _)| speed85_mph >= *lo && speed85_mph <= *hi).map(|(_, _, r)| r)
}

