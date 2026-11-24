//! GREENBIM: embodied carbon, material waste and transport of the model, and the energy of its
//! street lighting, from the same quantity lines the 5D estimate uses.
//!
//! The engine is `afrobim-green`. Like the cost crate it holds no numbers of its own: carbon
//! factors, densities and masses come from the user's factor file (loaded here and kept in the
//! document's project settings, with its source and edition), and waste, haul distances and
//! lighting use are entered in this window. With no factor file there is no total, never a zero.

use std::path::PathBuf;

use afrobim_green::{Assessment, FactorSet, LineIn, Outcome, Settings, Use};
use afrobim_kernel::Document;
use serde::{Deserialize, Serialize};

/// The key of the Green BIM record in the project settings.
pub const SETTINGS_KEY: &str = "green";

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

/// What the document keeps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GreenDoc {
    pub factors: Option<FactorSet>,
    pub settings: Settings,
    pub options: Vec<SavedOption>,
}

pub fn load(doc: &Document) -> GreenDoc {
    afrobim_kernel::project_setting(doc, SETTINGS_KEY).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

pub fn store(doc: &mut Document, g: &GreenDoc) {
    if let Ok(v) = serde_json::to_value(g) {
        afrobim_kernel::set_project_setting(doc, SETTINGS_KEY, v);
    }
}

/// The 5D quantity lines as the engine's input.
pub fn lines_in(set: &afrobim_cost::QuantitySet) -> Vec<LineIn> {
    set.lines
        .iter()
        .map(|l| LineIn { id: l.id, mark: l.mark.clone(), category: l.category.clone(), description: l.description.clone(), quantity: l.quantity.to_f64(), unit: l.unit.clone(), attributes: l.attributes.clone() })
        .collect()
}

/// Assesses a document with its own Green BIM record.
pub fn assess_doc(doc: &Document, signs: &[afrobim_fabrication::SignQuantity]) -> Assessment {
    let g = load(doc);
    let set = crate::cost_glue::collect(doc, signs);
    afrobim_green::assess(&lines_in(&set), g.factors.as_ref(), &g.settings)
}

/// The material keys a factor file needs for this document: the fixed keys of the rules, and the
/// keys the attribute rules make from the lines (pavement materials, concrete grades).
pub fn needed_keys(lines: &[LineIn], settings: &Settings) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let mut add = |k: String| {
        if !keys.iter().any(|x| x.eq_ignore_ascii_case(&k)) {
            keys.push(k);
        }
    };
    for r in &settings.rules {
        match &r.use_ {
            Use::Material { key } | Use::HaulOnly { key } => add(key.clone()),
            Use::MaterialFromAttribute { attribute, prefix } => {
                for l in lines.iter().filter(|l| l.category == r.category) {
                    if let Some(v) = l.attributes.get(attribute) {
                        add(format!("{prefix}{v}"));
                    }
                }
            }
            _ => {}
        }
    }
    keys
}

/// Tonnes CO2e with a thousands separator, from kg.
pub fn t_co2e(kg: f64) -> String {
    format!("{} t", crate::crs_ui::thousands((kg / 1000.0 * 10.0).round() / 10.0))
}

/// The window's state for this session.
#[derive(Default)]
pub struct GreenUi {
    pub window_open: bool,
    pub last: Option<Assessment>,
    pub option_name: String,
}

fn opt_field(ui: &mut egui::Ui, label: &str, value: &mut Option<f64>, unit: &str, range: std::ops::RangeInclusive<f64>, speed: f64) -> bool {
    let mut changed = false;
    ui.label(label);
    ui.horizontal(|ui| {
        let mut on = value.is_some();
        if ui.checkbox(&mut on, "").on_hover_text("Tick to enter a value; unticked means not entered (nothing is assumed)").changed() {
            *value = if on { Some(*range.start()) } else { None };
            changed = true;
        }
        if let Some(v) = value.as_mut() {
            changed |= ui.add(egui::DragValue::new(v).range(range).speed(speed).suffix(format!(" {unit}"))).changed();
        } else {
            ui.label(egui::RichText::new("not entered").weak());
        }
    });
    ui.end_row();
    changed
}

fn use_text(u: &Use) -> String {
    match u {
        Use::Material { key } => format!("material: {key}"),
        Use::MaterialFromAttribute { attribute, prefix } => format!("material: {prefix}<{attribute}>"),
        Use::HaulOnly { key } => format!("haul only: {key}"),
        Use::LightingEnergy => "lighting energy".into(),
        Use::Excluded { reason } => format!("excluded: {reason}"),
    }
}

impl crate::app::AfroBimApp {
    /// Runs the assessment on the current model and keeps it for the window.
    pub(crate) fn green_assess(&mut self) {
        let signs = crate::cost_glue::sign_rows(self.engine.as_deref(), &self.doc);
        let a = assess_doc(&self.doc, &signs);
        self.status = match a.embodied_kgco2e {
            Some(kg) => format!("Green BIM: {} CO2e embodied ({} lines counted, {} unmatched).", t_co2e(kg), a.counted, a.unmatched),
            None => format!("Green BIM: no total ({}).", if load(&self.doc).factors.is_none() { "load a factor file first" } else { "no line could be counted" }),
        };
        self.green.last = Some(a);
    }

