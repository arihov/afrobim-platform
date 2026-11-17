//! From ground points to a surface: thinning, decimation, output for `afrobim-terrain` and a fit report.
//!
//! * [`thin_to_grid`]: one point per grid cell at an input cell size: the lowest point, the mean
//!   or the median-elevation point. Output points are real coordinates: the minimum and median
//!   choices return an actual point of the cloud; the mean averages the offsets from the cell's
//!   first point (so 7-digit northings lose no precision) and returns the average position.
//! * [`decimate_to_tolerance`]: drops points while every dropped point stays within a vertical
//!   tolerance of the surface of the kept ones. This is `afrobim_terrain::simplify_points`
//!   (a greedy insertion of the worst point into a Delaunay triangulation); its guarantee is at
//!   the dropped input points only, not between them. It is slow for very many points: thin to a
//!   grid first.
//! * [`surface_points`], [`surface_operation`], [`tin_from_points`]: the points as an
//!   `afrobim-terrain` `AddPoints` operation or directly as a `Tin`.
//! * [`FitReport`]: signed vertical differences (point minus surface) at a set of points.

use crate::buffer::PointBuffer;
use crate::error::{PointCloudError, Result};
use afrobim_corridor::Tin;
use afrobim_terrain::{simplify_points, BuildReport, BuildSettings, Operation, SimplifyReport, SurfaceDefinition, SurfacePoint};
use serde::{Deserialize, Serialize};

/// Which value a grid cell keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellStat {
    /// The lowest point (the usual choice for a ground surface under noise from above).
    Min,
    /// The average position of the points in the cell.
    Mean,
    /// The point of median elevation (the lower of the two middle points for an even count).
    Median,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinResult {
    pub points: Vec<[f64; 3]>,
    pub input_count: usize,
    pub cells: usize,
}

