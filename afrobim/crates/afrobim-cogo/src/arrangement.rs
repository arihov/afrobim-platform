//! A planar arrangement of polylines: split every line where it meets another, merge vertices closer
//! than a tolerance, and trace the faces. Used by the topology report and the parcel split.
//!
//! Method. Vertices are merged by a grid of cell size equal to the tolerance, so two points closer
//! than the tolerance become one vertex. Segments are split (a) at proper crossings, at the crossing
//! point, and (b) at every vertex lying within the tolerance of the segment (T junctions and the ends
//! of overlapping collinear runs). The remaining pieces form a planar graph; dangling edges are
//! pruned (they bound no area). Faces are traced by always taking, at the end of a half-edge, the
//! next outgoing half-edge clockwise from its twin, which keeps the face on the left. A face with
//! positive signed area is a bounded region; a negative one is the outer boundary of a connected
//! component. Such an outer boundary lying inside a region of another component becomes a hole of the
//! smallest region containing it.
//!
//! Limits: merging by grid cells is approximate near cell edges (two points just under the tolerance
//! apart in different cells are still found, because neighbours are searched); chains of points each
//! within the tolerance of the next can all merge into one vertex.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::error::{CogoError, Result};
use crate::geom::{cross, ring_area, ring_winding, seg_nearest, Xy};
use crate::parcel::{interior_point, segs_cross};

/// A directed use of an edge by input line `tag`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Piece {
    pub tag: usize,
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct Face {
    pub vertex_ids: Vec<usize>,
    pub ring: Vec<Xy>,
    pub half_edges: Vec<(usize, usize)>,
    /// Signed area of the outer ring only.
    pub ring_area: f64,
    pub holes: Vec<Vec<Xy>>,
}

impl Face {
    /// Area of the ring minus its holes (positive faces only).
    pub fn net_area(&self) -> f64 {
        self.ring_area - self.holes.iter().map(|h| ring_area(h).abs()).sum::<f64>()
    }
    pub fn interior_point(&self) -> Option<Xy> {
        interior_point(&self.ring, &self.holes)
    }
}

pub(crate) struct Arrangement {
    pub verts: Vec<Xy>,
    pub pieces: Vec<Piece>,
    /// Bounded faces (positive area).
    pub faces: Vec<Face>,
}

struct Registry {
    tol: f64,
    verts: Vec<Xy>,
    grid: HashMap<(i64, i64), Vec<usize>>,
}

impl Registry {
    fn cell(&self, p: Xy) -> (i64, i64) {
        ((p.e / self.tol).floor() as i64, (p.n / self.tol).floor() as i64)
    }
    fn id(&mut self, p: Xy) -> usize {
        let (cx, cy) = self.cell(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(l) = self.grid.get(&(cx + dx, cy + dy)) {
                    for &v in l {
                        if self.verts[v].dist(p) <= self.tol {
                            return v;
                        }
                    }
                }
            }
        }
        let v = self.verts.len();
        self.verts.push(p);
        self.grid.entry((cx, cy)).or_default().push(v);
        v
    }
}

