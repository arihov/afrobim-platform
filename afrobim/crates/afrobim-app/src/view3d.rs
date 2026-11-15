//! The 3D view: the model's elements as upright plates, seen through an orbit camera.
//!
//! The sheet is mapped to the world the same way IFC export does it: sheet X → world X, sheet Y →
//! world Z (up), each sign face standing in the XZ plane and facing −Y. This is a viewing
//! convention until elements carry a real site placement.
//!
//! Drawing is a software projection onto the egui painter (no GPU code of its own): elements are
//! flat plates, so sorting them back to front is enough for a correct picture.

use afrobim_kernel::BBox;
use egui::{pos2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui};

use crate::app::AfroBimApp;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// An orbit camera: it looks at `target` from `distance` away, at `yaw` around the vertical axis
/// and `pitch` above the horizon (radians).
#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    pub target: V3,
    pub distance: f64,
    pub yaw: f64,
    pub pitch: f64,
}

impl Default for Camera {
    fn default() -> Self {
        // In front of the sign faces (−Y side), a little to the right and above.
        Self { target: [0.0, 0.0, 0.0], distance: 10_000.0, yaw: (-65.0f64).to_radians(), pitch: 14.0f64.to_radians() }
    }
}

/// Clips a triangle, given in view space (`[right, up, depth]`), to the half-space in front of the
/// near plane `depth >= near`, and returns the triangles that remain: none, one or two.
///
/// Why the clipping is needed. Perspective projection divides by depth: `x_screen = x / depth * f`.
/// A vertex at or behind the camera (depth <= 0) has no screen position, and one just in front of
/// it lands thousands of pixels away. Dropping every triangle that has such a vertex removes the
/// large triangles of a pavement or the ground as soon as the camera is close to them, because
/// those triangles reach behind the camera while the part in view is still large. Clipping keeps
/// the part in front.
///
/// Derivation. The edge from `a` to `b` is the line `p(t) = a + t (b - a)`, `t` in `[0, 1]`. Its
/// depth is linear in `t`: `z(t) = za + t (zb - za)`. It meets the plane `z = near` where
/// `za + t (zb - za) = near`, that is at `t = (near - za) / (zb - za)`. Because the depth is
/// linear, so is every other coordinate along the edge, so the intersection point is
/// `a + t (b - a)` with that `t`, and its depth is exactly `near`. Walking round the triangle
/// edge by edge and keeping the vertices in front plus the intersection points of the edges that
/// cross the plane gives the clipped polygon (the Sutherland-Hodgman step for one plane):
///
/// * all three vertices in front: the triangle is unchanged;
/// * one vertex behind: the edges to it are cut, leaving a quadrilateral, which is split into two
///   triangles along a diagonal;
/// * two vertices behind: the two edges from the vertex in front are cut, leaving one triangle;
/// * all three behind: nothing remains.
///
/// The winding order is kept, and the cut points lie on the original plane of the triangle, so the
/// shading (which comes from the world-space normal) is unchanged.
pub fn clip_triangle_near(tri: [V3; 3], near: f64) -> Vec<[V3; 3]> {
    let mut poly: Vec<V3> = Vec::with_capacity(4);
    for i in 0..3 {
        let (a, b) = (tri[i], tri[(i + 1) % 3]);
        let (a_in, b_in) = (a[2] >= near, b[2] >= near);
        if a_in {
            poly.push(a);
        }
        if a_in != b_in {
            let t = (near - a[2]) / (b[2] - a[2]);
            poly.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), near]);
        }
    }
    match poly.len() {
        3 => vec![[poly[0], poly[1], poly[2]]],
        4 => vec![[poly[0], poly[1], poly[2]], [poly[0], poly[2], poly[3]]],
        _ => Vec::new(),
    }
}

/// Vertical field of view.
const FOV: f64 = 0.61; // 35°
/// The same, in degrees, for saved viewpoints.
pub const FOV_DEG: f64 = 0.61 * 180.0 / std::f64::consts::PI;
const NEAR: f64 = 1.0;

impl Camera {
    pub fn eye(&self) -> V3 {
        let (cp, sp) = (self.pitch.cos(), self.pitch.sin());
        [
            self.target[0] + self.distance * cp * self.yaw.cos(),
            self.target[1] + self.distance * cp * self.yaw.sin(),
            self.target[2] + self.distance * sp,
        ]
    }

    /// Right, up and forward unit vectors of the view.
    fn basis(&self) -> (V3, V3, V3) {
        let forward = norm(sub(self.target, self.eye()));
        let right = norm(cross(forward, [0.0, 0.0, 1.0]));
        (right, cross(right, forward), forward)
    }

    /// Screen position and view depth of a world point, or None when it is behind the camera.
    pub fn project(&self, p: V3, viewport: Rect) -> Option<(Pos2, f64)> {
        let (right, up, forward) = self.basis();
        let d = sub(p, self.eye());
        let z = dot(d, forward);
        if z < NEAR {
            return None;
        }
        let focal = viewport.height() as f64 / (2.0 * (FOV / 2.0).tan());
        let c = viewport.center();
        Some((pos2((c.x as f64 + dot(d, right) / z * focal) as f32, (c.y as f64 - dot(d, up) / z * focal) as f32), z))
    }

    /// A world point in view space: `[right, up, depth]`, the depth along the viewing direction.
    pub fn to_view(&self, p: V3) -> V3 {
        let (right, up, forward) = self.basis();
        let d = sub(p, self.eye());
        [dot(d, right), dot(d, up), dot(d, forward)]
    }

