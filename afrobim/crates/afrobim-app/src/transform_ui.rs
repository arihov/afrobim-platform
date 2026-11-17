//! TRANSFORM and CALIBRATE, the transform log tab of the project settings, and the choice the
//! import dialogs offer when a file's coordinate system differs from the project's.
//!
//! Nothing here runs without the user: the plan chooser shows every candidate plan with its steps,
//! EPSG codes, stated accuracy and area-of-use warnings, and the button stays off until a plan is
//! picked and the stated accuracy is acknowledged. The work is `transform_doc`.

use afrobim_kernel::crs_plan::{plan_transform, ControlPair, DataExtent, HeightAssumption, PlanKind, SiteCalibration, StepKind, TransformPlan};
use afrobim_kernel::{project_crs, CrsKind, ElementId, ProjectCrs};
use egui::RichText;

use crate::app::AfroBimApp;
use crate::transform_doc as td;
use crate::ui_kit::{self, Dialog, Footer, Level};

// ─── a small system picker ─────────────────────────────────────────────

/// A choice of one coordinate system: a search over the systems the application can compute with,
/// WGS 84 UTM by zone, and the geographic systems.
#[derive(Clone, Default)]
pub struct CrsPick {
    query: String,
    zone: u32,
    south: usize,
    pub chosen: Option<ProjectCrs>,
}

impl CrsPick {
    pub fn new(chosen: Option<ProjectCrs>) -> CrsPick {
        CrsPick { query: String::new(), zone: 36, south: 1, chosen }
    }

    /// Draws the picker. Returns true when the choice changed.
    pub fn show(&mut self, ui: &mut egui::Ui, _id: &str, allow_geographic: bool) -> bool {
        let before = self.chosen.as_ref().map(|c| (c.epsg, c.kind));
        ui.label(RichText::new(self.chosen.as_ref().map(|c| c.label()).unwrap_or_else(|| "(none chosen)".into())).strong());
        ui.horizontal(|ui| {
            ui.label("Search");
            ui_kit::text_field(ui, &mut self.query, "EPSG code or name");
        });
        let mut found: Vec<ProjectCrs> = if self.query.trim().is_empty() { Vec::new() } else { ProjectCrs::search(&self.query, 8) };
        if allow_geographic && !self.query.trim().is_empty() {
            let q = self.query.trim().to_ascii_lowercase();
            found.extend(ProjectCrs::geographic_systems().into_iter().filter(|g| g.label().to_ascii_lowercase().contains(&q)));
        }
        for c in &found {
            if ui.selectable_label(self.chosen.as_ref().is_some_and(|x| x.epsg == c.epsg), c.label()).clicked() {
                self.chosen = Some(c.clone());
            }
        }
        ui.horizontal(|ui| {
            if self.zone == 0 {
                self.zone = 36;
            }
            ui.label("WGS 84 / UTM zone");
            ui.add(egui::DragValue::new(&mut self.zone).range(1..=60));
            let mut s = self.south;
            ui_kit::segmented(ui, &mut s, &["North", "South"]);
            self.south = s;
            if ui.button("Use").clicked() {
                self.chosen = ProjectCrs::utm_wgs84(self.zone as u8, self.south == 1);
            }
        });
        if allow_geographic {
            ui.horizontal_wrapped(|ui| {
                ui.label("Geographic");
                for g in ProjectCrs::geographic_systems() {
                    if ui.selectable_label(self.chosen.as_ref().is_some_and(|x| x.epsg == g.epsg), g.name.clone()).clicked() {
                        self.chosen = Some(g);
                    }
                }
            });
        }
        before != self.chosen.as_ref().map(|c| (c.epsg, c.kind))
    }
}

// ─── the plan chooser ──────────────────────────────────────────────────

/// Every candidate plan between two systems, which one the user picked, and the acknowledgement.
pub struct PlanChooser {
    key: Option<(Option<u32>, Option<u32>, [u64; 4])>,
    plans: Result<Vec<TransformPlan>, String>,
    pub selected: Option<usize>,
    pub ack: bool,
    pub zero_height: bool,
    /// The data's centre in the source system, for the bearing and scale change.
    centre: Option<(f64, f64)>,
}

