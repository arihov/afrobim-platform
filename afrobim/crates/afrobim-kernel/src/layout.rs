// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Layouts (paper space): the data a layout tab keeps in the document, and the plane geometry of
//! its viewports.
//!
//! A [`Layout`] is one sheet of paper. Its units are millimetres on the paper, Y up, with the
//! lower left paper corner on the origin. A [`Viewport`] is a window onto model space: a boundary
//! on the paper, the model point shown at the middle of the boundary and a drawing scale 1:n. The
//! model is in millimetres, so a paper length is a model length divided by `n` (1:500 shows 50 m
//! across 100 mm of paper). The scale type of the sheets crate (`afrobim_sheets::Scale`) says the
//! same in paper millimetres per model metre; the sheets crate checks that the two agree.
//!
//! This module holds no GUI and no output code: it is data, serde and arithmetic. Paper sizes are
//! worked out by the sheets crate (ISO 216) and stored here as millimetres.

use serde::{Deserialize, Serialize};

use crate::Pt;

/// The standard drawing scales offered for a viewport, 1:n.
pub const STANDARD_SCALES: [f64; 16] = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 250.0, 500.0, 1000.0, 1250.0, 2000.0, 2500.0, 5000.0, 10000.0];

/// The name of the model tab, which no layout may take.
pub const MODEL_TAB: &str = "Model";

/// Why a layout operation was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    #[error("A layout needs a name.")]
    EmptyName,
    #[error("\"{0}\" is the model tab. Choose another name.")]
    ReservedName(String),
    #[error("There is already a layout called \"{0}\".")]
    DuplicateName(String),
    #[error("There is no such layout.")]
    NotFound,
}

fn yes() -> bool {
    true
}

/// One corner or edge or vertex handle of a viewport boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VpGrip {
    /// Moves the viewport and its contents together.
    Move,
    /// Corner 0 lower left, 1 lower right, 2 upper right, 3 upper left.
    Corner(u8),
    /// Edge 0 bottom, 1 right, 2 top, 3 left.
    Edge(u8),
    /// Vertex of a clipped (polygon) boundary.
    Vertex(usize),
}

/// A window onto model space, placed on the paper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub id: u64,
    /// The boundary box on the paper: x0, y0, x1, y1 in mm. For a clipped viewport this is the
    /// bounding box of the clip polygon.
    pub rect: [f64; 4],
    /// A polygon boundary on the paper (mm), when the viewport is not a plain rectangle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<Vec<Pt>>,
    /// The model point (mm) shown at the middle of `rect`.
    pub centre: Pt,
    /// The drawing scale 1:`scale_den`.
    pub scale_den: f64,
    /// Display locked: pan and zoom inside it move the paper instead, and the scale cannot change.
    #[serde(default)]
    pub locked: bool,
    /// Display on. An off viewport keeps its boundary but shows no model.
    #[serde(default = "yes")]
    pub on: bool,
    /// Layers frozen in this viewport only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frozen: Vec<String>,
    /// The view title, printed under the viewport with the scale beneath it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// Print the title and scale label under the viewport.
    #[serde(default)]
    pub show_label: bool,
}

