use crate::{BBox, Pt};

/// Model space (mm, Y up) to screen (px, Y down). `scale` is pixels per millimetre and
/// (`origin_x`, `origin_y`) is the screen position of the model origin.
///
/// There is no fixed zoom limit. The only guards keep the numbers usable: the scale stays finite,
/// positive and non-zero, and the origin stays finite, so a zoom or pan that would break either
/// is ignored. The origin is the screen position of the model origin, so at a model position of
/// 1e10 mm and a scale of 1e6 px/mm it is about 1e16 px, where f64 resolves several pixels:
/// `to_screen` of a point is exact only to the spacing of f64 at `world * scale`. At the extremes
/// the tests exercise (see them) that is far below a pixel until the scale passes about 1e5 px/mm.
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
    /// Smallest scale the guards accept, px per mm. Not a zoom limit: it only keeps `scale` away
    /// from zero and `world * scale` away from overflow.
    pub const GUARD_MIN_SCALE: f64 = 1e-24;
    /// Largest scale the guards accept, px per mm. Not a zoom limit, see above.
    pub const GUARD_MAX_SCALE: f64 = 1e24;

    fn scale_ok(s: f64) -> bool {
        s.is_finite() && (Self::GUARD_MIN_SCALE..=Self::GUARD_MAX_SCALE).contains(&s)
    }

    pub fn to_screen(&self, world: Pt) -> Pt {
        Pt::new(self.origin_x + world.x * self.scale, self.origin_y - world.y * self.scale)
    }

    pub fn to_world(&self, screen: Pt) -> Pt {
        Pt::new((screen.x - self.origin_x) / self.scale, (self.origin_y - screen.y) / self.scale)
    }

    /// Screen to model space (millimetres). Same as [`to_world`](Self::to_world).
    pub fn to_model(&self, screen: Pt) -> Pt {
        self.to_world(screen)
    }

    pub fn box_to_world(&self, a: Pt, b: Pt) -> BBox {
        BBox::from_corners(self.to_world(a), self.to_world(b))
    }

    /// Pans by a pixel delta. A non-finite delta, or one that would make the origin non-finite,
    /// is ignored.
    pub fn pan(&mut self, dx_px: f64, dy_px: f64) {
        let (x, y) = (self.origin_x + dx_px, self.origin_y + dy_px);
        if x.is_finite() && y.is_finite() {
            self.origin_x = x;
            self.origin_y = y;
        }
    }

    /// Zooms by `factor`, keeping the model point under `screen` fixed. A factor that is not
    /// finite and positive, or that would take the scale or origin out of the guards, is ignored.
    pub fn zoom_about(&mut self, screen: Pt, factor: f64) {
        if !(factor.is_finite() && factor > 0.0) || !screen.x.is_finite() || !screen.y.is_finite() {
            return;
        }
        let anchor = self.to_world(screen);
        let scale = self.scale * factor;
        let (ox, oy) = (screen.x - anchor.x * scale, screen.y + anchor.y * scale);
        if Self::scale_ok(scale) && ox.is_finite() && oy.is_finite() {
            self.scale = scale;
            self.origin_x = ox;
            self.origin_y = oy;
        }
    }

    /// Fits `world` inside a viewport whose top-left is at screen (`left`, `top`), centred, with
    /// a pixel margin. See [`zoom_to_bounds`](Self::zoom_to_bounds).
    pub fn fit(&mut self, world: &BBox, left: f64, top: f64, width_px: f64, height_px: f64, margin_px: f64) {
        self.zoom_to_bounds(world, left, top, width_px, height_px, margin_px);
    }

    /// Zooms and centres on `world` in a viewport whose top-left is at (`left`, `top`), leaving
    /// `margin_px` free on every side. There is no limit on how far this zooms in or out. A box
    /// with no width and no height is only centred, at the current scale; a box with no width
    /// (or no height) is fitted by its other side. Does nothing for a non-finite value, or when
    /// the result would leave the guards.
    pub fn zoom_to_bounds(&mut self, world: &BBox, left: f64, top: f64, width_px: f64, height_px: f64, margin_px: f64) {
        let values = [world.min.x, world.min.y, world.max.x, world.max.y, left, top, width_px, height_px, margin_px];
        if !values.iter().all(|v| v.is_finite()) {
            return;
        }
        let avail_w = (width_px - 2.0 * margin_px).max(10.0);
        let avail_h = (height_px - 2.0 * margin_px).max(10.0);
        let (w, h) = (world.width(), world.height());
        let scale = if w <= 0.0 && h <= 0.0 {
            self.scale
        } else {
            let sx = if w > 0.0 { avail_w / w } else { f64::INFINITY };
            let sy = if h > 0.0 { avail_h / h } else { f64::INFINITY };
            sx.min(sy)
        };
        let c = world.centre();
        let (ox, oy) = (left + width_px / 2.0 - c.x * scale, top + height_px / 2.0 + c.y * scale);
        if Self::scale_ok(scale) && ox.is_finite() && oy.is_finite() {
            self.scale = scale;
            self.origin_x = ox;
            self.origin_y = oy;
        }
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
    fn zoom_keeps_the_cursor_point_fixed() {
        let mut v = ViewTransform::default();
        v.fit(&BBox::new(0.0, 0.0, 2000.0, 1000.0), 0.0, 0.0, 800.0, 600.0, 40.0);
        let cursor = Pt::new(250.0, 410.0);
        let before = v.to_world(cursor);
        v.zoom_about(cursor, 1.6);
        let after = v.to_world(cursor);
        close(before.x, after.x);
        close(before.y, after.y);
    }

    #[test]
    fn there_is_no_fixed_zoom_limit() {
        let mut v = ViewTransform::default();
        let cursor = Pt::new(400.0, 300.0);
        for _ in 0..60 {
            v.zoom_about(cursor, 2.0);
        }
        assert!(v.scale > 1e17, "{}", v.scale);
        for _ in 0..400 {
            v.zoom_about(cursor, 0.5);
        }
        assert!(v.scale.is_finite() && v.scale > 0.0);
    }

    #[test]
    fn guards_hold_scale_and_origin_finite() {
        let mut v = ViewTransform::default();
        let before = v.clone();
        for f in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            v.zoom_about(Pt::new(1.0, 1.0), f);
            assert_eq!(v, before);
        }
        v.zoom_about(Pt::new(f64::NAN, 0.0), 2.0);
        v.pan(f64::NAN, 1.0);
        v.pan(f64::INFINITY, 1.0);
        assert_eq!(v, before);
        // Runaway zoom stops at the guards instead of overflowing.
        for _ in 0..5000 {
            v.zoom_about(Pt::new(10.0, 10.0), 10.0);
        }
        assert!(v.scale.is_finite() && v.scale <= ViewTransform::GUARD_MAX_SCALE && v.origin_x.is_finite() && v.origin_y.is_finite());
        for _ in 0..5000 {
            v.zoom_about(Pt::new(10.0, 10.0), 0.1);
        }
        assert!(v.scale >= ViewTransform::GUARD_MIN_SCALE);
        // A bad box leaves the view alone.
        let keep = v.clone();
        v.zoom_to_bounds(&BBox::new(f64::NAN, 0.0, 1.0, 1.0), 0.0, 0.0, 800.0, 600.0, 10.0);
        assert_eq!(v, keep);
    }

    /// The origin of a UTM southern project: x about 5e8 mm, y about 1e10 mm.
    const FAR: Pt = Pt { x: 5.0e8, y: 1.0e10 };

    /// Round-trip error of `p` through the view, in model mm.
    fn mm_error(v: &ViewTransform, p: Pt) -> f64 {
        let back = v.to_model(v.to_screen(p));
        (back.x - p.x).abs().max((back.y - p.y).abs())
    }

    #[test]
    fn zooming_in_past_one_millimetre_over_the_screen_stays_precise_far_from_the_origin() {
        let mut v = ViewTransform::default();
        v.zoom_to_bounds(&BBox::new(FAR.x - 5000.0, FAR.y - 5000.0, FAR.x + 5000.0, FAR.y + 5000.0), 0.0, 0.0, 1000.0, 800.0, 0.0);
        let centre = Pt::new(500.0, 400.0);
        let mut passed_one_mm = false;
        while v.scale < 1e5 {
            v.zoom_about(centre, 1.5);
            // The point under the cursor stays put (the anchor property).
            let m = v.to_model(centre);
            assert!((m.x - FAR.x).abs() < 1e-3 && (m.y - FAR.y).abs() < 1e-3, "{m:?} at scale {}", v.scale);
            // A screen pixel is 1/scale mm. The round trip must stay within a tenth of a pixel.
            let tenth_pixel_mm = 0.1 / v.scale;
            assert!(mm_error(&v, FAR) < tenth_pixel_mm, "{} mm at scale {}", mm_error(&v, FAR), v.scale);
            if v.scale > 1000.0 {
                passed_one_mm = true; // 1000 px across: one model millimetre covers the screen.
            }
        }
        assert!(passed_one_mm);
        // A position 0.001 mm off the anchor is still resolved at 1e5 px/mm (0.001 mm is 100 to 150 px).
        let off = Pt::new(FAR.x + 0.001, FAR.y);
        assert!((v.to_screen(off).x - v.to_screen(FAR).x - 0.001 * v.scale).abs() < 1.0);
        // Beyond: the view keeps working and stays finite, with the anchor under the cursor.
        for _ in 0..40 {
            v.zoom_about(centre, 1.5);
        }
        assert!(v.scale.is_finite() && v.origin_x.is_finite() && v.origin_y.is_finite());
        let m = v.to_model(centre);
        assert!((m.x - FAR.x).abs() < 1e-3 && (m.y - FAR.y).abs() < 1e-3, "{m:?}");
    }

    #[test]
    fn zooming_out_until_a_ten_thousand_kilometre_extent_fits() {
        let mut v = ViewTransform::default();
        v.zoom_to_bounds(&BBox::new(FAR.x - 5000.0, FAR.y - 5000.0, FAR.x + 5000.0, FAR.y + 5000.0), 0.0, 0.0, 1000.0, 800.0, 0.0);
        let centre = Pt::new(500.0, 400.0);
        for _ in 0..80 {
            v.zoom_about(centre, 0.5);
        }
        assert!(v.scale > 0.0 && v.scale.is_finite());
        // 10 000 km = 1e10 mm across the 800 px height: 8e-8 px per mm.
        let ext = BBox::new(FAR.x - 5.0e9, FAR.y - 5.0e9, FAR.x + 5.0e9, FAR.y + 5.0e9);
        v.zoom_to_bounds(&ext, 0.0, 0.0, 1000.0, 800.0, 0.0);
        assert!((v.scale - 800.0 / 1.0e10).abs() / v.scale < 1e-12, "{}", v.scale);
        let c = v.to_screen(ext.centre());
        assert!((c.x - 500.0).abs() < 1e-3 && (c.y - 400.0).abs() < 1e-3, "{c:?}");
        // A model coordinate with millimetre detail comes back to a micrometre.
        let p = Pt::new(FAR.x + 123.456, FAR.y - 789.012);
        assert!(mm_error(&v, p) < 1e-3, "{}", mm_error(&v, p));
    }

    #[test]
    fn zoom_to_a_point_centres_it_at_the_current_scale() {
        let mut v = ViewTransform { scale: 2.0, origin_x: 10.0, origin_y: 10.0 };
        v.zoom_to_bounds(&BBox::new(300.0, 400.0, 300.0, 400.0), 0.0, 0.0, 800.0, 600.0, 60.0);
        assert_eq!(v.scale, 2.0);
        let c = v.to_screen(Pt::new(300.0, 400.0));
        close(c.x, 400.0);
        close(c.y, 300.0);
        // A line (zero height) fits by its width.
        v.zoom_to_bounds(&BBox::new(0.0, 5.0, 1000.0, 5.0), 0.0, 0.0, 800.0, 600.0, 50.0);
        close(v.scale, 0.7);
    }
}
