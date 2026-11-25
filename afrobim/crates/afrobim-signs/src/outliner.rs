// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Converts lettering to Transport glyph outlines. Port of C# `TextOutliner`.
//!
//! Each text run becomes one `Glyphs` primitive:
//! - Glyphs are scaled so the face's own x-height is the run's x-height (4 sw).
//! - The baseline sits half an x-height above the tile foot (Figure 2-1).
//! - Each character occupies its tile, tiled exactly as [`crate::measurer::tiles`] tiles it:
//!   the pack's tile table (UK TSM Chapter 7 Appendix C, with its pair rules) where the pack
//!   has one, otherwise the face's own advance. Words are separated by the layout's word
//!   spacing. The outlines therefore fill the tiles the layout reserved. A run that comes out
//!   wider than its tiles raises a `TEXT_WIDER_THAN_TILES` warning instead of being squeezed.
//! - A table tile is not the font's advance (they differ by up to about 0.4 sw), so each glyph
//!   is centred in its tile: it is drawn from the tile's left edge plus half of (tile width -
//!   font advance). The standard prints only the widths, not where a letter sits on its tile;
//!   centring splits the difference evenly between the two sides, so neither neighbour gains
//!   or loses all of it. A multi-character tile (the feet mark `'-`) is centred as a group.
//! - After outlining, if the ink of two neighbouring glyphs on a line overlaps horizontally by
//!   more than [`OVERLAP_TOLERANCE`] sw, a `GLYPHS_OVERLAP` warning is raised: letters must
//!   never touch.
//!
//! Other primitives pass through unchanged.

use afrobim_draw::{Primitive, Pt2};

use crate::font::FontMetrics;
use crate::measurer::{self, TextTileMeasurer};
use crate::model::{Diagnostic, FontFace};
use crate::pack::PanelRules;
use crate::prim::{Prim, TextRun};
use crate::SignError;

/// Horizontal overlap of neighbouring glyph boxes, in sw, above which a run is flagged.
pub const OVERLAP_TOLERANCE: f64 = 0.05;

pub(crate) struct TextOutliner<'a> {
    /// The measurer the layout used, to check that outlined lettering fits the tiles it was given.
    pub measurer: &'a TextTileMeasurer<'a>,
    /// Transport Medium, for white lettering on dark grounds.
    pub medium: &'a FontMetrics,
    /// Transport Heavy, for black lettering on light grounds.
    pub heavy: &'a FontMetrics,
    pub panel: &'a PanelRules,
}

/// The horizontal ink extent of one glyph.
struct GlyphBox {
    text: char,
    min_x: f64,
    max_x: f64,
}

