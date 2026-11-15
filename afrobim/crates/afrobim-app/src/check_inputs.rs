//! The inputs a check needs, kept on the element they were entered for.
//!
//! A check needs what only the engineer knows: the wind basis of a sign, a design flow, a speed
//! limit, traffic, the photometric results of a lighting calculation. The single-element CHECK
//! dialogs save what was entered as a **property of the element**, and the model-wide run
//! (`CHECKALL`) reads the same properties, so the whole model can be checked again with the same
//! inputs and a changed element shows its new result. Properties are saved with the document, so
//! the inputs survive save and open.
//!
//! The keys are documented here and are hidden from the generic property editor (every key that
//! starts with [`PREFIX`]). The value of each is JSON text of the type named beside it.
//!
//! | Key | On | Value |
//! |---|---|---|
//! | `check.support_design` | a sign | `supports_glue::StoredSupportDesign`: the DESIGN dialog's entries |
//! | `check.drain` | a side drain | [`DrainInputs`]: design flow (entered or from the rational method), basis, freeboard |
//! | `check.culvert` | a pipe culvert | [`CulvertInputs`]: flow, Manning's n, entrance loss, tailwater, basis, road level |
//! | `check.rational` | a drain or a culvert | [`RationalInputs`]: the catchment of the rational method (MoWT parts, or the SANRAL build-up) |
//! | `check.restraint_speed` | a barrier run | `road_glue::SpeedInput`: the speed limit and its unit |
//! | `check.lighting_inputs` | a lighting element | `lighting_inputs::Form`: every entry of the lighting check dialog |
//! | `check.pavement_design` | a pavement | `road_glue::DesignForm`: the method and its inputs |
//!
//! Nothing here holds an engineering value. A text field that is empty is "not entered", and the
//! parsers say which field is missing.

use afrobim_drainage::design::mowt::{self, RationalInput, RationalResult, SubCatchment};
use afrobim_drainage::design::{inlet::InletEdge, sanral, DesignBasis};
use afrobim_kernel::{Document, ElementId};
use serde::{Deserialize, Serialize};

pub const PREFIX: &str = "check.";
pub const KEY_SUPPORT: &str = "check.support_design";
pub const KEY_DRAIN: &str = "check.drain";
pub const KEY_CULVERT: &str = "check.culvert";
pub const KEY_RATIONAL: &str = "check.rational";
pub const KEY_RESTRAINT: &str = "check.restraint_speed";
pub const KEY_LIGHTING: &str = "check.lighting_inputs";
pub const KEY_PAVEMENT: &str = "check.pavement_design";

/// True for a property key this module owns (hidden from the generic editor).
pub fn is_check_key(key: &str) -> bool {
    key.starts_with(PREFIX)
}

/// The inputs stored under `key` on an element, when there are some and they can be read.
pub fn load<T: serde::de::DeserializeOwned>(doc: &Document, id: ElementId, key: &str) -> Option<T> {
    doc.find(id)?.properties.get(key).and_then(|text| serde_json::from_str(text).ok())
}

/// Stores inputs on an element. False when the element is gone.
pub fn save<T: Serialize>(doc: &mut Document, id: ElementId, key: &str, value: &T) -> bool {
    match (doc.find_mut(id), serde_json::to_string(value)) {
        (Some(e), Ok(text)) => {
            e.properties.insert(key.to_string(), text);
            true
        }
        _ => false,
    }
}

// ─── parsing text entries ───────────────────────────────────────────────────────────────

/// A number typed by the user. Empty is `Ok(None)`: not entered.
pub fn number(label: &str, text: &str) -> Result<Option<f64>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite()).map(Some).ok_or_else(|| format!("{label}: '{t}' is not a number"))
}

/// A number the check cannot go without.
pub fn required(label: &str, text: &str) -> Result<f64, String> {
    number(label, text)?.ok_or_else(|| format!("enter {label}"))
}

/// A number above zero the check cannot go without.
pub fn required_positive(label: &str, text: &str) -> Result<f64, String> {
    let v = required(label, text)?;
    if v > 0.0 {
        Ok(v)
    } else {
        Err(format!("{label} must be above zero (it is {v})"))
    }
}

// ─── the rational method ────────────────────────────────────────────────────────────────

