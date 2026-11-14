//! Street lighting, road restraint, pavement and corridor in the app: what the "add" dialogs
//! collect, how that becomes a definition in the crate, the checks, the pavement design
//! methods, the corridor's 3D surfaces and its cross-sections.
//!
//! None of these crates offers a placeholder definition, and this file adds none: every
//! dimension, class and slope is entered by the engineer in a dialog and starts at zero
//! ("not entered") where a number is needed. The crates refuse what they cannot use, with a
//! plain reason. Every value in the crates comes with its citation in the crate's own README.

use afrobim_alignment::Alignment;
use afrobim_corridor as corridor;
use afrobim_draw::Mesh3;
use afrobim_kernel::{Document, ElementId};
use afrobim_lighting as lighting;
use afrobim_pavement as pavement;
use afrobim_restraint as restraint;

use crate::alignment_glue;

fn alignment(doc: &Document, id: ElementId) -> Result<Alignment, String> {
    alignment_glue::alignment_of(doc, id).ok_or_else(|| "That alignment cannot be read.".to_string())
}

fn hide_layer(doc: &mut Document, name: &str) {
    if let Some(layer) = doc.layers_mut().iter_mut().find(|l| l.name == name) {
        layer.visible = false;
    }
}

/// The alignment element of the given name.
pub fn alignment_named(doc: &Document, name: &str) -> Option<Alignment> {
    alignment_glue::alignments(doc).into_iter().find(|(_, n)| n == name).and_then(|(id, _)| alignment_glue::alignment_of(doc, id))
}

// ─── street lighting ────────────────────────────────────────────────────────────────────

/// The column and luminaire the engineer enters. Nothing here has a default taken from a standard.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ColumnFields {
    pub mounting_height_m: f64,
    pub bracket_outreach_m: f64,
    pub tilt_deg: f64,
    pub product: String,
    pub lumen_output_lm: f64,
    pub power_w: f64,
}

impl ColumnFields {
    fn spec(&self) -> lighting::ColumnSpec {
        lighting::ColumnSpec {
            column_type: String::new(),
            mounting_height_m: self.mounting_height_m,
            bracket_outreach_m: self.bracket_outreach_m,
            tilt_deg: self.tilt_deg,
            luminaire: lighting::Luminaire {
                product: self.product.trim().to_string(),
                lumen_output_lm: self.lumen_output_lm,
                power_w: self.power_w,
                ..Default::default()
            },
        }
    }
}

pub const ARRANGEMENTS: [lighting::Arrangement; 5] = [
    lighting::Arrangement::SingleLeft,
    lighting::Arrangement::SingleRight,
    lighting::Arrangement::Staggered,
    lighting::Arrangement::Opposite,
    lighting::Arrangement::TwinCentral,
];

pub const END_TREATMENTS: [(lighting::EndTreatment, &str); 3] = [
    (lighting::EndTreatment::KeepSpacing, "Keep the spacing, the last bay is what is left"),
    (lighting::EndTreatment::ColumnAtEnd, "A column at the end station"),
    (lighting::EndTreatment::EqualiseSpacing, "Equal spacing, as near the entered spacing as it can be"),
];

/// What the "Street lighting" dialog collects: an installation along an alignment.
#[derive(Debug, Clone, PartialEq)]
pub struct LightingRequest {
    pub reference: String,
    pub start_station_m: f64,
    pub end_station_m: f64,
    pub arrangement: usize,
    pub spacing_m: f64,
    pub end_treatment: usize,
    pub first_side_right: bool,
    pub carriageway_width_m: f64,
    pub reference_offset_m: f64,
    pub setback_m: f64,
    pub column: ColumnFields,
    pub lighting_class: String,
}

impl LightingRequest {
    /// An empty request over a stretch of alignment. The numbers are zero: not entered.
    pub fn empty(reference: &str, start_station_m: f64, end_station_m: f64) -> LightingRequest {
        LightingRequest {
            reference: reference.to_string(),
            start_station_m,
            end_station_m,
            arrangement: 0,
            spacing_m: 0.0,
            end_treatment: 0,
            first_side_right: false,
            carriageway_width_m: 0.0,
            reference_offset_m: 0.0,
            setback_m: 0.0,
            column: ColumnFields::default(),
            lighting_class: String::new(),
        }
    }

    fn installation(&self, alignment: &Alignment) -> lighting::Installation {
        lighting::Installation {
            reference: self.reference.trim().to_string(),
            alignment: alignment.name.clone(),
            start_station_m: self.start_station_m,
            end_station_m: self.end_station_m,
            arrangement: ARRANGEMENTS[self.arrangement.min(ARRANGEMENTS.len() - 1)],
            spacing_m: self.spacing_m,
            end_treatment: END_TREATMENTS[self.end_treatment.min(END_TREATMENTS.len() - 1)].0,
            first_side: if self.first_side_right { lighting::Side::Right } else { lighting::Side::Left },
            carriageway_width_m: self.carriageway_width_m,
            reference_offset_m: self.reference_offset_m,
            setback_m: self.setback_m,
            column: self.column.spec(),
            lighting_class: self.lighting_class.trim().to_string(),
            mark_prefix: "LC".to_string(),
            first_number: 1,
        }
    }
}

