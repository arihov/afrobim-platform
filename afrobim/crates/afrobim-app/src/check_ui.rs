//! The windows of the model-wide check: the report (`CHECKALL`), the culvert and side drain input
//! dialogs and the rational method dialog (`RATIONAL`).
//!
//! Every input field starts empty. A field left empty is "not entered": the check says which one
//! it needs and nothing is assumed. What is entered is kept on the element (`check_inputs`), so
//! the report can run the same checks again for the whole model and the entries survive save and
//! open.

use std::collections::BTreeSet;

use afrobim_drainage::design::mowt::{PipeEntrance, PipeWall};
use afrobim_drainage::design::DesignBasis;
use afrobim_kernel::{Document, Element, ElementId, Note, SelectMode};
use egui::{Color32, RichText, Ui};

use crate::app::AfroBimApp;
use crate::check_all::{self, Report, Row, Sources, Status};
use crate::check_inputs::{self as inputs, CatchmentRow, CulvertInputs, DrainInputs, RationalInputs};

#[allow(non_snake_case)]

fn GOOD() -> egui::Color32 {

    crate::theme::success()

}
#[allow(non_snake_case)]
fn BAD() -> egui::Color32 {
    crate::theme::danger()
}
#[allow(non_snake_case)]
fn WARN() -> egui::Color32 {
    crate::theme::warn()
}
#[allow(non_snake_case)]
fn NEED() -> egui::Color32 {
    crate::theme::accent_text()
}
#[allow(non_snake_case)]
fn NOTE() -> egui::Color32 {
    crate::theme::muted()
}