impl Viewport {
    /// A rectangular viewport with the given paper corners, showing `centre` at scale 1:`scale_den`.
    pub fn new(id: u64, a: Pt, b: Pt, centre: Pt, scale_den: f64) -> Viewport {
        Viewport {
            id,
            rect: [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)],
            clip: None,
            centre,
            scale_den,
            locked: false,
            on: true,
            frozen: Vec::new(),
            title: String::new(),
            show_label: false,
        }
    }

    pub fn width(&self) -> f64 {
        self.rect[2] - self.rect[0]
    }

    pub fn height(&self) -> f64 {
        self.rect[3] - self.rect[1]
    }

    /// The middle of the boundary box, on the paper.
    pub fn paper_centre(&self) -> Pt {
        Pt::new((self.rect[0] + self.rect[2]) / 2.0, (self.rect[1] + self.rect[3]) / 2.0)
    }

    /// Paper millimetres per model millimetre.
    pub fn paper_per_model(&self) -> f64 {
        1.0 / self.scale_den
    }

    /// Paper millimetres per model metre (what `afrobim_sheets::Scale::paper_mm_per_m` returns).
    pub fn paper_mm_per_model_m(&self) -> f64 {
        1000.0 / self.scale_den
    }

    pub fn is_valid_scale(den: f64) -> bool {
        den.is_finite() && den > 0.0
    }

    /// The paper point where the model point `m` (mm) is shown.
    pub fn to_paper(&self, m: Pt) -> Pt {
        let (c, k) = (self.paper_centre(), self.paper_per_model());
        Pt::new(c.x + (m.x - self.centre.x) * k, c.y + (m.y - self.centre.y) * k)
    }

    /// The model point (mm) shown at the paper point `p`.
    pub fn to_model(&self, p: Pt) -> Pt {
        let (c, k) = (self.paper_centre(), self.paper_per_model());
        Pt::new(self.centre.x + (p.x - c.x) / k, self.centre.y + (p.y - c.y) / k)
    }

    /// The boundary as a polygon on the paper: the clip, or the four corners of the box.
    pub fn outline(&self) -> Vec<Pt> {
        match &self.clip {
            Some(c) if c.len() >= 3 => c.clone(),
            _ => vec![Pt::new(self.rect[0], self.rect[1]), Pt::new(self.rect[2], self.rect[1]), Pt::new(self.rect[2], self.rect[3]), Pt::new(self.rect[0], self.rect[3])],
        }
    }

    /// True when the paper point lies inside the boundary (the clip when there is one).
    pub fn contains_paper(&self, p: Pt) -> bool {
        match &self.clip {
            Some(c) if c.len() >= 3 => point_in_polygon(c, p),
            _ => p.x >= self.rect[0] && p.x <= self.rect[2] && p.y >= self.rect[1] && p.y <= self.rect[3],
        }
    }

    /// The width of model space (mm) the boundary box shows.
    pub fn model_width(&self) -> f64 {
        self.width() * self.scale_den
    }

    /// Sets the scale about the centre of the view. Returns false for an invalid scale or a
    /// locked viewport.
    pub fn set_scale(&mut self, den: f64) -> bool {
        if self.locked || !Self::is_valid_scale(den) {
            return false;
        }
        self.scale_den = den;
        true
    }

    /// Moves the boundary to `new_rect` without moving the contents on the paper: the model point
    /// under any paper point stays under it (this is what stretching a boundary does).
    fn rebound(&mut self, new_rect: [f64; 4]) {
        let (old_c, k) = (self.paper_centre(), self.paper_per_model());
        self.rect = new_rect;
        let new_c = self.paper_centre();
        self.centre = Pt::new(self.centre.x + (new_c.x - old_c.x) / k, self.centre.y + (new_c.y - old_c.y) / k);
    }

    /// Moves the viewport on the paper by `(dx, dy)` mm. The contents move with it.
    pub fn move_by(&mut self, dx: f64, dy: f64) {
        self.rect = [self.rect[0] + dx, self.rect[1] + dy, self.rect[2] + dx, self.rect[3] + dy];
        if let Some(c) = &mut self.clip {
            for p in c {
                p.x += dx;
                p.y += dy;
            }
        }
    }

    /// The handles of the boundary with their paper positions.
    pub fn grips(&self) -> Vec<(VpGrip, Pt)> {
        let mut out = vec![(VpGrip::Move, self.paper_centre())];
        match &self.clip {
            Some(c) if c.len() >= 3 => out.extend(c.iter().enumerate().map(|(i, p)| (VpGrip::Vertex(i), *p))),
            _ => {
                let r = self.rect;
                let (mx, my) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
                out.extend([
                    (VpGrip::Corner(0), Pt::new(r[0], r[1])),
                    (VpGrip::Corner(1), Pt::new(r[2], r[1])),
                    (VpGrip::Corner(2), Pt::new(r[2], r[3])),
                    (VpGrip::Corner(3), Pt::new(r[0], r[3])),
                    (VpGrip::Edge(0), Pt::new(mx, r[1])),
                    (VpGrip::Edge(1), Pt::new(r[2], my)),
                    (VpGrip::Edge(2), Pt::new(mx, r[3])),
                    (VpGrip::Edge(3), Pt::new(r[0], my)),
                ]);
            }
        }
        out
    }

    /// Drags a handle to the paper point `to`. `Move` takes the handle's start `from` so the
    /// whole viewport moves by the difference. A boundary cannot be made smaller than 1 mm.
    pub fn drag_grip(&mut self, grip: VpGrip, from: Pt, to: Pt) {
        const MIN: f64 = 1.0;
        let mut r = self.rect;
        match grip {
            VpGrip::Move => {
                self.move_by(to.x - from.x, to.y - from.y);
                return;
            }
            VpGrip::Corner(i) => {
                if matches!(i, 0 | 3) {
                    r[0] = to.x.min(r[2] - MIN);
                } else {
                    r[2] = to.x.max(r[0] + MIN);
                }
                if matches!(i, 0 | 1) {
                    r[1] = to.y.min(r[3] - MIN);
                } else {
                    r[3] = to.y.max(r[1] + MIN);
                }
            }
            VpGrip::Edge(i) => match i {
                0 => r[1] = to.y.min(r[3] - MIN),
                1 => r[2] = to.x.max(r[0] + MIN),
                2 => r[3] = to.y.max(r[1] + MIN),
                _ => r[0] = to.x.min(r[2] - MIN),
            },
            VpGrip::Vertex(i) => {
                if let Some(c) = &mut self.clip {
                    if let Some(v) = c.get_mut(i) {
                        *v = to;
                    }
                    let b = bbox_of(c);
                    self.rebound(b);
                }
                return;
            }
        }
        self.rebound(r);
    }

    /// Reshapes the boundary to the polygon `poly` (VPCLIP). The contents keep their place on the
    /// paper. Fewer than three points are refused.
    pub fn set_clip(&mut self, poly: Vec<Pt>) -> bool {
        if poly.len() < 3 || poly.iter().any(|p| !(p.x.is_finite() && p.y.is_finite())) {
            return false;
        }
        let b = bbox_of(&poly);
        if b[2] - b[0] < 1e-6 || b[3] - b[1] < 1e-6 {
            return false;
        }
        self.rebound(b);
        self.clip = Some(poly);
        true
    }

    /// Removes the clip; the boundary becomes the box around it.
    pub fn remove_clip(&mut self) -> bool {
        self.clip.take().is_some()
    }

    /// Freezes the layer in this viewport (true when it was not frozen already).
    pub fn freeze(&mut self, layer: &str) -> bool {
        if self.frozen.iter().any(|l| l == layer) {
            return false;
        }
        self.frozen.push(layer.to_string());
        true
    }

    /// Thaws the layer in this viewport (true when it was frozen).
    pub fn thaw(&mut self, layer: &str) -> bool {
        let n = self.frozen.len();
        self.frozen.retain(|l| l != layer);
        self.frozen.len() != n
    }

    pub fn is_frozen(&self, layer: &str) -> bool {
        self.frozen.iter().any(|l| l == layer)
    }

    /// "1:500", or "1:2.5" for a custom scale.
    pub fn scale_label(&self) -> String {
        scale_label(self.scale_den)
    }
}

