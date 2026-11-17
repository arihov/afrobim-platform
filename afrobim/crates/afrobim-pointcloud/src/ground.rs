//! Ground points: use an existing classification, or filter an unclassified cloud.
//!
//! Every threshold, window and cell size is an input. The structs below have no `Default`: the
//! caller (with the engineer behind it) fills them for the terrain and the point density at hand.
//!
//! # Methods
//!
//! * [`ground_from_classes`]: take the points whose class code is in a given set (class 2 is
//!   "Ground" in both class tables of the LAS specification, see [`crate::classes`]).
//! * [`progressive_morphological_filter`]: a grid implementation of the progressive
//!   morphological filter of Zhang, K., Chen, S.-C., Whitman, D., Shyu, M.-L., Yan, J. and
//!   Zhang, C. (2003), "A progressive morphological filter for removing nonground measurements
//!   from airborne LIDAR data", IEEE Transactions on Geoscience and Remote Sensing 41(4),
//!   872-882. It was written from the open description in the PDAL documentation of
//!   `filters.pmf` (which cites the paper) and from the abstract-level idea: a minimum
//!   elevation grid is opened (erosion then dilation) with windows that grow from one iteration
//!   to the next, and a point is nonground when it stands higher than the opened surface by more
//!   than that iteration's height threshold. The paper itself was not available to read, so
//!   details (how cells are filled, the point-level comparison, the first threshold) are this
//!   crate's reading and are stated in each function. Compare against a reference on your data.
//! * [`grid_minimum_slope_filter`]: this crate's own method, not from a paper: a grid minimum
//!   surface where a cell is kept only if it is not higher than its neighbours' minima by more
//!   than a slope allows.

use crate::buffer::PointBuffer;
use crate::error::{PointCloudError, Result};
use serde::{Deserialize, Serialize};

/// Ground and nonground point indices (into the buffer that was filtered).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroundResult {
    pub ground: Vec<u32>,
    pub non_ground: Vec<u32>,
    /// Cells of the minimum grid that held points.
    pub occupied_cells: usize,
    pub grid_size: [usize; 2],
}

/// Indices (within `indices`, or all points when `None`) whose class code is in `classes`.
pub fn ground_from_classes(buf: &PointBuffer, indices: Option<&[u32]>, classes: &[u8]) -> Vec<u32> {
    let mut mask = [false; 256];
    for &c in classes {
        mask[c as usize] = true;
    }
    match indices {
        Some(ix) => ix.iter().copied().filter(|&i| mask[buf.class[i as usize] as usize]).collect(),
        None => (0..buf.len() as u32).filter(|&i| mask[buf.class[i as usize] as usize]).collect(),
    }
}

/// Parameters of the progressive morphological filter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PmfParams {
    /// Side of the minimum grid cells, metres.
    pub cell_size: f64,
    /// Window width of each iteration in cells (made odd by adding 1 when even). Usually growing.
    pub window_cells: Vec<usize>,
    /// Height threshold of each iteration, metres; same length as `window_cells`.
    pub height_thresholds: Vec<f64>,
}

impl PmfParams {
    /// Thresholds from the description in the PDAL documentation of `filters.pmf`: for each
    /// iteration, `slope * cell_size * (window change since the previous iteration) +
    /// initial_distance`, never above `max_distance`. The first iteration has no previous
    /// window, so it takes `initial_distance` (this crate's reading). `slope` is rise over run
    /// (1.0 is 45 degrees). All four numbers are inputs.
    pub fn from_series(cell_size: f64, window_cells: Vec<usize>, slope: f64, initial_distance: f64, max_distance: f64) -> PmfParams {
        let mut height_thresholds = Vec::with_capacity(window_cells.len());
        for (k, &w) in window_cells.iter().enumerate() {
            let dh = if k == 0 { initial_distance } else { slope * cell_size * (w as f64 - window_cells[k - 1] as f64) + initial_distance };
            height_thresholds.push(dh.min(max_distance));
        }
        PmfParams { cell_size, window_cells, height_thresholds }
    }
}

/// Window widths `start, start + step, ...` up to and including `max` (cells).
pub fn linear_windows(start: usize, step: usize, max: usize) -> Vec<usize> {
    let step = step.max(1);
    (0..).map(|k| start + k * step).take_while(|&w| w <= max).collect()
}

/// Window widths `base^k` rounded, from `k = 1` while at most `max` (cells), without repeats.
pub fn exponential_windows(base: f64, max: usize) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    if base <= 1.0 {
        return out;
    }
    let mut k = 1;
    while let w @ 1.. = base.powi(k).round() as usize {
        if w > max {
            break;
        }
        if out.last() != Some(&w) {
            out.push(w);
        }
        k += 1;
        if k > 60 {
            break;
        }
    }
    out
}