impl Default for PlanChooser {
    fn default() -> Self {
        PlanChooser { key: None, plans: Ok(Vec::new()), selected: None, ack: false, zero_height: false, centre: None }
    }
}

impl PlanChooser {
    /// Recomputes the plans when the systems or the extent changed.
    pub fn set(&mut self, from: &ProjectCrs, to: &ProjectCrs, extent: Option<((f64, f64), (f64, f64))>) {
        let key = (from.epsg, to.epsg, extent.map(|(a, b)| [a.0.to_bits(), a.1.to_bits(), b.0.to_bits(), b.1.to_bits()]).unwrap_or([0; 4]));
        if self.key == Some(key) && !(from.epsg.is_none() && to.epsg.is_none()) {
            return;
        }
        self.key = Some(key);
        self.selected = None;
        self.ack = false;
        self.centre = extent.map(|(a, b)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0));
        self.plans = plan_transform(from, to, extent.map(|(a, b)| DataExtent::new(a, b))).map_err(|e| e.to_string());
        if let Ok(p) = &self.plans {
            if p.len() == 1 {
                self.selected = Some(0);
            }
        }
    }

    /// The plan the user picked and acknowledged, with the height assumption chosen.
    pub fn chosen(&self) -> Option<TransformPlan> {
        let i = self.selected?;
        if !self.ack {
            return None;
        }
        let p = self.plans.as_ref().ok()?.get(i)?.clone();
        Some(if self.zero_height { p.with_height_assumption(HeightAssumption::Zero) } else { p })
    }

    /// Why the button is off, or None when a plan is chosen and acknowledged.
    pub fn why_not(&self) -> Option<&'static str> {
        match &self.plans {
            Err(_) => Some("There is no path between these systems"),
            Ok(p) if p.is_empty() => Some("There is no plan"),
            Ok(_) if self.selected.is_none() => Some("Pick one of the plans"),
            Ok(_) if !self.ack => Some("Tick the acknowledgement of the stated accuracy"),
            _ => None,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, id: &str) {
        let t = crate::theme::tokens_ui(ui);
        let plans = match &self.plans {
            Err(why) => {
                ui_kit::callout(ui, Level::Warning, why.clone());
                return;
            }
            Ok(p) => p,
        };
        ui.label(RichText::new(format!("{} plan(s). Nothing is applied until you pick one, tick the acknowledgement and press the button.", plans.len())).size(crate::theme::FONT_SMALL).color(t.text_muted));
        let mut picked = self.selected;
        for (i, p) in plans.iter().enumerate() {
            let codes: Vec<String> = p.transformation_codes().iter().map(|c| format!("EPSG:{c}")).collect();
            let head = match p.kind {
                PlanKind::Identity => "Same system: nothing changes".to_string(),
                PlanKind::ProjectionOnly => "Same datum: projection change only (no datum accuracy figure applies)".to_string(),
                PlanKind::DatumTransformation => format!("Datum transformation {}, stated accuracy {} m", codes.join(" + "), p.accuracy_m.map(|a| format!("{a}")).unwrap_or_else(|| "not stated".into())),
            };
            ui.horizontal(|ui| {
                if ui.radio(picked == Some(i), RichText::new(format!("Plan {}: {head}", i + 1)).strong()).clicked() {
                    picked = Some(i);
                }
            });
            egui::CollapsingHeader::new(format!("Steps and warnings of plan {}", i + 1)).id_salt((id, "plan", i)).default_open(plans.len() == 1 || picked == Some(i)).show(ui, |ui| {
                for s in &p.steps {
                    let kind = match s.kind {
                        StepKind::InverseProjection => "Inverse projection",
                        StepKind::DatumTransformation => "Datum transformation",
                        StepKind::ForwardProjection => "Forward projection",
                    };
                    let mut line = format!("{kind}: {}{}", s.name, s.epsg.map(|c| format!(" (EPSG:{c})")).unwrap_or_default());
                    if s.reversed {
                        line.push_str(", applied in reverse");
                    }
                    ui.label(RichText::new(line).strong());
                    ui_kit::note(ui, format!("Method: {}. {}", s.method, s.parameters));
                    if !s.area_text.is_empty() {
                        ui_kit::note(ui, format!("Area of use: {}. Revision {}. {}", s.area_text, s.revision_date, s.source_url));
                    }
                    if let (StepKind::DatumTransformation, Some(a)) = (s.kind, s.accuracy_m) {
                        ui_kit::note(ui, format!("Stated accuracy of this step: {a} m"));
                    }
                }
                ui_kit::note(ui, p.accuracy_basis.clone());
                for w in &p.warnings {
                    ui_kit::callout(ui, Level::Warning, w.clone());
                }
                for n in &p.notes {
                    ui_kit::note(ui, n.clone());
                }
                if let Some((e, n)) = self.centre {
                    ui_kit::note(ui, format!("At the centre of the data: {}", td::effects_text(p, e, n, 0.0)));
                }
            });
        }
        self.selected = picked;
        ui.add_space(4.0);
        // The height treatment note: heights are never converted.
        let h = if self.zero_height { HeightAssumption::Zero } else { HeightAssumption::AsGiven };
        ui_kit::callout(ui, Level::Info, format!("Heights. {}", h.sentence()));
        ui.horizontal(|ui| {
            ui.label("Ellipsoidal height assumed in the datum step");
            let mut m = usize::from(self.zero_height);
            ui_kit::segmented(ui, &mut m, &["The height as given", "Zero"]);
            self.zero_height = m == 1;
        });
        if let Some(i) = self.selected {
            let a = plans.get(i).and_then(|p| p.accuracy_m);
            let text = match (plans.get(i).map(|p| p.kind), a) {
                (Some(PlanKind::DatumTransformation), Some(a)) => format!("I have read the stated accuracy of this plan ({a} m, the registry's figure, not survey accuracy) and its area-of-use warnings, and I accept that coordinates will change by up to hundreds of metres."),
                _ => "I have read the plan and accept that coordinates will change.".to_string(),
            };
            ui_kit::checkbox(ui, &mut self.ack, &text);
        }
    }
}

