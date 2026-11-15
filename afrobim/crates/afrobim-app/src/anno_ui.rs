//! Annotation on the canvas and its windows: the active command, object snap, grips, the layer
//! manager, units, the style managers, the measure results and the Properties editor of an
//! annotation. The logic of each command is in `anno_glue` (a state machine that takes points and
//! typed text); this file turns clicks, hover and the command line into calls of it, and paints
//! what it returns.

use afrobim_annotate as ann;
use afrobim_annotate::geometry::{dist, pt};
use afrobim_annotate::{Ann, AnnGeometry, AnnotationSettings, DimData, DimKind, DimStyle, DisplayUnits, Justify, LayerProps, LeaderText, Seg, TextStyle};
use afrobim_kernel::{display_spacing, ortho, snap, Document, Element, ElementId, Pt, ViewTransform};
use egui::{pos2, Align2, Color32, FontId, Painter, Pos2, RichText, Shape, Stroke, Ui};

use crate::alignment_glue;
use crate::anno_glue::{self, Cand, Kind, Osnap, Out, Session, SnapMode};
use crate::app::AfroBimApp;

const GHOST: Color32 = Color32::from_rgb(0xFF, 0xB0, 0x30);
const SNAP_COL: Color32 = Color32::from_rgb(0xFF, 0xE0, 0x40);
const GRIP_FILL: Color32 = Color32::from_rgb(0x2F, 0x7F, 0xE0);
const GRIP_HOT: Color32 = Color32::from_rgb(0xFF, 0x5A, 0x36);
const BAD: Color32 = Color32::from_rgb(0xEF, 0x6C, 0x63);
const WARN: Color32 = Color32::from_rgb(0xD9, 0x8A, 0x2B);
/// Pixels of a grip's reach.
const GRIP_REACH_PX: f64 = 7.0;
/// Pixels within which object snap takes a candidate.
const SNAP_REACH_PX: f64 = 12.0;

/// The layers the layer manager does not list: the underlay placeholders (their switches are in
/// the underlay window) as well as the hidden settings layer the engine already leaves out.
pub fn hidden_layer(name: &str) -> bool {
    name == ann::SETTINGS_LAYER || name == crate::underlay::LAYER || name == afrobim_kernel::PROJECT_SETTINGS_LAYER
}

/// A grip being dragged.
pub struct GripDrag {
    pub id: ElementId,
    pub index: usize,
}

#[derive(Default)]
pub struct LayerWin {
    pub new_name: String,
    pub rename: Option<(String, String)>,
    pub delete: Option<(String, String)>,
    pub message: String,
}

#[derive(Default)]
pub struct AnnoUi {
    pub session: Option<Session>,
    pub osnap: Osnap,
    pub osnap_open: bool,
    /// The current layer for new annotation; empty means the default.
    pub layer: String,
    pub last_dim: Option<ElementId>,
    pub results: Vec<String>,
    pub results_open: bool,
    pub layers_open: bool,
    pub layer_win: LayerWin,
    pub units_open: bool,
    pub text_styles_open: bool,
    pub dim_styles_open: bool,
    pub text_sel: usize,
    pub dim_sel: usize,
    pub edit: Option<(ElementId, String)>,
    pub grip_hover: Option<(ElementId, usize)>,
    pub grip_drag: Option<GripDrag>,
    /// The snapped cursor in model millimetres, and what it snapped to.
    pub cursor: Pt,
    pub snap: Option<Cand>,
    pub ghost: Option<AnnGeometry>,
    pub style_note: String,
    pub units_set_dims: bool,
}

impl AnnoUi {
    pub fn current_layer(&self) -> &str {
        if self.layer.is_empty() {
            anno_glue::LAYER
        } else {
            &self.layer
        }
    }

    pub fn active(&self) -> bool {
        self.session.is_some()
    }
}

// ─── painting ──────────────────────────────────────────────────────────────────────────

fn sp(view: &ViewTransform, p: Pt) -> Pos2 {
    let s = view.to_screen(p);
    pos2(s.x as f32, s.y as f32)
}

/// Paints an annotation's geometry in one colour (the ghost of a command in progress).
pub fn paint_geometry(painter: &Painter, view: &ViewTransform, g: &AnnGeometry, colour: Color32) {
    let stroke = Stroke::new(1.5_f32, colour);
    for seg in &g.segs {
        match seg {
            Seg::Line { a, b } => {
                painter.line_segment([sp(view, *a), sp(view, *b)], stroke);
            }
            Seg::Arc { centre, radius, start_deg, end_deg } => {
                let sweep = if end_deg >= start_deg { end_deg - start_deg } else { end_deg - start_deg + 360.0 };
                let n = ((sweep / 6.0).ceil() as usize).clamp(4, 120);
                let pts: Vec<Pos2> = (0..=n)
                    .map(|i| {
                        let a = (start_deg + sweep * i as f64 / n as f64).to_radians();
                        sp(view, pt(centre.x + radius * a.cos(), centre.y + radius * a.sin()))
                    })
                    .collect();
                painter.add(Shape::line(pts, stroke));
            }
            Seg::Poly { pts, closed } => {
                let pts: Vec<Pos2> = pts.iter().map(|p| sp(view, *p)).collect();
                painter.add(if *closed { Shape::closed_line(pts, stroke) } else { Shape::line(pts, stroke) });
            }
        }
    }
    for f in &g.fills {
        if f.pts.len() >= 3 && !matches!(f.kind, ann::FillKind::Mask) {
            painter.add(Shape::convex_polygon(f.pts.iter().map(|p| sp(view, *p)).collect(), colour, Stroke::NONE));
        }
    }
    for l in &g.labels {
        let px = ((l.height * view.scale) as f32 * 1.35).clamp(5.0, 80.0);
        let anchor = match l.align {
            afrobim_sheets::Align::Left => Align2::LEFT_BOTTOM,
            afrobim_sheets::Align::Centre => Align2::CENTER_BOTTOM,
            afrobim_sheets::Align::Right => Align2::RIGHT_BOTTOM,
        };
        painter.text(sp(view, l.at), anchor, &l.text, FontId::proportional(px), colour);
    }
}

fn snap_marker(painter: &Painter, at: Pos2, mode: SnapMode) {
    let st = Stroke::new(2.0_f32, SNAP_COL);
    let r = 6.0;
    match mode {
        SnapMode::Endpoint => {
            painter.rect_stroke(egui::Rect::from_center_size(at, egui::vec2(2.0 * r, 2.0 * r)), 0.0, st, egui::StrokeKind::Middle);
        }
        SnapMode::Midpoint => {
            painter.add(Shape::closed_line(vec![at + egui::vec2(0.0, -r), at + egui::vec2(r, r * 0.8), at + egui::vec2(-r, r * 0.8)], st));
        }
        SnapMode::Centre => {
            painter.circle_stroke(at, r, st);
        }
        SnapMode::Quadrant => {
            painter.add(Shape::closed_line(vec![at + egui::vec2(0.0, -r), at + egui::vec2(r, 0.0), at + egui::vec2(0.0, r), at + egui::vec2(-r, 0.0)], st));
        }
        SnapMode::Intersection => {
            painter.line_segment([at + egui::vec2(-r, -r), at + egui::vec2(r, r)], st);
            painter.line_segment([at + egui::vec2(-r, r), at + egui::vec2(r, -r)], st);
        }
        SnapMode::Node => {
            painter.circle_stroke(at, r * 0.6, st);
            painter.line_segment([at + egui::vec2(-r, -r), at + egui::vec2(r, r)], st);
            painter.line_segment([at + egui::vec2(-r, r), at + egui::vec2(r, -r)], st);
        }
        SnapMode::Grid => {
            painter.line_segment([at + egui::vec2(-r, 0.0), at + egui::vec2(r, 0.0)], st);
            painter.line_segment([at + egui::vec2(0.0, -r), at + egui::vec2(0.0, r)], st);
        }
    }
}