/// One part of the catchment as typed: what it is, its area in hectares and its runoff coefficient.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CatchmentRow {
    pub description: String,
    pub area_ha: String,
    pub runoff_coefficient: String,
}

/// The entries of the rational method dialog, as typed. Every field starts empty.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RationalInputs {
    /// The method: false the MoWT rational method of the parts below, true the SANRAL method.
    /// Documents saved before SANRAL was offered have no such entry and read as MoWT.
    #[serde(default)]
    pub use_sanral: bool,
    /// The SANRAL entries, kept whichever method is shown.
    #[serde(default)]
    pub sanral: SanralRational,
    pub rows: Vec<CatchmentRow>,
    /// Design rainfall intensity for a duration equal to the time of concentration, mm/h.
    pub intensity_mm_h: String,
    /// Where the intensity comes from, quoted in the calculation.
    pub intensity_source: String,
    /// Multiplier for a higher recurrence interval; empty means none is applied.
    pub frequency_multiplier: String,
    /// Storage factor Cs of the modified rational method; empty means the basic method.
    pub storage_factor: String,
}

impl RationalInputs {
    /// The engine's input, or the first entry that is missing or cannot be read.
    pub fn input(&self) -> Result<RationalInput, String> {
        let mut parts = Vec::new();
        for (i, r) in self.rows.iter().enumerate() {
            let n = i + 1;
            if r.description.trim().is_empty() && r.area_ha.trim().is_empty() && r.runoff_coefficient.trim().is_empty() {
                continue;
            }
            parts.push(SubCatchment {
                description: if r.description.trim().is_empty() { format!("Area {n}") } else { r.description.trim().to_string() },
                area_ha: required(&format!("the area of part {n} (ha)"), &r.area_ha)?,
                runoff_coefficient: required(&format!("the runoff coefficient of part {n}"), &r.runoff_coefficient)?,
            });
        }
        if parts.is_empty() {
            return Err("enter at least one part of the catchment".into());
        }
        Ok(RationalInput {
            sub_catchments: parts,
            intensity_mm_h: required("the design rainfall intensity (mm/h)", &self.intensity_mm_h)?,
            intensity_source: if self.intensity_source.trim().is_empty() { "entered, source not stated".to_string() } else { self.intensity_source.trim().to_string() },
            // An empty entry is "no multiplier", the plain method: a stated absence, not a value.
            frequency_multiplier: number("the recurrence multiplier", &self.frequency_multiplier)?.unwrap_or(1.0),
            storage_factor: number("the storage factor", &self.storage_factor)?,
        })
    }

    /// The MoWT rational method on the entries.
    pub fn run(&self) -> Result<RationalResult, String> {
        mowt::rational_method(&self.input()?).map_err(|e| e.to_string())
    }

    /// The design flow and the calculation lines of whichever method is chosen.
    pub fn run_flow(&self) -> Result<(f64, Vec<String>), String> {
        if self.use_sanral {
            return self.sanral.run();
        }
        let r = self.run()?;
        Ok((r.peak_flow_m3_s, r.calculation()))
    }
}

// ─── side drain ─────────────────────────────────────────────────────────────────────────

/// The design basis stored with an entry. Older documents stored a yes/no `dmrb` (false: MoWT,
/// true: UK DMRB) and no `basis`; they load unchanged. The basis a document is saved with is
/// written to both, so an older build still reads MoWT or DMRB; SANRAL is read as MoWT there.
fn basis_of(basis: Option<DesignBasis>, dmrb: bool) -> DesignBasis {
    basis.unwrap_or(if dmrb { DesignBasis::Dmrb } else { DesignBasis::Mowt })
}

/// The entries of the side drain check, as typed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DrainInputs {
    /// Design flow, m³/s, when it is entered directly.
    pub flow_m3_s: String,
    /// Take the design flow from the rational method inputs kept on the same element.
    pub flow_from_rational: bool,
    /// Old stored form of the basis: UK DMRB instead of the MoWT manual. Read when `basis` is absent.
    pub dmrb: bool,
    /// The design basis (MoWT, UK DMRB or SANRAL). Absent in documents saved before SANRAL was offered.
    #[serde(default)]
    pub basis: Option<DesignBasis>,
    /// Required freeboard, m, if the project states one. No basis states one for a drain.
    pub required_freeboard_m: String,
}

