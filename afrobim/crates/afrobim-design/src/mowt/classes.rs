//! Road design classes, their traffic ranges, design speeds by terrain, and the class parameter
//! tables of section 4 (Tables 4-2a, 4-2b and 4-3 to 4-9).
//!
//! The class parameter tables give, for each design class and terrain, the design speed and the
//! limits the manual calls "the design standards" (section 6.5.1, p. 122): minimum stopping and
//! passing sight distance, minimum radius, maximum gradients (desirable and absolute), minimum
//! gradient in cut, maximum superelevation, minimum `K` values, normal cross fall, shoulder cross
//! fall and right of way. Section 6.2.3 ii (p. 85) refers to "the minimum horizontal curve radius as
//! shown in Tables 4-3 through 4-9".
//!
//! Cells that contradict another table of the manual are withheld, see [`super::known_slips`].

use serde::{Deserialize, Serialize};

use super::{refuse, Cited, Entry, Source};
use crate::error::DesignError;

/// The seven design classes of the manual (section 4.5, p. 43): I, II and III are bitumen surfaced,
/// A, B and C gravel surfaced; I is divided into Ia (four lane) and Ib (two lane).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoadClass {
    Ia,
    Ib,
    II,
    III,
    A,
    B,
    C,
}

impl RoadClass {
    pub const ALL: [RoadClass; 7] = [RoadClass::Ia, RoadClass::Ib, RoadClass::II, RoadClass::III, RoadClass::A, RoadClass::B, RoadClass::C];

    /// The class as the manual's tables name it.
    pub fn manual_name(self) -> &'static str {
        match self {
            RoadClass::Ia => "Ia Paved",
            RoadClass::Ib => "Ib Paved",
            RoadClass::II => "II Paved",
            RoadClass::III => "III Paved",
            RoadClass::A => "A Gravel",
            RoadClass::B => "B Gravel",
            RoadClass::C => "C Gravel",
        }
    }

    /// Short id (`Ia`, `II`, `A`, ...).
    pub fn id(self) -> &'static str {
        match self {
            RoadClass::Ia => "Ia",
            RoadClass::Ib => "Ib",
            RoadClass::II => "II",
            RoadClass::III => "III",
            RoadClass::A => "A",
            RoadClass::B => "B",
            RoadClass::C => "C",
        }
    }

    pub fn parse(text: &str) -> Option<RoadClass> {
        let t = text.trim().to_ascii_lowercase();
        let t = t.trim_end_matches("paved").trim_end_matches("gravel").trim();
        RoadClass::ALL.into_iter().find(|c| c.id().eq_ignore_ascii_case(t))
    }

    pub fn surface(self) -> Surface {
        match self {
            RoadClass::Ia | RoadClass::Ib | RoadClass::II | RoadClass::III => Surface::Paved,
            _ => Surface::Gravel,
        }
    }

    fn index(self) -> usize {
        RoadClass::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Surface {
    /// Bitumen surfaced (design classes I, II, III).
    Paved,
    /// Gravel surfaced (design classes A, B, C).
    Gravel,
}

/// Terrain classes of section 5.2 (p. 53), plus the "Urban/Peri-Urban" column of the class tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Terrain {
    Flat,
    Rolling,
    Mountainous,
    /// The "Urban/Peri-Urban" column of Tables 4-3 to 4-9.
    UrbanPeriUrban,
    /// Defined in section 5.2 but without a column in the class tables: no limits are given.
    Escarpment,
}

impl Terrain {
    /// The terrains that have a column in the class tables.
    pub const TABULATED: [Terrain; 4] = [Terrain::Flat, Terrain::Rolling, Terrain::Mountainous, Terrain::UrbanPeriUrban];

