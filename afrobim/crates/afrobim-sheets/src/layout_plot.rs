// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Plotting a layout (paper space): the layout's paper, frame, title block, paper shapes and
//! viewports composed into a [`ComposedSheet`] at true paper size, so the existing PDF and DXF
//! writers print it with the viewport scale exact (1:500 is 2 mm per metre).
//!
//! The model is in millimetres. A viewport at 1:`n` puts a model length `L` mm at `L / n` mm on
//! the paper; the factor is taken from [`Scale`] (paper mm per model metre, divided by 1000), the
//! same type every other sheet uses. Viewport boundaries are not plotted (as with a non-plotting
//! viewport layer in CAD); the clip polygon only decides what is cut. The title and scale label
//! of a viewport is plotted when it asks for one.

use afrobim_draw::Pt2;
use afrobim_kernel::layout::{point_in_polygon, Layout, Viewport};
use afrobim_kernel::Element;

use crate::draw::draw_drawing;
use crate::geom::{Aff, Rect};
use crate::item::{layers, text as mk_text, ComposedSheet, Item};
use crate::model::Model;
use crate::paper::{Orientation, Page, PaperSize, Scale};
use crate::project::ProjectInfo;
use crate::style::Styles;
use crate::text::Align;
use crate::titleblock::{draw_title_block, SheetFields, TitleBlockLayout, TitleValues};

/// The layer paper shapes are plotted on.
pub const SHAPES_LAYER: &str = "LAYOUT-SHAPES";

/// The scale of a viewport as the sheets crate's [`Scale`].
pub fn viewport_scale(v: &Viewport) -> Scale {
    Scale(v.scale_den)
}

/// Paper millimetres per model millimetre of a viewport, taken from [`Scale`].
pub fn paper_per_model(v: &Viewport) -> f64 {
    viewport_scale(v).paper_mm_per_m() / 1000.0
}

/// A new, empty layout of an ISO paper size with the default title block (70 mm column on the
/// right of the frame) when `title_block` is set. Sizes come from [`PaperSize::dimensions_mm`].
pub fn layout_from_template(name: &str, size: PaperSize, orientation: Orientation, title_block: bool) -> Layout {
    let page = Page::new(size, orientation);
    let (w, h) = page.dimensions_mm();
    let mut l = Layout::new(name, &size.name(), orientation == Orientation::Landscape, w, h);
    let m = page.margins;
    l.margins = [m.left, m.right, m.top, m.bottom];
    if title_block {
        l.title_block_mm = TitleBlockLayout::default().width_mm;
    }
    l
}

/// Sets the paper of a layout to an ISO size and orientation (page setup), keeping the viewports.
pub fn set_paper(layout: &mut Layout, size: PaperSize, orientation: Orientation) {
    let (w, h) = Page::new(size, orientation).dimensions_mm();
    layout.size = size.name();
    layout.landscape = orientation == Orientation::Landscape;
    layout.width_mm = w;
    layout.height_mm = h;
}

/// The ISO size and orientation of a layout, when its size name is one of A0 to A4.
pub fn paper_of(layout: &Layout) -> Option<(PaperSize, Orientation)> {
    let size = PaperSize::ALL.into_iter().find(|s| s.name() == layout.size)?;
    Some((size, if layout.landscape { Orientation::Landscape } else { Orientation::Portrait }))
}

/// Extra content drawn into a viewport, in paper millimetres, after the elements: the app adds
/// what the sheets crate cannot know (annotation). It must draw within the given rectangle.
pub type Extras<'a> = &'a mut dyn FnMut(&Viewport, &Rect, &mut ComposedSheet);

