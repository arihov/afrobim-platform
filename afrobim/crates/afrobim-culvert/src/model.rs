//! The inputs of a culvert design. Every number is an input; the defaults are those of the
//! Km 39+136 reference drawing (Iganga-Bulopa-Kamuli road) and [`default_notes`] says which
//! default came from the drawing, which was back-calculated from it and which is an assumption.

use serde::{Deserialize, Serialize};

use crate::bbs::ReinforcementTable;
use crate::error::{finite, invalid, not_negative, positive, CulvertError};
use crate::road::{Ogl, RoadSection, RoadSource, SectionPoint, Side};

/// One concrete pipe size of the catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PipeCatalogueEntry {
    pub internal_diameter_mm: f64,
    pub wall_mm: f64,
}

/// The catalogue of concrete pipe sizes with a wall thickness each. Only the 1200 mm entry (wall
/// 100) is taken from the reference drawing; the others follow the same ratio of about
/// ID / 12 rounded to 5 mm and are **editable defaults to be confirmed against the supplier's
/// pipe class**, not a standard's values.
pub fn pipe_catalogue() -> Vec<PipeCatalogueEntry> {
    [
        (450.0, 40.0),
        (600.0, 50.0),
        (750.0, 65.0),
        (900.0, 75.0),
        (1050.0, 90.0),
        (1200.0, 100.0),
        (1350.0, 115.0),
        (1500.0, 125.0),
        (1800.0, 150.0),
    ]
    .iter()
    .map(|&(internal_diameter_mm, wall_mm)| PipeCatalogueEntry {
        internal_diameter_mm,
        wall_mm,
    })
    .collect()
}

/// A concrete pipe barrel of 1, 2 or 3 cells.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PipeSpec {
    pub internal_diameter_mm: f64,
    pub wall_mm: f64,
    pub cells: u32,
    /// Clear gap between the outsides of adjacent pipes, mm (ignored for one cell).
    pub clear_spacing_mm: f64,
}

impl PipeSpec {
    /// A catalogue size with its catalogue wall.
    pub fn catalogue(
        internal_diameter_mm: f64,
        cells: u32,
        clear_spacing_mm: f64,
    ) -> Result<PipeSpec, CulvertError> {
        match pipe_catalogue().iter().find(|e| (e.internal_diameter_mm - internal_diameter_mm).abs() < 1e-9) {
            Some(e) => Ok(PipeSpec { internal_diameter_mm, wall_mm: e.wall_mm, cells, clear_spacing_mm }),
            None => invalid(format!("{internal_diameter_mm} mm is not in the pipe catalogue: build a custom PipeSpec with its own wall thickness")),
        }
    }

    pub fn outside_diameter_mm(&self) -> f64 {
        self.internal_diameter_mm + 2.0 * self.wall_mm
    }
}

/// A reinforced concrete box barrel of 1, 2 or 3 cells.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoxSpec {
    pub clear_span_mm: f64,
    pub clear_height_mm: f64,
    pub cells: u32,
    /// Wall thickness, outer walls and the walls between cells, mm.
    pub wall_mm: f64,
    pub top_slab_mm: f64,
    pub bottom_slab_mm: f64,
    /// Size of the 45 degree haunch at each internal corner, mm (0 for none).
    pub haunch_mm: f64,
}

/// The barrel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Barrel {
    Pipe(PipeSpec),
    Box(BoxSpec),
}

impl Barrel {
    pub fn cells(&self) -> u32 {
        match self {
            Barrel::Pipe(p) => p.cells,
            Barrel::Box(b) => b.cells,
        }
    }

    /// Width across all cells, outside to outside, mm.
    pub fn outer_width_mm(&self) -> f64 {
        match self {
            Barrel::Pipe(p) => {
                p.cells as f64 * p.outside_diameter_mm()
                    + p.cells.saturating_sub(1) as f64 * p.clear_spacing_mm
            }
            Barrel::Box(b) => b.cells as f64 * b.clear_span_mm + (b.cells as f64 + 1.0) * b.wall_mm,
        }
    }

