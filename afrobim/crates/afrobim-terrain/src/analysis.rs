//! Analysis of a TIN: slope and aspect, elevation and slope bands, slope arrows, flow paths,
//! sections, extremes and areas.
//!
//! Every triangle is a plane, so slope and aspect are exact per triangle (no grid window, no
//! smoothing). With the plane `z = z0 + gx (x - x0) + gy (y - y0)`:
//!
//! * slope angle = `atan(hypot(gx, gy))`, slope percent = `100 hypot(gx, gy)`;
//! * aspect = the compass bearing of the steepest descent, clockwise from grid north, in
//!   degrees `[0, 360)`: `atan2(-gx, -gy)`. This is the usual "downslope direction" definition
//!   (see P. A. Burrough and R. A. McDonnell, *Principles of Geographical Information Systems*,
//!   Oxford University Press, 1998). A horizontal triangle has no aspect.
//!
//! "North" is the +y axis of the coordinate system the surface is in (grid north), not true
//! north.

use std::collections::HashMap;

use afrobim_corridor::Tin;
use serde::{Deserialize, Serialize};

use crate::error::TerrainError;
use crate::geom::{area2, clip_by, integrate_affine, orient, poly_area, Plane};

/// Slope and aspect of one triangle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TriangleSlope {
    /// Index into [`Tin::triangles`].
    pub triangle: usize,
    pub slope_degrees: f64,
    pub slope_percent: f64,
    /// Compass bearing of steepest descent, degrees clockwise from grid north; `None` if flat.
    pub aspect_degrees: Option<f64>,
    /// Plan area, m².
    pub plan_area: f64,
    /// Area of the sloping facet, m²: `plan_area * sqrt(1 + gx² + gy²)`.
    pub surface_area: f64,
}

fn corners(tin: &Tin, t: usize) -> [[f64; 3]; 3] {
    tin.triangles()[t].map(|i| tin.points()[i as usize])
}

/// Slope, aspect and areas of every triangle, in triangle order.
pub fn triangle_slopes(tin: &Tin) -> Vec<TriangleSlope> {
    (0..tin.triangles().len())
        .map(|t| {
            let c = corners(tin, t);
            let plan_area = area2(c[0], c[1], c[2]).abs() / 2.0;
            let (gx, gy) = Plane::of(c).map(|p| (p.gx, p.gy)).unwrap_or((0.0, 0.0));
            let g = gx.hypot(gy);
            let aspect = (g > 0.0).then(|| (-gx).atan2(-gy).to_degrees().rem_euclid(360.0));
            TriangleSlope { triangle: t, slope_degrees: g.atan().to_degrees(), slope_percent: 100.0 * g, aspect_degrees: aspect, plan_area, surface_area: plan_area * (1.0 + g * g).sqrt() }
        })
        .collect()
}

/// A band of elevation or slope: the plan polygons where the value is in `[lower, upper)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Band {
    /// Lower limit (inclusive); `None` is open (no lower limit).
    pub lower: Option<f64>,
    /// Upper limit (exclusive); `None` is open.
    pub upper: Option<f64>,
    /// Convex polygons `[x, y]` (3 to 5 vertices, counter-clockwise), one or more per triangle.
    pub polygons: Vec<Vec<[f64; 2]>>,
    /// Plan area of the band, m².
    pub plan_area: f64,
    /// Area of the sloping surface in the band, m².
    pub surface_area: f64,
}

/// Evenly spaced break values `first, first + step, ...` up to and including the last one not
/// above `last`. For use with the banding functions.
pub fn regular_breaks(first: f64, last: f64, step: f64) -> Result<Vec<f64>, TerrainError> {
    if !(first.is_finite() && last.is_finite() && step.is_finite() && step > 0.0 && last >= first) || (last - first) / step > 1.0e6 {
        return Err(TerrainError::InvalidInput("break values need finite limits, last >= first and a positive step with at most a million breaks".into()));
    }
    let n = ((last - first) / step + 1e-9).floor() as usize;
    Ok((0..=n).map(|k| first + k as f64 * step).collect())
}

fn check_breaks(breaks: &[f64]) -> Result<(), TerrainError> {
    if breaks.iter().any(|b| !b.is_finite()) || breaks.windows(2).any(|w| w[1] <= w[0]) {
        return Err(TerrainError::InvalidInput("break values must be finite and strictly increasing".into()));
    }
    Ok(())
}

