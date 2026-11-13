//! Measures legends on their character tiles. Port of C# `TextTileMeasurer`.
//!
//! Transport Medium (white lettering on dark grounds) and Transport Heavy (black lettering on
//! light grounds) have their own tile widths (UK TSM Chapter 7 §2.3.1: tile widths are listed
//! per alphabet), so each legend is measured in the face it is drawn in.
//!
//! Where the pack carries the standard's tile table (`panel.tileWidths`: UK TSM Chapter 7
//! Appendix C Tables 1 and 3), that table is the authority: each character takes its printed
//! width, with the pair rules applied to the next character in the same word (T, V and Y
//! narrow before a, e, g, o, r, u; W widens before T, V, X, Y, Z). A character the table does
//! not list falls back to the font's advance at the x-height scale and
//! [`TextTileMeasurer::font_fallbacks`] names it, so the layout can say so. A pack without a
//! table measures every character by the font.
//!
//! Without a Heavy font, Heavy legends still take the Heavy table, and fall back to Medium
//! advances for untabled characters.

use crate::font::FontMetrics;
use crate::model::FontFace;
use crate::pack::{PanelRules, TileWidthTable};
use crate::SignError;

/// App.1 §1.1: characters sit on imaginary tiles of fixed height, varying width. In sw.
#[derive(Debug, Clone, PartialEq)]
pub struct TextTile {
    pub text: String,
    pub width: f64,
    pub height: f64,
}

/// One character tile of a word: the characters it holds (usually one; a standard may print a
/// multi-character tile such as Appendix C's feet mark `'-`), its width in sw at a 4 sw
/// x-height, and whether that width came from the pack's tile table or from the font.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphTile {
    pub text: String,
    pub width: f64,
    pub from_table: bool,
}

#[derive(Debug, Clone)]
pub struct TextTileMeasurer<'a> {
    font: &'a FontMetrics,
    x_height_em: f64,
    heavy: Option<&'a FontMetrics>,
    panel: &'a PanelRules,
}

impl<'a> TextTileMeasurer<'a> {
    /// `font` is Transport Medium, the face for white lettering, measured at its own x-height.
    /// `heavy` is Transport Heavy, the face for black lettering, also measured at its own
    /// x-height, so both faces share the same 4 sw x-height.
    pub fn new(
        font: &'a FontMetrics,
        panel: &'a PanelRules,
        heavy: Option<&'a FontMetrics>,
    ) -> Self {
        TextTileMeasurer {
            font,
            x_height_em: font.x_height_em(),
            heavy,
            panel,
        }
    }

    /// Measures in the face the legend is drawn in.
    pub fn measure(&self, text: &str, face: FontFace) -> Result<TextTile, SignError> {
        let height = self.panel.text_tile_height;
        if text.is_empty() {
            return Ok(TextTile {
                text: text.to_string(),
                width: 0.0,
                height,
            });
        }

        let (font, x_height_em) = self.font_for(face);
        let table = table_for(self.panel, face);
        let words = words(text);
        let mut total = 0.0;

        for word in &words {
            for tile in tiles(word, table, font, x_height_em)? {
                total += tile.width;
            }
        }

        // App.1 §1.3: word spacing between words only, not trailing
        if words.len() > 1 {
            total += self.panel.word_spacing * (words.len() - 1) as f64;
        }

        Ok(TextTile {
            text: text.to_string(),
            width: total,
            height,
        })
    }

    /// A legend's tile width in the face it is drawn in.
    pub fn width(&self, text: &str, face: FontFace) -> Result<f64, SignError> {
        Ok(self.measure(text, face)?.width)
    }

    /// The characters of `text` whose width is not in the pack's tile table for `face`, in
    /// order of first appearance. Empty when the pack has no table: then the font is the
    /// documented source for every character.
    pub fn font_fallbacks(&self, text: &str, face: FontFace) -> Result<Vec<char>, SignError> {
        let Some(table) = table_for(self.panel, face) else {
            return Ok(Vec::new());
        };
        let (font, x_height_em) = self.font_for(face);
        let mut missing = Vec::new();
        for word in words(text) {
            for tile in tiles(word, Some(table), font, x_height_em)? {
                if tile.from_table {
                    continue;
                }
                for c in tile.text.chars() {
                    if !missing.contains(&c) {
                        missing.push(c);
                    }
                }
            }
        }
        Ok(missing)
    }

