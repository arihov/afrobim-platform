//! The layout's own primitives, before lettering is turned into glyph outlines.

use afrobim_draw::Pt2;

use crate::model::{Colour, FontFace};

/// A line of lettering placed on its character tile. `origin` is the left end of the tile
/// **foot**, not the letter baseline: layouts stack tiles, so the foot is the natural anchor.
/// Tile geometry (UK TSM Ch.7 §2.2.3–2.2.4 and Fig. 2-1; Uganda TSM Vol.2 App.1 §1.1) for an
/// x-height of 4 sw: the tile is 8 sw tall, lower-case letters sit 2 sw above its foot, and
/// capitals are 5.6 sw tall with 0.4 sw to spare.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextRun {
    pub origin: Pt2,
    pub text: String,
    pub face: FontFace,
    pub x_height: f64,
    pub ink: Option<Colour>,
}

impl TextRun {
    /// Tile foot to letter baseline: 2 sw on a 4 sw x-height.
    pub const BASELINE_OFFSET_PER_X_HEIGHT: f64 = 0.5;
    /// Capital height: 5.6 sw on a 4 sw x-height.
    pub const CAP_HEIGHT_PER_X_HEIGHT: f64 = 1.4;
    /// Character tile height: 8 sw on a 4 sw x-height.
    pub const TILE_HEIGHT_PER_X_HEIGHT: f64 = 2.0;

    /// Left end of the letter baseline.
    pub fn baseline(&self) -> Pt2 {
        Pt2::new(
            self.origin.x,
            self.origin.y + self.x_height * Self::BASELINE_OFFSET_PER_X_HEIGHT,
        )
    }

    pub fn tile_height(&self) -> f64 {
        self.x_height * Self::TILE_HEIGHT_PER_X_HEIGHT
    }

    /// Builds a run from a baseline point and capital height, as a CAD TEXT entity describes
    /// lettering. Inverse of [`TextRun::baseline`].
    pub fn from_baseline(
        baseline: Pt2,
        text: &str,
        face: FontFace,
        cap_height: f64,
        ink: Option<Colour>,
    ) -> TextRun {
        let x_height = cap_height / Self::CAP_HEIGHT_PER_X_HEIGHT;
        let origin = Pt2::new(
            baseline.x,
            baseline.y - x_height * Self::BASELINE_OFFSET_PER_X_HEIGHT,
        );
        TextRun {
            origin,
            text: text.to_string(),
            face,
            x_height,
            ink,
        }
    }
}

/// What a layout draws. Lettering stays a [`TextRun`] until the outliner converts it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Prim {
    Fill {
        outline: Vec<Pt2>,
        colour: Colour,
    },
    Text(TextRun),
    /// A placeholder for artwork that is not resolved to geometry.
    Symbol {
        origin: Pt2,
        id: String,
        size: f64,
        rotation_deg: f64,
    },
}

/// An axis-aligned rectangle as a fill, anticlockwise from its lower-left corner.
pub(crate) fn rect(x0: f64, y0: f64, x1: f64, y1: f64, colour: Colour) -> Prim {
    Prim::Fill {
        outline: vec![
            Pt2::new(x0, y0),
            Pt2::new(x1, y0),
            Pt2::new(x1, y1),
            Pt2::new(x0, y1),
        ],
        colour,
    }
}
