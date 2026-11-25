// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Sign artwork as triangle meshes: one mesh per colour and paint layer.
//!
//! A [`Drawing`] is a painter's list: later primitives cover earlier ones. In 3D that order has
//! to become distance from the plate, or coplanar faces of different colours would fight for the
//! same pixels. Each primitive therefore gets a *layer*: 0 if it covers nothing of another colour,
//! otherwise one more than the highest layer of a differently coloured primitive drawn earlier
//! whose bounding box it overlaps. Primitives of one colour on one layer share a mesh.

use std::collections::HashMap;

use afrobim_draw::{Colour, Drawing, Primitive, Pt2};
use lyon_tessellation::{
    math::point, path::Path, BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers,
};

/// Width given to an [`Primitive::Arc`], which is a centreline without a width of its own, mm.
pub const ARC_BAND_WIDTH_MM: f64 = 1.0;

/// Longest chord angle used when an arc is turned into a band, degrees.
const ARC_STEP_DEG: f64 = 6.0;

/// Triangles thinner than this (mm²) are dropped.
const MIN_TRIANGLE_AREA: f64 = 1e-9;

/// The triangles of one colour on one paint layer, in sheet coordinates (mm, Y up).
#[derive(Debug, Clone, PartialEq)]
pub struct ArtworkMesh {
    pub colour: Colour,
    /// Paint layer: 0 lies directly in front of the plate, higher layers lie in front of lower ones.
    pub layer: u32,
    pub vertices: Vec<[f64; 2]>,
    /// Indices into `vertices`, zero-based, anticlockwise.
    pub triangles: Vec<[u32; 3]>,
}

impl ArtworkMesh {
    /// Summed area of the triangles, mm².
    pub fn area(&self) -> f64 {
        self.triangles.iter().map(|t| signed_area(self.vertices[t[0] as usize], self.vertices[t[1] as usize], self.vertices[t[2] as usize])).sum()
    }
}

/// The meshes of a drawing and what was left out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Artwork {
    /// Ordered by layer, then by first appearance in the drawing.
    pub meshes: Vec<ArtworkMesh>,
    /// Symbol placeholders, which have no geometry yet.
    pub skipped_symbols: usize,
    /// Primitives that gave no triangles (degenerate outlines, zero-length bands and the like).
    pub skipped_degenerate: usize,
}

fn signed_area(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]))
}

/// Vertex coordinates (as bits) to their index in a mesh.
type VertexIndex = HashMap<(u64, u64), u32>;

/// What a primitive contributes before it is assigned to a mesh.
struct Piece {
    colour: Colour,
    vertices: Vec<[f64; 2]>,
    triangles: Vec<[u32; 3]>,
    min: [f64; 2],
    max: [f64; 2],
}

impl Piece {
    fn new(colour: Colour, vertices: Vec<[f64; 2]>, triangles: Vec<[u32; 3]>) -> Option<Piece> {
        // Keep only real triangles, all anticlockwise.
        let triangles: Vec<[u32; 3]> = triangles
            .into_iter()
            .filter_map(|t| {
                let area = signed_area(vertices[t[0] as usize], vertices[t[1] as usize], vertices[t[2] as usize]);
                if !area.is_finite() || area.abs() < MIN_TRIANGLE_AREA {
                    None
                } else if area < 0.0 {
                    Some([t[0], t[2], t[1]])
                } else {
                    Some(t)
                }
            })
            .collect();
        if triangles.is_empty() {
            return None;
        }
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for t in &triangles {
            for i in t {
                let v = vertices[*i as usize];
                for k in 0..2 {
                    min[k] = min[k].min(v[k]);
                    max[k] = max[k].max(v[k]);
                }
            }
        }
        Some(Piece { colour, vertices, triangles, min, max })
    }

    /// True when the bounding boxes share more than an edge.
    fn overlaps(&self, other: &Piece) -> bool {
        (0..2).all(|k| self.min[k] < other.max[k] - 1e-9 && other.min[k] < self.max[k] - 1e-9)
    }
}

