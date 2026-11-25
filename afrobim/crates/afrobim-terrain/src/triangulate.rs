// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Constrained Delaunay triangulation with a local shift and exact predicates. Not public API.
//!
//! Method: a constrained Delaunay triangulation (CDT) is the triangulation of a point set that
//! contains given constraint segments as edges and is as close to Delaunay as that allows. See
//! L. P. Chew, "Constrained Delaunay triangulations", Algorithmica 4, 1989, and for the
//! incremental construction with edge flips, Anglada, "An improved incremental algorithm for
//! constructing restricted Delaunay triangulations", Computers & Graphics 21(2), 1997. The
//! implementation is the `spade` crate (exact predicates after Shewchuk 1997).
//!
//! Coordinates handed to `spade` are measured from the south-west corner of the data (a local
//! shift), so eastings of 500 000 m and northings of 10 000 000 m lose nothing to the
//! subtraction. The triangles returned refer to points by index; the points keep the
//! coordinates they were given. Only points created at a breakline crossing are computed, and
//! those are `local + origin`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use spade::{ConstrainedDelaunayTriangulation, HasPosition, Point2, Triangulation};

use crate::error::TerrainError;
use crate::geom::{orient, proper_crossing};

/// Plan-position index of the vertices, merging points closer than a tolerance.
pub(crate) struct VertexTable {
    pub pts: Vec<[f64; 3]>,
    pub origin: [f64; 2],
    tol: f64,
    cell: f64,
    map: HashMap<(i64, i64), Vec<u32>>,
}

impl VertexTable {
    pub fn new(origin: [f64; 2], tol: f64) -> VertexTable {
        VertexTable { pts: Vec::new(), origin, tol, cell: tol.max(1e-9), map: HashMap::new() }
    }

    fn key(&self, x: f64, y: f64) -> (i64, i64) {
        (((x - self.origin[0]) / self.cell).floor() as i64, ((y - self.origin[1]) / self.cell).floor() as i64)
    }

    /// The nearest vertex within the tolerance of a plan position, if any.
    pub fn find(&self, x: f64, y: f64) -> Option<u32> {
        let (kx, ky) = self.key(x, y);
        let mut best: Option<(f64, u32)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = self.map.get(&(kx + dx, ky + dy)) {
                    for &i in list {
                        let p = self.pts[i as usize];
                        let d2 = (p[0] - x).powi(2) + (p[1] - y).powi(2);
                        if d2 <= self.tol * self.tol && best.is_none_or(|(b, _)| d2 < b) {
                            best = Some((d2, i));
                        }
                    }
                }
            }
        }
        best.map(|b| b.1)
    }

    /// The index of the vertex at a position, adding it when no vertex is within tolerance.
    /// Returns the index and whether it was newly added. An existing vertex keeps its z.
    pub fn add(&mut self, p: [f64; 3]) -> (u32, bool) {
        if let Some(i) = self.find(p[0], p[1]) {
            return (i, false);
        }
        let i = self.pts.len() as u32;
        let k = self.key(p[0], p[1]);
        self.map.entry(k).or_default().push(i);
        self.pts.push(p);
        (i, true)
    }

    pub fn local(&self, i: u32) -> [f64; 2] {
        let p = self.pts[i as usize];
        [p[0] - self.origin[0], p[1] - self.origin[1]]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ConstraintKind {
    Break,
    Boundary,
}

/// A polyline (or ring) of vertex indices that must appear as edges of the triangulation.
pub(crate) struct Constraint {
    pub verts: Vec<u32>,
    pub closed: bool,
    pub kind: ConstraintKind,
    pub name: String,
}

/// Where two constraints crossed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Crossing {
    /// Easting of the crossing, m.
    pub x: f64,
    /// Northing of the crossing, m.
    pub y: f64,
    /// Elevation of the first line at the crossing, m (`None` for a boundary, which has none).
    pub z_first: Option<f64>,
    /// Elevation of the second line at the crossing, m.
    pub z_second: Option<f64>,
    /// Elevation given to the new point, m.
    pub z_used: f64,
    /// Name of the first line.
    pub first: String,
    /// Name of the second line.
    pub second: String,
}

