// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Colour mapping helpers: colours as data (`[u8; 3]`), never drawn here.
//!
//! The ramps and the class palette are cosmetic choices of this crate, not a standard.

use crate::buffer::PointBuffer;
use crate::error::{PointCloudError, Result};
use serde::{Deserialize, Serialize};

/// Colour stops over 0 to 1, linear in RGB between stops.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorRamp {
    pub stops: Vec<(f64, [u8; 3])>,
}

impl ColorRamp {
    /// Stops must be at least two, strictly increasing and within 0 to 1.
    pub fn new(stops: Vec<(f64, [u8; 3])>) -> Result<ColorRamp> {
        if stops.len() < 2 || stops.windows(2).any(|w| !(w[0].0 < w[1].0)) || stops.iter().any(|s| !(0.0..=1.0).contains(&s.0)) {
            return Err(PointCloudError::Invalid("a colour ramp needs two or more stops, increasing, within 0 and 1".into()));
        }
        Ok(ColorRamp { stops })
    }

    /// Blue, cyan, green, yellow, red. A cosmetic default.
    pub fn rainbow() -> ColorRamp {
        ColorRamp { stops: vec![(0.0, [0, 0, 255]), (0.25, [0, 255, 255]), (0.5, [0, 255, 0]), (0.75, [255, 255, 0]), (1.0, [255, 0, 0])] }
    }

    /// Black to white.
    pub fn grey() -> ColorRamp {
        ColorRamp { stops: vec![(0.0, [0, 0, 0]), (1.0, [255, 255, 255])] }
    }

    /// The colour at `t` in 0..=1 (clamped).
    pub fn at(&self, t: f64) -> [u8; 3] {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        let s = &self.stops;
        if t <= s[0].0 {
            return s[0].1;
        }
        for w in s.windows(2) {
            if t <= w[1].0 {
                let f = (t - w[0].0) / (w[1].0 - w[0].0);
                let mix = |k: usize| (w[0].1[k] as f64 + (w[1].1[k] as f64 - w[0].1[k] as f64) * f).round() as u8;
                return [mix(0), mix(1), mix(2)];
            }
        }
        s[s.len() - 1].1
    }
}

/// A colour for each class code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassPalette {
    pub colors: Vec<[u8; 3]>,
}

impl ClassPalette {
    /// Colours for codes 0 to 18 chosen by this crate; every other code is mid grey. The meaning of
    /// the codes depends on the file (see [`crate::classes`]); this palette only keys on the number.
    pub fn standard() -> ClassPalette {
        let mut colors = vec![[128u8, 128, 128]; 256];
        let set: [(usize, [u8; 3]); 19] = [
            (0, [160, 160, 160]), (1, [200, 200, 200]), (2, [150, 100, 50]), (3, [140, 220, 120]), (4, [60, 190, 60]), (5, [0, 120, 0]),
            (6, [220, 60, 60]), (7, [255, 0, 255]), (8, [255, 160, 0]), (9, [40, 100, 255]), (10, [90, 90, 90]), (11, [70, 70, 110]),
            (12, [255, 200, 200]), (13, [255, 255, 0]), (14, [255, 220, 0]), (15, [0, 200, 200]), (16, [200, 120, 255]), (17, [120, 80, 160]), (18, [255, 0, 100]),
        ];
        for (c, rgb) in set {
            colors[c] = rgb;
        }
        ClassPalette { colors }
    }

    pub fn color(&self, class: u8) -> [u8; 3] {
        self.colors.get(class as usize).copied().unwrap_or([128, 128, 128])
    }
}

/// How the file's colour values map to 8 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RgbScale {
    /// Values are 16-bit (LAS specification); shown as `value >> 8`.
    Bits16,
    /// Values are already 0..=255 (many writers put 8-bit colour in the 16-bit fields).
    Bits8,
    /// If no value in the points considered exceeds 255 treat as 8-bit, else 16-bit. A heuristic.
    Auto,
}

/// What to colour by. Limits are inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColorMode {
    Elevation { min: f64, max: f64, ramp: ColorRamp },
    Intensity { min: u16, max: u16, ramp: ColorRamp },
    Class { palette: ClassPalette },
    Rgb { scale: RgbScale },
    Fixed([u8; 3]),
}