/// One point per `cell_size` square (cells anchored at the minimum corner of the candidates).
/// Output is ordered by row from the south, then by column.
pub fn thin_to_grid(buf: &PointBuffer, indices: Option<&[u32]>, cell_size: f64, stat: CellStat) -> Result<ThinResult> {
    if !(cell_size > 0.0 && cell_size.is_finite()) {
        return Err(PointCloudError::Invalid("the cell size must be positive".into()));
    }
    let all: Vec<u32>;
    let cand: &[u32] = match indices {
        Some(i) => i,
        None => {
            all = (0..buf.len() as u32).collect();
            &all
        }
    };
    if cand.is_empty() {
        return Ok(ThinResult { points: Vec::new(), input_count: 0, cells: 0 });
    }
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    for &i in cand {
        let p = buf.xyz[i as usize];
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
    }
    let mut keyed: Vec<(u64, u32)> = Vec::with_capacity(cand.len());
    for &i in cand {
        let p = buf.xyz[i as usize];
        let (cx, cy) = (((p[0] - x0) / cell_size) as u64, ((p[1] - y0) / cell_size) as u64);
        if cx >= 1 << 32 || cy >= 1 << 31 {
            return Err(PointCloudError::Invalid("the cell size is too small for the extent of the points".into()));
        }
        keyed.push(((cy << 32) | cx, i));
    }
    keyed.sort_unstable();
    let mut points = Vec::new();
    let mut cells = 0;
    let mut s = 0;
    while s < keyed.len() {
        let mut e = s + 1;
        while e < keyed.len() && keyed[e].0 == keyed[s].0 {
            e += 1;
        }
        let group = &keyed[s..e];
        cells += 1;
        points.push(match stat {
            CellStat::Min => group.iter().map(|g| buf.xyz[g.1 as usize]).min_by(|a, b| a[2].total_cmp(&b[2])).unwrap_or([0.0; 3]),
            CellStat::Median => {
                let mut zs: Vec<[f64; 3]> = group.iter().map(|g| buf.xyz[g.1 as usize]).collect();
                zs.sort_by(|a, b| a[2].total_cmp(&b[2]));
                zs[(zs.len() - 1) / 2]
            }
            CellStat::Mean => {
                let base = buf.xyz[group[0].1 as usize];
                let mut d = [0.0f64; 3];
                for g in group {
                    let p = buf.xyz[g.1 as usize];
                    for k in 0..3 {
                        d[k] += p[k] - base[k];
                    }
                }
                let n = group.len() as f64;
                [base[0] + d[0] / n, base[1] + d[1] / n, base[2] + d[2] / n]
            }
        });
        s = e;
    }
    Ok(ThinResult { points, input_count: cand.len(), cells })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecimationResult {
    pub points: Vec<[f64; 3]>,
    pub report: SimplifyReport,
    /// Points dropped before decimation because another point had exactly the same x and y.
    pub duplicates_removed: usize,
}

/// Keeps a subset such that every dropped point is within `tolerance` (metres, vertical) of the
/// surface of the kept points. Points with exactly equal x and y are reduced to the first.
pub fn decimate_to_tolerance(points: &[[f64; 3]], tolerance: f64) -> Result<DecimationResult> {
    let mut seen = std::collections::HashSet::with_capacity(points.len());
    let unique: Vec<[f64; 3]> = points.iter().copied().filter(|p| seen.insert((p[0].to_bits(), p[1].to_bits()))).collect();
    let (kept, report) = simplify_points(&unique, tolerance, &[])?;
    let mut idx = kept;
    idx.sort_unstable();
    Ok(DecimationResult { points: idx.into_iter().map(|i| unique[i]).collect(), report, duplicates_removed: points.len() - unique.len() })
}

/// Terrain surface points with ids `<prefix>1`, `<prefix>2`, ...
pub fn surface_points(points: &[[f64; 3]], id_prefix: &str) -> Vec<SurfacePoint> {
    points.iter().enumerate().map(|(i, p)| SurfacePoint::new(format!("{id_prefix}{}", i + 1), p[0], p[1], p[2])).collect()
}

/// An `AddPoints` operation for a surface definition.
pub fn surface_operation(group: &str, points: &[[f64; 3]]) -> Operation {
    Operation::AddPoints { group: group.to_string(), points: surface_points(points, "P") }
}

/// A `Tin` from points, through a one-operation `afrobim-terrain` definition. `settings` default
/// to the terrain crate's own.
pub fn tin_from_points(points: &[[f64; 3]], settings: Option<BuildSettings>) -> Result<(Tin, BuildReport)> {
    let mut def = SurfaceDefinition::new().with(surface_operation("point cloud", points));
    if let Some(s) = settings {
        def.settings = s;
    }
    let built = def.build()?;
    Ok((built.tin, built.report))
}

/// Vertical differences, point minus surface, at a set of points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FitReport {
    /// Points compared.
    pub compared: usize,
    /// Points outside the surface (not compared).
    pub outside: usize,
    pub mean: f64,
    pub rms: f64,
    /// Largest absolute difference.
    pub max_abs: f64,
    pub min: f64,
    pub max: f64,
}

impl FitReport {
    pub fn of(points: &[[f64; 3]], tin: &Tin) -> FitReport {
        let (mut n, mut outside) = (0usize, 0usize);
        let (mut sum, mut sq, mut max_abs, mut min, mut max) = (0.0f64, 0.0f64, 0.0f64, f64::INFINITY, f64::NEG_INFINITY);
        for p in points {
            match tin.elevation_at(p[0], p[1]) {
                Some(z) => {
                    let d = p[2] - z;
                    n += 1;
                    sum += d;
                    sq += d * d;
                    max_abs = max_abs.max(d.abs());
                    min = min.min(d);
                    max = max.max(d);
                }
                None => outside += 1,
            }
        }
        if n == 0 {
            return FitReport { compared: 0, outside, mean: 0.0, rms: 0.0, max_abs: 0.0, min: 0.0, max: 0.0 };
        }
        FitReport { compared: n, outside, mean: sum / n as f64, rms: (sq / n as f64).sqrt(), max_abs, min, max }
    }
}

/// What to do to ground points to make a surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceSpec {
    /// Grid thinning: cell size and statistic. `None`: no grid thinning.
    pub grid: Option<(f64, CellStat)>,
    /// Tolerance decimation, metres. `None`: none.
    pub tolerance: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct GroundSurface {
    /// The points of the surface, ready for [`surface_operation`].
    pub points: Vec<[f64; 3]>,
    pub tin: Tin,
    pub ground_points_in: usize,
    pub after_grid: usize,
    pub after_decimation: usize,
    /// Differences between the input ground points and the surface (point minus surface).
    pub fit: FitReport,
}