/// What the "Light column" dialog collects: one column at a station and offset.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnRequest {
    pub mark: String,
    pub station_m: f64,
    pub distance_m: f64,
    pub right: bool,
    /// None: the bracket points square to the alignment, towards the carriageway.
    pub bearing_deg: Option<f64>,
    pub column: ColumnFields,
    pub lighting_class: String,
}

impl ColumnRequest {
    pub fn empty(mark: &str, station_m: f64) -> ColumnRequest {
        ColumnRequest { mark: mark.to_string(), station_m, distance_m: 0.0, right: false, bearing_deg: None, column: ColumnFields::default(), lighting_class: String::new() }
    }
}

/// Lays an installation out on the alignment and adds it. Returns its mark.
pub fn add_lighting(doc: &mut Document, alignment_id: ElementId, req: &LightingRequest) -> Result<String, String> {
    let a = alignment(doc, alignment_id)?;
    let id = lighting::add_installation(doc, &a, req.installation(&a)).map_err(|e| e.to_string())?;
    hide_layer(doc, lighting::LAYER);
    Ok(doc.find(id).map(|e| e.name.clone()).unwrap_or_default())
}

/// Adds a single column. Returns its mark.
pub fn add_light_column(doc: &mut Document, alignment_id: ElementId, req: &ColumnRequest) -> Result<String, String> {
    let a = alignment(doc, alignment_id)?;
    let offset = if req.right { req.distance_m.abs() } else { -req.distance_m.abs() };
    let def = lighting::SingleColumn {
        mark: req.mark.trim().to_string(),
        alignment: a.name.clone(),
        station_m: req.station_m,
        offset_m: offset,
        aim: req.bearing_deg.map(lighting::BracketAim::Bearing).unwrap_or_default(),
        column: req.column.spec(),
        lighting_class: req.lighting_class.trim().to_string(),
    };
    let id = lighting::add_column(doc, &a, def).map_err(|e| e.to_string())?;
    hide_layer(doc, lighting::LAYER);
    Ok(doc.find(id).map(|e| e.name.clone()).unwrap_or_default())
}

/// The requirement checks of a lighting element, line by line, with the inputs the engineer
/// entered in the lighting check dialog. An input left empty is "not entered", so the report says
/// which ones it needs.
pub fn check_lighting_with(doc: &Document, id: ElementId, inputs: &lighting::design::CheckInputs) -> Result<Vec<String>, String> {
    let data = lighting::lighting_of(doc, id).map_err(|e| e.to_string())?;
    Ok(lighting::design::check_lighting(&data, inputs).calculation())
}

// ─── road restraint ─────────────────────────────────────────────────────────────────────

pub const FACES: [(restraint::TrafficFace, &str); 3] = [(restraint::TrafficFace::Left, "Left"), (restraint::TrafficFace::Right, "Right"), (restraint::TrafficFace::Both, "Both")];

pub fn terminal_classes() -> Vec<restraint::TerminalClass> {
    use restraint::TerminalClass::*;
    vec![P1, P2, P3, P4, T80_3, T110, T110_1]
}

/// What the "Road restraint" dialog collects: a safety barrier run.
#[derive(Debug, Clone, PartialEq)]
pub struct RestraintRequest {
    pub name: String,
    pub start_station_m: f64,
    pub end_station_m: f64,
    pub distance_m: f64,
    pub right: bool,
    pub face: usize,
    pub containment: usize,
    pub working_width: usize,
    pub impact_severity: usize,
    pub beam_height_m: f64,
    pub post_spacing_m: Option<f64>,
    pub set_back_m: Option<f64>,
    pub system: String,
    /// Index into [`terminal_classes`], None when the end is not stated.
    pub start_terminal: Option<usize>,
    pub end_terminal: Option<usize>,
}

impl RestraintRequest {
    pub fn empty(name: &str, start_station_m: f64, end_station_m: f64) -> RestraintRequest {
        RestraintRequest {
            name: name.to_string(),
            start_station_m,
            end_station_m,
            distance_m: 0.0,
            right: true,
            face: 0,
            containment: 0,
            working_width: 0,
            impact_severity: 0,
            beam_height_m: 0.0,
            post_spacing_m: None,
            set_back_m: None,
            system: String::new(),
            start_terminal: None,
            end_terminal: None,
        }
    }