impl TextOutliner<'_> {
    pub fn apply(
        &self,
        prims: Vec<Prim>,
        diags: &mut Vec<Diagnostic>,
    ) -> Result<Vec<Primitive>, SignError> {
        let mut out = Vec::with_capacity(prims.len());
        for p in prims {
            let t = match p {
                Prim::Fill { outline, colour } => {
                    out.push(Primitive::Fill { outline, colour });
                    continue;
                }
                Prim::Symbol {
                    origin,
                    id,
                    size,
                    rotation_deg,
                } => {
                    out.push(Primitive::Symbol {
                        origin,
                        id,
                        size,
                        rotation_deg,
                    });
                    continue;
                }
                Prim::Line {
                    from,
                    to,
                    width,
                    colour,
                } => {
                    out.push(Primitive::Line {
                        from,
                        to,
                        width,
                        colour,
                    });
                    continue;
                }
                Prim::Text(t) => t,
            };
            let (contours, boxes) = self.outline_with_boxes(&t)?;
            out.push(Primitive::Glyphs {
                contours,
                text: t.text.clone(),
                ink: t.ink,
            });
            for pair in boxes.windows(2) {
                let overlap = pair[0].max_x - pair[1].min_x;
                if overlap > OVERLAP_TOLERANCE {
                    let source = measurer::table_for(self.panel, t.face)
                        .map_or("App.1 §1.1 (character tiles)", |tb| tb.source.as_str());
                    diags.push(Diagnostic::warning(
                        "GLYPHS_OVERLAP",
                        format!(
                            "'{}' in {}: '{}' and '{}' overlap by {} sw",
                            t.text,
                            t.face.name(),
                            pair[0].text,
                            pair[1].text,
                            format_trimmed(overlap, 3)
                        ),
                        source,
                    ));
                }
            }
            let over = self.width(&t, self.face_for(&t))?
                - self.measurer.width(&t.text, t.face)? * t.x_height / 4.0;
            if over > 0.05 {
                diags.push(Diagnostic::warning(
                    "TEXT_WIDER_THAN_TILES",
                    format!(
                        "'{}' in {} is {} sw wider than the tiles the layout measured",
                        t.text,
                        t.face.name(),
                        format_trimmed(over, 2)
                    ),
                    "UK TSM Chapter 7 §2.3.1 (tile widths per alphabet)",
                ));
            }
        }
        Ok(out)
    }

    fn face_for(&self, t: &TextRun) -> &FontMetrics {
        if t.face == FontFace::TransportHeavy {
            self.heavy
        } else {
            self.medium
        }
    }

    /// A run's width in sw set in `font`, with the layout's word spacing.
    fn width(&self, t: &TextRun, font: &FontMetrics) -> Result<f64, SignError> {
        let ratio = t.x_height / 4.0;
        let mut w = 0.0;
        let table = measurer::table_for(self.panel, t.face);
        for (i, word) in measurer::words(&t.text).into_iter().enumerate() {
            if i > 0 {
                w += self.panel.word_spacing;
            }
            w += measurer::tiles(word, table, font, font.x_height_em())?
                .iter()
                .map(|tile| tile.width)
                .sum::<f64>();
        }
        Ok(w * ratio)
    }

    /// The outline, and the horizontal ink extent of each glyph that has ink, in line order.
    fn outline_with_boxes(&self, t: &TextRun) -> Result<(Vec<Vec<Pt2>>, Vec<GlyphBox>), SignError> {
        let face = self.face_for(t);
        // Glyph placement follows the tile convention (TSM Vol.2 App.1 §1.1: lower-case letters
        // sit 2 sw above the tile foot on a 4 sw x-height), not the font's own origin: the
        // face's baseline goes on the run's baseline and its x-height spans exactly one run
        // x-height, whatever baseline offset the font file was drawn with.
        let em_sw = t.x_height / face.x_height_em();
        let baseline = t.baseline().y - face.baseline_em() * em_sw;
        let ratio = t.x_height / 4.0; // tiles scale with the run's x-height
        let table = measurer::table_for(self.panel, t.face);

        let mut contours: Vec<Vec<Pt2>> = Vec::new();
        let mut boxes = Vec::new();
        let mut x = t.origin.x;
        for (w, word) in measurer::words(&t.text).into_iter().enumerate() {
            if w > 0 {
                x += self.panel.word_spacing * ratio;
            }
            for tile in measurer::tiles(word, table, face, face.x_height_em())? {
                // Centre the glyph(s) in the tile: offset by half of (tile - font advance).
                let mut advance = 0.0;
                for c in tile.text.chars() {
                    advance += face.advance_width_sw(c, face.x_height_em())?;
                }
                let mut gx = x + (tile.width - advance) / 2.0 * ratio;
                for c in tile.text.chars() {
                    let (mut min_x, mut max_x) = (f64::MAX, f64::MIN);
                    for contour in face.glyph_contours_em(c)? {
                        let pts: Vec<Pt2> = contour
                            .iter()
                            .map(|(px, py)| Pt2::new(gx + px * em_sw, baseline + py * em_sw))
                            .collect();
                        for q in &pts {
                            min_x = min_x.min(q.x);
                            max_x = max_x.max(q.x);
                        }
                        contours.push(pts);
                    }
                    if min_x <= max_x {
                        boxes.push(GlyphBox {
                            text: c,
                            min_x,
                            max_x,
                        });
                    }
                    gx += face.advance_width_sw(c, face.x_height_em())? * ratio;
                }
                x += tile.width * ratio;
            }
        }
        Ok((contours, boxes))
    }
}

