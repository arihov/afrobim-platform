// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Named text styles and dimension styles.
//!
//! Every number here is a default the user edits; none is taken from a drafting standard, and no
//! standard is claimed. Sizes in a style are paper millimetres; they are multiplied by the
//! annotation scale (the drawing scale denominator, 500 for 1:500) to give model millimetres, so
//! text printed at 2.5 mm stays 2.5 mm on paper at any scale.
//!
//! An annotation element stores a copy of the style it was made with. Editing a style in the
//! document does not silently change existing annotations; [`crate::settings::restyle`] applies
//! a changed style to the annotations that use it.

use serde::{Deserialize, Serialize};

use crate::units::{AngleFormat, DisplayUnits, LengthUnit};

/// How the text height of a style or text is given.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum HeightSpec {
    /// Capital height in model millimetres.
    Model(f64),
    /// Capital height in paper millimetres at `scale` (the denominator of 1:`scale`).
    Paper { mm: f64, scale: f64 },
}

impl HeightSpec {
    /// Capital height in model millimetres.
    pub fn model_mm(&self) -> f64 {
        match *self {
            HeightSpec::Model(h) => h,
            HeightSpec::Paper { mm, scale } => mm * scale,
        }
    }
}

/// A named set of text properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub name: String,
    /// Capital height in paper millimetres. A default, editable.
    pub height_paper_mm: f64,
    /// Horizontal stretch of the lettering (1 is normal).
    pub width_factor: f64,
    pub bold: bool,
    /// Distance between baselines of consecutive lines as a multiple of the capital height.
    pub line_spacing: f64,
    /// Name of the font to ask a CAD program for. The layout always uses the Helvetica metrics
    /// of `afrobim-sheets`; this name is carried so an application can map it.
    pub font: String,
}

impl Default for TextStyle {
    /// The neutral default style: 2.5 mm capitals, normal width, line pitch 1.5 capital heights.
    fn default() -> Self {
        TextStyle { name: "Standard".into(), height_paper_mm: 2.5, width_factor: 1.0, bold: false, line_spacing: 1.5, font: "Helvetica".into() }
    }
}

/// What ends a dimension line or a leader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Terminator {
    /// A filled triangle, `arrow_size` long and a third as wide.
    Arrow,
    /// A short stroke across the line at 45 degrees, `arrow_size` long.
    Tick,
    /// A filled disc of radius `arrow_size / 3`.
    Dot,
    None,
}

/// Where the text sits on a dimension line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextPlacement {
    /// Above the line, one text gap clear of it.
    Above,
    /// On the line, which is broken around the text.
    Centred,
}

/// A named set of dimension properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimStyle {
    pub name: String,
    /// Overall scale: every paper size below is multiplied by it. Set from the annotation scale.
    pub scale: f64,
    // Sizes, paper mm. All defaults, editable.
    pub text_height_mm: f64,
    /// Clear distance between text and the dimension line, and between text and arrows.
    pub text_gap_mm: f64,
    pub arrow_size_mm: f64,
    /// Gap between a measured point and the start of its extension line.
    pub ext_offset_mm: f64,
    /// How far an extension line runs past the dimension line.
    pub ext_extend_mm: f64,
    /// Distance between dimension lines of a baseline chain.
    pub baseline_spacing_mm: f64,
    /// Size of the centre mark of radius and diameter dimensions; 0 draws none.
    pub centre_mark_mm: f64,
    pub terminator: Terminator,
    pub text_placement: TextPlacement,
    pub width_factor: f64,
    pub bold: bool,
    pub text_style: String,
    // Value formatting.
    pub length_unit: LengthUnit,
    pub length_decimals: u8,
    pub prefix: String,
    pub suffix: String,
    /// Multiplier applied to a measured length before it is shown (a drawing at a scale other
    /// than 1:1, or a unit change).
    pub scale_factor: f64,
    pub strip_trailing_zeros: bool,
    pub angle_format: AngleFormat,
    pub angle_decimals: u8,
    pub radius_prefix: String,
    pub diameter_prefix: String,
    pub arc_prefix: String,
    /// Words in front of a station and offset text, and the side letters.
    pub station_prefix: String,
    pub offset_prefix: String,
    pub left_letter: String,
    pub right_letter: String,
    pub level_prefix: String,
    pub level_decimals: u8,
    /// Decimals of the metre values of a station and an offset.
    pub station_decimals: u8,
}

