//! The input dialog of the street lighting CHECK.
//!
//! The checks of `afrobim_lighting::design` take what the engineer has established elsewhere:
//! the appraisal, the design speed, the photometric results of a calculation made in a lighting
//! package, and the parameters of the class selection. Every field here starts empty. A field
//! left empty is "not entered" and the report says which checks it stops; nothing is assumed.
//! The entries are kept on the lighting element (`check.lighting_inputs`, see `check_inputs`), so
//! the dialog opens with them again and the model-wide check repeats them. The report quotes them.

use afrobim_kernel::{Document, ElementId};
use afrobim_lighting::bs5489_2003::{IpMinimum, Pollution, RoadType};
use afrobim_lighting::design::{CheckInputs, PhotometricResults, UnlitGap};
use afrobim_lighting::en13201::selection::{
    AmbientLuminance, CSelection, ClassSelectionInput, DesignSpeed, JunctionDensity, MSelection, NavigationalTask, PSelection, PedestrianSpeed, PedestrianTraffic, TrafficComposition,
    TrafficVolume, UseIntensity,
};
use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::road_dialogs::Outcome;
use crate::road_glue;

/// An enumeration of the lighting crate, kept by the name of its variant. The crate's enumerations
/// carry no serde support, and every variant the dialog offers is listed here.
pub trait Named: Copy + std::fmt::Debug + 'static {
    fn all() -> &'static [Self];
}

macro_rules! named {
    ($t:ty, [$($v:expr),* $(,)?]) => {
        impl Named for $t {
            fn all() -> &'static [Self] {
                &[$($v),*]
            }
        }
    };
}

named!(IpMinimum, [IpMinimum::Ip2x, IpMinimum::Ip5x, IpMinimum::Ip6x]);
named!(Pollution, [Pollution::High, Pollution::Medium, Pollution::Low]);
named!(
    RoadType,
    [
        RoadType::MotorwayComplexInterchange,
        RoadType::MotorwayInterchangesUnder3Km,
        RoadType::MotorwayInterchanges3KmOrMore,
        RoadType::MotorwayEmergencyLane,
        RoadType::StrategicSingle,
        RoadType::StrategicDual,
        RoadType::MainDistributorSingle,
        RoadType::MainDistributorDual,
        RoadType::SecondaryDistributorRural,
        RoadType::SecondaryDistributorUrban,
        RoadType::LinkRoadRural,
        RoadType::LinkRoadUrban,
        RoadType::LinkRoadUrbanHighPedestrian,
    ]
);
named!(DesignSpeed, [DesignSpeed::VeryHigh, DesignSpeed::High, DesignSpeed::Moderate, DesignSpeed::Low]);
named!(TrafficVolume, [TrafficVolume::High, TrafficVolume::Moderate, TrafficVolume::Low]);
named!(TrafficComposition, [TrafficComposition::MixedWithHighPercentageOfNonMotorized, TrafficComposition::Mixed, TrafficComposition::MotorizedOnly]);
named!(JunctionDensity, [JunctionDensity::High, JunctionDensity::Moderate]);
named!(AmbientLuminance, [AmbientLuminance::High, AmbientLuminance::Moderate, AmbientLuminance::Low]);
named!(NavigationalTask, [NavigationalTask::VeryDifficult, NavigationalTask::Difficult, NavigationalTask::Easy]);
named!(PedestrianSpeed, [PedestrianSpeed::Low, PedestrianSpeed::VeryLow]);
named!(UseIntensity, [UseIntensity::Busy, UseIntensity::Normal, UseIntensity::Quiet]);
named!(
    PedestrianTraffic,
    [
        PedestrianTraffic::PedestriansCyclistsAndMotorized,
        PedestrianTraffic::PedestriansAndMotorized,
        PedestrianTraffic::PedestriansAndCyclistsOnly,
        PedestrianTraffic::PedestriansOnly,
        PedestrianTraffic::CyclistsOnly,
    ]
);

/// serde for an optional [`Named`] enumeration: the variant's name, or null.
mod named_opt {
    use super::Named;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T: Named, S: Serializer>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error> {
        value.map(|v| format!("{v:?}")).serialize(serializer)
    }

