// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Point helpers and the generated form of an annotation: strokes, fills and labels.
//!
//! Everything an annotation draws is first reduced to an [`AnnGeometry`] in the element's local
//! frame (millimetres, local origin at the element's `origin`). The kernel drawing, the DXF
//! export and the sheet output all read that one description, so what is seen, exported and
//! printed agree.

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use afrobim_sheets::{text_width, Align, SheetDrawing, SheetLabel};
use serde::{Deserialize, Serialize};

use crate::font;

pub type Pt = Pt2;

pub fn pt(x: f64, y: f64) -> Pt {
    Pt2::new(x, y)
}
pub fn add(a: Pt, b: Pt) -> Pt {
    pt(a.x + b.x, a.y + b.y)
}
pub fn sub(a: Pt, b: Pt) -> Pt {
    pt(a.x - b.x, a.y - b.y)
}
pub fn mul(a: Pt, k: f64) -> Pt {
    pt(a.x * k, a.y * k)
}
pub fn dot(a: Pt, b: Pt) -> f64 {
    a.x * b.x + a.y * b.y
}
pub fn cross(a: Pt, b: Pt) -> f64 {
    a.x * b.y - a.y * b.x
}
pub fn len(a: Pt) -> f64 {
    a.x.hypot(a.y)
}
pub fn dist(a: Pt, b: Pt) -> f64 {
    len(sub(a, b))
}
pub fn mid(a: Pt, b: Pt) -> Pt {
    pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}
/// Unit vector, or `None` for a zero vector.
pub fn unit(a: Pt) -> Option<Pt> {
    let l = len(a);
    (l > 1e-12).then(|| pt(a.x / l, a.y / l))
}
/// Rotated 90 degrees anticlockwise.
pub fn perp(a: Pt) -> Pt {
    pt(-a.y, a.x)
}
pub fn rot(a: Pt, deg: f64) -> Pt {
    let (s, c) = deg.to_radians().sin_cos();
    pt(a.x * c - a.y * s, a.x * s + a.y * c)
}
pub fn dir(deg: f64) -> Pt {
    let (s, c) = deg.to_radians().sin_cos();
    pt(c, s)
}
/// Angle of a vector, degrees anticlockwise from +X, in [0, 360).
pub fn angle_of(a: Pt) -> f64 {
    a.y.atan2(a.x).to_degrees().rem_euclid(360.0)
}
/// A text rotation that reads left to right or bottom to top: an angle in (-90, 90]
/// turned by 180 degrees when it would read upside down. 90 is kept as 90.
pub fn readable(deg: f64) -> f64 {
    let mut a = deg.rem_euclid(360.0);
    if a > 180.0 {
        a -= 360.0;
    }
    if a > 90.0 + 1e-9 {
        a - 180.0
    } else if a <= -90.0 + 1e-9 {
        a + 180.0
    } else {
        a
    }
}

/// A stroked item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Seg {
    Line { a: Pt, b: Pt },
    /// Anticlockwise from `start_deg` to `end_deg`.
    Arc { centre: Pt, radius: f64, start_deg: f64, end_deg: f64 },
    Poly { pts: Vec<Pt>, closed: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillKind {
    Arrow,
    Dot,
    /// A background mask behind text.
    Mask,
}

/// A filled polygon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub kind: FillKind,
    pub pts: Vec<Pt>,
}

/// One line of text. `at` is the baseline point the horizontal `align` refers to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub text: String,
    pub at: Pt,
    /// Capital height in the same units as the geometry.
    pub height: f64,
    pub rotation_deg: f64,
    pub align: Align,
    pub width_factor: f64,
    pub bold: bool,
    pub style: String,
}

impl Label {
    /// Width of the text, in the geometry's units.
    pub fn width(&self) -> f64 {
        text_width(&self.text, self.height, self.bold) * self.width_factor
    }

    /// The four corners of the label's cap box (baseline to cap height), anticlockwise from the
    /// lower left of the unrotated box.
    pub fn corners(&self) -> [Pt; 4] {
        let w = self.width();
        let x0 = match self.align {
            Align::Left => 0.0,
            Align::Centre => -w / 2.0,
            Align::Right => -w,
        };
        let c = |x: f64, y: f64| add(self.at, rot(pt(x, y), self.rotation_deg));
        [c(x0, 0.0), c(x0 + w, 0.0), c(x0 + w, self.height), c(x0, self.height)]
    }
}

/// What an annotation draws, in the element's local frame.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AnnGeometry {
    pub segs: Vec<Seg>,
    pub fills: Vec<Fill>,
    pub labels: Vec<Label>,
    /// What the generator decided that the user may want to know, in words.
    pub notes: Vec<String>,
}

