//! The alignment design tools: the plan editor on the canvas (PI grips, create, insert, delete),
//! the PI table, and the entry points of the design commands.
//!
//! The plan editor draws in site metres on its own view, so it never touches the design sheet in
//! millimetres. Every committed edit goes through `design_glue` (the engine's edit, `store_design`
//! and the resynchronisation of everything on the alignment) and then through the document's undo.
//!
//! No radius, spiral length, K or grade is defaulted anywhere here: a curve starts with none and
//! the table says "radius needed" until the engineer enters one.

use std::collections::HashMap;

use afrobim_alignment::Pi;
use afrobim_annotate::DisplayUnits;
use afrobim_design::{plan_labels_with, AlignmentDesign, HorizontalSolution, PlanLabel, PlanLabelKind};
use afrobim_kernel::{display_spacing, ortho, snap, ElementId, Pt, SelectMode, ViewTransform};
use egui::{pos2, Align2, Color32, FontId, PointerButton, Rect, RichText, Sense, Shape, Stroke, Ui, Vec2};

use crate::app::AfroBimApp;
use crate::design_glue::{self as glue, HEdit};
use crate::disp::{cell_changed, cell_text, parse_cell, Cell};

#[allow(non_snake_case)]

pub fn BAD() -> egui::Color32 {

    crate::theme::danger()

}
#[allow(non_snake_case)]
pub fn WARN() -> egui::Color32 {
    crate::theme::warn()
}
#[allow(non_snake_case)]
pub fn GOOD() -> egui::Color32 {
    crate::theme::success()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlanTool {
    Select,
    Insert,
    Delete,
}

/// A design being made: the PIs picked so far.
pub struct Create {
    pub name: String,
    pub start_m: f64,
    pub points: Vec<(f64, f64)>,
}

#[derive(Default)]
pub struct NewDialog {
    pub name: String,
    pub start: String,
    pub problem: String,
    /// 0 picks the PIs on the plan, 1 types them: one a line, `x,y` or `@dx,dy` after the first.
    pub mode: usize,
    pub pis_text: String,
}

/// The PIs typed in the dialog: the points, or the first line that is not one.
pub fn parse_typed_pis(units: &DisplayUnits, text: &str) -> Result<Vec<(f64, f64)>, String> {
    let mut points: Vec<(f64, f64)> = Vec::new();
    for (i, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        match crate::disp::parse_point(units, line.trim(), points.last().copied()) {
            Some(p) => points.push(p),
            None => return Err(format!("Line {}: '{}' is not a point. Write x,y, or @dx,dy from the PI before.", i + 1, line.trim())),
        }
    }
    Ok(points)
}

/// A PI being dragged. `working` is the last design the engine accepted, `refused` the engine's
/// reason when the cursor is somewhere impossible.
struct Drag {
    pi: usize,
    working: AlignmentDesign,
    refused: Option<String>,
    cursor: (f64, f64),
    last_ok: (f64, f64),
}

/// What the plan editor draws of one alignment of the document.
#[derive(Clone)]
pub struct PlanAlignment {
    pub name: String,
    pub design: Option<ElementId>,
    pub polyline: Vec<(f64, f64)>,
    pub ends: [(f64, f64); 2],
}

/// The solved plan of a design with its labels.
pub struct PlanData {
    pub polyline: Vec<(f64, f64)>,
    pub pis: Vec<Pi>,
    pub solution: HorizontalSolution,
    pub labels: Vec<PlanLabel>,
}

/// The plan of a design with its labels written in the document's display units (stations as
/// `k+mmm`, lengths in the display length unit).
pub fn plan_data(design: &AlignmentDesign, units: &DisplayUnits) -> Result<PlanData, String> {
    let solution = design.horizontal.solve().map_err(|e| e.to_string())?;
    let polyline = solution.alignment.polyline(1.5);
    let labels = labels_in(design, units);
    Ok(PlanData { polyline, pis: design.horizontal.pis.clone(), solution, labels })
}

/// The labels of a design in the document's display units.
pub fn labels_in(design: &AlignmentDesign, units: &DisplayUnits) -> Vec<PlanLabel> {
    let station = |s: f64| crate::disp::station(units, s);
    let length = |m: f64| crate::disp::len_u(units, m);
    plan_labels_with(design, &station, Some(&length)).unwrap_or_default()
}

fn ui_kit_trim(v: f64) -> String {
    let t = format!("{v:.3}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub struct DesignUi {
    pub plan_on: bool,
    pub view: ViewTransform,
    pub fitted: bool,
    /// The design element being edited.
    pub active: Option<ElementId>,
    pub tool: PlanTool,
    pub create: Option<Create>,
    pub new_dialog: Option<NewDialog>,
    drag: Option<Drag>,
    pub sel_pi: Option<usize>,
    pub labels_on: bool,
    pub table_open: bool,
    pub bufs: HashMap<(usize, u8), String>,
    pub ranges: Vec<Option<(f64, f64)>>,
    pub ranges_for: String,
    pub last: Option<glue::Resync>,
    pub cursor: (f64, f64),
    /// Bumped by anything that may change the alignments, so the drawn copies are rebuilt.
    pub revision: u64,
    cache_rev: u64,
    pub cache: Vec<PlanAlignment>,
    active_plan: Option<(ElementId, Result<PlanData, String>, AlignmentDesign)>,
    /// The display units the drawn labels were written in.
    plan_units: Option<DisplayUnits>,
    pub profile: crate::design_profile::ProfileUi,
    pub check: crate::design_windows::AlignCheckUi,
    pub options: crate::design_windows::OptionsUi,
}

impl Default for DesignUi {
    fn default() -> Self {
        DesignUi {
            plan_on: false,
            view: ViewTransform::default(),
            fitted: false,
            active: None,
            tool: PlanTool::Select,
            create: None,
            new_dialog: None,
            drag: None,
            sel_pi: None,
            labels_on: true,
            table_open: false,
            bufs: HashMap::new(),
            ranges: Vec::new(),
            ranges_for: String::new(),
            last: None,
            cursor: (0.0, 0.0),
            revision: 1,
            cache_rev: 0,
            cache: Vec::new(),
            active_plan: None,
            plan_units: None,
            profile: Default::default(),
            check: Default::default(),
            options: Default::default(),
        }
    }
}

impl DesignUi {
    /// The alignments or designs changed (an edit, undo, open, import): redraw from the document.
    pub fn invalidate(&mut self) {
        self.revision += 1;
        self.bufs.clear();
        self.ranges_for.clear();
    }

    /// The prompt of the command line while a design tool wants a point.
    pub fn prompt(&self) -> Option<String> {
        let c = self.create.as_ref()?;
        Some(format!("Pick PI {} of {} (x,y or @dx,dy; Enter finishes, U removes the last, Esc cancels):", c.points.len() + 1, c.name))
    }

    pub fn wants_text(&self) -> bool {
        self.create.is_some()
    }
}

/// The alignments of the document as drawn in plan.
fn collect_alignments(doc: &afrobim_kernel::Document) -> Vec<PlanAlignment> {
    let designed = glue::designs(doc);
    doc.elements()
        .iter()
        .filter(|e| e.category == crate::alignment_glue::CATEGORY)
        .filter_map(|e| {
            let a = crate::alignment_glue::alignment_of(doc, e.id)?;
            let polyline = a.polyline(2.0);
            let first = *polyline.first()?;
            let last = *polyline.last()?;
            Some(PlanAlignment { name: e.name.clone(), design: designed.iter().find(|d| d.1 == e.id).map(|d| d.0), polyline, ends: [first, last] })
        })
        .collect()
}

fn dist_point_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> (f64, (f64, f64)) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-12 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) };
    let q = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - q.0).hypot(p.1 - q.1), q)
}

