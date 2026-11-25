// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The spatial index: an implicit octree on Morton-ordered points.
//!
//! # Choice and justification
//!
//! Every point gets a 63-bit Morton (Z-order) key: 21 bits per axis, quantised inside the cube
//! that encloses the data (the cube side is the longest side of the bounds, anchored at the
//! minimum corner). The points are stored sorted by key. An octree node at depth `d` is then a
//! contiguous range of the arrays (all points sharing the first `3d` key bits), found by binary
//! search, so the tree itself costs no memory beyond the 8-byte key per point. Compared with a
//! pointer octree this halves memory and makes the build one sort; compared with a grid pyramid
//! it adapts to uneven density (a scan of a road corridor is long, thin and clustered).
//! Level of detail comes free: a node at depth `d` stands for its points.
//!
//! Memory per point in the finished index: 24 (position, f64) + 2 (intensity) + 1 (class) +
//! 1 (returns) + 8 (key) = 36 bytes, plus 6 with colour and 8 with GPS time. During the build the
//! peak is higher (about 70 bytes per point: the input columns, a 16-byte sort array and the new
//! columns exist together); the README gives measured figures.
//!
//! Positions are stored in true coordinates. The only arithmetic on shifted values is the key
//! quantisation and the exact distance tests (done relative to a query point).

use crate::buffer::{Bounds3, PointBuffer};
use crate::error::{PointCloudError, Result};
use crate::source::PointSource;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

pub(crate) const BITS: u32 = 21;
const CELLS: f64 = (1u64 << BITS) as f64;

/// What to do when the file has more points than `max_points`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Overflow {
    /// Refuse with [`PointCloudError::TooManyPoints`].
    Fail,
    /// Keep every n-th point in file order (n chosen so the count fits). A plain systematic
    /// sample: it is not density aware, and it is reported in [`PointIndex::thinning_stride`].
    KeepEveryNth,
}

/// How to build an index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexOptions {
    /// A node with this many points or fewer is scanned instead of split (a speed setting).
    pub leaf_size: usize,
    /// Most points to hold in memory. `None`: no limit.
    pub max_points: Option<u64>,
    pub overflow: Overflow,
    /// Points per read chunk when building from a source.
    pub chunk: usize,
}

impl Default for IndexOptions {
    fn default() -> Self {
        IndexOptions { leaf_size: 32, max_points: None, overflow: Overflow::Fail, chunk: 500_000 }
    }
}

/// A neighbour: index into the index's point arrays and the distance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Neighbor {
    pub index: u32,
    pub distance: f64,
}

/// Result of a level-of-detail query.
#[derive(Debug, Clone, PartialEq)]
pub struct LodResult {
    pub indices: Vec<u32>,
    /// Octree depth the sample was taken at (0 to 21); 21 also when all points were returned.
    pub depth: u32,
    /// `true` when every point inside the view is in `indices`.
    pub complete: bool,
}

/// Points ready to draw: positions relative to `origin` (as `f32`), so drawing code need not
/// handle 7-digit northings. Add `origin` back to get true coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayBuffer {
    pub origin: [f64; 3],
    pub positions: Vec<[f32; 3]>,
    pub colors: Vec<[u8; 3]>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, Copy)]
struct Node {
    depth: u32,
    prefix: u64,
    lo: usize,
    hi: usize,
    min: [f64; 3],
    size: f64,
}

#[derive(Debug)]
pub struct PointIndex {
    pub(crate) points: PointBuffer,
    pub(crate) keys: Vec<u64>,
    pub(crate) bounds: Bounds3,
    pub(crate) extent: f64,
    pub(crate) leaf_size: usize,
    pub(crate) stride: u32,
    pub(crate) source_points: u64,
}

fn spread(v: u64) -> u64 {
    let mut x = v & 0x1f_ffff;
    x = (x | (x << 32)) & 0x001f_0000_0000_ffff;
    x = (x | (x << 16)) & 0x001f_0000_ff00_00ff;
    x = (x | (x << 8)) & 0x100f_00f0_0f00_f00f;
    x = (x | (x << 4)) & 0x10c3_0c30_c30c_30c3;
    x = (x | (x << 2)) & 0x1249_2492_4924_9249;
    x
}

