// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The survey database in the application, part one: the state, loading and saving, the Survey tab
//! of the toolspace, new and open, the database settings dialog, import of field data and the
//! import events. Part two (`sdb_ui2`) holds the observation editor, traverse, adjustment, figures
//! and the link to the drawing.

use std::path::PathBuf;

use afrobim_survey::*;
use std::result::Result;
use egui::RichText;

use crate::app::AfroBimApp;
use crate::survey_db as sd;
use crate::ui_kit::{self, Dialog, Footer, Level};

#[derive(Default)]
pub struct SdbUi {
    pub db: Option<SurveyDatabase>,
    pub path: Option<PathBuf>,
    /// The database is the synthetic one of the demonstration (never saved unless the user saves it).
    pub demo: bool,
    /// The stored path the open database was loaded for (so a document change loads its own).
    pub loaded_for: Option<String>,
    pub error: String,
    /// The network the dialogs start on.
    pub net: usize,
    pub settings: Option<SettingsForm>,
    pub import: Option<ImportForm>,
    pub events: Option<EventsForm>,
    pub obs: Option<crate::sdb_ui2::ObsForm>,
    pub trav: Option<crate::sdb_ui2::TravForm>,
    pub adjust: Option<crate::sdb_ui2::AdjustForm>,
    pub figures: Option<crate::sdb_ui2::FigForm>,
    pub insert: Option<crate::sdb_ui2::InsertForm>,
    pub update: Option<crate::sdb_ui2::UpdateForm>,
    pub apply: Option<crate::sdb_ui2::ApplyForm>,
    /// The last adjustment: network and result.
    pub result: Option<(String, sd::AdjOut)>,
}

pub fn parse_f(t: &str) -> Option<f64> {
    t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite())
}

pub fn fmt_opt(v: Option<f64>, d: usize) -> String {
    v.map(|v| format!("{v:.d$}")).unwrap_or_else(|| "-".into())
}

pub fn net_combo(ui: &mut egui::Ui, id: &str, db: &SurveyDatabase, sel: &mut usize) {
    let names: Vec<String> = db.networks.iter().map(|n| n.name.clone()).collect();
    if names.is_empty() {
        ui.label(RichText::new("(no network)").weak());
        return;
    }
    *sel = (*sel).min(names.len() - 1);
    ui_kit::combo_owned(ui, id, sel, &names);
}

impl AfroBimApp {
    /// Once a frame: loads the database the document names (when it is not the one open), and
    /// collects a finished adjustment.
    pub(crate) fn sdb_tick(&mut self) {
        let stored = sd::db_ref(&self.doc).map(|r| r.path);
        if stored != self.civil_ui.sdb.loaded_for && !(self.civil_ui.sdb.demo && stored.is_none()) {
            self.civil_ui.sdb.loaded_for = stored.clone();
            if let Some(p) = stored {
                let file = sd::resolve_path(self.doc_path(), &p);
                match sd::load_file(&file) {
                    Ok(db) => {
                        self.civil_ui.sdb.db = Some(db);
                        self.civil_ui.sdb.path = Some(file);
                        self.civil_ui.sdb.demo = false;
                        self.civil_ui.sdb.error.clear();
                    }
                    Err(e) => {
                        self.civil_ui.sdb.db = None;
                        self.civil_ui.sdb.path = Some(file);
                        self.civil_ui.sdb.error = format!("The survey database named in the document cannot be opened: {e}");
                    }
                }
            } else if !self.civil_ui.sdb.demo {
                self.civil_ui.sdb.db = None;
                self.civil_ui.sdb.path = None;
                self.civil_ui.sdb.error.clear();
            }
        }
        if let Some(polled) = crate::background::take(&mut self.bg.survey, &self.doc) {
            match polled {
                crate::background::Poll::Done(r) => match r {
                    Ok((net, out)) => {
                        self.status = match &out {
                            sd::AdjOut::Plan { adjustment, .. } => format!("Adjusted network {net}: {} observations, {} unknowns, {} degrees of freedom{}.", adjustment.n_observations, adjustment.n_unknowns, adjustment.dof, if adjustment.converged { "" } else { "; did NOT converge" }),
                            sd::AdjOut::Level { adjustment, .. } => format!("Adjusted levelling of {net}: {} observations, {} degrees of freedom.", adjustment.n_observations, adjustment.dof),
                        };
                        self.civil_ui.sdb.result = Some((net, out));
                    }
                    Err(e) => self.status = format!("The adjustment failed: {e}"),
                },
                other => self.status = crate::background::discard_message("Network adjustment", &other),
            }
        }
    }

    /// Writes the database to its file (a demonstration database has none until saved).
    pub(crate) fn sdb_save(&mut self) {
        let (Some(db), Some(path)) = (&self.civil_ui.sdb.db, self.civil_ui.sdb.path.clone()) else { return };
        if self.civil_ui.sdb.demo {
            return;
        }
        if let Err(e) = sd::save_file(db, &path) {
            self.status = format!("The survey database could not be saved: {e}");
        }
    }

    fn sdb_set_open(&mut self, db: SurveyDatabase, path: PathBuf) {
        let stored = sd::stored_path(self.doc_path(), &path);
        sd::set_db_ref(&mut self.doc, &sd::DbRef { path: stored.clone(), name: db.name.clone() });
        self.civil_ui.sdb.loaded_for = Some(stored);
        self.civil_ui.sdb.db = Some(db);
        self.civil_ui.sdb.path = Some(path);
        self.civil_ui.sdb.demo = false;
        self.civil_ui.sdb.error.clear();
        self.schedule_commit_now();
    }

    /// Installs the synthetic database of the demonstration (no file, nothing stored in the document).
    pub(crate) fn sdb_install_demo(&mut self) {
        self.civil_ui.sdb.db = Some(sd::demo_database(crate::civil_demo::E0, crate::civil_demo::N0));
        self.civil_ui.sdb.path = None;
        self.civil_ui.sdb.demo = true;
        self.civil_ui.sdb.loaded_for = None;
        // The demonstration also holds an adjustment made with EXAMPLE options, so that the results,
        // the ellipses and SURVEYAPPLY have something to show.
        if let Some(db) = &self.civil_ui.sdb.db {
            let options = afrobim_survey::AdjustOptions::new(20, 1e-10, 0.95, 0.05, afrobim_survey::normal_two_sided_critical(0.05));
            let req = sd::AdjRequest { db: db.clone(), network: "Main".into(), mode: sd::AdjMode::Plan2d, options, snoop_max: None };
            if let Ok(out) = sd::run_adjustment(&req) {
                self.civil_ui.sdb.result = Some(("Main".into(), out));
            }
        }
    }