    pub fn manual_name(self) -> &'static str {
        match self {
            Terrain::Flat => "Flat",
            Terrain::Rolling => "Rolling",
            Terrain::Mountainous => "Mountainous",
            Terrain::UrbanPeriUrban => "Urban/Peri-Urban",
            Terrain::Escarpment => "Escarpment",
        }
    }

    pub fn parse(text: &str) -> Option<Terrain> {
        let t = text.trim().to_ascii_lowercase();
        match t.as_str() {
            "flat" => Some(Terrain::Flat),
            "rolling" => Some(Terrain::Rolling),
            "mountainous" | "mountain" => Some(Terrain::Mountainous),
            "urban" | "peri-urban" | "urban/peri-urban" | "urban_peri_urban" => Some(Terrain::UrbanPeriUrban),
            "escarpment" => Some(Terrain::Escarpment),
            _ => None,
        }
    }

    fn column(self) -> Result<usize, DesignError> {
        match self {
            Terrain::Flat => Ok(0),
            Terrain::Rolling => Ok(1),
            Terrain::Mountainous => Ok(2),
            Terrain::UrbanPeriUrban => Ok(3),
            Terrain::Escarpment => refuse(
                "The manual defines escarpment terrain (section 5.2, p. 53, transverse slope above 70 %) but its class tables 4-3 to 4-9 have no escarpment column, and section 6.2.3 iii (p. 87) says switchback curves are generally outside the standards for all design classes. It gives no limits for escarpment terrain, so none are built in.",
            ),
        }
    }
}

/// Section 5.2 (printed p. 53, PDF p. 62): the transverse terrain slope that defines each terrain
/// class. Flat is "around 5 %", rolling is "20 % >= slope > 5 %", mountainous "70 % >= slope > 20 %",
/// escarpment "> 70 %". A slope of exactly 5 % or less is read as flat.
pub fn terrain_for_transverse_slope(percent: f64) -> Cited<Terrain> {
    let src = Source::new("text", "5.2", 53, 62);
    let t = if percent <= 5.0 {
        Terrain::Flat
    } else if percent <= 20.0 {
        Terrain::Rolling
    } else if percent <= 70.0 {
        Terrain::Mountainous
    } else {
        Terrain::Escarpment
    };
    Cited::new(t, "% transverse slope", src)
}

/// One class parameter of Tables 4-3 to 4-9, in the order of the printed rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Parameter {
    DesignSpeed,
    MinStoppingSightDistance,
    MinPassingSightDistance,
    MinHorizontalCurveRadius,
    MaxGradientDesirable,
    MaxGradientAbsolute,
    MinGradientInCut,
    MaxSuperelevation,
    CrestKStopping,
    CrestKPassing,
    SagKStopping,
    NormalCrossFall,
    ShoulderCrossFall,
    RightOfWay,
}

impl Parameter {
    pub const ALL: [Parameter; 14] = [
        Parameter::DesignSpeed,
        Parameter::MinStoppingSightDistance,
        Parameter::MinPassingSightDistance,
        Parameter::MinHorizontalCurveRadius,
        Parameter::MaxGradientDesirable,
        Parameter::MaxGradientAbsolute,
        Parameter::MinGradientInCut,
        Parameter::MaxSuperelevation,
        Parameter::CrestKStopping,
        Parameter::CrestKPassing,
        Parameter::SagKStopping,
        Parameter::NormalCrossFall,
        Parameter::ShoulderCrossFall,
        Parameter::RightOfWay,
    ];

    /// The row label as printed.
    pub fn label(self) -> &'static str {
        match self {
            Parameter::DesignSpeed => "Design Speed",
            Parameter::MinStoppingSightDistance => "Min. Stopping Sight Distance",
            Parameter::MinPassingSightDistance => "Min. Passing Sight Distance",
            Parameter::MinHorizontalCurveRadius => "Min. Horizontal Curve Radius",
            Parameter::MaxGradientDesirable => "Max. Gradient (desirable)",
            Parameter::MaxGradientAbsolute => "Max. Gradient (absolute)",
            Parameter::MinGradientInCut => "Minimum Gradient in cut",
            Parameter::MaxSuperelevation => "Maximum Superelevation",
            Parameter::CrestKStopping => "Crest Vertical Curve stopping",
            Parameter::CrestKPassing => "Crest Vertical Curve passing",
            Parameter::SagKStopping => "Sag Vertical Curve stopping",
            Parameter::NormalCrossFall => "Normal Cross fall",
            Parameter::ShoulderCrossFall => "Shoulder Cross fall",
            Parameter::RightOfWay => "Right of Way",
        }
    }

    /// The unit as printed.
    pub fn unit(self) -> &'static str {
        match self {
            Parameter::DesignSpeed => "km/h",
            Parameter::MinStoppingSightDistance | Parameter::MinPassingSightDistance | Parameter::MinHorizontalCurveRadius | Parameter::RightOfWay => "m",
            Parameter::MaxGradientDesirable | Parameter::MaxGradientAbsolute | Parameter::MinGradientInCut | Parameter::MaxSuperelevation | Parameter::NormalCrossFall | Parameter::ShoulderCrossFall => "%",
            Parameter::CrestKStopping | Parameter::CrestKPassing | Parameter::SagKStopping => "Kmin",
        }
    }

    fn index(self) -> usize {
        Parameter::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }
}