/// Morton key of a point inside the cube at `min` with side `extent`.
pub(crate) fn morton(p: [f64; 3], min: [f64; 3], extent: f64) -> u64 {
    let q = |k: usize| (((p[k] - min[k]) / extent * CELLS) as u64).min((1u64 << BITS) - 1);
    spread(q(0)) | (spread(q(1)) << 1) | (spread(q(2)) << 2)
}

fn permute<T: Copy>(old: Vec<T>, order: &[u32]) -> Vec<T> {
    order.iter().map(|&i| old[i as usize]).collect()
}

struct DistItem {
    d2: f64,
    node: Node,
}
impl PartialEq for DistItem {
    fn eq(&self, o: &Self) -> bool {
        self.d2.total_cmp(&o.d2) == Ordering::Equal
    }
}
impl Eq for DistItem {}
impl PartialOrd for DistItem {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for DistItem {
    // Reversed: the smallest distance pops first.
    fn cmp(&self, o: &Self) -> Ordering {
        o.d2.total_cmp(&self.d2)
    }
}

#[derive(PartialEq)]
struct Cand(f64, u32);
impl Eq for Cand {}
impl PartialOrd for Cand {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Cand {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.total_cmp(&o.0).then(self.1.cmp(&o.1))
    }
}

impl PointIndex {
    /// Builds an index from points held in memory (the buffer is consumed).
    pub fn build(points: PointBuffer, opts: &IndexOptions) -> Result<PointIndex> {
        if !points.is_consistent() {
            return Err(PointCloudError::Invalid("the point columns have different lengths".into()));
        }
        let n = points.len();
        if n as u64 > u32::MAX as u64 {
            return Err(PointCloudError::Invalid("an index holds at most 4 294 967 295 points".into()));
        }
        if opts.leaf_size == 0 {
            return Err(PointCloudError::Invalid("leaf_size must be at least 1".into()));
        }
        let bounds = Bounds3::of(&points.xyz);
        if n == 0 {
            return Ok(PointIndex { points, keys: Vec::new(), bounds: Bounds3::new([0.0; 3], [0.0; 3]), extent: 1.0, leaf_size: opts.leaf_size, stride: 1, source_points: 0 });
        }
        if points.xyz.iter().any(|p| p.iter().any(|v| !v.is_finite())) {
            return Err(PointCloudError::Invalid("a coordinate is not finite".into()));
        }
        let side = (0..3).map(|k| bounds.max[k] - bounds.min[k]).fold(0.0f64, f64::max);
        let extent = if side > 0.0 { side } else { 1.0 };
        let mut pairs: Vec<(u64, u32)> = points.xyz.iter().enumerate().map(|(i, &p)| (morton(p, bounds.min, extent), i as u32)).collect();
        pairs.sort_unstable();
        let keys: Vec<u64> = pairs.iter().map(|p| p.0).collect();
        let order: Vec<u32> = pairs.iter().map(|p| p.1).collect();
        drop(pairs);
        let PointBuffer { xyz, intensity, class, returns, rgb, gps_time } = points;
        let points = PointBuffer {
            xyz: permute(xyz, &order),
            intensity: permute(intensity, &order),
            class: permute(class, &order),
            returns: permute(returns, &order),
            rgb: rgb.map(|v| permute(v, &order)),
            gps_time: gps_time.map(|v| permute(v, &order)),
        };
        Ok(PointIndex { points, keys, bounds, extent, leaf_size: opts.leaf_size, stride: 1, source_points: n as u64 })
    }