    /// Height of the barrel above its invert to the top of the barrel, mm.
    pub fn crown_above_invert_mm(&self) -> f64 {
        match self {
            Barrel::Pipe(p) => p.internal_diameter_mm + p.wall_mm,
            Barrel::Box(b) => b.clear_height_mm + b.top_slab_mm,
        }
    }

    /// Depth of the barrel's own underside below the invert, mm.
    pub fn below_invert_mm(&self) -> f64 {
        match self {
            Barrel::Pipe(p) => p.wall_mm,
            Barrel::Box(b) => b.bottom_slab_mm,
        }
    }

    /// Height of the opening, mm.
    pub fn opening_height_mm(&self) -> f64 {
        match self {
            Barrel::Pipe(p) => p.internal_diameter_mm,
            Barrel::Box(b) => b.clear_height_mm,
        }
    }

    pub fn is_pipe(&self) -> bool {
        matches!(self, Barrel::Pipe(_))
    }

    fn check(&self) -> Result<(), CulvertError> {
        match self {
            Barrel::Pipe(p) => {
                positive("the pipe internal diameter", p.internal_diameter_mm)?;
                positive("the pipe wall thickness", p.wall_mm)?;
                not_negative("the clear spacing between pipes", p.clear_spacing_mm)?;
                if !(1..=3).contains(&p.cells) {
                    return invalid(format!(
                        "a pipe culvert has 1, 2 or 3 cells (got {})",
                        p.cells
                    ));
                }
                if p.cells > 1 && p.clear_spacing_mm <= 0.0 {
                    return invalid("a multi-cell pipe culvert needs a clear spacing between the pipes greater than zero");
                }
            }
            Barrel::Box(b) => {
                for (n, v) in [
                    ("the box clear span", b.clear_span_mm),
                    ("the box clear height", b.clear_height_mm),
                    ("the box wall thickness", b.wall_mm),
                    ("the top slab thickness", b.top_slab_mm),
                    ("the bottom slab thickness", b.bottom_slab_mm),
                ] {
                    positive(n, v)?;
                }
                not_negative("the haunch size", b.haunch_mm)?;
                if !(1..=3).contains(&b.cells) {
                    return invalid(format!(
                        "a box culvert has 1, 2 or 3 cells (got {})",
                        b.cells
                    ));
                }
                if b.haunch_mm * 2.0 >= b.clear_span_mm.min(b.clear_height_mm) {
                    return invalid(
                        "the haunches are as large as the opening: reduce the haunch size",
                    );
                }
            }
        }
        Ok(())
    }
}

/// How the invert levels are given.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InvertSpec {
    /// Invert levels at the two ends of the barrel (the outer faces of the headwalls), m.
    Ends { inlet_m: f64, outlet_m: f64 },
    /// The invert level where the culvert crosses the road centreline, m, and the gradient, %
    /// falling from the inlet to the outlet.
    CentreGradient { centre_m: f64, gradient_pct: f64 },
}

/// How the barrel length is found.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LengthRule {
    /// Each end where the headwall face meets the embankment side slope at the level of the top
    /// of the headwall.
    FromEmbankment,
    /// A fixed total length, mm, split equally either side of the centreline.
    Fixed { total_mm: f64 },
    /// Fixed lengths left and right of the centreline, mm.
    LeftRight { left_mm: f64, right_mm: f64 },
}

/// Limits the design is checked against. Defaults are labelled in [`default_notes`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    /// Smallest cover from the top of the barrel to the road surface, mm.
    pub min_cover_mm: f64,
    /// Smallest barrel gradient for self-cleansing, %.
    pub min_gradient_pct: f64,
}

/// Bedding, blinding and surround of the barrel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bedding {
    pub blinding_mm: f64,
    pub bedding_mm: f64,
    /// Surround beyond the outside of the outer barrels at each side, mm.
    pub surround_side_mm: f64,
    /// Surround above the top of the barrel, mm.
    pub surround_over_crown_mm: f64,
    /// Working space either side of the surround in the trench, mm.
    pub working_space_mm: f64,
}

