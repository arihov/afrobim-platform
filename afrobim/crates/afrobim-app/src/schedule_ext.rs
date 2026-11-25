// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The 4D tab's second round: the work breakdown structure and its roll-up, the critical path
//! basis, Primavera XER exchange, the time-location chart and 4D playback.
//!
//! The programme itself is still the document element of `schedule_glue`; the choices of method
//! made here (which basis makes an activity critical, how percent rolls up, which alignment the
//! time-location chart shows) are kept as properties of that element and are not part of the
//! programme. No duration, productivity or rate is assumed anywhere: dwell and direction are the
//! user's entries.

use std::collections::{HashMap, HashSet};

use afrobim_draw::{Colour, Primitive};
use afrobim_schedule::{
    outline, resolve_locations, time_location_chart, time_location_csv, wbs_rollup, wbs_rollup_csv, CriticalBasis, Date, GanttDrawing, GanttOptions, LocationRange, OutlineKind, Playback, Programme, RollupBasis,
    Schedule, ScheduleError, TimeLocationChart, TimeLocationOptions, TlLabelKind, WorkDirection,
};
use egui::{pos2, Align2, Color32, FontId, RichText, Sense, Shape, Stroke, Ui};

use crate::app::AfroBimApp;
use crate::schedule_glue as glue;

#[allow(non_snake_case)]

#[allow(non_snake_case)]
fn WARN() -> egui::Color32 {
    crate::theme::warn()
}
#[allow(non_snake_case)]
fn GOOD() -> egui::Color32 {
    crate::theme::success()
}

/// The entries for one activity's location range, kept while they are typed.
#[derive(Clone)]
pub struct TlForm {
    pub from: f64,
    pub to: f64,
    pub dir: Option<WorkDirection>,
    pub dwell: String,
}

/// The cached timelines of a playback, with what they were made for.
pub struct PlayCache {
    pub playback: Playback,
    pub key: (u64, usize, Date, Date),
}

pub struct PlayState {
    pub playing: bool,
    /// Calendar days of the programme per second of viewing: a viewing preference, not a value of the work.
    pub speed: f64,
    pub acc: f64,
    pub cache: Option<PlayCache>,
    /// The cache the held states were made from.
    pub states_key: Option<(u64, usize, Date, Date)>,
    /// How many times the states were rebuilt (a redraw on a change date).
    pub redraws: usize,
}

impl Default for PlayState {
    fn default() -> Self {
        PlayState { playing: false, speed: 5.0, acc: 0.0, cache: None, states_key: None, redraws: 0 }
    }
}

/// Writing the playback as PNG frames, headless (`--frames-dir`).
pub struct FrameExport {
    pub dir: std::path::PathBuf,
    pub dates: Vec<Date>,
    pub index: usize,
    pub wait: u32,
    pub asked: bool,
}

pub struct SchedExt {
    pub critical: CriticalBasis,
    pub rollup: RollupBasis,
    pub rollup_error: Option<String>,
    pub collapsed: HashSet<String>,
    pub settings_loaded: bool,
    pub wbs_open: bool,
    pub critical_open: bool,
    pub tl_open: bool,
    /// Title and lines of an XER import or export report.
    pub xer_report: Option<(String, Vec<String>)>,
    pub new_summary: (String, String, String, Option<String>),
    pub problem: String,
    /// Bumped every time the programme is scheduled, so drawings made from it are rebuilt.
    pub rev: u64,
    pub tl_alignment: Option<String>,
    pub tl_vertical: bool,
    pub tl_px_per_100m: f64,
    pub tl_px_per_day: f64,
    pub tl_tab: u8,
    pub tl_forms: HashMap<String, TlForm>,
    pub play: PlayState,
    pub frames: Option<FrameExport>,
}

impl Default for SchedExt {
    fn default() -> Self {
        SchedExt {
            critical: CriticalBasis::TotalFloat,
            rollup: RollupBasis::Duration,
            rollup_error: None,
            collapsed: HashSet::new(),
            settings_loaded: false,
            wbs_open: false,
            critical_open: false,
            tl_open: false,
            xer_report: None,
            new_summary: Default::default(),
            problem: String::new(),
            rev: 0,
            tl_alignment: None,
            tl_vertical: true,
            tl_px_per_100m: 85.0,
            tl_px_per_day: 6.0,
            tl_tab: 0,
            tl_forms: HashMap::new(),
            play: PlayState::default(),
            frames: None,
        }
    }
}

/// The Gantt chart of a programme: with summary rows and the roll-up when the programme has summary
/// tasks, plain otherwise. Collapsed summary tasks hide what is under them. The error is why the
/// roll-up could not be made (the plain chart is then drawn).
pub fn build_gantt(p: &Programme, s: &Schedule, basis: RollupBasis, collapsed: &HashSet<String>) -> (GanttDrawing, Option<String>) {
    let o = GanttOptions::default();
    if p.summaries.is_empty() {
        return (afrobim_schedule::gantt_drawing(p, s, &o), None);
    }
    match wbs_rollup(p, s, basis) {
        Ok(mut ru) => {
            let (hidden_acts, hidden_sums) = glue::hidden_by_collapse(p, collapsed);
            let mut s2 = s.clone();
            s2.rows.retain(|r| !hidden_acts.contains(&r.id));
            ru.rows.retain(|r| !hidden_sums.contains(&r.id));
            (afrobim_schedule::gantt_drawing_wbs(p, &s2, &o, &ru), None)
        }
        Err(e) => (afrobim_schedule::gantt_drawing(p, s, &o), Some(error_text(e))),
    }
}

pub fn error_text(e: ScheduleError) -> String {
    match e {
        ScheduleError::Invalid(issues) => issues.into_iter().map(|i| i.message).collect::<Vec<_>>().join("; "),
        ScheduleError::Other(m) => m,
    }
}

