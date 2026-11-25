// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Flattening rasters onto the paper: map tiles, georeferenced images and anything else that is a
//! picture is resampled into one upright RGBA raster at the paper's resolution, cut to the
//! viewport boundary, and handed to the PDF writer as a single image.
//!
//! The caller says where the corners of a source picture land on the paper (a viewport maps the
//! model to the paper, with its scale and twist), so a twisted viewport, a reprojected image and a
//! plain one all go through the same two triangles per cell. Nothing here knows about viewports.

use afrobim_draw::Pt2;
use afrobim_kernel::layout::point_in_polygon;

use crate::geom::Rect;
use crate::item::SheetImage;

/// The pixels of a source picture: `rgba`, row 0 at the top, straight (not premultiplied) alpha.
#[derive(Clone, Copy)]
pub struct Texture<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

impl Texture<'_> {
    /// Bilinear sample at (u, v) in 0..1 across the picture, v down. Edges are clamped.
    fn sample(&self, u: f64, v: f64) -> [f32; 4] {
        let (w, h) = (self.width as usize, self.height as usize);
        let x = (u * w as f64 - 0.5).clamp(0.0, (w - 1) as f64);
        let y = (v * h as f64 - 0.5).clamp(0.0, (h - 1) as f64);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
        let px = |xx: usize, yy: usize| {
            let i = (yy * w + xx) * 4;
            [self.rgba[i] as f32, self.rgba[i + 1] as f32, self.rgba[i + 2] as f32, self.rgba[i + 3] as f32]
        };
        let (a, b, c, d) = (px(x0, y0), px(x1, y0), px(x0, y1), px(x1, y1));
        let mut out = [0.0f32; 4];
        for k in 0..4 {
            let top = a[k] + (b[k] - a[k]) * fx;
            let bot = c[k] + (d[k] - c[k]) * fx;
            out[k] = top + (bot - top) * fy;
        }
        out
    }
}

/// A raster the size of a paper rectangle, built up by laying pictures on it from the bottom up.
pub struct PaperRaster {
    rect: Rect,
    width: u32,
    height: u32,
    /// Pixels per paper mm across and down: the pixel grid covers the rectangle exactly.
    ppm: (f64, f64),
    rgba: Vec<u8>,
    touched: bool,
    /// Which pixels the current picture has already painted, so a seam between two triangles of
    /// one picture is not blended twice.
    stamp: Vec<u32>,
    generation: u32,
}

impl PaperRaster {
    /// A blank (transparent) raster over `rect` at `dpi`, no longer than `max_px` on a side (the
    /// resolution drops to fit). `None` for an empty or non-finite rectangle.
    pub fn new(rect: &Rect, dpi: f64, max_px: u32) -> Option<PaperRaster> {
        let (w, h) = (rect.width(), rect.height());
        if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 && dpi > 0.0) {
            return None;
        }
        let mut ppm = dpi / 25.4;
        let longest = w.max(h) * ppm;
        if longest > max_px as f64 {
            ppm *= max_px as f64 / longest;
        }
        let (pw, ph) = (((w * ppm).ceil() as u32).max(1), ((h * ppm).ceil() as u32).max(1));
        Some(PaperRaster { rect: *rect, width: pw, height: ph, ppm: (pw as f64 / w, ph as f64 / h), rgba: vec![0; pw as usize * ph as usize * 4], touched: false, stamp: vec![0; pw as usize * ph as usize], generation: 0 })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The pixel (x right, y down, fractional) of a paper point.
    fn px(&self, p: Pt2) -> (f64, f64) {
        ((p.x - self.rect.x0) * self.ppm.0, (self.rect.y1 - p.y) * self.ppm.1)
    }

    /// The paper point at the middle of pixel (x, y).
    fn paper_at(&self, x: u32, y: u32) -> Pt2 {
        Pt2::new(self.rect.x0 + (x as f64 + 0.5) / self.ppm.0, self.rect.y1 - (y as f64 + 0.5) / self.ppm.1)
    }

