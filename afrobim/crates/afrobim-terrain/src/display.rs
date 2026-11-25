// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Display helpers: plain polylines and triangles in source coordinates. No rendering, no colour,
//! no drawing-library types; a front end draws them as it likes.
//!
//! Everything is `[x, y, z]` in metres exactly as stored in the surface (no shift, no scale).

use afrobim_corridor::Tin;
use serde::{Deserialize, Serialize};

use crate::contour::{ContourKind, ContourSet};
use crate::definition::{BreaklineKind, Operation, SurfaceDefinition};

/// A polyline with the facts a plot needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Polyline {
    /// A closed polyline does not repeat its first vertex.
    pub points: Vec<[f64; 3]>,
    pub closed: bool,
}

/// A contour ready to plot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContourLine {
    pub level: f64,
    pub major: bool,
    pub line: Polyline,
}

/// Every triangle as three corners, counter-clockwise.
pub fn triangles(tin: &Tin) -> Vec<[[f64; 3]; 3]> {
    tin.triangles()
        .iter()
        .map(|t| {
            let c = t.map(|i| tin.points()[i as usize]);
            let a2 = (c[1][0] - c[0][0]) * (c[2][1] - c[0][1]) - (c[1][1] - c[0][1]) * (c[2][0] - c[0][0]);
            if a2 < 0.0 {
                [c[0], c[2], c[1]]
            } else {
                c
            }
        })
        .collect()
}

/// Every edge once, as two end points (the wireframe).
pub fn wireframe(tin: &Tin) -> Vec<[[f64; 3]; 2]> {
    let mut edges: Vec<(u32, u32)> = Vec::with_capacity(tin.triangles().len() * 3);
    for t in tin.triangles() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            edges.push((a.min(b), a.max(b)));
        }
    }
    edges.sort_unstable();
    edges.dedup();
    edges.into_iter().map(|(a, b)| [tin.points()[a as usize], tin.points()[b as usize]]).collect()
}

/// The vertices in use, for plotting as points.
pub fn point_markers(tin: &Tin) -> Vec<[f64; 3]> {
    let mut used = vec![false; tin.points().len()];
    for &i in tin.triangles().iter().flatten() {
        used[i as usize] = true;
    }
    tin.points().iter().zip(used).filter(|(_, u)| *u).map(|(p, _)| *p).collect()
}

/// The outer edge and the edges of holes, chained into closed polylines.
pub fn outline(tin: &Tin) -> Vec<Polyline> {
    let edges = tin.boundary_edges();
    let mut used = vec![false; edges.len()];
    let mut out = Vec::new();
    for s in 0..edges.len() {
        if used[s] {
            continue;
        }
        used[s] = true;
        let start = edges[s][0];
        let mut at = edges[s][1];
        let mut verts = vec![start];
        while at != start {
            verts.push(at);
            let next = (0..edges.len()).find(|&k| !used[k] && (edges[k][0] == at || edges[k][1] == at));
            match next {
                Some(k) => {
                    used[k] = true;
                    at = if edges[k][0] == at { edges[k][1] } else { edges[k][0] };
                }
                None => break,
            }
        }
        let closed = at == start;
        out.push(Polyline { points: verts.into_iter().map(|v| tin.points()[v as usize]).collect(), closed });
    }
    out
}

/// Contours as 3D polylines (every vertex at the contour's level).
pub fn contour_lines(set: &ContourSet) -> Vec<ContourLine> {
    set.contours
        .iter()
        .map(|c| ContourLine { level: c.level, major: c.kind == ContourKind::Major, line: Polyline { points: c.points.iter().map(|p| [p[0], p[1], c.level]).collect(), closed: c.closed } })
        .collect()
}

/// The breaklines of a definition as polylines, as given (proximity breaklines carry the
/// elevations written in the definition, which the build ignores).
pub fn breaklines(def: &SurfaceDefinition) -> Vec<(String, BreaklineKind, Polyline)> {
    def.operations
        .iter()
        .filter_map(|op| match op {
            Operation::AddBreaklines { lines } => Some(lines),
            _ => None,
        })
        .flatten()
        .map(|l| (l.name.clone(), l.kind, Polyline { points: l.vertices.clone(), closed: false }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{contours, ContourOptions};

    #[test]
    fn display_helpers_keep_source_coordinates() {
        let (ox, oy) = (512_345.678, 9_876_543.219);
        let t = Tin::from_grid(ox, oy, 5.0, 5.0, 3, 3, |x, _| x - ox).unwrap();
        let tris = triangles(&t);
        assert_eq!(tris.len(), 8);
        assert!(tris.iter().flatten().all(|p| p[0] >= ox && p[1] >= oy));
        assert!(tris.iter().flatten().any(|p| p[0] == ox + 10.0 && p[1] == oy + 10.0));
        assert_eq!(wireframe(&t).len(), 16);
        assert_eq!(point_markers(&t).len(), 9);
        let o = outline(&t);
        assert_eq!(o.len(), 1);
        assert!(o[0].closed);
        assert_eq!(o[0].points.len(), 8);
        let set = contours(&t, &ContourOptions::new(2.0, Some(4.0))).unwrap();
        let lines = contour_lines(&set);
        assert!(!lines.is_empty());
        assert!(lines.iter().all(|l| l.line.points.iter().all(|p| p[2] == l.level)));
        assert!(lines.iter().any(|l| l.major) && lines.iter().any(|l| !l.major));
    }

    #[test]
    fn outline_of_a_surface_with_a_hole_has_two_loops() {
        use crate::{BoundaryKind, Operation, SurfaceDefinition, SurfacePoint};
        let mut pts = Vec::new();
        for j in 0..7 {
            for i in 0..7 {
                pts.push(SurfacePoint::new("p", i as f64 * 10.0, j as f64 * 10.0, 1.0));
            }
        }
        let b = SurfaceDefinition::new()
            .with(Operation::AddPoints { group: "g".into(), points: pts })
            .with(Operation::AddBoundary { name: "pond".into(), kind: BoundaryKind::Hide, vertices: vec![[20.0, 20.0], [40.0, 20.0], [40.0, 40.0], [20.0, 40.0]] })
            .build()
            .unwrap();
        let o = outline(&b.tin);
        assert_eq!(o.len(), 2);
        assert!(o.iter().all(|l| l.closed));
        let def = SurfaceDefinition::new().with(Operation::AddBreaklines { lines: vec![crate::Breakline { name: "k".into(), kind: BreaklineKind::Standard, vertices: vec![[0.0, 0.0, 1.0], [1.0, 1.0, 1.0]] }] });
        assert_eq!(breaklines(&def).len(), 1);
    }
}