/// A wing wall.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WingSpec {
    /// Angle between the wing and the culvert axis, degrees (45 in the reference).
    pub splay_deg: f64,
    /// Length of the water face of the wing along the wing, mm.
    pub length_mm: f64,
    pub thickness_mm: f64,
    pub height_at_headwall_mm: f64,
    pub height_at_end_mm: f64,
}

/// An apron.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ApronSpec {
    /// Length from the headwall face to the toe beam, mm.
    pub length_mm: f64,
    pub thickness_mm: f64,
}

/// A toe beam at the end of the apron.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ToeSpec {
    /// Depth below the underside of the apron, mm.
    pub depth_mm: f64,
    /// Width along the culvert axis, mm.
    pub width_mm: f64,
}

/// Weep holes through a headwall.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeepSpec {
    pub diameter_mm: f64,
    /// Number of holes in each headwall.
    pub count_per_headwall: u32,
}

/// One end of the culvert: headwall, wings, apron, toe beam and outfall protection.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EndStructure {
    /// Headwall width. None: the barrel's outer width plus twice the surround plus
    /// `headwall_extra_width_mm`.
    pub headwall_width_mm: Option<f64>,
    /// Added to the formula width when `headwall_width_mm` is None, mm (50 in the reference:
    /// 1850 against 1800).
    pub headwall_extra_width_mm: f64,
    pub headwall_height_mm: f64,
    pub headwall_thickness_mm: f64,
    /// Height of the headwall above the top of the apron, mm.
    pub headwall_above_apron_mm: f64,
    /// Level of the top of the apron below the barrel invert at this end, mm (0: level with it).
    pub apron_top_below_invert_mm: f64,
    pub wing: WingSpec,
    pub apron: ApronSpec,
    pub toe: ToeSpec,
    /// Length of the rip-rap outfall channel beyond the toe beam, mm.
    pub riprap_length_mm: f64,
    /// Blinding under the end structures, mm.
    pub blinding_mm: f64,
    pub weep: WeepSpec,
}

/// Concrete class of each element, as text so the engineer writes what the drawing says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConcreteClasses {
    /// The barrel of a box culvert (a pipe is supplied by length).
    pub barrel: String,
    pub blinding: String,
    pub bedding: String,
    pub surround: String,
    pub headwall: String,
    pub wing: String,
    pub apron: String,
    pub toe: String,
}

/// Position of the culvert in the project coordinate system, for setting out.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SettingOutInput {
    /// Easting of the culvert centreline where it crosses the road centreline, m.
    pub cl_easting_m: f64,
    pub cl_northing_m: f64,
    /// Bearing of the road centreline in the direction of increasing chainage, degrees clockwise
    /// from north.
    pub road_bearing_deg: f64,
}

/// A culvert as the engineer describes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CulvertInput {
    pub name: String,
    /// Chainage text for titles, e.g. `Km 39+136`.
    pub chainage_label: String,
    pub road: RoadSource,
    /// Original ground (offset m from the road centreline, level m) across the road at the
    /// culvert line.
    pub ogl: Vec<(f64, f64)>,
    /// Angle between the culvert axis and the road centreline, degrees; 90 is square.
    pub skew_deg: f64,
    pub barrel: Barrel,
    pub inverts: InvertSpec,
    /// The side the water comes from.
    pub inlet_side: Side,
    pub length: LengthRule,
    pub thresholds: Thresholds,
    pub bedding: Bedding,
    pub left_end: EndStructure,
    pub right_end: EndStructure,
    pub concrete: ConcreteClasses,
    /// Outfall channel bottom slope shown on the long section, %.
    pub outfall_slope_pct: f64,
    pub reinforcement: ReinforcementTable,
    pub options: QuantityOptions,
    pub setting_out: Option<SettingOutInput>,
}

/// Choices in how quantities are measured.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QuantityOptions {
    /// Deduct the barrel opening from the headwall concrete. False reproduces the reference
    /// drawing's 9.646 m3 of end-structure concrete within 2.5 %.
    pub deduct_barrel_opening: bool,
    /// Step of the numerical integration of the trench excavation along the barrel, mm.
    pub sample_step_mm: f64,
}

