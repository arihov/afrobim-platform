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
    ///
    /// Each vertex is placed from the difference between the element origin and the view anchor
    /// plus its local offset (`ViewTransform::to_screen_local`), in f64, so a mesh at 1e10 mm is
    /// as sharp as one at the origin. egui takes f32 positions, which resolve 0.03 px up to about
    /// 4e5 px; when the view is zoomed in so far that part of an element lies beyond that, the
    /// triangles and lines are clipped to a box of [`CLIP_PX`] around the screen in f64 before
    /// they are converted, so what is on screen keeps its exact shape.
    pub fn shapes_w(&self, view: &ViewTransform, origin: Pt, rep: Representation, ink: Color32, opacity: f32, width: f32) -> Vec<Shape> {
        let place = |p: &[f32; 2]| view.to_screen_local(origin, p[0] as f64, p[1] as f64);
        let placed: Vec<Pt> = self.vertices.iter().map(|(p, _)| place(p)).collect();
        let far = |q: &Pt| !(q.x.abs() <= CLIP_PX && q.y.abs() <= CLIP_PX);
        let needs_clip = placed.iter().any(far) || self.outlines.iter().any(|(pts, _)| pts.iter().any(|p| far(&place(p))));
        let pos = |q: Pt| Pos2::new(q.x as f32, q.y as f32);
        let mut out = Vec::new();
        if rep == Representation::Real {
            let mut mesh = Mesh::default();
            if !needs_clip {
                mesh.vertices = self
                    .vertices
                    .iter()
                    .zip(&placed)
                    .map(|((_, c), q)| Vertex { pos: pos(*q), uv: egui::epaint::WHITE_UV, color: c.gamma_multiply(opacity) })
                    .collect();
                mesh.indices = self.indices.clone();
            } else {
                for t in self.indices.chunks_exact(3) {
                    let tri = [placed[t[0] as usize], placed[t[1] as usize], placed[t[2] as usize]];
                    let colour = self.vertices[t[0] as usize].1.gamma_multiply(opacity);
                    let poly = clip_polygon(&tri, CLIP_PX);
                    if poly.len() < 3 {
                        continue;
                    }
                    let base = mesh.vertices.len() as u32;
                    for q in &poly {
                        mesh.vertices.push(Vertex { pos: pos(*q), uv: egui::epaint::WHITE_UV, color: colour });
                    }
                    for k in 1..poly.len() as u32 - 1 {
                        mesh.indices.extend([base, base + k, base + k + 1]);
                    }
                }
            }
            out.push(Shape::mesh(mesh));
        }
        let stroke = Stroke::new(width, ink.gamma_multiply(opacity));
        for (pts, closed) in &self.outlines {
            // In Real, only open centrelines (arcs) need a stroke; fills already show the closed ones.
            if rep == Representation::Real && *closed {
                continue;
            }
            let q: Vec<Pt> = pts.iter().map(place).collect();
            if !q.iter().any(far) {
                let pts: Vec<Pos2> = q.iter().map(|p| pos(*p)).collect();
                out.push(if *closed { Shape::closed_line(pts, stroke) } else { Shape::line(pts, stroke) });
                continue;
            }
            let n = q.len();
            let segs = if *closed { n } else { n.saturating_sub(1) };
            for i in 0..segs {
                if let Some((a, b)) = clip_segment(q[i], q[(i + 1) % n], CLIP_PX) {
                    out.push(Shape::line_segment([pos(a), pos(b)], stroke));
                }
            }
        }
        out
    }
}

/// Half-size (px) of the box around the screen to which geometry is clipped, in f64, before it is
/// handed to egui in f32. The view keeps the screen position of its anchor within 1e5 px of the
/// screen's corner, so the box always contains the screen.
pub const CLIP_PX: f64 = 4.0e5;

/// Sutherland-Hodgman clip of a polygon to the square [-`lim`, `lim`]^2.
pub fn clip_polygon(poly: &[Pt], lim: f64) -> Vec<Pt> {
    let mut cur: Vec<Pt> = poly.to_vec();
    // (axis 0 = x, 1 = y; sign +1 keeps v <= lim, -1 keeps v >= -lim)
    for (axis, sign) in [(0, 1.0), (0, -1.0), (1, 1.0), (1, -1.0)] {
        if cur.is_empty() {
            break;
        }
        let val = |p: &Pt| if axis == 0 { p.x } else { p.y };
        let inside = |p: &Pt| sign * val(p) <= lim;
        let mut next = Vec::with_capacity(cur.len() + 2);
        for i in 0..cur.len() {
            let (a, b) = (cur[i], cur[(i + 1) % cur.len()]);
            let (ia, ib) = (inside(&a), inside(&b));
            if ia {
                next.push(a);
            }
            if ia != ib {
                let target = sign * lim;
                let t = (target - val(&a)) / (val(&b) - val(&a));
                let mut m = Pt::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                if axis == 0 {
                    m.x = target;
                } else {
                    m.y = target;
                }
                next.push(m);
            }
        }
        cur = next;
    }
    cur
}

/// Liang-Barsky clip of the segment `a`-`b` to the square [-`lim`, `lim`]^2.
pub fn clip_segment(a: Pt, b: Pt, lim: f64) -> Option<(Pt, Pt)> {
    clip_segment_box(a, b, Pt::new(-lim, -lim), Pt::new(lim, lim))
}

/// Liang-Barsky clip of the segment `a`-`b` to the box `lo`..`hi`.
pub fn clip_segment_box(a: Pt, b: Pt, lo: Pt, hi: Pt) -> Option<(Pt, Pt)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [(-dx, a.x - lo.x), (dx, hi.x - a.x), (-dy, a.y - lo.y), (dy, hi.y - a.y)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return None;
                }
                t0 = t0.max(r);
            } else {
                if r < t0 {
                    return None;
                }
                t1 = t1.min(r);
            }
        }
    }
    Some((Pt::new(a.x + dx * t0, a.y + dy * t0), Pt::new(a.x + dx * t1, a.y + dy * t1)))
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

#[cfg(test)]
mod clip_tests {
    use super::*;

    #[test]
    fn a_triangle_far_off_screen_is_clipped_to_the_box() {
        let tri = [Pt::new(0.0, 0.0), Pt::new(1.0e12, 0.0), Pt::new(0.0, 1.0e12)];
        let poly = clip_polygon(&tri, 10.0);
        assert!(poly.len() >= 3);
        assert!(poly.iter().all(|p| p.x.abs() <= 10.0 && p.y.abs() <= 10.0));
        // The box corner (10, 10) is inside the triangle's half plane x + y <= 1e12, so it stays.
        assert!(poly.iter().any(|p| (p.x - 10.0).abs() < 1e-9 && (p.y - 10.0).abs() < 1e-9));
    }

    #[test]
    fn a_line_through_the_box_keeps_its_direction() {
        let (a, b) = clip_segment(Pt::new(-1.0e12, -1.0e12 + 5.0), Pt::new(1.0e12, 1.0e12 + 5.0), 100.0).unwrap();
        assert!((b.y - a.y - (b.x - a.x)).abs() < 1e-3);
        assert!(clip_segment(Pt::new(-1.0e12, 500.0), Pt::new(1.0e12, 500.0), 100.0).is_none());
    }
}