    fn run(&self) -> restraint::BarrierRun {
        let terminals = terminal_classes();
        let end = |t: Option<usize>| t.and_then(|i| terminals.get(i).cloned()).map(restraint::EndTreatment::terminal).unwrap_or_default();
        restraint::BarrierRun {
            name: self.name.trim().to_string(),
            alignment: String::new(),
            start_station_m: self.start_station_m,
            end_station_m: self.end_station_m,
            offset_m: if self.right { self.distance_m.abs() } else { -self.distance_m.abs() },
            traffic_face: FACES[self.face.min(FACES.len() - 1)].0,
            containment: restraint::ContainmentLevel::ALL[self.containment.min(restraint::ContainmentLevel::ALL.len() - 1)],
            working_width: restraint::WorkingWidthClass::ALL[self.working_width.min(restraint::WorkingWidthClass::ALL.len() - 1)],
            impact_severity: [restraint::ImpactSeverityLevel::A, restraint::ImpactSeverityLevel::B, restraint::ImpactSeverityLevel::C][self.impact_severity.min(2)],
            vehicle_intrusion: None,
            system: self.system.trim().to_string(),
            post_spacing_m: self.post_spacing_m,
            beam_height_m: self.beam_height_m,
            start_end: end(self.start_terminal),
            end_end: end(self.end_terminal),
            set_back_m: self.set_back_m,
        }
    }
}

pub fn add_restraint(doc: &mut Document, alignment_id: ElementId, req: &RestraintRequest) -> Result<String, String> {
    let a = alignment(doc, alignment_id)?;
    let id = restraint::add_barrier_run(doc, &a, req.run()).map_err(|e| e.to_string())?;
    Ok(doc.find(id).map(|e| e.name.clone()).unwrap_or_default())
}

/// The speed limit the restraint check is made for.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SpeedInput {
    pub value: f64,
    pub mph: bool,
}

/// The requirement checks of a barrier run (DMRB CD 377), which need a speed limit.
pub fn check_restraint(doc: &Document, id: ElementId, speed: SpeedInput) -> Result<Vec<String>, String> {
    check_restraint_design(doc, id, speed).map(|d| d.calculation())
}

/// The same checks with every line's status, for the model-wide report.
pub fn check_restraint_design(doc: &Document, id: ElementId, speed: SpeedInput) -> Result<restraint::design::RestraintDesign, String> {
    if !(speed.value.is_finite() && speed.value > 0.0) {
        return Err("Enter the speed limit.".into());
    }
    let b = restraint::barrier_run_of(doc, id).map_err(|e| e.to_string())?;
    let limit = if speed.mph { restraint::design::SpeedLimit::Mph(speed.value) } else { restraint::design::SpeedLimit::Kmh(speed.value) };
    let input = restraint::design::DesignInput::new(limit);
    Ok(restraint::design::check_run(&b.run, &b.resolved, &input))
}

// ─── pavement ───────────────────────────────────────────────────────────────────────────

/// The layer materials the pavement crate names (its free-text "other" is edited in Properties).
pub fn materials() -> Vec<pavement::Material> {
    use pavement::Material::*;
    vec![SurfaceDressing, HotMixAsphalt, GranularBase, CementedBase, BituminousBase, GranularSubbase, CementedSubbase, CementedUpperSubbase, SelectedLayer]
}

/// One layer row of a dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerRow {
    pub name: String,
    /// Index into [`materials`].
    pub material: usize,
    pub thickness_mm: f64,
}

impl LayerRow {
    pub fn empty() -> LayerRow {
        LayerRow { name: String::new(), material: 0, thickness_mm: 0.0 }
    }

    pub fn from_layers(layers: &[pavement::Layer]) -> Vec<LayerRow> {
        let list = materials();
        layers.iter().map(|l| LayerRow { name: l.name.clone(), material: list.iter().position(|m| *m == l.material).unwrap_or(0), thickness_mm: l.thickness_mm }).collect()
    }

    fn layer(&self) -> pavement::Layer {
        let list = materials();
        pavement::Layer::new(self.name.trim(), list[self.material.min(list.len() - 1)].clone(), self.thickness_mm)
    }
}

/// A shoulder the engineer enters.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShoulderRequest {
    pub width_m: f64,
    pub crossfall_percent: f64,
}

/// What the "Pavement" dialog collects.
#[derive(Debug, Clone, PartialEq)]
pub struct PavementRequest {
    pub name: String,
    pub start_station_m: f64,
    pub end_station_m: f64,
    pub carriageway_width_m: f64,
    pub crossfall_percent: f64,
    /// 0 crowned, 1 falling to the left, 2 falling to the right.
    pub shape: usize,
    pub layers: Vec<LayerRow>,
    pub left_shoulder: Option<ShoulderRequest>,
    pub right_shoulder: Option<ShoulderRequest>,
}

impl PavementRequest {
    pub fn empty(name: &str, start_station_m: f64, end_station_m: f64) -> PavementRequest {
        PavementRequest {
            name: name.to_string(),
            start_station_m,
            end_station_m,
            carriageway_width_m: 0.0,
            crossfall_percent: 0.0,
            shape: 0,
            layers: Vec::new(),
            left_shoulder: None,
            right_shoulder: None,
        }
    }