pub(crate) struct CdtOutput {
    /// Counter-clockwise triangles (exact orientation on the given coordinates).
    pub triangles: Vec<[u32; 3]>,
    /// Edges (smaller index first) that are constraints.
    pub constrained: HashSet<(u32, u32)>,
    pub crossings: Vec<Crossing>,
    pub degenerate_dropped: usize,
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

struct Seg {
    a: u32,
    b: u32,
    constraint: usize,
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

/// Triangulates every vertex of the table with the constraints as edges. Crossing constraints
/// are cut at their crossing, where a new vertex is added to the table (see `Crossing`).
pub(crate) fn triangulate(table: &mut VertexTable, constraints: &[Constraint]) -> Result<CdtOutput, TerrainError> {
    // Segments of every constraint.
    let mut segs: Vec<Seg> = Vec::new();
    for (ci, c) in constraints.iter().enumerate() {
        let n = c.verts.len();
        let last = if c.closed { n } else { n.saturating_sub(1) };
        for k in 0..last {
            let (a, b) = (c.verts[k], c.verts[(k + 1) % n]);
            if a != b {
                segs.push(Seg { a, b, constraint: ci });
            }
        }
    }

    // Crossings: sweep over the segments sorted by their smallest x.
    let mut crossings = Vec::new();
    let mut splits: Vec<Vec<(f64, u32)>> = vec![Vec::new(); segs.len()];
    {
        let loc: Vec<([f64; 2], [f64; 2])> = segs.iter().map(|s| (table.local(s.a), table.local(s.b))).collect();
        let mut order: Vec<usize> = (0..segs.len()).collect();
        order.sort_by(|&i, &j| loc[i].0[0].min(loc[i].1[0]).total_cmp(&loc[j].0[0].min(loc[j].1[0])));
        for (oi, &i) in order.iter().enumerate() {
            let (a, b) = loc[i];
            let max_x = a[0].max(b[0]);
            let (min_y, max_y) = (a[1].min(b[1]), a[1].max(b[1]));
            for &j in &order[oi + 1..] {
                let (c, d) = loc[j];
                if c[0].min(d[0]) > max_x {
                    break;
                }
                if c[1].min(d[1]) > max_y || c[1].max(d[1]) < min_y {
                    continue;
                }
                let (si, sj) = (&segs[i], &segs[j]);
                if si.a == sj.a || si.a == sj.b || si.b == sj.a || si.b == sj.b {
                    continue;
                }
                if !proper_crossing(a, b, c, d) {
                    continue;
                }
                let (r, s) = ([b[0] - a[0], b[1] - a[1]], [d[0] - c[0], d[1] - c[1]]);
                let den = cross(r, s);
                let ca = [c[0] - a[0], c[1] - a[1]];
                let (t, u) = (cross(ca, s) / den, cross(ca, r) / den);
                let (t, u) = (t.clamp(0.0, 1.0), u.clamp(0.0, 1.0));
                let (pa, pb, pc, pd) = (table.pts[si.a as usize], table.pts[si.b as usize], table.pts[sj.a as usize], table.pts[sj.b as usize]);
                let (ki, kj) = (constraints[si.constraint].kind, constraints[sj.constraint].kind);
                let (z1, z2) = (pa[2] + t * (pb[2] - pa[2]), pc[2] + u * (pd[2] - pc[2]));
                let z_used = match (ki, kj) {
                    (ConstraintKind::Break, ConstraintKind::Break) => (z1 + z2) / 2.0,
                    (ConstraintKind::Break, ConstraintKind::Boundary) => z1,
                    (ConstraintKind::Boundary, ConstraintKind::Break) => z2,
                    (ConstraintKind::Boundary, ConstraintKind::Boundary) => (z1 + z2) / 2.0,
                };
                let x = [a[0] + t * r[0] + table.origin[0], a[1] + t * r[1] + table.origin[1]];
                let (v, _) = table.add([x[0], x[1], z_used]);
                splits[i].push((t, v));
                splits[j].push((u, v));
                let first_z = (ki == ConstraintKind::Break).then_some(z1);
                let second_z = (kj == ConstraintKind::Break).then_some(z2);
                crossings.push(Crossing {
                    x: x[0],
                    y: x[1],
                    z_first: first_z,
                    z_second: second_z,
                    z_used,
                    first: constraints[si.constraint].name.clone(),
                    second: constraints[sj.constraint].name.clone(),
                });
            }
        }
    }
    let mut pieces: Vec<(u32, u32)> = Vec::with_capacity(segs.len());
    for (k, s) in segs.iter().enumerate() {
        let mut chain = std::mem::take(&mut splits[k]);
        chain.sort_by(|p, q| p.0.total_cmp(&q.0));
        let mut prev = s.a;
        for (_, v) in chain.into_iter().map(|c| c).chain(std::iter::once((1.0, s.b))) {
            if v != prev {
                pieces.push((prev, v));
                prev = v;
            }
        }
    }

    // The triangulation.
    let mut cdt = ConstrainedDelaunayTriangulation::<V>::new();
    let mut handles = Vec::with_capacity(table.pts.len());
    for i in 0..table.pts.len() as u32 {
        let l = table.local(i);
        let h = cdt.insert(V { pos: Point2::new(l[0], l[1]), idx: i }).map_err(|e| TerrainError::Triangulation(format!("{e:?}")))?;
        handles.push(h);
    }
    if cdt.num_inner_faces() == 0 {
        return Err(TerrainError::NoSurface("the points are all on one line".into()));
    }
    let extra: RefCell<Vec<[f64; 3]>> = RefCell::new(Vec::new());
    let base = table.pts.len() as u32;
    for &(a, b) in &pieces {
        let (ha, hb) = (handles[a as usize], handles[b as usize]);
        if ha == hb {
            continue;
        }
        if cdt.can_add_constraint(ha, hb) {
            cdt.add_constraint(ha, hb);
        } else {
            // A rounding error left two pieces crossing: cut at the crossing, with the elevation
            // taken along this piece.
            let (la, lb) = (table.local(a), table.local(b));
            let (za, zb) = (table.pts[a as usize][2], table.pts[b as usize][2]);
            let origin = table.origin;
            let make = |p: Point2<f64>| {
                let d = [lb[0] - la[0], lb[1] - la[1]];
                let len2 = d[0] * d[0] + d[1] * d[1];
                let t = (((p.x - la[0]) * d[0] + (p.y - la[1]) * d[1]) / len2).clamp(0.0, 1.0);
                let mut e = extra.borrow_mut();
                let idx = base + e.len() as u32;
                e.push([p.x + origin[0], p.y + origin[1], za + t * (zb - za)]);
                V { pos: p, idx }
            };
            cdt.add_constraint_and_split(ha, hb, make);
        }
    }
    table.pts.extend(extra.into_inner());

    let mut triangles = Vec::with_capacity(cdt.num_inner_faces());
    let mut degenerate_dropped = 0;
    for f in cdt.inner_faces() {
        let [mut a, mut b, mut c] = f.vertices().map(|v| v.data().idx);
        let xy = |i: u32| {
            let p = table.pts[i as usize];
            [p[0], p[1]]
        };
        let o = orient(xy(a), xy(b), xy(c));
        if o == 0.0 {
            degenerate_dropped += 1;
            continue;
        }
        if o < 0.0 {
            std::mem::swap(&mut b, &mut c);
        }
        let _ = &mut a;
        triangles.push([a, b, c]);
    }
    let mut constrained = HashSet::new();
    for e in cdt.undirected_edges() {
        if e.is_constraint_edge() {
            let [u, v] = e.vertices().map(|v| v.data().idx);
            constrained.insert((u.min(v), u.max(v)));
        }
    }
    Ok(CdtOutput { triangles, constrained, crossings, degenerate_dropped })
}