    pub fn deserialize<'de, T: Named, D: Deserializer<'de>>(deserializer: D) -> Result<Option<T>, D::Error> {
        let name = Option::<String>::deserialize(deserializer)?;
        Ok(name.and_then(|n| T::all().iter().copied().find(|v| format!("{v:?}") == n)))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize, Deserialize)]
pub enum Tri {
    #[default]
    NotEntered,
    Yes,
    No,
}

impl Tri {
    fn value(self) -> Option<bool> {
        match self {
            Tri::NotEntered => None,
            Tri::Yes => Some(true),
            Tri::No => Some(false),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize, Deserialize)]
pub enum Family {
    #[default]
    None,
    M,
    C,
    P,
}

/// Everything the dialog holds. Text fields are parsed when the checks run.
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Form {
    appraisal_done: Tri,
    design_speed_kph: String,
    stopping_sight_distance_m: String,
    gaps: Vec<(String, String)>,
    required_setback_m: String,
    required_setback_source: String,
    passively_safe: Tri,
    passive_safety_class: String,
    speed_limit_mph: String,
    aadt: String,
    maintenance_factor: String,
    maintenance_factor_source: String,
    #[serde(with = "named_opt")]
    road_type: Option<RoadType>,
    class_adt: String,
    conflict_approach_class: String,
    #[serde(with = "named_opt")]
    luminaire_ip: Option<IpMinimum>,
    #[serde(with = "named_opt")]
    pollution: Option<Pollution>,
    cleaning_interval_months: String,
    lamp_flux_maintenance_factor: String,
    // Photometric results.
    ph_source: String,
    l_av: String,
    u_o: String,
    u_l: String,
    u_o_wet: String,
    ti: String,
    eir: String,
    e_av: String,
    u_o_e: String,
    e_h_min: String,
    e_v_min: String,
    e_sc_min: String,
    facial: Tri,
    low_speed: Tri,
    // Class selection.
    family: Family,
    #[serde(with = "named_opt")]
    m_speed: Option<DesignSpeed>,
    #[serde(with = "named_opt")]
    m_volume: Option<TrafficVolume>,
    #[serde(with = "named_opt")]
    m_composition: Option<TrafficComposition>,
    m_separated: Tri,
    #[serde(with = "named_opt")]
    m_junctions: Option<JunctionDensity>,
    m_parked: Tri,
    #[serde(with = "named_opt")]
    m_ambient: Option<AmbientLuminance>,
    #[serde(with = "named_opt")]
    m_navigation: Option<NavigationalTask>,
    #[serde(with = "named_opt")]
    p_speed: Option<PedestrianSpeed>,
    #[serde(with = "named_opt")]
    p_intensity: Option<UseIntensity>,
    #[serde(with = "named_opt")]
    p_traffic: Option<PedestrianTraffic>,
    p_facial: Tri,
}

pub struct LightingCheck {
    pub element: ElementId,
    pub form: Form,
}

impl LightingCheck {
    /// The dialog for a lighting element, with the entries kept on it when there are some.
    pub fn open(doc: &Document, element: ElementId) -> LightingCheck {
        LightingCheck { element, form: crate::check_inputs::load(doc, element, crate::check_inputs::KEY_LIGHTING).unwrap_or_default() }
    }
}

fn number(label: &str, text: &str) -> Result<Option<f64>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite()).map(Some).ok_or_else(|| format!("{label}: '{t}' is not a number"))
}

fn whole(label: &str, text: &str) -> Result<Option<u32>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    t.parse::<u32>().map(Some).map_err(|_| format!("{label}: '{t}' is not a whole number"))
}

fn need<T: Copy>(v: Option<T>, what: &str) -> Result<T, String> {
    v.ok_or_else(|| format!("Class selection: state {what}"))
}

