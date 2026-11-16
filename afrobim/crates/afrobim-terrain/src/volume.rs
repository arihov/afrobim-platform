//! Volumes between TIN surfaces, inside a polygon, against a level, and stage-storage.
//!
//! # Method (exact)
//!
//! Each surface is piecewise planar. Overlay the two triangulations: every triangle `T` of the
//! first surface is cut by every triangle `U` of the second that overlaps it (convex polygon
//! clipping, Sutherland and Hodgman, "Reentrant polygon clipping", Communications of the ACM
//! 17(1), 1974; for two convex triangles the intersection is one convex polygon of at most six
//! vertices). On each such piece both surfaces are planes, so their difference
//! `d(x, y) = z_first - z_second` is an *affine* function of the plan position.
//!
//! The integral of an affine function over a triangle is the triangle's area times the function's
//! value at the centroid, which equals the mean of its three corner values. So over a triangle
//! with plan area `A` and corner differences `d0, d1, d2` the volume is `A (d0 + d1 + d2) / 3`
//! (the prismoid formula for a triangular prism with a plane top and bottom; it is exact, not an
//! approximation, because `d` is affine). A convex piece is fanned into triangles.
//!
//! Where `d` changes sign inside a piece the piece is cut along the line `d = 0` (one more
//! polygon clip) and the positive part is summed as **cut** and the negative part, with its sign
//! changed, as **fill**. The result is therefore exact up to floating point rounding: there is no
//! grid, no sampling interval and no dependence on how the two surfaces happen to be
//! triangulated. This is the volume between two TIN surfaces ("TIN volume surface") computed
//! without first building the composite surface explicitly; the integral is the same.
//!
//! Coordinates are shifted to the south-west corner of the two surfaces before the clipping, so
//! eastings and northings of many hundreds of thousands of metres cost no precision. Sums use
//! compensated (Neumaier) summation.
//!
//! # Conventions
//!
//! The first surface is the *existing* ground and the second the *design*. **Cut** is where the
//! existing ground is above the design (material to remove), **fill** where it is below
//! (material to place); both are positive volumes in m³. `net = cut - fill`: positive is a
//! surplus of cut, negative a shortage. No swell, shrinkage or compaction factor is applied.
//! Only ground where *both* surfaces exist is counted; the plan areas covered by only one are
//! reported so a mismatch is visible.
//!
//! The *grid method* ([`volume_between_grid`]) samples both surfaces at the centres of square
//! cells and sums `d * cell²`. It is the usual spreadsheet-style estimate. Its error is of the
//! order of the cell size times the length of the edges and creases of the surfaces; it is
//! provided because it is cheap and independent of the overlay, so it can be used as a check,
//! not as a replacement.

use afrobim_corridor::Tin;
use serde::{Deserialize, Serialize};

use crate::error::TerrainError;
use crate::geom::{area2, clip_by, clip_left, integrate_affine, point_in_polygon, poly_area, proper_crossing, Plane};
use crate::triangulate::{triangulate, Constraint, ConstraintKind, VertexTable};

/// Compensated (Neumaier) running sum.
#[derive(Default, Clone, Copy)]
struct Sum {
    s: f64,
    c: f64,
}

impl Sum {
    fn add(&mut self, v: f64) {
        let t = self.s + v;
        if self.s.abs() >= v.abs() {
            self.c += (self.s - t) + v;
        } else {
            self.c += (v - t) + self.s;
        }
        self.s = t;
    }
    fn get(&self) -> f64 {
        self.s + self.c
    }
}

/// The volume between two surfaces. See the module documentation for the conventions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VolumeResult {
    /// Volume where the first surface is above the second, m³.
    pub cut: f64,
    /// Volume where the first surface is below the second, m³.
    pub fill: f64,
    /// `cut - fill`, m³.
    pub net: f64,
    /// Plan area where both surfaces exist (inside the region, if one was given), m².
    pub overlap_area: f64,
    /// Plan area of the cut, m².
    pub cut_area: f64,
    /// Plan area of the fill, m².
    pub fill_area: f64,
    /// Plan area where only the first surface exists, m².
    pub first_only_area: f64,
    /// Plan area where only the second surface exists, m².
    pub second_only_area: f64,
}

/// Triangles `[a, b, c]` counter-clockwise in local plan coordinates.
type Tri2 = [[f64; 2]; 3];

fn ccw(t: Tri2) -> Tri2 {
    let a2 = (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]);
    if a2 < 0.0 {
        [t[0], t[2], t[1]]
    } else {
        t
    }
}

