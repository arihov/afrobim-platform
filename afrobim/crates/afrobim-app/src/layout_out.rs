// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Plotting what the screen paints and the modules do not draw: for each viewport, the online
//! map and the georeferenced images (flattened into one raster, in the PDF only), the DXF
//! underlays, the GIS overlays and the civil objects (as vectors), and annotation.
//!
//! Nothing here decides what shows. The shared selections do: `UnderlayState::shown`,
//! `ImageryState::shown`, `GisState::shown` and `Geo::features_in`, `Scene::lines_in` and the
//! other `_in` methods of the civil scene, and `lp::element_shows_in` for the document's layers.
//! The screen painters use the same functions, so a viewport plots what it shows. The viewport's
//! frozen layers are honoured by all of them. The maths of the viewport (scale, twist, centre)
//! is `Viewport::to_paper`, the one the screen transform is built from.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use afrobim_draw::Pt2;
use afrobim_kernel::layout::Viewport;
use afrobim_kernel::Pt;
use afrobim_raster::capture::plan_capture;
use afrobim_raster::{DecodeOptions, DiskCache, TileFetcher};
use afrobim_sheets::geom::{clip_polygon, clip_segment, Rect};
use afrobim_sheets::layout_plot as lp;
use afrobim_sheets::raster::{PaperRaster, Texture};
use afrobim_sheets::{text_width, Align, ComposedSheet, SheetText};

use crate::app::AfroBimApp;
use crate::gis_overlay::{layer_alphas, DrawFeature};
use crate::imagery::ShownImage;
use crate::map_capture::{frame_m, lonlat_of};

/// Resolution of the plotted rasters, dots per inch of paper.
pub const RASTER_DPI: f64 = 150.0;
/// The longest side of a plotted raster, pixels.
const RASTER_MAX_PX: u32 = 3200;
/// The layer rasters are on in the sheet.
pub const MAP_LAYER: &str = "MAP-IMAGES";
const UNDERLAY_PEN_MM: f64 = 0.13;
const GIS_PEN_MM: f64 = 0.25;
const CROSS_MM: f64 = 1.2;

/// A picture decoded for the plot, by file, longest side and transparent-black switch.
type DecodedKey = (String, u32, bool);

/// What a plot reads more than once (pictures decoded from their files), kept for one plot.
#[derive(Default)]
pub struct PlotCache {
    images: RefCell<HashMap<DecodedKey, Option<Arc<afrobim_raster::Decoded>>>>,
}

impl PlotCache {
    fn decoded(&self, path: &str, max_side: u32, transparent_black: bool) -> Option<Arc<afrobim_raster::Decoded>> {
        self.images
            .borrow_mut()
            .entry((path.to_string(), max_side, transparent_black))
            .or_insert_with(|| afrobim_raster::decode(std::path::Path::new(path), DecodeOptions { max_side, transparent_black }).ok().map(Arc::new))
            .clone()
    }
}

/// A colour shown at `alpha` (0 to 1) over white paper, as an opaque colour.
pub fn on_white(rgb: [u8; 3], alpha: f32) -> [u8; 3] {
    let a = alpha.clamp(0.0, 1.0);
    let f = |c: u8| (255.0 + (c as f32 - 255.0) * a).round() as u8;
    [f(rgb[0]), f(rgb[1]), f(rgb[2])]
}

/// Puts model geometry on the sheet through a viewport, cut to the viewport's box.
struct Sink<'a> {
    sheet: &'a mut ComposedSheet,
    v: &'a Viewport,
    rect: Rect,
}

impl Sink<'_> {
    fn paper(&self, m: Pt) -> Pt2 {
        self.v.to_paper(m)
    }

    fn segment(&mut self, layer: &str, a: Pt, b: Pt, rgb: [u8; 3], weight: f64) {
        if let Some((p, q)) = clip_segment(self.paper(a), self.paper(b), &self.rect) {
            self.sheet.line(layer, p, q, weight, rgb);
        }
    }

    fn polyline(&mut self, layer: &str, pts: &[[f64; 2]], rgb: [u8; 3], weight: f64) {
        let paper: Vec<Pt2> = pts.iter().map(|p| self.paper(Pt::new(p[0], p[1]))).collect();
        for piece in afrobim_sheets::geom::clip_polyline(&paper, &self.rect) {
            self.sheet.stroke(layer, piece, false, weight, rgb);
        }
    }

    fn fill(&mut self, layer: &str, rings: &[Vec<[f64; 2]>], rgb: [u8; 3]) {
        let cut: Vec<Vec<Pt2>> = rings
            .iter()
            .map(|r| clip_polygon(&r.iter().map(|p| self.paper(Pt::new(p[0], p[1]))).collect::<Vec<_>>(), &self.rect))
            .filter(|r| r.len() >= 3)
            .collect();
        self.sheet.fill(layer, cut, rgb);
    }

    fn cross(&mut self, layer: &str, at: Pt, rgb: [u8; 3], weight: f64) {
        let c = self.paper(at);
        if self.rect.contains(c) {
            self.sheet.line(layer, Pt2::new(c.x - CROSS_MM, c.y), Pt2::new(c.x + CROSS_MM, c.y), weight, rgb);
            self.sheet.line(layer, Pt2::new(c.x, c.y - CROSS_MM), Pt2::new(c.x, c.y + CROSS_MM), weight, rgb);
        }
    }
}