/// What a drain check runs with.
#[derive(Debug, Clone, PartialEq)]
pub struct DrainParams {
    pub flow_m3_s: f64,
    pub basis: DesignBasis,
    pub freeboard_m: Option<f64>,
    /// The rational method's lines when the flow came from it.
    pub rational_lines: Vec<String>,
}

impl DrainInputs {
    pub fn basis(&self) -> DesignBasis {
        basis_of(self.basis, self.dmrb)
    }

    pub fn set_basis(&mut self, basis: DesignBasis) {
        self.basis = Some(basis);
        self.dmrb = basis == DesignBasis::Dmrb;
    }

    pub fn params(&self, rational: Option<&RationalInputs>) -> Result<DrainParams, String> {
        let (flow, lines) = flow_of(self.flow_from_rational, &self.flow_m3_s, rational)?;
        Ok(DrainParams { flow_m3_s: flow, basis: self.basis(), freeboard_m: number("the required freeboard (m)", &self.required_freeboard_m)?, rational_lines: lines })
    }
}

/// The design flow: from the rational method when asked, else as typed.
fn flow_of(from_rational: bool, typed: &str, rational: Option<&RationalInputs>) -> Result<(f64, Vec<String>), String> {
    if from_rational {
        let r = rational.ok_or("the design flow is to come from the rational method, which has no inputs on this element: open RATIONAL")?;
        return r.run_flow();
    }
    Ok((required_positive("the design flow (m3/s)", typed)?, Vec::new()))
}

// ─── pipe culvert ───────────────────────────────────────────────────────────────────────

/// The entries of the culvert checks, as typed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CulvertInputs {
    pub flow_m3_s: String,
    pub flow_from_rational: bool,
    /// Manning's n of the barrel.
    pub manning_n: String,
    /// Entrance loss coefficient Ke, and where it comes from (the DDM Appendix 9.6 row, or another source).
    pub entrance_ke: String,
    pub entrance_note: String,
    /// Tailwater depth above the outlet invert, m.
    pub tailwater_m: String,
    /// Old stored form of the basis (UK DMRB layout limits instead of MoWT). Read when `basis` is absent.
    pub dmrb: bool,
    /// The design basis: layout limits for every basis; the complete hydraulic check for MoWT
    /// (inlet and outlet control) and for SANRAL (SDM Table 7.2 and Eq. 7.4); none for DMRB.
    #[serde(default)]
    pub basis: Option<DesignBasis>,
    /// MoWT complete check: the inlet edge (an index into [`circular_inlets`]). Empty at the start.
    #[serde(default)]
    pub inlet_edge: Option<usize>,
    /// MoWT complete check: the allowable headwater above the inlet invert, m. The engineer's.
    #[serde(default)]
    pub allowable_headwater_m: String,
    /// SANRAL culvert check entries.
    #[serde(default)]
    pub sanral: SanralCulvertEntries,
    /// Level of the road shoulder break point at the low point of the road, m, for the freeboard.
    pub shoulder_level_m: String,
    /// The road is a major road (true) or a minor road (false), to pick the freeboard of DDM 9.6.
    pub major_road: Option<bool>,
}

/// What the outlet control calculation runs with.
#[derive(Debug, Clone, PartialEq)]
pub struct CulvertParams {
    pub flow_m3_s: f64,
    pub manning_n: f64,
    pub entrance_ke: f64,
    pub tailwater_m: f64,
    pub shoulder_level_m: Option<f64>,
    pub major_road: Option<bool>,
    pub rational_lines: Vec<String>,
}

impl CulvertInputs {
    pub fn basis(&self) -> DesignBasis {
        basis_of(self.basis, self.dmrb)
    }

    pub fn set_basis(&mut self, basis: DesignBasis) {
        self.basis = Some(basis);
        self.dmrb = basis == DesignBasis::Dmrb;
    }

    /// The design flow and the rational method's lines when it came from there. The SANRAL check
    /// needs only this of the MoWT entries.
    pub fn flow(&self, rational: Option<&RationalInputs>) -> Result<(f64, Vec<String>), String> {
        flow_of(self.flow_from_rational, &self.flow_m3_s, rational)
    }