impl EndStructure {
    /// The reference drawing's end structure.
    pub fn reference() -> EndStructure {
        EndStructure {
            headwall_width_mm: None,
            headwall_extra_width_mm: 50.0,
            headwall_height_mm: 2000.0,
            headwall_thickness_mm: 200.0,
            headwall_above_apron_mm: 1500.0,
            apron_top_below_invert_mm: 0.0,
            wing: WingSpec {
                splay_deg: 45.0,
                length_mm: 3267.0,
                thickness_mm: 200.0,
                height_at_headwall_mm: 2000.0,
                height_at_end_mm: 500.0,
            },
            apron: ApronSpec {
                length_mm: 2110.0,
                thickness_mm: 200.0,
            },
            toe: ToeSpec {
                depth_mm: 300.0,
                width_mm: 200.0,
            },
            riprap_length_mm: 3000.0,
            blinding_mm: 50.0,
            weep: WeepSpec {
                diameter_mm: 75.0,
                count_per_headwall: 2,
            },
        }
    }

    fn check(&self, name: &str) -> Result<(), CulvertError> {
        for (n, v) in [
            ("headwall height", self.headwall_height_mm),
            ("headwall thickness", self.headwall_thickness_mm),
            ("wing thickness", self.wing.thickness_mm),
            ("wing length", self.wing.length_mm),
            (
                "wing height at the headwall",
                self.wing.height_at_headwall_mm,
            ),
            ("wing height at the end", self.wing.height_at_end_mm),
            ("apron length", self.apron.length_mm),
            ("apron thickness", self.apron.thickness_mm),
            ("toe beam depth", self.toe.depth_mm),
            ("toe beam width", self.toe.width_mm),
        ] {
            positive(&format!("the {name} {n}"), v)?;
        }
        for (n, v) in [
            ("blinding thickness", self.blinding_mm),
            ("rip-rap length", self.riprap_length_mm),
            ("headwall extra width", self.headwall_extra_width_mm),
            ("headwall above the apron", self.headwall_above_apron_mm),
        ] {
            not_negative(&format!("the {name} {n}"), v)?;
        }
        finite(
            &format!("the {name} apron top level offset"),
            self.apron_top_below_invert_mm,
        )?;
        if let Some(w) = self.headwall_width_mm {
            positive(&format!("the {name} headwall width"), w)?;
        }
        if !(self.wing.splay_deg > 0.0 && self.wing.splay_deg < 90.0) {
            return invalid(format!(
                "the {name} wing splay must be between 0 and 90 degrees (got {})",
                self.wing.splay_deg
            ));
        }
        if self.headwall_above_apron_mm > self.headwall_height_mm {
            return invalid(format!(
                "the {name} headwall is {} mm high but {} mm of it is above the apron",
                self.headwall_height_mm, self.headwall_above_apron_mm
            ));
        }
        if self.wing.height_at_end_mm > self.wing.height_at_headwall_mm {
            return invalid(format!(
                "the {name} wing is lower at the headwall ({} mm) than at its end ({} mm)",
                self.wing.height_at_headwall_mm, self.wing.height_at_end_mm
            ));
        }
        let reach = self.wing.length_mm * self.wing.splay_deg.to_radians().cos();
        if reach < self.apron.length_mm {
            return invalid(format!(
                "the {name} wings reach {reach:.0} mm along the culvert but the apron is {} mm long: the wings are shorter than the apron, so lengthen the wings or shorten the apron",
                self.apron.length_mm
            ));
        }
        if self.weep.diameter_mm < 0.0 {
            return invalid(format!(
                "the {name} weep hole diameter must not be negative"
            ));
        }
        Ok(())
    }
}