/// Composes one layout. `position` is its 1-based place among `total` layouts being plotted.
pub fn compose_layout(model: &Model, styles: &Styles, info: &ProjectInfo, layout: &Layout, position: usize, total: usize, extras: Extras) -> ComposedSheet {
    let mut out = ComposedSheet::new(&layout.name, &layout.name, layout.width_mm, layout.height_mm);
    let fr = layout.printable_rect();
    let frame = Rect::new(fr[0], fr[1], fr[2], fr[3]);
    let frame_pen = styles.pens.pen(layers::FRAME);
    out.rect(layers::FRAME, &frame, frame_pen.weight_mm, frame_pen.rgb);

    // Paper shapes (what the person drew on the layout).
    let sp = styles.pens.pen(SHAPES_LAYER);
    for s in &layout.shapes {
        out.stroke(SHAPES_LAYER, s.points(), false, sp.weight_mm, sp.rgb);
    }

    for v in layout.viewports.iter().filter(|v| v.on) {
        compose_viewport(&mut out, model, styles, v, extras);
    }

    // Title block, over the viewports so nothing hides it.
    if layout.title_block_mm > 0.0 {
        let tb = TitleBlockLayout::default();
        let x = frame.x1 - tb.width_mm.min(frame.width());
        out.line(layers::FRAME, Pt2::new(x, frame.y0), Pt2::new(x, frame.y1), frame_pen.weight_mm, frame_pen.rgb);
        let scale_text = match layout.viewports.iter().find(|v| v.on) {
            Some(first) if layout.viewports.iter().filter(|v| v.on).all(|v| v.scale_den == first.scale_den) => viewport_scale(first).label(),
            Some(_) => "AS SHOWN".to_string(),
            None => String::new(),
        };
        let fields = SheetFields::default();
        let values = TitleValues { info, fields: &fields, number: &layout.name, title: &layout.name, scale_text: &scale_text, sheet: (position, total) };
        draw_title_block(&mut out, &tb, &tb.rect_in(&frame), &values, styles);
    }
    out
}

fn compose_viewport(out: &mut ComposedSheet, model: &Model, styles: &Styles, v: &Viewport, extras: Extras) {
    if !Viewport::is_valid_scale(v.scale_den) {
        out.warn(format!("A viewport has no valid scale ({}).", v.scale_den));
        return;
    }
    let rect = Rect::new(v.rect[0], v.rect[1], v.rect[2], v.rect[3]);
    let k = paper_per_model(v);
    let pc = v.paper_centre();
    let mut tmp = ComposedSheet::new(&out.number, &out.title, out.width_mm, out.height_mm);
    let doc = model.doc;
    for e in doc.elements() {
        if !element_shows_in(doc, e, v) {
            continue;
        }
        if model.registry.get(&e.category).is_none() {
            continue;
        }
        match model.registry.generate(e) {
            Ok(g) => {
                // Local mm -> model offset from the view centre (taken first, so a far-away
                // model keeps its precision) -> paper mm.
                let m = Aff::translate(e.origin.x - v.centre.x, e.origin.y - v.centre.y).then(&Aff::scale_rotate(k, 0.0)).then(&Aff::translate(pc.x, pc.y));
                draw_drawing(&mut tmp, styles, &g.drawing, &m, &rect, &e.layer);
            }
            Err(err) => out.warn(format!("Element '{}' is not drawn: {err}", e.name)),
        }
    }
    extras(v, &rect, &mut tmp);
    for w in std::mem::take(&mut tmp.warnings) {
        out.warn(w);
    }
    match &v.clip {
        Some(poly) if poly.len() >= 3 => merge_clipped(out, tmp, poly),
        _ => out.items.extend(tmp.items),
    }

    if v.show_label {
        let pen = styles.pens.pen(layers::TEXT);
        let x = pc.x;
        if !v.title.trim().is_empty() {
            out.text(mk_text(layers::TEXT, Pt2::new(x, rect.y0 - 6.0), 3.5, v.title.clone(), Align::Centre, 0.0, true, pen.rgb));
        }
        out.text(mk_text(layers::TEXT, Pt2::new(x, rect.y0 - 10.0), 2.5, viewport_scale(v).label(), Align::Centre, 0.0, false, pen.rgb));
    }
}

/// True when the element is drawn in the viewport: its layer is on in the document and not
/// frozen in the viewport.
pub fn element_shows_in(doc: &afrobim_kernel::Document, e: &Element, v: &Viewport) -> bool {
    doc.layer_visible(&e.layer) && !v.is_frozen(&e.layer)
}

