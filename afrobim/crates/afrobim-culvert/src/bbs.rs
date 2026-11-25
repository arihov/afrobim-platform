//! Reinforcement: an editable table of bar rules per element, the BS 8666:2020 cutting-length
//! formulas, BS 4449 unit masses and the bar bending schedule.
//!
//! A [`BarRule`] says, for one bar mark, the bar size, the shape code, how each dimension A to F
//! follows the geometry ([`DimExpr`]), how many bars sit in one element and how many elements
//! there are. [`schedule`] evaluates the table against a [`Layout`].
//!
//! Shape codes are the Km 39+136 drawing's own. [`shape_defs`] maps each to the BS 8666 shape it
//! represents and gives its cutting-length formula. There is no allowance or adjustment table:
//! each cutting length is the formula of its shape, rounded to the nearest 10 mm with halves up.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use crate::error::{invalid, not_negative, positive, CulvertError};
use crate::layout::{EndLayout, Layout};
use crate::model::{Barrel, CulvertInput};

/// Mass per metre of a high-yield bar (BS 4449, nominal), kg/m.
pub fn unit_mass_kg_per_m(diameter_mm: u32) -> Option<f64> {
    match diameter_mm {
        8 => Some(0.395),
        10 => Some(0.617),
        12 => Some(0.888),
        16 => Some(1.579),
        20 => Some(2.466),
        25 => Some(3.854),
        _ => None,
    }
}

/// Minimum bend radius of the inside of a bend, mm (BS 8666:2020 Table 2: a former of 4 d up to
/// and including 16 mm, 7 d above).
pub fn min_bend_radius_mm(diameter_mm: f64) -> f64 {
    if diameter_mm <= 16.0 {
        2.0 * diameter_mm
    } else {
        3.5 * diameter_mm
    }
}

/// The element a bar belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ElementKind {
    Apron,
    WingWall,
    Headwall,
    ToeBeam,
    /// A box culvert's own walls and slabs.
    BoxBarrel,
}

impl ElementKind {
    pub fn label(self) -> &'static str {
        match self {
            ElementKind::Apron => "Apron",
            ElementKind::WingWall => "Wing walls",
            ElementKind::Headwall => "Headwall",
            ElementKind::ToeBeam => "Toe beam",
            ElementKind::BoxBarrel => "Box barrel",
        }
    }
}

/// A geometry quantity a dimension rule can use, mm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Q {
    HeadwallWidth,
    HeadwallThickness,
    HeadwallHeight,
    ApronLength,
    ApronThickness,
    ApronAndToe,
    ToeWidth,
    /// Toe beam depth below the apron underside.
    ToeDepth,
    /// Toe beam depth below the top of the apron.
    ToeTotalDepth,
    WingLength,
    WingThickness,
    WingHeightHeadwall,
    WingHeightEnd,
    /// Width across the outer corners of the wing ends.
    OverallWidth,
    /// Outside diameter of one pipe (zero for a box).
    PipeOd,
    Cells,
    CoverWall,
    CoverSlab,
    /// Barrel length between the outer faces of the headwalls.
    BarrelLength,
    /// Barrel width, outside to outside of all cells.
    BarrelWidth,
    /// Barrel height, underside to top.
    BarrelHeight,
}

/// `constant + sum(coefficient x quantity)`, mm.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DimExpr {
    pub constant: f64,
    pub terms: Vec<(f64, Q)>,
}

impl DimExpr {
    pub fn constant(c: f64) -> DimExpr {
        DimExpr {
            constant: c,
            terms: Vec::new(),
        }
    }

    pub fn of(c: f64, terms: &[(f64, Q)]) -> DimExpr {
        DimExpr {
            constant: c,
            terms: terms.to_vec(),
        }
    }
}

/// How one dimension A to F is found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DimRule {
    Expr(DimExpr),
    /// The straight distance across two perpendicular distances, `sqrt(x^2 + y^2)`.
    Hypot(DimExpr, DimExpr),
    /// A half circle of the given radius, `pi x r` (a half-round bar).
    HalfCircle(DimExpr),
    /// One value for each mark of a series (the last is repeated if the series is longer). For
    /// dimensions that the drawing steps irregularly from mark to mark.
    PerMark(Vec<DimExpr>),
}

/// How many bars sit in one element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Count {
    Fixed(u32),
    /// This many for each cell of the barrel.
    PerCell(u32),
    /// Bars at a spacing along a length: `floor(length / spacing) + 1`.
    Spaced {
        length: DimExpr,
        spacing_mm: f64,
    },
}

/// How many elements of this kind there are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementCount {
    /// This many at each end structure.
    PerEnd(u32),
    /// This many in the whole culvert.
    Total(u32),
    /// This many for every cell plus one (the walls of a box).
    PerCellPlusOne(u32),
}

/// Where a bar's dimensions come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Basis {
    /// Worked from the geometry and the covers.
    Geometry,
    /// A number copied from the Km 39+136 drawing that the geometry does not explain.
    DrawingFixed,
    /// A placeholder pending structural design.
    Assumed,
}

/// Which barrels a rule applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Applies {
    Any,
    PipeOnly,
    BoxOnly,
}

