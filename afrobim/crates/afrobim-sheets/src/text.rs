// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Text metrics and wrapping for the sheet's built-in lettering.
//!
//! Sheets are lettered in the PDF standard Helvetica and Helvetica-Bold faces, which every PDF
//! reader has, so no font is embedded. Text heights are capital heights in paper millimetres;
//! the font size is `height / CAP_HEIGHT`. Widths come from the faces' published advance widths
//! (see `afm.rs`).

use crate::afm::{HELVETICA, HELVETICA_BOLD};

/// Capital height of Helvetica as a fraction of the font size (the AFM `CapHeight`, 718 of 1000).
pub const CAP_HEIGHT: f64 = 0.718;

/// Horizontal justification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Align {
    #[default]
    Left,
    Centre,
    Right,
}

/// The WinAnsi (code page 1252) byte for a character, `?` when it has none.
pub fn winansi(c: char) -> u8 {
    let u = c as u32;
    match u {
        0x20..=0x7E | 0xA0..=0xFF => u as u8,
        0x20AC => 0x80,
        0x201A => 0x82,
        0x0192 => 0x83,
        0x201E => 0x84,
        0x2026 => 0x85,
        0x2020 => 0x86,
        0x2021 => 0x87,
        0x02C6 => 0x88,
        0x2030 => 0x89,
        0x0160 => 0x8A,
        0x2039 => 0x8B,
        0x0152 => 0x8C,
        0x017D => 0x8E,
        0x2018 => 0x91,
        0x2019 => 0x92,
        0x201C => 0x93,
        0x201D => 0x94,
        0x2022 => 0x95,
        0x2013 => 0x96,
        0x2014 => 0x97,
        0x02DC => 0x98,
        0x2122 => 0x99,
        0x0161 => 0x9A,
        0x203A => 0x9B,
        0x0153 => 0x9C,
        0x017E => 0x9E,
        0x0178 => 0x9F,
        _ => b'?',
    }
}

/// The text as WinAnsi bytes. Line breaks and tabs become spaces; other control characters go.
pub fn encode(text: &str) -> Vec<u8> {
    text.chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(b' '),
            c if (c as u32) < 0x20 => None,
            c => Some(winansi(c)),
        })
        .collect()
}

/// Width in font units (1/1000 em) of one WinAnsi byte.
fn advance(b: u8, bold: bool) -> f64 {
    let table = if bold { &HELVETICA_BOLD } else { &HELVETICA };
    table.get((b as usize).wrapping_sub(32)).copied().unwrap_or(278) as f64
}

/// Width in mm of `text` set with capital height `height_mm`.
pub fn text_width(text: &str, height_mm: f64, bold: bool) -> f64 {
    let size = height_mm / CAP_HEIGHT;
    encode(text).iter().map(|&b| advance(b, bold)).sum::<f64>() / 1000.0 * size
}

/// Slack, mm, in width comparisons so a column sized exactly to its text does not wrap it.
const WIDTH_EPS: f64 = 1e-6;

/// Breaks `text` into lines no wider than `max_width_mm` at spaces. A word wider than a line is
/// cut between characters. Explicit line breaks in the text are kept.
pub fn wrap(text: &str, max_width_mm: f64, height_mm: f64, bold: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if text_width(&candidate, height_mm, bold) <= max_width_mm + WIDTH_EPS {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            if text_width(word, height_mm, bold) <= max_width_mm + WIDTH_EPS {
                line = word.to_string();
            } else {
                // Cut the long word.
                let mut piece = String::new();
                for ch in word.chars() {
                    let mut next = piece.clone();
                    next.push(ch);
                    if text_width(&next, height_mm, bold) > max_width_mm + WIDTH_EPS && !piece.is_empty() {
                        lines.push(std::mem::take(&mut piece));
                    }
                    piece.push(ch);
                }
                line = piece;
            }
        }
        lines.push(line);
    }
    lines
}

/// `text` shortened with `...` so that it fits `max_width_mm`; unchanged when it already fits.
pub fn fit(text: &str, max_width_mm: f64, height_mm: f64, bold: bool) -> String {
    if text_width(text, height_mm, bold) <= max_width_mm + WIDTH_EPS {
        return text.to_string();
    }
    let mut out: Vec<char> = text.chars().collect();
    while !out.is_empty() {
        out.pop();
        let s: String = out.iter().collect::<String>() + "...";
        if text_width(&s, height_mm, bold) <= max_width_mm {
            return s;
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_follow_the_face_metrics() {
        // Helvetica advances: "A" 667, "B" 667, space 278. At height 0.718 mm the font size is 1 mm,
        // so "AB A" is (667 + 667 + 278 + 667) / 1000 mm.
        let w = text_width("AB A", CAP_HEIGHT, false);
        assert!((w - 2.279).abs() < 1e-9, "{w}");
        // Bold A is wider (722).
        assert!(text_width("A", CAP_HEIGHT, true) > text_width("A", CAP_HEIGHT, false));
    }

    #[test]
    fn wrap_breaks_between_words_and_never_exceeds_the_width() {
        let text = "the quick brown fox jumps over the lazy dog";
        let lines = wrap(text, 30.0, 2.5, false);
        assert!(lines.len() > 1);
        assert_eq!(lines.join(" "), text);
        for l in &lines {
            assert!(text_width(l, 2.5, false) <= 30.0 + 1e-9, "{l}");
        }
    }

    #[test]
    fn a_word_wider_than_the_line_is_cut() {
        let lines = wrap("ABCDEFGHIJKLMNOPQRSTUVWXYZ", 10.0, 2.5, false);
        assert!(lines.len() > 1);
        assert_eq!(lines.concat(), "ABCDEFGHIJKLMNOPQRSTUVWXYZ");
        for l in &lines {
            assert!(text_width(l, 2.5, false) <= 10.0 + 1e-9);
        }
    }

    #[test]
    fn explicit_breaks_are_kept_and_fit_shortens() {
        assert_eq!(wrap("a\nb", 100.0, 2.5, false), vec!["a", "b"]);
        let f = fit("a long piece of text", 15.0, 2.5, false);
        assert!(f.ends_with("...") && text_width(&f, 2.5, false) <= 15.0);
        assert_eq!(fit("ab", 15.0, 2.5, false), "ab");
    }

    #[test]
    fn winansi_maps_latin_one_and_punctuation() {
        assert_eq!(winansi('A'), b'A');
        assert_eq!(winansi('\u{b0}'), 0xB0);
        assert_eq!(winansi('\u{2013}'), 0x96);
        assert_eq!(winansi('\u{4e2d}'), b'?');
    }
}
