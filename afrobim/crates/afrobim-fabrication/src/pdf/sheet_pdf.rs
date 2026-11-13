//! Draws a [`FabricationSheet`] as a one-page A3 landscape PDF. Port of C# `SheetPdfWriter`.
//!
//! The geometry is the sheet the DXF carries; in addition the sign face is filled in its print
//! colours, drawn in layout order so later shapes (panels, lettering) sit on earlier ones (the
//! ground).

use afrobim_draw::{Primitive, Pt2};
use afrobim_signs::Colour;

use crate::composer::DIM_LAYER;
use crate::palette::print_rgb;
use crate::sheet::{FabricationSheet, SheetBox};

use super::document::Document;
use super::gfx::{Pen, Rgb, BLACK};
use super::{Canvas, PdfFonts, PdfStamp};

const DIM_COLOUR: Rgb = [0xB0, 0x10, 0x20];
const DARK_CYAN: Rgb = [0, 139, 139];
const GREY: Rgb = [128, 128, 128];

/// The sheet as PDF bytes, stamped with the current time.
pub fn write_sheet_pdf(sheet: &FabricationSheet, fonts: &PdfFonts) -> Vec<u8> {
    write_sheet_pdf_stamped(sheet, fonts, &PdfStamp::now())
}

/// The sheet as PDF bytes with a given stamp; the same stamp gives the same bytes.
pub fn write_sheet_pdf_stamped(sheet: &FabricationSheet, fonts: &PdfFonts, stamp: &PdfStamp) -> Vec<u8> {
    let mut doc = Document::new(fonts, format!("{} fabrication sheet", sheet.sign.code));
    doc.page(FabricationSheet::WIDTH_MM, FabricationSheet::HEIGHT_MM, |g| {
        let mut pdf = Canvas::new(g, FabricationSheet::HEIGHT_MM);
        draw_face(&mut pdf, sheet);
        for s in &sheet.swatches {
            pdf.fill_rect(&s.rect, print_rgb(s.colour));
            pdf.stroke_rect(&s.rect, &Pen { colour: BLACK, width: 0.15 });
        }
        let ink = |layer: &str| if layer == DIM_LAYER { DIM_COLOUR } else { BLACK };
        for l in &sheet.lines {
            pdf.line(l.x1, l.y1, l.x2, l.y2, &Pen { colour: ink(l.layer), width: l.weight });
        }
        for t in &sheet.texts {
            pdf.text(t, ink(t.layer));
        }
    });
    doc.finish(stamp)
}

fn draw_face(pdf: &mut Canvas, sheet: &FabricationSheet) {
    let k = sheet.sheet_mm_per_sw();
    let tx = |p: &Pt2| {
        let s = sheet.to_sheet(*p);
        (s.x, s.y)
    };
    for prim in &sheet.face.primitives {
        match prim {
            Primitive::Fill { outline, colour } => {
                if outline.len() >= 3 {
                    pdf.fill_polygons(&[outline.iter().map(tx).collect()], print_rgb(*colour));
                }
            }
            Primitive::Glyphs { contours, ink, .. } => {
                // One path for every contour of the legend, filled even-odd, so the counters of
                // letters such as 'a' and 'o' stay open.
                let polygons: Vec<Vec<(f64, f64)>> = contours.iter().filter(|c| c.len() >= 3).map(|c| c.iter().map(tx).collect()).collect();
                pdf.fill_polygons(&polygons, print_rgb(ink.unwrap_or(Colour::White)));
            }
            Primitive::Line { from, to, width, colour } => {
                let (a, b) = (tx(from), tx(to));
                let pen = Pen { colour: print_rgb(colour.unwrap_or(Colour::White)), width: f64::max(0.1, width * k) };
                pdf.line(a.0, a.1, b.0, b.1, &pen);
            }
            Primitive::Arc { centre, radius, start_deg, end_deg, colour } => {
                let pen = Pen { colour: print_rgb(colour.unwrap_or(Colour::White)), width: 0.25 };
                pdf.arc(tx(centre), sheet.length_to_sheet(*radius), *start_deg, *end_deg, &pen);
            }
            Primitive::Symbol { origin, size, .. } => {
                let (x, y) = tx(origin);
                let d = sheet.length_to_sheet(*size);
                let b = SheetBox::new(x, y, x + d, y + d);
                let pen = Pen { colour: DARK_CYAN, width: 0.2 };
                pdf.stroke_rect(&b, &pen);
                pdf.line(b.min_x, b.min_y, b.max_x, b.max_y, &pen);
                pdf.line(b.max_x, b.min_y, b.min_x, b.max_y, &pen);
            }
        }
    }
    // A hairline round the plate so a white sign still shows its edge on white paper.
    pdf.stroke_rect(&sheet.face_box, &Pen { colour: GREY, width: 0.1 });
}