fn new_bands(breaks: &[f64]) -> Vec<Band> {
    (0..=breaks.len())
        .map(|i| Band { lower: i.checked_sub(1).map(|k| breaks[k]), upper: breaks.get(i).copied(), polygons: Vec::new(), plan_area: 0.0, surface_area: 0.0 })
        .collect()
}

/// Band index of a value: the number of breaks at or below it.
fn band_of(breaks: &[f64], v: f64) -> usize {
    breaks.partition_point(|b| *b <= v)
}

/// Elevation bands: `n` break values give `n + 1` bands, the first open below and the last open
/// above. Each triangle is cut where its plane crosses a break (Sutherland and Hodgman polygon
/// clipping against the two half planes `z >= lower` and `z < upper`), so band edges follow the
/// contour lines exactly.
pub fn elevation_bands(tin: &Tin, breaks: &[f64]) -> Result<Vec<Band>, TerrainError> {
    check_breaks(breaks)?;
    let mut bands = new_bands(breaks);
    for t in 0..tin.triangles().len() {
        let c = corners(tin, t);
        let Some(plane) = Plane::of(c) else { continue };
        let poly: Vec<[f64; 2]> = c.iter().map(|p| [p[0], p[1]]).collect();
        let (zlo, zhi) = (c[0][2].min(c[1][2]).min(c[2][2]), c[0][2].max(c[1][2]).max(c[2][2]));
        for b in band_of(breaks, zlo)..=band_of(breaks, zhi) {
            let mut p = poly.clone();
            if let Some(lo) = bands[b].lower {
                p = clip_by(&p, |q| plane.z(q) - lo);
            }
            if let Some(hi) = bands[b].upper {
                p = clip_by(&p, |q| hi - plane.z(q));
            }
            let a = poly_area(&p);
            if a > 0.0 {
                let g = plane.gx.hypot(plane.gy);
                bands[b].plan_area += a;
                bands[b].surface_area += a * (1.0 + g * g).sqrt();
                bands[b].polygons.push(ccw(p));
            }
        }
    }
    Ok(bands)
}

fn ccw(mut p: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    if crate::geom::polygon_area2(&p) < 0.0 {
        p.reverse();
    }
    p
}

/// Unit of a slope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlopeUnit {
    Degrees,
    Percent,
}

/// Slope bands: each triangle is put whole in the band of its slope (a triangle has one slope).
/// `breaks` are in `unit`; `n` breaks give `n + 1` bands, open at both ends.
pub fn slope_bands(tin: &Tin, breaks: &[f64], unit: SlopeUnit) -> Result<Vec<Band>, TerrainError> {
    check_breaks(breaks)?;
    let mut bands = new_bands(breaks);
    for s in triangle_slopes(tin) {
        let v = match unit {
            SlopeUnit::Degrees => s.slope_degrees,
            SlopeUnit::Percent => s.slope_percent,
        };
        let b = band_of(breaks, v);
        let c = corners(tin, s.triangle);
        bands[b].polygons.push(ccw(c.iter().map(|p| [p[0], p[1]]).collect()));
        bands[b].plan_area += s.plan_area;
        bands[b].surface_area += s.surface_area;
    }
    Ok(bands)
}

/// An arrow for plotting the direction of fall.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SlopeArrow {
    /// Where the arrow starts: the centroid of the triangle.
    pub tail: [f64; 2],
    /// The head, `length` metres downslope of the tail.
    pub head: [f64; 2],
    pub slope_percent: f64,
    pub aspect_degrees: f64,
}

/// One arrow per triangle whose slope is at least `min_slope_percent`, pointing downslope, each
/// `length` metres long in plan.
pub fn slope_arrows(tin: &Tin, length: f64, min_slope_percent: f64) -> Result<Vec<SlopeArrow>, TerrainError> {
    if !(length.is_finite() && length > 0.0 && min_slope_percent.is_finite()) {
        return Err(TerrainError::InvalidInput("the arrow length must be a positive number".into()));
    }
    let mut out = Vec::new();
    for s in triangle_slopes(tin) {
        let Some(aspect) = s.aspect_degrees else { continue };
        if s.slope_percent < min_slope_percent {
            continue;
        }
        let c = corners(tin, s.triangle);
        let tail = [(c[0][0] + c[1][0] + c[2][0]) / 3.0, (c[0][1] + c[1][1] + c[2][1]) / 3.0];
        let (sa, ca) = aspect.to_radians().sin_cos();
        out.push(SlopeArrow { tail, head: [tail[0] + length * sa, tail[1] + length * ca], slope_percent: s.slope_percent, aspect_degrees: aspect });
    }
    Ok(out)
}