impl Form {
    /// Writes photometric results into the entries (the source label, and every result that is set),
    /// as the PHOTOMETRY window does for "use these results in the class check".
    pub fn set_photometry(&mut self, ph: &PhotometricResults) {
        let n = |v: f64| afrobim_sheets::trim_number(v, 6);
        self.ph_source = ph.source.clone();
        for (slot, value) in [
            (&mut self.l_av, ph.l_av_cd_m2),
            (&mut self.u_o, ph.u_o),
            (&mut self.u_l, ph.u_l),
            (&mut self.u_o_wet, ph.u_o_wet),
            (&mut self.ti, ph.ti_percent),
            (&mut self.eir, ph.eir),
            (&mut self.e_av, ph.e_av_lx),
            (&mut self.u_o_e, ph.u_o_e),
            (&mut self.e_h_min, ph.e_h_min_lx),
            (&mut self.e_v_min, ph.e_v_min_lx),
            (&mut self.e_sc_min, ph.e_sc_min_lx),
        ] {
            if let Some(v) = value {
                *slot = n(v);
            }
        }
    }

    /// The inputs the engineer entered, or the first entry that cannot be read.
    pub fn inputs(&self) -> Result<CheckInputs, String> {
        let mut gaps = Vec::new();
        for (d, l) in &self.gaps {
            if d.trim().is_empty() && l.trim().is_empty() {
                continue;
            }
            let length = number("Unlit gap length", l)?.ok_or("An unlit gap needs its length")?;
            gaps.push(UnlitGap { description: d.trim().to_string(), length_m: length });
        }
        let photometry = PhotometricResults {
            source: self.ph_source.trim().to_string(),
            l_av_cd_m2: number("L_av", &self.l_av)?,
            u_o: number("U_o", &self.u_o)?,
            u_l: number("U_l", &self.u_l)?,
            u_o_wet: number("U_o (wet)", &self.u_o_wet)?,
            ti_percent: number("TI", &self.ti)?,
            eir: number("EIR", &self.eir)?,
            e_av_lx: number("E_av", &self.e_av)?,
            u_o_e: number("U_o(E)", &self.u_o_e)?,
            e_h_min_lx: number("E_h,min", &self.e_h_min)?,
            e_v_min_lx: number("E_v,min", &self.e_v_min)?,
            e_sc_min_lx: number("E_sc,min", &self.e_sc_min)?,
            facial_recognition_necessary: self.facial.value(),
            low_or_very_low_speed: self.low_speed.value(),
        };
        let class_selection = match self.family {
            Family::None => None,
            Family::M => Some(ClassSelectionInput::M(MSelection {
                design_speed: need(self.m_speed, "the design speed")?,
                traffic_volume: need(self.m_volume, "the traffic volume")?,
                traffic_composition: need(self.m_composition, "the traffic composition")?,
                carriageway_separated: need(self.m_separated.value(), "whether the carriageways are separated")?,
                junction_density: need(self.m_junctions, "the junction density")?,
                parked_vehicles_present: need(self.m_parked.value(), "whether vehicles are parked")?,
                ambient_luminance: need(self.m_ambient, "the ambient luminance")?,
                navigational_task: need(self.m_navigation, "the navigational task")?,
            })),
            Family::C => Some(ClassSelectionInput::C(CSelection {
                design_speed: need(self.m_speed, "the design speed")?,
                traffic_volume: need(self.m_volume, "the traffic volume")?,
                traffic_composition: need(self.m_composition, "the traffic composition")?,
                carriageway_separated: need(self.m_separated.value(), "whether the carriageways are separated")?,
                parked_vehicles_present: need(self.m_parked.value(), "whether vehicles are parked")?,
                ambient_luminance: need(self.m_ambient, "the ambient luminance")?,
                navigational_task: need(self.m_navigation, "the navigational task")?,
            })),
            Family::P => Some(ClassSelectionInput::P(PSelection {
                design_speed: need(self.p_speed, "the pedestrian speed")?,
                use_intensity: need(self.p_intensity, "the intensity of use")?,
                traffic: need(self.p_traffic, "the traffic using the area")?,
                parked_vehicles_present: need(self.m_parked.value(), "whether vehicles are parked")?,
                ambient_luminance: need(self.m_ambient, "the ambient luminance")?,
                facial_recognition_necessary: need(self.p_facial.value(), "whether facial recognition is necessary")?,
            })),
        };
        Ok(CheckInputs {
            appraisal_done: self.appraisal_done.value(),
            design_speed_kph: number("Design speed", &self.design_speed_kph)?,
            stopping_sight_distance_m: number("Stopping sight distance", &self.stopping_sight_distance_m)?,
            unlit_gaps: gaps,
            required_setback_m: number("Required set-back", &self.required_setback_m)?,
            required_setback_source: self.required_setback_source.trim().to_string(),
            passively_safe: self.passively_safe.value(),
            passive_safety_class: self.passive_safety_class.trim().to_string(),
            speed_limit_mph: number("Speed limit", &self.speed_limit_mph)?,
            aadt: number("AADT", &self.aadt)?,
            maintenance_factor: number("Maintenance factor", &self.maintenance_factor)?,
            maintenance_factor_source: self.maintenance_factor_source.trim().to_string(),
            road_type: self.road_type,
            class_adt: number("Average daily traffic", &self.class_adt)?,
            conflict_approach_class: Some(self.conflict_approach_class.trim().to_string()).filter(|s| !s.is_empty()),
            luminaire_ip: self.luminaire_ip,
            pollution: self.pollution,
            cleaning_interval_months: whole("Cleaning interval", &self.cleaning_interval_months)?,
            lamp_flux_maintenance_factor: number("Lamp flux maintenance factor", &self.lamp_flux_maintenance_factor)?,
            photometry,
            class_selection,
        })
    }
}