/// A TIN in local coordinates with a bounding-box grid.
struct Local {
    tris: Vec<[[f64; 3]; 3]>,
    planes: Vec<Option<Plane>>,
    grid: Grid,
}

struct Grid {
    min: [f64; 2],
    cell: [f64; 2],
    n: [usize; 2],
    cells: Vec<Vec<u32>>,
}

impl Grid {
    fn new(boxes: &[([f64; 2], [f64; 2])], lo: [f64; 2], hi: [f64; 2]) -> Grid {
        let n = boxes.len().max(1) as f64;
        let (w, h) = ((hi[0] - lo[0]).max(1e-9), (hi[1] - lo[1]).max(1e-9));
        let nx = ((n * w / h).sqrt().ceil() as usize).clamp(1, 2048);
        let ny = ((n / nx as f64).ceil() as usize).clamp(1, 2048);
        let mut g = Grid { min: lo, cell: [w / nx as f64, h / ny as f64], n: [nx, ny], cells: vec![Vec::new(); nx * ny] };
        for (k, (bl, bh)) in boxes.iter().enumerate() {
            let ((i0, j0), (i1, j1)) = (g.at(*bl), g.at(*bh));
            for j in j0..=j1 {
                for i in i0..=i1 {
                    g.cells[j * nx + i].push(k as u32);
                }
            }
        }
        g
    }
    fn at(&self, p: [f64; 2]) -> (usize, usize) {
        (((((p[0] - self.min[0]) / self.cell[0]).max(0.0)) as usize).min(self.n[0] - 1), ((((p[1] - self.min[1]) / self.cell[1]).max(0.0)) as usize).min(self.n[1] - 1))
    }
}

impl Local {
    fn new(tin: &Tin, origin: [f64; 2]) -> Local {
        let pts = tin.points();
        let mut tris = Vec::new();
        let mut planes = Vec::new();
        let mut boxes = Vec::new();
        for t in tin.triangles() {
            let mut c = t.map(|i| {
                let p = pts[i as usize];
                [p[0] - origin[0], p[1] - origin[1], p[2]]
            });
            if area2(c[0], c[1], c[2]) < 0.0 {
                c.swap(1, 2);
            }
            let lo = [c.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min), c.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min)];
            let hi = [c.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max), c.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max)];
            boxes.push((lo, hi));
            planes.push(Plane::of(c));
            tris.push(c);
        }
        let b = tin.bounds();
        let grid = Grid::new(&boxes, [b.min_x - origin[0], b.min_y - origin[1]], [b.max_x - origin[0], b.max_y - origin[1]]);
        Local { tris, planes, grid }
    }

    fn xy(&self, k: usize) -> Tri2 {
        self.tris[k].map(|p| [p[0], p[1]])
    }

    /// Triangles whose box may overlap the box `lo..hi`, each once.
    fn candidates(&self, lo: [f64; 2], hi: [f64; 2], stamp: &mut Vec<u32>, tick: u32, out: &mut Vec<usize>) {
        out.clear();
        let g = &self.grid;
        let ((i0, j0), (i1, j1)) = (g.at(lo), g.at(hi));
        for j in j0..=j1 {
            for i in i0..=i1 {
                for &k in &g.cells[j * g.n[0] + i] {
                    if stamp[k as usize] != tick {
                        stamp[k as usize] = tick;
                        out.push(k as usize);
                    }
                }
            }
        }
    }
}

/// A region of plan, as counter-clockwise triangles in local coordinates.
struct Region {
    tris: Vec<Tri2>,
}

fn clip_tri(poly: &[[f64; 2]], t: &Tri2) -> Vec<[f64; 2]> {
    let mut p = poly.to_vec();
    for i in 0..3 {
        p = clip_left(&p, t[i], t[(i + 1) % 3]);
        if p.len() < 3 {
            return Vec::new();
        }
    }
    p
}

fn bbox(p: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    let lo = [p.iter().map(|q| q[0]).fold(f64::INFINITY, f64::min), p.iter().map(|q| q[1]).fold(f64::INFINITY, f64::min)];
    let hi = [p.iter().map(|q| q[0]).fold(f64::NEG_INFINITY, f64::max), p.iter().map(|q| q[1]).fold(f64::NEG_INFINITY, f64::max)];
    (lo, hi)
}

