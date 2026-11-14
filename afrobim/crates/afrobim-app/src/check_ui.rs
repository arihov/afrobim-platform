//! The windows of the model-wide check: the report (`CHECKALL`), the culvert and side drain input
//! dialogs and the rational method dialog (`RATIONAL`).
//!
//! Every input field starts empty. A field left empty is "not entered": the check says which one
//! it needs and nothing is assumed. What is entered is kept on the element (`check_inputs`), so
//! the report can run the same checks again for the whole model and the entries survive save and
//! open.

use std::collections::BTreeSet;

use afrobim_drainage::design::mowt::{self, PipeEntrance, PipeWall};
use afrobim_kernel::{Document, ElementId, Note, SelectMode};
use egui::{Color32, RichText, Ui};

use crate::app::AfroBimApp;
use crate::check_all::{self, Report, Row, Sources, Status};
use crate::check_inputs::{self as inputs, CatchmentRow, CulvertInputs, DrainInputs, RationalInputs};

const GOOD: Color32 = Color32::from_rgb(0x4D, 0xC0, 0x7A);
const BAD: Color32 = Color32::from_rgb(0xEF, 0x6C, 0x63);
const WARN: Color32 = Color32::from_rgb(0xD9, 0x8A, 0x2B);
const NEED: Color32 = Color32::from_rgb(0x5B, 0x9B, 0xE6);
const NOTE: Color32 = Color32::from_rgb(0x8A, 0x91, 0x99);

fn colour(status: Status) -> Color32 {
    match status {
        Status::Passes => GOOD,
        Status::Fails => BAD,
        Status::Differs => WARN,
        Status::InputNeeded => NEED,
        Status::NotChecked => NOTE,
    }
}

/// The side drain capacity dialog: the entries on the element, as typed.
pub struct DrainDialog {
    pub element: ElementId,
    pub form: DrainInputs,
}

/// The culvert dialog.
pub struct CulvertDialog {
    pub element: ElementId,
    pub form: CulvertInputs,
    /// The pipe row of DDM Appendix 9.7 being read for its range of n (a reading aid only).
    wall: Option<PipeWall>,
    entrance: Option<PipeEntrance>,
}

/// The rational method dialog. With an element, its entries are kept on it.
pub struct RationalDialog {
    pub element: Option<ElementId>,
    pub form: RationalInputs,
    result: Option<Result<mowt::RationalResult, String>>,
}

/// The state of the check windows.
pub struct CheckUi {
    pub report: Option<Report>,
    pub report_open: bool,
    statuses: [bool; 5],
    category: Option<String>,
    pub drain: Option<DrainDialog>,
    pub culvert: Option<CulvertDialog>,
    pub rational: Option<RationalDialog>,
}

impl Default for CheckUi {
    fn default() -> Self {
        CheckUi { report: None, report_open: false, statuses: [true; 5], category: None, drain: None, culvert: None, rational: None }
    }
}

impl CheckUi {
    pub fn open_drain(&mut self, doc: &Document, element: ElementId) {
        self.drain = Some(DrainDialog { element, form: inputs::load(doc, element, inputs::KEY_DRAIN).unwrap_or_default() });
    }

    pub fn open_culvert(&mut self, doc: &Document, element: ElementId) {
        self.culvert = Some(CulvertDialog { element, form: inputs::load(doc, element, inputs::KEY_CULVERT).unwrap_or_default(), wall: None, entrance: None });
    }

    /// The rational method, for the element when it is a drain or a culvert, else on its own.
    pub fn open_rational(&mut self, doc: &Document, element: Option<ElementId>) {
        let element = element.filter(|id| doc.find(*id).is_some_and(|e| e.category == afrobim_drainage::DRAIN_CATEGORY || e.category == afrobim_drainage::CULVERT_CATEGORY));
        let form = element.and_then(|id| inputs::load(doc, id, inputs::KEY_RATIONAL)).unwrap_or_else(|| RationalInputs { rows: vec![CatchmentRow::default()], ..Default::default() });
        self.rational = Some(RationalDialog { element, form, result: None });
    }
}