fn arc_points(centre: Pt, r: f64, a0: f64, a1: f64) -> Vec<Pt> {
    let mut end = a1;
    while end < a0 {
        end += 360.0;
    }
    let n = (((end - a0) / 5.0).ceil() as usize).max(2);
    (0..=n).map(|i| add(centre, mul(dir(a0 + (end - a0) * i as f64 / n as f64), r))).collect()
}

impl AnnGeometry {
    pub fn line(&mut self, a: Pt, b: Pt) {
        if dist(a, b) > 1e-9 {
            self.segs.push(Seg::Line { a, b });
        }
    }

    pub fn arc(&mut self, centre: Pt, radius: f64, start_deg: f64, end_deg: f64) {
        if radius > 1e-9 {
            self.segs.push(Seg::Arc { centre, radius, start_deg, end_deg });
        }
    }

    pub fn poly(&mut self, pts: Vec<Pt>, closed: bool) {
        if pts.len() >= 2 {
            self.segs.push(Seg::Poly { pts, closed });
        }
    }

    pub fn fill(&mut self, kind: FillKind, pts: Vec<Pt>) {
        if pts.len() >= 3 {
            self.fills.push(Fill { kind, pts });
        }
    }

    pub fn extend(&mut self, other: AnnGeometry) {
        self.segs.extend(other.segs);
        self.fills.extend(other.fills);
        self.labels.extend(other.labels);
        for n in other.notes {
            if !self.notes.contains(&n) {
                self.notes.push(n);
            }
        }
    }

    pub fn translate(&mut self, d: Pt) {
        let m = |p: &mut Pt| *p = add(*p, d);
        for s in &mut self.segs {
            match s {
                Seg::Line { a, b } => {
                    m(a);
                    m(b);
                }
                Seg::Arc { centre, .. } => m(centre),
                Seg::Poly { pts, .. } => pts.iter_mut().for_each(m),
            }
        }
        for f in &mut self.fills {
            f.pts.iter_mut().for_each(m);
        }
        for l in &mut self.labels {
            m(&mut l.at);
        }
    }

    /// Bounds of everything drawn, including the text boxes; `None` when empty.
    pub fn bounds(&self) -> Option<(Pt, Pt)> {
        let mut pts: Vec<Pt> = Vec::new();
        for s in &self.segs {
            match s {
                Seg::Line { a, b } => pts.extend([*a, *b]),
                Seg::Arc { centre, radius, start_deg, end_deg } => pts.extend(arc_points(*centre, *radius, *start_deg, *end_deg)),
                Seg::Poly { pts: p, .. } => pts.extend(p.iter().copied()),
            }
        }
        for f in &self.fills {
            pts.extend(f.pts.iter().copied());
        }
        for l in &self.labels {
            pts.extend(l.corners());
        }
        let first = *pts.first()?;
        Some(pts.iter().fold((first, first), |(lo, hi), p| (pt(lo.x.min(p.x), lo.y.min(p.y)), pt(hi.x.max(p.x), hi.y.max(p.y)))))
    }

    /// The kernel drawing: hairlines, arcs and fills, with each label as single-stroke lettering
    /// (see [`crate::font`]) so text is visible in a front end that cannot draw labels.
    /// Colour is left to the front end (`None` lines, black fills).
    pub fn to_drawing(&self, with_text_preview: bool) -> Drawing {
        let mut primitives = Vec::new();
        for f in &self.fills {
            if f.kind == FillKind::Mask {
                primitives.push(Primitive::Fill { outline: f.pts.clone(), colour: Colour::White });
            }
        }
        for s in &self.segs {
            match s {
                Seg::Line { a, b } => primitives.push(Primitive::Line { from: *a, to: *b, width: 0.0, colour: None }),
                Seg::Arc { centre, radius, start_deg, end_deg } => {
                    primitives.push(Primitive::Arc { centre: *centre, radius: *radius, start_deg: *start_deg, end_deg: *end_deg, colour: None })
                }
                Seg::Poly { pts, closed } => {
                    let n = pts.len();
                    let count = if *closed { n } else { n - 1 };
                    for i in 0..count {
                        primitives.push(Primitive::Line { from: pts[i], to: pts[(i + 1) % n], width: 0.0, colour: None });
                    }
                }
            }
        }
        for f in &self.fills {
            if f.kind != FillKind::Mask {
                primitives.push(Primitive::Fill { outline: f.pts.clone(), colour: Colour::Black });
            }
        }
        if with_text_preview {
            for l in &self.labels {
                let adv = |c: char| text_width(&c.to_string(), l.height, l.bold) * l.width_factor;
                let strokes = font::stroke_line(&l.text, l.height, adv);
                let w = l.width();
                let x0 = match l.align {
                    Align::Left => 0.0,
                    Align::Centre => -w / 2.0,
                    Align::Right => -w,
                };
                for s in strokes {
                    let p: Vec<Pt> = s.iter().map(|&(x, y)| add(l.at, rot(pt(x + x0, y), l.rotation_deg))).collect();
                    for w in p.windows(2) {
                        primitives.push(Primitive::Line { from: w[0], to: w[1], width: 0.0, colour: None });
                    }
                }
            }
        }
        let (min, max) = self.bounds().unwrap_or((pt(0.0, 0.0), pt(0.0, 0.0)));
        Drawing { primitives, extents: Extents { min, max } }
    }