    pub(crate) fn sdb_cmd(&mut self, cv: crate::civil_cmds::Cv) {
        use crate::civil_cmds::Cv::*;
        match cv {
            SurveyNew => self.sdb_new(),
            SurveyOpen => self.sdb_open(),
            _ => {}
        }
        if self.civil_ui.sdb.db.is_none() && !matches!(cv, SurveyNew | SurveyOpen) {
            self.status = "No survey database is open. Make one with SURVEYNEW or open one with SURVEYOPEN.".into();
            return;
        }
        let db = self.civil_ui.sdb.db.clone();
        let Some(db) = db else { return };
        match cv {
            SurveySettings => self.civil_ui.sdb.settings = Some(SettingsForm::new(&db)),
            SurveyImport => self.civil_ui.sdb.import = Some(ImportForm::new(&db, self.options.civil_demo)),
            SurveyEvents => self.civil_ui.sdb.events = Some(EventsForm { selected: 0, confirm_remove: false, message: String::new() }),
            SurveyObs => self.civil_ui.sdb.obs = Some(crate::sdb_ui2::ObsForm::new(&db, self.civil_ui.sdb.net)),
            SurveyTraverse => self.civil_ui.sdb.trav = Some(crate::sdb_ui2::TravForm::new(&db, self.civil_ui.sdb.net)),
            SurveyAdjust => self.civil_ui.sdb.adjust = Some(crate::sdb_ui2::AdjustForm::new(&db, self.civil_ui.sdb.net, self.options.civil_demo)),
            SurveyApply => match &self.civil_ui.sdb.result {
                Some((net, out)) => self.civil_ui.sdb.apply = Some(crate::sdb_ui2::ApplyForm::new(net.clone(), out.clone())),
                None => self.status = "There is no adjustment to apply yet: run SURVEYADJUST first.".into(),
            },
            SurveyFigures => self.civil_ui.sdb.figures = Some(crate::sdb_ui2::FigForm::new(&db, self.civil_ui.sdb.net)),
            SurveyInsert => self.civil_ui.sdb.insert = Some(crate::sdb_ui2::InsertForm::new(&db, self.civil_ui.sdb.net)),
            SurveyUpdate => self.civil_ui.sdb.update = Some(crate::sdb_ui2::UpdateForm::default()),
            _ => {}
        }
    }

    fn sdb_new(&mut self) {
        let dir = self.doc_path().and_then(|p| p.parent()).map(|p| p.to_path_buf());
        let mut dlg = rfd::FileDialog::new().add_filter("Survey database", &["afrosurvey", "json"]).set_file_name("survey.afrosurvey");
        if let Some(d) = dir {
            dlg = dlg.set_directory(d);
        }
        let Some(path) = dlg.save_file() else { return };
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Survey".into());
        let mut db = SurveyDatabase::new(&name);
        let _ = db.add_network("Main", &crate::transform_doc::now_iso());
        match sd::save_file(&db, &path) {
            Ok(()) => {
                self.status = format!("Made the survey database {name}: {}. Its path is stored in the document.", path.display());
                self.sdb_set_open(db, path);
            }
            Err(e) => self.status = format!("The survey database could not be made: {e}"),
        }
    }

    fn sdb_open(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Survey database", &["afrosurvey", "json"]).pick_file() else { return };
        match sd::load_file(&path) {
            Ok(db) => {
                self.status = format!("Opened the survey database {}: {} network(s). Its path is stored in the document.", db.name, db.networks.len());
                self.sdb_set_open(db, path);
            }
            Err(e) => self.status = format!("The survey database cannot be opened: {e}"),
        }
    }

    pub(crate) fn sdb_windows(&mut self, ctx: &egui::Context) {
        self.sdb_tick();
        self.sdb_settings_window(ctx);
        self.sdb_import_window(ctx);
        self.sdb_events_window(ctx);
        self.sdb_obs_window(ctx);
        self.sdb_trav_window(ctx);
        self.sdb_adjust_window(ctx);
        self.sdb_apply_window(ctx);
        self.sdb_figures_window(ctx);
        self.sdb_insert_window(ctx);
        self.sdb_update_window(ctx);
    }