// ─── small widgets ──────────────────────────────────────────────────────────────────────

fn field(ui: &mut Ui, label: &str, value: &mut String, hint: &str) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(180.0));
    ui.end_row();
}

fn flow_rows(ui: &mut Ui, flow: &mut String, from_rational: &mut bool, has_rational: bool, open_rational: &mut bool) {
    ui.label("Design flow (m³/s)");
    ui.vertical(|ui| {
        ui.add_enabled(!*from_rational, egui::TextEdit::singleline(flow).hint_text("enter the flow").desired_width(180.0));
        ui.horizontal(|ui| {
            ui.add_enabled(has_rational, egui::Checkbox::new(from_rational, "take it from the rational method"));
            if ui.small_button("Rational method…").clicked() {
                *open_rational = true;
            }
        });
        if !has_rational {
            *from_rational = false;
        }
    });
    ui.end_row();
}

// ─── the app side ───────────────────────────────────────────────────────────────────────

impl AfroBimApp {
    /// What the report needs from the app: each element's module messages and category names.
    fn with_sources<T>(&mut self, f: impl FnOnce(&Document, &Sources) -> T) -> T {
        self.sync_generated();
        let vis = crate::visibility_ui::any_stored(&self.doc).then(|| self.vis_source());
        let notes_of = |id: ElementId| -> Option<Result<Vec<Note>, String>> { self.cache.get(&id).map(|c| c.generated.as_ref().map(|g| g.notes.clone()).map_err(|e| e.clone())) };
        let labels = self.module_labels();
        let label_of = |category: &str| labels.get(category).cloned().unwrap_or_else(|| category.to_string());
        // The obstruction scene is only built when some sign has visibility inputs stored (above).
        f(&self.doc, &Sources { notes_of: &notes_of, label_of: &label_of, vis: vis.as_ref() })
    }

    /// CHECKALL: runs every check on every element and opens the report.
    pub(crate) fn run_check_all(&mut self) {
        let report = self.with_sources(check_all::run_all);
        self.status = format!(
            "Checked the model: {} check(s), {} pass, {} fail, {} differ, {} need input, {} not checked.",
            report.rows.len(),
            report.count(Status::Passes),
            report.count(Status::Fails),
            report.count(Status::Differs),
            report.count(Status::InputNeeded),
            report.count(Status::NotChecked)
        );
        self.checks.report = Some(report);
        self.checks.report_open = true;
    }

    /// The rows of the checks of one element, for the single-element dialogs.
    pub(crate) fn check_rows_of(&mut self, id: ElementId) -> Vec<Row> {
        self.with_sources(|doc, src| check_all::run_one(doc, id, src))
    }

    /// Selects an element and frames it in the current view.
    pub(crate) fn zoom_to_element(&mut self, id: ElementId) {
        self.doc.select(&[id], SelectMode::Replace);
        self.sync_generated();
        let Some(e) = self.doc.find(id).cloned() else { return };
        if self.view_3d {
            let site_mode = self.site_mode();
            let Some(frame) = self.frame_of(&e, site_mode) else {
                self.status = format!("{} is not shown in the 3D view.", e.name);
                return;
            };
            self.camera.yaw = (-60.0f64).to_radians();
            self.camera.pitch = 30.0f64.to_radians();
            match self.mesh_bounds(&e, frame) {
                Some((lo, hi)) if crate::view3d::is_site_solid(&e.category) => {
                    let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
                    let size = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
                    self.camera.fit_sphere(centre, (size / 2.0).max(1500.0));
                }
                _ => {
                    let centre = [frame.origin[0], frame.origin[1], frame.origin[2] + e.height / 2.0];
                    self.camera.fit_sphere(centre, e.width.max(e.height).max(1500.0));
                }
            }
            self.fitted_3d = true;
        } else {
            let (left, top, w, h) = self.ctl.viewport;
            if w > 50.0 {
                self.view.fit(&e.bounds(), left, top, w, h, 60.0);
            }
        }
        self.status = format!("{} selected.", e.name);
    }