impl Region {
    /// Triangulates a simple polygon by constrained Delaunay triangulation and keeps the
    /// triangles whose centroid is inside it. Rejects polygons that cross themselves.
    fn new(poly: &[[f64; 2]], origin: [f64; 2]) -> Result<Region, TerrainError> {
        let mut ring = poly.to_vec();
        if ring.len() > 1 && ring.first() == ring.last() {
            ring.pop();
        }
        if ring.len() < 3 || ring.iter().any(|p| !(p[0].is_finite() && p[1].is_finite())) {
            return Err(TerrainError::InvalidInput("a region needs at least three finite vertices".into()));
        }
        let n = ring.len();
        for i in 0..n {
            for j in i + 1..n {
                if ring[i] == ring[j] {
                    return Err(TerrainError::InvalidInput("a region may not repeat a vertex".into()));
                }
                if j != i + 1 && !(i == 0 && j == n - 1) && proper_crossing(ring[i], ring[(i + 1) % n], ring[j], ring[(j + 1) % n]) {
                    return Err(TerrainError::InvalidInput("the region crosses itself".into()));
                }
            }
        }
        let mut table = VertexTable::new(origin, 0.0);
        let verts: Vec<u32> = ring.iter().map(|p| table.add([p[0], p[1], 0.0]).0).collect();
        let out = triangulate(&mut table, &[Constraint { verts, closed: true, kind: ConstraintKind::Boundary, name: "region".into() }])?;
        let local: Vec<[f64; 2]> = ring.iter().map(|p| [p[0] - origin[0], p[1] - origin[1]]).collect();
        let mut tris = Vec::new();
        for t in out.triangles {
            let c = t.map(|i| {
                let p = table.pts[i as usize];
                [p[0] - origin[0], p[1] - origin[1]]
            });
            let cen = [(c[0][0] + c[1][0] + c[2][0]) / 3.0, (c[0][1] + c[1][1] + c[2][1]) / 3.0];
            if point_in_polygon(cen, &local) {
                tris.push(ccw(c));
            }
        }
        if tris.is_empty() {
            return Err(TerrainError::InvalidInput("the region has no area".into()));
        }
        Ok(Region { tris })
    }

    /// Calls `f` with each convex piece of `poly` inside the region.
    fn pieces(&self, poly: &[[f64; 2]], mut f: impl FnMut(&[[f64; 2]])) {
        let (lo, hi) = bbox(poly);
        for t in &self.tris {
            let (tl, th) = bbox(t);
            if tl[0] > hi[0] || th[0] < lo[0] || tl[1] > hi[1] || th[1] < lo[1] {
                continue;
            }
            let p = clip_tri(poly, t);
            if p.len() >= 3 {
                f(&p);
            }
        }
    }
}

fn origin_of(tins: &[&Tin], region: Option<&[[f64; 2]]>) -> [f64; 2] {
    let mut o = [f64::INFINITY, f64::INFINITY];
    for t in tins {
        let b = t.bounds();
        o = [o[0].min(b.min_x), o[1].min(b.min_y)];
    }
    if let Some(r) = region {
        for p in r {
            if p[0].is_finite() && p[1].is_finite() {
                o = [o[0].min(p[0]), o[1].min(p[1])];
            }
        }
    }
    o
}