/// "1:500". A whole `n` prints without decimals.
pub fn scale_label(den: f64) -> String {
    if (den - den.round()).abs() < 1e-9 {
        format!("1:{}", den.round() as i64)
    } else {
        let s = format!("{den:.2}");
        format!("1:{}", s.trim_end_matches('0').trim_end_matches('.'))
    }
}

/// The scale "1:500" or "500" typed by a person, as the denominator.
pub fn parse_scale(text: &str) -> Option<f64> {
    let t = text.trim();
    let t = t.strip_prefix("1:").or_else(|| t.strip_prefix("1 :")).unwrap_or(t).trim();
    let v: f64 = t.parse().ok()?;
    Viewport::is_valid_scale(v).then_some(v)
}

/// The bounding box of points: x0, y0, x1, y1.
pub fn bbox_of(points: &[Pt]) -> [f64; 4] {
    let mut r = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for p in points {
        r = [r[0].min(p.x), r[1].min(p.y), r[2].max(p.x), r[3].max(p.y)];
    }
    r
}

/// Even-odd point in polygon. A point on an edge counts as inside.
pub fn point_in_polygon(poly: &[Pt], p: Pt) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        // On the edge?
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        let len2 = (b.x - a.x).powi(2) + (b.y - a.y).powi(2);
        if len2 > 0.0 && cross * cross <= 1e-18 * len2 {
            let dot = (p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y);
            if (0.0..=len2).contains(&dot) {
                return true;
            }
        }
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// A circle as a polygon of `n` points (at least 8), for a viewport made from a circle.
pub fn circle_polygon(centre: Pt, radius: f64, n: usize) -> Vec<Pt> {
    let n = n.max(8);
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            Pt::new(centre.x + radius * a.cos(), centre.y + radius * a.sin())
        })
        .collect()
}

/// A thing drawn on the paper itself (not in model space): the source for "viewport from object".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PaperShape {
    /// A polyline; closed when `closed`.
    Poly { id: u64, points: Vec<Pt>, closed: bool },
    Circle { id: u64, centre: Pt, radius: f64 },
}

impl PaperShape {
    pub fn id(&self) -> u64 {
        match self {
            PaperShape::Poly { id, .. } | PaperShape::Circle { id, .. } => *id,
        }
    }

    /// The closed outline of the shape, or `None` for an open polyline.
    pub fn closed_outline(&self) -> Option<Vec<Pt>> {
        match self {
            PaperShape::Poly { points, closed: true, .. } if points.len() >= 3 => Some(points.clone()),
            PaperShape::Circle { centre, radius, .. } if *radius > 0.0 => Some(circle_polygon(*centre, *radius, 72)),
            _ => None,
        }
    }

    /// The shape as a polyline for drawing.
    pub fn points(&self) -> Vec<Pt> {
        match self {
            PaperShape::Poly { points, closed, .. } => {
                let mut p = points.clone();
                if *closed && p.len() >= 2 {
                    p.push(p[0]);
                }
                p
            }
            PaperShape::Circle { centre, radius, .. } => {
                let mut p = circle_polygon(*centre, *radius, 72);
                p.push(p[0]);
                p
            }
        }
    }

    /// Distance from `p` to the shape's line work, mm.
    pub fn distance_to(&self, p: Pt) -> f64 {
        let pts = self.points();
        pts.windows(2)
            .map(|w| {
                let (a, b) = (w[0], w[1]);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let l2 = dx * dx + dy * dy;
                let t = if l2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
                ((p.x - a.x - t * dx).powi(2) + (p.y - a.y - t * dy).powi(2)).sqrt()
            })
            .fold(f64::INFINITY, f64::min)
    }
}

