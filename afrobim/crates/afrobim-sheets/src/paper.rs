// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Paper sizes, page setup, frame and drawing scale.

use serde::{Deserialize, Serialize};

use crate::geom::Rect;

/// An ISO 216 A-series size, A0 to A4.
///
/// Derivation (ISO 216, with the ISO 5457 rule that sizes are whole millimetres): A0 has an area
/// of 1 m² and sides in the ratio 1 : √2. With short side `w` and long side `h` this gives
///
/// ```text
///   w h = 1 m²,   h = √2 w   =>   w = 2^(-1/4) m = 840.896 mm,   h = 2^(1/4) m = 1189.207 mm
/// ```
///
/// which rounds to the whole millimetres of A0. Each next size is the previous one cut in half
/// across its long side, and the long side of the previous size is halved and rounded down:
/// short(n) = floor(long(n-1) / 2) and long(n) = short(n-1). Nothing is typed in from a table:
/// [`PaperSize::dimensions_mm`] runs this derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaperSize {
    A0,
    A1,
    A2,
    A3,
    A4,
}

impl PaperSize {
    pub const ALL: [PaperSize; 5] = [PaperSize::A0, PaperSize::A1, PaperSize::A2, PaperSize::A3, PaperSize::A4];

    /// The `n` of `A<n>`.
    pub fn series_number(self) -> u32 {
        match self {
            PaperSize::A0 => 0,
            PaperSize::A1 => 1,
            PaperSize::A2 => 2,
            PaperSize::A3 => 3,
            PaperSize::A4 => 4,
        }
    }

    pub fn name(self) -> String {
        format!("A{}", self.series_number())
    }

    /// (short side, long side) in whole millimetres, from the derivation in the type
    /// documentation.
    pub fn dimensions_mm(self) -> (f64, f64) {
        let area_mm2: f64 = 1.0e6;
        // w h = area and h = sqrt(2) w, so w = sqrt(area / sqrt 2).
        let w0 = (area_mm2 / 2f64.sqrt()).sqrt();
        let h0 = w0 * 2f64.sqrt();
        let (mut short, mut long) = (w0.round(), h0.round());
        for _ in 0..self.series_number() {
            let next_short = (long / 2.0).floor();
            long = short;
            short = next_short;
        }
        (short, long)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Orientation {
    #[default]
    Landscape,
    Portrait,
}

/// Margins from the paper edge to the frame, mm. The user enters them; [`Margins::default`] is a
/// neutral starting value, not a requirement of any standard.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Margins {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

impl Margins {
    pub fn uniform(mm: f64) -> Margins {
        Margins { left: mm, right: mm, top: mm, bottom: mm }
    }
}

impl Default for Margins {
    /// Default only: 10 mm on every side. Change it to suit the office or the contract.
    fn default() -> Self {
        Margins::uniform(10.0)
    }
}

/// How the frame (the border of the drawing area) is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrameStyle {
    pub draw_frame: bool,
    /// Also draw the paper edge (a hairline at the sheet boundary), useful in DXF.
    pub draw_paper_edge: bool,
}

impl Default for FrameStyle {
    /// Default only: a single rectangle at the margins and no paper-edge line.
    fn default() -> Self {
        FrameStyle { draw_frame: true, draw_paper_edge: false }
    }
}

/// Paper, orientation, margins and frame of a sheet.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub size: PaperSize,
    pub orientation: Orientation,
    pub margins: Margins,
    pub frame: FrameStyle,
}

impl Default for Page {
    /// Default only: A3 landscape.
    fn default() -> Self {
        Page { size: PaperSize::A3, orientation: Orientation::Landscape, margins: Margins::default(), frame: FrameStyle::default() }
    }
}

impl Page {
    pub fn new(size: PaperSize, orientation: Orientation) -> Page {
        Page { size, orientation, ..Page::default() }
    }

    /// Paper (width, height) in mm for the orientation.
    pub fn dimensions_mm(&self) -> (f64, f64) {
        let (short, long) = self.size.dimensions_mm();
        match self.orientation {
            Orientation::Landscape => (long, short),
            Orientation::Portrait => (short, long),
        }
    }

    /// The paper as a rectangle with its lower left corner on the origin.
    pub fn paper_rect(&self) -> Rect {
        let (w, h) = self.dimensions_mm();
        Rect::new(0.0, 0.0, w, h)
    }

    /// The frame: the paper less the margins.
    pub fn frame_rect(&self) -> Rect {
        let p = self.paper_rect();
        Rect::new(p.x0 + self.margins.left, p.y0 + self.margins.bottom, p.x1 - self.margins.right, p.y1 - self.margins.top)
    }
}

/// A drawing scale 1:`n`: one millimetre on paper is `n` millimetres on the ground, so one metre
/// on the ground is `1000 / n` millimetres on paper.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Scale(pub f64);

impl Scale {
    pub fn is_valid(self) -> bool {
        self.0.is_finite() && self.0 > 0.0
    }