struct Grid {
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    cell_of: Vec<u32>,
    z: Vec<f64>,
}

fn candidate_list(buf: &PointBuffer, indices: Option<&[u32]>) -> Vec<u32> {
    match indices {
        Some(i) => i.to_vec(),
        None => (0..buf.len() as u32).collect(),
    }
}

fn min_grid(buf: &PointBuffer, cand: &[u32], cell: f64) -> Result<Grid> {
    if !(cell > 0.0 && cell.is_finite()) {
        return Err(PointCloudError::Invalid("the cell size must be positive".into()));
    }
    if cand.is_empty() {
        return Err(PointCloudError::Invalid("no points to filter".into()));
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &i in cand {
        let p = buf.xyz[i as usize];
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    let nx = ((x1 - x0) / cell).floor() as usize + 1;
    let ny = ((y1 - y0) / cell).floor() as usize + 1;
    if nx.checked_mul(ny).is_none_or(|n| n > 400_000_000) {
        return Err(PointCloudError::Invalid(format!("the minimum grid would be {nx} x {ny} cells; use a larger cell size or fewer points")));
    }
    let mut z = vec![f64::NAN; nx * ny];
    let mut cell_of = Vec::with_capacity(cand.len());
    for &i in cand {
        let p = buf.xyz[i as usize];
        let c = (((p[1] - y0) / cell) as usize).min(ny - 1) * nx + (((p[0] - x0) / cell) as usize).min(nx - 1);
        cell_of.push(c as u32);
        if z[c].is_nan() || p[2] < z[c] {
            z[c] = p[2];
        }
    }
    Ok(Grid { x0, y0, cell, nx, ny, cell_of, z })
}

/// Fills empty (NaN) cells with the value of the nearest filled cell by steps over the four
/// neighbours (a breadth-first fill; ties go to the earlier source).
fn fill_empty(z: &mut [f64], nx: usize, ny: usize) {
    let mut queue: std::collections::VecDeque<usize> = (0..z.len()).filter(|&i| !z[i].is_nan()).collect();
    while let Some(c) = queue.pop_front() {
        let (ix, iy) = (c % nx, c / nx);
        let v = z[c];
        let mut push = |n: usize| {
            if z[n].is_nan() {
                z[n] = v;
                queue.push_back(n);
            }
        };
        if ix > 0 {
            push(c - 1);
        }
        if ix + 1 < nx {
            push(c + 1);
        }
        if iy > 0 {
            push(c - nx);
        }
        if iy + 1 < ny {
            push(c + nx);
        }
    }
}

/// Square-window minimum (erosion) or maximum (dilation) with radius `r` cells, as two passes.
fn window_extreme(src: &[f64], nx: usize, ny: usize, r: usize, min: bool) -> Vec<f64> {
    let pick = |a: f64, b: f64| if min { a.min(b) } else { a.max(b) };
    let mut tmp = vec![0.0; src.len()];
    for iy in 0..ny {
        for ix in 0..nx {
            let (lo, hi) = (ix.saturating_sub(r), (ix + r).min(nx - 1));
            let mut v = src[iy * nx + lo];
            for k in lo + 1..=hi {
                v = pick(v, src[iy * nx + k]);
            }
            tmp[iy * nx + ix] = v;
        }
    }
    let mut out = vec![0.0; src.len()];
    for iy in 0..ny {
        let (lo, hi) = (iy.saturating_sub(r), (iy + r).min(ny - 1));
        for ix in 0..nx {
            let mut v = tmp[lo * nx + ix];
            for k in lo + 1..=hi {
                v = pick(v, tmp[k * nx + ix]);
            }
            out[iy * nx + ix] = v;
        }
    }
    out
}

/// Progressive morphological filter on a grid (see the module documentation for the source and
/// for what is this crate's own reading).
///
/// 1. A grid of cell minima is built from the candidate points (`indices`, or all); empty cells
///    take the value of the nearest filled cell.
/// 2. For each iteration `k`, the current surface is opened with a square window of
///    `window_cells[k]` cells (erosion then dilation), and every point that stands more than
///    `height_thresholds[k]` above the opened surface in its cell is marked nonground. The opened
///    surface becomes the current surface for the next iteration.
/// 3. Points never marked are ground.
///
/// The comparison is made per point (not per cell minimum), so on rough ground the first
/// threshold also sets the allowed ground noise.
pub fn progressive_morphological_filter(buf: &PointBuffer, indices: Option<&[u32]>, params: &PmfParams) -> Result<GroundResult> {
    if params.window_cells.is_empty() || params.window_cells.len() != params.height_thresholds.len() {
        return Err(PointCloudError::Invalid("window_cells and height_thresholds must be the same non-zero length".into()));
    }
    if params.height_thresholds.iter().any(|t| !t.is_finite() || *t < 0.0) || params.window_cells.contains(&0) {
        return Err(PointCloudError::Invalid("thresholds must be finite and not negative; windows at least 1 cell".into()));
    }
    let cand = candidate_list(buf, indices);
    let mut g = min_grid(buf, &cand, params.cell_size)?;
    let occupied = g.z.iter().filter(|v| !v.is_nan()).count();
    fill_empty(&mut g.z, g.nx, g.ny);
    let mut nonground = vec![false; cand.len()];
    let mut surface = g.z.clone();
    for (k, &w) in params.window_cells.iter().enumerate() {
        // Window width made odd; the radius is half of it.
        let r = (if w % 2 == 0 { w + 1 } else { w } - 1) / 2;
        let eroded = window_extreme(&surface, g.nx, g.ny, r, true);
        surface = window_extreme(&eroded, g.nx, g.ny, r, false);
        let dh = params.height_thresholds[k];
        for (j, &i) in cand.iter().enumerate() {
            if !nonground[j] && buf.xyz[i as usize][2] - surface[g.cell_of[j] as usize] > dh {
                nonground[j] = true;
            }
        }
    }
    let (mut ground, mut non_ground) = (Vec::new(), Vec::new());
    for (j, &i) in cand.iter().enumerate() {
        if nonground[j] { non_ground.push(i) } else { ground.push(i) }
    }
    Ok(GroundResult { ground, non_ground, occupied_cells: occupied, grid_size: [g.nx, g.ny] })
}

/// Parameters of the grid-minimum and slope filter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridMinSlopeParams {
    /// Side of the minimum grid cells, metres.
    pub cell_size: f64,
    /// Largest allowed rise over run between a cell minimum and a lower neighbouring cell
    /// minimum (1.0 is 45 degrees).
    pub max_slope: f64,
    /// Neighbours are the cells within this many cells in each direction.
    pub search_cells: usize,
    /// A point is ground when it is at most this far above the minimum of its (accepted) cell, metres.
    pub tolerance: f64,
}

/// This crate's own ground filter (not from a paper).
///
/// 1. Minimum elevation per cell.
/// 2. A cell is accepted when, for every occupied cell within `search_cells`, its minimum is not
///    higher than that cell's minimum by more than `max_slope` times the distance between the two
///    cell centres. A roof or a vegetation canopy that fills a cell sits above the ground around it
///    by more than the slope allows and is rejected. Cells with no occupied neighbour are accepted.
/// 3. A point is ground if its cell is accepted and it is no more than `tolerance` above the cell
///    minimum.
///
/// Limits: a low outlier (noise below the ground) is the minimum of its cell and spoils it and
/// its neighbours' acceptance; remove noise classes first (pass `indices`). Large flat roofs wider
/// than the search window are accepted as ground. Steep terrain needs a `max_slope` above the
/// steepest ground slope.
pub fn grid_minimum_slope_filter(buf: &PointBuffer, indices: Option<&[u32]>, params: &GridMinSlopeParams) -> Result<GroundResult> {
    if !(params.max_slope >= 0.0 && params.max_slope.is_finite() && params.tolerance >= 0.0 && params.tolerance.is_finite() && params.search_cells >= 1) {
        return Err(PointCloudError::Invalid("max_slope and tolerance must be finite and not negative; search_cells at least 1".into()));
    }
    let cand = candidate_list(buf, indices);
    let g = min_grid(buf, &cand, params.cell_size)?;
    let r = params.search_cells as isize;
    let mut accepted = vec![false; g.z.len()];
    for iy in 0..g.ny {
        for ix in 0..g.nx {
            let c = iy * g.nx + ix;
            if g.z[c].is_nan() {
                continue;
            }
            let mut ok = true;
            'scan: for dy in -r..=r {
                let jy = iy as isize + dy;
                if jy < 0 || jy >= g.ny as isize {
                    continue;
                }
                for dx in -r..=r {
                    let jx = ix as isize + dx;
                    if (dx == 0 && dy == 0) || jx < 0 || jx >= g.nx as isize {
                        continue;
                    }
                    let n = g.z[jy as usize * g.nx + jx as usize];
                    if n.is_nan() {
                        continue;
                    }
                    let dist = ((dx * dx + dy * dy) as f64).sqrt() * g.cell;
                    if g.z[c] - n > params.max_slope * dist {
                        ok = false;
                        break 'scan;
                    }
                }
            }
            accepted[c] = ok;
        }
    }
    let (mut ground, mut non_ground) = (Vec::new(), Vec::new());
    for (j, &i) in cand.iter().enumerate() {
        let c = g.cell_of[j] as usize;
        if accepted[c] && buf.xyz[i as usize][2] - g.z[c] <= params.tolerance {
            ground.push(i)
        } else {
            non_ground.push(i)
        }
    }
    let _ = (g.x0, g.y0);
    Ok(GroundResult { ground, non_ground, occupied_cells: g.z.iter().filter(|v| !v.is_nan()).count(), grid_size: [g.nx, g.ny] })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::PointRecord;

    /// Gently sloping ground with a 6 x 6 m, 8 m high block on it, 1 m point spacing, plus two
    /// pits are not added (keep the test about the block).
    fn scene() -> (PointBuffer, Vec<bool>) {
        let mut b = PointBuffer::new(false, false);
        let mut is_block = Vec::new();
        for ix in 0..40 {
            for iy in 0..40 {
                let (x, y) = (500000.0 + ix as f64, 9960000.0 + iy as f64);
                let ground_z = 100.0 + 0.02 * ix as f64;
                let block = (15..21).contains(&ix) && (15..21).contains(&iy);
                // The block's points are on the roof only; ground points under it are absent.
                let z = if block { ground_z + 8.0 } else { ground_z };
                b.push(&PointRecord { xyz: [x, y, z], class: 1, ..Default::default() });
                is_block.push(block);
            }
        }
        (b, is_block)
    }

    #[test]
    fn pmf_removes_the_block_and_keeps_the_slope() {
        let (b, is_block) = scene();
        // Windows in cells, all inputs; thresholds from the PDAL description with test numbers.
        let p = PmfParams::from_series(1.0, vec![3, 5, 9, 13], 0.3, 0.2, 2.0);
        assert_eq!(p.height_thresholds.len(), 4);
        let r = progressive_morphological_filter(&b, None, &p).unwrap();
        assert_eq!(r.ground.len() + r.non_ground.len(), 1600);
        for &i in &r.non_ground {
            assert!(is_block[i as usize] || b.xyz[i as usize][2] > 0.0);
        }
        // Every roof point is rejected, and almost every other point is kept.
        assert!(r.non_ground.iter().filter(|&&i| is_block[i as usize]).count() >= 30, "{}", r.non_ground.len());
        let kept_ground = r.ground.iter().filter(|&&i| !is_block[i as usize]).count();
        assert!(kept_ground >= 1500, "{kept_ground}");
        assert!(r.ground.iter().all(|&i| !is_block[i as usize]));
    }

    #[test]
    fn slope_filter_removes_the_block() {
        let (b, is_block) = scene();
        let p = GridMinSlopeParams { cell_size: 1.0, max_slope: 0.5, search_cells: 5, tolerance: 0.1 };
        let r = grid_minimum_slope_filter(&b, None, &p).unwrap();
        assert!(r.ground.iter().all(|&i| !is_block[i as usize]));
        assert!(r.ground.len() >= 1500);
    }

    #[test]
    fn classification_and_input_checks() {
        let (mut b, _) = scene();
        b.class[10] = 2;
        b.class[20] = 2;
        assert_eq!(ground_from_classes(&b, None, &[2]), vec![10, 20]);
        assert_eq!(ground_from_classes(&b, Some(&[10, 11]), &[2]), vec![10]);
        let bad = PmfParams { cell_size: 1.0, window_cells: vec![3], height_thresholds: vec![] };
        assert!(progressive_morphological_filter(&b, None, &bad).is_err());
        assert_eq!(linear_windows(3, 2, 9), vec![3, 5, 7, 9]);
        assert_eq!(exponential_windows(2.0, 20), vec![2, 4, 8, 16]);
    }

    #[test]
    fn window_extreme_is_a_square_window() {
        let src = vec![5.0, 5.0, 5.0, 5.0, 1.0, 5.0, 5.0, 5.0, 5.0];
        let e = window_extreme(&src, 3, 3, 1, true);
        assert!(e.iter().all(|&v| v == 1.0));
        let d = window_extreme(&e, 3, 3, 1, false);
        assert!(d.iter().all(|&v| v == 1.0));
    }
}
