// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! World-space bodies and the prepared scene the engine searches.

use std::collections::HashMap;

use afrobim_draw::Mesh3;
use afrobim_kernel::{Element, ElementId, SitePlacement};
use serde::{Deserialize, Serialize};

use crate::geom::*;

/// Where an element's own frame sits in the world: a translation and a rotation about Z, the
/// same rule the 3D view uses (`world = origin + Rz(rotation) * local`, millimetres).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub origin: V3,
    pub rotation_deg: f64,
}

impl Placement {
    pub fn identity() -> Placement {
        Placement { origin: [0.0; 3], rotation_deg: 0.0 }
    }
    pub fn new(origin: V3, rotation_deg: f64) -> Placement {
        Placement { origin, rotation_deg }
    }
    /// From a site placement, plus any extra lift the app applies on top of `site.z` (for
    /// example a sign's mounting height).
    pub fn from_site(site: &SitePlacement, extra_z: f64) -> Placement {
        Placement { origin: [site.x, site.y, site.z + extra_z], rotation_deg: site.rotation_deg }
    }
    /// The same placement without rotation (categories the 3D view leaves unrotated).
    pub fn unrotated(self) -> Placement {
        Placement { rotation_deg: 0.0, ..self }
    }
    pub fn apply(&self, p: V3) -> V3 {
        let (s, c) = self.rotation_deg.to_radians().sin_cos();
        [self.origin[0] + p[0] * c - p[1] * s, self.origin[1] + p[0] * s + p[1] * c, self.origin[2] + p[2]]
    }
}

/// One element's solids in world millimetres, with the attributes rules select on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Body {
    pub id: ElementId,
    /// The mark or code shown in reports (the element's `name`).
    pub mark: String,
    pub category: String,
    pub layer: String,
    pub host: Option<ElementId>,
    /// Already transformed to world millimetres, counter-clockwise seen from outside.
    pub meshes: Vec<Mesh3>,
}

impl Body {
    pub fn new(id: ElementId, mark: &str, category: &str, layer: &str, host: Option<ElementId>, meshes: Vec<Mesh3>) -> Body {
        Body { id, mark: mark.to_string(), category: category.to_string(), layer: layer.to_string(), host, meshes }
    }

    /// Builds a body from an element, its generated solids (own frame) and the placement the
    /// app uses to put them in the world.
    pub fn from_element(e: &Element, solids: &[Mesh3], placement: &Placement) -> Body {
        let meshes = solids
            .iter()
            .map(|m| Mesh3 {
                name: m.name.clone(),
                vertices: m.vertices.iter().map(|v| placement.apply(*v)).collect(),
                triangles: m.triangles.clone(),
                colour: m.colour,
            })
            .collect();
        Body { id: e.id, mark: e.name.clone(), category: e.category.clone(), layer: e.layer.clone(), host: e.host, meshes }
    }
}

/// One mesh prepared for testing.
pub(crate) struct Part {
    pub tris: Vec<Tri>,
    pub boxes: Vec<Aabb>,
    pub aabb: Aabb,
    pub closed: bool,
    pub verts: Vec<V3>,
}

impl Part {
    fn new(m: &Mesh3) -> Part {
        let n = m.vertices.len() as u32;
        let valid: Vec<[u32; 3]> = m.triangles.iter().copied().filter(|t| t.iter().all(|&i| i < n)).collect();
        let tris: Vec<Tri> = valid
            .iter()
            .map(|t| [m.vertices[t[0] as usize], m.vertices[t[1] as usize], m.vertices[t[2] as usize]])
            .filter(|t| unit_normal(t).is_some())
            .collect();
        let boxes: Vec<Aabb> = tris.iter().map(tri_box).collect();
        let aabb = boxes.iter().fold(Aabb::empty(), |a, b| a.union(b));
        // Closed: after welding vertices within a micron, every edge is shared by exactly two
        // triangles.
        let mut weld: HashMap<[i64; 3], u32> = HashMap::new();
        let mut key = |v: V3| {
            let k = [(v[0] * 1000.0).round() as i64, (v[1] * 1000.0).round() as i64, (v[2] * 1000.0).round() as i64];
            let next = weld.len() as u32;
            *weld.entry(k).or_insert(next)
        };
        let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
        for t in &tris {
            let ids = [key(t[0]), key(t[1]), key(t[2])];
            for i in 0..3 {
                let (a, b) = (ids[i], ids[(i + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        let closed = !tris.is_empty() && edges.values().all(|&c| c == 2);
        Part { tris, boxes, aabb, closed, verts: m.vertices.clone() }
    }

    /// Indices of triangles whose box is within `m` of `region`.
    pub fn tris_near(&self, region: &Aabb, m: f64) -> Vec<usize> {
        (0..self.tris.len()).filter(|&i| self.boxes[i].overlaps(region, m)).collect()
    }

    /// True when `p` is inside this closed part and more than `eps` from its surface.
    pub fn strictly_inside(&self, p: V3, eps: f64) -> bool {
        if !self.closed || !self.aabb.contains_point(p, -eps) {
            return false;
        }
        for (i, t) in self.tris.iter().enumerate() {
            if self.boxes[i].contains_point(p, eps) && dist(closest_pt_tri(p, t), p) <= eps {
                return false;
            }
        }
        // Parity of ray hits, majority of three skew directions so a ray through an edge or
        // vertex cannot decide alone.
        const DIRS: [V3; 3] = [[0.5773, 0.3011, 0.7529], [-0.2713, 0.8217, 0.5037], [0.1931, -0.4457, -0.8751]];
        let odd = DIRS
            .iter()
            .filter(|d| {
                let l = len(**d);
                let d = scale(**d, 1.0 / l);
                self.tris.iter().filter(|t| ray_hits_tri(p, d, t)).count() % 2 == 1
            })
            .count();
        odd >= 2
    }
}

/// Bodies prepared for searching: world triangles, boxes and closedness per mesh.
pub struct Scene {
    pub(crate) bodies: Vec<Body>,
    pub(crate) parts: Vec<Vec<Part>>,
    pub(crate) boxes: Vec<Aabb>,
    pub(crate) hosts: HashMap<ElementId, ElementId>,
}

impl Scene {
    pub fn new(bodies: Vec<Body>) -> Scene {
        let parts: Vec<Vec<Part>> = bodies.iter().map(|b| b.meshes.iter().map(Part::new).collect()).collect();
        let boxes = parts.iter().map(|ps| ps.iter().fold(Aabb::empty(), |a, p| a.union(&p.aabb))).collect();
        let hosts = bodies.iter().filter_map(|b| b.host.map(|h| (b.id, h))).collect();
        Scene { bodies, parts, boxes, hosts }
    }

    pub fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    /// Whether every mesh of the body is a closed solid (open meshes such as terrain get
    /// intersection and distance tests only).
    pub fn is_closed(&self, index: usize) -> bool {
        !self.parts[index].is_empty() && self.parts[index].iter().all(|p| p.closed)
    }

    /// True when `a` and `b` are on one host chain: one hosts the other, directly or through
    /// intermediate elements.
    pub(crate) fn host_related(&self, a: ElementId, b: ElementId) -> bool {
        let up = |from: ElementId, target: ElementId| {
            let mut cur = from;
            for _ in 0..64 {
                match self.hosts.get(&cur) {
                    Some(&h) if h == target => return true,
                    Some(&h) => cur = h,
                    None => return false,
                }
            }
            false
        };
        up(a, b) || up(b, a)
    }
}
