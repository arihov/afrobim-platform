// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
use crate::{BBox, Pt};

/// Model space (mm, Y up) to screen (px, Y down).
///
/// The view holds one model point, the *anchor* (`anchor_x`, `anchor_y`, millimetres, f64), and
/// the screen position of that point (`screen_x`, `screen_y`, pixels). A screen position is
/// computed from the *difference* between a model position and the anchor, never from the model
/// position itself:
///
/// ```text
/// screen = screen_anchor + (world - anchor) * scale
/// ```
///
/// so precision does not depend on how far the model is from (0, 0). A zoom about the cursor
/// makes the model point under the cursor the new anchor; a pan moves the screen position of the
/// anchor and, once that has wandered 1e5 px, moves the anchor instead. The only rounding is the
/// f64 spacing at the model coordinate: about 2e-6 mm at 1e10 mm, 6e-8 mm at 5e8 mm, which is the
/// limit of f64 at that position and no property of this type.
///
/// There is no fixed zoom limit. The only guards keep the numbers usable: the scale stays finite,
/// positive and inside 1e-24 to 1e24 px per mm, and every stored number stays finite, so a zoom
/// or pan that would break either is ignored.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewTransform {
    /// Pixels per millimetre.
    pub scale: f64,
    /// Model position (mm) of the anchor.
    pub anchor_x: f64,
    pub anchor_y: f64,
    /// Screen position (px) of the anchor.
    pub screen_x: f64,
    pub screen_y: f64,
    /// Degrees the model is turned anticlockwise on the screen about the anchor (the twist of a
    /// layout viewport). Zero for the model tab.
    pub twist_deg: f64,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self { scale: 0.1, anchor_x: 0.0, anchor_y: 0.0, screen_x: 0.0, screen_y: 0.0, twist_deg: 0.0 }
    }
}

impl ViewTransform {
    /// Smallest scale the guards accept, px per mm. Not a zoom limit: it only keeps `scale` away
    /// from zero and `world * scale` away from overflow.
    pub const GUARD_MIN_SCALE: f64 = 1e-24;
    /// Largest scale the guards accept, px per mm. Not a zoom limit, see above.
    pub const GUARD_MAX_SCALE: f64 = 1e24;
    /// How far (px) the screen position of the anchor may wander before a pan moves the anchor.
    pub const RENORMALISE_PX: f64 = 1.0e5;

    fn scale_ok(s: f64) -> bool {
        s.is_finite() && (Self::GUARD_MIN_SCALE..=Self::GUARD_MAX_SCALE).contains(&s)
    }

    /// A view with the model origin at screen (`origin_x`, `origin_y`) and `scale` px per mm.
    pub fn from_origin(scale: f64, origin_x: f64, origin_y: f64) -> Self {
        Self { scale, anchor_x: 0.0, anchor_y: 0.0, screen_x: origin_x, screen_y: origin_y, twist_deg: 0.0 }
    }

    /// A view that shows model point `centre` at screen position `at`, at `scale` px per mm.
    pub fn centred_on(centre: Pt, at: Pt, scale: f64) -> Self {
        Self { scale, anchor_x: centre.x, anchor_y: centre.y, screen_x: at.x, screen_y: at.y, twist_deg: 0.0 }
    }

    /// Screen position (px) of the model origin. Only a convenience: far from the origin this is
    /// a large number with f64's spacing at that size, so use [`to_screen`](Self::to_screen) for
    /// anything that must stay exact.
    pub fn origin_x(&self) -> f64 {
        self.screen_x - self.anchor_x * self.scale
    }
    pub fn origin_y(&self) -> f64 {
        self.screen_y + self.anchor_y * self.scale
    }

    /// The model point (mm) the view is anchored on.
    pub fn anchor(&self) -> Pt {
        Pt::new(self.anchor_x, self.anchor_y)
    }

    /// The same view with the model turned `deg` degrees anticlockwise about the anchor.
    pub fn with_twist(mut self, deg: f64) -> Self {
        self.twist_deg = if deg.is_finite() { deg } else { 0.0 };
        self
    }

    /// Turns the vector (x, y) by the twist (`sign` 1) or back (`sign` -1).
    fn turned(&self, x: f64, y: f64, sign: f64) -> (f64, f64) {
        if self.twist_deg == 0.0 {
            return (x, y);
        }
        let (s, c) = (sign * self.twist_deg).to_radians().sin_cos();
        (x * c - y * s, x * s + y * c)
    }

    pub fn to_screen(&self, world: Pt) -> Pt {
        let (dx, dy) = self.turned((world.x - self.anchor_x) * self.scale, (world.y - self.anchor_y) * self.scale, 1.0);
        Pt::new(self.screen_x + dx, self.screen_y - dy)
    }

