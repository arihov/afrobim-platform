// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Grid surfaces to TIN, and point reduction (simplification) of a TIN.
//!
//! # Grid to TIN
//!
//! A regular grid of elevations becomes a TIN with a vertex at every valid node. Each cell is
//! split into two triangles along the diagonal whose end nodes differ least in elevation (the
//! diagonal follows the ridge or valley better than a fixed direction would; a plain
//! heuristic, not taken from a standard). A cell with one invalid (no-data) node becomes the one
//! triangle of its three valid nodes; a cell with two or more invalid nodes is left out.
//! Rows are given from the southernmost row to the northernmost, west to east.
//!
//! The ASCII grid reader follows the Esri ASCII raster format (Esri, "Esri ASCII raster format",
//! ArcGIS documentation): header keywords `ncols`, `nrows`, `xllcorner` or `xllcenter`,
//! `yllcorner` or `yllcenter`, `cellsize` (a single cell size, so square cells only),
//! `NODATA_value`, then the rows from the north down. The cell value is taken at the cell
//! centre, which is how the format defines it.
//!
//! # Simplification
//!
//! Greedy insertion, after M. Garland and P. S. Heckbert, "Fast polygonal approximation of
//! terrains and height fields", Technical report CMU-CS-95-181, Carnegie Mellon University,
//! 1995 (the same idea in R. J. Fowler and J. J. Little, "Automatic extraction of irregular
//! digital terrain models", Computer Graphics 13(2), 1979, as an alternative to dropping
//! points): start from the points on the convex hull, triangulate them (Delaunay), find every
//! remaining point's vertical distance to the surface of the triangle it falls in, and insert,
//! in each triangle, the point with the largest distance if that is above the tolerance;
//! repeat until none is. **Guarantee:** every input point that was dropped is within `tolerance`
//! metres, measured vertically, of the simplified surface. Nothing is promised between the
//! points, and breaklines are not respected (the result is an unconstrained Delaunay
//! triangulation of the kept points).

use afrobim_corridor::Tin;
use serde::{Deserialize, Serialize};
use spade::handles::FixedFaceHandle;
use spade::{DelaunayTriangulation, HasPosition, Point2, PositionInTriangulation, Triangulation};
use std::collections::HashMap;

use crate::error::TerrainError;
use crate::geom::{area2, orient, Plane};

/// A regular grid: node `(i, j)` is at `(x0 + i dx, y0 + j dy)`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GridSpec {
    /// Easting of node column 0, m.
    pub x0: f64,
    /// Northing of node row 0 (the southernmost row), m.
    pub y0: f64,
    pub dx: f64,
    pub dy: f64,
    pub nx: usize,
    pub ny: usize,
}

/// Builds a TIN from grid values (row-major, southernmost row first, `nx * ny` values). A node
/// is invalid when its value is not finite or equals `nodata`.
pub fn tin_from_grid(spec: &GridSpec, values: &[f64], nodata: Option<f64>) -> Result<Tin, TerrainError> {
    if spec.nx < 2 || spec.ny < 2 {
        return Err(TerrainError::InvalidInput("a grid needs at least 2 x 2 nodes".into()));
    }
    if !(spec.dx.is_finite() && spec.dy.is_finite() && spec.dx > 0.0 && spec.dy > 0.0 && spec.x0.is_finite() && spec.y0.is_finite()) {
        return Err(TerrainError::InvalidInput("the grid spacing must be positive and the origin finite".into()));
    }
    if values.len() != spec.nx * spec.ny {
        return Err(TerrainError::InvalidInput(format!("{} values were given for a {} x {} grid", values.len(), spec.nx, spec.ny)));
    }
    let valid = |k: usize| values[k].is_finite() && nodata != Some(values[k]);
    let mut renum = vec![u32::MAX; values.len()];
    let mut pts: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut id = |k: usize, pts: &mut Vec<[f64; 3]>| -> u32 {
        if renum[k] == u32::MAX {
            renum[k] = pts.len() as u32;
            let (i, j) = (k % spec.nx, k / spec.nx);
            pts.push([spec.x0 + i as f64 * spec.dx, spec.y0 + j as f64 * spec.dy, values[k]]);
        }
        renum[k]
    };
    for j in 0..spec.ny - 1 {
        for i in 0..spec.nx - 1 {
            let (sw, se, nw, ne) = (j * spec.nx + i, j * spec.nx + i + 1, (j + 1) * spec.nx + i, (j + 1) * spec.nx + i + 1);
            let ok = [valid(sw), valid(se), valid(nw), valid(ne)];
            let bad = ok.iter().filter(|v| !**v).count();
            if bad > 1 {
                continue;
            }
            // Diagonal SW-NE when true.
            let sw_ne = if bad == 0 { (values[sw] - values[ne]).abs() <= (values[se] - values[nw]).abs() } else { !ok[1] || !ok[2] };
            let quad: [[usize; 3]; 2] = if sw_ne { [[sw, se, ne], [sw, ne, nw]] } else { [[sw, se, nw], [se, ne, nw]] };
            for t in quad {
                if t.iter().all(|&k| valid(k)) {
                    let v = [id(t[0], &mut pts), id(t[1], &mut pts), id(t[2], &mut pts)];
                    tris.push(v);
                }
            }
        }
    }
    if tris.is_empty() {
        return Err(TerrainError::NoSurface("no cell of the grid has three valid nodes".into()));
    }
    Ok(Tin::new(pts, tris)?)
}

