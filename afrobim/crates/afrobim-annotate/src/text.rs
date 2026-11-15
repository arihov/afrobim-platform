//! Text: single and multi-line, nine-point justification, wrapping, frame and mask.
//!
//! Layout rules (this crate's definitions, stated so they can be checked):
//!
//! - Text height is the capital height. A line is `height` tall from baseline to cap line;
//!   descenders are ignored for layout.
//! - Lines are `height * line_spacing` apart, baseline to baseline. The block is as wide as its
//!   widest line (Helvetica metrics of `afrobim-sheets`, stretched by the width factor) and
//!   `height + (lines - 1) * pitch` tall, from the baseline of the last line to the cap line of
//!   the first.
//! - The insertion point is the justification corner of that block: *Top* is the cap line of the
//!   first line, *Bottom* the baseline of the last line, *Middle* halfway between; *Left*,
//!   *Centre* and *Right* are the block's left edge, middle and right edge. Within a block each
//!   line is justified the same way (left lines start at the left edge, centred lines are centred
//!   on the block, right lines end at the right edge).
//! - The block is rotated about the insertion point, anticlockwise in degrees.
//! - A frame is the block grown by `frame_offset * height` on every side; a mask is the same
//!   rectangle filled with a background colour.

use afrobim_sheets::{text_width, wrap, Align};
use serde::{Deserialize, Serialize};

use crate::geometry::*;
use crate::styles::{HeightSpec, TextStyle};

/// The nine attachment points of a text block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Justify {
    TopLeft,
    TopCentre,
    TopRight,
    MiddleLeft,
    MiddleCentre,
    MiddleRight,
    BottomLeft,
    BottomCentre,
    BottomRight,
}

impl Justify {
    /// (horizontal, vertical) with vertical 0 top, 1 middle, 2 bottom.
    fn parts(self) -> (Align, u8) {
        use Justify::*;
        match self {
            TopLeft => (Align::Left, 0),
            TopCentre => (Align::Centre, 0),
            TopRight => (Align::Right, 0),
            MiddleLeft => (Align::Left, 1),
            MiddleCentre => (Align::Centre, 1),
            MiddleRight => (Align::Right, 1),
            BottomLeft => (Align::Left, 2),
            BottomCentre => (Align::Centre, 2),
            BottomRight => (Align::Right, 2),
        }
    }
}

/// A text element's definition. `at` is in the element's local frame (see the crate docs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextData {
    /// The text. `\n` starts a new line.
    pub text: String,
    pub at: Pt,
    pub rotation_deg: f64,
    pub height: HeightSpec,
    pub justify: Justify,
    pub width_factor: f64,
    /// Wrap lines wider than this (model mm, before rotation). `None` does not wrap.
    pub wrap_width: Option<f64>,
    /// Baseline to baseline, as a multiple of the capital height.
    pub line_spacing: f64,
    pub bold: bool,
    /// Name of the text style it was made from.
    pub style: String,
    /// Draw a frame, grown by this many text heights on each side.
    pub frame_offset: Option<f64>,
    /// Fill the same rectangle with a background mask, grown by this many text heights.
    pub mask_offset: Option<f64>,
}

impl TextData {
    /// A text at `at` with the properties of `style`, sized in paper millimetres at `scale`.
    pub fn new(text: &str, at: Pt, style: &TextStyle, scale: f64) -> TextData {
        TextData {
            text: text.to_string(),
            at,
            rotation_deg: 0.0,
            height: HeightSpec::Paper { mm: style.height_paper_mm, scale },
            justify: Justify::BottomLeft,
            width_factor: style.width_factor,
            wrap_width: None,
            line_spacing: style.line_spacing,
            bold: style.bold,
            style: style.name.clone(),
            frame_offset: None,
            mask_offset: None,
        }
    }

    /// The lines after wrapping.
    pub fn lines(&self) -> Vec<String> {
        let h = self.height.model_mm();
        match self.wrap_width {
            Some(w) if w > 0.0 && self.width_factor > 0.0 => wrap(&self.text, w / self.width_factor, h, self.bold),
            _ => self.text.split('\n').map(str::to_string).collect(),
        }
    }

    /// Unrotated block size (width, height) in model mm.
    pub fn block_size(&self) -> (f64, f64) {
        let h = self.height.model_mm();
        let lines = self.lines();
        let w = lines.iter().map(|l| text_width(l, h, self.bold) * self.width_factor).fold(0.0, f64::max);
        (w, h + (lines.len().saturating_sub(1)) as f64 * h * self.line_spacing)
    }