    fn definition(&self, alignment: Alignment) -> pavement::PavementDefinition {
        let layers: Vec<pavement::Layer> = self.layers.iter().map(LayerRow::layer).collect();
        let mut def = pavement::PavementDefinition::new(alignment, self.carriageway_width_m, self.crossfall_percent, layers);
        def.start_station_m = self.start_station_m;
        def.end_station_m = self.end_station_m;
        def.crossfall_shape = match self.shape {
            1 => pavement::CrossfallShape::OneWay { low_side: pavement::Side::Left },
            2 => pavement::CrossfallShape::OneWay { low_side: pavement::Side::Right },
            _ => pavement::CrossfallShape::Crowned,
        };
        let shoulder = |s: &ShoulderRequest| pavement::Shoulder { width_m: s.width_m, crossfall_percent: s.crossfall_percent, layers: pavement::ShoulderLayers::SameAsCarriageway };
        def.left_shoulder = self.left_shoulder.as_ref().map(shoulder);
        def.right_shoulder = self.right_shoulder.as_ref().map(shoulder);
        def
    }
}

pub fn add_pavement(doc: &mut Document, alignment_id: ElementId, req: &PavementRequest) -> Result<String, String> {
    let a = alignment(doc, alignment_id)?;
    let id = pavement::add_pavement(doc, req.name.trim(), &req.definition(a)).map_err(|e| e.to_string())?;
    Ok(doc.find(id).map(|e| e.name.clone()).unwrap_or_default())
}

// ─── pavement design methods ────────────────────────────────────────────────────────────

pub const METHODS: [pavement::design::DesignMethod; 3] = [pavement::design::DesignMethod::MowtRdmVol3, pavement::design::DesignMethod::DmrbCd226, pavement::design::DesignMethod::Aashto1993];

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ClassRow {
    pub name: String,
    pub daily_first_year: f64,
    pub esa_per_vehicle: f64,
}

/// Inputs of the Uganda MoWT Road Design Manual Volume 3 method.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MowtForm {
    /// One direction. The other direction is taken to carry the same.
    pub classes: Vec<ClassRow>,
    pub growth_percent: f64,
    pub design_period_years: u32,
    pub dual_carriageway: bool,
    pub paved_width_m: f64,
    pub narrow_road_factor: Option<f64>,
    pub commercial_vehicles_per_day_one_direction: f64,
    pub subgrade_cbr_percent: f64,
    pub dry: bool,
    pub drainage_good: bool,
    pub maintenance_good: bool,
    /// Index into `mowt::PavementType::ALL`.
    pub pavement: usize,
}

impl Default for MowtForm {
    fn default() -> Self {
        MowtForm {
            classes: vec![ClassRow { name: String::new(), daily_first_year: 0.0, esa_per_vehicle: 0.0 }],
            growth_percent: 0.0,
            design_period_years: 0,
            dual_carriageway: false,
            paved_width_m: 0.0,
            narrow_road_factor: None,
            commercial_vehicles_per_day_one_direction: 0.0,
            subgrade_cbr_percent: 0.0,
            dry: true,
            drainage_good: true,
            maintenance_good: true,
            pavement: 0,
        }
    }
}

pub const DMRB_PAVEMENTS: [&str; 6] = ["Asphalt base, AC 40/60", "Asphalt base, EME2", "HBGM base, category A", "HBGM base, category B", "HBGM base, category C", "HBGM base, category D"];

/// Inputs of the UK DMRB CD 224 / CD 226 method.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DmrbForm {
    /// (index into `dmrb::VehicleGroup::ALL`, annual average daily flow in one direction)
    pub flows: Vec<(usize, f64)>,
    pub maintenance: bool,
    pub design_period_years: u32,
    pub lanes_one_direction: u32,
    pub past_traffic: bool,
    pub foundation_class: u8,
    /// Index into [`DMRB_PAVEMENTS`].
    pub pavement: usize,
}

impl Default for DmrbForm {
    fn default() -> Self {
        DmrbForm { flows: vec![(0, 0.0)], maintenance: false, design_period_years: 0, lanes_one_direction: 0, past_traffic: false, foundation_class: 1, pavement: 0 }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SnRow {
    pub name: String,
    pub material: usize,
    pub thickness_mm: f64,
    pub layer_coefficient: f64,
    pub drainage_coefficient: f64,
    pub resilient_modulus_psi: Option<f64>,
}

/// Inputs of the AASHTO 1993 flexible pavement method.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AashtoForm {
    pub w18: f64,
    pub reliability_percent: f64,
    pub overall_standard_deviation: f64,
    pub delta_psi: f64,
    pub roadbed_modulus_psi: f64,
    pub layers: Vec<SnRow>,
}

/// Everything the "Pavement design" dialog holds.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DesignForm {
    pub method: usize,
    pub mowt: MowtForm,
    pub dmrb: DmrbForm,
    pub aashto: AashtoForm,
}

/// What a design run gives back.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignOutcome {
    /// The calculation, with the warnings the method returns as "Note:" lines.
    pub lines: Vec<String>,
    /// The layers the method arrives at, when it does.
    pub layers: Option<Vec<pavement::Layer>>,
}