struct ClassTable {
    table: &'static str,
    printed_page: u16,
    pdf_page: u16,
    /// 14 rows (see [`Parameter::ALL`]) by 4 columns (flat, rolling, mountainous, urban/peri-urban).
    rows: [[f64; 4]; 14],
}

const TABLES: [ClassTable; 7] = [
    // Table 4-3, Ia Paved (dual carriageway), p. 45
    ClassTable {
        table: "Table 4-3",
        printed_page: 45,
        pdf_page: 54,
        rows: [
            [120.0, 100.0, 80.0, 50.0],
            [205.0, 160.0, 115.0, 60.0],
            [795.0, 670.0, 545.0, 345.0],
            [710.0, 415.0, 240.0, 100.0],
            [3.0, 4.5, 6.0, 6.0],
            [4.0, 6.5, 8.0, 8.0],
            [0.5, 0.5, 0.5, 0.5],
            [7.0, 7.0, 7.0, 4.0],
            [103.0, 80.0, 32.0, 9.0],
            [664.0, 475.0, 310.0, 126.0],
            [50.0, 37.0, 25.0, 11.0],
            [2.5, 2.5, 2.5, 2.5],
            [4.0, 4.0, 4.0, 4.0],
            [60.0, 60.0, 60.0, 40.0],
        ],
    },
    // Table 4-4, Ib Paved, p. 45 (as rendered; the flat column is withheld)
    ClassTable {
        table: "Table 4-4",
        printed_page: 45,
        pdf_page: 54,
        rows: [
            [120.0, 100.0, 80.0, 50.0],
            [205.0, 160.0, 115.0, 60.0],
            [795.0, 670.0, 545.0, 345.0],
            [710.0, 415.0, 240.0, 100.0],
            [3.0, 4.5, 6.0, 6.0],
            [4.0, 6.5, 8.0, 8.0],
            [0.5, 0.5, 0.5, 0.5],
            [7.0, 7.0, 7.0, 4.0],
            [103.0, 80.0, 32.0, 9.0],
            [664.0, 475.0, 310.0, 126.0],
            [50.0, 37.0, 25.0, 11.0],
            [2.5, 2.5, 2.5, 2.5],
            [4.0, 4.0, 4.0, 4.0],
            [60.0, 60.0, 60.0, 40.0],
        ],
    },
    // Table 4-5, II Paved, p. 46
    ClassTable {
        table: "Table 4-5",
        printed_page: 46,
        pdf_page: 55,
        rows: [
            [90.0, 70.0, 60.0, 50.0],
            [135.0, 95.0, 75.0, 58.0],
            [605.0, 485.0, 410.0, 345.0],
            [320.0, 185.0, 130.0, 100.0],
            [3.5, 5.5, 6.0, 6.0],
            [5.5, 7.5, 8.0, 8.0],
            [0.5, 0.5, 0.5, 0.5],
            [7.0, 7.0, 7.0, 4.0],
            [43.0, 22.0, 14.0, 9.0],
            [307.0, 246.0, 176.0, 126.0],
            [30.0, 20.0, 15.0, 11.0],
            [2.5, 2.5, 2.5, 2.5],
            [4.0, 4.0, 4.0, 4.0],
            [50.0, 50.0, 50.0, 30.0],
        ],
    },
    // Table 4-6, III Paved, p. 46
    ClassTable {
        table: "Table 4-6",
        printed_page: 46,
        pdf_page: 55,
        rows: [
            [80.0, 70.0, 50.0, 50.0],
            [115.0, 95.0, 60.0, 60.0],
            [545.0, 485.0, 345.0, 345.0],
            [240.0, 185.0, 85.0, 100.0],
            [4.0, 5.5, 9.0, 9.0],
            [6.0, 7.5, 11.0, 11.0],
            [0.5, 0.5, 0.5, 0.5],
            [7.0, 7.0, 7.0, 4.0],
            [32.0, 22.0, 9.0, 9.0],
            [310.0, 246.0, 126.0, 126.0],
            [25.0, 20.0, 11.0, 11.0],
            [2.5, 2.5, 2.5, 2.5],
            [4.0, 4.0, 4.0, 4.0],
            [50.0, 50.0, 50.0, 30.0],
        ],
    },
    // Table 4-7, A Gravel, p. 46
    ClassTable {
        table: "Table 4-7",
        printed_page: 46,
        pdf_page: 55,
        rows: [
            [90.0, 80.0, 70.0, 50.0],
            [135.0, 115.0, 95.0, 60.0],
            [605.0, 545.0, 485.0, 345.0],
            [320.0, 240.0, 185.0, 100.0],
            [3.5, 5.0, 7.0, 7.0],
            [5.5, 7.0, 9.0, 9.0],
            [0.5, 0.5, 0.5, 0.5],
            [7.0, 7.0, 7.0, 4.0],
            [43.0, 32.0, 22.0, 9.0],
            [307.0, 310.0, 246.0, 126.0],
            [30.0, 25.0, 20.0, 11.0],
            [4.0, 4.0, 4.0, 4.0],
            [4.0, 4.0, 4.0, 4.0],
            [40.0, 40.0, 40.0, 30.0],
        ],
    },
    // Table 4-8, B Gravel, p. 47
    ClassTable {
        table: "Table 4-8",
        printed_page: 47,
        pdf_page: 56,
        rows: [
            [80.0, 60.0, 50.0, 50.0],
            [115.0, 75.0, 60.0, 60.0],
            [545.0, 410.0, 345.0, 345.0],
            [240.0, 130.0, 85.0, 100.0],
            [4.0, 6.0, 9.0, 9.0],
            [6.0, 8.0, 11.0, 11.0],
            [0.5, 0.5, 0.5, 0.5],
            [7.0, 7.0, 7.0, 4.0],
            [32.0, 14.0, 9.0, 9.0],
            [310.0, 176.0, 126.0, 126.0],
            [25.0, 15.0, 11.0, 11.0],
            [4.0, 4.0, 4.0, 4.0],
            [4.0, 4.0, 4.0, 4.0],
            [30.0, 30.0, 30.0, 30.0],
        ],
    },
    // Table 4-9, C Gravel, p. 47
    ClassTable {
        table: "Table 4-9",
        printed_page: 47,
        pdf_page: 56,
        rows: [
            [60.0, 50.0, 40.0, 50.0],
            [75.0, 60.0, 45.0, 60.0],
            [410.0, 345.0, 285.0, 345.0],
            [130.0, 85.0, 55.0, 100.0],
            [5.0, 7.0, 10.0, 7.0],
            [7.0, 9.0, 12.0, 9.0],
            [0.5, 0.5, 0.5, 0.5],
            [7.0, 7.0, 7.0, 4.0],
            [22.0, 14.0, 5.0, 9.0],
            [246.0, 176.0, 86.0, 126.0],
            [15.0, 11.0, 8.0, 11.0],
            [4.0, 4.0, 4.0, 4.0],
            [4.0, 4.0, 4.0, 4.0],
            [30.0, 30.0, 30.0, 30.0],
        ],
    },
];