/// One layout tab: a sheet of paper and what is on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub id: u64,
    pub name: String,
    /// "A3" and so on, for display; the size in mm below is what counts. "Custom" for others.
    pub size: String,
    pub landscape: bool,
    pub width_mm: f64,
    pub height_mm: f64,
    /// Paper edge to frame (and printable area): left, right, top, bottom, mm.
    pub margins: [f64; 4],
    /// Width of the title block column on the right of the frame, mm; 0 for no title block.
    #[serde(default)]
    pub title_block_mm: f64,
    #[serde(default)]
    pub viewports: Vec<Viewport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shapes: Vec<PaperShape>,
    /// Layers frozen in every viewport made from now on (VPLAYER "Vpvisdflt").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub new_viewport_frozen: Vec<String>,
    #[serde(default)]
    next_item: u64,
}

impl Layout {
    /// A layout of the given paper size in millimetres with no viewports.
    pub fn new(name: &str, size: &str, landscape: bool, width_mm: f64, height_mm: f64) -> Layout {
        Layout {
            id: 0,
            name: name.to_string(),
            size: size.to_string(),
            landscape,
            width_mm,
            height_mm,
            margins: [10.0; 4],
            title_block_mm: 0.0,
            viewports: Vec::new(),
            shapes: Vec::new(),
            new_viewport_frozen: Vec::new(),
            next_item: 1,
        }
    }

    /// The paper: x0, y0, x1, y1 in mm.
    pub fn paper_rect(&self) -> [f64; 4] {
        [0.0, 0.0, self.width_mm, self.height_mm]
    }

    /// The printable area (the frame): the paper less the margins.
    pub fn printable_rect(&self) -> [f64; 4] {
        let m = self.margins;
        [m[0], m[3], (self.width_mm - m[1]).max(m[0]), (self.height_mm - m[2]).max(m[3])]
    }

    /// What a Fit viewport fills: the printable area less the title block column.
    pub fn view_area(&self) -> [f64; 4] {
        let r = self.printable_rect();
        [r[0], r[1], (r[2] - self.title_block_mm).max(r[0]), r[3]]
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_item.max(1);
        self.next_item = id + 1;
        id
    }

    pub fn viewport(&self, id: u64) -> Option<&Viewport> {
        self.viewports.iter().find(|v| v.id == id)
    }

    pub fn viewport_mut(&mut self, id: u64) -> Option<&mut Viewport> {
        self.viewports.iter_mut().find(|v| v.id == id)
    }

    /// The topmost viewport whose boundary holds the paper point (the last added is on top).
    pub fn viewport_at(&self, p: Pt) -> Option<u64> {
        self.viewports.iter().rev().find(|v| v.contains_paper(p)).map(|v| v.id)
    }

    /// Adds a rectangular viewport and returns its id. It starts with the layers frozen that
    /// `new_viewport_frozen` lists.
    pub fn add_viewport(&mut self, a: Pt, b: Pt, centre: Pt, scale_den: f64) -> Option<u64> {
        if (a.x - b.x).abs() < 1.0 || (a.y - b.y).abs() < 1.0 || !Viewport::is_valid_scale(scale_den) {
            return None;
        }
        let id = self.take_id();
        let mut v = Viewport::new(id, a, b, centre, scale_den);
        v.frozen = self.new_viewport_frozen.clone();
        self.viewports.push(v);
        Some(id)
    }

    /// Adds a viewport with a polygon boundary.
    pub fn add_polygon_viewport(&mut self, poly: Vec<Pt>, centre: Pt, scale_den: f64) -> Option<u64> {
        if poly.len() < 3 || !Viewport::is_valid_scale(scale_den) {
            return None;
        }
        let b = bbox_of(&poly);
        let id = self.add_viewport(Pt::new(b[0], b[1]), Pt::new(b[2], b[3]), centre, scale_den)?;
        if !self.viewport_mut(id)?.set_clip(poly) {
            self.viewports.retain(|v| v.id != id);
            return None;
        }
        Some(id)
    }

    /// Adds a viewport from a closed polyline or a circle drawn on this layout.
    pub fn add_viewport_from_shape(&mut self, shape_id: u64, centre: Pt, scale_den: f64) -> Option<u64> {
        let outline = self.shapes.iter().find(|s| s.id() == shape_id)?.closed_outline()?;
        self.add_polygon_viewport(outline, centre, scale_den)
    }

    /// A viewport that fills the view area, showing `centre` at `scale_den`.
    pub fn add_fit_viewport(&mut self, centre: Pt, scale_den: f64) -> Option<u64> {
        let r = self.view_area();
        self.add_viewport(Pt::new(r[0], r[1]), Pt::new(r[2], r[3]), centre, scale_den)
    }