/// The deflection angle at an interior PI, signed (positive to the left), degrees.
pub fn deflection_deg(pis: &[Pi], i: usize) -> Option<f64> {
    if i == 0 || i + 1 >= pis.len() {
        return None;
    }
    let h0 = (pis[i].y - pis[i - 1].y).atan2(pis[i].x - pis[i - 1].x);
    let h1 = (pis[i + 1].y - pis[i].y).atan2(pis[i + 1].x - pis[i].x);
    Some(afrobim_alignment::wrap_pi(h1 - h0).to_degrees())
}

impl AfroBimApp {
    // ─── commands ────────────────────────────────────────────────────────────────────────

    /// `ALIGNDESIGN`: asks for the name and start station, then for the PIs on the plan.
    pub(crate) fn design_start_new(&mut self) {
        self.design.new_dialog = Some(NewDialog::default());
        self.design.create = None;
        self.status = "ALIGNDESIGN: name the alignment and enter its start station, then pick the PIs on the plan.".into();
    }

    /// `ALIGNEDIT`: the plan editor on the selected designed alignment (or the only one).
    pub(crate) fn design_edit_plan(&mut self) {
        let selected = glue::design_for_selection(&self.doc, self.doc.primary());
        let first = glue::designs(&self.doc).first().map(|d| d.0);
        match selected.or(self.design.active).filter(|d| self.doc.find(*d).is_some()).or(first) {
            Some(d) => {
                self.design.active = Some(d);
                self.design.plan_on = true;
                self.design.fitted = false;
                self.design.tool = PlanTool::Select;
                self.view_3d = false;
                self.design.invalidate();
                self.status = "Plan editor: drag a PI, or use Insert PI and Delete PI. Radii and spirals are entered in the PI table.".into();
            }
            None => self.status = "There is no designed alignment yet. Make one with ALIGNDESIGN. (An imported alignment has no PIs to edit.)".into(),
        }
    }

    /// The design the table, profile and checks work on: the selection, the active one, or the first.
    pub(crate) fn design_target(&mut self) -> Option<ElementId> {
        let selected = glue::design_for_selection(&self.doc, self.doc.primary());
        let first = glue::designs(&self.doc).first().map(|d| d.0);
        let pick = selected.or(self.design.active).filter(|d| self.doc.find(*d).is_some()).or(first);
        if pick.is_some() {
            self.design.active = pick;
        }
        pick
    }

    pub(crate) fn design_open_table(&mut self) {
        match self.design_target() {
            Some(_) => self.design.table_open = true,
            None => self.status = "There is no designed alignment yet. Make one with ALIGNDESIGN.".into(),
        }
    }

    /// A text typed on the command line while a design tool is waiting for a point. True when used.
    pub(crate) fn design_take_text(&mut self, text: &str) -> bool {
        let units = crate::anno_ui::units_of(&self.doc);
        let Some(create) = self.design.create.as_mut() else { return false };
        let t = text.trim();
        if t.is_empty() || t.eq_ignore_ascii_case("c") || t.eq_ignore_ascii_case("done") {
            self.design_finish_create();
            return true;
        }
        if t.eq_ignore_ascii_case("u") || t.eq_ignore_ascii_case("undo") {
            create.points.pop();
            self.status = "Last PI removed.".into();
            return true;
        }
        match crate::disp::parse_point(&units, t, create.points.last().copied()) {
            Some(p) => {
                create.points.push(p);
                self.status = format!("PI {} at E {} N {}.", create.points.len(), crate::disp::len_u(&units, p.0), crate::disp::len_u(&units, p.1));
            }
            None => self.status = format!("'{t}' is not a point. Type x,y or @dx,dy or @distance<angle, in {} (a trailing m or mm overrides).", if crate::disp::suffix(&units) == "m" { "metres" } else { "millimetres" }),
        }
        true
    }

    pub(crate) fn design_finish_create(&mut self) {
        let Some(c) = self.design.create.take() else { return };
        match glue::create(&mut self.doc, &c.name, c.start_m, &c.points) {
            Ok((design, alignment)) => {
                self.design.active = Some(design);
                self.design.tool = PlanTool::Select;
                self.design.invalidate();
                self.doc.select(&[alignment], SelectMode::Replace);
                self.sync_generated();
                self.schedule_commit_now();
                self.design.table_open = true;
                self.status = format!("Alignment {} made from {} PIs. Every curve needs a radius: enter them in the PI table.", c.name, c.points.len());
                self.log(format!("ALIGNDESIGN: {} made, {} PIs, no radius entered yet.", c.name, c.points.len()));
            }
            Err(e) => {
                self.status = format!("Not made: {e}");
                self.design.create = Some(c);
            }
        }
    }

    pub(crate) fn design_escape(&mut self) {
        if self.design.create.take().is_some() {
            self.status = "ALIGNDESIGN cancelled.".into();
        } else if self.design.drag.take().is_some() {
            self.status = "Move cancelled.".into();
        } else if self.design.plan_on && !self.design.new_dialog.is_some() {
            self.design.plan_on = false;
        }
    }

    // ─── applying edits ──────────────────────────────────────────────────────────────────

    /// Applies a plan edit and says what happened. A refused edit shows the engine's reason.
    pub(crate) fn design_apply_h(&mut self, edit: HEdit) -> bool {
        let Some(d) = self.design.active else { return false };
        match glue::edit_horizontal(&mut self.doc, d, edit) {
            Ok(r) => {
                self.design_after_edit(r);
                true
            }
            Err(e) => {
                self.status = format!("Refused: {e} Nothing was changed.");
                false
            }
        }
    }

    pub(crate) fn design_apply_v(&mut self, edit: glue::VEdit) -> bool {
        let Some(d) = self.design.active else { return false };
        match glue::edit_vertical(&mut self.doc, d, edit) {
            Ok(r) => {
                self.design_after_edit(r);
                true
            }
            Err(e) => {
                self.status = format!("Refused: {e} Nothing was changed.");
                false
            }
        }
    }

    /// After any stored edit: redraw, record for undo, and show what was resynchronised.
    pub(crate) fn design_after_edit(&mut self, r: glue::Resync) {
        self.design.invalidate();
        self.sync_generated();
        self.schedule_commit_now();
        self.status = r.note();
        for w in &r.warnings {
            self.log(format!("Design: {w}"));
        }
        self.log(format!("Design: {}", r.note()));
        self.design.last = Some(r);
    }

    // ─── the plan canvas ─────────────────────────────────────────────────────────────────