/// Dimensions that change from mark to mark (a, b, c ...) where the element width changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Series {
    /// Number of marks, lettered a, b, c ...
    pub count: u32,
    /// Added to dimension A to F from one letter to the next.
    pub step: [DimExpr; 6],
}

/// One bar mark, or one lettered series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarRule {
    pub mark: String,
    pub element: ElementKind,
    pub description: String,
    pub diameter_mm: u32,
    /// Spacing shown in the schedule, mm.
    pub spacing_mm: Option<f64>,
    /// The drawing's shape code.
    pub shape_code: u16,
    pub dims: [Option<DimRule>; 6],
    pub per_element: Count,
    pub elements: ElementCount,
    pub series: Option<Series>,
    pub basis: Basis,
    pub applies: Applies,
}

/// The editable reinforcement table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReinforcementTable {
    /// Cover to bars in walls and beams, mm.
    pub cover_wall_mm: f64,
    /// Cover to bars in slabs (aprons), mm.
    pub cover_slab_mm: f64,
    /// Cutting lengths are rounded to the nearest multiple of this, mm.
    pub rounding_mm: f64,
    /// Added to the pipe's outside radius for the half-round bars, mm.
    pub half_round_offset_mm: f64,
    pub rows: Vec<BarRule>,
}

/// One shape code of the drawing and the BS 8666 shape it stands for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeDef {
    pub code: u16,
    pub bs8666: &'static str,
    /// The cutting length before rounding, with `r` the bend radius and `d` the bar diameter.
    pub formula: &'static str,
}

/// The drawing's shape codes and their cutting-length formulas ([`raw_cutting_length_mm`]).
/// `r` is the bend radius ([`min_bend_radius_mm`], 2 d up to Y16). Derived from the drawing's own
/// schedule: all 85 rows of the reference reproduce.
pub fn shape_defs() -> Vec<ShapeDef> {
    let s = |code, bs8666, formula| ShapeDef {
        code,
        bs8666,
        formula,
    };
    vec![
        s(100, "00 (straight)", "A"),
        s(111, "11 (L)", "A + B - 0.5 r - d"),
        s(113, "cranked end", "A + C"),
        s(117, "half round, developed length", "A"),
        s(200, "21 (U)", "A + B + C - r - 2 d"),
        s(202, "U with one angled leg", "A + B + C - 0.5 r - d"),
        s(213, "three straights", "A + B + E"),
        s(
            500,
            "51 (closed link with hooks)",
            "2 (A + B) + 2 C - 2.5 r - 5 d",
        ),
        s(900, "four-bend crank", "A + B + C + D + E - 4 (0.5 r + d)"),
    ]
}

