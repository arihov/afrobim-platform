//! The 3D view: the model's elements as upright plates, seen through an orbit camera.
//!
//! The sheet is mapped to the world the same way IFC export does it: sheet X → world X, sheet Y →
//! world Z (up), each sign face standing in the XZ plane and facing −Y. This is a viewing
//! convention until elements carry a real site placement.
//!
//! Drawing is a software projection onto the egui painter (no GPU code of its own): elements are
//! flat plates, so sorting them back to front is enough for a correct picture.

use afrobim_kernel::{BBox, ElementId};
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
        self.distance = (self.distance / factor).clamp(50.0, 5.0e6);
    }

    /// Frames a sheet-space box (standing in the XZ plane) with a margin.
    pub fn fit(&mut self, sheet: &BBox) {
        let c = sheet.centre();
        self.target = [c.x, 0.0, c.y];
        let radius = (sheet.width().powi(2) + sheet.height().powi(2)).sqrt() / 2.0;
        self.distance = (radius.max(500.0) / (FOV / 2.0).sin() * 1.05).clamp(50.0, 5.0e6);
    }
}

/// Sheet point (x, y) on the plane at depth `y_world` → world.
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
        let ext = self.doc.extents().unwrap_or(BBox::new(0.0, 0.0, 5000.0, 3000.0));
        self.camera.fit(&ext);
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

        // Ground grid just below the lowest element: a depth cue, not a surveyed surface.
        let ext = self.doc.extents().unwrap_or(BBox::new(0.0, 0.0, 5000.0, 3000.0));
        let ground = ext.min.y - 300.0;
        let step = 1000.0;
        let (x0, x1) = (((ext.min.x - 2000.0) / step).floor() * step, ((ext.max.x + 2000.0) / step).ceil() * step);
        let (y0, y1) = (-4000.0, 4000.0);
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

        // Elements, farthest first.
        let eye = cam.eye();
        let from_front = eye[1] < 0.0;
        let mut order: Vec<(f64, ElementId)> = self
            .doc
            .elements()
            .iter()
            .filter(|e| self.doc.layer_visible(&e.layer))
            .filter_map(|e| {
                let c = e.bounds().centre();
                cam.project(world(c.x, c.y, 0.0), rect).map(|(_, depth)| (depth, e.id))
            })
            .collect();
        order.sort_by(|a, b| b.0.total_cmp(&a.0));

        let plate = if dark { Color32::from_rgb(0x8C, 0x93, 0x9B) } else { Color32::from_rgb(0x9A, 0xA1, 0xA8) };
        let label = if dark { Color32::from_rgb(0xA8, 0xB0, 0xBA) } else { Color32::from_rgb(0x50, 0x55, 0x5C) };
        for (_, id) in order {
            let Some(e) = self.doc.find(id) else { continue };
            let b = e.bounds();
            let corners = [(b.min.x, b.min.y), (b.max.x, b.min.y), (b.max.x, b.max.y), (b.min.x, b.max.y)];
            let quad: Option<Vec<Pos2>> = corners.iter().map(|(x, y)| cam.project(world(*x, *y, 0.0), rect).map(|(p, _)| p)).collect();
            let Some(quad) = quad else { continue };

            let artwork = self.cache.get(&id).filter(|c| c.generated.is_ok()).filter(|_| from_front);
            match artwork {
                Some(cached) => {
                    // The face, just in front of the plate.
                    let place = |x: f64, y: f64| cam.project(world(e.origin.x + x, e.origin.y + y, -0.5), rect).map(|(p, _)| p);
                    if let Some(shape) = cached.mesh.projected(&place) {
                        painter.add(shape);
                    }
                }
                // Seen from behind, or without geometry: the bare plate.
                None => {
                    painter.add(Shape::convex_polygon(quad.clone(), plate, Stroke::NONE));
                }
            }
            if self.doc.is_selected(id) {
                painter.add(Shape::closed_line(quad.clone(), Stroke::new(2.0_f32, Color32::from_rgb(0x4D, 0xA3, 0xFF))));
            }
            if self.show_labels {
                let anchor = pos2((quad[0].x + quad[1].x) / 2.0, quad[0].y.max(quad[1].y) + 4.0);
                painter.text(anchor, Align2::CENTER_TOP, &e.name, FontId::proportional(11.0), label);
            }
        }

        // World axes at the model origin.
        let len = (cam.distance * 0.06).max(100.0);
        for (dir, colour, name) in [
            ([len, 0.0, 0.0], Color32::from_rgb(0xD0, 0x50, 0x50), "X"),
            ([0.0, len, 0.0], Color32::from_rgb(0x50, 0xB0, 0x60), "Y"),
            ([0.0, 0.0, len], Color32::from_rgb(0x50, 0x80, 0xE0), "Z"),
        ] {
            let origin = [0.0, 0.0, ground];
            let tip = [origin[0] + dir[0], origin[1] + dir[1], origin[2] + dir[2]];
            line(origin, tip, Stroke::new(1.5_f32, colour));
            if let Some((p, _)) = cam.project(tip, rect) {
                painter.text(p, Align2::LEFT_BOTTOM, name, FontId::proportional(10.0), colour);
            }
        }

        painter.text(
            rect.left_bottom() + egui::vec2(10.0, -8.0),
            Align2::LEFT_BOTTOM,
            "3D view  ·  drag to orbit  ·  Shift + drag or middle button to pan  ·  wheel to zoom  ·  double-click to fit",
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
