// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! MAPCAPTURE: the online map of the current view, captured at a chosen ground resolution as a
//! georeferenced image in the project system (PNG, world file, .prj and an attribution note), and
//! attached as an image underlay. It works like "capture map" in CADEarth and Spatial Manager.
//!
//! The planning, resampling and writing are in `afrobim_raster::capture`. This module fetches the
//! tiles (from the disk cache first) on a background thread, decodes them and hands back the
//! written file for the app to attach.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

use afrobim_kernel::{Pt, TransformPlan};
use afrobim_raster::capture::{plan_capture, resample, write_capture, CapturePlan, CaptureSpec, TILE_PX};
use afrobim_raster::{tiles, DiskCache, TileFetcher, TileKey};

/// Ground resolutions offered, metres per pixel.
pub const RESOLUTIONS: &[f64] = &[0.25, 0.5, 1.0, 2.0, 5.0, 10.0];
/// Longest side of a capture, pixels.
pub const MAX_PX: u32 = 8000;

/// The capture part of the map window.
pub struct CaptureUi {
    pub m_per_px: f64,
    pub busy: Option<Receiver<Result<(PathBuf, usize), String>>>,
    pub last: String,
}

impl Default for CaptureUi {
    fn default() -> Self {
        CaptureUi { m_per_px: 0.5, busy: None, last: String::new() }
    }
}

impl CaptureUi {
    /// The finished capture, once.
    pub fn poll(&mut self) -> Option<Result<(PathBuf, usize), String>> {
        let r = self.busy.as_ref()?.try_recv().ok()?;
        self.busy = None;
        Some(r)
    }
}

/// A project-to-WGS 84 plan as the (longitude, latitude) closure the capture engine takes. The
/// plan returns latitude first for a geographic target.
pub fn lonlat_of(plan: &TransformPlan) -> impl Fn(f64, f64) -> Result<(f64, f64), String> + '_ {
    move |e, n| plan.apply(e, n, 0.0).map(|g| (g.second, g.first)).map_err(|x| x.to_string())
}

/// The frame of a view in project metres from its two screen corners in model millimetres.
pub fn frame_m(a: Pt, b: Pt) -> CaptureSpec {
    CaptureSpec { min: [a.x.min(b.x) / 1000.0, a.y.min(b.y) / 1000.0], max: [a.x.max(b.x) / 1000.0, a.y.max(b.y) / 1000.0], m_per_px: 0.5, max_px: MAX_PX }
}

/// Fetches and decodes the tiles of a plan (disk cache first), then resamples them. Tiles that
/// cannot be fetched or decoded stay transparent; the count of them is returned.
pub fn render(plan: &CapturePlan, to_lonlat: &dyn Fn(f64, f64) -> Result<(f64, f64), String>, url: &str, provider: &str, key: &str, fetcher: &dyn TileFetcher, cache: Option<&DiskCache>) -> (Vec<u8>, usize) {
    let mut got: HashMap<(u32, u32), Arc<Vec<u8>>> = HashMap::new();
    let mut missing = 0;
    for &(x, y) in &plan.tiles {
        let k = TileKey::new(provider, plan.zoom, x, y);
        let bytes = cache.and_then(|c| c.get(&k)).or_else(|| {
            let b = fetcher.fetch(&tiles::expand_url(url, plan.zoom, x, y, key)).ok()?;
            if let Some(c) = cache {
                let _ = c.put(&k, &b);
            }
            Some(b)
        });
        let px = bytes.and_then(|b| image::load_from_memory(&b).ok()).map(|i| image::imageops::resize(&i.to_rgba8(), TILE_PX, TILE_PX, image::imageops::FilterType::Triangle).into_raw());
        match px {
            Some(p) => {
                got.insert((x, y), Arc::new(p));
            }
            None => missing += 1,
        }
    }
    let rgba = resample(plan, to_lonlat, &|x, y| got.get(&(x, y)).cloned());
    (rgba, missing)
}