    fn plan_refresh(&mut self) {
        if self.design.cache_rev != self.design.revision {
            self.design.cache = collect_alignments(&self.doc);
            self.design.active_plan = None;
            self.design.cache_rev = self.design.revision;
        }
        let units = crate::anno_ui::units_of(&self.doc);
        if self.design.plan_units.as_ref() != Some(&units) {
            // The labels are written in the display units: write them again when those change.
            self.design.active_plan = None;
            self.design.plan_units = Some(units.clone());
        }
        let Some(active) = self.design.active else {
            self.design.active_plan = None;
            return;
        };
        if self.design.active_plan.as_ref().is_none_or(|(id, _, _)| *id != active) {
            self.design.active_plan = match glue::load(&self.doc, active) {
                Ok(d) => Some((active, plan_data(&d, &units), d)),
                Err(_) => None,
            };
        }
    }

    fn plan_fit(&mut self, rect: Rect) {
        let mut pts: Vec<(f64, f64)> = match &self.design.active_plan {
            Some((_, Ok(p), _)) => p.polyline.iter().step_by(4).copied().chain(p.pis.iter().map(|q| (q.x, q.y))).collect(),
            _ => self.design.cache.iter().flat_map(|a| a.polyline.iter().step_by(10).copied()).collect(),
        };
        if let Some(c) = &self.design.create {
            pts.extend(c.points.iter().copied());
        }
        if pts.is_empty() {
            pts = vec![(-100.0, -100.0), (500.0, 500.0)];
        }
        let (x0, x1) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.0), b.max(p.0)));
        let (y0, y1) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let bbox = afrobim_kernel::BBox::new(x0, y0, x1.max(x0 + 10.0), y1.max(y0 + 10.0));
        self.design.view.fit(&bbox, rect.left() as f64, rect.top() as f64, rect.width() as f64, rect.height() as f64, 70.0);
    }

    /// The point a pick lands on: the object snap to a PI or an alignment end (always on), then ortho
    /// and the grid, which follow the SNAP and ORTHO toggles of the status bar.
    fn plan_snap(&self, raw: Pt, from: Option<Pt>) -> Pt {
        let view = &self.design.view;
        let mut p = raw;
        if self.ctl.ortho_on {
            if let Some(f) = from {
                p = ortho(f, p);
            }
        }
        let limit = 10.0 / view.scale;
        let mut best: Option<(f64, Pt)> = None;
        let mut consider = |q: (f64, f64)| {
            let d = (q.0 - raw.x).hypot(q.1 - raw.y);
            if d <= limit && best.is_none_or(|b| d < b.0) {
                best = Some((d, Pt::new(q.0, q.1)));
            }
        };
        for a in &self.design.cache {
            consider(a.ends[0]);
            consider(a.ends[1]);
        }
        if let Some((_, Ok(plan), _)) = &self.design.active_plan {
            for pi in &plan.pis {
                consider((pi.x, pi.y));
            }
        }
        if let Some(c) = &self.design.create {
            for q in &c.points {
                consider(*q);
            }
        }
        if let Some((_, q)) = best {
            return q;
        }
        if self.ctl.snap_on {
            let step = display_spacing(10.0, view.scale, 14.0);
            p = snap(p, step);
        }
        p
    }

    pub(crate) fn canvas_plan(&mut self, ui: &mut Ui) {
        self.plan_refresh();
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;
        if !self.design.fitted && rect.width() > 50.0 {
            self.design.fitted = true;
            self.plan_fit(rect);
        }
        let dark = self.dark_canvas;
        let ink = if dark { Color32::from_gray(235) } else { Color32::from_gray(30) };
        let weak = if dark { Color32::from_rgb(0x9A, 0xA2, 0xAC) } else { Color32::from_rgb(0x55, 0x5B, 0x63) };

        // View: wheel zooms, middle button pans.
        let (scroll, mid) = ui.input(|i| (i.raw_scroll_delta.y, i.pointer.button_down(PointerButton::Middle)));
        let hover = response.hover_pos();
        if let Some(h) = hover {
            if scroll != 0.0 {
                self.design.view.zoom_about(Pt::new(h.x as f64, h.y as f64), (scroll as f64 * 0.0025).exp());
            }
            if mid {
                let d = ui.input(|i| i.pointer.delta());
                self.design.view.pan(d.x as f64, d.y as f64);
            }
        }
        if response.double_clicked_by(PointerButton::Middle) {
            self.plan_fit(rect);
        }
        let view = self.design.view.clone();
        let units = crate::anno_ui::units_of(&self.doc);
        let to_screen = |p: (f64, f64)| {
            let s = view.to_screen(Pt::new(p.0, p.1));
            pos2(s.x as f32, s.y as f32)
        };
        let world_of = |p: egui::Pos2| view.to_world(Pt::new(p.x as f64, p.y as f64));

        // What the pointer is over: a PI grip of the active design.
        let grip_px = 8.0_f32;
        let active_pis: Vec<Pi> = match (&self.design.drag, &self.design.active_plan) {
            (Some(d), _) => d.working.horizontal.pis.clone(),
            (None, Some((_, Ok(p), _))) => p.pis.clone(),
            _ => Vec::new(),
        };
        let hit_pi = |pos: egui::Pos2| active_pis.iter().position(|p| to_screen((p.x, p.y)).distance(pos) <= grip_px + 2.0);

        // Pointer logic.
        let creating = self.design.create.is_some();
        if let Some(h) = hover {
            let w = world_of(h);
            let from = self.design.create.as_ref().and_then(|c| c.points.last()).map(|p| Pt::new(p.0, p.1));
            let s = self.plan_snap(w, from);
            self.design.cursor = (s.x, s.y);
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if creating {
            if response.clicked_by(PointerButton::Primary) {
                if let Some(c) = self.design.create.as_mut() {
                    c.points.push(self.design.cursor);
                    self.status = format!("PI {} at E {:.3} N {:.3}.", c.points.len(), self.design.cursor.0, self.design.cursor.1);
                }
            }
            if response.clicked_by(PointerButton::Secondary) {
                self.design_finish_create();
            }
        } else {
            self.plan_edit_pointer(ui, &response, hit_pi, world_of);
        }
        let hover_pi = hover.and_then(hit_pi);

        // ── painting ──
        painter.rect_filled(rect, 0.0, if dark { Color32::from_rgb(0x1F, 0x23, 0x29) } else { Color32::from_rgb(0xF2, 0xF2, 0xEE) });
        if self.grid_on {
            let step = display_spacing(10.0, view.scale, 14.0);
            let (minor, major) = if dark { (Color32::from_rgb(0x26, 0x2B, 0x32), Color32::from_rgb(0x31, 0x37, 0x40)) } else { (Color32::from_rgb(0xE6, 0xE6, 0xE1), Color32::from_rgb(0xD4, 0xD4, 0xCE)) };
            let min = view.to_world(Pt::new(rect.left() as f64, rect.bottom() as f64));
            let max = view.to_world(Pt::new(rect.right() as f64, rect.top() as f64));
            let (ix0, ix1) = ((min.x / step).ceil() as i64, (max.x / step).floor() as i64);
            let (iy0, iy1) = ((min.y / step).ceil() as i64, (max.y / step).floor() as i64);
            if ix1 - ix0 < 600 && iy1 - iy0 < 600 {
                for i in ix0..=ix1 {
                    let x = to_screen((i as f64 * step, 0.0)).x.round() + 0.5;
                    painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0_f32, if i % 5 == 0 { major } else { minor }));
                }
                for i in iy0..=iy1 {
                    let y = to_screen((0.0, i as f64 * step)).y.round() + 0.5;
                    painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0_f32, if i % 5 == 0 { major } else { minor }));
                }
                painter.text(rect.right_bottom() + Vec2::new(-10.0, -8.0), Align2::RIGHT_BOTTOM, format!("grid {}", crate::disp::len_u(&units, step)), FontId::proportional(10.0), weak);
            }
        }
        let active_name = self.design.active.and_then(|d| glue::alignment_of_design(&self.doc, d)).and_then(|a| self.doc.find(a)).map(|e| e.name.clone());
        for a in &self.design.cache {
            if Some(&a.name) == active_name.as_ref() {
                continue;
            }
            let pts: Vec<egui::Pos2> = a.polyline.iter().map(|p| to_screen(*p)).collect();
            painter.add(Shape::line(pts, Stroke::new(1.5_f32, weak)));
            painter.text(to_screen(a.ends[0]) + Vec2::new(6.0, -8.0), Align2::LEFT_BOTTOM, &a.name, FontId::proportional(10.5), weak);
        }

        // The active design: from the drag's working copy while dragging.
        let live: Option<PlanData> = self.design.drag.as_ref().and_then(|d| plan_data(&d.working, &units).ok());
        let shown: Option<&PlanData> = live.as_ref().or(match &self.design.active_plan {
            Some((_, Ok(p), _)) => Some(p),
            _ => None,
        });
        if let Some(p) = shown {
            // Tangent legs between the PIs, dashed.
            let legs: Vec<egui::Pos2> = p.pis.iter().map(|q| to_screen((q.x, q.y))).collect();
            painter.extend(Shape::dashed_line(&legs, Stroke::new(1.0_f32, weak), 6.0, 4.0));
            let pts: Vec<egui::Pos2> = p.polyline.iter().map(|q| to_screen(*q)).collect();
            painter.add(Shape::line(pts, Stroke::new(2.0_f32, ink)));
            // Curve centres, small blue crosses.
            for c in &p.solution.curves {
                let s = to_screen(c.centre);
                let st = Stroke::new(1.0_f32, Color32::from_rgb(0x4D, 0xA3, 0xFF));
                painter.line_segment([s + Vec2::new(-4.0, 0.0), s + Vec2::new(4.0, 0.0)], st);
                painter.line_segment([s + Vec2::new(0.0, -4.0), s + Vec2::new(0.0, 4.0)], st);
            }
            // Labels.
            if self.design.labels_on {
                let blue = Color32::from_rgb(0x4D, 0xA3, 0xFF);
                let mut curve_data = p.labels.iter().filter(|l| l.kind == PlanLabelKind::CurveData);
                for l in &p.labels {
                    let at = to_screen((l.x_m, l.y_m));
                    match l.kind {
                        PlanLabelKind::Pi => plan_text(&painter, at + Vec2::new(10.0, -10.0), Align2::LEFT_BOTTOM, &l.text, 11.0, ink, dark),
                        PlanLabelKind::CurveData => {}
                        PlanLabelKind::CurvePoint => {
                            // The tangent points sit on the two sides of the curve; spiral points above them.
                            let (anchor, off) = match l.text.split_whitespace().next().unwrap_or("") {
                                "TS" | "PC" => (Align2::RIGHT_TOP, Vec2::new(-6.0, 6.0)),
                                "ST" | "PT" => (Align2::LEFT_TOP, Vec2::new(6.0, 6.0)),
                                "SC" => (Align2::RIGHT_BOTTOM, Vec2::new(-6.0, -6.0)),
                                _ => (Align2::LEFT_BOTTOM, Vec2::new(6.0, -6.0)),
                            };
                            plan_text(&painter, at + off, anchor, &l.text, 9.5, weak, dark);
                        }
                    }
                }
                // Each curve's data as a callout on the outside of the curve, joined to its PI.
                for c in &p.solution.curves {
                    let Some(l) = curve_data.next() else { break };
                    let pi = p.pis[c.pi_index];
                    let (a, b) = (to_screen((pi.x, pi.y)), to_screen(c.centre));
                    let dir = (a - b).normalized();
                    let dir = if dir.x.is_finite() && dir.y.is_finite() { dir } else { Vec2::new(1.0, -1.0).normalized() };
                    // Towards the middle of the canvas, so the text stays on screen.
                    let side = if a.x <= rect.center().x { 1.0_f32 } else { -1.0 };
                    let end = a + Vec2::new(side * 46.0, dir.y * 30.0);
                    painter.line_segment([a, end], Stroke::new(1.0_f32, blue));
                    plan_text(&painter, end, if side > 0.0 { Align2::LEFT_CENTER } else { Align2::RIGHT_CENTER }, &l.text, 10.0, blue, dark);
                }
            }
            // Curves with no radius yet.
            for (i, q) in p.pis.iter().enumerate() {
                if q.radius == 0.0 && deflection_deg(&p.pis, i).is_some_and(|d| d.abs() > 0.05) {
                    plan_text(&painter, to_screen((q.x, q.y)) + Vec2::new(10.0, 12.0), Align2::LEFT_TOP, "radius needed", 10.5, WARN(), dark);
                }
            }
            // Grips.
            for (i, q) in p.pis.iter().enumerate() {
                let s = to_screen((q.x, q.y));
                let hot = hover_pi == Some(i) || self.design.drag.as_ref().is_some_and(|d| d.pi == i);
                let sel = self.design.sel_pi == Some(i);
                let r = Rect::from_center_size(s, Vec2::splat(grip_px * 1.5));
                let fill = if hot { Color32::from_rgb(0xFF, 0x5A, 0x36) } else if sel { Color32::from_rgb(0xF2, 0xA9, 0x1D) } else { Color32::from_rgb(0x2F, 0x7F, 0xE0) };
                painter.rect_filled(r, 0.0, fill);
                painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::WHITE), egui::StrokeKind::Inside);
            }
        } else if let Some((_, Err(e), _)) = &self.design.active_plan {
            plan_text(&painter, rect.left_top() + Vec2::new(14.0, 60.0), Align2::LEFT_TOP, &format!("This design cannot be drawn: {e}"), 12.0, BAD(), dark);
        }

        // A move the engine refuses: the cursor, in red, with the engine's reason.
        if let Some(d) = &self.design.drag {
            if let Some(reason) = &d.refused {
                let s = to_screen(d.cursor);
                painter.circle_stroke(s, 9.0, Stroke::new(2.0_f32, BAD()));
                painter.line_segment([s + Vec2::new(-6.0, -6.0), s + Vec2::new(6.0, 6.0)], Stroke::new(2.0_f32, BAD()));
                painter.line_segment([s + Vec2::new(-6.0, 6.0), s + Vec2::new(6.0, -6.0)], Stroke::new(2.0_f32, BAD()));
                plan_text(&painter, s + Vec2::new(14.0, 14.0), Align2::LEFT_TOP, &format!("Refused: {reason}"), 11.0, BAD(), dark);
            }
        }

        // The design being made.
        if let Some(c) = &self.design.create {
            let mut pts: Vec<egui::Pos2> = c.points.iter().map(|p| to_screen(*p)).collect();
            if let Some(h) = hover {
                pts.push(to_screen(self.design.cursor));
                let _ = h;
            }
            painter.extend(Shape::dashed_line(&pts, Stroke::new(1.5_f32, Color32::from_rgb(0xFF, 0xB0, 0x30)), 6.0, 4.0));
            for (i, p) in c.points.iter().enumerate() {
                let s = to_screen(*p);
                painter.rect_filled(Rect::from_center_size(s, Vec2::splat(9.0)), 0.0, Color32::from_rgb(0xFF, 0xB0, 0x30));
                plan_text(&painter, s + Vec2::new(8.0, -8.0), Align2::LEFT_BOTTOM, &format!("PI {i}"), 11.0, ink, dark);
            }
            if let (Some(last), true) = (c.points.last(), hover.is_some()) {
                let (dx, dy) = (self.design.cursor.0 - last.0, self.design.cursor.1 - last.1);
                plan_text(&painter, to_screen(self.design.cursor) + Vec2::new(14.0, 18.0), Align2::LEFT_TOP, &format!("{}  bearing {:.3} deg", crate::disp::len_u(&units, dx.hypot(dy)), (90.0 - dy.atan2(dx).to_degrees()).rem_euclid(360.0)), 11.0, ink, dark);
            }
        }

        // The cursor position, as the status bar of a CAD plan view would give it.
        if hover.is_some() {
            plan_text(&painter, rect.left_bottom() + Vec2::new(12.0, -10.0), Align2::LEFT_BOTTOM, &format!("E {}   N {}", crate::disp::len(&units, self.design.cursor.0), crate::disp::len(&units, self.design.cursor.1)), 11.0, ink, dark);
        }

        // What was resynchronised by the last edit.
        if let Some(r) = &self.design.last {
            let text = r.note();
            plan_text(&painter, rect.right_bottom() + Vec2::new(-12.0, -26.0), Align2::RIGHT_BOTTOM, &text, 10.5, if r.problems.is_empty() { GOOD() } else { WARN() }, dark);
        }

        // The plan toolbar, on top of the canvas.
        let bar = Rect::from_min_size(rect.min + Vec2::new(10.0, 8.0), Vec2::new((rect.width() - 20.0).max(300.0), 30.0));
        let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
        self.plan_toolbar(&mut bar_ui);
    }

    fn plan_toolbar(&mut self, ui: &mut Ui) {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                crate::ui_kit::heading(ui, "Plan");
                for (tool, label, tip) in [
                    (PlanTool::Select, "Select and drag PIs", "Drag a PI grip. A move the engine refuses shows its reason and the PI goes back."),
                    (PlanTool::Insert, "Insert PI", "Click on a tangent to add a PI there."),
                    (PlanTool::Delete, "Delete PI", "Click a PI to delete it. At least two PIs must remain."),
                ] {
                    if ui.selectable_label(self.design.tool == tool, label).on_hover_text(tip).clicked() {
                        self.design.tool = tool;
                    }
                }
                ui.separator();
                let designs = glue::designs(&self.doc);
                let current = self.design.active.and_then(|d| designs.iter().find(|x| x.0 == d)).map(|x| x.2.clone()).unwrap_or_else(|| "choose".into());
                egui::ComboBox::from_id_salt("plan-design").selected_text(current).show_ui(ui, |ui| {
                    for (d, _, name) in &designs {
                        if ui.selectable_label(self.design.active == Some(*d), name).clicked() {
                            self.design.active = Some(*d);
                            self.design.sel_pi = None;
                            self.design.invalidate();
                        }
                    }
                });
                ui.checkbox(&mut self.design.labels_on, "Curve data labels");
                if ui.button("PI table").clicked() {
                    self.design.table_open = true;
                }
                if ui.button("Profile").clicked() {
                    self.design_open_profile();
                }
                if ui.button("Fit view").clicked() {
                    self.design.fitted = false;
                }
                if ui.button("Close plan").clicked() {
                    self.design.plan_on = false;
                }
            });
        });
    }

    /// The pointer in the editing tools: grips, insert on a tangent, delete.
    fn plan_edit_pointer(&mut self, ui: &Ui, response: &egui::Response, hit_pi: impl Fn(egui::Pos2) -> Option<usize>, world_of: impl Fn(egui::Pos2) -> Pt) {
        let tool = self.design.tool;
        // Pick up a PI: the drag starts where the button went down.
        if tool == PlanTool::Select && response.drag_started_by(PointerButton::Primary) {
            let origin = ui.input(|i| i.pointer.press_origin());
            if let (Some(o), Some((_, Ok(_), design))) = (origin, &self.design.active_plan) {
                if let Some(i) = hit_pi(o) {
                    let p = design.horizontal.pis[i];
                    self.design.sel_pi = Some(i);
                    self.design.drag = Some(Drag { pi: i, working: design.clone(), refused: None, cursor: (p.x, p.y), last_ok: (p.x, p.y) });
                }
            }
        }
        if let Some(pos) = response.interact_pointer_pos() {
            if self.design.drag.is_some() && response.dragged_by(PointerButton::Primary) {
                let w = world_of(pos);
                let (pi, base) = {
                    let d = self.design.drag.as_ref().expect("a drag is active");
                    (d.pi, d.working.clone())
                };
                let from = if pi > 0 { Some(Pt::new(base.horizontal.pis[pi - 1].x, base.horizontal.pis[pi - 1].y)) } else { None };
                let target = self.plan_snap(w, from);
                // The engine decides: ask it on every step, on the design as it was when the drag began.
                let original = match &self.design.active_plan {
                    Some((_, _, d)) => d.clone(),
                    None => base.clone(),
                };
                let result = glue::try_horizontal(&original, HEdit::MovePi(pi, target.x, target.y));
                if let Some(d) = self.design.drag.as_mut() {
                    d.cursor = (target.x, target.y);
                    match result {
                        Ok((next, _)) => {
                            d.working = next;
                            d.refused = None;
                            d.last_ok = (target.x, target.y);
                        }
                        Err(reason) => d.refused = Some(reason),
                    }
                }
                if let Some(reason) = self.design.drag.as_ref().and_then(|d| d.refused.clone()) {
                    self.status = format!("Refused: {reason}");
                }
            }
        }
        if response.drag_stopped_by(PointerButton::Primary) {
            if let Some(d) = self.design.drag.take() {
                match d.refused {
                    Some(reason) => self.status = format!("Move refused: {reason} The PI is back where it was."),
                    None => {
                        self.design_apply_h(HEdit::MovePi(d.pi, d.last_ok.0, d.last_ok.1));
                    }
                }
            }
        }
        if response.clicked_by(PointerButton::Primary) {
            let Some(pos) = response.interact_pointer_pos() else { return };
            match tool {
                PlanTool::Select => {
                    if let Some(i) = hit_pi(pos) {
                        self.design.sel_pi = Some(i);
                        return;
                    }
                    // Pick a designed alignment by its line.
                    let w = world_of(pos);
                    let limit = 8.0 / self.design.view.scale;
                    let mut best: Option<(f64, ElementId)> = None;
                    for a in &self.design.cache {
                        let Some(design) = a.design else { continue };
                        for s in a.polyline.windows(2) {
                            let (d, _) = dist_point_segment((w.x, w.y), s[0], s[1]);
                            if d <= limit && best.is_none_or(|b| d < b.0) {
                                best = Some((d, design));
                            }
                        }
                    }
                    if let Some((_, d)) = best {
                        self.design.active = Some(d);
                        self.design.sel_pi = None;
                        if let Some(a) = glue::alignment_of_design(&self.doc, d) {
                            self.doc.select(&[a], SelectMode::Replace);
                        }
                        self.design.invalidate();
                        self.status = "Designed alignment selected. Drag a PI grip to edit it.".into();
                    }
                }
                PlanTool::Insert => {
                    let w = world_of(pos);
                    let Some((_, Ok(plan), _)) = &self.design.active_plan else { return };
                    let limit = 10.0 / self.design.view.scale;
                    let mut best: Option<(f64, usize, (f64, f64))> = None;
                    for (i, s) in plan.pis.windows(2).enumerate() {
                        let (d, q) = dist_point_segment((w.x, w.y), (s[0].x, s[0].y), (s[1].x, s[1].y));
                        if d <= limit && best.is_none_or(|b| d < b.0) {
                            best = Some((d, i, q));
                        }
                    }
                    match best {
                        Some((_, i, q)) => {
                            // On the tangent itself, so the alignment does not change until a radius is entered.
                            self.design_apply_h(HEdit::InsertPi(i + 1, q.0, q.1));
                        }
                        None => self.status = "Click on a tangent (the dashed line between two PIs).".into(),
                    }
                }
                PlanTool::Delete => {
                    if let Some(i) = hit_pi(pos) {
                        if self.design_apply_h(HEdit::DeletePi(i)) {
                            self.design.sel_pi = None;
                        }
                    } else {
                        self.status = "Click a PI to delete it.".into();
                    }
                }
            }
        }
    }

    // ─── windows ─────────────────────────────────────────────────────────────────────────

    pub(crate) fn design_windows(&mut self, ctx: &egui::Context) {
        self.new_alignment_window(ctx);
        if self.design.table_open {
            self.pi_table_window(ctx);
        }
        if self.design.profile.open {
            self.profile_window(ctx);
        }
        if self.design.check.open {
            self.align_check_window(ctx);
        }
        if self.design.options.open {
            self.options_window(ctx);
        }
        // The first frame after an undo, redo or open: redraw from the document.
        if self.design.active.is_some_and(|d| self.doc.find(d).is_none()) {
            self.design.active = None;
            self.design.invalidate();
        }
    }

    fn new_alignment_window(&mut self, ctx: &egui::Context) {
        use crate::ui_kit::{self, Col, Dialog, Footer, Level};
        if self.design.new_dialog.is_none() {
            return;
        }
        let units = crate::anno_ui::units_of(&self.doc);
        let existing: Vec<String> = crate::alignment_glue::alignments(&self.doc).into_iter().map(|a| a.1).collect();
        let nd = self.design.new_dialog.as_mut().expect("checked above");
        let start = crate::disp::parse_station(&units, &nd.start);
        let name_ok = !nd.name.trim().is_empty();
        let typed = parse_typed_pis(&units, &nd.pis_text);
        let (enabled, reason, primary) = match (name_ok, start, nd.mode, &typed) {
            (false, _, _, _) => (false, "Give the alignment a name".to_string(), "Pick the PIs on the plan"),
            (_, None, _, _) => (false, "Enter the start station as 1+250.000 or as metres".to_string(), "Pick the PIs on the plan"),
            (_, _, 0, _) => (true, String::new(), "Pick the PIs on the plan"),
            (_, _, _, Err(e)) => (false, e.clone(), "Create the alignment"),
            (_, _, _, Ok(p)) if p.len() < 2 => (false, "An alignment needs at least two PIs".to_string(), "Create the alignment"),
            _ => (true, String::new(), "Create the alignment"),
        };
        let name_taken = name_ok && existing.iter().any(|n| n.eq_ignore_ascii_case(nd.name.trim()));
        let length_unit = if crate::disp::suffix(&units) == "m" { "metres" } else { "millimetres" };
        let out = Dialog::new("ALIGNDESIGN: new alignment", crate::glyphs::by_name("ALIGNDESIGN"))
            .id_title("ALIGNDESIGN: new alignment")
            .subtitle("Name it, say where it starts, then give its PIs")
            .width(500.0)
            .resizable(false)
            .help("A PI is a point where the alignment changes direction. Curves start with no radius: you enter each one afterwards in the PI table, and the engine tells you the radii that are possible. The start station is the station at the first PI.")
            .show(
                ctx,
                |ui| {
                    ui_kit::section(ui, "Alignment", |ui| {
                        ui_kit::form(ui, "newalign", |f| {
                            f.row("Name", |ui| {
                                let r = ui_kit::text_field(ui, &mut nd.name, "for example MC10");
                                ui_kit::focus_once(ui, &r, "newalign-name");
                            });
                            f.row_note("Start station", &match start {
                                Some(m) => format!("{} m internal, shown as {}", ui_kit_trim(m), crate::disp::station(&units, m)),
                                None => "enter 1+250.000 or a number of metres".to_string(),
                            }, |ui| {
                                ui_kit::text_field(ui, &mut nd.start, "for example 1+000.000");
                            });
                        });
                        if name_taken {
                            ui.add_space(2.0);
                            ui_kit::callout(ui, Level::Warning, "An alignment with this name is already in the document. Names should be unique: change this one, or the two will be hard to tell apart.");
                        }
                    });
                    ui_kit::section(ui, "Points of intersection", |ui| {
                        ui_kit::segmented(ui, &mut nd.mode, &["Pick on the plan", "Type the coordinates"]);
                        ui.add_space(4.0);
                        if nd.mode == 0 {
                            ui.label(format!("Click each PI on the plan, or type x,y on the command line (in {length_unit}; @dx,dy is relative to the last PI). Enter or C finishes, U removes the last."));
                        } else {
                            crate::ui_kit::note(ui, format!("One PI a line, in {length_unit}: easting, northing. A line that starts with @ is relative to the PI before it."));
                            ui.add(egui::TextEdit::multiline(&mut nd.pis_text).font(egui::TextStyle::Monospace).desired_rows(5).desired_width(f32::INFINITY).hint_text("0, 0\n250, 40\n@300, -20"));
                            match &typed {
                                Err(e) => {
                                    ui_kit::callout(ui, Level::Error, e.as_str());
                                }
                                Ok(p) if !p.is_empty() => {
                                    let rows: Vec<Vec<String>> = p.iter().enumerate().map(|(i, (x, y))| vec![format!("{}", i + 1), crate::disp::len_u(&units, *x), crate::disp::len_u(&units, *y)]).collect();
                                    ui_kit::results_table(ui, "newalign-pis", &["PI", "Easting", "Northing"], &[Col::Num, Col::Num, Col::Num], &rows);
                                }
                                _ => {}
                            }
                        }
                    });
                    if !nd.problem.is_empty() {
                        ui.add_space(4.0);
                        ui_kit::callout(ui, Level::Error, nd.problem.clone());
                    }
                },
                |ui| Footer::new(primary).icon(crate::glyphs::by_name("ALIGNDESIGN")).enabled(enabled, &reason).show(ui),
            );
        let mode = nd.mode;
        let (name, pis) = (nd.name.trim().to_string(), typed.clone());
        if out.closed || out.inner.is_some_and(|(_, f)| f.cancel) {
            self.design.new_dialog = None;
            return;
        }
        if out.inner.is_some_and(|(_, f)| f.primary) {
            let Some(start) = start else { return };
            self.design.new_dialog = None;
            if mode == 0 {
                self.design.create = Some(Create { name, start_m: start, points: Vec::new() });
                self.design.plan_on = true;
                self.design.fitted = false;
                self.view_3d = false;
                self.design.invalidate();
                self.status = "Pick the first PI on the plan, or type x,y on the command line.".into();
            } else if let Ok(points) = pis {
                self.design.create = Some(Create { name, start_m: start, points });
                self.design.plan_on = true;
                self.design.fitted = false;
                self.view_3d = false;
                self.design_finish_create();
            }
        }
    }

    fn pi_table_window(&mut self, ctx: &egui::Context) {
        let Some(design_id) = self.design.active else {
            self.design.table_open = false;
            return;
        };
        let Ok(design) = glue::load(&self.doc, design_id) else {
            self.design.table_open = false;
            return;
        };
        let json = serde_json::to_string(&design.horizontal).unwrap_or_default();
        if self.design.ranges_for != json {
            self.design.ranges = (0..design.horizontal.pis.len()).map(|i| design.horizontal.feasible_radius_range(i)).collect();
            self.design.ranges_for = json;
            self.design.bufs.clear();
        }
        let solution = design.horizontal.solve();
        let mut open = true;
        let mut edit: Option<HEdit> = None;
        let mut sel: Option<usize> = None;
        let name = design.horizontal.name.clone();
        let pis = design.horizontal.pis.clone();
        let ranges = self.design.ranges.clone();
        let sel_now = self.design.sel_pi;
        let mut bufs = std::mem::take(&mut self.design.bufs);
        let last = self.design.last.clone();
        let mut bad_input = String::new();
        let units = crate::anno_ui::units_of(&self.doc);
        let u = &units;
        let work = crate::win::work_rect(ctx);
        let dlg = crate::ui_kit::Dialog::titled("PI table").subtitle("The points of intersection of the horizontal geometry").width(900.0).default_pos(egui::pos2(work.min.x + 16.0, (work.max.y - 250.0).max(work.min.y + 40.0))).show_body(ctx, |ui| {
            ui.label(RichText::new(format!("{name}: start station {}", units.station(design.horizontal.start_station_m))).strong());
            crate::ui_kit::note(ui, "Stations are the stations shown on the drawings; a designed alignment has no station equations, so they are the internal stations. Lengths are in the document's display unit (a trailing m or mm overrides it when typing). Enter a radius to make a curve. A blank radius is no curve. Feasible range is the radius that fits now with the spirals shown, from the engine.");
            if let Err(e) = &solution {
                ui.label(RichText::new(format!("The design cannot be solved: {e}")).color(BAD()));
            }
            egui::ScrollArea::both().auto_shrink([false, true]).max_height(260.0).show(ui, |ui| {
                egui::Grid::new("pi-table").num_columns(12).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                    let heads = ["PI".to_string(), "Station".to_string(), crate::disp::head(u, "East"), crate::disp::head(u, "North"), "Deflection".to_string(), crate::disp::head(u, "Radius"), crate::disp::head(u, "Feasible R"), crate::disp::head(u, "Spiral in"), crate::disp::head(u, "Spiral out"), crate::disp::head(u, "T"), crate::disp::head(u, "L"), String::new()];
                    for h in heads {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (i, pi) in pis.iter().enumerate() {
                        let curve = solution.as_ref().ok().and_then(|s| s.curve_at_pi(i));
                        let station = match (&solution, curve) {
                            (_, Some(c)) => Some(c.pi_station_m),
                            (Ok(s), None) => s.alignment.station_offset(pi.x, pi.y).map(|x| x.0),
                            _ => None,
                        };
                        let label = if sel_now == Some(i) { RichText::new(format!("{i}")).strong().color(WARN()) } else { RichText::new(format!("{i}")) };
                        if ui.selectable_label(sel_now == Some(i), label).clicked() {
                            sel = Some(i);
                        }
                        ui.label(station.map(|s| units.station(s)).unwrap_or_default());
                        for (k, value) in [(0u8, pi.x), (1u8, pi.y)] {
                            let cell = if k == 0 { Cell::East } else { Cell::North };
                            let text = bufs.entry((i, k)).or_insert_with(|| cell_text(u, cell, value));
                            let r = ui.add_sized([100.0, 20.0], egui::TextEdit::singleline(text));
                            if r.lost_focus() {
                                let typed = |k: u8, c: Cell| bufs.get(&(i, k)).and_then(|t| parse_cell(u, c, t).ok().flatten());
                                match (typed(0, Cell::East), typed(1, Cell::North)) {
                                    (Some(x), Some(y)) => {
                                        if cell_changed(u, Cell::East, pi.x, Some(x)) || cell_changed(u, Cell::North, pi.y, Some(y)) {
                                            edit = Some(HEdit::MovePi(i, x, y));
                                        }
                                    }
                                    _ => bad_input = format!("PI {i}: the coordinates must be lengths (a number in {}, or with m or mm).", crate::disp::suffix(u)),
                                }
                            }
                        }
                        let defl = deflection_deg(&pis, i);
                        ui.label(match defl {
                            Some(d) if d.abs() > 1e-6 => format!("{:.4} deg {}", d.abs(), if d > 0.0 { "left" } else { "right" }),
                            Some(_) => "straight".to_string(),
                            None => "end".to_string(),
                        });
                        let interior = i > 0 && i + 1 < pis.len();
                        if interior && defl.is_some_and(|d| d.abs() > 1e-6) {
                            let mut radius_text = bufs.entry((i, 2)).or_insert_with(|| cell_text(u, Cell::Radius, pi.radius)).clone();
                            ui.horizontal(|ui| {
                                let r = ui.add_sized([62.0, 20.0], egui::TextEdit::singleline(&mut radius_text).hint_text("needed"));
                                if pi.radius == 0.0 {
                                    ui.label(RichText::new("needed").small().color(WARN()));
                                }
                                if r.lost_focus() {
                                    let t = radius_text.clone();
                                    match parse_cell(u, Cell::Radius, &t) {
                                        Ok(None) => {
                                            if pi.radius != 0.0 {
                                                edit = Some(HEdit::SetRadius(i, 0.0));
                                            }
                                        }
                                        Ok(Some(v)) => {
                                            if cell_changed(u, Cell::Radius, pi.radius, Some(v)) {
                                                edit = Some(HEdit::SetRadius(i, v));
                                            }
                                        }
                                        Err(()) => bad_input = format!("PI {i}: the radius must be a length greater than zero."),
                                    }
                                }
                            });
                            bufs.insert((i, 2), radius_text);
                            ui.label(match ranges.get(i).copied().flatten() {
                                Some((lo, hi)) if hi >= 1.0e7 - 1.0 => format!("{} to no upper limit", crate::disp::len(u, lo)),
                                Some((lo, hi)) if lo < 0.01 => format!("up to {}", crate::disp::len(u, hi)),
                                Some((lo, hi)) => format!("{} to {}", crate::disp::len(u, lo), crate::disp::len(u, hi)),
                                None => "none fits".to_string(),
                            });
                            for k in [3u8, 4u8] {
                                let value = if k == 3 { pi.spiral_in } else { pi.spiral_out };
                                let text = bufs.entry((i, k)).or_insert_with(|| cell_text(u, if k == 3 { Cell::SpiralIn } else { Cell::SpiralOut }, value));
                                let r = ui.add_enabled(pi.radius > 0.0, egui::TextEdit::singleline(text).desired_width(56.0).hint_text("none"));
                                if r.lost_focus() {
                                    let a = bufs.get(&(i, 3)).cloned().unwrap_or_default();
                                    let b = bufs.get(&(i, 4)).cloned().unwrap_or_default();
                                    let parse = |t: &str, c: Cell| parse_cell(u, c, t).ok().map(|v| v.unwrap_or(0.0));
                                    match (parse(&a, Cell::SpiralIn), parse(&b, Cell::SpiralOut)) {
                                        (Some(x), Some(y)) => {
                                            if cell_changed(u, Cell::SpiralIn, pi.spiral_in, Some(x)) || cell_changed(u, Cell::SpiralOut, pi.spiral_out, Some(y)) {
                                                edit = Some(HEdit::SetSpirals(i, x, y));
                                            }
                                        }
                                        _ => bad_input = format!("PI {i}: a spiral length must be a length, zero or more; blank is none."),
                                    }
                                }
                            }
                        } else {
                            for _ in 0..4 {
                                ui.label("");
                            }
                        }
                        ui.label(curve.map(|c| crate::disp::len(u, c.tangent_in_m)).unwrap_or_default());
                        ui.label(curve.map(|c| crate::disp::len(u, c.total_length_m)).unwrap_or_default());
                        ui.horizontal(|ui| {
                            if i + 1 < pis.len() && ui.small_button("Ins").on_hover_text("Adds a PI on the tangent to the next PI. The alignment does not change.").clicked() {
                                let (a, b) = (pis[i], pis[i + 1]);
                                edit = Some(HEdit::InsertPi(i + 1, (a.x + b.x) / 2.0, (a.y + b.y) / 2.0));
                            }
                            if ui.small_button("Del").clicked() {
                                edit = Some(HEdit::DeletePi(i));
                            }
                        });
                        ui.end_row();
                    }
                });
            });
            if let Ok(s) = &solution {
                for w in &s.warnings {
                    ui.label(RichText::new(w).small().color(WARN()));
                }
                ui.label(RichText::new(format!("Length {}, end station {}.", crate::disp::len_u(u, s.alignment.length()), crate::disp::station(u, s.alignment.end_station_m()))).small());
            }
            if let Some(r) = &last {
                ui.label(RichText::new(r.note()).small().color(if r.problems.is_empty() { GOOD() } else { WARN() }));
            }
        });
        if dlg.closed {
            open = false;
        }
        self.design.bufs = bufs;
        if !bad_input.is_empty() {
            self.status = bad_input;
        }
        if let Some(i) = sel {
            self.design.sel_pi = Some(i);
        }
        if let Some(e) = edit {
            if !self.design_apply_h(e) {
                self.design.bufs.clear();
            }
        }
        if !open {
            self.design.table_open = false;
        }
    }

    /// `LANDXMLOUT`: writes a designed alignment as LandXML.
    pub(crate) fn design_landxml_out(&mut self) {
        let Some(d) = self.design_target() else {
            self.status = "There is no designed alignment to export. An imported alignment is already in a LandXML file; make a new one with ALIGNDESIGN.".into();
            return;
        };
        let name = glue::alignment_of_design(&self.doc, d).and_then(|a| self.doc.find(a)).map(|e| e.name.clone()).unwrap_or_else(|| "alignment".into());
        let text = match glue::landxml(&self.doc, d) {
            Ok(t) => t,
            Err(e) => {
                self.status = format!("Not exported: {e}");
                return;
            }
        };
        let Some(mut path) = rfd::FileDialog::new().add_filter("LandXML", &["xml"]).set_file_name(format!("{name}.xml")).save_file() else { return };
        if path.extension().is_none() {
            path.set_extension("xml");
        }
        self.design_write_landxml(&path, &text, &name);
    }

    pub(crate) fn design_write_landxml(&mut self, path: &std::path::Path, text: &str, name: &str) {
        match std::fs::write(path, text) {
            Ok(()) => self.status = format!("Exported {name} to {}. LandXML 1.2 as the engine writes it: checked against the schema and AfroBIM's own importer, not read by Civil 3D. Superelevation is not written.", path.display()),
            Err(e) => self.status = format!("Could not write {}: {e}", path.display()),
        }
    }
}