/// The exact volume between two surfaces, optionally only inside a polygon.
///
/// `first` is the existing ground and `second` the design; see the module documentation for the
/// method and conventions. `region` is a simple (non-self-intersecting) polygon `[x, y]` in
/// metres, convex or not; it is not required to repeat its first vertex.
pub fn volume_between(first: &Tin, second: &Tin, region: Option<&[[f64; 2]]>) -> Result<VolumeResult, TerrainError> {
    let origin = origin_of(&[first, second], region);
    let region = region.map(|r| Region::new(r, origin)).transpose()?;
    let (a, b) = (Local::new(first, origin), Local::new(second, origin));

    let (mut cut, mut fill, mut overlap, mut cut_area, mut fill_area) = (Sum::default(), Sum::default(), Sum::default(), Sum::default(), Sum::default());
    let mut stamp = vec![0u32; b.tris.len()];
    let mut cand = Vec::new();
    let mut first_in_region = Sum::default();
    let mut tick = 0u32;
    for ka in 0..a.tris.len() {
        let Some(pa) = a.planes[ka] else { continue };
        let ta = a.xy(ka);
        let (lo, hi) = bbox(&ta);
        match &region {
            Some(r) => r.pieces(&ta, |p| first_in_region.add(poly_area(p))),
            None => first_in_region.add(poly_area(&ta)),
        }
        tick += 1;
        b.candidates(lo, hi, &mut stamp, tick, &mut cand);
        for &kb in &cand {
            let Some(pb) = b.planes[kb] else { continue };
            let piece = clip_tri(&ta, &ccw(b.xy(kb)));
            if piece.len() < 3 {
                continue;
            }
            let mut one = |p: &[[f64; 2]]| {
                let d = |q: [f64; 2]| pa.z(q) - pb.z(q);
                overlap.add(poly_area(p));
                let pos = clip_by(p, d);
                let neg = clip_by(p, |q| -d(q));
                let (area_p, int_p) = integrate_affine(&pos, d);
                let (area_n, int_n) = integrate_affine(&neg, |q| -d(q));
                cut.add(int_p);
                cut_area.add(area_p);
                fill.add(int_n);
                fill_area.add(area_n);
            };
            match &region {
                Some(r) => r.pieces(&piece, &mut one),
                None => one(&piece),
            }
        }
    }
    let second_total = {
        let mut s = Sum::default();
        for kb in 0..b.tris.len() {
            let tb = b.xy(kb);
            match &region {
                Some(r) => r.pieces(&tb, |p| s.add(poly_area(p))),
                None => s.add(poly_area(&tb)),
            }
        }
        s.get()
    };
    let (cut, fill, overlap) = (cut.get(), fill.get(), overlap.get());
    Ok(VolumeResult {
        cut,
        fill,
        net: cut - fill,
        overlap_area: overlap,
        cut_area: cut_area.get(),
        fill_area: fill_area.get(),
        first_only_area: (first_in_region.get() - overlap).max(0.0),
        second_only_area: (second_total - overlap).max(0.0),
    })
}

/// The grid-method estimate of the volume between two surfaces: both are sampled at the centres
/// of square cells of side `cell` metres over the extent they share (inside `region`, if given)
/// and `(z_first - z_second) * cell²` is summed. Cut and fill are the positive and negative
/// sums. A cell whose centre is off either surface is skipped, so the edges of the surfaces are
/// stepped. See the module documentation for the difference from [`volume_between`].
pub fn volume_between_grid(first: &Tin, second: &Tin, cell: f64, region: Option<&[[f64; 2]]>) -> Result<VolumeResult, TerrainError> {
    if !(cell.is_finite() && cell > 0.0) {
        return Err(TerrainError::InvalidInput("the grid cell size must be a positive number".into()));
    }
    let (fb, sb) = (first.bounds(), second.bounds());
    let (x0, y0) = (fb.min_x.max(sb.min_x), fb.min_y.max(sb.min_y));
    let (x1, y1) = (fb.max_x.min(sb.max_x), fb.max_y.min(sb.max_y));
    let ring = region.map(|r| {
        let mut r = r.to_vec();
        if r.len() > 1 && r.first() == r.last() {
            r.pop();
        }
        r
    });
    if let Some(r) = &ring {
        if r.len() < 3 {
            return Err(TerrainError::InvalidInput("a region needs at least three vertices".into()));
        }
    }
    let empty = VolumeResult { cut: 0.0, fill: 0.0, net: 0.0, overlap_area: 0.0, cut_area: 0.0, fill_area: 0.0, first_only_area: 0.0, second_only_area: 0.0 };
    if x1 <= x0 || y1 <= y0 {
        return Ok(empty);
    }
    let (nx, ny) = (((x1 - x0) / cell).ceil() as usize, ((y1 - y0) / cell).ceil() as usize);
    if nx.saturating_mul(ny) > 100_000_000 {
        return Err(TerrainError::InvalidInput("the grid would have more than 100 million cells".into()));
    }
    let (mut cut, mut fill, mut ca, mut fa) = (Sum::default(), Sum::default(), Sum::default(), Sum::default());
    for j in 0..ny {
        let y = y0 + (j as f64 + 0.5) * cell;
        for i in 0..nx {
            let x = x0 + (i as f64 + 0.5) * cell;
            if let Some(r) = &ring {
                if !point_in_polygon([x, y], r) {
                    continue;
                }
            }
            if let (Some(z1), Some(z2)) = (first.elevation_at(x, y), second.elevation_at(x, y)) {
                let d = (z1 - z2) * cell * cell;
                if d > 0.0 {
                    cut.add(d);
                    ca.add(cell * cell);
                } else if d < 0.0 {
                    fill.add(-d);
                    fa.add(cell * cell);
                }
            }
        }
    }
    let (cut, fill) = (cut.get(), fill.get());
    Ok(VolumeResult { cut, fill, net: cut - fill, overlap_area: ca.get() + fa.get(), cut_area: ca.get(), fill_area: fa.get(), first_only_area: 0.0, second_only_area: 0.0 })
}

