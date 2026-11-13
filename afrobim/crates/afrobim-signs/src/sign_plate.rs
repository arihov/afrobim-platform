//! The plate of a single-panel sign: its ground and, in a pack with a `signBorder` section,
//! its drawn border (UK TSM Chapter 7 §2.15, Figure 2-4). Port of C# `SignPlate`.
//!
//! With a border the plate is two fills: the whole plate in the legend colour with
//! `SignBorderRules::outer_corner_radius` corners, then the ground inset by
//! `PanelRules::border_width` with `PanelRules::corner_radius` corners (concentric with the
//! outside ones). The layouts already keep every tile clearance to the inside edge of the
//! border, so the border paints that band and moves nothing else.
//!
//! Without the section the plate is the original square ground.

use afrobim_draw::Pt2;

use crate::model::Colour;
use crate::pack::RulePack;
use crate::prim::{rect, Prim};
use crate::stack::Palette;

pub(crate) fn draw(pack: &RulePack, width: f64, height: f64, palette: &Palette) -> Vec<Prim> {
    let Some(sb) = &pack.dir.sign_border else {
        return vec![rect(0.0, 0.0, width, height, palette.ground)];
    };
    let b = pack.panel.border_width;
    vec![
        rounded_rect(
            0.0,
            0.0,
            width,
            height,
            sb.outer_corner_radius,
            sb.segments_per_corner,
            palette.lettering,
        ),
        rounded_rect(
            b,
            b,
            width - b,
            height - b,
            pack.panel.corner_radius,
            sb.segments_per_corner,
            palette.ground,
        ),
    ]
}

/// A rectangle with circular corners of radius `r`, as one anticlockwise ring of the four
/// corner arcs (the straight edges join them).
fn rounded_rect(x0: f64, y0: f64, x1: f64, y1: f64, r: f64, segments: i64, fill: Colour) -> Prim {
    let r = r.min((x1 - x0).min(y1 - y0) / 2.0);
    let mut pts = Vec::with_capacity(4 * (segments.max(0) as usize + 1));
    arc(&mut pts, x0 + r, y0 + r, r, 180.0, segments); // bottom-left
    arc(&mut pts, x1 - r, y0 + r, r, 270.0, segments); // bottom-right
    arc(&mut pts, x1 - r, y1 - r, r, 0.0, segments); // top-right
    arc(&mut pts, x0 + r, y1 - r, r, 90.0, segments); // top-left
    Prim::Fill {
        outline: pts,
        colour: fill,
    }
}

fn arc(pts: &mut Vec<Pt2>, cx: f64, cy: f64, r: f64, start_deg: f64, segments: i64) {
    for i in 0..=segments {
        let t = (start_deg + 90.0 * i as f64 / segments as f64) * std::f64::consts::PI / 180.0;
        pts.push(Pt2::new(cx + r * t.cos(), cy + r * t.sin()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FontFace;
    use crate::pack::tests::uk_pack;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    const PALETTE: Palette = Palette {
        ground: Colour::Green,
        lettering: Colour::White,
        arm: Colour::White,
        face: FontFace::TransportMedium,
    };

    fn outline(p: &Prim) -> (&[Pt2], Colour) {
        match p {
            Prim::Fill { outline, colour } => (outline, *colour),
            other => panic!("not a fill: {other:?}"),
        }
    }

    /// UK pack: border 1.5 sw (§2.5.4), outside corners r3, inside corners r1.5 (Figure 2-4),
    /// 12 segments per corner.
    #[test]
    fn a_bordered_plate_is_the_legend_colour_under_an_inset_ground() {
        let pack = uk_pack();
        let prims = draw(&pack, 60.0, 24.0, &PALETTE);
        assert_eq!(prims.len(), 2);

        let (outer, outer_colour) = outline(&prims[0]);
        assert_eq!(outer_colour, Colour::White);
        assert_eq!(outer.len(), 4 * 13);
        // Bottom-left arc: from the left edge at the corner's centre height to the bottom edge.
        assert!(close(outer[0].x, 0.0) && close(outer[0].y, 3.0));
        assert!(close(outer[12].x, 3.0) && close(outer[12].y, 0.0));
        // Bottom-right, top-right and top-left arcs follow, anticlockwise.
        assert!(close(outer[13].x, 57.0) && close(outer[13].y, 0.0));
        assert!(close(outer[25].x, 60.0) && close(outer[25].y, 3.0));
        assert!(close(outer[26].x, 60.0) && close(outer[26].y, 21.0));
        assert!(close(outer[38].x, 57.0) && close(outer[38].y, 24.0));
        assert!(close(outer[39].x, 3.0) && close(outer[39].y, 24.0));
        assert!(close(outer[51].x, 0.0) && close(outer[51].y, 21.0));

        let (inner, inner_colour) = outline(&prims[1]);
        assert_eq!(inner_colour, Colour::Green);
        assert_eq!(inner.len(), 4 * 13);
        assert!(close(inner[0].x, 1.5) && close(inner[0].y, 3.0));
        assert!(close(inner[12].x, 3.0) && close(inner[12].y, 1.5));
        // Every corner point lies on its circle, so the two rings are concentric.
        for (ring, r) in [(outer, 3.0), (inner, 1.5)] {
            for p in &ring[..13] {
                assert!(close((p.x - 3.0).hypot(p.y - 3.0), r), "{p:?}");
            }
        }
    }

    #[test]
    fn a_corner_radius_never_exceeds_half_the_shorter_side() {
        let pack = uk_pack();
        // 4 sw high: the outside radius is capped at 2, the inside (1 sw high) at 0.5.
        let prims = draw(&pack, 20.0, 4.0, &PALETTE);
        let (outer, _) = outline(&prims[0]);
        assert!(close(outer[0].x, 0.0) && close(outer[0].y, 2.0));
        assert!(close(outer[12].x, 2.0) && close(outer[12].y, 0.0));
        let (inner, _) = outline(&prims[1]);
        assert!(close(inner[0].x, 1.5) && close(inner[0].y, 2.0));
        assert!(close(inner[12].x, 2.0) && close(inner[12].y, 1.5));
    }

    #[test]
    fn without_a_border_section_the_plate_is_the_square_ground() {
        let mut pack = uk_pack();
        pack.dir.sign_border = None;
        let prims = draw(&pack, 60.0, 24.0, &PALETTE);
        assert_eq!(prims, vec![rect(0.0, 0.0, 60.0, 24.0, Colour::Green)]);
    }
}