    /// The boundary boxes of the 1, 2, 3 or 4 viewport arrangement of the view area, with `gap`
    /// mm between them: 1 fills it; 2 side by side; 3 one tall on the left and two stacked on the
    /// right; 4 in a grid of 2 by 2.
    pub fn preset_rects(&self, count: u8, gap: f64) -> Vec<[f64; 4]> {
        let r = self.view_area();
        let (w, h) = (r[2] - r[0], r[3] - r[1]);
        let (hw, hh) = ((w - gap) / 2.0, (h - gap) / 2.0);
        let (xm, ym) = (r[0] + hw + gap, r[1] + hh + gap);
        match count {
            1 => vec![r],
            2 => vec![[r[0], r[1], r[0] + hw, r[3]], [xm, r[1], r[2], r[3]]],
            3 => vec![[r[0], r[1], r[0] + hw, r[3]], [xm, r[1], r[2], r[1] + hh], [xm, ym, r[2], r[3]]],
            _ => vec![[r[0], r[1], r[0] + hw, r[1] + hh], [xm, r[1], r[2], r[1] + hh], [r[0], ym, r[0] + hw, r[3]], [xm, ym, r[2], r[3]]],
        }
    }

    /// Adds the 1 to 4 viewport arrangement, every one showing `centre` at `scale_den`. Returns
    /// the ids.
    pub fn add_preset(&mut self, count: u8, centre: Pt, scale_den: f64) -> Vec<u64> {
        let count = count.clamp(1, 4);
        self.preset_rects(count, 5.0).into_iter().filter_map(|r| self.add_viewport(Pt::new(r[0], r[1]), Pt::new(r[2], r[3]), centre, scale_den)).collect()
    }

    pub fn delete_viewport(&mut self, id: u64) -> bool {
        let n = self.viewports.len();
        self.viewports.retain(|v| v.id != id);
        self.viewports.len() != n
    }

    /// A copy of the viewport, moved 10 mm up and right. Returns the new id.
    pub fn duplicate_viewport(&mut self, id: u64) -> Option<u64> {
        let mut v = self.viewport(id)?.clone();
        v.id = self.take_id();
        v.move_by(10.0, 10.0);
        // The copy shows the same model at the same place on the paper relative to its boundary.
        let nid = v.id;
        self.viewports.push(v);
        Some(nid)
    }

    pub fn add_shape_poly(&mut self, points: Vec<Pt>, closed: bool) -> Option<u64> {
        if points.len() < 2 {
            return None;
        }
        let id = self.take_id();
        self.shapes.push(PaperShape::Poly { id, points, closed });
        Some(id)
    }

    pub fn add_shape_circle(&mut self, centre: Pt, radius: f64) -> Option<u64> {
        if !(radius.is_finite() && radius > 0.0) {
            return None;
        }
        let id = self.take_id();
        self.shapes.push(PaperShape::Circle { id, centre, radius });
        Some(id)
    }

    /// The closest shape within `tol` mm of the paper point.
    pub fn shape_at(&self, p: Pt, tol: f64) -> Option<u64> {
        self.shapes.iter().map(|s| (s.id(), s.distance_to(p))).filter(|(_, d)| *d <= tol).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(id, _)| id)
    }
}

/// The layouts of a document, in tab order.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LayoutList {
    #[serde(default)]
    items: Vec<Layout>,
    #[serde(default)]
    next_id: u64,
}

impl LayoutList {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Layout> {
        self.items.iter()
    }

    pub fn as_slice(&self) -> &[Layout] {
        &self.items
    }

    pub fn names(&self) -> Vec<String> {
        self.items.iter().map(|l| l.name.clone()).collect()
    }