impl DesignForm {
    pub fn input(&self) -> Result<pavement::design::DesignInput, String> {
        use pavement::design::{aashto93, dmrb, mowt, DesignInput};
        Ok(match METHODS[self.method.min(METHODS.len() - 1)] {
            pavement::design::DesignMethod::MowtRdmVol3 => {
                let f = &self.mowt;
                let provision = |good: bool| if good { mowt::Provision::Good } else { mowt::Provision::Deficient };
                DesignInput::Mowt(mowt::MowtInput {
                    traffic: mowt::TrafficInput {
                        direction_a: f.classes.iter().map(|c| mowt::VehicleClass::new(c.name.trim(), c.daily_first_year, c.esa_per_vehicle)).collect(),
                        direction_b: None,
                        growth_percent: f.growth_percent,
                        design_period_years: f.design_period_years,
                        road: if f.dual_carriageway {
                            mowt::RoadType::DualCarriageway { commercial_vehicles_per_day_one_direction: f.commercial_vehicles_per_day_one_direction }
                        } else {
                            mowt::RoadType::SingleCarriageway { paved_width_m: f.paved_width_m, narrow_road_factor: f.narrow_road_factor }
                        },
                    },
                    subgrade_cbr_percent: f.subgrade_cbr_percent,
                    climate: if f.dry { mowt::Climate::PredominantlyDry } else { mowt::Climate::PredominantlyWet { drainage: provision(f.drainage_good), maintenance: provision(f.maintenance_good) } },
                    pavement: mowt::PavementType::ALL[f.pavement.min(mowt::PavementType::ALL.len() - 1)],
                })
            }
            pavement::design::DesignMethod::DmrbCd226 => {
                let f = &self.dmrb;
                DesignInput::Dmrb(dmrb::DmrbInput {
                    traffic: dmrb::TrafficInput {
                        flows: f.flows.iter().map(|(g, aadf)| dmrb::Flow::new(dmrb::VehicleGroup::ALL[(*g).min(dmrb::VehicleGroup::ALL.len() - 1)], *aadf)).collect(),
                        scheme: if f.maintenance { dmrb::Scheme::Maintenance } else { dmrb::Scheme::New },
                        design_period_years: f.design_period_years,
                        lanes_one_direction: f.lanes_one_direction,
                        past_traffic: f.past_traffic,
                    },
                    foundation_class: f.foundation_class,
                    pavement: match f.pavement {
                        0 => dmrb::FlexiblePavement::AsphaltBase { material: dmrb::AsphaltMaterial::Ac4060 },
                        1 => dmrb::FlexiblePavement::AsphaltBase { material: dmrb::AsphaltMaterial::Eme2 },
                        2 => dmrb::FlexiblePavement::HbgmBase { category: dmrb::HbgmCategory::A },
                        3 => dmrb::FlexiblePavement::HbgmBase { category: dmrb::HbgmCategory::B },
                        4 => dmrb::FlexiblePavement::HbgmBase { category: dmrb::HbgmCategory::C },
                        _ => dmrb::FlexiblePavement::HbgmBase { category: dmrb::HbgmCategory::D },
                    },
                })
            }
            // The last method; DesignMethod is non-exhaustive, so this arm also answers any added later,
            // which METHODS does not list.
            _ => {
                let f = &self.aashto;
                let list = materials();
                DesignInput::Aashto93(aashto93::Aashto93Input {
                    w18: f.w18,
                    reliability: aashto93::Reliability::Percent(f.reliability_percent),
                    overall_standard_deviation: f.overall_standard_deviation,
                    delta_psi: f.delta_psi,
                    roadbed_modulus_psi: f.roadbed_modulus_psi,
                    layers: f
                        .layers
                        .iter()
                        .map(|l| aashto93::SnLayer {
                            name: l.name.trim().to_string(),
                            material: list[l.material.min(list.len() - 1)].clone(),
                            thickness_mm: l.thickness_mm,
                            layer_coefficient: l.layer_coefficient,
                            drainage_coefficient: l.drainage_coefficient,
                            resilient_modulus_psi: l.resilient_modulus_psi,
                        })
                        .collect(),
                })
            }
        })
    }

    /// Runs the chosen method. The warnings are in the lines, as the method writes them.
    pub fn run(&self) -> Result<DesignOutcome, String> {
        let result = pavement::design::design(&self.input()?).map_err(|e| e.to_string())?;
        Ok(DesignOutcome { lines: result.calculation(), layers: result.layers() })
    }
}

// ─── ground surfaces ────────────────────────────────────────────────────────────────────

/// Imports the TIN surfaces of a LandXML file. Returns how many were added and the reader's notes.
pub fn import_surfaces(doc: &mut Document, path: &std::path::Path) -> Result<(usize, Vec<String>), String> {
    let (ids, notes) = corridor::import_landxml_surfaces(doc, path)?;
    Ok((ids.len(), notes))
}

// ─── corridors ──────────────────────────────────────────────────────────────────────────

