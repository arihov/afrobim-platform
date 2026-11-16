//! The Construction tab's windows: site record entry, the records table, the roll-up and update
//! of the programme, earned value, the look-ahead and delay lists, and the 3D colouring by
//! recorded progress. The book itself is a document element (`construction_glue`).
//!
//! Every field starts empty. Actual cost is entered and never derived, so every cost figure reads
//! "input needed" until it is. The EAC formula starts unchosen.

use std::collections::BTreeMap;

use afrobim_cost::Decimal;
use afrobim_kernel::ElementId;
use afrobim_progress::{ApplyReport, DelayRow, Event, Figure, LookAheadRow, ProgressBook, RollupResult, SCurveData};
use afrobim_schedule::{Date, ElementRule};
use egui::{pos2, Align2, Color32, FontId, RichText, Sense, Shape, Stroke, Ui};

use crate::app::AfroBimApp;
use crate::construction_glue::{self as glue, EacKind, EvmOutput, Measure, RecordEntry, Settings};

#[allow(non_snake_case)]

#[allow(non_snake_case)]
fn GOOD() -> egui::Color32 {
    crate::theme::success()
}

/// The colour a model element is drawn in when the 3D view shows recorded progress: grey with no
/// record, a cool grey at 0 %, then amber at the first percent through to green at 100 %.
pub fn percent_colour(percent: Option<f64>) -> [u8; 3] {
    match percent {
        None => [0x5A, 0x62, 0x6B],
        Some(p) if p <= 0.0 => [0x6F, 0x7D, 0x93],
        Some(p) => {
            let t = (p / 100.0).clamp(0.0, 1.0);
            let (a, b) = ([0xF2u8, 0xA9, 0x1D], [0x3F, 0xB9, 0x68]);
            [0, 1, 2].map(|i| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * t).round() as u8)
        }
    }
}

/// Where a site record takes its elements from.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Source {
    #[default]
    Selection,
    Rule,
}

#[derive(Default)]
struct RecordForm {
    source: Source,
    rule_category: Option<String>,
    rule_layer: Option<String>,
    rule_pattern: String,
    quantity: bool,
    percent: String,
    measured: String,
    unit: String,
    date: String,
    by: String,
    note: String,
    files: Vec<String>,
    correction: bool,
    reason: String,
    target_quantity: String,
    target_unit: String,
    target_item: Option<String>,
    message: Option<Result<String, String>>,
}

#[derive(Default)]
struct LogFilter {
    element: Option<ElementId>,
    from: String,
    to: String,
}

#[derive(Default)]
struct CostForm {
    date: String,
    activity: Option<String>,
    amount: String,
    note: String,
    error: String,
}

pub struct ConstructionUi {
    pub id: Option<ElementId>,
    pub book: ProgressBook,
    pub settings: Settings,
    loaded: Option<(serde_json::Value, Option<String>)>,
    pub record_open: bool,
    pub log_open: bool,
    pub rollup_open: bool,
    pub evm_open: bool,
    pub look_open: bool,
    /// The 3D view shows recorded progress instead of the programme's state.
    pub colour_by_record: bool,
    percents: Option<(Date, usize, BTreeMap<ElementId, f64>)>,
    rec: RecordForm,
    filter: LogFilter,
    preview: Option<Result<RollupResult, String>>,
    applied: Option<(ApplyReport, Result<String, String>)>,
    evm: Option<Result<EvmOutput, String>>,
    evm_dirty: bool,
    cost: CostForm,
}

impl Default for ConstructionUi {
    fn default() -> Self {
        ConstructionUi {
            id: None,
            book: ProgressBook::new(),
            settings: Settings::default(),
            loaded: None,
            record_open: false,
            log_open: false,
            rollup_open: false,
            evm_open: false,
            look_open: false,
            colour_by_record: false,
            percents: None,
            rec: RecordForm::default(),
            filter: LogFilter::default(),
            preview: None,
            applied: None,
            evm: None,
            evm_dirty: true,
            cost: CostForm::default(),
        }
    }
}

impl ConstructionUi {
    /// Reads the document's progress book when its data changed (open, undo).
    pub fn sync(&mut self, doc: &afrobim_kernel::Document) {
        let id = glue::find(doc);
        let current = id.and_then(|i| doc.find(i)).map(|e| (e.data.clone(), e.properties.get(glue::PROP_SETTINGS).cloned()));
        if current == self.loaded {
            return;
        }
        self.loaded = current;
        match glue::load(doc) {
            Some((id, book, settings)) => {
                self.id = Some(id);
                self.book = book;
                self.settings = settings;
            }
            None => {
                self.id = None;
                self.book = ProgressBook::new();
            }
        }
        self.preview = None;
        self.evm_dirty = true;
        self.percents = None;
    }

    /// Writes the book and the settings into the document (adding the element the first time).
    pub fn commit(&mut self, doc: &mut afrobim_kernel::Document) -> Result<(), String> {
        let id = glue::store(doc, self.id, &self.book, &self.settings)?;
        self.id = Some(id);
        self.loaded = doc.find(id).map(|e| (e.data.clone(), e.properties.get(glue::PROP_SETTINGS).cloned()));
        self.percents = None;
        self.evm_dirty = true;
        Ok(())
    }

    /// The recorded percent of an element at the date the 3D view shows, from the cache the app
    /// refreshes before it paints.
    pub fn percent_of(&self, id: ElementId) -> Option<f64> {
        self.percents.as_ref().and_then(|(_, _, m)| m.get(&id).copied())
    }

    /// Brings the colouring cache up to date for a date.
    pub fn refresh_percents(&mut self, date: Option<Date>) {
        let Some(date) = date else { return };
        let n = self.book.log.records().len();
        if self.percents.as_ref().is_some_and(|(d, count, _)| *d == date && *count == n) {
            return;
        }
        self.percents = Some((date, n, glue::percents_at(&self.book.log, date)));
    }
}

// ─── small widgets ──────────────────────────────────────────────────────────────────────

fn tf(ui: &mut Ui, text: &mut String, hint: &str, width: f32) -> egui::Response {
    ui.add_sized([width, 20.0], egui::TextEdit::singleline(text).hint_text(hint))
}

fn money(v: Decimal, decimals: u32) -> String {
    v.to_fixed(decimals)
}

