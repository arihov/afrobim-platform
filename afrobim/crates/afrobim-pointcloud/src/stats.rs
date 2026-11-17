//! Statistics that can be accumulated chunk by chunk, so they work on files larger than memory:
//! density grid (points per square metre), class histogram and elevation histogram.

use crate::buffer::PointBuffer;
use crate::error::{PointCloudError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Count of points per class code.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClassHistogram {
    pub counts: BTreeMap<u8, u64>,
}

impl ClassHistogram {
    pub fn add_buffer(&mut self, buf: &PointBuffer) {
        for &c in &buf.class {
            *self.counts.entry(c).or_insert(0) += 1;
        }
    }

    pub fn add_indices(&mut self, buf: &PointBuffer, indices: &[u32]) {
        for &i in indices {
            *self.counts.entry(buf.class[i as usize]).or_insert(0) += 1;
        }
    }

    pub fn total(&self) -> u64 {
        self.counts.values().sum()
    }
}

/// Equal-width bins from `min`: bin `k` holds `min + k*width <= v < min + (k+1)*width` (the last
/// bin also holds its upper edge). Values outside are counted in `below` and `above`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Histogram {
    pub min: f64,
    pub bin_width: f64,
    pub counts: Vec<u64>,
    pub below: u64,
    pub above: u64,
}

impl Histogram {
    pub fn new(min: f64, bin_width: f64, bins: usize) -> Result<Histogram> {
        if !(bin_width > 0.0 && bin_width.is_finite() && min.is_finite() && bins > 0 && bins <= 10_000_000) {
            return Err(PointCloudError::Invalid("a histogram needs a finite minimum, a positive bin width and 1 to 10 000 000 bins".into()));
        }
        Ok(Histogram { min, bin_width, counts: vec![0; bins], below: 0, above: 0 })
    }

    pub fn add(&mut self, v: f64) {
        let max = self.min + self.bin_width * self.counts.len() as f64;
        if v < self.min || v.is_nan() {
            self.below += 1;
        } else if v > max {
            self.above += 1;
        } else {
            let k = (((v - self.min) / self.bin_width) as usize).min(self.counts.len() - 1);
            self.counts[k] += 1;
        }
    }

    pub fn add_elevations(&mut self, buf: &PointBuffer) {
        for p in &buf.xyz {
            self.add(p[2]);
        }
    }

    pub fn total(&self) -> u64 {
        self.counts.iter().sum::<u64>() + self.below + self.above
    }
}

/// Point counts on a regular plan grid. The grid origin is a true coordinate (the minimum corner
/// given at construction).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DensityGrid {
    pub x0: f64,
    pub y0: f64,
    pub cell: f64,
    pub nx: usize,
    pub ny: usize,
    /// Row-major from the south-west corner: `counts[iy * nx + ix]`.
    pub counts: Vec<u32>,
    /// Points that fell outside the grid.
    pub outside: u64,
}

/// A summary of a [`DensityGrid`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DensitySummary {
    pub cells: usize,
    pub empty_cells: usize,
    /// Mean density over cells that hold points, points per square metre.
    pub mean_nonempty: f64,
    pub max: f64,
    /// Mean over all cells.
    pub mean_all: f64,
}

impl DensityGrid {
    /// A grid covering `[min_x, max_x] x [min_y, max_y]` with square cells of side `cell` metres.
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64, cell: f64) -> Result<DensityGrid> {
        if !(cell > 0.0 && cell.is_finite()) || !(max_x >= min_x && max_y >= min_y) {
            return Err(PointCloudError::Invalid("the density cell size must be positive and the extent valid".into()));
        }
        let nx = ((max_x - min_x) / cell).floor() as usize + 1;
        let ny = ((max_y - min_y) / cell).floor() as usize + 1;
        if nx.checked_mul(ny).is_none_or(|n| n > 200_000_000) {
            return Err(PointCloudError::Invalid(format!("the density grid would have {nx} x {ny} cells; use a larger cell size")));
        }
        Ok(DensityGrid { x0: min_x, y0: min_y, cell, nx, ny, counts: vec![0; nx * ny], outside: 0 })
    }

    pub fn add_buffer(&mut self, buf: &PointBuffer) {
        for p in &buf.xyz {
            self.add(p[0], p[1]);
        }
    }

    pub fn add(&mut self, x: f64, y: f64) {
        let (fx, fy) = ((x - self.x0) / self.cell, (y - self.y0) / self.cell);
        if fx < 0.0 || fy < 0.0 || fx >= self.nx as f64 || fy >= self.ny as f64 {
            self.outside += 1;
        } else {
            self.counts[fy as usize * self.nx + fx as usize] += 1;
        }
    }

    /// Points per square metre in a cell.
    pub fn density(&self, ix: usize, iy: usize) -> f64 {
        self.counts[iy * self.nx + ix] as f64 / (self.cell * self.cell)
    }

    pub fn summary(&self) -> DensitySummary {
        let a = self.cell * self.cell;
        let nonempty: Vec<u32> = self.counts.iter().copied().filter(|&c| c > 0).collect();
        let total: u64 = self.counts.iter().map(|&c| c as u64).sum();
        DensitySummary {
            cells: self.counts.len(),
            empty_cells: self.counts.len() - nonempty.len(),
            mean_nonempty: if nonempty.is_empty() { 0.0 } else { nonempty.iter().map(|&c| c as f64).sum::<f64>() / nonempty.len() as f64 / a },
            max: self.counts.iter().copied().max().unwrap_or(0) as f64 / a,
            mean_all: total as f64 / self.counts.len() as f64 / a,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::PointRecord;

    #[test]
    fn histograms_and_density() {
        let mut b = PointBuffer::new(false, false);
        for (x, y, z, c) in [(500000.1, 9960000.1, 1.0, 2u8), (500000.9, 9960000.2, 1.5, 2), (500001.5, 9960000.5, 9.0, 5), (500001.6, 9960001.4, -3.0, 7)] {
            b.push(&PointRecord { xyz: [x, y, z], class: c, ..Default::default() });
        }
        let mut ch = ClassHistogram::default();
        ch.add_buffer(&b);
        assert_eq!(ch.counts[&2], 2);
        assert_eq!(ch.total(), 4);
        let mut h = Histogram::new(0.0, 5.0, 2).unwrap();
        h.add_elevations(&b);
        assert_eq!((h.counts.clone(), h.below, h.above), (vec![2, 1], 1, 0));
        assert_eq!(h.total(), 4);
        let mut g = DensityGrid::new(500000.0, 9960000.0, 500002.0, 9960002.0, 1.0).unwrap();
        g.add_buffer(&b);
        assert_eq!((g.nx, g.ny), (3, 3));
        assert_eq!(g.counts[0], 2);
        assert_eq!(g.density(1, 0), 1.0);
        let s = g.summary();
        assert_eq!(s.cells, 9);
        assert_eq!(s.empty_cells, 6);
        assert!((s.mean_all - 4.0 / 9.0).abs() < 1e-12);
        assert!(DensityGrid::new(0.0, 0.0, 1e9, 1e9, 1.0).is_err());
    }
}
