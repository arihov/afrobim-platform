//! The composed sheet: a flat list of strokes, fills and texts in paper millimetres (Y up, origin
//! at the paper's lower left corner), one layer name each. The PDF and DXF writers and the
//! on-screen [`SheetDrawing`] all read this one list.

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};

use crate::geom::Rect;
use crate::text::Align;

/// Layer names the composer gives to sheet furniture. Model geometry keeps the name of the
/// document layer it came from.
pub mod layers {
    pub const PAPER: &str = "SHEET-PAPER";
    pub const FRAME: &str = "SHEET-FRAME";
    pub const TITLE_BLOCK: &str = "SHEET-TITLEBLOCK";
    pub const TEXT: &str = "SHEET-TEXT";
    pub const NOTES: &str = "SHEET-NOTES";
    pub const TABLE: &str = "SHEET-TABLE";
    pub const TABLE_TEXT: &str = "SHEET-TABLETEXT";
    pub const PLAN_ALIGNMENT: &str = "PLAN-ALIGNMENT";
    pub const PLAN_STATIONS: &str = "PLAN-STATIONS";
    pub const PLAN_MARKS: &str = "PLAN-MARKS";
    pub const PLAN_MATCH: &str = "PLAN-MATCHLINE";
    pub const PLAN_FURNITURE: &str = "PLAN-FURNITURE";
    pub const PROFILE_GRID: &str = "PROFILE-GRID";
    pub const PROFILE_DESIGN: &str = "PROFILE-DESIGN";
    pub const PROFILE_GROUND: &str = "PROFILE-GROUND";
    pub const PROFILE_BAND: &str = "PROFILE-BAND";
    pub const PROFILE_NOTES: &str = "PROFILE-ANNOTATION";
    pub const SECTION: &str = "SECTION-LINES";
    pub const SECTION_TEXT: &str = "SECTION-TEXT";
    pub const DETAIL: &str = "DETAIL";
    pub const DETAIL_TEXT: &str = "DETAIL-TEXT";
    pub const VIEWPORT: &str = "VIEWPORT";
}

/// One thing on the sheet.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// A pen stroke along a polyline (closed when `closed`), `weight_mm` wide.
    Stroke { layer: String, points: Vec<Pt2>, closed: bool, weight_mm: f64, rgb: [u8; 3] },
    /// A filled region: the rings are filled even-odd, so an inner ring is a hole.
    Fill { layer: String, rings: Vec<Vec<Pt2>>, rgb: [u8; 3] },
    Text(SheetText),
}

/// One line of text. `at` is the baseline point the alignment refers to; `height_mm` is the
/// capital height; `rotation_deg` is anticlockwise.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetText {
    pub layer: String,
    pub at: Pt2,
    pub height_mm: f64,
    pub text: String,
    pub align: Align,
    pub rotation_deg: f64,
    pub bold: bool,
    pub rgb: [u8; 3],
}

impl Item {
    pub fn layer(&self) -> &str {
        match self {
            Item::Stroke { layer, .. } | Item::Fill { layer, .. } => layer,
            Item::Text(t) => &t.layer,
        }
    }
}

/// A sheet as composed: ready to preview or write.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposedSheet {
    /// The sheet number as printed, e.g. `ROAD-002`.
    pub number: String,
    pub title: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub items: Vec<Item>,
    /// What did not fit or could not be drawn, in words.
    pub warnings: Vec<String>,
}

impl ComposedSheet {
    pub fn new(number: &str, title: &str, width_mm: f64, height_mm: f64) -> ComposedSheet {
        ComposedSheet { number: number.to_string(), title: title.to_string(), width_mm, height_mm, items: Vec::new(), warnings: Vec::new() }
    }

    pub fn stroke(&mut self, layer: &str, points: Vec<Pt2>, closed: bool, weight_mm: f64, rgb: [u8; 3]) {
        if points.len() >= 2 {
            self.items.push(Item::Stroke { layer: layer.to_string(), points, closed, weight_mm, rgb });
        }
    }

    pub fn line(&mut self, layer: &str, a: Pt2, b: Pt2, weight_mm: f64, rgb: [u8; 3]) {
        self.stroke(layer, vec![a, b], false, weight_mm, rgb);
    }

    pub fn rect(&mut self, layer: &str, r: &Rect, weight_mm: f64, rgb: [u8; 3]) {
        self.stroke(layer, r.corners().to_vec(), true, weight_mm, rgb);
    }

