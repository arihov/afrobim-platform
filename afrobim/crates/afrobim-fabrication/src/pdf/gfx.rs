// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Page content streams, written the way PDFsharp 6.2.4's `XGraphics` writes them for a page in
//! millimetres with the y axis pointing down.
//!
//! The C# writers draw through `XGraphics`; the bytes PDFsharp turns those calls into are the
//! reference. This module reproduces them: the graphics-state bookkeeping (an operator is
//! written only when the value differs from what the content stream already has), the number
//! formats, text placement from the font's advance widths, and arcs as Bézier curves.

use std::collections::BTreeSet;

use crate::num::fixed;

use super::ttf::TrueTypeFont;

/// An RGB colour as PDFsharp's `XColor` holds it.
pub(crate) type Rgb = [u8; 3];

pub(crate) const BLACK: Rgb = [0, 0, 0];

/// A pen: colour and width in page units. Flat caps, mitre joins, solid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pen {
    pub colour: Rgb,
    pub width: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Centre,
    Right,
}

/// The two faces a document sets text in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Face {
    Regular = 0,
    Bold = 1,
}

/// The two graphics-state dictionaries PDFsharp creates: non-stroking and stroking alpha 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExtGState {
    Fill,
    Stroke,
}

/// What a document's pages share: object numbers handed out in order of first use, and the
/// glyphs each face has drawn (for the font subsets).
#[derive(Debug, Default)]
pub(crate) struct DocResources {
    pub next_object: u32,
    pub ext_fill: Option<u32>,
    pub ext_stroke: Option<u32>,
    /// (descriptor object, font object) per face.
    pub fonts: [Option<(u32, u32)>; 2],
    pub glyphs: [BTreeSet<u16>; 2],
}

impl DocResources {
    pub fn allocate(&mut self) -> u32 {
        self.next_object += 1;
        self.next_object
    }
}

/// A 2D affine matrix in the row-vector convention of `XMatrix`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Matrix {
    m11: f64,
    m12: f64,
    m21: f64,
    m22: f64,
    dx: f64,
    dy: f64,
}

impl Matrix {
    const IDENTITY: Matrix = Matrix { m11: 1.0, m12: 0.0, m21: 0.0, m22: 1.0, dx: 0.0, dy: 0.0 };

    /// `self * other`: apply `self` first.
    fn then(&self, o: &Matrix) -> Matrix {
        Matrix {
            m11: self.m11 * o.m11 + self.m12 * o.m21,
            m12: self.m11 * o.m12 + self.m12 * o.m22,
            m21: self.m21 * o.m11 + self.m22 * o.m21,
            m22: self.m21 * o.m12 + self.m22 * o.m22,
            dx: self.dx * o.m11 + self.dy * o.m21 + o.dx,
            dy: self.dx * o.m12 + self.dy * o.m22 + o.dy,
        }
    }

    fn rotation_at(degrees: f64, cx: f64, cy: f64) -> Matrix {
        let radians = (degrees % 360.0) * (std::f64::consts::PI / 180.0);
        let (sin, cos) = (radians.sin(), radians.cos());
        Matrix { m11: cos, m12: sin, m21: -sin, m22: cos, dx: cx * (1.0 - cos) + cy * sin, dy: cy * (1.0 - cos) - cx * sin }
    }

    fn inverse(&self) -> Matrix {
        let invdet = 1.0 / (self.m11 * self.m22 - self.m12 * self.m21);
        Matrix {
            m11: self.m22 * invdet,
            m12: -self.m12 * invdet,
            m21: -self.m21 * invdet,
            m22: self.m11 * invdet,
            dx: (self.m21 * self.dy - self.dx * self.m22) * invdet,
            dy: (self.dx * self.m12 - self.m11 * self.dy) * invdet,
        }
    }

    fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        if *self == Matrix::IDENTITY {
            return (x, y);
        }
        (x * self.m11 + y * self.m21 + self.dx, x * self.m12 + y * self.m22 + self.dy)
    }
}

