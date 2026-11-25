// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Writes a [`FabricationSheet`] as an ASCII DXF R12 drawing in sheet millimetres. Port of C#
//! `SheetDxfWriter`, byte for byte.
//!
//! The rules are those of the per-sign DXF: `$ACADVER AC1009`, classic POLYLINE / VERTEX /
//! SEQEND, ACI colours only, and no group 420. There is no MTEXT, HATCH, DIMENSION or lineweight
//! (370) group either: dimensions are lines and TEXT, and colour swatches are SOLIDs. Text is
//! reduced to ASCII because R12 strings are code-page text.
//!
//! The sign face is drawn at the sheet scale on the same colour layers as the per-sign DXF
//! (SIGN-GREEN, SIGN-TEXT, ...); the frame, title block and dimensions go on SHEET, TITLE and DIM.

use std::fmt::Write as _;

use afrobim_draw::{Primitive, Pt2};
use afrobim_dxf::{aci, layer_for};
use afrobim_signs::Colour;
use unicode_normalization::UnicodeNormalization;

use crate::composer::{DIM_LAYER, FRAME_LAYER, TEXT_LAYER};
use crate::num::four;
use crate::sheet::{FabricationSheet, SheetSwatch, SheetText, SheetTextAlign};

const LAYERS: [(&str, u8); 15] = [
    ("SIGN", 7),
    ("SIGN-WHITE", 7),
    ("SIGN-BLACK", 7),
    ("SIGN-GREEN", 3),
    ("SIGN-BLUE", 5),
    ("SIGN-BROWN", 30),
    ("SIGN-YELLOW", 2),
    ("SIGN-RED", 1),
    ("SIGN-GREY", 8),
    ("SIGN-TEXT", 7),
    ("SYMBOL", 4),
    (FRAME_LAYER, 7),
    (TEXT_LAYER, 7),
    (DIM_LAYER, 1),
    ("SWATCH", 7),
];

/// The sheet as DXF text (lines end with `\n`).
pub fn write(sheet: &FabricationSheet) -> String {
    let mut w = String::new();
    write_header(&mut w);
    write_tables(&mut w);

    pair(&mut w, 0, "SECTION");
    pair(&mut w, 2, "ENTITIES");
    write_face(&mut w, sheet);
    for s in &sheet.swatches {
        write_swatch(&mut w, s);
    }
    for l in &sheet.lines {
        write_line(&mut w, l.layer, None, Pt2::new(l.x1, l.y1), Pt2::new(l.x2, l.y2));
    }
    for t in &sheet.texts {
        write_text(&mut w, t);
    }
    pair(&mut w, 0, "ENDSEC");

    w.push_str("0\nEOF\n");
    w
}

/// Reduces text to printable ASCII, spelling out the few symbols a sheet uses. Accents are
/// dropped (é becomes e); anything else outside ASCII becomes `?`.
pub fn ascii(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '×' => out.push('x'),
            '²' => out.push('2'),
            '³' => out.push('3'),
            '§' => out.push('S'),
            '°' => out.push_str("deg"),
            '·' => out.push('-'),
            '–' | '—' | '‐' | '−' => out.push('-'),
            '‘' | '’' => out.push('\''),
            '“' | '”' => out.push('"'),
            '\r' | '\n' | '\t' => out.push(' '),
            ' '..='~' => out.push(c),
            _ => {
                let plain: String = c.nfd().filter(|ch| (' '..='~').contains(ch)).collect();
                if plain.is_empty() {
                    // C# works on UTF-16 code units: a character outside the BMP is two of them.
                    out.push_str(if c.len_utf16() == 2 { "??" } else { "?" });
                } else {
                    out.push_str(&plain);
                }
            }
        }
    }
    out
}

// ─── sections ─────────────────────────────────────────────────────────────