/// What the "Corridor" dialog collects. The template is the same both sides of the centreline.
#[derive(Debug, Clone, PartialEq)]
pub struct CorridorRequest {
    pub name: String,
    pub surface: Option<ElementId>,
    /// Width of the lane on each side of the centreline.
    pub lane_width_m: f64,
    /// Fall of the lane away from the centreline, per cent.
    pub lane_crossfall_percent: f64,
    /// The lane takes the cross-fall the alignment stores, where it stores one.
    pub follow_superelevation: bool,
    pub shoulder: Option<ShoulderRequest>,
    pub shoulder_paved: bool,
    pub cut_run_per_rise: f64,
    pub fill_run_per_rise: f64,
    pub formation_depth_m: f64,
    pub interval_m: f64,
    /// Internal start and end station of a stretch; None builds the whole alignment.
    pub stretch: Option<(f64, f64)>,
}

impl CorridorRequest {
    pub fn empty(name: &str) -> CorridorRequest {
        CorridorRequest {
            name: name.to_string(),
            surface: None,
            lane_width_m: 0.0,
            lane_crossfall_percent: 0.0,
            follow_superelevation: false,
            shoulder: None,
            shoulder_paved: false,
            cut_run_per_rise: 0.0,
            fill_run_per_rise: 0.0,
            formation_depth_m: 0.0,
            interval_m: 0.0,
            stretch: None,
        }
    }

    pub fn definition(&self) -> corridor::CorridorDefinition {
        let mut lane = corridor::Link::new("lane", self.lane_width_m, corridor::Slope::Percent(-self.lane_crossfall_percent)).paved();
        if self.follow_superelevation {
            lane = lane.following_superelevation();
        }
        let mut links = vec![lane];
        if let Some(s) = &self.shoulder {
            let mut link = corridor::Link::new("shoulder", s.width_m, corridor::Slope::Percent(-s.crossfall_percent));
            if self.shoulder_paved {
                link = link.paved();
            }
            links.push(link);
        }
        let daylight = corridor::Daylight { cut_run_per_rise: self.cut_run_per_rise, fill_run_per_rise: self.fill_run_per_rise };
        let template = corridor::Template::symmetric("Typical section", links, daylight, self.formation_depth_m);
        let mut def = corridor::CorridorDefinition::single(self.name.trim(), template, self.interval_m);
        if let Some((start, end)) = self.stretch {
            def.start_station_m = Some(start);
            def.end_station_m = Some(end);
        }
        def
    }
}

/// Builds a corridor on an alignment and a surface and adds it. Returns its mark and a line on
/// what was computed.
pub fn add_corridor(doc: &mut Document, alignment_id: ElementId, req: &CorridorRequest) -> Result<(String, String), String> {
    let surface = req.surface.ok_or("Choose a ground surface. Import one with SURFACE.")?;
    let def = req.definition();
    def.check()?;
    let (id, built) = corridor::add_corridor(doc, alignment_id, surface, def).map_err(|e| e.to_string())?;
    let mark = doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
    let (cut, fill) = totals(doc, id).unwrap_or((0.0, 0.0));
    let summary = format!("{} sections, cut {cut:.0} m3, fill {fill:.0} m3", built.sections.len());
    Ok((mark, summary))
}

fn totals(doc: &Document, id: ElementId) -> Option<(f64, f64)> {
    corridor::corridor_element(doc, id).and_then(|c| c.computed).map(|s| (s.total_cut_m3, s.total_fill_m3))
}

/// The corridor surfaces for the 3D site view: the meshes (top, cut slopes, fill slopes) and the
/// site position of their origin, in millimetres.
pub struct CorridorSolids {
    pub origin: [f64; 3],
    pub meshes: Vec<Mesh3>,
}

/// Builds the corridor of an element and its 3D surfaces. Decoding the surface takes time on a
/// large survey, so the app keeps the result until the element's data changes.
pub fn corridor_solids(doc: &Document, id: ElementId) -> Result<CorridorSolids, String> {
    let built = corridor::corridor_of(doc, id).map_err(|e| e.to_string())?;
    if built.sections.is_empty() {
        return Err("The corridor has no sections.".into());
    }
    let m = corridor::corridor_meshes(&built);
    Ok(CorridorSolids { origin: [m.origin_x_m * 1000.0, m.origin_y_m * 1000.0, 0.0], meshes: vec![m.top, m.cut_slopes, m.fill_slopes] })
}

/// The built corridor of an element, for its cross-sections.
pub fn built_corridor(doc: &Document, id: ElementId) -> Result<corridor::Corridor, String> {
    corridor::corridor_of(doc, id).map_err(|e| e.to_string())
}

// ─── keeping elements in step, and quantities ───────────────────────────────────────────

/// Brings a lighting element's layout back in step with its definition and alignment.
pub fn refresh_lighting(doc: &mut Document, id: ElementId) -> Result<(), String> {
    let data = lighting::lighting_of(doc, id).map_err(|e| e.to_string())?;
    let name = data.definition.alignment().to_string();
    let a = alignment_named(doc, &name).ok_or_else(|| format!("There is no alignment named '{name}'."))?;
    lighting::refresh_layout(doc, id, &a, None).map_err(|e| e.to_string())
}