/// Text on the canvas with a faint backing so it reads over lines.
pub fn plan_text(painter: &egui::Painter, at: egui::Pos2, align: Align2, text: &str, size: f32, colour: Color32, dark: bool) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(size), colour);
    let rect = align.anchor_size(at, galley.size());
    painter.rect_filled(rect.expand2(Vec2::new(3.0, 1.0)), 2.0, if dark { Color32::from_rgba_unmultiplied(20, 22, 26, 190) } else { Color32::from_rgba_unmultiplied(246, 246, 242, 200) });
    painter.galley(rect.min, galley, colour);
}

#[cfg(test)]
mod new_dialog_tests {
    use super::*;

    #[test]
    fn typed_pis_are_read_one_a_line_with_relative_points() {
        let u = DisplayUnits::default();
        let p = parse_typed_pis(&u, "0, 0\n\n250, 40\n@300,-20\n").unwrap();
        assert_eq!(p.len(), 3);
        assert!((p[1].0 - 250.0).abs() < 1e-9 && (p[2].0 - 550.0).abs() < 1e-9 && (p[2].1 - 20.0).abs() < 1e-9, "{p:?}");
        let e = parse_typed_pis(&u, "0,0\nnot a point").unwrap_err();
        assert!(e.starts_with("Line 2:"), "{e}");
        assert!(parse_typed_pis(&u, "").unwrap().is_empty());
    }
}