/// What the content stream currently has set, as far as the writer tracks it.
#[derive(Debug, Clone, PartialEq)]
struct State {
    line_width: Option<f64>,
    line_cap_set: bool,
    line_join_set: bool,
    dash_set: bool,
    stroke: Rgb,
    stroke_alpha_set: bool,
    fill: Option<Rgb>,
    fill_alpha_set: bool,
    font: Option<(Face, f64)>,
    /// The caller's transform (y down).
    world: Matrix,
    /// The `cm` operators written so far, after the page's own.
    effective: Matrix,
    inverse_effective: Matrix,
    /// Transforms asked for and not written yet.
    unrealized: Matrix,
}

/// One page being drawn. Coordinates are millimetres from the top-left corner, y down.
pub(crate) struct Gfx<'a> {
    out: String,
    page_height: f64,
    fonts: [&'a TrueTypeFont; 2],
    res: &'a mut DocResources,
    state: State,
    saved: Vec<State>,
    in_text: bool,
    /// Where the last `Td` left the text line, in the current text object.
    text_origin: (f64, f64),
    /// Resource names in order of first use on this page: `/GS0`, `/GS1` and `/F0`, `/F1`.
    pub ext_gstates: Vec<ExtGState>,
    pub faces: Vec<Face>,
}

const KAPPA: f64 = 0.5522847498;
const DEG2RAD: f64 = std::f64::consts::PI / 180.0;

#[derive(Clone, Copy, PartialEq)]
enum PathStart {
    MoveTo,
    Ignore,
}