const WHY_IB_FLAT: &str = "in Table 4-4 the flat column of Class Ib shows design speed 120 km/h (the same figures as Class Ia), but Table 4-2a gives 110 km/h for Class Ib in flat terrain, and the PDF page also carries a second, hidden set of figures for Class Ib (110 km/h, stopping sight distance 180 m, passing 730 m, radius 530 m) that is not visible on the rendered page; the manual does not settle which governs";
const WHY_CREST_80: &str = "the printed Kmin 80 for Class Ia and Ib in rolling terrain (100 km/h) is not the 62 that Table 6-11 and Figure 6-25 give for 100 km/h, nor S^2/404 = 160^2/404 = 63.4; the PDF page also holds a hidden set with 62 for Class Ib";
const WHY_PASS_307: &str = "the printed passing Kmin 307 for 90 km/h is not the 387 of Table 6-12 (605^2/946 = 386.9)";
const WHY_II_GRADE: &str = "Table 4-5 gives 6 % desirable and 8 % absolute for Class II in mountainous terrain (60 km/h), but Tables 6-14 and 6-15 give 8 % and 10 % for mountainous terrain at 60 km/h; section 6.5.1 says the class tables govern, yet the values equal those of Class Ia at 80 km/h";
const WHY_C_FLAT: &str = "the printed Kmin values for Class C in flat terrain (60 km/h) are those of 70 km/h (stopping 22 against 14 in Table 6-11, passing 246 against 176 in Table 6-12), while the sight distances in the same column are those of 60 km/h";
const WHY_C_ROLL: &str = "the printed Kmin values for Class C in rolling terrain (50 km/h) are those of 60 km/h (stopping 14 against 9 in Table 6-11, passing 176 against 126 in Table 6-12), while the sight distances in the same column are those of 50 km/h";