    pub fn get(&self, id: u64) -> Option<&Layout> {
        self.items.iter().find(|l| l.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Layout> {
        self.items.iter_mut().find(|l| l.id == id)
    }

    pub fn by_name(&self, name: &str) -> Option<&Layout> {
        self.items.iter().find(|l| l.name.eq_ignore_ascii_case(name.trim()))
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.items.iter().position(|l| l.id == id)
    }

    pub fn at(&self, index: usize) -> Option<&Layout> {
        self.items.get(index)
    }

    /// `base`, or `base (2)`, `base (3)`... when taken. Never "Model".
    pub fn unique_name(&self, base: &str) -> String {
        let base = if base.trim().is_empty() { "Layout" } else { base.trim() };
        let taken = |n: &str| n.eq_ignore_ascii_case(MODEL_TAB) || self.items.iter().any(|l| l.name.eq_ignore_ascii_case(n));
        if !taken(base) {
            return base.to_string();
        }
        (2..).map(|i| format!("{base} ({i})")).find(|n| !taken(n)).expect("an unused name exists")
    }

    fn check_name(&self, name: &str, except: Option<u64>) -> Result<String, LayoutError> {
        let n = name.trim();
        if n.is_empty() {
            return Err(LayoutError::EmptyName);
        }
        if n.eq_ignore_ascii_case(MODEL_TAB) {
            return Err(LayoutError::ReservedName(n.to_string()));
        }
        if self.items.iter().any(|l| Some(l.id) != except && l.name.eq_ignore_ascii_case(n)) {
            return Err(LayoutError::DuplicateName(n.to_string()));
        }
        Ok(n.to_string())
    }

    /// Adds a layout at the end under its own name; the name must be free.
    pub fn add(&mut self, mut layout: Layout) -> Result<u64, LayoutError> {
        layout.name = self.check_name(&layout.name, None)?;
        self.next_id = self.next_id.max(self.items.iter().map(|l| l.id + 1).max().unwrap_or(1)).max(1);
        layout.id = self.next_id;
        self.next_id += 1;
        let id = layout.id;
        self.items.push(layout);
        Ok(id)
    }

    pub fn rename(&mut self, id: u64, name: &str) -> Result<(), LayoutError> {
        let n = self.check_name(name, Some(id))?;
        self.get_mut(id).ok_or(LayoutError::NotFound)?.name = n;
        Ok(())
    }

    pub fn delete(&mut self, id: u64) -> Result<(), LayoutError> {
        let i = self.index_of(id).ok_or(LayoutError::NotFound)?;
        self.items.remove(i);
        Ok(())
    }

    /// A copy placed right after the original, named "<name> (2)" and so on.
    pub fn duplicate(&mut self, id: u64) -> Result<u64, LayoutError> {
        let i = self.index_of(id).ok_or(LayoutError::NotFound)?;
        let mut copy = self.items[i].clone();
        copy.name = self.unique_name(&copy.name);
        self.next_id = self.next_id.max(self.items.iter().map(|l| l.id + 1).max().unwrap_or(1)).max(1);
        copy.id = self.next_id;
        self.next_id += 1;
        let nid = copy.id;
        self.items.insert(i + 1, copy);
        Ok(nid)
    }

    /// Moves a layout one place left (`delta` -1) or right (+1), or further. Returns whether the
    /// order changed.
    pub fn move_by(&mut self, id: u64, delta: i32) -> bool {
        let Some(i) = self.index_of(id) else { return false };
        let j = (i as i64 + delta as i64).clamp(0, self.items.len() as i64 - 1) as usize;
        if i == j {
            return false;
        }
        let l = self.items.remove(i);
        self.items.insert(j, l);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a3() -> Layout {
        Layout::new("A3 sheet", "A3", true, 420.0, 297.0)
    }

    fn vp_1000() -> Viewport {
        Viewport::new(1, Pt::new(10.0, 10.0), Pt::new(210.0, 160.0), Pt::new(480_000.0, 120_000.0), 1000.0)
    }

    #[test]
    fn paper_and_model_round_trip_at_every_standard_scale() {
        for den in STANDARD_SCALES {
            let v = Viewport::new(1, Pt::new(10.0, 20.0), Pt::new(250.0, 180.0), Pt::new(123_456.789, -98_765.4), den);
            for p in [Pt::new(10.0, 20.0), Pt::new(130.0, 100.0), Pt::new(250.0, 180.0), Pt::new(33.3, 171.7)] {
                let back = v.to_paper(v.to_model(p));
                assert!((back.x - p.x).abs() < 1e-9 && (back.y - p.y).abs() < 1e-9, "1:{den} {p:?} {back:?}");
            }
            let m = Pt::new(124_000.0, -98_000.0);
            let back = v.to_model(v.to_paper(m));
            assert!((back.x - m.x).abs() < 1e-6 && (back.y - m.y).abs() < 1e-6, "1:{den}");
            // The centre of the model view is shown at the middle of the boundary.
            let c = v.to_paper(v.centre);
            assert_eq!(c, v.paper_centre());
        }
    }

    #[test]
    fn one_to_500_shows_exactly_50_m_across_100_mm() {
        let v = Viewport::new(1, Pt::new(0.0, 0.0), Pt::new(100.0, 60.0), Pt::new(0.0, 0.0), 500.0);
        let (a, b) = (v.to_model(Pt::new(0.0, 30.0)), v.to_model(Pt::new(100.0, 30.0)));
        assert!((b.x - a.x - 50_000.0).abs() < 1e-9, "{} mm", b.x - a.x);
        assert!((v.model_width() - 50_000.0).abs() < 1e-9);
        assert!((v.paper_mm_per_model_m() - 2.0).abs() < 1e-12, "1:500 is 2 mm per metre");
    }

    #[test]
    fn scale_changes_are_refused_when_locked_or_invalid() {
        let mut v = vp_1000();
        assert!(v.set_scale(500.0) && v.scale_den == 500.0);
        v.locked = true;
        assert!(!v.set_scale(100.0) && v.scale_den == 500.0);
        v.locked = false;
        assert!(!v.set_scale(0.0) && !v.set_scale(f64::NAN) && !v.set_scale(-5.0));
        assert_eq!(scale_label(500.0), "1:500");
        assert_eq!(scale_label(2.5), "1:2.5");
        assert_eq!(parse_scale("1:200"), Some(200.0));
        assert_eq!(parse_scale(" 250 "), Some(250.0));
        assert_eq!(parse_scale("abc"), None);
        assert_eq!(parse_scale("0"), None);
    }

    #[test]
    fn clip_polygon_containment() {
        let l_shape = vec![Pt::new(0.0, 0.0), Pt::new(100.0, 0.0), Pt::new(100.0, 40.0), Pt::new(40.0, 40.0), Pt::new(40.0, 100.0), Pt::new(0.0, 100.0)];
        assert!(point_in_polygon(&l_shape, Pt::new(20.0, 20.0)));
        assert!(point_in_polygon(&l_shape, Pt::new(20.0, 90.0)));
        assert!(point_in_polygon(&l_shape, Pt::new(90.0, 20.0)));
        assert!(!point_in_polygon(&l_shape, Pt::new(70.0, 70.0)), "the notch of the L is outside");
        assert!(!point_in_polygon(&l_shape, Pt::new(-1.0, 20.0)));
        assert!(point_in_polygon(&l_shape, Pt::new(100.0, 20.0)), "an edge counts as inside");
        assert!(!point_in_polygon(&[Pt::new(0.0, 0.0), Pt::new(1.0, 1.0)], Pt::new(0.5, 0.5)));

        let mut v = vp_1000();
        assert!(v.contains_paper(Pt::new(200.0, 150.0)));
        assert!(v.set_clip(vec![Pt::new(20.0, 20.0), Pt::new(100.0, 20.0), Pt::new(60.0, 100.0)]));
        assert!(v.contains_paper(Pt::new(60.0, 40.0)));
        assert!(!v.contains_paper(Pt::new(200.0, 150.0)), "outside the clip");
        assert!(!v.contains_paper(Pt::new(25.0, 90.0)), "inside the box but outside the triangle");
        assert_eq!(v.rect, [20.0, 20.0, 100.0, 100.0]);
        assert!(!v.set_clip(vec![Pt::new(0.0, 0.0), Pt::new(1.0, 1.0)]));
        assert!(v.remove_clip() && !v.remove_clip());
        assert!(v.contains_paper(Pt::new(25.0, 90.0)));
    }

    #[test]
    fn a_clip_or_a_resize_does_not_move_the_contents_on_the_paper() {
        let mut v = vp_1000();
        let m = Pt::new(500_000.0, 100_000.0);
        let before = v.to_paper(m);
        assert!(v.set_clip(vec![Pt::new(30.0, 30.0), Pt::new(90.0, 30.0), Pt::new(60.0, 120.0)]));
        let after = v.to_paper(m);
        assert!((before.x - after.x).abs() < 1e-9 && (before.y - after.y).abs() < 1e-9);
        let mut w = vp_1000();
        w.drag_grip(VpGrip::Corner(2), Pt::new(210.0, 160.0), Pt::new(300.0, 200.0));
        assert_eq!(w.rect, [10.0, 10.0, 300.0, 200.0]);
        let (b, a) = (vp_1000().to_paper(m), w.to_paper(m));
        assert!((b.x - a.x).abs() < 1e-9 && (b.y - a.y).abs() < 1e-9, "stretching keeps the contents in place");
        // Moving carries the contents along.
        let mut mv = vp_1000();
        mv.drag_grip(VpGrip::Move, Pt::new(110.0, 85.0), Pt::new(120.0, 95.0));
        let p = mv.to_paper(m);
        assert!((p.x - (before.x + 10.0)).abs() < 1e-9 && (p.y - (before.y + 10.0)).abs() < 1e-9);
        assert_eq!(mv.scale_den, 1000.0);
        // A boundary keeps at least 1 mm.
        let mut s = vp_1000();
        s.drag_grip(VpGrip::Edge(1), Pt::new(210.0, 85.0), Pt::new(-50.0, 85.0));
        assert!(s.width() >= 1.0);
    }

    #[test]
    fn layer_freeze_and_new_viewport_defaults() {
        let mut l = a3();
        l.new_viewport_frozen = vec!["Survey".into()];
        let id = l.add_viewport(Pt::new(10.0, 10.0), Pt::new(100.0, 100.0), Pt::new(0.0, 0.0), 500.0).unwrap();
        let v = l.viewport_mut(id).unwrap();
        assert!(v.is_frozen("Survey"), "frozen in new viewports");
        assert!(v.freeze("Drainage") && !v.freeze("Drainage"));
        assert!(v.thaw("Survey") && !v.thaw("Survey"));
        assert_eq!(v.frozen, vec!["Drainage".to_string()]);
        assert!(l.add_viewport(Pt::new(0.0, 0.0), Pt::new(0.5, 50.0), Pt::new(0.0, 0.0), 500.0).is_none(), "a sliver is no viewport");
        assert!(l.add_viewport(Pt::new(0.0, 0.0), Pt::new(50.0, 50.0), Pt::new(0.0, 0.0), 0.0).is_none());
    }

    #[test]
    fn presets_tile_the_view_area_without_overlap() {
        let mut l = a3();
        l.title_block_mm = 70.0;
        let area = l.view_area();
        assert_eq!(area, [10.0, 10.0, 340.0, 287.0]);
        for n in 1..=4u8 {
            let rects = l.preset_rects(n, 5.0);
            assert_eq!(rects.len(), n as usize);
            for (i, a) in rects.iter().enumerate() {
                assert!(a[0] >= area[0] - 1e-9 && a[1] >= area[1] - 1e-9 && a[2] <= area[2] + 1e-9 && a[3] <= area[3] + 1e-9, "{n}: {a:?} inside");
                for b in &rects[i + 1..] {
                    let overlap = a[0] < b[2] - 1e-9 && b[0] < a[2] - 1e-9 && a[1] < b[3] - 1e-9 && b[1] < a[3] - 1e-9;
                    assert!(!overlap, "{n}: {a:?} and {b:?} overlap");
                }
            }
        }
        let ids = l.add_preset(4, Pt::new(0.0, 0.0), 1000.0);
        assert_eq!(ids.len(), 4);
        assert_eq!(l.viewports.len(), 4);
        let fit = l.add_fit_viewport(Pt::new(0.0, 0.0), 500.0).unwrap();
        assert_eq!(l.viewport(fit).unwrap().rect, area);
        // The topmost viewport wins a pick.
        assert_eq!(l.viewport_at(Pt::new(100.0, 100.0)), Some(fit));
        assert_eq!(l.viewport_at(Pt::new(5.0, 5.0)), None);
    }

    #[test]
    fn viewports_from_a_closed_polyline_or_a_circle() {
        let mut l = a3();
        let open = l.add_shape_poly(vec![Pt::new(10.0, 10.0), Pt::new(50.0, 10.0), Pt::new(50.0, 50.0)], false).unwrap();
        let tri = l.add_shape_poly(vec![Pt::new(10.0, 10.0), Pt::new(80.0, 10.0), Pt::new(40.0, 70.0)], true).unwrap();
        let circ = l.add_shape_circle(Pt::new(200.0, 150.0), 40.0).unwrap();
        assert!(l.add_viewport_from_shape(open, Pt::new(0.0, 0.0), 500.0).is_none(), "an open polyline is no boundary");
        let t = l.add_viewport_from_shape(tri, Pt::new(0.0, 0.0), 500.0).unwrap();
        assert_eq!(l.viewport(t).unwrap().rect, [10.0, 10.0, 80.0, 70.0]);
        let c = l.add_viewport_from_shape(circ, Pt::new(0.0, 0.0), 500.0).unwrap();
        let cv = l.viewport(c).unwrap();
        assert!(cv.contains_paper(Pt::new(200.0, 150.0)) && cv.contains_paper(Pt::new(235.0, 150.0)));
        assert!(!cv.contains_paper(Pt::new(238.0, 178.0)), "the corner of the box is outside the circle");
        assert_eq!(l.shape_at(Pt::new(200.0, 190.5), 1.0), Some(circ));
        assert_eq!(l.shape_at(Pt::new(150.0, 100.0), 1.0), None);
        assert!(l.add_shape_circle(Pt::new(0.0, 0.0), 0.0).is_none());
    }

    #[test]
    fn tabs_add_rename_delete_duplicate_reorder() {
        let mut list = LayoutList::default();
        let a = list.add(Layout::new("A", "A3", true, 420.0, 297.0)).unwrap();
        let b = list.add(Layout::new("B", "A3", true, 420.0, 297.0)).unwrap();
        let c = list.add(Layout::new("C", "A4", false, 210.0, 297.0)).unwrap();
        assert_eq!(list.names(), ["A", "B", "C"]);
        assert!(a != b && b != c);
        // Names: unique, not empty, never the model tab, case-insensitive.
        assert_eq!(list.add(Layout::new("a", "A3", true, 420.0, 297.0)), Err(LayoutError::DuplicateName("a".into())));
        assert_eq!(list.add(Layout::new("  ", "A3", true, 420.0, 297.0)), Err(LayoutError::EmptyName));
        assert_eq!(list.add(Layout::new("model", "A3", true, 420.0, 297.0)), Err(LayoutError::ReservedName("model".into())));
        assert_eq!(list.unique_name("A"), "A (2)");
        assert_eq!(list.unique_name("Model"), "Model (2)");
        assert_eq!(list.unique_name(""), "Layout");
        // Rename.
        list.rename(b, " Plan ").unwrap();
        assert_eq!(list.names(), ["A", "Plan", "C"]);
        assert_eq!(list.rename(b, "c"), Err(LayoutError::DuplicateName("c".into())));
        assert_eq!(list.rename(b, "Plan"), Ok(()), "keeping a name is fine");
        assert_eq!(list.rename(999, "x"), Err(LayoutError::NotFound));
        // Reorder.
        assert!(list.move_by(c, -1));
        assert_eq!(list.names(), ["A", "C", "Plan"]);
        assert!(!list.move_by(a, -1), "already first");
        assert!(list.move_by(a, 5));
        assert_eq!(list.names(), ["C", "Plan", "A"]);
        // Duplicate follows the original and has its own id and name.
        let d = list.duplicate(c).unwrap();
        assert_eq!(list.names(), ["C", "C (2)", "Plan", "A"]);
        assert!(list.get(d).is_some() && d != c);
        // Delete.
        list.delete(a).unwrap();
        assert_eq!(list.names(), ["C", "C (2)", "Plan"]);
        assert_eq!(list.delete(a), Err(LayoutError::NotFound));
        // Ids are not reused after a delete.
        let e = list.add(Layout::new("E", "A3", true, 420.0, 297.0)).unwrap();
        assert!(e > d);
    }
}