fn figure(f: &Figure<Decimal>, decimals: u32) -> String {
    f.text(|v| v.to_fixed(decimals))
}

fn ratio(f: &Figure<Decimal>) -> String {
    f.text(|v| v.to_string())
}

fn date_of(text: &str) -> Option<Date> {
    let t = text.trim();
    if t.len() == 10 { Date::parse(t) } else { None }
}

fn message(ui: &mut Ui, m: &Option<Result<String, String>>) {
    match m {
        Some(Ok(t)) => {
            ui.label(RichText::new(t).color(GOOD()));
        }
        Some(Err(t)) => {
            crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, t);
        }
        None => {}
    }
}

impl AfroBimApp {
    /// The money decimals of reports: those of the estimate's reporting currency when there is an
    /// estimate, else the app's primary currency convention.
    fn construction_decimals(&self) -> u32 {
        match &self.cost.est {
            Some(est) => est.currencies.currency(&est.reporting_currency).map(|c| c.decimals).unwrap_or(crate::cost_glue::PRIMARY_DECIMALS),
            None => crate::cost_glue::PRIMARY_DECIMALS,
        }
    }

    /// Makes sure the estimate has a result when there is one to make: the roll-up weights and the
    /// budget can come from it.
    fn construction_estimate(&mut self) {
        self.cost_sync();
        if self.cost.est.is_some() && !matches!(self.cost.result, Some(Ok(_))) {
            self.cost_collect();
        }
    }

    /// Opens a Construction window by its command.
    pub(crate) fn construction_open(&mut self, which: crate::commands::Action) {
        use crate::commands::Action;
        self.construction.sync(&self.doc);
        self.sched.sync(&self.doc);
        match which {
            Action::SiteRecord => self.construction.record_open = true,
            Action::SiteLog => self.construction.log_open = true,
            Action::RollUp => {
                self.construction_estimate();
                self.construction.rollup_open = true;
                // A data date already entered gives the roll-up straight away.
                if self.construction.settings.date().is_ok() && self.sched.prog.is_some() {
                    self.rollup_preview();
                }
            }
            Action::Evm => {
                self.construction_estimate();
                self.construction.evm_open = true;
                self.construction.evm_dirty = true;
            }
            Action::LookAhead => self.construction.look_open = true,
            _ => {}
        }
    }

    /// SITEVIEW: the 3D view shows recorded progress at the 4D date, or the programme's state.
    pub(crate) fn toggle_site_colour(&mut self) {
        self.construction.sync(&self.doc);
        self.sched.sync(&self.doc);
        self.construction.colour_by_record = !self.construction.colour_by_record;
        if self.construction.colour_by_record {
            if self.construction.book.log.records().is_empty() {
                self.status = "There are no site records yet (SITEREC): every element will show as not recorded.".into();
            } else {
                self.status = "The 3D view shows recorded percent complete at the 4D date.".into();
            }
            // The colouring follows the 4D date control, which needs a scheduled programme for its range.
            if self.sched.date.is_none() {
                if let Some(last) = self.construction.book.log.latest_record_date() {
                    self.sched.date = Some(last);
                }
            }
            self.sched.fourd_on = true;
            self.view_3d = true;
        } else {
            self.status = "The 3D view shows the programme's state at the 4D date.".into();
        }
    }

    /// Writes the progress tables into a folder the user picks.
    pub(crate) fn export_evm_csvs(&mut self) {
        self.construction_open(crate::commands::Action::Evm);
        self.construction.evm_open = false;
        let Some(dir) = rfd::FileDialog::new().set_title("Folder for the earned value tables").pick_folder() else { return };
        self.write_evm_csvs(&dir);
    }

    pub(crate) fn write_evm_csvs(&mut self, dir: &std::path::Path) {
        self.construction_estimate();
        let out = match self.compute_evm() {
            Ok(o) => o,
            Err(e) => {
                self.status = format!("Not exported: {e}");
                return;
            }
        };
        let (look, delays) = self.look_ahead_rows().map(|(l, d)| (Some(l), Some(d))).unwrap_or((None, None));
        let records = glue::records_csv(&self.doc, &self.construction.book.log);
        self.status = match glue::write_csvs(dir, &out, look.as_deref(), delays.as_deref()).and_then(|mut files| {
            std::fs::write(dir.join("site-records.csv"), records).map_err(|e| format!("could not write site-records.csv: {e}"))?;
            files.push("site-records.csv".into());
            Ok(files)
        }) {
            Ok(files) => format!("Wrote {} to {}.", files.join(", "), dir.display()),
            Err(e) => e,
        };
    }

    fn compute_evm(&self) -> Result<EvmOutput, String> {
        let prog = self.sched.prog.as_ref().ok_or("there is no programme yet (PROGRAMME)")?;
        let estimate = self.cost.result.as_ref().and_then(|r| r.as_ref().ok());
        glue::evm(&self.doc, prog, estimate, &self.construction.book, &self.construction.settings, self.construction_decimals())
    }

    /// The look-ahead and delay rows for the programme as it stands, when the length was entered and
    /// the programme can be scheduled.
    fn look_ahead_rows(&self) -> Result<(Vec<LookAheadRow>, Vec<DelayRow>), String> {
        let prog = self.sched.prog.as_ref().ok_or("there is no programme yet (PROGRAMME)")?;
        let schedule = match &self.sched.result {
            Some(Ok(s)) => s,
            Some(Err(m)) => return Err(format!("the programme cannot be scheduled: {}", m.join("; "))),
            None => return Err("the programme has not been scheduled".into()),
        };
        let from = prog.data_date.ok_or("the programme has no data date: apply the roll-up first (ROLLUP)")?;
        glue::look_ahead_and_delays(prog, schedule, from, &self.construction.settings.look_ahead_days)
    }

    pub(crate) fn construction_windows(&mut self, ctx: &egui::Context) {
        self.construction.sync(&self.doc);
        let c = &self.construction;
        if !(c.record_open || c.log_open || c.rollup_open || c.evm_open || c.look_open) {
            return;
        }
        if self.construction.record_open {
            self.record_window(ctx);
        }
        if self.construction.log_open {
            self.log_window(ctx);
        }
        if self.construction.rollup_open {
            self.rollup_window(ctx);
        }
        if self.construction.evm_open {
            self.evm_window(ctx);
        }
        if self.construction.look_open {
            self.look_window(ctx);
        }
    }