    /// The Survey tab of the toolspace: the open database as a tree.
    pub(crate) fn sdb_tab(&mut self, ui: &mut egui::Ui) {
        use crate::civil_cmds::Cv;
        use egui_phosphor::regular as ph;
        let mut run: Option<Cv> = None;
        ui.horizontal_wrapped(|ui| {
            for (cv, glyph, tip) in [(Cv::SurveyNew, ph::FILE_PLUS, "New survey database"), (Cv::SurveyOpen, ph::FOLDER_OPEN, "Open survey database"), (Cv::SurveySettings, ph::SLIDERS_HORIZONTAL, "Database settings"), (Cv::SurveyImport, ph::DOWNLOAD_SIMPLE, "Import field data"), (Cv::SurveyAdjust, ph::CHART_SCATTER, "Adjust network")] {
                if ui_kit::icon_button(ui, glyph, tip).clicked() {
                    run = Some(cv);
                }
            }
        });
        ui.separator();
        let s = &self.civil_ui.sdb;
        if !s.error.is_empty() {
            ui_kit::callout(ui, Level::Error, s.error.clone());
        }
        match &s.db {
            None => {
                ui_kit::note(ui, "No survey database is open. The database is a file beside the project; its path is stored in the document.");
            }
            Some(db) => {
                ui.label(RichText::new(&db.name).strong());
                ui_kit::note(ui, match (&s.path, s.demo) {
                    (_, true) => sd::DEMO_LABEL.to_string(),
                    (Some(p), _) => p.display().to_string(),
                    _ => "not saved".into(),
                });
                let text = ui.visuals().text_color();
                egui::CollapsingHeader::new(crate::theme::icon_text(ph::SLIDERS_HORIZONTAL, "Settings", 13.0, text)).id_salt("sdb-settings").show(ui, |ui| {
                    let c = &db.settings.corrections;
                    ui_kit::note(ui, format!("Angles {:?}, {:?}, vertical {:?}; coordinate system: {}", db.settings.angle_unit, db.settings.hz_direction, db.settings.vertical_kind, if db.settings.coordinate_system.is_empty() { "not stated" } else { &db.settings.coordinate_system }));
                    ui_kit::note(ui, format!("Corrections on: {}", [(c.prism_constant_m.is_some(), "prism constant"), (c.atmospheric_ppm.is_some(), "atmospheric ppm"), (c.curvature_refraction.is_some(), "curvature and refraction"), (c.sea_level.is_some(), "sea level"), (c.grid_scale_factor.is_some(), "grid scale")].iter().filter(|x| x.0).map(|x| x.1).collect::<Vec<_>>().join(", ")));
                });
                for (ni, net) in db.networks.iter().enumerate() {
                    egui::CollapsingHeader::new(crate::theme::icon_text(ph::GRAPH, &format!("{} ({} points)", net.name, net.points.len()), 13.0, text)).id_salt(("sdb-net", ni)).default_open(true).show(ui, |ui| {
                        let ctrl = net.points.iter().filter(|p| p.control).count();
                        egui::CollapsingHeader::new(format!("Control points ({ctrl})")).id_salt(("sdb-ctl", ni)).show(ui, |ui| {
                            for p in net.points.iter().filter(|p| p.control) {
                                ui.label(RichText::new(format!("{}  E {}  N {}  Z {}", p.name, fmt_opt(p.e, 3), fmt_opt(p.n, 3), fmt_opt(p.z, 3))).monospace().size(11.0));
                            }
                        });
                        egui::CollapsingHeader::new(format!("Other points ({})", net.points.len() - ctrl)).id_salt(("sdb-pts", ni)).show(ui, |ui| {
                            for p in net.points.iter().filter(|p| !p.control).take(300) {
                                ui.label(RichText::new(format!("{}  E {}  N {}  {}", p.name, fmt_opt(p.e, 3), fmt_opt(p.n, 3), p.description)).monospace().size(11.0));
                            }
                        });
                        egui::CollapsingHeader::new(format!("Setups ({})", net.setups.len())).id_salt(("sdb-set", ni)).show(ui, |ui| {
                            for s in &net.setups {
                                egui::CollapsingHeader::new(format!("{}  ({} observations)", s.station, s.observations.len())).id_salt(("sdb-su", ni, s.id)).show(ui, |ui| {
                                    for o in &s.observations {
                                        ui.label(RichText::new(format!("-> {}  Hz {}  SD {}", o.target, fmt_opt(o.hz_deg, 4), fmt_opt(o.slope_distance.or(o.horizontal_distance), 3))).monospace().size(11.0));
                                    }
                                });
                            }
                        });
                        if !net.level_runs.is_empty() || !net.baselines.is_empty() || !net.directions.is_empty() {
                            ui_kit::note(ui, format!("{} level run(s), {} baseline(s), {} known direction(s)", net.level_runs.len(), net.baselines.len(), net.directions.len()));
                        }
                        egui::CollapsingHeader::new(format!("Figures ({})", net.figures.len())).id_salt(("sdb-fig", ni)).show(ui, |ui| {
                            for f in &net.figures {
                                ui.label(format!("{}  {} vertices{}{}", f.name, f.vertices.len(), if f.closed { ", closed" } else { "" }, if f.breakline { ", breakline" } else { "" }));
                            }
                        });
                    });
                }
                egui::CollapsingHeader::new(crate::theme::icon_text(ph::CLOCK_COUNTER_CLOCKWISE, &format!("Import events ({})", db.import_events.len()), 13.0, text)).id_salt("sdb-ev").show(ui, |ui| {
                    for e in &db.import_events {
                        ui.label(format!("{}  {}  ({} points, {} observations)", e.id, e.file_name, e.added.points.len(), e.added.observations));
                    }
                });
                egui::CollapsingHeader::new(crate::theme::icon_text(ph::WRENCH, &format!("Equipment ({})", db.equipment.len()), 13.0, text)).id_salt("sdb-eq").show(ui, |ui| {
                    for e in &db.equipment {
                        ui.label(&e.name);
                    }
                });
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for (cv, l) in [(Cv::SurveyObs, "Observations..."), (Cv::SurveyTraverse, "Traverse..."), (Cv::SurveyFigures, "Figures..."), (Cv::SurveyInsert, "Insert points..."), (Cv::SurveyUpdate, "Update..."), (Cv::SurveyEvents, "Events...")] {
                        if ui.small_button(l).clicked() {
                            run = Some(cv);
                        }
                    }
                });
            }
        }
        if let Some(cv) = run {
            self.civil_run_pending = Some(cv);
        }
    }
}

// ─── settings ───────────────────────────────────────────────────────────

/// Equipment as text (every number is the user's input).
#[derive(Clone, Default)]
pub struct EqText {
    name: String,
    v: [String; 11],
    colim: String,
    index: String,
}

const EQ_FIELDS: [(&str, &str); 11] = [
    ("Distance, constant part", "m"),
    ("Distance, ppm part", "ppm"),
    ("Pointing", "arc-seconds"),
    ("Circle reading", "arc-seconds"),
    ("Zenith reading", "arc-seconds"),
    ("Centring, instrument", "m"),
    ("Centring, target", "m"),
    ("Instrument height", "m"),
    ("Target height", "m"),
    ("Levelling, constant part", "m"),
    ("Levelling, per square root km", "m"),
];

impl EqText {
    fn from(e: &Equipment) -> EqText {
        let n = |x: f64| format!("{x}");
        EqText {
            name: e.name.clone(),
            v: [n(e.sigma_distance_const_m), n(e.sigma_distance_ppm), n(e.sigma_pointing_arcsec), n(e.sigma_reading_arcsec), n(e.sigma_zenith_arcsec), n(e.centring_instrument_m), n(e.centring_target_m), n(e.sigma_instrument_height_m), n(e.sigma_target_height_m), n(e.level_sigma_const_m), n(e.level_sigma_per_sqrt_km_m)],
            colim: e.horizontal_collimation_deg.map(n).unwrap_or_default(),
            index: e.vertical_index_error_deg.map(n).unwrap_or_default(),
        }
    }