/// Paints a drawing of lines and fills with labels on a light sheet, one drawing unit being one
/// pixel times `scale` (the colours of the charts need a light ground).
pub fn paint_sheet(ui: &mut Ui, drawing: &afrobim_draw::Drawing, labels: &[(afrobim_draw::Pt2, String, f32, Color32)], scale: f32) {
    let ext = drawing.extents;
    let (w, h) = (ext.width() as f32 * scale, ext.height() as f32 * scale);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w + 24.0, h + 24.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(0xF6, 0xF5, 0xF0));
    let map = |x: f64, y: f64| pos2(rect.left() + 12.0 + (x - ext.min.x) as f32 * scale, rect.bottom() - 12.0 - (y - ext.min.y) as f32 * scale);
    let rgb = |c: Option<Colour>| {
        let [r, g, b] = c.map(|c| c.display_rgb()).unwrap_or([0x10, 0x10, 0x10]);
        Color32::from_rgb(r, g, b)
    };
    for prim in &drawing.primitives {
        match prim {
            Primitive::Fill { outline, colour } => {
                let pts: Vec<_> = outline.iter().map(|p| map(p.x, p.y)).collect();
                if pts.len() >= 3 {
                    painter.add(Shape::convex_polygon(pts, rgb(Some(*colour)), Stroke::NONE));
                }
            }
            Primitive::Line { from, to, width, colour } => {
                painter.line_segment([map(from.x, from.y), map(to.x, to.y)], Stroke::new((*width as f32 * scale).max(1.0), rgb(*colour)));
            }
            _ => {}
        }
    }
    for (at, text, size, colour) in labels {
        painter.text(map(at.x, at.y), Align2::LEFT_CENTER, text, FontId::proportional(*size), *colour);
    }
}

fn tl_label_style(kind: TlLabelKind) -> (f32, Color32) {
    match kind {
        TlLabelKind::Title => (13.0, Color32::from_rgb(0x22, 0x22, 0x22)),
        TlLabelKind::Station => (10.0, Color32::from_rgb(0x44, 0x44, 0x44)),
        TlLabelKind::Month => (11.0, Color32::from_rgb(0x22, 0x22, 0x22)),
        TlLabelKind::Week => (9.0, Color32::from_rgb(0x55, 0x55, 0x55)),
        TlLabelKind::Activity => (10.0, Color32::from_rgb(0x22, 0x22, 0x22)),
        TlLabelKind::DataDate => (11.0, Color32::from_rgb(0x1F, 0x7A, 0x3A)),
        TlLabelKind::RequiredFinish => (11.0, Color32::from_rgb(0x6B, 0x3A, 0x1E)),
    }
}

fn direction_text(d: WorkDirection) -> &'static str {
    match d {
        WorkDirection::Increasing => "towards increasing station",
        WorkDirection::Decreasing => "towards decreasing station",
        WorkDirection::Fixed => "whole range together",
    }
}

impl AfroBimApp {
    // ─── Primavera XER ───────────────────────────────────────────────────────────────────

    pub(crate) fn import_xer_file(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Primavera XER", &["xer"]).pick_file() else { return };
        self.import_xer_path(&path);
    }