impl AfroBimApp {
    /// What a command in progress, a grip drag and object snap add on top of the scene.
    pub(crate) fn paint_anno(&self, painter: &Painter, cursor_inside: bool) {
        let a = &self.anno;
        if let Some(g) = &a.ghost {
            paint_geometry(painter, &self.view, g, GHOST);
        }
        if let Some(s) = &a.session {
            // The points picked so far, joined by a rubber line to the cursor.
            let pts = s.points();
            for p in pts {
                painter.circle_filled(sp(&self.view, *p), 3.0, GHOST);
            }
            if let (Some(last), true) = (pts.last(), cursor_inside) {
                painter.add(Shape::dashed_line(&[sp(&self.view, *last), sp(&self.view, a.cursor)], Stroke::new(1.0_f32, GHOST.gamma_multiply(0.7)), 5.0, 3.0));
            }
        }
        if cursor_inside && (a.session.is_some() || a.grip_drag.is_some() || self.ctl.wants_object_snap()) {
            if let Some(c) = &a.snap {
                let at = sp(&self.view, c.p);
                snap_marker(painter, at, c.mode);
                let galley = painter.layout_no_wrap(format!("{} ({})", c.mode.label(), c.from), FontId::proportional(11.0), Color32::from_gray(240));
                let r = egui::Rect::from_min_size(at + egui::vec2(12.0, 10.0), galley.size()).expand2(egui::vec2(5.0, 2.0));
                painter.rect_filled(r, 3.0, Color32::from_rgba_unmultiplied(16, 18, 22, 230));
                painter.galley(r.min + egui::vec2(5.0, 2.0), galley, Color32::WHITE);
            }
            if let Some(s) = &a.session {
                let galley = painter.layout_no_wrap(s.prompt(&self.doc), FontId::proportional(11.5), Color32::from_gray(240));
                let at = sp(&self.view, a.cursor) + egui::vec2(16.0, 30.0);
                let r = egui::Rect::from_min_size(at, galley.size()).expand2(egui::vec2(6.0, 3.0));
                painter.rect_filled(r, 3.0, Color32::from_rgba_unmultiplied(16, 18, 22, 230));
                painter.galley(at, galley, Color32::WHITE);
            }
        }
    }