impl<'a> Gfx<'a> {
    pub fn new(res: &'a mut DocResources, fonts: [&'a TrueTypeFont; 2], page_height_mm: f64) -> Self {
        Gfx {
            out: String::new(),
            page_height: page_height_mm,
            fonts,
            res,
            state: State {
                line_width: None,
                line_cap_set: false,
                line_join_set: false,
                dash_set: false,
                stroke: BLACK,
                stroke_alpha_set: false,
                fill: None,
                fill_alpha_set: false,
                font: None,
                world: Matrix::IDENTITY,
                effective: Matrix::IDENTITY,
                inverse_effective: Matrix::IDENTITY,
                unrealized: Matrix::IDENTITY,
            },
            saved: Vec::new(),
            in_text: false,
            text_origin: (0.0, 0.0),
            ext_gstates: Vec::new(),
            faces: Vec::new(),
        }
    }

    /// The finished content stream. A page nothing was drawn on has an empty one. PDFsharp
    /// ends the stream on the last `Q`, without a line end.
    pub fn finish(mut self) -> Vec<u8> {
        if self.out.is_empty() {
            return Vec::new();
        }
        self.end_text();
        self.out.push_str("Q\nQ");
        self.out.into_bytes()
    }

    // ─── state ────────────────────────────────────────────────────────────

    fn begin_page(&mut self) {
        if self.out.is_empty() {
            let k = fixed(72.0 / 25.4, 0, 7);
            self.out.push_str(&format!("q\n{k} 0 0 {k} 0 0 cm q\n"));
        }
    }

    fn end_text(&mut self) {
        if self.in_text {
            self.out.push_str("ET\n");
            self.in_text = false;
        }
    }

    /// `XGraphics.Save`.
    pub fn save(&mut self) {
        self.begin_page();
        self.end_text();
        self.out.push_str("q\n");
        self.saved.push(self.state.clone());
    }

    /// `XGraphics.Restore`.
    pub fn restore(&mut self) {
        if let Some(state) = self.saved.pop() {
            self.end_text();
            self.out.push_str("Q\n");
            self.state = state;
        }
    }

    /// `XGraphics.RotateAtTransform`: `degrees` clockwise on the page (y down) about a point.
    pub fn rotate_at(&mut self, degrees: f64, x: f64, y: f64) {
        let m = Matrix::rotation_at(degrees, x, y);
        self.state.world = m.then(&self.state.world);
        // The page's y axis is flipped, which reverses the sense of rotation.
        let flipped = Matrix { m12: -m.m12, m21: -m.m21, ..m };
        self.state.unrealized = flipped.then(&self.state.unrealized);
    }

    fn realize_transform(&mut self) {
        let u = self.state.unrealized;
        if u == Matrix::IDENTITY {
            return;
        }
        let f = |v: f64| fixed(v, 0, 7);
        self.out.push_str(&format!("{} {} {} {} {} {} cm\n", f(u.m11), f(u.m12), f(u.m21), f(u.m22), f(u.dx), f(u.dy)));
        self.state.effective = u.then(&self.state.effective);
        self.state.inverse_effective = self.state.effective.inverse();
        self.state.unrealized = Matrix::IDENTITY;
    }

    fn begin_graphic(&mut self) {
        self.begin_page();
        self.end_text();
        self.realize_transform();
    }

    fn ext_gstate_name(&mut self, kind: ExtGState) -> String {
        let slot = match kind {
            ExtGState::Fill => &mut self.res.ext_fill,
            ExtGState::Stroke => &mut self.res.ext_stroke,
        };
        if slot.is_none() {
            self.res.next_object += 1;
            *slot = Some(self.res.next_object);
        }
        let index = match self.ext_gstates.iter().position(|k| *k == kind) {
            Some(i) => i,
            None => {
                self.ext_gstates.push(kind);
                self.ext_gstates.len() - 1
            }
        };
        format!("/GS{index}")
    }

    fn realize_pen(&mut self, pen: &Pen) {
        if self.state.line_width != Some(pen.width) {
            self.out.push_str(&format!("{} w\n", fixed(pen.width, 0, 3)));
            self.state.line_width = Some(pen.width);
        }
        if !self.state.line_cap_set {
            self.out.push_str("0 J\n");
            self.state.line_cap_set = true;
        }
        if !self.state.line_join_set {
            self.out.push_str("0 j\n");
            self.state.line_join_set = true;
        }
        if !self.state.dash_set {
            self.out.push_str("[]0 d\n");
            self.state.dash_set = true;
        }
        if self.state.stroke != pen.colour {
            self.out.push_str(&format!("{} RG\n", colour(pen.colour)));
            self.state.stroke = pen.colour;
        }
        if !self.state.stroke_alpha_set {
            let name = self.ext_gstate_name(ExtGState::Stroke);
            self.out.push_str(&format!("{name} gs\n"));
            self.state.stroke_alpha_set = true;
        }
    }

    fn realize_brush(&mut self, rgb: Rgb) {
        if self.state.fill != Some(rgb) {
            self.out.push_str(&format!("{} rg\n", colour(rgb)));
            self.state.fill = Some(rgb);
        }
        if !self.state.fill_alpha_set {
            let name = self.ext_gstate_name(ExtGState::Fill);
            self.out.push_str(&format!("{name} gs\n"));
            self.state.fill_alpha_set = true;
        }
    }

    /// A point in the caller's space as the content stream writes it.
    fn view(&self, x: f64, y: f64) -> (f64, f64) {
        let (wx, wy) = self.state.world.apply(x, y);
        self.state.inverse_effective.apply(wx, self.page_height - wy)
    }

    fn point(&self, x: f64, y: f64) -> String {
        let (vx, vy) = self.view(x, y);
        format!("{} {}", fixed(vx, 0, 4), fixed(vy, 0, 4))
    }

    // ─── drawing ──────────────────────────────────────────────────────────

    /// `XGraphics.DrawLine`.
    pub fn draw_line(&mut self, pen: &Pen, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.begin_graphic();
        self.realize_pen(pen);
        let (a, b) = (self.point(x1, y1), self.point(x2, y2));
        self.out.push_str(&format!("{a} m\n{b} l\nS\n"));
    }

    fn rectangle(&mut self, x: f64, y: f64, width: f64, height: f64) {
        let corner = self.point(x, y + height);
        self.out.push_str(&format!("{corner} {} {} re\n", fixed(width, 0, 4), fixed(height, 0, 4)));
    }

    /// `XGraphics.DrawRectangle` with a brush.
    pub fn fill_rectangle(&mut self, rgb: Rgb, x: f64, y: f64, width: f64, height: f64) {
        self.begin_graphic();
        self.realize_brush(rgb);
        self.rectangle(x, y, width, height);
        self.out.push_str("f\n");
    }

    /// `XGraphics.DrawRectangle` with a pen.
    pub fn stroke_rectangle(&mut self, pen: &Pen, x: f64, y: f64, width: f64, height: f64) {
        self.begin_graphic();
        self.realize_pen(pen);
        self.rectangle(x, y, width, height);
        self.out.push_str("S\n");
    }

    /// `XGraphics.DrawPath` with a brush on a path of closed polygons, filled even-odd.
    pub fn fill_polygons(&mut self, rgb: Rgb, polygons: &[Vec<(f64, f64)>]) {
        self.begin_graphic();
        self.realize_brush(rgb);
        for polygon in polygons {
            for (i, &(x, y)) in polygon.iter().enumerate() {
                let p = self.point(x, y);
                self.out.push_str(&format!("{p} {}\n", if i == 0 { "m" } else { "l" }));
            }
            self.out.push_str("h\n");
        }
        self.out.push_str("f*\n");
    }

    /// `XGraphics.DrawArc` for a circle: part of the circle in the square box at (`x`, `y`),
    /// angles in degrees clockwise on the page from the +x axis. A sweep of a full turn or more
    /// draws the closed circle.
    pub fn draw_arc(&mut self, pen: &Pen, x: f64, y: f64, diameter: f64, start_angle: f64, sweep_angle: f64) {
        self.begin_graphic();
        self.realize_pen(pen);
        if sweep_angle.abs() >= 360.0 {
            self.ellipse(x, y, diameter, diameter);
            self.out.push_str("h S\n");
        } else {
            self.partial_arc(x, y, diameter, start_angle, sweep_angle);
            self.out.push_str("S\n");
        }
    }

    fn curve(&mut self, p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)) {
        let s = format!("{} {} {} c\n", self.point(p1.0, p1.1), self.point(p2.0, p2.1), self.point(p3.0, p3.1));
        self.out.push_str(&s);
    }

    fn ellipse(&mut self, x: f64, y: f64, width: f64, height: f64) {
        let (dx, dy) = (width / 2.0, height / 2.0);
        let (fx, fy) = (dx * KAPPA, dy * KAPPA);
        let (x0, y0) = (x + dx, y + dy);
        let start = self.point(x0 + dx, y0);
        self.out.push_str(&format!("{start} m\n"));
        self.curve((x0 + dx, y0 + fy), (x0 + fx, y0 + dy), (x0, y0 + dy));
        self.curve((x0 - fx, y0 + dy), (x0 - dx, y0 + fy), (x0 - dx, y0));
        self.curve((x0 - dx, y0 - fy), (x0 - fx, y0 - dy), (x0, y0 - dy));
        self.curve((x0 + fx, y0 - dy), (x0 + dx, y0 - fy), (x0 + dx, y0));
    }

    /// PDFsharp's arc: one Bézier segment per quadrant the arc crosses.
    fn partial_arc(&mut self, x: f64, y: f64, diameter: f64, start_angle: f64, sweep_angle: f64) {
        let mut alpha = start_angle;
        if alpha < 0.0 {
            alpha += (1.0 + (alpha.abs() / 360.0).floor()) * 360.0;
        } else if alpha > 360.0 {
            alpha -= (alpha / 360.0).floor() * 360.0;
        }
        let mut beta = sweep_angle.clamp(-360.0, 360.0);
        if alpha == 0.0 && beta < 0.0 {
            alpha = 360.0;
        } else if alpha == 360.0 && beta > 0.0 {
            alpha = 0.0;
        }

        // Can the arc start and end in the same quadrant?
        let mut small_angle = beta.abs() <= 90.0;

        beta += alpha;
        if beta < 0.0 {
            beta += (1.0 + (beta.abs() / 360.0).floor()) * 360.0;
        }

        let clockwise = sweep_angle > 0.0;
        let start_quadrant = quadrant(alpha, true, clockwise);
        let end_quadrant = quadrant(beta, false, clockwise);

        if start_quadrant == end_quadrant && small_angle {
            self.arc_quadrant(x, y, diameter, alpha, beta, PathStart::MoveTo);
            return;
        }
        let mut current = start_quadrant;
        let mut first_loop = true;
        loop {
            let q = f64::from(current) * 90.0;
            if current == start_quadrant && first_loop {
                let xi = q + if clockwise { 90.0 } else { 0.0 };
                self.arc_quadrant(x, y, diameter, alpha, xi, PathStart::MoveTo);
            } else if current == end_quadrant {
                let xi = q + if clockwise { 0.0 } else { 90.0 };
                self.arc_quadrant(x, y, diameter, xi, beta, PathStart::Ignore);
            } else {
                let xi1 = q + if clockwise { 0.0 } else { 90.0 };
                let xi2 = q + if clockwise { 90.0 } else { 0.0 };
                self.arc_quadrant(x, y, diameter, xi1, xi2, PathStart::Ignore);
            }

            // Do not stop at once when the arc is longer than 270 degrees.
            if current == end_quadrant && small_angle {
                break;
            }
            small_angle = true;
            current = if clockwise { (current + 1) % 4 } else { (current + 3) % 4 };
            first_loop = false;
        }
    }

    fn arc_quadrant(&mut self, x: f64, y: f64, diameter: f64, mut alpha: f64, mut beta: f64, start: PathStart) {
        if beta > 360.0 {
            beta -= (beta / 360.0).floor() * 360.0;
        }
        let r = diameter / 2.0;
        let (x0, y0) = (x + r, y + r);

        // In the two upper quadrants the curve is the lower one reflected in the centre.
        let mut reflect = false;
        if alpha >= 180.0 && beta >= 180.0 {
            alpha -= 180.0;
            beta -= 180.0;
            reflect = true;
        }

        alpha *= DEG2RAD;
        beta *= DEG2RAD;

        let kappa = 4.0 * (1.0 - ((alpha - beta) / 2.0).cos()) / (3.0 * ((beta - alpha) / 2.0).sin());
        let (sin_a, cos_a, sin_b, cos_b) = (alpha.sin(), alpha.cos(), beta.sin(), beta.cos());
        let sign = if reflect { -1.0 } else { 1.0 };
        let at = |cx: f64, cy: f64| (x0 + sign * r * cx, y0 + sign * r * cy);

        if start == PathStart::MoveTo {
            let (px, py) = at(cos_a, sin_a);
            let p = self.point(px, py);
            self.out.push_str(&format!("{p} m\n"));
        }
        self.curve(at(cos_a - kappa * sin_a, sin_a + kappa * cos_a), at(cos_b + kappa * sin_b, sin_b - kappa * cos_b), at(cos_b, sin_b));
    }

    /// `XGraphics.DrawString` at a baseline point. `font` is the face and its em size in page
    /// units; the text must already be WinAnsi (the writers pass ASCII).
    pub fn draw_string(&mut self, text: &str, font: (Face, f64), rgb: Rgb, x: f64, y: f64, align: Align) {
        let (face, size) = font;
        if text.is_empty() {
            return;
        }
        self.begin_page();
        let font = self.fonts[face as usize];
        let width = font.text_width_units(text) as f64 * size / f64::from(font.units_per_em());

        if self.state.unrealized != Matrix::IDENTITY {
            self.end_text();
            self.realize_transform();
        }
        if !self.in_text {
            self.out.push_str("BT\n");
            self.in_text = true;
            self.text_origin = (0.0, 0.0);
        }
        self.realize_brush(rgb);
        if self.state.font != Some((face, size)) {
            if self.res.fonts[face as usize].is_none() {
                let descriptor = self.res.allocate();
                let font_object = self.res.allocate();
                self.res.fonts[face as usize] = Some((descriptor, font_object));
            }
            let index = match self.faces.iter().position(|f| *f == face) {
                Some(i) => i,
                None => {
                    self.faces.push(face);
                    self.faces.len() - 1
                }
            };
            self.out.push_str(&format!("/F{index} {} Tf\n", fixed(size, 0, 3)));
            self.state.font = Some((face, size));
        }
        self.res.glyphs[face as usize].extend(text.encode_utf16().map(|c| font.glyph(c)));

        let x = match align {
            Align::Left => x,
            Align::Centre => x + (0.0 - width) / 2.0,
            Align::Right => x + (0.0 - width),
        };
        let (px, py) = self.view(x, y);
        self.out.push_str(&format!(
            "{} {} Td\n{} Tj\n",
            fixed(px - self.text_origin.0, 0, 4),
            fixed(py - self.text_origin.1, 0, 4),
            string_literal(text)
        ));
        self.text_origin = (px, py);
    }
}