    fn to_equipment(&self) -> Result<Equipment, String> {
        if self.name.trim().is_empty() {
            return Err("Name the equipment.".into());
        }
        let mut e = Equipment::exact(self.name.trim());
        let mut vals = [0.0; 11];
        for (i, (label, _)) in EQ_FIELDS.iter().enumerate() {
            vals[i] = parse_f(&self.v[i]).filter(|v| *v >= 0.0).ok_or_else(|| format!("{}: {label} must be a number of zero or more (your input: it has no default).", self.name))?;
        }
        e.sigma_distance_const_m = vals[0];
        e.sigma_distance_ppm = vals[1];
        e.sigma_pointing_arcsec = vals[2];
        e.sigma_reading_arcsec = vals[3];
        e.sigma_zenith_arcsec = vals[4];
        e.centring_instrument_m = vals[5];
        e.centring_target_m = vals[6];
        e.sigma_instrument_height_m = vals[7];
        e.sigma_target_height_m = vals[8];
        e.level_sigma_const_m = vals[9];
        e.level_sigma_per_sqrt_km_m = vals[10];
        e.horizontal_collimation_deg = if self.colim.trim().is_empty() { None } else { Some(parse_f(&self.colim).ok_or("The collimation error is not a number.")?) };
        e.vertical_index_error_deg = if self.index.trim().is_empty() { None } else { Some(parse_f(&self.index).ok_or("The index error is not a number.")?) };
        Ok(e)
    }
}

pub struct SettingsForm {
    tab: usize,
    name: String,
    angle: usize,
    hzdir: usize,
    vert: usize,
    cs: String,
    epsg: String,
    // corrections: (enabled, text...)
    prism: (bool, String),
    ppm: (bool, String),
    cr: (bool, String, String),
    sea: (bool, String),
    grid: (bool, String),
    eq: Vec<EqText>,
    eq_sel: usize,
    message: String,
}

const ANGLES: [(AngleUnit, &str); 4] = [(AngleUnit::Degrees, "Decimal degrees"), (AngleUnit::Gon, "Gon"), (AngleUnit::DmsText, "Degrees-minutes-seconds"), (AngleUnit::DmsPacked, "Packed DDD.MMSS")];
const VERTS: [(VerticalKind, &str); 3] = [(VerticalKind::Zenith, "Zenith angle"), (VerticalKind::Elevation, "Elevation angle"), (VerticalKind::Nadir, "Nadir angle")];

impl SettingsForm {
    fn new(db: &SurveyDatabase) -> SettingsForm {
        let s = &db.settings;
        let c = &s.corrections;
        let t = |v: f64| format!("{v}");
        SettingsForm {
            tab: 0,
            name: db.name.clone(),
            angle: ANGLES.iter().position(|a| a.0 == s.angle_unit).unwrap_or(0),
            hzdir: usize::from(s.hz_direction == HzDirection::Left),
            vert: VERTS.iter().position(|a| a.0 == s.vertical_kind).unwrap_or(0),
            cs: s.coordinate_system.clone(),
            epsg: s.epsg.map(|e| e.to_string()).unwrap_or_default(),
            prism: (c.prism_constant_m.is_some(), c.prism_constant_m.map(t).unwrap_or_default()),
            ppm: (c.atmospheric_ppm.is_some(), c.atmospheric_ppm.map(t).unwrap_or_default()),
            cr: (c.curvature_refraction.is_some(), c.curvature_refraction.map(|x| t(x.earth_radius_m)).unwrap_or_default(), c.curvature_refraction.map(|x| t(x.refraction_coefficient)).unwrap_or_default()),
            sea: (c.sea_level.is_some(), c.sea_level.map(|x| t(x.earth_radius_m)).unwrap_or_default()),
            grid: (c.grid_scale_factor.is_some(), c.grid_scale_factor.map(t).unwrap_or_default()),
            eq: db.equipment.iter().map(EqText::from).collect(),
            eq_sel: 0,
            message: String::new(),
        }
    }

    /// The settings and equipment the form describes, or what is missing.
    fn build(&self) -> Result<(Settings, Vec<Equipment>), String> {
        let need = |what: &str, t: &str| parse_f(t).ok_or_else(|| format!("{what} is an input and is not a number."));
        let mut s = Settings::metric();
        s.angle_unit = ANGLES[self.angle].0;
        s.hz_direction = if self.hzdir == 1 { HzDirection::Left } else { HzDirection::Right };
        s.vertical_kind = VERTS[self.vert].0;
        s.coordinate_system = self.cs.trim().to_string();
        s.epsg = if self.epsg.trim().is_empty() { None } else { Some(self.epsg.trim().trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().map_err(|_| "The EPSG code must be a number.")?) };
        let c = &mut s.corrections;
        if self.prism.0 {
            c.prism_constant_m = Some(need("The prism constant", &self.prism.1)?);
        }
        if self.ppm.0 {
            c.atmospheric_ppm = Some(need("The atmospheric correction", &self.ppm.1)?);
        }
        if self.cr.0 {
            c.curvature_refraction = Some(CurvatureRefraction { earth_radius_m: need("The earth radius for curvature and refraction", &self.cr.1)?, refraction_coefficient: need("The refraction coefficient", &self.cr.2)? });
        }
        if self.sea.0 {
            c.sea_level = Some(SeaLevel { earth_radius_m: need("The earth radius for the sea level reduction", &self.sea.1)? });
        }
        if self.grid.0 {
            c.grid_scale_factor = Some(need("The grid scale factor", &self.grid.1)?);
        }
        let eq = self.eq.iter().map(EqText::to_equipment).collect::<Result<Vec<_>, _>>()?;
        Ok((s, eq))
    }
}

