// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The things a sight line can hit: triangles of the ground, corridor surfaces and every other
//! modelled solid, held in site coordinates in **metres**, with a plan grid so that a sight line
//! only meets the triangles near it.

use std::sync::Arc;

use afrobim_coordination::geom::{seg_tri_cross, tri_box, Aabb, Tri, V3};
use afrobim_coordination::{Body, Placement};
use afrobim_corridor::Tin;
use afrobim_draw::Mesh3;
use afrobim_kernel::ElementId;
use serde::{Deserialize, Serialize};

/// What kind of thing blocked a sight line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObstacleKind {
    /// The existing ground (a TIN surface).
    Ground,
    /// The corridor's finished surface or side slopes.
    CorridorSurface,
    /// The road's own vertical profile (a crest), from the alignment's profile.
    RoadProfile,
    /// A lateral obstruction entered by the user (an offset line beside the road).
    LateralOffset,
    /// The back of the sign face itself (the eye is not in front of the face).
    BackOfSign,
    Sign,
    Post,
    Foundation,
    Gantry,
    Lighting,
    Barrier,
    /// Any other modelled solid (drains, culverts, pavement, structures).
    Other,
}

impl ObstacleKind {
    pub fn label(self) -> &'static str {
        match self {
            ObstacleKind::Ground => "ground surface",
            ObstacleKind::CorridorSurface => "corridor surface",
            ObstacleKind::RoadProfile => "road vertical profile (crest)",
            ObstacleKind::LateralOffset => "lateral obstruction offset",
            ObstacleKind::BackOfSign => "back of the sign face",
            ObstacleKind::Sign => "another sign",
            ObstacleKind::Post => "post",
            ObstacleKind::Foundation => "foundation",
            ObstacleKind::Gantry => "gantry",
            ObstacleKind::Lighting => "lighting column",
            ObstacleKind::Barrier => "barrier",
            ObstacleKind::Other => "other modelled solid",
        }
    }

    /// The kind for an element category. The category names are those of the AfroBIM modules.
    pub fn of_category(category: &str) -> ObstacleKind {
        match category {
            "sign" => ObstacleKind::Sign,
            "post" => ObstacleKind::Post,
            "foundation" => ObstacleKind::Foundation,
            "gantry" => ObstacleKind::Gantry,
            "lighting" => ObstacleKind::Lighting,
            "restraint" => ObstacleKind::Barrier,
            "surface" => ObstacleKind::Ground,
            "corridor" => ObstacleKind::CorridorSurface,
            _ => ObstacleKind::Other,
        }
    }
}

/// A named set of triangles: one mesh of one element.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub kind: ObstacleKind,
    pub label: String,
    pub element: Option<ElementId>,
}

/// The nearest triangle a segment crosses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// Fraction of the segment from its start, 0 to 1.
    pub t: f64,
    pub point: V3,
    pub group: usize,
}

struct Grid {
    min_x: f64,
    min_y: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

/// Triangles in site metres, grouped by element, with a plan grid. Build it with the `add_*`
/// methods, call [`ObstacleScene::finish`] once, then query. Without `finish` queries still work
/// but test every triangle.
#[derive(Default)]
pub struct ObstacleScene {
    tris: Vec<Tri>,
    boxes: Vec<Aabb>,
    group_of: Vec<u32>,
    groups: Vec<Group>,
    grid: Option<Grid>,
    tins: Vec<Arc<Tin>>,
}

/// Tolerance of the crossing test, m: a segment must pass more than this off a triangle's plane
/// at both ends.
const CROSS_EPS_M: f64 = 1e-6;

impl ObstacleScene {
    pub fn new() -> ObstacleScene {
        ObstacleScene::default()
    }

    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    pub fn group(&self, index: usize) -> &Group {
        &self.groups[index]
    }

    fn new_group(&mut self, kind: ObstacleKind, label: &str, element: Option<ElementId>) -> u32 {
        self.groups.push(Group { kind, label: label.to_string(), element });
        self.grid = None;
        (self.groups.len() - 1) as u32
    }

    fn push_tri(&mut self, t: Tri, group: u32) {
        self.boxes.push(tri_box(&t));
        self.tris.push(t);
        self.group_of.push(group);
    }

    /// Adds triangles given as vertices and index triples, already in site metres.
    pub fn add_mesh_m(&mut self, kind: ObstacleKind, label: &str, element: Option<ElementId>, vertices: &[V3], triangles: &[[u32; 3]]) {
        let g = self.new_group(kind, label, element);
        for t in triangles {
            if t.iter().all(|&i| (i as usize) < vertices.len()) {
                self.push_tri([vertices[t[0] as usize], vertices[t[1] as usize], vertices[t[2] as usize]], g);
            }
        }
    }