fn tri(ui: &mut Ui, salt: &str, v: &mut Tri) {
    egui::ComboBox::from_id_salt(salt).width(110.0).selected_text(match v {
        Tri::NotEntered => "not entered",
        Tri::Yes => "yes",
        Tri::No => "no",
    })
    .show_ui(ui, |ui| {
        ui.selectable_value(v, Tri::NotEntered, "not entered");
        ui.selectable_value(v, Tri::Yes, "yes");
        ui.selectable_value(v, Tri::No, "no");
    });
}

fn choose<T: Copy + PartialEq>(ui: &mut Ui, salt: &str, v: &mut Option<T>, options: &[(T, &str)]) {
    let shown = options.iter().find(|o| Some(o.0) == *v).map(|o| o.1).unwrap_or("not entered");
    egui::ComboBox::from_id_salt(salt).width(260.0).selected_text(shown).show_ui(ui, |ui| {
        ui.selectable_value(v, None, "not entered");
        for (value, label) in options {
            ui.selectable_value(v, Some(*value), *label);
        }
    });
}

fn field(ui: &mut Ui, label: &str, value: &mut String, hint: &str) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(150.0));
    ui.end_row();
}

const ROAD_TYPES: [RoadType; 13] = [
    RoadType::MotorwayComplexInterchange,
    RoadType::MotorwayInterchangesUnder3Km,
    RoadType::MotorwayInterchanges3KmOrMore,
    RoadType::MotorwayEmergencyLane,
    RoadType::StrategicSingle,
    RoadType::StrategicDual,
    RoadType::MainDistributorSingle,
    RoadType::MainDistributorDual,
    RoadType::SecondaryDistributorRural,
    RoadType::SecondaryDistributorUrban,
    RoadType::LinkRoadRural,
    RoadType::LinkRoadUrban,
    RoadType::LinkRoadUrbanHighPedestrian,
];