impl AfroBimApp {
    fn sdb_settings_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.sdb.settings.take() else { return };
        let mut go = false;
        let demo = self.civil_ui.sdb.demo;
        let out = Dialog::new("Survey database settings", crate::glyphs::by_name("SURVEYSETTINGS")).width(680.0).resizable(true).max_body_height(620.0).help("Units, the corrections applied to the readings and the equipment standard errors. Every correction value and every standard error is YOUR input: nothing has a default, because the right value belongs to the instrument, the day and the project. A correction that is ticked needs its numbers.").show(
            ctx,
            |ui| {
                ui_kit::tabs(ui, &mut f.tab, &["Units and system", "Corrections", "Equipment"]);
                match f.tab {
                    0 => {
                        ui_kit::form(ui, "sdbs-u", |fm| {
                            fm.row("Database name", |ui| {
                                ui_kit::text_field(ui, &mut f.name, "");
                            });
                            fm.row("Angle unit", |ui| {
                                ui_kit::combo(ui, "sdbs-ang", &mut f.angle, &ANGLES.map(|a| a.1));
                            });
                            fm.row_note("Horizontal angles increase", "clockwise is the usual setting", |ui| {
                                ui_kit::segmented(ui, &mut f.hzdir, &["Clockwise", "Anticlockwise"]);
                            });
                            fm.row("Vertical angle", |ui| {
                                ui_kit::combo(ui, "sdbs-vert", &mut f.vert, &VERTS.map(|a| a.1));
                            });
                            fm.row_note("Coordinate system", "free text; the system of the coordinates the database holds", |ui| {
                                ui_kit::text_field(ui, &mut f.cs, "for example Arc 1960 / UTM zone 36S");
                            });
                            fm.row("EPSG code", |ui| {
                                ui_kit::text_field(ui, &mut f.epsg, "optional");
                            });
                        });
                        ui_kit::note(ui, "Distances are kept in metres; other units enter only through the unit chosen at import.");
                    }
                    1 => {
                        ui_kit::note(ui, "Applied in this order: prism constant, atmospheric ppm, face reduction, slope to horizontal, curvature and refraction, sea level, grid scale. Tick only what you want applied, and type its values.");
                        ui_kit::form(ui, "sdbs-c", |fm| {
                            fm.row("Prism constant", |ui| {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut f.prism.0, "");
                                    ui.add_enabled_ui(f.prism.0, |ui| crate::civil_dlg::num(ui, &mut f.prism.1, "m"));
                                });
                            });
                            fm.row("Atmospheric correction", |ui| {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut f.ppm.0, "");
                                    ui.add_enabled_ui(f.ppm.0, |ui| crate::civil_dlg::num(ui, &mut f.ppm.1, "ppm"));
                                });
                            });
                            fm.row("Curvature and refraction", |ui| {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut f.cr.0, "");
                                    ui.add_enabled_ui(f.cr.0, |ui| {
                                        crate::civil_dlg::num(ui, &mut f.cr.1, "earth radius m");
                                        crate::civil_dlg::num(ui, &mut f.cr.2, "coefficient k");
                                    });
                                });
                            });
                            fm.row("Sea level reduction", |ui| {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut f.sea.0, "");
                                    ui.add_enabled_ui(f.sea.0, |ui| crate::civil_dlg::num(ui, &mut f.sea.1, "earth radius m"));
                                });
                            });
                            fm.row("Grid scale factor", |ui| {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut f.grid.0, "");
                                    ui.add_enabled_ui(f.grid.0, |ui| crate::civil_dlg::num(ui, &mut f.grid.1, ""));
                                });
                            });
                        });
                    }
                    _ => {
                        ui_kit::note(ui, "Standard errors weight the observations in the adjustment. Take them from the instrument's calibration or its documentation: they are inputs, and zero means noise-free (for simulations).");
                        if demo {
                            ui_kit::callout(ui, Level::Info, "The demonstration equipment holds EXAMPLE values, not an instrument's specification.");
                        }
                        ui.horizontal(|ui| {
                            let names: Vec<String> = f.eq.iter().map(|e| e.name.clone()).collect();
                            if !names.is_empty() {
                                f.eq_sel = f.eq_sel.min(names.len() - 1);
                                ui_kit::combo_owned(ui, "sdbs-eq", &mut f.eq_sel, &names);
                            }
                            if ui.button("Add equipment").clicked() {
                                f.eq.push(EqText { name: format!("Equipment {}", f.eq.len() + 1), ..Default::default() });
                                f.eq_sel = f.eq.len() - 1;
                            }
                            if ui.button("Remove").clicked() && !f.eq.is_empty() {
                                f.eq.remove(f.eq_sel);
                                f.eq_sel = 0;
                            }
                        });
                        let sel = f.eq_sel;
                        if let Some(e) = f.eq.get_mut(sel) {
                            ui_kit::form(ui, "sdbs-e", |fm| {
                                fm.row("Name", |ui| {
                                    ui_kit::text_field(ui, &mut e.name, "");
                                });
                                for (i, (label, unit)) in EQ_FIELDS.iter().enumerate() {
                                    fm.row(label, |ui| {
                                        crate::civil_dlg::num(ui, &mut e.v[i], unit);
                                    });
                                }
                                fm.row_note("Collimation error", "optional, degrees", |ui| {
                                    crate::civil_dlg::num_opt(ui, &mut e.colim, "deg").ok();
                                });
                                fm.row_note("Vertical index error", "optional, degrees", |ui| {
                                    crate::civil_dlg::num_opt(ui, &mut e.index, "deg").ok();
                                });
                            });
                        }
                    }
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Error, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new("Apply").icon(crate::glyphs::by_name("SURVEYSETTINGS")).show(ui);
                go = o.primary;
                o
            },
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if go {
            match f.build() {
                Ok((s, eq)) => {
                    if let Some(db) = &mut self.civil_ui.sdb.db {
                        db.name = f.name.trim().to_string();
                        db.settings = s;
                        db.equipment = eq;
                        db.log(&crate::transform_doc::now_iso(), "settings", "changed in the settings dialog");
                    }
                    self.sdb_save();
                    self.status = "Survey database settings applied.".into();
                    return;
                }
                Err(e) => f.message = e,
            }
        }
        self.civil_ui.sdb.settings = Some(f);
    }
}

// ─── import ─────────────────────────────────────────────────────────────

