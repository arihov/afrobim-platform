// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! A small single-stroke lettering set, so that the cross-section carries its own labels.
//!
//! The draw crate has no text primitive (its `Glyphs` are outlines of a loaded font), so labels
//! are drawn here as plain lines: capitals, digits and a few signs on a 4 × 6 grid. Lower case is
//! drawn as capitals; an unknown character is a small box. This is drafting lettering of this
//! crate's own making and carries no standard.

use afrobim_draw::{Primitive, Pt2};

/// Advance from one character to the next, as a fraction of the text height.
pub(crate) const ADVANCE: f64 = 5.5 / 6.0;

/// Strokes of a character: each stroke is a run of points written as digit pairs `xy` on a grid
/// with x in 0..=4 and y in 0..=6.
fn strokes(c: char) -> &'static [&'static str] {
    match c.to_ascii_uppercase() {
        '0' | 'O' => &["103041453616050110"],
        '1' => &["152620", "1030"],
        '2' => &["05163645440040"],
        '3' => &["05163645443313", "334241301001"],
        '4' => &["30360242"],
        '5' => &["460603334241301001"],
        '6' => &["36160501103041423303"],
        '7' => &["064610"],
        '8' => &["13040516364544331302011030414233"],
        '9' => &["10304145361605041343"],
        'A' => &["0004264440", "0242"],
        'B' => &["00063645443303", "3342413000"],
        'C' => &["4536160501103041"],
        'D' => &["00063645413000"],
        'E' => &["46060040", "0333"],
        'F' => &["000646", "0333"],
        'G' => &["45361605011030414323"],
        'H' => &["0006", "4046", "0343"],
        'I' => &["1030", "2026", "1636"],
        'J' => &["0110304146"],
        'K' => &["0006", "460340"],
        'L' => &["060040"],
        'M' => &["0006234640"],
        'N' => &["00064046"],
        'P' => &["00063645443303"],
        'Q' => &["103041453616050110", "2240"],
        'R' => &["00063645443303", "2340"],
        'S' => &["453616050413334241301001"],
        'T' => &["0646", "2026"],
        'U' => &["060110304146"],
        'V' => &["062046"],
        'W' => &["0610233046"],
        'X' => &["0046", "0640"],
        'Y' => &["062346", "2320"],
        'Z' => &["06460040"],
        '.' => &["2021"],
        ',' => &["2110"],
        '-' => &["1333"],
        '/' => &["0046"],
        '%' => &["0046", "0406161404", "3032424030"],
        '(' => &["36242230"],
        ')' => &["16242210"],
        '>' => &["054301"],
        '<' => &["450341"],
        '=' => &["0444", "0242"],
        ':' => &["2122", "2425"],
        '+' => &["1343", "2125"],
        '×' | '*' => &["0145", "0541"],
        ' ' => &[],
        _ => &["1114343111"],
    }
}

/// Width of a line of text at the given height.
pub(crate) fn width(text: &str, height: f64) -> f64 {
    let n = text.chars().count() as f64;
    if n == 0.0 {
        0.0
    } else {
        (n * ADVANCE - (ADVANCE - 4.0 / 6.0)) * height
    }
}

/// The text as lines, its lower-left corner at `at`.
pub(crate) fn draw(text: &str, at: Pt2, height: f64, out: &mut Vec<Primitive>) {
    let unit = height / 6.0;
    let weight = height / 10.0;
    for (i, c) in text.chars().enumerate() {
        let x0 = at.x + i as f64 * ADVANCE * height;
        for stroke in strokes(c) {
            let pts: Vec<Pt2> = stroke
                .as_bytes()
                .chunks(2)
                .map(|p| Pt2::new(x0 + f64::from(p[0] - b'0') * unit, at.y + f64::from(p[1] - b'0') * unit))
                .collect();
            for w in pts.windows(2) {
                out.push(Primitive::Line { from: w[0], to: w[1], width: weight, colour: None });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stroke_is_on_the_grid() {
        for c in (32u8..127).map(char::from).chain(['×']) {
            for s in strokes(c) {
                assert!(s.len() % 2 == 0 && s.len() >= 4, "{c}: {s}");
                for p in s.as_bytes().chunks(2) {
                    assert!((b'0'..=b'4').contains(&p[0]) && (b'0'..=b'6').contains(&p[1]), "{c}: {s}");
                }
            }
        }
    }

    #[test]
    fn text_stays_inside_its_box() {
        let mut out = Vec::new();
        draw("Base 150 mm", Pt2::new(100.0, 50.0), 60.0, &mut out);
        assert!(!out.is_empty());
        let w = width("Base 150 mm", 60.0);
        for p in &out {
            let Primitive::Line { from, to, .. } = p else { unreachable!() };
            for q in [from, to] {
                assert!(q.x >= 100.0 - 1e-9 && q.x <= 100.0 + w + 1e-9 && q.y >= 50.0 - 1e-9 && q.y <= 110.0 + 1e-9);
            }
        }
    }
}