/// Why a cell is held back, if it is.
fn withheld(class: RoadClass, terrain: Terrain, p: Parameter) -> Option<&'static str> {
    use Parameter::*;
    match (class, terrain, p) {
        (RoadClass::Ib, Terrain::Flat, _) => Some(WHY_IB_FLAT),
        (RoadClass::Ia | RoadClass::Ib, Terrain::Rolling, CrestKStopping) => Some(WHY_CREST_80),
        (RoadClass::II | RoadClass::A, Terrain::Flat, CrestKPassing) => Some(WHY_PASS_307),
        (RoadClass::II, Terrain::Mountainous, MaxGradientDesirable | MaxGradientAbsolute) => Some(WHY_II_GRADE),
        (RoadClass::C, Terrain::Flat, CrestKStopping | CrestKPassing) => Some(WHY_C_FLAT),
        (RoadClass::C, Terrain::Rolling, CrestKStopping | CrestKPassing) => Some(WHY_C_ROLL),
        _ => None,
    }
}

pub(super) fn class_entry(class: RoadClass, terrain: Terrain, p: Parameter) -> Result<(Entry, Source), DesignError> {
    let col = terrain.column()?;
    let t = &TABLES[class.index()];
    let src = Source::new(t.table, "4.5", t.printed_page, t.pdf_page);
    let e = match withheld(class, terrain, p) {
        Some(why) => Entry::Withheld(why),
        None => Entry::V(t.rows[p.index()][col]),
    };
    Ok((e, src))
}

/// One cell of Tables 4-3 to 4-9: design class, terrain and the printed row.
pub fn class_parameter(class: RoadClass, terrain: Terrain, p: Parameter) -> Result<Cited<f64>, DesignError> {
    let (e, src) = class_entry(class, terrain, p)?;
    let what = format!("{} for Class {} in {} terrain", p.label(), class.manual_name(), terrain.manual_name());
    let mut c = e.get(&what, src)?;
    c.unit = p.unit();
    Ok(c)
}

/// The design speed of a class and terrain, km/h. For Table 4-2a this is the **maximum** design speed.
/// Class Ib in flat terrain is refused (Table 4-2a: 110, Table 4-4: 120). The urban column is from the
/// class parameter tables; Table 4-2a has no urban column.
pub fn max_design_speed(class: RoadClass, terrain: Terrain) -> Result<Cited<f64>, DesignError> {
    class_parameter(class, terrain, Parameter::DesignSpeed)
}

/// Table 4-2a, one row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassDefinition {
    pub class: RoadClass,
    /// Name as printed (`Ia Paved`).
    pub name: &'static str,
    /// "Capacity [pcu x 1,000/day]" as printed: low and high. `None` for Class C Gravel, where the cell is empty.
    pub traffic_kpcu_per_day: Option<(f64, f64)>,
    /// Road-way width, m: low and high (a range for Ia only).
    pub roadway_width_m: (f64, f64),
    /// Functional classifications (A to E) ticked for the class.
    pub functional_classes: &'static str,
    pub source: Source,
}

/// Table 4-2a (printed p. 44, PDF p. 53).
pub fn class_definition(class: RoadClass) -> ClassDefinition {
    let source = Source::new("Table 4-2a", "4.5", 44, 53);
    let (traffic, width, func) = match class {
        RoadClass::Ia => (Some((12.0, 20.0)), (20.80, 24.60), "A"),
        RoadClass::Ib => (Some((6.0, 10.0)), (11.0, 11.0), "A, B"),
        RoadClass::II => (Some((4.0, 8.0)), (10.0, 10.0), "A, B, C"),
        RoadClass::III => (Some((2.0, 6.0)), (8.6, 8.6), "A, B, C"),
        RoadClass::A => (Some((4.0, 8.0)), (10.0, 10.0), "B, C, D"),
        RoadClass::B => (Some((2.0, 6.0)), (8.6, 8.6), "D, E"),
        RoadClass::C => (None, (6.4, 6.4), "E"),
    };
    ClassDefinition { class, name: class.manual_name(), traffic_kpcu_per_day: traffic, roadway_width_m: width, functional_classes: func, source }
}