const FORMATS: [&str; 5] = ["AfroBIM field book", "Civil 3D field book (FBK subset)", "Leica GSI", "Topcon GTS-7 (structure only)", "Delimited table (CSV) with column mapping"];
const DIST_UNITS: [(&str, f64); 3] = [("Metres", 1.0), ("International feet", 0.3048), ("US survey feet", 1200.0 / 3937.0)];
const CSV_ROLES: [(CsvRole, &str); 13] = [
    (CsvRole::Station, "Station"),
    (CsvRole::InstrumentHeight, "Instrument height"),
    (CsvRole::Backsight, "Backsight"),
    (CsvRole::BacksightReading, "Backsight reading"),
    (CsvRole::Target, "Target"),
    (CsvRole::Hz, "Horizontal angle"),
    (CsvRole::Vertical, "Vertical angle"),
    (CsvRole::SlopeDistance, "Slope distance"),
    (CsvRole::HorizontalDistance, "Horizontal distance"),
    (CsvRole::VerticalDistance, "Vertical distance"),
    (CsvRole::TargetHeight, "Target height"),
    (CsvRole::Description, "Description"),
    (CsvRole::Face, "Face"),
];

pub struct ImportForm {
    file: String,
    text: String,
    fmt: usize,
    angle: usize,
    vert: usize,
    unit: usize,
    gsi_feet: bool,
    code_follows: bool,
    bs_rule: usize,
    bs_name: String,
    csv_delim: usize,
    csv_header: bool,
    /// For each CSV role: 0 none, else column number from 1.
    csv_cols: [usize; 13],
    strict: bool,
    network: usize,
    new_name: String,
    dup: usize,
    keep_source: bool,
    description: String,
    key: String,
    reading: Option<sd::Reading>,
    message: String,
    example: bool,
}

impl ImportForm {
    fn new(db: &SurveyDatabase, example: bool) -> ImportForm {
        let s = &db.settings;
        ImportForm {
            file: String::new(),
            text: String::new(),
            fmt: 0,
            angle: ANGLES.iter().position(|a| a.0 == s.angle_unit).unwrap_or(0),
            vert: VERTS.iter().position(|a| a.0 == s.vertical_kind).unwrap_or(0),
            unit: 0,
            gsi_feet: false,
            code_follows: false,
            bs_rule: 0,
            bs_name: String::new(),
            csv_delim: 0,
            csv_header: true,
            csv_cols: [0; 13],
            strict: true,
            network: 1,
            new_name: String::new(),
            dup: 0,
            keep_source: true,
            description: String::new(),
            key: String::new(),
            reading: None,
            message: String::new(),
            example,
        }
    }

    fn headers(&self) -> Vec<String> {
        let first = self.text.lines().next().unwrap_or("");
        let d = [',', '\t', ';', ' '][self.csv_delim.min(3)];
        first.split(d).map(|s| s.trim().trim_matches('"').to_string()).collect()
    }

    fn guess_columns(&mut self) {
        let h = self.headers();
        for (i, (_, name)) in CSV_ROLES.iter().enumerate() {
            let want = name.to_ascii_lowercase().replace(' ', "");
            self.csv_cols[i] = h.iter().position(|c| c.to_ascii_lowercase().replace([' ', '_'], "") == want || (i == 5 && c.eq_ignore_ascii_case("hz"))).map(|p| p + 1).unwrap_or(0);
        }
    }

    fn format(&self) -> Result<ImportFormat, String> {
        let (angle, vert, unit) = (ANGLES[self.angle].0, VERTS[self.vert].0, DIST_UNITS[self.unit].1);
        Ok(match self.fmt {
            0 => ImportFormat::Fieldbook,
            1 => ImportFormat::Fbk(FbkOptions { angle_unit: angle, vertical_kind: vert, distance_factor_m: unit, strict: self.strict }),
            2 => ImportFormat::Gsi(GsiOptions {
                feet_to_m: self.gsi_feet.then_some(DIST_UNITS[1].1),
                vertical_kind: vert,
                code_follows_measurement: self.code_follows,
                backsight: match self.bs_rule {
                    0 => GsiBacksight::None,
                    1 => GsiBacksight::FirstObservation,
                    _ => GsiBacksight::Named(self.bs_name.trim().to_string()),
                },
            }),
            3 => ImportFormat::Topcon(TopconOptions { angle_unit: angle, vertical_kind: vert, distance_factor_m: unit }),
            _ => {
                let mut columns = Vec::new();
                for (i, (role, _)) in CSV_ROLES.iter().enumerate() {
                    if self.csv_cols[i] > 0 {
                        columns.push(CsvColumn { role: *role, index: self.csv_cols[i] - 1 });
                    }
                }
                ImportFormat::Csv(CsvMapping { delimiter: [',', '\t', ';', ' '][self.csv_delim.min(3)], has_header: self.csv_header, columns, angle_unit: angle, vertical_kind: vert, distance_factor_m: unit })
            }
        })
    }

    fn key_now(&self) -> String {
        format!("{}|{}|{}|{}|{}|{}|{}|{}|{:?}|{}|{}|{}", self.text.len(), self.fmt, self.angle, self.vert, self.unit, self.gsi_feet, self.code_follows, self.bs_rule, self.csv_cols, self.csv_delim, self.csv_header, self.strict) + &self.bs_name
    }
}

