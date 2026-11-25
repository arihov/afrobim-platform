// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Putting an `afrobim_draw::Drawing` on a sheet through an affine map, clipped to a rectangle.

use afrobim_draw::{Colour, Drawing, Primitive, Pt2};

use crate::geom::{arc_points, clip_polygon, clip_polyline, Aff, Rect};
use crate::item::ComposedSheet;
use crate::style::Styles;

/// Largest distance, paper mm, between a drawn chord and the true arc.
const ARC_SAG_MM: f64 = 0.03;

/// Draws `drawing` on `sheet`. `m` takes drawing units to paper millimetres (it may rotate and
/// scale); everything is cut to `clip`. Hairlines and widths follow the pen of `layer`: a line of
/// width 0 is drawn at the pen weight, a wider one at its true width on paper.
pub fn draw_drawing(sheet: &mut ComposedSheet, styles: &Styles, drawing: &Drawing, m: &Aff, clip: &Rect, layer: &str) {
    let pen = styles.pens.pen(layer);
    let k = m.scale();
    let stroke_rgb = |c: Option<Colour>| match c {
        Some(c) if styles.model_colours && c != Colour::White => c.display_rgb(),
        _ => pen.rgb,
    };
    for prim in &drawing.primitives {
        match prim {
            Primitive::Line { from, to, width, colour } => {
                let weight = if *width > 0.0 { (width * k).max(0.05) } else { pen.weight_mm };
                for piece in clip_polyline(&[m.apply(*from), m.apply(*to)], clip) {
                    sheet.stroke(layer, piece, false, weight, stroke_rgb(*colour));
                }
            }
            Primitive::Arc { centre, radius, start_deg, end_deg, colour } => {
                let rot = m.rotation_deg();
                let pts = arc_points(m.apply(*centre), radius * k, start_deg + rot, end_deg + rot, ARC_SAG_MM);
                for piece in clip_polyline(&pts, clip) {
                    sheet.stroke(layer, piece, false, pen.weight_mm, stroke_rgb(*colour));
                }
            }
            Primitive::Fill { outline, colour } => {
                let poly: Vec<Pt2> = outline.iter().map(|p| m.apply(*p)).collect();
                let cut = clip_polygon(&poly, clip);
                if cut.len() >= 3 {
                    let rgb = if styles.model_colours { colour.display_rgb() } else { pen.rgb };
                    sheet.fill(layer, vec![cut], rgb);
                }
            }
            Primitive::Glyphs { contours, ink, .. } => {
                let rings: Vec<Vec<Pt2>> = contours
                    .iter()
                    .map(|c| clip_polygon(&c.iter().map(|p| m.apply(*p)).collect::<Vec<_>>(), clip))
                    .filter(|c| c.len() >= 3)
                    .collect();
                let rgb = match ink {
                    Some(c) if styles.model_colours => c.display_rgb(),
                    Some(_) => pen.rgb,
                    None => [255, 255, 255],
                };
                sheet.fill(layer, rings, rgb);
            }
            Primitive::Symbol { origin, size, rotation_deg, .. } => {
                // Unresolved artwork: a box with a cross, so its place on the sheet is not blank.
                let (s, c) = rotation_deg.to_radians().sin_cos();
                let corner = |dx: f64, dy: f64| m.apply(Pt2::new(origin.x + dx * c - dy * s, origin.y + dx * s + dy * c));
                let q = [corner(0.0, 0.0), corner(*size, 0.0), corner(*size, *size), corner(0.0, *size)];
                let mut ring = q.to_vec();
                ring.push(q[0]);
                for piece in clip_polyline(&ring, clip) {
                    sheet.stroke(layer, piece, false, pen.weight_mm, pen.rgb);
                }
                for (a, b) in [(q[0], q[2]), (q[1], q[3])] {
                    for piece in clip_polyline(&[a, b], clip) {
                        sheet.stroke(layer, piece, false, pen.weight_mm, pen.rgb);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_draw::Extents;

    #[test]
    fn hairlines_take_the_pen_weight_and_are_clipped() {
        let styles = Styles::default();
        let d = Drawing {
            primitives: vec![Primitive::Line { from: Pt2::new(0.0, 5.0), to: Pt2::new(100.0, 5.0), width: 0.0, colour: None }],
            extents: Extents::default(),
        };
        let mut sheet = ComposedSheet::new("1", "t", 100.0, 100.0);
        // Drawing units are mm at 1:10: a 100 unit line is 10 mm on paper, then moved to x = 20.
        let m = Aff::scale_rotate(0.1, 0.0).then(&Aff::translate(20.0, 20.0));
        draw_drawing(&mut sheet, &styles, &d, &m, &Rect::new(0.0, 0.0, 25.0, 100.0), "ANY");
        assert_eq!(sheet.items.len(), 1);
        match &sheet.items[0] {
            crate::item::Item::Stroke { points, weight_mm, .. } => {
                // The line runs from x = 20 to x = 30 but the clip ends at 25.
                assert_eq!(points[0], Pt2::new(20.0, 20.5));
                assert!((points[1].x - 25.0).abs() < 1e-9);
                assert_eq!(*weight_mm, styles.pens.default.weight_mm);
            }
            _ => panic!("expected a stroke"),
        }
    }

    #[test]
    fn a_wide_line_keeps_its_true_width_on_paper() {
        let styles = Styles::default();
        let d = Drawing {
            primitives: vec![Primitive::Line { from: Pt2::new(0.0, 0.0), to: Pt2::new(100.0, 0.0), width: 150.0, colour: Some(Colour::Yellow) }],
            extents: Extents::default(),
        };
        let mut sheet = ComposedSheet::new("1", "t", 100.0, 100.0);
        draw_drawing(&mut sheet, &styles, &d, &Aff::scale_rotate(0.01, 0.0).then(&Aff::translate(10.0, 10.0)), &Rect::new(0.0, 0.0, 100.0, 100.0), "ANY");
        match &sheet.items[0] {
            // 150 units at 1:100 is 1.5 mm.
            crate::item::Item::Stroke { weight_mm, rgb, .. } => assert!((weight_mm - 1.5).abs() < 1e-12 && *rgb == Colour::Yellow.display_rgb()),
            _ => panic!(),
        }
    }
}