    /// True when nothing has been laid on it.
    pub fn is_blank(&self) -> bool {
        !self.touched
    }

    /// Lays a picture whose four corners land on `quad` (paper mm) in the order top left, top
    /// right, bottom right, bottom left of the picture, at `opacity` (0 to 1).
    pub fn blit_quad(&mut self, quad: [Pt2; 4], tex: Texture, opacity: f32) {
        self.generation += 1;
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        self.triangle([quad[0], quad[1], quad[2]], [uv[0], uv[1], uv[2]], tex, opacity);
        self.triangle([quad[0], quad[2], quad[3]], [uv[0], uv[2], uv[3]], tex, opacity);
    }

    /// Lays a picture placed on a grid of `(n + 1) * (n + 1)` paper points, row by row from the top
    /// left (what a reprojected image's placement is), showing the part `uv` (u0, v0, u1, v1) of it.
    pub fn blit_grid(&mut self, grid: &[Pt2], n: usize, uv: [f32; 4], tex: Texture, opacity: f32) {
        if n == 0 || grid.len() != (n + 1) * (n + 1) {
            return;
        }
        self.generation += 1;
        let at = |i: usize, j: usize| grid[j * (n + 1) + i];
        let uv_at = |i: usize, j: usize| [(uv[0] + (uv[2] - uv[0]) * i as f32 / n as f32) as f64, (uv[1] + (uv[3] - uv[1]) * j as f32 / n as f32) as f64];
        for j in 0..n {
            for i in 0..n {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                let (ua, ub, uc, ud) = (uv_at(i, j), uv_at(i + 1, j), uv_at(i + 1, j + 1), uv_at(i, j + 1));
                self.triangle([a, b, c], [ua, ub, uc], tex, opacity);
                self.triangle([a, c, d], [ua, uc, ud], tex, opacity);
            }
        }
    }

    fn triangle(&mut self, p: [Pt2; 3], uv: [[f64; 2]; 3], tex: Texture, opacity: f32) {
        if tex.width == 0 || tex.height == 0 || tex.rgba.len() < tex.width as usize * tex.height as usize * 4 {
            return;
        }
        let s = [self.px(p[0]), self.px(p[1]), self.px(p[2])];
        if s.iter().any(|q| !(q.0.is_finite() && q.1.is_finite())) {
            return;
        }
        let den = (s[1].1 - s[2].1) * (s[0].0 - s[2].0) + (s[2].0 - s[1].0) * (s[0].1 - s[2].1);
        if den.abs() < 1e-12 {
            return;
        }
        let (x0, x1) = (s.iter().map(|q| q.0).fold(f64::MAX, f64::min), s.iter().map(|q| q.0).fold(f64::MIN, f64::max));
        let (y0, y1) = (s.iter().map(|q| q.1).fold(f64::MAX, f64::min), s.iter().map(|q| q.1).fold(f64::MIN, f64::max));
        if x1 < 0.0 || y1 < 0.0 || x0 > self.width as f64 || y0 > self.height as f64 {
            return;
        }
        let (ix0, ix1) = (x0.floor().max(0.0) as u32, (x1.ceil().min(self.width as f64)) as u32);
        let (iy0, iy1) = (y0.floor().max(0.0) as u32, (y1.ceil().min(self.height as f64)) as u32);
        const EPS: f64 = -1e-7;
        for y in iy0..iy1.min(self.height) {
            for x in ix0..ix1.min(self.width) {
                let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
                let l0 = ((s[1].1 - s[2].1) * (cx - s[2].0) + (s[2].0 - s[1].0) * (cy - s[2].1)) / den;
                let l1 = ((s[2].1 - s[0].1) * (cx - s[2].0) + (s[0].0 - s[2].0) * (cy - s[2].1)) / den;
                let l2 = 1.0 - l0 - l1;
                if l0 < EPS || l1 < EPS || l2 < EPS {
                    continue;
                }
                let u = l0 * uv[0][0] + l1 * uv[1][0] + l2 * uv[2][0];
                let v = l0 * uv[0][1] + l1 * uv[1][1] + l2 * uv[2][1];
                let c = tex.sample(u, v);
                let sa = c[3] / 255.0 * opacity.clamp(0.0, 1.0);
                if sa <= 0.0 {
                    continue;
                }
                let pix = y as usize * self.width as usize + x as usize;
                if self.stamp[pix] == self.generation {
                    continue;
                }
                self.stamp[pix] = self.generation;
                let i = pix * 4;
                let da = self.rgba[i + 3] as f32 / 255.0;
                let oa = sa + da * (1.0 - sa);
                for (k, src) in c.iter().take(3).enumerate() {
                    let d = self.rgba[i + k] as f32;
                    self.rgba[i + k] = ((src * sa + d * da * (1.0 - sa)) / oa).round().clamp(0.0, 255.0) as u8;
                }
                self.rgba[i + 3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
                self.touched = true;
            }
        }
    }

    /// Makes everything outside the polygon (paper mm) transparent.
    pub fn mask_outside(&mut self, poly: &[Pt2]) {
        if poly.len() < 3 {
            return;
        }
        for y in 0..self.height {
            for x in 0..self.width {
                let i = (y as usize * self.width as usize + x as usize) * 4;
                if self.rgba[i + 3] != 0 && !point_in_polygon(poly, self.paper_at(x, y)) {
                    self.rgba[i + 3] = 0;
                }
            }
        }
    }

    /// The pixel at (x, y) as RGBA (tests).
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }

    /// The pixel under a paper point (tests).
    pub fn pixel_at_paper(&self, p: Pt2) -> [u8; 4] {
        let (x, y) = self.px(p);
        self.pixel((x.floor().max(0.0) as u32).min(self.width - 1), (y.floor().max(0.0) as u32).min(self.height - 1))
    }

    /// The finished image, or `None` when nothing was laid on it.
    pub fn into_image(self, layer: &str) -> Option<SheetImage> {
        if self.touched {
            Some(SheetImage { layer: layer.to_string(), rect: self.rect, width: self.width, height: self.height, rgba: std::sync::Arc::new(self.rgba) })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> Vec<u8> {
        (0..w * h).flat_map(|_| c).collect()
    }

    fn quad(r: &Rect) -> [Pt2; 4] {
        [Pt2::new(r.x0, r.y1), Pt2::new(r.x1, r.y1), Pt2::new(r.x1, r.y0), Pt2::new(r.x0, r.y0)]
    }

    #[test]
    fn a_picture_fills_the_part_of_the_paper_it_lands_on() {
        let page = Rect::new(0.0, 0.0, 100.0, 50.0);
        let mut r = PaperRaster::new(&page, 25.4, 4000).unwrap();
        assert_eq!(r.size(), (100, 50), "1 px per mm at 25.4 dpi");
        assert!(r.is_blank());
        let red = solid(4, 4, [255, 0, 0, 255]);
        r.blit_quad(quad(&Rect::new(10.0, 10.0, 60.0, 40.0)), Texture { width: 4, height: 4, rgba: &red }, 1.0);
        assert!(!r.is_blank());
        assert_eq!(r.pixel_at_paper(Pt2::new(30.0, 25.0)), [255, 0, 0, 255]);
        assert_eq!(r.pixel_at_paper(Pt2::new(70.0, 25.0))[3], 0, "outside the picture stays clear");
        assert_eq!(r.pixel_at_paper(Pt2::new(5.0, 25.0))[3], 0);
    }

    #[test]
    fn the_top_of_the_picture_lands_at_the_top_of_the_paper() {
        let page = Rect::new(0.0, 0.0, 20.0, 20.0);
        let mut r = PaperRaster::new(&page, 25.4, 4000).unwrap();
        // 1 px wide, 2 px tall: red above blue.
        let tex = [255, 0, 0, 255, 0, 0, 255, 255];
        r.blit_quad(quad(&page), Texture { width: 1, height: 2, rgba: &tex }, 1.0);
        assert_eq!(r.pixel_at_paper(Pt2::new(10.0, 18.0)), [255, 0, 0, 255]);
        assert_eq!(r.pixel_at_paper(Pt2::new(10.0, 2.0)), [0, 0, 255, 255]);
    }

    #[test]
    fn a_turned_picture_lands_turned_and_the_resolution_caps() {
        let page = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut r = PaperRaster::new(&page, 25.4, 4000).unwrap();
        // A 40 x 10 mm picture turned 90 degrees about (50, 50): 10 wide, 40 tall on the paper.
        let c = Pt2::new(50.0, 50.0);
        let turn = |x: f64, y: f64| Pt2::new(c.x - y, c.y + x);
        let q = [turn(-20.0, 5.0), turn(20.0, 5.0), turn(20.0, -5.0), turn(-20.0, -5.0)];
        let green = solid(2, 2, [0, 200, 0, 255]);
        r.blit_quad(q, Texture { width: 2, height: 2, rgba: &green }, 1.0);
        assert_eq!(r.pixel_at_paper(Pt2::new(50.0, 65.0))[3], 255, "inside the turned strip");
        assert_eq!(r.pixel_at_paper(Pt2::new(65.0, 50.0))[3], 0, "where an unturned picture would be");
        let small = PaperRaster::new(&Rect::new(0.0, 0.0, 1000.0, 500.0), 600.0, 2000).unwrap();
        assert!(small.size().0 <= 2000 && small.size().1 <= 2000);
        assert!(PaperRaster::new(&Rect::new(0.0, 0.0, 0.0, 5.0), 150.0, 100).is_none());
    }

    #[test]
    fn a_grid_placement_and_opacity_and_the_polygon_mask() {
        let page = Rect::new(0.0, 0.0, 40.0, 40.0);
        let mut r = PaperRaster::new(&page, 25.4, 4000).unwrap();
        // A 2 x 2 grid of cells over the whole page.
        let mut grid = Vec::new();
        for j in 0..3 {
            for i in 0..3 {
                grid.push(Pt2::new(i as f64 * 20.0, 40.0 - j as f64 * 20.0));
            }
        }
        let blue = solid(4, 4, [0, 0, 255, 255]);
        r.blit_grid(&grid, 2, [0.0, 0.0, 1.0, 1.0], Texture { width: 4, height: 4, rgba: &blue }, 0.5);
        let p = r.pixel_at_paper(Pt2::new(20.0, 20.0));
        assert_eq!(&p[0..3], &[0, 0, 255]);
        assert!((p[3] as i32 - 128).abs() <= 1, "half opaque: {p:?}");
        // Cut to a triangle: the far corner is cleared, the near one stays.
        r.mask_outside(&[Pt2::new(0.0, 0.0), Pt2::new(40.0, 0.0), Pt2::new(0.0, 40.0)]);
        assert_eq!(r.pixel_at_paper(Pt2::new(35.0, 35.0))[3], 0);
        assert!(r.pixel_at_paper(Pt2::new(5.0, 5.0))[3] > 0);
        let img = r.into_image("MAP").unwrap();
        assert_eq!((img.width, img.height, img.rgba.len()), (40, 40, 40 * 40 * 4));
    }

    #[test]
    fn a_picture_wholly_off_the_page_leaves_it_blank() {
        let mut r = PaperRaster::new(&Rect::new(0.0, 0.0, 10.0, 10.0), 25.4, 100).unwrap();
        let t = solid(2, 2, [1, 2, 3, 255]);
        r.blit_quad(quad(&Rect::new(100.0, 100.0, 120.0, 120.0)), Texture { width: 2, height: 2, rgba: &t }, 1.0);
        assert!(r.is_blank());
        assert!(r.into_image("MAP").is_none());
    }
}