impl ReinforcementTable {
    /// The rules of the Km 39+136 drawing, plus placeholder rules for the walls and slabs of a box
    /// barrel.
    pub fn reference() -> ReinforcementTable {
        use ApplyHelper as H;
        let c = DimExpr::constant;
        let hw = |k: f64| DimExpr::of(k, &[(1.0, Q::HeadwallWidth)]);
        let leg_slab = DimExpr::of(0.0, &[(1.0, Q::ApronThickness), (-2.0, Q::CoverSlab)]);
        let hw_wall = DimExpr::of(0.0, &[(1.0, Q::HeadwallThickness), (-2.0, Q::CoverWall)]);
        let none6 = || {
            [
                DimExpr::default(),
                DimExpr::default(),
                DimExpr::default(),
                DimExpr::default(),
                DimExpr::default(),
                DimExpr::default(),
            ]
        };
        let step = |b: DimExpr| {
            let mut s = none6();
            s[1] = b;
            s
        };
        let dims = |v: Vec<Option<DimExpr>>| {
            let mut a: [Option<DimExpr>; 6] = Default::default();
            for (i, x) in v.into_iter().enumerate() {
                a[i] = x;
            }
            a
        };
        let some = |v: f64| Some(c(v));
        let list = |v: &[f64]| v.iter().map(|x| c(*x)).collect::<Vec<_>>();
        // Series B of the 03 bars: the drawing's lengths at the reference overall width, growing
        // by a tenth of the extra overall width per step so a wider end structure lengthens them.
        let b03: Vec<DimExpr> = [
            2380.0, 2790.0, 3190.0, 3600.0, 4010.0, 4410.0, 4820.0, 5220.0, 5630.0, 6040.0, 6440.0,
        ]
        .iter()
        .enumerate()
        .map(|(k, v)| {
            // Whole tenths and hundredths, so the table survives a JSON round trip exactly.
            let f = k as f64 / 10.0;
            DimExpr::of(
                ((v - f * 6753.1) * 100.0).round() / 100.0,
                &[(f, Q::OverallWidth)],
            )
        })
        .collect();
        let rows = vec![
            // Apron
            H::row(
                "01",
                ElementKind::Apron,
                "Apron U-bar",
                12,
                None,
                200,
                dims(vec![
                    Some(leg_slab.clone()),
                    Some(hw(-1350.0)),
                    Some(leg_slab.clone()),
                ]),
                Count::Fixed(4),
                ElementCount::PerEnd(2),
                Basis::DrawingFixed,
            )
            .with_series(12, step(c(170.0))),
            H::row(
                "02",
                ElementKind::Apron,
                "Apron U-bar",
                12,
                None,
                200,
                dims(vec![
                    Some(leg_slab.clone()),
                    Some(DimExpr::of(
                        0.0,
                        &[
                            (1.0, Q::ApronAndToe),
                            (1.0, Q::HeadwallThickness),
                            (-2.0, Q::CoverWall),
                        ],
                    )),
                    Some(leg_slab.clone()),
                ]),
                Count::Spaced {
                    length: DimExpr::of(0.0, &[(1.0, Q::OverallWidth)]),
                    spacing_mm: 380.0,
                },
                ElementCount::PerEnd(2),
                Basis::Geometry,
            ),
            H::row(
                "03",
                ElementKind::Apron,
                "Apron U-bar",
                12,
                None,
                200,
                dims(vec![None, None, None]),
                Count::Fixed(2),
                ElementCount::PerEnd(2),
                Basis::DrawingFixed,
            )
            .with_series(11, none6())
            .with_per_mark(
                0,
                list(&[
                    100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 170.0, 170.0, 170.0, 170.0, 170.0,
                ]),
            )
            .with_per_mark(1, b03)
            .with_per_mark(
                2,
                list(&[
                    100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 50.0, 50.0, 50.0, 50.0, 50.0,
                ]),
            ),
            // Wing walls
            H::row(
                "04",
                ElementKind::WingWall,
                "Wing wall bar",
                12,
                None,
                202,
                dims(vec![
                    some(170.0),
                    Some(DimExpr::of(-23.0, &[(1.0, Q::WingLength)])),
                    some(100.0),
                ]),
                Count::Fixed(3),
                ElementCount::PerEnd(2),
                Basis::DrawingFixed,
            )
            .with_series(2, step(c(50.0))),
            H::row(
                "05A",
                ElementKind::WingWall,
                "Wing wall U-bar",
                12,
                None,
                202,
                dims(vec![some(170.0), None, some(100.0)]),
                Count::Fixed(1),
                ElementCount::PerEnd(2),
                Basis::DrawingFixed,
            )
            .with_series(8, none6())
            .with_per_mark(
                1,
                list(&[310.0, 720.0, 1140.0, 1550.0, 1960.0, 2380.0, 2790.0, 3200.0]),
            ),
            H::row(
                "05B",
                ElementKind::WingWall,
                "Wing wall U-bar",
                12,
                None,
                202,
                dims(vec![some(170.0), None, some(100.0)]),
                Count::Fixed(1),
                ElementCount::PerEnd(2),
                Basis::DrawingFixed,
            )
            .with_series(8, none6())
            .with_per_mark(
                1,
                list(&[360.0, 770.0, 1190.0, 1600.0, 2010.0, 2430.0, 2840.0, 3250.0]),
            ),
            H::row(
                "06",
                ElementKind::WingWall,
                "Wing wall L-bar",
                12,
                Some(120.0),
                111,
                dims(vec![some(100.0), some(800.0)]),
                Count::Spaced {
                    length: DimExpr::of(0.0, &[(1.0, Q::WingLength)]),
                    spacing_mm: 120.0,
                },
                ElementCount::PerEnd(2),
                Basis::DrawingFixed,
            ),
            H::row(
                "06A",
                ElementKind::WingWall,
                "Wing wall L-bar",
                12,
                None,
                111,
                dims(vec![some(100.0), some(634.0)]),
                Count::Fixed(2),
                ElementCount::PerEnd(2),
                Basis::DrawingFixed,
            )
            .with_series(3, step(c(92.0))),
            H::row(
                "06B",
                ElementKind::WingWall,
                "Wing wall U-bar",
                12,
                None,
                200,
                dims(vec![None, some(170.0), None]),
                Count::Fixed(1),
                ElementCount::PerEnd(2),
                Basis::DrawingFixed,
            )
            .with_series(14, none6())
            .with_per_mark(
                0,
                list(&[
                    710.0, 800.0, 890.0, 980.0, 1080.0, 1170.0, 1260.0, 1350.0, 1440.0, 1530.0,
                    1620.0, 1720.0, 1810.0, 1900.0,
                ]),
            )
            .with_per_mark(
                2,
                list(&[
                    710.0, 800.0, 890.0, 980.0, 1080.0, 1170.0, 1260.0, 1350.0, 1440.0, 1530.0,
                    1620.0, 1720.0, 1810.0, 1900.0,
                ]),
            ),
            H::row(
                "16",
                ElementKind::WingWall,
                "Wing wall bar",
                10,
                None,
                202,
                dims(vec![
                    Some(DimExpr::of(294.0, &[(1.0, Q::WingLength)])),
                    some(151.0),
                    some(100.0),
                ]),
                Count::Fixed(2),
                ElementCount::PerEnd(2),
                Basis::DrawingFixed,
            ),
            // Headwall
            H::row(
                "07",
                ElementKind::Headwall,
                "Headwall L-bar",
                12,
                Some(250.0),
                111,
                dims(vec![
                    some(100.0),
                    Some(DimExpr::of(
                        0.0,
                        &[(1.0, Q::HeadwallHeight), (-2.0, Q::CoverWall)],
                    )),
                ]),
                Count::Spaced {
                    length: DimExpr::of(0.0, &[(1.0, Q::HeadwallWidth), (-2.0, Q::CoverWall)]),
                    spacing_mm: 250.0,
                },
                ElementCount::PerEnd(1),
                Basis::Geometry,
            ),
            H::row(
                "08",
                ElementKind::Headwall,
                "Headwall link",
                12,
                None,
                500,
                dims(vec![None, Some(hw_wall.clone()), some(160.0)]),
                Count::PerCell(2),
                ElementCount::PerEnd(1),
                Basis::DrawingFixed,
            )
            .with_series(3, none6())
            .with_per_mark(0, list(&[768.0, 650.0, 600.0])),
            H::row(
                "09",
                ElementKind::Headwall,
                "Headwall bar",
                12,
                None,
                213,
                dims(vec![some(300.0), some(300.0)]),
                Count::PerCell(5),
                ElementCount::PerEnd(1),
                Basis::DrawingFixed,
            )
            .with_hypot(4, hw(0.0), c(350.0))
            .with_series(2, {
                let mut s = none6();
                s[4] = c(100.0);
                s
            }),
            H::row(
                "09c",
                ElementKind::Headwall,
                "Headwall bar",
                12,
                None,
                113,
                dims(vec![some(300.0), None, some(523.0)]),
                Count::Fixed(2),
                ElementCount::PerEnd(1),
                Basis::DrawingFixed,
            ),
            H::row(
                "09d",
                ElementKind::Headwall,
                "Headwall bar",
                12,
                None,
                113,
                dims(vec![some(300.0), None, some(573.0)]),
                Count::Fixed(2),
                ElementCount::PerEnd(1),
                Basis::DrawingFixed,
            ),
            H::row(
                "10",
                ElementKind::Headwall,
                "Headwall U-bar",
                12,
                None,
                200,
                dims(vec![some(800.0), Some(hw_wall.clone()), some(800.0)]),
                Count::PerCell(4),
                ElementCount::PerEnd(1),
                Basis::DrawingFixed,
            ),
            H::row(
                "11",
                ElementKind::Headwall,
                "Headwall link",
                8,
                None,
                500,
                dims(vec![None, Some(hw_wall.clone()), some(105.0)]),
                Count::PerCell(1),
                ElementCount::PerEnd(1),
                Basis::DrawingFixed,
            )
            .with_series(6, none6())
            .with_per_mark(0, list(&[404.0, 250.0, 177.0, 161.0, 199.0, 299.0])),
            H::row(
                "12",
                ElementKind::Headwall,
                "Headwall link",
                8,
                None,
                500,
                dims(vec![some(168.0), Some(hw_wall.clone()), some(105.0)]),
                Count::PerCell(2),
                ElementCount::PerEnd(1),
                Basis::DrawingFixed,
            ),
            H::row(
                "17",
                ElementKind::Headwall,
                "Headwall L-bar",
                8,
                None,
                111,
                dims(vec![some(150.0), some(500.0)]),
                Count::PerCell(5),
                ElementCount::PerEnd(1),
                Basis::DrawingFixed,
            ),
            // Toe beam
            H::row(
                "13",
                ElementKind::ToeBeam,
                "Toe beam U-bar",
                12,
                None,
                200,
                dims(vec![
                    some(150.0),
                    Some(DimExpr::of(
                        0.0,
                        &[(1.0, Q::OverallWidth), (-2.0, Q::CoverWall)],
                    )),
                    some(150.0),
                ]),
                Count::Fixed(6),
                ElementCount::PerEnd(1),
                Basis::Geometry,
            ),
            H::row(
                "14",
                ElementKind::ToeBeam,
                "Toe beam link",
                10,
                Some(300.0),
                500,
                dims(vec![
                    Some(DimExpr::of(
                        0.0,
                        &[(1.0, Q::ToeTotalDepth), (-2.0, Q::CoverWall)],
                    )),
                    Some(DimExpr::of(
                        0.0,
                        &[(1.0, Q::ToeWidth), (-2.0, Q::CoverWall)],
                    )),
                    some(130.0),
                ]),
                Count::Spaced {
                    length: DimExpr::of(0.0, &[(1.0, Q::OverallWidth), (-2.0, Q::CoverWall)]),
                    spacing_mm: 300.0,
                },
                ElementCount::PerEnd(1),
                Basis::Geometry,
            ),
            H::row(
                "15",
                ElementKind::ToeBeam,
                "Half-round bar",
                8,
                None,
                117,
                dims(vec![None]),
                Count::PerCell(4),
                ElementCount::PerEnd(1),
                Basis::Geometry,
            )
            .with_half_circle(0, DimExpr::of(0.0, &[(0.5, Q::PipeOd)]))
            .pipe_only(),
            H::row(
                "add1",
                ElementKind::ToeBeam,
                "Additional bar",
                12,
                Some(1000.0),
                900,
                dims(vec![
                    some(250.0),
                    some(70.0),
                    some(250.0),
                    some(70.0),
                    some(250.0),
                ]),
                Count::Fixed(11),
                ElementCount::PerEnd(1),
                Basis::DrawingFixed,
            ),
            H::row(
                "add2",
                ElementKind::ToeBeam,
                "Additional bar",
                12,
                Some(1000.0),
                200,
                dims(vec![some(100.0), some(70.0), some(70.0)]),
                Count::Fixed(4),
                ElementCount::PerEnd(2),
                Basis::DrawingFixed,
            ),
            H::row(
                "add3",
                ElementKind::ToeBeam,
                "Additional bar",
                12,
                Some(1000.0),
                200,
                dims(vec![some(100.0), some(70.0), some(70.0)]),
                Count::Fixed(2),
                ElementCount::PerEnd(1),
                Basis::DrawingFixed,
            ),
            H::row(
                "add4",
                ElementKind::ToeBeam,
                "Additional bar",
                12,
                Some(1000.0),
                100,
                dims(vec![some(200.0)]),
                Count::Fixed(10),
                ElementCount::PerEnd(1),
                Basis::DrawingFixed,
            ),
            // Box barrel: placeholders pending structural design, all editable.
            H::row(
                "BX1",
                ElementKind::BoxBarrel,
                "Bottom slab transverse bar",
                12,
                Some(150.0),
                100,
                [
                    Some(DimExpr::of(
                        0.0,
                        &[(1.0, Q::BarrelWidth), (-2.0, Q::CoverWall)],
                    )),
                    None,
                    None,
                    None,
                    None,
                    None,
                ],
                Count::Spaced {
                    length: DimExpr::of(0.0, &[(1.0, Q::BarrelLength), (-2.0, Q::CoverWall)]),
                    spacing_mm: 150.0,
                },
                ElementCount::Total(1),
                Basis::Assumed,
            )
            .box_only(),
            H::row(
                "BX2",
                ElementKind::BoxBarrel,
                "Top slab transverse bar",
                12,
                Some(150.0),
                100,
                [
                    Some(DimExpr::of(
                        0.0,
                        &[(1.0, Q::BarrelWidth), (-2.0, Q::CoverWall)],
                    )),
                    None,
                    None,
                    None,
                    None,
                    None,
                ],
                Count::Spaced {
                    length: DimExpr::of(0.0, &[(1.0, Q::BarrelLength), (-2.0, Q::CoverWall)]),
                    spacing_mm: 150.0,
                },
                ElementCount::Total(1),
                Basis::Assumed,
            )
            .box_only(),
            H::row(
                "BX3",
                ElementKind::BoxBarrel,
                "Slab distribution bar",
                10,
                Some(200.0),
                100,
                [
                    Some(DimExpr::of(
                        0.0,
                        &[(1.0, Q::BarrelLength), (-2.0, Q::CoverWall)],
                    )),
                    None,
                    None,
                    None,
                    None,
                    None,
                ],
                Count::Spaced {
                    length: DimExpr::of(0.0, &[(1.0, Q::BarrelWidth), (-2.0, Q::CoverWall)]),
                    spacing_mm: 200.0,
                },
                ElementCount::Total(2),
                Basis::Assumed,
            )
            .box_only(),
            H::row(
                "BX4",
                ElementKind::BoxBarrel,
                "Wall vertical bar",
                12,
                Some(150.0),
                100,
                [
                    Some(DimExpr::of(
                        0.0,
                        &[(1.0, Q::BarrelHeight), (-2.0, Q::CoverWall)],
                    )),
                    None,
                    None,
                    None,
                    None,
                    None,
                ],
                Count::Spaced {
                    length: DimExpr::of(0.0, &[(1.0, Q::BarrelLength), (-2.0, Q::CoverWall)]),
                    spacing_mm: 150.0,
                },
                ElementCount::PerCellPlusOne(2),
                Basis::Assumed,
            )
            .box_only(),
        ];
        ReinforcementTable {
            cover_wall_mm: 40.0,
            cover_slab_mm: 50.0,
            rounding_mm: 10.0,
            half_round_offset_mm: 14.6,
            rows,
        }
    }

