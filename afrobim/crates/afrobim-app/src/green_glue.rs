// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! GREENBIM: embodied carbon, material waste and transport of the model, and the energy of its
//! street lighting, from the same quantity lines the 5D estimate uses.
//!
//! The engine is `afrobim-green`. Like the cost crate it holds no numbers of its own: carbon
//! factors, densities and masses come from the user's factor file (loaded here and kept in the
//! document's project settings, with its source and edition), and waste, haul distances and
//! lighting use are entered in this window. With no factor file there is no total, never a zero.
//!
//! The document keeps the factor file, what the user changed from the default rules (not the
//! whole rule list, so default rules added later reach old projects), the lighting inputs and the
//! saved options. A record that cannot be read is reported, never replaced by defaults: nothing is
//! stored over it until the user resets it on purpose.

use std::collections::HashSet;
use std::path::PathBuf;

use afrobim_green::{normalise_key, Assessment, FactorSet, LineIn, Outcome, Overrides, Settings, Use, EMBODIED_BASIS};
use afrobim_kernel::Document;
use serde::{Deserialize, Serialize};

/// The key of the Green BIM record in the project settings.
pub const SETTINGS_KEY: &str = "green";

/// The haul distance the window accepts, in kilometres.
pub const MAX_HAUL_KM: f64 = 20_000.0;

/// One design option kept for comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SavedOption {
    pub name: String,
    /// The factor file it was worked out with: name and edition.
    pub factor_set: String,
    pub embodied_kgco2e: Option<f64>,
    pub transport_kgco2e: Option<f64>,
    /// Material, tonnes, kgCO2e.
    pub by_material: Vec<(String, f64, f64)>,
    pub counted: usize,
    pub excluded: usize,
    pub unmatched: usize,
    pub lighting_kwh_per_year: Option<f64>,
    pub lighting_kgco2e_per_year: Option<f64>,
}

/// What the document keeps, with the rules already merged over the current defaults.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GreenDoc {
    pub factors: Option<FactorSet>,
    pub settings: Settings,
    pub options: Vec<SavedOption>,
}

/// The stored shape. The first version stored `settings` (the whole rule list); it is read once
/// and written back as `overrides`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    factors: Option<FactorSet>,
    overrides: Option<Overrides>,
    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<Settings>,
    options: Vec<SavedOption>,
}

impl GreenDoc {
    fn from_stored(s: Stored) -> GreenDoc {
        let settings = match (s.overrides, s.settings) {
            (Some(o), _) => o.apply(),
            (None, Some(old)) => Overrides::from_legacy(&old.rules, old.lighting).apply(),
            (None, None) => Settings::default(),
        };
        GreenDoc { factors: s.factors, settings, options: s.options }
    }

    fn to_stored(&self) -> Stored {
        Stored { factors: self.factors.clone(), overrides: Some(Overrides::of(&self.settings)), settings: None, options: self.options.clone() }
    }

    /// What an assessment depends on: the factor file and the settings, not the saved options.
    fn inputs(&self) -> (Option<FactorSet>, Settings) {
        (self.factors.clone(), self.settings.clone())
    }
}

/// The Green BIM record of a document. A document with none gives the defaults; one whose record
/// cannot be read gives the reason, so the caller can show it instead of working from defaults.
pub fn load(doc: &Document) -> Result<GreenDoc, String> {
    match afrobim_kernel::project_setting(doc, SETTINGS_KEY) {
        None => Ok(GreenDoc::default()),
        Some(v) => serde_json::from_value::<Stored>(v).map(GreenDoc::from_stored).map_err(|e| format!("The Green BIM record in this project cannot be read ({e}).")),
    }
}

/// Stores the record. It refuses when the record already in the document cannot be read, so a
/// factor file the user has not lost sight of is not overwritten with defaults; use [`reset`] to
/// replace it on purpose.
pub fn store(doc: &mut Document, g: &GreenDoc) -> Result<(), String> {
    load(doc)?;
    write(doc, g)
}

fn write(doc: &mut Document, g: &GreenDoc) -> Result<(), String> {
    let v = serde_json::to_value(g.to_stored()).map_err(|e| e.to_string())?;
    afrobim_kernel::set_project_setting(doc, SETTINGS_KEY, v);
    Ok(())
}

/// Replaces the record with the defaults, whatever it held.
pub fn reset(doc: &mut Document) {
    // Writing the defaults cannot fail to serialise.
    let _ = write(doc, &GreenDoc::default());
}

/// The unit as the engine spells it: `no` (the civil bill's spelling) is `nr`.
fn unit_of(unit: &str) -> String {
    let u = unit.trim();
    if u.eq_ignore_ascii_case("no") {
        "nr".to_string()
    } else {
        u.to_string()
    }
}

/// The 5D quantity lines as the engine's input.
pub fn lines_in(set: &afrobim_cost::QuantitySet) -> Vec<LineIn> {
    set.lines
        .iter()
        .map(|l| LineIn {
            id: l.id,
            element: l.element.map(|e| e.0),
            mark: l.mark.clone(),
            category: l.category.clone(),
            description: l.description.clone(),
            quantity: l.quantity.to_f64(),
            unit: unit_of(&l.unit),
            attributes: l.attributes.clone(),
        })
        .collect()
}

/// Notes about what could not be measured at all: those sources have no quantity line, so no
/// line outcome can mention them.
pub fn skipped_notes(set: &afrobim_cost::QuantitySet) -> Vec<String> {
    if set.skipped.is_empty() {
        return Vec::new();
    }
    let shown: Vec<String> = set.skipped.iter().take(20).map(|s| format!("{} ({}): {}", s.mark, s.source, s.reason)).collect();
    let more = set.skipped.len().saturating_sub(shown.len());
    vec![format!(
        "{} source(s) could not be measured and have no quantity line, so they are in no total and not in the counts below: {}{}.",
        set.skipped.len(),
        shown.join("; "),
        if more > 0 { format!("; and {more} more") } else { String::new() }
    )]
}

