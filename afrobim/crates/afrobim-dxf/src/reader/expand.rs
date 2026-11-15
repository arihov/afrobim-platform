//! Expanding an underlay for display: blocks inserted, curves flattened, colours resolved.
//!
//! INSERT places a block with `T(insert) * R(rotation) * S(sx, sy) * T(-base)`. A MINSERT array
//! (`70` columns, `71` rows) offsets each copy by `(col * dx, row * dy)` turned by the rotation and
//! not scaled, as `ezdxf` `Insert.multi_insert` does. Entities inside a block on layer `0` take the
//! layer of the INSERT, and a colour *by block* takes the INSERT's colour (AutoCAD's rule).
//! Nesting is followed to `max_depth`.

use std::collections::BTreeSet;

use afrobim_draw::Pt2;

use super::aci::ACI_RGB;
use super::geom::{arc_points, bulge_arc, ellipse_points, pt, sweep, Nurbs};
use super::model::*;
use super::Underlay;

/// A 2D affine map: `x' = a x + c y + tx`, `y' = b x + d y + ty`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };

    pub fn apply(&self, p: Pt2) -> Pt2 {
        pt(self.a * p.x + self.c * p.y + self.tx, self.b * p.x + self.d * p.y + self.ty)
    }

    /// `self` after `o`: first `o`, then `self`.
    pub fn after(&self, o: &Affine) -> Affine {
        Affine {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            tx: self.a * o.tx + self.c * o.ty + self.tx,
            ty: self.b * o.tx + self.d * o.ty + self.ty,
        }
    }

    pub fn translate(tx: f64, ty: f64) -> Affine {
        Affine { tx, ty, ..Affine::IDENTITY }
    }

    pub fn rotate_scale(rot_deg: f64, sx: f64, sy: f64) -> Affine {
        let (s, c) = rot_deg.to_radians().sin_cos();
        Affine { a: c * sx, b: s * sx, c: -s * sy, d: c * sy, tx: 0.0, ty: 0.0 }
    }

    pub fn det(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// Length of the image of a unit X step, and of a unit Y step.
    pub fn scales(&self) -> (f64, f64) {
        (self.a.hypot(self.b), self.c.hypot(self.d))
    }

    /// A rotation and uniform scale with no mirroring: arcs stay arcs.
    pub fn is_similarity(&self) -> bool {
        let (sx, sy) = self.scales();
        (sx - sy).abs() <= 1e-9 * sx.max(sy).max(1e-300) && self.det() > 0.0 && (self.a - self.d).abs() <= 1e-9 * sx.max(1e-300) && (self.b + self.c).abs() <= 1e-9 * sx.max(1e-300)
    }

    pub fn rotation_deg(&self) -> f64 {
        self.b.atan2(self.a).to_degrees()
    }
}

/// What an entity looks like after expansion.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Line(Pt2, Pt2),
    Arc { c: Pt2, r: f64, a0: f64, a1: f64 },
    Circle { c: Pt2, r: f64 },
    Poly { pts: Vec<Pt2>, closed: bool },
    /// A filled outline (SOLID, 3DFACE).
    Solid(Vec<Pt2>),
    Point(Pt2),
    Text(TextGeom),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resolved {
    pub rgb: [u8; 3],
    pub aci: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub layer: u32,
    pub colour: Resolved,
    /// The DXF type name of the entity it came from (for tests and the report).
    pub kind: &'static str,
    pub shape: Shape,
}

#[derive(Debug, Clone)]
pub struct ExpandOptions {
    /// Chord tolerance for curves, in file units.
    pub tol: f64,
    pub max_depth: usize,
    /// Layers (by index) not to draw, in addition to the file's frozen and off layers.
    pub hidden: BTreeSet<u32>,
    /// Follow the file's frozen and off flags.
    pub honour_file_layers: bool,
    /// Stop after this many items.
    pub max_items: usize,
    /// Collect snap candidates too.
    pub snaps: bool,
}