/// Fills closed contours. Vertices that lyon passes through unchanged get their original `f64`
/// coordinates back, so only the points lyon creates at self-intersections carry `f32` precision.
fn fill(tess: &mut FillTessellator, contours: &[Vec<Pt2>], rule: FillRule) -> (Vec<[f64; 2]>, Vec<[u32; 3]>) {
    let mut exact: HashMap<(u32, u32), [f64; 2]> = HashMap::new();
    let mut builder = Path::builder();
    let mut any = false;
    for contour in contours {
        if contour.len() < 3 || contour.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            continue;
        }
        any = true;
        for (i, p) in contour.iter().enumerate() {
            let (x, y) = (p.x as f32, p.y as f32);
            exact.entry((x.to_bits(), y.to_bits())).or_insert([p.x, p.y]);
            if i == 0 {
                builder.begin(point(x, y));
            } else {
                builder.line_to(point(x, y));
            }
        }
        builder.end(true);
    }
    if !any {
        return (Vec::new(), Vec::new());
    }
    let path = builder.build();
    let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let options = FillOptions::default().with_fill_rule(rule);
    if tess
        .tessellate_path(&path, &options, &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position().to_array()))
        .is_err()
    {
        return (Vec::new(), Vec::new());
    }
    let vertices =
        buffers.vertices.iter().map(|p| exact.get(&(p[0].to_bits(), p[1].to_bits())).copied().unwrap_or([f64::from(p[0]), f64::from(p[1])])).collect();
    let triangles = buffers.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
    (vertices, triangles)
}

/// A straight band with square ends.
fn band(from: Pt2, to: Pt2, width: f64) -> (Vec<[f64; 2]>, Vec<[u32; 3]>) {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = (dx * dx + dy * dy).sqrt();
    if !len.is_finite() || len <= 0.0 || !width.is_finite() || width <= 0.0 {
        return (Vec::new(), Vec::new());
    }
    let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
    (
        vec![[from.x + nx, from.y + ny], [to.x + nx, to.y + ny], [to.x - nx, to.y - ny], [from.x - nx, from.y - ny]],
        vec![[0, 1, 2], [0, 2, 3]],
    )
}

/// An arc centreline as a thin band between two concentric polylines.
fn arc_band(centre: Pt2, radius: f64, start_deg: f64, end_deg: f64) -> (Vec<[f64; 2]>, Vec<[u32; 3]>) {
    let sweep = end_deg - start_deg;
    if !(radius.is_finite() && radius > 0.0 && sweep.is_finite() && sweep != 0.0 && centre.x.is_finite() && centre.y.is_finite()) {
        return (Vec::new(), Vec::new());
    }
    let segments = ((sweep.abs() / ARC_STEP_DEG).ceil() as usize).clamp(8, 720);
    let half = (ARC_BAND_WIDTH_MM / 2.0).min(radius);
    let mut vertices = Vec::with_capacity(2 * (segments + 1));
    let mut triangles = Vec::with_capacity(2 * segments);
    for i in 0..=segments {
        let a = (start_deg + sweep * i as f64 / segments as f64).to_radians();
        let (c, s) = (a.cos(), a.sin());
        vertices.push([centre.x + (radius - half) * c, centre.y + (radius - half) * s]);
        vertices.push([centre.x + (radius + half) * c, centre.y + (radius + half) * s]);
        if i > 0 {
            let k = 2 * i as u32;
            triangles.push([k - 2, k - 1, k + 1]);
            triangles.push([k - 2, k + 1, k]);
        }
    }
    (vertices, triangles)
}