pub fn refresh_restraint(doc: &mut Document, id: ElementId) -> Result<(), String> {
    let b = restraint::barrier_run_of(doc, id).map_err(|e| e.to_string())?;
    let a = alignment_named(doc, &b.run.alignment).ok_or_else(|| format!("There is no alignment named '{}'.", b.run.alignment))?;
    restraint::update_barrier_run(doc, id, &a, b.run).map_err(|e| e.to_string())
}

pub fn refresh_pavement(doc: &mut Document, id: ElementId) -> Result<(), String> {
    let def = doc.find(id).ok_or("The element no longer exists.")?.data.clone();
    let def = pavement::PavementDefinition::from_value(&def).map_err(|e| e.to_string())?;
    pavement::update_pavement(doc, id, &def).map_err(|e| e.to_string())
}

pub fn refresh_corridor(doc: &mut Document, id: ElementId) -> Result<(), String> {
    let data = corridor::corridor_element(doc, id).ok_or("The corridor cannot be read.")?;
    corridor::set_corridor_definition(doc, id, data.definition).map(|_| ()).map_err(|e| e.to_string())
}

/// Quantity CSVs of lighting, restraint, pavement and corridors: (file suffix, contents), and
/// what could not be measured and why.
pub fn quantities(doc: &Document) -> (Vec<(String, String)>, Vec<String>) {
    let (mut files, mut problems) = (Vec::new(), Vec::new());
    let l = lighting::LightingQuantities::of_document(doc);
    if l.column_count() > 0 {
        files.push(("lighting.csv".to_string(), lighting::schedule_to_csv(&l)));
        files.push(("lighting.summary.csv".to_string(), lighting::summary_to_csv(&l)));
    }
    problems.extend(l.skipped.iter().map(|s| format!("Lighting {} not measured: {}", s.name, s.reason)));
    let r = restraint::RestraintQuantities::measure(doc.elements());
    if !r.runs.is_empty() {
        files.push(("restraint.csv".to_string(), restraint::runs_to_csv(&r.runs)));
        files.push(("restraint.summary.csv".to_string(), restraint::summary_to_csv(&r)));
    }
    problems.extend(r.skipped.iter().map(|s| format!("Restraint {} not measured: {}", s.mark, s.reason)));
    let p = pavement::PavementQuantities::measure(doc.elements());
    if !p.rows.is_empty() {
        files.push(("pavement.csv".to_string(), pavement::layers_to_csv(&p.rows)));
        files.push(("pavement.summary.csv".to_string(), pavement::summary_to_csv(&p)));
    }
    problems.extend(p.skipped.iter().map(|s| format!("Pavement {} not measured: {}", s.mark, s.reason)));
    let corridors: Vec<(ElementId, String)> = doc.elements().iter().filter(|e| e.category == corridor::CORRIDOR_CATEGORY).map(|e| (e.id, e.name.clone())).collect();
    for (id, name) in &corridors {
        let result = corridor::corridor_element(doc, *id).ok_or_else(|| "its data cannot be read".to_string()).and_then(|data| {
            let built = corridor::corridor_of(doc, *id).map_err(|e| e.to_string())?;
            Ok(corridor::earthworks_csv(&corridor::earthworks(&built.sections, &data.definition.volumes)))
        });
        match result {
            Ok(csv) if corridors.len() == 1 => files.push(("earthworks.csv".to_string(), csv)),
            Ok(csv) => files.push((format!("earthworks-{}.csv", name.replace(|c: char| !c.is_ascii_alphanumeric(), "-")), csv)),
            Err(why) => problems.push(format!("Corridor {name} not measured: {why}")),
        }
    }
    (files, problems)
}

// ─── the demonstration ──────────────────────────────────────────────────────────────────

/// The stretch of the first alignment the demo pavement covers, as fractions of its length
/// (arbitrary demo values). The demo road markings are laid over the same stretch, see
/// `markings_glue::demo`.
pub const DEMO_PAVEMENT_FRACTIONS: (f64, f64) = (0.55, 0.95);