    /// The allowable headwater, when entered (the complete MoWT check needs it).
    pub fn allowable_headwater(&self) -> Result<Option<f64>, String> {
        let v = number("the allowable headwater (m)", &self.allowable_headwater_m)?;
        match v {
            Some(x) if x <= 0.0 => Err(format!("the allowable headwater must be above zero (it is {x})")),
            other => Ok(other),
        }
    }

    pub fn params(&self, rational: Option<&RationalInputs>) -> Result<CulvertParams, String> {
        let (flow, lines) = flow_of(self.flow_from_rational, &self.flow_m3_s, rational)?;
        let ke = required("the entrance loss coefficient Ke", &self.entrance_ke)?;
        if ke < 0.0 {
            return Err(format!("the entrance loss coefficient Ke must not be negative (it is {ke})"));
        }
        let tailwater = required("the tailwater depth (m)", &self.tailwater_m)?;
        if tailwater < 0.0 {
            return Err(format!("the tailwater depth must not be negative (it is {tailwater})"));
        }
        let shoulder_level_m = number("the shoulder break level (m)", &self.shoulder_level_m)?;
        if shoulder_level_m.is_some() && self.major_road.is_none() {
            return Err("say whether the road is a major or a minor road, for the freeboard".into());
        }
        Ok(CulvertParams {
            flow_m3_s: flow,
            manning_n: required_positive("Manning's n of the barrel", &self.manning_n)?,
            entrance_ke: ke,
            tailwater_m: tailwater,
            shoulder_level_m,
            major_road: self.major_road,
            rational_lines: lines,
        })
    }
}

// ─── the complete culvert check, MoWT ───────────────────────────────────────────────────

/// The inlet rows of HDS-5 Table A.1 for a circular barrel, which is what a pipe culvert is.
pub fn circular_inlets() -> Vec<InletEdge> {
    use afrobim_drainage::design::inlet::InletShape;
    InletEdge::ALL.iter().copied().filter(|e| e.coefficients().shape == InletShape::Circular).collect()
}

// ─── SANRAL: culvert ────────────────────────────────────────────────────────────────────

/// The entries of the SANRAL culvert check beyond what the culvert element holds (barrels,
/// diameter, length and fall come from the element). Every field starts empty; the slope unit is
/// a question the manual does not answer, so it has no default.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SanralCulvertEntries {
    /// How the bed slope is read in SDM Table 7.2: 0 metre per metre, 1 per cent. None: not answered.
    pub slope_unit: Option<u8>,
    pub k_in: String,
    pub k_out: String,
    /// 0 Manning's n, 1 Colebrook-White roughness ks (round barrels).
    pub friction_mode: u8,
    pub friction_value: String,
    /// Downstream energy level above the outlet invert for the design flood, m.
    pub h2_m: String,
    /// Height of the shoulder break point above the inlet invert, m.
    pub shoulder_break_height_m: String,
    /// Index into [`ROAD_CLASSES`].
    pub road_class: Option<usize>,
    /// Flow and H2 at twice the return period, optional.
    pub twice_flow_m3_s: String,
    pub twice_h2_m: String,
}

pub const ROAD_CLASSES: [sanral::RoadClass; 5] = [sanral::RoadClass::Class1, sanral::RoadClass::Class2, sanral::RoadClass::Class3, sanral::RoadClass::Class4Or5, sanral::RoadClass::Class6];

/// The note that goes with the slope unit question.
pub const SLOPE_UNIT_NOTE: &str = "The slope in SDM Table 7.2 is printed as (S0 / 0,4) with no unit: in metres per metre 0,4 would be a 40 % slope, in per cent it is 0,4 %. The manual does not say which, and the factor differs by about 20 % for a 0,15 % slope. The program does not choose; the result repeats your answer.";

