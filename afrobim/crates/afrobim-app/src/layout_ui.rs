// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Layout tabs (paper space): the state of the tab strip and of the layout being worked on, every
//! operation on layouts and viewports, the commands, the windows and the plot.
//!
//! The data lives in the document (`afrobim_kernel::layout`), so it is saved with it and goes
//! through the same undo history as the elements. Nothing here is saved on its own: the current
//! tab, the paper view and the viewport being worked in are session state.
//!
//! Painting and pointer handling of the paper are in `layout_canvas.rs`.

use std::collections::HashMap;

use afrobim_kernel::{Layout, Pt, ViewTransform, Viewport, VpGrip, MODEL_TAB, STANDARD_SCALES};
use afrobim_sheets::layout_plot as lp;
use afrobim_sheets::{Orientation, PaperSize};
use egui::{Context, RichText, Ui};

use crate::app::AfroBimApp;
use crate::layout_cmds::Lv;
use crate::ui_kit;

/// What a click on the paper does while a creation command is running.
#[derive(Debug, Clone, PartialEq)]
pub enum PaperTool {
    None,
    /// MVIEW: the first corner, once picked.
    Rect { first: Option<Pt> },
    /// MVIEWPOLY.
    Poly { pts: Vec<Pt> },
    /// MVIEWOBJECT: pick a closed polyline or a circle.
    Object,
    /// VPCLIP: the polygon so far.
    Clip { vp: u64, pts: Vec<Pt> },
    DrawRect { first: Option<Pt> },
    DrawCircle { centre: Option<Pt> },
    DrawPoly { pts: Vec<Pt> },
}

impl PaperTool {
    /// The prompt shown while the tool runs.
    pub fn prompt(&self) -> &'static str {
        match self {
            PaperTool::None => "",
            PaperTool::Rect { first: None } => "MVIEW: pick the first corner",
            PaperTool::Rect { first: Some(_) } => "MVIEW: pick the opposite corner",
            PaperTool::Poly { .. } => "MVIEWPOLY: pick the next corner (Enter or right click to finish)",
            PaperTool::Object => "MVIEWOBJECT: pick a closed polyline or a circle",
            PaperTool::Clip { .. } => "VPCLIP: pick the next corner of the boundary (Enter or right click to finish)",
            PaperTool::DrawRect { first: None } => "PRECT: pick the first corner",
            PaperTool::DrawRect { first: Some(_) } => "PRECT: pick the opposite corner",
            PaperTool::DrawCircle { centre: None } => "PCIRCLE: pick the centre",
            PaperTool::DrawCircle { centre: Some(_) } => "PCIRCLE: pick a point on the circle",
            PaperTool::DrawPoly { .. } => "PPOLY: pick the next corner (Enter or right click to close)",
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, PaperTool::None)
    }

    /// True for the tools that collect points until Enter.
    pub fn collects_points(&self) -> bool {
        matches!(self, PaperTool::Poly { .. } | PaperTool::Clip { .. } | PaperTool::DrawPoly { .. })
    }
}

/// The page setup form.
#[derive(Debug, Clone, PartialEq)]
pub struct PageForm {
    pub layout: u64,
}

/// What the tab strip's context menu asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum TabAct {
    New,
    Template(PaperSize, Orientation),
    Rename(u64),
    Delete(u64),
    Duplicate(u64),
    Move(u64, i32),
    PageSetup(u64),
    Plot(u64),
}

/// Session state of the layouts. Nothing in it is saved.
pub struct LayoutUi {
    /// The layout shown, or `None` for the Model tab.
    pub current: Option<u64>,
    /// The paper view of each layout (screen pixels per paper millimetre), made on first show.
    pub views: HashMap<u64, ViewTransform>,
    pub selected: Option<u64>,
    /// The viewport being worked in (model space through it).
    pub active: Option<u64>,
    /// A viewport maximised to the canvas, with the model view it is shown with meanwhile.
    pub maximised: Option<(u64, ViewTransform)>,
    pub tool: PaperTool,
    /// A viewport handle being dragged: viewport, handle and the last paper point.
    pub drag: Option<(u64, VpGrip, Pt)>,
    pub cursor_paper: Option<Pt>,
    // The tab strip.
    pub tab_first: usize,
    pub rename: Option<(u64, String)>,
    pub rename_fresh: bool,
    pub confirm_delete: Option<u64>,
    // Windows.
    pub manager_open: bool,
    pub scale_open: bool,
    pub scale_text: String,
    pub layers_open: bool,
    pub props_open: bool,
    pub page: Option<PageForm>,
    pub plot_open: bool,
    pub plot_all: bool,
    pub new_name: String,
    /// What the last plot said, for the Plot window.
    pub plot_note: String,
    /// A layout asked for in the manager window, made after the window is drawn.
    pub pending: Option<(Option<String>, PaperSize, Orientation)>,
    /// The canvas rectangle (left, top, width, height) the Model tab draws in.
    pub model_rect: (f64, f64, f64, f64),
}

impl Default for LayoutUi {
    fn default() -> Self {
        LayoutUi {
            current: None,
            views: HashMap::new(),
            selected: None,
            active: None,
            maximised: None,
            tool: PaperTool::None,
            drag: None,
            cursor_paper: None,
            tab_first: 0,
            rename: None,
            rename_fresh: false,
            confirm_delete: None,
            manager_open: false,
            scale_open: false,
            scale_text: String::new(),
            layers_open: false,
            props_open: false,
            page: None,
            plot_open: false,
            plot_all: false,
            new_name: String::new(),
            plot_note: String::new(),
            pending: None,
            model_rect: (0.0, 0.0, 0.0, 0.0),
        }
    }
}

impl LayoutUi {
    /// True when the paper (not a viewport) is what the pointer works on.
    pub fn in_paper_space(&self) -> bool {
        self.current.is_some() && self.active.is_none() && self.maximised.is_none()
    }

    /// True when a command that collects points on the paper is running.
    pub fn tool_wants_enter(&self) -> bool {
        self.tool.collects_points()
    }
}

/// "A3 landscape" and so on.
pub fn paper_text(l: &Layout) -> String {
    format!("{} {} ({} x {} mm)", l.size, if l.landscape { "landscape" } else { "portrait" }, l.width_mm.round(), l.height_mm.round())
}

impl AfroBimApp {
    // ─── reading ─────────────────────────────────────────────────────────────

    pub(crate) fn current_layout(&self) -> Option<&Layout> {
        self.lay.current.and_then(|id| self.doc.layouts().get(id))
    }

    /// The viewport a command acts on: the active one, the selected one, the maximised one, or the
    /// only one the layout has.
    pub(crate) fn target_vp(&self) -> Option<u64> {
        let l = self.current_layout()?;
        let wanted = self.lay.active.or(self.lay.selected).or(self.lay.maximised.as_ref().map(|m| m.0));
        match wanted.filter(|id| l.viewport(*id).is_some()) {
            Some(id) => Some(id),
            None if l.viewports.len() == 1 => Some(l.viewports[0].id),
            None => None,
        }
    }

    pub(crate) fn target_viewport(&self) -> Option<&Viewport> {
        let id = self.target_vp()?;
        self.current_layout()?.viewport(id)
    }

    // ─── tabs ────────────────────────────────────────────────────────────────

    /// Switches to a layout tab, or to the model tab with `None`. Whatever tool or viewport work
    /// was going on is dropped.
    pub(crate) fn set_current_layout(&mut self, id: Option<u64>) {
        let id = id.filter(|i| self.doc.layouts().get(*i).is_some());
        if id != self.lay.current {
            // A model command in progress belongs to the tab it was started in.
            self.ctl.escape(&mut self.doc);
            self.lay.active = None;
            self.lay.maximised = None;
            self.lay.selected = None;
            self.lay.tool = PaperTool::None;
            self.lay.drag = None;
            self.lay.current = id;
        }
        if let Some(i) = id.and_then(|i| self.doc.layouts().index_of(i)) {
            self.lay.tab_first = self.lay.tab_first.min(i);
        }
        self.status = match id.and_then(|i| self.doc.layouts().get(i)) {
            Some(l) => format!("Layout {}: {}.", l.name, paper_text(l)),
            None => "Model.".into(),
        };
    }

