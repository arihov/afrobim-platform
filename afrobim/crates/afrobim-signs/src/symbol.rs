// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Pictogram placement inside a sign panel (App.1 §2.1.4). Port of C# `SymbolResolver`.
//!
//! The parking "P" is drawn here; every other symbol is real vector artwork from the
//! [`crate::pictogram_library`] when a glyph exists, otherwise a positioned placeholder for the
//! renderer to swap in.

use afrobim_draw::Pt2;

use crate::measurer::TextTileMeasurer;
use crate::model::{Colour, Direction, FontFace};
use crate::pictogram_library;
use crate::prim::{rect, Prim, TextRun};
use crate::SignError;

/// Angle (deg) a symbol points to match its row's arrow, under the convention 0° = up/ahead,
/// +90° = left, −90° = right.
pub(crate) fn rotation_for(dir: Direction) -> f64 {
    match dir {
        Direction::Left => 90.0,
        Direction::Right => -90.0,
        Direction::Ahead => 0.0,
    }
}

/// The aircraft "is rotated to point in the same direction as the route arm or arrow, but it
/// should never point below the horizontal" (App.1 §2.1.4). The lower hemisphere is any
/// bearing beyond ±90° from vertical, which is folded back onto the nearest horizontal.
pub(crate) fn clamp_not_below_horizontal(rot_deg: f64) -> f64 {
    let a = ((rot_deg + 180.0) % 360.0 + 360.0) % 360.0 - 180.0;
    // would aim down-left → due left; down-right → due right
    a.clamp(-90.0, 90.0)
}

/// Resolves a symbol inside a `size`-square box whose bottom-left corner is `box_min`, drawn
/// in black.
pub(crate) fn resolve(
    symbol_id: &str,
    size: f64,
    box_min: Pt2,
    rotation_deg: f64,
    measurer: &TextTileMeasurer,
) -> Result<Vec<Prim>, SignError> {
    resolve_inked(
        symbol_id,
        size,
        box_min,
        rotation_deg,
        measurer,
        Colour::Black,
    )
}

/// As [`resolve`], with the glyph drawn in `ink` (unless the glyph fixes its own colour).
pub(crate) fn resolve_inked(
    symbol_id: &str,
    size: f64,
    box_min: Pt2,
    rotation_deg: f64,
    measurer: &TextTileMeasurer,
    ink: Colour,
) -> Result<Vec<Prim>, SignError> {
    let key = symbol_id.trim().to_lowercase();
    match key.as_str() {
        "parking-p" | "p" => parking_square(size, box_min, measurer),
        _ => Ok(
            pictogram_library::try_build(symbol_id, size, box_min, rotation_deg, ink)
                .unwrap_or_else(|| {
                    vec![Prim::Symbol {
                        origin: box_min,
                        id: symbol_id.to_string(),
                        size,
                        rotation_deg,
                    }]
                }),
        ),
    }
}

/// White letter "P" on a blue square (App.1 §2.1.4).
fn parking_square(
    s: f64,
    box_min: Pt2,
    measurer: &TextTileMeasurer,
) -> Result<Vec<Prim>, SignError> {
    let (x, y) = (box_min.x, box_min.y);
    let square = rect(x, y, x + s, y + s, Colour::Blue);

    // Capital height ≈ half the box, centred within it. Advance width is taken from the
    // measurer at the tile x-height (4 sw) and scaled linearly to the run's x-height.
    let cap = s / 2.0;
    let xh = cap / TextRun::CAP_HEIGHT_PER_X_HEIGHT;
    let pw = measurer.width("P", FontFace::TransportHeavy)? * (xh / 4.0);
    let px = x + (s - pw) / 2.0;
    let py = y + (s - cap) / 2.0; // letter baseline

    let letter = TextRun::from_baseline(
        Pt2::new(px, py),
        "P",
        FontFace::TransportHeavy,
        cap,
        Some(Colour::White),
    );
    Ok(vec![square, Prim::Text(letter)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_symbol_never_points_below_the_horizontal() {
        assert_eq!(clamp_not_below_horizontal(0.0), 0.0);
        assert_eq!(clamp_not_below_horizontal(90.0), 90.0);
        assert_eq!(clamp_not_below_horizontal(-90.0), -90.0);
        assert_eq!(clamp_not_below_horizontal(135.0), 90.0);
        assert_eq!(clamp_not_below_horizontal(-135.0), -90.0);
        assert_eq!(clamp_not_below_horizontal(45.0), 45.0);
        assert_eq!(clamp_not_below_horizontal(360.0), 0.0);
    }
}