/// Plan and surface area of a TIN, m².
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Areas {
    pub plan: f64,
    /// The area of the facets themselves (always at least the plan area).
    pub surface: f64,
}

pub fn areas(tin: &Tin) -> Areas {
    let s = triangle_slopes(tin);
    Areas { plan: s.iter().map(|t| t.plan_area).sum(), surface: s.iter().map(|t| t.surface_area).sum() }
}

/// Lowest and highest points of the surface.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Extremes {
    /// `[x, y, z]` of the lowest vertex.
    pub lowest: [f64; 3],
    pub highest: [f64; 3],
}

/// The lowest and highest vertices. On a TIN the extremes of the surface are at vertices.
pub fn extremes(tin: &Tin) -> Extremes {
    let mut used = tin.triangles().iter().flatten().map(|&i| tin.points()[i as usize]);
    let first = used.next().unwrap_or([0.0; 3]);
    let (mut lo, mut hi) = (first, first);
    for p in used {
        if p[2] < lo[2] {
            lo = p;
        }
        if p[2] > hi[2] {
            hi = p;
        }
    }
    Extremes { lowest: lo, highest: hi }
}

/// The elevation of the surface at a plan point, m; `None` outside the surface.
pub fn spot_elevation(tin: &Tin, x: f64, y: f64) -> Option<f64> {
    tin.elevation_at(x, y)
}

/// The ground along a polyline in plan: runs of `(chainage, elevation)`, chainage measured along
/// the polyline from its first vertex. A run ends where the line leaves the surface and a new
/// one starts where it re-enters; runs that meet at a polyline vertex are joined.
pub fn section_along(tin: &Tin, polyline: &[[f64; 2]]) -> Vec<Vec<(f64, f64)>> {
    let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut c0 = 0.0;
    for w in polyline.windows(2) {
        let len = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
        for run in tin.section((w[0][0], w[0][1]), (w[1][0], w[1][1])) {
            let run: Vec<(f64, f64)> = run.into_iter().map(|(d, z)| (c0 + d, z)).collect();
            match runs.last_mut() {
                Some(last) if last.last().is_some_and(|l| (l.0 - run[0].0).abs() < 1e-9) => {
                    last.extend(run.into_iter().skip(1));
                }
                _ => runs.push(run),
            }
        }
        c0 += len;
    }
    runs
}

/// How a flow path ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowStop {
    /// At a vertex with nothing lower around it on the surface: a pit (a local minimum of the
    /// TIN), or the lowest point on the surface's edge, where the water would leave it.
    Pit,
    /// On a horizontal facet or edge, with no direction of fall.
    Flat,
    /// Off the edge of the surface (or into a hole).
    LeftSurface,
    /// The start point is not on the surface.
    StartOutside,
}

/// A steepest-descent path over a TIN.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowPath {
    /// `[x, y, z]` along the path, from the start.
    pub points: Vec<[f64; 3]>,
    pub stop: FlowStop,
    /// Plan length, m.
    pub plan_length: f64,
    /// Length along the surface, m.
    pub length_3d: f64,
}

struct Topo {
    tris: Vec<[u32; 3]>,
    planes: Vec<Option<Plane>>,
    adj: Vec<[Option<u32>; 3]>,
    fan: Vec<Vec<u32>>,
    grid: Locator,
}

struct Locator {
    min: [f64; 2],
    cell: [f64; 2],
    n: [usize; 2],
    cells: Vec<Vec<u32>>,
}