    /// The drawing and labels in the form `afrobim-sheets` consumes ([`SheetDrawing`]): paper
    /// millimetres, each model coordinate `p` becoming `(p + offset) * scale`. Pass
    /// `scale = 1 / denominator` for a 1:denominator sheet. Text height scales with the
    /// geometry, so text drawn at paper height stays at paper height when it was sized in paper
    /// millimetres at the same annotation scale.
    pub fn to_sheet(&self, offset: Pt, scale: f64, layer: &str) -> SheetDrawing {
        let mut g = self.clone();
        g.translate(offset);
        let mut d = g.to_drawing(false);
        for p in &mut d.primitives {
            *p = scale_primitive(p, scale);
        }
        d.extents = Extents { min: mul(d.extents.min, scale), max: mul(d.extents.max, scale) };
        let labels = g
            .labels
            .iter()
            .map(|l| SheetLabel {
                at: mul(l.at, scale),
                text: l.text.clone(),
                height_mm: l.height * scale,
                rotation_deg: l.rotation_deg,
                align: l.align,
                bold: l.bold,
                layer: layer.to_string(),
            })
            .collect();
        SheetDrawing { drawing: d, labels }
    }
}

fn scale_primitive(p: &Primitive, k: f64) -> Primitive {
    match p {
        Primitive::Fill { outline, colour } => Primitive::Fill { outline: outline.iter().map(|q| mul(*q, k)).collect(), colour: *colour },
        Primitive::Line { from, to, width, colour } => Primitive::Line { from: mul(*from, k), to: mul(*to, k), width: width * k, colour: *colour },
        Primitive::Arc { centre, radius, start_deg, end_deg, colour } => {
            Primitive::Arc { centre: mul(*centre, k), radius: radius * k, start_deg: *start_deg, end_deg: *end_deg, colour: *colour }
        }
        other => other.clone(),
    }
}

/// An arrow head with its tip at `tip`, pointing along `dir_out` (away from the line it ends),
/// `size` long and `size / 3` wide at the base.
pub fn arrow_head(tip: Pt, dir_out: Pt, size: f64) -> Vec<Pt> {
    let base = sub(tip, mul(dir_out, size));
    let w = mul(perp(dir_out), size / 6.0);
    vec![tip, add(base, w), sub(base, w)]
}

/// A disc as a polygon.
pub fn disc(centre: Pt, radius: f64) -> Vec<Pt> {
    (0..16).map(|i| add(centre, mul(dir(i as f64 * 22.5), radius))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_turns_upside_down_text() {
        assert_eq!(readable(0.0), 0.0);
        assert_eq!(readable(45.0), 45.0);
        assert_eq!(readable(90.0), 90.0);
        assert!((readable(135.0) - -45.0).abs() < 1e-9);
        assert!((readable(180.0) - 0.0).abs() < 1e-9);
        assert!((readable(270.0) - 90.0).abs() < 1e-9);
        assert!((readable(315.0) - -45.0).abs() < 1e-9);
        assert!((readable(200.0) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn label_corners_follow_alignment_and_rotation() {
        let l = Label { text: "AB".into(), at: pt(10.0, 20.0), height: 2.5, rotation_deg: 90.0, align: Align::Left, width_factor: 1.0, bold: false, style: String::new() };
        let w = l.width();
        let c = l.corners();
        // Rotated 90 degrees: the box rises from the baseline point; its height goes to -X.
        assert!((c[1].x - 10.0).abs() < 1e-9 && (c[1].y - (20.0 + w)).abs() < 1e-9);
        assert!((c[3].x - (10.0 - 2.5)).abs() < 1e-9 && (c[3].y - 20.0).abs() < 1e-9);
    }

    #[test]
    fn arrow_head_has_tip_and_base_width() {
        let a = arrow_head(pt(0.0, 0.0), pt(1.0, 0.0), 3.0);
        assert_eq!(a[0], pt(0.0, 0.0));
        assert!((a[1].x - -3.0).abs() < 1e-12 && (a[1].y - 0.5).abs() < 1e-12 && (a[2].y - -0.5).abs() < 1e-12);
    }
}
