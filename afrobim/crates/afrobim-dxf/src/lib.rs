// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! DXF export.
//!
//! Writes [`Drawing`]s as an R12 (AC1009) ASCII DXF in millimetres, the format the C# SignCore
//! `DxfWriter` produces and AutoCAD / Civil 3D read without complaint: every outline is a classic
//! POLYLINE / VERTEX / SEQEND, geometry is split across one layer per sign colour, lettering
//! outlines go on `SIGN-TEXT` and unresolved artwork on `SYMBOL`. Output is byte-identical to the
//! C# writer for the same primitives (see `tests/golden.rs`).

pub mod reader;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use afrobim_draw::{Colour, Drawing, Primitive, Pt2};

/// AutoCAD Colour Index for a sign colour. White and black both map to 7, which a CAD system
/// shows as white on a dark background and black on a light one.
pub fn aci(colour: Colour) -> u8 {
    match colour {
        Colour::Red => 1,
        Colour::Yellow => 2,
        Colour::Green => 3,
        Colour::Blue => 5,
        Colour::Brown => 30,
        Colour::Grey => 8,
        Colour::Black | Colour::White => 7,
    }
}

/// The layer a colour's geometry is written to.
pub fn layer_for(colour: Colour) -> &'static str {
    match colour {
        Colour::White => "SIGN-WHITE",
        Colour::Black => "SIGN-BLACK",
        Colour::Green => "SIGN-GREEN",
        Colour::Blue => "SIGN-BLUE",
        Colour::Brown => "SIGN-BROWN",
        Colour::Yellow => "SIGN-YELLOW",
        Colour::Red => "SIGN-RED",
        Colour::Grey => "SIGN-GREY",
    }
}

const TEXT_STYLES: [(&str, &str); 2] = [("TransportMedium", "tranm___.ttf"), ("TransportHeavy", "tranh___.ttf")];

/// One drawing, scaled by `scale` (pass the sign's stroke width in mm for a drawing in stroke
/// widths, or 1.0 for a drawing already in millimetres).
pub fn write(drawing: &Drawing, scale: f64) -> String {
    let mut doc = DxfDocument::new();
    doc.add(drawing, scale, Pt2::new(0.0, 0.0));
    doc.finish()
}

/// A DXF file under construction. Add any number of drawings, each with its own scale and
/// position in millimetres, then call [`DxfDocument::finish`].
#[derive(Default)]
pub struct DxfDocument {
    entities: String,
    /// Layers named by the free-form entity calls (`add_text`, `add_polyline`...), with their
    /// colour. Empty unless those are used, in which case the file is exactly as before.
    extra_layers: BTreeMap<String, u8>,
}

/// Horizontal justification of a [`DxfText`] about its point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Centre,
    Right,
}

/// One line of text as a DXF TEXT entity.
///
/// `at` is the baseline point the alignment refers to and `height` the capital height, both in
/// drawing units (millimetres for the files this crate writes). R12 has TEXT but no MTEXT, so
/// several lines are several entities: call `add_text` once per line. Characters above 127 are
/// written as `\U+XXXX` escapes, which CAD readers decode, so the file stays plain ASCII.
#[derive(Debug, Clone, PartialEq)]
pub struct DxfText {
    pub text: String,
    pub at: Pt2,
    pub height: f64,
    pub rotation_deg: f64,
    pub align: TextAlign,
    pub layer: String,
    /// AutoCAD Colour Index.
    pub aci: u8,
}

impl DxfText {
    /// Left-aligned, upright text on `layer` in colour 7.
    pub fn new(text: impl Into<String>, at: Pt2, height: f64, layer: impl Into<String>) -> Self {
        DxfText { text: text.into(), at, height, rotation_deg: 0.0, align: TextAlign::Left, layer: layer.into(), aci: 7 }
    }
}