    /// Screen position of the point `local` (mm) from `origin` (model mm). The difference between
    /// `origin` and the anchor is taken first and `local` added to it after, so a mesh held in
    /// element-local numbers is placed without ever adding a small number to a large one.
    pub fn to_screen_local(&self, origin: Pt, local_x: f64, local_y: f64) -> Pt {
        let (dx, dy) = self.turned(((origin.x - self.anchor_x) + local_x) * self.scale, ((origin.y - self.anchor_y) + local_y) * self.scale, 1.0);
        Pt::new(self.screen_x + dx, self.screen_y - dy)
    }

    pub fn to_world(&self, screen: Pt) -> Pt {
        let (dx, dy) = self.turned(screen.x - self.screen_x, self.screen_y - screen.y, -1.0);
        Pt::new(self.anchor_x + dx / self.scale, self.anchor_y + dy / self.scale)
    }

    /// Screen to model space (millimetres). Same as [`to_world`](Self::to_world).
    pub fn to_model(&self, screen: Pt) -> Pt {
        self.to_world(screen)
    }

    pub fn box_to_world(&self, a: Pt, b: Pt) -> BBox {
        BBox::from_corners(self.to_world(a), self.to_world(b))
    }

    /// Pans by a pixel delta. A non-finite delta, or one that would make a stored number
    /// non-finite, is ignored.
    pub fn pan(&mut self, dx_px: f64, dy_px: f64) {
        let (x, y) = (self.screen_x + dx_px, self.screen_y + dy_px);
        if !(x.is_finite() && y.is_finite()) {
            return;
        }
        if x.abs() > Self::RENORMALISE_PX || y.abs() > Self::RENORMALISE_PX {
            // Move the anchor to the model point at screen (0, 0) so the pixel numbers stay small.
            let w = ViewTransform { screen_x: x, screen_y: y, ..self.clone() }.to_world(Pt::new(0.0, 0.0));
            let (ax, ay) = (w.x, w.y);
            if ax.is_finite() && ay.is_finite() {
                self.anchor_x = ax;
                self.anchor_y = ay;
                self.screen_x = 0.0;
                self.screen_y = 0.0;
            }
            return;
        }
        self.screen_x = x;
        self.screen_y = y;
    }