/// A number with at most `decimals` decimal places and no trailing zeros (the C# `0.###`
/// custom format).
fn format_trimmed(value: f64, decimals: usize) -> String {
    let s = format!("{value:.decimals$}");
    if !s.contains('.') {
        return s;
    }
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::tests::{heavy, medium};
    use crate::model::Colour;
    use crate::pack::tests::uk_pack;

    fn outline(text: &str, x_height: f64) -> (Vec<Vec<Pt2>>, Vec<Diagnostic>) {
        let (pack, m, h) = (uk_pack(), medium(), heavy());
        let measurer = TextTileMeasurer::new(&m, &pack.panel, Some(&h));
        let outliner = TextOutliner {
            measurer: &measurer,
            medium: &m,
            heavy: &h,
            panel: &pack.panel,
        };
        let run = TextRun {
            origin: Pt2::new(10.0, 20.0),
            text: text.into(),
            face: FontFace::TransportMedium,
            x_height,
            ink: Some(Colour::White),
        };
        let mut diags = Vec::new();
        let out = outliner.apply(vec![Prim::Text(run)], &mut diags).unwrap();
        match out.into_iter().next().unwrap() {
            Primitive::Glyphs { contours, .. } => (contours, diags),
            other => panic!("expected glyphs, got {other:?}"),
        }
    }

    fn y_range(contours: &[Vec<Pt2>]) -> (f64, f64) {
        contours
            .iter()
            .flatten()
            .fold((f64::MAX, f64::MIN), |(lo, hi), p| {
                (lo.min(p.y), hi.max(p.y))
            })
    }

    /// Figure 2-1: on a 4 sw x-height the baseline is 2 sw above the tile foot, lower-case
    /// letters are 4 sw tall and capitals 5.6 sw.
    #[test]
    fn letters_sit_on_the_tile_baseline_at_the_run_x_height() {
        let (x, _) = outline("x", 4.0);
        let (lo, hi) = y_range(&x);
        // Transport Medium draws the foot of x 4 units (0.015 sw) above the baseline.
        assert!(
            (lo - 22.0).abs() < 0.02 && (hi - 26.0).abs() < 1e-9,
            "{lo} {hi}"
        );
        let (h, _) = outline("H", 4.0);
        let (lo, hi) = y_range(&h);
        assert!(
            (lo - 22.0).abs() < 1e-9 && (hi - 27.6).abs() < 0.01,
            "{lo} {hi}"
        );
    }

    #[test]
    fn a_larger_x_height_scales_about_the_tile_foot() {
        let (h, _) = outline("H", 8.0);
        let (lo, hi) = y_range(&h);
        assert!(
            (lo - 24.0).abs() < 1e-9 && (hi - 35.2).abs() < 0.02,
            "{lo} {hi}"
        );
    }

    /// Each glyph is centred in its Appendix C tile: Medium 'H' is 6.4 sw wide.
    #[test]
    fn a_glyph_is_centred_in_its_tile() {
        let (h, _) = outline("H", 4.0);
        let (lo, hi) = h
            .iter()
            .flatten()
            .fold((f64::MAX, f64::MIN), |(lo, hi), p| {
                (lo.min(p.x), hi.max(p.x))
            });
        // 'H' is symmetric, so its ink is centred on the tile centre.
        assert!(
            ((lo + hi) / 2.0 - (10.0 + 6.4 / 2.0)).abs() < 0.02,
            "{lo} {hi}"
        );
    }

    #[test]
    fn ordinary_words_raise_no_overlap_or_width_warnings() {
        let (contours, diags) = outline("Fort Portal", 4.0);
        assert!(contours.len() >= 10);
        assert!(diags.is_empty(), "{diags:?}");
        assert!(outline("", 4.0).0.is_empty());
    }

    #[test]
    fn numbers_are_formatted_like_the_csharp_custom_format() {
        assert_eq!(format_trimmed(0.123456, 3), "0.123");
        assert_eq!(format_trimmed(0.1, 3), "0.1");
        assert_eq!(format_trimmed(2.0, 2), "2");
        assert_eq!(format_trimmed(1.25, 2), "1.25");
    }
}