impl AfroBimApp {
    /// What goes under the module drawings of a viewport, bottom to top as on the screen: the
    /// map and images (one raster), DXF underlays, GIS overlays, civil objects.
    pub(crate) fn plot_under(&self, cache: &PlotCache, v: &Viewport, rect: &Rect, sheet: &mut ComposedSheet) {
        self.plot_rasters(cache, v, rect, sheet);
        let mut sink = Sink { sheet, v, rect: *rect };
        self.plot_underlays(&mut sink);
        self.plot_gis(&mut sink);
        self.plot_civil(&mut sink);
    }

    /// The online map and the georeferenced images of a viewport as one raster in the sheet. The
    /// map is left out, with a warning that says why, when its provider does not allow saving.
    fn plot_rasters(&self, cache: &PlotCache, v: &Viewport, rect: &Rect, sheet: &mut ComposedSheet) {
        let win = v.model_window();
        let settings = crate::basemap::settings(&self.doc);
        let images: Vec<ShownImage> = self
            .imagery
            .shown(&self.doc, &v.frozen)
            .into_iter()
            .filter(|i| i.placement.hi.x >= win[0] && i.placement.lo.x <= win[2] && i.placement.hi.y >= win[1] && i.placement.lo.y <= win[3])
            .collect();
        if !settings.enabled && images.is_empty() {
            return;
        }
        let Some(mut raster) = PaperRaster::new(rect, RASTER_DPI, RASTER_MAX_PX) else { return };
        if settings.enabled {
            match self.plot_basemap(v, &settings, &mut raster) {
                Ok(attribution) => sheet.text(SheetText {
                    layer: MAP_LAYER.into(),
                    at: Pt2::new(rect.x0 + 1.0, rect.y0 + 1.0),
                    height_mm: 1.6,
                    text: attribution,
                    align: Align::Left,
                    rotation_deg: 0.0,
                    bold: false,
                    rgb: [70, 70, 70],
                }),
                Err(why) => sheet.warn(why),
            }
        }
        for img in &images {
            let side = img.def.detail.clamp(256, 4096);
            let Some(pic) = cache.decoded(&img.def.path, side, img.def.transparent_black) else {
                sheet.warn(format!("The image {} could not be read for the plot and is left out.", img.name));
                continue;
            };
            let grid: Vec<Pt2> = img.placement.pts.iter().map(|p| v.to_paper(*p)).collect();
            raster.blit_grid(&grid, img.placement.n, [0.0, 0.0, 1.0, 1.0], Texture { width: pic.width, height: pic.height, rgba: &pic.rgba }, img.def.opacity.clamp(0.05, 1.0));
        }
        if v.clip.is_some() {
            raster.mask_outside(&v.outline());
        }
        if let Some(im) = raster.into_image(MAP_LAYER) {
            sheet.image(im);
        }
    }

    /// Why the online map cannot be in a plot, or `None` when it can (or is not shown). The same
    /// refusal MAPCAPTURE gives.
    pub(crate) fn plot_map_refusal(&self) -> Option<String> {
        let s = crate::basemap::settings(&self.doc);
        if !s.enabled {
            return None;
        }
        crate::basemap::capture_refusal(&s)
    }