/// Table 4-2a, "Maximum Design speed" by terrain (flat, rolling, mountainous). Class Ib in flat terrain
/// is refused: Table 4-2a prints 110 km/h and Table 4-4 prints 120 km/h.
pub fn table_4_2a_max_design_speed(class: RoadClass, terrain: Terrain) -> Result<Cited<f64>, DesignError> {
    let source = Source::new("Table 4-2a", "4.5", 44, 53);
    let (flat, rolling, mountainous): (Entry, Entry, Entry) = match class {
        RoadClass::Ia => (Entry::V(120.0), Entry::V(100.0), Entry::V(80.0)),
        RoadClass::Ib => (Entry::Withheld("Table 4-2a prints 110 km/h for Class Ib in flat terrain but Table 4-4 prints 120 km/h"), Entry::V(100.0), Entry::V(80.0)),
        RoadClass::II => (Entry::V(90.0), Entry::V(70.0), Entry::V(60.0)),
        RoadClass::III => (Entry::V(80.0), Entry::V(70.0), Entry::V(50.0)),
        RoadClass::A => (Entry::V(90.0), Entry::V(80.0), Entry::V(70.0)),
        RoadClass::B => (Entry::V(80.0), Entry::V(60.0), Entry::V(50.0)),
        RoadClass::C => (Entry::V(60.0), Entry::V(50.0), Entry::V(40.0)),
    };
    let e = match terrain {
        Terrain::Flat => flat,
        Terrain::Rolling => rolling,
        Terrain::Mountainous => mountainous,
        Terrain::UrbanPeriUrban => return refuse("Table 4-2a has no urban column; the urban design speed is in the class parameter tables 4-3 to 4-9."),
        Terrain::Escarpment => return Err(Terrain::Escarpment.column().unwrap_err()),
    };
    let mut c = e.get(&format!("the maximum design speed of Class {} in {} terrain", class.manual_name(), terrain.manual_name()), source)?;
    c.unit = "km/h";
    Ok(c)
}

/// Table 4-2b: the cross-section dimensions of a class.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassCrossSection {
    pub class: RoadClass,
    pub right_of_way_m: f64,
    /// Road way width, m (a range for Ia).
    pub roadway_width_m: (f64, f64),
    pub carriageway_width_m: f64,
    pub lane_width_m: f64,
    pub number_of_lanes: u8,
    /// Shoulder width on each side, m (the table prints `2 x 2.5`).
    pub shoulder_width_each_side_m: f64,
    /// Median width, m (low, high): Class Ia only, printed `1.2 - 5.0`; other classes print a dash.
    pub median_width_m: Option<(f64, f64)>,
}

/// Table 4-2b (printed p. 45, PDF p. 54). The roadway width in Table 4-2b equals carriageway plus both
/// shoulders (plus the median for Ia), checked in the tests.
pub fn class_cross_section(class: RoadClass) -> Cited<ClassCrossSection> {
    let src = Source::new("Table 4-2b", "4.5", 45, 54);
    let (row, rw, cw, lane, lanes, sh, median) = match class {
        RoadClass::Ia => (60.0, (20.80, 24.60), 14.6, 3.65, 4, 2.5, Some((1.2, 5.0))),
        RoadClass::Ib => (60.0, (11.0, 11.0), 7.0, 3.5, 2, 2.0, None),
        RoadClass::II => (50.0, (10.0, 10.0), 6.0, 3.0, 2, 2.0, None),
        RoadClass::III => (50.0, (8.6, 8.6), 5.6, 2.8, 2, 1.5, None),
        RoadClass::A => (40.0, (10.0, 10.0), 6.0, 3.0, 2, 2.0, None),
        RoadClass::B => (30.0, (8.6, 8.6), 5.6, 2.8, 2, 1.5, None),
        RoadClass::C => (30.0, (6.4, 6.4), 4.0, 4.0, 1, 1.2, None),
    };
    Cited::new(
        ClassCrossSection { class, right_of_way_m: row, roadway_width_m: rw, carriageway_width_m: cw, lane_width_m: lane, number_of_lanes: lanes, shoulder_width_each_side_m: sh, median_width_m: median },
        "m",
        src,
    )
}