    /// Adds a mesh in an element's own frame in millimetres, moved to site metres by `placement`
    /// (`world = origin + Rz(rotation) * local`, the rule of the 3D view).
    pub fn add_mesh_mm(&mut self, kind: ObstacleKind, label: &str, element: Option<ElementId>, mesh: &Mesh3, placement: &Placement) {
        let v: Vec<V3> = mesh.vertices.iter().map(|p| mm_to_m(placement.apply(*p))).collect();
        self.add_mesh_m(kind, label, element, &v, &mesh.triangles);
    }

    /// Adds a body whose meshes are already in world millimetres (the coordination crate's
    /// [`Body`]). The kind comes from the body's category.
    pub fn add_body(&mut self, body: &Body) {
        let kind = ObstacleKind::of_category(&body.category);
        for m in &body.meshes {
            let v: Vec<V3> = m.vertices.iter().map(|p| mm_to_m(*p)).collect();
            self.add_mesh_m(kind, &body.mark, Some(body.id), &v, &m.triangles);
        }
    }

    /// Adds the ground as a TIN (site metres). The TIN is also kept for ground elevation queries.
    pub fn add_tin(&mut self, label: &str, element: Option<ElementId>, tin: Arc<Tin>) {
        let g = self.new_group(ObstacleKind::Ground, label, element);
        for t in tin.triangles() {
            let p = |i: u32| tin.points()[i as usize];
            self.push_tri([p(t[0]), p(t[1]), p(t[2])], g);
        }
        self.tins.push(tin);
    }

    /// The elevation of the ground at a plan point, m, from the first TIN that covers it.
    pub fn ground_z(&self, x: f64, y: f64) -> Option<f64> {
        self.tins.iter().find_map(|t| t.elevation_at(x, y))
    }

    pub fn has_ground(&self) -> bool {
        !self.tins.is_empty()
    }

    /// Builds the plan grid. Cheap to call again after adding more.
    pub fn finish(&mut self) {
        if self.tris.len() < 64 {
            self.grid = None;
            return;
        }
        let all = self.boxes.iter().fold(Aabb::empty(), |a, b| a.union(b));
        let (w, h) = (all.max[0] - all.min[0], all.max[1] - all.min[1]);
        let mean_extent = self.boxes.iter().map(|b| (b.max[0] - b.min[0]).max(b.max[1] - b.min[1])).sum::<f64>() / self.boxes.len() as f64;
        let mut cell = (mean_extent * 3.0).clamp(2.0, 100.0);
        while (w / cell + 1.0) * (h / cell + 1.0) > 250_000.0 {
            cell *= 2.0;
        }
        let nx = (w / cell).floor() as usize + 1;
        let ny = (h / cell).floor() as usize + 1;
        let mut cells = vec![Vec::new(); nx * ny];
        for (i, b) in self.boxes.iter().enumerate() {
            let ix0 = (((b.min[0] - all.min[0]) / cell).floor() as usize).min(nx - 1);
            let ix1 = (((b.max[0] - all.min[0]) / cell).floor() as usize).min(nx - 1);
            let iy0 = (((b.min[1] - all.min[1]) / cell).floor() as usize).min(ny - 1);
            let iy1 = (((b.max[1] - all.min[1]) / cell).floor() as usize).min(ny - 1);
            for iy in iy0..=iy1 {
                for ix in ix0..=ix1 {
                    cells[iy * nx + ix].push(i as u32);
                }
            }
        }
        self.grid = Some(Grid { min_x: all.min[0], min_y: all.min[1], cell, nx, ny, cells });
    }