impl SanralCulvertEntries {
    /// The engine's input for a culvert of this size and flow, or the first thing missing.
    pub fn input(&self, def: &afrobim_drainage::PipeCulvertDefinition, flow_m3_s: f64) -> Result<sanral::SanralCulvertInput, String> {
        let slope_unit = match self.slope_unit {
            Some(0) => sanral::SlopeUnit::MetrePerMetre,
            Some(_) => sanral::SlopeUnit::Percent,
            None => return Err("answer how the slope is read in SDM Table 7.2 (metre per metre or per cent): the manual does not say".into()),
        };
        let friction_value = required_positive(if self.friction_mode == 0 { "Manning's n of the barrel" } else { "the roughness ks (m)" }, &self.friction_value)?;
        let twice = match (number("the flow at twice the return period", &self.twice_flow_m3_s)?, number("H2 at twice the return period (m)", &self.twice_h2_m)?) {
            (Some(q), Some(h2)) => Some(sanral::CulvertFlood { flow_m3_s: q, h2_m: h2 }),
            (None, None) => None,
            _ => return Err("enter both the flow and H2 at twice the return period, or neither".into()),
        };
        Ok(sanral::SanralCulvertInput {
            barrel: sanral::SdmBarrel::Round { diameter_m: def.internal_diameter_mm / 1000.0 },
            barrels: def.barrels,
            length_m: def.length_mm / 1000.0,
            slope_m_per_m: def.fall_m() / (def.length_mm / 1000.0),
            slope_unit,
            k_in: required("the inlet loss coefficient Kin", &self.k_in)?,
            k_out: required("the outlet loss coefficient Kout", &self.k_out)?,
            friction: if self.friction_mode == 0 { sanral::BarrelFriction::Manning { n: friction_value } } else { sanral::BarrelFriction::Colebrook { ks_m: friction_value } },
            design: sanral::CulvertFlood { flow_m3_s, h2_m: required("the downstream energy level H2 (m)", &self.h2_m)? },
            twice,
            shoulder_break_height_m: number("the shoulder break height (m)", &self.shoulder_break_height_m)?,
            road_class: ROAD_CLASSES[self.road_class.ok_or("choose the road class (SDM Table 7.1)")?.min(4)],
        })
    }
}

// ─── SANRAL: rational method ────────────────────────────────────────────────────────────

/// One urban land use of the SANRAL run-off coefficient, as typed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct UrbanRow {
    /// Index into [`sanral::UrbanUse::ALL`].
    pub land_use: Option<usize>,
    pub percent: String,
    /// The factor chosen within the printed range of the land use.
    pub factor: String,
}

/// One reach of the flow path, as typed. `kind`: 0 overland (Kerby), 1 defined channel, 2 urban.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ReachRow {
    pub kind: u8,
    /// Overland: r. Channel: unused. Urban: unused.
    pub r: String,
    /// Length: km for overland and channel, m for urban.
    pub length: String,
    /// Slope m/m (overland, channel) or velocity m/s (urban).
    pub slope_or_velocity: String,
}

pub const RURAL_SLOPES: [(sanral::RuralSlope, &str); 4] = [(sanral::RuralSlope::VleisAndPans, "Vleis and pans (< 3 %)"), (sanral::RuralSlope::Flat, "Flat areas (3 to 10 %)"), (sanral::RuralSlope::Hilly, "Hilly (10 to 30 %)"), (sanral::RuralSlope::Steep, "Steep areas (> 30 %)")];
pub const RURAL_PERMEABILITY: [(sanral::Permeability, &str); 4] = [(sanral::Permeability::VeryPermeable, "Very permeable"), (sanral::Permeability::Permeable, "Permeable"), (sanral::Permeability::SemiPermeable, "Semi-permeable"), (sanral::Permeability::Impermeable, "Impermeable")];
pub const RURAL_VEGETATION: [(sanral::Vegetation, &str); 4] = [(sanral::Vegetation::ThickBushAndPlantation, "Thick bush and plantation"), (sanral::Vegetation::LightBushAndFarmLands, "Light bush and farm lands"), (sanral::Vegetation::Grasslands, "Grasslands"), (sanral::Vegetation::NoVegetation, "No vegetation")];

/// The SANRAL rational method entries (SDM section 3.5.1): run-off coefficient from the Table 3.7
/// classes, time of concentration by reach, and the point rainfall depth, which is always entered.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SanralRational {
    pub area_km2: String,
    pub map_mm: String,
    /// Shares of the rural area, percent, by class in the order of [`RURAL_SLOPES`] and so on. An
    /// empty share is a class that is not present; each group must add up to 100.
    pub slope: [String; 4],
    pub permeability: [String; 4],
    pub vegetation: [String; 4],
    pub dolomite_percent: String,
    /// 0 steep and impermeable, 1 flat and permeable (SDM Table 3.8).
    pub ft_class: Option<u8>,
    pub urban: Vec<UrbanRow>,
    pub lakes_c3: String,
    /// Area distribution factors: rural, urban, lakes. They must add up to 1.
    pub alpha: String,
    pub beta: String,
    pub gamma: String,
    /// The return period in years (2, 5, 10, 20, 50 or 100) or the word `maximum` for the probable maximum flood.
    pub return_period: String,
    pub reaches: Vec<ReachRow>,
    pub point_rainfall_mm: String,
    pub rainfall_source: String,
    /// 0 SDM Eq. 3.10, 1 a value entered (percent), 2 not applied.
    pub area_reduction: u8,
    pub area_reduction_percent: String,
}