    /// Reads a source in chunks and indexes it. With a point limit and [`Overflow::KeepEveryNth`] a
    /// file larger than the limit is indexed from a systematic sample.
    pub fn build_from_source(source: &mut dyn PointSource, opts: &IndexOptions) -> Result<PointIndex> {
        let count = source.header().point_count;
        let mut stride = 1u64;
        if let Some(limit) = opts.max_points {
            if count > limit {
                match opts.overflow {
                    Overflow::Fail => return Err(PointCloudError::TooManyPoints { count, limit }),
                    Overflow::KeepEveryNth => stride = count.div_ceil(limit.max(1)),
                }
            }
        }
        let kept = count.div_ceil(stride) as usize;
        let mut all = source.new_buffer(kept.min(200_000_000));
        let mut chunk = source.new_buffer(opts.chunk.min(1 << 22));
        let mut seen = 0u64;
        loop {
            chunk.clear();
            let n = source.read_chunk(opts.chunk.max(1), &mut chunk)?;
            if n == 0 {
                break;
            }
            if stride == 1 {
                all.extend_from(&chunk);
            } else {
                for i in 0..n {
                    if (seen + i as u64) % stride == 0 {
                        all.push_from(&chunk, i);
                    }
                }
            }
            seen += n as u64;
        }
        drop(chunk);
        let mut idx = PointIndex::build(all, opts)?;
        idx.stride = stride as u32;
        idx.source_points = seen;
        Ok(idx)
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// The points, in index order (Morton order). Query results are indices into these arrays.
    pub fn points(&self) -> &PointBuffer {
        &self.points
    }

    /// The exact bounds of the points held.
    pub fn bounds(&self) -> Bounds3 {
        self.bounds
    }

    /// 1 unless the index was built from a sample (see [`Overflow::KeepEveryNth`]).
    pub fn thinning_stride(&self) -> u32 {
        self.stride
    }

    /// Points in the source (more than [`PointIndex::len`] when thinned).
    pub fn source_point_count(&self) -> u64 {
        self.source_points
    }

    /// Bytes of heap held by the index (by length).
    pub fn memory_bytes(&self) -> usize {
        self.points.bytes() + self.keys.len() * 8
    }

    fn root(&self) -> Node {
        Node { depth: 0, prefix: 0, lo: 0, hi: self.len(), min: self.bounds.min, size: self.extent }
    }

    fn margin(&self) -> f64 {
        self.extent / CELLS * 2.0
    }

    /// The occupied children of a node.
    fn children(&self, n: &Node) -> impl Iterator<Item = Node> + '_ {
        let n = *n;
        let shift = 63 - 3 * (n.depth + 1);
        let half = n.size / 2.0;
        let slice = &self.keys[n.lo..n.hi];
        let mut start = 0usize;
        (0..8u64).filter_map(move |c| {
            let cp = (n.prefix << 3) | c;
            let end = if c == 7 { slice.len() } else { slice.partition_point(|&k| k < ((cp + 1) << shift)) };
            let (lo, hi) = (n.lo + start, n.lo + end);
            start = end;
            (hi > lo).then_some(Node {
                depth: n.depth + 1,
                prefix: cp,
                lo,
                hi,
                min: [n.min[0] + if c & 1 != 0 { half } else { 0.0 }, n.min[1] + if c & 2 != 0 { half } else { 0.0 }, n.min[2] + if c & 4 != 0 { half } else { 0.0 }],
                size: half,
            })
        })
    }

    fn is_leaf(&self, n: &Node) -> bool {
        n.hi - n.lo <= self.leaf_size || n.depth >= BITS
    }

    // ------------------------------------------------------------------ box

    /// Indices of the points inside the box (edges included). Use infinite z limits for a plan box.
    pub fn query_box(&self, q: &Bounds3) -> Vec<u32> {
        let mut out = Vec::new();
        if !self.is_empty() {
            self.box_rec(&self.root(), q, &mut out, usize::MAX);
        }
        out
    }

    /// Returns `false` when the limit stopped the walk.
    fn box_rec(&self, n: &Node, q: &Bounds3, out: &mut Vec<u32>, limit: usize) -> bool {
        let m = self.margin();
        let (mut disjoint, mut inside) = (false, true);
        for k in 0..3 {
            let (lo, hi) = (n.min[k], n.min[k] + n.size);
            if hi + m < q.min[k] || lo - m > q.max[k] {
                disjoint = true;
                break;
            }
            if !(q.min[k] <= lo + m && hi - m <= q.max[k]) {
                inside = false;
            }
        }
        if disjoint {
            return true;
        }
        if inside {
            for i in n.lo..n.hi {
                if out.len() >= limit {
                    return false;
                }
                out.push(i as u32);
            }
            return true;
        }
        if self.is_leaf(n) {
            for i in n.lo..n.hi {
                if q.contains(self.points.xyz[i]) {
                    if out.len() >= limit {
                        return false;
                    }
                    out.push(i as u32);
                }
            }
            return true;
        }
        for c in self.children(n) {
            if !self.box_rec(&c, q, out, limit) {
                return false;
            }
        }
        true
    }