impl DxfDocument {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `drawing` scaled by `scale`, then moved by `offset` (mm).
    pub fn add(&mut self, drawing: &Drawing, scale: f64, offset: Pt2) {
        let w = &mut self.entities;
        let tx = |p: &Pt2| Pt2::new(p.x * scale + offset.x, p.y * scale + offset.y);
        for prim in &drawing.primitives {
            match prim {
                Primitive::Line { from, to, width, colour } => {
                    let colour = colour.unwrap_or(Colour::White);
                    let (dx, dy) = (to.x - from.x, to.y - from.y);
                    let len = (dx * dx + dy * dy).sqrt();
                    if len < 1e-12 {
                        continue;
                    }
                    if *width > 0.0 {
                        // A band is written as the closed rectangle that is cut.
                        let h = width / 2.0;
                        let (nx, ny) = (-dy / len * h, dx / len * h);
                        let quad = [
                            Pt2::new(from.x - nx, from.y - ny),
                            Pt2::new(to.x - nx, to.y - ny),
                            Pt2::new(to.x + nx, to.y + ny),
                            Pt2::new(from.x + nx, from.y + ny),
                        ];
                        let layer = layer_for(colour);
                        begin_polyline(w, layer);
                        pair(w, 62, &aci(colour).to_string());
                        end_polyline(w, layer, quad.iter().map(&tx));
                    } else {
                        let (a, b) = (tx(from), tx(to));
                        pair(w, 0, "LINE");
                        pair(w, 8, layer_for(colour));
                        pair(w, 62, &aci(colour).to_string());
                        point(w, 10, a);
                        point(w, 11, b);
                    }
                }
                Primitive::Arc { centre, radius, start_deg, end_deg, colour } => {
                    let colour = colour.unwrap_or(Colour::White);
                    pair(w, 0, "ARC");
                    pair(w, 8, layer_for(colour));
                    pair(w, 62, &aci(colour).to_string());
                    point(w, 10, tx(centre));
                    num(w, 40, radius * scale);
                    num(w, 50, *start_deg);
                    num(w, 51, *end_deg);
                }
                Primitive::Fill { outline, colour } => {
                    let layer = layer_for(*colour);
                    begin_polyline(w, layer);
                    pair(w, 62, &aci(*colour).to_string());
                    end_polyline(w, layer, outline.iter().map(&tx));
                }
                Primitive::Glyphs { contours, ink, .. } => {
                    for contour in contours.iter().filter(|c| c.len() >= 3) {
                        begin_polyline(w, "SIGN-TEXT");
                        if let Some(ink) = ink {
                            pair(w, 62, &aci(*ink).to_string());
                        }
                        end_polyline(w, "SIGN-TEXT", contour.iter().map(&tx));
                    }
                }
                Primitive::Symbol { origin, size, .. } => {
                    // A cyan box with a diagonal cross marks artwork that is not resolved yet.
                    let c = [
                        Pt2::new(origin.x, origin.y),
                        Pt2::new(origin.x + size, origin.y),
                        Pt2::new(origin.x + size, origin.y + size),
                        Pt2::new(origin.x, origin.y + size),
                    ];
                    begin_polyline(w, "SYMBOL");
                    pair(w, 62, "4");
                    end_polyline(w, "SYMBOL", c.iter().map(&tx));
                    for (a, b) in [(c[0], c[2]), (c[1], c[3])] {
                        pair(w, 0, "LINE");
                        pair(w, 8, "SYMBOL");
                        pair(w, 62, "4");
                        point(w, 10, tx(&a));
                        point(w, 11, tx(&b));
                    }
                }
            }
        }
    }

    /// Adds a TEXT entity. The layer is created in the layer table if it is not one of the
    /// built-in ones. Empty text and a non-positive height are skipped.
    pub fn add_text(&mut self, t: &DxfText) {
        if t.text.trim().is_empty() || t.height <= 0.0 || t.height.is_nan() {
            return;
        }
        self.note_layer(&t.layer, t.aci);
        let w = &mut self.entities;
        pair(w, 0, "TEXT");
        pair(w, 8, &t.layer);
        pair(w, 62, &t.aci.to_string());
        point(w, 10, t.at);
        num(w, 40, t.height);
        pair(w, 1, &dxf_text(&t.text));
        if t.rotation_deg != 0.0 {
            num(w, 50, t.rotation_deg);
        }
        pair(w, 7, "STANDARD");
        if t.align != TextAlign::Left {
            pair(w, 72, if t.align == TextAlign::Centre { "1" } else { "2" });
            // With a justification the second alignment point is the one that counts.
            point(w, 11, t.at);
        }
    }

