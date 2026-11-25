// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! What a civil object draws: lines, filled polygons, text and point markers in model millimetres
//! (the source coordinates times 1000), and a mesh for the 3D view. A scene is built off the UI
//! thread (`civil_build`) and painted here every frame, culled to the view.
//!
//! Text sizes are in pixels on screen (so a label never grows with the zoom), or in model
//! millimetres where the text belongs to a drawing (a profile view's grid labels); a text in
//! model millimetres is drawn only when it would be between 4 and 48 pixels high.

use afrobim_draw::Mesh3;
use afrobim_kernel::{Pt, ViewTransform};
use egui::{pos2, Align2, Color32, FontId, Painter, Rect, Shape, Stroke};

#[derive(Debug, Clone)]
pub struct SLine {
    pub pts: Vec<[f64; 2]>,
    pub rgb: [u8; 3],
    pub width: f32,
    pub bbox: [f64; 4],
}

impl SLine {
    pub fn new(pts: Vec<[f64; 2]>, rgb: [u8; 3], width: f32) -> SLine {
        let mut bbox = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        for p in &pts {
            bbox = [bbox[0].min(p[0]), bbox[1].min(p[1]), bbox[2].max(p[0]), bbox[3].max(p[1])];
        }
        SLine { pts, rgb, width, bbox }
    }
}

/// A polyline in 3D, in the same frame as the meshes: millimetres from the element's origin in
/// plan, and millimetres of level.
#[derive(Debug, Clone)]
pub struct L3 {
    pub pts: Vec<[f64; 3]>,
    pub rgb: [u8; 3],
    pub width: f32,
}

#[derive(Debug, Clone)]
pub struct SFill {
    /// A convex polygon.
    pub poly: Vec<[f64; 2]>,
    pub rgba: [u8; 4],
}

#[derive(Debug, Clone)]
pub struct SText {
    pub at: [f64; 2],
    pub text: String,
    pub rgb: [u8; 3],
    /// Height in model millimetres; 0 means `px` pixels whatever the zoom.
    pub height_mm: f64,
    pub px: f32,
    /// 0 centre, 1 left, 2 right (of the point), 3 above, 4 below.
    pub anchor: u8,
    /// Which label wins when two would overlap: the higher number is drawn, the other is skipped.
    pub prio: u8,
    /// A background behind the text so that it reads over linework.
    pub halo: bool,
}

#[derive(Debug, Clone)]
pub struct PMark {
    pub at: [f64; 2],
    pub number: u32,
    pub z: Option<f64>,
    pub desc: String,
}

#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub lines: Vec<SLine>,
    pub fills: Vec<SFill>,
    pub texts: Vec<SText>,
    pub points: Vec<PMark>,
    pub meshes: Vec<Mesh3>,
    /// Linework for the 3D view (contours at their level, outlines draped on a surface).
    pub lines3: Vec<L3>,
    /// Label options for `points`.
    pub label_number: bool,
    pub label_elevation: bool,
    pub label_description: bool,
    pub decimals: usize,
    /// Read-only figures for the Properties panel.
    pub derived: Vec<(String, String)>,
    pub notes: Vec<String>,
}