impl AfroBimApp {
    fn sdb_import_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.sdb.import.take() else { return };
        let Some(db) = self.civil_ui.sdb.db.clone() else { return };
        // The demonstration offers its own field book as the file to read.
        if f.text.is_empty() && f.file.is_empty() && f.example {
            if let Some(ev) = db.import_events.first().and_then(|e| e.source_text.clone()) {
                f.text = ev;
                f.file = "demo.afrofb (the demonstration's own field book)".into();
                f.dup = 1;
            }
        }
        let now = f.key_now();
        if f.key != now && !f.text.is_empty() {
            f.key = now;
            match f.format() {
                Ok(fmt) => f.reading = Some(sd::read_text(&fmt, &f.text, f.strict)),
                Err(e) => f.message = e,
            }
        }
        let mut pick = false;
        let mut go = false;
        let ready = f.reading.as_ref().is_some_and(|r| r.data.is_some());
        let out = Dialog::new("Import field data", crate::glyphs::by_name("SURVEYIMPORT")).width(760.0).resizable(true).max_body_height(640.0).help("Reads a field file into a network of the database. Nothing is added until you press Import. The preview shows what would be added; every error carries its line number. Strict stops at the first error; lenient leaves the bad lines out and lists them all. The import is recorded as an event that can be removed or reprocessed later.").show(
            ctx,
            |ui| {
                ui_kit::section(ui, "File", |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Choose a file...").clicked() {
                            pick = true;
                        }
                        ui.label(if f.file.is_empty() { "(none)".to_string() } else { f.file.clone() });
                    });
                    if f.example && !f.text.is_empty() {
                        ui_kit::note(ui, sd::DEMO_LABEL);
                    }
                });
                ui_kit::section(ui, "Format", |ui| {
                    ui_kit::form(ui, "sdbi-f", |fm| {
                        fm.row("Format", |ui| {
                            ui_kit::combo(ui, "sdbi-fmt", &mut f.fmt, &FORMATS);
                        });
                        if matches!(f.fmt, 1 | 3 | 4) {
                            fm.row("Angle unit", |ui| {
                                ui_kit::combo(ui, "sdbi-ang", &mut f.angle, &ANGLES.map(|a| a.1));
                            });
                        }
                        if f.fmt != 0 {
                            fm.row("Vertical angle", |ui| {
                                ui_kit::combo(ui, "sdbi-vert", &mut f.vert, &VERTS.map(|a| a.1));
                            });
                        }
                        if matches!(f.fmt, 1 | 3 | 4) {
                            fm.row("Distance unit", |ui| {
                                ui_kit::combo(ui, "sdbi-unit", &mut f.unit, &DIST_UNITS.map(|a| a.0));
                            });
                        }
                        if f.fmt == 2 {
                            fm.row("Distances in feet", |ui| {
                                ui.checkbox(&mut f.gsi_feet, "International feet");
                            });
                            fm.row("Code follows the measurement", |ui| {
                                ui.checkbox(&mut f.code_follows, "");
                            });
                            fm.row_note("Backsight", "a GSI file does not say which target is the backsight", |ui| {
                                ui.vertical(|ui| {
                                    ui_kit::radio_group(ui, &mut f.bs_rule, &["None", "The first observation", "A named point"]);
                                    if f.bs_rule == 2 {
                                        ui_kit::text_field(ui, &mut f.bs_name, "point name");
                                    }
                                });
                            });
                        }
                        fm.row("Reading", |ui| {
                            let mut s = usize::from(!f.strict);
                            ui_kit::segmented(ui, &mut s, &["Strict", "Lenient"]);
                            f.strict = s == 0;
                        });
                    });
                    if f.fmt == 4 {
                        ui_kit::form(ui, "sdbi-csv", |fm| {
                            fm.row("Delimiter", |ui| {
                                ui_kit::combo(ui, "sdbi-dl", &mut f.csv_delim, &["Comma", "Tab", "Semicolon", "Space"]);
                            });
                            fm.row("First line is a header", |ui| {
                                ui.checkbox(&mut f.csv_header, "");
                            });
                        });
                        let h = f.headers();
                        if ui.button("Guess the columns from the header").clicked() {
                            f.guess_columns();
                        }
                        let mut items: Vec<String> = vec!["(not in the file)".into()];
                        items.extend(h.iter().enumerate().map(|(i, c)| format!("{}: {c}", i + 1)));
                        egui::Grid::new("sdbi-map").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                            for (i, (_, label)) in CSV_ROLES.iter().enumerate() {
                                ui.label(*label);
                                f.csv_cols[i] = f.csv_cols[i].min(items.len() - 1);
                                ui_kit::combo_owned(ui, &format!("sdbi-col{i}"), &mut f.csv_cols[i], &items);
                                ui.end_row();
                            }
                        });
                    }
                });
                ui_kit::section(ui, "Preview", |ui| match &f.reading {
                    None => ui_kit::note(ui, "Choose a file to see what it would add."),
                    Some(r) => {
                        if let Some(d) = &r.data {
                            for l in sd::preview_lines(d) {
                                ui.label(l);
                            }
                            let rows: Vec<Vec<String>> = d.points.iter().take(8).map(|p| vec![p.name.clone(), fmt_opt(p.e, 3), fmt_opt(p.n, 3), fmt_opt(p.z, 3), p.description.clone()]).collect();
                            if !rows.is_empty() {
                                ui_kit::results_table(ui, "sdbi-prev", &["Point", "Easting", "Northing", "Elevation", "Description"], &[ui_kit::Col::Text, ui_kit::Col::Num, ui_kit::Col::Num, ui_kit::Col::Num, ui_kit::Col::Text], &rows);
                            }
                        }
                        if !r.dropped.is_empty() {
                            ui_kit::callout(ui, Level::Warning, format!("Lenient reading left out {} line(s): {}.", r.dropped.len(), r.dropped.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", ")));
                        }
                        for e in r.errors.iter().take(12) {
                            ui_kit::callout(ui, if r.data.is_some() { Level::Warning } else { Level::Error }, e.clone());
                        }
                        if r.errors.len() > 12 {
                            ui_kit::note(ui, format!("{} more error(s) not shown.", r.errors.len() - 12));
                        }
                    }
                });
                ui_kit::section(ui, "Into", |ui| {
                    ui_kit::form(ui, "sdbi-into", |fm| {
                        fm.row("Network", |ui| {
                            let mut items: Vec<String> = vec!["(new network)".into()];
                            items.extend(db.networks.iter().map(|n| n.name.clone()));
                            f.network = f.network.min(items.len() - 1);
                            ui_kit::combo_owned(ui, "sdbi-net", &mut f.network, &items);
                        });
                        if f.network == 0 {
                            fm.row("New network name", |ui| {
                                ui_kit::text_field(ui, &mut f.new_name, "");
                            });
                        }
                        fm.row("A point name already there", |ui| {
                            ui_kit::radio_group(ui, &mut f.dup, &["Refuse the import", "Keep the existing point", "Replace it (the change log keeps the old values)"]);
                        });
                        fm.row_note("Keep the source text", "needed to reprocess the event later", |ui| {
                            ui.checkbox(&mut f.keep_source, "");
                        });
                        fm.row("Description", |ui| {
                            ui_kit::text_field(ui, &mut f.description, "");
                        });
                    });
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Error, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new("Import").icon(crate::glyphs::by_name("SURVEYIMPORT")).enabled(ready, "Choose a file that reads").show(ui);
                go = o.primary;
                o
            },
        );
        if pick {
            if let Some(path) = rfd::FileDialog::new().add_filter("Field data", &["txt", "csv", "fbk", "gsi", "afrofb", "raw", "tsv"]).add_filter("All files", &["*"]).pick_file() {
                match std::fs::read_to_string(&path) {
                    Ok(t) => {
                        f.text = t;
                        f.file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        f.key.clear();
                        if f.fmt == 4 {
                            f.guess_columns();
                        }
                    }
                    Err(e) => f.message = format!("The file cannot be read: {e}"),
                }
            }
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if go {
            let net = if f.network == 0 { f.new_name.trim().to_string() } else { db.networks.get(f.network - 1).map(|n| n.name.clone()).unwrap_or_default() };
            if net.is_empty() {
                f.message = "Name the new network.".into();
            } else {
                let policy = [DuplicatePolicy::Error, DuplicatePolicy::Skip, DuplicatePolicy::Replace][f.dup.min(2)];
                let time = crate::transform_doc::now_iso();
                let result = f.format().and_then(|fmt| {
                    // Lenient reading imports the text with the bad lines left out.
                    let text = match &f.reading {
                        Some(r) if !r.dropped.is_empty() => f.text.lines().enumerate().map(|(i, l)| if r.dropped.contains(&(i + 1)) { "" } else { l }).collect::<Vec<_>>().join("\n"),
                        _ => f.text.clone(),
                    };
                    let mut work = db.clone();
                    if work.network(&net).is_err() {
                        work.add_network(&net, &time).map_err(|e| e.to_string())?;
                    }
                    let id = work.import_text(&net, fmt, &text, &f.file, &time, &f.description, f.keep_source, policy).map_err(|e| e.to_string())?;
                    Ok((work, id))
                });
                match result {
                    Ok((work, id)) => {
                        let added = work.import_events.iter().find(|e| e.id == id).map(|e| (e.added.points.len(), e.added.observations, e.warnings.len())).unwrap_or_default();
                        self.civil_ui.sdb.db = Some(work);
                        self.sdb_save();
                        self.status = format!("Imported {}: {} point(s), {} observation(s), {} warning(s). Recorded as import event {id}.", f.file, added.0, added.1, added.2);
                        return;
                    }
                    Err(e) => f.message = e,
                }
            }
        }
        self.civil_ui.sdb.import = Some(f);
    }
}

