// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The layout canvas: the paper at true size on a grey ground, the viewports with the model in
//! them, the tools that make and reshape viewports, and model space through a viewport.
//!
//! The model is drawn by the same code as the Model tab (`paint_scene`, then the overlay for the
//! active viewport): this file only puts the viewport's transform in `self.view`, clips to its
//! boundary and puts the Model tab's view back afterwards. Nothing about drawing the model is
//! copied here.

use afrobim_kernel::{Pt, ViewTransform, Viewport};
use afrobim_sheets::layout_plot as lp;
use egui::{pos2, Align2, Color32, Context, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui};

use crate::app::AfroBimApp;
use crate::commands::Action;
use crate::layout_cmds::Lv;
use crate::layout_ui::PaperTool;

const PAPER: Color32 = Color32::from_rgb(0xFB, 0xFB, 0xF9);
const ACCENT: Color32 = Color32::from_rgb(0x2F, 0x7F, 0xE0);
const VP_EDGE: Color32 = Color32::from_rgb(0x6B, 0x7B, 0x8C);
const RUBBER: Color32 = Color32::from_rgb(0xFF, 0xB0, 0x30);

fn pp(q: Pt) -> Pos2 {
    pos2(q.x as f32, q.y as f32)
}

fn sp(pv: &ViewTransform, q: Pt) -> Pos2 {
    pp(pv.to_screen(q))
}

/// The screen rectangle of a paper box x0, y0, x1, y1.
fn screen_box(pv: &ViewTransform, r: [f64; 4]) -> Rect {
    Rect::from_two_pos(sp(pv, Pt::new(r[0], r[3])), sp(pv, Pt::new(r[2], r[1])))
}

/// The transform of the model through a viewport at the paper view `pv`.
pub fn vp_screen_view(pv: &ViewTransform, vp: &Viewport) -> ViewTransform {
    let at = pv.to_screen(vp.paper_centre());
    ViewTransform::centred_on(vp.centre, at, pv.scale * lp::paper_per_model(vp))
}

fn dashed_poly(painter: &Painter, pts: &[Pos2], closed: bool, stroke: Stroke) {
    let mut v = pts.to_vec();
    if closed && !v.is_empty() {
        v.push(v[0]);
    }
    painter.extend(Shape::dashed_line(&v, stroke, 5.0, 3.0));
}