/// Adds one of each road module on the first alignment of the document, for `--demo`, and
/// returns what could not be added and why.
///
/// **Every number in this function is an arbitrary demonstration value.** None is a design
/// value, a value from a standard or a recommendation: they are chosen only so that each module
/// has something to show on the test alignments. The ground is a made-up undulating surface
/// built here, not a survey.
pub fn demo(doc: &mut Document) -> Vec<String> {
    let mut problems = Vec::new();
    let Some((id, _)) = alignment_glue::alignments(doc).into_iter().next() else { return vec!["There is no alignment yet. Import one with File → Import LandXML.".to_string()] };
    let Some(a) = alignment_glue::alignment_of(doc, id) else { return vec!["That alignment cannot be read.".to_string()] };
    let at = |fraction: f64| a.start_station_m + a.length() * fraction;

    // A made-up ground: a gentle undulation about the profile's start level over the alignment's
    // plan extent plus a margin (arbitrary: 1 m below that level, 4 m of relief, 25 m grid, 150 m margin).
    let plan = a.polyline(20.0);
    let (x0, x1) = plan.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
    let (y0, y1) = plan.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
    let base = a.profile.as_ref().and_then(|p| p.range().and_then(|(lo, _)| p.elevation_at(lo).ok())).unwrap_or(0.0);
    let (margin, step) = (150.0, 25.0);
    let (nx, ny) = (((x1 - x0 + 2.0 * margin) / step).ceil() as usize + 1, ((y1 - y0 + 2.0 * margin) / step).ceil() as usize + 1);
    let ground = corridor::Tin::from_grid(x0 - margin, y0 - margin, step, step, nx.max(2), ny.max(2), |x, y| base - 1.0 + 4.0 * (x / 90.0).sin() * (y / 70.0).cos());
    let surface = match ground {
        Ok(tin) => corridor::add_surface(doc, "Demo ground", &tin, Some("built in code for the demo")).map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };

    // A corridor over the first half of the alignment: 3.3 m lanes falling 3 % away from the
    // centreline, 1.2 m paved shoulders falling 4 %, 2 : 1 side slopes, 0.4 m formation, 20 m sections.
    match surface {
        Ok(surface) => {
            let mut req = CorridorRequest::empty("Demo corridor");
            req.surface = Some(surface);
            req.lane_width_m = 3.3;
            req.lane_crossfall_percent = 3.0;
            req.shoulder = Some(ShoulderRequest { width_m: 1.2, crossfall_percent: 4.0 });
            req.shoulder_paved = true;
            req.cut_run_per_rise = 2.0;
            req.fill_run_per_rise = 2.0;
            req.formation_depth_m = 0.4;
            req.interval_m = 20.0;
            let mut def = req.definition();
            def.start_station_m = Some(at(0.0));
            def.end_station_m = Some(at(0.5));
            note(&mut problems, "Corridor", corridor::add_corridor(doc, id, surface, def).map(|_| ()).map_err(|e| e.to_string()));
        }
        Err(e) => problems.push(format!("Ground surface: {e}")),
    }

    // Street lighting: staggered columns every 40 m on the first stretch; one extra column.
    let mut req = LightingRequest::empty("LZ-01", at(0.03), at(0.3));
    req.arrangement = 2;
    req.spacing_m = 40.0;
    req.carriageway_width_m = 6.6;
    req.setback_m = 1.0;
    req.column = ColumnFields { mounting_height_m: 8.0, bracket_outreach_m: 1.0, tilt_deg: 0.0, product: "Demo luminaire".into(), lumen_output_lm: 5000.0, power_w: 50.0 };
    req.lighting_class = "demo".into();
    note(&mut problems, "Street lighting", add_lighting(doc, id, &req).map(|_| ()));
    let mut col = ColumnRequest::empty("LC-901", at(0.7));
    col.distance_m = 5.0;
    col.right = true;
    col.column = req.column.clone();
    note(&mut problems, "Light column", add_light_column(doc, id, &col).map(|_| ()));

    // A safety barrier run on the right: the 5th containment level in the crate's list, the 4th
    // working width class, impact severity A, a 0.7 m beam, posts at 2 m, a terminal at each end.
    let mut req = RestraintRequest::empty("VRS-001", at(0.1), at(0.2));
    req.distance_m = 5.5;
    req.right = true;
    req.containment = 4;
    req.working_width = 3;
    req.beam_height_m = 0.7;
    req.post_spacing_m = Some(2.0);
    req.start_terminal = Some(0);
    req.end_terminal = Some(0);
    note(&mut problems, "Road restraint", add_restraint(doc, id, &req).map(|_| ()));

    // A pavement from 55 % to 95 % of the alignment: 6.6 m wide, 2.5 % cross-fall, four layers of 40, 100, 150
    // and 200 mm, with 1.2 m shoulders.
    let mut req = PavementRequest::empty("PV-001", at(DEMO_PAVEMENT_FRACTIONS.0), at(DEMO_PAVEMENT_FRACTIONS.1));
    req.carriageway_width_m = 6.6;
    req.crossfall_percent = 2.5;
    req.layers = [("Surfacing", 1, 40.0), ("Base", 2, 100.0), ("Subbase", 5, 150.0), ("Selected layer", 8, 200.0)]
        .iter()
        .map(|(name, material, thickness_mm)| LayerRow { name: name.to_string(), material: *material, thickness_mm: *thickness_mm })
        .collect();
    req.left_shoulder = Some(ShoulderRequest { width_m: 1.2, crossfall_percent: 4.0 });
    req.right_shoulder = Some(ShoulderRequest { width_m: 1.2, crossfall_percent: 4.0 });
    note(&mut problems, "Pavement", add_pavement(doc, id, &req).map(|_| ()));
    problems
}

fn note(problems: &mut Vec<String>, what: &str, result: Result<(), String>) {
    if let Err(e) = result {
        problems.push(format!("{what}: {e}"));
    }
}