fn write_header(w: &mut String) {
    pair(w, 0, "SECTION");
    pair(w, 2, "HEADER");
    pair(w, 9, "$ACADVER");
    pair(w, 1, "AC1009");
    pair(w, 9, "$INSUNITS");
    pair(w, 70, "4");
    pair(w, 9, "$MEASUREMENT");
    pair(w, 70, "1");
    pair(w, 9, "$EXTMIN");
    point(w, 10, Pt2::new(0.0, 0.0));
    pair(w, 9, "$EXTMAX");
    point(w, 10, Pt2::new(FabricationSheet::WIDTH_MM, FabricationSheet::HEIGHT_MM));
    pair(w, 9, "$LIMMIN");
    num(w, 10, 0.0);
    num(w, 20, 0.0);
    pair(w, 9, "$LIMMAX");
    num(w, 10, FabricationSheet::WIDTH_MM);
    num(w, 20, FabricationSheet::HEIGHT_MM);
    pair(w, 0, "ENDSEC");
}

fn write_tables(w: &mut String) {
    pair(w, 0, "SECTION");
    pair(w, 2, "TABLES");

    pair(w, 0, "TABLE");
    pair(w, 2, "LAYER");
    pair(w, 70, &(LAYERS.len() + 1).to_string());
    add_layer(w, "0", 7);
    for (name, colour) in LAYERS {
        add_layer(w, name, colour);
    }
    pair(w, 0, "ENDTAB");

    pair(w, 0, "TABLE");
    pair(w, 2, "STYLE");
    pair(w, 70, "1");
    pair(w, 0, "STYLE");
    pair(w, 2, "STANDARD");
    pair(w, 70, "0");
    num(w, 40, 0.0);
    num(w, 41, 1.0);
    num(w, 50, 0.0);
    pair(w, 71, "0");
    num(w, 42, 2.5);
    pair(w, 3, "txt");
    pair(w, 4, "");
    pair(w, 0, "ENDTAB");

    pair(w, 0, "ENDSEC");
}

fn add_layer(w: &mut String, name: &str, colour: u8) {
    pair(w, 0, "LAYER");
    pair(w, 2, name);
    pair(w, 70, "0");
    pair(w, 62, &colour.to_string());
    pair(w, 6, "CONTINUOUS");
}

// ─── sign face ────────────────────────────────────────────────────────────

fn write_face(w: &mut String, sheet: &FabricationSheet) {
    let tx = |p: &Pt2| sheet.to_sheet(*p);
    for prim in &sheet.face.primitives {
        match prim {
            Primitive::Line { from, to, width, colour } => {
                // A sign-face line with a width is material (an arrow shaft, a route arm): it is
                // cut as the closed rectangle of its true width with square ends. Only a line
                // with no width stays a LINE; one with no length is skipped.
                let c = colour.unwrap_or(Colour::White);
                let (dx, dy) = (to.x - from.x, to.y - from.y);
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1e-12 {
                    continue;
                }
                if *width > 0.0 {
                    let h = width / 2.0;
                    let (nx, ny) = (-dy / len * h, dx / len * h);
                    let quad = [
                        Pt2::new(from.x + -nx, from.y + -ny),
                        Pt2::new(to.x + -nx, to.y + -ny),
                        Pt2::new(to.x + nx, to.y + ny),
                        Pt2::new(from.x + nx, from.y + ny),
                    ];
                    write_polyline(w, layer_for(c), aci(c), quad.iter().map(tx));
                } else {
                    write_line(w, layer_for(c), Some(aci(c)), tx(from), tx(to));
                }
            }
            Primitive::Arc { centre, radius, start_deg, end_deg, colour } => {
                let c = colour.unwrap_or(Colour::White);
                pair(w, 0, "ARC");
                pair(w, 8, layer_for(c));
                pair(w, 62, &aci(c).to_string());
                point(w, 10, tx(centre));
                num(w, 40, sheet.length_to_sheet(*radius));
                num(w, 50, *start_deg);
                num(w, 51, *end_deg);
            }
            Primitive::Fill { outline, colour } => {
                write_polyline(w, layer_for(*colour), aci(*colour), outline.iter().map(tx));
            }
            Primitive::Glyphs { contours, ink, .. } => {
                let colour = aci(ink.unwrap_or(Colour::White));
                for contour in contours.iter().filter(|c| c.len() >= 3) {
                    write_polyline(w, "SIGN-TEXT", colour, contour.iter().map(tx));
                }
            }
            Primitive::Symbol { origin, size, .. } => {
                let corners = [
                    *origin,
                    Pt2::new(origin.x + size, origin.y),
                    Pt2::new(origin.x + size, origin.y + size),
                    Pt2::new(origin.x, origin.y + size),
                ]
                .map(|p| sheet.to_sheet(p));
                write_polyline(w, "SYMBOL", 4, corners.into_iter());
                write_line(w, "SYMBOL", Some(4), corners[0], corners[2]);
                write_line(w, "SYMBOL", Some(4), corners[1], corners[3]);
            }
        }
    }
}