impl AfroBimApp {
    /// The grey ground, paper, printable area, frame, title block, shapes and viewports of the
    /// layout in the current tab, and the work on them.
    pub(crate) fn canvas_layout(&mut self, ctx: &Context, ui: &mut Ui) {
        let Some(lid) = self.lay.current else { return };
        let Some(layout) = self.doc.layouts().get(lid).cloned() else {
            self.lay.current = None;
            return;
        };
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;
        let model_rect = (rect.left() as f64, rect.top() as f64, rect.width() as f64, rect.height() as f64);
        self.lay.model_rect = model_rect;
        self.sync_generated();

        // A viewport maximised to the canvas: the whole canvas is model space.
        if let Some((vid, mview)) = self.lay.maximised.clone() {
            let Some(vp) = layout.viewport(vid).cloned() else {
                self.lay.maximised = None;
                return;
            };
            let resp = ui.interact(rect, egui::Id::new(("vp-max", lid, vid)), Sense::click_and_drag());
            let (_, after) = self.with_model_view(mview, &vp.frozen, |app, v| {
                app.basemap.sync(ctx, &app.doc, v, model_rect);
                app.canvas_model_pass(ctx, &resp, &painter, rect, true);
            });
            self.lay.maximised = Some((vid, after));
            painter.text(pos2(rect.center().x, rect.top() + 14.0), Align2::CENTER_CENTER, format!("Viewport maximised ({}). VPMIN or Esc returns to the layout.", vp.scale_label()), FontId::proportional(12.0), Color32::from_gray(60));
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !self.ctl.is_busy() {
                self.minimise_vp();
            }
            self.ctl.viewport = model_rect;
            return;
        }

        let mut pv = match self.lay.views.get(&lid) {
            Some(v) => v.clone(),
            None if rect.width() > 50.0 => {
                let mut v = ViewTransform::default();
                v.fit(&afrobim_kernel::BBox::new(0.0, 0.0, layout.width_mm, layout.height_mm), rect.left() as f64, rect.top() as f64, rect.width() as f64, rect.height() as f64, 36.0);
                v
            }
            None => return,
        };
        let pointer = ctx.input(|i| i.pointer.latest_pos());
        let active_vp = self.lay.active.and_then(|id| layout.viewport(id)).cloned();
        if self.lay.active.is_some() && active_vp.is_none() {
            self.lay.active = None;
        }
        let active_clip = active_vp.as_ref().map(|v| screen_box(&pv, v.rect).intersect(rect));
        let pointer_in_active = matches!((pointer, &active_clip), (Some(p), Some(c)) if c.contains(p));

        // Pan and zoom of the paper: everywhere in paper space; outside the active viewport and
        // inside a locked one while working in model space.
        let nav_paper = response.contains_pointer() && (active_vp.is_none() || !pointer_in_active || active_vp.as_ref().is_some_and(|v| v.locked));
        self.paper_navigate(ctx, &mut pv, rect, nav_paper);

        self.paint_paper(&painter, rect, &pv, &layout);

        // The viewports. The active one is drawn by the model pass below.
        let active_id = self.lay.active;
        for vp in layout.viewports.iter().filter(|v| v.on && Some(v.id) != active_id) {
            self.paint_viewport_model(ctx, &painter, rect, &pv, vp);
        }
        for vp in &layout.viewports {
            if Some(vp.id) == self.lay.active {
                continue;
            }
            self.paint_viewport_border(&painter, &pv, vp);
        }

        if let (Some(vp), Some(clip)) = (active_vp.clone(), active_clip) {
            self.active_viewport_pass(ctx, ui, &painter, rect, &pv, lid, &vp, clip);
            self.paint_viewport_border(&painter, &pv, &vp);
        } else {
            self.paper_input(ctx, &response, &painter, &pv, &layout, rect);
        }
        self.paint_tool_overlay(&painter, &pv, &layout);
        self.lay.views.insert(lid, pv);
        self.ctl.viewport = model_rect;
    }