    /// Grips of the selected annotations: their definition points, drawn as squares.
    pub(crate) fn paint_anno_grips(&self, painter: &Painter) {
        for id in self.doc.selection() {
            let Some(e) = self.doc.find(*id) else { continue };
            if !ann::is_annotation(e) {
                continue;
            }
            for (i, g) in anno_glue::grips_of(&self.doc, *id).iter().enumerate() {
                let s = sp(&self.view, *g);
                let hot = self.anno.grip_hover == Some((*id, i)) || self.anno.grip_drag.as_ref().is_some_and(|d| d.id == *id && d.index == i);
                let r = egui::Rect::from_center_size(pos2(s.x.round(), s.y.round()), egui::vec2(10.0, 10.0));
                painter.rect_filled(r, 0.0, if hot { GRIP_HOT } else { GRIP_FILL });
                painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::WHITE), egui::StrokeKind::Inside);
            }
        }
    }

    // ─── the point under the cursor ─────────────────────────────────────────────────────

    /// Every object snap candidate near `raw` (model mm): the model (an element with a drawing by
    /// its lines and arcs, otherwise its bounding rectangle), the underlays and the grid, for the
    /// modes that are on. The one source for the annotation commands, grips, MOVE, COPY and
    /// placing a sign.
    pub(crate) fn osnap_candidates(&self, raw: Pt) -> Vec<Cand> {
        let radius = SNAP_REACH_PX / self.view.scale;
        let extra = self.under.snaps_near(&self.doc, raw, radius);
        let grid = self.grid_on.then(|| display_spacing(self.grid_spacing, self.view.scale, 14.0));
        let drawn = |id: afrobim_kernel::ElementId| self.cache.get(&id).and_then(|c| c.generated.as_ref().ok()).map(|g| &g.drawing);
        anno_glue::candidates_with(&self.doc, raw, radius, &self.anno.osnap, grid, &extra, &drawn)
    }

    /// The point a click here would give: ortho and grid snap, then object snap over both.
    fn anno_snap_point(&mut self, raw: Pt) -> Pt {
        let last = self.anno.session.as_ref().and_then(|s| s.points().last().copied());
        let mut p = raw;
        if let (true, Some(l)) = (self.ctl.ortho_on, last) {
            p = ortho(l, p);
        }
        if self.ctl.snap_on {
            p = snap(p, self.ctl.snap_spacing);
        }
        // The marker is the annotation command's own here; MOVE, COPY and placing set theirs in
        // `ctl_object_snap`, which runs before this on the same event.
        if self.anno.session.is_some() || self.anno.grip_drag.is_some() {
            self.anno.snap = None;
        }
        if self.anno.osnap.on && (self.anno.session.is_some() || self.anno.grip_drag.is_some()) {
            let mut cands = self.osnap_candidates(raw);
            if let Some(d) = &self.anno.grip_drag {
                // A grip does not snap to its own annotation.
                let own: Vec<Pt> = anno_glue::grips_of(&self.doc, d.id);
                cands.retain(|c| !(c.mode == SnapMode::Node && own.iter().any(|g| dist(*g, c.p) < 1e-6)));
            }
            if let Some(c) = anno_glue::best(cands, raw) {
                p = c.p;
                self.anno.snap = Some(c);
            }
        }
        p
    }

    /// Object snap for MOVE, COPY, a dragged grip and a sign hung from the cursor: the winning
    /// candidate under the pointer, never one of the elements being moved (they would snap to
    /// themselves), handed to the controller as the point picked. Called with the screen point
    /// before every pointer event the controller gets. Clears the marker when there is no
    /// candidate, object snap is off or the controller is not waiting for a point.
    pub(crate) fn ctl_object_snap(&mut self, screen: Pt) {
        if self.anno.session.is_some() || self.anno.grip_drag.is_some() {
            return;
        }
        self.anno.snap = None;
        self.ctl.set_object_snap(None);
        if !self.anno.osnap.on || !self.ctl.wants_object_snap() {
            return;
        }
        let raw = self.view.to_world(screen);
        let moving = self.ctl.ghost_ids().to_vec();
        let mut cands = self.osnap_candidates(raw);
        cands.retain(|c| !c.owner.is_some_and(|o| moving.contains(&o)));
        if let Some(c) = anno_glue::best(cands, raw) {
            self.ctl.set_object_snap(Some(c.p));
            self.anno.snap = Some(c);
        }
    }

    /// The pointer moved over the 2D canvas.
    pub(crate) fn anno_pointer(&mut self, screen: Pt) {
        let raw = self.view.to_world(screen);
        let p = self.anno_snap_point(raw);
        self.anno.cursor = p;
        self.anno.ghost = None;
        self.anno.grip_hover = None;
        if let Some(s) = &self.anno.session {
            self.anno.ghost = s.preview(&self.doc, p);
        } else if let Some(d) = &self.anno.grip_drag {
            if let Some(e) = self.doc.find(d.id) {
                if let Ok(mut a) = ann::read_absolute(e) {
                    if a.set_grip(d.index, p) {
                        self.anno.ghost = a.geometry().ok();
                    }
                }
            }
        } else {
            let reach = GRIP_REACH_PX / self.view.scale;
            'outer: for id in self.doc.selection().to_vec() {
                for (i, g) in anno_glue::grips_of(&self.doc, id).iter().enumerate() {
                    if dist(*g, raw) <= reach {
                        self.anno.grip_hover = Some((id, i));
                        break 'outer;
                    }
                }
            }
        }
    }

    /// A left press on the canvas. True when annotation took it (a point for the command, or the
    /// start of a grip drag).
    pub(crate) fn anno_press(&mut self, screen: Pt, shift: bool) -> bool {
        self.anno_pointer(screen);
        if self.anno.session.is_some() {
            let p = self.anno.cursor;
            let radius = 6.0 / self.view.scale;
            let out = match self.anno.session.as_mut() {
                Some(s) => s.point(&mut self.doc, p, radius),
                None => return true,
            };
            self.anno_apply(out);
            return true;
        }
        if let (Some((id, index)), false) = (self.anno.grip_hover, shift) {
            self.anno.grip_drag = Some(GripDrag { id, index });
            return true;
        }
        false
    }

    /// The button came up: a grip drag ends where the pointer is.
    pub(crate) fn anno_release(&mut self, screen: Pt) {
        let Some(d) = self.anno.grip_drag.take() else { return };
        self.anno.ghost = None;
        let _ = screen;
        let p = self.anno.cursor;
        match anno_glue::move_grip(&mut self.doc, d.id, d.index, p) {
            Ok(()) => {
                self.sync_generated();
                self.schedule_commit_now();
                self.status = "Annotation point moved.".into();
            }
            Err(e) => self.status = format!("Not moved: {e}"),
        }
    }

    /// Right button or Enter during a command.
    pub(crate) fn anno_enter(&mut self) {
        let out = match self.anno.session.as_mut() {
            Some(s) => s.enter(&mut self.doc),
            None => return,
        };
        self.anno_apply(out);
    }

    /// Typed text on the command line during a command.
    pub(crate) fn anno_text(&mut self, text: &str) {
        let out = match self.anno.session.as_mut() {
            Some(s) => s.text(&mut self.doc, text),
            None => return,
        };
        self.anno_apply(out);
    }

    pub(crate) fn anno_cancel(&mut self) -> bool {
        self.anno.ghost = None;
        self.anno.grip_drag = None;
        if self.anno.session.take().is_some() {
            self.log("*Cancel*");
            true
        } else {
            false
        }
    }

    /// Takes what a command returned: history lines, measure results, the undo step.
    pub(crate) fn anno_apply(&mut self, out: Out) {
        for l in &out.echo {
            self.log(l.clone());
        }
        for r in &out.results {
            self.anno.results.push(r.clone());
            self.anno.results_open = true;
        }
        if let Some(l) = out.echo.last() {
            self.status = l.clone();
        }
        if let Some(id) = out.created {
            if self.doc.find(id).is_some_and(|e| e.category == ann::DIM_CATEGORY) {
                self.anno.last_dim = Some(id);
            }
        }
        if out.changed {
            self.sync_generated();
            self.schedule_commit_now();
        }
        if out.finished {
            self.anno.session = None;
            self.anno.ghost = None;
            if out.echo.is_empty() {
                self.status = "Ready.".into();
            }
        } else if let Some(s) = &self.anno.session {
            let prompt = s.prompt(&self.doc);
            self.status = prompt;
        }
    }

    /// Starts a text, dimension, leader or measure command.
    pub(crate) fn anno_start(&mut self, kind: Kind) {
        if self.view_3d || self.design.plan_on {
            self.status = "Annotation is made in the 2D model view. Switch to 2D first.".into();
            return;
        }
        if self.ctl.is_busy() {
            let _ = self.ctl.escape(&mut self.doc);
        }
        self.anno.grip_drag = None;
        let layer = self.anno.current_layer().to_string();
        if !self.doc.layers().iter().any(|l| l.name == layer) {
            self.doc.ensure_layer(&layer);
        }
        match Session::start(kind, &self.doc, &layer, self.anno.last_dim) {
            Ok(s) => {
                self.log(format!("Command: {}", crate::commands::COMMANDS.iter().find(|c| c.action == crate::commands::Action::Anno(kind)).map(|c| c.name).unwrap_or("")));
                if matches!(kind, Kind::DimStation | Kind::Id) {
                    self.show_alignments(true);
                }
                self.status = s.prompt(&self.doc);
                self.anno.session = Some(s);
            }
            Err(e) => self.status = e,
        }
    }

    /// The alignments layer is off by default (an alignment is drawn in site coordinates, far
    /// from the sign sheet). Station commands turn it on and, the first time, bring it into view.
    pub(crate) fn show_alignments(&mut self, zoom: bool) {
        let was_hidden = self.doc.layers().iter().any(|l| l.name == alignment_glue::LAYER && !l.visible);
        if let Some(l) = self.doc.layers_mut().iter_mut().find(|l| l.name == alignment_glue::LAYER) {
            l.visible = true;
        }
        if zoom && was_hidden {
            let b = self.doc.elements().iter().filter(|e| e.category == alignment_glue::CATEGORY).map(|e| e.bounds()).reduce(|a, b| a.union(&b));
            if let Some(b) = b {
                let (l, t, w, h) = self.ctl.viewport;
                self.view.fit(&b, l, t, w, h, 60.0);
            }
            self.log("The Alignments layer is now on and in view: station commands work in site coordinates.");
        }
    }

    pub(crate) fn anno_open_text_edit(&mut self, id: ElementId) -> bool {
        let Some(e) = self.doc.find(id) else { return false };
        let text = match ann::read(e) {
            Ok(Ann::Text(t)) => t.text,
            Ok(Ann::Leader(l)) => match l.text {
                LeaderText::Literal(s) => s,
                LeaderText::Mark { .. } => {
                    self.status = "This leader shows the mark of an element; rename the element to change it.".into();
                    return false;
                }
                LeaderText::None => String::new(),
            },
            _ => {
                self.status = "TEXTEDIT edits text and leaders. A dimension's text is its override in Properties.".into();
                return false;
            }
        };
        self.anno.edit = Some((id, text));
        true
    }

    /// The model-wide refresh after anything that can change what annotation reads from the rest
    /// of the document: names, erased elements, alignments, undo.
    pub(crate) fn anno_refresh(&mut self) -> bool {
        if !self.doc.elements().iter().any(|e| matches!(e.category.as_str(), ann::DIM_CATEGORY | ann::LEADER_CATEGORY)) {
            return false;
        }
        ann::refresh(&mut self.doc).updated > 0
    }

    // ─── windows ────────────────────────────────────────────────────────────────────────

    pub(crate) fn anno_windows(&mut self, ctx: &egui::Context) {
        self.anno_results_window(ctx);
        self.anno_osnap_window(ctx);
        self.anno_layers_window(ctx);
        self.anno_units_window(ctx);
        self.anno_text_styles_window(ctx);
        self.anno_dim_styles_window(ctx);
        self.anno_edit_window(ctx);
    }

    fn anno_results_window(&mut self, ctx: &egui::Context) {
        if !self.anno.results_open {
            return;
        }
        let mut open = true;
        let mut clear = false;
        crate::win::new(ctx, "Measure results").open(&mut open).default_width(560.0).show(ctx, |ui| {
            if self.anno.results.is_empty() {
                ui.label(RichText::new("Nothing measured yet. Use DIST, AREA, ID, MEASUREANGLE or MEASURERADIUS.").weak());
            }
            egui::ScrollArea::vertical().max_height(260.0).stick_to_bottom(true).show(ui, |ui| {
                for (i, r) in self.anno.results.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("{}.", i + 1)).weak());
                        ui.label(RichText::new(r).monospace());
                        if ui.small_button("Copy").clicked() {
                            ui.ctx().copy_text(r.clone());
                        }
                    });
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Copy all").clicked() {
                    ui.ctx().copy_text(self.anno.results.join("\n"));
                }
                if ui.button("Clear").clicked() {
                    clear = true;
                }
            });
            ui.label(RichText::new("Results use the document's units (UNITS). Stored values stay in millimetres.").small().weak());
        });
        if clear {
            self.anno.results.clear();
        }
        self.anno.results_open = open;
    }

    fn anno_osnap_window(&mut self, ctx: &egui::Context) {
        if !self.anno.osnap_open {
            return;
        }
        let mut open = true;
        crate::win::new(ctx, "Object snap settings").open(&mut open).default_width(560.0).show(ctx, |ui| {
            ui.checkbox(&mut self.anno.osnap.on, "Object snap on (F3)");
            ui.label(RichText::new("Used by the text, dimension, leader and measure commands and when dragging an annotation grip. Pick a mode to see which elements provide it.").small().weak());
            ui.add_space(4.0);
            egui::Grid::new("osnap-modes").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                for m in SnapMode::ALL {
                    let mut on = self.anno.osnap.has(m);
                    if ui.checkbox(&mut on, m.label()).changed() {
                        self.anno.osnap.set(m, on);
                    }
                    ui.label(RichText::new(m.provided_by()).small().weak());
                    ui.end_row();
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new("Limits: elements other than alignments, annotations and underlays give snaps from the bounding rectangle of their drawing, not from the artwork inside it. Nothing snaps on a hidden layer. Snap does not yet apply to MOVE, COPY or the signs commands.").small());
        });
        self.anno.osnap_open = open;
    }

    fn anno_layers_window(&mut self, ctx: &egui::Context) {
        if !self.anno.layers_open {
            return;
        }
        let mut open = true;
        let mut committed = false;
        crate::win::new(ctx, "Layer manager").open(&mut open).default_width(700.0).show(ctx, |ui| {
            let rows: Vec<ann::LayerInfo> = ann::layers::list(&self.doc).into_iter().filter(|l| !hidden_layer(&l.name)).collect();
            let mut msg: Option<String> = None;
            egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                egui::Grid::new("layers-grid").num_columns(8).spacing([10.0, 4.0]).striped(true).show(ui, |ui| {
                    for h in ["Current", "Name", "On", "Lock", "Colour", "Line weight (mm)", "Elements", ""] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for l in &rows {
                        let current = self.anno.current_layer() == l.name;
                        if ui.radio(current, "").on_hover_text("Make this the current layer: new annotation goes on it").clicked() {
                            self.anno.layer = l.name.clone();
                        }
                        match self.anno.layer_win.rename.clone() {
                            Some((old, mut new)) if old == l.name => {
                                let mut done = false;
                                ui.horizontal(|ui| {
                                    ui.add(egui::TextEdit::singleline(&mut new).desired_width(120.0));
                                    if ui.small_button("OK").clicked() {
                                        done = true;
                                    }
                                });
                                if done {
                                    match ann::layers::rename(&mut self.doc, &old, &new) {
                                        Ok(()) => {
                                            if self.anno.layer == old {
                                                self.anno.layer = new.clone();
                                            }
                                            committed = true;
                                            msg = Some(format!("Renamed {old} to {new}; its elements moved with it."));
                                        }
                                        Err(e) => msg = Some(e.to_string()),
                                    }
                                    self.anno.layer_win.rename = None;
                                } else {
                                    self.anno.layer_win.rename = Some((old, new));
                                }
                            }
                            _ => {
                                ui.label(&l.name);
                            }
                        }
                        let mut on = l.visible;
                        if ui.checkbox(&mut on, "").changed() {
                            let _ = ann::layers::set_visible(&mut self.doc, &l.name, on);
                        }
                        let mut locked = l.locked;
                        if ui.checkbox(&mut locked, "").changed() {
                            let _ = ann::layers::set_locked(&mut self.doc, &l.name, locked);
                        }
                        ui.horizontal(|ui| {
                            let mut rgb = l.colour;
                            if ui.color_edit_button_srgb(&mut rgb).changed() {
                                let _ = ann::layers::set_colour(&mut self.doc, &l.name, rgb);
                                committed = true;
                            }
                            if l.colour == [0, 0, 0] {
                                ui.label(RichText::new("auto").small().weak());
                            } else if ui.small_button("auto").on_hover_text("Back to the canvas ink colour").clicked() {
                                let _ = ann::layers::set_colour(&mut self.doc, &l.name, [0, 0, 0]);
                                committed = true;
                            }
                        });
                        let mut w = l.lineweight_mm;
                        if ui.add(egui::DragValue::new(&mut w).speed(0.05).range(0.0..=2.11).fixed_decimals(2)).changed() {
                            let _ = ann::layers::set_lineweight(&mut self.doc, &l.name, w);
                            committed = true;
                        }
                        ui.label(l.elements.to_string());
                        ui.horizontal(|ui| {
                            if ui.small_button("Rename").clicked() {
                                self.anno.layer_win.rename = Some((l.name.clone(), l.name.clone()));
                            }
                            if ui.small_button("Delete").clicked() {
                                let target = rows.iter().find(|r| r.name != l.name).map(|r| r.name.clone()).unwrap_or_default();
                                self.anno.layer_win.delete = Some((l.name.clone(), target));
                            }
                        });
                        ui.end_row();
                    }
                });
            });
            if let Some((name, target)) = self.anno.layer_win.delete.clone() {
                let count = rows.iter().find(|r| r.name == name).map(|r| r.elements).unwrap_or(0);
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if count == 0 {
                        ui.label(format!("Delete the empty layer {name}?"));
                    } else {
                        ui.label(format!("{name} holds {count} element(s). A layer is deleted only when it is empty, or its elements are moved to:"));
                        let mut t = target.clone();
                        egui::ComboBox::from_id_salt("layer-move-target").selected_text(t.clone()).show_ui(ui, |ui| {
                            for r in rows.iter().filter(|r| r.name != name) {
                                ui.selectable_value(&mut t, r.name.clone(), &r.name);
                            }
                        });
                        if t != target {
                            self.anno.layer_win.delete = Some((name.clone(), t));
                        }
                    }
                    if ui.button(if count == 0 { "Delete" } else { "Move the elements and delete" }).clicked() {
                        let policy = if count == 0 { ann::OnDelete::RefuseUnlessEmpty } else { ann::OnDelete::MoveTo(self.anno.layer_win.delete.as_ref().map(|d| d.1.clone()).unwrap_or_default()) };
                        match ann::layers::delete(&mut self.doc, &name, &policy) {
                            Ok(moved) => {
                                msg = Some(format!("Deleted layer {name}{}.", if moved > 0 { format!(", {moved} element(s) moved") } else { String::new() }));
                                if self.anno.layer == name {
                                    self.anno.layer.clear();
                                }
                                committed = true;
                            }
                            Err(e) => msg = Some(e.to_string()),
                        }
                        self.anno.layer_win.delete = None;
                    }
                    if ui.button("Keep it").clicked() {
                        self.anno.layer_win.delete = None;
                    }
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("New layer");
                ui.add(egui::TextEdit::singleline(&mut self.anno.layer_win.new_name).desired_width(160.0).hint_text("name"));
                if ui.button("Create").clicked() {
                    let name = std::mem::take(&mut self.anno.layer_win.new_name);
                    match ann::layers::create(&mut self.doc, &name, LayerProps::default()) {
                        Ok(()) => {
                            committed = true;
                            msg = Some(format!("Layer {name} created."));
                        }
                        Err(e) => {
                            self.anno.layer_win.new_name = name;
                            msg = Some(e.to_string());
                        }
                    }
                }
            });
            if let Some(m) = msg {
                self.anno.layer_win.message = m;
            }
            if !self.anno.layer_win.message.is_empty() {
                ui.label(RichText::new(&self.anno.layer_win.message).small());
            }
            ui.label(RichText::new(format!("Current layer: {}. Colour auto means the canvas ink. On and lock are not part of undo (the kernel keeps them outside its history); name, colour, line weight, create and delete are.", self.anno.current_layer())).small().weak());
            ui.label(RichText::new("Colour and line weight show on the 2D canvas (the weight as a pen width in pixels) and in the DXF for annotation; DXF R12 carries no line weight. Layer 0 and the built-in layers cannot be renamed or deleted.").small().weak());
        });
        if committed {
            self.sync_generated();
            self.schedule_commit_now();
        }
        self.anno.layers_open = open;
    }

    fn anno_units_window(&mut self, ctx: &egui::Context) {
        if !self.anno.units_open {
            return;
        }
        let mut open = true;
        let mut st = ann::settings::load(&self.doc);
        let before = st.units.clone();
        let mut set_dims = self.anno.units_set_dims;
        crate::win::new(ctx, "Units").open(&mut open).default_width(480.0).show(ctx, |ui| {
            let u = &mut st.units;
            egui::Grid::new("units-grid").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                ui.label("Length");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut u.length, ann::LengthUnit::Millimetre, "millimetres");
                    ui.radio_value(&mut u.length, ann::LengthUnit::Metre, "metres");
                });
                ui.end_row();
                ui.label("Length decimals");
                ui.add(egui::DragValue::new(&mut u.length_decimals).range(0..=6));
                ui.end_row();
                ui.label("Station decimals");
                ui.add(egui::DragValue::new(&mut u.station_decimals).range(0..=6));
                ui.end_row();
                ui.label("Area");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut u.area, ann::AreaUnit::SquareMetre, "square metres");
                    ui.radio_value(&mut u.area, ann::AreaUnit::Hectare, "hectares");
                });
                ui.end_row();
                ui.label("Area decimals");
                ui.add(egui::DragValue::new(&mut u.area_decimals).range(0..=6));
                ui.end_row();
                ui.label("Volume (cubic metres) decimals");
                ui.add(egui::DragValue::new(&mut u.volume_decimals).range(0..=6));
                ui.end_row();
                ui.label("Angle");
                egui::ComboBox::from_id_salt("angle-format").selected_text(angle_label(u.angle)).show_ui(ui, |ui| {
                    for f in [ann::AngleFormat::Decimal, ann::AngleFormat::Dms, ann::AngleFormat::QuadrantBearing, ann::AngleFormat::Azimuth] {
                        ui.selectable_value(&mut u.angle, f, angle_label(f));
                    }
                });
                ui.end_row();
                ui.label("Angle decimals");
                ui.add(egui::DragValue::new(&mut u.angle_decimals).range(0..=6));
                ui.end_row();
            });
            ui.checkbox(&mut set_dims, "Also set the dimension styles to these units");
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(RichText::new("Examples").strong());
                ui.label(format!("12 345.6789 mm is shown as {}", u.length_u(12_345.6789)));
                ui.label(format!("Station 1250.5 m is shown as {}", u.station(1250.5)));
                ui.label(format!("1 234 567 890 mm2 is shown as {}", u.area_u(1_234_567_890.0)));
                ui.label(format!("2 500 000 000 mm3 is shown as {} m3", u.volume(2_500_000_000.0)));
                ui.label(format!("37.5 degrees is shown as {}", u.angle(37.5)));
                ui.label(format!("A direction of 37.5 degrees is shown as {}", u.direction(37.5)));
            });
            ui.label(RichText::new("Typed lengths, stations, areas and angles are read in these formats (12.5, 12.5m, 12500mm, 1+250.500, 45d30m15s). Stored values stay in millimetres.").small().weak());
            ui.label(RichText::new("Applied to: the status bar coordinates, Properties position and size, measure results, dimension text (styles), the typed point input of annotation commands and the station fields. Still raw: see the README.").small().weak());
        });
        self.anno.units_set_dims = set_dims;
        if st.units != before {
            if set_dims {
                for d in &mut st.dim_styles {
                    d.length_unit = st.units.length;
                    d.length_decimals = st.units.length_decimals;
                    d.angle_format = st.units.angle;
                    d.angle_decimals = st.units.angle_decimals;
                    d.station_decimals = st.units.station_decimals;
                }
            }
            ann::settings::store(&mut self.doc, &st);
            if set_dims {
                ann::settings::restyle(&mut self.doc);
            }
            self.sync_generated();
            self.schedule_commit_now();
        }
        self.anno.units_open = open;
    }

    fn anno_text_styles_window(&mut self, ctx: &egui::Context) {
        if !self.anno.text_styles_open {
            return;
        }
        let mut open = true;
        let mut st = ann::settings::load(&self.doc);
        let before = st.clone();
        let mut apply = false;
        crate::win::new(ctx, "Text styles").open(&mut open).default_width(520.0).show(ctx, |ui| {
            self.anno.text_sel = self.anno.text_sel.min(st.text_styles.len().saturating_sub(1));
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("text-style-pick").selected_text(st.text_styles[self.anno.text_sel].name.clone()).show_ui(ui, |ui| {
                    for (i, s) in st.text_styles.iter().enumerate() {
                        ui.selectable_value(&mut self.anno.text_sel, i, &s.name);
                    }
                });
                if ui.button("New from this").clicked() {
                    let mut n = st.text_styles[self.anno.text_sel].clone();
                    n.name = unique_style_name(&st.text_styles.iter().map(|s| s.name.clone()).collect::<Vec<_>>(), &n.name);
                    st.text_styles.push(n);
                    self.anno.text_sel = st.text_styles.len() - 1;
                }
                if ui.button("Delete").clicked() {
                    let name = st.text_styles[self.anno.text_sel].name.clone();
                    if !st.remove_text_style(&name) {
                        self.anno.style_note = "The last text style cannot be deleted.".into();
                    }
                    self.anno.text_sel = 0;
                }
            });
            let current_name = st.current_text_style.clone();
            let s: &mut TextStyle = &mut st.text_styles[self.anno.text_sel];
            let is_current = s.name == current_name;
            let mut make_current = false;
            egui::Grid::new("text-style-grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(160.0));
                ui.end_row();
                ui.label("Height (paper mm)");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut s.height_paper_mm).speed(0.1).range(0.1..=50.0));
                    ui.label(RichText::new("a default, not a standard").small().weak());
                });
                ui.end_row();
                ui.label("Width factor");
                ui.add(egui::DragValue::new(&mut s.width_factor).speed(0.01).range(0.2..=5.0));
                ui.end_row();
                ui.label("Line spacing (x height)");
                ui.add(egui::DragValue::new(&mut s.line_spacing).speed(0.05).range(1.0..=4.0));
                ui.end_row();
                ui.label("Bold");
                ui.checkbox(&mut s.bold, "");
                ui.end_row();
                ui.label("Font name");
                ui.add(egui::TextEdit::singleline(&mut s.font).desired_width(160.0));
                ui.end_row();
            });
            if !is_current && ui.button("Make current").clicked() {
                make_current = true;
            }
            if is_current {
                ui.label(RichText::new("This is the current style.").small());
            }
            if make_current {
                st.current_text_style = st.text_styles[self.anno.text_sel].name.clone();
            }
            ui.add_space(4.0);
            if ui.button("Apply to existing text").on_hover_text("Text keeps the style it was made with until you apply a changed style").clicked() {
                apply = true;
            }
            ui.label(RichText::new(format!("Annotation scale 1:{}: a paper height of {:.1} mm is {:.0} mm in the model. Sizes are paper millimetres, multiplied by the annotation scale. Every size here is an editable default and no drafting standard is claimed. Layout uses Helvetica widths whatever font is named.", st.annotation_scale, st.text_styles[self.anno.text_sel].height_paper_mm, st.text_styles[self.anno.text_sel].height_paper_mm * st.annotation_scale)).small().weak());
            if !self.anno.style_note.is_empty() {
                ui.label(RichText::new(&self.anno.style_note).small().color(WARN));
            }
        });
        if st != before {
            ann::settings::store(&mut self.doc, &st);
            self.schedule_commit_now();
        }
        if apply {
            let n = ann::settings::restyle(&mut self.doc);
            self.status = format!("{n} annotation(s) restyled.");
            self.sync_generated();
            self.schedule_commit_now();
        }
        self.anno.text_styles_open = open;
    }

    fn anno_dim_styles_window(&mut self, ctx: &egui::Context) {
        if !self.anno.dim_styles_open {
            return;
        }
        let mut open = true;
        let mut st = ann::settings::load(&self.doc);
        let before = st.clone();
        let mut apply = false;
        crate::win::new(ctx, "Dimension styles").open(&mut open).default_width(560.0).default_height(560.0).show(ctx, |ui| {
            self.anno.dim_sel = self.anno.dim_sel.min(st.dim_styles.len().saturating_sub(1));
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("dim-style-pick").selected_text(st.dim_styles[self.anno.dim_sel].name.clone()).show_ui(ui, |ui| {
                    for (i, s) in st.dim_styles.iter().enumerate() {
                        ui.selectable_value(&mut self.anno.dim_sel, i, &s.name);
                    }
                });
                if ui.button("New from this").clicked() {
                    let mut n = st.dim_styles[self.anno.dim_sel].clone();
                    n.name = unique_style_name(&st.dim_styles.iter().map(|s| s.name.clone()).collect::<Vec<_>>(), &n.name);
                    st.dim_styles.push(n);
                    self.anno.dim_sel = st.dim_styles.len() - 1;
                }
                if ui.button("Delete").clicked() {
                    let name = st.dim_styles[self.anno.dim_sel].name.clone();
                    if !st.remove_dim_style(&name) {
                        self.anno.style_note = "The last dimension style cannot be deleted.".into();
                    }
                    self.anno.dim_sel = 0;
                }
            });
            let current_name = st.current_dim_style.clone();
            let scale = st.annotation_scale;
            let text_style_names: Vec<String> = st.text_styles.iter().map(|s| s.name.clone()).collect();
            let s: &mut DimStyle = &mut st.dim_styles[self.anno.dim_sel];
            let is_current = s.name == current_name;
            let mut make_current = false;
            egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                ui.label(RichText::new("Sizes (paper millimetres, defaults to edit; the annotation scale multiplies them)").strong());
                egui::Grid::new("dim-style-sizes").num_columns(2).spacing([12.0, 5.0]).show(ui, |ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(160.0));
                    ui.end_row();
                    for (label, v) in [
                        ("Text height", &mut s.text_height_mm),
                        ("Text gap", &mut s.text_gap_mm),
                        ("Arrow size", &mut s.arrow_size_mm),
                        ("Extension line offset", &mut s.ext_offset_mm),
                        ("Extension line extension", &mut s.ext_extend_mm),
                        ("Baseline spacing", &mut s.baseline_spacing_mm),
                        ("Centre mark size (0 for none)", &mut s.centre_mark_mm),
                    ] {
                        ui.label(label);
                        ui.add(egui::DragValue::new(v).speed(0.05).range(0.0..=50.0));
                        ui.end_row();
                    }
                    ui.label("Terminator");
                    egui::ComboBox::from_id_salt("dim-term").selected_text(format!("{:?}", s.terminator)).show_ui(ui, |ui| {
                        for t in [ann::Terminator::Arrow, ann::Terminator::Tick, ann::Terminator::Dot, ann::Terminator::None] {
                            ui.selectable_value(&mut s.terminator, t, format!("{t:?}"));
                        }
                    });
                    ui.end_row();
                    ui.label("Text placement");
                    egui::ComboBox::from_id_salt("dim-place").selected_text(format!("{:?}", s.text_placement)).show_ui(ui, |ui| {
                        for t in [ann::TextPlacement::Above, ann::TextPlacement::Centred] {
                            ui.selectable_value(&mut s.text_placement, t, format!("{t:?}"));
                        }
                    });
                    ui.end_row();
                    ui.label("Text style");
                    egui::ComboBox::from_id_salt("dim-textstyle").selected_text(s.text_style.clone()).show_ui(ui, |ui| {
                        for n in &text_style_names {
                            ui.selectable_value(&mut s.text_style, n.clone(), n);
                        }
                    });
                    ui.end_row();
                    ui.label("Width factor");
                    ui.add(egui::DragValue::new(&mut s.width_factor).speed(0.01).range(0.2..=5.0));
                    ui.end_row();
                    ui.label("Bold");
                    ui.checkbox(&mut s.bold, "");
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Value text").strong());
                egui::Grid::new("dim-style-text").num_columns(2).spacing([12.0, 5.0]).show(ui, |ui| {
                    ui.label("Length unit");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut s.length_unit, ann::LengthUnit::Millimetre, "mm");
                        ui.radio_value(&mut s.length_unit, ann::LengthUnit::Metre, "m");
                    });
                    ui.end_row();
                    ui.label("Length decimals");
                    ui.add(egui::DragValue::new(&mut s.length_decimals).range(0..=6));
                    ui.end_row();
                    ui.label("Scale factor");
                    ui.add(egui::DragValue::new(&mut s.scale_factor).speed(0.01));
                    ui.end_row();
                    ui.label("Prefix and suffix");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut s.prefix).desired_width(60.0));
                        ui.add(egui::TextEdit::singleline(&mut s.suffix).desired_width(60.0));
                    });
                    ui.end_row();
                    ui.label("Strip trailing zeros");
                    ui.checkbox(&mut s.strip_trailing_zeros, "");
                    ui.end_row();
                    ui.label("Angle format and decimals");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("dim-angle").selected_text(angle_label(s.angle_format)).show_ui(ui, |ui| {
                            for f in [ann::AngleFormat::Decimal, ann::AngleFormat::Dms] {
                                ui.selectable_value(&mut s.angle_format, f, angle_label(f));
                            }
                        });
                        ui.add(egui::DragValue::new(&mut s.angle_decimals).range(0..=6));
                    });
                    ui.end_row();
                    for (label, v) in [("Radius prefix", &mut s.radius_prefix), ("Diameter prefix", &mut s.diameter_prefix), ("Arc prefix", &mut s.arc_prefix), ("Station prefix", &mut s.station_prefix), ("Offset prefix", &mut s.offset_prefix), ("Left letter", &mut s.left_letter), ("Right letter", &mut s.right_letter), ("Level prefix", &mut s.level_prefix)] {
                        ui.label(label);
                        ui.add(egui::TextEdit::singleline(v).desired_width(90.0));
                        ui.end_row();
                    }
                    ui.label("Level and station decimals");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut s.level_decimals).range(0..=6));
                        ui.add(egui::DragValue::new(&mut s.station_decimals).range(0..=6));
                    });
                    ui.end_row();
                });
            });
            if !is_current && ui.button("Make current").clicked() {
                make_current = true;
            }
            if is_current {
                ui.label(RichText::new("This is the current style.").small());
            }
            if make_current {
                st.current_dim_style = st.dim_styles[self.anno.dim_sel].name.clone();
            }
            ui.add_space(4.0);
            if ui.button("Apply to existing dimensions and leaders").on_hover_text("Dimensions keep the style they were made with until you apply a changed style").clicked() {
                apply = true;
            }
            ui.label(RichText::new(format!("A preview at 1:{scale}: a dimension line with text {:.0} mm high and arrows {:.0} mm long in the model.", st.dim_styles[self.anno.dim_sel].text_height_mm * scale, st.dim_styles[self.anno.dim_sel].arrow_size_mm * scale)).small().weak());
            ui.label(RichText::new("Every size is an editable default; none is taken from a drafting standard. Text override of one dimension is in its Properties and keeps the measured value shown.").small().weak());
            if !self.anno.style_note.is_empty() {
                ui.label(RichText::new(&self.anno.style_note).small().color(WARN));
            }
        });
        if st != before {
            ann::settings::store(&mut self.doc, &st);
            self.schedule_commit_now();
        }
        if apply {
            let n = ann::settings::restyle(&mut self.doc);
            self.status = format!("{n} annotation(s) restyled.");
            self.sync_generated();
            self.schedule_commit_now();
        }
        self.anno.dim_styles_open = open;
    }

    fn anno_edit_window(&mut self, ctx: &egui::Context) {
        let Some((id, mut text)) = self.anno.edit.take() else { return };
        let (mut ok, mut cancel) = (false, false);
        let mark = self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
        crate::win::new(ctx, format!("Edit text of {mark}")).collapsible(false).default_width(420.0).show(ctx, |ui| {
            ui.add(egui::TextEdit::multiline(&mut text).desired_rows(5).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    ok = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if ok {
            match anno_glue::set_text(&mut self.doc, id, &text) {
                Ok(()) => {
                    self.sync_generated();
                    self.schedule_commit_now();
                    self.status = format!("{mark} edited.");
                }
                Err(e) => self.status = e,
            }
        } else if !cancel {
            self.anno.edit = Some((id, text));
        }
    }
}

fn angle_label(f: ann::AngleFormat) -> &'static str {
    match f {
        ann::AngleFormat::Decimal => "decimal degrees",
        ann::AngleFormat::Dms => "degrees, minutes, seconds",
        ann::AngleFormat::QuadrantBearing => "quadrant bearing (directions)",
        ann::AngleFormat::Azimuth => "azimuth (directions)",
    }
}

fn unique_style_name(existing: &[String], base: &str) -> String {
    (2..).map(|i| format!("{base}-{i}")).find(|n| !existing.iter().any(|e| e.eq_ignore_ascii_case(n))).unwrap_or_else(|| format!("{base}-new"))
}

// ─── Properties of an annotation ─────────────────────────────────────────────────────────

const JUSTIFY: [(Justify, &str); 9] = [
    (Justify::TopLeft, "Top left"),
    (Justify::TopCentre, "Top centre"),
    (Justify::TopRight, "Top right"),
    (Justify::MiddleLeft, "Middle left"),
    (Justify::MiddleCentre, "Middle centre"),
    (Justify::MiddleRight, "Middle right"),
    (Justify::BottomLeft, "Bottom left"),
    (Justify::BottomCentre, "Bottom centre"),
    (Justify::BottomRight, "Bottom right"),
];

/// The Definition section of the Properties panel for an annotation. True when something changed.
pub fn edit_ui(ui: &mut Ui, e: &mut Element, st: &AnnotationSettings, alignments: &[String]) -> bool {
    let Ok(mut a) = ann::read(e) else {
        ui.label(RichText::new("The annotation cannot be read.").color(BAD));
        return false;
    };
    let before = a.clone();
    let u = &st.units;
    match &mut a {
        Ann::Text(t) => {
            ui.label("Text");
            ui.add(egui::TextEdit::multiline(&mut t.text).desired_rows(3).desired_width(f32::INFINITY));
            egui::Grid::new("ann-text-grid").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Style");
                egui::ComboBox::from_id_salt("ann-text-style").selected_text(t.style.clone()).show_ui(ui, |ui| {
                    for s in &st.text_styles {
                        if ui.selectable_label(t.style == s.name, &s.name).clicked() {
                            t.style = s.name.clone();
                            t.width_factor = s.width_factor;
                            t.bold = s.bold;
                            t.line_spacing = s.line_spacing;
                            if let ann::HeightSpec::Paper { scale, .. } = t.height {
                                t.height = ann::HeightSpec::Paper { mm: s.height_paper_mm, scale };
                            }
                        }
                    }
                });
                ui.end_row();
                ui.label("Height (model)");
                let mut h = t.height.model_mm();
                if ui.add(egui::DragValue::new(&mut h).speed(1.0).range(0.1..=1.0e6).custom_formatter(|v, _| u.length_u(v)).custom_parser(|s| u.parse_length(s).ok())).changed() {
                    t.height = ann::HeightSpec::Model(h);
                }
                ui.end_row();
                ui.label("Justification");
                egui::ComboBox::from_id_salt("ann-justify").selected_text(JUSTIFY.iter().find(|(j, _)| *j == t.justify).map(|j| j.1).unwrap_or("")).show_ui(ui, |ui| {
                    for (j, label) in JUSTIFY {
                        ui.selectable_value(&mut t.justify, j, label);
                    }
                });
                ui.end_row();
                ui.label("Rotation");
                ui.add(egui::DragValue::new(&mut t.rotation_deg).speed(1.0).suffix(" deg"));
                ui.end_row();
                ui.label("Width factor");
                ui.add(egui::DragValue::new(&mut t.width_factor).speed(0.01).range(0.2..=5.0));
                ui.end_row();
                ui.label("Wrap width");
                ui.horizontal(|ui| {
                    let mut on = t.wrap_width.is_some();
                    if ui.checkbox(&mut on, "").changed() {
                        t.wrap_width = on.then_some(t.block_size().0.max(1000.0));
                    }
                    if let Some(w) = &mut t.wrap_width {
                        ui.add(egui::DragValue::new(w).speed(10.0).range(1.0..=1.0e7).custom_formatter(|v, _| u.length_u(v)).custom_parser(|s| u.parse_length(s).ok()));
                    }
                });
                ui.end_row();
                ui.label("Frame");
                ui.horizontal(|ui| {
                    let mut on = t.frame_offset.is_some();
                    if ui.checkbox(&mut on, "").changed() {
                        t.frame_offset = on.then_some(0.5);
                    }
                    if let Some(f) = &mut t.frame_offset {
                        ui.add(egui::DragValue::new(f).speed(0.05).range(0.0..=5.0).suffix(" heights"));
                    }
                });
                ui.end_row();
            });
        }
        Ann::Dim(d) => {
            ui.label(RichText::new(ann::element::dim_kind_name(&d.kind)).strong());
            let mut measured = DimData { overrides: Vec::new(), ..d.clone() };
            measured.overrides.clear();
            let texts = measured.compute().map(|r| r.texts).unwrap_or_default();
            if d.overrides.len() < texts.len() {
                d.overrides.resize(texts.len(), None);
            }
            egui::Grid::new("ann-dim-grid").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Style");
                egui::ComboBox::from_id_salt("ann-dim-style").selected_text(d.style.name.clone()).show_ui(ui, |ui| {
                    for s in &st.dim_styles {
                        if ui.selectable_label(d.style.name == s.name, &s.name).clicked() {
                            let scale = d.style.scale;
                            d.style = DimStyle { scale, ..s.clone() };
                        }
                    }
                });
                ui.end_row();
                for (i, t) in texts.iter().enumerate() {
                    ui.label(if texts.len() > 1 { format!("Measured {}", i + 1) } else { "Measured".to_string() });
                    ui.label(RichText::new(t).monospace());
                    ui.end_row();
                    ui.label("Text override");
                    let mut s = d.overrides.get(i).cloned().flatten().unwrap_or_default();
                    if ui.add(egui::TextEdit::singleline(&mut s).desired_width(160.0).hint_text("<> is the measured value")).changed() {
                        d.overrides[i] = (!s.is_empty()).then_some(s);
                    }
                    ui.end_row();
                }
                match &mut d.kind {
                    DimKind::StationOffset { alignment, .. } => {
                        ui.label("Alignment");
                        egui::ComboBox::from_id_salt("ann-dim-align").selected_text(alignment.clone()).show_ui(ui, |ui| {
                            for n in alignments {
                                ui.selectable_value(alignment, n.clone(), n);
                            }
                        });
                        ui.end_row();
                        if let Some(p) = &d.cache.problem {
                            ui.label("");
                            ui.label(RichText::new(p).color(BAD).small());
                            ui.end_row();
                        }
                    }
                    DimKind::Level { value_m, source, .. } => {
                        ui.label("Level (m)");
                        if ui.add(egui::DragValue::new(value_m).speed(0.01).fixed_decimals(3)).changed() {
                            *source = ann::LevelSource::Entered;
                        }
                        ui.end_row();
                        ui.label("Level source");
                        ui.label(match source {
                            ann::LevelSource::Entered => "typed",
                            ann::LevelSource::Sampled => "sampled from the surface or profile",
                        });
                        ui.end_row();
                    }
                    _ => {}
                }
            });
            ui.label(RichText::new("The measured value is recomputed from the points; an override shows beside it here.").small().weak());
        }
        Ann::Leader(l) => {
            match &mut l.text {
                LeaderText::Mark { cached, .. } => {
                    ui.label(format!("Shows the mark of an element: {cached}. It follows the element if it is renamed."));
                }
                LeaderText::Literal(s) => {
                    ui.label("Text");
                    ui.add(egui::TextEdit::singleline(s).desired_width(f32::INFINITY));
                }
                LeaderText::None => {
                    ui.label("No text.");
                    if ui.small_button("Add text").clicked() {
                        l.text = LeaderText::Literal("Text".into());
                    }
                }
            }
            ui.horizontal(|ui| {
                ui.label("Style");
                egui::ComboBox::from_id_salt("ann-ldr-style").selected_text(l.style.name.clone()).show_ui(ui, |ui| {
                    for s in &st.dim_styles {
                        if ui.selectable_label(l.style.name == s.name, &s.name).clicked() {
                            let scale = l.style.scale;
                            l.style = DimStyle { scale, ..s.clone() };
                        }
                    }
                });
            });
        }
    }
    if a != before {
        match ann::write(e, a) {
            Ok(()) => return true,
            Err(err) => {
                ui.label(RichText::new(format!("Not applied: {err}")).color(BAD));
                let _ = ann::write(e, before);
            }
        }
    }
    false
}

