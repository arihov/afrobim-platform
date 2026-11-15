//! Leaders: a polyline from a pointed-at spot to a text, with an optional landing.
//!
//! The first point is the spot pointed at; it carries the terminator. The last point is where
//! the text attaches. With a landing, a horizontal line of the landing length runs from the last
//! point towards the side the last segment heads to (right when its X does not decrease), and
//! the text starts one text gap beyond it, centred on the line's height. Without a landing the
//! text starts one gap beyond the last point on the same side.
//!
//! The text is a literal, nothing, or a reference to another element's mark (its name, e.g.
//! `ADS-001`). A mark reference is resolved by [`crate::element::refresh`], which the application
//! calls after edits; the leader draws the cached text so it can be generated without the
//! document.

use serde::{Deserialize, Serialize};

use crate::geometry::*;
use crate::styles::{DimStyle, HeightSpec, Terminator};
use crate::text::{Justify, TextData};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LeaderText {
    None,
    Literal(String),
    /// The mark (name) of the element with this id. `cached` is what it last read.
    Mark { element: u64, cached: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaderData {
    /// The spot pointed at first, then the bends, then the text end. At least two.
    pub pts: Vec<Pt>,
    /// Landing length in paper millimetres (scaled by the style's scale); `None` for no landing.
    pub landing_paper_mm: Option<f64>,
    pub text: LeaderText,
    /// The style supplies the terminator, arrow size, text height and the scale.
    pub style: DimStyle,
}

impl LeaderData {
    pub fn new(pts: Vec<Pt>, text: LeaderText, style: &DimStyle) -> LeaderData {
        LeaderData { pts, landing_paper_mm: Some(style.arrow_size_mm), text, style: style.clone() }
    }

    /// The text as drawn.
    pub fn display_text(&self) -> String {
        match &self.text {
            LeaderText::None => String::new(),
            LeaderText::Literal(s) => s.clone(),
            LeaderText::Mark { cached, .. } => cached.clone(),
        }
    }

    pub fn geometry(&self) -> Result<AnnGeometry, String> {
        let st = &self.style;
        let n = self.pts.len();
        if n < 2 {
            return Err("A leader needs at least two points.".into());
        }
        let mut g = AnnGeometry::default();
        g.poly(self.pts.clone(), false);
        if let Some(d) = unit(sub(self.pts[0], self.pts[1])) {
            let a = st.m(st.arrow_size_mm);
            match st.terminator {
                Terminator::Arrow => g.fill(FillKind::Arrow, arrow_head(self.pts[0], d, a)),
                Terminator::Tick => {
                    let t = rot(d, 45.0);
                    g.line(sub(self.pts[0], mul(t, a / 2.0)), add(self.pts[0], mul(t, a / 2.0)));
                }
                Terminator::Dot => g.fill(FillKind::Dot, disc(self.pts[0], a / 3.0)),
                Terminator::None => {}
            }
        }
        let (last, prev) = (self.pts[n - 1], self.pts[n - 2]);
        let side = if last.x >= prev.x { 1.0 } else { -1.0 };
        let mut end = last;
        if let Some(l) = self.landing_paper_mm.filter(|l| *l > 0.0) {
            end = pt(last.x + side * st.m(l), last.y);
            g.line(last, end);
        }
        let text = self.display_text();
        if !text.trim().is_empty() {
            let t = TextData {
                text,
                at: pt(end.x + side * st.m(st.text_gap_mm), end.y),
                rotation_deg: 0.0,
                height: HeightSpec::Model(st.m(st.text_height_mm)),
                justify: if side > 0.0 { Justify::MiddleLeft } else { Justify::MiddleRight },
                width_factor: st.width_factor,
                wrap_width: None,
                line_spacing: 1.5,
                bold: st.bold,
                style: st.text_style.clone(),
                frame_offset: None,
                mask_offset: None,
            };
            g.extend(t.geometry());
        }
        Ok(g)
    }

    pub fn translate(&mut self, d: Pt) {
        for p in &mut self.pts {
            *p = add(*p, d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> DimStyle {
        DimStyle { text_height_mm: 10.0, arrow_size_mm: 10.0, text_gap_mm: 2.0, ..DimStyle::default() }
    }

    #[test]
    fn leader_with_landing_to_the_right() {
        let l = LeaderData::new(vec![pt(0.0, 0.0), pt(100.0, 50.0)], LeaderText::Literal("R61".into()), &st());
        let g = l.geometry().unwrap();
        // Landing 10 long to the right of the last point, text 2 beyond it, middle-left justified:
        // the baseline sits half a height (5) below the line.
        assert!(g.segs.iter().any(|s| matches!(s, Seg::Line { a, b } if *a == pt(100.0, 50.0) && *b == pt(110.0, 50.0))));
        let t = &g.labels[0];
        assert_eq!((t.at.x, t.at.y), (112.0, 45.0));
        // Arrow tip at the first point pointing away from the leader line (down-left).
        let tip = g.fills[0].pts[0];
        assert_eq!(tip, pt(0.0, 0.0));
        assert!(g.fills[0].pts[1].x > 0.0 && g.fills[0].pts[1].y > 0.0);
    }

    #[test]
    fn leader_to_the_left_and_multi_segment() {
        let l = LeaderData::new(vec![pt(0.0, 0.0), pt(-20.0, 30.0), pt(-60.0, 30.0)], LeaderText::Literal("A".into()), &st());
        let g = l.geometry().unwrap();
        let t = &g.labels[0];
        // Last segment heads left: landing to x = -70, text ends 2 further left, right-justified.
        assert_eq!(t.at.x, -72.0);
        assert_eq!(t.align, afrobim_sheets::Align::Right);
        assert!(matches!(&g.segs[0], Seg::Poly { pts, closed: false } if pts.len() == 3));
    }

    #[test]
    fn mark_reference_draws_the_cached_text() {
        let mut l = LeaderData::new(vec![pt(0.0, 0.0), pt(10.0, 10.0)], LeaderText::Mark { element: 7, cached: "ADS-001".into() }, &st());
        assert_eq!(l.geometry().unwrap().labels[0].text, "ADS-001");
        l.text = LeaderText::None;
        l.landing_paper_mm = None;
        assert!(l.geometry().unwrap().labels.is_empty());
        l.pts.truncate(1);
        assert!(l.geometry().is_err());
    }
}