    /// The four corners of the block, anticlockwise from its lower left, rotated and placed.
    pub fn block_corners(&self) -> [Pt; 4] {
        let (w, bh) = self.block_size();
        let (x0, y_bottom) = self.block_origin(w, bh);
        let c = |x: f64, y: f64| add(self.at, rot(pt(x, y), self.rotation_deg));
        [c(x0, y_bottom), c(x0 + w, y_bottom), c(x0 + w, y_bottom + bh), c(x0, y_bottom + bh)]
    }

    /// Left edge x and bottom edge y of the block relative to the insertion point, unrotated.
    fn block_origin(&self, w: f64, bh: f64) -> (f64, f64) {
        let (h, v) = self.justify.parts();
        let x0 = match h {
            Align::Left => 0.0,
            Align::Centre => -w / 2.0,
            Align::Right => -w,
        };
        let y_bottom = match v {
            0 => -bh,
            1 => -bh / 2.0,
            _ => 0.0,
        };
        (x0, y_bottom)
    }

    /// The drawn form: one [`Label`] per line, plus frame and mask when asked for.
    pub fn geometry(&self) -> AnnGeometry {
        let mut g = AnnGeometry::default();
        let h = self.height.model_mm();
        if !(h > 0.0 && h.is_finite()) {
            g.notes.push("The text height is not positive; nothing is drawn.".into());
            return g;
        }
        let lines = self.lines();
        let (w, bh) = self.block_size();
        let (x0, y_bottom) = self.block_origin(w, bh);
        let (halign, _) = self.justify.parts();
        let pitch = h * self.line_spacing;
        let n = lines.len();
        for (i, line) in lines.iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            // Baseline of line i: the last line sits on the bottom edge.
            let y = y_bottom + (n - 1 - i) as f64 * pitch;
            let x = match halign {
                Align::Left => x0,
                Align::Centre => x0 + w / 2.0,
                Align::Right => x0 + w,
            };
            g.labels.push(Label {
                text: line.clone(),
                at: add(self.at, rot(pt(x, y), self.rotation_deg)),
                height: h,
                rotation_deg: self.rotation_deg,
                align: halign,
                width_factor: self.width_factor,
                bold: self.bold,
                style: self.style.clone(),
            });
        }
        let grown = |k: f64| {
            let (m, c) = (k * h, self.block_corners());
            let (ex, ey) = (unit(sub(c[1], c[0])).unwrap_or(pt(1.0, 0.0)), unit(sub(c[3], c[0])).unwrap_or(pt(0.0, 1.0)));
            let (ex, ey) = (mul(ex, m), mul(ey, m));
            vec![sub(sub(c[0], ex), ey), add(sub(c[1], ey), ex), add(add(c[2], ex), ey), sub(add(c[3], ey), ex)]
        };
        if let Some(k) = self.mask_offset {
            g.fill(FillKind::Mask, grown(k));
        }
        if let Some(k) = self.frame_offset {
            g.poly(grown(k), true);
        }
        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(text: &str, justify: Justify) -> TextData {
        TextData { justify, height: HeightSpec::Model(10.0), line_spacing: 1.5, width_factor: 1.0, ..TextData::new(text, pt(100.0, 200.0), &TextStyle::default(), 1.0) }
    }

    fn close(a: Pt, b: Pt) -> bool {
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
    }

    #[test]
    fn single_line_justification_hand_worked() {
        // "AB": Helvetica A=667, B=667; at cap height 10 the font size is 10/0.718, so
        // the width is 1334/1000 * 10/0.718 = 18.5792...
        let w = text_width("AB", 10.0, false);
        assert!((w - 1.334 * 10.0 / 0.718).abs() < 1e-9);
        let cases = [
            (Justify::BottomLeft, pt(100.0, 200.0), Align::Left),
            (Justify::BottomCentre, pt(100.0, 200.0), Align::Centre),
            (Justify::BottomRight, pt(100.0, 200.0), Align::Right),
            // Middle: the baseline is half a height below the insertion point.
            (Justify::MiddleLeft, pt(100.0, 195.0), Align::Left),
            // Top: the cap line is at the insertion point, so the baseline is a full height below.
            (Justify::TopRight, pt(100.0, 190.0), Align::Right),
        ];
        for (j, expect, align) in cases {
            let g = base("AB", j).geometry();
            assert_eq!(g.labels.len(), 1);
            let l = &g.labels[0];
            // The label anchor sits on the justification x for centre and right, and on the left edge
            // for left, so the anchor itself equals the insertion point horizontally in all three.
            assert!(close(l.at, expect), "{j:?} {:?}", l.at);
            assert_eq!(l.align, align);
        }
        // The block's corners show the extent: bottom-centre spans -w/2..w/2.
        let c = base("AB", Justify::BottomCentre).block_corners();
        assert!((c[0].x - (100.0 - w / 2.0)).abs() < 1e-9 && (c[1].x - (100.0 + w / 2.0)).abs() < 1e-9);
        assert!((c[0].y - 200.0).abs() < 1e-9 && (c[2].y - 210.0).abs() < 1e-9);
    }