    /// Ctrl+PageDown (`delta` 1) and Ctrl+PageUp (-1): the next or previous tab, Model first.
    pub(crate) fn cycle_layout_tab(&mut self, delta: i32) {
        let n = self.doc.layouts().len() as i64 + 1;
        let here = self.lay.current.and_then(|id| self.doc.layouts().index_of(id)).map(|i| i as i64 + 1).unwrap_or(0);
        let next = (here + delta as i64).rem_euclid(n);
        let id = if next == 0 { None } else { self.doc.layouts().at(next as usize - 1).map(|l| l.id) };
        self.set_current_layout(id);
    }

    /// Adds a layout of the given paper and switches to it. A blank or taken name gets a free one.
    pub(crate) fn new_layout(&mut self, ctx: &Context, name: Option<&str>, size: PaperSize, orientation: Orientation) -> u64 {
        let base = name.map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| format!("Layout{}", self.doc.layouts().len() + 1));
        let name = self.doc.layouts().unique_name(&base);
        let layout = lp::layout_from_template(&name, size, orientation, true);
        let id = self.doc.layouts_mut().add(layout).expect("a unique name is accepted");
        self.set_current_layout(Some(id));
        self.status = format!("Added layout {name}: {} {}.", size.name(), if orientation == Orientation::Landscape { "landscape" } else { "portrait" });
        self.schedule_commit(ctx);
        id
    }

    pub(crate) fn rename_layout(&mut self, ctx: &Context, id: u64, name: &str) -> bool {
        match self.doc.layouts_mut().rename(id, name) {
            Ok(()) => {
                self.status = format!("Layout renamed to {}.", name.trim());
                self.schedule_commit(ctx);
                true
            }
            Err(e) => {
                self.status = e.to_string();
                false
            }
        }
    }

    pub(crate) fn delete_layout(&mut self, ctx: &Context, id: u64) {
        let name = self.doc.layouts().get(id).map(|l| l.name.clone()).unwrap_or_default();
        let idx = self.doc.layouts().index_of(id);
        if self.doc.layouts_mut().delete(id).is_ok() {
            self.lay.views.remove(&id);
            if self.lay.current == Some(id) {
                self.lay.current = None;
                let next = idx.and_then(|i| self.doc.layouts().at(i.saturating_sub(1)).or(self.doc.layouts().at(0))).map(|l| l.id);
                self.set_current_layout(next);
            }
            self.status = format!("Deleted layout {name}.");
            self.schedule_commit(ctx);
        }
    }

    pub(crate) fn duplicate_layout(&mut self, ctx: &Context, id: u64) {
        if let Ok(n) = self.doc.layouts_mut().duplicate(id) {
            self.set_current_layout(Some(n));
            self.status = "Layout copied.".into();
            self.schedule_commit(ctx);
        }
    }

    pub(crate) fn move_layout(&mut self, ctx: &Context, id: u64, delta: i32) {
        if self.doc.layouts_mut().move_by(id, delta) {
            self.schedule_commit(ctx);
        }
    }

    fn apply_tab_act(&mut self, ctx: &Context, act: TabAct) {
        match act {
            TabAct::New => {
                self.new_layout(ctx, None, PaperSize::A3, Orientation::Landscape);
            }
            TabAct::Template(s, o) => {
                self.new_layout(ctx, None, s, o);
            }
            TabAct::Rename(id) => {
                let name = self.doc.layouts().get(id).map(|l| l.name.clone()).unwrap_or_default();
                self.set_current_layout(Some(id));
                self.lay.rename = Some((id, name));
                self.lay.rename_fresh = true;
            }
            TabAct::Delete(id) => self.lay.confirm_delete = Some(id),
            TabAct::Duplicate(id) => self.duplicate_layout(ctx, id),
            TabAct::Move(id, d) => self.move_layout(ctx, id, d),
            TabAct::PageSetup(id) => {
                self.set_current_layout(Some(id));
                self.lay.page = Some(PageForm { layout: id });
            }
            TabAct::Plot(id) => {
                self.set_current_layout(Some(id));
                self.lay.plot_open = true;
            }
        }
    }

    // ─── viewports ───────────────────────────────────────────────────────────

    /// A layout to work on: the current one, else the first, else a new one.
    pub(crate) fn ensure_layout(&mut self, ctx: &Context) -> u64 {
        if let Some(id) = self.lay.current {
            return id;
        }
        let first = self.doc.layouts().at(0).map(|l| l.id);
        match first {
            Some(id) => {
                self.set_current_layout(Some(id));
                id
            }
            None => self.new_layout(ctx, None, PaperSize::A3, Orientation::Landscape),
        }
    }

    /// The model point at the middle of the model view, and the smallest standard scale at which
    /// a `w` by `h` mm viewport shows the whole of what the model view shows now.
    pub(crate) fn default_view_for(&self, w_mm: f64, h_mm: f64) -> (Pt, f64) {
        let (l, t, cw, ch) = if self.lay.model_rect.2 > 0.0 { self.lay.model_rect } else { self.ctl.viewport };
        let (cw, ch) = (cw.max(50.0), ch.max(50.0));
        let centre = self.view.to_world(Pt::new(l + cw / 2.0, t + ch / 2.0));
        let (mw, mh) = (cw / self.view.scale, ch / self.view.scale);
        let need = (mw / w_mm.max(1.0)).max(mh / h_mm.max(1.0));
        let den = STANDARD_SCALES.iter().copied().find(|s| *s >= need).unwrap_or_else(|| afrobim_sheets::round_up_1_2_5(need));
        (centre, den)
    }

    fn add_vp_rect(&mut self, ctx: &Context, a: Pt, b: Pt) {
        let Some(lid) = self.lay.current else { return };
        let (centre, den) = self.default_view_for((a.x - b.x).abs(), (a.y - b.y).abs());
        let id = self.doc.layouts_mut().get_mut(lid).and_then(|l| l.add_viewport(a, b, centre, den));
        self.finish_new_vp(ctx, id, "Viewport");
    }

    fn add_vp_polygon(&mut self, ctx: &Context, poly: Vec<Pt>) {
        let Some(lid) = self.lay.current else { return };
        let r = afrobim_kernel::layout::bbox_of(&poly);
        let (centre, den) = self.default_view_for(r[2] - r[0], r[3] - r[1]);
        let id = self.doc.layouts_mut().get_mut(lid).and_then(|l| l.add_polygon_viewport(poly, centre, den));
        self.finish_new_vp(ctx, id, "Polygonal viewport");
    }

    fn finish_new_vp(&mut self, ctx: &Context, id: Option<u64>, what: &str) {
        match id {
            Some(id) => {
                self.lay.selected = Some(id);
                let den = self.current_layout().and_then(|l| l.viewport(id)).map(|v| v.scale_label()).unwrap_or_default();
                self.status = format!("{what} added at {den}. Double click inside it to work in model space.");
                self.schedule_commit(ctx);
            }
            None => self.status = "The viewport was not made: it needs a size of at least 1 mm.".into(),
        }
    }

    pub(crate) fn add_fit_vp(&mut self, ctx: &Context) {
        let lid = self.ensure_layout(ctx);
        let area = self.doc.layouts().get(lid).map(|l| l.view_area()).unwrap_or([0.0; 4]);
        let (centre, den) = self.default_view_for(area[2] - area[0], area[3] - area[1]);
        let id = self.doc.layouts_mut().get_mut(lid).and_then(|l| l.add_fit_viewport(centre, den));
        self.finish_new_vp(ctx, id, "Viewport");
    }

    pub(crate) fn add_vp_preset(&mut self, ctx: &Context, n: u8) {
        let lid = self.ensure_layout(ctx);
        let area = self.doc.layouts().get(lid).map(|l| l.view_area()).unwrap_or([0.0; 4]);
        let (centre, den) = self.default_view_for((area[2] - area[0]) / 2.0, (area[3] - area[1]) / if n >= 3 { 2.0 } else { 1.0 });
        let ids = self.doc.layouts_mut().get_mut(lid).map(|l| l.add_preset(n, centre, den)).unwrap_or_default();
        if let Some(last) = ids.last() {
            self.lay.selected = Some(*last);
        }
        self.status = format!("Added {} viewport(s) at 1:{den}.", ids.len());
        self.schedule_commit(ctx);
    }

    /// Runs `f` on the viewport and schedules the undo step.
    pub(crate) fn with_vp<R>(&mut self, ctx: &Context, id: u64, f: impl FnOnce(&mut Viewport) -> R) -> Option<R> {
        let lid = self.lay.current?;
        let r = f(self.doc.layouts_mut().get_mut(lid)?.viewport_mut(id)?);
        self.schedule_commit(ctx);
        Some(r)
    }

    pub(crate) fn set_vp_scale(&mut self, ctx: &Context, id: u64, den: f64) {
        match self.with_vp(ctx, id, |v| v.set_scale(den)) {
            Some(true) => self.status = format!("Viewport scale {}.", afrobim_kernel::layout::scale_label(den)),
            Some(false) if Viewport::is_valid_scale(den) => self.status = "The viewport is locked. Unlock it (VPLOCK) to change its scale.".into(),
            Some(false) => self.status = format!("{den} is not a scale. Use a number above 0, as 1:500."),
            None => {}
        }
    }

    pub(crate) fn delete_vp(&mut self, ctx: &Context, id: u64) {
        if let Some(lid) = self.lay.current {
            if self.doc.layouts_mut().get_mut(lid).is_some_and(|l| l.delete_viewport(id)) {
                if self.lay.selected == Some(id) {
                    self.lay.selected = None;
                }
                if self.lay.active == Some(id) {
                    self.lay.active = None;
                }
                if self.lay.maximised.as_ref().is_some_and(|m| m.0 == id) {
                    self.lay.maximised = None;
                }
                self.status = "Viewport deleted.".into();
                self.schedule_commit(ctx);
            }
        }
    }

    /// Enters model space through the viewport.
    pub(crate) fn activate_vp(&mut self, id: u64) {
        if self.current_layout().and_then(|l| l.viewport(id)).is_some_and(|v| v.on) {
            self.lay.active = Some(id);
            self.lay.selected = Some(id);
            self.lay.tool = PaperTool::None;
            self.status = "Model space through the viewport. Double click outside it or use PSPACE to return to the paper.".into();
        } else {
            self.status = "That viewport is off. Turn its display on (VPDISPLAY) to work in it.".into();
        }
    }

    pub(crate) fn deactivate_vp(&mut self) {
        if self.lay.active.take().is_some() || self.lay.maximised.is_some() {
            self.ctl.escape(&mut self.doc);
            self.lay.maximised = None;
            self.status = "Paper space.".into();
        }
    }

    /// VPMAX: the viewport fills the canvas, at the largest scale that shows what it shows.
    pub(crate) fn maximise_vp(&mut self, id: u64) {
        let Some(v) = self.current_layout().and_then(|l| l.viewport(id)).cloned() else { return };
        let (l, t, cw, ch) = if self.lay.model_rect.2 > 0.0 { self.lay.model_rect } else { self.ctl.viewport };
        let (cw, ch) = (cw.max(100.0), ch.max(100.0));
        let (mw, mh) = (v.model_width(), v.height() * v.scale_den);
        let scale = (cw * 0.95 / mw.max(1e-9)).min(ch * 0.95 / mh.max(1e-9));
        let view = ViewTransform::centred_on(v.centre, Pt::new(l + cw / 2.0, t + ch / 2.0), scale);
        self.lay.maximised = Some((id, view));
        self.lay.active = None;
        self.lay.selected = Some(id);
        self.status = "Viewport maximised. VPMIN returns to the layout.".into();
    }

    pub(crate) fn minimise_vp(&mut self) {
        if self.lay.maximised.take().is_some() {
            self.ctl.escape(&mut self.doc);
            self.status = "Viewport restored to the layout.".into();
        } else {
            self.status = "No viewport is maximised.".into();
        }
    }

    /// The Escape key: ends a paper tool. Returns true when it did.
    pub(crate) fn layout_escape(&mut self) -> bool {
        if !self.lay.tool.is_none() {
            self.lay.tool = PaperTool::None;
            self.status = "Cancelled.".into();
            return true;
        }
        false
    }

    // ─── paper tools ─────────────────────────────────────────────────────────

    /// A click on the paper while a tool runs. Returns true when the tool took it.
    pub(crate) fn tool_click(&mut self, ctx: &Context, p: Pt, tol_mm: f64) -> bool {
        let tool = std::mem::replace(&mut self.lay.tool, PaperTool::None);
        match tool {
            PaperTool::None => return false,
            PaperTool::Rect { first: None } => self.lay.tool = PaperTool::Rect { first: Some(p) },
            PaperTool::Rect { first: Some(a) } => self.add_vp_rect(ctx, a, p),
            PaperTool::Poly { mut pts } => {
                pts.push(p);
                self.lay.tool = PaperTool::Poly { pts };
            }
            PaperTool::Clip { vp, mut pts } => {
                pts.push(p);
                self.lay.tool = PaperTool::Clip { vp, pts };
            }
            PaperTool::Object => {
                let shape = self.current_layout().and_then(|l| l.shape_at(p, tol_mm).map(|id| (l.id, id)));
                match shape {
                    Some((lid, sid)) => {
                        let (w, h) = self.current_layout().and_then(|l| l.shapes.iter().find(|s| s.id() == sid)).and_then(|s| s.closed_outline()).map(|o| {
                            let r = afrobim_kernel::layout::bbox_of(&o);
                            (r[2] - r[0], r[3] - r[1])
                        }).unwrap_or((100.0, 100.0));
                        let (centre, den) = self.default_view_for(w, h);
                        let id = self.doc.layouts_mut().get_mut(lid).and_then(|l| l.add_viewport_from_shape(sid, centre, den));
                        if id.is_none() {
                            self.status = "That is not a closed polyline or a circle.".into();
                            self.lay.tool = PaperTool::Object;
                        } else {
                            self.finish_new_vp(ctx, id, "Viewport");
                        }
                    }
                    None => {
                        self.status = "Nothing to pick there. Draw a closed polyline or a circle on the layout first (PRECT, PCIRCLE, PPOLY).".into();
                        self.lay.tool = PaperTool::Object;
                    }
                }
            }
            PaperTool::DrawRect { first: None } => self.lay.tool = PaperTool::DrawRect { first: Some(p) },
            PaperTool::DrawRect { first: Some(a) } => {
                let pts = vec![a, Pt::new(p.x, a.y), p, Pt::new(a.x, p.y)];
                self.add_shape(ctx, |l| l.add_shape_poly(pts, true));
            }
            PaperTool::DrawCircle { centre: None } => self.lay.tool = PaperTool::DrawCircle { centre: Some(p) },
            PaperTool::DrawCircle { centre: Some(c) } => {
                let r = ((p.x - c.x).powi(2) + (p.y - c.y).powi(2)).sqrt();
                self.add_shape(ctx, |l| l.add_shape_circle(c, r));
            }
            PaperTool::DrawPoly { mut pts } => {
                pts.push(p);
                self.lay.tool = PaperTool::DrawPoly { pts };
            }
        }
        true
    }

    fn add_shape(&mut self, ctx: &Context, f: impl FnOnce(&mut Layout) -> Option<u64>) {
        let Some(lid) = self.lay.current else { return };
        if self.doc.layouts_mut().get_mut(lid).and_then(f).is_some() {
            self.status = "Added to the layout. MVIEWOBJECT makes a viewport from it.".into();
            self.schedule_commit(ctx);
        } else {
            self.status = "Nothing was drawn: the shape needs a size.".into();
        }
    }

    /// Enter or a right click during a point-collecting tool: finishes it.
    pub(crate) fn tool_finish(&mut self, ctx: &Context) {
        match std::mem::replace(&mut self.lay.tool, PaperTool::None) {
            PaperTool::Poly { pts } => {
                if pts.len() >= 3 {
                    self.add_vp_polygon(ctx, pts);
                } else {
                    self.status = "A polygon needs at least three corners.".into();
                }
            }
            PaperTool::Clip { vp, pts } => {
                if pts.len() >= 3 {
                    let ok = self.with_vp(ctx, vp, |v| v.set_clip(pts));
                    self.status = if ok == Some(true) { "Viewport boundary reshaped.".into() } else { "The boundary was not changed.".into() };
                } else {
                    self.status = "A boundary needs at least three corners.".into();
                }
            }
            PaperTool::DrawPoly { pts } => {
                if pts.len() >= 3 {
                    self.add_shape(ctx, |l| l.add_shape_poly(pts, true));
                } else {
                    self.status = "A closed polyline needs at least three corners.".into();
                }
            }
            other => self.lay.tool = other,
        }
    }

    // ─── commands ────────────────────────────────────────────────────────────

    pub(crate) fn layout_run(&mut self, ctx: &Context, lv: Lv) {
        let needs_vp = |app: &mut AfroBimApp| -> Option<u64> {
            let id = app.target_vp();
            if id.is_none() {
                app.status = "Select a viewport first (click inside it on a layout tab).".into();
            }
            id
        };
        match lv {
            Lv::Layout => self.lay.manager_open = true,
            Lv::LayoutNew => {
                self.new_layout(ctx, None, PaperSize::A3, Orientation::Landscape);
            }
            Lv::LayoutRename => match self.lay.current {
                Some(id) => self.apply_tab_act(ctx, TabAct::Rename(id)),
                None => self.status = "Switch to a layout tab to rename it. The Model tab keeps its name.".into(),
            },
            Lv::LayoutDelete => match self.lay.current {
                Some(id) => self.lay.confirm_delete = Some(id),
                None => self.status = "Switch to the layout tab to delete. The Model tab cannot be deleted.".into(),
            },
            Lv::LayoutDuplicate => match self.lay.current {
                Some(id) => self.duplicate_layout(ctx, id),
                None => self.status = "Switch to a layout tab to copy it.".into(),
            },
            Lv::Model => self.set_current_layout(None),
            Lv::Mview => {
                self.ensure_layout(ctx);
                self.lay.tool = PaperTool::Rect { first: None };
                self.status = self.lay.tool.prompt().into();
            }
            Lv::MviewPoly => {
                self.ensure_layout(ctx);
                self.lay.tool = PaperTool::Poly { pts: Vec::new() };
                self.status = self.lay.tool.prompt().into();
            }
            Lv::MviewObject => {
                self.ensure_layout(ctx);
                self.lay.tool = PaperTool::Object;
                self.status = self.lay.tool.prompt().into();
            }
            Lv::MviewFit => self.add_fit_vp(ctx),
            Lv::MviewPreset(n) => self.add_vp_preset(ctx, n),
            Lv::Mspace => {
                self.ensure_layout(ctx);
                if let Some(id) = needs_vp(self) {
                    self.activate_vp(id);
                }
            }
            Lv::Pspace => {
                if self.lay.active.is_none() && self.lay.maximised.is_none() {
                    self.status = "Already in paper space.".into();
                } else {
                    self.deactivate_vp();
                }
            }
            Lv::VpScale => {
                if let Some(id) = needs_vp(self) {
                    self.lay.scale_text = self.current_layout().and_then(|l| l.viewport(id)).map(|v| v.scale_label()).unwrap_or_default();
                    self.lay.scale_open = true;
                }
            }
            Lv::VpLock => {
                if let Some(id) = needs_vp(self) {
                    let now = self.with_vp(ctx, id, |v| {
                        v.locked = !v.locked;
                        v.locked
                    });
                    self.status = if now == Some(true) { "Viewport display locked.".into() } else { "Viewport display unlocked.".into() };
                }
            }
            Lv::VpDisplay => {
                if let Some(id) = needs_vp(self) {
                    let now = self.with_vp(ctx, id, |v| {
                        v.on = !v.on;
                        v.on
                    });
                    if now == Some(false) && self.lay.active == Some(id) {
                        self.deactivate_vp();
                    }
                    self.status = if now == Some(true) { "Viewport display on.".into() } else { "Viewport display off.".into() };
                }
            }
            Lv::VpMax => {
                if let Some(id) = needs_vp(self) {
                    self.maximise_vp(id);
                }
            }
            Lv::VpMin => self.minimise_vp(),
            Lv::VpLayer => {
                self.ensure_layout(ctx);
                self.lay.layers_open = true;
            }
            Lv::VpClip => {
                if let Some(id) = needs_vp(self) {
                    self.lay.active = None;
                    self.lay.tool = PaperTool::Clip { vp: id, pts: Vec::new() };
                    self.status = self.lay.tool.prompt().into();
                }
            }
            Lv::VpUnclip => {
                if let Some(id) = needs_vp(self) {
                    let done = self.with_vp(ctx, id, |v| v.remove_clip());
                    self.status = if done == Some(true) { "Viewport clip removed.".into() } else { "That viewport has no clip.".into() };
                }
            }
            Lv::VpDelete => {
                if let Some(id) = needs_vp(self) {
                    self.delete_vp(ctx, id);
                }
            }
            Lv::VpCopy => {
                if let Some(id) = needs_vp(self) {
                    let lid = self.lay.current.unwrap_or(0);
                    let n = self.doc.layouts_mut().get_mut(lid).and_then(|l| l.duplicate_viewport(id));
                    if let Some(n) = n {
                        self.lay.selected = Some(n);
                        self.status = "Viewport copied 10 mm up and right.".into();
                        self.schedule_commit(ctx);
                    }
                }
            }
            Lv::VpProps => {
                if needs_vp(self).is_some() {
                    self.lay.props_open = true;
                }
            }
            Lv::PaperRect | Lv::PaperCircle | Lv::PaperPoly => {
                self.ensure_layout(ctx);
                self.lay.tool = match lv {
                    Lv::PaperRect => PaperTool::DrawRect { first: None },
                    Lv::PaperCircle => PaperTool::DrawCircle { centre: None },
                    _ => PaperTool::DrawPoly { pts: Vec::new() },
                };
                self.status = self.lay.tool.prompt().into();
            }
            Lv::PageSetup => {
                let id = self.ensure_layout(ctx);
                self.lay.page = Some(PageForm { layout: id });
            }
            Lv::Plot => {
                self.ensure_layout(ctx);
                self.lay.plot_open = true;
            }
        }
    }

    /// `LAYOUT NEW name`, `LAYOUT SET name`, `LAYOUT RENAME name`, `LAYOUT DELETE`, `LAYOUT LIST`,
    /// `LAYOUT COPY`, and `VPSCALE 1:500`, typed on the command line. Returns false when `rest`
    /// is not understood.
    pub(crate) fn layout_cli(&mut self, ctx: &Context, lv: Lv, rest: &str) -> bool {
        let rest = rest.trim();
        match lv {
            Lv::Layout => {
                let (word, arg) = rest.split_once(char::is_whitespace).map(|(a, b)| (a, b.trim())).unwrap_or((rest, ""));
                match word.to_ascii_uppercase().as_str() {
                    "NEW" | "N" => {
                        self.new_layout(ctx, Some(arg), PaperSize::A3, Orientation::Landscape);
                    }
                    "SET" | "S" => {
                        if arg.eq_ignore_ascii_case(MODEL_TAB) {
                            self.set_current_layout(None);
                        } else if let Some(id) = self.doc.layouts().by_name(arg).map(|l| l.id) {
                            self.set_current_layout(Some(id));
                        } else {
                            self.status = format!("There is no layout called \"{arg}\". LAYOUT LIST names them.");
                        }
                    }
                    "RENAME" | "R" => match self.lay.current {
                        Some(id) => {
                            self.rename_layout(ctx, id, arg);
                        }
                        None => self.status = "Switch to a layout tab to rename it.".into(),
                    },
                    "DELETE" | "D" => match if arg.is_empty() { self.lay.current } else { self.doc.layouts().by_name(arg).map(|l| l.id) } {
                        Some(id) => self.lay.confirm_delete = Some(id),
                        None => self.status = "No such layout to delete.".into(),
                    },
                    "COPY" | "C" => match self.lay.current {
                        Some(id) => self.duplicate_layout(ctx, id),
                        None => self.status = "Switch to a layout tab to copy it.".into(),
                    },
                    "LIST" | "L" | "?" => {
                        let mut line = MODEL_TAB.to_string();
                        for l in self.doc.layouts().iter() {
                            line.push_str(&format!(", {} ({})", l.name, l.size));
                        }
                        self.log(format!("Layouts: {line}"));
                        self.status = format!("{} layout(s). The list is in the command history.", self.doc.layouts().len());
                    }
                    _ => return false,
                }
                true
            }
            Lv::VpScale => match afrobim_kernel::layout::parse_scale(rest) {
                Some(den) => match self.target_vp() {
                    Some(id) => {
                        self.set_vp_scale(ctx, id, den);
                        true
                    }
                    None => {
                        self.status = "Select a viewport first.".into();
                        true
                    }
                },
                None => false,
            },
            _ => false,
        }
    }

    // ─── plot ────────────────────────────────────────────────────────────────

    /// The layouts composed at true paper size, with the document's annotation in each viewport.
    pub(crate) fn compose_layouts_for_plot(&self, ids: &[u64]) -> Vec<afrobim_sheets::ComposedSheet> {
        use afrobim_sheets::geom::{Aff, Rect};
        let model = afrobim_sheets::Model { doc: &self.doc, registry: &self.modules };
        let info = afrobim_sheets::project_info(&self.doc);
        let styles = afrobim_sheets::Styles::default();
        let layouts: Vec<&Layout> = ids.iter().filter_map(|id| self.doc.layouts().get(*id)).collect();
        let doc = &self.doc;
        let mut extras = |v: &Viewport, rect: &Rect, sheet: &mut afrobim_sheets::ComposedSheet| {
            if !doc.elements().iter().any(afrobim_annotate::is_annotation) {
                return;
            }
            let k = lp::paper_per_model(v);
            let pc = v.paper_centre();
            let corners = [v.to_model(Pt::new(rect.x0, rect.y0)), v.to_model(Pt::new(rect.x1, rect.y1))];
            let window = afrobim_kernel::BBox::from_corners(corners[0], corners[1]);
            let offset = Pt::new(pc.x / k - v.centre.x, pc.y / k - v.centre.y);
            let sd = afrobim_annotate::sheet_drawing(doc, offset, k, Some(&window));
            afrobim_sheets::draw_drawing(sheet, &styles, &sd.drawing, &Aff::IDENTITY, rect, crate::sheets_glue::ANNOTATION_LAYER);
            let rgb = styles.pens.pen(crate::sheets_glue::ANNOTATION_LAYER).rgb;
            for l in &sd.labels {
                if rect.contains(l.at) {
                    sheet.text(afrobim_sheets::SheetText { layer: crate::sheets_glue::ANNOTATION_LAYER.into(), at: l.at, height_mm: l.height_mm, text: l.text.clone(), align: l.align, rotation_deg: l.rotation_deg, bold: l.bold, rgb });
                }
            }
        };
        lp::compose_layouts(&model, &styles, &info, &layouts, &mut extras)
    }

    /// Plots the current layout (or every layout) to a file. `pdf` or DXF. Returns the account.
    pub(crate) fn plot_to_path(&mut self, ids: &[u64], pdf: bool, path: &std::path::Path) -> Result<String, String> {
        let sheets = self.compose_layouts_for_plot(ids);
        if sheets.is_empty() {
            return Err("There is no layout to plot.".into());
        }
        let styles = afrobim_sheets::Styles::default();
        let title = self.doc_path().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "AfroBIM layouts".into());
        let bytes = if pdf { afrobim_sheets::write_pdf(&sheets, &title) } else { afrobim_sheets::write_dxf_set(&sheets, &styles.pens, 20.0).into_bytes() };
        std::fs::write(path, bytes).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
        let warnings: Vec<String> = sheets.iter().flat_map(|s| s.warnings.iter().map(|w| format!("{}: {w}", s.number))).collect();
        let mut note = format!("Wrote {} page(s) to {}.", sheets.len(), path.display());
        if !warnings.is_empty() {
            note.push_str(&format!(" {} warning(s): {}", warnings.len(), warnings.join(" ")));
        }
        Ok(note)
    }

    fn plot_ask(&mut self, pdf: bool) {
        let ids: Vec<u64> = if self.lay.plot_all { self.doc.layouts().iter().map(|l| l.id).collect() } else { self.lay.current.into_iter().collect() };
        let stem = if self.lay.plot_all { "layouts".to_string() } else { self.current_layout().map(|l| crate::sheets_glue::file_stem(&l.name)).unwrap_or_else(|| "layout".into()) };
        let (ext, name) = if pdf { ("pdf", "PDF") } else { ("dxf", "AutoCAD DXF (R12)") };
        let Some(mut path) = rfd::FileDialog::new().add_filter(name, &[ext]).set_file_name(format!("{stem}.{ext}")).save_file() else { return };
        if path.extension().is_none() {
            path.set_extension(ext);
        }
        let note = match self.plot_to_path(&ids, pdf, &path) {
            Ok(n) | Err(n) => n,
        };
        self.status = note.clone();
        self.lay.plot_note = note;
    }

    // ─── tab strip ───────────────────────────────────────────────────────────

    /// The Model and layout tabs along the bottom of the canvas.
    pub(crate) fn layout_tabs(&mut self, ctx: &Context) {
        let mut act: Option<TabAct> = None;
        let mut goto: Option<Option<u64>> = None;
        egui::TopBottomPanel::bottom("layout_tabs").exact_height(26.0).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let model_tab = ui.selectable_label(self.lay.current.is_none(), RichText::new(MODEL_TAB).strong()).on_hover_text("Model space. Ctrl+PageDown goes to the first layout");
                if model_tab.clicked() {
                    goto = Some(None);
                }
                model_tab.context_menu(|ui| {
                    act = new_layout_menu(ui, act.take());
                });
                ui.separator();

                let items: Vec<(u64, String)> = self.doc.layouts().iter().map(|l| (l.id, l.name.clone())).collect();
                let body = egui::TextStyle::Body.resolve(ui.style());
                let widths: Vec<f32> = items.iter().map(|(_, n)| ui.fonts(|f| f.layout_no_wrap(n.clone(), body.clone(), egui::Color32::WHITE).size().x) + 22.0).collect();
                let plus_w = 30.0;
                let arrows_w = 2.0 * 24.0;
                let avail = (ui.available_width() - plus_w - 8.0).max(60.0);
                let total: f32 = widths.iter().sum();
                let overflow = total > avail;
                let room = if overflow { avail - arrows_w } else { avail };
                // Keep the current tab in view.
                let cur_idx = self.lay.current.and_then(|id| items.iter().position(|(i, _)| *i == id));
                self.lay.tab_first = self.lay.tab_first.min(items.len().saturating_sub(1));
                let last_visible = |first: usize| {
                    let mut used = 0.0;
                    let mut last = first;
                    for (i, w) in widths.iter().enumerate().skip(first) {
                        if used + w > room && i > first {
                            break;
                        }
                        used += w;
                        last = i;
                    }
                    last
                };
                if let Some(ci) = cur_idx {
                    if ci < self.lay.tab_first {
                        self.lay.tab_first = ci;
                    }
                    while overflow && last_visible(self.lay.tab_first) < ci && self.lay.tab_first < ci {
                        self.lay.tab_first += 1;
                    }
                }
                let first = if overflow { self.lay.tab_first } else { 0 };
                let last = if overflow { last_visible(first) } else { items.len().saturating_sub(1) };
                if overflow && ui.add_enabled(first > 0, egui::Button::new("\u{25C0}").small().frame(false)).on_hover_text("Scroll the tabs left").clicked() {
                    self.lay.tab_first = first - 1;
                }
                for (i, (id, name)) in items.iter().enumerate() {
                    if i < first || i > last || items.is_empty() {
                        continue;
                    }
                    if self.lay.rename.as_ref().is_some_and(|r| r.0 == *id) {
                        let fresh = std::mem::take(&mut self.lay.rename_fresh);
                        let mut done: Option<bool> = None;
                        if let Some((_, buf)) = &mut self.lay.rename {
                            let r = ui.add(egui::TextEdit::singleline(buf).desired_width(110.0).id(egui::Id::new(("layout-rename", *id))));
                            if fresh {
                                r.request_focus();
                            }
                            if r.lost_focus() {
                                done = Some(!ui.input(|i| i.key_pressed(egui::Key::Escape)));
                            }
                        }
                        if let Some(commit) = done {
                            let text = self.lay.rename.take().map(|r| r.1).unwrap_or_default();
                            if commit && text.trim() != name.as_str() {
                                self.rename_layout(ctx, *id, &text);
                            }
                        }
                        continue;
                    }
                    let selected = self.lay.current == Some(*id);
                    let r = ui.selectable_label(selected, name.as_str()).on_hover_text(self.doc.layouts().get(*id).map(paper_text).unwrap_or_default());
                    if r.clicked() {
                        goto = Some(Some(*id));
                    }
                    if r.double_clicked() {
                        act = Some(TabAct::Rename(*id));
                    }
                    r.context_menu(|ui| {
                        act = layout_tab_menu(ui, *id, i, items.len(), act.take());
                    });
                }
                if overflow && ui.add_enabled(last + 1 < items.len(), egui::Button::new("\u{25B6}").small().frame(false)).on_hover_text("Scroll the tabs right").clicked() {
                    self.lay.tab_first = (first + 1).min(items.len() - 1);
                }
                let plus = ui.add_sized([plus_w - 6.0, 18.0], egui::Button::new(RichText::new("+").strong())).on_hover_text("New layout (right click for paper sizes)");
                if plus.clicked() {
                    act = Some(TabAct::New);
                }
                plus.context_menu(|ui| {
                    act = new_layout_menu(ui, act.take());
                });
            });
        });
        if let Some(g) = goto {
            self.set_current_layout(g);
        }
        if let Some(a) = act {
            self.apply_tab_act(ctx, a);
        }
    }

    // ─── windows ─────────────────────────────────────────────────────────────

    pub(crate) fn layout_windows(&mut self, ctx: &Context) {
        self.layout_delete_dialog(ctx);
        if self.lay.manager_open {
            self.layout_manager_window(ctx);
        }
        if self.lay.scale_open {
            self.vp_scale_window(ctx);
        }
        if self.lay.layers_open {
            self.vp_layers_window(ctx);
        }
        if self.lay.props_open {
            let dlg = ui_kit::Dialog::titled("Viewport properties").subtitle("Scale, lock, display, title and label").width(380.0).show_body(ctx, |ui| {
                self.viewport_props_ui(ctx, ui);
            });
            if dlg.closed {
                self.lay.props_open = false;
            }
        }
        if self.lay.page.is_some() {
            self.page_setup_window(ctx);
        }
        if self.lay.plot_open {
            self.plot_window(ctx);
        }
    }

    fn layout_delete_dialog(&mut self, ctx: &Context) {
        let Some(id) = self.lay.confirm_delete else { return };
        let Some(l) = self.doc.layouts().get(id) else {
            self.lay.confirm_delete = None;
            return;
        };
        let (name, n) = (l.name.clone(), l.viewports.len());
        let out = ui_kit::Dialog::titled(format!("Delete layout {name}")).subtitle("The layout and its viewports are removed. Undo brings them back").width(420.0).resizable(false).show(
            ctx,
            |ui| {
                ui.label(format!("Delete the layout \"{name}\" with {n} viewport(s)? The model is not touched."));
            },
            |ui| ui_kit::Footer::new("Delete").show(ui),
        );
        let (mut go, mut cancel) = (false, out.closed);
        if let Some((_, f)) = &out.inner {
            go = f.primary;
            cancel = f.cancel || out.closed;
        }
        if go {
            self.lay.confirm_delete = None;
            self.delete_layout(ctx, id);
        } else if cancel {
            self.lay.confirm_delete = None;
        }
    }

    fn layout_manager_window(&mut self, ctx: &Context) {
        let mut act: Option<TabAct> = None;
        let mut set: Option<Option<u64>> = None;
        let dlg = ui_kit::Dialog::titled("Layouts").subtitle("The tabs of this drawing: list, create, rename, delete and switch").width(560.0).show_body(ctx, |ui| {
            ui_kit::heading(ui, "Layouts");
            let rows: Vec<(u64, String, String, usize)> = self.doc.layouts().iter().map(|l| (l.id, l.name.clone(), paper_text(l), l.viewports.len())).collect();
            let n = rows.len();
            egui::Grid::new("layout-list").num_columns(4).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                for h in ["Layout", "Paper", "Viewports", ""] {
                    ui_kit::th(ui, h);
                }
                ui.end_row();
                if ui.selectable_label(self.lay.current.is_none(), MODEL_TAB).clicked() {
                    set = Some(None);
                }
                ui.label("Model space");
                ui.label("");
                ui.label("");
                ui.end_row();
                for (i, (id, name, paper, vps)) in rows.iter().enumerate() {
                    if ui.selectable_label(self.lay.current == Some(*id), name).clicked() {
                        set = Some(Some(*id));
                    }
                    ui.label(paper);
                    ui.label(vps.to_string());
                    ui.horizontal(|ui| {
                        if ui.small_button("Rename").clicked() {
                            act = Some(TabAct::Rename(*id));
                        }
                        if ui.small_button("Copy").clicked() {
                            act = Some(TabAct::Duplicate(*id));
                        }
                        if ui.add_enabled(i > 0, egui::Button::new("\u{25C0}").small()).on_hover_text("Move left").clicked() {
                            act = Some(TabAct::Move(*id, -1));
                        }
                        if ui.add_enabled(i + 1 < n, egui::Button::new("\u{25B6}").small()).on_hover_text("Move right").clicked() {
                            act = Some(TabAct::Move(*id, 1));
                        }
                        if ui.small_button("Delete").clicked() {
                            act = Some(TabAct::Delete(*id));
                        }
                    });
                    ui.end_row();
                }
            });
            ui_kit::heading(ui, "New layout");
            ui.horizontal(|ui| {
                ui_kit::text_field(ui, &mut self.lay.new_name, "name (optional)");
            });
            ui.horizontal_wrapped(|ui| {
                for size in PaperSize::ALL {
                    for o in [Orientation::Landscape, Orientation::Portrait] {
                        let label = format!("{} {}", size.name(), if o == Orientation::Landscape { "landscape" } else { "portrait" });
                        if ui.button(label).clicked() {
                            let name = self.lay.new_name.trim().to_string();
                            self.lay.new_name.clear();
                            self.lay.pending = Some((if name.is_empty() { None } else { Some(name) }, size, o));
                        }
                    }
                }
            });
            ui_kit::callout(ui, ui_kit::Level::Info, "Each new layout has the AfroBIM title block, filled from Project information. Command line: LAYOUT NEW name, LAYOUT SET name, LAYOUT RENAME name, LAYOUT DELETE, LAYOUT LIST.");
        });
        if let Some(s) = set {
            self.set_current_layout(s);
        }
        if let Some(a) = act {
            self.apply_tab_act(ctx, a);
        }
        if let Some((name, size, o)) = self.lay.pending.take() {
            self.new_layout(ctx, name.as_deref(), size, o);
        }
        if dlg.closed {
            self.lay.manager_open = false;
        }
    }

    fn vp_scale_window(&mut self, ctx: &Context) {
        let Some(id) = self.target_vp() else {
            self.lay.scale_open = false;
            return;
        };
        let (now, locked) = self.target_viewport().map(|v| (v.scale_den, v.locked)).unwrap_or((100.0, false));
        let mut pick: Option<f64> = None;
        let dlg = ui_kit::Dialog::titled("Viewport scale").subtitle("The scale of the selected viewport").width(380.0).show_body(ctx, |ui| {
            ui_kit::heading(ui, "Standard scales");
            ui.horizontal_wrapped(|ui| {
                for s in STANDARD_SCALES {
                    if ui.add_enabled(!locked, egui::Button::new(afrobim_kernel::layout::scale_label(s)).selected((s - now).abs() < 1e-9)).clicked() {
                        pick = Some(s);
                    }
                }
            });
            ui_kit::heading(ui, "Custom");
            ui.horizontal(|ui| {
                let r = ui_kit::text_field(ui, &mut self.lay.scale_text, "1:750");
                if ui.add_enabled(!locked, egui::Button::new("Apply")).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !locked) {
                    match afrobim_kernel::layout::parse_scale(&self.lay.scale_text) {
                        Some(d) => pick = Some(d),
                        None => self.status = "Type a scale such as 1:750 or 750.".into(),
                    }
                }
            });
            if locked {
                ui_kit::callout(ui, ui_kit::Level::Warning, "The viewport is locked. Unlock it (VPLOCK) to change its scale.");
            } else {
                ui_kit::callout(ui, ui_kit::Level::Info, format!("At {} one metre of the model is {} mm on the paper.", afrobim_kernel::layout::scale_label(now), afrobim_kernel::layout::trim_mm(1000.0 / now)));
            }
        });
        if let Some(d) = pick {
            self.set_vp_scale(ctx, id, d);
        }
        if dlg.closed {
            self.lay.scale_open = false;
        }
    }

    fn vp_layers_window(&mut self, ctx: &Context) {
        let Some(lid) = self.lay.current else {
            self.lay.layers_open = false;
            return;
        };
        let vp = self.target_vp();
        let names: Vec<String> = self.doc.layers().iter().map(|l| l.name.clone()).filter(|n| !crate::anno_ui::hidden_layer(n)).collect();
        let (frozen_here, frozen_new): (Vec<String>, Vec<String>) = (
            self.target_viewport().map(|v| v.frozen.clone()).unwrap_or_default(),
            self.doc.layouts().get(lid).map(|l| l.new_viewport_frozen.clone()).unwrap_or_default(),
        );
        let mut toggles: Vec<(String, bool, bool)> = Vec::new(); // layer, new_viewports, freeze
        let dlg = ui_kit::Dialog::titled("Viewport layers").subtitle("Freeze layers in one viewport without touching the others").width(460.0).show_body(ctx, |ui| {
            if vp.is_none() {
                ui_kit::callout(ui, ui_kit::Level::Info, "Select a viewport to freeze layers in it. The second column applies to viewports made from now on.");
            }
            egui::Grid::new("vplayers").num_columns(3).striped(true).spacing([18.0, 4.0]).show(ui, |ui| {
                ui_kit::th(ui, "Layer");
                ui_kit::th(ui, "Frozen in this viewport");
                ui_kit::th(ui, "Frozen in new viewports");
                ui.end_row();
                for n in &names {
                    ui.label(n);
                    let mut here = frozen_here.contains(n);
                    if ui.add_enabled(vp.is_some(), egui::Checkbox::without_text(&mut here)).changed() {
                        toggles.push((n.clone(), false, here));
                    }
                    let mut new = frozen_new.contains(n);
                    if ui.checkbox(&mut new, "").changed() {
                        toggles.push((n.clone(), true, new));
                    }
                    ui.end_row();
                }
            });
        });
        for (layer, for_new, freeze) in toggles {
            if for_new {
                if let Some(l) = self.doc.layouts_mut().get_mut(lid) {
                    l.new_viewport_frozen.retain(|x| *x != layer);
                    if freeze {
                        l.new_viewport_frozen.push(layer);
                    }
                }
                self.schedule_commit(ctx);
            } else if let Some(id) = vp {
                self.with_vp(ctx, id, |v| if freeze { v.freeze(&layer) } else { v.thaw(&layer) });
            }
        }
        if dlg.closed {
            self.lay.layers_open = false;
        }
    }

    fn page_setup_window(&mut self, ctx: &Context) {
        let Some(form) = self.lay.page.clone() else { return };
        let Some(layout) = self.doc.layouts().get(form.layout).cloned() else {
            self.lay.page = None;
            return;
        };
        let mut l = layout.clone();
        let current = lp::paper_of(&l);
        let (mut size, mut orient) = current.unwrap_or((PaperSize::A3, if l.landscape { Orientation::Landscape } else { Orientation::Portrait }));
        let initial = (size, orient);
        let mut tb = l.title_block_mm > 0.0;
        let dlg = ui_kit::Dialog::titled("Page setup").subtitle(format!("Paper of the layout {}", l.name)).width(420.0).show_body(ctx, |ui| {
            ui_kit::form(ui, "pagesetup", |fm| {
                fm.row("Paper size", |ui| {
                    egui::ComboBox::from_id_salt("ps-size").selected_text(if current.is_some() { size.name() } else { l.size.clone() }).show_ui(ui, |ui| {
                        for s in PaperSize::ALL {
                            ui.selectable_value(&mut size, s, s.name());
                        }
                    });
                });
                fm.row("Orientation", |ui| {
                    ui.selectable_value(&mut orient, Orientation::Landscape, "Landscape");
                    ui.selectable_value(&mut orient, Orientation::Portrait, "Portrait");
                });
                fm.row("Paper", |ui| {
                    ui.label(RichText::new(format!("{} x {} mm", l.width_mm.round(), l.height_mm.round())).monospace());
                });
                fm.row("Margin left", |ui| {
                    ui_kit::f64_field(ui, &mut l.margins[0], "mm", 0.0..=100.0, 1.0);
                });
                fm.row("Margin right", |ui| {
                    ui_kit::f64_field(ui, &mut l.margins[1], "mm", 0.0..=100.0, 1.0);
                });
                fm.row("Margin top", |ui| {
                    ui_kit::f64_field(ui, &mut l.margins[2], "mm", 0.0..=100.0, 1.0);
                });
                fm.row("Margin bottom", |ui| {
                    ui_kit::f64_field(ui, &mut l.margins[3], "mm", 0.0..=100.0, 1.0);
                });
                fm.row("Title block", |ui| {
                    ui.checkbox(&mut tb, "AfroBIM title block on the right of the frame");
                });
            });
            ui_kit::callout(ui, ui_kit::Level::Info, "The dashed line on the layout is the printable area. Viewports stay where they are when the paper changes.");
        });
        if (size, orient) != initial {
            lp::set_paper(&mut l, size, orient);
        }
        l.title_block_mm = if tb { afrobim_sheets::TitleBlockLayout::default().width_mm } else { 0.0 };
        if l != layout {
            if let Some(slot) = self.doc.layouts_mut().get_mut(form.layout) {
                *slot = l;
            }
            self.lay.views.remove(&form.layout);
            self.schedule_commit(ctx);
        }
        if dlg.closed {
            self.lay.page = None;
        }
    }

    fn plot_window(&mut self, ctx: &Context) {
        let n = self.doc.layouts().len();
        let name = self.current_layout().map(|l| l.name.clone());
        let mut export: Option<bool> = None;
        let dlg = ui_kit::Dialog::titled("Plot layouts").subtitle("PDF or DXF at true paper size").width(460.0).show_body(ctx, |ui| {
            ui_kit::heading(ui, "What to plot");
            ui.radio_value(&mut self.lay.plot_all, false, format!("The current layout{}", name.as_ref().map(|n| format!(" ({n})")).unwrap_or_default()));
            ui.radio_value(&mut self.lay.plot_all, true, format!("All {n} layouts, one page each"));
            ui_kit::heading(ui, "Plot to");
            let can = if self.lay.plot_all { n > 0 } else { name.is_some() };
            ui.horizontal(|ui| {
                if ui.add_enabled(can, egui::Button::new("PDF...")).on_hover_text("Vector PDF, one page per layout, page size equal to the paper").clicked() {
                    export = Some(true);
                }
                if ui.add_enabled(can, egui::Button::new("DXF...")).on_hover_text("DXF R12 in paper millimetres").clicked() {
                    export = Some(false);
                }
            });
            if !self.lay.plot_note.is_empty() {
                ui_kit::callout(ui, ui_kit::Level::Info, self.lay.plot_note.clone());
            }
            ui_kit::callout(ui, ui_kit::Level::Info, "The viewport scale is exact on the paper: at 1:500 one metre is 2 mm. Viewport borders are not plotted. Civil objects, point clouds, images and map tiles are not in the plot yet.");
        });
        if let Some(pdf) = export {
            self.plot_ask(pdf);
        }
        if dlg.closed {
            self.lay.plot_open = false;
        }
    }

    /// The viewport's properties: in the Properties panel and in the VPPROPS window.
    pub(crate) fn viewport_props_ui(&mut self, ctx: &Context, ui: &mut Ui) {
        let Some(id) = self.target_vp() else {
            ui.label(RichText::new("No viewport selected.").weak());
            return;
        };
        let Some(mut v) = self.target_viewport().cloned() else { return };
        let before = v.clone();
        ui_kit::grid(ui, "vp-props", |ui| {
            ui.label("Viewport");
            ui.label(format!("{} on paper {} x {} mm", if v.clip.is_some() { "Polygon" } else { "Rectangle" }, afrobim_kernel::layout::trim_mm(v.width()), afrobim_kernel::layout::trim_mm(v.height())));
            ui.end_row();
            ui.label("Scale");
            ui.add_enabled_ui(!v.locked, |ui| {
                egui::ComboBox::from_id_salt(("vp-scale", id)).selected_text(v.scale_label()).show_ui(ui, |ui| {
                    for s in STANDARD_SCALES {
                        ui.selectable_value(&mut v.scale_den, s, afrobim_kernel::layout::scale_label(s));
                    }
                });
            });
            ui.end_row();
            ui.label("Custom scale 1:");
            ui.add_enabled(!v.locked, egui::DragValue::new(&mut v.scale_den).speed(1.0).range(0.001..=1.0e9));
            ui.end_row();
            ui.label("Paper per metre");
            ui.label(RichText::new(format!("{} mm", afrobim_kernel::layout::trim_mm(lp::viewport_scale(&v).paper_mm_per_m()))).monospace());
            ui.end_row();
            ui.label("Display locked");
            ui.checkbox(&mut v.locked, "");
            ui.end_row();
            ui.label("Display on");
            ui.checkbox(&mut v.on, "");
            ui.end_row();
            ui.label("Title");
            ui.text_edit_singleline(&mut v.title);
            ui.end_row();
            ui.label("Label");
            ui.checkbox(&mut v.show_label, "Print the title and the scale under the viewport");
            ui.end_row();
            ui.label("Frozen layers");
            ui.label(if v.frozen.is_empty() { "none".to_string() } else { v.frozen.join(", ") });
            ui.end_row();
        });
        // A locked viewport keeps the scale it had.
        if before.locked {
            v.scale_den = before.scale_den;
        }
        if v != before {
            self.with_vp(ctx, id, |slot| *slot = v);
        }
    }
}