    pub(crate) fn import_xer_path(&mut self, path: &std::path::Path) {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                self.status = format!("Could not read {}: {e}", path.display());
                return;
            }
        };
        self.sched.sync(&self.doc);
        let old = self.sched.prog.clone();
        match glue::import_xer_file(&bytes, old.as_ref()) {
            Ok((p, warnings)) => {
                let (n, w) = (p.activities.len(), p.summaries.len());
                let mut lines = vec![glue::XER_UNTESTED.to_string(), format!("Imported {n} activities and {w} summary task(s) from {}.", path.display())];
                if warnings.is_empty() {
                    lines.push("Nothing was left out.".into());
                } else {
                    lines.push(format!("{} item(s) the file could not carry or that were skipped:", warnings.len()));
                    lines.extend(warnings.iter().map(|x| format!("  {x}")));
                }
                self.sched.date = None;
                self.sched.ext.collapsed.clear();
                match self.sched.replace_programme(&mut self.doc, p) {
                    Ok(()) => self.status = format!("Imported {n} activities from {} ({} item(s) listed). Not tested in P6.", path.display(), warnings.len()),
                    Err(e) => self.status = format!("Imported but not stored: {e}"),
                }
                self.sched.ext.xer_report = Some(("Primavera XER import report".into(), lines));
                self.schedule_commit_now();
            }
            Err(e) => self.status = format!("Could not import {}: {e}", path.display()),
        }
    }

    pub(crate) fn export_xer_file(&mut self) {
        self.sched.sync(&self.doc);
        let Some(p) = self.sched.prog.clone() else {
            self.status = "There is no programme to export.".into();
            return;
        };
        let Some(mut path) = rfd::FileDialog::new().add_filter("Primavera XER", &["xer"]).set_file_name("programme.xer").save_file() else { return };
        if path.extension().is_none() {
            path.set_extension("xer");
        }
        self.export_xer_path(&p, &path);
    }

    pub(crate) fn export_xer_path(&mut self, p: &Programme, path: &std::path::Path) {
        let s = self.sched.result.as_ref().and_then(|r| r.as_ref().ok());
        let (text, warnings) = glue::export_xer_text(p, s);
        match std::fs::write(path, text) {
            Ok(()) => {
                let mut lines = vec![glue::XER_UNTESTED.to_string(), format!("Exported {} activities and {} summary task(s) to {}.", p.activities.len(), p.summaries.len(), path.display())];
                if warnings.is_empty() {
                    lines.push("The file carries everything the programme holds.".into());
                } else {
                    lines.push(format!("{} thing(s) the file does not carry:", warnings.len()));
                    lines.extend(warnings.iter().map(|x| format!("  {x}")));
                }
                self.status = format!("Exported {} activities to {} ({} item(s) listed). Not tested in P6.", p.activities.len(), path.display(), warnings.len());
                self.sched.ext.xer_report = Some(("Primavera XER export report".into(), lines));
            }
            Err(e) => self.status = format!("Could not write {}: {e}", path.display()),
        }
    }

    fn xer_report_window(&mut self, ctx: &egui::Context) {
        let Some((title, lines)) = self.sched.ext.xer_report.clone() else { return };
        let mut open = true;
        let dlg = crate::ui_kit::Dialog::titled(title).width(640.0).show_body(ctx, |ui| {
            crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, glue::XER_UNTESTED);
            crate::ui_kit::heading(ui, "What the file holds");
            egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                for l in lines.iter().skip(1) {
                    ui.label(l);
                }
            });
        });
        if dlg.closed {
            open = false;
        }
        if !open {
            self.sched.ext.xer_report = None;
        }
    }

    // ─── windows of this file ────────────────────────────────────────────────────────────

    /// Opens the windows of this file that are asked for. True when the programme changed.
    pub(crate) fn schedule_ext_windows(&mut self, ctx: &egui::Context) -> bool {
        let mut changed = false;
        if self.sched.ext.wbs_open {
            changed |= self.wbs_window(ctx);
        }
        if self.sched.ext.critical_open {
            self.critical_window(ctx);
        }
        if self.sched.ext.tl_open {
            changed |= self.tl_window(ctx);
        }
        self.xer_report_window(ctx);
        changed
    }

    // ─── critical path basis ─────────────────────────────────────────────────────────────

    fn critical_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut chosen: Option<CriticalBasis> = None;
        let current = self.sched.ext.critical;
        let counts = match &self.sched.result {
            Some(Ok(s)) => Some((s.critical().count(), s.rows.iter().filter(|r| r.on_longest_path).count(), s.rows.len())),
            _ => None,
        };
        let dlg = crate::ui_kit::Dialog::titled("Critical path basis").subtitle("How the critical path is found").width(640.0).show_body(ctx, |ui| {
            crate::ui_kit::heading(ui, "What marks an activity as critical");
            let mut ix = if current == CriticalBasis::TotalFloat { 0 } else { 1 };
            if crate::ui_kit::segmented(ui, &mut ix, &["Total float of zero or less", "Longest path"]) {
                chosen = Some(if ix == 0 { CriticalBasis::TotalFloat } else { CriticalBasis::LongestPath });
            }
            crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, if current == CriticalBasis::TotalFloat { glue::CRITICAL_TOTAL_FLOAT_TEXT } else { glue::CRITICAL_LONGEST_PATH_TEXT });
            crate::ui_kit::note(ui, "The Gantt chart, the schedule table and the time-location chart follow the choice.");
            crate::ui_kit::heading(ui, "Now");
            match counts {
                Some((critical, path, n)) => {
                    crate::ui_kit::results_table(ui, "crit-counts", &["Count", "Activities"], &[crate::ui_kit::Col::Text, crate::ui_kit::Col::Num], &[vec!["Critical on the chosen basis".into(), critical.to_string()], vec!["On the longest path, on either basis".into(), path.to_string()], vec!["In the programme".into(), n.to_string()]]);
                }
                None => {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "There is no schedule yet (PROGRAMME, then SCHEDULE).");
                }
            }
        });
        if dlg.closed {
            open = false;
        }
        if let Some(b) = chosen {
            self.sched.ext.critical = b;
            glue::set_setting(&mut self.doc, glue::KEY_CRITICAL, glue::critical_key(b));
            self.sched.recompute(&self.doc);
            self.schedule_commit_now();
            self.status = format!("Critical path basis: {}.", if b == CriticalBasis::TotalFloat { "total float" } else { "longest path" });
        }
        self.sched.ext.critical_open = open;
    }

    // ─── WBS ─────────────────────────────────────────────────────────────────────────────

    fn wbs_window(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        let mut changed = false;
        let mut basis_changed = false;
        let mut csv: Option<String> = None;
        let mut csv_text: Option<String> = None;
        let s = &mut self.sched;
        let ext = &mut s.ext;
        let result = &s.result;
        let dlg = crate::ui_kit::Dialog::titled("WBS: summary tasks and roll-up").subtitle("The work breakdown structure and its summary tasks").width(900.0).show_close(ctx, |ui| {
            let Some(p) = s.prog.as_mut() else {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "There is no programme yet (PROGRAMME).");
                return;
            };
            crate::ui_kit::heading(ui, "Summary tasks");
            crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "A summary task has no duration of its own: it spans the activities under it. Put an activity under a summary task in the Under column of PROGRAMME. Indent puts a summary task under the one above it; outdent moves it up a level.");
            let ids: Vec<String> = p.summaries.iter().map(|x| x.id.clone()).collect();
            let mut act: Option<(usize, u8)> = None;
            egui::ScrollArea::vertical().id_salt("wbs-sums").max_height(190.0).auto_shrink([false, true]).show(ui, |ui| {
                egui::Grid::new("wbs-grid").num_columns(6).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                    for h in ["WBS", "Name", "Id", "Under", "", ""] {
                        crate::ui_kit::th(ui, &h);
                    }
                    ui.end_row();
                    for i in 0..p.summaries.len() {
                        let below = glue::descendants_of(p, &p.summaries[i].id);
                        let me = p.summaries[i].id.clone();
                        let sm = &mut p.summaries[i];
                        changed |= ui.add_sized([70.0, 20.0], egui::TextEdit::singleline(&mut sm.wbs)).changed();
                        changed |= ui.add_sized([240.0, 20.0], egui::TextEdit::singleline(&mut sm.name)).changed();
                        ui.label(&sm.id);
                        let shown = sm.parent.clone().unwrap_or_else(|| "top level".into());
                        egui::ComboBox::from_id_salt(("wbs-parent", i)).width(130.0).selected_text(shown).show_ui(ui, |ui| {
                            changed |= ui.selectable_value(&mut sm.parent, None, "top level").changed();
                            for q in ids.iter().filter(|q| **q != me && !below.contains(*q)) {
                                changed |= ui.selectable_value(&mut sm.parent, Some(q.clone()), q).changed();
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.small_button("Indent").clicked() {
                                act = Some((i, 0));
                            }
                            if ui.small_button("Outdent").clicked() {
                                act = Some((i, 1));
                            }
                        });
                        if ui.small_button("Delete").clicked() {
                            act = Some((i, 2));
                        }
                        ui.end_row();
                    }
                });
            });
            if let Some((i, what)) = act {
                let id = p.summaries[i].id.clone();
                let r = match what {
                    0 => glue::indent_summary(p, &id),
                    1 => glue::outdent_summary(p, &id),
                    _ => {
                        glue::remove_summary(p, &id);
                        Ok(())
                    }
                };
                match r {
                    Ok(()) => {
                        ext.problem.clear();
                        changed = true;
                    }
                    Err(e) => ext.problem = e,
                }
            }
            crate::ui_kit::heading(ui, "Add a summary task");
            ui.horizontal(|ui| {
                ui.add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut ext.new_summary.0).hint_text("id"));
                ui.add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut ext.new_summary.1).hint_text("WBS"));
                ui.add_sized([220.0, 20.0], egui::TextEdit::singleline(&mut ext.new_summary.2).hint_text("name"));
                let shown = ext.new_summary.3.clone().unwrap_or_else(|| "top level".into());
                egui::ComboBox::from_id_salt("wbs-new-parent").width(120.0).selected_text(shown).show_ui(ui, |ui| {
                    ui.selectable_value(&mut ext.new_summary.3, None, "top level");
                    for q in &ids {
                        ui.selectable_value(&mut ext.new_summary.3, Some(q.clone()), q);
                    }
                });
                if ui.button("Add summary task").clicked() {
                    let n = ext.new_summary.clone();
                    match glue::add_summary(p, &n.0, &n.1, &n.2, n.3.as_deref()) {
                        Ok(()) => {
                            ext.new_summary = Default::default();
                            ext.problem.clear();
                            changed = true;
                        }
                        Err(e) => ext.problem = e,
                    }
                }
            });
            if !ext.problem.is_empty() {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, &ext.problem);
            }
            crate::ui_kit::heading(ui, "Roll-up");
            crate::ui_kit::form(ui, "wbs-basis", |f| {
                f.row("Percent complete weighs each activity by its", |ui| {
                    let mut ix = if ext.rollup == RollupBasis::Duration { 0 } else { 1 };
                    if crate::ui_kit::segmented(ui, &mut ix, &["Duration", "Weight"]) {
                        ext.rollup = if ix == 0 { RollupBasis::Duration } else { RollupBasis::Weight };
                        basis_changed = true;
                    }
                });
            });
            crate::ui_kit::note(ui, if ext.rollup == RollupBasis::Duration { glue::ROLLUP_DURATION_TEXT } else { glue::ROLLUP_WEIGHT_TEXT });
            match result {
                Some(Ok(sch)) => match wbs_rollup(p, sch, ext.rollup) {
                    Ok(ru) => {
                        ext.rollup_error = None;
                        let order: Vec<_> = outline(p).into_iter().filter_map(|o| if let OutlineKind::Summary(k) = o.kind { Some(p.summaries[k].id.clone()) } else { None }).collect();
                        egui::ScrollArea::both().id_salt("wbs-roll").max_height(230.0).auto_shrink([false, true]).show(ui, |ui| {
                            egui::Grid::new("wbs-roll-grid").num_columns(8).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
                                for (k, h) in ["WBS", "Name", "Start", "Finish", "Complete (%)", "Activities", "Critical", "Least float (d)"].iter().enumerate() {
                                    if k >= 4 { crate::ui_kit::th_num(ui, h) } else { crate::ui_kit::th(ui, h) }
                                }
                                ui.end_row();
                                for id in &order {
                                    let Some(r) = ru.row(id) else { continue };
                                    let indent = "    ".repeat(r.level.saturating_sub(1));
                                    ui.label(RichText::new(format!("{indent}{}", r.wbs)).strong());
                                    ui.label(RichText::new(&r.name).strong());
                                    ui.label(r.start.map(|d| d.to_string()).unwrap_or_default());
                                    ui.label(r.finish.map(|d| d.to_string()).unwrap_or_default());
                                    crate::ui_kit::cell_num(ui, format!("{:.1}", r.percent_complete));
                                    crate::ui_kit::cell_num(ui, r.activities.to_string());
                                    crate::ui_kit::cell_num(ui, r.critical.to_string());
                                    crate::ui_kit::cell_num(ui, r.total_float.map(|f| f.to_string()).unwrap_or_default());
                                    ui.end_row();
                                }
                            });
                        });
                        csv_text = Some(wbs_rollup_csv(&ru));
                    }
                    Err(e) => {
                        ext.rollup_error = Some(error_text(e));
                        crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, format!("The roll-up cannot be made on this basis: {}", ext.rollup_error.clone().unwrap_or_default()));
                    }
                },
                Some(Err(m)) => {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, format!("The programme cannot be scheduled: {}", m.join("; ")));
                }
                None => {}
            }
        }, &["Save the roll-up as CSV..."]);
        if dlg.closed {
            open = false;
        }
        if matches!(dlg.inner, Some((_, Some(0)))) {
            csv = csv_text.take();
        }
        if basis_changed {
            glue::set_setting(&mut self.doc, glue::KEY_ROLLUP, glue::rollup_key(self.sched.ext.rollup));
            self.sched.rebuild_gantt();
            self.schedule_commit_now();
        }
        if let Some(text) = csv {
            if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("wbs-rollup.csv").save_file() {
                if path.extension().is_none() {
                    path.set_extension("csv");
                }
                self.status = match std::fs::write(&path, text) {
                    Ok(()) => format!("Saved the roll-up to {}.", path.display()),
                    Err(e) => format!("Could not write {}: {e}", path.display()),
                };
            }
        }
        self.sched.ext.wbs_open = open;
        changed
    }

    // ─── time-location chart ─────────────────────────────────────────────────────────────

    fn tl_options(&self, alignment: Option<String>) -> TimeLocationOptions {
        let e = &self.sched.ext;
        TimeLocationOptions {
            alignment,
            metre_scale: e.tl_px_per_100m as f64 / 100.0,
            day_scale: e.tl_px_per_day as f64,
            time_vertical: e.tl_vertical,
            margin: 44.0,
            station_tick_m: None,
            min_bar_width: 4.0,
            line_width: 1.6,
            baseline: true,
        }
    }

    fn tl_window(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        let mut changed = false;
        let Some(p) = self.sched.prog.clone() else {
            self.sched.ext.tl_open = false;
            self.status = "There is no programme yet (PROGRAMME).".into();
            return false;
        };
        let alignments = crate::alignment_glue::alignments(&self.doc);
        let resolved = resolve_locations(&p, Some(&self.doc));
        let chosen = self.sched.ext.tl_alignment.clone().filter(|n| alignments.iter().any(|a| a.1 == *n)).or_else(|| resolved.alignments().first().cloned()).or_else(|| alignments.first().map(|a| a.1.clone()));
        let options = self.tl_options(chosen.clone());
        let chart = match &self.sched.result {
            Some(Ok(s)) => time_location_chart(&p, s, &resolved, &options).map(|mut c| {
                relabel_stations(&self.doc, &mut c);
                c
            }),
            _ => None,
        };
        let csv = match &self.sched.result {
            Some(Ok(s)) => Some(time_location_csv(&p, s, &resolved, &options)),
            _ => None,
        };
        let mut new_alignment: Option<String> = None;
        let mut save_csv = false;
        let mut set_range: Option<(String, Option<LocationRange>)> = None;
        let schedule_ok = matches!(self.sched.result, Some(Ok(_)));
        let doc = &self.doc;
        let ext = &mut self.sched.ext;
        let alignment_id = chosen.as_ref().and_then(|n| alignments.iter().find(|a| a.1 == *n)).map(|a| a.0);
        let dlg = crate::ui_kit::Dialog::titled("Time-location chart").subtitle("Activities against station and time").width(980.0).show_close(ctx, |ui| {
            crate::ui_kit::tabs_u8(ui, &mut ext.tl_tab, &["Chart", "Location ranges"]);
            crate::ui_kit::form(ui, "tl-alignment-row", |f| {
                f.row("Alignment", |ui| {
                    egui::ComboBox::from_id_salt("tl-alignment").selected_text(chosen.clone().unwrap_or_else(|| "none".into())).show_ui(ui, |ui| {
                        for (_, n) in &alignments {
                            if ui.selectable_label(chosen.as_deref() == Some(n.as_str()), n).clicked() {
                                new_alignment = Some(n.clone());
                            }
                        }
                    });
                });
            });
            if !schedule_ok {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, "The programme is not scheduled. Fix it in PROGRAMME first.");
                return;
            }
            if ext.tl_tab == 0 {
                crate::ui_kit::form(ui, "tl-view", |f| {
                    f.row("Time runs up the page", |ui| crate::ui_kit::toggle(ui, &mut ext.tl_vertical, "Time up the page, stations across"));
                    f.row("Zoom, station", |ui| ui.add(egui::Slider::new(&mut ext.tl_px_per_100m, 10.0..=300.0).show_value(false)));
                    f.row("Zoom, time", |ui| ui.add(egui::Slider::new(&mut ext.tl_px_per_day, 1.0..=20.0).show_value(false)));
                });
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "Station is across the page and time up it. A diagonal is work moving along the alignment; a parallelogram shows the dwell you entered; a bar is work on the whole range at once (a culvert, a structure). Critical red, complete grey, others blue; thin grey outline is the baseline; green line the data date; brown line the required finish. Stations are internal stations (a designed or imported alignment's own, before any station equation).");
                match &chart {
                    Some(c) => {
                        let labels: Vec<(afrobim_draw::Pt2, String, f32, Color32)> = c
                            .labels
                            .iter()
                            .map(|l| {
                                let (size, colour) = tl_label_style(l.kind);
                                (l.at, l.text.clone(), size, colour)
                            })
                            .collect();
                        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| paint_sheet(ui, &c.drawing, &labels, 1.0));
                    }
                    None => {
                        crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, "No activity has a location range on this alignment. Enter one on the Location ranges tab, or link activities to elements placed on the alignment (LINK4D, STATION).");
                    }
                }
                for n in &resolved.notes {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, n);
                }
            } else {
                tl_ranges_table(ui, &p, &resolved, doc, alignment_id, chosen.as_deref(), ext, &mut set_range);
            }
        }, &["Save as CSV..."]);
        if dlg.closed {
            open = false;
        }
        if matches!(dlg.inner, Some((_, Some(0)))) && csv.is_some() {
            save_csv = true;
        }
        if let Some(n) = new_alignment {
            self.sched.ext.tl_alignment = Some(n.clone());
            glue::set_setting(&mut self.doc, glue::KEY_TL_ALIGNMENT, &n);
            self.schedule_commit_now();
        }
        if save_csv {
            if let (Some(text), Some(mut path)) = (csv, rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("time-location.csv").save_file()) {
                if path.extension().is_none() {
                    path.set_extension("csv");
                }
                self.status = match std::fs::write(&path, text) {
                    Ok(()) => format!("Saved the plotted data to {}.", path.display()),
                    Err(e) => format!("Could not write {}: {e}", path.display()),
                };
            }
        }
        if let Some((id, range)) = set_range {
            if let Some(a) = self.sched.prog.as_mut().and_then(|p| p.activities.iter_mut().find(|a| a.id == id)) {
                a.location = range;
                changed = true;
            }
        }
        self.sched.ext.tl_open = open;
        changed
    }

    // ─── playback ────────────────────────────────────────────────────────────────────────

    /// The first and last date of playback: the programme's span.
    pub(crate) fn play_span(&self) -> Option<(Date, Date)> {
        match &self.sched.result {
            Some(Ok(s)) => Some((s.project_start, s.project_finish)),
            _ => None,
        }
    }

    /// Makes sure the timelines of every linked element are worked out for the span, once.
    pub(crate) fn play_prepare(&mut self) {
        let (Some((from, to)), Some(p), Some(Ok(s))) = (self.play_span(), self.sched.prog.clone(), self.sched.result.clone()) else { return };
        let key = (self.sched.ext.rev, self.doc.elements().len(), from, to);
        if self.sched.ext.play.cache.as_ref().is_some_and(|c| c.key == key) {
            return;
        }
        let playback = afrobim_schedule::element_timelines(&p, &s, &self.doc, from, to);
        self.sched.ext.play.cache = Some(PlayCache { playback, key });
    }

    /// Puts the element states of a date in place from the playback, working out nothing else. The
    /// states are rebuilt only when something changes between the date held and this one.
    pub(crate) fn play_to(&mut self, date: Date) {
        self.play_prepare();
        let Some(c) = &self.sched.ext.play.cache else { return };
        let held = self.sched.states.as_ref().filter(|_| self.sched.ext.play.states_key == Some(c.key));
        match glue::states_update(held, &c.playback, date) {
            Some(states) => {
                self.sched.states = Some(states);
                self.sched.ext.play.states_key = Some(c.key);
                self.sched.ext.play.redraws += 1;
            }
            None => {
                if let Some(s) = self.sched.states.as_mut() {
                    s.date = date;
                }
            }
        }
        self.sched.date = Some(date);
    }

    pub(crate) fn play_toggle(&mut self, ctx: &egui::Context) {
        self.sched.sync(&self.doc);
        let Some((from, to)) = self.play_span() else {
            self.status = "There is no schedule to play. Create the programme and fix any errors first (PROGRAMME).".into();
            self.sched.table_open = true;
            return;
        };
        let play = &mut self.sched.ext.play;
        if play.playing {
            play.playing = false;
            self.status = "Playback paused.".into();
            return;
        }
        // At the end, play starts again from the beginning.
        let now = self.sched.date.unwrap_or(from);
        let start = if now >= to || now < from { from } else { now };
        self.sched.ext.play.playing = true;
        self.sched.ext.play.acc = 0.0;
        self.sched.fourd_on = true;
        self.sched.date_open = true;
        self.view_3d = true;
        self.design.plan_on = false;
        self.play_to(start);
        ctx.request_repaint();
        self.status = "Playing the programme in the 3D view.".into();
    }

    /// Advances playback by the time the last frame took, and writes frames when asked to.
    pub(crate) fn sched_tick(&mut self, ctx: &egui::Context) {
        if self.sched.ext.play.playing {
            let (Some((from, to)), Some(date)) = (self.play_span(), self.sched.date) else {
                self.sched.ext.play.playing = false;
                return;
            };
            let dt = ctx.input(|i| i.stable_dt) as f64;
            let play = &mut self.sched.ext.play;
            play.acc += dt * play.speed;
            let whole = play.acc.floor() as i64;
            if whole >= 1 {
                play.acc -= whole as f64;
                let next = date.add_days(whole).min(to);
                self.play_to(next);
                if next >= to {
                    self.sched.ext.play.playing = false;
                    self.status = format!("Playback reached the finish, {to}.");
                }
            }
            let _ = from;
            ctx.request_repaint();
        }
        self.frames_tick(ctx);
    }

    /// Headless: one PNG per date into a folder, for making a video outside AfroBIM.
    fn frames_tick(&mut self, ctx: &egui::Context) {
        let Some(mut fx) = self.sched.ext.frames.take() else { return };
        if fx.index >= fx.dates.len() {
            self.request_quit();
            return;
        }
        let date = fx.dates[fx.index];
        if !fx.asked {
            self.play_to(date);
            fx.wait += 1;
            // A few frames for the 3D view to draw the new date before it is photographed.
            if fx.wait >= 4 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                fx.asked = true;
            }
        } else {
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(full) = shot {
                // Only the view, not the menus and panels: a frame for a video.
                let img = full.region(&crate::win::work_rect(ctx), Some(ctx.pixels_per_point()));
                let path = fx.dir.join(format!("frame_{:04}_{}.png", fx.index + 1, date));
                if let Err(e) = image::save_buffer(&path, img.as_raw(), img.width() as u32, img.height() as u32, image::ColorType::Rgba8) {
                    eprintln!("AfroBIM: could not write {}: {e}", path.display());
                }
                fx.index += 1;
                fx.asked = false;
                fx.wait = 0;
            }
        }
        if fx.index >= fx.dates.len() {
            eprintln!("AfroBIM: wrote {} frame(s) to {}", fx.dates.len(), fx.dir.display());
            self.request_quit();
        } else {
            self.sched.ext.frames = Some(fx);
        }
        ctx.request_repaint();
    }

    /// Prepares `--frames-dir`: the dates, one every `--frames-step` days from the programme's start (or `--frames-from`) to its finish (or `--frames-to`).
    pub(crate) fn frames_setup(&mut self) {
        let Some(dir) = self.options.frames_dir.clone() else { return };
        self.sched.sync(&self.doc);
        let Some((a, b)) = self.play_span() else {
            eprintln!("AfroBIM: --frames-dir needs a scheduled programme (use --demo).");
            self.request_quit();
            return;
        };
        let parse = |t: &Option<String>, d: Date| t.as_deref().and_then(Date::parse).unwrap_or(d);
        let (from, to) = (parse(&self.options.frames_from, a), parse(&self.options.frames_to, b));
        let step = self.options.frames_step.filter(|s| *s > 0).unwrap_or(7);
        let mut dates = Vec::new();
        let mut d = from;
        while d <= to {
            dates.push(d);
            d = d.add_days(step);
        }
        if dates.last() != Some(&to) && from <= to {
            dates.push(to);
        }
        let _ = std::fs::create_dir_all(&dir);
        self.sched.fourd_on = true;
        self.view_3d = true;
        self.sched.ext.frames = Some(FrameExport { dir, dates, index: 0, wait: 0, asked: false });
    }
}

