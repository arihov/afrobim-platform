// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Geometry overlap check on a laid-out guidance sign. Port of the rectangular-border path of
//! C# `LayoutOverlapCheck` (the path every stack-type sign takes).
//!
//! It uses the arrows the layout registered and every lettering tile (a text run measured to
//! its full tile: the box every spacing rule in UK TSM Chapter 7 §2–5 is measured from), and
//! raises a Warning when:
//! - a tile runs into a symbol (`LAYOUT_LEGEND_OVERLAPS_SYMBOL`);
//! - a symbol or tile crosses the inside edge of the border (`LAYOUT_SYMBOL_IN_BORDER`,
//!   `LAYOUT_LEGEND_IN_BORDER`).
//!
//! Only overlap with area counts. Shapes that merely touch do not. The minimum gaps themselves
//! (2.5 sw, 4 sw and so on) are each layout's job; this check catches geometry that collides
//! outright.

use afrobim_draw::Pt2;

use crate::measurer::TextTileMeasurer;
use crate::model::Diagnostic;
use crate::pack::RulePack;
use crate::prim::Prim;
use crate::stack::RawLayout;
use crate::SignError;

/// Overlap area below this (sw²) is rounding, not an overlap.
const AREA_TOLERANCE: f64 = 1e-6;

type Poly = Vec<(f64, f64)>;

pub(crate) fn check(
    pack: &RulePack,
    measurer: &TextTileMeasurer,
    result: &RawLayout,
) -> Result<Vec<Diagnostic>, SignError> {
    let mut diags = Vec::new();
    if result.symbols.is_empty() {
        return Ok(diags);
    }
    let clause = pack.clause("layoutOverlap", &pack.source);

    let symbols: Vec<Poly> = result
        .symbols
        .iter()
        .map(|o| o.iter().map(|p: &Pt2| (p.x, p.y)).collect::<Poly>())
        .filter(|o| o.len() >= 3)
        .collect();

    let b = pack.panel.border_width;
    let (ix0, iy0) = (b, b);
    let (ix1, iy1) = (result.width - b, result.height - b);

    for prim in &result.prims {
        let Prim::Text(t) = prim else { continue };
        let w = measurer.width(&t.text, t.face)?;
        let (x0, y0) = (t.origin.x, t.origin.y);
        let (x1, y1) = (t.origin.x + w, t.origin.y + t.tile_height());

        if symbols
            .iter()
            .any(|s| clipped_area(s, x0, y0, x1, y1) > AREA_TOLERANCE)
        {
            diags.push(Diagnostic::warning(
                "LAYOUT_LEGEND_OVERLAPS_SYMBOL",
                format!(
                    "Legend tile '{}' runs into an arrow or route symbol",
                    t.text
                ),
                clause,
            ));
        }
        if x0 < ix0 - 1e-9 || y0 < iy0 - 1e-9 || x1 > ix1 + 1e-9 || y1 > iy1 + 1e-9 {
            diags.push(Diagnostic::warning(
                "LAYOUT_LEGEND_IN_BORDER",
                format!(
                    "Legend tile '{}' crosses the inside edge of the border",
                    t.text
                ),
                clause,
            ));
        }
    }

    for s in &symbols {
        let area = signed_area(s).abs();
        if area - clipped_area(s, ix0, iy0, ix1, iy1) > AREA_TOLERANCE {
            diags.push(Diagnostic::warning(
                "LAYOUT_SYMBOL_IN_BORDER",
                "An arrow or route symbol crosses the inside edge of the border".to_string(),
                clause,
            ));
            break;
        }
    }
    Ok(diags)
}

pub(crate) fn signed_area(poly: &[(f64, f64)]) -> f64 {
    let mut a = 0.0;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        a += poly[j].0 * poly[i].1 - poly[i].0 * poly[j].1;
        j = i;
    }
    a / 2.0
}

/// Area of `poly` inside the axis-aligned box (Sutherland–Hodgman against the four box edges;
/// correct for concave polygons, whose clipped result may carry zero-area bridging edges that
/// do not change the area).
pub(crate) fn clipped_area(poly: &[(f64, f64)], x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    if x1 <= x0 || y1 <= y0 {
        return 0.0;
    }
    let p = clip(poly, |q| q.0 >= x0, |a, c| cross(a, c, x0, true));
    let p = clip(&p, |q| q.0 <= x1, |a, c| cross(a, c, x1, true));
    let p = clip(&p, |q| q.1 >= y0, |a, c| cross(a, c, y0, false));
    let p = clip(&p, |q| q.1 <= y1, |a, c| cross(a, c, y1, false));
    if p.len() < 3 {
        0.0
    } else {
        signed_area(&p).abs()
    }
}

pub(crate) fn clip(
    poly: &[(f64, f64)],
    inside: impl Fn((f64, f64)) -> bool,
    crossing: impl Fn((f64, f64), (f64, f64)) -> (f64, f64),
) -> Poly {
    let mut res = Vec::new();
    let Some(&last) = poly.last() else { return res };
    let mut prev = last;
    let mut prev_in = inside(prev);
    for &cur in poly {
        let cur_in = inside(cur);
        if cur_in {
            if !prev_in {
                res.push(crossing(prev, cur));
            }
            res.push(cur);
        } else if prev_in {
            res.push(crossing(prev, cur));
        }
        prev = cur;
        prev_in = cur_in;
    }
    res
}

fn cross(a: (f64, f64), c: (f64, f64), v: f64, vertical: bool) -> (f64, f64) {
    if vertical {
        let t = (v - a.0) / (c.0 - a.0);
        (v, a.1 + t * (c.1 - a.1))
    } else {
        let t = (v - a.1) / (c.1 - a.1);
        (a.0 + t * (c.0 - a.0), v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_a_concave_arrow_keeps_only_the_area_inside_the_box() {
        let square = [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)];
        assert_eq!(clipped_area(&square, 1.0, 1.0, 3.0, 5.0), 6.0);
        assert_eq!(clipped_area(&square, 4.0, 0.0, 6.0, 4.0), 0.0); // touching is not overlap
        assert_eq!(clipped_area(&square, 5.0, 5.0, 6.0, 6.0), 0.0);
        // An L (concave): 3 unit squares; the box takes the corner square and half of another.
        let l = [
            (0.0, 0.0),
            (2.0, 0.0),
            (2.0, 1.0),
            (1.0, 1.0),
            (1.0, 2.0),
            (0.0, 2.0),
        ];
        assert_eq!(signed_area(&l), 3.0);
        assert_eq!(clipped_area(&l, 0.0, 0.0, 1.5, 1.0), 1.5);
    }
}