    /// Mouse wheel zoom and middle button pan of the paper, and a middle double click to fit.
    fn paper_navigate(&mut self, ctx: &Context, pv: &mut ViewTransform, rect: Rect, allowed: bool) {
        if !allowed {
            return;
        }
        let (pos, mid, delta, scroll, mid_double) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.button_down(egui::PointerButton::Middle), i.pointer.delta(), i.raw_scroll_delta.y, i.pointer.button_double_clicked(egui::PointerButton::Middle)));
        let Some(pos) = pos else { return };
        if mid {
            pv.pan(delta.x as f64, delta.y as f64);
        }
        if scroll != 0.0 {
            pv.zoom_about(Pt::new(pos.x as f64, pos.y as f64), (scroll as f64 * 0.0025).exp());
        }
        if mid_double {
            if let Some(l) = self.current_layout() {
                pv.fit(&afrobim_kernel::BBox::new(0.0, 0.0, l.width_mm, l.height_mm), rect.left() as f64, rect.top() as f64, rect.width() as f64, rect.height() as f64, 36.0);
            }
        }
    }

    /// Runs `f` with `view` as the model view, the grid off, the light canvas and the viewport's
    /// frozen layers hidden, then puts everything back. Returns what `f` returned and the view as
    /// `f` left it (pan and zoom inside the pass).
    pub(crate) fn with_model_view<R>(&mut self, view: ViewTransform, frozen: &[String], f: impl FnOnce(&mut Self, &ViewTransform) -> R) -> (R, ViewTransform) {
        let saved_view = std::mem::replace(&mut self.view, view);
        let saved_grid = std::mem::replace(&mut self.grid_on, false);
        let saved_dark = std::mem::replace(&mut self.dark_canvas, false);
        let mut was: Vec<(usize, bool)> = Vec::new();
        for (i, l) in self.doc.layers_mut().iter_mut().enumerate() {
            if l.visible && frozen.contains(&l.name) {
                was.push((i, true));
                l.visible = false;
            }
        }
        let v = self.view.clone();
        let r = f(self, &v);
        for (i, vis) in was {
            if let Some(l) = self.doc.layers_mut().get_mut(i) {
                l.visible = vis;
            }
        }
        self.grid_on = saved_grid;
        self.dark_canvas = saved_dark;
        let after = std::mem::replace(&mut self.view, saved_view);
        (r, after)
    }

    // ─── painting ────────────────────────────────────────────────────────────

    fn paint_paper(&self, painter: &Painter, rect: Rect, pv: &ViewTransform, layout: &afrobim_kernel::Layout) {
        let dark = self.dark_canvas;
        painter.rect_filled(rect, 0.0, if dark { Color32::from_rgb(0x2A, 0x2E, 0x34) } else { Color32::from_rgb(0x8E, 0x93, 0x99) });
        let paper = screen_box(pv, layout.paper_rect());
        let clip = painter.clip_rect();
        // The shadow: the paper's shape, moved down and right, in translucent black.
        let off = (5.0 * (pv.scale as f32 / 2.0).clamp(0.4, 1.6)).max(2.0);
        painter.rect_filled(paper.translate(egui::vec2(off, off)).intersect(clip.expand(20.0)), 0.0, Color32::from_black_alpha(90));
        painter.rect_filled(paper.intersect(clip.expand(20.0)), 0.0, PAPER);
        painter.rect_stroke(paper, 0.0, Stroke::new(1.0_f32, Color32::from_gray(120)), egui::StrokeKind::Outside);

        // The printable area: dashed.
        let printable = screen_box(pv, layout.printable_rect());
        dashed_poly(painter, &[printable.left_top(), printable.right_top(), printable.right_bottom(), printable.left_bottom()], true, Stroke::new(1.0_f32, Color32::from_rgb(0x90, 0x60, 0x60)));

        // Frame, title block: composed by the plot code, so the screen and the print agree.
        let info = afrobim_sheets::project_info(&self.doc);
        let model = afrobim_sheets::Model { doc: &self.doc, registry: &self.modules };
        let mut furniture = layout.clone();
        furniture.viewports.clear();
        furniture.shapes.clear();
        let pos = self.doc.layouts().index_of(layout.id).map(|i| i + 1).unwrap_or(1);
        let sheet = lp::compose_layout(&model, &afrobim_sheets::Styles::default(), &info, &furniture, pos, self.doc.layouts().len().max(1), &mut |_, _, _| {});
        paint_items(painter, pv, &sheet.items);

        // What was drawn on the paper.
        for s in &layout.shapes {
            let pts: Vec<Pos2> = s.points().iter().map(|p| sp(pv, *p)).collect();
            let selected = self.lay.tool == PaperTool::Object;
            painter.add(Shape::line(pts, Stroke::new(1.5_f32, if selected { RUBBER } else { Color32::from_rgb(0x30, 0x60, 0x90) })));
        }
        if layout.viewports.is_empty() && layout.shapes.is_empty() && self.lay.tool.is_none() {
            painter.text(
                pos2(paper.center().x, paper.center().y),
                Align2::CENTER_CENTER,
                "No viewports yet. Right click for MVIEW, or use Output, Viewports.",
                FontId::proportional(13.0),
                Color32::from_gray(120),
            );
        }
    }

    /// The model inside one viewport (not the active one): the model painter with the viewport's
    /// transform, clipped to its boundary box and then to its polygon.
    fn paint_viewport_model(&mut self, ctx: &Context, painter: &Painter, rect: Rect, pv: &ViewTransform, vp: &Viewport) {
        let srect = screen_box(pv, vp.rect);
        let clip = srect.intersect(rect).intersect(painter.clip_rect());
        if !(clip.width() > 1.0 && clip.height() > 1.0) {
            return;
        }
        let view = vp_screen_view(pv, vp);
        let vr = (srect.left() as f64, srect.top() as f64, srect.width() as f64, srect.height() as f64);
        let pt = painter.with_clip_rect(clip);
        self.with_model_view(view, &vp.frozen, |app, v| {
            app.basemap.sync(ctx, &app.doc, v, vr);
            app.paint_scene(&pt, srect);
        });
        if vp.clip.is_some() {
            mask_outside_polygon(painter, pv, vp, clip);
        }
    }

    fn paint_viewport_border(&self, painter: &Painter, pv: &ViewTransform, vp: &Viewport) {
        let pts: Vec<Pos2> = vp.outline().iter().map(|p| sp(pv, *p)).collect();
        let active = self.lay.active == Some(vp.id);
        let selected = self.lay.selected == Some(vp.id);
        if active {
            painter.add(Shape::closed_line(pts.clone(), Stroke::new(3.5_f32, ACCENT)));
        } else if !vp.on {
            dashed_poly(painter, &pts, true, Stroke::new(1.0_f32, Color32::from_gray(140)));
        } else {
            painter.add(Shape::closed_line(pts.clone(), Stroke::new(1.0_f32, VP_EDGE)));
        }
        if selected && !active {
            dashed_poly(painter, &pts, true, Stroke::new(1.5_f32, ACCENT));
        }
        let b = screen_box(pv, vp.rect);
        if vp.locked {
            let c = Rect::from_min_size(pos2(b.right() - 20.0, b.top() + 2.0), egui::vec2(18.0, 18.0));
            painter.rect_filled(c, 3.0, Color32::from_rgba_unmultiplied(255, 255, 255, 200));
            painter.text(c.center(), Align2::CENTER_CENTER, egui_phosphor::regular::LOCK_SIMPLE, FontId::proportional(14.0), Color32::from_rgb(0x9A, 0x30, 0x30));
        }
        if !vp.on {
            painter.text(b.center(), Align2::CENTER_CENTER, "display off", FontId::proportional(11.0), Color32::from_gray(120));
        }
        if vp.show_label {
            let ink = Color32::from_gray(30);
            let mut y = b.bottom() + 4.0;
            if !vp.title.trim().is_empty() {
                let size = (3.5 * pv.scale as f32 / afrobim_sheets::CAP_HEIGHT as f32).clamp(8.0, 40.0);
                painter.text(pos2(b.center().x, y), Align2::CENTER_TOP, &vp.title, FontId::proportional(size), ink);
                y += size + 2.0;
            }
            let size = (2.5 * pv.scale as f32 / afrobim_sheets::CAP_HEIGHT as f32).clamp(7.0, 30.0);
            painter.text(pos2(b.center().x, y), Align2::CENTER_TOP, vp.scale_label(), FontId::proportional(size), ink);
        }
        // Handles of the selected viewport.
        if selected && !active {
            for (_, g) in vp.grips() {
                let s = sp(pv, g);
                let r = Rect::from_center_size(pos2(s.x.round(), s.y.round()), egui::vec2(10.0, 10.0));
                painter.rect_filled(r, 0.0, ACCENT);
                painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::WHITE), egui::StrokeKind::Inside);
            }
        }
    }

    fn paint_tool_overlay(&self, painter: &Painter, pv: &ViewTransform, layout: &afrobim_kernel::Layout) {
        let _ = layout;
        let (tool, cur) = (&self.lay.tool, self.lay.cursor_paper);
        let Some(cur) = cur else { return };
        let st = Stroke::new(1.5_f32, RUBBER);
        let cs = sp(pv, cur);
        let poly_preview = |pts: &[Pt], closed: bool| {
            let mut v: Vec<Pos2> = pts.iter().map(|p| sp(pv, *p)).collect();
            v.push(cs);
            if closed && v.len() > 2 {
                v.push(v[0]);
            }
            painter.extend(Shape::dashed_line(&v, st, 5.0, 3.0));
        };
        match tool {
            PaperTool::Rect { first: Some(a) } | PaperTool::DrawRect { first: Some(a) } => {
                let r = Rect::from_two_pos(sp(pv, *a), cs);
                dashed_poly(painter, &[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()], true, st);
            }
            PaperTool::DrawCircle { centre: Some(c) } => {
                let (o, rad) = (sp(pv, *c), (cs - sp(pv, *c)).length());
                painter.circle_stroke(o, rad, st);
            }
            PaperTool::Poly { pts } | PaperTool::Clip { pts, .. } | PaperTool::DrawPoly { pts } if !pts.is_empty() => poly_preview(pts, true),
            _ => {}
        }
        if !tool.is_none() {
            let g = painter.layout_no_wrap(tool.prompt().to_owned(), FontId::proportional(11.5), Color32::from_gray(240));
            let at = cs + egui::vec2(16.0, 18.0);
            let r = Rect::from_min_size(at, g.size()).expand2(egui::vec2(6.0, 3.0));
            painter.rect_filled(r, 3.0, Color32::from_rgba_unmultiplied(16, 18, 22, 230));
            painter.galley(at, g, Color32::WHITE);
            painter.line_segment([pos2(cs.x - 12.0, cs.y), pos2(cs.x + 12.0, cs.y)], Stroke::new(1.0_f32, Color32::BLACK));
            painter.line_segment([pos2(cs.x, cs.y - 12.0), pos2(cs.x, cs.y + 12.0)], Stroke::new(1.0_f32, Color32::BLACK));
        }
    }

    // ─── model space through a viewport ──────────────────────────────────────

    /// The active viewport: the model painter and the model tools run in its boundary box with
    /// its transform; pan and zoom change the viewport's view (unless it is locked, when they
    /// change the paper instead).
    #[allow(clippy::too_many_arguments)]
    fn active_viewport_pass(&mut self, ctx: &Context, ui: &mut Ui, painter: &Painter, rect: Rect, pv: &ViewTransform, lid: u64, vp: &Viewport, clip: Rect) {
        let srect = screen_box(pv, vp.rect);
        let view = vp_screen_view(pv, vp);
        // A double click on the paper outside the boundary returns to paper space.
        let (pos, dbl) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.button_double_clicked(egui::PointerButton::Primary)));
        if let (Some(pos), true) = (pos, dbl) {
            let p = pv.to_world(Pt::new(pos.x as f64, pos.y as f64));
            if rect.contains(pos) && (!clip.contains(pos) || !vp.contains_paper(p)) {
                self.deactivate_vp();
                if let Some(other) = self.current_layout().and_then(|l| l.viewport_at(p)) {
                    self.lay.selected = Some(other);
                }
                return;
            }
        }
        let resp = ui.interact(clip, egui::Id::new(("vp-model", lid, vp.id)), Sense::click_and_drag());
        let pt = painter.with_clip_rect(clip);
        let nav = !vp.locked;
        let vr = (srect.left() as f64, srect.top() as f64, srect.width() as f64, srect.height() as f64);
        let (_, after) = self.with_model_view(view.clone(), &vp.frozen, |app, v| {
            app.basemap.sync(ctx, &app.doc, v, vr);
            app.canvas_model_pass(ctx, &resp, &pt, srect, nav);
        });
        if vp.clip.is_some() {
            mask_outside_polygon(painter, pv, vp, clip);
        }
        if nav {
            // Pan or zoom inside the viewport changes what it shows.
            let centre = after.to_world(pv.to_screen(vp.paper_centre()));
            let den = pv.scale / after.scale;
            let moved_px = ((centre.x - vp.centre.x).powi(2) + (centre.y - vp.centre.y).powi(2)).sqrt() * after.scale;
            let rescaled = ((den - vp.scale_den) / vp.scale_den).abs() > 1e-9;
            if (moved_px > 0.01 || rescaled) && den.is_finite() && den > 0.0 {
                self.with_vp(ctx, vp.id, |v| {
                    v.centre = centre;
                    if rescaled {
                        v.scale_den = den;
                    }
                });
            }
        }
    }

    // ─── paper space input ───────────────────────────────────────────────────

    fn paper_input(&mut self, ctx: &Context, response: &egui::Response, painter: &Painter, pv: &ViewTransform, layout: &afrobim_kernel::Layout, rect: Rect) {
        let _ = (painter, rect);
        let (pos, pressed, released, down, dbl, secondary) = ctx.input(|i| {
            (
                i.pointer.latest_pos(),
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.pointer.button_released(egui::PointerButton::Primary),
                i.pointer.button_down(egui::PointerButton::Primary),
                i.pointer.button_double_clicked(egui::PointerButton::Primary),
                i.pointer.button_pressed(egui::PointerButton::Secondary),
            )
        });
        let over = response.contains_pointer();
        self.lay.cursor_paper = None;
        if over {
            ctx.set_cursor_icon(if self.lay.tool.is_none() { egui::CursorIcon::Default } else { egui::CursorIcon::Crosshair });
        }
        if let Some(pos) = pos.filter(|_| over || self.lay.drag.is_some()) {
            let p = pv.to_world(Pt::new(pos.x as f64, pos.y as f64));
            self.lay.cursor_paper = Some(p);
            let tol_mm = 8.0 / pv.scale;
            if pressed && over {
                ctx.memory_mut(|m| m.request_focus(egui::Id::new(crate::app::CMD_ID)));
                if !self.lay.tool.is_none() {
                    self.tool_click(ctx, p, tol_mm);
                } else {
                    // A handle of the selected viewport first, then a viewport under the pointer.
                    let grip = self.lay.selected.and_then(|id| layout.viewport(id)).and_then(|v| {
                        v.grips().into_iter().find(|(_, g)| (sp(pv, *g) - pos).length() <= 7.0).map(|(k, g)| (v.id, k, g))
                    });
                    match grip {
                        Some((id, k, g)) => self.lay.drag = Some((id, k, if k == afrobim_kernel::VpGrip::Move { p } else { g })),
                        None => {
                            self.lay.selected = layout.viewport_at(p);
                            if let (Some(id), true) = (self.lay.selected, true) {
                                // Dragging from inside moves the viewport, as the move handle does.
                                self.lay.drag = Some((id, afrobim_kernel::VpGrip::Move, p));
                            }
                        }
                    }
                }
            }
            if let Some((id, kind, last)) = self.lay.drag {
                if down {
                    if (p.x - last.x).abs() > 1e-9 || (p.y - last.y).abs() > 1e-9 {
                        self.with_vp(ctx, id, |v| v.drag_grip(kind, last, p));
                        self.lay.drag = Some((id, kind, p));
                    }
                }
                if released || !down {
                    self.lay.drag = None;
                }
            }
            if dbl && over && self.lay.tool.is_none() {
                match layout.viewport_at(p) {
                    Some(id) => {
                        self.lay.drag = None;
                        self.activate_vp(id);
                    }
                    None => self.lay.selected = None,
                }
            }
            if secondary && over {
                if self.lay.tool.collects_points() {
                    self.tool_finish(ctx);
                } else if self.lay.tool.is_none() {
                    self.lay.selected = layout.viewport_at(p);
                }
            }
        }
        if !self.lay.tool.collects_points() && self.lay.tool.is_none() {
            let mut pick: Option<Action> = None;
            response.context_menu(|ui| {
                pick = self.paper_context_menu(ui);
                if pick.is_some() {
                    ui.close_menu();
                }
            });
            if let Some(a) = pick {
                self.run(ctx, a);
            }
        }
    }

    /// The right click menu of the paper: viewport commands when one is selected.
    fn paper_context_menu(&mut self, ui: &mut Ui) -> Option<Action> {
        let pick: std::cell::Cell<Option<Action>> = std::cell::Cell::new(None);
        let item = |ui: &mut Ui, label: &str, lv: Lv| {
            if ui.button(label).clicked() {
                pick.set(Some(Action::Layout(lv)));
            }
        };
        if self.lay.selected.is_some() {
            item(ui, "Model space (MSPACE)", Lv::Mspace);
            item(ui, "Scale...", Lv::VpScale);
            item(ui, "Lock or unlock display", Lv::VpLock);
            item(ui, "Display on or off", Lv::VpDisplay);
            item(ui, "Maximise (VPMAX)", Lv::VpMax);
            item(ui, "Layers in this viewport...", Lv::VpLayer);
            ui.separator();
            item(ui, "Clip boundary (VPCLIP)", Lv::VpClip);
            item(ui, "Remove clip", Lv::VpUnclip);
            item(ui, "Copy", Lv::VpCopy);
            item(ui, "Delete", Lv::VpDelete);
            item(ui, "Properties...", Lv::VpProps);
            ui.separator();
        }
        item(ui, "New viewport (MVIEW)", Lv::Mview);
        item(ui, "Polygonal viewport", Lv::MviewPoly);
        item(ui, "Viewport from object", Lv::MviewObject);
        item(ui, "Viewport to fit the page", Lv::MviewFit);
        ui.menu_button("Viewport arrangement", |ui| {
            for n in 1..=4u8 {
                if ui.button(format!("{n} viewport{}", if n > 1 { "s" } else { "" })).clicked() {
                    pick.set(Some(Action::Layout(Lv::MviewPreset(n))));
                    ui.close_menu();
                }
            }
        });
        ui.separator();
        item(ui, "Draw a rectangle on the paper", Lv::PaperRect);
        item(ui, "Draw a circle on the paper", Lv::PaperCircle);
        item(ui, "Draw a closed polyline on the paper", Lv::PaperPoly);
        ui.separator();
        item(ui, "Page setup...", Lv::PageSetup);
        item(ui, "Plot...", Lv::Plot);
        pick.get()
    }
}