    pub(crate) fn check_windows(&mut self, ctx: &egui::Context) {
        if self.checks.report_open {
            self.check_report_window(ctx);
        }
        self.drain_dialog(ctx);
        self.culvert_dialog(ctx);
        self.rational_dialog(ctx);
    }

    // ─── the report ──────────────────────────────────────────────────────────────────────

    fn check_report_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let (mut again, mut csv, mut pack) = (false, false, false);
        let mut zoom: Option<ElementId> = None;
        let c = &mut self.checks;
        let Some(report) = c.report.as_ref() else {
            self.checks.report_open = false;
            return;
        };
        crate::win::new(ctx, "Check the whole model").open(&mut open).default_width(900.0).default_height(640.0).show(ctx, |ui| {
            ui.label(
                RichText::new("Every check an engine has, on every element it applies to, with the inputs kept on each element. A pass means the engine's own lines passed on those inputs: it is a calculation for an engineer to check, not a certification. Warnings the engines raise are in the calculation lines, as the engines wrote them.")
                    .small()
                    .weak(),
            );
            ui.horizontal_wrapped(|ui| {
                for (i, s) in Status::ALL.iter().enumerate() {
                    let text = RichText::new(format!("{} ({})", s.label(), report.count(*s))).color(colour(*s));
                    ui.toggle_value(&mut c.statuses[i], text);
                }
                ui.separator();
                ui.label("Category");
                let shown = c.category.clone().unwrap_or_else(|| "all".to_string());
                egui::ComboBox::from_id_salt("check-category").selected_text(shown).show_ui(ui, |ui| {
                    ui.selectable_value(&mut c.category, None, "all");
                    for cat in report.categories() {
                        ui.selectable_value(&mut c.category, Some(cat.clone()), cat);
                    }
                });
            });
            ui.horizontal(|ui| {
                if ui.button("Run again").clicked() {
                    again = true;
                }
                if ui.button("Export CSV…").clicked() {
                    csv = true;
                }
                if ui.button("Export calculation pack…").clicked() {
                    pack = true;
                }
            });
            ui.separator();
            // What needs attention first: failures, then missing input, then differences, then the rest.
            let rank = |s: Status| match s {
                Status::Fails => 0,
                Status::InputNeeded => 1,
                Status::Differs => 2,
                Status::NotChecked => 3,
                Status::Passes => 4,
            };
            let mut visible = report.visible(&c.statuses, c.category.as_deref());
            visible.sort_by_key(|i| rank(report.rows[*i].status));
            ui.label(RichText::new(format!("{} of {} check(s) shown", visible.len(), report.rows.len())).small().weak());
            egui::ScrollArea::vertical().id_salt("check-rows").max_height(ui.available_height() - 90.0).auto_shrink([false, false]).show(ui, |ui| {
                for i in visible {
                    let r = &report.rows[i];
                    ui.horizontal(|ui| {
                        if ui.small_button("Show").on_hover_text("Select the element and zoom to it").clicked() {
                            zoom = Some(r.element);
                        }
                        ui.label(RichText::new(r.status.label()).strong().color(colour(r.status)));
                        if ui.link(RichText::new(&r.mark).strong()).on_hover_text("Select the element and zoom to it").clicked() {
                            zoom = Some(r.element);
                        }
                        ui.label(RichText::new(format!("{}: {}", r.category, r.check)).small());
                    });
                    ui.label(RichText::new(&r.reason).small().weak());
                    if !r.lines.is_empty() || !r.notes.is_empty() {
                        let heading = if r.lines.is_empty() { "Messages from the model".to_string() } else { format!("Calculation ({} line(s))", r.lines.len()) };
                        egui::CollapsingHeader::new(RichText::new(heading).small()).id_salt(("check-lines", i)).show(ui, |ui| {
                            for l in &r.lines {
                                let flagged = l.contains("FAILS") || l.starts_with("WARNING") || l.starts_with("Check:") || l.contains("DIFFERS");
                                let text = RichText::new(l).small().monospace();
                                ui.label(if flagged { text.color(WARN) } else { text });
                            }
                            if !r.notes.is_empty() {
                                ui.label(RichText::new("Messages from the model").small().strong());
                                for n in &r.notes {
                                    ui.label(RichText::new(n).small().monospace());
                                }
                            }
                        });
                    }
                    ui.separator();
                }
            });
            egui::CollapsingHeader::new(format!("Categories with no check ({})", report.no_check.len())).default_open(true).show(ui, |ui| {
                for (category, why) in &report.no_check {
                    ui.label(RichText::new(format!("{category}: {why}")).small().weak());
                }
            });
        });
        self.checks.report_open = open;
        if again {
            self.run_check_all();
        }
        if let Some(id) = zoom {
            self.zoom_to_element(id);
        }
        if csv {
            self.export_check_csv();
        }
        if pack {
            self.export_check_pack();
        }
    }

    fn export_check_csv(&mut self) {
        let Some(report) = self.checks.report.clone() else { return };
        let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("model-check.csv").save_file() else { return };
        if path.extension().is_none() {
            path.set_extension("csv");
        }
        self.write_check_csv(&report, &path);
    }

    pub(crate) fn write_check_csv(&mut self, report: &Report, path: &std::path::Path) {
        self.status = match std::fs::write(path, report.to_csv()) {
            Ok(()) => format!("Wrote {} check(s) to {}.", report.rows.len(), path.display()),
            Err(e) => format!("Could not write {}: {e}", path.display()),
        };
    }

    fn export_check_pack(&mut self) {
        let Some(report) = self.checks.report.clone() else { return };
        let Some(mut path) = rfd::FileDialog::new().add_filter("Text", &["txt"]).set_file_name("calculation-pack.txt").save_file() else { return };
        if path.extension().is_none() {
            path.set_extension("txt");
        }
        self.write_check_pack(&report, &path);
    }

    pub(crate) fn write_check_pack(&mut self, report: &Report, path: &std::path::Path) {
        let title = self.title_for_pack();
        self.status = match std::fs::write(path, report.to_pack(&title)) {
            Ok(()) => format!("Wrote the calculation pack to {}.", path.display()),
            Err(e) => format!("Could not write {}: {e}", path.display()),
        };
    }

    // ─── side drain ──────────────────────────────────────────────────────────────────────

    fn drain_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.checks.drain.take() else { return };
        let mark = self.doc.find(d.element).map(|e| e.name.clone()).unwrap_or_default();
        let has_rational = inputs::load::<RationalInputs>(&self.doc, d.element, inputs::KEY_RATIONAL).is_some();
        let (mut keep, mut run, mut rational) = (true, false, false);
        crate::win::new(ctx, format!("Check the capacity of {mark}")).collapsible(false).resizable(false).show(ctx, |ui| {
            egui::Grid::new("drain-check").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                flow_rows(ui, &mut d.form.flow_m3_s, &mut d.form.flow_from_rational, has_rational, &mut rational);
                ui.label("Basis");
                ui.vertical(|ui| {
                    ui.radio_value(&mut d.form.dmrb, false, "MoWT Road Design Manual Vol. 2 (2010)");
                    ui.radio_value(&mut d.form.dmrb, true, "UK DMRB (CG 501, CD 521)");
                });
                ui.end_row();
                field(ui, "Required freeboard (m)", &mut d.form.required_freeboard_m, "only if the project states one");
            });
            ui.add_space(4.0);
            ui.label(RichText::new("The design flow is the engineer's: AfroBIM holds no rainfall data. Every field starts empty. The drain's gradient, lining and roughness are those entered in Properties. What you enter is kept on the drain, for CHECKALL.").small().weak());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Check").clicked() {
                    run = true;
                }
                if ui.button("Cancel").clicked() {
                    keep = false;
                }
            });
        });
        if rational {
            self.checks.open_rational(&self.doc, Some(d.element));
        }
        if run {
            inputs::save(&mut self.doc, d.element, inputs::KEY_DRAIN, &d.form);
            self.schedule_commit_now();
            let rows = self.check_rows_of(d.element);
            match rows.iter().find(|r| r.check == "Side drain capacity") {
                Some(r) if r.status == Status::InputNeeded => {
                    self.status = format!("Not checked: {}", r.reason);
                    self.checks.drain = Some(d);
                }
                Some(r) => {
                    self.status = format!("Checked {mark}: {}. {}", r.status.label(), r.reason);
                    self.design_report = Some((mark, r.lines.clone()));
                }
                None => self.status = "The drain could not be checked.".into(),
            }
        } else if keep {
            // Entries typed so far stay in the dialog and are kept on the element when it is checked.
            self.checks.drain = Some(d);
        }
    }

    // ─── culvert ─────────────────────────────────────────────────────────────────────────

    fn culvert_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.checks.culvert.take() else { return };
        let mark = self.doc.find(d.element).map(|e| e.name.clone()).unwrap_or_default();
        let has_rational = inputs::load::<RationalInputs>(&self.doc, d.element, inputs::KEY_RATIONAL).is_some();
        let (mut keep, mut run, mut rational) = (true, false, false);
        crate::win::new(ctx, format!("Check the culvert {mark}")).collapsible(false).resizable(false).show(ctx, |ui| {
            egui::Grid::new("culvert-check").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                flow_rows(ui, &mut d.form.flow_m3_s, &mut d.form.flow_from_rational, has_rational, &mut rational);
                ui.label("Manning's n of the barrel");
                ui.vertical(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut d.form.manning_n).hint_text("enter n").desired_width(180.0));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("DDM Appendix 9.7 row").small());
                        let shown = d.wall.map(|w| format!("{w:?}")).unwrap_or_else(|| "read a range".to_string());
                        egui::ComboBox::from_id_salt("culvert-wall").selected_text(shown).show_ui(ui, |ui| {
                            for w in [PipeWall::ConcreteSmooth, PipeWall::CorrugatedMetal68x13, PipeWall::CorrugatedMetal150x25, PipeWall::CorrugatedMetal125x25, PipeWall::CorrugatedMetal75x25, PipeWall::StructuralPlate150x50, PipeWall::StructuralPlate230x64, PipeWall::CorrugatedMetalHelical68x13FullCircularFlow, PipeWall::SpiralRibMetalSmooth] {
                                ui.selectable_value(&mut d.wall, Some(w), format!("{w:?}"));
                            }
                        });
                    });
                    if let Some(w) = d.wall {
                        let (lo, hi) = w.manning_n_range();
                        ui.label(RichText::new(format!("The appendix recommends {lo} to {hi}. The value within it is yours to choose and type above.")).small().weak());
                    }
                });
                ui.end_row();
                ui.label("Entrance loss coefficient Ke");
                ui.vertical(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut d.form.entrance_ke).hint_text("enter Ke").desired_width(180.0));
                    let shown = d.entrance.map(|e| e.label().to_string()).unwrap_or_else(|| "fill from DDM Appendix 9.6".to_string());
                    egui::ComboBox::from_id_salt("culvert-entrance").width(380.0).selected_text(shown).show_ui(ui, |ui| {
                        for entrance in ENTRANCES {
                            if ui.selectable_value(&mut d.entrance, Some(entrance), entrance.label()).clicked() {
                                d.form.entrance_ke = entrance.ke().to_string();
                                d.form.entrance_note = format!("DDM Appendix 9.6: {}", entrance.label());
                            }
                        }
                    });
                });
                ui.end_row();
                field(ui, "Tailwater depth above the outlet invert (m)", &mut d.form.tailwater_m, "enter the depth");
                ui.label("Layout limits of");
                ui.vertical(|ui| {
                    ui.radio_value(&mut d.form.dmrb, false, "MoWT Road Design Manual Vol. 2 (2010)");
                    ui.radio_value(&mut d.form.dmrb, true, "UK DMRB");
                });
                ui.end_row();
                field(ui, "Shoulder break level at the road low point (m)", &mut d.form.shoulder_level_m, "only for the freeboard");
                ui.label("The road is");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut d.form.major_road, None, "not stated");
                    ui.selectable_value(&mut d.form.major_road, Some(true), "a major road");
                    ui.selectable_value(&mut d.form.major_road, Some(false), "a minor road");
                });
                ui.end_row();
            });
            ui.add_space(4.0);
            ui.label(RichText::new("The outlet-control headwater follows MoWT DDM Section 9.4.2 and is one half of a culvert check: inlet control is not computed. Every field starts empty and what you enter is kept on the culvert, for CHECKALL.").small().weak());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Check").clicked() {
                    run = true;
                }
                if ui.button("Cancel").clicked() {
                    keep = false;
                }
            });
        });
        if rational {
            self.checks.open_rational(&self.doc, Some(d.element));
        }
        if run {
            inputs::save(&mut self.doc, d.element, inputs::KEY_CULVERT, &d.form);
            self.schedule_commit_now();
            let rows = self.check_rows_of(d.element);
            let mut lines: Vec<String> = Vec::new();
            for r in &rows {
                lines.push(format!("{}: {}. {}", r.check, r.status.label(), r.reason));
                lines.extend(r.lines.iter().cloned());
            }
            match rows.iter().find(|r| r.check.starts_with("Culvert outlet control")) {
                Some(r) if r.status == Status::InputNeeded => {
                    self.status = format!("Not checked: {}", r.reason);
                    self.checks.culvert = Some(d);
                }
                Some(_) => {
                    self.status = format!("Checked {mark}. Read the calculation.");
                    self.design_report = Some((mark, lines));
                }
                None => self.status = "The culvert could not be checked.".into(),
            }
        } else if keep {
            self.checks.culvert = Some(d);
        }
    }

    // ─── rational method ─────────────────────────────────────────────────────────────────

    fn rational_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.checks.rational.take() else { return };
        let target = d.element.and_then(|id| self.doc.find(id)).map(|e| e.name.clone());
        let (mut keep, mut calculate, mut use_it) = (true, false, false);
        crate::win::new(ctx, match &target {
            Some(mark) => format!("Rational method for {mark}"),
            None => "Rational method".to_string(),
        })
        .collapsible(false)
        .resizable(true)
        .default_width(600.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(520.0).auto_shrink([false, true]).show(ui, |ui| {
                ui.label(RichText::new("Parts of the catchment").strong());
                let mut remove = None;
                egui::Grid::new("rational-rows").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
                    for h in ["What the part is", "Area (ha)", "Runoff coefficient C", ""] {
                        ui.label(RichText::new(h).small());
                    }
                    ui.end_row();
                    for (i, r) in d.form.rows.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(&mut r.description).hint_text("description").desired_width(180.0));
                        ui.add(egui::TextEdit::singleline(&mut r.area_ha).hint_text("area").desired_width(80.0));
                        ui.add(egui::TextEdit::singleline(&mut r.runoff_coefficient).hint_text("C").desired_width(80.0));
                        if ui.small_button("x").clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = remove {
                    d.form.rows.remove(i);
                }
                if ui.button("Add a part").clicked() {
                    d.form.rows.push(CatchmentRow::default());
                }
                ui.add_space(6.0);
                egui::Grid::new("rational-flow").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    field(ui, "Design rainfall intensity (mm/h)", &mut d.form.intensity_mm_h, "for the time of concentration");
                    field(ui, "Where the intensity comes from", &mut d.form.intensity_source, "your source and its date");
                    field(ui, "Recurrence multiplier", &mut d.form.frequency_multiplier, "empty: none applied");
                    field(ui, "Storage factor Cs", &mut d.form.storage_factor, "empty: the basic method");
                });
                ui.add_space(4.0);
                ui.label(RichText::new("MoWT Road Design Manual Vol. 2, Sections 4.4.2 and 4.4.3. AfroBIM holds no rainfall data: the intensity, the areas and the runoff coefficients are yours. Every field starts empty.").small().weak());
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Calculate").clicked() {
                        calculate = true;
                    }
                    if let Some(mark) = &target {
                        if ui.button(format!("Keep on {mark} and use as its design flow")).clicked() {
                            calculate = true;
                            use_it = true;
                        }
                    }
                    if ui.button("Close").clicked() {
                        keep = false;
                    }
                });
                match &d.result {
                    Some(Ok(r)) => {
                        ui.separator();
                        ui.label(RichText::new(format!("Design peak flow Q = {:.3} m³/s", r.peak_flow_m3_s)).strong());
                        for l in r.calculation() {
                            let flagged = l.starts_with("Check:");
                            let text = RichText::new(l).small().monospace();
                            ui.label(if flagged { text.color(WARN) } else { text });
                        }
                    }
                    Some(Err(e)) => {
                        ui.separator();
                        ui.label(RichText::new(format!("Not calculated: {e}")).color(BAD));
                    }
                    None => {}
                }
            });
        });
        if calculate {
            d.result = Some(d.form.run());
            if let Some(Ok(r)) = &d.result {
                self.status = format!("Rational method: Q = {:.3} m³/s.", r.peak_flow_m3_s);
                if let (true, Some(id)) = (use_it, d.element) {
                    inputs::save(&mut self.doc, id, inputs::KEY_RATIONAL, &d.form);
                    // The drain or culvert takes its flow from these inputs from now on.
                    let is_drain = self.doc.find(id).is_some_and(|e| e.category == afrobim_drainage::DRAIN_CATEGORY);
                    if is_drain {
                        let mut f: DrainInputs = inputs::load(&self.doc, id, inputs::KEY_DRAIN).unwrap_or_default();
                        f.flow_from_rational = true;
                        inputs::save(&mut self.doc, id, inputs::KEY_DRAIN, &f);
                        if let Some(dd) = self.checks.drain.as_mut().filter(|dd| dd.element == id) {
                            dd.form.flow_from_rational = true;
                        }
                    } else {
                        let mut f: CulvertInputs = inputs::load(&self.doc, id, inputs::KEY_CULVERT).unwrap_or_default();
                        f.flow_from_rational = true;
                        inputs::save(&mut self.doc, id, inputs::KEY_CULVERT, &f);
                        if let Some(dd) = self.checks.culvert.as_mut().filter(|dd| dd.element == id) {
                            dd.form.flow_from_rational = true;
                        }
                    }
                    self.schedule_commit_now();
                }
            }
        }
        if keep {
            self.checks.rational = Some(d);
        }
    }

    /// The display name of each category: the module's own.
    fn module_labels(&self) -> std::collections::HashMap<String, String> {
        self.module_categories().into_iter().collect()
    }

    /// The categories present in the document, kept apart for tests.
    #[allow(dead_code)]
    pub(crate) fn categories_present(&self) -> BTreeSet<String> {
        self.doc.elements().iter().map(|e| e.category.clone()).collect()
    }
}

/// Every pipe entrance DDM Appendix 9.6 gives, in the engine's order.
const ENTRANCES: [PipeEntrance; 15] = [
    PipeEntrance::ConcreteMitredToFillSlope,
    PipeEntrance::ConcreteEndSectionConformingToFillSlope,
    PipeEntrance::ConcreteProjectingSquareCutEnd,
    PipeEntrance::ConcreteHeadwallSquareEdge,
    PipeEntrance::ConcreteHeadwallRounded,
    PipeEntrance::ConcreteHeadwallSocketEnd,
    PipeEntrance::ConcreteProjectingSocketEnd,
    PipeEntrance::ConcreteBevelledEdges,
    PipeEntrance::ConcreteSideOrSlopeTaperedInlet,
    PipeEntrance::CorrugatedMetalProjectingNoHeadwall,
    PipeEntrance::CorrugatedMetalMitredToFillSlope,
    PipeEntrance::CorrugatedMetalHeadwallSquareEdge,
    PipeEntrance::CorrugatedMetalEndSectionConformingToFillSlope,
    PipeEntrance::CorrugatedMetalBevelledEdges,
    PipeEntrance::CorrugatedMetalSideOrSlopeTaperedInlet,
];