impl Topo {
    fn new(tin: &Tin) -> Topo {
        let pts = tin.points();
        let tris: Vec<[u32; 3]> = tin
            .triangles()
            .iter()
            .map(|&[a, b, c]| if area2(pts[a as usize], pts[b as usize], pts[c as usize]) < 0.0 { [a, c, b] } else { [a, b, c] })
            .collect();
        let planes = tris.iter().map(|t| Plane::of(t.map(|i| pts[i as usize]))).collect();
        let mut edge: HashMap<(u32, u32), (u32, u8)> = HashMap::new();
        let mut adj = vec![[None; 3]; tris.len()];
        let mut fan = vec![Vec::new(); pts.len()];
        for (ti, t) in tris.iter().enumerate() {
            for i in 0..3 {
                fan[t[i] as usize].push(ti as u32);
                let (a, b) = (t[i], t[(i + 1) % 3]);
                // The neighbour across edge (a, b) holds the directed edge (b, a).
                if let Some(&(tj, j)) = edge.get(&(b, a)) {
                    adj[ti][i] = Some(tj);
                    adj[tj as usize][j as usize] = Some(ti as u32);
                } else {
                    edge.insert((a, b), (ti as u32, i as u8));
                }
            }
        }
        let b = tin.bounds();
        let n = tris.len() as f64;
        let (w, h) = ((b.max_x - b.min_x).max(1e-9), (b.max_y - b.min_y).max(1e-9));
        let nx = ((n * w / h).sqrt().ceil() as usize).clamp(1, 2048);
        let ny = ((n / nx as f64).ceil() as usize).clamp(1, 2048);
        let mut grid = Locator { min: [b.min_x, b.min_y], cell: [w / nx as f64, h / ny as f64], n: [nx, ny], cells: vec![Vec::new(); nx * ny] };
        for (ti, t) in tris.iter().enumerate() {
            let c = t.map(|i| pts[i as usize]);
            let lo = [c.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min), c.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min)];
            let hi = [c.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max), c.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max)];
            let (i0, j0) = grid.at(lo);
            let (i1, j1) = grid.at(hi);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    grid.cells[j * nx + i].push(ti as u32);
                }
            }
        }
        Topo { tris, planes, adj, fan, grid }
    }

    fn locate(&self, pts: &[[f64; 3]], p: [f64; 2]) -> Option<u32> {
        let b = &self.grid;
        if p[0] < b.min[0] - 1e-9 || p[1] < b.min[1] - 1e-9 || p[0] > b.min[0] + b.cell[0] * b.n[0] as f64 + 1e-9 || p[1] > b.min[1] + b.cell[1] * b.n[1] as f64 + 1e-9 {
            return None;
        }
        let (i, j) = b.at(p);
        b.cells[j * b.n[0] + i].iter().copied().find(|&t| {
            let c = self.tris[t as usize].map(|v| [pts[v as usize][0], pts[v as usize][1]]);
            (0..3).all(|k| orient(c[k], c[(k + 1) % 3], p) >= 0.0)
        })
    }
}

impl Locator {
    fn at(&self, p: [f64; 2]) -> (usize, usize) {
        let i = (((p[0] - self.min[0]) / self.cell[0]).max(0.0) as usize).min(self.n[0] - 1);
        let j = (((p[1] - self.min[1]) / self.cell[1]).max(0.0) as usize).min(self.n[1] - 1);
        (i, j)
    }
}

enum At {
    Tri { t: u32, entry: Option<usize> },
    Vertex(u32),
}