fn col(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

fn text_anchor(a: u8) -> Align2 {
    match a {
        1 => Align2::LEFT_CENTER,
        2 => Align2::RIGHT_CENTER,
        3 => Align2::CENTER_BOTTOM,
        4 => Align2::CENTER_TOP,
        _ => Align2::CENTER_CENTER,
    }
}

impl Scene {
    /// Paints what is in `rect`. `ink` is the canvas ink, used for the point markers.
    pub fn paint(&self, painter: &Painter, labels: &mut Labels, view: &ViewTransform, rect: Rect, ink: Color32, selected: bool) {
        let lo = view.to_world(Pt::new(rect.left() as f64, rect.bottom() as f64));
        let hi = view.to_world(Pt::new(rect.right() as f64, rect.top() as f64));
        let (x0, y0, x1, y1) = (lo.x.min(hi.x), lo.y.min(hi.y), lo.x.max(hi.x), lo.y.max(hi.y));
        let to = |p: [f64; 2]| {
            let s = view.to_screen(Pt::new(p[0], p[1]));
            pos2(s.x as f32, s.y as f32)
        };
        for f in &self.fills {
            if f.poly.len() < 3 || !f.poly.iter().any(|p| p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1) && !poly_crosses(&f.poly, x0, y0, x1, y1) {
                continue;
            }
            let pts: Vec<_> = f.poly.iter().map(|p| to(*p)).collect();
            painter.add(Shape::convex_polygon(pts, Color32::from_rgba_unmultiplied(f.rgba[0], f.rgba[1], f.rgba[2], f.rgba[3]), Stroke::NONE));
        }
        for l in &self.lines {
            if l.bbox[2] < x0 || l.bbox[0] > x1 || l.bbox[3] < y0 || l.bbox[1] > y1 || l.pts.len() < 2 {
                continue;
            }
            // A line much smaller than a pixel is not drawn: it would be a smudge.
            if (l.bbox[2] - l.bbox[0]).max(l.bbox[3] - l.bbox[1]) * view.scale < 0.4 {
                continue;
            }
            let pts: Vec<_> = l.pts.iter().map(|p| to(*p)).collect();
            let w = if selected { l.width + 1.0 } else { l.width };
            painter.add(Shape::line(pts, Stroke::new(w, col(l.rgb))));
        }
        for t in &self.texts {
            if t.at[0] < x0 || t.at[0] > x1 || t.at[1] < y0 || t.at[1] > y1 {
                continue;
            }
            let size = if t.height_mm > 0.0 {
                let px = (t.height_mm * view.scale) as f32;
                if !(4.0..=48.0).contains(&px) {
                    continue;
                }
                px
            } else {
                t.px
            };
            labels.add(Label { pos: to(t.at), anchor: text_anchor(t.anchor), text: t.text.clone(), size, colour: col(t.rgb), prio: t.prio, halo: t.halo });
        }
        if !self.points.is_empty() {
            self.paint_points(painter, labels, view, (x0, y0, x1, y1), rect, ink, selected);
        }
    }

    fn paint_points(&self, painter: &Painter, labels: &mut Labels, view: &ViewTransform, (x0, y0, x1, y1): (f64, f64, f64, f64), rect: Rect, ink: Color32, selected: bool) {
        let visible: Vec<&PMark> = self.points.iter().filter(|p| p.at[0] >= x0 && p.at[0] <= x1 && p.at[1] >= y0 && p.at[1] <= y1).collect();
        // More than this many markers cannot be told apart: draw an even share of them.
        const MARK_CAP: usize = 20_000;
        let stride = (visible.len() / MARK_CAP).max(0) + 1;
        let colour = if selected { Color32::from_rgb(0xFF, 0xB0, 0x2E) } else { ink };
        for (i, p) in visible.iter().enumerate() {
            if i % stride != 0 {
                continue;
            }
            let s = view.to_screen(Pt::new(p.at[0], p.at[1]));
            let c = pos2(s.x as f32, s.y as f32);
            painter.line_segment([c - egui::vec2(3.0, 0.0), c + egui::vec2(3.0, 0.0)], Stroke::new(1.0_f32, colour));
            painter.line_segment([c - egui::vec2(0.0, 3.0), c + egui::vec2(0.0, 3.0)], Stroke::new(1.0_f32, colour));
        }
        // Labels only when there is room. The typical spacing of the points on screen decides how
        // much is written: nothing when they are closer than a label is high, the number first,
        // then the elevation, then the description.
        const LABEL_CAP: usize = 3000;
        if visible.is_empty() || visible.len() > LABEL_CAP || (!self.label_number && !self.label_elevation && !self.label_description) {
            return;
        }
        let (mut bx0, mut by0, mut bx1, mut by1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in &visible {
            bx0 = bx0.min(p.at[0]);
            by0 = by0.min(p.at[1]);
            bx1 = bx1.max(p.at[0]);
            by1 = by1.max(p.at[1]);
        }
        let area_px = (((bx1 - bx0) * view.scale).max(1.0) * ((by1 - by0) * view.scale).max(1.0)).min((rect.width() * rect.height()) as f64);
        let spacing = (area_px / visible.len() as f64).sqrt();
        let lines = if spacing >= 58.0 { 3 } else if spacing >= 40.0 { 2 } else if spacing >= 26.0 { 1 } else { 0 };
        if lines == 0 {
            return;
        }
        let weak = colour.gamma_multiply(0.8);
        for p in &visible {
            let s = view.to_screen(Pt::new(p.at[0], p.at[1]));
            let c = pos2(s.x as f32 + 5.0, s.y as f32);
            let mut parts: Vec<(String, bool)> = Vec::new();
            if self.label_number {
                parts.push((p.number.to_string(), true));
            }
            if self.label_elevation {
                if let Some(z) = p.z {
                    parts.push((format!("{z:.*}", self.decimals), false));
                }
            }
            if self.label_description && !p.desc.is_empty() {
                parts.push((p.desc.clone(), false));
            }
            let shown = parts.len().min(lines);
            let mut y = c.y - 5.5 * shown as f32;
            for (t, strong) in parts.into_iter().take(shown) {
                labels.add(Label { pos: pos2(c.x, y), anchor: Align2::LEFT_CENTER, text: t, size: 10.5, colour: if strong { colour } else { weak }, prio: if strong { 2 } else { 1 }, halo: false });
                y += 11.0;
            }
        }
    }
}

/// Whether the bounding box of a polygon overlaps a rectangle (a coarse test: the box is enough to
/// decide that something might show).
fn poly_crosses(poly: &[[f64; 2]], x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    let (mut a, mut b, mut c, mut d) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in poly {
        a = a.min(p[0]);
        b = b.min(p[1]);
        c = c.max(p[0]);
        d = d.max(p[1]);
    }
    c >= x0 && a <= x1 && d >= y0 && b <= y1
}

/// One label waiting to be placed.
pub struct Label {
    pub pos: egui::Pos2,
    pub anchor: Align2,
    pub text: String,
    pub size: f32,
    pub colour: Color32,
    pub prio: u8,
    pub halo: bool,
}

/// The labels of a frame, collected from every object and placed together: by priority, and a
/// label that would overlap one already placed is skipped, so that text never piles up.
#[derive(Default)]
pub struct Labels {
    items: Vec<Label>,
}

impl Labels {
    pub fn add(&mut self, l: Label) {
        self.items.push(l);
    }

    /// Places and paints the labels. `dark` picks the colour of the halo behind the ones that have one.
    pub fn flush(&mut self, painter: &Painter, dark: bool) {
        use std::collections::HashMap;
        let mut items = std::mem::take(&mut self.items);
        // Stable: among equal priorities the first added (nearer the start of the drawing order) wins.
        items.sort_by_key(|l| std::cmp::Reverse(l.prio));
        const CELL: f32 = 96.0;
        let mut grid: HashMap<(i32, i32), Vec<Rect>> = HashMap::new();
        let halo_fill = if dark { Color32::from_rgba_unmultiplied(18, 22, 28, 215) } else { Color32::from_rgba_unmultiplied(255, 255, 255, 220) };
        for l in items {
            let galley = painter.layout_no_wrap(l.text, FontId::proportional(l.size), l.colour);
            let rect = l.anchor.anchor_size(l.pos, galley.size());
            let probe = rect.expand(1.5);
            let (cx0, cy0, cx1, cy1) = ((probe.left() / CELL).floor() as i32, (probe.top() / CELL).floor() as i32, (probe.right() / CELL).floor() as i32, (probe.bottom() / CELL).floor() as i32);
            let mut clash = false;
            'scan: for cx in cx0..=cx1 {
                for cy in cy0..=cy1 {
                    if grid.get(&(cx, cy)).is_some_and(|v| v.iter().any(|r| r.intersects(probe))) {
                        clash = true;
                        break 'scan;
                    }
                }
            }
            if clash {
                continue;
            }
            for cx in cx0..=cx1 {
                for cy in cy0..=cy1 {
                    grid.entry((cx, cy)).or_default().push(rect);
                }
            }
            if l.halo {
                painter.rect_filled(rect.expand2(egui::vec2(3.0, 1.0)), 3.0, halo_fill);
            }
            painter.galley(rect.min, galley, l.colour);
        }
    }
}
