// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Neutral 2D drawing primitives.
//!
//! A module (signs today; markings, alignments and others later) turns an element into a
//! [`Drawing`]: a flat list of [`Primitive`]s with Y up, in whatever unit the module works in.
//! Front ends (desktop canvas, DXF, PDF, web) consume the same list, so a sign is drawn, exported
//! and fabricated from one description.

use serde::{Deserialize, Serialize};

/// Named sign colours. The standards name colours, not RGB values; each front end maps them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Colour {
    White,
    Black,
    Green,
    Blue,
    Brown,
    Yellow,
    Red,
    Grey,
}

impl Colour {
    /// Display sRGB. An approximation for screens: the standards specify colours by name and
    /// chromaticity, so this must not be read as a specification value.
    pub fn display_rgb(self) -> [u8; 3] {
        match self {
            Colour::White => [0xFF, 0xFF, 0xFF],
            Colour::Black => [0x10, 0x10, 0x10],
            Colour::Green => [0x00, 0x5C, 0x2E],
            Colour::Blue => [0x00, 0x4F, 0x9E],
            Colour::Brown => [0x6B, 0x3A, 0x1E],
            Colour::Yellow => [0xFF, 0xD2, 0x00],
            Colour::Red => [0xC8, 0x10, 0x2E],
            Colour::Grey => [0x80, 0x84, 0x88],
        }
    }
}

/// A 2D point, Y up.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Pt2 {
    pub x: f64,
    pub y: f64,
}

impl Pt2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl std::ops::Add for Pt2 {
    type Output = Pt2;
    fn add(self, o: Pt2) -> Pt2 {
        Pt2::new(self.x + o.x, self.y + o.y)
    }
}

impl std::ops::Sub for Pt2 {
    type Output = Pt2;
    fn sub(self, o: Pt2) -> Pt2 {
        Pt2::new(self.x - o.x, self.y - o.y)
    }
}

impl Pt2 {
    pub fn distance_to(self, o: Pt2) -> f64 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
}

/// One drawable item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Primitive {
    /// A closed, filled outline (may be concave).
    Fill { outline: Vec<Pt2>, colour: Colour },
    /// A straight band of the given width with square ends. `colour` None means black.
    Line { from: Pt2, to: Pt2, width: f64, colour: Option<Colour> },
    /// A circular arc centreline, angles in degrees anticlockwise from +X.
    Arc { centre: Pt2, radius: f64, start_deg: f64, end_deg: f64, colour: Option<Colour> },
    /// Lettering as closed glyph contours, filled even-odd so counters stay open.
    /// `ink` None means white.
    Glyphs { contours: Vec<Vec<Pt2>>, text: String, ink: Option<Colour> },
    /// A placeholder for artwork that is not resolved to geometry yet.
    Symbol { origin: Pt2, id: String, size: f64, rotation_deg: f64 },
}

/// A coloured triangle mesh in three dimensions (millimetres, Z up): the solid form of an
/// element, for the 3D view, IFC export and quantities.
///
/// The frame is the element's own: +X along its width from the lower-left corner, +Y into its
/// depth (a sign face lies in the XZ plane and looks towards −Y), +Z up from its base.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Mesh3 {
    /// What the solid is, e.g. "Post" or "Foundation".
    pub name: String,
    pub vertices: Vec<[f64; 3]>,
    /// Counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
    /// Display sRGB.
    pub colour: [u8; 3],
}

impl Mesh3 {
    /// An axis-aligned box from `min` to `max`.
    pub fn cuboid(name: &str, min: [f64; 3], max: [f64; 3], colour: [u8; 3]) -> Mesh3 {
        let (a, b) = (min, max);
        let vertices = vec![
            [a[0], a[1], a[2]], [b[0], a[1], a[2]], [b[0], b[1], a[2]], [a[0], b[1], a[2]],
            [a[0], a[1], b[2]], [b[0], a[1], b[2]], [b[0], b[1], b[2]], [a[0], b[1], b[2]],
        ];
        let triangles = vec![
            [0, 2, 1], [0, 3, 2], // bottom
            [4, 5, 6], [4, 6, 7], // top
            [0, 1, 5], [0, 5, 4], // front (−Y)
            [1, 2, 6], [1, 6, 5], // right
            [2, 3, 7], [2, 7, 6], // back
            [3, 0, 4], [3, 4, 7], // left
        ];
        Mesh3 { name: name.to_string(), vertices, triangles, colour }
    }