fn colour(status: Status) -> Color32 {
    match status {
        Status::Passes => GOOD(),
        Status::Fails => BAD(),
        Status::Differs => WARN(),
        Status::InputNeeded => NEED(),
        Status::NotChecked => NOTE(),
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
    /// The peak flow and the calculation lines of whichever method ran.
    pub(crate) result: Option<Result<(f64, Vec<String>), String>>,
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

/// The three bases, one choice. Each basis keeps its own values; nothing carries across.
fn basis_radios(ui: &mut Ui, form: &mut DrainInputs) {
    let mut basis = form.basis();
    ui.vertical(|ui| {
        for b in DesignBasis::EVERY {
            ui.radio_value(&mut basis, b, basis_label(b));
        }
    });
    if basis != form.basis() {
        form.set_basis(basis);
    }
}

fn culvert_basis_radios(ui: &mut Ui, form: &mut CulvertInputs) {
    let mut basis = form.basis();
    ui.vertical(|ui| {
        for b in DesignBasis::EVERY {
            ui.radio_value(&mut basis, b, basis_label(b));
        }
    });
    if basis != form.basis() {
        form.set_basis(basis);
    }
}

fn basis_label(b: DesignBasis) -> &'static str {
    match b {
        DesignBasis::Mowt => "MoWT Road Design Manual Vol. 2 (2010)",
        DesignBasis::Dmrb => "UK DMRB (CG 501, CD 521)",
        _ => "SANRAL Drainage Manual, 6th edition (2013)",
    }
}

/// The SANRAL rational method entries: run-off coefficient from the Table 3.7 classes, the time
/// of concentration by reach, and the point rainfall depth, which is always yours.
fn sanral_rational_form(ui: &mut Ui, s: &mut inputs::SanralRational) {
    use afrobim_drainage::design::sanral::UrbanUse;
    let small = |ui: &mut Ui, text: &str| {
        ui.label(RichText::new(text).small());
    };
    crate::ui_kit::heading(ui, "Run-off coefficient (SDM Table 3.7 and 3.8)");
    crate::ui_kit::grid(ui, "sanral-basic", |ui| {
        field(ui, "Catchment area (km²)", &mut s.area_km2, "enter the area");
        field(ui, "Mean annual precipitation (mm)", &mut s.map_mm, "selects the column of Table 3.7");
        field(ui, "Dolomitic share of the rural area (%)", &mut s.dolomite_percent, "enter 0 if none");
        ui.label("Catchment kind for Ft (Table 3.8)");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut s.ft_class, Some(0), "steep and impermeable");
            ui.selectable_value(&mut s.ft_class, Some(1), "flat and permeable");
        });
        ui.end_row();
        field(ui, "Return period (years, or maximum)", &mut s.return_period, "2, 5, 10, 20, 50, 100");
        field(ui, "Rural area factor alpha", &mut s.alpha, "alpha + beta + gamma = 1");
        field(ui, "Urban area factor beta", &mut s.beta, "");
        field(ui, "Lakes area factor gamma", &mut s.gamma, "");
        field(ui, "Run-off coefficient of lakes C3", &mut s.lakes_c3, "only with lakes");
    });
    small(ui, "Shares of the rural area in each class, percent; leave a class empty when it is not present. Each group adds up to 100.");
    egui::Grid::new("sanral-shares").num_columns(3).spacing([14.0, 4.0]).show(ui, |ui| {
        ui.vertical(|ui| {
            for ((_, name), v) in inputs::RURAL_SLOPES.iter().zip(s.slope.iter_mut()) {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(v).desired_width(44.0));
                    ui.label(RichText::new(*name).small());
                });
            }
        });
        ui.vertical(|ui| {
            for ((_, name), v) in inputs::RURAL_PERMEABILITY.iter().zip(s.permeability.iter_mut()) {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(v).desired_width(44.0));
                    ui.label(RichText::new(*name).small());
                });
            }
        });
        ui.vertical(|ui| {
            for ((_, name), v) in inputs::RURAL_VEGETATION.iter().zip(s.vegetation.iter_mut()) {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(v).desired_width(44.0));
                    ui.label(RichText::new(*name).small());
                });
            }
        });
        ui.end_row();
        small(ui, "Surface slope (Cs)");
        small(ui, "Permeability (Cp)");
        small(ui, "Vegetation (Cv)");
        ui.end_row();
    });
    ui.add_space(4.0);
    crate::ui_kit::heading(ui, "Urban parts (C2): the factor is yours, within the printed range shown");
    let mut remove = None;
    for (i, u) in s.urban.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let shown = u.land_use.map(|k| UrbanUse::ALL[k.min(11)].label().to_string()).unwrap_or_else(|| "land use".to_string());
            egui::ComboBox::from_id_salt(format!("sanral-urban-{i}")).width(210.0).selected_text(shown).show_ui(ui, |ui| {
                for (k, use_) in UrbanUse::ALL.iter().enumerate() {
                    let (lo, hi) = use_.range();
                    ui.selectable_value(&mut u.land_use, Some(k), format!("{} ({lo} to {hi})", use_.label()));
                }
            });
            ui.add(egui::TextEdit::singleline(&mut u.percent).hint_text("share %").desired_width(60.0));
            ui.add(egui::TextEdit::singleline(&mut u.factor).hint_text("factor").desired_width(60.0));
            if crate::ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        s.urban.remove(i);
    }
    if ui.button("Add an urban part").clicked() {
        s.urban.push(inputs::UrbanRow::default());
    }
    ui.add_space(6.0);
    crate::ui_kit::heading(ui, "Time of concentration, by reach (SDM 3.5.1.4)");
    let mut remove = None;
    for (i, r) in s.reaches.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut r.kind, 0, "overland (Kerby)");
            ui.selectable_value(&mut r.kind, 1, "channel");
            ui.selectable_value(&mut r.kind, 2, "urban");
            if r.kind == 0 {
                ui.add(egui::TextEdit::singleline(&mut r.r).hint_text("r").desired_width(44.0));
            }
            ui.add(egui::TextEdit::singleline(&mut r.length).hint_text(if r.kind == 2 { "length m" } else { "length km" }).desired_width(70.0));
            ui.add(egui::TextEdit::singleline(&mut r.slope_or_velocity).hint_text(if r.kind == 2 { "velocity m/s" } else { "slope m/m" }).desired_width(80.0));
            if crate::ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        s.reaches.remove(i);
    }
    if ui.button("Add a reach").clicked() {
        s.reaches.push(inputs::ReachRow::default());
    }
    ui.add_space(6.0);
    crate::ui_kit::heading(ui, "Rainfall (always yours)");
    crate::ui_kit::grid(ui, "sanral-rain", |ui| {
        field(ui, "Point rainfall depth for Tc and T (mm)", &mut s.point_rainfall_mm, "from the DDF, Hershfield or Design Rainfall Estimation");
        field(ui, "Where the depth comes from", &mut s.rainfall_source, "your source and its date");
        ui.label("Area reduction factor");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut s.area_reduction, 0, "SDM Eq. 3.10");
            ui.selectable_value(&mut s.area_reduction, 1, "a value I enter");
            ui.selectable_value(&mut s.area_reduction, 2, "not applied");
            if s.area_reduction == 1 {
                ui.add(egui::TextEdit::singleline(&mut s.area_reduction_percent).hint_text("%").desired_width(50.0));
            }
        });
        ui.end_row();
    });
    crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "SANRAL Drainage Manual, 6th edition (2013), Section 3.5.1. AfroBIM holds no rainfall data and no map: the depth, the shares and the factors are yours. Every field starts empty.");
}