fn quadrant(mut phi: f64, start: bool, clockwise: bool) -> i32 {
    if phi > 360.0 {
        phi -= (phi / 360.0).floor() * 360.0;
    }
    let mut quadrant = (phi / 90.0) as i32;
    if phi == f64::from(quadrant) * 90.0 {
        if (start && !clockwise) || (!start && clockwise) {
            quadrant = if quadrant == 0 { 3 } else { quadrant - 1 };
        }
    } else {
        quadrant = if clockwise { ((phi / 90.0).floor() as i32) % 4 } else { (phi / 90.0).floor() as i32 };
    }
    quadrant
}

/// The three components as PDFsharp writes a device RGB colour.
fn colour(rgb: Rgb) -> String {
    let c = |v: u8| fixed(f64::from(v) / 255.0, 0, 3);
    format!("{} {} {}", c(rgb[0]), c(rgb[1]), c(rgb[2]))
}

/// A PDF literal string of WinAnsi bytes. Characters outside WinAnsi's Latin-1 range are
/// written as `?`.
pub(crate) fn string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('(');
    for c in text.chars() {
        match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            ' '..='~' => out.push(c),
            _ => out.push('?'),
        }
    }
    out.push(')');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::PdfFonts;
    use std::path::Path;

    fn fonts() -> PdfFonts {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fonts");
        PdfFonts::load(&dir.join("tranm___.TTF"), &dir.join("tranh___.TTF")).unwrap()
    }

    /// Draws on a page 50 mm high and returns the content stream as text.
    fn content(draw: impl FnOnce(&mut Gfx)) -> (String, DocResources, Vec<ExtGState>, Vec<Face>) {
        let fonts = fonts();
        let mut res = DocResources { next_object: 5, ..DocResources::default() };
        let (text, ext, faces) = {
            let mut g = Gfx::new(&mut res, [&fonts.regular, &fonts.bold], 50.0);
            draw(&mut g);
            let (ext, faces) = (g.ext_gstates.clone(), g.faces.clone());
            (String::from_utf8(g.finish()).unwrap(), ext, faces)
        };
        (text, res, ext, faces)
    }

    const PAGE_START: &str = "q\n2.8346457 0 0 2.8346457 0 0 cm q\n";

    // The expected streams below are what PDFsharp 6.2.4 wrote for the same calls on a
    // 100 x 50 mm page (XGraphicsUnit.Millimeter), with the Transport fonts of this repository.

    #[test]
    fn nothing_drawn_is_an_empty_stream() {
        let (text, res, ext, faces) = content(|_| {});
        assert_eq!(text, "");
        assert_eq!(res.next_object, 5);
        assert!(ext.is_empty() && faces.is_empty());
    }

    #[test]
    fn a_fill_alone_names_its_graphics_state_gs0() {
        let (text, res, ext, _) = content(|g| g.fill_rectangle(BLACK, 1.0, 1.0, 5.0, 5.0));
        assert_eq!(text, format!("{PAGE_START}0 0 0 rg\n/GS0 gs\n1 44 5 5 re\nf\nQ\nQ"));
        assert_eq!((res.ext_fill, res.ext_stroke), (Some(6), None));
        assert_eq!(ext, [ExtGState::Fill]);
    }

    #[test]
    fn a_black_stroke_alone_writes_no_colour() {
        let (text, res, ext, _) = content(|g| g.stroke_rectangle(&Pen { colour: BLACK, width: 0.5 }, 1.0, 1.0, 5.0, 5.0));
        assert_eq!(text, format!("{PAGE_START}0.5 w\n0 J\n0 j\n[]0 d\n/GS0 gs\n1 44 5 5 re\nS\nQ\nQ"));
        assert_eq!((res.ext_fill, res.ext_stroke), (None, Some(6)));
        assert_eq!(ext, [ExtGState::Stroke]);
    }

    #[test]
    fn text_lines_and_state_changes_follow_pdfsharp() {
        let (text, res, ext, faces) = content(|g| {
            g.draw_string("a(b)c\\d", (Face::Regular, 4.0), BLACK, 1.0, 10.0, Align::Left);
            g.save();
            g.draw_string("", (Face::Regular, 4.0), BLACK, 1.0, 20.0, Align::Left);
            g.restore();
            g.draw_string("   ", (Face::Bold, 4.0), BLACK, 1.0, 30.0, Align::Right);
            g.draw_string("~|{}`^_@#$%", (Face::Bold, 4.0), [255, 0, 0], 50.0, 40.0, Align::Centre);
            g.draw_line(&Pen { colour: BLACK, width: 0.123456 }, 1.0, 1.0, 2.0, 2.0);
            g.draw_line(&Pen { colour: [1, 2, 3], width: 12345.6789 }, 1.000049, 1.00005, 2.99995, 2.999949);
            g.draw_string("x", (Face::Regular, 4.0), BLACK, 1.0, 45.0, Align::Left);
            g.draw_string("y", (Face::Regular, 4.0), BLACK, 10.0, 46.0, Align::Left);
        });
        let expected = [
            "BT", "0 0 0 rg", "/GS0 gs", "/F0 4 Tf", "1 40 Td", "(a\\(b\\)c\\\\d) Tj", "ET", "q", "Q",
            "BT", "/F1 4 Tf", "-2.5156 20 Td", "(   ) Tj", "1 0 0 rg", "35.1006 -10 Td", "(~|{}`^_@#$%) Tj", "ET",
            "0.123 w", "0 J", "0 j", "[]0 d", "/GS1 gs", "1 49 m", "2 48 l", "S",
            "12345.679 w", "0.004 0.008 0.012 RG", "1 49 m", "3 47.0001 l", "S",
            "BT", "0 0 0 rg", "/F0 4 Tf", "1 5 Td", "(x) Tj", "9 -1 Td", "(y) Tj", "ET", "Q", "Q",
        ];
        assert_eq!(text, format!("{PAGE_START}{}", expected.join("\n")));
        // Fill state 6, regular face 7 and 8, bold face 9 and 10, stroke state 11: first use.
        assert_eq!((res.ext_fill, res.fonts[0], res.fonts[1], res.ext_stroke), (Some(6), Some((7, 8)), Some((9, 10)), Some(11)));
        assert_eq!(ext, [ExtGState::Fill, ExtGState::Stroke]);
        assert_eq!(faces, [Face::Regular, Face::Bold]);
        assert_eq!(res.glyphs[0].len(), 9, "a ( ) b c \\ d x y");
    }

    #[test]
    fn polygons_close_each_figure_and_fill_even_odd() {
        let (text, ..) = content(|g| {
            g.fill_polygons(
                [0x02, 0x5C, 0x2E],
                &[vec![(1.0, 1.0), (5.0, 1.0), (5.0, 5.0)], vec![(11.0, 1.0), (15.0, 1.0), (15.0, 5.0), (11.0, 1.0)]],
            );
        });
        assert_eq!(
            text,
            format!("{PAGE_START}0.008 0.361 0.18 rg\n/GS0 gs\n1 49 m\n5 49 l\n5 45 l\nh\n11 49 m\n15 49 l\n15 45 l\n11 49 l\nh\nf*\nQ\nQ")
        );
    }

    #[test]
    fn arcs_are_one_bezier_per_quadrant() {
        let pen = Pen { colour: BLACK, width: 0.25 };
        let arc = |start: f64, sweep: f64| {
            let (text, ..) = content(|g| g.draw_arc(&pen, 10.0, 10.0, 20.0, start, sweep));
            let body = text.strip_prefix(&format!("{PAGE_START}0.25 w\n0 J\n0 j\n[]0 d\n/GS0 gs\n")).unwrap().to_string();
            body.strip_suffix("\nQ\nQ").unwrap().to_string()
        };
        assert_eq!(
            arc(-30.0, -282.5),
            "28.6603 35 m\n26.8739 38.094 23.5727 40 20 40 c\n14.4772 40 10 35.5228 10 30 c\n\
             10 24.4772 14.4772 20 20 20 c\n22.5013 20 24.9118 20.9374 26.7559 22.6272 c\nS"
        );
        assert_eq!(
            arc(0.0, -360.0),
            "30 30 m\n30 24.4772 25.5228 20 20 20 c\n14.4772 20 10 24.4772 10 30 c\n\
             10 35.5228 14.4772 40 20 40 c\n25.5228 40 30 35.5228 30 30 c\nh S"
        );
        assert_eq!(arc(-90.0, -90.0), "20 40 m\n14.4772 40 10 35.5228 10 30 c\nS");
        assert_eq!(
            arc(-200.0, -180.0),
            "10.6031 26.5798 m\n12.0409 22.6293 15.796 20 20 20 c\n25.5228 20 30 24.4772 30 30 c\n\
             30 31.1665 29.7959 32.324 29.3969 33.4202 c\nS"
        );
        assert_eq!(
            arc(-350.0, -40.0),
            "29.8481 28.2635 m\n29.9492 28.8368 30 29.4179 30 30 c\n30 31.7554 29.5379 33.4798 28.6603 35 c\nS"
        );
        assert_eq!(arc(45.0, -10.0), "27.0711 22.9289 m\n27.4827 23.3406 27.8576 23.7874 28.1915 24.2642 c\nS");
        assert_eq!(arc(-720.0, -0.5), "30 30 m\n30 30.0291 29.9999 30.0582 29.9996 30.0873 c\nS");
    }

    #[test]
    fn rotated_text_is_placed_in_the_rotated_frame() {
        // The overall-height dimension of the R-616 reference sheet (page 297 mm high): text
        // "750" at cap height 3, centred on sheet point (71.3, 146), reading upwards.
        let fonts = fonts();
        let mut res = DocResources::default();
        let mut g = Gfx::new(&mut res, [&fonts.regular, &fonts.bold], 297.0);
        g.save();
        g.rotate_at(-90.0, 71.3, 151.0);
        g.draw_string("750", (Face::Regular, 3.0 / 0.7), [0xB0, 0x10, 0x20], 71.3, 151.0, Align::Centre);
        g.restore();
        let text = String::from_utf8(g.finish()).unwrap();
        assert_eq!(
            text,
            format!("{PAGE_START}q\n0 1 -1 0 -79.7 222.3 cm\nBT\n0.69 0.063 0.125 rg\n/GS0 gs\n/F0 4.286 Tf\n-80.3765 -151 Td\n(750) Tj\nET\nQ\nQ\nQ")
        );
    }

    #[test]
    fn colours_are_three_decimals_of_a_fraction() {
        assert_eq!(colour([0xB0, 0x10, 0x20]), "0.69 0.063 0.125");
        assert_eq!(colour([0, 139, 139]), "0 0.545 0.545");
        assert_eq!(colour([255, 255, 255]), "1 1 1");
        assert_eq!(colour([1, 2, 3]), "0.004 0.008 0.012");
    }

    #[test]
    fn literals_escape_parentheses_and_backslashes() {
        assert_eq!(string_literal("a(b)c\\d"), "(a\\(b\\)c\\\\d)");
        assert_eq!(string_literal("R-616  Regulatory"), "(R-616  Regulatory)");
    }

    #[test]
    fn rotation_about_a_point_matches_the_reference_matrix() {
        // The vertical dimension text of the R-616 sheet: PDFsharp wrote `0 1 -1 0 -79.7 222.3 cm`.
        let m = Matrix::rotation_at(-90.0, 71.3, 151.0);
        let f = |v: f64| fixed(v, 0, 7);
        assert_eq!([f(m.m11), f(-m.m12), f(-m.m21), f(m.m22), f(m.dx), f(m.dy)], ["0", "1", "-1", "0", "-79.7", "222.3"]);
        let (x, y) = m.then(&m.inverse()).apply(3.0, 4.0);
        assert!((x - 3.0).abs() < 1e-9 && (y - 4.0).abs() < 1e-9);
    }
}
