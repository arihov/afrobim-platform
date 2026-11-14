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

/// Vertical field of view.
const FOV: f64 = 0.61; // 35°
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
struct Frame {
    origin: V3,
    cos: f64,
    sin: f64,
}

impl Frame {
    fn at(&self, u: f64, d: f64, v: f64) -> V3 {
        [self.origin[0] + u * self.cos - d * self.sin, self.origin[1] + u * self.sin + d * self.cos, self.origin[2] + v]
    }
    /// The direction a face in the local XZ plane looks (local −Y), in the world.
    fn facing(&self) -> V3 {
        [self.sin, -self.cos, 0.0]
    }
}

impl AfroBimApp {
    /// Site mode: at least one sign has been placed on site, so the 3D view shows the site (signs
    /// at their stations, alignments on the ground) instead of the design sheet stood upright.
    pub(crate) fn site_mode(&self) -> bool {
        self.doc.elements().iter().any(|e| e.category == crate::signs_glue::CATEGORY && e.site.is_some())
    }

    /// The post a support hangs from (itself for a post) and the sign above it.
    fn support_chain<'a>(&'a self, e: &'a afrobim_kernel::Element) -> Option<(&'a afrobim_kernel::Element, &'a afrobim_kernel::Element)> {
        let post = if e.category == crate::supports_glue::POST_CATEGORY { e } else { self.doc.find(e.host?)? };
        Some((post, self.doc.find(post.host?)?))
    }

    /// The mounting height recorded on a sign's first post, or 0 for a sign without supports.
    fn mounting_height(&self, sign: afrobim_kernel::ElementId) -> f64 {
        self.doc
            .elements()
            .iter()
            .find(|e| e.category == crate::supports_glue::POST_CATEGORY && e.host == Some(sign))
            .and_then(|p| p.properties.get(crate::supports_glue::PROP_MOUNTING_HEIGHT_MM))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0)
    }

    /// The frame of a sign face (local origin at its lower-left corner), or of a support's solids
    /// (local origin at its plan corner, at ground level). None when the element is not shown in
    /// the current mode.
    fn frame_of(&self, e: &afrobim_kernel::Element, site_mode: bool) -> Option<Frame> {
        let support = crate::supports_glue::is_support(&e.category);
        if site_mode {
            let s = e.site.as_ref()?;
            let (sin, cos) = s.rotation_deg.to_radians().sin_cos();
            // A sign's site level is the ground under it; its face starts one mounting height up.
            let lift = if support { 0.0 } else { self.mounting_height(e.id) };
            return Some(Frame { origin: [s.x, s.y, s.z + lift], cos, sin });
        }
        if !support {
            return Some(Frame { origin: [e.origin.x, 0.0, e.origin.y], cos: 1.0, sin: 0.0 });
        }
        // On the upright sheet a support stands under its sign: the sign's lower edge is at its
        // sheet height, so its ground is one mounting height below.
        use crate::supports_glue::{PROP_MOUNTING_HEIGHT_MM, PROP_POSITION_ALONG_SIGN_MM, PROP_SETBACK_MM};
        let (post, sign) = self.support_chain(e)?;
        let number = |key: &str| post.properties.get(key).and_then(|v| v.parse::<f64>().ok());
        let (u, setback, mounting) = (number(PROP_POSITION_ALONG_SIGN_MM)?, number(PROP_SETBACK_MM)?, number(PROP_MOUNTING_HEIGHT_MM)?);
        let solids = &self.cache.get(&e.id)?.generated.as_ref().ok()?.solids;
        let (mut max_x, mut max_y) = (0.0f64, 0.0f64);
        for v in solids.iter().flat_map(|m| m.vertices.iter()) {
            max_x = max_x.max(v[0]);
            max_y = max_y.max(v[1]);
        }
        Some(Frame { origin: [sign.origin.x + u - max_x / 2.0, setback - max_y / 2.0, sign.origin.y - mounting], cos: 1.0, sin: 0.0 })
    }

    /// The triangles of a support's solids, flat-shaded, each with its own view depth.
    fn support_triangles(&self, e: &afrobim_kernel::Element, frame: Frame, rect: Rect, items: &mut Vec<(f64, Vec<Shape>)>) {
        let Some(Ok(generated)) = self.cache.get(&e.id).map(|c| c.generated.as_ref()) else { return };
        let cam = &self.camera;
        let light = norm([-0.35, -0.75, 0.55]);
        let selected = self.doc.is_selected(e.id);
        for mesh in &generated.solids {
            for t in &mesh.triangles {
                let p: Vec<V3> = t.iter().map(|i| mesh.vertices[*i as usize]).map(|v| frame.at(v[0], v[1], v[2])).collect();
                let normal = norm(cross(sub(p[1], p[0]), sub(p[2], p[0])));
                // Skip faces turned away from the camera: the solids are closed.
                if dot(normal, sub(cam.eye(), p[0])) <= 0.0 {
                    continue;
                }
                let projected: Option<Vec<(Pos2, f64)>> = p.iter().map(|q| cam.project(*q, rect)).collect();
                let Some(projected) = projected else { continue };
                let depth = projected.iter().map(|(_, d)| *d).sum::<f64>() / 3.0;
                let shade = (0.45 + 0.55 * dot(normal, light).max(0.0)) as f32;
                let [r, g, b] = mesh.colour;
                let mut colour = Color32::from_rgb((r as f32 * shade) as u8, (g as f32 * shade) as u8, (b as f32 * shade) as u8);
                if selected {
                    colour = Color32::from_rgb(colour.r() / 2 + 0x26, colour.g() / 2 + 0x51, colour.b() / 2 + 0x7F);
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

    /// World-space corners of everything the current mode shows, for framing and the ground grid.
    fn shown_points(&self, site_mode: bool) -> Vec<V3> {
        let mut points = Vec::new();
        for e in self.doc.elements() {
            if e.category == crate::alignment_glue::CATEGORY {
                // Frame the alignments that carry signs; one far away with nothing on it would
                // shrink the signs to a dot.
                let used = self.doc.elements().iter().any(|x| x.site.as_ref().and_then(|p| p.alignment.as_deref()) == Some(e.name.as_str()));
                if site_mode && used {
                    if let Some(a) = crate::alignment_glue::alignment_of(&self.doc, e.id) {
                        points.extend(a.polyline(50.0).into_iter().map(|(x, y)| [x * 1000.0, y * 1000.0, 0.0]));
                    }
                }
                continue;
            }
            let Some(frame) = self.frame_of(e, site_mode) else { continue };
            if crate::supports_glue::is_support(&e.category) {
                points.push(frame.at(0.0, 0.0, 0.0));
            } else {
                points.extend([frame.at(0.0, 0.0, 0.0), frame.at(e.width, 0.0, e.height)]);
            }
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
        }

        self.sync_generated();
        self.paint_3d(&painter, rect);
    }

    pub(crate) fn fit_3d(&mut self) {
        self.sync_generated();
        if let Some(mark) = self.options.focus.clone() {
            let site_mode = self.site_mode();
            if let Some((frame, e)) = self.doc.elements().iter().find(|e| e.name == mark).and_then(|e| self.frame_of(e, site_mode).map(|f| (f, e))) {
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
        let ground = if site_mode { 0.0 } else { lo[2] - 300.0 };
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
                        let (start, end) = (a.start_station_m, a.end_station_m());
                        let steps = (((end - start) / 5.0).ceil() as usize).clamp(1, 20_000);
                        let pts: Vec<[f64; 3]> = (0..=steps)
                            .filter_map(|i| {
                                let s = start + (end - start) * i as f64 / steps as f64;
                                let p = a.point_at(s)?;
                                // Beyond the profile's ends the line is shown at the end level, so
                                // that it does not drop to zero.
                                let z = a
                                    .profile
                                    .as_ref()
                                    .and_then(|profile| profile.range().and_then(|(lo, hi)| profile.elevation_at(s.clamp(lo, hi)).ok()))
                                    .unwrap_or(0.0);
                                Some([p.x * 1000.0, p.y * 1000.0, z * 1000.0])
                            })
                            .collect();
                        for w in pts.windows(2) {
                            line(w[0], w[1], centre);
                        }
                    }
                }
                continue;
            }
            if !self.doc.layer_visible(&e.layer) {
                continue;
            }
            let Some(frame) = self.frame_of(e, site_mode) else { continue };
            if crate::supports_glue::is_support(&e.category) {
                self.support_triangles(e, frame, rect, &mut items);
                continue;
            }
            let centre = frame.at(e.width / 2.0, 0.0, e.height / 2.0);
            let Some((_, depth)) = cam.project(centre, rect) else { continue };
            let corners = [(0.0, 0.0), (e.width, 0.0), (e.width, e.height), (0.0, e.height)];
            let quad: Option<Vec<Pos2>> = corners.iter().map(|(u, v)| cam.project(frame.at(*u, 0.0, *v), rect).map(|(p, _)| p)).collect();
            let Some(quad) = quad else { continue };
            let mut shapes = Vec::new();

            let from_front = dot(frame.facing(), sub(eye, centre)) > 0.0;
            let artwork = self.cache.get(&e.id).filter(|c| c.generated.is_ok()).filter(|_| from_front);
            match artwork {
                Some(cached) => {
                    // The face, just in front of the plate.
                    let place = |u: f64, v: f64| cam.project(frame.at(u, -0.5, v), rect).map(|(p, _)| p);
                    if let Some(shape) = cached.mesh.projected(&place) {
                        shapes.push(shape);
                    }
                }
                // Seen from behind, or without geometry: the bare plate.
                None => shapes.push(Shape::convex_polygon(quad.clone(), plate, Stroke::NONE)),
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
        items.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, shapes) in items {
            painter.extend(shapes);
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