    /// Lays the tiles of the viewport's window on the raster. Returns the attribution line.
    fn plot_basemap(&self, v: &Viewport, s: &crate::basemap::BasemapSettings, raster: &mut PaperRaster) -> Result<String, String> {
        if let Some(why) = crate::basemap::capture_refusal(s) {
            return Err(format!("The online map is not in the plot: {why}"));
        }
        let (url, provider, attribution, max_zoom) = crate::basemap::source(s, &self.basemap.key).map_err(|e| format!("The online map is not in the plot: {e}"))?;
        let project = afrobim_kernel::project_crs(&self.doc);
        let (to, _, _) = crate::basemap::plans_for(project.as_ref()).map_err(|e| format!("The online map is not in the plot: {e}"))?;
        let w = v.model_window();
        let mut spec = frame_m(Pt::new(w[0], w[1]), Pt::new(w[2], w[3]));
        // The ground size of one pixel of the paper raster; coarser when the provider's limits ask.
        let ideal = v.scale_den * 0.0254 / RASTER_DPI;
        spec.m_per_px = ideal.max(0.1);
        let mut plan = None;
        let mut last = String::new();
        for _ in 0..8 {
            match plan_capture(&spec, &lonlat_of(&to), max_zoom) {
                Ok(p) => {
                    plan = Some(p);
                    break;
                }
                Err(e) => {
                    last = e;
                    spec.m_per_px *= 1.6;
                }
            }
        }
        let plan = plan.ok_or_else(|| format!("The online map is not in the plot: {last}"))?;
        let fetcher: Arc<dyn TileFetcher> = match &self.lay.plot_fetcher {
            Some(f) => f.clone(),
            None => Arc::new(afrobim_raster::cache::HttpFetcher::new()),
        };
        let disk = if self.lay.plot_fetcher.is_some() { None } else { DiskCache::default_root().map(DiskCache::new) };
        let (rgba, missing) = crate::map_capture::render(&plan, &lonlat_of(&to), &url, &provider, &self.basemap.key, fetcher.as_ref(), disk.as_ref());
        if missing == plan.tiles.len() {
            return Err("The online map is not in the plot: no tile could be fetched (offline, or the provider refused).".into());
        }
        let corner = |c: f64, r: f64| {
            let (e, n) = plan.affine.apply(c, r);
            v.to_paper(Pt::new(e * 1000.0, n * 1000.0))
        };
        let (pw, ph) = (plan.width as f64, plan.height as f64);
        raster.blit_quad([corner(0.0, 0.0), corner(pw, 0.0), corner(pw, ph), corner(0.0, ph)], Texture { width: plan.width, height: plan.height, rgba: &rgba }, s.opacity.clamp(0.05, 1.0));
        Ok(if missing == 0 { attribution } else { format!("{attribution} ({missing} tiles missing)") })
    }

    fn plot_underlays(&self, sink: &mut Sink) {
        let win = sink.v.model_window();
        let k = sink.v.paper_per_model();
        for (e, def, b) in self.under.shown(&self.doc, &sink.v.frozen) {
            let rgb = on_white([26, 26, 26], def.opacity.clamp(0.05, 1.0) * 0.8);
            for (a, c) in b.world_segments() {
                if a.x.max(c.x) < win[0] || a.x.min(c.x) > win[2] || a.y.max(c.y) < win[1] || a.y.min(c.y) > win[3] {
                    continue;
                }
                sink.segment(&e.name, a, c, rgb, UNDERLAY_PEN_MM);
            }
            for (p, text, height, rot) in &b.labels {
                let h = (height * k).clamp(0.6, 25.0);
                if p.x < win[0] || p.x > win[2] || p.y < win[1] || p.y > win[3] || height * k < 0.8 {
                    continue;
                }
                let at = sink.paper(*p);
                if sink.rect.contains(at) {
                    sink.sheet.text(SheetText { layer: e.name.clone(), at, height_mm: h, text: text.replace('\n', " "), align: Align::Left, rotation_deg: rot + sink.v.twist_deg, bold: false, rgb });
                }
            }
        }
    }

    fn plot_gis(&self, sink: &mut Sink) {
        let win = sink.v.model_window();
        for (e, geo, look) in self.gis.shown(&self.doc, &sink.v.frozen) {
            let (a8, fa8) = layer_alphas(&look);
            for DrawFeature { f, layer, tris } in geo.features_in(&look, win) {
                let rgb = on_white(layer.colour, a8 as f32 / 255.0);
                match &f.geom {
                    crate::gis_overlay::PGeom::Point(p) => sink.cross(&e.name, Pt::new(p[0], p[1]), rgb, GIS_PEN_MM),
                    crate::gis_overlay::PGeom::Line(v) => sink.polyline(&e.name, v, rgb, GIS_PEN_MM),
                    crate::gis_overlay::PGeom::Area(rings) => {
                        let tint = on_white(layer.colour, fa8 as f32 / 255.0);
                        let rings_tri: Vec<Vec<[f64; 2]>> = tris.iter().map(|t| t.to_vec()).collect();
                        sink.fill(&e.name, &rings_tri, tint);
                        for ring in rings {
                            sink.polyline(&e.name, ring, rgb, GIS_PEN_MM);
                        }
                    }
                }
            }
        }
    }