/// Colours for the points at `indices` (all points when `None`), in that order. `Rgb` on a buffer
/// with no colour gives mid grey.
pub fn colorize(buf: &PointBuffer, indices: Option<&[u32]>, mode: &ColorMode) -> Vec<[u8; 3]> {
    let all: Vec<u32>;
    let idx: &[u32] = match indices {
        Some(i) => i,
        None => {
            all = (0..buf.len() as u32).collect();
            &all
        }
    };
    match mode {
        ColorMode::Elevation { min, max, ramp } => {
            let span = (max - min).max(f64::MIN_POSITIVE);
            idx.iter().map(|&i| ramp.at((buf.xyz[i as usize][2] - min) / span)).collect()
        }
        ColorMode::Intensity { min, max, ramp } => {
            let span = (*max as f64 - *min as f64).max(1.0);
            idx.iter().map(|&i| ramp.at((buf.intensity[i as usize] as f64 - *min as f64) / span)).collect()
        }
        ColorMode::Class { palette } => idx.iter().map(|&i| palette.color(buf.class[i as usize])).collect(),
        ColorMode::Fixed(c) => idx.iter().map(|_| *c).collect(),
        ColorMode::Rgb { scale } => match &buf.rgb {
            None => idx.iter().map(|_| [128, 128, 128]).collect(),
            Some(rgb) => {
                let eight = match scale {
                    RgbScale::Bits8 => true,
                    RgbScale::Bits16 => false,
                    RgbScale::Auto => idx.iter().all(|&i| rgb[i as usize].iter().all(|&v| v <= 255)),
                };
                idx.iter().map(|&i| {
                    let c = rgb[i as usize];
                    if eight { [c[0].min(255) as u8, c[1].min(255) as u8, c[2].min(255) as u8] } else { [(c[0] >> 8) as u8, (c[1] >> 8) as u8, (c[2] >> 8) as u8] }
                }).collect()
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::PointRecord;

    #[test]
    fn ramp_interpolates_and_clamps() {
        let r = ColorRamp::grey();
        assert_eq!(r.at(0.0), [0, 0, 0]);
        assert_eq!(r.at(0.5), [128, 128, 128]);
        assert_eq!(r.at(7.0), [255, 255, 255]);
        assert!(ColorRamp::new(vec![(0.0, [0; 3])]).is_err());
        assert!(ColorRamp::new(vec![(0.5, [0; 3]), (0.5, [1; 3])]).is_err());
    }

    #[test]
    fn modes_return_colours_as_data() {
        let mut b = PointBuffer::new(true, false);
        b.push(&PointRecord { xyz: [0.0, 0.0, 10.0], class: 2, intensity: 0, rgb: Some([255, 0, 0]), ..Default::default() });
        b.push(&PointRecord { xyz: [0.0, 0.0, 20.0], class: 6, intensity: 100, rgb: Some([65535, 32768, 0]), ..Default::default() });
        let e = colorize(&b, None, &ColorMode::Elevation { min: 10.0, max: 20.0, ramp: ColorRamp::grey() });
        assert_eq!(e, vec![[0, 0, 0], [255, 255, 255]]);
        let i = colorize(&b, Some(&[1]), &ColorMode::Intensity { min: 0, max: 100, ramp: ColorRamp::grey() });
        assert_eq!(i, vec![[255, 255, 255]]);
        let c = colorize(&b, None, &ColorMode::Class { palette: ClassPalette::standard() });
        assert_eq!(c[0], [150, 100, 50]);
        // Auto: the second point has values above 255, so both read as 16-bit.
        let a = colorize(&b, None, &ColorMode::Rgb { scale: RgbScale::Auto });
        assert_eq!(a[1], [255, 128, 0]);
        assert_eq!(a[0], [0, 0, 0]);
        let a8 = colorize(&b, Some(&[0]), &ColorMode::Rgb { scale: RgbScale::Auto });
        assert_eq!(a8[0], [255, 0, 0]);
    }
}