/// The New layout and From template entries.
fn new_layout_menu(ui: &mut Ui, mut act: Option<TabAct>) -> Option<TabAct> {
    if ui.button("New layout").clicked() {
        act = Some(TabAct::New);
        ui.close_menu();
    }
    ui.menu_button("From template", |ui| {
        for size in PaperSize::ALL {
            for o in [Orientation::Landscape, Orientation::Portrait] {
                let (w, h) = afrobim_sheets::Page::new(size, o).dimensions_mm();
                if ui.button(format!("{} {} ({} x {} mm)", size.name(), if o == Orientation::Landscape { "landscape" } else { "portrait" }, w, h)).clicked() {
                    act = Some(TabAct::Template(size, o));
                    ui.close_menu();
                }
            }
        }
    });
    act
}

/// The right click menu of a layout tab.
fn layout_tab_menu(ui: &mut Ui, id: u64, index: usize, count: usize, mut act: Option<TabAct>) -> Option<TabAct> {
    act = new_layout_menu(ui, act);
    ui.separator();
    let mut item = |ui: &mut Ui, text: &str, enabled: bool, a: TabAct| {
        if ui.add_enabled(enabled, egui::Button::new(text)).clicked() {
            act = Some(a);
            ui.close_menu();
        }
    };
    item(ui, "Rename", true, TabAct::Rename(id));
    item(ui, "Delete...", true, TabAct::Delete(id));
    item(ui, "Duplicate", true, TabAct::Duplicate(id));
    item(ui, "Move left", index > 0, TabAct::Move(id, -1));
    item(ui, "Move right", index + 1 < count, TabAct::Move(id, 1));
    ui.separator();
    item(ui, "Page setup...", true, TabAct::PageSetup(id));
    item(ui, "Plot / Export...", true, TabAct::Plot(id));
    act
}