    // ─── site record entry ───────────────────────────────────────────────────────────────

    fn record_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut add = false;
        let mut set_target = false;
        let mut target_from_estimate = false;
        let mut pick_file = false;
        let (categories, layers) = crate::coordination_glue::selectable(&self.doc);
        let selection: Vec<ElementId> = self.doc.selection().to_vec();
        let rule = {
            let r = &self.construction.rec;
            ElementRule { category: r.rule_category.clone(), layer: r.rule_layer.clone(), mark_pattern: Some(r.rule_pattern.trim().to_string()).filter(|p| !p.is_empty()) }
        };
        let use_rule = self.construction.rec.source == Source::Rule;
        let targets = glue::target_elements(&self.doc, &selection, use_rule.then_some(&rule));
        let marks: Vec<String> = targets.iter().filter_map(|id| self.doc.find(*id)).map(|e| e.name.clone()).collect();
        let items: Vec<(String, String)> = match &self.cost.result {
            Some(Ok(r)) => r.items.iter().map(|i| (i.code.clone(), format!("{} {} ({})", i.code, i.description, i.unit))).collect(),
            _ => Vec::new(),
        };
        let target_info: Vec<(String, Option<String>)> = targets
            .iter()
            .take(8)
            .map(|id| {
                let mark = self.doc.find(*id).map(|e| e.name.clone()).unwrap_or_default();
                (mark, self.construction.book.log.target_of(*id).map(|t| format!("{} {}", t.quantity, t.unit)))
            })
            .collect();
        let c = &mut self.construction;
        let dlg = crate::ui_kit::Dialog::titled("Record site progress").subtitle("Percent complete of the elements, by date").width(560.0).show_body(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(620.0).auto_shrink([true, true]).show(ui, |ui| {
                let r = &mut c.rec;
                crate::ui_kit::heading(ui, "Elements");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut r.source, Source::Selection, format!("the selected elements ({})", selection.len()));
                    ui.radio_value(&mut r.source, Source::Rule, "by rule");
                });
                if r.source == Source::Rule {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("rec-category").selected_text(r.rule_category.clone().unwrap_or_else(|| "any category".into())).show_ui(ui, |ui| {
                            ui.selectable_value(&mut r.rule_category, None, "any category");
                            for cat in &categories {
                                ui.selectable_value(&mut r.rule_category, Some(cat.clone()), cat);
                            }
                        });
                        egui::ComboBox::from_id_salt("rec-layer").selected_text(r.rule_layer.clone().unwrap_or_else(|| "any layer".into())).show_ui(ui, |ui| {
                            ui.selectable_value(&mut r.rule_layer, None, "any layer");
                            for l in &layers {
                                ui.selectable_value(&mut r.rule_layer, Some(l.clone()), l);
                            }
                        });
                        tf(ui, &mut r.rule_pattern, "mark pattern, e.g. SD-*", 150.0);
                    });
                }
                let shown: Vec<&str> = marks.iter().take(12).map(String::as_str).collect();
                ui.label(RichText::new(format!("{} element(s): {}{}", marks.len(), shown.join(", "), if marks.len() > 12 { ", ..." } else { "" })).small().weak());
                ui.separator();
                crate::ui_kit::heading(ui, "What was measured");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut r.quantity, false, "percent complete");
                    ui.radio_value(&mut r.quantity, true, "quantity done to date");
                });
                crate::ui_kit::grid(ui, "rec-measure", |ui| {
                    if r.quantity {
                        ui.label("Quantity done");
                        ui.horizontal(|ui| {
                            tf(ui, &mut r.measured, "quantity", 100.0);
                            tf(ui, &mut r.unit, "unit", 60.0);
                        });
                    } else {
                        ui.label("Percent complete");
                        tf(ui, &mut r.percent, "0 to 100", 100.0);
                    }
                    ui.end_row();
                    ui.label("Date of the measurement");
                    tf(ui, &mut r.date, "YYYY-MM-DD", 100.0);
                    ui.end_row();
                    ui.label("Recorded by");
                    tf(ui, &mut r.by, "name (required)", 200.0);
                    ui.end_row();
                    ui.label("Note");
                    tf(ui, &mut r.note, "optional", 300.0);
                    ui.end_row();
                });
                ui.label("Files and photos (paths only; AfroBIM does not open or copy them)");
                let mut remove = None;
                for (i, f) in r.files.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        tf(ui, f, "path", 360.0);
                        if crate::ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    r.files.remove(i);
                }
                if ui.button("Add a file…").clicked() {
                    pick_file = true;
                }
                ui.checkbox(&mut r.correction, "This is a correction of earlier records");
                if r.correction {
                    ui.horizontal(|ui| {
                        ui.label("Reason");
                        tf(ui, &mut r.reason, "required for a correction", 340.0);
                    });
                }
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "A percent below the previous record is refused unless it is a correction with a reason. A quantity needs a target quantity in the same unit.");
                if r.quantity {
                    ui.separator();
                    crate::ui_kit::heading(ui, "Target quantities");
                    for (mark, target) in &target_info {
                        ui.label(RichText::new(format!("{mark}: {}", target.as_deref().unwrap_or("no target (input needed)"))).small());
                    }
                    ui.horizontal(|ui| {
                        tf(ui, &mut r.target_quantity, "full quantity", 100.0);
                        tf(ui, &mut r.target_unit, "unit", 60.0);
                        if ui.button("Set as the target of each element").clicked() {
                            set_target = true;
                        }
                    });
                    if !items.is_empty() {
                        ui.horizontal(|ui| {
                            let shown = r.target_item.as_ref().and_then(|code| items.iter().find(|i| &i.0 == code)).map(|i| i.1.clone()).unwrap_or_else(|| "take it from a bill item".into());
                            egui::ComboBox::from_id_salt("rec-item").width(300.0).selected_text(shown).show_ui(ui, |ui| {
                                for (code, label) in &items {
                                    ui.selectable_value(&mut r.target_item, Some(code.clone()), label);
                                }
                            });
                            if ui.add_enabled(r.target_item.is_some(), egui::Button::new("Use the item's quantity")).clicked() {
                                target_from_estimate = true;
                            }
                        });
                    }
                }
                ui.add_space(6.0);
                if ui.button("Add the record").clicked() {
                    add = true;
                }
                message(ui, &r.message);
            });
        });
        if dlg.closed {
            open = false;
        }
        self.construction.record_open = open;
        if pick_file {
            if let Some(path) = rfd::FileDialog::new().set_title("Photo or file for the record").pick_file() {
                self.construction.rec.files.push(path.display().to_string());
            }
        }
        if set_target || target_from_estimate {
            self.construction.rec.message = Some(self.set_targets(&targets, target_from_estimate));
        }
        if add {
            let r = &self.construction.rec;
            let entry = RecordEntry {
                date: r.date.clone(),
                measure: if r.quantity { Measure::Quantity { measured: r.measured.clone(), unit: r.unit.clone() } } else { Measure::Percent(r.percent.clone()) },
                recorded_by: r.by.clone(),
                note: r.note.clone(),
                files: r.files.clone(),
                correction: r.correction.then(|| r.reason.clone()),
            };
            let result = glue::add_records(&mut self.construction.book.log, &targets, &entry).and_then(|n| {
                self.construction.commit(&mut self.doc)?;
                Ok(format!("Added {n} record(s)."))
            });
            if result.is_ok() {
                self.schedule_commit_now();
                self.construction.rec.note.clear();
                self.construction.rec.files.clear();
            }
            self.status = match &result {
                Ok(t) => t.clone(),
                Err(e) => format!("Not recorded: {e}"),
            };
            self.construction.rec.message = Some(result);
        }
    }

    /// Sets the target quantity of each element from the typed quantity or from a bill item.
    fn set_targets(&mut self, targets: &[ElementId], from_estimate: bool) -> Result<String, String> {
        if targets.is_empty() {
            return Err("no element is selected, and the rule picks none".into());
        }
        let mut next = self.construction.book.log.clone();
        let mut done = 0;
        if from_estimate {
            let code = self.construction.rec.target_item.clone().ok_or("choose a bill item")?;
            let Some(Ok(result)) = &self.cost.result else { return Err("the estimate has no result".into()) };
            let mut skipped = Vec::new();
            for e in targets {
                match next.set_target_from_estimate(result, *e, &code) {
                    Ok(()) => done += 1,
                    Err(err) => skipped.push(format!("{}: {err}", self.doc.find(*e).map(|x| x.name.clone()).unwrap_or_default())),
                }
            }
            if done == 0 {
                return Err(skipped.join("; "));
            }
            self.construction.book.log = next;
            self.construction.commit(&mut self.doc)?;
            self.schedule_commit_now();
            return Ok(format!("Target set for {done} element(s) from item {code}.{}", if skipped.is_empty() { String::new() } else { format!(" Not set: {}", skipped.join("; ")) }));
        }
        let r = &self.construction.rec;
        let quantity = if r.target_quantity.trim().is_empty() { return Err("enter the target quantity".into()) } else { Decimal::parse(r.target_quantity.trim()).map_err(|e| format!("the target quantity: {e}"))? };
        for e in targets {
            next.set_target(*e, quantity, r.target_unit.trim()).map_err(|err| err.to_string())?;
            done += 1;
        }
        self.construction.book.log = next;
        self.construction.commit(&mut self.doc)?;
        self.schedule_commit_now();
        Ok(format!("Target set for {done} element(s)."))
    }

    // ─── the records table ───────────────────────────────────────────────────────────────

    fn log_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut export = false;
        let doc = &self.doc;
        let c = &mut self.construction;
        let with_records: Vec<(ElementId, String)> = {
            let mut ids: Vec<ElementId> = c.book.log.records().iter().map(|r| r.element).collect();
            ids.sort();
            ids.dedup();
            ids.into_iter().map(|id| (id, doc.find(id).map(|e| e.name.clone()).unwrap_or_else(|| format!("element {}", id.0)))).collect()
        };
        let dlg = crate::ui_kit::Dialog::titled("Site records").subtitle("The progress records kept in the document").width(860.0).show_body(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Element");
                let shown = c.filter.element.and_then(|id| with_records.iter().find(|w| w.0 == id)).map(|w| w.1.clone()).unwrap_or_else(|| "all".into());
                egui::ComboBox::from_id_salt("log-element").selected_text(shown).show_ui(ui, |ui| {
                    ui.selectable_value(&mut c.filter.element, None, "all");
                    for (id, name) in &with_records {
                        ui.selectable_value(&mut c.filter.element, Some(*id), name);
                    }
                });
                ui.label("from");
                tf(ui, &mut c.filter.from, "YYYY-MM-DD", 90.0);
                ui.label("to");
                tf(ui, &mut c.filter.to, "YYYY-MM-DD", 90.0);
                if ui.button("Export CSV…").clicked() {
                    export = true;
                }
            });
            let (from, to) = (date_of(&c.filter.from), date_of(&c.filter.to));
            if (!c.filter.from.trim().is_empty() && from.is_none()) || (!c.filter.to.trim().is_empty() && to.is_none()) {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, "A date is typed as YYYY-MM-DD; a date that is not read is not used.");
            }
            let rows = glue::filtered_records(&c.book.log, c.filter.element, from, to);
            ui.label(RichText::new(format!("{} of {} record(s)", rows.len(), c.book.log.records().len())).small().weak());
            egui::ScrollArea::both().max_height(ui.available_height() - 30.0).auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("log-grid").num_columns(8).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
                    for h in ["Date", "Element", "Measured", "Percent", "Recorded by", "Note", "Files", "Correction"] {
                        crate::ui_kit::th(ui, &h);
                    }
                    ui.end_row();
                    for (_, r, percent) in &rows {
                        ui.label(r.date.to_string());
                        ui.label(doc.find(r.element).map(|e| e.name.clone()).unwrap_or_else(|| format!("element {}", r.element.0)));
                        ui.label(match &r.measurement {
                            afrobim_progress::Measurement::Percent(p) => format!("{p} %"),
                            afrobim_progress::Measurement::Quantity { measured, unit } => format!("{measured} {unit}"),
                        });
                        ui.label(percent.map(|p| format!("{p} %")).unwrap_or_else(|| "input needed".into()));
                        ui.label(&r.recorded_by);
                        ui.label(&r.note);
                        ui.label(r.files.join("; "));
                        ui.label(r.correction.clone().unwrap_or_default());
                        ui.end_row();
                    }
                });
                if !c.book.log.targets().is_empty() {
                    ui.add_space(8.0);
                    crate::ui_kit::heading(ui, "Target quantities");
                    for t in c.book.log.targets() {
                        ui.label(format!("{}: {} {}", doc.find(t.element).map(|e| e.name.clone()).unwrap_or_else(|| format!("element {}", t.element.0)), t.quantity, t.unit));
                    }
                }
            });
        });
        if dlg.closed {
            open = false;
        }
        self.construction.log_open = open;
        if export {
            if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("site-records.csv").save_file() {
                if path.extension().is_none() {
                    path.set_extension("csv");
                }
                self.status = match std::fs::write(&path, glue::records_csv(&self.doc, &self.construction.book.log)) {
                    Ok(()) => format!("Wrote the site records to {}.", path.display()),
                    Err(e) => format!("Could not write {}: {e}", path.display()),
                };
            }
        }
    }

    // ─── roll-up and update ──────────────────────────────────────────────────────────────

    fn rollup_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let (mut preview, mut apply) = (false, false);
        let has_prog = self.sched.prog.is_some();
        let doc = &self.doc;
        let c = &mut self.construction;
        let dlg = crate::ui_kit::Dialog::titled("Roll up site progress and update the programme").subtitle("Activity progress from the records of the elements").width(900.0).show_body(ctx, |ui| {
            if !has_prog {
                ui.label("There is no programme yet. Create one in PROGRAMME and link the elements to its activities.");
                return;
            }
            ui.label(
                RichText::new("Each activity's percent is the weighted mean of the percent of the elements linked to it, whatever the link action. The weights are the priced values of the elements when every element of the activity is priced above zero, otherwise equal; the basis used is shown. Actual dates are inferred from the records and are marked so: check them against the site diary before you accept.")
                    .small()
                    .weak(),
            );
            ui.horizontal(|ui| {
                ui.label("Data date");
                if tf(ui, &mut c.settings.data_date, "YYYY-MM-DD", 100.0).changed() {
                    c.evm_dirty = true;
                }
                if ui.button("Roll up at this date").clicked() {
                    preview = true;
                }
            });
            match &c.preview {
                Some(Err(e)) => {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, e);
                }
                Some(Ok(r)) => {
                    egui::ScrollArea::vertical().max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                        egui::Grid::new("rollup-grid").num_columns(7).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
                            for h in ["Activity", "Percent", "Weights", "Elements recorded", "Actual start", "Actual finish", ""] {
                                crate::ui_kit::th(ui, &h);
                            }
                            ui.end_row();
                            for a in &r.activities {
                                ui.label(&a.activity);
                                ui.label(format!("{} %", a.percent));
                                ui.label(a.basis.label()).on_hover_text(a.basis_note.clone().unwrap_or_default());
                                ui.label(format!("{} of {}", a.elements_recorded, a.elements_total));
                                ui.label(a.actual_start.as_ref().map(|d| format!("{} (inferred)", d.date)).unwrap_or_else(|| "none".into())).on_hover_text(a.actual_start.as_ref().map(|d| d.basis.clone()).unwrap_or_default());
                                ui.label(a.actual_finish.as_ref().map(|d| format!("{} (inferred)", d.date)).unwrap_or_else(|| "none".into())).on_hover_text(a.actual_finish.as_ref().map(|d| d.basis.clone()).unwrap_or_default());
                                ui.label(RichText::new(a.basis_note.clone().unwrap_or_default()).small().weak());
                                ui.end_row();
                            }
                        });
                        if !r.unlinked_activities.is_empty() {
                            ui.add_space(4.0);
                            crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, format!("No element is linked to: {}. Their progress is left as it is.", r.unlinked_activities.join(", ")));
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new("Accepting replaces the percent complete and the actual dates of these activities in the programme, sets its data date and reschedules it. Activities no element reaches keep their own progress.").small());
                    if ui.button("Accept: apply to the programme and reschedule").clicked() {
                        apply = true;
                    }
                }
                None => {}
            }
            if let Some((report, outcome)) = &c.applied {
                ui.separator();
                ui.label(RichText::new(format!("Applied to {} activities at the data date {}.", report.applied.len(), report.data_date)).strong());
                for (activity, what, date) in &report.inferred_dates {
                    ui.label(RichText::new(format!("{activity}: {what} {date} (inferred from the records)")).small());
                }
                if !report.unknown_activities.is_empty() {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, format!("Not in the programme: {}", report.unknown_activities.join(", ")));
                }
                match outcome {
                    Ok(t) => {
                        ui.label(RichText::new(t).small());
                    }
                    Err(e) => {
                        crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, format!("The programme could not be rescheduled: {e}"));
                    }
                }
            }
            let _ = doc;
        });
        if dlg.closed {
            open = false;
        }
        self.construction.rollup_open = open;
        if preview {
            self.rollup_preview();
        }
        if apply {
            self.rollup_apply();
        }
    }

    fn rollup_preview(&mut self) {
        self.construction_estimate();
        let result = (|| {
            let date = self.construction.settings.date()?;
            let prog = self.sched.prog.as_ref().ok_or("there is no programme yet")?;
            let estimate = self.cost.result.as_ref().and_then(|r| r.as_ref().ok());
            glue::rollup_at(&self.doc, prog, estimate, &self.construction.book.log, date)
        })();
        self.construction.applied = None;
        self.construction.preview = Some(result);
    }

    fn rollup_apply(&mut self) {
        let Some(Ok(result)) = self.construction.preview.clone() else { return };
        let Some(prog) = self.sched.prog.clone() else { return };
        match glue::apply(&prog, &result) {
            Ok((next, report, schedule)) => {
                let summary = schedule.as_ref().map(|s| format!("Rescheduled: {} to {}, {} critical.", s.project_start, s.project_finish, s.critical().count())).map_err(|e| e.clone());
                match self.sched.replace_programme(&mut self.doc, next) {
                    Ok(()) => {
                        self.schedule_commit_now();
                        self.status = format!("Applied the site progress at {} to {} activities.", report.data_date, report.applied.len());
                        // The settings keep the typed data date; the book is stored with them.
                        let _ = self.construction.commit(&mut self.doc);
                        self.construction.applied = Some((report, summary));
                    }
                    Err(e) => self.status = format!("Programme not stored: {e}"),
                }
            }
            Err(e) => self.construction.applied = Some((ApplyReport { data_date: result.date, applied: Vec::new(), unknown_activities: Vec::new(), inferred_dates: Vec::new() }, Err(e))),
        }
    }

    // ─── earned value ────────────────────────────────────────────────────────────────────

    fn evm_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut store_report = false;
        let mut export = false;
        let mut changed = false;
        if self.construction.evm_dirty {
            self.construction.evm = Some(self.compute_evm());
            self.construction.evm_dirty = false;
        }
        let decimals = self.construction_decimals();
        let activities: Vec<(String, String)> = self.sched.prog.as_ref().map(|p| p.activities.iter().map(|a| (a.id.clone(), a.name.clone())).collect()).unwrap_or_default();
        let baselined: Vec<String> = self.sched.prog.as_ref().and_then(|p| p.baseline.as_ref()).map(|b| b.entries.iter().map(|e| e.id.clone()).collect()).unwrap_or_default();
        let has_estimate = matches!(self.cost.result, Some(Ok(_)));
        let c = &mut self.construction;
        let dlg = crate::ui_kit::Dialog::titled("Earned value").subtitle("Planned, earned and actual cost against the data date").width(980.0).show_body(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.label(
                    RichText::new("Definitions are in the afrobim-progress README and are algebraic: no published standard was held. Actual cost is entered, never derived: until it is, every cost figure reads input needed. Name the EAC formula wherever you quote the EAC.")
                        .small()
                        .weak(),
                );
                crate::ui_kit::grid(ui, "evm-setup", |ui| {
                    ui.label("Data date");
                    changed |= tf(ui, &mut c.settings.data_date, "YYYY-MM-DD", 100.0).changed();
                    ui.end_row();
                    ui.label("Budget at completion");
                    ui.vertical(|ui| {
                        changed |= ui.radio_value(&mut c.settings.budget_from_estimate, true, if has_estimate { "from the estimate (priced element values)" } else { "from the estimate (it has no result yet)" }).changed();
                        changed |= ui.radio_value(&mut c.settings.budget_from_estimate, false, "entered per activity").changed();
                    });
                    ui.end_row();
                    ui.label("EAC formula");
                    ui.vertical(|ui| {
                        let shown = c.settings.eac.map(|k| k.label()).unwrap_or_else(|| "Choose a formula".to_string());
                        egui::ComboBox::from_id_salt("evm-eac").width(560.0).selected_text(shown).show_ui(ui, |ui| {
                            changed |= ui.selectable_value(&mut c.settings.eac, None, "Choose a formula").changed();
                            for k in EacKind::ALL {
                                changed |= ui.selectable_value(&mut c.settings.eac, Some(k), k.label()).changed();
                            }
                        });
                        match c.settings.eac {
                            Some(EacKind::CpiSpiWeighted) => {
                                ui.horizontal(|ui| {
                                    ui.label("CPI weight");
                                    changed |= tf(ui, &mut c.settings.cpi_weight, "wc", 60.0).changed();
                                    ui.label("SPI weight");
                                    changed |= tf(ui, &mut c.settings.spi_weight, "ws", 60.0).changed();
                                    crate::ui_kit::note(ui, "each at least 0, adding to 1");
                                });
                            }
                            Some(EacKind::BottomUp) => {
                                ui.horizontal(|ui| {
                                    ui.label("Your estimate of the remaining work (ETC)");
                                    changed |= tf(ui, &mut c.settings.etc, "amount", 100.0).changed();
                                });
                            }
                            _ => {}
                        }
                    });
                    ui.end_row();
                });
                if !c.settings.budget_from_estimate {
                    ui.label(RichText::new("Budget per activity (activities with a baseline can carry one)").strong());
                    egui::Grid::new("evm-budget").num_columns(3).spacing([10.0, 3.0]).show(ui, |ui| {
                        for (id, name) in &activities {
                            ui.label(id);
                            ui.label(name);
                            if baselined.contains(id) {
                                let text = c.settings.budget_text.entry(id.clone()).or_default();
                                changed |= tf(ui, text, "budget, none if empty", 120.0).changed();
                            } else {
                                crate::ui_kit::note(ui, "no baseline");
                            }
                            ui.end_row();
                        }
                    });
                }
                ui.separator();
                crate::ui_kit::heading(ui, "Actual cost");
                let mut delete = None;
                if !c.book.costs.entries.is_empty() {
                    egui::Grid::new("evm-costs").num_columns(5).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
                        for (i, e) in c.book.costs.entries.iter().enumerate() {
                            ui.label(e.date.to_string());
                            ui.label(e.activity.clone().unwrap_or_else(|| "project".into()));
                            ui.label(money(e.amount, decimals));
                            ui.label(&e.note);
                            if ui.small_button("Delete").clicked() {
                                delete = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                } else {
                    crate::ui_kit::note(ui, "No actual cost entered.");
                }
                if let Some(i) = delete {
                    c.book.costs.entries.remove(i);
                    changed = true;
                    c.evm_dirty = true;
                    let _ = i;
                }
                ui.horizontal(|ui| {
                    tf(ui, &mut c.cost.date, "date", 90.0);
                    egui::ComboBox::from_id_salt("cost-activity").selected_text(c.cost.activity.clone().unwrap_or_else(|| "project".into())).show_ui(ui, |ui| {
                        ui.selectable_value(&mut c.cost.activity, None, "project (no activity)");
                        for (id, name) in &activities {
                            ui.selectable_value(&mut c.cost.activity, Some(id.clone()), format!("{id} {name}"));
                        }
                    });
                    tf(ui, &mut c.cost.amount, "amount", 100.0);
                    tf(ui, &mut c.cost.note, "note", 160.0);
                    if ui.button("Add cost").clicked() {
                        let r = &c.cost;
                        match glue::add_cost(&mut c.book, &r.date, r.activity.as_deref(), &r.amount, &r.note) {
                            Ok(()) => {
                                c.cost = CostForm::default();
                                changed = true;
                            }
                            Err(e) => c.cost.error = e,
                        }
                    }
                });
                if !c.cost.error.is_empty() {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, &c.cost.error);
                }
                ui.separator();
                match c.evm.clone() {
                    Some(Err(e)) => {
                        crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, format!("Not calculated: {e}"));
                    }
                    Some(Ok(out)) => {
                        evm_output(ui, &out, decimals);
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!out.eac_unchosen && c.id.is_some(), egui::Button::new("Store this report in the document")).on_hover_text("Keeps the figures and draws the S-curve on the sheet. It needs a chosen EAC formula.").clicked() {
                                store_report = true;
                            }
                            if ui.button("Export every table as CSV…").clicked() {
                                export = true;
                            }
                        });
                    }
                    None => {}
                }
            });
        });
        if dlg.closed {
            open = false;
        }
        self.construction.evm_open = open;
        if changed {
            self.construction.evm_dirty = true;
            self.construction.rec.message = None;
            if self.construction.id.is_some() || !self.construction.book.costs.entries.is_empty() {
                if let Err(e) = self.construction.commit(&mut self.doc) {
                    self.status = format!("Not stored: {e}");
                } else {
                    self.schedule_commit_now();
                }
            }
        }
        if store_report {
            self.store_evm_report();
        }
        if export {
            self.export_evm_csvs();
            self.construction.evm_open = true;
        }
    }

    fn store_evm_report(&mut self) {
        let (Some(id), Some(prog)) = (self.construction.id, self.sched.prog.clone()) else { return };
        let estimate = self.cost.result.as_ref().and_then(|r| r.as_ref().ok()).cloned();
        let decimals = self.construction_decimals();
        let settings = self.construction.settings.clone();
        match glue::store_report(&mut self.doc, id, &prog, estimate.as_ref(), &settings, decimals) {
            Ok(_) => {
                self.status = "The earned value report is stored in the document.".into();
                self.schedule_commit_now();
                self.construction.sync(&self.doc);
            }
            Err(e) => self.status = format!("Report not stored: {e}"),
        }
    }

    // ─── look-ahead and delays ───────────────────────────────────────────────────────────

    fn look_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let result = self.look_ahead_rows();
        let from = self.sched.prog.as_ref().and_then(|p| p.data_date);
        let c = &mut self.construction;
        let mut changed = false;
        let dlg = crate::ui_kit::Dialog::titled("Look-ahead and delays").subtitle("The weeks ahead, and what is late").width(900.0).show_body(ctx, |ui| {
            ui.label(
                RichText::new("Taken from the programme as it stands after the roll-up was applied. The look-ahead lists activities not started whose early start, or not complete whose early finish, falls in the days after the data date. The delay list holds activities that finish later than their baseline, worst first.")
                    .small()
                    .weak(),
            );
            ui.horizontal(|ui| {
                ui.label("Look ahead");
                changed |= tf(ui, &mut c.settings.look_ahead_days, "days", 60.0).changed();
                ui.label("days from the data date");
                ui.label(from.map(|d| d.to_string()).unwrap_or_else(|| "(none: apply the roll-up first)".into()));
            });
            match &result {
                Err(e) => {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, e);
                }
                Ok((look, delays)) => {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.label(RichText::new(format!("Look-ahead ({} activities)", look.len())).strong());
                        egui::Grid::new("look-grid").num_columns(7).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
                            for h in ["Activity", "WBS", "Name", "Event", "Start", "Finish", "Float (days)"] {
                                crate::ui_kit::th(ui, &h);
                            }
                            ui.end_row();
                            for r in look {
                                ui.label(&r.id);
                                ui.label(&r.wbs);
                                ui.label(&r.name);
                                ui.label(match r.event {
                                    Event::Starts => "starts",
                                    Event::Finishes => "finishes",
                                    Event::StartsAndFinishes => "starts and finishes",
                                });
                                ui.label(r.start.to_string());
                                ui.label(r.finish.to_string());
                                ui.label(if r.critical { format!("{} (critical)", r.total_float) } else { r.total_float.to_string() });
                                ui.end_row();
                            }
                        });
                        ui.add_space(8.0);
                        ui.label(RichText::new(format!("Delay list ({} activities)", delays.len())).strong());
                        egui::Grid::new("delay-grid").num_columns(6).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
                            for h in ["Activity", "Name", "Baseline finish", "Forecast finish", "Finish variance (working days)", "Start variance"] {
                                crate::ui_kit::th(ui, &h);
                            }
                            ui.end_row();
                            for r in delays {
                                ui.label(&r.id);
                                ui.label(&r.name);
                                ui.label(r.baseline_finish.to_string());
                                ui.label(r.finish.to_string());
                                ui.label(r.finish_variance.to_string());
                                ui.label(r.start_variance.to_string());
                                ui.end_row();
                            }
                        });
                    });
                }
            }
        });
        if dlg.closed {
            open = false;
        }
        self.construction.look_open = open;
        if changed && self.construction.id.is_some() {
            let _ = self.construction.commit(&mut self.doc);
            self.schedule_commit_now();
        }
    }
}