/// The annotation scale control of the status bar: "1:" and the denominator.
pub fn scale_control(ui: &mut Ui, doc: &mut Document) -> bool {
    let mut st = ann::settings::load(doc);
    let mut v = st.annotation_scale;
    let r = ui
        .add(egui::DragValue::new(&mut v).prefix("1:").speed(1.0).range(1.0..=100_000.0).fixed_decimals(0))
        .on_hover_text("Annotation scale: the drawing scale new text, dimensions and leaders are sized for. Paper sizes in the styles are multiplied by it. Existing annotation keeps its own scale.");
    if r.changed() && v >= 1.0 {
        st.annotation_scale = v.round();
        ann::settings::store(doc, &st);
        return true;
    }
    false
}

/// The units of a document, for formatting where the app prints numbers itself.
pub fn units_of(doc: &Document) -> DisplayUnits {
    ann::settings::load(doc).units
}

/// The alignment names of a document (for the Properties editor).
pub fn alignment_names(doc: &Document) -> Vec<String> {
    alignment_glue::alignments(doc).into_iter().map(|a| a.1).collect()
}

// ─── demo ─────────────────────────────────────────────────────────────────────────────────

/// Demo only: a text note, an aligned dimension, a mark leader and, when there is an alignment,
/// a station and offset dimension.
///
/// **Every value here is an arbitrary demo value**: the 1:50 annotation scale, the note's wording,
/// the dimension's position (800 mm left of the sign), the 40 percent station, the 6 m offset and the 1:500 size of the station dimension.
/// The station dimension is in site coordinates, so it sits with the alignment, on its own layer
/// that the demo leaves off (turn it on in LAYER or run DIMSTATION).
pub fn demo(doc: &mut Document) -> Vec<String> {
    let mut problems = Vec::new();
    let mut st = ann::settings::load(doc);
    st.annotation_scale = 50.0; // demo value
    ann::settings::store(doc, &st);
    let first_sign = doc.elements().iter().find(|e| e.category == crate::signs_glue::CATEGORY).map(|e| (e.id, e.bounds()));
    if let Some((id, b)) = first_sign {
        let style = st.current_dim();
        let note = ann::TextData::new("Demo note: existing sign to be replaced", pt(b.min.x, b.max.y + 700.0), &st.current_text(), st.annotation_scale); // demo value
        if let Err(e) = ann::add_text(doc, note, anno_glue::LAYER) {
            problems.push(format!("Demo text: {e}"));
        }
        // An aligned dimension of the sign's height, 800 mm to the left of it (demo value).
        let d = DimData::new(DimKind::Linear { p1: pt(b.min.x, b.min.y), p2: pt(b.min.x, b.max.y), line_pt: pt(b.min.x - 800.0, b.min.y), orient: ann::LinearOrient::Aligned }, &style);
        if let Err(e) = ann::add_dimension(doc, d, anno_glue::LAYER) {
            problems.push(format!("Demo dimension: {e}"));
        }
        // A leader that shows the sign's mark, landing 1.2 m above and to the right (demo value).
        let c = pt((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0);
        let l = ann::LeaderData::new(vec![c, pt(b.max.x + 600.0, b.max.y + 500.0), pt(b.max.x + 1600.0, b.max.y + 500.0)], LeaderText::Mark { element: id.0, cached: String::new() }, &style);
        if let Err(e) = ann::add_leader(doc, l, anno_glue::LAYER) {
            problems.push(format!("Demo leader: {e}"));
        }
    }
    if let Some((_, a)) = anno_glue::alignment_list(doc).into_iter().next() {
        let internal = a.start_station_m + a.length() * 0.4; // demo value
        if let Some(p) = a.point_at_offset(internal, 6.0) {
            let site = pt(p.0 * 1000.0, p.1 * 1000.0);
            // Demo value: this dimension is read at site scale, so it is sized for 1:500.
            let mut style = st.current_dim();
            style.scale = 500.0;
            let d = DimData::new(DimKind::StationOffset { point: site, alignment: a.name.clone(), text_pt: pt(site.x + 4000.0, site.y + 6000.0) }, &style);
            match ann::add_dimension(doc, d, "ANNOTATION-SITE") {
                Ok(_) => {
                    if let Some(l) = doc.layers_mut().iter_mut().find(|l| l.name == "ANNOTATION-SITE") {
                        l.visible = false;
                    }
                }
                Err(e) => problems.push(format!("Demo station dimension: {e}")),
            }
        }
    }
    ann::refresh(doc);
    problems
}