    /// Adds an open or closed POLYLINE of constant `width` (0 for a hairline) on `layer`.
    /// R12 has no lineweight on entities, so a plotted pen width is carried as polyline width.
    pub fn add_polyline(&mut self, layer: &str, aci: u8, points: &[Pt2], closed: bool, width: f64) {
        if points.len() < 2 {
            return;
        }
        self.note_layer(layer, aci);
        let w = &mut self.entities;
        pair(w, 0, "POLYLINE");
        pair(w, 8, layer);
        pair(w, 62, &aci.to_string());
        pair(w, 66, "1");
        num(w, 10, 0.0);
        num(w, 20, 0.0);
        num(w, 30, 0.0);
        pair(w, 70, if closed { "1" } else { "0" });
        if width > 0.0 {
            num(w, 40, width); // default starting width
            num(w, 41, width); // default ending width
        }
        end_polyline(w, layer, points.iter().copied());
    }

    /// Adds a LINE (hairline) on `layer`.
    pub fn add_line(&mut self, layer: &str, aci: u8, from: Pt2, to: Pt2) {
        self.note_layer(layer, aci);
        let w = &mut self.entities;
        pair(w, 0, "LINE");
        pair(w, 8, layer);
        pair(w, 62, &aci.to_string());
        point(w, 10, from);
        point(w, 11, to);
    }

    /// Adds a TEXT entity with a width factor (group 41, the X scale relative to the style).
    /// A factor of 1 writes exactly what [`add_text`](Self::add_text) writes.
    pub fn add_text_scaled(&mut self, t: &DxfText, width_factor: f64) {
        if t.text.trim().is_empty() || t.height <= 0.0 || t.height.is_nan() {
            return;
        }
        if (width_factor - 1.0).abs() < 1e-12 || !width_factor.is_finite() || width_factor <= 0.0 {
            self.add_text(t);
            return;
        }
        self.note_layer(&t.layer, t.aci);
        let w = &mut self.entities;
        pair(w, 0, "TEXT");
        pair(w, 8, &t.layer);
        pair(w, 62, &t.aci.to_string());
        point(w, 10, t.at);
        num(w, 40, t.height);
        pair(w, 1, &dxf_text(&t.text));
        if t.rotation_deg != 0.0 {
            num(w, 50, t.rotation_deg);
        }
        num(w, 41, width_factor);
        pair(w, 7, "STANDARD");
        if t.align != TextAlign::Left {
            pair(w, 72, if t.align == TextAlign::Centre { "1" } else { "2" });
            point(w, 11, t.at);
        }
    }

    /// Adds an ARC (group codes 10, 40, 50, 51: centre, radius, start and end angle in degrees,
    /// anticlockwise from +X). Skipped when the radius is not positive.
    pub fn add_arc(&mut self, layer: &str, aci: u8, centre: Pt2, radius: f64, start_deg: f64, end_deg: f64) {
        if radius.is_nan() || radius <= 0.0 {
            return;
        }
        self.note_layer(layer, aci);
        let w = &mut self.entities;
        pair(w, 0, "ARC");
        pair(w, 8, layer);
        pair(w, 62, &aci.to_string());
        point(w, 10, centre);
        num(w, 40, radius);
        num(w, 50, start_deg);
        num(w, 51, end_deg);
    }