    /// Indices of triangles whose cell the segment passes through (every triangle without a grid).
    fn candidates(&self, a: V3, b: V3) -> Vec<u32> {
        let Some(g) = &self.grid else {
            return (0..self.tris.len() as u32).collect();
        };
        let (mut p, mut q) = ((a[0], a[1]), (b[0], b[1]));
        if p.0 > q.0 {
            std::mem::swap(&mut p, &mut q);
        }
        let col = |x: f64| ((x - g.min_x) / g.cell).floor();
        let row = |y: f64| ((y - g.min_y) / g.cell).floor();
        let (c0, c1) = (col(p.0).max(0.0), col(q.0).min(g.nx as f64 - 1.0));
        if c0 > c1 {
            return Vec::new();
        }
        let mut out: Vec<u32> = Vec::new();
        let dx = q.0 - p.0;
        for ix in (c0 as usize)..=(c1 as usize) {
            let (xl, xr) = (p.0.max(g.min_x + ix as f64 * g.cell), q.0.min(g.min_x + (ix + 1) as f64 * g.cell));
            let (yl, yr) = if dx.abs() < 1e-12 {
                (p.1, q.1)
            } else {
                (p.1 + (q.1 - p.1) * (xl - p.0) / dx, p.1 + (q.1 - p.1) * (xr - p.0) / dx)
            };
            let r0 = row(yl.min(yr)).max(0.0);
            let r1 = row(yl.max(yr)).min(g.ny as f64 - 1.0);
            if r0 > r1 {
                continue;
            }
            for iy in (r0 as usize)..=(r1 as usize) {
                out.extend_from_slice(&g.cells[iy * g.nx + ix]);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The nearest triangle the segment `a`..`b` crosses, skipping groups whose element is in
    /// `ignore`. A segment that only touches a triangle or ends on it is not blocked.
    pub fn first_hit(&self, a: V3, b: V3, ignore: &[ElementId]) -> Option<Hit> {
        let seg = Aabb::of_points([&a, &b]);
        let mut best: Option<Hit> = None;
        let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
        if len <= 0.0 {
            return None;
        }
        for i in self.candidates(a, b) {
            let i = i as usize;
            if !self.boxes[i].overlaps(&seg, 0.0) {
                continue;
            }
            let g = self.group_of[i] as usize;
            if self.groups[g].element.is_some_and(|e| ignore.contains(&e)) {
                continue;
            }
            if let Some((x, _)) = seg_tri_cross(a, b, &self.tris[i], CROSS_EPS_M) {
                let t = ((x[0] - a[0]).powi(2) + (x[1] - a[1]).powi(2) + (x[2] - a[2]).powi(2)).sqrt() / len;
                if best.is_none_or(|h| t < h.t) {
                    best = Some(Hit { t, point: x, group: g });
                }
            }
        }
        best
    }
}

pub(crate) fn mm_to_m(p: V3) -> V3 {
    [p[0] / 1000.0, p[1] / 1000.0, p[2] / 1000.0]
}

/// A cuboid in site metres as two triangles per face, for hand-built obstructions and the thin
/// slab that stands for a sign face.
pub fn cuboid_m(min: V3, max: V3) -> (Vec<V3>, Vec<[u32; 3]>) {
    let m = Mesh3::cuboid("box", min, max, [0, 0, 0]);
    (m.vertices, m.triangles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_segment_through_a_box_is_blocked_and_one_beside_it_is_not() {
        let mut s = ObstacleScene::new();
        let (v, t) = cuboid_m([10.0, -1.0, 0.0], [11.0, 1.0, 3.0]);
        s.add_mesh_m(ObstacleKind::Other, "wall", Some(ElementId(5)), &v, &t);
        s.finish();
        let hit = s.first_hit([0.0, 0.0, 1.0], [20.0, 0.0, 1.0], &[]).unwrap();
        assert!((hit.t - 0.5).abs() < 1e-9);
        assert!(s.first_hit([0.0, 5.0, 1.0], [20.0, 5.0, 1.0], &[]).is_none());
        assert!(s.first_hit([0.0, 0.0, 1.0], [20.0, 0.0, 1.0], &[ElementId(5)]).is_none());
    }

    #[test]
    fn the_grid_finds_what_brute_force_finds() {
        // A field of small boxes; the grid is built (more than 64 triangles).
        let mut s = ObstacleScene::new();
        for i in 0..40 {
            let (v, t) = cuboid_m([i as f64 * 5.0, 2.0, 0.0], [i as f64 * 5.0 + 1.0, 3.0, 2.0]);
            s.add_mesh_m(ObstacleKind::Other, "b", Some(ElementId(i)), &v, &t);
        }
        let brute = |s: &ObstacleScene, a: V3, b: V3| s.first_hit(a, b, &[]).map(|h| h.group);
        let probes = [([0.0, 0.0, 1.0], [190.0, 5.0, 1.0]), ([60.5, 10.0, 1.0], [60.5, -10.0, 1.0]), ([0.0, 2.5, 5.0], [190.0, 2.5, 5.0]), ([-5.0, 2.5, 1.0], [300.0, 2.5, 1.0])];
        let before: Vec<_> = probes.iter().map(|(a, b)| brute(&s, *a, *b)).collect();
        s.finish();
        assert!(s.grid.is_some());
        let after: Vec<_> = probes.iter().map(|(a, b)| brute(&s, *a, *b)).collect();
        assert_eq!(before, after);
        assert_eq!(after[3], Some(0));
        assert_eq!(after[2], None);
    }
}