// ─── the TRANSFORM dialog ──────────────────────────────────────────────

pub struct TransformForm {
    /// 0 the whole document, 1 the selected objects.
    scope: usize,
    to: CrsPick,
    chooser: PlanChooser,
    message: String,
}

#[derive(Default)]
pub struct TransformUi {
    pub form: Option<TransformForm>,
    pub calib: Option<CalibForm>,
}

impl AfroBimApp {
    pub(crate) fn transform_cmd(&mut self) {
        let sel = !self.doc.selection().is_empty();
        self.civil_ui.transform.form = Some(TransformForm { scope: usize::from(sel), to: CrsPick::new(if self.options.civil_demo { ProjectCrs::from_epsg(21036) } else { None }), chooser: PlanChooser::default(), message: String::new() });
    }

    fn transform_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.transform.form.take() else { return };
        let selected: Option<Vec<ElementId>> = (f.scope == 1).then(|| self.doc.selection().to_vec());
        let rows = td::survey(&self.doc, selected.as_deref());
        let refused: Vec<String> = td::unsupported(&self.doc, selected.as_deref()).iter().map(|r| format!("{} ({})", r.category, r.count)).collect();
        let extent = td::extent_of(&self.doc, selected.as_deref());
        let project = project_crs(&self.doc);
        // The source is the project system: that is what the document's numbers mean.
        let mut go = false;
        if let (Some(from), Some(to)) = (project.as_ref(), f.to.chosen.as_ref()) {
            if to.kind != CrsKind::Geographic || true {
                f.chooser.set(from, to, extent);
            }
        }
        let n_sel = self.doc.selection().len();
        let reason = std::cell::Cell::new("");
        let out = Dialog::new("Transform coordinates", crate::glyphs::by_name("TRANSFORM")).width(760.0).resizable(true).max_body_height(640.0).help("TRANSFORM is the only command that changes coordinates from one system to another. It shows every way the registry offers, with the stated accuracy, and applies the one you pick to every object that carries coordinates. The step is logged in the document (Project settings, Transform log) and undo restores the coordinates exactly. Heights are never converted: there is no geoid model.").show(
            ctx,
            |ui| {
                ui_kit::section(ui, "What to transform", |ui| {
                    let mut s = f.scope;
                    ui_kit::radio_group(ui, &mut s, &["The whole document", &format!("The selected objects ({n_sel})")]);
                    if s == 1 && n_sel == 0 {
                        s = 0;
                    }
                    f.scope = s;
                    ui_kit::note(ui, "An incoming dataset (LandXML, points, surfaces, point clouds, pipes) is transformed at import: the import dialog offers the choice when the file's system differs from the project's.");
                    egui::CollapsingHeader::new(format!("What is in scope: {} element(s) in {} categories", rows.iter().map(|r| r.count).sum::<usize>(), rows.len())).id_salt("tr-scope").default_open(!refused.is_empty()).show(ui, |ui| {
                        for r in &rows {
                            ui.label(RichText::new(format!("{}  x {}: {}", r.category, r.count, r.how.label())).size(crate::theme::FONT_SMALL));
                        }
                    });
                    if !refused.is_empty() {
                        ui_kit::callout(ui, Level::Error, format!("These categories cannot be transformed: {}. The whole-document transform is refused so that the document is never left half in each system.{}", refused.join(", "), if f.scope == 1 { " Deselect them to go on." } else { " Remove them, or transform the rest as a selection." }));
                    }
                });
                ui_kit::section(ui, "From and to", |ui| match &project {
                    None => {
                        ui_kit::callout(ui, Level::Warning, "The project has no coordinate system, so there is nothing to transform from. Set it on the Coordinate system tab of the project settings (CRS).");
                    }
                    Some(p) => {
                        ui.label(RichText::new(format!("From the project system: {}", p.label())).strong());
                        ui.label("To:");
                        let changed = f.to.show(ui, "tr-to", false);
                        if changed {
                            f.message.clear();
                        }
                    }
                });
                if project.is_some() && f.to.chosen.is_some() {
                    ui_kit::section(ui, "Plans", |ui| f.chooser.show(ui, "tr-plans"));
                }
                if let Some(e) = extent {
                    let (e0, n0) = ((e.0 .0 + e.1 .0) / 2.0, (e.0 .1 + e.1 .1) / 2.0);
                    ui_kit::note(ui, format!("Extent of what will move (project system): E {:.1} to {:.1}, N {:.1} to {:.1}; centre E {:.1} N {:.1}.", e.0 .0, e.1 .0, e.0 .1, e.1 .1, e0, n0));
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Error, f.message.clone());
                }
                reason.set(if project.is_none() {
                    "No project coordinate system"
                } else if f.to.chosen.is_none() {
                    "Choose the system to transform to"
                } else if !refused.is_empty() {
                    "A category present cannot be transformed"
                } else if f.scope == 1 && n_sel == 0 {
                    "Nothing is selected"
                } else {
                    f.chooser.why_not().unwrap_or("")
                });
            },
            |ui| {
                let reason = reason.get();
                let o = Footer::new("Transform").icon(crate::glyphs::by_name("TRANSFORM")).enabled(reason.is_empty(), reason).show(ui);
                go = o.primary;
                o
            },
        );
        if out.closed || out.inner.as_ref().is_some_and(|fo| fo.1.cancel) {
            return;
        }
        if go {
            if let (Some(plan), Some(from), Some(to)) = (f.chooser.chosen(), project.as_ref(), f.to.chosen.clone()) {
                let what = if let Some(s) = &selected { format!("{} selected object(s) from {}", s.len(), from.name) } else { format!("Whole document from {}", from.name) };
                match td::apply_plan(&mut self.doc, selected.as_deref(), &plan, &to, &what, td::now_iso()) {
                    Ok(entry) => {
                        self.status = format!("{} {} element(s), {} position(s) moved. Logged in the project settings (Transform log). Undo restores the coordinates.", entry.summary(), entry.elements, entry.positions);
                        self.log(format!("TRANSFORM: {}", entry.summary()));
                        self.schedule_commit_now();
                        self.zoom_extents_visible();
                        return;
                    }
                    Err(why) => f.message = why,
                }
            }
        }
        self.civil_ui.transform.form = Some(f);
    }

    pub(crate) fn transform_windows(&mut self, ctx: &egui::Context) {
        self.transform_window(ctx);
        self.calibrate_window(ctx);
    }

    /// Opens the Transform log tab.
    pub(crate) fn open_transform_log(&mut self) {
        self.projset.open = true;
        self.projset.tab = crate::projset::LOG_TAB;
    }
}