    /// Adds a filled SOLID of three or four points given in order around the outline (a
    /// triangle or a quadrilateral). The file stores the corners in the order a SOLID uses, which
    /// swaps the third and fourth point of a quadrilateral.
    pub fn add_solid(&mut self, layer: &str, aci: u8, outline: &[Pt2]) {
        if outline.len() < 3 || outline.len() > 4 {
            return;
        }
        self.note_layer(layer, aci);
        let w = &mut self.entities;
        let p = |i: usize| outline[i.min(outline.len() - 1)];
        let order = if outline.len() == 4 { [0, 1, 3, 2] } else { [0, 1, 2, 2] };
        pair(w, 0, "SOLID");
        pair(w, 8, layer);
        pair(w, 62, &aci.to_string());
        for (k, i) in order.iter().enumerate() {
            point(w, 10 + k as u32, p(*i));
        }
    }

    fn note_layer(&mut self, name: &str, aci: u8) {
        const BUILT_IN: [&str; 12] = [
            "0", "SIGN", "SIGN-WHITE", "SIGN-BLACK", "SIGN-GREEN", "SIGN-BLUE", "SIGN-BROWN", "SIGN-YELLOW", "SIGN-RED", "SIGN-GREY", "SIGN-TEXT", "SYMBOL",
        ];
        if !BUILT_IN.contains(&name) {
            self.extra_layers.entry(name.to_string()).or_insert(aci);
        }
    }

    /// The complete DXF text.
    pub fn finish(self) -> String {
        let mut w = String::with_capacity(self.entities.len() + 2048);

        pair(&mut w, 0, "SECTION");
        pair(&mut w, 2, "HEADER");
        // R12, declared explicitly: AutoCAD rejects a versionless file.
        pair(&mut w, 9, "$ACADVER");
        pair(&mut w, 1, "AC1009");
        pair(&mut w, 9, "$INSUNITS");
        pair(&mut w, 70, "4"); // millimetres
        pair(&mut w, 9, "$MEASUREMENT");
        pair(&mut w, 70, "1"); // metric
        pair(&mut w, 0, "ENDSEC");

        let layers: [(&str, u8); 11] = [
            ("SIGN", 7),
            ("SIGN-WHITE", aci(Colour::White)),
            ("SIGN-BLACK", aci(Colour::Black)),
            ("SIGN-GREEN", aci(Colour::Green)),
            ("SIGN-BLUE", aci(Colour::Blue)),
            ("SIGN-BROWN", aci(Colour::Brown)),
            ("SIGN-YELLOW", aci(Colour::Yellow)),
            ("SIGN-RED", aci(Colour::Red)),
            ("SIGN-GREY", aci(Colour::Grey)),
            ("SIGN-TEXT", 7),
            ("SYMBOL", 4),
        ];
        pair(&mut w, 0, "SECTION");
        pair(&mut w, 2, "TABLES");
        pair(&mut w, 0, "TABLE");
        pair(&mut w, 2, "LAYER");
        pair(&mut w, 70, &(layers.len() + 1 + self.extra_layers.len()).to_string()); // + layer 0
        layer(&mut w, "0", 7);
        for (name, colour) in layers {
            layer(&mut w, name, colour);
        }
        for (name, colour) in &self.extra_layers {
            layer(&mut w, name, *colour);
        }
        pair(&mut w, 0, "ENDTAB");

        // Text styles, so a reader that meets TEXT entities keeps the Transport lettering.
        pair(&mut w, 0, "TABLE");
        pair(&mut w, 2, "STYLE");
        pair(&mut w, 70, &(TEXT_STYLES.len() + 1).to_string()); // + STANDARD
        style(&mut w, "STANDARD", "txt");
        for (name, file) in TEXT_STYLES {
            style(&mut w, name, file);
        }
        pair(&mut w, 0, "ENDTAB");
        pair(&mut w, 0, "ENDSEC");

        pair(&mut w, 0, "SECTION");
        pair(&mut w, 2, "ENTITIES");
        w.push_str(&self.entities);
        pair(&mut w, 0, "ENDSEC");
        w.push_str("0\nEOF\n");
        w
    }
}

fn pair(w: &mut String, code: u32, value: &str) {
    let _ = write!(w, "{code}\n{value}\n");
}

