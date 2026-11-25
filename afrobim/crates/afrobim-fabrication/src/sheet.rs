// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The composed fabrication sheet. Port of C# `FabricationSheet.cs`.

use afrobim_draw::{Drawing, Pt2};
use afrobim_signs::{Colour, Diagnostic, SignDefinition};

use crate::quantities::SignQuantity;

/// Project fields for the title block. Every field is free text supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetInfo {
    pub project: String,
    pub drawn_by: String,
    pub revision: String,
    /// Issue date as printed; the batch fills in today's date (yyyy-MM-dd) when blank.
    pub date: String,
    pub drawing_number: String,
}

impl Default for SheetInfo {
    fn default() -> Self {
        SheetInfo { project: String::new(), drawn_by: String::new(), revision: "A".to_string(), date: String::new(), drawing_number: String::new() }
    }
}

/// An axis-aligned box in sheet millimetres (y up, origin at the sheet's bottom-left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheetBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl SheetBox {
    pub const fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        SheetBox { min_x, min_y, max_x, max_y }
    }
    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }
    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }
    pub fn centre_x(&self) -> f64 {
        (self.min_x + self.max_x) / 2.0
    }
    pub fn centre_y(&self) -> f64 {
        (self.min_y + self.max_y) / 2.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionOrientation {
    Horizontal,
    Vertical,
}

/// One dimension on the sheet. `start` and `end` are the sheet coordinates of the two measured
/// edges (x for a horizontal dimension, y for a vertical one) and `offset` is where the
/// dimension line sits (its y, or its x). `value_mm` is the real size on the sign, which the
/// label prints.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetDimension {
    pub name: String,
    pub orientation: DimensionOrientation,
    pub start: f64,
    pub end: f64,
    pub offset: f64,
    pub value_mm: f64,
}

impl SheetDimension {
    pub fn label(&self) -> String {
        crate::composer::format_mm(self.value_mm)
    }
}

/// A drawn line on the sheet. `weight` is the plotted pen width in mm.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetLine {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub layer: &'static str,
    pub weight: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SheetTextAlign {
    #[default]
    Left,
    Centre,
    Right,
}

/// Sheet text. (`x`, `y`) is the baseline point the alignment refers to; `height` is the
/// capital height in mm.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetText {
    pub x: f64,
    pub y: f64,
    pub height: f64,
    pub text: String,
    pub layer: &'static str,
    pub align: SheetTextAlign,
    pub rotation_deg: f64,
    pub bold: bool,
}

impl SheetText {
    /// Left-aligned, upright, regular weight.
    pub fn new(x: f64, y: f64, height: f64, text: impl Into<String>, layer: &'static str) -> Self {
        SheetText { x, y, height, text: text.into(), layer, align: SheetTextAlign::Left, rotation_deg: 0.0, bold: false }
    }
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    pub fn aligned(mut self, align: SheetTextAlign) -> Self {
        self.align = align;
        self
    }
    pub fn rotated(mut self, degrees: f64) -> Self {
        self.rotation_deg = degrees;
        self
    }
}

/// A filled colour sample in the colour schedule.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetSwatch {
    pub rect: SheetBox,
    pub colour: Colour,
}

/// One row of the colour schedule. `bs381c` and `ral` are the references the rule pack gives for
/// the colour, or blank when it gives none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColourRow {
    pub role: String,
    pub colour: Colour,
    pub bs381c: String,
    pub ral: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleField {
    pub label: String,
    pub value: String,
}

/// A composed A3 fabrication sheet in sheet millimetres, ready for any renderer. The sign face
/// keeps its primitives in stroke-width space; [`FabricationSheet::to_sheet`] maps them onto the
/// sheet at `scale`. Everything else (frame, title block, dimensions, schedules) is already
/// reduced to lines, texts and swatches, so the DXF and the PDF draw the same sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct FabricationSheet {
    pub sign: SignDefinition,
    /// The sign face in stroke widths, lettering as outlines.
    pub face: Drawing,
    /// The layout's findings (warnings and errors are listed on the sheet).
    pub diagnostics: Vec<Diagnostic>,
    pub sw_mm: f64,
    /// The drawing scale denominator: the face is drawn at 1:`scale`.
    pub scale: u32,
    /// Where the face's extents land on the sheet.
    pub face_box: SheetBox,
    pub quantity: SignQuantity,
    pub reference: String,
    pub colours: Vec<ColourRow>,
    pub title_block: Vec<TitleField>,
    pub dimensions: Vec<SheetDimension>,
    pub lines: Vec<SheetLine>,
    pub texts: Vec<SheetText>,
    pub swatches: Vec<SheetSwatch>,
}

impl FabricationSheet {
    /// A3 landscape.
    pub const WIDTH_MM: f64 = 420.0;
    pub const HEIGHT_MM: f64 = 297.0;

    /// Sheet millimetres per stroke width.
    pub fn sheet_mm_per_sw(&self) -> f64 {
        self.sw_mm / f64::from(self.scale)
    }

    /// A face point (stroke widths) in sheet millimetres.
    pub fn to_sheet(&self, p: Pt2) -> Pt2 {
        let k = self.sheet_mm_per_sw();
        Pt2::new(self.face_box.min_x + (p.x - self.face.extents.min.x) * k, self.face_box.min_y + (p.y - self.face.extents.min.y) * k)
    }

    /// A face length (stroke widths) in sheet millimetres.
    pub fn length_to_sheet(&self, sw: f64) -> f64 {
        sw * self.sheet_mm_per_sw()
    }
}