impl Default for DimStyle {
    /// The neutral default dimension style. Every value is a default to be edited, not a
    /// requirement of any standard.
    fn default() -> Self {
        DimStyle {
            name: "Standard".into(),
            scale: 1.0,
            text_height_mm: 2.5,
            text_gap_mm: 0.8,
            arrow_size_mm: 2.5,
            ext_offset_mm: 1.0,
            ext_extend_mm: 1.25,
            baseline_spacing_mm: 7.0,
            centre_mark_mm: 2.5,
            terminator: Terminator::Arrow,
            text_placement: TextPlacement::Above,
            width_factor: 1.0,
            bold: false,
            text_style: "Standard".into(),
            length_unit: LengthUnit::Metre,
            length_decimals: 3,
            prefix: String::new(),
            suffix: String::new(),
            scale_factor: 1.0,
            strip_trailing_zeros: false,
            angle_format: AngleFormat::Decimal,
            angle_decimals: 2,
            radius_prefix: "R".into(),
            diameter_prefix: "\u{d8}".into(),
            arc_prefix: "arc ".into(),
            station_prefix: "STA ".into(),
            offset_prefix: "OFF ".into(),
            left_letter: "L".into(),
            right_letter: "R".into(),
            level_prefix: String::new(),
            level_decimals: 3,
            station_decimals: 3,
        }
    }
}

impl DimStyle {
    /// A paper size in model millimetres.
    pub fn m(&self, paper_mm: f64) -> f64 {
        paper_mm * self.scale
    }

    /// A measured length in model mm as dimension text: scaled, converted, rounded, with the
    /// prefix and suffix.
    pub fn format_length(&self, mm: f64) -> String {
        let v = mm * self.scale_factor / self.length_unit.mm_per_unit();
        let mut s = crate::units::fixed(v, self.length_decimals);
        if self.strip_trailing_zeros && s.contains('.') {
            s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        }
        format!("{}{}{}", self.prefix, s, self.suffix)
    }

    /// A measured angle in degrees as dimension text.
    pub fn format_angle(&self, deg: f64) -> String {
        DisplayUnits { angle: self.angle_format, angle_decimals: self.angle_decimals, ..DisplayUnits::default() }.angle(deg)
    }

    /// The text of an override with `<>` replaced by the measured text, or the measured text.
    pub fn with_override(measured: &str, over: Option<&str>) -> String {
        match over {
            Some(o) if !o.is_empty() => o.replace("<>", measured),
            _ => measured.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_height_scales_to_model() {
        assert_eq!(HeightSpec::Paper { mm: 2.5, scale: 500.0 }.model_mm(), 1250.0);
        assert_eq!(HeightSpec::Model(300.0).model_mm(), 300.0);
        let st = DimStyle { scale: 500.0, ..DimStyle::default() };
        assert_eq!(st.m(st.text_height_mm), 1250.0);
    }

    #[test]
    fn length_formatting_applies_scale_factor_prefix_and_suffix() {
        let mut st = DimStyle::default();
        assert_eq!(st.format_length(12500.0), "12.500");
        st.length_unit = LengthUnit::Millimetre;
        st.length_decimals = 1;
        st.prefix = "(".into();
        st.suffix = ")".into();
        st.scale_factor = 0.5;
        assert_eq!(st.format_length(1001.0), "(500.5)");
        st.strip_trailing_zeros = true;
        st.length_decimals = 3;
        st.prefix.clear();
        st.suffix.clear();
        st.scale_factor = 1.0;
        assert_eq!(st.format_length(1500.0), "1500");
        assert_eq!(st.format_length(1500.5), "1500.5");
    }

    #[test]
    fn override_substitutes_the_measured_value() {
        assert_eq!(DimStyle::with_override("3.000", None), "3.000");
        assert_eq!(DimStyle::with_override("3.000", Some("")), "3.000");
        assert_eq!(DimStyle::with_override("3.000", Some("<> TYP")), "3.000 TYP");
        assert_eq!(DimStyle::with_override("3.000", Some("EXISTING")), "EXISTING");
    }
}