impl CulvertInput {
    /// The reference drawing: one 1200 mm concrete pipe at Km 39+136, square to the road.
    pub fn reference() -> CulvertInput {
        let points = [
            (-6.58, 1110.053),
            (-5.0, 1110.070),
            (-3.5, 1110.107),
            (0.0, 1110.195),
            (3.5, 1110.107),
            (5.0, 1110.070),
            (6.58, 1110.053),
        ];
        let road =
            RoadSection::from_points(&points, 0.5, 1.77, None).unwrap_or_else(|_| RoadSection {
                points: points
                    .iter()
                    .map(|&(o, l)| SectionPoint {
                        offset_m: o,
                        level_m: l,
                    })
                    .collect(),
                pavement_depth_m: 0.5,
                embankment_slope_n: 1.77,
                cut_slope_n: None,
            });
        CulvertInput {
            name: "1 x 1200 CPC".to_string(),
            chainage_label: "Km 39+136".to_string(),
            road: RoadSource::Points(road),
            ogl: vec![
                (-12.0, 1107.9),
                (-6.0, 1107.7),
                (0.0, 1107.5),
                (6.0, 1107.7),
                (12.0, 1107.9),
            ],
            skew_deg: 90.0,
            barrel: Barrel::Pipe(PipeSpec {
                internal_diameter_mm: 1200.0,
                wall_mm: 100.0,
                cells: 1,
                clear_spacing_mm: 0.0,
            }),
            inverts: InvertSpec::Ends {
                inlet_m: 1107.595,
                outlet_m: 1107.340,
            },
            inlet_side: Side::Left,
            length: LengthRule::FromEmbankment,
            thresholds: Thresholds {
                min_cover_mm: 600.0,
                min_gradient_pct: 1.0,
            },
            bedding: Bedding {
                blinding_mm: 50.0,
                bedding_mm: 200.0,
                surround_side_mm: 200.0,
                surround_over_crown_mm: 500.0,
                working_space_mm: 300.0,
            },
            left_end: EndStructure::reference(),
            right_end: EndStructure::reference(),
            concrete: ConcreteClasses {
                barrel: "C30/20".to_string(),
                blinding: "C15/20".to_string(),
                bedding: "C15/20".to_string(),
                surround: "C25/20".to_string(),
                headwall: "C30/20".to_string(),
                wing: "C30/20".to_string(),
                apron: "C30/20".to_string(),
                toe: "C30/20".to_string(),
            },
            outfall_slope_pct: 0.5,
            reinforcement: ReinforcementTable::reference(),
            options: QuantityOptions {
                deduct_barrel_opening: false,
                sample_step_mm: 100.0,
            },
            setting_out: None,
        }
    }

    /// Checks every input; the design calls this first.
    pub fn validate(&self) -> Result<(), CulvertError> {
        self.road.section()?;
        if self.ogl.is_empty() {
            return invalid(
                "enter the original ground line (offset and level) across the road at the culvert",
            );
        }
        Ogl::from_points(&self.ogl)?;
        if !(self.skew_deg > 0.0 && self.skew_deg <= 90.0) {
            return invalid(format!("the skew must be greater than 0 and not more than 90 degrees (got {}); 90 is square to the road", self.skew_deg));
        }
        if self.skew_deg < 15.0 {
            return invalid(format!("a skew of {} degrees is too sharp to lay out: the culvert would lie almost along the road", self.skew_deg));
        }
        self.barrel.check()?;
        match self.inverts {
            InvertSpec::Ends { inlet_m, outlet_m } => {
                finite("the inlet invert", inlet_m)?;
                finite("the outlet invert", outlet_m)?;
                if inlet_m <= outlet_m {
                    return invalid(format!("the inlet invert ({inlet_m}) must be above the outlet invert ({outlet_m}) so the barrel falls from inlet to outlet"));
                }
            }
            InvertSpec::CentreGradient {
                centre_m,
                gradient_pct,
            } => {
                finite("the centreline invert", centre_m)?;
                positive("the barrel gradient (%)", gradient_pct)?;
            }
        }
        not_negative("the minimum cover", self.thresholds.min_cover_mm)?;
        not_negative("the minimum gradient", self.thresholds.min_gradient_pct)?;
        for (n, v) in [
            ("blinding", self.bedding.blinding_mm),
            ("bedding", self.bedding.bedding_mm),
            ("surround at the sides", self.bedding.surround_side_mm),
            (
                "surround over the crown",
                self.bedding.surround_over_crown_mm,
            ),
            ("working space", self.bedding.working_space_mm),
        ] {
            not_negative(&format!("the {n} thickness"), v)?;
        }
        finite("the outfall slope", self.outfall_slope_pct)?;
        self.left_end.check("left")?;
        self.right_end.check("right")?;
        match self.length {
            LengthRule::FromEmbankment => {}
            LengthRule::Fixed { total_mm } => {
                positive("the fixed barrel length", total_mm)?;
            }
            LengthRule::LeftRight { left_mm, right_mm } => {
                positive("the left barrel length", left_mm)?;
                positive("the right barrel length", right_mm)?;
            }
        }
        self.reinforcement.check()?;
        positive("the excavation sample step", self.options.sample_step_mm)?;
        Ok(())
    }
}