    #[test]
    fn multi_line_block_and_pitch() {
        // Three lines, pitch 15, block height 10 + 2 * 15 = 40.
        let t = base("A\nB\nC", Justify::TopLeft);
        assert_eq!(t.block_size().1, 40.0);
        let g = t.geometry();
        let ys: Vec<f64> = g.labels.iter().map(|l| l.at.y).collect();
        // Top attachment: the cap line of line 1 is at y = 200, so its baseline is 190, then 175, 160.
        assert_eq!(ys, vec![190.0, 175.0, 160.0]);
        let b = base("A\nB\nC", Justify::BottomLeft).geometry();
        let ys: Vec<f64> = b.labels.iter().map(|l| l.at.y).collect();
        assert_eq!(ys, vec![230.0, 215.0, 200.0]); // last baseline on the insertion point
        let m = base("A\nB\nC", Justify::MiddleCentre).geometry();
        let ys: Vec<f64> = m.labels.iter().map(|l| l.at.y).collect();
        assert_eq!(ys, vec![210.0, 195.0, 180.0]); // block spans 180..220 about y = 200
    }

    #[test]
    fn rotation_turns_the_block_about_the_insertion_point() {
        let mut t = base("AB", Justify::BottomLeft);
        t.rotation_deg = 90.0;
        let c = t.block_corners();
        let w = text_width("AB", 10.0, false);
        // The baseline now runs up +Y from the insertion point; the cap height goes to -X.
        assert!(close(c[1], pt(100.0, 200.0 + w)));
        assert!(close(c[3], pt(90.0, 200.0)));
        let l = &t.geometry().labels[0];
        assert!(close(l.at, pt(100.0, 200.0)) && l.rotation_deg == 90.0);
    }

    #[test]
    fn wrapping_respects_the_width_and_the_width_factor() {
        let mut t = base("the quick brown fox jumps over the lazy dog", Justify::TopLeft);
        t.wrap_width = Some(150.0);
        let lines = t.lines();
        assert!(lines.len() > 1);
        assert_eq!(lines.join(" "), "the quick brown fox jumps over the lazy dog");
        for l in &lines {
            assert!(text_width(l, 10.0, false) <= 150.0 + 1e-6, "{l}");
        }
        // Stretched 2x: lines may be at most half as wide before stretching, so more lines.
        let mut w2 = t.clone();
        w2.width_factor = 2.0;
        assert!(w2.lines().len() > lines.len());
        for l in &w2.lines() {
            assert!(text_width(l, 10.0, false) * 2.0 <= 150.0 + 1e-6);
        }
        // Block width never exceeds the wrap width.
        assert!(t.block_size().0 <= 150.0 + 1e-6);
    }

    #[test]
    fn frame_and_mask_grow_the_block() {
        let mut t = base("AB", Justify::BottomLeft);
        t.frame_offset = Some(0.5);
        t.mask_offset = Some(0.5);
        let g = t.geometry();
        let w = text_width("AB", 10.0, false);
        let Seg::Poly { pts, closed } = &g.segs[0] else { panic!() };
        assert!(*closed);
        assert!(close(pts[0], pt(95.0, 195.0)) && close(pts[2], pt(100.0 + w + 5.0, 215.0)));
        assert_eq!(g.fills[0].kind, FillKind::Mask);
        assert!(close(g.fills[0].pts[0], pts[0]));
    }

    #[test]
    fn paper_height_follows_the_annotation_scale() {
        let t = TextData::new("X", pt(0.0, 0.0), &TextStyle::default(), 500.0);
        assert_eq!(t.geometry().labels[0].height, 1250.0);
        let mut z = base("X", Justify::BottomLeft);
        z.height = HeightSpec::Model(0.0);
        assert!(z.geometry().labels.is_empty());
    }
}