/// Traces the path water would take from a plan point: steepest descent across the facets.
///
/// On a facet the path runs in the direction of fall of its plane until it meets an edge, and
/// carries on in the facet across that edge. Where both facets of an edge fall towards it (a
/// valley) the path follows the edge down to its lower end. At a vertex it takes the steepest
/// way down among the incident edges and facets. It ends at a pit, on flat ground or at the edge
/// of the surface. This is the steepest-descent flow-path construction on a TIN described by
/// N. L. Jones, S. G. Wright and D. R. Maidment, "Watershed delineation with triangle-based
/// terrain models", Journal of Hydraulic Engineering 116(10), 1990, pp. 1232-1251, without
/// their treatment of flow dispersion: water on a ridge goes one way, never splits. Pits are not
/// filled, so the path stops in every depression, however small.
pub fn flow_path(tin: &Tin, x: f64, y: f64) -> FlowPath {
    let pts = tin.points();
    let topo = Topo::new(tin);
    let mut path: Vec<[f64; 3]> = Vec::new();
    let finish = |path: Vec<[f64; 3]>, stop: FlowStop| {
        let (mut pl, mut l3) = (0.0, 0.0);
        for w in path.windows(2) {
            let (dx, dy, dz) = (w[1][0] - w[0][0], w[1][1] - w[0][1], w[1][2] - w[0][2]);
            pl += dx.hypot(dy);
            l3 += (dx * dx + dy * dy + dz * dz).sqrt();
        }
        FlowPath { points: path, stop, plan_length: pl, length_3d: l3 }
    };
    let Some(t0) = topo.locate(pts, [x, y]) else { return finish(path, FlowStop::StartOutside) };
    let z0 = topo.planes[t0 as usize].map(|p| p.z([x, y])).unwrap_or(0.0);
    let mut p = [x, y, z0];
    path.push(p);
    let mut at = match topo.tris[t0 as usize].iter().find(|&&v| pts[v as usize][0] == x && pts[v as usize][1] == y) {
        Some(&v) => {
            p = pts[v as usize];
            path[0] = p;
            At::Vertex(v)
        }
        None => At::Tri { t: t0, entry: None },
    };
    let limit = 4 * topo.tris.len() + 16;
    for _ in 0..limit {
        match at {
            At::Tri { t, entry } => {
                let tri = topo.tris[t as usize];
                let Some(plane) = topo.planes[t as usize] else { return finish(path, FlowStop::Flat) };
                let g = plane.gx.hypot(plane.gy);
                if g == 0.0 {
                    return finish(path, FlowStop::Flat);
                }
                let d = [-plane.gx / g, -plane.gy / g];
                let scale = (0..3).map(|i| (pts[tri[i] as usize][0] - pts[tri[(i + 1) % 3] as usize][0]).hypot(pts[tri[i] as usize][1] - pts[tri[(i + 1) % 3] as usize][1])).fold(0.0, f64::max);
                let mut best: Option<(f64, usize, f64)> = None;
                for i in 0..3 {
                    if entry == Some(i) {
                        continue;
                    }
                    let (a, b) = (pts[tri[i] as usize], pts[tri[(i + 1) % 3] as usize]);
                    let (ra, e) = ([a[0] - p[0], a[1] - p[1]], [b[0] - a[0], b[1] - a[1]]);
                    let den = d[0] * e[1] - d[1] * e[0];
                    if den == 0.0 {
                        continue;
                    }
                    let s = (ra[0] * e[1] - ra[1] * e[0]) / den;
                    let u = (ra[0] * d[1] - ra[1] * d[0]) / den;
                    if s > 1e-12 * scale && (-1e-12..=1.0 + 1e-12).contains(&u) && best.is_none_or(|b| s < b.0) {
                        best = Some((s, i, u.clamp(0.0, 1.0)));
                    }
                }
                match best {
                    Some((_, i, u)) => {
                        let (a, b) = (pts[tri[i] as usize], pts[tri[(i + 1) % 3] as usize]);
                        if u <= 1e-9 || u >= 1.0 - 1e-9 {
                            let v = if u <= 1e-9 { tri[i] } else { tri[(i + 1) % 3] };
                            p = pts[v as usize];
                            path.push(p);
                            at = At::Vertex(v);
                        } else {
                            p = [a[0] + u * (b[0] - a[0]), a[1] + u * (b[1] - a[1]), a[2] + u * (b[2] - a[2])];
                            path.push(p);
                            match topo.adj[t as usize][i] {
                                None => return finish(path, FlowStop::LeftSurface),
                                Some(n) => {
                                    let nt = topo.tris[n as usize];
                                    let j = (0..3).find(|&j| nt[j] == tri[(i + 1) % 3] && nt[(j + 1) % 3] == tri[i]).unwrap_or(0);
                                    at = At::Tri { t: n, entry: Some(j) };
                                }
                            }
                        }
                    }
                    None => match entry {
                        // Both facets fall towards the shared edge: follow it down.
                        Some(e) => {
                            let (va, vb) = (tri[e], tri[(e + 1) % 3]);
                            let (za, zb) = (pts[va as usize][2], pts[vb as usize][2]);
                            if za == zb {
                                return finish(path, FlowStop::Flat);
                            }
                            let v = if za < zb { va } else { vb };
                            p = pts[v as usize];
                            path.push(p);
                            at = At::Vertex(v);
                        }
                        // Started on an edge and the fall is out of this facet: use the facet
                        // across it.
                        None => {
                            let q = [p[0], p[1]];
                            let on = (0..3).find(|&i| orient([pts[tri[i] as usize][0], pts[tri[i] as usize][1]], [pts[tri[(i + 1) % 3] as usize][0], pts[tri[(i + 1) % 3] as usize][1]], q) == 0.0);
                            match on.and_then(|i| topo.adj[t as usize][i].map(|n| (i, n))) {
                                Some((i, n)) => {
                                    let nt = topo.tris[n as usize];
                                    let j = (0..3).find(|&j| nt[j] == tri[(i + 1) % 3] && nt[(j + 1) % 3] == tri[i]).unwrap_or(0);
                                    at = At::Tri { t: n, entry: Some(j) };
                                }
                                None => return finish(path, FlowStop::LeftSurface),
                            }
                        }
                    },
                }
            }
            At::Vertex(v) => {
                let pv = pts[v as usize];
                let mut best = (0.0_f64, None::<At>);
                for &t in &topo.fan[v as usize] {
                    let tri = topo.tris[t as usize];
                    let k = (0..3).find(|&k| tri[k] == v).unwrap();
                    let (w1, w2) = (pts[tri[(k + 1) % 3] as usize], pts[tri[(k + 2) % 3] as usize]);
                    if let Some(pl) = topo.planes[t as usize] {
                        let g = pl.gx.hypot(pl.gy);
                        let d = [-pl.gx, -pl.gy];
                        let (e1, e2) = ([w1[0] - pv[0], w1[1] - pv[1]], [w2[0] - pv[0], w2[1] - pv[1]]);
                        // Strictly inside the wedge between the two edges (counter-clockwise).
                        if g > best.0 && e1[0] * d[1] - e1[1] * d[0] > 0.0 && d[0] * e2[1] - d[1] * e2[0] > 0.0 {
                            best = (g, Some(At::Tri { t, entry: None }));
                        }
                    }
                    for w in [tri[(k + 1) % 3], tri[(k + 2) % 3]] {
                        let q = pts[w as usize];
                        let drop = (pv[2] - q[2]) / (q[0] - pv[0]).hypot(q[1] - pv[1]);
                        if drop > best.0 {
                            best = (drop, Some(At::Vertex(w)));
                        }
                    }
                }
                match best.1 {
                    None => return finish(path, FlowStop::Pit),
                    Some(next) => {
                        if let At::Vertex(w) = next {
                            p = pts[w as usize];
                            path.push(p);
                        }
                        at = next;
                    }
                }
            }
        }
    }
    finish(path, FlowStop::Flat)
}