/// Where a default came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefaultSource {
    /// Read from the Km 39+136 drawing.
    Drawing,
    /// Worked back from drawing figures; the note says how.
    BackCalculated,
    /// Not on the drawing: an assumption the engineer must confirm.
    Assumed,
}

/// A label for one default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefaultNote {
    pub field: &'static str,
    pub value: &'static str,
    pub source: DefaultSource,
    pub note: &'static str,
}

/// Every default that is not a plain dimension of the drawing, each with its source.
pub fn default_notes() -> Vec<DefaultNote> {
    use DefaultSource::*;
    let n = |field, value, source, note| DefaultNote {
        field,
        value,
        source,
        note,
    };
    vec![
        n(
            "thresholds.min_cover_mm",
            "600",
            Assumed,
            "not on the drawing; confirm with the specification",
        ),
        n(
            "thresholds.min_gradient_pct",
            "1.0",
            Drawing,
            "the gradient the drawing states (its inverts give 1.5 %)",
        ),
        n(
            "bedding.surround_over_crown_mm",
            "500",
            BackCalculated,
            "gives the drawing's 31.817 m3 of surround within 0.5 %",
        ),
        n(
            "bedding.working_space_mm",
            "300",
            Assumed,
            "excavation each side of the surround; not on the drawing",
        ),
        n(
            "end.headwall_extra_width_mm",
            "50",
            Drawing,
            "headwall 1850 against 1800 of surround",
        ),
        n(
            "end.riprap_length_mm",
            "3000",
            Assumed,
            "the drawing shows rip-rap but no length",
        ),
        n(
            "end.apron_top_below_invert_mm",
            "0",
            BackCalculated,
            "headwall 200 above the pipe crown puts the apron top at the invert",
        ),
        n(
            "end.weep.count_per_headwall",
            "2",
            BackCalculated,
            "0.8 m of weep holes in 200 mm headwalls is four holes",
        ),
        n(
            "road.embankment_slope_n",
            "1.77",
            BackCalculated,
            "the slope that gives the drawing's 17 000 mm barrel; its left/right split implies 1.2",
        ),
        n(
            "ogl",
            "1107.5 to 1107.9",
            BackCalculated,
            "the dashed ground line of the long section, read by eye",
        ),
        n(
            "concrete classes",
            "C15/20, C25/20, C30/20",
            Drawing,
            "the drawing notes; the payable table names C20 and C25",
        ),
        n(
            "rebar.rounding_mm",
            "10",
            BackCalculated,
            "every cutting length of the schedule is a multiple of 10",
        ),
        n(
            "rebar.cover_wall_mm",
            "40",
            BackCalculated,
            "toe bar 6673 = 6753 - 2 x 40",
        ),
        n(
            "rebar.cover_slab_mm",
            "50",
            BackCalculated,
            "apron bar legs of 100 in 200 mm slabs",
        ),
        n(
            "rebar.half_round_offset_mm",
            "14.6",
            BackCalculated,
            "half-round bar A = 2245 on a 1400 mm pipe",
        ),
        n(
            "pipe catalogue walls",
            "see pipe_catalogue()",
            Assumed,
            "only the 1200 mm wall of 100 is from the drawing",
        ),
    ]
}