    /// Screen position of a view-space point (see [`Camera::to_view`]); the depth must be positive.
    pub fn view_to_screen(&self, v: V3, viewport: Rect) -> Pos2 {
        let focal = viewport.height() as f64 / (2.0 * (FOV / 2.0).tan());
        let c = viewport.center();
        pos2((c.x as f64 + v[0] / v[2] * focal) as f32, (c.y as f64 - v[1] / v[2] * focal) as f32)
    }

    /// The viewing direction (unit) and the up vector of the view, world axes.
    pub fn look(&self) -> (V3, V3) {
        let (_, up, forward) = self.basis();
        (forward, up)
    }

    /// Looks from `position` along `direction`, with the orbit target `distance` ahead.
    pub fn set_look(&mut self, position: V3, direction: V3, distance: f64) {
        let d = norm(direction);
        self.distance = distance.clamp(50.0, 5.0e9);
        self.target = [position[0] + d[0] * self.distance, position[1] + d[1] * self.distance, position[2] + d[2] * self.distance];
        self.pitch = (-d[2]).clamp(-1.0, 1.0).asin().clamp(-1.45, 1.45);
        self.yaw = (-d[1]).atan2(-d[0]);
    }

    pub fn orbit(&mut self, dx_px: f64, dy_px: f64) {
        self.yaw -= dx_px * 0.008;
        self.pitch = (self.pitch + dy_px * 0.008).clamp(-1.45, 1.45);
    }

    /// Slides the target in the view plane so the model follows the pointer.
    pub fn pan(&mut self, dx_px: f64, dy_px: f64, viewport: Rect) {
        let (right, up, _) = self.basis();
        let focal = viewport.height() as f64 / (2.0 * (FOV / 2.0).tan());
        let k = self.distance / focal;
        for i in 0..3 {
            self.target[i] += (-dx_px * right[i] + dy_px * up[i]) * k;
        }
    }

    pub fn zoom(&mut self, factor: f64) {
        self.distance = (self.distance / factor).clamp(50.0, 5.0e9);
    }

    /// Frames a sphere.
    pub fn fit_sphere(&mut self, centre: V3, radius: f64) {
        self.target = centre;
        self.distance = (radius.max(500.0) / (FOV / 2.0).sin() * 1.05).clamp(50.0, 5.0e9);
    }

    /// Frames a sheet-space box (standing in the XZ plane) with a margin.
    pub fn fit(&mut self, sheet: &BBox) {
        let c = sheet.centre();
        self.target = [c.x, 0.0, c.y];
        let radius = (sheet.width().powi(2) + sheet.height().powi(2)).sqrt() / 2.0;
        self.distance = (radius.max(500.0) / (FOV / 2.0).sin() * 1.05).clamp(50.0, 5.0e6);
    }
}

/// Where an element's own frame sits in the world: local (u along the width, d into the depth,
/// v up) maps to `origin + u·(cos, sin, 0) + d·(−sin, cos, 0) + v·(0, 0, 1)`.
#[derive(Clone, Copy)]
pub(crate) struct Frame {
    pub origin: V3,
    pub cos: f64,
    pub sin: f64,
}

impl Frame {
    /// The same frame as a placement for clash detection, which uses the same transform.
    pub fn placement(&self) -> afrobim_coordination::Placement {
        afrobim_coordination::Placement::new(self.origin, self.sin.atan2(self.cos).to_degrees())
    }
    fn at(&self, u: f64, d: f64, v: f64) -> V3 {
        [self.origin[0] + u * self.cos - d * self.sin, self.origin[1] + u * self.sin + d * self.cos, self.origin[2] + v]
    }
    /// The direction a face in the local XZ plane looks (local −Y), in the world.
    fn facing(&self) -> V3 {
        [self.sin, -self.cos, 0.0]
    }
}

/// Categories whose solids are modelled directly in site coordinates (X east, Y north, Z up from
/// their site placement) and are shown only in the 3D site view, never on the design sheet.
///
/// That includes lighting, restraint, pavement and corridors (through `modules_glue::owns`) and
/// ground surfaces. A corridor has no solids of its own: its surfaces are built from the
/// corridor and cached by the app (`AfroBimApp::corridor_cache`).
pub(crate) fn is_site_solid(category: &str) -> bool {
    category == crate::markings_glue::CATEGORY || category == crate::ext_markings_glue::CATEGORY_EXT || crate::modules_glue::owns(category) || crate::modules_glue::is_surface(category)
}

/// The alignment's centreline as site points in millimetres, at the profile's level where the
/// alignment has one (held at the end levels beyond the profile's ends) and at zero where not.
fn centreline_points(a: &afrobim_alignment::Alignment, step_m: f64) -> Vec<V3> {
    let (start, end) = (a.start_station_m, a.end_station_m());
    let steps = (((end - start) / step_m).ceil() as usize).clamp(1, 20_000);
    (0..=steps)
        .filter_map(|i| {
            let s = start + (end - start) * i as f64 / steps as f64;
            let p = a.point_at(s)?;
            let z = a.profile.as_ref().and_then(|profile| profile.range().and_then(|(lo, hi)| profile.elevation_at(s.clamp(lo, hi)).ok())).unwrap_or(0.0);
            Some([p.x * 1000.0, p.y * 1000.0, z * 1000.0])
        })
        .collect()
}

/// What the frame of an element depends on besides the document: the solids generated for it and
/// the origin of a built corridor. Passed as closures so the transform can be used (and tested)
/// without the app's caches.
pub(crate) struct WorldCtx<'a> {
    pub doc: &'a afrobim_kernel::Document,
    pub solids_of: &'a dyn Fn(afrobim_kernel::ElementId) -> Option<&'a [afrobim_draw::Mesh3]>,
    pub corridor_origin: &'a dyn Fn(afrobim_kernel::ElementId) -> Option<V3>,
}