    /// A vertical cylinder (a prism of `segments` sides) on the Z axis through (`cx`, `cy`).
    pub fn cylinder(name: &str, cx: f64, cy: f64, z0: f64, z1: f64, radius: f64, segments: u32, colour: [u8; 3]) -> Mesh3 {
        let n = segments.max(3);
        let mut vertices = Vec::with_capacity(2 * n as usize + 2);
        for z in [z0, z1] {
            for i in 0..n {
                let a = i as f64 / n as f64 * std::f64::consts::TAU;
                vertices.push([cx + radius * a.cos(), cy + radius * a.sin(), z]);
            }
        }
        let (bottom, top) = (2 * n, 2 * n + 1);
        vertices.push([cx, cy, z0]);
        vertices.push([cx, cy, z1]);
        let mut triangles = Vec::with_capacity(4 * n as usize);
        for i in 0..n {
            let j = (i + 1) % n;
            triangles.push([i, j, n + j]);
            triangles.push([i, n + j, n + i]);
            triangles.push([bottom, j, i]);
            triangles.push([top, n + i, n + j]);
        }
        Mesh3 { name: name.to_string(), vertices, triangles, colour }
    }

    /// Enclosed volume in mm³ (signed-tetrahedron sum; exact for a closed, outward-wound mesh).
    pub fn volume(&self) -> f64 {
        self.triangles
            .iter()
            .map(|t| {
                let [a, b, c] = [self.vertices[t[0] as usize], self.vertices[t[1] as usize], self.vertices[t[2] as usize]];
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
            })
            .sum()
    }
}

/// Axis-aligned bounds.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Extents {
    pub min: Pt2,
    pub max: Pt2,
}

impl Extents {
    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }
    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }
}

/// A finished drawing: primitives plus their overall bounds.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Drawing {
    pub primitives: Vec<Primitive>,
    pub extents: Extents,
}

impl Drawing {
    /// The same drawing with every coordinate and width multiplied by `k`, moved so that its
    /// lower-left extent sits on the origin. Used to bring a sign from stroke widths to millimetres.
    pub fn scaled_to_origin(&self, k: f64) -> Drawing {
        let (ox, oy) = (self.extents.min.x, self.extents.min.y);
        let p = |q: &Pt2| Pt2::new((q.x - ox) * k, (q.y - oy) * k);
        let primitives = self
            .primitives
            .iter()
            .map(|prim| match prim {
                Primitive::Fill { outline, colour } => {
                    Primitive::Fill { outline: outline.iter().map(p).collect(), colour: *colour }
                }
                Primitive::Line { from, to, width, colour } => {
                    Primitive::Line { from: p(from), to: p(to), width: width * k, colour: *colour }
                }
                Primitive::Arc { centre, radius, start_deg, end_deg, colour } => Primitive::Arc {
                    centre: p(centre),
                    radius: radius * k,
                    start_deg: *start_deg,
                    end_deg: *end_deg,
                    colour: *colour,
                },
                Primitive::Glyphs { contours, text, ink } => Primitive::Glyphs {
                    contours: contours.iter().map(|c| c.iter().map(p).collect()).collect(),
                    text: text.clone(),
                    ink: *ink,
                },
                Primitive::Symbol { origin, id, size, rotation_deg } => Primitive::Symbol {
                    origin: p(origin),
                    id: id.clone(),
                    size: size * k,
                    rotation_deg: *rotation_deg,
                },
            })
            .collect();
        Drawing {
            primitives,
            extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(self.extents.width() * k, self.extents.height() * k) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solids_have_the_right_volume_and_face_outwards() {
        let b = Mesh3::cuboid("b", [1.0, 2.0, 3.0], [4.0, 6.0, 8.0], [0, 0, 0]);
        assert!((b.volume() - 3.0 * 4.0 * 5.0).abs() < 1e-9);
        // A 64-sided prism is within 0.2 % of the cylinder it stands for, and never larger.
        let c = Mesh3::cylinder("c", 10.0, -5.0, 0.0, 1000.0, 50.0, 64, [0, 0, 0]);
        let exact = std::f64::consts::PI * 50.0 * 50.0 * 1000.0;
        assert!(c.volume() > 0.0 && c.volume() < exact && c.volume() > exact * 0.998);
    }

    #[test]
    fn scaling_moves_the_lower_left_to_the_origin() {
        let d = Drawing {
            primitives: vec![Primitive::Line { from: Pt2::new(2.0, 3.0), to: Pt2::new(6.0, 3.0), width: 1.0, colour: None }],
            extents: Extents { min: Pt2::new(2.0, 1.0), max: Pt2::new(10.0, 5.0) },
        };
        let s = d.scaled_to_origin(25.0);
        assert_eq!(s.extents.max, Pt2::new(200.0, 100.0));
        match &s.primitives[0] {
            Primitive::Line { from, to, width, .. } => {
                assert_eq!(*from, Pt2::new(0.0, 50.0));
                assert_eq!(*to, Pt2::new(100.0, 50.0));
                assert_eq!(*width, 25.0);
            }
            _ => unreachable!(),
        }
    }
}