/// The SANRAL culvert entries below the flow. The slope unit is a question with no default.
fn sanral_culvert_rows(ui: &mut Ui, s: &mut inputs::SanralCulvertEntries) {
    ui.label("How the bed slope is read in Table 7.2");
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut s.slope_unit, Some(0), "metre per metre");
            ui.selectable_value(&mut s.slope_unit, Some(1), "per cent");
            if s.slope_unit.is_none() {
                ui.label(RichText::new("not answered").color(WARN()));
            }
        });
        ui.label(RichText::new(inputs::SLOPE_UNIT_NOTE).small().weak());
    });
    ui.end_row();
    field(ui, "Inlet loss coefficient Kin", &mut s.k_in, "enter Kin");
    field(ui, "Outlet loss coefficient Kout", &mut s.k_out, "enter Kout");
    ui.label("Barrel friction");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut s.friction_mode, 0, "Manning n");
        ui.selectable_value(&mut s.friction_mode, 1, "Colebrook ks (m)");
        ui.add(egui::TextEdit::singleline(&mut s.friction_value).hint_text("enter the value").desired_width(110.0));
    });
    ui.end_row();
    field(ui, "Downstream energy level H2 (m)", &mut s.h2_m, "above the outlet invert");
    field(ui, "Shoulder break height above the inlet invert (m)", &mut s.shoulder_break_height_m, "optional");
    ui.label("Road class (SDM Table 7.1)");
    let shown = s.road_class.map(|i| inputs::ROAD_CLASSES[i.min(4)].label().to_string()).unwrap_or_else(|| "choose the class".to_string());
    egui::ComboBox::from_id_salt("culvert-road-class").width(380.0).selected_text(shown).show_ui(ui, |ui| {
        for (i, c) in inputs::ROAD_CLASSES.iter().enumerate() {
            ui.selectable_value(&mut s.road_class, Some(i), c.label());
        }
    });
    ui.end_row();
    field(ui, "Flow at twice the return period (m³/s)", &mut s.twice_flow_m3_s, "optional");
    field(ui, "H2 at twice the return period (m)", &mut s.twice_h2_m, "with the flow above");
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

/// What the model-wide check needs: a copy of the document, each element's module messages (or why
/// it could not be generated), the category names, and for the sign visibility check the solids and
/// x-heights its obstruction scene is built from (when some sign has stored visibility inputs).
pub struct CheckAllInput {
    pub doc: Document,
    pub notes: std::collections::HashMap<ElementId, Result<Vec<Note>, String>>,
    pub labels: std::collections::HashMap<String, String>,
    pub vis: Option<crate::visibility_ui::VisInput>,
}