/// Pieces of a surface (convex polygons with their plane), optionally inside a region.
fn pieces_of(tin: &Tin, region: Option<&[[f64; 2]]>) -> Result<(Vec<(Vec<[f64; 2]>, Plane)>, [f64; 2]), TerrainError> {
    let origin = origin_of(&[tin], region);
    let region = region.map(|r| Region::new(r, origin)).transpose()?;
    let l = Local::new(tin, origin);
    let mut out = Vec::new();
    for k in 0..l.tris.len() {
        let Some(pl) = l.planes[k] else { continue };
        let t = l.xy(k);
        match &region {
            Some(r) => r.pieces(&t, |p| out.push((p.to_vec(), pl))),
            None => out.push((t.to_vec(), pl)),
        }
    }
    Ok((out, origin))
}

/// Volume of one surface against a horizontal level: `cut` is the volume above the level, `fill`
/// the volume below it (the water that would stand at that level, see [`stage_storage`]).
/// `first_only_area` and `second_only_area` are zero.
pub fn volume_against_level(tin: &Tin, level: f64, region: Option<&[[f64; 2]]>) -> Result<VolumeResult, TerrainError> {
    if !level.is_finite() {
        return Err(TerrainError::NonFinite("the level".into()));
    }
    let (pieces, _) = pieces_of(tin, region)?;
    let (mut cut, mut fill, mut ca, mut fa) = (Sum::default(), Sum::default(), Sum::default(), Sum::default());
    for (p, pl) in &pieces {
        let d = |q: [f64; 2]| pl.z(q) - level;
        let (a1, i1) = integrate_affine(&clip_by(p, d), d);
        let (a2, i2) = integrate_affine(&clip_by(p, |q| -d(q)), |q| -d(q));
        cut.add(i1);
        ca.add(a1);
        fill.add(i2);
        fa.add(a2);
    }
    let (cut, fill) = (cut.get(), fill.get());
    Ok(VolumeResult { cut, fill, net: cut - fill, overlap_area: ca.get() + fa.get(), cut_area: ca.get(), fill_area: fa.get(), first_only_area: 0.0, second_only_area: 0.0 })
}

/// One row of a stage-storage table.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StagePoint {
    /// Water level, m.
    pub elevation: f64,
    /// Volume of water below this level, m³.
    pub volume: f64,
    /// Area of the water surface at this level, m².
    pub water_area: f64,
    /// Volume added since the previous row (the first row is the volume itself), m³.
    pub incremental_volume: f64,
}

