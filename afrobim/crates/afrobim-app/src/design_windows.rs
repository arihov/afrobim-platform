//! `ALIGNCHECK` (design checks against a limits table the user loads) and `OPTIONS` (an option set,
//! its comparison, weighted scoring and CSV export).
//!
//! No limit, weight or criterion is built in. With no limits table the check window says so and
//! offers a blank template; the option comparison shows every figure with how it was computed and
//! the scoring starts with no criteria.

use std::collections::HashMap;
use std::path::PathBuf;

use afrobim_design::{figure_keys, options_comparison_csv, score, scoring_csv, CheckReport, CheckStatus, Criterion, DesignBasis, Direction, OptionsComparison};
use afrobim_kernel::ElementId;
use egui::{Color32, RichText};

use crate::app::AfroBimApp;
use crate::design_glue::{self as glue, AlignCheckInputs, BuiltinChoice, LimitSource};
use crate::design_ui::{BAD, GOOD, WARN};

#[derive(Default)]
pub struct AlignCheckUi {
    pub open: bool,
    pub path: String,
    pub basis: Option<Result<DesignBasis, String>>,
    pub row: String,
    pub report: Option<Result<CheckReport, String>>,
    report_rev: u64,
    pub design: Option<ElementId>,
    /// Where the limits come from, and the built-in selection (class, terrain, speed, emax).
    pub source: LimitSource,
    pub builtin: BuiltinChoice,
}

impl AlignCheckUi {
    /// The inputs as stored on the alignment.
    fn inputs(&self) -> AlignCheckInputs {
        AlignCheckInputs { limits_path: self.path.trim().to_string(), row_label: self.row.clone(), source: self.source, builtin: self.builtin }
    }
}

#[derive(Default)]
pub struct OptionsUi {
    pub open: bool,
    pub set: Option<ElementId>,
    pub new_set: String,
    pub new_option: String,
    pub rename: HashMap<usize, String>,
    pub cmp: Option<(String, Result<OptionsComparison, String>)>,
    pub weights: HashMap<usize, String>,
    pub crit_key: Option<String>,
    pub crit_weight: String,
    pub crit_dir: Option<Direction>,
    pub message: String,
}

fn status_colour(s: CheckStatus) -> Color32 {
    match s {
        CheckStatus::Pass => GOOD(),
        CheckStatus::Fail => BAD(),
        CheckStatus::NotChecked => WARN(),
    }
}

fn status_text(s: CheckStatus) -> &'static str {
    match s {
        CheckStatus::Pass => "Pass",
        CheckStatus::Fail => "Fail",
        CheckStatus::NotChecked => "Not checked",
    }
}

/// A figure's value as text: counts as whole numbers, lengths (metres) in the document's display
/// unit, everything else to three decimals.
fn figure_text(u: &afrobim_annotate::DisplayUnits, value: f64, unit: &str) -> String {
    match unit {
        "nr" => format!("{value:.0}"),
        "m" => crate::disp::len(u, value),
        _ => format!("{value:.3}"),
    }
}

/// The unit column of the comparison: a length (metres) follows the document's length unit; areas,
/// volumes, percentages, degrees and counts have their own.
fn figure_unit(u: &afrobim_annotate::DisplayUnits, unit: &str) -> String {
    if unit == "m" { crate::disp::suffix(u).to_string() } else { unit.to_string() }
}

/// The value and the limit of a check row for the results table. A length (metres) is shown in the
/// document's display unit, with the unit; a grade (percent) is not a length and stays as it is.
/// The row's own message (the engine's calculation text) and the limit's source citation, which
/// keeps the unit the limits table printed, are not touched.
fn check_cells(u: &afrobim_annotate::DisplayUnits, i: &afrobim_design::CheckItem) -> (String, String) {
    let shown = |v: Option<f64>| match (v, i.unit.as_str()) {
        (None, _) => "none".to_string(),
        (Some(v), "m") => crate::disp::len_u(u, v),
        (Some(v), unit) => format!("{v:.3} {unit}"),
    };
    (shown(i.value), shown(i.limit))
}

