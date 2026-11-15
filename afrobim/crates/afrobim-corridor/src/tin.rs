//! The existing ground as a TIN (triangulated irregular network): points and triangles.
//!
//! Plan X is easting and Y northing, in metres; Z is elevation in metres. The surface is the
//! union of the triangles, each a plane. Nothing is interpolated beyond the triangles: a point
//! outside every triangle has no elevation.
//!
//! A uniform grid over the plan bounds indexes the triangles (about one cell per triangle; each
//! cell lists the triangles whose bounding box touches it). It is built on first use and is not
//! stored. Elevation-at-point looks in one cell; a section cut visits only the cells the line
//! passes through.
//!
//! This type does not triangulate. It takes triangles from a file or from the caller; a surface
//! from scattered survey points is built in the `survey` module (`build_survey_surface`).

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Why a TIN could not be built or decoded.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TinError {
    #[error("point {0} has a coordinate that is not a finite number")]
    BadPoint(usize),
    #[error("triangle {triangle} refers to point {point}, but there are only {count} points")]
    BadIndex { triangle: usize, point: u32, count: usize },
    #[error("triangle {0} has no area in plan (its three points are on one line or coincide)")]
    Degenerate(usize),
    #[error("a surface needs at least one triangle")]
    Empty,
    #[error("the stored surface cannot be decoded: {0}")]
    Encoding(String),
}

/// The extent of a surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TinBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}

/// A triangulated surface. See the module documentation.
///
/// In JSON it is stored compactly, see [`TinEncoded`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(into = "TinEncoded", try_from = "TinEncoded")]
pub struct Tin {
    points: Vec<[f64; 3]>,
    triangles: Vec<[u32; 3]>,
    bounds: TinBounds,
    index: OnceLock<Grid>,
}

impl PartialEq for Tin {
    fn eq(&self, other: &Tin) -> bool {
        self.points == other.points && self.triangles == other.triangles
    }
}

#[derive(Debug, Clone)]
struct Grid {
    nx: usize,
    ny: usize,
    cell_x: f64,
    cell_y: f64,
    /// `items[starts[c]..starts[c + 1]]` are the triangles touching cell `c = iy * nx + ix`.
    starts: Vec<u32>,
    items: Vec<u32>,
}