impl crate::app::AfroBimApp {
    /// Plans a capture of the current view and starts it on a background thread.
    pub(crate) fn start_map_capture(&mut self, png: PathBuf) {
        let settings = crate::basemap::settings(&self.doc);
        let project = afrobim_kernel::project_crs(&self.doc);
        let result = (|| -> Result<(CapturePlan, String, String, String, TransformPlan), String> {
            let (url, provider, attribution, max_zoom) = crate::basemap::source(&settings, &self.basemap.key)?;
            let (to, _, _) = crate::basemap::plans_for(project.as_ref())?;
            let (l, t, w, h) = self.ctl.viewport;
            let mut spec = frame_m(self.view.to_world(Pt::new(l, t)), self.view.to_world(Pt::new(l + w, t + h)));
            spec.m_per_px = self.basemap.capture.m_per_px;
            let plan = plan_capture(&spec, &lonlat_of(&to), max_zoom)?;
            Ok((plan, url, provider, attribution, to))
        })();
        let (plan, url, provider, attribution, to) = match result {
            Ok(v) => v,
            Err(e) => {
                self.basemap.capture.last = format!("Not captured: {e}");
                self.status = self.basemap.capture.last.clone();
                return;
            }
        };
        let crs = project.expect("plans_for needs a project system");
        let key = self.basemap.key.clone();
        let (tx, rx) = channel();
        self.basemap.capture.last = format!("Capturing {} x {} px at {} m per pixel from {} tiles (zoom {})...", plan.width, plan.height, self.basemap.capture.m_per_px, plan.tiles.len(), plan.zoom);
        self.status = self.basemap.capture.last.clone();
        self.basemap.capture.busy = Some(rx);
        std::thread::spawn(move || {
            let fetcher = afrobim_raster::cache::HttpFetcher::new();
            let cache = DiskCache::default_root().map(DiskCache::new);
            let (rgba, missing) = render(&plan, &lonlat_of(&to), &url, &provider, &key, &fetcher, cache.as_ref());
            let r = if missing == plan.tiles.len() {
                Err("no tile could be fetched (offline, or the provider refused).".to_string())
            } else {
                write_capture(&png, &plan, &rgba, &crs, &attribution).map(|_| (png, missing))
            };
            let _ = tx.send(r);
        });
    }

    /// Picks up a finished capture and attaches it.
    pub(crate) fn finish_map_capture(&mut self) {
        match self.basemap.capture.poll() {
            Some(Ok((path, missing))) => {
                self.attach_image(&path);
                self.basemap.capture.last = if missing == 0 {
                    format!("Captured {}.", path.display())
                } else {
                    format!("Captured {}, but {missing} tiles could not be fetched and are left transparent: capture again to fill them.", path.display())
                };
                self.status = self.basemap.capture.last.clone();
            }
            Some(Err(e)) => {
                self.basemap.capture.last = format!("Capture failed: {e}");
                self.status = self.basemap.capture.last.clone();
            }
            None => {}
        }
    }
}

/// The capture controls for the map window. True when the user asked to capture.
pub fn capture_ui(ui: &mut egui::Ui, c: &mut CaptureUi, enabled: bool) -> bool {
    let mut go = false;
    crate::ui_kit::heading(ui, "Capture the map");
    ui.horizontal(|ui| {
        ui.label("Resolution");
        egui::ComboBox::from_id_salt("capture-res").selected_text(format!("{} m per pixel", c.m_per_px)).show_ui(ui, |ui| {
            for r in RESOLUTIONS {
                ui.selectable_value(&mut c.m_per_px, *r, format!("{r} m per pixel"));
            }
        });
        let busy = c.busy.is_some();
        if ui.add_enabled(enabled && !busy, egui::Button::new(if busy { "Capturing..." } else { "Capture the view..." })).on_hover_text("Saves the map of the current view as a georeferenced PNG in the project system and attaches it as an image").clicked() {
            go = true;
        }
    });
    if !c.last.is_empty() {
        ui.label(egui::RichText::new(&c.last).small());
    }
    ui.label(egui::RichText::new("A capture works offline afterwards. AfroBIM fetches at most 400 tiles per capture to keep the load on the provider light. The imagery stays under the provider's terms of use: check them before printing or sharing, and keep the attribution note saved beside the image with it.").small().weak());
    go
}