/// A grid read from text.
#[derive(Debug, Clone, PartialEq)]
pub struct GridImport {
    pub spec: GridSpec,
    /// Southernmost row first.
    pub values: Vec<f64>,
    pub nodata: Option<f64>,
}

impl GridImport {
    pub fn to_tin(&self) -> Result<Tin, TerrainError> {
        tin_from_grid(&self.spec, &self.values, self.nodata)
    }
}

/// Reads an Esri ASCII grid. See the module documentation.
pub fn parse_esri_ascii(text: &str) -> Result<GridImport, TerrainError> {
    let bad = |m: &str| TerrainError::InvalidInput(format!("ASCII grid: {m}"));
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty()).peekable();
    let mut head: HashMap<String, f64> = HashMap::new();
    while let Some(l) = lines.peek() {
        let mut it = l.split_whitespace();
        let (Some(k), Some(v)) = (it.next(), it.next()) else { break };
        if !k.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            break;
        }
        head.insert(k.to_ascii_lowercase(), v.parse::<f64>().map_err(|_| bad(&format!("the value of {k} is not a number")))?);
        lines.next();
    }
    let get = |k: &str| head.get(k).copied();
    let (ncols, nrows) = (get("ncols").ok_or_else(|| bad("ncols is missing"))?, get("nrows").ok_or_else(|| bad("nrows is missing"))?);
    let cell = get("cellsize").ok_or_else(|| bad("cellsize is missing"))?;
    if !(ncols >= 2.0 && nrows >= 2.0 && ncols.fract() == 0.0 && nrows.fract() == 0.0 && cell.is_finite() && cell > 0.0) {
        return Err(bad("ncols and nrows must be whole numbers of at least 2 and cellsize positive"));
    }
    let (nx, ny) = (ncols as usize, nrows as usize);
    let x0 = match (get("xllcorner"), get("xllcenter")) {
        (Some(c), _) => c + cell / 2.0,
        (None, Some(c)) => c,
        _ => return Err(bad("xllcorner or xllcenter is missing")),
    };
    let y0 = match (get("yllcorner"), get("yllcenter")) {
        (Some(c), _) => c + cell / 2.0,
        (None, Some(c)) => c,
        _ => return Err(bad("yllcorner or yllcenter is missing")),
    };
    let nodata = get("nodata_value");
    let mut north_down: Vec<f64> = Vec::with_capacity(nx * ny);
    for l in lines {
        for tok in l.split_whitespace() {
            north_down.push(tok.parse::<f64>().map_err(|_| bad(&format!("'{tok}' is not a number")))?);
        }
    }
    if north_down.len() != nx * ny {
        return Err(bad(&format!("{} values were found, {} expected", north_down.len(), nx * ny)));
    }
    let mut values = Vec::with_capacity(nx * ny);
    for r in (0..ny).rev() {
        values.extend_from_slice(&north_down[r * nx..(r + 1) * nx]);
    }
    Ok(GridImport { spec: GridSpec { x0, y0, dx: cell, dy: cell, nx, ny }, values, nodata })
}

/// What a simplification did.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SimplifyReport {
    pub points_in: usize,
    pub points_kept: usize,
    /// The largest vertical distance from a dropped point to the simplified surface, m.
    pub max_error: f64,
}

#[derive(Clone, Copy)]
struct V {
    pos: Point2<f64>,
    idx: u32,
}

impl HasPosition for V {
    type Scalar = f64;
    fn position(&self) -> Point2<f64> {
        self.pos
    }
}

/// Indices of the points to keep so every dropped point is within `tolerance` (vertical, m) of
/// the surface of the kept ones. `always` lists indices that must be kept (the convex hull is
/// always kept). Points are `[x, y, z]`; duplicates in plan are not allowed.
pub fn simplify_points(points: &[[f64; 3]], tolerance: f64, always: &[usize]) -> Result<(Vec<usize>, SimplifyReport), TerrainError> {
    simplify_core(points, tolerance, always).map(|(k, r, _)| (k, r))
}