// ─── CALIBRATE ─────────────────────────────────────────────────────────

pub struct CalibForm {
    name: String,
    target: CrsPick,
    /// Local x, local y, target easting, target northing, label (all typed).
    rows: Vec<[String; 5]>,
    /// Which row the next pick fills, and which half (0 local, 1 target).
    pick_row: Option<(usize, usize)>,
    fit: Option<Result<SiteCalibration, String>>,
    apply_doc: bool,
    message: String,
}

impl AfroBimApp {
    pub(crate) fn calibrate_cmd(&mut self) {
        let target = project_crs(&self.doc).filter(|c| c.kind == CrsKind::Projected);
        self.civil_ui.transform.calib = Some(CalibForm { name: String::new(), target: CrsPick::new(target), rows: vec![Default::default(); 3], pick_row: None, fit: None, apply_doc: false, message: String::new() });
    }

    fn calibrate_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.transform.calib.take() else { return };
        for (e, n) in std::mem::take(&mut self.civil_ui.queue) {
            if let Some((i, half)) = f.pick_row.take() {
                if let Some(r) = f.rows.get_mut(i) {
                    let (a, b) = if half == 0 { (0, 1) } else { (2, 3) };
                    r[a] = format!("{e:.4}");
                    r[b] = format!("{n:.4}");
                }
            }
        }
        let project = project_crs(&self.doc);
        let local_project = project.as_ref().is_some_and(|c| c.kind == CrsKind::LocalGrid);
        let existing = td::calibrations_of(&self.doc);
        let mut fit_now = false;
        let mut apply_now = false;
        let mut pick_req: Option<(usize, usize)> = None;
        let out = Dialog::new("Site calibration", crate::glyphs::by_name("CALIBRATE")).width(780.0).resizable(true).max_body_height(640.0).help("A site calibration relates a local grid to a projected system by a similarity transformation (scale, rotation, two shifts) fitted by least squares to control points known in both. Type the coordinates or pick local points in the model. The residuals show how well the points agree: they are the quality of the calibration. It is two-dimensional and valid among and near the control points; heights are not calibrated.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "cal-head", |fm| {
                    fm.row("Name", |ui| {
                        ui_kit::text_field(ui, &mut f.name, "for example Site grid to Arc 1960 UTM 36S");
                    });
                });
                ui_kit::section(ui, "Target system", |ui| {
                    f.target.show(ui, "cal-target", false);
                });
                ui_kit::section(ui, "Control point pairs", |ui| {
                    egui::Grid::new("cal-rows").num_columns(7).spacing([6.0, 4.0]).show(ui, |ui| {
                        for h in ["Label", "Local X (m)", "Local Y (m)", "Target E (m)", "Target N (m)", "", ""] {
                            ui.label(RichText::new(h).small().strong());
                        }
                        ui.end_row();
                        let mut remove = None;
                        for (i, r) in f.rows.iter_mut().enumerate() {
                            ui.add(egui::TextEdit::singleline(&mut r[4]).desired_width(70.0).hint_text(format!("{}", i + 1)));
                            ui.add(egui::TextEdit::singleline(&mut r[0]).desired_width(95.0));
                            ui.add(egui::TextEdit::singleline(&mut r[1]).desired_width(95.0));
                            ui.add(egui::TextEdit::singleline(&mut r[2]).desired_width(105.0));
                            ui.add(egui::TextEdit::singleline(&mut r[3]).desired_width(105.0));
                            if ui.small_button("Pick local").on_hover_text("Click a position in the model for the local X and Y").clicked() {
                                pick_req = Some((i, 0));
                            }
                            if ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove this pair").clicked() {
                                remove = Some(i);
                            }
                            ui.end_row();
                        }
                        if let Some(i) = remove {
                            f.rows.remove(i);
                        }
                    });
                    if ui.button("Add a pair").clicked() {
                        f.rows.push(Default::default());
                    }
                });
                if let Some(fit) = &f.fit {
                    match fit {
                        Err(why) => {
                            ui_kit::callout(ui, Level::Error, why.clone());
                        }
                        Ok(c) => {
                            ui_kit::section(ui, "Result", |ui| {
                                ui.label(RichText::new(c.sentence()).strong());
                                let rows: Vec<Vec<String>> = c.residuals.iter().map(|r| vec![r.label.clone(), format!("{:.4}", r.d_east), format!("{:.4}", r.d_north), format!("{:.4}", r.length)]).collect();
                                ui_kit::results_table(ui, "cal-res", &["Point", "dE (m)", "dN (m)", "Length (m)"], &[ui_kit::Col::Text, ui_kit::Col::Num, ui_kit::Col::Num, ui_kit::Col::Num], &rows);
                                for w in &c.warnings {
                                    ui_kit::callout(ui, Level::Warning, w.clone());
                                }
                                ui_kit::note(ui, c.method.clone());
                            });
                            ui_kit::toggle(ui, &mut f.apply_doc, "Also convert the whole document from the local grid to the target system (logged, undoable)");
                            if f.apply_doc && !local_project {
                                ui_kit::callout(ui, Level::Warning, "The project system is not a local site grid. Converting would treat the document's numbers as local grid coordinates.");
                            }
                        }
                    }
                }
                if !existing.is_empty() {
                    ui_kit::section(ui, "Calibrations stored in this document", |ui| {
                        for c in &existing {
                            ui.label(c.sentence());
                        }
                    });
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Error, f.message.clone());
                }
            },
            |ui| {
                let ok = f.fit.as_ref().is_some_and(|x| x.is_ok());
                let o = Footer::new("Store calibration").icon(crate::glyphs::by_name("CALIBRATE")).enabled(ok, "Fit the calibration first").secondary(&["Fit"]).show(ui);
                if o.secondary == Some(0) {
                    fit_now = true;
                }
                apply_now = o.primary;
                o
            },
        );
        if let Some(p) = pick_req {
            f.pick_row = Some(p);
            self.civil_ui.pick = Some(crate::civil_ui::PickFor::Queue);
            self.civil_ui.pick_repeat = false;
            self.status = "Click the local position in the model.".into();
        }
        if out.closed || out.inner.as_ref().is_some_and(|fo| fo.1.cancel) {
            return;
        }
        if fit_now {
            let parse = |s: &str| s.trim().replace(',', ".").parse::<f64>().ok();
            let mut pairs = Vec::new();
            let mut bad = None;
            for (i, r) in f.rows.iter().enumerate() {
                if r[..4].iter().all(|s| s.trim().is_empty()) {
                    continue;
                }
                match (parse(&r[0]), parse(&r[1]), parse(&r[2]), parse(&r[3])) {
                    (Some(a), Some(b), Some(c), Some(d)) => pairs.push(ControlPair { label: if r[4].trim().is_empty() { format!("{}", i + 1) } else { r[4].trim().to_string() }, local: (a, b), target: (c, d) }),
                    _ => bad = Some(format!("Row {} is not complete: all four coordinates are numbers.", i + 1)),
                }
            }
            f.fit = Some(match (bad, f.target.chosen.as_ref()) {
                (Some(b), _) => Err(b),
                (None, None) => Err("Choose the target system first.".into()),
                (None, Some(t)) => SiteCalibration::fit(if f.name.trim().is_empty() { "Site calibration" } else { f.name.trim() }, t, &pairs, Some(td::now_iso())).map_err(|e| e.to_string()),
            });
        }
        if apply_now {
            if let (Some(Ok(cal)), Some(target)) = (f.fit.clone(), f.target.chosen.clone()) {
                td::store_calibration(&mut self.doc, cal.clone());
                self.status = format!("Calibration '{}' stored in the document: {}", cal.name, cal.sentence());
                if f.apply_doc {
                    match td::apply_calibration(&mut self.doc, &cal, &target, td::now_iso()) {
                        Ok(e) => {
                            self.status = format!("{} {} element(s) moved to {}. Logged; undo restores them.", e.summary(), e.elements, target.name);
                        }
                        Err(why) => {
                            f.message = why;
                            self.civil_ui.transform.calib = Some(f);
                            return;
                        }
                    }
                }
                self.schedule_commit_now();
                return;
            }
        }
        self.civil_ui.transform.calib = Some(f);
    }
}