    // -------------------------------------------------------------- polygon

    /// Indices of the points inside a polygon in plan (ring of `[x, y]`, closed or not), with an
    /// optional elevation range. Points on an edge may fall either way.
    pub fn query_polygon(&self, ring: &[[f64; 2]], z_range: Option<(f64, f64)>) -> Vec<u32> {
        if ring.len() < 3 || self.is_empty() {
            return Vec::new();
        }
        let mut q = Bounds3::EMPTY;
        for r in ring {
            q.include([r[0], r[1], 0.0]);
        }
        let (z0, z1) = z_range.unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
        q.min[2] = z0;
        q.max[2] = z1;
        let o = ring[0];
        let local: Vec<[f64; 2]> = ring.iter().map(|r| [r[0] - o[0], r[1] - o[1]]).collect();
        self.query_box(&q).into_iter().filter(|&i| {
            let p = self.points.xyz[i as usize];
            point_in_ring([p[0] - o[0], p[1] - o[1]], &local)
        }).collect()
    }

    // ------------------------------------------------------------- polyline

    /// Indices of the points within `distance` (plan distance, edges included) of a polyline: a
    /// corridor swath. Each point appears once.
    pub fn query_polyline(&self, line: &[[f64; 2]], distance: f64) -> Vec<u32> {
        if line.is_empty() || self.is_empty() || !(distance >= 0.0) {
            return Vec::new();
        }
        let mut out: Vec<u32> = Vec::new();
        let segs: Vec<([f64; 2], [f64; 2])> = if line.len() == 1 { vec![(line[0], line[0])] } else { line.windows(2).map(|w| (w[0], w[1])).collect() };
        for (a, b) in segs {
            let q = Bounds3::plan([a[0].min(b[0]) - distance, a[1].min(b[1]) - distance], [a[0].max(b[0]) + distance, a[1].max(b[1]) + distance]);
            for i in self.query_box(&q) {
                let p = self.points.xyz[i as usize];
                if dist_to_segment([p[0] - a[0], p[1] - a[1]], [0.0, 0.0], [b[0] - a[0], b[1] - a[1]]) <= distance {
                    out.push(i);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    // ------------------------------------------------------------------ LOD

    /// At most `max_points` points for a view box, spread evenly in space, for drawing.
    ///
    /// If the view holds `max_points` points or fewer, all are returned (`complete`). Otherwise
    /// the octree is cut at the shallowest depth where the nodes in view outnumber `max_points`;
    /// each node contributes its first point inside the view, and the list (which is in Morton
    /// order) is thinned evenly to `max_points`.
    pub fn lod(&self, view: &Bounds3, max_points: usize) -> LodResult {
        if self.is_empty() || max_points == 0 {
            return LodResult { indices: Vec::new(), depth: 0, complete: self.is_empty() };
        }
        let mut all = Vec::new();
        if self.box_rec(&self.root(), view, &mut all, max_points + 1) || all.len() <= max_points {
            return LodResult { indices: all, depth: BITS, complete: true };
        }
        let mut depth = BITS;
        for d in 0..=BITS {
            let mut count = 0usize;
            self.reps(&self.root(), view, d, &mut |_| {
                count += 1;
                count <= max_points
            });
            if count > max_points {
                depth = d;
                break;
            }
        }
        let mut reps = Vec::new();
        self.reps(&self.root(), view, depth, &mut |i| {
            reps.push(i);
            true
        });
        if reps.len() > max_points {
            let m = reps.len();
            reps = (0..max_points).map(|j| reps[j * m / max_points]).collect();
        }
        LodResult { indices: reps, depth, complete: false }
    }

    /// Calls `f` with the representative point of each node at `depth` that intersects the view.
    /// Returns `false` when `f` returned `false` and the walk stopped.
    fn reps(&self, n: &Node, view: &Bounds3, depth: u32, f: &mut dyn FnMut(u32) -> bool) -> bool {
        let m = self.margin();
        for k in 0..3 {
            if n.min[k] + n.size + m < view.min[k] || n.min[k] - m > view.max[k] {
                return true;
            }
        }
        if n.depth >= depth || n.hi - n.lo == 1 || n.depth >= BITS {
            if let Some(i) = (n.lo..n.hi).find(|&i| view.contains(self.points.xyz[i])) {
                return f(i as u32);
            }
            return true;
        }
        for c in self.children(n) {
            if !self.reps(&c, view, depth, f) {
                return false;
            }
        }
        true
    }

    // ------------------------------------------------------------ neighbours

    /// The `k` nearest points to `p`, nearest first. `plan_only` ignores elevation.
    pub fn nearest(&self, p: [f64; 3], k: usize, plan_only: bool) -> Vec<Neighbor> {
        if self.is_empty() || k == 0 {
            return Vec::new();
        }
        let m = self.margin();
        let dist2 = |q: [f64; 3]| {
            let mut d = 0.0;
            for a in 0..(if plan_only { 2 } else { 3 }) {
                let t = q[a] - p[a];
                d += t * t;
            }
            d
        };
        let node_d2 = |n: &Node| {
            let mut d = 0.0;
            for a in 0..(if plan_only { 2 } else { 3 }) {
                let (lo, hi) = (n.min[a] - m, n.min[a] + n.size + m);
                let t = if p[a] < lo { lo - p[a] } else if p[a] > hi { p[a] - hi } else { 0.0 };
                d += t * t;
            }
            d
        };
        let mut heap = BinaryHeap::new();
        let root = self.root();
        heap.push(DistItem { d2: node_d2(&root), node: root });
        let mut best: BinaryHeap<Cand> = BinaryHeap::new();
        while let Some(DistItem { d2, node }) = heap.pop() {
            if best.len() == k && d2 > best.peek().map_or(f64::INFINITY, |c| c.0) {
                break;
            }
            if self.is_leaf(&node) {
                for i in node.lo..node.hi {
                    let d = dist2(self.points.xyz[i]);
                    if best.len() < k {
                        best.push(Cand(d, i as u32));
                    } else if best.peek().is_some_and(|c| d < c.0) {
                        best.pop();
                        best.push(Cand(d, i as u32));
                    }
                }
            } else {
                for c in self.children(&node) {
                    let cd = node_d2(&c);
                    if best.len() < k || best.peek().is_some_and(|b| cd <= b.0) {
                        heap.push(DistItem { d2: cd, node: c });
                    }
                }
            }
        }
        let mut v: Vec<Cand> = best.into_vec();
        v.sort();
        v.into_iter().map(|c| Neighbor { index: c.1, distance: c.0.sqrt() }).collect()
    }

    // --------------------------------------------------------------- display

    /// Positions relative to a local origin (the centre of the chosen points' box) as `f32`,
    /// plus the origin to add back. `colors` (one per chosen point, in the order of `indices`) is
    /// copied; pass an empty slice for none.
    pub fn display_buffer(&self, indices: &[u32], colors: &[[u8; 3]]) -> DisplayBuffer {
        let mut b = Bounds3::EMPTY;
        for &i in indices {
            b.include(self.points.xyz[i as usize]);
        }
        let origin = if b.is_empty() { [0.0; 3] } else { [(b.min[0] + b.max[0]) / 2.0, (b.min[1] + b.max[1]) / 2.0, (b.min[2] + b.max[2]) / 2.0] };
        let positions = indices.iter().map(|&i| {
            let p = self.points.xyz[i as usize];
            [(p[0] - origin[0]) as f32, (p[1] - origin[1]) as f32, (p[2] - origin[2]) as f32]
        }).collect();
        DisplayBuffer { origin, positions, colors: colors.to_vec(), indices: indices.to_vec() }
    }
}

/// Even-odd point in ring test (coordinates relative to a nearby origin).
pub(crate) fn point_in_ring(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = ring.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (ring[i], ring[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Distance from `p` to the segment `a`-`b` (all relative to a nearby origin).
pub(crate) fn dist_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let (cx, cy) = (a[0] + t * dx, a[1] + t * dy);
    ((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt()
}