// ─── entities ─────────────────────────────────────────────────────────────

fn write_line(w: &mut String, layer: &str, colour: Option<u8>, a: Pt2, b: Pt2) {
    pair(w, 0, "LINE");
    pair(w, 8, layer);
    if let Some(c) = colour {
        pair(w, 62, &c.to_string());
    }
    point(w, 10, a);
    point(w, 11, b);
}

/// A closed polyline.
fn write_polyline(w: &mut String, layer: &str, colour: u8, points: impl Iterator<Item = Pt2>) {
    pair(w, 0, "POLYLINE");
    pair(w, 8, layer);
    pair(w, 62, &colour.to_string());
    pair(w, 66, "1");
    point(w, 10, Pt2::new(0.0, 0.0));
    pair(w, 70, "1");
    for p in points {
        pair(w, 0, "VERTEX");
        pair(w, 8, layer);
        point(w, 10, p);
    }
    pair(w, 0, "SEQEND");
    pair(w, 8, layer);
}

/// A filled colour sample: an R12 SOLID (corners in Z order) plus its outline.
fn write_swatch(w: &mut String, s: &SheetSwatch) {
    let b = &s.rect;
    pair(w, 0, "SOLID");
    pair(w, 8, "SWATCH");
    pair(w, 62, &aci(s.colour).to_string());
    point(w, 10, Pt2::new(b.min_x, b.min_y));
    point(w, 11, Pt2::new(b.max_x, b.min_y));
    point(w, 12, Pt2::new(b.min_x, b.max_y));
    point(w, 13, Pt2::new(b.max_x, b.max_y));
    let outline = [Pt2::new(b.min_x, b.min_y), Pt2::new(b.max_x, b.min_y), Pt2::new(b.max_x, b.max_y), Pt2::new(b.min_x, b.max_y)];
    write_polyline(w, "SWATCH", 7, outline.into_iter());
}

/// TEXT on the STANDARD style. Left-aligned text uses its insertion point; centred and
/// right-aligned text also carries the alignment point (11/21), which AutoCAD positions by.
fn write_text(w: &mut String, t: &SheetText) {
    let text = ascii(&t.text);
    if text.is_empty() {
        return;
    }
    pair(w, 0, "TEXT");
    pair(w, 8, t.layer);
    point(w, 10, Pt2::new(t.x, t.y));
    num(w, 40, t.height);
    pair(w, 1, &text);
    if t.rotation_deg != 0.0 {
        num(w, 50, t.rotation_deg);
    }
    pair(w, 7, "STANDARD");
    let h = match t.align {
        SheetTextAlign::Centre => 1,
        SheetTextAlign::Right => 2,
        SheetTextAlign::Left => 0,
    };
    if h != 0 {
        pair(w, 72, &h.to_string());
        point(w, 11, Pt2::new(t.x, t.y));
    }
}

// ─── helpers ──────────────────────────────────────────────────────────────

fn pair(w: &mut String, code: u32, value: &str) {
    let _ = write!(w, "{code}\n{value}\n");
}

fn num(w: &mut String, code: u32, value: f64) {
    pair(w, code, &four(value));
}

/// X, Y, Z group codes starting at `code` (10 → 10/20/30, 11 → 11/21/31).
fn point(w: &mut String, code: u32, p: Pt2) {
    num(w, code, p.x);
    num(w, code + 10, p.y);
    num(w, code + 20, 0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_spells_out_sheet_symbols_and_drops_accents() {
        assert_eq!(ascii("1200 × 600 mm² – §4.2 · 45°"), "1200 x 600 mm2 - S4.2 - 45deg");
        assert_eq!(ascii("Café “Zoë’s”\tNo.1"), "Cafe \"Zoe's\" No.1");
        assert_eq!(ascii("½ 道 😀"), "? ? ??");
        assert_eq!(ascii("plain ~ text"), "plain ~ text");
    }
}
