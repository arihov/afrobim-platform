//! Compare a cloud to a surface: per-point vertical difference to a `Tin`, with a histogram and a
//! banded result for colouring, for as-built checks. Volumes are left to the terrain crate.
//!
//! The difference is `point z - surface z`: positive where the cloud is above the surface.
//! Bands and the histogram are inputs; there are no default tolerances.

use crate::buffer::PointBuffer;
use crate::error::{PointCloudError, Result};
use crate::stats::Histogram;
use crate::surface::FitReport;
use afrobim_corridor::Tin;
use serde::{Deserialize, Serialize};

/// Ascending break values of the difference, metres. With breaks `b0 < b1 < ... < b(n-1)` there are
/// `n + 1` bands: band 0 is below `b0`, band `k` is `b(k-1) <= d < b(k)`, the last is `>= b(n-1)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DifferenceBands {
    pub breaks: Vec<f64>,
}

impl DifferenceBands {
    pub fn new(breaks: Vec<f64>) -> Result<DifferenceBands> {
        if breaks.is_empty() || breaks.len() > 253 || breaks.iter().any(|b| !b.is_finite()) || breaks.windows(2).any(|w| !(w[0] < w[1])) {
            return Err(PointCloudError::Invalid("the breaks must be 1 to 253 finite values in increasing order".into()));
        }
        Ok(DifferenceBands { breaks })
    }

    pub fn band_count(&self) -> usize {
        self.breaks.len() + 1
    }

    pub fn band_of(&self, d: f64) -> u8 {
        self.breaks.partition_point(|&b| b <= d) as u8
    }
}

/// A colour per band and one for points outside the surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BandColors {
    pub colors: Vec<[u8; 3]>,
    pub outside: [u8; 3],
}

impl BandColors {
    /// Blue (below) through white (middle) to red (above) over `bands` bands: a cosmetic choice.
    pub fn blue_white_red(bands: usize) -> BandColors {
        let n = bands.max(1);
        let colors = (0..n)
            .map(|k| {
                let t = if n == 1 { 0.5 } else { k as f64 / (n - 1) as f64 };
                let (lo, hi) = if t < 0.5 { ([40.0, 80.0, 255.0], [255.0, 255.0, 255.0]) } else { ([255.0, 255.0, 255.0], [255.0, 40.0, 40.0]) };
                let f = if t < 0.5 { t * 2.0 } else { (t - 0.5) * 2.0 };
                [0, 1, 2].map(|c| (lo[c] + (hi[c] - lo[c]) * f).round() as u8)
            })
            .collect();
        BandColors { colors, outside: [128, 128, 128] }
    }
}

/// The result of comparing points to a surface.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// The points compared, as given (indices into the buffer).
    pub indices: Vec<u32>,
    /// `point z - surface z`, `NaN` for points outside the surface.
    pub dz: Vec<f64>,
    /// Band of each point; 255 for outside.
    pub band: Vec<u8>,
    pub band_counts: Vec<u64>,
    pub outside: usize,
    pub stats: FitReport,
    pub histogram: Histogram,
}

impl Comparison {
    /// Colours in the order of `indices`.
    pub fn colors(&self, c: &BandColors) -> Vec<[u8; 3]> {
        self.band.iter().map(|&b| if b == 255 { c.outside } else { c.colors.get(b as usize).copied().unwrap_or(c.outside) }).collect()
    }
}

/// Compares the points at `indices` (all when `None`) with a surface. The histogram is
/// `bins` bins of `bin_width` from `hist_min`.
pub fn compare_to_tin(buf: &PointBuffer, indices: Option<&[u32]>, tin: &Tin, hist_min: f64, bin_width: f64, bins: usize, bands: &DifferenceBands) -> Result<Comparison> {
    let mut histogram = Histogram::new(hist_min, bin_width, bins)?;
    let idx: Vec<u32> = match indices {
        Some(i) => i.to_vec(),
        None => (0..buf.len() as u32).collect(),
    };
    let mut dz = Vec::with_capacity(idx.len());
    let mut band = Vec::with_capacity(idx.len());
    let mut band_counts = vec![0u64; bands.band_count()];
    let (mut n, mut outside) = (0usize, 0usize);
    let (mut sum, mut sq, mut max_abs, mut min, mut max) = (0.0f64, 0.0f64, 0.0f64, f64::INFINITY, f64::NEG_INFINITY);
    for &i in &idx {
        let p = buf.xyz[i as usize];
        match tin.elevation_at(p[0], p[1]) {
            Some(z) => {
                let d = p[2] - z;
                dz.push(d);
                let b = bands.band_of(d);
                band.push(b);
                band_counts[b as usize] += 1;
                histogram.add(d);
                n += 1;
                sum += d;
                sq += d * d;
                max_abs = max_abs.max(d.abs());
                min = min.min(d);
                max = max.max(d);
            }
            None => {
                dz.push(f64::NAN);
                band.push(255);
                outside += 1;
            }
        }
    }
    let stats = if n == 0 { FitReport { compared: 0, outside, mean: 0.0, rms: 0.0, max_abs: 0.0, min: 0.0, max: 0.0 } } else { FitReport { compared: n, outside, mean: sum / n as f64, rms: (sq / n as f64).sqrt(), max_abs, min, max } };
    Ok(Comparison { indices: idx, dz, band, band_counts, outside, stats, histogram })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::PointRecord;

    #[test]
    fn bands_histogram_and_outside() {
        let tin = Tin::from_grid(500000.0, 9960000.0, 1.0, 1.0, 11, 11, |_, _| 100.0).unwrap();
        let mut b = PointBuffer::new(false, false);
        for (x, y, z) in [(500002.0, 9960002.0, 100.00), (500003.0, 9960003.0, 100.04), (500004.0, 9960004.0, 99.90), (500005.0, 9960005.0, 100.30), (600000.0, 9960000.0, 5.0)] {
            b.push(&PointRecord { xyz: [x, y, z], ..Default::default() });
        }
        let bands = DifferenceBands::new(vec![-0.05, 0.05, 0.2]).unwrap();
        assert_eq!(bands.band_count(), 4);
        let c = compare_to_tin(&b, None, &tin, -0.5, 0.25, 4, &bands).unwrap();
        assert_eq!(c.band, vec![1, 1, 0, 3, 255]);
        assert_eq!(c.band_counts, vec![1, 2, 0, 1]);
        assert_eq!(c.outside, 1);
        assert_eq!(c.stats.compared, 4);
        assert!((c.stats.mean - (0.0 + 0.04 - 0.10 + 0.30) / 4.0).abs() < 1e-9);
        assert!((c.stats.max_abs - 0.30).abs() < 1e-9);
        assert_eq!(c.histogram.counts.iter().sum::<u64>(), 4);
        let colors = c.colors(&BandColors::blue_white_red(4));
        assert_eq!(colors[4], [128, 128, 128]);
        assert_eq!(colors[2], BandColors::blue_white_red(4).colors[0]);
        assert!(DifferenceBands::new(vec![1.0, 1.0]).is_err());
        assert_eq!(bands.band_of(0.05), 2);
    }
}