    /// Zooms by `factor`, keeping the model point under `screen` fixed. A factor that is not
    /// finite and positive, or that would take the scale out of the guards, is ignored.
    pub fn zoom_about(&mut self, screen: Pt, factor: f64) {
        if !(factor.is_finite() && factor > 0.0) || !screen.x.is_finite() || !screen.y.is_finite() {
            return;
        }
        let scale = self.scale * factor;
        if !Self::scale_ok(scale) {
            return;
        }
        let anchor = self.to_world(screen);
        if anchor.x.is_finite() && anchor.y.is_finite() {
            self.scale = scale;
            self.anchor_x = anchor.x;
            self.anchor_y = anchor.y;
            self.screen_x = screen.x;
            self.screen_y = screen.y;
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
        if Self::scale_ok(scale) && c.x.is_finite() && c.y.is_finite() {
            self.scale = scale;
            self.anchor_x = c.x;
            self.anchor_y = c.y;
            self.screen_x = left + width_px / 2.0;
            self.screen_y = top + height_px / 2.0;
        }
    }

    /// Zooms to the model rectangle `world` as a zoom window: fits it with no margin.
    pub fn zoom_window(&mut self, world: &BBox, left: f64, top: f64, width_px: f64, height_px: f64) {
        self.zoom_to_bounds(world, left, top, width_px, height_px, 0.0);
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
        assert!(v.scale.is_finite() && v.scale <= ViewTransform::GUARD_MAX_SCALE && v.screen_x.is_finite() && v.screen_y.is_finite() && v.anchor_x.is_finite() && v.anchor_y.is_finite());
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
        assert!(v.scale.is_finite() && v.screen_x.is_finite() && v.screen_y.is_finite() && v.anchor_x.is_finite() && v.anchor_y.is_finite());
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
        let mut v = ViewTransform::from_origin(2.0, 10.0, 10.0);
        v.zoom_to_bounds(&BBox::new(300.0, 400.0, 300.0, 400.0), 0.0, 0.0, 800.0, 600.0, 60.0);
        assert_eq!(v.scale, 2.0);
        let c = v.to_screen(Pt::new(300.0, 400.0));
        close(c.x, 400.0);
        close(c.y, 300.0);
        // A line (zero height) fits by its width.
        v.zoom_to_bounds(&BBox::new(0.0, 5.0, 1000.0, 5.0), 0.0, 0.0, 800.0, 600.0, 50.0);
        close(v.scale, 0.7);
    }

    #[test]
    fn pan_far_from_the_origin_is_exact_and_renormalises() {
        let mut v = ViewTransform::default();
        v.zoom_to_bounds(&BBox::new(FAR.x - 1.0, FAR.y - 1.0, FAR.x + 1.0, FAR.y + 1.0), 0.0, 0.0, 1000.0, 800.0, 0.0);
        let p = Pt::new(FAR.x + 0.25, FAR.y - 0.125);
        let before = v.to_screen(p);
        // Pan by whole pixels forth and back, across the renormalisation threshold.
        for _ in 0..30 {
            v.pan(5000.0, -3000.0);
        }
        for _ in 0..30 {
            v.pan(-5000.0, 3000.0);
        }
        let after = v.to_screen(p);
        // One pixel is 1/scale mm (about 1e-3 mm here); the round trip stays within 1e-3 px.
        assert!((after.x - before.x).abs() < 1e-3 && (after.y - before.y).abs() < 1e-3, "{before:?} {after:?}");
        assert!(v.screen_x.abs() <= ViewTransform::RENORMALISE_PX * 2.0);
    }

    #[test]
    fn local_placement_keeps_sub_millimetre_detail_at_huge_scale() {
        // 1 mm fills a 1000 px screen: scale 1000 px/mm, far from the origin.
        let mut v = ViewTransform::default();
        v.zoom_to_bounds(&BBox::new(FAR.x, FAR.y, FAR.x + 1.0, FAR.y + 1.0), 0.0, 0.0, 1000.0, 1000.0, 0.0);
        let origin = Pt::new(FAR.x - 0.5, FAR.y - 0.5);
        // A local vertex 1 mm from the element origin, against the direct route.
        let s = v.to_screen_local(origin, 1.0, 1.0);
        let d = v.to_screen(Pt::new(FAR.x + 0.5, FAR.y + 0.5));
        assert!((s.x - d.x).abs() < 1e-6 && (s.y - d.y).abs() < 1e-6, "{s:?} {d:?}");
        assert!((s.x - 500.0).abs() < 1e-6, "the box centre is at the screen centre");
    }

    #[test]
    fn the_f64_limit_at_1e10_mm_is_about_two_micrometres_in_a_millionth() {
        let spacing = f64::EPSILON * 1.0e10;
        assert!(spacing < 4.0e-6 && spacing > 1.0e-6, "{spacing}");
        // The view resolves a 1e-5 mm step at y = 1e10 mm to within the f64 spacing.
        let mut v = ViewTransform::default();
        v.zoom_to_bounds(&BBox::new(FAR.x - 1e-3, FAR.y - 1e-3, FAR.x + 1e-3, FAR.y + 1e-3), 0.0, 0.0, 1000.0, 1000.0, 0.0);
        let a = v.to_screen(FAR);
        let b = v.to_screen(Pt::new(FAR.x, FAR.y + 1.0e-5));
        let px = (a.y - b.y) / v.scale;
        assert!((px - 1.0e-5).abs() <= spacing, "{px}");
    }

    #[test]
    fn a_twisted_view_round_trips_and_turns_the_model_anticlockwise() {
        for deg in [0.0, 30.0, 90.0, -135.0] {
            let v = ViewTransform::centred_on(Pt::new(5000.0, -2000.0), Pt::new(400.0, 300.0), 0.2).with_twist(deg);
            for p in [Pt::new(5000.0, -2000.0), Pt::new(9000.0, 1000.0), Pt::new(-30.0, 77.0)] {
                let back = v.to_world(v.to_screen(p));
                close(back.x, p.x);
                close(back.y, p.y);
            }
            let s = v.to_screen(Pt::new(5000.0, -2000.0));
            close(s.x, 400.0);
            close(s.y, 300.0);
        }
        // 90 degrees: a point east of the anchor is shown above it (north), the screen's y going down.
        let v = ViewTransform::centred_on(Pt::new(0.0, 0.0), Pt::new(100.0, 100.0), 1.0).with_twist(90.0);
        let s = v.to_screen(Pt::new(10.0, 0.0));
        close(s.x, 100.0);
        close(s.y, 90.0);
        // to_screen_local agrees with to_screen.
        let (a, b) = (v.to_screen(Pt::new(13.0, 4.0)), v.to_screen_local(Pt::new(10.0, 0.0), 3.0, 4.0));
        close(a.x, b.x);
        close(a.y, b.y);
    }

    #[test]
    fn zoom_and_a_far_pan_keep_a_twisted_view_consistent() {
        let mut v = ViewTransform::centred_on(Pt::new(1000.0, 1000.0), Pt::new(200.0, 200.0), 0.5).with_twist(30.0);
        let cursor = Pt::new(250.0, 180.0);
        let before = v.to_world(cursor);
        v.zoom_about(cursor, 1.7);
        let after = v.to_world(cursor);
        close(before.x, after.x);
        close(before.y, after.y);
        let p = Pt::new(1234.0, 987.0);
        let at = v.to_screen(p);
        v.pan(2.0e5, -3.0e5); // beyond the renormalise limit: the anchor moves
        let moved = v.to_screen(p);
        close(moved.x, at.x + 2.0e5);
        close(moved.y, at.y - 3.0e5);
    }
}
