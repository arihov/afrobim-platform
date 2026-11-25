// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Renders a laid-out sign (the same primitives the DXF is written from, lettering as Transport
//! glyph outlines) to a standalone SVG in millimetres. Port of C# `SvgPreview`: stroke-width
//! space is Y up, so Y is flipped; colours follow the C# desktop app's display palette, so the
//! two servers return the same SVG for the same sign.

use std::fmt::Write as _;

use afrobim_draw::{Colour, Drawing, Primitive, Pt2};

pub fn hex(c: Colour) -> &'static str {
    match c {
        Colour::White => "#FFFFFF",
        Colour::Black => "#000000",
        Colour::Green => "#025C2E",
        Colour::Blue => "#005DA6",
        Colour::Brown => "#6B421F",
        Colour::Yellow => "#FFC800",
        Colour::Red => "#C8102E",
        Colour::Grey => "#8C8C8C",
    }
}

/// A number to two decimals, trailing zeros dropped (C# `Math.Round(v, 2).ToString("0.##")`,
/// which also writes a tiny negative as `-0`).
fn f(v: f64) -> String {
    let r = (v * 100.0).round_ties_even() / 100.0;
    let mut s = format!("{r:.2}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

/// `drawing` is in stroke widths; `sw_mm` is one stroke width in millimetres.
pub fn render(drawing: &Drawing, sw_mm: f64) -> String {
    let (min_x, max_y) = (drawing.extents.min.x, drawing.extents.max.y);
    let (w, h) = (
        drawing.extents.width() * sw_mm,
        drawing.extents.height() * sw_mm,
    );
    let x = |v: f64| f((v - min_x) * sw_mm);
    let y = |v: f64| f((max_y - v) * sw_mm);
    let contour = |out: &mut String, pts: &[Pt2]| {
        for (i, p) in pts.iter().enumerate() {
            let _ = write!(
                out,
                "{}{} {}",
                if i == 0 { 'M' } else { 'L' },
                x(p.x),
                y(p.y)
            );
        }
        out.push('Z');
    };

    let mut s = String::new();
    let _ = write!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}mm\" height=\"{}mm\">",
        f(w),
        f(h),
        f(w),
        f(h)
    );
    for p in &drawing.primitives {
        match p {
            Primitive::Fill { outline, colour } if outline.len() >= 3 => {
                let _ = write!(s, "<path fill=\"{}\" d=\"", hex(*colour));
                contour(&mut s, outline);
                s.push_str("\"/>");
            }
            Primitive::Fill { .. } => {}
            Primitive::Glyphs { contours, ink, .. } => {
                let _ = write!(
                    s,
                    "<path fill-rule=\"evenodd\" fill=\"{}\" d=\"",
                    hex(ink.unwrap_or(Colour::Black))
                );
                for c in contours.iter().filter(|c| c.len() >= 3) {
                    contour(&mut s, c);
                }
                s.push_str("\"/>");
            }
            Primitive::Line {
                from,
                to,
                width,
                colour,
            } => {
                let _ = write!(
                    s,
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>",
                    x(from.x),
                    y(from.y),
                    x(to.x),
                    y(to.y),
                    hex(colour.unwrap_or(Colour::White)),
                    f(width.max(0.1) * sw_mm)
                );
            }
            Primitive::Arc {
                centre,
                radius,
                start_deg,
                end_deg,
                colour,
            } => {
                // Arcs run anticlockwise from start to end in a Y-up frame.
                let mut sweep = ((end_deg - start_deg) % 360.0 + 360.0) % 360.0;
                if sweep == 0.0 {
                    sweep = 360.0;
                }
                let at = |deg: f64| {
                    Pt2::new(
                        centre.x + radius * deg.to_radians().cos(),
                        centre.y + radius * deg.to_radians().sin(),
                    )
                };
                let (start, mid, end) = (
                    at(*start_deg),
                    at(start_deg + sweep / 2.0),
                    at(start_deg + sweep),
                );
                let rr = f(radius * sw_mm);
                // Two half-sweeps avoid the degenerate full-circle case. Y is flipped, so an
                // anticlockwise arc becomes clockwise on screen (sweep-flag 0).
                let _ = write!(
                    s,
                    "<path fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" d=\"M{} {}A{rr} {rr} 0 0 0 {} {}A{rr} {rr} 0 0 0 {} {}\"/>",
                    hex(colour.unwrap_or(Colour::White)),
                    f(0.25 * sw_mm),
                    x(start.x),
                    y(start.y),
                    x(mid.x),
                    y(mid.y),
                    x(end.x),
                    y(end.y)
                );
            }
            Primitive::Symbol { origin, size, .. } => {
                // Unresolved symbol: a dashed cyan box, as in the DXF placeholder.
                let side = f(size * sw_mm);
                let _ = write!(
                    s,
                    "<rect x=\"{}\" y=\"{}\" width=\"{side}\" height=\"{side}\" fill=\"none\" stroke=\"#00FFFF\" stroke-dasharray=\"4 2\"/>",
                    x(origin.x),
                    y(origin.y + size)
                );
            }
        }
    }
    s.push_str("</svg>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_draw::Extents;

    #[test]
    fn numbers_are_rounded_to_two_decimals_without_trailing_zeros() {
        assert_eq!(f(1125.0), "1125");
        assert_eq!(f(2111.875), "2111.88");
        assert_eq!(f(46.875), "46.88");
        assert_eq!(f(0.125), "0.12");
        assert_eq!(f(640.6), "640.6");
        assert_eq!(f(-0.0001), "-0");
    }

    #[test]
    fn y_is_flipped_and_every_primitive_kind_is_drawn() {
        let drawing = Drawing {
            primitives: vec![
                Primitive::Fill {
                    outline: vec![
                        Pt2::new(0.0, 0.0),
                        Pt2::new(10.0, 0.0),
                        Pt2::new(10.0, 4.0),
                        Pt2::new(0.0, 4.0),
                    ],
                    colour: Colour::Green,
                },
                Primitive::Glyphs {
                    contours: vec![
                        vec![Pt2::new(1.0, 1.0), Pt2::new(2.0, 1.0), Pt2::new(2.0, 3.0)],
                        vec![Pt2::new(0.0, 0.0)],
                    ],
                    text: "A".into(),
                    ink: Some(Colour::White),
                },
                Primitive::Line {
                    from: Pt2::new(0.0, 2.0),
                    to: Pt2::new(10.0, 2.0),
                    width: 0.5,
                    colour: None,
                },
                Primitive::Arc {
                    centre: Pt2::new(5.0, 2.0),
                    radius: 1.0,
                    start_deg: 0.0,
                    end_deg: 180.0,
                    colour: Some(Colour::Red),
                },
                Primitive::Symbol {
                    origin: Pt2::new(1.0, 1.0),
                    id: "x".into(),
                    size: 2.0,
                    rotation_deg: 0.0,
                },
            ],
            extents: Extents {
                min: Pt2::new(0.0, 0.0),
                max: Pt2::new(10.0, 4.0),
            },
        };
        let svg = render(&drawing, 25.0);
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 250 100\" width=\"250mm\" height=\"100mm\">"));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains("<path fill=\"#025C2E\" d=\"M0 100L250 100L250 0L0 0Z\"/>"));
        assert!(svg
            .contains("<path fill-rule=\"evenodd\" fill=\"#FFFFFF\" d=\"M25 75L50 75L50 25Z\"/>"));
        assert!(svg.contains("<line x1=\"0\" y1=\"50\" x2=\"250\" y2=\"50\" stroke=\"#FFFFFF\" stroke-width=\"12.5\"/>"));
        assert!(svg.contains("stroke=\"#C8102E\" stroke-width=\"6.25\" d=\"M150 50A25 25 0 0 0 125 25A25 25 0 0 0 100 50\"/>"));
        assert!(svg.contains(
            "<rect x=\"25\" y=\"25\" width=\"50\" height=\"50\" fill=\"none\" stroke=\"#00FFFF\""
        ));
    }
}