    pub fn fill(&mut self, layer: &str, rings: Vec<Vec<Pt2>>, rgb: [u8; 3]) {
        if rings.iter().any(|r| r.len() >= 3) {
            self.items.push(Item::Fill { layer: layer.to_string(), rings, rgb });
        }
    }

    pub fn fill_rect(&mut self, layer: &str, r: &Rect, rgb: [u8; 3]) {
        self.fill(layer, vec![r.corners().to_vec()], rgb);
    }

    pub fn text(&mut self, t: SheetText) {
        if !t.text.trim().is_empty() && t.height_mm > 0.0 {
            self.items.push(Item::Text(t));
        }
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        let m = message.into();
        if !self.warnings.contains(&m) {
            self.warnings.push(m);
        }
    }

    pub fn texts(&self) -> impl Iterator<Item = &SheetText> {
        self.items.iter().filter_map(|i| if let Item::Text(t) = i { Some(t) } else { None })
    }

    pub fn strokes(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|i| matches!(i, Item::Stroke { .. }))
    }

    /// The sheet as a [`Drawing`] in paper millimetres plus its texts, for an on-screen preview.
    /// Strokes become `Line` primitives of their pen width (colours snapped to the nearest named
    /// colour); fills become `Fill` (a fill with holes becomes `Glyphs`, which the draw crate
    /// fills even-odd); the texts are handed over as labels because the draw crate has no text
    /// primitive.
    pub fn to_drawing(&self) -> SheetDrawing {
        let mut primitives = vec![];
        let mut labels = vec![];
        for item in &self.items {
            match item {
                Item::Stroke { points, closed, weight_mm, rgb, .. } => {
                    let colour = Some(nearest_colour(*rgb));
                    let n = points.len();
                    let segs = if *closed { n } else { n - 1 };
                    for i in 0..segs {
                        primitives.push(Primitive::Line { from: points[i], to: points[(i + 1) % n], width: *weight_mm, colour });
                    }
                }
                Item::Fill { rings, rgb, .. } => {
                    let c = nearest_colour(*rgb);
                    let rings: Vec<&Vec<Pt2>> = rings.iter().filter(|r| r.len() >= 3).collect();
                    if rings.len() == 1 {
                        primitives.push(Primitive::Fill { outline: rings[0].clone(), colour: c });
                    } else {
                        primitives.push(Primitive::Glyphs { contours: rings.into_iter().cloned().collect(), text: String::new(), ink: Some(c) });
                    }
                }
                Item::Text(t) => labels.push(SheetLabel {
                    at: t.at,
                    text: t.text.clone(),
                    height_mm: t.height_mm,
                    rotation_deg: t.rotation_deg,
                    align: t.align,
                    bold: t.bold,
                    layer: t.layer.clone(),
                }),
            }
        }
        SheetDrawing {
            drawing: Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(self.width_mm, self.height_mm) } },
            labels,
        }
    }
}

/// A text of a [`SheetDrawing`].
#[derive(Debug, Clone, PartialEq)]
pub struct SheetLabel {
    pub at: Pt2,
    pub text: String,
    /// Capital height, paper mm.
    pub height_mm: f64,
    pub rotation_deg: f64,
    pub align: Align,
    pub bold: bool,
    pub layer: String,
}

/// The composed sheet as drawing primitives and labels, in paper millimetres with the paper's
/// lower left corner on the origin.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetDrawing {
    pub drawing: Drawing,
    pub labels: Vec<SheetLabel>,
}

/// The named colour whose display RGB is nearest to `rgb`.
pub fn nearest_colour(rgb: [u8; 3]) -> Colour {
    const ALL: [Colour; 8] = [Colour::White, Colour::Black, Colour::Green, Colour::Blue, Colour::Brown, Colour::Yellow, Colour::Red, Colour::Grey];
    let d = |c: Colour| {
        let r = c.display_rgb();
        (0..3).map(|i| (r[i] as i32 - rgb[i] as i32).pow(2)).sum::<i32>()
    };
    ALL.into_iter().min_by_key(|c| d(*c)).unwrap_or(Colour::Black)
}

/// Helper for building a text item.
#[allow(clippy::too_many_arguments)]
pub fn text(layer: &str, at: Pt2, height_mm: f64, text: impl Into<String>, align: Align, rotation_deg: f64, bold: bool, rgb: [u8; 3]) -> SheetText {
    SheetText { layer: layer.to_string(), at, height_mm, text: text.into(), align, rotation_deg, bold, rgb }
}