/// The location ranges table: one row per activity, with the stations as the drawings show them.
#[allow(clippy::too_many_arguments)]
fn tl_ranges_table(
    ui: &mut Ui,
    p: &Programme,
    resolved: &afrobim_schedule::ResolvedLocations,
    doc: &afrobim_kernel::Document,
    alignment_id: Option<afrobim_kernel::ElementId>,
    alignment_name: Option<&str>,
    ext: &mut SchedExt,
    set_range: &mut Option<(String, Option<LocationRange>)>,
) {
    let Some(name) = alignment_name else {
        ui.label("There is no alignment in the document. Import one with LANDXML or design one with ALIGNDESIGN.");
        return;
    };
    let alignment = alignment_id.and_then(|id| crate::alignment_glue::alignment_of(doc, id));
    crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "Enter where along the alignment each activity works, in the stations shown on the drawings (the region picker appears where the alignment has station equations; AfroBIM converts to internal stations). Direction and dwell are your entries: nothing is assumed. A range derived from linked elements is shown for adopting; it is drawn as a bar because the elements' stations do not say in which order they are built.");
    ui.label(RichText::new(crate::station_input::note(doc, alignment_id)).small().weak());
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("tl-ranges").num_columns(8).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
            for h in ["Id", "Name", "From", "To", "Direction", "Dwell (days)", "Source", ""] {
                crate::ui_kit::th(ui, &h);
            }
            ui.end_row();
            for a in &p.activities {
                ui.label(&a.id);
                ui.label(&a.name);
                let current = resolved.get(&a.id).filter(|l| l.range.alignment == name);
                let entered = a.location.as_ref().filter(|r| r.alignment == name);
                let mut form = ext.tl_forms.get(&a.id).cloned().unwrap_or_else(|| match current {
                    Some(l) => TlForm { from: l.range.from_m, to: l.range.to_m, dir: if l.derived { None } else { Some(l.range.direction) }, dwell: l.range.dwell_days.map(|d| d.to_string()).unwrap_or_default() },
                    None => TlForm { from: alignment.as_ref().map(|x| x.start_station_m).unwrap_or(0.0), to: alignment.as_ref().map(|x| x.start_station_m).unwrap_or(0.0), dir: None, dwell: String::new() },
                });
                crate::station_input::station_field_units(ui, doc, alignment_id, &format!("tl-from-{}", a.id), &mut form.from);
                crate::station_input::station_field_units(ui, doc, alignment_id, &format!("tl-to-{}", a.id), &mut form.to);
                egui::ComboBox::from_id_salt(("tl-dir", &a.id)).width(190.0).selected_text(form.dir.map(direction_text).unwrap_or("choose")).show_ui(ui, |ui| {
                    for d in [WorkDirection::Increasing, WorkDirection::Decreasing, WorkDirection::Fixed] {
                        ui.selectable_value(&mut form.dir, Some(d), direction_text(d));
                    }
                });
                ui.add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut form.dwell).hint_text("none"));
                ui.label(match (entered, current) {
                    (Some(_), _) => RichText::new("entered").color(GOOD()),
                    (None, Some(l)) if l.derived => RichText::new("derived from elements").color(WARN()),
                    _ => RichText::new("none").weak(),
                });
                let mut cleared = false;
                ui.horizontal(|ui| {
                    if ui.small_button("Set").on_hover_text("Stores this range on the activity.").clicked() {
                        match glue::range_from_entries(name, form.from, form.to, form.dir, &form.dwell) {
                            Ok(r) => {
                                *set_range = Some((a.id.clone(), Some(r)));
                                ext.problem.clear();
                            }
                            Err(e) => ext.problem = format!("{}: {e}", a.id),
                        }
                    }
                    if entered.is_some() && ui.small_button("Clear").clicked() {
                        *set_range = Some((a.id.clone(), None));
                        cleared = true;
                    }
                });
                if cleared {
                    ext.tl_forms.remove(&a.id);
                } else {
                    ext.tl_forms.insert(a.id.clone(), form);
                }
                ui.end_row();
            }
        });
    });
    if !ext.problem.is_empty() {
        crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, &ext.problem);
    }
}