fn speed_options() -> [(DesignSpeed, &'static str); 4] {
    [(DesignSpeed::VeryHigh, "very high"), (DesignSpeed::High, "high"), (DesignSpeed::Moderate, "moderate"), (DesignSpeed::Low, "low")]
}

/// Draws the dialog. Returns the outcome when the checks were run.
pub fn show(ctx: &egui::Context, doc: &mut Document, slot: &mut Option<LightingCheck>, out: &mut Vec<Outcome>) {
    let Some(mut d) = slot.take() else { return };
    let mark = doc.find(d.element).map(|e| e.name.clone()).unwrap_or_default();
    let (mut keep, mut run) = (true, false);
        let footer_ok = true;
    let dlg = crate::ui_kit::Dialog::titled(format!("Lighting check inputs for {mark}")).width(560.0).show(ctx, |ui| {
        crate::ui_kit::note(ui, "Enter what you have established elsewhere. Every field starts empty; a field left empty stops the checks that need it and the report says so. Photometric values are the maintained results of a calculation: enter those from your lighting package, or run PHOTOMETRY on a luminaire file and use its results here (the entries are then labelled as computed by AfroBIM). AfroBIM's own photometry is not an EN 13201-3 calculation, has not been validated against DIALux, Relux or AGi32, calculates luminance only from an r-table you supply, and does not compute TI, U_l or the wet U_o: enter those from your own calculation.");
        egui::ScrollArea::vertical().max_height(480.0).auto_shrink([false, false]).show(ui, |ui| {
            let f = &mut d.form;
            crate::ui_kit::heading(ui, "The road and the installation");
            crate::ui_kit::grid(ui, "lc-road", |ui| {
                ui.label("Lighting need appraised (TA 501)");
                tri(ui, "lc-appraisal", &mut f.appraisal_done);
                ui.end_row();
                field(ui, "Design speed (km/h)", &mut f.design_speed_kph, "");
                field(ui, "Stopping sight distance (m)", &mut f.stopping_sight_distance_m, "if you take one");
                field(ui, "Speed limit (mph)", &mut f.speed_limit_mph, "");
                field(ui, "Two-way flow (AADT)", &mut f.aadt, "");
                field(ui, "Required set-back (m)", &mut f.required_setback_m, "");
                field(ui, "Source of the set-back", &mut f.required_setback_source, "e.g. the document used");
                ui.label("Columns passively safe");
                tri(ui, "lc-passive", &mut f.passively_safe);
                ui.end_row();
                field(ui, "Passive safety class", &mut f.passive_safety_class, "as certified");
                field(ui, "Maintenance factor", &mut f.maintenance_factor, "");
                field(ui, "Source of the factor", &mut f.maintenance_factor_source, "");
                field(ui, "Lamp flux maintenance factor", &mut f.lamp_flux_maintenance_factor, "from the manufacturer");
                ui.label("Luminaire sealing");
                choose(ui, "lc-ip", &mut f.luminaire_ip, &[(IpMinimum::Ip2x, "IP2X"), (IpMinimum::Ip5x, "IP5X"), (IpMinimum::Ip6x, "IP6X")]);
                ui.end_row();
                ui.label("Pollution category");
                choose(ui, "lc-pollution", &mut f.pollution, &[(Pollution::High, "high"), (Pollution::Medium, "medium"), (Pollution::Low, "low")]);
                ui.end_row();
                field(ui, "Cleaning interval (months)", &mut f.cleaning_interval_months, "");
                ui.label("Type of road (BS 5489-1:2003)");
                let types: Vec<(RoadType, &str)> = ROAD_TYPES.iter().map(|t| (*t, t.label())).collect();
                choose(ui, "lc-roadtype", &mut f.road_type, &types);
                ui.end_row();
                field(ui, "Average daily traffic, both directions", &mut f.class_adt, "");
                field(ui, "Approach class of a conflict area", &mut f.conflict_approach_class, "");
            });
            ui.add_space(4.0);
            ui.label("Unlit lengths to adjacent lit sections");
            let mut remove = None;
            egui::Grid::new("lc-gaps").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
                for (i, (desc, len)) in f.gaps.iter_mut().enumerate() {
                    ui.add(egui::TextEdit::singleline(desc).hint_text("what the gap is").desired_width(200.0));
                    ui.add(egui::TextEdit::singleline(len).hint_text("length, m").desired_width(80.0));
                    if crate::ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
            if let Some(i) = remove {
                f.gaps.remove(i);
            }
            if ui.button("Add an unlit length").clicked() {
                f.gaps.push((String::new(), String::new()));
            }

            ui.add_space(8.0);
            ui.label(RichText::new("Photometric results (maintained values), entered or from PHOTOMETRY").strong());
            crate::ui_kit::grid(ui, "lc-photo", |ui| {
                field(ui, "Where the results come from", &mut f.ph_source, "program, file, date");
                field(ui, "L_av, average luminance (cd/m2)", &mut f.l_av, "M classes");
                field(ui, "U_o, overall uniformity", &mut f.u_o, "M classes");
                field(ui, "U_l, longitudinal uniformity", &mut f.u_l, "M classes, long sections");
                field(ui, "U_o, wet", &mut f.u_o_wet, "M classes, where it applies");
                field(ui, "TI, threshold increment (%)", &mut f.ti, "");
                field(ui, "EIR, edge illuminance ratio", &mut f.eir, "M classes");
                field(ui, "E_av, average illuminance (lx)", &mut f.e_av, "C and P classes");
                field(ui, "U_o(E), uniformity of illuminance", &mut f.u_o_e, "C classes");
                field(ui, "E_h,min (lx)", &mut f.e_h_min, "P classes");
                field(ui, "E_v,min (lx)", &mut f.e_v_min, "P classes");
                field(ui, "E_sc,min (lx)", &mut f.e_sc_min, "P classes");
                ui.label("Facial recognition necessary");
                tri(ui, "lc-facial", &mut f.facial);
                ui.end_row();
                ui.label("Low or very low design speed");
                tri(ui, "lc-lowspeed", &mut f.low_speed);
                ui.end_row();
            });

            ui.add_space(8.0);
            ui.label(RichText::new("Class selection parameters (CEN/TR 13201-1, a suggestion only)").strong());
            ui.horizontal(|ui| {
                ui.radio_value(&mut f.family, Family::None, "none");
                ui.radio_value(&mut f.family, Family::M, "M class");
                ui.radio_value(&mut f.family, Family::C, "C class");
                ui.radio_value(&mut f.family, Family::P, "P class");
            });
            if f.family != Family::None {
                crate::ui_kit::grid(ui, "lc-sel", |ui| {
                    if f.family == Family::P {
                        ui.label("Pedestrian speed");
                        choose(ui, "lc-psp", &mut f.p_speed, &[(PedestrianSpeed::Low, "low"), (PedestrianSpeed::VeryLow, "very low")]);
                        ui.end_row();
                        ui.label("Intensity of use");
                        choose(ui, "lc-pint", &mut f.p_intensity, &[(UseIntensity::Busy, "busy"), (UseIntensity::Normal, "normal"), (UseIntensity::Quiet, "quiet")]);
                        ui.end_row();
                        ui.label("Traffic using the area");
                        choose(
                            ui,
                            "lc-ptr",
                            &mut f.p_traffic,
                            &[
                                (PedestrianTraffic::PedestriansCyclistsAndMotorized, "pedestrians, cyclists and motor vehicles"),
                                (PedestrianTraffic::PedestriansAndMotorized, "pedestrians and motor vehicles"),
                                (PedestrianTraffic::PedestriansAndCyclistsOnly, "pedestrians and cyclists only"),
                                (PedestrianTraffic::PedestriansOnly, "pedestrians only"),
                                (PedestrianTraffic::CyclistsOnly, "cyclists only"),
                            ],
                        );
                        ui.end_row();
                        ui.label("Facial recognition necessary");
                        tri(ui, "lc-pfr", &mut f.p_facial);
                        ui.end_row();
                    } else {
                        ui.label("Design speed");
                        choose(ui, "lc-msp", &mut f.m_speed, &speed_options());
                        ui.end_row();
                        ui.label("Traffic volume");
                        choose(ui, "lc-mvol", &mut f.m_volume, &[(TrafficVolume::High, "high"), (TrafficVolume::Moderate, "moderate"), (TrafficVolume::Low, "low")]);
                        ui.end_row();
                        ui.label("Traffic composition");
                        choose(
                            ui,
                            "lc-mcomp",
                            &mut f.m_composition,
                            &[
                                (TrafficComposition::MixedWithHighPercentageOfNonMotorized, "mixed, with a high share of non-motorised"),
                                (TrafficComposition::Mixed, "mixed"),
                                (TrafficComposition::MotorizedOnly, "motorised only"),
                            ],
                        );
                        ui.end_row();
                        ui.label("Carriageways separated");
                        tri(ui, "lc-msep", &mut f.m_separated);
                        ui.end_row();
                        if f.family == Family::M {
                            ui.label("Junction density");
                            choose(ui, "lc-mjun", &mut f.m_junctions, &[(JunctionDensity::High, "high"), (JunctionDensity::Moderate, "moderate")]);
                            ui.end_row();
                        }
                        ui.label("Navigational task");
                        choose(ui, "lc-mnav", &mut f.m_navigation, &[(NavigationalTask::VeryDifficult, "very difficult"), (NavigationalTask::Difficult, "difficult"), (NavigationalTask::Easy, "easy")]);
                        ui.end_row();
                    }
                    ui.label("Vehicles parked");
                    tri(ui, "lc-mpark", &mut f.m_parked);
                    ui.end_row();
                    ui.label("Ambient luminance");
                    choose(ui, "lc-mamb", &mut f.m_ambient, &[(AmbientLuminance::High, "high"), (AmbientLuminance::Moderate, "moderate"), (AmbientLuminance::Low, "low")]);
                    ui.end_row();
                });
            }
        });
    }, |ui| crate::ui_kit::Footer::new("Run the checks").enabled(footer_ok, "Complete the inputs").cancel(Some("Cancel")).show(ui));
    if let Some((_, f)) = &dlg.inner {
        run = f.primary;
        if f.cancel {
            keep = false;
        }
    }
    if dlg.closed {
        keep = false;
    }
    if run {
        match d.form.inputs().and_then(|inputs| road_glue::check_lighting_with(doc, d.element, &inputs)) {
            Ok(lines) => {
                // The entries stay on the element for the model-wide check.
                crate::check_inputs::save(doc, d.element, crate::check_inputs::KEY_LIGHTING, &d.form);
                out.push(Outcome::Report(mark, lines));
            }
            Err(e) => {
                out.push(Outcome::Status(format!("Not checked: {e}")));
                *slot = Some(d);
            }
        }
    } else if keep {
        *slot = Some(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entries_survive_being_kept_on_the_element() {
        let mut f = Form::default();
        f.l_av = "1.5".into();
        f.family = Family::M;
        f.road_type = Some(RoadType::StrategicDual);
        f.m_speed = Some(DesignSpeed::High);
        f.appraisal_done = Tri::Yes;
        f.gaps.push(("to the junction".into(), "40".into()));
        let text = serde_json::to_string(&f).unwrap();
        let back: Form = serde_json::from_str(&text).unwrap();
        assert_eq!(back.l_av, "1.5");
        assert_eq!(back.family, Family::M);
        assert_eq!(back.road_type, Some(RoadType::StrategicDual));
        assert_eq!(back.m_speed, Some(DesignSpeed::High));
        assert_eq!(back.appraisal_done, Tri::Yes);
        assert_eq!(back.gaps, vec![("to the junction".to_string(), "40".to_string())]);
        assert_eq!(back.pollution, None);
    }

    #[test]
    fn an_empty_form_enters_nothing() {
        let inputs = Form::default().inputs().unwrap();
        assert_eq!(inputs, CheckInputs::default());
    }

    #[test]
    fn entered_values_reach_the_inputs_and_bad_ones_are_named() {
        let mut f = Form::default();
        f.l_av = "1.5".into();
        f.design_speed_kph = "70".into();
        f.appraisal_done = Tri::Yes;
        let i = f.inputs().unwrap();
        assert_eq!(i.photometry.l_av_cd_m2, Some(1.5));
        assert_eq!(i.design_speed_kph, Some(70.0));
        assert_eq!(i.appraisal_done, Some(true));
        f.u_o = "abc".into();
        assert!(f.inputs().unwrap_err().contains("U_o"));
        f.u_o.clear();
        f.family = Family::M;
        assert!(f.inputs().unwrap_err().contains("Class selection"));
    }
}