/// Assesses a set of quantity lines with a record, adding the notes about what was skipped and
/// what in the record cannot be used.
pub fn assess_set(set: &afrobim_cost::QuantitySet, g: &GreenDoc) -> Assessment {
    let mut a = afrobim_green::assess(&lines_in(set), g.factors.as_ref(), &g.settings);
    a.notes.extend(skipped_notes(set));
    a.notes.extend(g.settings.problems());
    a
}

/// The material keys a factor file needs for this document: the fixed keys of the rules, the
/// fallback keys of the attribute rules, and the keys the attribute rules make from the lines
/// (pavement materials, concrete grades, post sections). Keys that differ only in case or
/// spacing are one key, as the factor-file reader treats them.
pub fn needed_keys(lines: &[LineIn], settings: &Settings) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut add = |k: String| {
        let k = k.trim().to_string();
        if !k.is_empty() && seen.insert(normalise_key(&k)) {
            keys.push(k);
        }
    };
    for r in &settings.rules {
        match &r.use_ {
            Use::Material { key } | Use::HaulOnly { key } => add(key.clone()),
            Use::MaterialFromAttribute { fallback, .. } => {
                for l in lines.iter().filter(|l| l.category == r.category) {
                    if let Ok(k) = r.use_.key_for(&l.attributes) {
                        add(k);
                    }
                }
                if let Some(f) = fallback {
                    add(f.clone());
                }
            }
            _ => {}
        }
    }
    keys
}

/// Groups the digits of a number in threes with a thin space, to `decimals` places.
fn grouped(v: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, v.abs());
    let (int, frac) = s.split_once('.').map_or((s.as_str(), ""), |(i, f)| (i, f));
    let mut out = String::new();
    for (i, ch) in int.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push('\u{2009}');
        }
        out.push(ch);
    }
    let int: String = out.chars().rev().collect();
    let sign = if v < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0') { "-" } else { "" };
    if frac.is_empty() {
        format!("{sign}{int}")
    } else {
        format!("{sign}{int}.{frac}")
    }
}

/// A figure to `decimals` places; a non-zero figure that would round to zero is shown as
/// `< 0.1` (to that precision) rather than as 0.
fn rounded_text(v: f64, decimals: usize) -> String {
    let step = 10f64.powi(-(decimals as i32));
    if v != 0.0 && (v / step).round() == 0.0 {
        return format!("< {}", grouped(step, decimals));
    }
    grouped(v, decimals)
}

/// Tonnes CO2e with a thousands separator, from kg, to 0.1 t. A non-zero figure under 0.05 t is
/// shown as `< 0.1 t`, never as 0.
pub fn t_co2e(kg: f64) -> String {
    format!("{} t", rounded_text(kg / 1000.0, 1))
}

/// A whole number of kilowatt-hours with a thousands separator.
fn kwh_text(k: f64) -> String {
    grouped(k.round(), 0)
}

/// What the user typed into an optional number: empty is not entered (None), anything else must
/// be a finite number inside the range. A typed 0 is a real 0.
pub fn parse_opt(text: &str, range: &std::ops::RangeInclusive<f64>) -> Result<Option<f64>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    match t.parse::<f64>() {
        Ok(v) if v.is_finite() && range.contains(&v) => Ok(Some(v)),
        _ => Err(format!("enter a number from {} to {}, or leave it empty for not entered", range.start(), range.end())),
    }
}

fn format_opt(v: Option<f64>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

/// The window's state for this session.
#[derive(Default)]
pub struct GreenUi {
    pub window_open: bool,
    pub last: Option<Assessment>,
    pub option_name: String,
    /// The record as edited while the pointer is down (a drag in progress); stored on release.
    pending: Option<GreenDoc>,
    /// The factor file and settings the last assessment was worked out with.
    assessed_inputs: Option<(Option<FactorSet>, Settings)>,
}

/// An optional number as a text field: empty is not entered, and the value is stored only once
/// a valid number is typed. Returns true when the value changed.
fn opt_text(ui: &mut egui::Ui, tag: impl std::hash::Hash, value: &mut Option<f64>, unit: &str, range: std::ops::RangeInclusive<f64>) -> bool {
    let id = egui::Id::new(("green-opt", tag));
    let mut text: String = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| format_opt(*value));
    let mut changed = false;
    let r = ui.add(egui::TextEdit::singleline(&mut text).id(id.with("edit")).hint_text("not entered").desired_width(72.0));
    if r.changed() {
        if let Ok(v) = parse_opt(&text, &range) {
            if *value != v {
                *value = v;
                changed = true;
            }
        }
    }
    if !r.has_focus() && !r.changed() {
        // Not being edited: show what is stored.
        text = format_opt(*value);
    }
    if let Err(e) = parse_opt(&text, &range) {
        ui.colored_label(ui.visuals().error_fg_color, "!").on_hover_text(e);
    }
    if !unit.is_empty() {
        ui.label(egui::RichText::new(unit).weak());
    }
    ui.data_mut(|d| d.insert_temp(id, text));
    changed
}

fn use_text(u: &Use) -> String {
    match u {
        Use::Material { key } => format!("material: {key}"),
        Use::MaterialFromAttribute { attribute, prefix, fallback } => match fallback {
            Some(f) => format!("material: {prefix}<{attribute}>, else {f}"),
            None => format!("material: {prefix}<{attribute}>"),
        },
        Use::HaulOnly { key } => format!("haul only: {key}"),
        Use::LightingEnergy => "lighting energy".into(),
        Use::Excluded { reason } => format!("excluded: {reason}"),
    }
}