/// The chart a sheet carries: drawing and labels.
pub fn time_location_for_sheet(p: &Programme, s: &Schedule, doc: &afrobim_kernel::Document) -> Result<(afrobim_draw::Drawing, Vec<(afrobim_draw::Pt2, String)>), String> {
    let resolved = resolve_locations(p, Some(doc));
    if resolved.items.is_empty() {
        return Err("No activity has a location range: enter ranges in TIMELOCATION, or link activities to elements placed on an alignment.".into());
    }
    let alignment = glue::setting(doc, glue::KEY_TL_ALIGNMENT).filter(|n| resolved.alignments().contains(n));
    let mut chart: TimeLocationChart = time_location_chart(p, s, &resolved, &TimeLocationOptions { alignment, ..Default::default() }).ok_or("The time-location chart has nothing to plot on that alignment.")?;
    relabel_stations(doc, &mut chart);
    Ok((chart.drawing, chart.labels.into_iter().map(|l| (l.at, l.text)).collect()))
}

/// A tick of the station scale as the document shows stations: `k+mmm`. Ticks are at whole metres
/// in practice, so no decimals are shown for a whole number; a tick that is not whole gets the
/// document's station decimals.
pub fn tick_station_text(u: &afrobim_annotate::DisplayUnits, station_m: f64) -> String {
    if (station_m - station_m.round()).abs() < 1e-9 {
        afrobim_alignment::format_station(station_m, 0)
    } else {
        crate::disp::station(u, station_m)
    }
}