impl AfroBimApp {
    /// After undo or redo: a layout or viewport the restored state no longer has is let go of.
    pub(crate) fn lay_after_restore(&mut self) {
        if self.lay.current.is_some_and(|id| self.doc.layouts().get(id).is_none()) {
            self.lay.current = None;
        }
        let vp_ok = |app: &AfroBimApp, id: u64| app.current_layout().is_some_and(|l| l.viewport(id).is_some());
        if self.lay.selected.is_some_and(|id| !vp_ok(self, id)) {
            self.lay.selected = None;
        }
        if self.lay.active.is_some_and(|id| !vp_ok(self, id)) {
            self.lay.active = None;
        }
        if self.lay.maximised.as_ref().is_some_and(|m| !vp_ok(self, m.0)) {
            self.lay.maximised = None;
        }
        self.lay.drag = None;
        let live: Vec<u64> = self.doc.layouts().iter().map(|l| l.id).collect();
        self.lay.views.retain(|k, _| live.contains(k));
    }
}

impl AfroBimApp {
    /// The MODEL / PAPER toggle and the scale of the viewport, in the status bar of a layout tab.
    pub(crate) fn layout_status_controls(&mut self, ctx: &Context, ui: &mut Ui) {
        let in_model = self.lay.active.is_some() || self.lay.maximised.is_some();
        if ui.selectable_label(!in_model, RichText::new("PAPER").size(11.5).strong()).on_hover_text("Paper space: select and move viewports (PSPACE)").clicked() && in_model {
            self.deactivate_vp();
        }
        if ui.selectable_label(in_model, RichText::new("MODEL").size(11.5).strong()).on_hover_text("Model space through the selected viewport (MSPACE)").clicked() && !in_model {
            self.layout_run(ctx, Lv::Mspace);
        }
        if let (Some(id), Some(v)) = (self.target_vp(), self.target_viewport().cloned()) {
            let mut pick: Option<f64> = None;
            egui::ComboBox::from_id_salt("status-vp-scale").width(78.0).selected_text(RichText::new(v.scale_label()).monospace()).show_ui(ui, |ui| {
                for s in STANDARD_SCALES {
                    if ui.selectable_label((s - v.scale_den).abs() < 1e-9, afrobim_kernel::layout::scale_label(s)).clicked() {
                        pick = Some(s);
                    }
                }
            });
            if let Some(s) = pick {
                self.set_vp_scale(ctx, id, s);
            }
            let lock = if v.locked { egui_phosphor::regular::LOCK_SIMPLE } else { egui_phosphor::regular::LOCK_SIMPLE_OPEN };
            if ui.selectable_label(v.locked, lock).on_hover_text("Lock the viewport display (VPLOCK)").clicked() {
                self.layout_run(ctx, Lv::VpLock);
            }
        }
    }
}