/// The result of an assessment: the total and what it covers, the notes, the materials, the
/// lighting block and the unmatched lines.
fn result_ui(ui: &mut egui::Ui, a: &Assessment) {
    let total = a.counted + a.excluded + a.unmatched + a.energy_lines;
    match a.embodied_kgco2e {
        Some(kg) => {
            ui.label(egui::RichText::new(format!("Embodied carbon {} CO2e", t_co2e(kg))).size(18.0).strong());
            crate::ui_kit::note(ui, format!("{EMBODIED_BASIS}."));
            if let Some(t) = a.transport_kgco2e {
                crate::ui_kit::note(ui, format!("of which transport {} CO2e", t_co2e(t)));
            }
            crate::ui_kit::note(
                ui,
                format!(
                    "Of {} counted lines, {} include A4 and {} do not; {} include a waste allowance and {} do not.",
                    a.counted, a.counted_with_a4, a.counted_without_a4, a.counted_with_waste, a.counted_without_waste
                ),
            );
        }
        None => {
            ui.label(egui::RichText::new("No total: nothing could be counted").strong());
        }
    }
    crate::ui_kit::note(ui, format!("{} of {total} quantity lines counted, {} excluded on purpose, {} unmatched, {} lighting. Factor file: {}.", a.counted, a.excluded, a.unmatched, a.energy_lines, if a.factor_set.is_empty() { "none" } else { &a.factor_set }));
    for n in &a.notes {
        crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, n.clone());
    }
    if !a.by_material.is_empty() {
        let rows: Vec<Vec<String>> = a.by_material.iter().map(|m| vec![m.material.clone(), rounded_text(m.tonnes, 1), rounded_text(m.waste_t, 1), t_co2e(m.kgco2e)]).collect();
        crate::ui_kit::results_table(ui, "green-materials", &["Material", "Tonnes", "Waste t", "CO2e"], &[crate::ui_kit::Col::Text, crate::ui_kit::Col::Num, crate::ui_kit::Col::Num, crate::ui_kit::Col::Num], &rows);
    }
    let l = &a.lighting;
    if a.energy_lines > 0 {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Street lighting (operational, kept apart from embodied carbon)").strong());
        let mut s = format!("{} luminaires", grouped(l.luminaires, 0));
        match l.installed_kw {
            Some(kw) => s += &format!(", {} kW installed", rounded_text(kw, 2)),
            None => s += ", installed power not calculated",
        }
        if let Some(k) = l.kwh_per_year {
            s += &format!(", {} kWh per year", kwh_text(k));
        }
        if let Some(c) = l.kgco2e_per_year {
            s += &format!(", {} CO2e per year", t_co2e(c));
        }
        if let Some(c) = l.life_kgco2e {
            s += &format!(", {} CO2e over the design life", t_co2e(c));
        }
        ui.label(s);
        if !l.missing.is_empty() {
            crate::ui_kit::note(ui, format!("Not worked out: {} not entered.", l.missing.join(", ")));
        }
    }
    let unmatched: Vec<&afrobim_green::Row> = a.rows.iter().filter(|r| matches!(r.outcome, Outcome::Unmatched(_))).collect();
    if !unmatched.is_empty() {
        egui::CollapsingHeader::new(format!("Unmatched lines ({})", unmatched.len())).id_salt("green-unmatched").show(ui, |ui| {
            for r in unmatched.iter().take(200) {
                if let Outcome::Unmatched(why) = &r.outcome {
                    ui.label(egui::RichText::new(format!("{}  {}: {why}", r.mark, r.description)).small());
                }
            }
        });
    }
}

impl crate::app::AfroBimApp {
    /// Runs the assessment on the current model and keeps it for the window.
    pub(crate) fn green_assess(&mut self) {
        let g = match load(&self.doc) {
            Ok(g) => g,
            Err(e) => {
                self.status = format!("Green BIM: {e} Reset the Green BIM settings to continue.");
                self.green.last = None;
                self.green.assessed_inputs = None;
                return;
            }
        };
        let signs = crate::cost_glue::sign_rows(self.engine.as_deref(), &self.doc);
        let a = assess_set(&crate::cost_glue::collect(&self.doc, &signs), &g);
        self.status = match a.embodied_kgco2e {
            Some(kg) => format!("Green BIM: {} CO2e embodied, {EMBODIED_BASIS} ({} lines counted, {} unmatched).", t_co2e(kg), a.counted, a.unmatched),
            None => format!("Green BIM: no total ({}).", if g.factors.is_none() { "load a factor file first" } else { "no line could be counted" }),
        };
        self.green.assessed_inputs = Some(g.inputs());
        self.green.last = Some(a);
    }

    /// Saves the last assessment as a design option, unless it was worked out with a factor file
    /// or settings other than the current ones.
    pub(crate) fn green_save_option(&mut self, g: &mut GreenDoc) -> bool {
        let fresh = self.green.assessed_inputs.as_ref() == Some(&g.inputs());
        let Some(a) = self.green.last.clone().filter(|_| fresh) else {
            self.green.last = None;
            self.status = "Green BIM: the factor file or settings changed since the last assessment; assess again before saving an option.".to_string();
            return false;
        };
        g.options.push(SavedOption {
            name: self.green.option_name.trim().to_string(),
            factor_set: a.factor_set.clone(),
            embodied_kgco2e: a.embodied_kgco2e,
            transport_kgco2e: a.transport_kgco2e,
            by_material: a.by_material.iter().map(|m| (m.material.clone(), m.tonnes, m.kgco2e)).collect(),
            counted: a.counted,
            excluded: a.excluded,
            unmatched: a.unmatched,
            lighting_kwh_per_year: a.lighting.kwh_per_year,
            lighting_kgco2e_per_year: a.lighting.kgco2e_per_year,
        });
        self.green.option_name.clear();
        true
    }

    /// Takes an edited record into the document. While the pointer is down (a drag) the edit is
    /// held and stored on release. When what an assessment depends on changed and there is a
    /// result on show, it is worked out again so the result never describes old settings.
    pub(crate) fn green_apply(&mut self, edited: GreenDoc, stored: &GreenDoc, can_store: bool, pointer_down: bool) {
        if !can_store || edited == *stored {
            self.green.pending = None;
            return;
        }
        if pointer_down {
            self.green.pending = Some(edited);
            return;
        }
        self.green.pending = None;
        if store(&mut self.doc, &edited).is_ok() {
            self.schedule_commit_now();
            if self.green.last.is_some() && edited.inputs() != stored.inputs() {
                self.green_assess();
            }
        }
    }