/// Stage-storage of a basin: the volume held below each level.
///
/// For a level `h` the volume is the integral of `max(h - z, 0)` over the surface, exact for the
/// piecewise-planar TIN (each triangle is clipped to the part below `h` and the affine depth is
/// integrated as in the module documentation), and the water area is the plan area of that
/// part. `region` limits the basin to a polygon; without one every part of the surface below `h`
/// counts, including separate lower ground that would not drain into the basin. The method does
/// not test that the water is contained: above the lowest point of the rim of the region the
/// numbers describe a level the basin cannot hold, so keep the levels below the spill level.
/// `levels` must be finite and strictly increasing.
pub fn stage_storage(tin: &Tin, region: Option<&[[f64; 2]]>, levels: &[f64]) -> Result<Vec<StagePoint>, TerrainError> {
    if levels.iter().any(|l| !l.is_finite()) || levels.windows(2).any(|w| w[1] <= w[0]) {
        return Err(TerrainError::InvalidInput("levels must be finite and strictly increasing".into()));
    }
    let (pieces, _) = pieces_of(tin, region)?;
    let zmin: Vec<f64> = pieces.iter().map(|(p, pl)| p.iter().map(|q| pl.z(*q)).fold(f64::INFINITY, f64::min)).collect();
    let mut out: Vec<StagePoint> = Vec::with_capacity(levels.len());
    for &h in levels {
        let (mut v, mut a) = (Sum::default(), Sum::default());
        for (k, (p, pl)) in pieces.iter().enumerate() {
            if zmin[k] >= h {
                continue;
            }
            let d = |q: [f64; 2]| h - pl.z(q);
            let (area, int) = integrate_affine(&clip_by(p, d), d);
            v.add(int);
            a.add(area);
        }
        let volume = v.get();
        let prev = out.last().map(|s| s.volume).unwrap_or(0.0);
        out.push(StagePoint { elevation: h, volume, water_area: a.get(), incremental_volume: volume - prev });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, w: f64, h: f64, zf: impl Fn(f64, f64) -> f64, flip: bool) -> Tin {
        let p = |x: f64, y: f64| [x, y, zf(x, y)];
        let pts = vec![p(x0, y0), p(x0 + w, y0), p(x0 + w, y0 + h), p(x0, y0 + h)];
        let tris = if flip { vec![[0, 1, 3], [1, 2, 3]] } else { vec![[0, 1, 2], [0, 2, 3]] };
        Tin::new(pts, tris).unwrap()
    }

    fn close(a: f64, b: f64, rel: f64) {
        assert!((a - b).abs() <= rel * b.abs().max(1.0), "{a} vs {b}");
    }

    #[test]
    fn prism() {
        let a = rect(0.0, 0.0, 100.0, 50.0, |_, _| 10.0, false);
        let b = rect(0.0, 0.0, 100.0, 50.0, |_, _| 4.0, true);
        let v = volume_between(&a, &b, None).unwrap();
        close(v.cut, 30_000.0, 1e-12);
        assert_eq!(v.fill, 0.0);
        close(v.net, 30_000.0, 1e-12);
        close(v.overlap_area, 5000.0, 1e-12);
        assert!(v.first_only_area < 1e-9 && v.second_only_area < 1e-9);
        let r = volume_between(&b, &a, None).unwrap();
        close(r.fill, 30_000.0, 1e-12);
        close(r.net, -30_000.0, 1e-12);
    }

    #[test]
    fn wedge_with_different_triangulations_and_at_survey_magnitude() {
        for (ox, oy) in [(0.0, 0.0), (512_345.25, 9_876_543.5)] {
            let a = Tin::from_grid(ox, oy, 10.0, 10.0, 11, 6, |x, _| 0.1 * (x - ox)).unwrap();
            let b = rect(ox, oy, 100.0, 50.0, |_, _| 0.0, true);
            let v = volume_between(&a, &b, None).unwrap();
            close(v.cut, 25_000.0, 1e-10);
            assert_eq!(v.fill, 0.0);
            close(v.cut_area, 5000.0, 1e-10);
        }
    }

    #[test]
    fn pyramid() {
        for (ox, oy) in [(0.0, 0.0), (500_000.0, 9_000_000.0)] {
            let pts = vec![[ox, oy, 0.0], [ox + 20.0, oy, 0.0], [ox + 20.0, oy + 20.0, 0.0], [ox, oy + 20.0, 0.0], [ox + 10.0, oy + 10.0, 6.0]];
            let a = Tin::new(pts, vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]]).unwrap();
            let b = rect(ox, oy, 20.0, 20.0, |_, _| 0.0, false);
            let v = volume_between(&a, &b, None).unwrap();
            close(v.cut, 800.0, 1e-11);
        }
    }

    #[test]
    fn cut_and_fill_balance_on_a_crossing_plane() {
        // z = 0.1 x - 5 against z = 0 on 100 x 50: cut for x > 50 and fill for x < 50.
        let a = Tin::from_grid(0.0, 0.0, 10.0, 10.0, 11, 6, |x, _| 0.1 * x - 5.0).unwrap();
        let b = rect(0.0, 0.0, 100.0, 50.0, |_, _| 0.0, false);
        let v = volume_between(&a, &b, None).unwrap();
        close(v.cut, 6250.0, 1e-11);
        close(v.fill, 6250.0, 1e-11);
        assert!(v.net.abs() < 1e-8);
        close(v.cut_area, 2500.0, 1e-11);
        close(v.fill_area, 2500.0, 1e-11);
    }

    #[test]
    fn two_planes_on_unrelated_triangulations_agree_with_the_closed_form() {
        // Built through the CDT from jittered points of two planes: any triangulation of a
        // plane is that plane, so the net volume is area * difference at the centroid.
        use crate::{Operation, SurfaceDefinition, SurfacePoint};
        let (ox, oy) = (500_000.0, 9_000_000.0);
        let jitter = |i: usize, j: usize, s: f64| ((i * 7 + j * 13) as f64 * 0.37 + s).sin() * 2.0;
        let mk = |f: &dyn Fn(f64, f64) -> f64, s: f64, n: usize| {
            let mut pts = Vec::new();
            for j in 0..=n {
                for i in 0..=n {
                    let (mut x, mut y) = (i as f64 * 100.0 / n as f64, j as f64 * 100.0 / n as f64);
                    if i > 0 && i < n && j > 0 && j < n {
                        x += jitter(i, j, s);
                        y += jitter(j, i, s + 1.0);
                    }
                    pts.push(SurfacePoint::new(format!("{i}-{j}"), ox + x, oy + y, f(x, y)));
                }
            }
            SurfaceDefinition::new().with(Operation::AddPoints { group: "g".into(), points: pts }).build().unwrap().tin
        };
        let a = mk(&|x, y| 20.0 + 0.05 * x + 0.02 * y, 0.0, 9);
        let b = mk(&|x, y| 22.0 - 0.03 * x + 0.04 * y, 5.0, 13);
        let v = volume_between(&a, &b, None).unwrap();
        // d = a - b = -2 + 0.08 x - 0.02 y; mean over the square at (50, 50): -2 + 4 - 1 = 1.
        close(v.net, 1.0 * 10_000.0, 1e-9);
        close(v.overlap_area, 10_000.0, 1e-9);
        // cut + fill = integral of |d|, checked by midpoint sampling of the same planes.
        let n = 1000;
        let mut s = 0.0;
        for j in 0..n {
            for i in 0..n {
                let (x, y) = ((i as f64 + 0.5) * 0.1, (j as f64 + 0.5) * 0.1);
                s += (-2.0 + 0.08 * x - 0.02 * y).abs() * 0.01;
            }
        }
        assert!((v.cut + v.fill - s).abs() < 0.05, "{} vs {s}", v.cut + v.fill);
    }

    #[test]
    fn bounded_volume_inside_polygons() {
        let a = Tin::from_grid(0.0, 0.0, 10.0, 10.0, 11, 6, |x, _| 0.1 * x).unwrap();
        let b = rect(0.0, 0.0, 100.0, 50.0, |_, _| 0.0, true);
        let strip = [[20.0, 0.0], [40.0, 0.0], [40.0, 50.0], [20.0, 50.0]];
        close(volume_between(&a, &b, Some(&strip)).unwrap().cut, 3000.0, 1e-11);
        // An L shape: the square [0,50]^2 without the quadrant [25,50]^2; clockwise vertices too.
        let l = [[0.0, 0.0], [50.0, 0.0], [50.0, 25.0], [25.0, 25.0], [25.0, 50.0], [0.0, 50.0]];
        let want = 6250.0 - 0.05 * (2500.0 - 625.0) * 25.0;
        close(volume_between(&a, &b, Some(&l)).unwrap().cut, want, 1e-11);
        let mut rev = l.to_vec();
        rev.reverse();
        close(volume_between(&a, &b, Some(&rev)).unwrap().cut, want, 1e-11);
        // A diagonal region not aligned with any triangle edge: the triangle (0,0) (100,0) (0,50)
        // has x-integral of 0.1 x: int over y from 0 to 50 (1 - x/100) of 0.1x dx dy.
        let tri = [[0.0, 0.0], [100.0, 0.0], [0.0, 50.0]];
        let want = 0.1 * (50.0 * (100.0f64.powi(2) / 2.0 - 100.0f64.powi(3) / (3.0 * 100.0)));
        close(volume_between(&a, &b, Some(&tri)).unwrap().cut, want, 1e-11);
        // Region partly off the surfaces: only the shared part counts.
        let wide = [[-50.0, -50.0], [200.0, -50.0], [200.0, 100.0], [-50.0, 100.0]];
        close(volume_between(&a, &b, Some(&wide)).unwrap().cut, 25_000.0, 1e-11);
    }

    #[test]
    fn bad_regions_are_refused() {
        let a = rect(0.0, 0.0, 10.0, 10.0, |_, _| 1.0, false);
        let b = rect(0.0, 0.0, 10.0, 10.0, |_, _| 0.0, false);
        let bow = [[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]];
        assert!(volume_between(&a, &b, Some(&bow)).is_err());
        assert!(volume_between(&a, &b, Some(&[[0.0, 0.0], [1.0, 1.0]])).is_err());
        assert!(volume_between(&a, &b, Some(&[[0.0, 0.0], [f64::NAN, 1.0], [1.0, 0.0]])).is_err());
    }

    #[test]
    fn surfaces_that_do_not_overlap_have_no_volume() {
        let a = rect(0.0, 0.0, 10.0, 10.0, |_, _| 5.0, false);
        let b = rect(100.0, 100.0, 10.0, 10.0, |_, _| 0.0, false);
        let v = volume_between(&a, &b, None).unwrap();
        assert_eq!((v.cut, v.fill, v.overlap_area), (0.0, 0.0, 0.0));
        close(v.first_only_area, 100.0, 1e-12);
        close(v.second_only_area, 100.0, 1e-12);
        // Partial overlap: 5 x 10 shared.
        let c = rect(5.0, 0.0, 10.0, 10.0, |_, _| 1.0, true);
        let v = volume_between(&a, &c, None).unwrap();
        close(v.cut, 4.0 * 50.0, 1e-12);
        close(v.first_only_area, 50.0, 1e-9);
        close(v.second_only_area, 50.0, 1e-9);
    }

    #[test]
    fn grid_method_approaches_the_exact_value() {
        let a = Tin::from_grid(0.0, 0.0, 10.0, 10.0, 11, 6, |x, _| 0.1 * x - 5.0).unwrap();
        let b = rect(0.0, 0.0, 100.0, 50.0, |_, _| 0.0, false);
        let exact = volume_between(&a, &b, None).unwrap();
        let coarse = volume_between_grid(&a, &b, 5.0, None).unwrap();
        let fine = volume_between_grid(&a, &b, 0.5, None).unwrap();
        let (ec, ef) = ((coarse.cut - exact.cut).abs(), (fine.cut - exact.cut).abs());
        assert!(ef < ec + 1e-9 && ef / exact.cut < 0.01, "{ec} {ef}");
        // On a prism the grid method is exact.
        let p = rect(0.0, 0.0, 100.0, 50.0, |_, _| 10.0, false);
        let q = rect(0.0, 0.0, 100.0, 50.0, |_, _| 4.0, true);
        close(volume_between_grid(&p, &q, 2.5, None).unwrap().cut, 30_000.0, 1e-10);
        assert!(volume_between_grid(&p, &q, 0.0, None).is_err());
        let strip = [[20.0, 0.0], [40.0, 0.0], [40.0, 50.0], [20.0, 50.0]];
        close(volume_between_grid(&p, &q, 1.0, Some(&strip)).unwrap().cut, 6.0 * 1000.0, 1e-10);
    }

    #[test]
    fn stage_storage_of_an_inverted_pyramid() {
        let (ox, oy) = (500_000.0, 9_000_000.0);
        let pts = vec![[ox, oy, 6.0], [ox + 20.0, oy, 6.0], [ox + 20.0, oy + 20.0, 6.0], [ox, oy + 20.0, 6.0], [ox + 10.0, oy + 10.0, 0.0]];
        let pit = Tin::new(pts, vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]]).unwrap();
        let levels = [0.0, 1.0, 2.0, 3.5, 5.0, 6.0];
        let table = stage_storage(&pit, None, &levels).unwrap();
        for (row, h) in table.iter().zip(levels) {
            close(row.volume, 400.0 * h.powi(3) / 108.0, 1e-11);
            close(row.water_area, (20.0 * h / 6.0).powi(2), 1e-10);
        }
        close(table[5].volume, 800.0, 1e-11);
        close(table[3].incremental_volume, table[3].volume - table[2].volume, 1e-12);
        // Limited to a region round the pit (a diamond of half-width 5 m about the apex).
        let diamond = [[ox + 5.0, oy + 10.0], [ox + 10.0, oy + 5.0], [ox + 15.0, oy + 10.0], [ox + 10.0, oy + 15.0]];
        let r = stage_storage(&pit, Some(&diamond), &[1.0]).unwrap();
        // At h = 1 the water square has half-diagonal... the square side 20/6 is inside the diamond
        // (corner distance 20/6 * 0.5 * sqrt(2) = 2.36 < 5 along the axes, and the diamond's
        // edge is at |dx|+|dy| = 5; the water square has |dx|+|dy| max = 3.33).
        close(r[0].volume, 400.0 / 108.0, 1e-11);
        assert!(stage_storage(&pit, None, &[2.0, 1.0]).is_err());
        let rel = volume_against_level(&pit, 3.0, None).unwrap();
        close(rel.fill, 400.0 * 27.0 / 108.0, 1e-11);
        // net = integral of (z - 3) = (1600 - 3 * 400) = 400, so cut = 400 + 100.
        close(rel.cut, 500.0, 1e-11);
        assert!(volume_against_level(&pit, f64::NAN, None).is_err());
    }
}
