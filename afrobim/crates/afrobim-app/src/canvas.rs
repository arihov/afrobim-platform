//! Model-space painting: grid, axes, elements, and the CAD cursor furniture on top.

use afrobim_kernel::{display_spacing_fine, BBox, CanvasTool, Pt};
use egui::{pos2, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2};

use crate::app::AfroBimApp;
use crate::render::Representation;

const SELECT_BLUE: Color32 = Color32::from_rgb(0x4D, 0xA3, 0xFF);
const GRIP_FILL: Color32 = Color32::from_rgb(0x2F, 0x7F, 0xE0);
const GRIP_HOT: Color32 = Color32::from_rgb(0xFF, 0x5A, 0x36);
const CROSSING_GREEN: Color32 = Color32::from_rgb(0x40, 0xC8, 0x70);
const RUBBER: Color32 = Color32::from_rgb(0xFF, 0xB0, 0x30);
const AXIS_X: Color32 = Color32::from_rgb(0xD0, 0x50, 0x50);
const AXIS_Y: Color32 = Color32::from_rgb(0x50, 0xB0, 0x60);

fn p(q: Pt) -> Pos2 {
    pos2(q.x as f32, q.y as f32)
}

fn outline(rect: Rect, stroke: Stroke) -> Shape {
    Shape::closed_line(vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()], stroke)
}

/// A dashed rectangle. Each edge is clipped to the visible area first (in f64), so a box that is
/// millions of pixels wide at a deep zoom neither makes millions of dashes nor loses precision.
fn dashed(painter: &Painter, rect: Rect, stroke: Stroke) {
    let clip = painter.clip_rect().expand(30.0);
    let (lo, hi) = (Pt::new(clip.left() as f64, clip.top() as f64), Pt::new(clip.right() as f64, clip.bottom() as f64));
    let c = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
    for w in c.windows(2) {
        let (a, b) = (Pt::new(w[0].x as f64, w[0].y as f64), Pt::new(w[1].x as f64, w[1].y as f64));
        if let Some((a, b)) = crate::render::clip_segment_box(a, b, lo, hi) {
            painter.extend(Shape::dashed_line(&[p(a), p(b)], stroke, 5.0, 3.0));
        }
    }
}

fn tip(painter: &Painter, at: Pos2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(11.5), Color32::from_gray(240));
    let rect = Rect::from_min_size(at, galley.size()).expand2(Vec2::new(6.0, 3.0));
    painter.rect_filled(rect, 3.0, Color32::from_rgba_unmultiplied(16, 18, 22, 230));
    painter.galley(at, galley, Color32::WHITE);
}

impl AfroBimApp {
    fn screen_rect(&self, b: &BBox) -> Rect {
        let a = self.view.to_screen(Pt::new(b.min.x, b.max.y));
        let c = self.view.to_screen(Pt::new(b.max.x, b.min.y));
        Rect::from_min_max(p(a), p(c))
    }