/// Moves what `tmp` holds into `out`, cut to the polygon.
fn merge_clipped(out: &mut ComposedSheet, tmp: ComposedSheet, poly: &[Pt2]) {
    let convex = is_convex(poly);
    let mut approximate = false;
    for item in tmp.items {
        match item {
            Item::Stroke { layer, points, closed, weight_mm, rgb } => {
                for piece in clip_polyline_to_polygon(&points, closed, poly) {
                    out.stroke(&layer, piece, false, weight_mm, rgb);
                }
            }
            Item::Fill { layer, rings, rgb } => {
                let rings: Vec<Vec<Pt2>> = if convex {
                    rings.iter().map(|r| clip_ring_convex(r, poly)).filter(|r| r.len() >= 3).collect()
                } else {
                    // A concave boundary cannot clip a ring by the simple rule: keep a ring that
                    // is wholly inside, drop one wholly outside, and keep (and say so) a partial one.
                    rings
                        .into_iter()
                        .filter(|r| {
                            let inside = r.iter().filter(|p| point_in_polygon(poly, **p)).count();
                            if inside > 0 && inside < r.len() {
                                approximate = true;
                            }
                            inside > 0
                        })
                        .collect()
                };
                out.fill(&layer, rings, rgb);
            }
            Item::Text(t) => {
                if point_in_polygon(poly, t.at) {
                    out.text(t);
                }
            }
        }
    }
    if approximate {
        out.warn("A filled area crosses a concave viewport boundary and is not cut exactly to it.");
    }
}