/// Twice the signed plan area of a triangle (positive counter-clockwise).
fn area2(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

impl Tin {
    /// A surface from points `[x (easting), y (northing), z]` in metres and triangles of point
    /// indices (from 0). Either winding is accepted. Every coordinate must be finite, every
    /// index in range and every triangle must have area in plan. Triangles that overlap in plan
    /// are not detected; where they do, the elevation is that of whichever is found first.
    pub fn new(points: Vec<[f64; 3]>, triangles: Vec<[u32; 3]>) -> Result<Tin, TinError> {
        if triangles.is_empty() {
            return Err(TinError::Empty);
        }
        if let Some(i) = points.iter().position(|p| p.iter().any(|v| !v.is_finite())) {
            return Err(TinError::BadPoint(i));
        }
        for (i, t) in triangles.iter().enumerate() {
            if let Some(&p) = t.iter().find(|&&p| p as usize >= points.len()) {
                return Err(TinError::BadIndex { triangle: i, point: p, count: points.len() });
            }
            let [a, b, c] = t.map(|p| points[p as usize]);
            if area2(a, b, c) == 0.0 {
                return Err(TinError::Degenerate(i));
            }
        }
        let mut b = TinBounds {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            min_z: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
            max_z: f64::NEG_INFINITY,
        };
        // Bounds are those of the points used by a triangle.
        for p in triangles.iter().flatten().map(|&p| points[p as usize]) {
            b.min_x = b.min_x.min(p[0]);
            b.max_x = b.max_x.max(p[0]);
            b.min_y = b.min_y.min(p[1]);
            b.max_y = b.max_y.max(p[1]);
            b.min_z = b.min_z.min(p[2]);
            b.max_z = b.max_z.max(p[2]);
        }
        Ok(Tin { points, triangles, bounds: b, index: OnceLock::new() })
    }

    /// A surface on a regular grid of `nx` × `ny` points starting at (`x0`, `y0`) with spacing
    /// `dx`, `dy`, the elevation of each given by `z(x, y)`. Every grid square is split by the
    /// diagonal from its lower-left to its upper-right corner, giving 2 (nx − 1)(ny − 1) triangles.
    pub fn from_grid(x0: f64, y0: f64, dx: f64, dy: f64, nx: usize, ny: usize, z: impl Fn(f64, f64) -> f64) -> Result<Tin, TinError> {
        if nx < 2 || ny < 2 {
            return Err(TinError::Empty);
        }
        let mut points = Vec::with_capacity(nx * ny);
        for j in 0..ny {
            for i in 0..nx {
                let (x, y) = (x0 + i as f64 * dx, y0 + j as f64 * dy);
                points.push([x, y, z(x, y)]);
            }
        }
        let mut triangles = Vec::with_capacity(2 * (nx - 1) * (ny - 1));
        for j in 0..ny - 1 {
            for i in 0..nx - 1 {
                let a = (j * nx + i) as u32;
                let (b, c, d) = (a + 1, a + nx as u32 + 1, a + nx as u32);
                triangles.push([a, b, c]);
                triangles.push([a, c, d]);
            }
        }
        Tin::new(points, triangles)
    }

    /// The points, `[x (easting), y (northing), z]` in metres.
    pub fn points(&self) -> &[[f64; 3]] {
        &self.points
    }

    /// The triangles, as indices into [`Tin::points`].
    pub fn triangles(&self) -> &[[u32; 3]] {
        &self.triangles
    }

    /// The extent of the triangles.
    pub fn bounds(&self) -> TinBounds {
        self.bounds
    }

    /// The plan area of the surface, m²: the sum of the triangles' plan areas.
    pub fn plan_area(&self) -> f64 {
        self.triangles.iter().map(|t| area2(self.points[t[0] as usize], self.points[t[1] as usize], self.points[t[2] as usize]).abs() / 2.0).sum()
    }

    fn corners(&self, triangle: u32) -> [[f64; 3]; 3] {
        self.triangles[triangle as usize].map(|p| self.points[p as usize])
    }

    fn grid(&self) -> &Grid {
        self.index.get_or_init(|| {
            let b = self.bounds;
            let (w, h) = ((b.max_x - b.min_x).max(1e-9), (b.max_y - b.min_y).max(1e-9));
            let n = self.triangles.len() as f64;
            let nx = ((n * w / h).sqrt().ceil() as usize).clamp(1, 4096);
            let ny = ((n / nx as f64).ceil() as usize).clamp(1, 4096);
            let (cell_x, cell_y) = (w / nx as f64, h / ny as f64);
            let span = |t: u32| {
                let c = self.corners(t);
                let lo = |k: usize| c.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min);
                let hi = |k: usize| c.iter().map(|p| p[k]).fold(f64::NEG_INFINITY, f64::max);
                let ix = |x: f64| (((x - b.min_x) / cell_x) as usize).min(nx - 1);
                let iy = |y: f64| (((y - b.min_y) / cell_y) as usize).min(ny - 1);
                (ix(lo(0)), ix(hi(0)), iy(lo(1)), iy(hi(1)))
            };
            let mut starts = vec![0u32; nx * ny + 1];
            for t in 0..self.triangles.len() as u32 {
                let (x0, x1, y0, y1) = span(t);
                for iy in y0..=y1 {
                    for ix in x0..=x1 {
                        starts[iy * nx + ix + 1] += 1;
                    }
                }
            }
            for c in 0..nx * ny {
                starts[c + 1] += starts[c];
            }
            let mut next = starts.clone();
            let mut items = vec![0u32; starts[nx * ny] as usize];
            for t in 0..self.triangles.len() as u32 {
                let (x0, x1, y0, y1) = span(t);
                for iy in y0..=y1 {
                    for ix in x0..=x1 {
                        let c = iy * nx + ix;
                        items[next[c] as usize] = t;
                        next[c] += 1;
                    }
                }
            }
            Grid { nx, ny, cell_x, cell_y, starts, items }
        })
    }

    /// Elevation of the surface at a plan point, m: the plane of the triangle the point is in.
    /// `None` when the point is in no triangle. A point on an edge or a corner (to within a
    /// billionth of the triangle's size) belongs to the surface.
    pub fn elevation_at(&self, x: f64, y: f64) -> Option<f64> {
        let b = self.bounds;
        let pad = 1e-9 * (1.0 + (b.max_x - b.min_x).max(b.max_y - b.min_y));
        if !(x >= b.min_x - pad && x <= b.max_x + pad && y >= b.min_y - pad && y <= b.max_y + pad) {
            return None;
        }
        let g = self.grid();
        let ix = (((x - b.min_x) / g.cell_x).max(0.0) as usize).min(g.nx - 1);
        let iy = (((y - b.min_y) / g.cell_y).max(0.0) as usize).min(g.ny - 1);
        let c = iy * g.nx + ix;
        for &t in &g.items[g.starts[c] as usize..g.starts[c + 1] as usize] {
            let [p0, p1, p2] = self.corners(t);
            let a = area2(p0, p1, p2);
            let q = [x, y, 0.0];
            let (w0, w1, w2) = (area2(p1, p2, q) / a, area2(p2, p0, q) / a, area2(p0, p1, q) / a);
            if w0 >= -1e-9 && w1 >= -1e-9 && w2 >= -1e-9 {
                return Some(w0 * p0[2] + w1 * p1[2] + w2 * p2[2]);
            }
        }
        None
    }

    /// The ground along the straight line from `a` to `b` (plan points, m).
    ///
    /// The line is cut by every triangle it crosses; inside a triangle the ground is straight,
    /// so the result has a point wherever the line crosses a triangle edge (points in a straight
    /// run are merged). Each point is (distance from `a` along the line in metres, elevation).
    /// The result is a list of **runs**: one run where the line stays on the surface, several
    /// when it leaves and re-enters it (a hole, a concave edge), none when it misses it.
    pub fn section(&self, a: (f64, f64), b: (f64, f64)) -> Vec<Vec<(f64, f64)>> {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length = dx.hypot(dy);
        if !(length > 0.0 && length.is_finite()) {
            return Vec::new();
        }
        const TOL: f64 = 1e-7; // metres along the line
        let mut pieces: Vec<[f64; 4]> = Vec::new();
        for t in self.candidates(a, b) {
            let mut c = self.corners(t);
            if area2(c[0], c[1], c[2]) < 0.0 {
                c.swap(1, 2);
            }
            // Clip the parameter range [0, 1] by the three edges; inside is to the left of each.
            let (mut t0, mut t1) = (0.0f64, 1.0f64);
            for k in 0..3 {
                let (p, q) = (c[k], c[(k + 1) % 3]);
                let (ex, ey) = (q[0] - p[0], q[1] - p[1]);
                let e0 = ex * (a.1 - p[1]) - ey * (a.0 - p[0]);
                let e1 = ex * dy - ey * dx;
                if e1 > 0.0 {
                    t0 = t0.max(-e0 / e1);
                } else if e1 < 0.0 {
                    t1 = t1.min(-e0 / e1);
                } else if e0 < 0.0 {
                    t1 = -1.0;
                }
            }
            if (t1 - t0) * length <= TOL {
                continue;
            }
            // The triangle's plane: z = z0 + gx (x − x0) + gy (y − y0).
            let (ux, uy, uz) = (c[1][0] - c[0][0], c[1][1] - c[0][1], c[1][2] - c[0][2]);
            let (vx, vy, vz) = (c[2][0] - c[0][0], c[2][1] - c[0][1], c[2][2] - c[0][2]);
            let det = ux * vy - uy * vx;
            let (gx, gy) = ((uz * vy - uy * vz) / det, (ux * vz - uz * vx) / det);
            let z = |t: f64| c[0][2] + gx * (a.0 + t * dx - c[0][0]) + gy * (a.1 + t * dy - c[0][1]);
            pieces.push([t0 * length, z(t0), t1 * length, z(t1)]);
        }
        pieces.sort_by(|p, q| p[0].total_cmp(&q[0]).then(p[2].total_cmp(&q[2])));
        let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
        for [s0, z0, s1, z1] in pieces {
            let end = runs.last().and_then(|r| r.last()).map(|p| p.0);
            match end {
                Some(end) if s0 <= end + TOL => {
                    if s1 <= end + TOL {
                        continue; // a triangle sharing the edge the line runs along
                    }
                    let run = runs.last_mut().unwrap();
                    let n = run.len();
                    // Merge with the piece before when the three points are on one straight.
                    if n >= 2 {
                        let (p, q) = (run[n - 2], run[n - 1]);
                        let bend = (q.1 - p.1) * (s1 - q.0) - (z1 - q.1) * (q.0 - p.0);
                        if bend.abs() <= 1e-9 * (1.0 + (s1 - p.0).abs()) {
                            run[n - 1] = (s1, z1);
                            continue;
                        }
                    }
                    run.push((s1, z1));
                }
                _ => runs.push(vec![(s0, z0), (s1, z1)]),
            }
        }
        runs
    }

    /// The triangles in the grid cells a segment passes through, each once.
    fn candidates(&self, a: (f64, f64), b: (f64, f64)) -> Vec<u32> {
        let g = self.grid();
        let bd = self.bounds;
        let (x_lo, x_hi) = (a.0.min(b.0), a.0.max(b.0));
        if x_hi < bd.min_x || x_lo > bd.max_x {
            return Vec::new();
        }
        let col = |x: f64| (((x - bd.min_x) / g.cell_x).max(0.0) as usize).min(g.nx - 1);
        let row = |y: f64| (((y - bd.min_y) / g.cell_y).max(0.0) as usize).min(g.ny - 1);
        let mut out = Vec::new();
        let dx = b.0 - a.0;
        for ix in col(x_lo)..=col(x_hi) {
            // The part of the segment over this column, a little widened.
            let (cx0, cx1) = (bd.min_x + ix as f64 * g.cell_x - 1e-6, bd.min_x + (ix + 1) as f64 * g.cell_x + 1e-6);
            let (y0, y1) = if dx.abs() < 1e-12 {
                (a.1, b.1)
            } else {
                let at = |x: f64| a.1 + (b.1 - a.1) * ((x.clamp(x_lo, x_hi) - a.0) / dx);
                (at(cx0), at(cx1))
            };
            let (y_lo, y_hi) = (y0.min(y1) - 1e-6, y0.max(y1) + 1e-6);
            if y_hi < bd.min_y || y_lo > bd.max_y {
                continue;
            }
            for iy in row(y_lo)..=row(y_hi) {
                let c = iy * g.nx + ix;
                out.extend_from_slice(&g.items[g.starts[c] as usize..g.starts[c + 1] as usize]);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The edges that belong to exactly one triangle: the outer boundary and the rims of holes.
    /// Each is a pair of point indices; they are not joined into loops.
    pub fn boundary_edges(&self) -> Vec<[u32; 2]> {
        let mut count: HashMap<(u32, u32), u32> = HashMap::with_capacity(self.triangles.len() * 2);
        for t in &self.triangles {
            for k in 0..3 {
                let (p, q) = (t[k], t[(k + 1) % 3]);
                *count.entry((p.min(q), p.max(q))).or_insert(0) += 1;
            }
        }
        let mut edges: Vec<[u32; 2]> = count.into_iter().filter(|(_, n)| *n == 1).map(|((p, q), _)| [p, q]).collect();
        edges.sort_unstable();
        edges
    }
}

/// The name of the only encoding written and read.
pub const TIN_ENCODING: &str = "f64le-xyz/u32le-abc/base64";

/// How a [`Tin`] is stored in JSON.
///
/// - `points`: base64 (RFC 4648, with padding) of the points as consecutive little-endian IEEE
///   754 doubles `x y z`, 24 bytes a point. Exact: nothing is rounded.
/// - `triangles`: base64 of the triangles as consecutive little-endian unsigned 32-bit point
///   indices `a b c`, 12 bytes a triangle.
///
/// That is 32 characters of JSON per point and 16 per triangle. It is not compressed. A surface
/// of 100 000 points and 200 000 triangles is about 6.4 MB of JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TinEncoded {
    pub encoding: String,
    pub point_count: usize,
    pub triangle_count: usize,
    pub points: String,
    pub triangles: String,
}

impl From<Tin> for TinEncoded {
    fn from(t: Tin) -> TinEncoded {
        let mut p = Vec::with_capacity(t.points.len() * 24);
        t.points.iter().flatten().for_each(|v| p.extend_from_slice(&v.to_le_bytes()));
        let mut f = Vec::with_capacity(t.triangles.len() * 12);
        t.triangles.iter().flatten().for_each(|v| f.extend_from_slice(&v.to_le_bytes()));
        TinEncoded {
            encoding: TIN_ENCODING.to_string(),
            point_count: t.points.len(),
            triangle_count: t.triangles.len(),
            points: base64_encode(&p),
            triangles: base64_encode(&f),
        }
    }
}

impl TryFrom<TinEncoded> for Tin {
    type Error = TinError;
    fn try_from(e: TinEncoded) -> Result<Tin, TinError> {
        if e.encoding != TIN_ENCODING {
            return Err(TinError::Encoding(format!("unknown encoding '{}'", e.encoding)));
        }
        let p = base64_decode(&e.points).map_err(TinError::Encoding)?;
        let f = base64_decode(&e.triangles).map_err(TinError::Encoding)?;
        if p.len() != e.point_count * 24 || f.len() != e.triangle_count * 12 {
            return Err(TinError::Encoding("the data does not match the stated number of points and triangles".to_string()));
        }
        let number = |b: &[u8]| f64::from_le_bytes(b.try_into().unwrap());
        let points = p.chunks_exact(24).map(|c| [number(&c[0..8]), number(&c[8..16]), number(&c[16..24])]).collect();
        let index = |b: &[u8]| u32::from_le_bytes(b.try_into().unwrap());
        let triangles = f.chunks_exact(12).map(|c| [index(&c[0..4]), index(&c[4..8]), index(&c[8..12])]).collect();
        Tin::new(points, triangles)
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let bytes = text.trim_end_matches('=').as_bytes();
    if text.len() % 4 != 0 || text.len() - bytes.len() > 2 {
        return Err("the base64 text has the wrong length".to_string());
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &b in bytes {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(format!("'{}' is not a base64 character", b as char)),
        };
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_every_length() {
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"M"), "TQ==");
        for n in 0..20u8 {
            let data: Vec<u8> = (0..n).map(|i| i.wrapping_mul(37).wrapping_add(200)).collect();
            assert_eq!(base64_decode(&base64_encode(&data)).unwrap(), data);
        }
        assert!(base64_decode("TW=u").is_err());
        assert!(base64_decode("TWF").is_err());
    }
}