    /// The GREENBIM window.
    pub(crate) fn green_window(&mut self, ctx: &egui::Context) {
        if !self.green.window_open {
            return;
        }
        let mut g = load(&self.doc);
        let before = g.clone();
        let (mut load_file, mut save_template, mut run, mut save_option) = (false, false, false, false);
        let mut remove_option: Option<usize> = None;
        let last = self.green.last.clone();
        let dlg = crate::ui_kit::Dialog::titled("Green BIM").subtitle("Embodied carbon, waste and transport from the model's quantities; street lighting energy").width(760.0).show_close(
            ctx,
            |ui| {
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
                egui::Grid::new("green-lighting").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                    opt_field(ui, "Burning hours", &mut g.settings.lighting.hours_per_year, "h per year", 0.0..=8784.0, 10.0);
                    opt_field(ui, "Energy after dimming", &mut g.settings.lighting.dimming_factor, "(1 = no dimming)", 0.0..=1.0, 0.01);
                    opt_field(ui, "Design life", &mut g.settings.lighting.design_life_years, "years", 0.0..=100.0, 1.0);
                });
                ui.add_space(6.0);

                // Rules.
                egui::CollapsingHeader::new(egui::RichText::new("How each quantity is used").strong()).id_salt("green-rules").show(ui, |ui| {
                    crate::ui_kit::note(ui, "One quantity per family carries the material (the volume of a pavement layer, the mass of a post); the overlapping ones are excluded so nothing is counted twice. Waste and haul are counted only where entered.");
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
                                let counts = matches!(r.use_, Use::Material { .. } | Use::MaterialFromAttribute { .. } | Use::HaulOnly { .. });
                                for (j, (v, unit, max)) in [(&mut r.waste_pct, "%", 100.0), (&mut r.haul_km, "km", 1000.0)].into_iter().enumerate() {
                                    ui.push_id((i, j), |ui| {
                                        if !counts {
                                            ui.label("");
                                            return;
                                        }
                                        ui.horizontal(|ui| {
                                            let mut on = v.is_some();
                                            if ui.checkbox(&mut on, "").changed() {
                                                *v = on.then_some(0.0);
                                            }
                                            if let Some(x) = v.as_mut() {
                                                ui.add(egui::DragValue::new(x).range(0.0..=max).speed(0.5).suffix(format!(" {unit}")));
                                            }
                                        });
                                    });
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
                    let total = a.counted + a.excluded + a.unmatched + a.energy_lines;
                    match a.embodied_kgco2e {
                        Some(kg) => {
                            ui.label(egui::RichText::new(format!("Embodied carbon {} CO2e", t_co2e(kg))).size(18.0).strong());
                            if let Some(t) = a.transport_kgco2e {
                                crate::ui_kit::note(ui, format!("of which transport {} CO2e", t_co2e(t)));
                            }
                        }
                        None => {
                            ui.label(egui::RichText::new("No total: nothing could be counted").strong());
                        }
                    }
                    crate::ui_kit::note(ui, format!("{} of {total} quantity lines counted, {} excluded on purpose, {} unmatched, {} lighting. Factor file: {}.", a.counted, a.excluded, a.unmatched, a.energy_lines, if a.factor_set.is_empty() { "none" } else { &a.factor_set }));
                    if !a.by_material.is_empty() {
                        let rows: Vec<Vec<String>> = a.by_material.iter().map(|m| vec![m.material.clone(), format!("{:.1}", m.tonnes), format!("{:.1}", m.waste_t), t_co2e(m.kgco2e)]).collect();
                        crate::ui_kit::results_table(ui, "green-materials", &["Material", "Tonnes", "Waste t", "CO2e"], &[crate::ui_kit::Col::Text, crate::ui_kit::Col::Num, crate::ui_kit::Col::Num, crate::ui_kit::Col::Num], &rows);
                    }
                    let l = &a.lighting;
                    if l.luminaires > 0.0 {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Street lighting (operational, kept apart from embodied carbon)").strong());
                        let mut s = format!("{} luminaires, {:.2} kW installed", l.luminaires, l.installed_kw);
                        if let Some(k) = l.kwh_per_year {
                            s += &format!(", {} kWh per year", crate::crs_ui::thousands(k.round()));
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
                            crate::ui_kit::cell_num(ui, o.lighting_kgco2e_per_year.map(t_co2e).or(o.lighting_kwh_per_year.map(|k| format!("{} kWh", crate::crs_ui::thousands(k.round())))).unwrap_or_else(|| "-".into()));
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
        let export = dlg.inner.as_ref().and_then(|(_, s)| *s) == Some(0);
        if dlg.closed {
            self.green.window_open = false;
        }
        if let Some(i) = remove_option {
            g.options.remove(i);
        }
        if save_option {
            if let Some(a) = &self.green.last {
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
            }
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
        // Compared as JSON: a factor left blank in the file is NaN ("not entered"), which never
        // equals itself, so a plain comparison would store the record on every frame.
        if serde_json::to_value(&g).ok() != serde_json::to_value(&before).ok() {
            store(&mut self.doc, &g);
            self.schedule_commit_now();
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
        let mut g = load(&app.doc);
        g.factors = Some(afrobim_green::factors_from_csv(FACTORS).unwrap());
        store(&mut app.doc, &g);
        app.green_assess();
        let a = app.green.last.clone().unwrap();
        let vol = |m: &str| lines.iter().filter(|l| l.category == "pavement.layer_volume" && l.attributes.get("material").is_some_and(|x| x == m)).map(|l| l.quantity).sum::<f64>();
        let want = vol("Hot mix asphalt") * 2.0 * 50.0 + vol("Granular base") * 2.0 * 5.0;
        let got = a.embodied_kgco2e.expect("a total once factors are loaded");
        assert!((got - want).abs() < 1e-6 * want.max(1.0), "{got} vs {want}");
        // The record survives save and load.
        let back = Document::from_json(&app.doc.to_json()).unwrap();
        assert_eq!(load(&back).factors.map(|f| f.name), Some("Test factors".to_string()));
    }
}
