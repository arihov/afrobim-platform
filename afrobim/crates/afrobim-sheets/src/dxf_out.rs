// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! DXF output in paper millimetres: one drawing unit is one millimetre on the printed sheet.
//!
//! The file is R12 (AC1009), as written by `afrobim-dxf`. Each composed sheet has its lower left
//! paper corner on the origin; a set is laid side by side along X with a gap. Limits of R12:
//! there are no lineweights, so a pen width is carried as polyline width; there are no hatches,
//! so a filled region is written as its outline; there is no MTEXT, so every text is one TEXT
//! entity; and R12 has no layouts, so the "paper space" is the drawing space itself at 1:1.

use afrobim_draw::Pt2;
use afrobim_dxf::{DxfDocument, DxfText, TextAlign};

use crate::item::{layers, ComposedSheet, Item};
use crate::style::PenTable;
use crate::text::Align;

/// The AutoCAD Colour Index for an RGB colour. A clearly coloured one (channels differing by at
/// least 60) takes the standard colour of nearest hue (1 red, 2 yellow, 3 green, 4 cyan, 5 blue,
/// 6 magenta), however dark; a grey takes 8 (mid grey) or 9 (light grey), and black and white
/// both come out as 7, which a CAD system shows black on white.
pub fn aci_for_rgb(rgb: [u8; 3]) -> u8 {
    let [r, g, b] = rgb.map(|c| c as f64);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    if max - min >= 60.0 {
        let d = max - min;
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } * 60.0;
        return [1, 2, 3, 4, 5, 6][((h / 60.0).round() as usize) % 6];
    }
    let lum = 0.299 * r + 0.587 * g + 0.114 * b;
    if !(60.0..=225.0).contains(&lum) {
        7
    } else if lum < 160.0 {
        8
    } else {
        9
    }
}

/// A layer name DXF accepts: upper case letters, digits, `-`, `_` and `$`; anything else becomes `_`.
pub fn dxf_layer_name(name: &str) -> String {
    let s: String = name.to_uppercase().chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '$') { c } else { '_' }).collect();
    if s.is_empty() {
        "0".to_string()
    } else {
        s
    }
}

/// How many rasters the sheets hold that a DXF cannot carry (map tiles, images). The writer
/// leaves them out; the caller says so.
pub fn skipped_rasters(sheets: &[ComposedSheet]) -> usize {
    sheets.iter().map(ComposedSheet::image_count).sum()
}

/// One sheet as DXF text.
pub fn write_dxf(sheet: &ComposedSheet, pens: &PenTable) -> String {
    write_dxf_set(std::slice::from_ref(sheet), pens, 0.0)
}

/// Several sheets in one DXF, side by side along X with `gap_mm` between them. The paper edge of
/// each is drawn on `SHEET-PAPER`.
pub fn write_dxf_set(sheets: &[ComposedSheet], pens: &PenTable, gap_mm: f64) -> String {
    let mut doc = DxfDocument::new();
    let mut x0 = 0.0;
    for sheet in sheets {
        let at = |p: &Pt2| Pt2::new(p.x + x0, p.y);
        let paper = [Pt2::new(0.0, 0.0), Pt2::new(sheet.width_mm, 0.0), Pt2::new(sheet.width_mm, sheet.height_mm), Pt2::new(0.0, sheet.height_mm)];
        let paper_pen = pens.pen(layers::PAPER);
        doc.add_polyline(&dxf_layer_name(layers::PAPER), aci_for_rgb(paper_pen.rgb), &paper.iter().map(at).collect::<Vec<_>>(), true, 0.0);
        for item in &sheet.items {
            match item {
                Item::Stroke { layer, points, closed, weight_mm, rgb } => {
                    doc.add_polyline(&dxf_layer_name(layer), aci_for_rgb(*rgb), &points.iter().map(at).collect::<Vec<_>>(), *closed, *weight_mm);
                }
                Item::Fill { layer, rings, rgb } => {
                    for ring in rings.iter().filter(|r| r.len() >= 3) {
                        doc.add_polyline(&dxf_layer_name(layer), aci_for_rgb(*rgb), &ring.iter().map(at).collect::<Vec<_>>(), true, 0.0);
                    }
                }
                // DXF R12 has no raster: images are left out (see `skipped_rasters`).
                Item::Image(_) => {}
                Item::Text(t) => doc.add_text(&DxfText {
                    text: t.text.clone(),
                    at: at(&t.at),
                    height: t.height_mm,
                    rotation_deg: t.rotation_deg,
                    align: match t.align {
                        Align::Left => TextAlign::Left,
                        Align::Centre => TextAlign::Centre,
                        Align::Right => TextAlign::Right,
                    },
                    layer: dxf_layer_name(&t.layer),
                    aci: aci_for_rgb(t.rgb),
                }),
            }
        }
        x0 += sheet.width_mm + gap_mm;
    }
    doc.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_snap_to_the_nearest_index_and_black_and_white_are_seven() {
        assert_eq!(aci_for_rgb([0, 0, 0]), 7);
        assert_eq!(aci_for_rgb([255, 255, 255]), 7);
        assert_eq!(aci_for_rgb([200, 10, 10]), 1);
        assert_eq!(aci_for_rgb([160, 0, 0]), 1);
        assert_eq!(aci_for_rgb([0, 100, 40]), 3); // dark green keeps its hue
        assert_eq!(aci_for_rgb([0, 79, 158]), 5);
        assert_eq!(aci_for_rgb([130, 130, 130]), 8);
    }

    #[test]
    fn layer_names_are_made_safe() {
        assert_eq!(dxf_layer_name("Markings & lines"), "MARKINGS___LINES");
        assert_eq!(dxf_layer_name("plan-alignment"), "PLAN-ALIGNMENT");
        assert_eq!(dxf_layer_name(""), "0");
    }
}