/// Triangulates a drawing into one mesh per colour and paint layer.
///
/// Fills use the non-zero rule and glyph contours the even-odd rule, as on the canvas. Bands keep
/// their square ends. Arcs become bands [`ARC_BAND_WIDTH_MM`] wide. Symbol placeholders are
/// counted in [`Artwork::skipped_symbols`] and give no geometry.
pub fn tessellate_artwork(drawing: &Drawing) -> Artwork {
    let mut tess = FillTessellator::new();
    let mut artwork = Artwork::default();
    let mut pieces: Vec<(Piece, u32)> = Vec::new();

    for primitive in &drawing.primitives {
        let (colour, (vertices, triangles)) = match primitive {
            Primitive::Fill { outline, colour } => (*colour, fill(&mut tess, std::slice::from_ref(outline), FillRule::NonZero)),
            Primitive::Glyphs { contours, ink, .. } => (ink.unwrap_or(Colour::White), fill(&mut tess, contours, FillRule::EvenOdd)),
            Primitive::Line { from, to, width, colour } => (colour.unwrap_or(Colour::Black), band(*from, *to, *width)),
            Primitive::Arc { centre, radius, start_deg, end_deg, colour } => {
                (colour.unwrap_or(Colour::Black), arc_band(*centre, *radius, *start_deg, *end_deg))
            }
            Primitive::Symbol { .. } => {
                artwork.skipped_symbols += 1;
                continue;
            }
        };
        let Some(piece) = Piece::new(colour, vertices, triangles) else {
            artwork.skipped_degenerate += 1;
            continue;
        };
        let layer = pieces
            .iter()
            .filter(|(earlier, _)| piece.overlaps(earlier))
            .map(|(earlier, layer)| layer + u32::from(earlier.colour != piece.colour))
            .max()
            .unwrap_or(0);
        pieces.push((piece, layer));
    }

    // Merge the pieces of one colour and layer, sharing vertices.
    let mut slots: Vec<(ArtworkMesh, VertexIndex)> = Vec::new();
    for (piece, layer) in &pieces {
        let slot = match slots.iter().position(|(m, _)| m.layer == *layer && m.colour == piece.colour) {
            Some(i) => i,
            None => {
                slots.push((ArtworkMesh { colour: piece.colour, layer: *layer, vertices: Vec::new(), triangles: Vec::new() }, HashMap::new()));
                slots.len() - 1
            }
        };
        let (mesh, index) = &mut slots[slot];
        for t in &piece.triangles {
            let mut tri = [0u32; 3];
            for (k, i) in t.iter().enumerate() {
                // The file carries micro-millimetres; share vertices at that resolution.
                let v = piece.vertices[*i as usize].map(|c| (c * 1e6).round() / 1e6 + 0.0);
                tri[k] = *index.entry((v[0].to_bits(), v[1].to_bits())).or_insert_with(|| {
                    mesh.vertices.push(v);
                    mesh.vertices.len() as u32 - 1
                });
            }
            if tri[0] != tri[1] && tri[1] != tri[2] && tri[0] != tri[2] {
                mesh.triangles.push(tri);
            }
        }
    }
    let mut meshes: Vec<ArtworkMesh> = slots.into_iter().map(|(m, _)| m).filter(|m| !m.triangles.is_empty()).collect();
    meshes.sort_by_key(|m| m.layer); // stable: first appearance order within a layer
    artwork.meshes = meshes;
    artwork
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_draw::Extents;

    fn drawing(primitives: Vec<Primitive>) -> Drawing {
        Drawing { primitives, extents: Extents::default() }
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt2> {
        vec![Pt2::new(x0, y0), Pt2::new(x1, y0), Pt2::new(x1, y1), Pt2::new(x0, y1)]
    }

    #[test]
    fn layers_follow_paint_order_only_where_colours_overlap() {
        let art = tessellate_artwork(&drawing(vec![
            Primitive::Fill { outline: rect(0.0, 0.0, 100.0, 100.0), colour: Colour::Green },
            Primitive::Fill { outline: rect(10.0, 10.0, 40.0, 40.0), colour: Colour::White },
            Primitive::Fill { outline: rect(60.0, 60.0, 90.0, 90.0), colour: Colour::White },
            Primitive::Fill { outline: rect(15.0, 15.0, 20.0, 20.0), colour: Colour::Black },
            Primitive::Fill { outline: rect(200.0, 0.0, 210.0, 10.0), colour: Colour::Black },
        ]));
        let summary: Vec<(Colour, u32, usize)> = art.meshes.iter().map(|m| (m.colour, m.layer, m.triangles.len())).collect();
        assert_eq!(
            summary,
            vec![(Colour::Green, 0, 2), (Colour::Black, 0, 2), (Colour::White, 1, 4), (Colour::Black, 2, 2)]
        );
    }

    #[test]
    fn every_triangle_is_anticlockwise() {
        // A clockwise outline still gives anticlockwise triangles.
        let mut outline = rect(0.0, 0.0, 10.0, 5.0);
        outline.reverse();
        let art = tessellate_artwork(&drawing(vec![Primitive::Fill { outline, colour: Colour::Red }]));
        let m = &art.meshes[0];
        for t in &m.triangles {
            assert!(signed_area(m.vertices[t[0] as usize], m.vertices[t[1] as usize], m.vertices[t[2] as usize]) > 0.0);
        }
        assert!((m.area() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn degenerate_primitives_and_symbols_are_counted_not_exported() {
        let art = tessellate_artwork(&drawing(vec![
            Primitive::Line { from: Pt2::new(1.0, 1.0), to: Pt2::new(1.0, 1.0), width: 5.0, colour: None },
            Primitive::Fill { outline: vec![Pt2::new(0.0, 0.0), Pt2::new(1.0, 1.0)], colour: Colour::Red },
            Primitive::Symbol { origin: Pt2::new(0.0, 0.0), id: "airport".into(), size: 10.0, rotation_deg: 0.0 },
        ]));
        assert!(art.meshes.is_empty());
        assert_eq!((art.skipped_symbols, art.skipped_degenerate), (1, 2));
    }
}