/// A number with at most four decimals and no trailing zeros, as the C# `"0.####"` format writes
/// it: the value is first taken to 15 significant digits, then rounded half away from zero.
/// (Rounding the binary value directly would turn 1790.78125 into 1790.7812 and
/// 2614.8437499999995 into 2614.8437, where the C# writer gives ...13 and ...38.)
fn fmt_num(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".to_string();
    }
    // 15 significant digits: "d.dddddddddddddde±x".
    let sci = format!("{:.14e}", v.abs());
    let (mantissa, exp) = sci.split_once('e').expect("scientific format has an exponent");
    let exp: i32 = exp.parse().expect("exponent is an integer");
    let mut digits: Vec<u8> = mantissa.bytes().filter(|b| b.is_ascii_digit()).map(|b| b - b'0').collect();
    // Number of digits before the decimal point (may be ≤ 0 for small values).
    let mut int_len = exp + 1;
    if int_len < 0 {
        // Pad so there is one leading zero slot before the first kept decimal.
        let pad = (-int_len) as usize;
        digits.splice(0..0, std::iter::repeat_n(0, pad));
        int_len = 0;
    }
    let keep = (int_len as usize) + 4; // integer digits + four decimals
    while digits.len() < keep + 1 {
        digits.push(0);
    }
    let round_up = digits[keep] >= 5;
    digits.truncate(keep);
    if round_up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, 1);
                int_len += 1;
                break;
            }
            i -= 1;
            if digits[i] == 9 {
                digits[i] = 0;
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let split = int_len as usize;
    let mut out = String::new();
    if v < 0.0 {
        out.push('-');
    }
    if split == 0 {
        out.push('0');
    }
    out.extend(digits[..split].iter().map(|d| (d + b'0') as char));
    let frac: String = digits[split..].iter().map(|d| (d + b'0') as char).collect();
    let frac = frac.trim_end_matches('0');
    if !frac.is_empty() {
        out.push('.');
        out.push_str(frac);
    }
    out
}

/// Text for a group 1 value: ASCII as is, anything else as `\U+XXXX` (UTF-16 units above the
/// basic plane as two escapes), control characters dropped.
fn dxf_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\n' | '\r' | '\t' => out.push(' '),
            c if (c as u32) < 32 => {}
            c if c.is_ascii() => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\U+{u:04X}");
                }
            }
        }
    }
    out
}

fn num(w: &mut String, code: u32, value: f64) {
    pair(w, code, &fmt_num(value));
}

/// X, Y, Z group codes starting at `code` (10 → 10/20/30, 11 → 11/21/31).
fn point(w: &mut String, code: u32, p: Pt2) {
    num(w, code, p.x);
    num(w, code + 10, p.y);
    num(w, code + 20, 0.0);
}

fn layer(w: &mut String, name: &str, colour: u8) {
    pair(w, 0, "LAYER");
    pair(w, 2, name);
    pair(w, 70, "0");
    pair(w, 62, &colour.to_string());
    pair(w, 6, "CONTINUOUS");
}

fn style(w: &mut String, name: &str, font_file: &str) {
    pair(w, 0, "STYLE");
    pair(w, 2, name);
    pair(w, 70, "0");
    num(w, 40, 0.0); // fixed height 0: each TEXT's own height applies
    num(w, 41, 1.0); // width factor
    num(w, 50, 0.0); // oblique angle
    pair(w, 71, "0");
    num(w, 42, 2.5); // last height used (informational)
    pair(w, 3, font_file);
    pair(w, 4, "");
}

fn begin_polyline(w: &mut String, layer: &str) {
    pair(w, 0, "POLYLINE");
    pair(w, 8, layer);
    pair(w, 66, "1");
    num(w, 10, 0.0);
    num(w, 20, 0.0);
    num(w, 30, 0.0);
    pair(w, 70, "1"); // closed
}