/// The station scale of the chart is in internal stations (the engine knows nothing of station
/// equations or display units). Each station label is rewritten as the station shown on the
/// drawings (through the alignment's station equations where it has them) in the document's
/// station format. The engine's CSV keeps its own text.
pub fn relabel_stations(doc: &afrobim_kernel::Document, chart: &mut TimeLocationChart) {
    let Some(a) = crate::alignment_glue::alignments(doc).into_iter().find(|a| a.1 == chart.alignment).and_then(|a| crate::alignment_glue::alignment_of(doc, a.0)) else { return };
    let units = crate::anno_ui::units_of(doc);
    let equations = a.station_equations.region_count() > 1;
    for l in chart.labels.iter_mut().filter(|l| l.kind == TlLabelKind::Station) {
        if let Ok(internal) = l.text.trim().parse::<f64>() {
            let shown = if equations { a.displayed_station(internal).map(|d| d.station_m) } else { Some(internal) };
            if let Some(m) = shown {
                l.text = tick_station_text(&units, m);
            }
        }
    }
}

#[cfg(test)]
mod relabel_tests {
    use super::*;

    #[test]
    fn the_time_location_scale_is_labelled_in_displayed_stations_where_there_are_equations() {
        let doc = crate::sheets_glue::test_support::demo_doc();
        let (_, p) = crate::schedule_glue::load(&doc).unwrap();
        let s = afrobim_schedule::schedule_with(&p, &afrobim_schedule::ScheduleOptions { critical: crate::schedule_glue::critical_basis_of(&doc) }).unwrap();
        let (aid, _) = crate::alignment_glue::alignments(&doc).remove(0);
        assert!(crate::alignment_glue::alignment_of(&doc, aid).unwrap().station_equations.region_count() > 1);
        let (_, labels) = time_location_for_sheet(&p, &s, &doc).unwrap();
        assert!(labels.iter().any(|(_, t)| t.contains('+')), "station labels read as chainage, e.g. 1+500: {:?}", labels.iter().map(|l| l.1.clone()).take(12).collect::<Vec<_>>());
    }

