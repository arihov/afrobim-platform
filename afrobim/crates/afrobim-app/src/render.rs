//! Turns a module's [`Drawing`] into GPU triangles once, then places them every frame.

use afrobim_draw::{Colour, Drawing, Primitive, Pt2};
use afrobim_kernel::{Pt, ViewTransform};
use egui::{epaint::Vertex, Color32, Mesh, Pos2, Shape, Stroke};
use lyon_tessellation::{
    math::point, path::Path, BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers,
};

/// How elements are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Representation {
    /// Full colour, as fabricated.
    Real,
    /// Outlines only, in the canvas foreground colour.
    Outlined,
}

fn rgb(c: Colour) -> Color32 {
    let [r, g, b] = c.display_rgb();
    Color32::from_rgb(r, g, b)
}

/// A drawing tessellated in element-local millimetres (Y up).
#[derive(Default, Clone)]
pub struct ElementMesh {
    vertices: Vec<([f32; 2], Color32)>,
    indices: Vec<u32>,
    /// Closed outlines for the Outlined representation and open centrelines (arcs).
    outlines: Vec<(Vec<[f32; 2]>, bool)>,
}

impl ElementMesh {
    pub fn build(drawing: &Drawing) -> ElementMesh {
        let mut m = ElementMesh::default();
        let mut tess = FillTessellator::new();
        for prim in &drawing.primitives {
            match prim {
                Primitive::Fill { outline, colour } => {
                    m.fill(&mut tess, std::slice::from_ref(outline), rgb(*colour), FillRule::NonZero);
                    m.outlines.push((outline.iter().map(f).collect(), true));
                }
                Primitive::Glyphs { contours, ink, .. } => {
                    m.fill(&mut tess, contours, ink.map(rgb).unwrap_or(Color32::WHITE), FillRule::EvenOdd);
                    for c in contours {
                        m.outlines.push((c.iter().map(f).collect(), true));
                    }
                }
                Primitive::Line { from, to, width, colour } => {
                    // A band with square ends: exactly the rectangle that is cut.
                    let (dx, dy) = (to.x - from.x, to.y - from.y);
                    let len = (dx * dx + dy * dy).sqrt();
                    if len <= 0.0 {
                        continue;
                    }
                    // A hairline (annotation, underlays) has no width to fill: it is a stroke.
                    if *width <= 0.0 {
                        m.outlines.push((vec![f(from), f(to)], false));
                        continue;
                    }
                    let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
                    let quad = vec![
                        Pt2::new(from.x + nx, from.y + ny),
                        Pt2::new(to.x + nx, to.y + ny),
                        Pt2::new(to.x - nx, to.y - ny),
                        Pt2::new(from.x - nx, from.y - ny),
                    ];
                    m.fill(&mut tess, std::slice::from_ref(&quad), colour.map(rgb).unwrap_or(Color32::from_rgb(0x10, 0x10, 0x10)), FillRule::NonZero);
                    m.outlines.push((quad.iter().map(f).collect(), true));
                }
                Primitive::Arc { centre, radius, start_deg, end_deg, .. } => {
                    let segs = (((end_deg - start_deg).abs() / 6.0) as usize).max(8);
                    let pts = (0..=segs)
                        .map(|i| {
                            let a = (start_deg + (end_deg - start_deg) * i as f64 / segs as f64).to_radians();
                            [(centre.x + radius * a.cos()) as f32, (centre.y + radius * a.sin()) as f32]
                        })
                        .collect();
                    m.outlines.push((pts, false));
                }
                Primitive::Symbol { origin, size, .. } => {
                    let (x, y, s) = (origin.x as f32, origin.y as f32, *size as f32);
                    m.outlines.push((vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s]], true));
                }
            }
        }
        m
    }

    fn fill(&mut self, tess: &mut FillTessellator, contours: &[Vec<Pt2>], colour: Color32, rule: FillRule) {
        let mut b = Path::builder();
        for c in contours.iter().filter(|c| c.len() >= 3) {
            b.begin(point(c[0].x as f32, c[0].y as f32));
            for p in &c[1..] {
                b.line_to(point(p.x as f32, p.y as f32));
            }
            b.end(true);
        }
        let path = b.build();
        let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
        let options = FillOptions::default().with_fill_rule(rule).with_tolerance(0.05);
        if tess
            .tessellate_path(&path, &options, &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position().to_array()))
            .is_err()
        {
            return; // a degenerate outline: skip it and keep the rest of the element
        }
        let base = self.vertices.len() as u32;
        self.vertices.extend(buffers.vertices.into_iter().map(|p| (p, colour)));
        self.indices.extend(buffers.indices.into_iter().map(|i| base + i));
    }

    /// Shapes for this mesh with its lower-left corner at `origin` (model mm).
    pub fn shapes(&self, view: &ViewTransform, origin: Pt, rep: Representation, ink: Color32, opacity: f32) -> Vec<Shape> {
        self.shapes_w(view, origin, rep, ink, opacity, 1.0)
    }

    /// As [`shapes`](Self::shapes) with the stroke `width` in pixels (a layer's line weight).
    pub fn shapes_w(&self, view: &ViewTransform, origin: Pt, rep: Representation, ink: Color32, opacity: f32, width: f32) -> Vec<Shape> {
        let place = |p: &[f32; 2]| {
            let s = view.to_screen(Pt::new(origin.x + p[0] as f64, origin.y + p[1] as f64));
            Pos2::new(s.x as f32, s.y as f32)
        };
        let mut out = Vec::new();
        if rep == Representation::Real {
            let mut mesh = Mesh::default();
            mesh.vertices = self
                .vertices
                .iter()
                .map(|(p, c)| Vertex { pos: place(p), uv: egui::epaint::WHITE_UV, color: c.gamma_multiply(opacity) })
                .collect();
            mesh.indices = self.indices.clone();
            out.push(Shape::mesh(mesh));
        }
        let stroke = Stroke::new(width, ink.gamma_multiply(opacity));
        for (pts, closed) in &self.outlines {
            // In Real, only open centrelines (arcs) need a stroke; fills already show the closed ones.
            if rep == Representation::Real && *closed {
                continue;
            }
            let pts: Vec<Pos2> = pts.iter().map(place).collect();
            out.push(if *closed { Shape::closed_line(pts, stroke) } else { Shape::line(pts, stroke) });
        }
        out
    }
}

impl ElementMesh {
    /// The filled artwork with each local vertex (mm, from the element's lower-left corner)
    /// placed by `place`, for the 3D view. None when any vertex cannot be shown (behind the
    /// camera), so a face is never drawn half-projected.
    pub fn projected(&self, place: &dyn Fn(f64, f64) -> Option<Pos2>) -> Option<Shape> {
        let mut mesh = Mesh::default();
        mesh.vertices.reserve(self.vertices.len());
        for (p, c) in &self.vertices {
            mesh.vertices.push(Vertex { pos: place(p[0] as f64, p[1] as f64)?, uv: egui::epaint::WHITE_UV, color: *c });
        }
        mesh.indices = self.indices.clone();
        Some(Shape::mesh(mesh))
    }
}

fn f(p: &Pt2) -> [f32; 2] {
    [p.x as f32, p.y as f32]
}