/// `lines`: (points, closed, tag). Tags are the caller's labels carried to the pieces.
pub(crate) fn build(lines: &[(Vec<Xy>, bool, usize)], snap_tol: f64) -> Result<Arrangement> {
    if !(snap_tol > 0.0) || !snap_tol.is_finite() {
        return Err(CogoError::Invalid("snap tolerance must be positive".into()));
    }
    let mut reg = Registry { tol: snap_tol, verts: vec![], grid: HashMap::new() };
    struct Raw {
        tag: usize,
        a: usize,
        b: usize,
        extra: Vec<usize>,
    }
    let mut raws: Vec<Raw> = Vec::new();
    for (pts, closed, tag) in lines {
        let ids: Vec<usize> = pts.iter().map(|p| reg.id(*p)).collect();
        let n = ids.len();
        let m = if *closed { n } else { n.saturating_sub(1) };
        for i in 0..m {
            let (a, b) = (ids[i], ids[(i + 1) % n]);
            if a != b {
                raws.push(Raw { tag: *tag, a, b, extra: vec![] });
            }
        }
    }
    // (a) proper crossings
    for i in 0..raws.len() {
        for j in i + 1..raws.len() {
            let (ra, rb) = (&raws[i], &raws[j]);
            if ra.a == rb.a || ra.a == rb.b || ra.b == rb.a || ra.b == rb.b {
                continue;
            }
            let (p, q, r, s) = (reg.verts[ra.a], reg.verts[ra.b], reg.verts[rb.a], reg.verts[rb.b]);
            if segs_cross(p, q, r, s) {
                let d1 = q.sub(p);
                let d2 = s.sub(r);
                let den = cross(d1, d2);
                let t = cross(r.sub(p), d2) / den;
                let x = p.add(t * d1.0, t * d1.1);
                let v = reg.id(x);
                raws[i].extra.push(v);
                raws[j].extra.push(v);
            }
        }
    }
    // (b) vertices on segments, then split
    let nv = reg.verts.len();
    let mut pieces: Vec<Piece> = Vec::new();
    for r in &raws {
        let (pa, pb) = (reg.verts[r.a], reg.verts[r.b]);
        let mut on: BTreeSet<usize> = r.extra.iter().copied().collect();
        for v in 0..nv {
            if v != r.a && v != r.b && seg_nearest(reg.verts[v], pa, pb).0 <= snap_tol {
                on.insert(v);
            }
        }
        on.remove(&r.a);
        on.remove(&r.b);
        let d = pb.sub(pa);
        let l2 = d.0 * d.0 + d.1 * d.1;
        let mut chain: Vec<(f64, usize)> = on
            .into_iter()
            .map(|v| {
                let w = reg.verts[v].sub(pa);
                ((w.0 * d.0 + w.1 * d.1) / l2, v)
            })
            .collect();
        chain.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
        let mut prev = r.a;
        for (_, v) in chain.into_iter().map(|c| c).chain(std::iter::once((1.0, r.b))) {
            if v != prev {
                pieces.push(Piece { tag: r.tag, from: prev, to: v });
                prev = v;
            }
        }
    }
    let verts = reg.verts;

    // undirected edges
    let mut edges: BTreeSet<(usize, usize)> = BTreeSet::new();
    for p in &pieces {
        edges.insert((p.from.min(p.to), p.from.max(p.to)));
    }
    // prune dangling
    loop {
        let mut deg: HashMap<usize, usize> = HashMap::new();
        for (a, b) in &edges {
            *deg.entry(*a).or_default() += 1;
            *deg.entry(*b).or_default() += 1;
        }
        let before = edges.len();
        edges.retain(|(a, b)| deg[a] > 1 && deg[b] > 1);
        if edges.len() == before {
            break;
        }
    }
    // outgoing half-edges sorted by angle
    let mut out: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (a, b) in &edges {
        out.entry(*a).or_default().push(*b);
        out.entry(*b).or_default().push(*a);
    }
    for (u, list) in out.iter_mut() {
        let pu = verts[*u];
        list.sort_by(|x, y| {
            let (ax, ay) = verts[*x].sub(pu);
            let (bx, by) = verts[*y].sub(pu);
            ay.atan2(ax).partial_cmp(&by.atan2(bx)).unwrap()
        });
    }
    let mut visited: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut all_faces: Vec<Face> = Vec::new();
    for (a, b) in &edges {
        for (u0, v0) in [(*a, *b), (*b, *a)] {
            if visited.contains(&(u0, v0)) {
                continue;
            }
            let mut ids = Vec::new();
            let mut hes = Vec::new();
            let (mut u, mut v) = (u0, v0);
            loop {
                visited.insert((u, v));
                ids.push(u);
                hes.push((u, v));
                let list = &out[&v];
                let k = list.iter().position(|w| *w == u).unwrap();
                let w = list[(k + list.len() - 1) % list.len()];
                u = v;
                v = w;
                if (u, v) == (u0, v0) || visited.contains(&(u, v)) {
                    break;
                }
            }
            let ring: Vec<Xy> = ids.iter().map(|i| verts[*i]).collect();
            let ra = ring_area(&ring);
            all_faces.push(Face { vertex_ids: ids, ring, half_edges: hes, ring_area: ra, holes: vec![] });
        }
    }
    // holes: negative rings belong to the smallest positive face (of another component) that holds them
    let (neg, mut pos): (Vec<Face>, Vec<Face>) = all_faces.into_iter().partition(|f| f.ring_area < 0.0);
    for nf in &neg {
        let own: BTreeSet<usize> = nf.vertex_ids.iter().copied().collect();
        let probe = nf.ring[0];
        let mut best: Option<(f64, usize)> = None;
        for (i, pf) in pos.iter().enumerate() {
            if pf.vertex_ids.iter().any(|v| own.contains(v)) {
                continue;
            }
            if ring_winding(&pf.ring, probe) != 0 && best.map_or(true, |b| pf.ring_area < b.0) {
                best = Some((pf.ring_area, i));
            }
        }
        if let Some((_, i)) = best {
            pos[i].holes.push(nf.ring.clone());
        }
    }
    pos.retain(|f| f.ring_area > 0.0);
    Ok(Arrangement { verts, pieces, faces: pos })
}
