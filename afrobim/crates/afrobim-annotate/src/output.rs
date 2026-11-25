// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Annotations in DXF export and on composed sheets.
//!
//! **DXF.** R12 has TEXT but no MTEXT, no dimension entity this crate could write faithfully and
//! no wipeout, so every annotation is exploded: strokes become LINE, ARC and POLYLINE, arrow
//! heads SOLID, dots a closed wide polyline, and each line of text one TEXT entity (rotation,
//! left, centre or right alignment on the baseline, width factor). Background masks are not
//! exported. A dimension is therefore no longer associative in the DXF file; that is a limit of
//! R12, not a choice. No DXF written here has been opened in AutoCAD; ezdxf and this crate's own
//! reader read it back in the tests.
//!
//! **Sheets.** [`sheet_drawing`] returns the annotation as an `afrobim_sheets::SheetDrawing`, the
//! form `afrobim-sheets` previews: primitives in paper millimetres plus labels. Text is a label
//! (the draw crate has no text primitive); no stroke-font preview is included.

use afrobim_draw::{Drawing, Extents, Pt2};
use afrobim_dxf::{DxfDocument, DxfText, TextAlign};
use afrobim_kernel::{BBox, Document, ElementId};
use afrobim_sheets::{aci_for_rgb, dxf_layer_name, Align, SheetDrawing};

use crate::element::{geometry_absolute, is_annotation};
use crate::geometry::*;
use crate::settings;

/// One annotation's geometry in model space.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnItem {
    pub id: ElementId,
    pub layer: String,
    pub geometry: AnnGeometry,
}