/// The class, terrain, design speed and emax pickers, fed from the engine's list of built-in bases.
fn builtin_picker(ui: &mut egui::Ui, b: &mut BuiltinChoice) {
    let bases = afrobim_design::builtin_bases();
    let Some(info) = bases.first() else { return };
    ui.label(RichText::new(&info.name).small());
    crate::ui_kit::grid(ui, "chk-builtin", |ui| {
        ui.label("Road class");
        let shown = b.class.and_then(|c| info.classes.iter().find(|x| x.class == c)).map(|x| x.name.to_string()).unwrap_or_else(|| "choose".into());
        egui::ComboBox::from_id_salt("chk-class").selected_text(shown).show_ui(ui, |ui| {
            for c in &info.classes {
                if ui.selectable_value(&mut b.class, Some(c.class), c.name).clicked() {
                    b.terrain = None;
                    b.speed_kmh = None;
                }
            }
        });
        ui.end_row();
        let class = b.class.and_then(|c| info.classes.iter().find(|x| x.class == c));
        ui.label("Terrain");
        let shown = b.terrain.and_then(|t| class.and_then(|c| c.terrains.iter().find(|x| x.terrain == t))).map(|x| x.name.to_string()).unwrap_or_else(|| "choose".into());
        ui.add_enabled_ui(class.is_some(), |ui| {
            egui::ComboBox::from_id_salt("chk-terrain").selected_text(shown).show_ui(ui, |ui| {
                if let Some(class) = class {
                    for t in &class.terrains {
                        if ui.selectable_value(&mut b.terrain, Some(t.terrain), format!("{} (up to {} km/h)", t.name, t.max_design_speed_kmh)).clicked() {
                            b.speed_kmh = None;
                        }
                    }
                }
            });
        });
        ui.end_row();
        if let Some(class) = class {
            for (terrain, why) in &class.refused_terrains {
                ui.label("");
                ui.label(RichText::new(format!("{terrain:?} is not offered for this class: {why}")).small().weak());
                ui.end_row();
            }
        }
        ui.label("Design speed");
        let terrain = b.terrain.and_then(|t| class.and_then(|c| c.terrains.iter().find(|x| x.terrain == t)));
        let shown = b.speed_kmh.map(|s| format!("{s} km/h")).unwrap_or_else(|| "choose".into());
        ui.add_enabled_ui(terrain.is_some(), |ui| {
            egui::ComboBox::from_id_salt("chk-speed").selected_text(shown).show_ui(ui, |ui| {
                if let Some(t) = terrain {
                    for s in &t.speeds_kmh {
                        ui.selectable_value(&mut b.speed_kmh, Some(*s), format!("{s} km/h"));
                    }
                }
            });
        });
        ui.end_row();
        ui.label("Maximum superelevation emax");
        let shown = b.emax_percent.map(|e| format!("{e} %")).unwrap_or_else(|| "choose".into());
        egui::ComboBox::from_id_salt("chk-emax").selected_text(shown).show_ui(ui, |ui| {
            for e in &info.emax_percent_options {
                ui.selectable_value(&mut b.emax_percent, Some(*e), format!("{e} %"));
            }
        });
        ui.end_row();
        ui.label("Maximum gradient");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut b.absolute_grade, false, "desirable");
            ui.selectable_value(&mut b.absolute_grade, true, "absolute");
        });
        ui.end_row();
        ui.label("Minimum grade in cut");
        ui.checkbox(&mut b.minimum_grade_in_cut, "check the 0.5 % minimum (applied to every grade, so use it only when the alignment is in cut throughout)");
        ui.end_row();
    });
}

/// What the manual does not cover, and the slips found in it.
fn mowt_panels(ui: &mut egui::Ui) {
    // A screenshot aid: AFROBIM_AID_OPEN_PANELS opens the panels so that they can be captured.
    let open = std::env::var_os("AFROBIM_AID_OPEN_PANELS").is_some();
    egui::CollapsingHeader::new("Checks the manual does not cover (stay Not checked)").id_salt("chk-notcovered").default_open(open).show(ui, |ui| {
        for n in afrobim_design::mowt::not_checked_under_mowt_2010() {
            ui.add(egui::Label::new(RichText::new(format!("{}: {}. {}", n.check_id, n.what, n.reason)).small()).wrap());
        }
    });
    egui::CollapsingHeader::new("Known slips in the manual").id_salt("chk-slips").default_open(open).show(ui, |ui| {
        ui.add(egui::Label::new(RichText::new(format!("{} Where a table cell contradicts another table of the same manual it is withheld and the check is reported Not checked, or derived and labelled so; the engineer decides which figure governs.", glue::THIRD_PARTY_NOTE)).small().color(WARN())).wrap());
        for s in afrobim_design::mowt::known_slips() {
            ui.add(egui::Label::new(RichText::new(format!("{} ({}): {} Treatment: {}", s.id, s.place, s.what, s.treatment)).small()).wrap());
        }
    });
}

impl AfroBimApp {
    pub(crate) fn design_open_check(&mut self) {
        match self.design_target() {
            Some(d) => {
                let c = &mut self.design.check;
                c.open = true;
                c.design = Some(d);
                // `--limits` preloads a table and `--limits-row` runs the checks against one of its rows.
                if let (true, Some(p)) = (c.path.is_empty(), self.options.limits.clone()) {
                    c.path = p.display().to_string();
                    c.basis = Some(glue::load_basis(&p));
                    if let (Some(row), Some(Ok(b))) = (self.options.limits_row.clone(), &c.basis) {
                        if b.rows.iter().any(|r| r.label == row) {
                            c.row = row.clone();
                            c.report = glue::load(&self.doc, d).and_then(|design| glue::run_check(&design, b, &row)).into();
                            c.report_rev = self.design.revision;
                        }
                    }
                }
                // The inputs kept on the alignment open again, and are run again at once so that the
                // window shows the stored selection's results.
                if let Some(a) = glue::alignment_of_design(&self.doc, d) {
                    if let Some(stored) = glue::stored_check(&self.doc, a) {
                        if c.path.is_empty() && c.row.is_empty() && c.builtin == BuiltinChoice::default() {
                            c.source = stored.source;
                            c.builtin = stored.builtin;
                            if !stored.limits_path.is_empty() {
                                c.path = stored.limits_path.clone();
                                c.basis = Some(glue::load_basis(std::path::Path::new(&stored.limits_path)));
                            }
                            c.row = stored.row_label.clone();
                            if let Ok((basis, label)) = glue::resolve_basis(&stored) {
                                c.report = glue::load(&self.doc, d).and_then(|design| glue::run_check(&design, &basis, &label)).into();
                                c.report_rev = self.design.revision;
                            }
                        }
                    }
                }
            }
            None => self.status = "There is no designed alignment to check. Make one with ALIGNDESIGN.".into(),
        }
    }