    /// Checks the table's numbers.
    pub fn check(&self) -> Result<(), CulvertError> {
        not_negative("the wall cover", self.cover_wall_mm)?;
        not_negative("the slab cover", self.cover_slab_mm)?;
        positive("the cutting length rounding", self.rounding_mm)?;
        not_negative("the half-round offset", self.half_round_offset_mm)?;
        for r in &self.rows {
            if unit_mass_kg_per_m(r.diameter_mm).is_none() {
                return invalid(format!(
                    "bar mark {}: Y{} has no unit mass in the table (Y8 to Y25 only)",
                    r.mark, r.diameter_mm
                ));
            }
            if !shape_defs().iter().any(|s| s.code == r.shape_code) {
                return invalid(format!(
                    "bar mark {}: shape code {} is not one of the known codes",
                    r.mark, r.shape_code
                ));
            }
            if let Count::Spaced { spacing_mm, .. } = &r.per_element {
                positive(&format!("the spacing of bar mark {}", r.mark), *spacing_mm)?;
            }
            if let Some(s) = &r.series {
                if s.count == 0 || s.count > 26 {
                    return invalid(format!(
                        "bar mark {}: a series has 1 to 26 marks (a to z)",
                        r.mark
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Builder helpers for the reference table.
struct ApplyHelper;

impl ApplyHelper {
    #[allow(clippy::too_many_arguments)]
    fn row(
        mark: &str,
        element: ElementKind,
        description: &str,
        diameter_mm: u32,
        spacing_mm: Option<f64>,
        shape_code: u16,
        dims: [Option<DimExpr>; 6],
        per_element: Count,
        elements: ElementCount,
        basis: Basis,
    ) -> BarRule {
        BarRule {
            mark: mark.to_string(),
            element,
            description: description.to_string(),
            diameter_mm,
            spacing_mm,
            shape_code,
            dims: dims.map(|d| d.map(DimRule::Expr)),
            per_element,
            elements,
            series: None,
            basis,
            applies: Applies::Any,
        }
    }
}

impl BarRule {
    fn with_series(mut self, count: u32, step: [DimExpr; 6]) -> BarRule {
        self.series = Some(Series { count, step });
        self
    }

    fn with_hypot(mut self, index: usize, x: DimExpr, y: DimExpr) -> BarRule {
        self.dims[index] = Some(DimRule::Hypot(x, y));
        self
    }

    fn with_per_mark(mut self, index: usize, values: Vec<DimExpr>) -> BarRule {
        self.dims[index] = Some(DimRule::PerMark(values));
        self
    }

    fn with_half_circle(mut self, index: usize, radius: DimExpr) -> BarRule {
        self.dims[index] = Some(DimRule::HalfCircle(radius));
        self
    }

    fn pipe_only(mut self) -> BarRule {
        self.applies = Applies::PipeOnly;
        self
    }

    fn box_only(mut self) -> BarRule {
        self.applies = Applies::BoxOnly;
        self
    }
}

/// One row of the bar bending schedule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BbsRow {
    pub element: ElementKind,
    pub mark: String,
    pub description: String,
    pub diameter_mm: u32,
    pub spacing_mm: Option<f64>,
    pub bars_per_element: u32,
    pub elements: u32,
    pub total_bars: u32,
    /// Cutting length, mm.
    pub cutting_length_mm: f64,
    pub shape_code: u16,
    /// Dimensions A to F, mm.
    pub dims: [Option<f64>; 6],
    pub mass_kg: f64,
    pub basis: Basis,
}

/// The bar bending schedule.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Bbs {
    pub rows: Vec<BbsRow>,
    /// Mass by bar diameter, kg.
    pub by_diameter_kg: BTreeMap<u32, f64>,
    /// Mass by element, kg.
    pub by_element_kg: BTreeMap<ElementKind, f64>,
    pub total_kg: f64,
}

struct Ctx<'a> {
    input: &'a CulvertInput,
    layout: &'a Layout,
    end: &'a EndLayout,
}

impl Ctx<'_> {
    fn q(&self, q: Q) -> f64 {
        let e = self.end;
        let s = &e.spec;
        let t = &self.input.reinforcement;
        match q {
            Q::HeadwallWidth => e.headwall_width_mm,
            Q::HeadwallThickness => s.headwall_thickness_mm,
            Q::HeadwallHeight => s.headwall_height_mm,
            Q::ApronLength => s.apron.length_mm,
            Q::ApronThickness => s.apron.thickness_mm,
            Q::ApronAndToe => e.apron_and_toe_mm,
            Q::ToeWidth => s.toe.width_mm,
            Q::ToeDepth => s.toe.depth_mm,
            Q::ToeTotalDepth => e.toe_total_depth_mm,
            Q::WingLength => s.wing.length_mm,
            Q::WingThickness => s.wing.thickness_mm,
            Q::WingHeightHeadwall => s.wing.height_at_headwall_mm,
            Q::WingHeightEnd => s.wing.height_at_end_mm,
            Q::OverallWidth => e.overall_width_mm,
            Q::PipeOd => match self.input.barrel {
                Barrel::Pipe(p) => p.outside_diameter_mm(),
                Barrel::Box(_) => 0.0,
            },
            Q::Cells => self.input.barrel.cells() as f64,
            Q::CoverWall => t.cover_wall_mm,
            Q::CoverSlab => t.cover_slab_mm,
            Q::BarrelLength => self.layout.barrel_len_mm,
            Q::BarrelWidth => self.input.barrel.outer_width_mm(),
            Q::BarrelHeight => {
                self.input.barrel.crown_above_invert_mm() + self.input.barrel.below_invert_mm()
            }
        }
    }

    fn eval(&self, e: &DimExpr) -> f64 {
        e.constant + e.terms.iter().map(|&(k, q)| k * self.q(q)).sum::<f64>()
    }
}

/// Cutting length of one bar, mm, before rounding. `dims` are A to F in mm, `diameter_mm` is the
/// bar diameter `d`, and `r` is the bend radius ([`min_bend_radius_mm`]; 2 d up to Y16). With
/// `b = 0.5 r + d` (the allowance of one 90 degree bend):
///
/// | Code | Shape | Cutting length |
/// |---|---|---|
/// | 100 | straight | `A` |
/// | 111 | L (BS 8666 shape 11) | `A + B - b` |
/// | 113 | cranked end | `A + C` |
/// | 117 | half round, developed | `A` |
/// | 200 | U (shape 21) | `A + B + C - r - 2 d` (two bends) |
/// | 202 | U, one angled leg | `A + B + C - b` |
/// | 213 | three straights | `A + B + E` |
/// | 500 | link (shape 51) | `2 (A + B) + 2 C - 2.5 r - 5 d` |
/// | 900 | four-bend crank | `A + B + C + D + E - 4 b` |
pub fn raw_cutting_length_mm(
    code: u16,
    dims: &[Option<f64>; 6],
    diameter_mm: f64,
) -> Result<f64, CulvertError> {
    let d = diameter_mm;
    let r = min_bend_radius_mm(d);
    let bend = 0.5 * r + d;
    let names = ["A", "B", "C", "D", "E", "F"];
    // The dimensions a shape uses, which must all be present.
    let get = |used: &[usize]| -> Result<Vec<f64>, CulvertError> {
        used.iter()
            .map(|&i| {
                dims[i].ok_or_else(|| {
                    CulvertError::Invalid(format!("shape {code} needs dimension {}", names[i]))
                })
            })
            .collect()
    };
    Ok(match code {
        100 | 117 => get(&[0])?[0],
        111 => {
            let v = get(&[0, 1])?;
            v[0] + v[1] - bend
        }
        113 => {
            let v = get(&[0, 2])?;
            v[0] + v[1]
        }
        200 => {
            let v = get(&[0, 1, 2])?;
            v[0] + v[1] + v[2] - r - 2.0 * d
        }
        202 => {
            let v = get(&[0, 1, 2])?;
            v[0] + v[1] + v[2] - bend
        }
        213 => {
            let v = get(&[0, 1, 4])?;
            v[0] + v[1] + v[2]
        }
        500 => {
            let v = get(&[0, 1, 2])?;
            2.0 * (v[0] + v[1]) + 2.0 * v[2] - 2.5 * r - 5.0 * d
        }
        900 => get(&[0, 1, 2, 3, 4])?.iter().sum::<f64>() - 4.0 * bend,
        other => return invalid(format!("shape code {other} is not known")),
    })
}

/// Rounds `v` to the nearest multiple of `step`, halves up (Excel `ROUND`, not banker's rounding).
fn round_to(v: f64, step: f64) -> f64 {
    (v / step + 0.5).floor() * step
}

fn count_of(c: &Count, ctx: &Ctx) -> u32 {
    match c {
        Count::Fixed(n) => *n,
        Count::PerCell(n) => n * ctx.input.barrel.cells(),
        Count::Spaced { length, spacing_mm } => {
            let l = ctx.eval(length);
            if l < 0.0 {
                0
            } else {
                (l / spacing_mm + 1e-9).floor() as u32 + 1
            }
        }
    }
}

fn elements_of(c: ElementCount, ctx: &Ctx, per_end_multiplier: u32) -> u32 {
    match c {
        ElementCount::PerEnd(n) => n * per_end_multiplier,
        ElementCount::Total(n) => n,
        ElementCount::PerCellPlusOne(n) => n * (ctx.input.barrel.cells() + 1),
    }
}

/// The schedule for a culvert. End-structure rules are evaluated once and counted for both ends
/// when the two ends are the same, else once per end with an `R` added to the mark of the right
/// end.
pub fn schedule(input: &CulvertInput, layout: &Layout) -> Result<Bbs, CulvertError> {
    let table = &input.reinforcement;
    table.check()?;
    let same_ends = layout.left.spec == layout.right.spec
        && layout.left.headwall_width_mm == layout.right.headwall_width_mm;
    let mut rows = Vec::new();
    for rule in &table.rows {
        match (rule.applies, input.barrel.is_pipe()) {
            (Applies::PipeOnly, false) | (Applies::BoxOnly, true) => continue,
            _ => {}
        }
        let passes: Vec<(&EndLayout, u32, &str)> = if rule.element == ElementKind::BoxBarrel {
            vec![(&layout.left, 1, "")]
        } else if same_ends {
            vec![(&layout.left, 2, "")]
        } else {
            vec![(&layout.left, 1, ""), (&layout.right, 1, "R")]
        };
        for (end, mult, tag) in passes {
            let ctx = Ctx { input, layout, end };
            let per_element = count_of(&rule.per_element, &ctx);
            if per_element == 0 {
                continue;
            }
            let elements = elements_of(rule.elements, &ctx, mult);
            let n_marks = rule.series.as_ref().map_or(1, |s| s.count);
            for k in 0..n_marks {
                let mut dims = [None; 6];
                for (i, slot) in dims.iter_mut().enumerate() {
                    if let Some(rule_dim) = &rule.dims[i] {
                        let base = match rule_dim {
                            DimRule::Expr(e) => ctx.eval(e),
                            DimRule::Hypot(x, y) => ctx.eval(x).hypot(ctx.eval(y)),
                            DimRule::HalfCircle(r) => {
                                PI * (ctx.eval(r) + table.half_round_offset_mm)
                            }
                            DimRule::PerMark(list) => match list.get(k as usize).or(list.last()) {
                                Some(e) => ctx.eval(e),
                                None => {
                                    return invalid(format!(
                                        "bar mark {}: an empty list of dimensions",
                                        rule.mark
                                    ))
                                }
                            },
                        };
                        let step = rule.series.as_ref().map_or(0.0, |s| ctx.eval(&s.step[i]));
                        let v = (base + step * k as f64).round();
                        if v < 0.0 {
                            return invalid(format!("bar mark {}: dimension {} comes out at {v} mm: the element is too small for its covers", rule.mark, ["A", "B", "C", "D", "E", "F"][i]));
                        }
                        *slot = Some(v);
                    }
                }
                let raw = raw_cutting_length_mm(rule.shape_code, &dims, rule.diameter_mm as f64)?;
                let cut = round_to(raw, table.rounding_mm);
                if cut <= 0.0 {
                    return invalid(format!(
                        "bar mark {}: the cutting length is not positive",
                        rule.mark
                    ));
                }
                let mass_per_m = unit_mass_kg_per_m(rule.diameter_mm).ok_or_else(|| {
                    CulvertError::Invalid(format!("no unit mass for Y{}", rule.diameter_mm))
                })?;
                let total_bars = per_element * elements;
                let mark = if rule.series.is_some() {
                    format!("{}{}{}", rule.mark, (b'a' + k as u8) as char, tag)
                } else {
                    format!("{}{}", rule.mark, tag)
                };
                rows.push(BbsRow {
                    element: rule.element,
                    mark,
                    description: rule.description.clone(),
                    diameter_mm: rule.diameter_mm,
                    spacing_mm: rule.spacing_mm,
                    bars_per_element: per_element,
                    elements,
                    total_bars,
                    cutting_length_mm: cut,
                    shape_code: rule.shape_code,
                    dims,
                    // To the three decimals the schedule prints, so the grand total is the sum of the printed rows.
                    mass_kg: (total_bars as f64 * cut / 1000.0 * mass_per_m * 1000.0).round()
                        / 1000.0,
                    basis: rule.basis,
                });
            }
        }
    }
    let mut bbs = Bbs {
        rows,
        ..Bbs::default()
    };
    for r in &bbs.rows {
        *bbs.by_diameter_kg.entry(r.diameter_mm).or_insert(0.0) += r.mass_kg;
        *bbs.by_element_kg.entry(r.element).or_insert(0.0) += r.mass_kg;
        bbs.total_kg += r.mass_kg;
    }
    Ok(bbs)
}

impl Bbs {
    /// Mass of the rows of one element, kg.
    pub fn element_kg(&self, e: ElementKind) -> f64 {
        self.by_element_kg.get(&e).copied().unwrap_or(0.0)
    }

    /// The schedule as CSV: one row per bar mark grouped by element, then the totals by
    /// diameter and the grand total. Masses to three decimals.
    pub fn to_csv(&self) -> String {
        fn q(s: &str) -> String {
            if s.contains([',', '"', '\n']) {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s.to_string()
            }
        }
        let mut out = String::from("element,bar mark,spacing mm,description,diameter,bars per element,elements,total bars,cutting length mm,shape code,A,B,C,D,E,F,mass kg\n");
        let mut last: Option<ElementKind> = None;
        for r in &self.rows {
            if last != Some(r.element) {
                if last.is_some() {
                    out.push_str(&format!(
                        ",,,Subtotal {},,,,,,,,,,,,,{:.3}\n",
                        q(last.map_or("", ElementKind::label)),
                        self.element_kg(last.unwrap_or(r.element))
                    ));
                }
                last = Some(r.element);
            }
            let d = |i: usize| r.dims[i].map_or(String::new(), |v| format!("{v:.0}"));
            out.push_str(&format!(
                "{},{},{},{},Y{},{},{},{},{:.0},{},{},{},{},{},{},{},{:.3}\n",
                q(r.element.label()),
                q(&r.mark),
                r.spacing_mm.map_or(String::new(), |v| format!("{v:.0}")),
                q(&r.description),
                r.diameter_mm,
                r.bars_per_element,
                r.elements,
                r.total_bars,
                r.cutting_length_mm,
                r.shape_code,
                d(0),
                d(1),
                d(2),
                d(3),
                d(4),
                d(5),
                r.mass_kg
            ));
        }
        if let Some(l) = last {
            out.push_str(&format!(
                ",,,Subtotal {},,,,,,,,,,,,,{:.3}\n",
                q(l.label()),
                self.element_kg(l)
            ));
        }
        for (d, kg) in &self.by_diameter_kg {
            out.push_str(&format!(",,,Total Y{d},,,,,,,,,,,,,{kg:.3}\n"));
        }
        out.push_str(&format!(
            ",,,Grand total,,,,,,,,,,,,,{:.3}\n",
            self.total_kg
        ));
        out
    }
}