/// Paints composed sheet items (strokes, fills, texts) on the paper view.
fn paint_items(painter: &Painter, pv: &ViewTransform, items: &[afrobim_sheets::Item]) {
    use afrobim_sheets::{Align, Item};
    for item in items {
        match item {
            Item::Stroke { points, closed, weight_mm, rgb, .. } => {
                let pts: Vec<Pos2> = points.iter().map(|p| sp(pv, *p)).collect();
                let w = (*weight_mm as f32 * pv.scale as f32).clamp(0.6, 6.0);
                let st = Stroke::new(w, Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
                painter.add(if *closed { Shape::closed_line(pts, st) } else { Shape::line(pts, st) });
            }
            Item::Fill { rings, rgb, .. } => {
                for r in rings.iter().filter(|r| r.len() >= 3) {
                    let pts: Vec<Pos2> = r.iter().map(|p| sp(pv, *p)).collect();
                    painter.add(Shape::convex_polygon(pts, Color32::from_rgb(rgb[0], rgb[1], rgb[2]), Stroke::NONE));
                }
            }
            Item::Text(t) => {
                let size = (t.height_mm * pv.scale / afrobim_sheets::CAP_HEIGHT) as f32;
                if size < 3.5 {
                    continue; // too small to read: skipped, as a viewer does
                }
                let font = FontId::proportional(size.min(200.0));
                let colour = Color32::from_rgb(t.rgb[0], t.rgb[1], t.rgb[2]);
                let galley = painter.layout_no_wrap(t.text.clone(), font, colour);
                let w = galley.size().x;
                let frac = match t.align {
                    Align::Left => 0.0,
                    Align::Centre => 0.5,
                    Align::Right => 1.0,
                };
                // From the baseline anchor to the galley's top left, turned by the text angle.
                let phi = (-t.rotation_deg).to_radians() as f32;
                let (dx, dy) = (-frac * w, -0.8 * galley.size().y);
                let a = sp(pv, t.at);
                let origin = pos2(a.x + dx * phi.cos() - dy * phi.sin(), a.y + dx * phi.sin() + dy * phi.cos());
                painter.add(egui::epaint::TextShape::new(origin, galley, colour).with_angle(phi));
            }
        }
    }
}

/// Covers the part of `bounds` outside the viewport polygon with the paper colour.
fn mask_outside_polygon(painter: &Painter, pv: &ViewTransform, vp: &Viewport, bounds: Rect) {
    let poly: Vec<Pos2> = vp.outline().iter().map(|p| sp(pv, *p)).collect();
    if poly.len() < 3 {
        return;
    }
    let step = if bounds.height() > 700.0 { 2.0 } else { 1.0 };
    let mut y = bounds.top();
    while y < bounds.bottom() {
        let yc = y + step / 2.0;
        let mut xs: Vec<f32> = Vec::new();
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            if (a.y <= yc) != (b.y <= yc) {
                xs.push(a.x + (yc - a.y) * (b.x - a.x) / (b.y - a.y));
            }
        }
        xs.sort_by(f32::total_cmp);
        let mut cursor = bounds.left();
        let h = step.min(bounds.bottom() - y);
        for pair in xs.chunks(2).filter(|p| p.len() == 2) {
            if pair[0] > cursor {
                painter.rect_filled(Rect::from_min_max(pos2(cursor, y), pos2(pair[0].min(bounds.right()), y + h)), 0.0, PAPER);
            }
            cursor = cursor.max(pair[1]);
        }
        if cursor < bounds.right() {
            painter.rect_filled(Rect::from_min_max(pos2(cursor, y), pos2(bounds.right(), y + h)), 0.0, PAPER);
        }
        y += step;
    }
}