fn shares<T: Copy>(label: &str, classes: &[(T, &str)], values: &[String; 4]) -> Result<Vec<(T, f64)>, String> {
    let mut out = Vec::new();
    for ((class, name), text) in classes.iter().zip(values) {
        if let Some(v) = number(&format!("the {label} share of {name} (%)"), text)? {
            out.push((*class, v));
        }
    }
    Ok(out)
}

impl SanralRational {
    pub fn input(&self) -> Result<(sanral::RationalInput, sanral::TimeOfConcentration), String> {
        let urban_total_given = !self.urban.is_empty();
        let rural = if shares("slope", &RURAL_SLOPES, &self.slope)?.is_empty() && shares("permeability", &RURAL_PERMEABILITY, &self.permeability)?.is_empty() && shares("vegetation", &RURAL_VEGETATION, &self.vegetation)?.is_empty() {
            None
        } else {
            Some(sanral::RuralCatchment {
                map_mm: required_positive("the mean annual precipitation (mm)", &self.map_mm)?,
                slope: shares("slope", &RURAL_SLOPES, &self.slope)?,
                permeability: shares("permeability", &RURAL_PERMEABILITY, &self.permeability)?,
                vegetation: shares("vegetation", &RURAL_VEGETATION, &self.vegetation)?,
                dolomite_fraction: required("the dolomitic share of the catchment (%, enter 0 if none)", &self.dolomite_percent)? / 100.0,
                ft_class: match self.ft_class {
                    Some(0) => sanral::FtClass::SteepAndImpermeable,
                    Some(_) => sanral::FtClass::FlatAndPermeable,
                    None => return Err("choose the catchment kind for the return period factor Ft (SDM Table 3.8)".into()),
                },
            })
        };
        let mut urban = Vec::new();
        for (i, u) in self.urban.iter().enumerate() {
            let n = i + 1;
            urban.push(sanral::UrbanPart {
                land_use: sanral::UrbanUse::ALL[u.land_use.ok_or_else(|| format!("choose the land use of urban part {n}"))?.min(11)],
                percent: required(&format!("the share of urban part {n} (%)"), &u.percent)?,
                factor: required(&format!("the run-off factor of urban part {n}"), &u.factor)?,
            });
        }
        let _ = urban_total_given;
        let return_period = if self.return_period.trim().eq_ignore_ascii_case("maximum") {
            sanral::ReturnPeriod::MaximumFlood
        } else {
            sanral::ReturnPeriod::Years(required("the return period (years, or the word maximum)", &self.return_period)?.round() as u32)
        };
        let runoff = sanral::RunoffInput {
            rural,
            urban,
            lakes_c3: number("the run-off coefficient of lakes C3", &self.lakes_c3)?.unwrap_or(0.0),
            alpha: required("the rural area factor alpha", &self.alpha)?,
            beta: required("the urban area factor beta", &self.beta)?,
            gamma: required("the lakes area factor gamma", &self.gamma)?,
            return_period,
        };
        let mut reaches = Vec::new();
        for (i, r) in self.reaches.iter().enumerate() {
            let n = i + 1;
            reaches.push(match r.kind {
                0 => sanral::TcReach::Overland { r: required(&format!("r of reach {n}"), &r.r)?, length_km: required(&format!("the length of reach {n} (km)"), &r.length)?, slope_m_per_m: required(&format!("the slope of reach {n} (m/m)"), &r.slope_or_velocity)? },
                1 => sanral::TcReach::Channel { length_km: required(&format!("the length of reach {n} (km)"), &r.length)?, slope_m_per_m: required(&format!("the slope of reach {n} (m/m)"), &r.slope_or_velocity)? },
                _ => sanral::TcReach::Urban { length_m: required(&format!("the length of reach {n} (m)"), &r.length)?, velocity_m_s: required(&format!("the velocity of reach {n} (m/s)"), &r.slope_or_velocity)? },
            });
        }
        let tc = sanral::time_of_concentration(&reaches).map_err(|e| e.to_string())?;
        let area_reduction = match self.area_reduction {
            0 => sanral::AreaReduction::Equation,
            1 => sanral::AreaReduction::Entered(required("the area reduction factor (%)", &self.area_reduction_percent)?),
            _ => sanral::AreaReduction::NotApplied,
        };
        let input = sanral::RationalInput {
            runoff,
            area_km2: required_positive("the catchment area (km2)", &self.area_km2)?,
            time_of_concentration_h: tc.total_h,
            rainfall: sanral::RainfallInput {
                point_rainfall_mm: required_positive("the point rainfall depth (mm)", &self.point_rainfall_mm)?,
                source: if self.rainfall_source.trim().is_empty() { "entered, source not stated".to_string() } else { self.rainfall_source.trim().to_string() },
                area_reduction,
            },
        };
        Ok((input, tc))
    }