    pub(crate) fn align_check_window(&mut self, ctx: &egui::Context) {
        let designs = glue::designs(&self.doc);
        let mut c = std::mem::take(&mut self.design.check);
        let revision = self.design.revision;
        let mut open = true;
        let mut run = false;
        let mut load: Option<PathBuf> = None;
        let mut template = false;
        let mut csv = false;
        let units = crate::anno_ui::units_of(&self.doc);
        let dlg = crate::ui_kit::Dialog::titled("ALIGNCHECK: design checks").width(860.0).show_body(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Alignment");
                let current = c.design.and_then(|d| designs.iter().find(|x| x.0 == d)).map(|x| x.2.clone()).unwrap_or_else(|| "choose".into());
                egui::ComboBox::from_id_salt("chk-design").selected_text(current).show_ui(ui, |ui| {
                    for (d, _, n) in &designs {
                        if ui.selectable_label(c.design == Some(*d), n).clicked() {
                            c.design = Some(*d);
                            c.report = None;
                        }
                    }
                });
            });
            ui.separator();
            ui.label(RichText::new("Limits source").strong());
            ui.horizontal_wrapped(|ui| {
                for s in LimitSource::ALL {
                    if ui.selectable_value(&mut c.source, s, s.label()).clicked() {
                        c.report = None;
                    }
                }
            });
            if c.source != LimitSource::UserFile {
                builtin_picker(ui, &mut c.builtin);
                ui.label(RichText::new(glue::THIRD_PARTY_NOTE).small().weak());
                mowt_panels(ui);
            }
            if c.source != LimitSource::Builtin {
            ui.separator();
            ui.label(RichText::new(if c.source == LimitSource::BuiltinWithOverlay { "Your limits table (its stated values replace the built-in ones)" } else { "Limits table" }).strong());
            ui.scope(|ui| {
            ui.horizontal(|ui| {
                ui.add_sized([420.0, 20.0], egui::TextEdit::singleline(&mut c.path).hint_text("path of a limits file (CSV or JSON)"));
                if ui.button("Browse...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Limits table", &["csv", "json"]).pick_file() {
                        load = Some(p);
                    }
                }
                if ui.button("Load").clicked() && !c.path.trim().is_empty() {
                    load = Some(PathBuf::from(c.path.trim()));
                }
                if ui.button("Save a blank limits template...").clicked() {
                    template = true;
                }
            });
            });
            }
            let builtin_ready = c.source == LimitSource::Builtin && c.builtin.selection().is_ok();
            if c.source == LimitSource::Builtin {
                // The built-in basis needs no file: the run button is here.
                ui.horizontal(|ui| {
                    if ui.add_enabled(builtin_ready && c.design.is_some(), egui::Button::new("Run the checks")).clicked() {
                        run = true;
                    }
                    if let Err(why) = c.builtin.selection() {
                        ui.label(RichText::new(format!("Still to choose: {why}.")).small().color(WARN()));
                    }
                });
            }
            match if c.source == LimitSource::Builtin { &None } else { &c.basis } {
                None if c.source == LimitSource::Builtin => {}
                None => {
                    egui::Frame::new().fill(Color32::from_rgb(0x4A, 0x36, 0x12)).inner_margin(8.0).corner_radius(4.0).show(ui, |ui| {
                        ui.label(RichText::new("No limits are loaded").strong().color(Color32::from_rgb(0xFF, 0xD0, 0x80)));
                        ui.label(RichText::new(glue::NO_LIMITS_TEXT).color(Color32::from_rgb(0xF2, 0xE0, 0xC0)));
                        ui.label(RichText::new("Until a table is loaded every check of this alignment reads Not checked, in CHECKALL too.").small().color(Color32::from_rgb(0xF2, 0xE0, 0xC0)));
                    });
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(e).color(BAD()));
                }
                Some(Ok(b)) => {
                    ui.label(format!("Loaded: {} with {} row(s).", b.name, b.rows.len()));
                    ui.label(RichText::new(format!("Source of the limits: {}", b.source)).small());
                    ui.horizontal(|ui| {
                        ui.label("Design speed row");
                        let shown = if c.row.is_empty() { "choose".to_string() } else { c.row.clone() };
                        egui::ComboBox::from_id_salt("chk-row").selected_text(shown).show_ui(ui, |ui| {
                            for r in &b.rows {
                                let text = match r.design_speed_kmh {
                                    Some(v) => format!("{} ({v} km/h)", r.label),
                                    None => r.label.clone(),
                                };
                                if ui.selectable_label(c.row == r.label, text).clicked() {
                                    c.row = r.label.clone();
                                    c.report = None;
                                }
                            }
                        });
                        if ui.add_enabled(!c.row.is_empty() && c.design.is_some() && (c.source == LimitSource::UserFile || c.builtin.selection().is_ok()), egui::Button::new("Run the checks")).clicked() {
                            run = true;
                        }
                    });
                }
            }
            ui.separator();
            match &c.report {
                None => {
                    ui.label(RichText::new("No results yet.").weak());
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(e).color(BAD()));
                }
                Some(Ok(r)) => {
                    if c.report_rev != revision {
                        ui.label(RichText::new("The design changed since these results: run the checks again.").color(WARN()));
                    }
                    ui.label(format!("{} pass, {} fail, {} not checked. Limits: {}; row {}.", r.count(CheckStatus::Pass), r.count(CheckStatus::Fail), r.count(CheckStatus::NotChecked), r.basis_name, r.row_label));
                    ui.label(RichText::new(format!("Source string of the limits table: {}", r.source)).small());
                    if ui.button("Save the results as CSV...").clicked() {
                        csv = true;
                    }
                    egui::ScrollArea::both().auto_shrink([false, true]).max_height(330.0).show(ui, |ui| {
                        // By element: the worst status of each PI, PVI or grade.
                        ui.label(RichText::new("By element").strong());
                        egui::Grid::new("chk-elements").num_columns(5).striped(true).spacing([14.0, 3.0]).show(ui, |ui| {
                            for h in ["Element", "Status", "Pass", "Fail", "Not checked"] {
                                ui.label(RichText::new(h).strong());
                            }
                            ui.end_row();
                            let mut seen: Vec<&str> = Vec::new();
                            for i in &r.items {
                                if seen.contains(&i.location.as_str()) {
                                    continue;
                                }
                                seen.push(&i.location);
                                let of = |s| r.items.iter().filter(|x| x.location == i.location && x.status == s).count();
                                let worst = if of(CheckStatus::Fail) > 0 { CheckStatus::Fail } else if of(CheckStatus::NotChecked) > 0 { CheckStatus::NotChecked } else { CheckStatus::Pass };
                                ui.label(&i.location);
                                ui.label(RichText::new(status_text(worst)).color(status_colour(worst)));
                                ui.label(of(CheckStatus::Pass).to_string());
                                ui.label(of(CheckStatus::Fail).to_string());
                                ui.label(of(CheckStatus::NotChecked).to_string());
                                ui.end_row();
                            }
                        });
                        ui.add_space(6.0);
                        ui.label(RichText::new("Every check").strong());
                        ui.label(RichText::new("Values and limits are shown in the document's units. The source of a limit keeps the unit the limits table prints, and the message is the engine's own text.").small().weak());
                        egui::Grid::new("chk-items").num_columns(7).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
                            for h in ["Element", "Check", "Value", "Limit", "Status", "Source of the limit", "Message"] {
                                ui.label(RichText::new(h).strong());
                            }
                            ui.end_row();
                            for i in &r.items {
                                ui.label(&i.location);
                                ui.label(&i.description);
                                let (value, limit) = check_cells(&units, i);
                                ui.label(value);
                                ui.label(limit);
                                ui.label(RichText::new(status_text(i.status)).color(status_colour(i.status)));
                                ui.label(RichText::new(format!("{} ({})", i.limit_basis, r.source)).small());
                                ui.label(RichText::new(&i.message).small()).on_hover_text(&i.message);
                                ui.end_row();
                            }
                        });
                    });
                }
            }
            ui.label(RichText::new("These are checks of the geometry against the limits chosen: lateral sight distance, passing and decision sight distance, superelevation and combinations of horizontal and vertical alignment are not checked. A pass is not a certification.").small().weak());
        });
        if dlg.closed {
            open = false;
        }
        if let Some(p) = load {
            c.path = p.display().to_string();
            c.basis = Some(glue::load_basis(&p));
            if let Some(Ok(b)) = &c.basis {
                if !b.rows.iter().any(|r| r.label == c.row) {
                    c.row.clear();
                }
            }
            c.report = None;
        }
        if run {
            if let Some(d) = c.design {
                let inputs = c.inputs();
                let result = glue::resolve_basis(&inputs).and_then(|(basis, label)| glue::load(&self.doc, d).and_then(|design| glue::run_check(&design, &basis, &label)));
                if result.is_ok() {
                    if let Some(a) = glue::alignment_of_design(&self.doc, d) {
                        glue::store_check(&mut self.doc, a, &inputs);
                        self.schedule_commit_now();
                        self.status = "Checked. The limits selection is kept on the alignment (check.alignment) for CHECKALL and the report pack.".into();
                    }
                }
                c.report_rev = revision;
                c.report = Some(result);
            }
        }
        if template {
            if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("alignment-limits-template.csv").save_file() {
                if path.extension().is_none() {
                    path.set_extension("csv");
                }
                self.status = match std::fs::write(&path, glue::blank_template()) {
                    Ok(()) => format!("Saved a blank limits template to {}: fill in the source and the values from your standard.", path.display()),
                    Err(e) => format!("Could not write {}: {e}", path.display()),
                };
            }
        }
        if csv {
            if let Some(Ok(r)) = &c.report {
                if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("alignment-checks.csv").save_file() {
                    if path.extension().is_none() {
                        path.set_extension("csv");
                    }
                    self.status = match std::fs::write(&path, r.to_csv()) {
                        Ok(()) => format!("Saved the check results to {}.", path.display()),
                        Err(e) => format!("Could not write {}: {e}", path.display()),
                    };
                }
            }
        }
        if !open {
            c.open = false;
        }
        self.design.check = c;
    }

    pub(crate) fn design_open_options(&mut self) {
        self.design.options.open = true;
        if self.design.options.set.is_none_or(|s| self.doc.find(s).is_none()) {
            self.design.options.set = glue::option_sets(&self.doc).first().map(|s| s.0);
        }
        self.design_target();
    }

    pub(crate) fn options_window(&mut self, ctx: &egui::Context) {
        let mut o = std::mem::take(&mut self.design.options);
        let sets = glue::option_sets(&self.doc);
        if o.set.is_some_and(|s| self.doc.find(s).is_none()) {
            o.set = None;
        }
        if o.set.is_none() {
            o.set = sets.first().map(|s| s.0);
        }
        let designs = glue::designs(&self.doc);
        let surfaces = afrobim_corridor::surfaces(&self.doc);
        let estimates = crate::cost_glue::list(&self.doc);
        let corridors: Vec<(ElementId, String)> = self.doc.elements().iter().filter(|e| e.category == afrobim_corridor::CORRIDOR_CATEGORY).map(|e| (e.id, e.name.clone())).collect();
        let target = self.design.active;
        let mut open = true;
        let mut action: Option<Box<dyn FnOnce(&mut AfroBimApp) -> Result<String, String>>> = None;
        let set_id = o.set;
        let loaded = set_id.map(|s| glue::load_set(&self.doc, s));
        // The comparison is worked out again only when the set or the model changed.
        let key = format!("{}|{}|{}", set_id.and_then(|s| self.doc.find(s)).map(|e| e.data.to_string()).unwrap_or_default(), self.design.revision, self.doc.elements().len());
        if let (Some(Ok(_)), Some(s)) = (&loaded, set_id) {
            if o.cmp.as_ref().is_none_or(|(k, _)| *k != key) {
                o.cmp = Some((key.clone(), glue::compare(&self.doc, s)));
            }
        } else {
            o.cmp = None;
        }
        let dlg = crate::ui_kit::Dialog::titled("OPTIONS: alignment options").width(1040.0).show_body(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Option set");
                    let current = set_id.and_then(|s| sets.iter().find(|x| x.0 == s)).map(|x| x.1.clone()).unwrap_or_else(|| "none yet".into());
                    egui::ComboBox::from_id_salt("opt-set").selected_text(current).show_ui(ui, |ui| {
                        for (id, n) in &sets {
                            if ui.selectable_label(set_id == Some(*id), n).clicked() {
                                o.set = Some(*id);
                            }
                        }
                    });
                    ui.add_sized([180.0, 20.0], egui::TextEdit::singleline(&mut o.new_set).hint_text("name of a new set"));
                    if ui.button("New set").clicked() {
                        let name = o.new_set.clone();
                        action = Some(Box::new(move |app| {
                            let id = glue::new_set(&mut app.doc, &name)?;
                            app.design.options.set = Some(id);
                            app.design.options.new_set.clear();
                            Ok(format!("Option set {name} made."))
                        }));
                    }
                });
                if !o.message.is_empty() {
                    ui.label(RichText::new(&o.message).color(WARN()));
                }
                let (Some(set_id), Some(loaded)) = (set_id, loaded) else {
                    ui.label("There is no option set yet. Name one above and press New set, then add the current design to it as an option.");
                    return;
                };
                let set = match loaded {
                    Ok(s) => s,
                    Err(e) => {
                        ui.label(RichText::new(e).color(BAD()));
                        return;
                    }
                };
                ui.separator();
                ui.label(RichText::new("What the options are measured against").strong());
                ui.horizontal(|ui| {
                    ui.label("Ground surface");
                    let cur = set.surface.and_then(|s| surfaces.iter().find(|x| x.0 == s)).map(|x| x.1.clone()).unwrap_or_else(|| "none".into());
                    egui::ComboBox::from_id_salt("opt-surface").selected_text(cur).show_ui(ui, |ui| {
                        if ui.selectable_label(set.surface.is_none(), "none").clicked() {
                            let mut s = set.clone();
                            s.surface = None;
                            action = Some(Box::new(move |app| glue::store_set(&mut app.doc, set_id, &s).map(|_| "Surface cleared.".into())));
                        }
                        for (id, n) in &surfaces {
                            if ui.selectable_label(set.surface == Some(*id), n).clicked() {
                                let mut s = set.clone();
                                s.surface = Some(*id);
                                action = Some(Box::new(move |app| glue::store_set(&mut app.doc, set_id, &s).map(|_| "Surface set.".into())));
                            }
                        }
                    });
                    ui.label("Estimate (your rates)");
                    let cur = set.estimate.and_then(|s| estimates.iter().find(|x| x.0 == s)).map(|x| x.1.clone()).unwrap_or_else(|| "none".into());
                    egui::ComboBox::from_id_salt("opt-estimate").selected_text(cur).show_ui(ui, |ui| {
                        if ui.selectable_label(set.estimate.is_none(), "none").clicked() {
                            let mut s = set.clone();
                            s.estimate = None;
                            action = Some(Box::new(move |app| glue::store_set(&mut app.doc, set_id, &s).map(|_| "Estimate cleared.".into())));
                        }
                        for (id, n) in &estimates {
                            if ui.selectable_label(set.estimate == Some(*id), n).clicked() {
                                let mut s = set.clone();
                                s.estimate = Some(*id);
                                action = Some(Box::new(move |app| glue::store_set(&mut app.doc, set_id, &s).map(|_| "Estimate set.".into())));
                            }
                        }
                    });
                    ui.label("Typical section from corridor");
                    egui::ComboBox::from_id_salt("opt-corridor").selected_text(if set.options.iter().any(|x| x.corridor.is_some()) { "copied" } else { "none" }).show_ui(ui, |ui| {
                        if ui.selectable_label(false, "none").clicked() {
                            action = Some(Box::new(move |app| glue::set_corridor_from(&mut app.doc, set_id, None).map(|_| "Typical section cleared.".into())));
                        }
                        for (id, n) in &corridors {
                            if ui.selectable_label(false, n).clicked() {
                                let id = *id;
                                action = Some(Box::new(move |app| glue::set_corridor_from(&mut app.doc, set_id, Some(id)).map(|_| "Typical section copied to every option.".into())));
                            }
                        }
                    });
                });
                ui.label(RichText::new("Cut, fill and land take need a ground surface and a typical section; cost needs your estimate. A figure that cannot be computed says why.").small().weak());
                ui.separator();
                ui.label(RichText::new("Options").strong());
                ui.horizontal(|ui| {
                    ui.label("Add the current design as");
                    ui.add_sized([160.0, 20.0], egui::TextEdit::singleline(&mut o.new_option).hint_text("option name"));
                    let cur = target.and_then(|d| designs.iter().find(|x| x.0 == d)).map(|x| x.2.clone()).unwrap_or_else(|| "choose".into());
                    egui::ComboBox::from_id_salt("opt-current").selected_text(cur).show_ui(ui, |ui| {
                        for (d, _, n) in &designs {
                            if ui.selectable_label(target == Some(*d), n).clicked() {
                                action = Some(Box::new({
                                    let d = *d;
                                    move |app| {
                                        app.design.active = Some(d);
                                        app.design.invalidate();
                                        Ok("Current design chosen.".into())
                                    }
                                }));
                            }
                        }
                    });
                    if ui.add_enabled(target.is_some(), egui::Button::new("Add option")).clicked() {
                        let name = o.new_option.clone();
                        if let Some(d) = target {
                            action = Some(Box::new(move |app| {
                                glue::add_option(&mut app.doc, set_id, d, &name)?;
                                app.design.options.new_option.clear();
                                Ok(format!("Option {name} added."))
                            }));
                        }
                    }
                });
                egui::Grid::new("opt-list").num_columns(3).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                    for (i, opt) in set.options.iter().enumerate() {
                        let text = o.rename.entry(i).or_insert_with(|| opt.name.clone());
                        let r = ui.add_sized([200.0, 20.0], egui::TextEdit::singleline(text));
                        if r.lost_focus() && *text != opt.name {
                            let new = text.clone();
                            action = Some(Box::new(move |app| glue::rename_option(&mut app.doc, set_id, i, &new).map(|_| format!("Renamed to {new}."))));
                        }
                        ui.label(RichText::new(match &opt.design.vertical { Some(_) => "plan and profile", None => "plan only" }).small().weak());
                        ui.horizontal(|ui| {
                            if ui.small_button("Duplicate").clicked() {
                                action = Some(Box::new(move |app| glue::duplicate_option(&mut app.doc, set_id, i).map(|_| "Option duplicated.".into())));
                            }
                            if ui.small_button("Delete").clicked() {
                                action = Some(Box::new(move |app| glue::delete_option(&mut app.doc, set_id, i).map(|_| "Option deleted.".into())));
                            }
                            if ui.add_enabled(target.is_some(), egui::Button::new("Make this option current").small()).on_hover_text("Replaces the PIs and PVIs of the current design with this option's, and resynchronises everything on the alignment.").clicked() {
                                if let Some(d) = target {
                                    action = Some(Box::new(move |app| {
                                        let r = glue::make_current(&mut app.doc, set_id, i, d)?;
                                        let note = r.note();
                                        app.design_after_edit(r);
                                        Ok(format!("Option made current. {note}"))
                                    }));
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
                ui.separator();
                ui.label(RichText::new("Comparison: the figures of each option, unweighted").strong());
                match o.cmp.as_ref().map(|(_, c)| c) {
                    Some(Ok(cmp)) if !cmp.options.is_empty() => {
                        let units = crate::anno_ui::units_of(&self.doc);
                        egui::Grid::new("opt-cmp").num_columns(cmp.options.len() + 2).striped(true).spacing([14.0, 3.0]).show(ui, |ui| {
                            ui.label(RichText::new("Figure").strong());
                            ui.label(RichText::new("Unit").strong());
                            for r in &cmp.options {
                                ui.label(RichText::new(&r.name).strong());
                            }
                            ui.end_row();
                            for (key, label, unit) in figure_keys() {
                                ui.label(label);
                                ui.label(RichText::new(figure_unit(&units, unit)).weak());
                                for r in &cmp.options {
                                    match r.figure(key) {
                                        Some(f) if f.value.is_some() => {
                                            let shown = f.exact.clone().unwrap_or_else(|| figure_text(&units, f.value.unwrap_or_default(), &f.unit));
                                            ui.label(shown).on_hover_text(&f.how);
                                        }
                                        Some(f) => {
                                            ui.label(RichText::new("not computed").weak()).on_hover_text(f.not_computed.clone().unwrap_or_default());
                                        }
                                        None => {
                                            ui.label("");
                                        }
                                    }
                                }
                                ui.end_row();
                            }
                        });
                        ui.add_space(4.0);
                        egui::CollapsingHeader::new("How each figure is computed").default_open(true).show(ui, |ui| {
                            for (key, label, _) in figure_keys() {
                                let how = cmp.options.iter().filter_map(|r| r.figure(key)).map(|f| f.how.clone()).find(|h| !h.is_empty());
                                let why = cmp.options.iter().filter_map(|r| r.figure(key)).filter_map(|f| f.not_computed.clone()).next();
                                match (how, why) {
                                    (Some(h), _) => ui.label(RichText::new(format!("{label}: {h}.")).small()),
                                    (None, Some(w)) => ui.label(RichText::new(format!("{label}: not computed here, because {w}.")).small().color(WARN())),
                                    _ => ui.label(""),
                                };
                            }
                        });
                        for r in &cmp.options {
                            if r.notes.is_empty() && r.error.is_none() {
                                continue;
                            }
                            egui::CollapsingHeader::new(format!("Notes: {}", r.name)).default_open(false).show(ui, |ui| {
                                if let Some(e) = &r.error {
                                    ui.label(RichText::new(format!("Cannot be built: {e}")).color(BAD()));
                                }
                                for n in &r.notes {
                                    ui.label(RichText::new(n).small());
                                }
                            });
                        }
                        if ui.button("Save the comparison as CSV...").clicked() {
                            if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("options-comparison.csv").save_file() {
                                if path.extension().is_none() {
                                    path.set_extension("csv");
                                }
                                let text = options_comparison_csv(cmp);
                                let note = match std::fs::write(&path, text) {
                                    Ok(()) => format!("Saved the comparison to {}.", path.display()),
                                    Err(e) => format!("Could not write {}: {e}", path.display()),
                                };
                                action = Some(Box::new(move |_| Ok(note)));
                            }
                        }
                        ui.separator();
                        ui.label(RichText::new("Weighted scoring").strong());
                        ui.label(RichText::new("You choose the criteria, their weights and which way is better. Each criterion is scaled 0 to 1 between the worst and the best option and weighted by your weight. None is prefilled, and the unweighted figures above stay in view.").small().weak());
                        egui::Grid::new("opt-criteria").num_columns(4).spacing([10.0, 4.0]).show(ui, |ui| {
                            for (i, c) in set.criteria.iter().enumerate() {
                                let label = figure_keys().iter().find(|k| k.0 == c.key).map(|k| k.1.to_string()).unwrap_or_else(|| c.key.clone());
                                ui.label(label);
                                let text = o.weights.entry(i).or_insert_with(|| format!("{}", c.weight));
                                let r = ui.add_sized([60.0, 20.0], egui::TextEdit::singleline(text));
                                if r.lost_focus() {
                                    match text.trim().parse::<f64>().ok().filter(|w| w.is_finite() && *w >= 0.0) {
                                        Some(w) => {
                                            let mut s = set.clone();
                                            s.criteria[i].weight = w;
                                            action = Some(Box::new(move |app| glue::store_set(&mut app.doc, set_id, &s).map(|_| "Weight changed.".into())));
                                        }
                                        None => o.message = "A weight must be a number, zero or more.".into(),
                                    }
                                }
                                ui.label(if c.direction == Direction::LowerIsBetter { "lower is better" } else { "higher is better" });
                                if ui.small_button("Remove").clicked() {
                                    let mut s = set.clone();
                                    s.criteria.remove(i);
                                    o.weights.clear();
                                    action = Some(Box::new(move |app| glue::store_set(&mut app.doc, set_id, &s).map(|_| "Criterion removed.".into())));
                                }
                                ui.end_row();
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Add a criterion");
                            let shown = o.crit_key.as_ref().and_then(|k| figure_keys().iter().find(|x| x.0 == k).map(|x| x.1.to_string())).unwrap_or_else(|| "choose a figure".into());
                            egui::ComboBox::from_id_salt("crit-key").selected_text(shown).show_ui(ui, |ui| {
                                for (k, label, _) in figure_keys() {
                                    ui.selectable_value(&mut o.crit_key, Some(k.to_string()), label);
                                }
                            });
                            ui.add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut o.crit_weight).hint_text("weight"));
                            egui::ComboBox::from_id_salt("crit-dir").selected_text(match o.crit_dir { Some(Direction::LowerIsBetter) => "lower is better", Some(Direction::HigherIsBetter) => "higher is better", None => "which way is better" }).show_ui(ui, |ui| {
                                ui.selectable_value(&mut o.crit_dir, Some(Direction::LowerIsBetter), "lower is better");
                                ui.selectable_value(&mut o.crit_dir, Some(Direction::HigherIsBetter), "higher is better");
                            });
                            if ui.button("Add").clicked() {
                                match (o.crit_key.clone(), o.crit_weight.trim().parse::<f64>().ok().filter(|w| w.is_finite() && *w >= 0.0), o.crit_dir) {
                                    (Some(key), Some(weight), Some(direction)) => {
                                        let mut s = set.clone();
                                        s.criteria.push(Criterion { key, weight, direction });
                                        o.crit_key = None;
                                        o.crit_weight.clear();
                                        o.crit_dir = None;
                                        o.weights.clear();
                                        action = Some(Box::new(move |app| glue::store_set(&mut app.doc, set_id, &s).map(|_| "Criterion added.".into())));
                                    }
                                    _ => o.message = "Choose the figure, enter a weight (a number, zero or more) and choose which way is better.".into(),
                                }
                            }
                        });
                        if !set.criteria.is_empty() {
                            let table = score(cmp, &set.criteria);
                            egui::Grid::new("opt-score").num_columns(set.criteria.len() + 2).striped(true).spacing([14.0, 3.0]).show(ui, |ui| {
                                ui.label(RichText::new("Option").strong());
                                for c in &table.criteria {
                                    ui.label(RichText::new(figure_keys().iter().find(|k| k.0 == c.key).map(|k| k.1).unwrap_or(&c.key)).strong());
                                }
                                ui.label(RichText::new("Weighted score").strong());
                                ui.end_row();
                                for r in &table.options {
                                    ui.label(&r.name);
                                    for cell in &r.cells {
                                        match (cell.raw, cell.scaled) {
                                            (Some(raw), Some(sc)) => ui.label(format!("{raw:.3} (scaled {sc:.2})")),
                                            _ => ui.label(RichText::new("n/a").weak()),
                                        };
                                    }
                                    match (r.score, &r.reason) {
                                        (Some(s), _) => ui.label(RichText::new(format!("{s:.3}")).strong()),
                                        (None, Some(why)) => ui.label(RichText::new(why).small().color(WARN())),
                                        _ => ui.label(""),
                                    };
                                    ui.end_row();
                                }
                            });
                            if ui.button("Save the scoring as CSV...").clicked() {
                                if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("options-scoring.csv").save_file() {
                                    if path.extension().is_none() {
                                        path.set_extension("csv");
                                    }
                                    let note = match std::fs::write(&path, scoring_csv(&table)) {
                                        Ok(()) => format!("Saved the scoring to {}.", path.display()),
                                        Err(e) => format!("Could not write {}: {e}", path.display()),
                                    };
                                    action = Some(Box::new(move |_| Ok(note)));
                                }
                            }
                        }
                    }
                    Some(Ok(_)) => {
                        ui.label("The set has no options yet. Add the current design above.");
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(BAD()));
                    }
                    None => {}
                }
            });
        });
        if dlg.closed {
            open = false;
        }
        if !open {
            o.open = false;
        }
        self.design.options = o;
        if let Some(a) = action {
            match a(self) {
                Ok(text) => {
                    self.design.options.message.clear();
                    self.status = text;
                    self.design.options.rename.clear();
                    self.design.options.weights.clear();
                    self.design.invalidate();
                    self.schedule_commit_now();
                }
                Err(e) => self.design.options.message = e,
            }
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use afrobim_annotate::{DisplayUnits, LengthUnit};
    use afrobim_design::{Bound, CheckItem};

    fn item(value: Option<f64>, limit: Option<f64>, unit: &str) -> CheckItem {
        CheckItem {
            id: "H.radius".into(),
            location: "PI 1".into(),
            description: "Curve radius".into(),
            value,
            limit,
            bound: Bound::AtLeast,
            unit: unit.into(),
            status: CheckStatus::Pass,
            limit_basis: "from the table: minimum radius 100 m".into(),
            message: "Curve radius: 250.000 m, limit at least 100.000 m.".into(),
        }
    }

    fn mm() -> DisplayUnits {
        DisplayUnits { length: LengthUnit::Millimetre, length_decimals: 0, ..DisplayUnits::default() }
    }

    #[test]
    fn the_check_table_shows_lengths_in_the_documents_unit_with_the_unit() {
        let metres = DisplayUnits::default();
        let (v, l) = check_cells(&metres, &item(Some(250.0), Some(100.0), "m"));
        assert_eq!((v.as_str(), l.as_str()), ("250.000 m", "100.000 m"));
        // The same row in a millimetre document: 250 m is 250 000 mm.
        let (v, l) = check_cells(&mm(), &item(Some(250.0), Some(100.0), "m"));
        assert_eq!((v.as_str(), l.as_str()), ("250000 mm", "100000 mm"));
    }

    #[test]
    fn a_grade_is_not_a_length_and_a_missing_limit_says_none() {
        let (v, l) = check_cells(&mm(), &item(Some(6.5), Some(8.0), "%"));
        assert_eq!((v.as_str(), l.as_str()), ("6.500 %", "8.000 %"));
        let (v, l) = check_cells(&mm(), &item(Some(250.0), None, "m"));
        assert_eq!((v.as_str(), l.as_str()), ("250000 mm", "none"));
        let (v, _) = check_cells(&mm(), &item(None, None, "m"));
        assert_eq!(v, "none");
    }

    #[test]
    fn the_rows_message_and_the_limits_source_keep_the_tables_unit() {
        // The cells change; the item itself, with the unit its table printed, does not.
        let i = item(Some(250.0), Some(100.0), "m");
        let _ = check_cells(&mm(), &i);
        assert!(i.limit_basis.contains("100 m") && i.message.contains("250.000 m"));
    }

    #[test]
    fn the_option_comparison_converts_lengths_only() {
        let metres = DisplayUnits::default();
        assert_eq!(figure_text(&metres, 1234.5, "m"), "1234.500");
        assert_eq!(figure_text(&mm(), 1234.5, "m"), "1234500");
        assert_eq!(figure_unit(&metres, "m"), "m");
        assert_eq!(figure_unit(&mm(), "m"), "mm");
        // Areas, volumes, percentages and counts are not lengths.
        assert_eq!(figure_text(&mm(), 12000.0, "m3"), "12000.000");
        assert_eq!(figure_text(&mm(), 4.25, "%"), "4.250");
        assert_eq!(figure_text(&mm(), 3.0, "nr"), "3");
        for unit in ["m2", "m3", "%", "nr", "deg/km", ""] {
            assert_eq!(figure_unit(&mm(), unit), unit);
        }
    }
}