/// The geometry of every annotation on a visible layer (an annotation that cannot be drawn is
/// left out and named in the second value).
pub fn annotation_items(doc: &Document) -> (Vec<AnnItem>, Vec<String>) {
    let mut items = Vec::new();
    let mut problems = Vec::new();
    for e in doc.elements().iter().filter(|e| is_annotation(e)) {
        if !doc.layer_visible(&e.layer) {
            continue;
        }
        match geometry_absolute(e) {
            Ok(geometry) => items.push(AnnItem { id: e.id, layer: e.layer.clone(), geometry }),
            Err(err) => problems.push(format!("{}: {err}", e.name)),
        }
    }
    (items, problems)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportReport {
    pub annotations: usize,
    pub text_lines: usize,
    pub warnings: Vec<String>,
}

/// Writes every annotation on a visible layer into `dxf`, in model millimetres.
pub fn export_dxf(doc: &Document, dxf: &mut DxfDocument) -> ExportReport {
    let s = settings::load(doc);
    let (items, problems) = annotation_items(doc);
    let mut r = ExportReport { warnings: problems, ..Default::default() };
    let mut masks = false;
    for it in &items {
        let layer = dxf_layer_name(&it.layer);
        let aci = s.layers.get(&it.layer).map(|p| aci_for_rgb(p.colour)).unwrap_or(7);
        r.annotations += 1;
        for seg in &it.geometry.segs {
            match seg {
                Seg::Line { a, b } => dxf.add_line(&layer, aci, *a, *b),
                Seg::Arc { centre, radius, start_deg, end_deg } => dxf.add_arc(&layer, aci, *centre, *radius, *start_deg, *end_deg),
                Seg::Poly { pts, closed } => dxf.add_polyline(&layer, aci, pts, *closed, 0.0),
            }
        }
        for f in &it.geometry.fills {
            match f.kind {
                FillKind::Arrow => dxf.add_solid(&layer, aci, &f.pts),
                FillKind::Dot => {
                    let n = f.pts.len() as f64;
                    let c = pt(f.pts.iter().map(|p| p.x).sum::<f64>() / n, f.pts.iter().map(|p| p.y).sum::<f64>() / n);
                    let radius = dist(c, f.pts[0]);
                    // A circle of half the radius drawn with a width of the full radius covers the disc.
                    let ring: Vec<Pt2> = f.pts.iter().map(|p| mid(c, *p)).collect();
                    dxf.add_polyline(&layer, aci, &ring, true, radius);
                }
                FillKind::Mask => masks = true,
            }
        }
        for l in &it.geometry.labels {
            r.text_lines += 1;
            dxf.add_text_scaled(
                &DxfText {
                    text: l.text.clone(),
                    at: l.at,
                    height: l.height,
                    rotation_deg: l.rotation_deg,
                    align: match l.align {
                        Align::Left => TextAlign::Left,
                        Align::Centre => TextAlign::Centre,
                        Align::Right => TextAlign::Right,
                    },
                    layer: layer.clone(),
                    aci,
                },
                l.width_factor,
            );
        }
    }
    if masks {
        r.warnings.push("Background masks are not written: DXF R12 has no wipeout.".into());
    }
    r
}

/// The annotations as one [`SheetDrawing`] in paper millimetres: each model point `p` becomes
/// `(p + offset) * scale`. Pass the plan view's translation as `offset` and `1 / denominator` as
/// `scale`. With `window`, only annotations whose bounds touch that model-space box are included.
pub fn sheet_drawing(doc: &Document, offset: Pt, scale: f64, window: Option<&BBox>) -> SheetDrawing {
    let (items, _) = annotation_items(doc);
    let mut out = SheetDrawing { drawing: Drawing { primitives: Vec::new(), extents: Extents::default() }, labels: Vec::new() };
    let mut first = true;
    for it in items {
        if let Some(w) = window {
            if !doc.find(it.id).is_some_and(|e| w.intersects(&e.bounds())) {
                continue;
            }
        }
        let s = it.geometry.to_sheet(offset, scale, &it.layer);
        out.drawing.primitives.extend(s.drawing.primitives);
        out.labels.extend(s.labels);
        let (lo, hi) = (s.drawing.extents.min, s.drawing.extents.max);
        if first {
            out.drawing.extents = Extents { min: lo, max: hi };
            first = false;
        } else {
            let e = &mut out.drawing.extents;
            e.min = pt(e.min.x.min(lo.x), e.min.y.min(lo.y));
            e.max = pt(e.max.x.max(hi.x), e.max.y.max(hi.y));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dim::{DimData, DimKind, LinearOrient};
    use crate::element::{add_dimension, add_text};
    use crate::styles::{DimStyle, TextStyle};
    use crate::text::TextData;

    fn doc_with_annotations() -> Document {
        let mut doc = Document::default();
        let st = DimStyle { text_height_mm: 10.0, arrow_size_mm: 10.0, ..DimStyle::default() };
        let d = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(5000.0, 0.0), line_pt: pt(0.0, 500.0), orient: LinearOrient::Horizontal }, &st);
        add_dimension(&mut doc, d, "DIMS").unwrap();
        let mut t = TextData::new("Existing kerb", pt(100.0, 900.0), &TextStyle::default(), 100.0);
        t.rotation_deg = 30.0;
        t.width_factor = 0.8;
        add_text(&mut doc, t, "NOTES").unwrap();
        doc
    }

    #[test]
    fn dxf_has_text_lines_solids_and_layers() {
        let doc = doc_with_annotations();
        let mut dxf = DxfDocument::new();
        let r = export_dxf(&doc, &mut dxf);
        assert_eq!((r.annotations, r.text_lines), (2, 2));
        let text = dxf.finish();
        assert!(text.contains("0\nSOLID\n8\nDIMS\n"));
        assert!(text.contains("0\nTEXT\n8\nDIMS\n"));
        assert!(text.contains("1\n5.000\n"));
        assert!(text.contains("1\nExisting kerb\n"));
        assert!(text.contains("0\nLAYER\n2\nNOTES\n"));
        // The rotated, narrowed text carries its rotation (50) and width factor (41).
        assert!(text.contains("50\n30\n41\n0.8\n"));
    }

    #[test]
    fn hidden_layers_are_not_exported_and_masks_are_reported() {
        let mut doc = doc_with_annotations();
        crate::layers::set_visible(&mut doc, "DIMS", false).unwrap();
        let mut dxf = DxfDocument::new();
        assert_eq!(export_dxf(&doc, &mut dxf).annotations, 1);
        let mut t = TextData::new("MASKED", pt(0.0, 0.0), &TextStyle::default(), 1.0);
        t.mask_offset = Some(0.3);
        add_text(&mut doc, t, "NOTES").unwrap();
        let mut dxf = DxfDocument::new();
        assert!(export_dxf(&doc, &mut dxf).warnings.iter().any(|w| w.contains("masks")));
    }

    #[test]
    fn sheet_form_scales_geometry_and_text_height() {
        let doc = doc_with_annotations();
        let s = sheet_drawing(&doc, pt(0.0, 0.0), 0.01, None);
        // The kerb text: 2.5 mm paper at 1:100 is 250 mm in the model, 2.5 mm again on a 1:100 sheet.
        let l = s.labels.iter().find(|l| l.text == "Existing kerb").unwrap();
        assert!((l.height_mm - 2.5).abs() < 1e-9);
        assert_eq!(l.layer, "NOTES");
        assert!((l.at.x - 1.0).abs() < 1e-9 && (l.at.y - 9.0).abs() < 1e-9);
        assert!(s.drawing.extents.max.x >= 50.0 - 1e-6); // the 5 m dimension is 50 mm at 1:100
        // A window that misses everything leaves nothing.
        let none = sheet_drawing(&doc, pt(0.0, 0.0), 0.01, Some(&BBox::new(1e6, 1e6, 2e6, 2e6)));
        assert!(none.labels.is_empty() && none.drawing.primitives.is_empty());
    }
}