fn is_convex(poly: &[Pt2]) -> bool {
    let n = poly.len();
    let mut sign = 0.0f64;
    for i in 0..n {
        let (a, b, c) = (poly[i], poly[(i + 1) % n], poly[(i + 2) % n]);
        let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
        if cross.abs() < 1e-12 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

/// Sutherland-Hodgman against a convex polygon of either winding.
fn clip_ring_convex(ring: &[Pt2], poly: &[Pt2]) -> Vec<Pt2> {
    let area: f64 = (0..poly.len()).map(|i| poly[i].x * poly[(i + 1) % poly.len()].y - poly[(i + 1) % poly.len()].x * poly[i].y).sum();
    let ccw = area >= 0.0;
    let mut out: Vec<Pt2> = ring.to_vec();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let side = |p: Pt2| {
            let c = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
            if ccw {
                c
            } else {
                -c
            }
        };
        let input = std::mem::take(&mut out);
        for j in 0..input.len() {
            let (p, q) = (input[j], input[(j + 1) % input.len()]);
            let (sp, sq) = (side(p), side(q));
            if sp >= 0.0 {
                out.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                let t = sp / (sp - sq);
                out.push(Pt2::new(p.x + t * (q.x - p.x), p.y + t * (q.y - p.y)));
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

/// The parts of a polyline inside a polygon.
pub fn clip_polyline_to_polygon(points: &[Pt2], closed: bool, poly: &[Pt2]) -> Vec<Vec<Pt2>> {
    let n = points.len();
    if n < 2 || poly.len() < 3 {
        return Vec::new();
    }
    let segs = if closed { n } else { n - 1 };
    let mut runs: Vec<Vec<Pt2>> = Vec::new();
    for s in 0..segs {
        let (a, b) = (points[s], points[(s + 1) % n]);
        let mut ts = vec![0.0f64, 1.0];
        for i in 0..poly.len() {
            let (c, d) = (poly[i], poly[(i + 1) % poly.len()]);
            let (r, e) = ((b.x - a.x, b.y - a.y), (d.x - c.x, d.y - c.y));
            let den = r.0 * e.1 - r.1 * e.0;
            if den.abs() < 1e-14 {
                continue;
            }
            let t = ((c.x - a.x) * e.1 - (c.y - a.y) * e.0) / den;
            let u = ((c.x - a.x) * r.1 - (c.y - a.y) * r.0) / den;
            if t > 0.0 && t < 1.0 && (0.0..=1.0).contains(&u) {
                ts.push(t);
            }
        }
        ts.sort_by(f64::total_cmp);
        for w in ts.windows(2) {
            if w[1] - w[0] < 1e-12 {
                continue;
            }
            let at = |t: f64| Pt2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
            let (p, q) = (at(w[0]), at(w[1]));
            if point_in_polygon(poly, at((w[0] + w[1]) / 2.0)) {
                match runs.last_mut() {
                    Some(run) if run.last().is_some_and(|l| (l.x - p.x).abs() < 1e-9 && (l.y - p.y).abs() < 1e-9) => run.push(q),
                    _ => runs.push(vec![p, q]),
                }
            }
        }
    }
    runs
}

/// Composes every layout in order (page n of N for the title block).
pub fn compose_layouts(model: &Model, styles: &Styles, info: &ProjectInfo, layouts: &[&Layout], extras: Extras) -> Vec<ComposedSheet> {
    let total = layouts.len();
    layouts.iter().enumerate().map(|(i, l)| compose_layout(model, styles, info, l, i + 1, total, &mut *extras)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_draw::{Drawing, Extents, Primitive};
    use afrobim_kernel::{Document, Generated, Module, ModuleRegistry, Pt};
    use std::collections::BTreeMap;

    /// A module whose element is a 50 m long horizontal line (drawing units are model mm).
    struct Line50;
    impl Module for Line50 {
        fn category(&self) -> &str {
            "line50"
        }
        fn display_name(&self) -> &str {
            "Line"
        }
        fn generate(&self, _e: &Element) -> Result<Generated, String> {
            Ok(Generated {
                drawing: Drawing {
                    primitives: vec![Primitive::Line { from: Pt2::new(0.0, 0.0), to: Pt2::new(50_000.0, 0.0), width: 0.0, colour: None }],
                    extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(50_000.0, 0.0) },
                },
                notes: vec![],
                derived: BTreeMap::new(),
                solids: vec![],
            })
        }
    }

    fn model_doc() -> (Document, ModuleRegistry) {
        let mut doc = Document::default();
        doc.add("line50", "L1", Pt::new(1_000_000.0, 2_000_000.0), 50_000.0, 0.0, "Roads", serde_json::Value::Null);
        let mut reg = ModuleRegistry::default();
        reg.register(Box::new(Line50));
        (doc, reg)
    }

    fn compose(doc: &Document, reg: &ModuleRegistry, l: &Layout) -> ComposedSheet {
        let m = Model { doc, registry: reg };
        compose_layout(&m, &Styles::default(), &ProjectInfo::default(), l, 1, 1, &mut |_, _, _| {})
    }

    #[test]
    fn viewport_scale_is_the_sheets_scale_at_every_standard_scale() {
        for den in afrobim_kernel::STANDARD_SCALES {
            let v = Viewport::new(1, Pt::new(0.0, 0.0), Pt::new(100.0, 100.0), Pt::new(0.0, 0.0), den);
            assert_eq!(viewport_scale(&v).paper_mm_per_m(), v.paper_mm_per_model_m(), "1:{den}");
            assert!((paper_per_model(&v) - v.paper_per_model()).abs() < 1e-15);
            assert_eq!(viewport_scale(&v).label(), v.scale_label());
        }
        let v = Viewport::new(1, Pt::new(0.0, 0.0), Pt::new(100.0, 60.0), Pt::new(0.0, 0.0), 500.0);
        assert_eq!(viewport_scale(&v).paper_mm_per_m(), 2.0, "1:500 is 2 mm per metre");
    }

    #[test]
    fn a_line_of_50_m_is_100_mm_on_paper_at_1_to_500() {
        let (doc, reg) = model_doc();
        let mut l = layout_from_template("P1", PaperSize::A3, Orientation::Landscape, false);
        // Show the model point 25 m along the line at the middle of a 200 by 100 mm viewport.
        let id = l.add_viewport(Pt::new(50.0, 50.0), Pt::new(250.0, 150.0), Pt::new(1_025_000.0, 2_000_000.0), 500.0).unwrap();
        let sheet = compose(&doc, &reg, &l);
        let line = sheet.items.iter().find_map(|i| match i {
            Item::Stroke { layer, points, .. } if layer == "Roads" => Some(points.clone()),
            _ => None,
        });
        let pts = line.expect("the model line is on the sheet");
        assert_eq!(pts.len(), 2);
        assert!((pts[1].x - pts[0].x - 100.0).abs() < 1e-9, "50 m at 1:500 is 100 mm, got {}", pts[1].x - pts[0].x);
        assert!((pts[0].x - 100.0).abs() < 1e-9 && (pts[1].x - 200.0).abs() < 1e-9, "centred on the view centre: {pts:?}");
        assert!((pts[0].y - 100.0).abs() < 1e-9);
        // Frozen in the viewport: the layer disappears from it.
        l.viewport_mut(id).unwrap().freeze("Roads");
        assert!(!compose(&doc, &reg, &l).items.iter().any(|i| i.layer() == "Roads"));
        // Display off: nothing at all from the viewport.
        let v = l.viewport_mut(id).unwrap();
        v.thaw("Roads");
        v.on = false;
        assert!(!compose(&doc, &reg, &l).items.iter().any(|i| i.layer() == "Roads"));
    }

    #[test]
    fn a_polygon_clip_cuts_the_model_to_the_boundary() {
        let (doc, reg) = model_doc();
        let mut l = layout_from_template("P1", PaperSize::A3, Orientation::Landscape, false);
        let id = l.add_viewport(Pt::new(50.0, 50.0), Pt::new(250.0, 150.0), Pt::new(1_025_000.0, 2_000_000.0), 500.0).unwrap();
        // A triangle that covers the left half of the line only: the line runs from x = 100 to 200 at y = 100.
        assert!(l.viewport_mut(id).unwrap().set_clip(vec![Pt::new(60.0, 60.0), Pt::new(150.0, 60.0), Pt::new(150.0, 140.0), Pt::new(60.0, 140.0)]));
        let sheet = compose(&doc, &reg, &l);
        let pts = sheet.items.iter().find_map(|i| match i {
            Item::Stroke { layer, points, .. } if layer == "Roads" => Some(points.clone()),
            _ => None,
        });
        let pts = pts.expect("part of the line remains");
        assert!((pts[0].x - 100.0).abs() < 1e-9 && (pts[1].x - 150.0).abs() < 1e-9, "{pts:?}");
    }

    #[test]
    fn polyline_and_ring_clipping_against_a_polygon() {
        let tri = [Pt2::new(0.0, 0.0), Pt2::new(10.0, 0.0), Pt2::new(0.0, 10.0)];
        let runs = clip_polyline_to_polygon(&[Pt2::new(-5.0, 2.0), Pt2::new(20.0, 2.0)], false, &tri);
        assert_eq!(runs.len(), 1);
        assert!((runs[0][0].x - 0.0).abs() < 1e-9 && (runs[0][1].x - 8.0).abs() < 1e-9, "{runs:?}");
        assert!(clip_polyline_to_polygon(&[Pt2::new(8.0, 8.0), Pt2::new(9.0, 9.0)], false, &tri).is_empty());
        let ring = [Pt2::new(-5.0, -5.0), Pt2::new(15.0, -5.0), Pt2::new(15.0, 15.0), Pt2::new(-5.0, 15.0)];
        let cut = clip_ring_convex(&ring, &tri);
        // The triangle lies wholly inside the ring, so the cut is the triangle.
        let area: f64 = (0..cut.len()).map(|i| cut[i].x * cut[(i + 1) % cut.len()].y - cut[(i + 1) % cut.len()].x * cut[i].y).sum::<f64>().abs() / 2.0;
        assert!((area - 50.0).abs() < 1e-9, "{area}");
        assert!(is_convex(&tri) && !is_convex(&[Pt2::new(0.0, 0.0), Pt2::new(10.0, 0.0), Pt2::new(10.0, 10.0), Pt2::new(5.0, 2.0), Pt2::new(0.0, 10.0)]));
    }

    #[test]
    fn pdf_of_a_layout_has_the_page_size_of_the_paper() {
        let (doc, reg) = model_doc();
        for (size, o, w, h) in [(PaperSize::A3, Orientation::Landscape, 420.0, 297.0), (PaperSize::A1, Orientation::Portrait, 594.0, 841.0), (PaperSize::A4, Orientation::Landscape, 297.0, 210.0)] {
            let mut l = layout_from_template("Plot", size, o, true);
            assert_eq!((l.width_mm, l.height_mm), (w, h));
            let area = l.view_area();
            l.add_viewport(Pt::new(area[0], area[1]), Pt::new(area[2], area[3]), Pt::new(1_025_000.0, 2_000_000.0), 1000.0).unwrap();
            let sheet = compose(&doc, &reg, &l);
            assert_eq!((sheet.width_mm, sheet.height_mm), (w, h));
            let pdf = crate::pdf::write_pdf(&[sheet], "t");
            let text = String::from_utf8_lossy(&pdf);
            let mb = format!("/MediaBox [0 0 {} {}]", fmt_pt(w), fmt_pt(h));
            assert!(text.contains(&mb), "{mb} not in the PDF");
            assert!(text.starts_with("%PDF-1.4") && text.contains("/Count 1"));
        }
        // One page per layout in a multi-page PDF.
        let m = Model { doc: &doc, registry: &reg };
        let a = layout_from_template("A", PaperSize::A3, Orientation::Landscape, true);
        let b = layout_from_template("B", PaperSize::A4, Orientation::Portrait, true);
        let sheets = compose_layouts(&m, &Styles::default(), &ProjectInfo::default(), &[&a, &b], &mut |_, _, _| {});
        let text = String::from_utf8_lossy(&crate::pdf::write_pdf(&sheets, "all")).into_owned();
        assert!(text.contains("/Count 2") && text.contains(&format!("/MediaBox [0 0 {} {}]", fmt_pt(210.0), fmt_pt(297.0))));
        // And a DXF of a layout is written in paper millimetres.
        let dxf = crate::dxf_out::write_dxf(&sheets[0], &Styles::default().pens);
        assert!(dxf.contains("SHEET-FRAME") && dxf.contains("420"));
    }

    fn fmt_pt(mm: f64) -> String {
        let s = format!("{:.2}", mm * 72.0 / 25.4);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }

    #[test]
    fn a_labelled_viewport_prints_its_title_and_scale_and_the_title_block_shows_the_scale() {
        let (doc, reg) = model_doc();
        let mut l = layout_from_template("Sheet 1", PaperSize::A3, Orientation::Landscape, true);
        let id = l.add_fit_viewport(Pt::new(1_025_000.0, 2_000_000.0), 2500.0).unwrap();
        let v = l.viewport_mut(id).unwrap();
        v.title = "Plan".into();
        v.show_label = true;
        let sheet = compose(&doc, &reg, &l);
        let texts: Vec<&str> = sheet.texts().map(|t| t.text.as_str()).collect();
        assert!(texts.contains(&"Plan") && texts.iter().filter(|t| **t == "1:2500").count() >= 2, "label and title block scale: {texts:?}");
        assert!(texts.contains(&"Sheet 1"));
    }

    #[test]
    fn templates_come_from_the_iso_derivation() {
        let l = layout_from_template("x", PaperSize::A0, Orientation::Landscape, true);
        assert_eq!((l.width_mm, l.height_mm, l.size.as_str()), (1189.0, 841.0, "A0"));
        assert_eq!(l.title_block_mm, 70.0);
        assert_eq!(paper_of(&l), Some((PaperSize::A0, Orientation::Landscape)));
        let mut p = layout_from_template("y", PaperSize::A4, Orientation::Portrait, false);
        assert_eq!((p.width_mm, p.height_mm), (210.0, 297.0));
        set_paper(&mut p, PaperSize::A2, Orientation::Landscape);
        assert_eq!((p.width_mm, p.height_mm, p.size.as_str()), (594.0, 420.0, "A2"));
    }
}