// ─── the earned value output ────────────────────────────────────────────────────────────

fn evm_output(ui: &mut Ui, out: &EvmOutput, decimals: u32) {
    let e = &out.evm;
    ui.label(RichText::new(format!("Earned value at {}", e.data_date)).strong());
    crate::ui_kit::note(ui, format!("Budget: {}", out.budget.source));
    let hidden = |f: &Figure<Decimal>, out: &EvmOutput| if out.eac_unchosen { "choose an EAC formula".to_string() } else { figure(f, decimals) };
    egui::Grid::new("evm-figures").num_columns(4).spacing([24.0, 3.0]).show(ui, |ui| {
        let rows: [(&str, String, &str, String); 6] = [
            ("BAC", money(e.bac, decimals), "SV = EV - PV", money(e.sv, decimals)),
            ("PV", money(e.pv, decimals), "CV = EV - AC", figure(&e.cv, decimals)),
            ("EV", money(e.ev, decimals), "SPI = EV / PV", ratio(&e.spi)),
            ("AC", figure(&e.ac, decimals), "CPI = EV / AC", ratio(&e.cpi)),
            ("EAC", hidden(&e.eac.value, out), "ETC = EAC - AC", hidden(&e.etc, out)),
            ("VAC = BAC - EAC", hidden(&e.vac, out), "TCPI to BAC", ratio(&e.tcpi_bac)),
        ];
        for (a, av, b, bv) in rows {
            ui.label(RichText::new(a).strong());
            ui.label(av);
            ui.label(RichText::new(b).strong());
            ui.label(bv);
            ui.end_row();
        }
        crate::ui_kit::heading(ui, "TCPI to EAC");
        ui.label(if out.eac_unchosen { "choose an EAC formula".to_string() } else { ratio(&e.tcpi_eac) });
        ui.end_row();
    });
    if out.eac_unchosen {
        crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, "No EAC formula is chosen, so the EAC and the figures that depend on it are not shown.");
    } else {
        ui.label(RichText::new(format!("EAC by {}: {}", e.eac.formula, e.eac.definition)).small());
    }
    for n in &e.notes {
        crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, n);
    }
    ui.add_space(6.0);
    crate::ui_kit::heading(ui, "By activity");
    egui::Grid::new("evm-activities").num_columns(7).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
        for h in ["Activity", "Name", "BAC", "PV", "Percent", "EV", "AC"] {
            crate::ui_kit::th(ui, &h);
        }
        ui.end_row();
        for a in &e.activities {
            ui.label(&a.id);
            ui.label(&a.name);
            ui.label(money(a.bac, decimals));
            ui.label(money(a.pv, decimals));
            ui.label(format!("{} %{}", a.percent_complete, if a.measured { "" } else { " (no element)" }));
            ui.label(money(a.ev, decimals));
            ui.label(figure(&a.ac, decimals));
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new("By month (cumulative to the month end or the data date)").strong());
    egui::Grid::new("evm-periods").num_columns(7).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
        for h in ["Month", "PV in month", "PV to date", "EV in month", "EV to date", "AC in month", "AC to date"] {
            crate::ui_kit::th(ui, &h);
        }
        ui.end_row();
        for r in &out.periods.rows {
            ui.label(&r.label);
            ui.label(money(r.pv_period, decimals));
            ui.label(money(r.pv_cum, decimals));
            ui.label(figure(&r.ev_period, decimals));
            ui.label(figure(&r.ev_cum, decimals));
            ui.label(figure(&r.ac_period, decimals));
            ui.label(figure(&r.ac_cum, decimals));
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    crate::ui_kit::heading(ui, "S-curve");
    paint_curve(ui, &out.curve);
}