    #[test]
    fn a_tick_is_a_station_in_the_documents_format() {
        let mut u = afrobim_annotate::DisplayUnits::default();
        assert_eq!(tick_station_text(&u, 1500.0), "1+500");
        assert_eq!(tick_station_text(&u, 0.0), "0+000");
        assert_eq!(tick_station_text(&u, 12345.0), "12+345");
        // A tick that is not a whole number of metres takes the document's station decimals.
        assert_eq!(tick_station_text(&u, 1500.5), "1+500.500");
        u.station_decimals = 1;
        assert_eq!(tick_station_text(&u, 1500.5), "1+500.5");
    }

    #[test]
    fn the_scale_of_an_alignment_without_station_equations_reads_as_stations_too() {
        let mut doc = crate::sheets_glue::test_support::demo_doc();
        let (aid, _) = crate::alignment_glue::alignments(&doc).remove(0);
        let mut a = crate::alignment_glue::alignment_of(&doc, aid).unwrap();
        a.station_equations.equations.clear();
        assert!(a.station_equations.region_count() <= 1);
        doc.find_mut(aid).unwrap().data = serde_json::to_value(&a).unwrap();
        let (_, p) = crate::schedule_glue::load(&doc).unwrap();
        let s = afrobim_schedule::schedule_with(&p, &afrobim_schedule::ScheduleOptions { critical: crate::schedule_glue::critical_basis_of(&doc) }).unwrap();
        let (_, labels) = time_location_for_sheet(&p, &s, &doc).unwrap();
        // Station labels are k+mmm; the engine's own text for them was a plain number of metres.
        let stations: Vec<&String> = labels.iter().map(|l| &l.1).filter(|t| t.contains('+') && t.chars().next().is_some_and(|c| c.is_ascii_digit())).collect();
        assert!(!stations.is_empty(), "{:?}", labels.iter().map(|l| l.1.clone()).take(12).collect::<Vec<_>>());
        assert!(stations.iter().all(|t| t.split('+').nth(1).is_some_and(|m| m.len() == 3 && m.chars().all(|c| c.is_ascii_digit()))), "{stations:?}");
    }
}