    /// The flow and the lines of the time of concentration and the rational method.
    pub fn run(&self) -> Result<(f64, Vec<String>), String> {
        let (input, tc) = self.input()?;
        let result = sanral::rational_method(&input).map_err(|e| e.to_string())?;
        let mut lines = tc.lines.clone();
        lines.extend(result.calculation());
        Ok((result.peak_flow_m3_s, lines))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_kernel::Pt;

    #[test]
    fn an_empty_entry_is_not_entered_and_the_parsers_say_which() {
        assert_eq!(number("x", "  ").unwrap(), None);
        assert_eq!(number("x", "2.5").unwrap(), Some(2.5));
        assert!(number("the flow", "abc").unwrap_err().contains("the flow"));
        assert!(required("the flow", "").unwrap_err().contains("enter the flow"));
        assert!(required_positive("the flow", "0").is_err());
        assert!(DrainInputs::default().params(None).unwrap_err().contains("design flow"));
        let culvert = CulvertInputs { flow_m3_s: "1".into(), ..Default::default() };
        assert!(culvert.params(None).unwrap_err().contains("Ke"));
    }

    #[test]
    fn inputs_round_trip_on_an_element_and_through_save_and_open() {
        let mut doc = Document::default();
        let id = doc.add("drain", "SD-1", Pt::new(0.0, 0.0), 10.0, 10.0, "Drains", serde_json::Value::Null);
        let inputs = DrainInputs { flow_m3_s: "0.35".into(), flow_from_rational: false, dmrb: true, basis: None, required_freeboard_m: "0.1".into() };
        assert!(load::<DrainInputs>(&doc, id, KEY_DRAIN).is_none());
        assert!(save(&mut doc, id, KEY_DRAIN, &inputs));
        let reopened = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(load::<DrainInputs>(&reopened, id, KEY_DRAIN), Some(inputs));
        assert!(is_check_key(KEY_DRAIN) && !is_check_key("Material"));
    }

    #[test]
    fn the_rational_method_runs_on_entered_text() {
        let mut r = RationalInputs::default();
        assert!(r.run().unwrap_err().contains("at least one part"));
        // Arbitrary test values: 10 ha at C 0.5, 50 mm/h. Q = C i A / 360 = 0.5 * 50 * 10 / 360.
        r.rows.push(CatchmentRow { description: "test".into(), area_ha: "10".into(), runoff_coefficient: "0.5".into() });
        assert!(r.run().unwrap_err().contains("intensity"));
        r.intensity_mm_h = "50".into();
        let result = r.run().unwrap();
        assert!((result.peak_flow_m3_s - 0.5 * 50.0 * 10.0 / 360.0).abs() < 1e-12);
        // A drain takes its flow from the rational inputs when asked.
        let drain = DrainInputs { flow_from_rational: true, ..Default::default() };
        let p = drain.params(Some(&r)).unwrap();
        assert!((p.flow_m3_s - result.peak_flow_m3_s).abs() < 1e-12 && !p.rational_lines.is_empty());
        assert!(drain.params(None).unwrap_err().contains("RATIONAL"));
    }
}