// ─── import events ──────────────────────────────────────────────────────

pub struct EventsForm {
    selected: usize,
    confirm_remove: bool,
    message: String,
}

impl AfroBimApp {
    fn sdb_events_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.sdb.events.take() else { return };
        let Some(db) = self.civil_ui.sdb.db.clone() else { return };
        let mut remove: Option<u64> = None;
        let mut reprocess: Option<(u64, bool)> = None;
        let out = Dialog::new("Import events", crate::glyphs::by_name("SURVEYEVENTS")).width(720.0).resizable(true).max_body_height(560.0).help("Every import is an event: the file, its SHA-256, the time, the format and options, and what it added. Removing an event removes exactly what it added. Reprocessing removes it and reads its stored source text again, with the same options.").show_body(
            ctx,
            |ui| {
                if db.import_events.is_empty() {
                    ui_kit::note(ui, "No field data has been imported.");
                    return;
                }
                let names: Vec<String> = db.import_events.iter().map(|e| format!("{}: {}", e.id, e.file_name)).collect();
                f.selected = f.selected.min(names.len() - 1);
                ui_kit::combo_owned(ui, "sdbe-sel", &mut f.selected, &names);
                let e = &db.import_events[f.selected];
                ui_kit::form(ui, "sdbe-d", |fm| {
                    fm.row("File", |ui| {
                        ui.label(&e.file_name);
                    });
                    fm.row("SHA-256", |ui| {
                        ui.label(RichText::new(&e.sha256).monospace().size(10.0));
                    });
                    fm.row("Time", |ui| {
                        ui.label(&e.imported_at);
                    });
                    fm.row("Network", |ui| {
                        ui.label(&e.network);
                    });
                    fm.row("Format", |ui| {
                        ui.label(format!("{:?}", e.format).chars().take(120).collect::<String>());
                    });
                    fm.row("Added", |ui| {
                        ui.label(format!("{} point(s), {} setup(s), {} observation(s), {} level run(s), {} baseline(s), {} figure(s)", e.added.points.len(), e.added.setups.len(), e.added.observations, e.added.level_runs.len(), e.added.baselines, e.added.figures.len()));
                    });
                    fm.row("Source text", |ui| {
                        ui.label(if e.source_text.is_some() { "kept" } else { "not kept: it cannot be reprocessed" });
                    });
                });
                for w in &e.warnings {
                    ui_kit::callout(ui, Level::Warning, w.clone());
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut f.confirm_remove, "I understand that removing deletes what the event added");
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(f.confirm_remove, egui::Button::new("Remove this event")).clicked() {
                        remove = Some(e.id);
                    }
                    if ui.add_enabled(e.source_text.is_some(), egui::Button::new("Reprocess (same options)")).clicked() {
                        reprocess = Some((e.id, false));
                    }
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
        );
        if out.closed {
            return;
        }
        let time = crate::transform_doc::now_iso();
        if let Some(id) = remove {
            if let Some(db) = &mut self.civil_ui.sdb.db {
                match db.remove_import_event(id, &time) {
                    Ok(()) => f.message = format!("Removed import event {id} and what it added."),
                    Err(e) => f.message = e.to_string(),
                }
            }
            f.confirm_remove = false;
            f.selected = 0;
            self.sdb_save();
        }
        if let Some((id, _)) = reprocess {
            if let Some(db) = &mut self.civil_ui.sdb.db {
                match db.reprocess_import_event(id, None, &time, DuplicatePolicy::Replace) {
                    Ok(n) => f.message = format!("Reprocessed event {id}; it is now event {n}."),
                    Err(e) => f.message = format!("Not reprocessed (the database is unchanged): {e}"),
                }
            }
            self.sdb_save();
        }
        self.civil_ui.sdb.events = Some(f);
    }
}