fn end_polyline(w: &mut String, layer: &str, points: impl Iterator<Item = Pt2>) {
    for p in points {
        pair(w, 0, "VERTEX");
        pair(w, 8, layer);
        point(w, 10, p);
    }
    pair(w, 0, "SEQEND");
    pair(w, 8, layer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_draw::Extents;

    #[test]
    fn numbers_use_four_decimals_without_trailing_zeros() {
        assert_eq!(fmt_num(0.0), "0");
        assert_eq!(fmt_num(12.5), "12.5");
        assert_eq!(fmt_num(std::f64::consts::PI), "3.1416");
        assert_eq!(fmt_num(2112.0), "2112");
        assert_eq!(fmt_num(-7.25), "-7.25");
        assert_eq!(fmt_num(100.00004), "100");
        assert_eq!(fmt_num(1790.78125), "1790.7813"); // an exact tie rounds away from zero
        assert_eq!(fmt_num(0.99996), "1");
        assert_eq!(fmt_num(9.99995), "10");
        assert_eq!(fmt_num(-0.00001), "-0");
        assert_eq!(fmt_num(2614.8437499999995), "2614.8438"); // 15 significant digits first
        assert_eq!(fmt_num(0.00005), "0.0001");
        assert_eq!(fmt_num(0.00004), "0");
        assert_eq!(fmt_num(0.5), "0.5");
        assert_eq!(fmt_num(123456.0), "123456");
    }

    #[test]
    fn offset_moves_a_drawing_and_scale_applies_first() {
        let d = Drawing {
            primitives: vec![Primitive::Fill {
                outline: vec![Pt2::new(0.0, 0.0), Pt2::new(2.0, 0.0), Pt2::new(2.0, 1.0)],
                colour: Colour::Green,
            }],
            extents: Extents::default(),
        };
        let mut doc = DxfDocument::new();
        doc.add(&d, 25.0, Pt2::new(1000.0, -500.0));
        let text = doc.finish();
        assert!(text.contains("VERTEX\n8\nSIGN-GREEN\n10\n1050\n20\n-475\n30\n0\n"));
        assert!(text.starts_with("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n"));
        assert!(text.ends_with("0\nENDSEC\n0\nEOF\n"));
    }

    #[test]
    fn text_and_free_polylines_add_layers_and_leave_plain_output_untouched() {
        let plain = DxfDocument::new().finish();
        let mut doc = DxfDocument::new();
        doc.add_text(&DxfText { align: TextAlign::Centre, rotation_deg: 90.0, aci: 1, ..DxfText::new("STA 1+000 \u{b0}", Pt2::new(10.0, 20.0), 2.5, "SHEET-TEXT") });
        doc.add_polyline("SHEET-FRAME", 7, &[Pt2::new(0.0, 0.0), Pt2::new(5.0, 0.0)], false, 0.5);
        let text = doc.finish();
        assert!(text.contains("0\nTEXT\n8\nSHEET-TEXT\n62\n1\n10\n10\n20\n20\n30\n0\n40\n2.5\n1\nSTA 1+000 \\U+00B0\n50\n90\n7\nSTANDARD\n72\n1\n11\n10\n21\n20\n31\n0\n"));
        assert!(text.contains("0\nLAYER\n2\nSHEET-TEXT\n70\n0\n62\n1\n6\nCONTINUOUS\n"));
        assert!(text.contains("70\n0\n40\n0.5\n41\n0.5\n0\nVERTEX\n8\nSHEET-FRAME"));
        assert!(text.contains("2\nLAYER\n70\n14\n")); // 11 built in + layer 0 + two new
        // No text, no new layers: the file is what it was.
        assert!(plain.contains("2\nLAYER\n70\n12\n"));
        assert!(!plain.contains("TEXT\n8\n"));
    }

    #[test]
    fn empty_text_and_zero_height_are_skipped() {
        let mut doc = DxfDocument::new();
        doc.add_text(&DxfText::new("  ", Pt2::new(0.0, 0.0), 2.5, "T"));
        doc.add_text(&DxfText::new("x", Pt2::new(0.0, 0.0), 0.0, "T"));
        assert_eq!(doc.finish(), DxfDocument::new().finish());
    }
}