    /// The GREENBIM window.
    pub(crate) fn green_window(&mut self, ctx: &egui::Context) {
        if !self.green.window_open {
            return;
        }
        let loaded = load(&self.doc);
        let load_error = loaded.as_ref().err().cloned();
        let stored = loaded.unwrap_or_default();
        let mut g = self.green.pending.take().unwrap_or_else(|| stored.clone());
        let (mut load_file, mut save_template, mut run, mut save_option, mut reset_record) = (false, false, false, false, false);
        let mut remove_option: Option<usize> = None;
        let last = self.green.last.clone();
        let problems = g.settings.problems();
        let dlg = crate::ui_kit::Dialog::titled("Green BIM").subtitle("Embodied carbon, waste and transport from the model's quantities; street lighting energy").width(760.0).show_close(
            ctx,
            |ui| {
                if let Some(e) = &load_error {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, format!("{e} Nothing is shown from it and nothing is stored over it. If it cannot be recovered from a saved copy of the project, reset the Green BIM settings; that removes the factor file, the settings and the saved options of this project."));
                    if ui.button("Reset Green BIM settings").clicked() {
                        reset_record = true;
                    }
                    return;
                }
                for p in &problems {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, p.clone());
                }
                // The factor file.
                crate::ui_kit::heading(ui, "Factor file");
                match &g.factors {
                    Some(f) => {
                        ui.label(egui::RichText::new(format!("{}  ({} factors)", f.name, f.factors.len())).strong());
                        crate::ui_kit::note(ui, format!("Source: {}   Edition: {}   Boundary: {}", f.source, f.edition, f.boundary));
                        crate::ui_kit::note(
                            ui,
                            format!(
                                "Transport: {}   Grid electricity: {}",
                                f.transport_kgco2e_per_tkm.map(|v| format!("{v} kgCO2e per t.km")).unwrap_or_else(|| "not given".into()),
                                f.grid_kgco2e_per_kwh.map(|v| format!("{v} kgCO2e per kWh ({})", f.grid_source)).unwrap_or_else(|| "not given".into())
                            ),
                        );
                    }
                    None => {
                        crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "No factor file loaded, so no carbon total is shown. AfroBIM ships no carbon factors: choose a source (for example the ICE database, a supplier's EPD or the client's own figures), fill the blank template with its values, densities and edition, and load it.");
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Load factor file…").clicked() {
                        load_file = true;
                    }
                    if ui.button("Save blank template…").on_hover_text("A CSV with the material keys this model needs and every number left blank").clicked() {
                        save_template = true;
                    }
                    if g.factors.is_some() && ui.button("Remove").clicked() {
                        g.factors = None;
                    }
                });
                ui.add_space(6.0);

                // Street lighting use.
                crate::ui_kit::heading(ui, "Street lighting use");
                crate::ui_kit::note(ui, "Leave a field empty for not entered: nothing is assumed. A typed 0 is a real 0.");
                egui::Grid::new("green-lighting").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                    ui.label("Burning hours");
                    ui.horizontal(|ui| opt_text(ui, "hours", &mut g.settings.lighting.hours_per_year, "h per year", 0.0..=8784.0));
                    ui.end_row();
                    ui.label("Energy after dimming");
                    ui.horizontal(|ui| opt_text(ui, "dimming", &mut g.settings.lighting.dimming_factor, "(1 = no dimming)", 0.0..=1.0));
                    ui.end_row();
                    ui.label("Design life");
                    ui.horizontal(|ui| opt_text(ui, "life", &mut g.settings.lighting.design_life_years, "years", 0.0..=100.0));
                    ui.end_row();
                });
                ui.add_space(6.0);

                // Rules.
                egui::CollapsingHeader::new(egui::RichText::new("How each quantity is used").strong()).id_salt("green-rules").show(ui, |ui| {
                    crate::ui_kit::note(ui, "One quantity per family carries the material (the volume of a pavement layer, the length of a post); the overlapping ones are excluded so nothing is counted twice. Waste and haul are counted only where entered; empty means not entered. The embodied total includes transport (A4) only for the lines whose rule has a haul.");
                    egui::ScrollArea::vertical().max_height(260.0).id_salt("green-rules-scroll").show(ui, |ui| {
                        egui::Grid::new("green-rules-grid").num_columns(5).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                            for h in ["Quantity", "Unit", "Used as", "Waste", "Haul"] {
                                crate::ui_kit::th(ui, h);
                            }
                            ui.end_row();
                            for (i, r) in g.settings.rules.iter_mut().enumerate() {
                                ui.label(&r.category);
                                ui.label(&r.unit);
                                ui.label(egui::RichText::new(use_text(&r.use_)).small());
                                let counts = r.use_.is_counting();
                                for (j, (v, unit, max)) in [(&mut r.waste_pct, "%", 100.0), (&mut r.haul_km, "km", MAX_HAUL_KM)].into_iter().enumerate() {
                                    if counts {
                                        ui.horizontal(|ui| opt_text(ui, (i, j), v, unit, 0.0..=max));
                                    } else {
                                        ui.label("");
                                    }
                                }
                                ui.end_row();
                            }
                        });
                    });
                });
                ui.add_space(6.0);

                // Results.
                crate::ui_kit::heading(ui, "Result");
                if ui.button("Assess the model").clicked() {
                    run = true;
                }
                if let Some(a) = &last {
                    result_ui(ui, a);
                    // Options.
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        crate::ui_kit::text_field(ui, &mut self.green.option_name, "Option name, e.g. Flexible pavement");
                        if ui.add_enabled(!self.green.option_name.trim().is_empty(), egui::Button::new("Save as option")).clicked() {
                            save_option = true;
                        }
                    });
                }
                if !g.options.is_empty() {
                    ui.add_space(6.0);
                    crate::ui_kit::heading(ui, "Design options");
                    egui::Grid::new("green-options").num_columns(6).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
                        for h in ["Option", "Embodied", "Transport", "Coverage", "Lighting per year", ""] {
                            crate::ui_kit::th(ui, h);
                        }
                        ui.end_row();
                        for (i, o) in g.options.iter().enumerate() {
                            ui.label(&o.name).on_hover_text(format!("Factor file: {}", o.factor_set));
                            crate::ui_kit::cell_num(ui, o.embodied_kgco2e.map(t_co2e).unwrap_or_else(|| "none".into()));
                            crate::ui_kit::cell_num(ui, o.transport_kgco2e.map(t_co2e).unwrap_or_else(|| "-".into()));
                            ui.label(format!("{} counted, {} unmatched", o.counted, o.unmatched));
                            crate::ui_kit::cell_num(ui, o.lighting_kgco2e_per_year.map(t_co2e).or(o.lighting_kwh_per_year.map(|k| format!("{} kWh", kwh_text(k)))).unwrap_or_else(|| "-".into()));
                            if ui.small_button("Remove").clicked() {
                                remove_option = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                    crate::ui_kit::note(ui, "Compare options only when they were worked out with the same factor file (hover a name to see it).");
                }
            },
            &["Export CSV…"],
        );
        let export = load_error.is_none() && dlg.inner.as_ref().and_then(|(_, s)| *s) == Some(0);
        if dlg.closed {
            self.green.window_open = false;
            self.green.pending = None;
        }
        if reset_record {
            reset(&mut self.doc);
            self.schedule_commit_now();
            self.green.last = None;
            self.green.assessed_inputs = None;
            self.green.pending = None;
            self.status = "Green BIM settings reset.".to_string();
            return;
        }
        if load_error.is_some() {
            return;
        }
        if let Some(i) = remove_option {
            g.options.remove(i);
        }
        if save_option {
            self.green_save_option(&mut g);
        }
        if load_file {
            if let Some(path) = rfd::FileDialog::new().add_filter("Factor file (CSV)", &["csv"]).set_title("Load a carbon factor file").pick_file() {
                match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| afrobim_green::factors_from_csv(&t)) {
                    Ok(f) => {
                        self.status = format!("Loaded {} factors from {}.", f.factors.len(), path.display());
                        g.factors = Some(f);
                    }
                    Err(e) => self.status = format!("Could not load {}: {e}", path.display()),
                }
            }
        }
        if save_template {
            let signs = crate::cost_glue::sign_rows(self.engine.as_deref(), &self.doc);
            let lines = lines_in(&crate::cost_glue::collect(&self.doc, &signs));
            let keys = needed_keys(&lines, &g.settings);
            let pairs: Vec<(&str, &str)> = keys.iter().map(|k| (k.as_str(), k.as_str())).collect();
            if let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("carbon-factors.csv").set_title("Save a blank factor file").save_file() {
                self.status = match std::fs::write(&path, afrobim_green::template_csv(&pairs)) {
                    Ok(()) => format!("Wrote a blank factor file with {} materials to {}.", keys.len(), path.display()),
                    Err(e) => format!("Could not write {}: {e}", path.display()),
                };
            }
        }
        // Compared with the record read at the start of the frame, with a blank factor (NaN) equal
        // to itself, so nothing is stored on every frame. A change to the factor file or settings
        // works the result out again; a change while the pointer is down waits for its release.
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        let held = pointer_down && g != stored;
        self.green_apply(g.clone(), &stored, true, pointer_down);
        if held {
            ctx.request_repaint();
        }
        if run || (load_file && g.factors.is_some()) {
            self.green_assess();
        }
        if export {
            if self.green.last.is_none() {
                self.green_assess();
            }
            if let (Some(a), Some(path)) = (&self.green.last, rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("green-bim.csv").save_file()) {
                let path: PathBuf = path;
                self.status = match std::fs::write(&path, afrobim_green::assessment_csv(a)) {
                    Ok(()) => format!("Exported the Green BIM assessment to {}.", path.display()),
                    Err(e) => format!("Could not write {}: {e}", path.display()),
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FACTORS: &str = "# name: Test factors\n# source: invented test data\n# edition: test\n# boundary: A1-A3\n# transport_kgco2e_per_tkm: 0.1\nkey,material,kgco2e_per_t,t_per_m3,t_per_m2,t_per_m,t_per_nr,source,edition\npavement: Hot mix asphalt,Asphalt,50,2,,,,test,test\npavement: Granular base,Base,5,2,,,,test,test\n";

    fn civil_app() -> crate::app::AfroBimApp {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx);
        crate::app::AfroBimApp::new(&cc, crate::Options { civil_demo: true, screenshot: Some(std::env::temp_dir().join("afrobim-green-test.png")), ..crate::Options::default() })
    }

    fn with_factors(app: &mut crate::app::AfroBimApp) -> GreenDoc {
        let mut g = load(&app.doc).unwrap();
        g.factors = Some(afrobim_green::factors_from_csv(FACTORS).unwrap());
        store(&mut app.doc, &g).unwrap();
        g
    }

    #[test]
    fn the_civil_demonstration_is_assessed_from_its_pavement_volumes_and_lighting() {
        let mut app = civil_app();
        app.doc = crate::sheets_glue::test_support::demo_doc();
        let set = crate::cost_glue::collect(&app.doc, &[]);
        let lines = lines_in(&set);
        assert!(!lines.is_empty(), "the civil demonstration has quantities");
        // Without a factor file there is no total, never zero.
        app.green_assess();
        let a = app.green.last.clone().unwrap();
        assert_eq!(a.embodied_kgco2e, None);
        assert_eq!(a.counted + a.excluded + a.unmatched + a.energy_lines, lines.len());
        // The template asks for the pavement materials the model really has.
        let keys = needed_keys(&lines, &Settings::default());
        assert!(keys.iter().any(|k| k == "pavement: Hot mix asphalt"), "{keys:?}");
        // With factors, the asphalt and base volumes are counted at volume x density x factor.
        with_factors(&mut app);
        app.green_assess();
        let a = app.green.last.clone().unwrap();
        let vol = |m: &str| lines.iter().filter(|l| l.category == "pavement.layer_volume" && l.attributes.get("material").is_some_and(|x| x == m)).map(|l| l.quantity).sum::<f64>();
        let want = vol("Hot mix asphalt") * 2.0 * 50.0 + vol("Granular base") * 2.0 * 5.0;
        let got = a.embodied_kgco2e.expect("a total once factors are loaded");
        assert!((got - want).abs() < 1e-6 * want.max(1.0), "{got} vs {want}");
        // The total says what it covers: no haul was entered, so no line includes A4.
        assert_eq!(a.counted_with_a4, 0);
        assert!(a.notes.iter().any(|n| n.contains(EMBODIED_BASIS)));
        // The record survives save and load.
        let back = Document::from_json(&app.doc.to_json()).unwrap();
        assert_eq!(load(&back).unwrap().factors.map(|f| f.name), Some("Test factors".to_string()));
    }

    #[test]
    fn green_the_record_keeps_only_what_the_user_changed() {
        let mut app = civil_app();
        let mut g = with_factors(&mut app);
        let i = g.settings.rules.iter().position(|r| r.category == "pavement.layer_volume").unwrap();
        g.settings.rules[i].haul_km = Some(12.0);
        store(&mut app.doc, &g).unwrap();
        let raw = afrobim_kernel::project_setting(&app.doc, SETTINGS_KEY).unwrap();
        assert!(raw.get("settings").is_none(), "the whole rule list is not stored: {raw}");
        let rules = &raw["overrides"]["rules"];
        assert_eq!(rules.as_array().unwrap().len(), 1, "{raw}");
        assert_eq!(rules[0]["haul_km"], 12.0);
        assert_eq!(load(&app.doc).unwrap(), g);
        // a default rule added after the project was saved reaches it: it is merged at load
        assert_eq!(load(&app.doc).unwrap().settings.rules.len(), afrobim_green::default_rules().len());
    }

    #[test]
    fn green_a_record_stored_by_the_first_version_migrates_and_posts_follow_the_new_defaults() {
        let mut app = civil_app();
        // A full list as the first version wrote it: post.mass counted, post.length excluded.
        let old = serde_json::json!({
            "factors": null,
            "settings": {"rules": [
                {"category": "post.mass", "unit": "kg", "use_": {"Material": {"key": "steel post"}}, "waste_pct": 5.0, "haul_km": null},
                {"category": "post.length", "unit": "m", "use_": {"Excluded": {"reason": "mass carries the post"}}, "waste_pct": null, "haul_km": null},
                {"category": "pavement.layer_volume", "unit": "m3", "use_": {"MaterialFromAttribute": {"attribute": "material", "prefix": "pavement: "}}, "waste_pct": 2.0, "haul_km": null}
            ], "lighting": {"hours_per_year": 4000.0, "dimming_factor": null, "design_life_years": null}},
            "options": []
        });
        afrobim_kernel::set_project_setting(&mut app.doc, SETTINGS_KEY, old);
        let g = load(&app.doc).unwrap();
        let rule = |c: &str| g.settings.rules.iter().find(|r| r.category == c).unwrap().clone();
        assert!(rule("post.length").use_.is_counting(), "the new default reaches the old project");
        assert!(matches!(rule("post.mass").use_, Use::Excluded { .. }));
        assert_eq!(rule("post.length").waste_pct, Some(5.0), "waste entered on the mass moves to the length");
        assert_eq!(rule("pavement.layer_volume").waste_pct, Some(2.0));
        assert_eq!(g.settings.lighting.hours_per_year, Some(4000.0));
        assert_eq!(g.settings.rules.len(), afrobim_green::default_rules().len());
        // once stored it is the small form
        store(&mut app.doc, &g).unwrap();
        assert!(afrobim_kernel::project_setting(&app.doc, SETTINGS_KEY).unwrap().get("settings").is_none());
        assert_eq!(load(&app.doc).unwrap(), g);
    }

    #[test]
    fn green_an_unreadable_record_is_reported_and_never_overwritten() {
        let mut app = civil_app();
        let bad = serde_json::json!({"factors": 5, "options": []});
        afrobim_kernel::set_project_setting(&mut app.doc, SETTINGS_KEY, bad.clone());
        let e = load(&app.doc).unwrap_err();
        assert!(e.contains("cannot be read"), "{e}");
        // an edit is refused and the record is untouched
        let g = GreenDoc::default();
        assert!(store(&mut app.doc, &g).is_err());
        assert_eq!(afrobim_kernel::project_setting(&app.doc, SETTINGS_KEY), Some(bad));
        app.green_apply(GreenDoc { options: vec![SavedOption::default()], ..GreenDoc::default() }, &g, true, false);
        assert!(load(&app.doc).is_err(), "still the unreadable record");
        // assessing says why instead of working from defaults
        app.green_assess();
        assert!(app.green.last.is_none());
        assert!(app.status.contains("cannot be read"), "{}", app.status);
        // only an explicit reset replaces it
        reset(&mut app.doc);
        assert_eq!(load(&app.doc).unwrap(), GreenDoc::default());
    }

    #[test]
    fn green_a_changed_input_works_the_result_out_again_and_an_option_is_never_saved_from_a_stale_one() {
        let mut app = civil_app();
        app.doc = crate::sheets_glue::test_support::demo_doc();
        let stored = with_factors(&mut app);
        app.green_assess();
        assert!(app.green.last.as_ref().unwrap().embodied_kgco2e.is_some());
        // a waste entered: the shown result follows at once
        let mut edited = stored.clone();
        let i = edited.settings.rules.iter().position(|r| r.category == "pavement.layer_volume").unwrap();
        edited.settings.rules[i].waste_pct = Some(10.0);
        let before = app.green.last.as_ref().unwrap().embodied_kgco2e.unwrap();
        app.green_apply(edited.clone(), &stored, true, false);
        let after = app.green.last.as_ref().unwrap().embodied_kgco2e.unwrap();
        assert!((after - before * 1.1).abs() < 1e-6 * before, "{after} vs {before}");
        // removing the factor file: no total, not the old one
        let mut gone = edited.clone();
        gone.factors = None;
        app.green_apply(gone, &edited, true, false);
        assert_eq!(app.green.last.as_ref().unwrap().embodied_kgco2e, None);
        // saving an option from an assessment made with other inputs is refused
        app.green.option_name = "A".into();
        let mut now = load(&app.doc).unwrap();
        now.factors = stored.factors.clone();
        assert!(!app.green_save_option(&mut now), "the shown result was made with no factor file");
        assert!(now.options.is_empty());
        assert!(app.green.last.is_none());
        assert!(app.status.contains("assess again"));
        // fresh result: saved
        app.green_assess();
        app.green.option_name = "B".into();
        let mut cur = load(&app.doc).unwrap();
        assert!(app.green_save_option(&mut cur));
        assert_eq!(cur.options.len(), 1);
        // saving an option changes no input, so it does not make the result stale
        app.green_apply(cur.clone(), &load(&app.doc).unwrap(), true, false);
        assert!(app.green.last.is_some());
    }

    #[test]
    fn green_an_edit_during_a_drag_waits_for_the_release() {
        let mut app = civil_app();
        let stored = with_factors(&mut app);
        let mut edited = stored.clone();
        edited.settings.lighting.hours_per_year = Some(3000.0);
        app.green_apply(edited.clone(), &stored, true, true);
        assert_eq!(load(&app.doc).unwrap(), stored, "nothing stored while the pointer is down");
        assert_eq!(app.green.pending, Some(edited.clone()));
        app.green_apply(edited.clone(), &stored, true, false);
        assert_eq!(load(&app.doc).unwrap(), edited);
        assert_eq!(app.green.pending, None);
    }

    #[test]
    fn green_optional_numbers_are_none_until_a_number_is_typed() {
        let r = 0.0..=100.0;
        assert_eq!(parse_opt("", &r), Ok(None));
        assert_eq!(parse_opt("  ", &r), Ok(None));
        assert_eq!(parse_opt("0", &r), Ok(Some(0.0)), "a typed 0 is a real 0");
        assert_eq!(parse_opt(" 2.5 ", &r), Ok(Some(2.5)));
        for bad in ["abc", "nan", "NaN", "inf", "-1", "101", "1,5"] {
            assert!(parse_opt(bad, &r).is_err(), "{bad}");
        }
        assert_eq!(format_opt(None), "");
        assert_eq!(format_opt(Some(4000.0)), "4000");
        assert_eq!(parse_opt("20000", &(0.0..=MAX_HAUL_KM)), Ok(Some(20_000.0)));
        assert!(parse_opt("20001", &(0.0..=MAX_HAUL_KM)).is_err());
    }

    #[test]
    fn green_figures_keep_the_precision_they_are_rounded_to() {
        assert_eq!(t_co2e(30.0), "< 0.1 t");
        assert_eq!(t_co2e(0.0), "0.0 t");
        assert_eq!(t_co2e(50.0), "0.1 t");
        assert_eq!(t_co2e(1_234_567.0), "1\u{2009}234.6 t");
        assert_eq!(kwh_text(4000.0), "4\u{2009}000");
        assert_eq!(rounded_text(0.004, 1), "< 0.1");
        assert_eq!(rounded_text(2.0, 1), "2.0");
    }

    #[test]
    fn green_the_template_keys_have_no_repeats_and_no_stray_spaces() {
        let attr = |v: &str| LineIn { category: "pavement.layer_volume".into(), unit: "m3".into(), quantity: 1.0, attributes: [("material".to_string(), v.to_string())].into(), ..LineIn::default() };
        let lines = vec![attr("Hot mix asphalt"), attr(" hot mix  asphalt "), attr("Granular base"), attr("   ")];
        let keys = needed_keys(&lines, &Settings::default());
        let pave: Vec<&String> = keys.iter().filter(|k| k.starts_with("pavement:")).collect();
        assert_eq!(pave.len(), 2, "{keys:?}");
        assert!(keys.iter().all(|k| k == k.trim()));
        let normal: HashSet<String> = keys.iter().map(|k| normalise_key(k)).collect();
        assert_eq!(normal.len(), keys.len(), "no two keys the factor reader would call repeated: {keys:?}");
        // the template reads back with the factor-file reader
        let pairs: Vec<(&str, &str)> = keys.iter().map(|k| (k.as_str(), k.as_str())).collect();
        assert!(afrobim_green::factors_from_csv(&afrobim_green::template_csv(&pairs)).is_ok());
        // posts: the fallback key and the key of each section are asked for
        let post = LineIn { category: "post.length".into(), unit: "m".into(), quantity: 1.0, attributes: [("section".to_string(), "60x3 CHS".to_string())].into(), ..LineIn::default() };
        let keys = needed_keys(&[post], &Settings::default());
        assert!(keys.iter().any(|k| k == "steel post") && keys.iter().any(|k| k == "steel post 60x3 CHS"), "{keys:?}");
    }

    #[test]
    fn green_lines_carry_the_element_and_read_no_as_nr() {
        let mut set = afrobim_cost::QuantitySet::new();
        set.push(Some(afrobim_kernel::ElementId(7)), "N1", "Pipe networks", "Manhole", 3.0, "no", &[]);
        let lines = lines_in(&set);
        assert_eq!(lines[0].unit, "nr");
        assert_eq!(lines[0].element, Some(7));
    }

    #[test]
    fn green_unmeasured_sources_and_an_unreadable_bill_reach_the_notes() {
        let mut app = civil_app();
        let mut doc = crate::sheets_glue::test_support::demo_doc();
        // a corridor-like element with a bill property nobody can read
        let id = doc.add(crate::civil::CORRIDOR, "Road 1", afrobim_kernel::Pt::new(0.0, 0.0), 10.0, 10.0, "0", serde_json::Value::Null);
        doc.find_mut(id).unwrap().properties.insert(crate::civil_quantities::PROP_BILL.into(), "not json".into());
        app.doc = doc;
        let set = crate::cost_glue::collect(&app.doc, &[]);
        let bill: Vec<_> = set.skipped.iter().filter(|s| s.source == "civil bill").collect();
        assert_eq!(bill.len(), 1, "{:?}", set.skipped);
        assert_eq!(bill[0].mark, "Road 1");
        let a = assess_set(&set, &GreenDoc::default());
        let note = a.notes.iter().find(|n| n.contains("could not be measured")).expect("a note about skipped sources");
        assert!(note.contains("Road 1") && note.contains("civil.bill"), "{note}");
        // and every collector's skip is carried, with its count
        let mut set = afrobim_cost::QuantitySet::new();
        set.skipped.push(afrobim_cost::SkippedSource { source: "supports".into(), mark: "P1".into(), reason: "no section".into() });
        set.skipped.push(afrobim_cost::SkippedSource { source: "gantry".into(), mark: "G1".into(), reason: "bad".into() });
        let notes = skipped_notes(&set);
        assert!(notes[0].starts_with("2 source(s)") && notes[0].contains("P1 (supports): no section"), "{notes:?}");
        assert!(skipped_notes(&afrobim_cost::QuantitySet::new()).is_empty());
    }

    fn shape_text(shape: &egui::Shape, out: &mut String) {
        match shape {
            egui::Shape::Text(t) => {
                out.push_str(t.galley.text());
                out.push('\n');
            }
            egui::Shape::Vec(v) => v.iter().for_each(|s| shape_text(s, out)),
            _ => {}
        }
    }

    /// All the text the application draws in its last of `frames` frames.
    fn rendered(ctx: &egui::Context, app: &mut crate::app::AfroBimApp, frames: u32) -> String {
        let mut f = eframe::Frame::_new_kittest();
        let mut text = String::new();
        for _ in 0..frames {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 1400.0))), ..Default::default() };
            let out = ctx.run(input, |ctx| eframe::App::update(app, ctx, &mut f));
            text.clear();
            out.shapes.iter().for_each(|s| shape_text(&s.shape, &mut text));
        }
        text
    }

    #[test]
    fn green_the_result_shows_the_basis_of_the_total_and_the_notes() {
        let (ctx, mut app) = crate::u4_dialogs_test::app_for("dark", true);
        crate::u4_dialogs_test::frame(&ctx, &mut app, 2);
        app.doc = crate::sheets_glue::test_support::demo_doc();
        with_factors(&mut app);
        app.green_assess();
        let a = app.green.last.clone().unwrap();
        let mut text = String::new();
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 4000.0))), ..Default::default() };
        let out = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| result_ui(ui, &a));
        });
        out.shapes.iter().for_each(|s| shape_text(&s.shape, &mut text));
        assert!(text.contains("Embodied carbon"), "{text}");
        assert!(text.contains(EMBODIED_BASIS), "the basis of the total is shown: {text}");
        assert!(text.contains("include A4"), "the coverage of A4 is shown: {text}");
        for n in &a.notes {
            assert!(text.contains(n.as_str()), "the note is shown under the result: {n}");
        }
        assert!(text.contains("Counted rules with no haul_km"), "{text}");
    }

    #[test]
    fn green_the_window_shows_why_a_record_cannot_be_read_and_offers_the_reset() {
        let (ctx, mut app) = crate::u4_dialogs_test::app_for("dark", true);
        afrobim_kernel::set_project_setting(&mut app.doc, SETTINGS_KEY, serde_json::json!({"factors": 5}));
        app.green.window_open = true;
        let text = rendered(&ctx, &mut app, 3);
        assert!(text.contains("cannot be read"), "{text}");
        assert!(text.contains("Reset Green BIM settings"), "{text}");
        assert!(!text.contains("Assess the model"), "no work is offered from defaults");
        assert!(load(&app.doc).is_err(), "the window did not overwrite it");
    }

    #[test]
    fn green_the_civil_bill_lines_are_excluded_with_a_reason_and_the_second_marking_set_is_ruled() {
        let mut set = afrobim_cost::QuantitySet::new();
        set.push(None, "R", "Civil earthworks", "Cut", 100.0, "m3", &[]);
        set.push(None, "R", "Civil materials", "Gravel", 10.0, "m3", &[]);
        set.push(None, "N", "Pipe networks", "Pipe 300", 40.0, "m", &[]);
        set.push(None, "N", "Pipe networks", "Manhole", 3.0, "no", &[]);
        set.push(None, "N", "Pipe networks", "Trench", 30.0, "m3", &[]);
        set.push(None, "G", "Grading", "Net", 5.0, "m3", &[]);
        set.push(None, "M", "marking_ext.painted_area", "Edge line", 12.0, "m2", &[]);
        set.push(None, "M", "marking_ext.block_area", "Block", 2.0, "m2", &[]);
        set.push(None, "M", "marking_ext.length", "Edge line, length", 40.0, "m", &[]);
        set.push(None, "M", "marking_ext.number", "Edge line, number", 1.0, "nr", &[]);
        let a = assess_set(&set, &GreenDoc::default());
        assert_eq!(a.unmatched, 1, "only the paint, for want of a factor file: {:?}", a.rows);
        assert_eq!(a.rows[6].outcome, Outcome::Unmatched("no factor file loaded".into()));
        for r in a.rows.iter().take(6) {
            assert!(matches!(&r.outcome, Outcome::Excluded(w) if w.contains("cut, fill or net")), "{r:?}");
        }
        assert!(matches!(&a.rows[7].outcome, Outcome::Excluded(w) if w == "placeholder block, not paint"));
        assert_eq!(a.counted + a.excluded + a.unmatched + a.energy_lines, set.lines.len());
        // with a factor for road marking, the painted area is counted
        let mut f = afrobim_green::factors_from_csv("key,material,kgco2e_per_t,t_per_m3,t_per_m2,t_per_m,t_per_nr,source,edition\nroad marking,Paint,10,,0.5,,,t,t\n").unwrap();
        f.name = "T".into();
        let g = GreenDoc { factors: Some(f), ..GreenDoc::default() };
        let a = assess_set(&set, &g);
        assert_eq!(a.counted, 1);
        assert_eq!(a.embodied_kgco2e, Some(12.0 * 0.5 * 10.0));
    }
}