    fn font_for(&self, face: FontFace) -> (&'a FontMetrics, f64) {
        match (face, self.heavy) {
            (FontFace::TransportHeavy, Some(h)) => (h, h.x_height_em()),
            _ => (self.font, self.x_height_em),
        }
    }
}

/// The words of a legend: split on spaces, empty entries dropped.
pub(crate) fn words(text: &str) -> Vec<&str> {
    text.split(' ').filter(|w| !w.is_empty()).collect()
}

/// The pack's tile table for a face, or `None` when it has none.
pub fn table_for(panel: &PanelRules, face: FontFace) -> Option<&TileWidthTable> {
    let tables = panel.tile_widths.as_ref()?;
    match face {
        FontFace::TransportHeavy => tables.transport_heavy.as_ref(),
        FontFace::TransportMedium => tables.transport_medium.as_ref(),
    }
}

/// Splits one word (no spaces) into its tiles. With a table: the longest multi-character key
/// that matches comes first, then a pair rule on the next character of the word, then the
/// character's own width; otherwise the font's advance at `x_height_em`. The layout's measurer
/// and the outliner both tile words here, so they cannot disagree.
pub fn tiles(
    word: &str,
    table: Option<&TileWidthTable>,
    font: &FontMetrics,
    x_height_em: f64,
) -> Result<Vec<GlyphTile>, SignError> {
    let chars: Vec<char> = word.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let longest = table.map_or(1, |t| t.longest_key());
    let mut i = 0;
    'next: while i < chars.len() {
        if let Some(table) = table {
            let mut len = longest.min(chars.len() - i);
            while len >= 2 {
                let key: String = chars[i..i + len].iter().collect();
                if let Some(w) = table.widths.get(&key) {
                    out.push(GlyphTile {
                        text: key,
                        width: *w,
                        from_table: true,
                    });
                    i += len;
                    continue 'next;
                }
                len -= 1;
            }

            let c = chars[i];
            if let Some(next) = chars.get(i + 1) {
                let rule = table
                    .pair_rules
                    .iter()
                    .flatten()
                    .find(|r| r.character.starts_with(c) && r.followed_by.contains(*next));
                if let Some(rule) = rule {
                    out.push(GlyphTile {
                        text: c.to_string(),
                        width: rule.width,
                        from_table: true,
                    });
                    i += 1;
                    continue;
                }
            }
            if let Some(w) = table.widths.get(c.to_string().as_str()) {
                out.push(GlyphTile {
                    text: c.to_string(),
                    width: *w,
                    from_table: true,
                });
                i += 1;
                continue;
            }
        }
        out.push(GlyphTile {
            text: chars[i].to_string(),
            width: font.advance_width_sw(chars[i], x_height_em)?,
            from_table: false,
        });
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::tests::{heavy, medium};
    use crate::pack::tests::uk_pack;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// Appendix C Table 1: K 5.52, a 4.44, s 3.88, e 4.36.
    #[test]
    fn a_word_is_the_sum_of_its_tabled_tiles() {
        let (pack, m, h) = (uk_pack(), medium(), heavy());
        let measurer = TextTileMeasurer::new(&m, &pack.panel, Some(&h));
        let t = measurer
            .measure("Kasese", FontFace::TransportMedium)
            .unwrap();
        assert!(
            close(t.width, 5.52 + 4.44 + 3.88 + 4.36 + 3.88 + 4.36),
            "{}",
            t.width
        );
        assert_eq!(t.height, 8.0);
    }

    /// Appendix C Table 3 differs from Table 1: each face takes its own widths.
    #[test]
    fn heavy_legends_take_the_heavy_table() {
        let (pack, m, h) = (uk_pack(), medium(), heavy());
        let measurer = TextTileMeasurer::new(&m, &pack.panel, Some(&h));
        assert!(close(
            measurer.width("Kasese", FontFace::TransportHeavy).unwrap(),
            5.52 + 4.44 + 4.0 + 4.4 + 4.0 + 4.4
        ));
        // Still the Heavy table when no Heavy font is loaded.
        let no_heavy = TextTileMeasurer::new(&m, &pack.panel, None);
        assert!(close(
            no_heavy.width("Kasese", FontFace::TransportHeavy).unwrap(),
            5.52 + 4.44 + 4.0 + 4.4 + 4.0 + 4.4
        ));
    }

    /// App.1 §1.3: 2.5 sw between words, none trailing; repeated spaces count once.
    #[test]
    fn word_spacing_goes_between_words_only() {
        let (pack, m) = (uk_pack(), medium());
        let measurer = TextTileMeasurer::new(&m, &pack.panel, None);
        let one = measurer.width("Fort", FontFace::TransportMedium).unwrap();
        let other = measurer.width("Portal", FontFace::TransportMedium).unwrap();
        assert!(close(
            measurer
                .width("Fort Portal", FontFace::TransportMedium)
                .unwrap(),
            one + 2.5 + other
        ));
        assert!(close(
            measurer
                .width(" Fort   Portal ", FontFace::TransportMedium)
                .unwrap(),
            one + 2.5 + other
        ));
        assert_eq!(measurer.width("", FontFace::TransportMedium).unwrap(), 0.0);
    }

    /// Appendix C: T, V, Y narrow before a, e, g, o, r, u; W widens before T, V, X, Y, Z.
    #[test]
    fn pair_rules_apply_to_the_next_character_of_the_same_word() {
        let (pack, m) = (uk_pack(), medium());
        let table = table_for(&pack.panel, FontFace::TransportMedium);
        let w = |word: &str| {
            tiles(word, table, &m, m.x_height_em())
                .unwrap()
                .iter()
                .map(|t| t.width)
                .collect::<Vec<_>>()
        };
        assert_eq!(w("Ta"), vec![3.92, 4.44]);
        assert_eq!(w("Th"), vec![4.36, 4.48]);
        assert_eq!(w("T"), vec![4.36]); // nothing follows
        assert_eq!(w("WY"), vec![7.56, 4.92]);
        assert_eq!(w("Wa"), vec![7.32, 4.44]);
    }

    /// Appendix C's feet mark "'-" is one 3.4 sw tile, not ' (1.56) and - (2.64).
    #[test]
    fn a_multi_character_tile_wins_over_its_parts() {
        let (pack, m) = (uk_pack(), medium());
        let table = table_for(&pack.panel, FontFace::TransportMedium);
        let t = tiles("14'-6", table, &m, m.x_height_em()).unwrap();
        assert_eq!(
            t.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(),
            vec!["1", "4", "'-", "6"]
        );
        assert_eq!(t[2].width, 3.4);
        assert!(t.iter().all(|t| t.from_table));
    }

    #[test]
    fn an_untabled_character_takes_the_font_advance_and_is_reported() {
        let (pack, m) = (uk_pack(), medium());
        let measurer = TextTileMeasurer::new(&m, &pack.panel, None);
        let table = table_for(&pack.panel, FontFace::TransportMedium);
        let t = tiles("a+", table, &m, m.x_height_em()).unwrap();
        assert!(!t[1].from_table);
        assert!(close(
            t[1].width,
            m.advance_width_em('+').unwrap() * 4.0 / m.x_height_em()
        ));
        assert_eq!(
            measurer
                .font_fallbacks("a+b ++", FontFace::TransportMedium)
                .unwrap(),
            vec!['+']
        );
        assert!(measurer
            .font_fallbacks("Kasese", FontFace::TransportMedium)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn without_a_table_every_character_is_measured_by_the_font() {
        let (mut pack, m) = (uk_pack(), medium());
        pack.panel.tile_widths = None;
        let measurer = TextTileMeasurer::new(&m, &pack.panel, None);
        let by_font: f64 = "Kasese"
            .chars()
            .map(|c| m.advance_width_sw(c, m.x_height_em()).unwrap())
            .sum();
        assert!(close(
            measurer.width("Kasese", FontFace::TransportMedium).unwrap(),
            by_font
        ));
        assert!(measurer
            .font_fallbacks("Kasese", FontFace::TransportMedium)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_character_with_no_tile_and_no_glyph_is_an_error() {
        let (pack, m) = (uk_pack(), medium());
        let measurer = TextTileMeasurer::new(&m, &pack.panel, None);
        assert!(matches!(
            measurer.measure("Kasese \u{4E00}", FontFace::TransportMedium),
            Err(SignError::MissingGlyph {
                character: '\u{4E00}',
                ..
            })
        ));
    }
}