impl Default for ExpandOptions {
    fn default() -> Self {
        ExpandOptions { tol: 0.1, max_depth: 16, hidden: BTreeSet::new(), honour_file_layers: true, max_items: 5_000_000, snaps: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SnapKind {
    End,
    Mid,
    Centre,
    Quadrant,
    /// A point entity or text or block insertion point.
    Node,
    Intersection,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snap {
    pub p: Pt2,
    pub kind: SnapKind,
}

#[derive(Debug, Default)]
pub struct Expanded {
    pub items: Vec<Item>,
    pub snaps: Vec<Snap>,
    pub warnings: Vec<String>,
    pub truncated: bool,
}

struct Ctx {
    layer: Option<u32>,
    colour: Option<Resolved>,
}

struct Walker<'a> {
    u: &'a Underlay,
    o: &'a ExpandOptions,
    out: Expanded,
    warned: BTreeSet<String>,
}

pub fn aci_rgb(aci: u8) -> [u8; 3] {
    ACI_RGB[aci as usize]
}

impl Underlay {
    /// The layer colour as resolved.
    pub fn layer_colour(&self, layer: u32) -> Resolved {
        let l = &self.layers[layer as usize];
        match (l.rgb, l.aci) {
            (Some(rgb), _) => Resolved { rgb, aci: None },
            (None, Some(a)) => Resolved { rgb: aci_rgb(a), aci: Some(a) },
            _ => Resolved { rgb: aci_rgb(7), aci: Some(7) },
        }
    }

    /// Model space with blocks expanded, curves flattened and colours resolved. Coordinates are
    /// in file units.
    pub fn expand(&self, o: &ExpandOptions) -> Expanded {
        let mut w = Walker { u: self, o, out: Expanded::default(), warned: BTreeSet::new() };
        w.walk(&self.entities, &Affine::IDENTITY, &Ctx { layer: None, colour: None }, 0, Pt2::new(0.0, 0.0));
        w.out
    }

    pub(crate) fn compute_bounds(&self) -> Option<(Pt2, Pt2)> {
        let ex = self.expand(&ExpandOptions { tol: 1.0e9, ..ExpandOptions::default() });
        bounds_of(&ex.items)
    }
}

/// The bounds of the geometry of `items` (text anchors included, text extents not).
pub fn bounds_of(items: &[Item]) -> Option<(Pt2, Pt2)> {
    let mut lo = pt(f64::INFINITY, f64::INFINITY);
    let mut hi = pt(f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut any = false;
    let mut add = |p: Pt2| {
        if p.x.is_finite() && p.y.is_finite() {
            any = true;
            lo = pt(lo.x.min(p.x), lo.y.min(p.y));
            hi = pt(hi.x.max(p.x), hi.y.max(p.y));
        }
    };
    for it in items {
        match &it.shape {
            Shape::Line(a, b) => {
                add(*a);
                add(*b);
            }
            Shape::Arc { c, r, a0, a1 } => {
                // The end points and any axis crossing in the sweep.
                for a in [*a0, *a1] {
                    add(pt(c.x + r * a.to_radians().cos(), c.y + r * a.to_radians().sin()));
                }
                for q in [0.0, 90.0, 180.0, 270.0] {
                    if sweep(*a0, q) <= sweep(*a0, *a1) {
                        add(pt(c.x + r * q.to_radians().cos(), c.y + r * q.to_radians().sin()));
                    }
                }
            }
            Shape::Circle { c, r } => {
                add(pt(c.x - r, c.y - r));
                add(pt(c.x + r, c.y + r));
            }
            Shape::Poly { pts, .. } | Shape::Solid(pts) => pts.iter().for_each(|p| add(*p)),
            Shape::Point(p) => add(*p),
            Shape::Text(t) => add(t.at),
        }
    }
    any.then_some((lo, hi))
}

impl Walker<'_> {
    fn warn(&mut self, m: String) {
        if self.warned.insert(m.clone()) && self.out.warnings.len() < 100 {
            self.out.warnings.push(m);
        }
    }

    fn full(&self) -> bool {
        self.out.items.len() >= self.o.max_items
    }

    fn layer_hidden(&self, layer: u32) -> bool {
        self.o.hidden.contains(&layer) || (self.o.honour_file_layers && !self.u.layers[layer as usize].visible())
    }

    fn snap(&mut self, p: Pt2, kind: SnapKind) {
        if self.o.snaps {
            self.out.snaps.push(Snap { p, kind });
        }
    }

    fn walk(&mut self, ents: &[Entity], m: &Affine, ctx: &Ctx, depth: usize, base: Pt2) {
        let _ = base;
        for e in ents {
            if self.full() {
                self.out.truncated = true;
                self.warn(format!("Stopped after {} items: the drawing is larger than the display limit.", self.o.max_items));
                return;
            }
            let layer = if e.layer == 0 { ctx.layer.unwrap_or(0) } else { e.layer };
            if self.layer_hidden(layer) {
                continue;
            }
            let colour = match e.colour {
                ColourSpec::Rgb(rgb) => Resolved { rgb, aci: None },
                ColourSpec::Aci(a) => Resolved { rgb: aci_rgb(a), aci: Some(a) },
                ColourSpec::ByBlock => ctx.colour.unwrap_or_else(|| self.u.layer_colour(layer)),
                ColourSpec::ByLayer => self.u.layer_colour(layer),
            };
            self.entity(e, layer, colour, m, depth);
        }
    }

    fn push(&mut self, layer: u32, colour: Resolved, kind: &'static str, shape: Shape) {
        self.out.items.push(Item { layer, colour, kind, shape });
    }

    /// Tolerance in the entity's own units for a map `m`.
    fn local_tol(&self, m: &Affine) -> f64 {
        let (sx, sy) = m.scales();
        self.o.tol / sx.max(sy).max(1e-12)
    }

    fn entity(&mut self, e: &Entity, layer: u32, colour: Resolved, m: &Affine, depth: usize) {
        let tol = self.local_tol(m);
        match &e.geom {
            Geom::Line(a, b) => {
                let (a, b) = (m.apply(*a), m.apply(*b));
                self.snap(a, SnapKind::End);
                self.snap(b, SnapKind::End);
                self.snap(pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0), SnapKind::Mid);
                self.push(layer, colour, "LINE", Shape::Line(a, b));
            }
            Geom::Poly { pts, closed } => {
                let kind = "POLYLINE";
                let mut out: Vec<Pt2> = Vec::with_capacity(pts.len());
                let n = pts.len();
                let segs = if *closed { n } else { n - 1 };
                for i in 0..segs {
                    let (a, b) = (pts[i].p, pts[(i + 1) % n].p);
                    out.push(m.apply(a));
                    self.snap(m.apply(a), SnapKind::End);
                    match bulge_arc(a, b, pts[i].bulge) {
                        None => self.snap(m.apply(pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)), SnapKind::Mid),
                        Some(arc) => {
                            let sw = if arc.forward { sweep(arc.start_deg, arc.end_deg) } else { -sweep(arc.start_deg, arc.end_deg) };
                            let start = if arc.forward { arc.start_deg } else { arc.end_deg };
                            let pts = arc_points(arc.centre, arc.radius, start, sw, tol);
                            // Interior points only; the vertices themselves are pushed by the loop.
                            for q in &pts[1..pts.len() - 1] {
                                out.push(m.apply(*q));
                            }
                            let mid = arc_points(arc.centre, arc.radius, start, sw / 2.0, tol).last().copied().unwrap_or(a);
                            self.snap(m.apply(mid), SnapKind::Mid);
                            self.snap(m.apply(arc.centre), SnapKind::Centre);
                        }
                    }
                }
                if !*closed {
                    out.push(m.apply(pts[n - 1].p));
                    self.snap(m.apply(pts[n - 1].p), SnapKind::End);
                }
                self.push(layer, colour, kind, Shape::Poly { pts: out, closed: *closed });
            }
            Geom::Arc { c, r, a0, a1 } => {
                let sw = sweep(*a0, *a1);
                let sw = if sw == 0.0 { 360.0 } else { sw };
                if m.is_similarity() {
                    let (s, rot) = (m.scales().0, m.rotation_deg());
                    let (cc, rr) = (m.apply(*c), r * s);
                    let (b0, b1) = (a0 + rot, a1 + rot);
                    self.snap(cc, SnapKind::Centre);
                    for a in [b0, b1] {
                        self.snap(pt(cc.x + rr * a.to_radians().cos(), cc.y + rr * a.to_radians().sin()), SnapKind::End);
                    }
                    let am = (b0 + sw / 2.0).to_radians();
                    self.snap(pt(cc.x + rr * am.cos(), cc.y + rr * am.sin()), SnapKind::Mid);
                    self.push(layer, colour, "ARC", Shape::Arc { c: cc, r: rr, a0: b0, a1: b0 + sw });
                } else {
                    let pts: Vec<Pt2> = arc_points(*c, *r, *a0, sw, tol).into_iter().map(|p| m.apply(p)).collect();
                    self.snap(pts[0], SnapKind::End);
                    self.snap(pts[pts.len() - 1], SnapKind::End);
                    self.push(layer, colour, "ARC", Shape::Poly { pts, closed: false });
                }
            }
            Geom::Circle { c, r } => {
                let cc = m.apply(*c);
                self.snap(cc, SnapKind::Centre);
                let (sx, sy) = m.scales();
                if (sx - sy).abs() <= 1e-9 * sx.max(1e-300) && (m.a * m.c + m.b * m.d).abs() <= 1e-9 * sx * sy {
                    let rr = r * sx;
                    for a in [0.0f64, 90.0, 180.0, 270.0] {
                        self.snap(pt(cc.x + rr * a.to_radians().cos(), cc.y + rr * a.to_radians().sin()), SnapKind::Quadrant);
                    }
                    self.push(layer, colour, "CIRCLE", Shape::Circle { c: cc, r: rr });
                } else {
                    let pts: Vec<Pt2> = arc_points(*c, *r, 0.0, 360.0, tol).into_iter().map(|p| m.apply(p)).collect();
                    self.push(layer, colour, "CIRCLE", Shape::Poly { pts, closed: true });
                }
            }
            Geom::Ellipse { c, major, minor, t0, t1 } => {
                let pts: Vec<Pt2> = ellipse_points(*c, *major, *minor, *t0, *t1, tol).into_iter().map(|p| m.apply(p)).collect();
                self.snap(m.apply(*c), SnapKind::Centre);
                let full = (t1 - t0 - std::f64::consts::TAU).abs() < 1e-9;
                if !full {
                    self.snap(pts[0], SnapKind::End);
                    self.snap(pts[pts.len() - 1], SnapKind::End);
                }
                self.push(layer, colour, "ELLIPSE", Shape::Poly { pts, closed: full });
            }
            Geom::Spline { degree, knots, ctrl, weights, fit, closed } => {
                let (pts, note) = spline_points(*degree, knots, ctrl, weights, fit, tol);
                if let Some(n) = note {
                    self.warn(n);
                }
                if pts.len() >= 2 {
                    let pts: Vec<Pt2> = pts.into_iter().map(|p| m.apply(p)).collect();
                    self.snap(pts[0], SnapKind::End);
                    self.snap(pts[pts.len() - 1], SnapKind::End);
                    self.push(layer, colour, "SPLINE", Shape::Poly { pts, closed: *closed });
                }
            }
            Geom::Text(t) => {
                let (sx, sy) = m.scales();
                let mut t = t.clone();
                t.at = m.apply(t.at);
                t.height *= sy;
                t.width_factor *= if sy > 0.0 { sx / sy } else { 1.0 };
                t.rotation_deg += m.rotation_deg();
                self.snap(t.at, SnapKind::Node);
                self.push(layer, colour, "TEXT", Shape::Text(t));
            }
            Geom::Point(p) => {
                let p = m.apply(*p);
                self.snap(p, SnapKind::Node);
                self.push(layer, colour, "POINT", Shape::Point(p));
            }
            Geom::Solid(pts) => {
                let pts: Vec<Pt2> = pts.iter().map(|p| m.apply(*p)).collect();
                for p in &pts {
                    self.snap(*p, SnapKind::End);
                }
                self.push(layer, colour, "SOLID", Shape::Solid(pts));
            }
            Geom::Hatch(loops) => {
                for l in loops {
                    let pts = chain_edges(l, tol);
                    if pts.len() >= 3 {
                        let pts: Vec<Pt2> = pts.into_iter().map(|p| m.apply(p)).collect();
                        self.push(layer, colour, "HATCH", Shape::Poly { pts, closed: true });
                    }
                }
            }
            Geom::Insert { block, at, sx, sy, rot_deg, cols, rows, dx, dy } => {
                let Some(b) = self.u.blocks.get(&block.to_ascii_uppercase()) else {
                    self.warn(format!("Block '{block}' is not defined: its INSERTs are not drawn."));
                    return;
                };
                if depth >= self.o.max_depth {
                    self.warn(format!("Blocks are nested deeper than {}: the deepest are not drawn.", self.o.max_depth));
                    return;
                }
                self.snap(m.apply(*at), SnapKind::Node);
                let ctx = Ctx { layer: Some(layer), colour: Some(colour) };
                let rs = Affine::rotate_scale(*rot_deg, *sx, *sy);
                let to_base = Affine::translate(-b.base.x, -b.base.y);
                let (cs, sn) = (rot_deg.to_radians().cos(), rot_deg.to_radians().sin());
                for r in 0..*rows {
                    for c in 0..*cols {
                        if self.full() {
                            self.out.truncated = true;
                            return;
                        }
                        let (ox, oy) = (c as f64 * dx, r as f64 * dy);
                        let off = (ox * cs - oy * sn, ox * sn + oy * cs);
                        let place = Affine::translate(at.x + off.0, at.y + off.1).after(&rs).after(&to_base);
                        let mm = m.after(&place);
                        self.walk(&b.entities, &mm, &ctx, depth + 1, b.base);
                    }
                }
            }
        }
    }
}

/// Points of a spline and a note when it could only be approximated.
fn spline_points(degree: usize, knots: &[f64], ctrl: &[Pt2], weights: &[f64], fit: &[Pt2], tol: f64) -> (Vec<Pt2>, Option<String>) {
    if !ctrl.is_empty() {
        match Nurbs::new(degree, knots.to_vec(), ctrl.to_vec(), weights.to_vec()) {
            Ok(n) => return (n.flatten(tol.max(1e-9)), None),
            Err(why) => {
                return (ctrl.to_vec(), Some(format!("A SPLINE could not be evaluated ({why}): its control polygon is drawn.")));
            }
        }
    }
    if fit.len() >= 2 {
        return (fit.to_vec(), Some("A SPLINE with fit points only is drawn as a polyline through them, not as the interpolating curve.".into()));
    }
    (Vec::new(), None)
}

/// One closed outline from a HATCH loop's edges, joined end to start (an edge is reversed when
/// that makes it connect).
fn chain_edges(edges: &[Geom], tol: f64) -> Vec<Pt2> {
    let mut out: Vec<Pt2> = Vec::new();
    for e in edges {
        let mut pts = match e {
            Geom::Line(a, b) => vec![*a, *b],
            Geom::Arc { c, r, a0, a1 } => arc_points(*c, *r, *a0, sweep(*a0, *a1), tol),
            Geom::Ellipse { c, major, minor, t0, t1 } => ellipse_points(*c, *major, *minor, *t0, *t1, tol),
            Geom::Spline { degree, knots, ctrl, weights, fit, .. } => spline_points(*degree, knots, ctrl, weights, fit, tol).0,
            Geom::Poly { pts, closed } => {
                let mut v = Vec::new();
                let n = pts.len();
                for i in 0..if *closed { n } else { n - 1 } {
                    let (a, b) = (pts[i].p, pts[(i + 1) % n].p);
                    v.push(a);
                    if let Some(arc) = bulge_arc(a, b, pts[i].bulge) {
                        let sw = if arc.forward { sweep(arc.start_deg, arc.end_deg) } else { -sweep(arc.start_deg, arc.end_deg) };
                        let start = if arc.forward { arc.start_deg } else { arc.end_deg };
                        let p = arc_points(arc.centre, arc.radius, start, sw, tol);
                        v.extend_from_slice(&p[1..p.len() - 1]);
                    }
                }
                if !*closed {
                    v.push(pts[n - 1].p);
                }
                v
            }
            _ => continue,
        };
        if pts.is_empty() {
            continue;
        }
        if let Some(&last) = out.last() {
            let d = |p: Pt2| (p.x - last.x).hypot(p.y - last.y);
            if d(pts[pts.len() - 1]) < d(pts[0]) {
                pts.reverse();
            }
            if d(pts[0]) < 1e-9 * (1.0 + last.x.abs().max(last.y.abs())) {
                pts.remove(0);
            }
        }
        out.extend(pts);
    }
    if out.len() > 1 && (out[0].x - out[out.len() - 1].x).hypot(out[0].y - out[out.len() - 1].y) < 1e-9 {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_composition_is_apply_second_after_first() {
        let t = Affine::translate(10.0, 0.0);
        let r = Affine::rotate_scale(90.0, 2.0, 2.0);
        // Rotate-scale first, then translate.
        let p = t.after(&r).apply(pt(1.0, 0.0));
        assert!((p.x - 10.0).abs() < 1e-12 && (p.y - 2.0).abs() < 1e-12);
        assert!(r.is_similarity() && !Affine::rotate_scale(0.0, -1.0, 1.0).is_similarity() && !Affine::rotate_scale(0.0, 2.0, 1.0).is_similarity());
        assert!((r.rotation_deg() - 90.0).abs() < 1e-12);
    }
}