/// The default file name of a capture.
pub fn default_name(provider: &str, m_per_px: f64) -> String {
    format!("map-{provider}-{}cm.png", (m_per_px * 100.0).round() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_kernel::ProjectCrs;

    struct Solid;
    impl TileFetcher for Solid {
        fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
            if url.contains("/missing/") {
                return Err("404".into());
            }
            let img = image::RgbaImage::from_pixel(256, 256, image::Rgba([10, 200, 30, 255]));
            let mut out = Vec::new();
            image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).map_err(|e| e.to_string())?;
            Ok(out)
        }
    }

    #[test]
    fn a_capture_of_a_kampala_view_is_written_and_reads_back_in_the_project_system() {
        let crs = ProjectCrs::from_epsg(32636).unwrap();
        let (to, _, _) = crate::basemap::plans_for(Some(&crs)).unwrap();
        let mut spec = frame_m(Pt::new(448_000_000.0, 38_400_000.0), Pt::new(448_200_000.0, 38_500_000.0));
        spec.m_per_px = 2.0;
        let plan = plan_capture(&spec, &lonlat_of(&to), 19).unwrap();
        assert_eq!((plan.width, plan.height), (100, 50));
        let (rgba, missing) = render(&plan, &lonlat_of(&to), "https://t/{z}/{x}/{y}.png", "test", "", &Solid, None);
        assert_eq!(missing, 0);
        assert_eq!(&rgba[0..4], &[10, 200, 30, 255], "the tile colour reaches the capture");
        let d = std::env::temp_dir().join(format!("afrobim-cap-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let png = d.join(default_name("test", 2.0));
        write_capture(&png, &plan, &rgba, &crs, "© test").unwrap();
        let g = afrobim_raster::read_georef(&png).unwrap().unwrap();
        assert_eq!(g.crs.and_then(|c| c.epsg), Some(32636));
        let [x0, y0] = [g.affine.c, g.affine.f];
        assert!((x0 - 448_000.0).abs() < 1e-6 && (y0 - 38_500.0).abs() < 1e-6, "{x0} {y0}");
    }

    #[test]
    fn unfetchable_tiles_are_counted_and_left_transparent() {
        let crs = ProjectCrs::from_epsg(32636).unwrap();
        let (to, _, _) = crate::basemap::plans_for(Some(&crs)).unwrap();
        let mut spec = frame_m(Pt::new(448_000_000.0, 38_400_000.0), Pt::new(448_100_000.0, 38_450_000.0));
        spec.m_per_px = 5.0;
        let plan = plan_capture(&spec, &lonlat_of(&to), 19).unwrap();
        let (rgba, missing) = render(&plan, &lonlat_of(&to), "https://t/missing/{z}/{x}/{y}.png", "test", "", &Solid, None);
        assert_eq!(missing, plan.tiles.len());
        assert!(rgba.chunks(4).all(|p| p[3] == 0));
    }

    /// Fetches real Esri tiles; run with `cargo test -- --ignored live_capture` when online.
    #[test]
    #[ignore]
    fn live_capture_of_acp_park_in_arc_1960() {
        let crs = ProjectCrs::from_epsg(21036).unwrap();
        let (to, _, _) = crate::basemap::plans_for(Some(&crs)).unwrap();
        let mut spec = frame_m(Pt::new(179_100_000.0, 9_909_000_000.0), Pt::new(179_500_000.0, 9_909_300_000.0));
        spec.m_per_px = 0.5;
        let p = tiles::provider("esri-imagery").unwrap();
        let plan = plan_capture(&spec, &lonlat_of(&to), p.max_zoom).unwrap();
        let fetcher = afrobim_raster::cache::HttpFetcher::new();
        let (rgba, missing) = render(&plan, &lonlat_of(&to), p.url, p.id, "", &fetcher, None);
        assert_eq!(missing, 0, "every tile fetched");
        let out = std::env::var("AFROBIM_CAPTURE_OUT").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir().join("acp-capture.png"));
        write_capture(&out, &plan, &rgba, &crs, p.attribution).unwrap();
        eprintln!("wrote {} ({} x {}, {} tiles, zoom {})", out.display(), plan.width, plan.height, plan.tiles.len(), plan.zoom);
    }
}
