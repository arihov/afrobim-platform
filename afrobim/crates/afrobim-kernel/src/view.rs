use crate::{BBox, Pt};

/// Model space (mm, Y up) to screen (px, Y down). `scale` is pixels per millimetre and
/// (`origin_x`, `origin_y`) is the screen position of the model origin.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewTransform {
    pub scale: f64,
    pub origin_x: f64,
    pub origin_y: f64,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self { scale: 0.1, origin_x: 0.0, origin_y: 0.0 }
    }
}

impl ViewTransform {
    /// 1 px = 2 m.
    pub const MIN_SCALE: f64 = 0.0005;
    /// 1 mm = 50 px.
    pub const MAX_SCALE: f64 = 50.0;

    pub fn to_screen(&self, world: Pt) -> Pt {
        Pt::new(self.origin_x + world.x * self.scale, self.origin_y - world.y * self.scale)
    }

    pub fn to_world(&self, screen: Pt) -> Pt {
        Pt::new((screen.x - self.origin_x) / self.scale, (self.origin_y - screen.y) / self.scale)
    }

    pub fn box_to_world(&self, a: Pt, b: Pt) -> BBox {
        BBox::from_corners(self.to_world(a), self.to_world(b))
    }

    pub fn pan(&mut self, dx_px: f64, dy_px: f64) {
        self.origin_x += dx_px;
        self.origin_y += dy_px;
    }

    /// Zooms by `factor`, keeping the model point under `screen` fixed.
    pub fn zoom_about(&mut self, screen: Pt, factor: f64) {
        if !(factor > 0.0) {
            return;
        }
        let anchor = self.to_world(screen);
        self.scale = (self.scale * factor).clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        self.origin_x = screen.x - anchor.x * self.scale;
        self.origin_y = screen.y + anchor.y * self.scale;
    }

    /// Fits `world` inside a viewport whose top-left is at screen (`left`, `top`), centred, with
    /// a pixel margin.
    pub fn fit(&mut self, world: &BBox, left: f64, top: f64, width_px: f64, height_px: f64, margin_px: f64) {
        let w = world.width().max(1e-6);
        let h = world.height().max(1e-6);
        let avail_w = (width_px - 2.0 * margin_px).max(10.0);
        let avail_h = (height_px - 2.0 * margin_px).max(10.0);
        self.scale = (avail_w / w).min(avail_h / h).clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        let c = world.centre();
        self.origin_x = left + width_px / 2.0 - c.x * self.scale;
        self.origin_y = top + height_px / 2.0 + c.y * self.scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "{a} != {b}");
    }

    #[test]
    fn round_trips_and_flips_y() {
        let mut v = ViewTransform::default();
        v.fit(&BBox::new(0.0, 0.0, 2000.0, 1000.0), 0.0, 0.0, 800.0, 600.0, 40.0);
        let p = Pt::new(123.4, 567.8);
        let back = v.to_world(v.to_screen(p));
        close(back.x, p.x);
        close(back.y, p.y);
        assert!(v.to_screen(Pt::new(0.0, 1000.0)).y < v.to_screen(Pt::new(0.0, 0.0)).y);
    }

    #[test]
    fn fit_centres_the_box_inside_an_offset_viewport() {
        let mut v = ViewTransform::default();
        v.fit(&BBox::new(0.0, 0.0, 2000.0, 1000.0), 100.0, 50.0, 800.0, 600.0, 40.0);
        close(v.scale, 0.36); // (800 - 80) / 2000
        let c = v.to_screen(Pt::new(1000.0, 500.0));
        close(c.x, 500.0);
        close(c.y, 350.0);
    }

    #[test]
    fn zoom_keeps_the_cursor_point_fixed_and_is_clamped() {
        let mut v = ViewTransform::default();
        v.fit(&BBox::new(0.0, 0.0, 2000.0, 1000.0), 0.0, 0.0, 800.0, 600.0, 40.0);
        let cursor = Pt::new(250.0, 410.0);
        let before = v.to_world(cursor);
        v.zoom_about(cursor, 1.6);
        let after = v.to_world(cursor);
        close(before.x, after.x);
        close(before.y, after.y);
        for _ in 0..200 {
            v.zoom_about(cursor, 2.0);
        }
        assert_eq!(v.scale, ViewTransform::MAX_SCALE);
        for _ in 0..400 {
            v.zoom_about(cursor, 0.5);
        }
        assert_eq!(v.scale, ViewTransform::MIN_SCALE);
    }
}
