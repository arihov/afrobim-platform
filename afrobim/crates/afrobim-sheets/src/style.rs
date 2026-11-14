//! Line weights, text heights and the layer-to-pen mapping.
//!
//! Every value here is entered by the user. The defaults are neutral starting values chosen so a
//! sheet reads well at A3; they are not taken from a drafting standard or from any organisation,
//! and nothing here claims to meet one. Change them to the office or contract requirement.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::item::layers;

/// A pen: the stroke width on paper and the colour it prints in.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pen {
    pub weight_mm: f64,
    pub rgb: [u8; 3],
}

impl Pen {
    pub const fn new(weight_mm: f64, rgb: [u8; 3]) -> Pen {
        Pen { weight_mm, rgb }
    }
}

/// Which pen a layer is drawn with. A layer not in the table gets `default`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PenTable {
    pub pens: BTreeMap<String, Pen>,
    pub default: Pen,
}

impl PenTable {
    pub fn pen(&self, layer: &str) -> Pen {
        self.pens.get(layer).copied().unwrap_or(self.default)
    }

    pub fn set(&mut self, layer: &str, pen: Pen) {
        self.pens.insert(layer.to_string(), pen);
    }
}

impl Default for PenTable {
    /// Default values, editable: a few weights from hairline to heavy, all black except the
    /// profile ground (dark green) and the match line (dark red).
    fn default() -> Self {
        const BLACK: [u8; 3] = [0, 0, 0];
        let mut pens = BTreeMap::new();
        let mut put = |layer: &str, w: f64, rgb: [u8; 3]| {
            pens.insert(layer.to_string(), Pen::new(w, rgb));
        };
        put(layers::PAPER, 0.1, [150, 150, 150]);
        put(layers::FRAME, 0.5, BLACK);
        put(layers::TITLE_BLOCK, 0.25, BLACK);
        put(layers::TABLE, 0.18, BLACK);
        put(layers::VIEWPORT, 0.18, BLACK);
        put(layers::PLAN_ALIGNMENT, 0.35, BLACK);
        put(layers::PLAN_STATIONS, 0.18, BLACK);
        put(layers::PLAN_MATCH, 0.35, [160, 0, 0]);
        put(layers::PLAN_FURNITURE, 0.25, BLACK);
        put(layers::PROFILE_GRID, 0.1, [130, 130, 130]);
        put(layers::PROFILE_DESIGN, 0.5, BLACK);
        put(layers::PROFILE_GROUND, 0.25, [0, 100, 40]);
        put(layers::PROFILE_BAND, 0.18, BLACK);
        put(layers::PROFILE_NOTES, 0.18, BLACK);
        put(layers::SECTION, 0.18, BLACK);
        put(layers::DETAIL, 0.18, BLACK);
        PenTable { pens, default: Pen::new(0.13, BLACK) }
    }
}

/// Capital heights of the lettering, paper mm.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextHeights {
    /// The drawing title in the title block.
    pub title: f64,
    /// Headings inside views and tables.
    pub heading: f64,
    /// Body text: notes, table cells, title block values.
    pub normal: f64,
    /// Station labels, level and offset labels, dimensions.
    pub label: f64,
    /// Small print: title block field captions, scale bar numbers.
    pub small: f64,
}

impl Default for TextHeights {
    /// Default values, editable.
    fn default() -> Self {
        TextHeights { title: 5.0, heading: 3.5, normal: 2.5, label: 2.0, small: 1.8 }
    }
}

/// Pens and text heights of a sheet set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Styles {
    pub pens: PenTable,
    pub text: TextHeights,
    /// Print model geometry in the colours the modules gave it (markings, sign faces) instead of
    /// the layer's pen colour. Line widths still come from the pen unless the geometry is wider.
    pub model_colours: bool,
}

impl Default for Styles {
    fn default() -> Self {
        Styles { pens: PenTable::default(), text: TextHeights::default(), model_colours: true }
    }
}