/// As [`simplify_points`], also returning the triangles (indices into `points`) of the
/// triangulation the guarantee was checked against.
fn simplify_core(points: &[[f64; 3]], tolerance: f64, always: &[usize]) -> Result<(Vec<usize>, SimplifyReport, Vec<[usize; 3]>), TerrainError> {
    if !(tolerance.is_finite() && tolerance >= 0.0) {
        return Err(TerrainError::InvalidInput("the tolerance must be zero or more".into()));
    }
    if points.len() < 3 {
        return Err(TerrainError::TooFewPoints(points.len()));
    }
    let (ox, oy) = points.iter().fold((f64::INFINITY, f64::INFINITY), |m, p| (m.0.min(p[0]), m.1.min(p[1])));
    let local = |i: usize| Point2::new(points[i][0] - ox, points[i][1] - oy);
    let mut cdt = DelaunayTriangulation::<V>::new();
    let mut selected = vec![false; points.len()];
    let insert = |cdt: &mut DelaunayTriangulation<V>, i: usize, selected: &mut Vec<bool>| -> Result<(), TerrainError> {
        if !selected[i] {
            cdt.insert(V { pos: local(i), idx: i as u32 }).map_err(|e| TerrainError::Triangulation(format!("{e:?}")))?;
            selected[i] = true;
        }
        Ok(())
    };
    // The hull of all the points: triangulate everything once to read it.
    let mut all = DelaunayTriangulation::<V>::new();
    for i in 0..points.len() {
        all.insert(V { pos: local(i), idx: i as u32 }).map_err(|e| TerrainError::Triangulation(format!("{e:?}")))?;
    }
    if all.num_inner_faces() == 0 {
        return Err(TerrainError::NoSurface("the points are all on one line".into()));
    }
    let hull: Vec<usize> = all.convex_hull().map(|e| e.from().data().idx as usize).collect();
    drop(all);
    for i in hull.into_iter().chain(always.iter().copied()) {
        insert(&mut cdt, i, &mut selected)?;
    }
    if cdt.num_inner_faces() == 0 {
        return Err(TerrainError::NoSurface("the points are all on one line".into()));
    }
    loop {
        let mut worst: HashMap<usize, (f64, usize)> = HashMap::new();
        for i in 0..points.len() {
            if selected[i] {
                continue;
            }
            let face: Option<FixedFaceHandle<spade::handles::InnerTag>> = match cdt.locate(local(i)) {
                PositionInTriangulation::OnFace(f) => Some(f),
                PositionInTriangulation::OnEdge(e) => {
                    let de = cdt.directed_edge(e);
                    de.face().as_inner().or_else(|| de.rev().face().as_inner()).map(|f| f.fix())
                }
                _ => None,
            };
            let Some(f) = face else { continue };
            let vs = cdt.face(f).vertices().map(|v| v.data().idx as usize);
            let c = vs.map(|k| [points[k][0] - ox, points[k][1] - oy, points[k][2]]);
            let Some(pl) = Plane::of(c) else { continue };
            let err = (pl.z([points[i][0] - ox, points[i][1] - oy]) - points[i][2]).abs();
            if err > tolerance {
                let e = worst.entry(f.index()).or_insert((err, i));
                if err > e.0 {
                    *e = (err, i);
                }
            }
        }
        if worst.is_empty() {
            break;
        }
        let mut add: Vec<usize> = worst.values().map(|v| v.1).collect();
        add.sort_unstable();
        for i in add {
            insert(&mut cdt, i, &mut selected)?;
        }
    }
    // Final error.
    let mut max_error = 0.0_f64;
    for i in 0..points.len() {
        if selected[i] {
            continue;
        }
        let face = match cdt.locate(local(i)) {
            PositionInTriangulation::OnFace(f) => Some(f),
            PositionInTriangulation::OnEdge(e) => {
                let de = cdt.directed_edge(e);
                de.face().as_inner().or_else(|| de.rev().face().as_inner()).map(|f| f.fix())
            }
            _ => None,
        };
        if let Some(f) = face {
            let c = cdt.face(f).vertices().map(|v| {
                let k = v.data().idx as usize;
                [points[k][0] - ox, points[k][1] - oy, points[k][2]]
            });
            if let Some(pl) = Plane::of(c) {
                max_error = max_error.max((pl.z([points[i][0] - ox, points[i][1] - oy]) - points[i][2]).abs());
            }
        }
    }
    let kept: Vec<usize> = (0..points.len()).filter(|&i| selected[i]).collect();
    let report = SimplifyReport { points_in: points.len(), points_kept: kept.len(), max_error };
    let tris: Vec<[usize; 3]> = cdt.inner_faces().map(|f| f.vertices().map(|v| v.data().idx as usize)).collect();
    Ok((kept, report, tris))
}