impl CheckAllInput {
    /// Builds the obstruction scene (the slow part) and runs every check: the same two steps the
    /// synchronous path took.
    pub fn compute(self) -> check_all::Report {
        let CheckAllInput { doc, notes, labels, vis } = self;
        let src = vis.map(|v| {
            let solids = v.solids;
            crate::visibility_ui::source_for(&doc, &|e: &Element| solids.get(&e.id).cloned().unwrap_or_default(), v.x_heights)
        });
        let notes_of = |id: ElementId| notes.get(&id).cloned();
        let label_of = |category: &str| labels.get(category).cloned().unwrap_or_else(|| category.to_string());
        check_all::run_all(&doc, &Sources { notes_of: &notes_of, label_of: &label_of, vis: src.as_ref() })
    }
}

/// Starts [`CheckAllInput::compute`] on a worker thread, to be applied only while the document is
/// unchanged.
pub fn spawn_check_all(input: CheckAllInput) -> crate::background::Job<check_all::Report> {
    let fp = crate::background::fingerprint(&input.doc);
    crate::background::Job::spawn("Model check", fp, move || input.compute())
}

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

    /// CHECKALL: runs every check on every element on a worker thread, with a progress indicator
    /// and Cancel, and opens the report when it arrives. The obstruction scene of the sign
    /// visibility check is built there too. The report is applied only if the document has not
    /// changed since the run started.
    pub(crate) fn run_check_all(&mut self) {
        if self.bg.check_all.is_some() {
            self.status = "The model check is already running.".into();
            return;
        }
        let input = self.check_all_input();
        self.bg.check_all = Some(spawn_check_all(input));
        self.status = "Model check started on a worker thread; the model stays editable, but an edit discards the result.".into();
    }

    /// What the model-wide check needs, taken on the UI thread (see [`CheckAllInput`]).
    pub(crate) fn check_all_input(&mut self) -> CheckAllInput {
        self.sync_generated();
        let notes = self.doc.elements().iter().filter_map(|e| self.cache.get(&e.id).map(|c| (e.id, c.generated.as_ref().map(|g| g.notes.clone()).map_err(|e| e.clone())))).collect();
        let vis = crate::visibility_ui::any_stored(&self.doc).then(|| self.vis_input(None));
        CheckAllInput { doc: self.doc.clone(), notes, labels: self.module_labels(), vis }
    }

    /// Takes the finished model check in.
    pub(crate) fn apply_check_all(&mut self, report: check_all::Report) {
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
        // ZOOM Previous returns to where the view was before.
        self.ctl.remember_view(&self.view);
        self.camera_history.push(self.camera.clone());
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
        let dlg = crate::ui_kit::Dialog::titled("Check the whole model").subtitle("Every check of every element, with its status").width(900.0).show(ctx, |ui| {
            crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "Every check an engine has, on every element it applies to, with the inputs kept on each element. A pass means the engine's own lines passed on those inputs: it is a calculation for an engineer to check, not a certification. Warnings the engines raise are in the calculation lines, as the engines wrote them.");
            crate::ui_kit::heading(ui, "Show");
            ui.horizontal_wrapped(|ui| {
                for (i, s) in Status::ALL.iter().enumerate() {
                    let text = RichText::new(format!("{} ({})", s.label(), report.count(*s))).color(colour(*s));
                    ui.toggle_value(&mut c.statuses[i], text);
                }
            });
            crate::ui_kit::form(ui, "check-filter", |f| {
                f.row("Category", |ui| {
                    let shown = c.category.clone().unwrap_or_else(|| "all".to_string());
                    egui::ComboBox::from_id_salt("check-category").selected_text(shown).show_ui(ui, |ui| {
                        ui.selectable_value(&mut c.category, None, "all");
                        for cat in report.categories() {
                            ui.selectable_value(&mut c.category, Some(cat.clone()), cat);
                        }
                    });
                });
            });
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
            crate::ui_kit::heading(ui, &format!("Checks ({} of {} shown)", visible.len(), report.rows.len()));
            {
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
                                ui.label(if flagged { text.color(WARN()) } else { text });
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
            }
            egui::CollapsingHeader::new(format!("Categories with no check ({})", report.no_check.len())).default_open(true).show(ui, |ui| {
                for (category, why) in &report.no_check {
                    crate::ui_kit::note(ui, format!("{category}: {why}"));
                }
            });
        }, |ui| crate::ui_kit::Footer::new("Run again").secondary(&["Export CSV...", "Export calculation pack..."]).cancel(Some("Close")).show(ui));
        if dlg.closed {
            open = false;
        }
        if let Some((_, out)) = &dlg.inner {
            if out.cancel {
                open = false;
            }
            if out.primary {
                again = true;
            }
            match out.secondary {
                Some(0) => csv = true,
                Some(1) => pack = true,
                _ => {}
            }
        }
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
                let footer_ok = true;
        let dlg = crate::ui_kit::Dialog::titled(format!("Check the capacity of {mark}")).width(480.0).resizable(false).show(ctx, |ui| {
            crate::ui_kit::grid(ui, "drain-check", |ui| {
                flow_rows(ui, &mut d.form.flow_m3_s, &mut d.form.flow_from_rational, has_rational, &mut rational);
                ui.label("Basis");
                basis_radios(ui, &mut d.form);
                ui.end_row();
                field(ui, "Required freeboard (m)", &mut d.form.required_freeboard_m, "only if the project states one");
            });
            ui.add_space(4.0);
            crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "The design flow is the engineer's: AfroBIM holds no rainfall data. Every field starts empty. The drain's gradient, lining and roughness are those entered in Properties. What you enter is kept on the drain, for CHECKALL.");
        }, |ui| crate::ui_kit::Footer::new("Check").enabled(footer_ok, "Complete the inputs").cancel(Some("Cancel")).show(ui));
        if let Some((_, f)) = &dlg.inner {
            run = f.primary;
            if f.cancel {
                keep = false;
            }
        }
        if dlg.closed {
            keep = false;
        }
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
                let footer_ok = true;
        let dlg = crate::ui_kit::Dialog::titled(format!("Check the culvert {mark}")).width(480.0).resizable(false).show(ctx, |ui| {
            crate::ui_kit::grid(ui, "culvert-check", |ui| {
                flow_rows(ui, &mut d.form.flow_m3_s, &mut d.form.flow_from_rational, has_rational, &mut rational);
                ui.label("Design basis");
                culvert_basis_radios(ui, &mut d.form);
                ui.end_row();
                let basis = d.form.basis();
                if basis == DesignBasis::Mowt {
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
                        crate::ui_kit::note(ui, format!("The appendix recommends {lo} to {hi}. The value within it is yours to choose and type above."));
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
                ui.label("Inlet edge (HDS-5 Table A.1)");
                let inlets = inputs::circular_inlets();
                let shown = d.form.inlet_edge.and_then(|i| inlets.get(i)).map(|e| e.label().to_string()).unwrap_or_else(|| "choose the inlet".to_string());
                egui::ComboBox::from_id_salt("culvert-inlet-edge").width(380.0).selected_text(shown).show_ui(ui, |ui| {
                    for (i, e) in inlets.iter().enumerate() {
                        ui.selectable_value(&mut d.form.inlet_edge, Some(i), e.label());
                    }
                });
                ui.end_row();
                field(ui, "Allowable headwater above the inlet invert (m)", &mut d.form.allowable_headwater_m, "the engineer's limit");
                field(ui, "Shoulder break level at the road low point (m)", &mut d.form.shoulder_level_m, "only for the freeboard");
                ui.label("The road is");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut d.form.major_road, None, "not stated");
                    ui.selectable_value(&mut d.form.major_road, Some(true), "a major road");
                    ui.selectable_value(&mut d.form.major_road, Some(false), "a minor road");
                });
                ui.end_row();
                } else if basis == DesignBasis::Sanral {
                    sanral_culvert_rows(ui, &mut d.form.sanral);
                } else {
                    ui.label("");
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, "The UK DMRB basis has no culvert hydraulic check in the engine: only its layout limits are checked. Choose MoWT or SANRAL for the hydraulics.");
                    ui.end_row();
                }
            });
            ui.add_space(4.0);
            crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "MoWT: the complete check is inlet control (FHWA HDS-5 equations, the inlet edge you choose) and outlet control (DDM Section 9.4.2); the greater governs and is named, and the outlet lines are shown first. With no inlet edge or allowable headwater only the outlet half is shown, and it is marked as incomplete. SANRAL: SDM Table 7.2 and Eq. 7.4, for the barrel of this element. Each basis keeps its own values. Every field starts empty and what you enter is kept on the culvert, for CHECKALL.");
        }, |ui| crate::ui_kit::Footer::new("Check").enabled(footer_ok, "Complete the inputs").cancel(Some("Cancel")).show(ui));
        if let Some((_, f)) = &dlg.inner {
            run = f.primary;
            if f.cancel {
                keep = false;
            }
        }
        if dlg.closed {
            keep = false;
        }
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
            match rows.iter().find(|r| r.check.starts_with("Culvert outlet control") || r.check.starts_with("Culvert complete check") || r.check.starts_with("Culvert SANRAL") || r.check.starts_with("Culvert hydraulic")) {
                Some(r) if r.status == Status::InputNeeded && r.lines.is_empty() => {
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
        let keep_label = target.as_ref().map(|mark| format!("Keep on {mark} and use as its design flow"));
        let dlg = crate::ui_kit::Dialog::titled(match &target {
            Some(mark) => format!("Rational method for {mark}"),
            None => "Rational method".to_string(),
        }).subtitle("Peak flow from the catchment, by the rational method").width(600.0).show(
            ctx,
            |ui| {
            ui.scope(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Method");
                    ui.selectable_value(&mut d.form.use_sanral, false, "MoWT (Vol. 2, 4.4.2)");
                    ui.selectable_value(&mut d.form.use_sanral, true, "SANRAL (Drainage Manual 3.5.1)");
                });
                if d.form.use_sanral {
                    sanral_rational_form(ui, &mut d.form.sanral);
                } else {
                crate::ui_kit::heading(ui, "Parts of the catchment");
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
                        if crate::ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove").clicked() {
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
                crate::ui_kit::grid(ui, "rational-flow", |ui| {
                    field(ui, "Design rainfall intensity (mm/h)", &mut d.form.intensity_mm_h, "for the time of concentration");
                    field(ui, "Where the intensity comes from", &mut d.form.intensity_source, "your source and its date");
                    field(ui, "Recurrence multiplier", &mut d.form.frequency_multiplier, "empty: none applied");
                    field(ui, "Storage factor Cs", &mut d.form.storage_factor, "empty: the basic method");
                });
                ui.add_space(4.0);
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "MoWT Road Design Manual Vol. 2, Sections 4.4.2 and 4.4.3. AfroBIM holds no rainfall data: the intensity, the areas and the runoff coefficients are yours. Every field starts empty.");
                }
                match &d.result {
                    Some(Ok((q, lines))) => {
                        ui.separator();
                        crate::ui_kit::heading(ui, &format!("Design peak flow Q = {q:.3} m³/s"));
                        for l in lines.iter().cloned() {
                            let flagged = l.starts_with("Check:");
                            let text = RichText::new(l).small().monospace();
                            ui.label(if flagged { text.color(WARN()) } else { text });
                        }
                    }
                    Some(Err(e)) => {
                        ui.separator();
                        crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, format!("Not calculated: {e}"));
                    }
                    None => {}
                }
            });
        },
            |ui| {
                let labels: Vec<&str> = keep_label.iter().map(String::as_str).collect();
                crate::ui_kit::Footer::new("Calculate").secondary(&labels).cancel(Some("Close")).show(ui)
            },
        );
        if let Some((_, f)) = &dlg.inner {
            if f.primary {
                calculate = true;
            }
            if f.secondary == Some(0) {
                calculate = true;
                use_it = true;
            }
            if f.cancel {
                keep = false;
            }
        }
        if dlg.closed {
            keep = false;
        }
        if calculate {
            d.result = Some(d.form.run_flow());
            if let Some(Ok((q, _))) = &d.result {
                self.status = format!("Rational method ({}): Q = {q:.3} m³/s.", if d.form.use_sanral { "SANRAL" } else { "MoWT" });
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