// ─── the Transform log tab ─────────────────────────────────────────────

pub fn log_tab(ui: &mut egui::Ui, log: &[td::LogEntry], cals: &[SiteCalibration]) {
    ui_kit::section(ui, "Transform log", |ui| {
        if log.is_empty() {
            ui_kit::callout(ui, Level::Info, "No transformation has been run on this document. Its objects are at the coordinates of their sources. TRANSFORM changes them only when you ask, and writes each step here.");
        }
        for (i, e) in log.iter().enumerate().rev() {
            egui::CollapsingHeader::new(RichText::new(format!("{}  {}", e.when.clone().unwrap_or_default(), e.what)).strong()).id_salt(("tl", i)).default_open(i + 1 == log.len()).show(ui, |ui| {
                ui.label(e.summary());
                ui_kit::note(ui, format!("{} element(s), {} position(s) moved.", e.elements, e.positions));
                if let Some(t) = &e.transform {
                    ui_kit::note(ui, format!("From {} to {}.", t.from.name, t.to.name));
                    for s in &t.steps {
                        ui_kit::note(ui, format!("{:?}: {}{}  {}", s.kind, s.name, s.epsg.map(|c| format!(" (EPSG:{c})")).unwrap_or_default(), s.parameters));
                    }
                    ui_kit::note(ui, t.accuracy_basis.clone());
                    ui_kit::note(ui, t.height_treatment.clone());
                    for w in &t.warnings {
                        ui_kit::callout(ui, Level::Warning, w.clone());
                    }
                }
                for n in &e.notes {
                    ui_kit::note(ui, n.clone());
                }
            });
        }
    });
    if !cals.is_empty() {
        ui_kit::section(ui, "Site calibrations", |ui| {
            for c in cals {
                ui.label(c.sentence());
            }
        });
    }
}