    fn plot_civil(&self, sink: &mut Sink) {
        let w = sink.v.model_window();
        let k = sink.v.paper_per_model();
        let mut labels: Vec<(u8, SheetText)> = Vec::new();
        for e in self.doc.elements() {
            if !(crate::civil::is_civil(&e.category) && !crate::civil::hidden(e) && lp::element_shows_in(&self.doc, e, sink.v)) {
                continue;
            }
            let Some((_, b)) = self.civil.built.get(&e.id) else { continue };
            let sc = &b.scene;
            for f in sc.fills_in(w) {
                sink.fill(&e.layer, std::slice::from_ref(&f.poly), on_white([f.rgba[0], f.rgba[1], f.rgba[2]], f.rgba[3] as f32 / 255.0));
            }
            // A line much smaller than the pen is a smudge on paper too.
            for l in sc.lines_in(w, 0.2 / k) {
                sink.polyline(&e.layer, &l.pts, l.rgb, (l.width as f64 * 0.18).clamp(0.13, 0.8));
            }
            for p in sc.points_in(w) {
                sink.cross(&e.layer, Pt::new(p.at[0], p.at[1]), [26, 26, 26], 0.13);
            }
            for t in sc.texts_in(w) {
                let h = if t.height_mm > 0.0 { t.height_mm * k } else { (t.px as f64 * 0.2646 * 0.75).clamp(1.8, 3.2) };
                if !(1.2..=15.0).contains(&h) {
                    continue;
                }
                let at = sink.paper(Pt::new(t.at[0], t.at[1]));
                let (align, dy) = match t.anchor {
                    1 => (Align::Left, -h / 2.0),
                    2 => (Align::Right, -h / 2.0),
                    3 => (Align::Centre, 0.4),
                    4 => (Align::Centre, -h - 0.4),
                    _ => (Align::Centre, -h / 2.0),
                };
                labels.push((t.prio, SheetText { layer: e.layer.clone(), at: Pt2::new(at.x, at.y + dy), height_mm: h, text: t.text.clone(), align, rotation_deg: 0.0, bold: false, rgb: t.rgb }));
            }
        }
        place_labels(sink, labels);
    }

    /// Annotation (text, dimensions, leaders) in a viewport, over the module drawings.
    pub(crate) fn plot_over(&self, v: &Viewport, rect: &Rect, sheet: &mut ComposedSheet) {
        use afrobim_sheets::geom::Aff;
        let doc = &self.doc;
        if !doc.elements().iter().any(afrobim_annotate::is_annotation) {
            return;
        }
        let styles = afrobim_sheets::Styles::default();
        let k = lp::paper_per_model(v);
        let pc = v.paper_centre();
        let window = {
            let w = v.model_window();
            afrobim_kernel::BBox::new(w[0], w[1], w[2], w[3])
        };
        // The annotation drawing is placed by an offset and a scale; the twist is applied after.
        let offset = Pt::new(pc.x / k - v.centre.x, pc.y / k - v.centre.y);
        let sd = afrobim_annotate::sheet_drawing(doc, offset, k, Some(&window));
        let turn = if v.twist_deg == 0.0 { Aff::IDENTITY } else { Aff::translate(-pc.x, -pc.y).then(&Aff::scale_rotate(1.0, v.twist_deg)).then(&Aff::translate(pc.x, pc.y)) };
        afrobim_sheets::draw_drawing(sheet, &styles, &sd.drawing, &turn, rect, crate::sheets_glue::ANNOTATION_LAYER);
        let rgb = styles.pens.pen(crate::sheets_glue::ANNOTATION_LAYER).rgb;
        for l in &sd.labels {
            let at = turn.apply(l.at);
            if rect.contains(at) {
                sheet.text(SheetText { layer: crate::sheets_glue::ANNOTATION_LAYER.into(), at, height_mm: l.height_mm, text: l.text.clone(), align: l.align, rotation_deg: l.rotation_deg + v.twist_deg, bold: l.bold, rgb });
            }
        }
    }
}

/// Puts the labels on the sheet, highest priority first, leaving out one that would overlap a
/// label already placed (as the screen does with `Labels::flush`).
fn place_labels(sink: &mut Sink, mut labels: Vec<(u8, SheetText)>) {
    labels.sort_by_key(|l| std::cmp::Reverse(l.0));
    let mut taken: Vec<[f64; 4]> = Vec::new();
    for (_, t) in labels {
        let w = text_width(&t.text, t.height_mm, t.bold);
        let x0 = match t.align {
            Align::Left => t.at.x,
            Align::Centre => t.at.x - w / 2.0,
            Align::Right => t.at.x - w,
        };
        let r = [x0 - 0.3, t.at.y - 0.3, x0 + w + 0.3, t.at.y + t.height_mm + 0.3];
        if !sink.rect.contains(Pt2::new(t.at.x, t.at.y)) || taken.iter().any(|o| r[0] < o[2] && r[2] > o[0] && r[1] < o[3] && r[3] > o[1]) {
            continue;
        }
        taken.push(r);
        sink.sheet.text(t);
    }
}