    pub(crate) fn paint_scene(&self, painter: &Painter, rect: Rect) {
        let dark = self.dark_canvas;
        painter.rect_filled(rect, 0.0, if dark { Color32::from_rgb(0x1F, 0x23, 0x29) } else { Color32::from_rgb(0xF2, 0xF2, 0xEE) });
        let ink = if dark { Color32::from_gray(242) } else { Color32::from_gray(26) };

        if self.grid_on {
            let step = display_spacing_fine(self.grid_spacing, self.view.scale, 14.0);
            let (minor, major) = if dark {
                (Color32::from_rgb(0x26, 0x2B, 0x32), Color32::from_rgb(0x31, 0x37, 0x40))
            } else {
                (Color32::from_rgb(0xE6, 0xE6, 0xE1), Color32::from_rgb(0xD4, 0xD4, 0xCE))
            };
            let min = self.view.to_world(Pt::new(rect.left() as f64, rect.bottom() as f64));
            let max = self.view.to_world(Pt::new(rect.right() as f64, rect.top() as f64));
            let (ix0, ix1) = ((min.x / step).ceil() as i64, (max.x / step).floor() as i64);
            let (iy0, iy1) = ((min.y / step).ceil() as i64, (max.y / step).floor() as i64);
            if ix1 - ix0 < 600 && iy1 - iy0 < 600 {
                for i in ix0..=ix1 {
                    let x = self.view.to_screen(Pt::new(i as f64 * step, 0.0)).x.round() as f32 + 0.5;
                    painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0_f32, if i % 5 == 0 { major } else { minor }));
                }
                for i in iy0..=iy1 {
                    let y = self.view.to_screen(Pt::new(0.0, i as f64 * step)).y.round() as f32 + 0.5;
                    painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0_f32, if i % 5 == 0 { major } else { minor }));
                }
            }
        }

        // Axes through the model origin.
        // (Only when the origin is on screen: far from it the axes are millions of pixels away.)
        let o = self.view.to_screen(Pt::new(0.0, 0.0));
        let (ox, oy) = (o.x.round() as f32 + 0.5, o.y.round() as f32 + 0.5);
        if o.y >= rect.top() as f64 && o.y <= rect.bottom() as f64 {
            painter.line_segment([pos2(rect.left(), oy), pos2(rect.right(), oy)], Stroke::new(1.0_f32, AXIS_X.gamma_multiply(0.7)));
        }
        if o.x >= rect.left() as f64 && o.x <= rect.right() as f64 {
            painter.line_segment([pos2(ox, rect.top()), pos2(ox, rect.bottom())], Stroke::new(1.0_f32, AXIS_Y.gamma_multiply(0.7)));
        }

        // DXF underlays sit behind the model.
        self.under.paint(&self.doc, painter, &self.view, rect, ink);

        let label = if dark { Color32::from_rgb(0xA8, 0xB0, 0xBA) } else { Color32::from_rgb(0x50, 0x55, 0x5C) };
        // Layer colour and line weight: black is "automatic" (the canvas ink); the weight is drawn
        // as a pen width in pixels (about 4 px per millimetre, between 1 and 6).
        let layer_props = afrobim_annotate::settings::load(&self.doc).layers;
        for e in self.doc.elements() {
            if !self.doc.layer_visible(&e.layer) {
                continue;
            }
            if e.category == crate::underlay::CATEGORY || e.category == afrobim_annotate::SETTINGS_CATEGORY {
                continue;
            }
            let (ink, width) = match layer_props.get(&e.layer) {
                Some(p) => (if p.colour == [0, 0, 0] { ink } else { Color32::from_rgb(p.colour[0], p.colour[1], p.colour[2]) }, if p.lineweight_mm > 0.0 { (1.0 + p.lineweight_mm * 4.0).min(6.0) as f32 } else { 1.0 }),
                None => (ink, 1.0),
            };
            // Annotation is drawn as outlines so that arrow heads show on a dark canvas too.
            let rep = if afrobim_annotate::is_annotation(e) { Representation::Outlined } else { self.rep };
            let r = self.screen_rect(&e.bounds());
            if !r.intersects(rect) {
                continue;
            }
            match self.cache.get(&e.id) {
                Some(c) if c.generated.is_ok() => painter.extend(c.mesh.shapes_w(&self.view, e.origin, rep, ink, 1.0, width)),
                _ => {
                    // No geometry (module missing or the definition is invalid): keep it visible and pickable.
                    dashed(painter, r, Stroke::new(1.0_f32, label));
                    painter.line_segment([r.left_top(), r.right_bottom()], Stroke::new(1.0_f32, label));
                    painter.line_segment([r.right_top(), r.left_bottom()], Stroke::new(1.0_f32, label));
                }
            }
            if self.show_labels && r.width() > 46.0 {
                painter.text(pos2(r.left(), r.bottom() + 4.0), Align2::LEFT_TOP, &e.name, FontId::proportional(11.0), label);
            }
        }
    }

    pub(crate) fn paint_overlay(&self, painter: &Painter, rect: Rect, cursor_inside: bool) {
        let c = &self.ctl;
        let ink = if self.dark_canvas { Color32::WHITE } else { Color32::BLACK };

        // Hover highlight: what a click would pick.
        if cursor_inside && matches!(c.tool(), CanvasTool::Idle | CanvasTool::SelectObjects) {
            if let Some(e) = c.hover().filter(|id| !self.doc.is_selected(*id)).and_then(|id| self.doc.find(id)) {
                painter.add(outline(self.screen_rect(&e.bounds()).expand(2.0), Stroke::new(2.0_f32, SELECT_BLUE.gamma_multiply(0.6))));
            }
        }

        // Selection: dashed outline plus grips.
        for id in self.doc.selection() {
            let Some(e) = self.doc.find(*id) else { continue };
            dashed(painter, self.screen_rect(&e.bounds()).expand(2.0), Stroke::new(1.5_f32, SELECT_BLUE));
            if afrobim_annotate::is_annotation(e) {
                // An annotation's grips are its definition points (see `paint_anno_grips`).
                continue;
            }
            for (i, g) in e.grips().iter().enumerate() {
                let s = p(self.view.to_screen(*g));
                let hot = c.hover_grip() == Some((*id, i));
                let r = Rect::from_center_size(pos2(s.x.round(), s.y.round()), Vec2::splat(2.0 * c.grip_px as f32));
                painter.rect_filled(r, 0.0, if hot { GRIP_HOT } else { GRIP_FILL });
                painter.add(outline(r, Stroke::new(1.0_f32, Color32::WHITE)));
            }
        }

        self.paint_anno_grips(painter);

        // Ghosts: what is being moved, copied or placed, at its previewed position.
        if matches!(c.tool(), CanvasTool::DragMove | CanvasTool::GripDrag | CanvasTool::SecondPoint | CanvasTool::Place) {
            let d = c.displacement();
            for (n, id) in c.ghost_ids().iter().enumerate() {
                let Some(e) = self.doc.find(*id) else { continue };
                let origin = e.origin + d;
                if n < 25 {
                    if let Some(cached) = self.cache.get(id).filter(|c| c.generated.is_ok()) {
                        painter.extend(cached.mesh.shapes(&self.view, origin, self.rep, ink, 0.55));
                    }
                }
                let b = BBox::new(origin.x, origin.y, origin.x + e.width, origin.y + e.height);
                dashed(painter, self.screen_rect(&b), Stroke::new(1.0_f32, RUBBER));
            }
            if let (Some(base), true) = (c.base_point(), c.tool() != CanvasTool::Place) {
                let (a, b) = (p(self.view.to_screen(base)), p(self.view.to_screen(base + d)));
                painter.extend(Shape::dashed_line(&[a, b], Stroke::new(1.0_f32, RUBBER), 5.0, 3.0));
                let u = crate::anno_ui::units_of(&self.doc);
                tip(painter, b + Vec2::new(16.0, 18.0), &format!("Δ {} , {}   {}", u.length(d.x), u.length(d.y), u.length_u((d.x * d.x + d.y * d.y).sqrt())));
            }
        }

        // Selection or zoom box.
        if let (Some(start), true) = (c.box_start(), matches!(c.tool(), CanvasTool::BoxSecond | CanvasTool::ZoomSecond)) {
            let r = Rect::from_two_pos(p(start), p(c.cursor_screen()));
            if c.tool() == CanvasTool::ZoomSecond {
                dashed(painter, r, Stroke::new(1.0_f32, ink));
            } else if c.box_is_crossing() {
                painter.rect_filled(r, 0.0, CROSSING_GREEN.gamma_multiply(0.2));
                dashed(painter, r, Stroke::new(1.0_f32, CROSSING_GREEN));
            } else {
                painter.rect_filled(r, 0.0, SELECT_BLUE.gamma_multiply(0.2));
                painter.add(outline(r, Stroke::new(1.0_f32, SELECT_BLUE)));
            }
        }

        // UCS icon.
        let (x0, y0) = (rect.left() + 22.0, rect.bottom() - 22.0);
        painter.line_segment([pos2(x0, y0), pos2(x0 + 34.0, y0)], Stroke::new(1.5_f32, AXIS_X));
        painter.line_segment([pos2(x0, y0), pos2(x0, y0 - 34.0)], Stroke::new(1.5_f32, AXIS_Y));
        painter.text(pos2(x0 + 38.0, y0), Align2::LEFT_CENTER, "X", FontId::proportional(10.0), ink);
        painter.text(pos2(x0, y0 - 38.0), Align2::CENTER_BOTTOM, "Y", FontId::proportional(10.0), ink);

        if !cursor_inside {
            return;
        }

        // Snap marker: where the point will actually land.
        if matches!(c.tool(), CanvasTool::BasePoint | CanvasTool::SecondPoint | CanvasTool::GripDrag) && (c.snap_on || c.ortho_on) {
            let s = p(self.view.to_screen(c.cursor_world()));
            let st = Stroke::new(1.5_f32, RUBBER);
            painter.line_segment([s + Vec2::new(-5.0, -5.0), s + Vec2::new(5.0, 5.0)], st);
            painter.line_segment([s + Vec2::new(-5.0, 5.0), s + Vec2::new(5.0, -5.0)], st);
        }

        // Crosshair with pickbox.
        let (cx, cy) = (c.cursor_screen().x.round() as f32 + 0.5, c.cursor_screen().y.round() as f32 + 0.5);
        let st = Stroke::new(1.0_f32, ink);
        let selecting = matches!(c.tool(), CanvasTool::Idle | CanvasTool::SelectObjects);
        let gap = if selecting { c.pick_box_px as f32 } else { 0.0 };
        let arm = 46.0;
        painter.line_segment([pos2(cx - arm, cy), pos2(cx - gap, cy)], st);
        painter.line_segment([pos2(cx + gap, cy), pos2(cx + arm, cy)], st);
        painter.line_segment([pos2(cx, cy - arm), pos2(cx, cy - gap)], st);
        painter.line_segment([pos2(cx, cy + gap), pos2(cx, cy + arm)], st);
        if selecting {
            painter.add(outline(Rect::from_center_size(pos2(cx, cy), Vec2::splat(2.0 * gap)), st));
        }

        // The active prompt follows the cursor.
        if c.is_busy() && !c.prompt().is_empty() && !matches!(c.tool(), CanvasTool::SecondPoint | CanvasTool::GripDrag | CanvasTool::DragMove) {
            tip(painter, pos2(cx + 16.0, cy + 18.0), c.prompt());
        }
        let _ = self.rep == Representation::Real;
    }
}