// ─── the offer at import ───────────────────────────────────────────────

/// What an import dialog offers when the file's system differs from the project's.
pub struct ImportCrs {
    /// 0 keep source coordinates (the default), 1 transform to the project system, 2 cancel the import.
    pub choice: usize,
    pub from: CrsPick,
    chooser: PlanChooser,
}

impl Default for ImportCrs {
    fn default() -> Self {
        ImportCrs { choice: 0, from: CrsPick::new(None), chooser: PlanChooser::default() }
    }
}

impl ImportCrs {
    pub fn new(file: Option<&ProjectCrs>) -> ImportCrs {
        ImportCrs { choice: 0, from: CrsPick::new(file.cloned()), chooser: PlanChooser::default() }
    }

    /// Whether the import button may be pressed.
    pub fn ready(&self) -> bool {
        match self.choice {
            1 => self.chooser.chosen().is_some(),
            2 => false,
            _ => true,
        }
    }

    pub fn why_not(&self) -> &'static str {
        match self.choice {
            1 => self.chooser.why_not().unwrap_or(""),
            2 => "You chose to cancel the import",
            _ => "",
        }
    }

    /// Starts the "file's system" choice from what the file's dialog already states.
    pub fn seed(&mut self, file: &ProjectCrs) {
        if self.from.chosen.is_none() {
            self.from.chosen = Some(file.clone());
        }
    }

    /// The choice stops the import button: cancel, or transform without a plan picked and acknowledged.
    pub fn cancelled_or_unready(&self) -> bool {
        self.choice == 2 || (self.choice == 1 && self.chooser.chosen().is_none())
    }

    pub fn wants_transform(&self) -> bool {
        self.choice == 1
    }

    pub fn plan(&self) -> Option<TransformPlan> {
        self.chooser.chosen()
    }

    /// Draws the offer. Returns nothing when the file has no system that differs (the caller decides
    /// whether to call this at all with [`differs`]).
    pub fn show(&mut self, ui: &mut egui::Ui, id: &str, project: &ProjectCrs, extent: Option<((f64, f64), (f64, f64))>) {
        ui_kit::section(ui, "The file's coordinate system differs from the project's", |ui| {
            ui_kit::note(ui, "The default keeps the coordinates exactly as the file has them. Transforming moves them to the project system and is written to the transform log.");
            let mut c = self.choice;
            ui_kit::radio_group(ui, &mut c, &["Keep source coordinates", &format!("Transform to the project system ({})", project.name), "Cancel the import"]);
            self.choice = c;
            if self.choice == 1 {
                ui.label("The file's system:");
                self.from.show(ui, id, true);
                if let Some(from) = &self.from.chosen {
                    self.chooser.set(from, project, extent);
                    self.chooser.show(ui, id);
                } else {
                    ui_kit::note(ui, "Choose the system the file's coordinates are in.");
                }
            }
        });
    }
}

