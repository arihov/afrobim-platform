//! A plain SVG of a drawing in millimetres (Y up, lower-left on the origin).
//!
//! One element per primitive, in drawing order, each with a `class` naming its kind:
//! `fill`, `glyphs` (even-odd, so counters stay open), `line`, `arc` and `symbol`. The colours
//! are the display approximations of `afrobim_draw::Colour::display_rgb`, not specification
//! values. The picture is for looking at and for reports; fabricate from the DXF.

use std::fmt::Write;

use afrobim_draw::{Colour, Drawing, Primitive, Pt2};

/// Stroke for centrelines and placeholders, which have no width of their own: one screen pixel.
const HAIRLINE: &str = "stroke-width=\"1\" vector-effect=\"non-scaling-stroke\"";

pub fn hex(c: Colour) -> String {
    let [r, g, b] = c.display_rgb();
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// A coordinate with at most three decimals (a micron) and no trailing zeros.
fn n(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".to_string()
    } else {
        s.to_string()
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `sign`: the stroke width and x-height in mm, recorded on the root element for a sign.
pub fn write(drawing: &Drawing, sign: Option<(f64, f64)>) -> String {
    let (w, h) = (drawing.extents.width(), drawing.extents.height());
    let (ox, oy) = (drawing.extents.min.x, drawing.extents.min.y);
    // SVG has Y down: flip about the top edge.
    let p = |q: &Pt2| format!("{},{}", n(q.x - ox), n(h - (q.y - oy)));
    let ring = |pts: &[Pt2]| {
        let mut d = String::new();
        for (i, q) in pts.iter().enumerate() {
            d.push_str(if i == 0 { "M" } else { " L" });
            d.push_str(&p(q));
        }
        d.push_str(" Z");
        d
    };

    let mut s = String::new();
    let _ = write!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\" data-units=\"mm\"",
        w = n(w),
        h = n(h),
    );
    if let Some((sw_mm, x_height_mm)) = sign {
        let _ = write!(s, " data-sw-mm=\"{}\" data-x-height-mm=\"{}\"", n(sw_mm), n(x_height_mm));
    }
    s.push_str(">\n");
    for prim in &drawing.primitives {
        match prim {
            Primitive::Fill { outline, colour } => {
                if outline.len() >= 3 {
                    let _ = writeln!(s, "  <path class=\"fill\" fill=\"{}\" d=\"{}\"/>", hex(*colour), ring(outline));
                }
            }
            Primitive::Glyphs { contours, text, ink } => {
                let d: Vec<String> = contours.iter().filter(|c| c.len() >= 3).map(|c| ring(c)).collect();
                if d.is_empty() {
                    continue;
                }
                let _ = writeln!(
                    s,
                    "  <path class=\"glyphs\" fill=\"{}\" fill-rule=\"evenodd\" d=\"{}\"><title>{}</title></path>",
                    hex(ink.unwrap_or(Colour::White)),
                    d.join(" "),
                    escape(text),
                );
            }
            Primitive::Line { from, to, width, colour } => {
                let stroke = hex(colour.unwrap_or(Colour::Black));
                let weight = if *width > 0.0 { format!("stroke-width=\"{}\"", n(*width)) } else { HAIRLINE.to_string() };
                let _ = writeln!(
                    s,
                    "  <line class=\"line\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{stroke}\" {weight} stroke-linecap=\"butt\"/>",
                    n(from.x - ox),
                    n(h - (from.y - oy)),
                    n(to.x - ox),
                    n(h - (to.y - oy)),
                );
            }
            Primitive::Arc { centre, radius, start_deg, end_deg, colour } => {
                let stroke = hex(colour.unwrap_or(Colour::Black));
                let sweep = end_deg - start_deg;
                if sweep.abs() >= 360.0 {
                    let _ = writeln!(
                        s,
                        "  <circle class=\"arc\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke=\"{stroke}\" {HAIRLINE}/>",
                        n(centre.x - ox),
                        n(h - (centre.y - oy)),
                        n(*radius),
                    );
                    continue;
                }
                let at = |deg: f64| {
                    let a = deg.to_radians();
                    Pt2::new(centre.x + radius * a.cos(), centre.y + radius * a.sin())
                };
                // Anticlockwise with Y up is still anticlockwise on screen after the flip:
                // SVG sweep-flag 0. A negative sweep runs clockwise.
                let _ = writeln!(
                    s,
                    "  <path class=\"arc\" fill=\"none\" stroke=\"{stroke}\" {HAIRLINE} d=\"M{} A{r},{r} 0 {} {} {}\"/>",
                    p(&at(*start_deg)),
                    u8::from(sweep.abs() > 180.0),
                    u8::from(sweep < 0.0),
                    p(&at(*end_deg)),
                    r = n(*radius),
                );
            }
            Primitive::Symbol { origin, id, size, .. } => {
                // Artwork not resolved to geometry: a dashed box, as the DXF writer marks it.
                let _ = writeln!(
                    s,
                    "  <rect class=\"symbol\" data-symbol-id=\"{}\" x=\"{}\" y=\"{}\" width=\"{sz}\" height=\"{sz}\" fill=\"none\" \
                     stroke=\"#00B7C3\" stroke-dasharray=\"4 3\" {HAIRLINE}/>",
                    escape(id),
                    n(origin.x - ox),
                    n(h - (origin.y - oy) - size),
                    sz = n(*size),
                );
            }
        }
    }
    s.push_str("</svg>\n");
    s
}