/// Ground points (indices into `buf`) to a surface: grid thinning, then tolerance decimation,
/// then a `Tin`, and the fit of the surface to all the input ground points.
pub fn ground_surface(buf: &PointBuffer, ground: &[u32], spec: &SurfaceSpec) -> Result<GroundSurface> {
    let input: Vec<[f64; 3]> = ground.iter().map(|&i| buf.xyz[i as usize]).collect();
    let mut pts = match spec.grid {
        Some((cell, stat)) => thin_to_grid(buf, Some(ground), cell, stat)?.points,
        None => input.clone(),
    };
    let after_grid = pts.len();
    if let Some(t) = spec.tolerance {
        pts = decimate_to_tolerance(&pts, t)?.points;
    }
    let (tin, _) = tin_from_points(&pts, None)?;
    let fit = FitReport::of(&input, &tin);
    Ok(GroundSurface { after_decimation: pts.len(), points: pts, tin, ground_points_in: input.len(), after_grid, fit })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::PointRecord;

    fn plane_cloud() -> PointBuffer {
        let mut b = PointBuffer::new(false, false);
        // 20 x 20 points at 0.5 m on the plane z = 100 + 0.1 (x - x0) + 0.05 (y - y0), true UTM-size coordinates.
        for ix in 0..20 {
            for iy in 0..20 {
                let (x, y) = (500000.0 + ix as f64 * 0.5, 9960000.0 + iy as f64 * 0.5);
                b.push(&PointRecord { xyz: [x, y, 100.0 + 0.1 * ix as f64 * 0.5 + 0.05 * iy as f64 * 0.5], class: 2, ..Default::default() });
            }
        }
        b
    }

    #[test]
    fn thinning_statistics() {
        let b = plane_cloud();
        let min = thin_to_grid(&b, None, 2.0, CellStat::Min).unwrap();
        assert_eq!(min.cells, 25);
        assert_eq!(min.points.len(), 25);
        // The first cell's lowest point is the south-west corner point itself.
        assert_eq!(min.points[0], [500000.0, 9960000.0, 100.0]);
        let mean = thin_to_grid(&b, None, 2.0, CellStat::Mean).unwrap();
        // Mean of a 4 x 4 block of a plane lies on the plane at the block centre.
        assert!((mean.points[0][0] - 500000.75).abs() < 1e-9);
        assert!((mean.points[0][2] - (100.0 + 0.1 * 0.75 + 0.05 * 0.75)).abs() < 1e-9);
        let med = thin_to_grid(&b, None, 2.0, CellStat::Median).unwrap();
        assert_eq!(med.points.len(), 25);
        assert!(thin_to_grid(&b, None, 0.0, CellStat::Min).is_err());
    }

    #[test]
    fn plane_decimates_to_few_points_with_exact_fit() {
        let b = plane_cloud();
        let ground: Vec<u32> = (0..b.len() as u32).collect();
        let s = ground_surface(&b, &ground, &SurfaceSpec { grid: None, tolerance: Some(0.001) }).unwrap();
        assert_eq!(s.ground_points_in, 400);
        assert!(s.after_decimation < 150, "{}", s.after_decimation);
        assert_eq!(s.fit.compared, 400);
        assert!(s.fit.max_abs < 0.001 + 1e-9, "{:?}", s.fit);
        assert!(s.fit.rms <= s.fit.max_abs);
        // The same surface through terrain's definition.
        let op = surface_operation("cloud", &s.points);
        match op {
            Operation::AddPoints { points, .. } => assert_eq!(points.len(), s.points.len()),
            _ => panic!(),
        }
        // Coordinates are not recentred.
        assert!(s.points.iter().all(|p| p[1] >= 9960000.0));
    }

    #[test]
    fn fit_report_counts_outside_points() {
        let b = plane_cloud();
        let ground: Vec<u32> = (0..b.len() as u32).collect();
        let s = ground_surface(&b, &ground, &SurfaceSpec { grid: Some((2.0, CellStat::Min)), tolerance: None }).unwrap();
        let r = FitReport::of(&[[500001.0, 9960001.0, 101.0], [0.0, 0.0, 0.0]], &s.tin);
        assert_eq!((r.compared, r.outside), (1, 1));
        assert!((r.mean - (101.0 - (100.0 + 0.1 + 0.05))).abs() < 1e-6);
    }
}