/// Site mode: at least one sign or site solid has been placed on site.
pub(crate) fn site_mode_of(doc: &afrobim_kernel::Document) -> bool {
    doc.elements().iter().any(|e| (e.category == crate::signs_glue::CATEGORY || is_site_solid(&e.category)) && e.site.is_some())
}

/// The post a support hangs from (itself for a post) and the sign above it.
fn support_chain<'a>(doc: &'a afrobim_kernel::Document, e: &'a afrobim_kernel::Element) -> Option<(&'a afrobim_kernel::Element, &'a afrobim_kernel::Element)> {
    let post = if e.category == crate::supports_glue::POST_CATEGORY { e } else { doc.find(e.host?)? };
    Some((post, doc.find(post.host?)?))
}

/// The mounting height recorded on a sign's first post, or 0 for a sign without supports.
fn mounting_height_in(doc: &afrobim_kernel::Document, sign: afrobim_kernel::ElementId) -> f64 {
    doc.elements()
        .iter()
        .find(|e| e.category == crate::supports_glue::POST_CATEGORY && e.host == Some(sign))
        .and_then(|p| p.properties.get(crate::supports_glue::PROP_MOUNTING_HEIGHT_MM))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

/// Where an element's own frame sits in the world, the rule the 3D view draws by.
pub(crate) fn frame_in(ctx: &WorldCtx, e: &afrobim_kernel::Element, site_mode: bool) -> Option<Frame> {
    let support = crate::supports_glue::is_support(&e.category);
    if is_site_solid(&e.category) {
        // Modelled for the site: shown in the site view only, in the frame of the placement.
        let s = e.site.as_ref().filter(|_| site_mode)?;
        if e.category == afrobim_corridor::CORRIDOR_CATEGORY {
            // The built surfaces have their own origin (the least plan point of the sections).
            return Some(Frame { origin: (ctx.corridor_origin)(e.id)?, cos: 1.0, sin: 0.0 });
        }
        let (sin, cos) = s.rotation_deg.to_radians().sin_cos();
        return Some(Frame { origin: [s.x, s.y, s.z], cos, sin });
    }
    if site_mode {
        let s = e.site.as_ref()?;
        let (sin, cos) = s.rotation_deg.to_radians().sin_cos();
        // A sign's site level is the ground under it; its face starts one mounting height up.
        let lift = if support { 0.0 } else { mounting_height_in(ctx.doc, e.id) };
        return Some(Frame { origin: [s.x, s.y, s.z + lift], cos, sin });
    }
    if !support {
        return Some(Frame { origin: [e.origin.x, 0.0, e.origin.y], cos: 1.0, sin: 0.0 });
    }
    // On the upright sheet a support stands under its sign: the sign's lower edge is at its
    // sheet height, so its ground is one mounting height below.
    use crate::supports_glue::{PROP_MOUNTING_HEIGHT_MM, PROP_POSITION_ALONG_SIGN_MM, PROP_SETBACK_MM};
    let (post, sign) = support_chain(ctx.doc, e)?;
    let number = |key: &str| post.properties.get(key).and_then(|v| v.parse::<f64>().ok());
    let (u, setback, mounting) = (number(PROP_POSITION_ALONG_SIGN_MM)?, number(PROP_SETBACK_MM)?, number(PROP_MOUNTING_HEIGHT_MM)?);
    let solids = (ctx.solids_of)(e.id)?;
    let (mut max_x, mut max_y) = (0.0f64, 0.0f64);
    for v in solids.iter().flat_map(|m| m.vertices.iter()) {
        max_x = max_x.max(v[0]);
        max_y = max_y.max(v[1]);
    }
    Some(Frame { origin: [sign.origin.x + u - max_x / 2.0, setback - max_y / 2.0, sign.origin.y - mounting], cos: 1.0, sin: 0.0 })
}

impl AfroBimApp {
    /// The 3D view shows recorded progress (the Construction tab) instead of the programme's state.
    pub(crate) fn by_record(&self) -> bool {
        self.construction.colour_by_record && self.sched.fourd_on
    }

    /// Site mode: at least one sign has been placed on site, so the 3D view shows the site (signs
    /// at their stations, alignments on the ground) instead of the design sheet stood upright.
    pub(crate) fn site_mode(&self) -> bool {
        site_mode_of(&self.doc)
    }

    /// The frame of a sign face (local origin at its lower-left corner), or of a support's solids
    /// (local origin at its plan corner, at ground level). None when the element is not shown in
    /// the current mode.
    pub(crate) fn frame_of(&self, e: &afrobim_kernel::Element, site_mode: bool) -> Option<Frame> {
        let solids = |id: afrobim_kernel::ElementId| self.meshes_by_id(id);
        let origin = |id: afrobim_kernel::ElementId| self.corridor_cache.get(&id)?.solids.as_ref().ok().map(|s| s.origin);
        frame_in(&WorldCtx { doc: &self.doc, solids_of: &solids, corridor_origin: &origin }, e, site_mode)
    }

    /// The solids of an element by id: its generated solids, or for a corridor the built surfaces.
    pub(crate) fn meshes_by_id(&self, id: afrobim_kernel::ElementId) -> Option<&[afrobim_draw::Mesh3]> {
        if self.doc.find(id).is_some_and(|e| e.category == afrobim_corridor::CORRIDOR_CATEGORY) {
            return self.corridor_cache.get(&id)?.solids.as_ref().ok().map(|s| s.meshes.as_slice());
        }
        self.cache.get(&id)?.generated.as_ref().ok().map(|g| g.solids.as_slice())
    }

    /// The meshes an element is shown with in 3D: its generated solids, or for a corridor the
    /// surfaces built from it.
    fn meshes_of(&self, e: &afrobim_kernel::Element) -> Option<&[afrobim_draw::Mesh3]> {
        self.meshes_by_id(e.id)
    }

    /// Lowest and highest world corner of an element's meshes in its frame, for framing.
    pub(crate) fn mesh_bounds(&self, e: &afrobim_kernel::Element, frame: Frame) -> Option<(V3, V3)> {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for v in self.meshes_of(e)?.iter().flat_map(|m| m.vertices.iter()) {
            let p = frame.at(v[0], v[1], v[2]);
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        (lo[0] <= hi[0]).then_some((lo, hi))
    }

    /// The triangles of a support's solids, flat-shaded, each with its own view depth.
    fn support_triangles(&self, e: &afrobim_kernel::Element, frame: Frame, rect: Rect, items: &mut Vec<(f64, Vec<Shape>)>) {
        let Some(solids) = self.meshes_of(e) else { return };
        // Recorded progress colours the model by percent complete; the ground stays as it is.
        let by_record = self.by_record() && !crate::modules_glue::is_surface(&e.category);
        let record_rgb = by_record.then(|| crate::construction_ui::percent_colour(self.construction.percent_of(e.id)));
        let style = if self.by_record() { crate::schedule_ui::FourD::Normal } else { self.sched.fourd_style(e.id) };
        if style == crate::schedule_ui::FourD::Hidden {
            return;
        }
        let highlighted = self.coord.highlight.contains(&e.id);
        let cam = &self.camera;
        let light = norm([-0.35, -0.75, 0.55]);
        let selected = self.doc.is_selected(e.id);
        let both_sides = is_site_solid(&e.category);
        for mesh in solids {
            for t in &mesh.triangles {
                let p: Vec<V3> = t.iter().map(|i| mesh.vertices[*i as usize]).map(|v| frame.at(v[0], v[1], v[2])).collect();
                let normal = norm(cross(sub(p[1], p[0]), sub(p[2], p[0])));
                // Skip faces turned away from the camera: the solids are closed. Site solids are
                // thin strips and sheets whose winding is not relied on, so both sides are drawn
                // and lit as if facing the viewer.
                let facing = dot(normal, sub(cam.eye(), p[0]));
                if facing <= 0.0 && !both_sides {
                    continue;
                }
                let normal = if facing < 0.0 { [-normal[0], -normal[1], -normal[2]] } else { normal };
                // Clipped to the near plane, so a slab with a vertex behind the camera still shows
                // the part in front (one or two triangles); see `clip_triangle_near`.
                let view = [cam.to_view(p[0]), cam.to_view(p[1]), cam.to_view(p[2])];
                for piece in clip_triangle_near(view, NEAR) {
                let projected: Vec<(Pos2, f64)> = piece.iter().map(|v| (cam.view_to_screen(*v, rect), v[2])).collect();
                // The ground is a backdrop: its triangles are large, so sorting them by centre
                // against the small things that stand on them goes wrong. They are painted
                // first (in order among themselves) and everything else over them.
                let depth = projected.iter().map(|(_, d)| *d).sum::<f64>() / 3.0 + if crate::modules_glue::is_surface(&e.category) { 1.0e12 } else { 0.0 };
                let shade = (0.45 + 0.55 * dot(normal, light).max(0.0)) as f32;
                let [r, g, b] = record_rgb.unwrap_or(mesh.colour);
                let mut colour = Color32::from_rgb((r as f32 * shade) as u8, (g as f32 * shade) as u8, (b as f32 * shade) as u8);
                if selected {
                    colour = Color32::from_rgb(colour.r() / 2 + 0x26, colour.g() / 2 + 0x51, colour.b() / 2 + 0x7F);
                }
                match style {
                    // Work in progress is amber; work not started is a faint ghost.
                    crate::schedule_ui::FourD::Active => colour = Color32::from_rgb(colour.r() / 3 + 0xA0, colour.g() / 3 + 0x70, colour.b() / 6 + 0x10),
                    crate::schedule_ui::FourD::Ghost => colour = Color32::from_rgba_unmultiplied(colour.r(), colour.g(), colour.b(), 70),
                    _ => {}
                }
                // The two elements of the clash last zoomed to.
                if highlighted {
                    colour = Color32::from_rgb(0xE0, 0x3C, 0xC8);
                }
                // A raw mesh, not a polygon shape: the painter's edge smoothing turns the sliver
                // triangles of a round post into long spikes.
                let mut tri = egui::Mesh::default();
                for (q, _) in &projected {
                    tri.vertices.push(egui::epaint::Vertex { pos: *q, uv: egui::epaint::WHITE_UV, color: colour });
                }
                tri.indices = vec![0, 1, 2];
                items.push((depth, vec![Shape::mesh(tri)]));
                }
            }
        }
    }

    /// World-space corners of everything the current mode shows, for framing and the ground grid.
    pub(crate) fn shown_points(&self, site_mode: bool) -> Vec<V3> {
        let mut points = Vec::new();
        for e in self.doc.elements() {
            if e.category == crate::alignment_glue::CATEGORY {
                // Frame the alignments that carry signs; one far away with nothing on it would
                // shrink the signs to a dot.
                let used = self.doc.elements().iter().any(|x| x.site.as_ref().and_then(|p| p.alignment.as_deref()) == Some(e.name.as_str()));
                if site_mode && used {
                    if let Some(a) = crate::alignment_glue::alignment_of(&self.doc, e.id) {
                        points.extend(centreline_points(&a, 50.0));
                    }
                }
                continue;
            }
            let Some(frame) = self.frame_of(e, site_mode) else { continue };
            if is_site_solid(&e.category) {
                // The corners of the solids themselves, so that levels well above zero are framed.
                match self.mesh_bounds(e, frame) {
                    Some((lo, hi)) => points.extend([lo, hi]),
                    None => points.extend([frame.at(0.0, 0.0, 0.0), frame.at(e.width, e.height, 0.0)]),
                }
            } else if crate::supports_glue::is_support(&e.category) {
                points.push(frame.at(0.0, 0.0, 0.0));
            } else {
                points.extend([frame.at(0.0, 0.0, 0.0), frame.at(e.width, 0.0, e.height)]);
            }
        }
        if let Some((lo, hi)) = self.refs.world_bounds() {
            points.extend([lo, hi]);
        }
        points
    }
}

/// Sheet point (x, y) on the plane at depth `y_world` → world.
#[cfg(test)]
fn world(x: f64, y: f64, y_world: f64) -> V3 {
    [x, y_world, y]
}

impl AfroBimApp {
    pub(crate) fn canvas_3d(&mut self, ui: &mut Ui) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;
        if !self.fitted_3d && rect.width() > 50.0 {
            self.fitted_3d = true;
            self.fit_3d();
            // `--camera distance,yaw,pitch[,station]`: a chosen viewpoint, for screenshots.
            if let Some(spec) = self.options.camera.take() {
                self.apply_camera_option(&spec);
            }
        }

        // Left drag orbits, middle or Shift + left drag pans, the wheel zooms, double-click fits.
        let (delta, primary, middle, shift, scroll) = ui.input(|i| {
            (
                i.pointer.delta(),
                i.pointer.button_down(egui::PointerButton::Primary),
                i.pointer.button_down(egui::PointerButton::Middle),
                i.modifiers.shift,
                i.raw_scroll_delta.y,
            )
        });
        if response.dragged() || (response.contains_pointer() && middle) {
            if middle || (primary && shift) {
                self.camera.pan(delta.x as f64, delta.y as f64, rect);
            } else if primary {
                self.camera.orbit(delta.x as f64, delta.y as f64);
            }
        }
        if response.contains_pointer() && scroll != 0.0 {
            self.camera.zoom((scroll as f64 * 0.0025).exp());
        }
        if response.double_clicked() {
            self.fit_3d();
        } else if response.clicked() {
            if let Some(at) = response.interact_pointer_pos() {
                if let Some(hit) = self.refs.pick(&self.camera, rect, at) {
                    self.refs.picked = Some(hit);
                }
            }
        }

        self.sync_generated();
        if self.by_record() {
            self.construction.sync(&self.doc);
            self.construction.refresh_percents(self.sched.date);
        }
        self.paint_3d(&painter, rect);
    }

    /// Applies `--camera distance_mm,yaw_deg,pitch_deg[,station_m]`; a spec that does not parse is ignored.
    fn apply_camera_option(&mut self, spec: &str) {
        let v: Vec<f64> = spec.split(',').filter_map(|t| t.trim().parse().ok()).collect();
        if v.len() < 3 {
            return;
        }
        self.camera.distance = v[0].max(50.0);
        self.camera.yaw = v[1].to_radians();
        self.camera.pitch = v[2].to_radians();
        if let Some(station) = v.get(3) {
            let first = crate::alignment_glue::alignments(&self.doc).into_iter().next().and_then(|(id, _)| crate::alignment_glue::alignment_of(&self.doc, id));
            if let Some(a) = first {
                let pts = centreline_points(&a, 1.0);
                let span = a.end_station_m() - a.start_station_m;
                if let Some(p) = pts.get((((station - a.start_station_m) / span.max(1e-9)).clamp(0.0, 1.0) * (pts.len() - 1) as f64).round() as usize) {
                    self.camera.target = *p;
                }
            }
        }
    }

    pub(crate) fn fit_3d(&mut self) {
        self.sync_generated();
        if let Some(mark) = self.options.focus.clone() {
            let site_mode = self.site_mode();
            if let Some((frame, e)) = self.doc.elements().iter().find(|e| e.name == mark).and_then(|e| self.frame_of(e, site_mode).map(|f| (f, e))) {
                if is_site_solid(&e.category) {
                    // A site element: frame its solids, looking at them from the south-east above.
                    if let Some((lo, hi)) = self.mesh_bounds(e, frame) {
                        let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
                        self.camera.yaw = (-60.0f64).to_radians();
                        self.camera.pitch = 30.0f64.to_radians();
                        self.camera.fit_sphere(centre, dot(sub(hi, lo), sub(hi, lo)).sqrt() / 2.0);
                        return;
                    }
                }
                let centre = frame.at(e.width / 2.0, 0.0, e.height / 2.0);
                // Look at the face from in front of it, a little to one side.
                let facing = frame.facing();
                self.camera.yaw = facing[1].atan2(facing[0]) + 0.45;
                self.camera.fit_sphere(centre, e.width.max(e.height) * 1.6);
                return;
            }
        }
        let points = self.shown_points(self.site_mode());
        if points.is_empty() {
            self.camera.fit(&BBox::new(0.0, 0.0, 5000.0, 3000.0));
            return;
        }
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &points {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        self.camera.fit_sphere(centre, dot(sub(hi, lo), sub(hi, lo)).sqrt() / 2.0);
    }

    fn paint_3d(&self, painter: &Painter, rect: Rect) {
        let dark = self.dark_canvas;
        painter.rect_filled(rect, 0.0, if dark { Color32::from_rgb(0x1B, 0x1F, 0x25) } else { Color32::from_rgb(0xEE, 0xF0, 0xF2) });
        let cam = &self.camera;
        let line = |a: V3, b: V3, stroke: Stroke| {
            if let (Some((p, _)), Some((q, _))) = (cam.project(a, rect), cam.project(b, rect)) {
                painter.line_segment([p, q], stroke);
            }
        };

        let site_mode = self.site_mode();
        let points = self.shown_points(site_mode);
        let (mut lo, mut hi) = ([0.0, -2000.0, 0.0], [5000.0, 2000.0, 3000.0]);
        if !points.is_empty() {
            lo = [f64::INFINITY; 3];
            hi = [f64::NEG_INFINITY; 3];
            for p in &points {
                for i in 0..3 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
        }
        // Ground grid: at site level 0, or just below the lowest element of the upright sheet.
        // A depth cue, not a surveyed surface. The step grows so there are never too many lines.
        let ground = if site_mode && !points.is_empty() { lo[2] } else if site_mode { 0.0 } else { lo[2] - 300.0 };
        let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(4000.0);
        let mut step = 1000.0;
        while span / step > 40.0 {
            step *= 10.0;
        }
        let pad = 2.0 * step;
        let (x0, x1) = (((lo[0] - pad) / step).floor() * step, ((hi[0] + pad) / step).ceil() * step);
        let (y0, y1) = if site_mode { (((lo[1] - pad) / step).floor() * step, ((hi[1] + pad) / step).ceil() * step) } else { (-4000.0, 4000.0) };
        let grid = Stroke::new(1.0_f32, if dark { Color32::from_rgb(0x2E, 0x34, 0x3C) } else { Color32::from_rgb(0xCF, 0xD3, 0xD8) });
        let mut x = x0;
        while x <= x1 + 0.5 {
            line([x, y0, ground], [x, y1, ground], grid);
            x += step;
        }
        let mut y = y0;
        while y <= y1 + 0.5 {
            line([x0, y, ground], [x1, y, ground], grid);
            y += step;
        }

        if site_mode {
            self.under.paint_3d(&self.doc, painter, cam, rect, if dark { Color32::from_gray(200) } else { Color32::from_gray(60) });
        }
        // Everything is gathered with its view depth, then painted farthest first.
        let eye = cam.eye();
        let plate = if dark { Color32::from_rgb(0x8C, 0x93, 0x9B) } else { Color32::from_rgb(0x9A, 0xA1, 0xA8) };
        let label = if dark { Color32::from_rgb(0xA8, 0xB0, 0xBA) } else { Color32::from_rgb(0x50, 0x55, 0x5C) };
        let mut items: Vec<(f64, Vec<Shape>)> = Vec::new();

        for e in self.doc.elements() {
            if e.category == crate::alignment_glue::CATEGORY {
                // The centreline on the ground, in the site view only.
                if site_mode {
                    if let Some(a) = crate::alignment_glue::alignment_of(&self.doc, e.id) {
                        let centre = Stroke::new(2.0_f32, Color32::from_rgb(0xE8, 0xC5, 0x47));
                        // At the profile's level where the alignment has one, else on z = 0.
                        let pts = centreline_points(&a, 5.0);
                        for w in pts.windows(2) {
                            line(w[0], w[1], centre);
                        }
                    }
                }
                continue;
            }
            // Site-only elements are on layers hidden from the design sheet, like alignments.
            if !self.doc.layer_visible(&e.layer) && !is_site_solid(&e.category) {
                continue;
            }
            let Some(frame) = self.frame_of(e, site_mode) else { continue };
            if crate::supports_glue::is_support(&e.category) || is_site_solid(&e.category) {
                self.support_triangles(e, frame, rect, &mut items);
                continue;
            }
            let style = if self.by_record() { crate::schedule_ui::FourD::Normal } else { self.sched.fourd_style(e.id) };
            if style == crate::schedule_ui::FourD::Hidden {
                continue;
            }
            let record_colour = self.by_record().then(|| {
                let [r, g, b] = crate::construction_ui::percent_colour(self.construction.percent_of(e.id));
                Color32::from_rgb(r, g, b)
            });
            let centre = frame.at(e.width / 2.0, 0.0, e.height / 2.0);
            let Some((_, depth)) = cam.project(centre, rect) else { continue };
            let corners = [(0.0, 0.0), (e.width, 0.0), (e.width, e.height), (0.0, e.height)];
            let quad: Option<Vec<Pos2>> = corners.iter().map(|(u, v)| cam.project(frame.at(*u, 0.0, *v), rect).map(|(p, _)| p)).collect();
            let Some(quad) = quad else { continue };
            let mut shapes = Vec::new();

            let from_front = dot(frame.facing(), sub(eye, centre)) > 0.0;
            let artwork = self.cache.get(&e.id).filter(|c| c.generated.is_ok()).filter(|_| from_front);
            let artwork = artwork.filter(|_| style != crate::schedule_ui::FourD::Ghost && record_colour.is_none());
            match artwork {
                Some(cached) => {
                    // The face, just in front of the plate.
                    let place = |u: f64, v: f64| cam.project(frame.at(u, -0.5, v), rect).map(|(p, _)| p);
                    if let Some(shape) = cached.mesh.projected(&place) {
                        shapes.push(shape);
                    }
                }
                // Recorded progress: the whole plate in the colour of its percent complete.
                None if record_colour.is_some() => shapes.push(Shape::convex_polygon(quad.clone(), record_colour.unwrap_or(plate), Stroke::NONE)),
                // Seen from behind, or without geometry: the bare plate.
                None if style == crate::schedule_ui::FourD::Ghost => shapes.push(Shape::convex_polygon(quad.clone(), Color32::from_rgba_unmultiplied(plate.r(), plate.g(), plate.b(), 70), Stroke::NONE)),
                None => shapes.push(Shape::convex_polygon(quad.clone(), plate, Stroke::NONE)),
            }
            if style == crate::schedule_ui::FourD::Active {
                shapes.push(Shape::closed_line(quad.clone(), Stroke::new(3.0_f32, Color32::from_rgb(0xF2, 0xA9, 0x1D))));
            }
            if self.coord.highlight.contains(&e.id) {
                shapes.push(Shape::closed_line(quad.clone(), Stroke::new(3.0_f32, Color32::from_rgb(0xE0, 0x3C, 0xC8))));
            }
            if self.doc.is_selected(e.id) {
                shapes.push(Shape::closed_line(quad.clone(), Stroke::new(2.0_f32, Color32::from_rgb(0x4D, 0xA3, 0xFF))));
            }
            if self.show_labels {
                let anchor = pos2((quad[0].x + quad[1].x) / 2.0, quad[0].y.max(quad[1].y) + 4.0);
                shapes.push(painter.fonts(|f| Shape::text(f, anchor, Align2::CENTER_TOP, &e.name, FontId::proportional(11.0), label)));
            }
            items.push((depth, shapes));
        }
        self.refs.draw(cam, rect, dark, &mut items);
        items.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, shapes) in items {
            painter.extend(shapes);
        }

        // The limiting sight line and its obstruction for the selected sign (VISIBILITY).
        self.vis.paint_overlay(painter, cam, rect);
        self.photo.paint_overlay(painter, cam, rect);

        // Clash markers: a ring and a cross at each live clash.
        if self.coord.markers {
            let red = Color32::from_rgb(0xFF, 0x4D, 0x4D);
            for c in self.coord.marked() {
                if let Some((p, _)) = cam.project(c.point, rect) {
                    painter.circle_stroke(p, 9.0, Stroke::new(2.0_f32, red));
                    painter.line_segment([p + egui::vec2(-5.0, -5.0), p + egui::vec2(5.0, 5.0)], Stroke::new(2.0_f32, red));
                    painter.line_segment([p + egui::vec2(-5.0, 5.0), p + egui::vec2(5.0, -5.0)], Stroke::new(2.0_f32, red));
                    if self.coord.highlight.contains(&c.element_a) && self.coord.highlight.contains(&c.element_b) {
                        painter.text(p + egui::vec2(12.0, -10.0), Align2::LEFT_BOTTOM, &c.id, FontId::proportional(11.0), red);
                    }
                }
            }
        }
        if self.sched.fourd_on {
            if let Some(d) = self.sched.date {
                let amber = Color32::from_rgb(0xF2, 0xA9, 0x1D);
                let small = if self.by_record() { "Recorded progress at" } else { "4D date" };
                painter.text(rect.left_top() + egui::vec2(14.0, 10.0), Align2::LEFT_TOP, small, FontId::proportional(13.0), amber);
                painter.text(rect.left_top() + egui::vec2(12.0, 24.0), Align2::LEFT_TOP, format!("{d}"), FontId::proportional(40.0), amber);
                painter.text(rect.left_top() + egui::vec2(14.0, 70.0), Align2::LEFT_TOP, crate::schedule_glue::weekday_label(d.weekday()), FontId::proportional(14.0), amber);
            }
            if self.by_record() {
                // The legend: no record, then 0 % to 100 % in ten steps.
                let origin = rect.left_top() + egui::vec2(12.0, 98.0);
                let swatch = |i: usize, p: Option<f64>| {
                    let [r, g, b] = crate::construction_ui::percent_colour(p);
                    let at = origin + egui::vec2(i as f32 * 22.0, 0.0);
                    painter.rect_filled(egui::Rect::from_min_size(at, egui::vec2(20.0, 12.0)), 2.0, Color32::from_rgb(r, g, b));
                };
                swatch(0, None);
                for i in 0..=10 {
                    swatch(i + 1, Some(i as f64 * 10.0));
                }
                painter.text(origin + egui::vec2(0.0, 16.0), Align2::LEFT_TOP, "no record", FontId::proportional(10.0), label);
                painter.text(origin + egui::vec2(66.0, 16.0), Align2::LEFT_TOP, "0 %", FontId::proportional(10.0), label);
                painter.text(origin + egui::vec2(22.0 * 11.0 + 20.0, 16.0), Align2::RIGHT_TOP, "100 %", FontId::proportional(10.0), label);
            }
        }

        // World axes at the model origin.
        let len = (cam.distance * 0.06).max(100.0);
        for (dir, colour, name) in [
            ([len, 0.0, 0.0], Color32::from_rgb(0xD0, 0x50, 0x50), "X"),
            ([0.0, len, 0.0], Color32::from_rgb(0x50, 0xB0, 0x60), "Y"),
            ([0.0, 0.0, len], Color32::from_rgb(0x50, 0x80, 0xE0), "Z"),
        ] {
            let origin = if site_mode { [lo[0], lo[1], ground] } else { [0.0, 0.0, ground] };
            let tip = [origin[0] + dir[0], origin[1] + dir[1], origin[2] + dir[2]];
            line(origin, tip, Stroke::new(1.5_f32, colour));
            if let Some((p, _)) = cam.project(tip, rect) {
                painter.text(p, Align2::LEFT_BOTTOM, name, FontId::proportional(10.0), colour);
            }
        }

        painter.text(
            rect.left_bottom() + egui::vec2(10.0, -8.0),
            Align2::LEFT_BOTTOM,
            if site_mode { "3D site view  ·  drag to orbit  ·  Shift + drag or middle button to pan  ·  wheel to zoom  ·  double-click to fit" } else { "3D view of the design sheet  ·  drag to orbit  ·  Shift + drag or middle button to pan  ·  wheel to zoom  ·  double-click to fit" },
            FontId::proportional(11.0),
            label,
        );
    }
}

#[cfg(test)]
mod tests {
    // Hand-worked clipping, near plane at depth 1, view-space coordinates `[right, up, depth]`.

    #[test]
    fn a_triangle_wholly_in_front_is_unchanged_and_one_wholly_behind_is_dropped() {
        let t = [[0.0, 0.0, 3.0], [4.0, 0.0, 3.0], [2.0, 6.0, 5.0]];
        assert_eq!(clip_triangle_near(t, 1.0), vec![t]);
        let behind = [[0.0, 0.0, -3.0], [4.0, 0.0, 0.0], [2.0, 6.0, 0.5]];
        assert!(clip_triangle_near(behind, 1.0).is_empty());
    }

    #[test]
    fn one_vertex_behind_the_plane_gives_two_triangles() {
        // A (0,0,3), B (4,0,3), C (2,6,-1). Edge B to C: t = (1 - 3) / (-1 - 3) = 0.5, so the cut
        // is (4 + 0.5 (2 - 4), 0 + 0.5 (6 - 0), 1) = (3, 3, 1). Edge C to A: t = (1 + 1) / (3 + 1)
        // = 0.5, so the cut is (2 + 0.5 (0 - 2), 6 + 0.5 (0 - 6), 1) = (1, 3, 1).
        let (a, b, c) = ([0.0, 0.0, 3.0], [4.0, 0.0, 3.0], [2.0, 6.0, -1.0]);
        let out = clip_triangle_near([a, b, c], 1.0);
        let (x1, x2) = ([3.0, 3.0, 1.0], [1.0, 3.0, 1.0]);
        assert_eq!(out, vec![[a, b, x1], [a, x1, x2]]);
        // The quadrilateral A B X1 X2 has area 4 * ... : the trapezoid with parallel sides 4 (at
        // up = 0) and 2 (at up = 3), height 3, area 9. Plain area in the (right, up) plane.
        let area = |t: &[V3; 3]| ((t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[2][0] - t[0][0]) * (t[1][1] - t[0][1])).abs() / 2.0;
        assert!((out.iter().map(area).sum::<f64>() - 9.0).abs() < 1e-12);
        // Every vertex is at or in front of the plane, and the winding (sign of the area) is kept.
        assert!(out.iter().flatten().all(|v| v[2] >= 1.0 - 1e-12));
    }

    #[test]
    fn two_vertices_behind_the_plane_give_one_triangle() {
        // A (0,0,3), B (4,0,-1), C (0,8,-5). Edge A to B: t = (1 - 3) / (-1 - 3) = 0.5, so the
        // cut is (2, 0, 1). Edge C to A: t = (1 + 5) / (3 + 5) = 0.75, so the cut is
        // (0 + 0.75 * 0, 8 + 0.75 * (0 - 8), 1) = (0, 2, 1).
        let (a, b, c) = ([0.0, 0.0, 3.0], [4.0, 0.0, -1.0], [0.0, 8.0, -5.0]);
        let out = clip_triangle_near([a, b, c], 1.0);
        assert_eq!(out, vec![[a, [2.0, 0.0, 1.0], [0.0, 2.0, 1.0]]]);
    }

    #[test]
    fn a_big_slab_under_a_close_camera_still_projects_its_front_part() {
        // A camera 300 mm above the middle of a 100 m square slab, looking along the slab.
        let cam = Camera { target: [0.0, 0.0, 0.0], distance: 300.0, yaw: 0.0, pitch: 0.0 };
        let slab = [[-50_000.0, -50_000.0, 0.0], [50_000.0, -50_000.0, 0.0], [50_000.0, 50_000.0, 0.0]];
        // The eye is at x = 300 looking towards -x, so the two corners at x = 50 000 are behind it.
        let view = [cam.to_view(slab[0]), cam.to_view(slab[1]), cam.to_view(slab[2])];
        assert!(view.iter().any(|v| v[2] < NEAR), "a vertex is behind the camera");
        let pieces = clip_triangle_near(view, NEAR);
        assert!(!pieces.is_empty(), "the part in front is kept");
        let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        for t in &pieces {
            for v in t {
                assert!(v[2] >= NEAR - 1e-9);
                let p = cam.view_to_screen(*v, rect);
                assert!(p.x.is_finite() && p.y.is_finite());
            }
        }
    }

    use super::*;

    fn viewport() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0))
    }

    #[test]
    fn the_target_projects_to_the_centre_at_the_camera_distance() {
        let cam = Camera { target: [1000.0, 0.0, 500.0], distance: 8000.0, ..Camera::default() };
        let (p, depth) = cam.project(cam.target, viewport()).unwrap();
        assert!((p.x - 400.0).abs() < 1e-3 && (p.y - 300.0).abs() < 1e-3);
        assert!((depth - 8000.0).abs() < 1e-6);
    }

    #[test]
    fn up_is_up_and_points_behind_the_camera_are_rejected() {
        let cam = Camera::default();
        let (low, _) = cam.project([0.0, 0.0, 0.0], viewport()).unwrap();
        let (high, _) = cam.project([0.0, 0.0, 1000.0], viewport()).unwrap();
        assert!(high.y < low.y, "world Z must go up the screen");
        let eye = cam.eye();
        let behind = [2.0 * eye[0], 2.0 * eye[1], 2.0 * eye[2]];
        assert!(cam.project(behind, viewport()).is_none());
    }

    #[test]
    fn fit_puts_the_whole_sheet_inside_the_viewport() {
        let mut cam = Camera::default();
        let sheet = BBox::new(-500.0, -2000.0, 9000.0, 1500.0);
        cam.fit(&sheet);
        for (x, y) in [(-500.0, -2000.0), (9000.0, -2000.0), (9000.0, 1500.0), (-500.0, 1500.0)] {
            let (p, _) = cam.project(world(x, y, 0.0), viewport()).unwrap();
            assert!(viewport().contains(p), "corner {x},{y} projects outside: {p:?}");
        }
    }

    #[test]
    fn the_default_camera_looks_at_the_front_of_the_signs() {
        assert!(Camera::default().eye()[1] < 0.0, "sign faces look towards −Y");
    }
}