/// Simplifies a TIN to a coarser one within a vertical tolerance (see the module documentation).
///
/// The vertices on the boundary of the surface are always kept, the kept points are re-triangulated
/// (Delaunay), and triangles that fall outside the original surface (a concave bay, a hole) are
/// dropped, so the outline is kept at the vertex level but a boundary edge of the original may be
/// replaced by a slightly different chain of edges. The vertical guarantee holds at the original
/// vertices only.
pub fn simplify_tin(tin: &Tin, tolerance: f64) -> Result<(Tin, SimplifyReport), TerrainError> {
    // Points in use, renumbered.
    let mut map = vec![u32::MAX; tin.points().len()];
    let mut pts: Vec<[f64; 3]> = Vec::new();
    for &i in tin.triangles().iter().flatten() {
        if map[i as usize] == u32::MAX {
            map[i as usize] = pts.len() as u32;
            pts.push(tin.points()[i as usize]);
        }
    }
    let always: Vec<usize> = tin.boundary_edges().iter().flatten().map(|&i| map[i as usize] as usize).collect();
    let (kept, report, faces) = simplify_core(&pts, tolerance, &always)?;
    let mut rank = vec![u32::MAX; pts.len()];
    let sel: Vec<[f64; 3]> = kept.iter().enumerate().map(|(r, &i)| {
        rank[i] = r as u32;
        pts[i]
    }).collect();
    let mut tris = Vec::new();
    for f in faces {
        let [a, mut b, mut c] = f.map(|i| rank[i]);
        let q = |i: u32| [sel[i as usize][0], sel[i as usize][1]];
        let o = orient(q(a), q(b), q(c));
        if o == 0.0 || area2(sel[a as usize], sel[b as usize], sel[c as usize]) == 0.0 {
            continue;
        }
        if o < 0.0 {
            std::mem::swap(&mut b, &mut c);
        }
        let cen = [(sel[a as usize][0] + sel[b as usize][0] + sel[c as usize][0]) / 3.0, (sel[a as usize][1] + sel[b as usize][1] + sel[c as usize][1]) / 3.0];
        if tin.elevation_at(cen[0], cen[1]).is_some() {
            tris.push([a, b, c]);
        }
    }
    if tris.is_empty() {
        return Err(TerrainError::NoSurface("no triangle of the simplified surface lies on the original".into()));
    }
    // Only the points in use.
    let mut renum = vec![u32::MAX; sel.len()];
    let mut used = Vec::new();
    for c in tris.iter_mut().flatten() {
        if renum[*c as usize] == u32::MAX {
            renum[*c as usize] = used.len() as u32;
            used.push(sel[*c as usize]);
        }
        *c = renum[*c as usize];
    }
    Ok((Tin::new(used, tris)?, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_to_tin_matches_the_values() {
        let spec = GridSpec { x0: 500_000.0, y0: 9_000_000.0, dx: 5.0, dy: 5.0, nx: 4, ny: 3 };
        let values: Vec<f64> = (0..3).flat_map(|j| (0..4).map(move |i| 10.0 + i as f64 + 2.0 * j as f64)).collect();
        let t = tin_from_grid(&spec, &values, None).unwrap();
        assert_eq!(t.triangles().len(), 12);
        assert!((t.plan_area() - 15.0 * 10.0).abs() < 1e-9);
        assert!((t.elevation_at(500_005.0 + 1.0, 9_000_005.0).unwrap() - (11.0 + 2.0 + 0.2)).abs() < 1e-9);
        assert!(t.points().iter().any(|p| p[0] == 500_015.0 && p[1] == 9_000_010.0 && p[2] == 10.0 + 3.0 + 4.0));
        assert!(tin_from_grid(&spec, &values[1..], None).is_err());
        assert!(tin_from_grid(&GridSpec { nx: 1, ..spec }, &values, None).is_err());
    }

    #[test]
    fn nodata_cells_are_left_out_or_become_one_triangle() {
        let spec = GridSpec { x0: 0.0, y0: 0.0, dx: 1.0, dy: 1.0, nx: 3, ny: 3 };
        let mut v = vec![1.0; 9];
        v[8] = -9999.0; // NE corner node
        let t = tin_from_grid(&spec, &v, Some(-9999.0)).unwrap();
        // 3 full cells (6 triangles) plus one triangle in the cell with the missing corner.
        assert_eq!(t.triangles().len(), 7);
        assert!((t.plan_area() - 3.5).abs() < 1e-12);
        let mut v2 = vec![1.0; 9];
        v2[8] = f64::NAN;
        v2[7] = f64::NAN;
        let t2 = tin_from_grid(&spec, &v2, None).unwrap();
        assert!(t2.plan_area() < 3.5);
        assert!(tin_from_grid(&spec, &[f64::NAN; 9], None).is_err());
        // Each corner of a cell being the missing one still leaves a valid triangle.
        for bad in [0usize, 1, 2, 3] {
            let mut w = vec![1.0; 4];
            w[bad] = f64::NAN;
            let t = tin_from_grid(&GridSpec { nx: 2, ny: 2, ..spec }, &w, None).unwrap();
            assert_eq!(t.triangles().len(), 1);
            assert!((t.plan_area() - 0.5).abs() < 1e-12);
        }
    }

    #[test]
    fn esri_ascii_is_read_north_down() {
        let text = "ncols 3\nnrows 2\nxllcorner 1000\nyllcorner 2000\ncellsize 10\nNODATA_value -9999\n5 6 7\n1 2 -9999\n";
        let g = parse_esri_ascii(text).unwrap();
        assert_eq!(g.spec.x0, 1005.0);
        assert_eq!(g.spec.y0, 2005.0);
        assert_eq!(g.values, vec![1.0, 2.0, -9999.0, 5.0, 6.0, 7.0]);
        let t = g.to_tin().unwrap();
        // One full cell (2 triangles) and one cell with a missing corner (1 triangle).
        assert_eq!(t.triangles().len(), 3);
    }

    #[test]
    fn esri_ascii_errors() {
        assert!(parse_esri_ascii("ncols 3\nnrows 2\ncellsize 1\n1 2 3 4 5 6").is_err());
        assert!(parse_esri_ascii("ncols 3\nnrows 2\nxllcenter 0\nyllcenter 0\ncellsize 1\n1 2 3 4 5").is_err());
        assert!(parse_esri_ascii("ncols 3\nnrows 2\nxllcenter 0\nyllcenter 0\ncellsize 1\n1 2 x 4 5 6").is_err());
        let ok = parse_esri_ascii("ncols 2\nnrows 2\nxllcenter 7\nyllcenter 8\ncellsize 2\n1 2\n3 4").unwrap();
        assert_eq!((ok.spec.x0, ok.spec.y0), (7.0, 8.0));
    }

    #[test]
    fn simplify_drops_flat_detail_and_keeps_the_guarantee() {
        // A 41 x 41 grid of a plane with one bump: the plane needs 4 points, the bump a few more.
        let (ox, oy) = (500_000.0, 9_000_000.0);
        let z = |x: f64, y: f64| 0.05 * x + 0.02 * y + 3.0 * (-(((x - 50.0).powi(2) + (y - 50.0).powi(2)) / 50.0)).exp();
        let t = Tin::from_grid(ox, oy, 2.5, 2.5, 41, 41, |x, y| z(x - ox, y - oy)).unwrap();
        let (s, r) = simplify_tin(&t, 0.05).unwrap();
        assert!(r.points_kept < 1681 / 3, "{r:?}");
        assert!(r.max_error <= 0.05 + 1e-12, "{r:?}");
        assert!((s.plan_area() - t.plan_area()).abs() < 1e-6);
        // Independent check of the guarantee at every original vertex.
        for p in t.points() {
            let zs = s.elevation_at(p[0], p[1]).unwrap();
            assert!((zs - p[2]).abs() <= 0.05 + 1e-9, "{p:?} {zs}");
        }
        // Zero tolerance keeps everything that matters (a plane with no bump needs only the hull).
        let plane = Tin::from_grid(0.0, 0.0, 1.0, 1.0, 10, 10, |x, y| 0.3 * x - 0.1 * y).unwrap();
        let (sp, rp) = simplify_tin(&plane, 1e-9).unwrap();
        assert!(rp.points_kept <= 4 * 9 + 4 && sp.triangles().len() < plane.triangles().len() / 3 + 40, "{rp:?}");
        assert!(simplify_tin(&plane, -1.0).is_err());
    }

    #[test]
    fn simplify_points_refuses_degenerate_input() {
        let line: Vec<[f64; 3]> = (0..5).map(|i| [i as f64, i as f64, 0.0]).collect();
        assert!(simplify_points(&line, 0.1, &[]).is_err());
        assert!(simplify_points(&line[..2], 0.1, &[]).is_err());
    }
}