/// Whether a file's declared system differs from the project's (both known).
pub fn differs(file: Option<&ProjectCrs>, project: Option<&ProjectCrs>) -> bool {
    match (file, project) {
        (Some(f), Some(p)) => f.epsg != p.epsg || (f.epsg.is_none() && f.name != p.name),
        _ => false,
    }
}

impl AfroBimApp {
    /// After an import added elements: when the user chose to transform, moves exactly the new
    /// elements with the plan and logs it. `before` is the ids present before the import.
    pub(crate) fn finish_import_transform(&mut self, choice: &ImportCrs, before: &[ElementId], what: &str) {
        if !choice.wants_transform() {
            return;
        }
        let (Some(plan), Some(from), Some(to)) = (choice.plan(), choice.from.chosen.clone(), project_crs(&self.doc)) else { return };
        let new: Vec<ElementId> = self.doc.elements().iter().map(|e| e.id).filter(|id| !before.contains(id)).collect();
        if new.is_empty() {
            return;
        }
        match td::apply_plan(&mut self.doc, Some(&new), &plan, &to, &format!("Import {what} from {}", from.name), td::now_iso()) {
            Ok(e) => {
                self.status.push_str(&format!(" Transformed to {}: {}", to.name, e.summary()));
            }
            Err(why) => self.status.push_str(&format!(" The imported data was left at its source coordinates: {why}")),
        }
    }
}

/// The extent of a set of plan points in metres, as the plan chooser takes it.
pub fn extent_of_points(points: impl Iterator<Item = (f64, f64)>) -> Option<((f64, f64), (f64, f64))> {
    let mut b: Option<((f64, f64), (f64, f64))> = None;
    for (e, n) in points.filter(|(e, n)| e.is_finite() && n.is_finite()) {
        b = Some(match b {
            None => ((e, n), (e, n)),
            Some((lo, hi)) => ((lo.0.min(e), lo.1.min(n)), (hi.0.max(e), hi.1.max(n))),
        });
    }
    b
}