    /// Paper millimetres per ground metre.
    pub fn paper_mm_per_m(self) -> f64 {
        1000.0 / self.0
    }

    /// Ground metres covered by a paper length in mm.
    pub fn ground_m(self, paper_mm: f64) -> f64 {
        paper_mm * self.0 / 1000.0
    }

    /// "1:500". A whole `n` prints without decimals.
    pub fn label(self) -> String {
        if (self.0 - self.0.round()).abs() < 1e-9 {
            format!("1:{}", self.0.round() as i64)
        } else {
            format!("1:{}", trim_number(self.0, 2))
        }
    }
}

/// A number with at most `decimals` decimals and no trailing zeros.
pub fn trim_number(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// The smallest of the numbers 1, 2, 5 times a power of ten that is at least `x`, so a scale
/// chosen to fit a view is a round one. A neutral rule, not a standard list.
pub fn round_up_1_2_5(x: f64) -> f64 {
    if !(x.is_finite() && x > 0.0) {
        return 1.0;
    }
    let p = 10f64.powf(x.log10().floor());
    for m in [1.0, 2.0, 5.0, 10.0] {
        if m * p >= x * (1.0 - 1e-12) {
            return m * p;
        }
    }
    10.0 * p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_series_follows_the_iso_216_derivation() {
        // Independent check straight from the definition: area 2^-n m2, ratio sqrt 2. The exact
        // sides of A<n> are 2^(-(2n+1)/4) m and 2^(-(2n-1)/4) m; the standard's whole-millimetre
        // sizes are within 1 mm of them (A0 is the nearest whole millimetre, the rest are cut
        // from it and rounded down).
        for size in PaperSize::ALL {
            let n = size.series_number() as f64;
            let (w, h) = size.dimensions_mm();
            let exact_w = 1000.0 * 2f64.powf(-(2.0 * n + 1.0) / 4.0);
            let exact_h = 1000.0 * 2f64.powf(-(2.0 * n - 1.0) / 4.0);
            assert!((w - exact_w).abs() < 1.0, "{} short {w} vs {exact_w}", size.name());
            assert!((h - exact_h).abs() < 1.0, "{} long {h} vs {exact_h}", size.name());
            assert_eq!(w.fract(), 0.0);
            assert_eq!(h.fract(), 0.0);
            // Area within 0.3 % of 2^-n m2 (whole-millimetre rounding of A4 costs 0.21 %) and the
            // ratio within rounding of sqrt 2.
            assert!((w * h / (1.0e6 * 2f64.powf(-n)) - 1.0).abs() < 0.003);
            assert!((h / w - 2f64.sqrt()).abs() < 0.01);
        }
        // Each size is half of the one before: short(n) = floor(long(n-1) / 2), long(n) = short(n-1).
        for pair in PaperSize::ALL.windows(2) {
            let (ps, pl) = pair[0].dimensions_mm();
            let (s, l) = pair[1].dimensions_mm();
            assert_eq!(l, ps);
            assert_eq!(s, (pl / 2.0).floor());
        }
        // A0 itself: 2^-1/4 and 2^1/4 metres to the nearest millimetre.
        assert_eq!(PaperSize::A0.dimensions_mm(), ((1000.0 * 2f64.powf(-0.25)).round(), (1000.0 * 2f64.powf(0.25)).round()));
    }

    #[test]
    fn orientation_swaps_the_sides_and_the_frame_follows_the_margins() {
        let mut page = Page::new(PaperSize::A3, Orientation::Landscape);
        let (s, l) = PaperSize::A3.dimensions_mm();
        assert_eq!(page.dimensions_mm(), (l, s));
        page.orientation = Orientation::Portrait;
        assert_eq!(page.dimensions_mm(), (s, l));
        page.margins = Margins { left: 20.0, right: 5.0, top: 7.0, bottom: 9.0 };
        let f = page.frame_rect();
        assert_eq!((f.x0, f.y0, f.x1, f.y1), (20.0, 9.0, s - 5.0, l - 7.0));
    }

    #[test]
    fn scale_converts_between_paper_and_ground() {
        // 1:500: 1 m on the ground is 2 mm on paper; 100 mm on paper is 50 m.
        let s = Scale(500.0);
        assert_eq!(s.paper_mm_per_m(), 2.0);
        assert_eq!(s.ground_m(100.0), 50.0);
        assert_eq!(s.label(), "1:500");
        assert_eq!(Scale(2.5).label(), "1:2.5");
    }

    #[test]
    fn round_up_picks_one_two_five() {
        assert_eq!(round_up_1_2_5(0.8), 1.0);
        assert_eq!(round_up_1_2_5(1.2), 2.0);
        assert_eq!(round_up_1_2_5(230.0), 500.0);
        assert_eq!(round_up_1_2_5(500.0), 500.0);
        assert_eq!(round_up_1_2_5(501.0), 1000.0);
    }
}