/// Mean elevation of a TIN weighted by plan area, m. Convenience for reports.
pub fn mean_elevation(tin: &Tin) -> f64 {
    let (mut a, mut v) = (0.0, 0.0);
    for t in 0..tin.triangles().len() {
        let c = corners(tin, t);
        let poly: Vec<[f64; 2]> = c.iter().map(|p| [p[0], p[1]]).collect();
        if let Some(pl) = Plane::of(c) {
            let (ar, int) = integrate_affine(&poly, |q| pl.z(q));
            a += ar;
            v += int;
        }
    }
    if a > 0.0 {
        v / a
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tilted(ox: f64, oy: f64) -> Tin {
        // z = 0.1 x + 0.05 y over 100 x 60 m, 10 m grid.
        Tin::from_grid(ox, oy, 10.0, 10.0, 11, 7, |x, y| 0.1 * (x - ox) + 0.05 * (y - oy)).unwrap()
    }

    #[test]
    fn slope_and_aspect_of_a_plane() {
        let s = triangle_slopes(&tilted(500_000.0, 9_000_000.0));
        let g = (0.1f64 * 0.1 + 0.05 * 0.05).sqrt();
        for t in &s {
            assert!((t.slope_percent - 100.0 * g).abs() < 1e-6);
            assert!((t.slope_degrees - g.atan().to_degrees()).abs() < 1e-9);
            // Falls towards -x and -y: south-west, bearing 180 + atan(0.1/0.05).
            let want = 180.0 + (0.1f64).atan2(0.05).to_degrees();
            assert!((t.aspect_degrees.unwrap() - want).abs() < 1e-6, "{:?}", t);
            assert!((t.surface_area - t.plan_area * (1.0 + g * g).sqrt()).abs() < 1e-9);
        }
        let flat = Tin::from_grid(0.0, 0.0, 1.0, 1.0, 3, 3, |_, _| 5.0).unwrap();
        assert!(triangle_slopes(&flat).iter().all(|t| t.aspect_degrees.is_none() && t.slope_percent == 0.0));
        // Falls to the north (aspect 0) and to the east (aspect 90).
        let n = Tin::from_grid(0.0, 0.0, 1.0, 1.0, 3, 3, |_, y| -y).unwrap();
        assert!(triangle_slopes(&n).iter().all(|t| t.aspect_degrees.unwrap().abs() < 1e-9));
        let e = Tin::from_grid(0.0, 0.0, 1.0, 1.0, 3, 3, |x, _| -x).unwrap();
        assert!(triangle_slopes(&e).iter().all(|t| (t.aspect_degrees.unwrap() - 90.0).abs() < 1e-9));
    }

    #[test]
    fn areas_extremes_and_spot() {
        let t = tilted(0.0, 0.0);
        let a = areas(&t);
        let g = (0.1f64 * 0.1 + 0.05 * 0.05).sqrt();
        assert!((a.plan - 6000.0).abs() < 1e-9);
        assert!((a.surface - 6000.0 * (1.0 + g * g).sqrt()).abs() < 1e-6);
        let e = extremes(&t);
        assert_eq!(e.lowest, [0.0, 0.0, 0.0]);
        assert_eq!(e.highest, [100.0, 60.0, 13.0]);
        assert!((spot_elevation(&t, 33.0, 21.0).unwrap() - (3.3 + 1.05)).abs() < 1e-12);
        assert!(spot_elevation(&t, -1.0, 0.0).is_none());
        assert!((mean_elevation(&t) - (5.0 + 1.5)).abs() < 1e-9);
    }

    #[test]
    fn elevation_bands_partition_the_area() {
        let t = tilted(500_000.0, 9_000_000.0);
        let bands = elevation_bands(&t, &[2.0, 6.0, 10.0]).unwrap();
        assert_eq!(bands.len(), 4);
        let total: f64 = bands.iter().map(|b| b.plan_area).sum();
        assert!((total - 6000.0).abs() < 1e-6);
        // z = 0.1 x + 0.05 y. Area below z = 2: triangle x/20 + y/40 <= 1 inside the box: 20*40/2.
        assert!((bands[0].plan_area - 400.0).abs() < 1e-6, "{}", bands[0].plan_area);
        assert_eq!(bands[0].lower, None);
        assert_eq!(bands[3].upper, None);
        assert!(bands.iter().flat_map(|b| &b.polygons).all(|p| p.len() >= 3 && crate::geom::polygon_area2(p) > 0.0));
        assert!(elevation_bands(&t, &[3.0, 3.0]).is_err());
        assert!(elevation_bands(&t, &[f64::NAN]).is_err());
        assert_eq!(regular_breaks(0.0, 1.0, 0.25).unwrap(), vec![0.0, 0.25, 0.5, 0.75, 1.0]);
        assert!(regular_breaks(0.0, 1.0, 0.0).is_err());
    }

    #[test]
    fn slope_bands_and_arrows() {
        // Left half falls at 10 %, right half at 30 %.
        let t = Tin::from_grid(0.0, 0.0, 10.0, 10.0, 5, 3, |x, _| if x <= 20.0 { 0.1 * x } else { 2.0 + 0.3 * (x - 20.0) }).unwrap();
        let bands = slope_bands(&t, &[20.0], SlopeUnit::Percent).unwrap();
        assert!((bands[0].plan_area - 400.0).abs() < 1e-9);
        assert!((bands[1].plan_area - 400.0).abs() < 1e-9);
        let deg = slope_bands(&t, &[10.0], SlopeUnit::Degrees).unwrap();
        assert!((deg[0].plan_area - 400.0).abs() < 1e-9); // atan(0.1) = 5.7 degrees, atan(0.3) = 16.7
        let arrows = slope_arrows(&t, 3.0, 5.0).unwrap();
        assert_eq!(arrows.len(), 16);
        let a = arrows[0];
        assert!((a.aspect_degrees - 270.0).abs() < 1e-9); // falls west
        assert!((a.head[0] - (a.tail[0] - 3.0)).abs() < 1e-9 && (a.head[1] - a.tail[1]).abs() < 1e-9);
        assert_eq!(slope_arrows(&t, 3.0, 20.0).unwrap().len(), 8);
        assert!(slope_arrows(&t, 0.0, 0.0).is_err());
    }

    #[test]
    fn section_along_a_polyline_joins_runs() {
        let t = tilted(500_000.0, 9_000_000.0);
        let (ox, oy) = (500_000.0, 9_000_000.0);
        let line = [[ox + 10.0, oy + 10.0], [ox + 50.0, oy + 10.0], [ox + 50.0, oy + 50.0]];
        let runs = section_along(&t, &line);
        assert_eq!(runs.len(), 1);
        let r = &runs[0];
        assert!((r.last().unwrap().0 - 80.0).abs() < 1e-9);
        assert!((r[0].1 - 1.5).abs() < 1e-9);
        assert!((r.last().unwrap().1 - (5.0 + 2.5)).abs() < 1e-9);
        // Leaves the surface and comes back.
        let out = [[ox + 10.0, oy + 10.0], [ox + 10.0, oy + 100.0], [ox + 20.0, oy + 100.0], [ox + 20.0, oy + 10.0]];
        assert_eq!(section_along(&t, &out).len(), 2);
    }

    #[test]
    fn flow_path_follows_the_fall_line() {
        let (ox, oy) = (500_000.0, 9_000_000.0);
        // Falls towards -x only (a plane); the path from the middle runs straight west.
        let t = Tin::from_grid(ox, oy, 10.0, 10.0, 11, 7, |x, _| 0.1 * (x - ox)).unwrap();
        let f = flow_path(&t, ox + 77.0, oy + 33.0);
        assert_eq!(f.stop, FlowStop::LeftSurface);
        let last = f.points.last().unwrap();
        assert!((last[0] - ox).abs() < 1e-6 && (last[1] - (oy + 33.0)).abs() < 1e-6 && last[2].abs() < 1e-9);
        assert!((f.plan_length - 77.0).abs() < 1e-6);
        assert!((f.length_3d - 77.0 * (1.0f64 + 0.01).sqrt()).abs() < 1e-6);
        // Outside.
        assert_eq!(flow_path(&t, ox - 5.0, oy).stop, FlowStop::StartOutside);
        // Flat ground.
        let flat = Tin::from_grid(0.0, 0.0, 1.0, 1.0, 3, 3, |_, _| 1.0).unwrap();
        assert_eq!(flow_path(&flat, 0.5, 0.2).stop, FlowStop::Flat);
    }

    #[test]
    fn flow_path_runs_into_a_pit_and_down_a_valley() {
        // A bowl: z = ((x-50)^2 + (y-50)^2) / 100, sampled on a 10 m grid.
        let t = Tin::from_grid(0.0, 0.0, 10.0, 10.0, 11, 11, |x, y| ((x - 50.0).powi(2) + (y - 50.0).powi(2)) / 100.0).unwrap();
        let f = flow_path(&t, 13.0, 27.0);
        assert_eq!(f.stop, FlowStop::Pit);
        let end = f.points.last().unwrap();
        assert_eq!((end[0], end[1], end[2]), (50.0, 50.0, 0.0));
        assert!(f.points.windows(2).all(|w| w[1][2] <= w[0][2] + 1e-12));
        // A V valley along y: z = |x - 50| / 10 + 0.01 y. The path from the side runs to the
        // valley floor and then down the valley.
        let v = Tin::from_grid(0.0, 0.0, 10.0, 10.0, 11, 11, |x, y| (x - 50.0).abs() / 10.0 + 0.01 * y).unwrap();
        let f = flow_path(&v, 20.0, 80.0);
        let end = f.points.last().unwrap();
        assert!((end[0] - 50.0).abs() < 1e-6 && end[1].abs() < 1e-6, "{end:?} {:?}", f.stop);
        assert_eq!(f.stop, FlowStop::Pit);
    }
}