/// The S-curve of cumulative planned (blue), earned (green) and actual (red) value. A series with
/// no value is not drawn.
fn paint_curve(ui: &mut Ui, curve: &SCurveData) {
    let size = egui::vec2(ui.available_width().clamp(420.0, 760.0), 260.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let ink = ui.visuals().text_color();
    let faint = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let inner = rect.shrink2(egui::vec2(70.0, 30.0));
    let pts = &curve.points;
    if pts.len() < 2 {
        painter.text(rect.center(), Align2::CENTER_CENTER, "No curve to draw", FontId::proportional(12.0), ink);
        return;
    }
    let (first, last) = (pts[0].date, pts[pts.len() - 1].date);
    let span = last.days_since(first).max(1) as f32;
    let value = |f: &Figure<Decimal>| f.value().map(|v| v.to_f64());
    let max = pts
        .iter()
        .flat_map(|p| [Some(p.pv.to_f64()), value(&p.ev), value(&p.ac)])
        .flatten()
        .fold(curve.bac.to_f64(), f64::max)
        .max(1e-9);
    for q in 0..=4 {
        let y = inner.bottom() - inner.height() * q as f32 / 4.0;
        painter.line_segment([pos2(inner.left(), y), pos2(inner.right(), y)], Stroke::new(1.0_f32, faint));
        painter.text(pos2(inner.left() - 6.0, y), Align2::RIGHT_CENTER, format!("{:.0}", max * q as f64 / 4.0), FontId::proportional(10.0), ink);
    }
    let map = |date: Date, v: f64| pos2(inner.left() + inner.width() * date.days_since(first) as f32 / span, inner.bottom() - inner.height() * (v / max) as f32);
    let series: [(&str, Color32, Vec<Option<f64>>); 3] = [
        ("Planned", Color32::from_rgb(0x3B, 0x82, 0xF6), pts.iter().map(|p| Some(p.pv.to_f64())).collect()),
        ("Earned", Color32::from_rgb(0x22, 0xC5, 0x5E), pts.iter().map(|p| value(&p.ev)).collect()),
        ("Actual", Color32::from_rgb(0xEF, 0x44, 0x44), pts.iter().map(|p| value(&p.ac)).collect()),
    ];
    for (i, (name, colour, values)) in series.iter().enumerate() {
        let line: Vec<_> = pts.iter().zip(values).filter_map(|(p, v)| v.map(|v| map(p.date, v))).collect();
        if line.len() >= 2 {
            painter.add(Shape::line(line, Stroke::new(2.0_f32, *colour)));
        } else if line.len() == 1 {
            painter.circle_filled(line[0], 3.0, *colour);
        }
        let drawn = values.iter().any(Option::is_some);
        painter.text(pos2(inner.left() + 90.0 * i as f32, rect.top() + 8.0), Align2::LEFT_CENTER, if drawn { name.to_string() } else { format!("{name} (not drawn)") }, FontId::proportional(11.0), *colour);
    }
    painter.text(pos2(inner.left(), rect.bottom() - 6.0), Align2::LEFT_BOTTOM, first.to_string(), FontId::proportional(10.0), ink);
    painter.text(pos2(inner.right(), rect.bottom() - 6.0), Align2::RIGHT_BOTTOM, last.to_string(), FontId::proportional(10.0), ink);
    painter.text(pos2(rect.center().x, rect.bottom() - 6.0), Align2::CENTER_BOTTOM, format!("data date {}", curve.data_date), FontId::proportional(10.0), ink);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_colour_runs_from_grey_through_amber_to_green() {
        assert_eq!(percent_colour(None), [0x5A, 0x62, 0x6B]);
        assert_eq!(percent_colour(Some(0.0)), [0x6F, 0x7D, 0x93]);
        assert_eq!(percent_colour(Some(100.0)), [0x3F, 0xB9, 0x68]);
        let mid = percent_colour(Some(50.0));
        assert!(mid[0] < 0xF2 && mid[0] > 0x3F);
        assert_ne!(percent_colour(Some(1.0)), percent_colour(Some(99.0)));
    }
}
