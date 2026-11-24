//! The online background map: satellite imagery or a street map drawn behind the model.
//!
//! It is off until switched on (command `BASEMAP`, or the Images and background map window), and
//! it is the only part of AfroBIM that uses the network. It needs the project coordinate system
//! (command `CRS`): the view is moved to WGS 84 to find the tiles, and each tile is moved back to
//! the project system as an 8 by 8 (coarse zoom) or 4 by 4 grid, through the same transformation
//! plans the CRS command uses. A project in Arc 1960 is therefore shifted to WGS 84 with a stated
//! accuracy, which the status line shows; a local grid or a system known by name only cannot show
//! the map, and the window says why.
//!
//! The settings (on or off, provider, opacity) are kept in the project settings, so a document
//! opens with its map. A provider key or token is kept for the session only (or read from the
//! `AFROBIM_TILE_KEY` environment variable): it is never written to the document or to disk.
//! Tiles are cached on disk for 30 days (`AFROBIM_TILE_CACHE` moves the cache), at most a few are
//! fetched at a time and only what the view needs, and the provider's attribution is drawn in the
//! corner of the canvas, as the providers' terms require.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use afrobim_kernel::{project_crs, set_project_setting, CrsKind, Document, ProjectCrs, Pt, TransformPlan, ViewTransform};
use afrobim_raster::tiles::{self, TileKey};
use afrobim_raster::TileService;
use egui::{pos2, Color32, FontId, Painter, Rect, TextureHandle};
use serde::{Deserialize, Serialize};

pub const SETTINGS_KEY: &str = "basemap";
/// The id of a custom template provider.
pub const CUSTOM: &str = "custom";
/// The most tiles drawn for one view; beyond it the zoom is coarsened.
const MAX_TILES: usize = 64;
/// The most tile textures kept.
const MAX_TEXTURES: usize = 400;
/// A tile that failed is not asked for again for this long.
const RETRY: Duration = Duration::from_secs(60);

/// What the document stores.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BasemapSettings {
    pub enabled: bool,
    pub provider: String,
    /// The template of the custom provider.
    pub custom_url: String,
    pub custom_attribution: String,
    pub opacity: f32,
}

impl Default for BasemapSettings {
    fn default() -> Self {
        BasemapSettings { enabled: false, provider: tiles::PROVIDERS[0].id.into(), custom_url: String::new(), custom_attribution: String::new(), opacity: 1.0 }
    }
}

pub fn settings(doc: &Document) -> BasemapSettings {
    afrobim_kernel::project_setting(doc, SETTINGS_KEY).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

pub fn set_settings(doc: &mut Document, s: &BasemapSettings) {
    if let Ok(v) = serde_json::to_value(s) {
        set_project_setting(doc, SETTINGS_KEY, v);
    }
}

/// The URL template, the cache id and the attribution of the chosen provider, or why there is none.
pub fn source(s: &BasemapSettings, key: &str) -> Result<(String, String, String, u8), String> {
    if s.provider == CUSTOM {
        tiles::check_template(&s.custom_url)?;
        // The cache folder of a custom template is named from a hash of it.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in s.custom_url.trim().bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        if s.custom_url.contains("{key}") && key.trim().is_empty() {
            return Err("The custom URL has {key}: type the key or token in the window.".into());
        }
        let attribution = if s.custom_attribution.trim().is_empty() { "Custom tiles: check the provider's terms and attribution".to_string() } else { s.custom_attribution.clone() };
        return Ok((s.custom_url.trim().to_string(), format!("custom-{h:016x}"), attribution, 20));
    }
    let p = tiles::provider(&s.provider).ok_or_else(|| format!("Unknown map provider '{}'.", s.provider))?;
    if p.needs_key && key.trim().is_empty() {
        return Err(format!("{} needs a key or token: type it in the window (it is kept for this session only).", p.name));
    }
    Ok((p.url.to_string(), p.id.to_string(), p.attribution.to_string(), p.max_zoom))
}

/// The two plans between the project and WGS 84 geographic, and a sentence on how.
pub fn plans_for(project: Option<&ProjectCrs>) -> Result<Plans, String> {
    let project = project.ok_or("Set the project coordinate system (command CRS) to show the map: AfroBIM needs to know where the model is on Earth.")?;
    match project.kind {
        CrsKind::LocalGrid => return Err(format!("The project is on a local site grid ({}), which has no position on Earth: the map cannot be placed.", project.label())),
        CrsKind::NamedOnly => return Err(format!("The project's system ({}) is known by name only: AfroBIM cannot compute positions in it, so the map cannot be placed.", project.label())),
        _ => {}
    }
    let wgs = ProjectCrs::geographic(4326).ok_or("WGS 84 is missing from the kernel.")?;
    let to = afrobim_kernel::plan_transform(project, &wgs, None).map_err(|e| format!("No way from {} to WGS 84: {e}.", project.label()))?;
    let from = afrobim_kernel::plan_transform(&wgs, project, None).map_err(|e| format!("No way from WGS 84 to {}: {e}.", project.label()))?;
    let (to, from) = (to.into_iter().next().ok_or("No transformation to WGS 84.")?, from.into_iter().next().ok_or("No transformation from WGS 84.")?);
    let how = format!("Placed from WGS 84 into {}{}.", project.label(), crate::imagery::plan_words(&from));
    Ok((to, from, how))
}

/// Project to WGS 84, WGS 84 to project, and how, in words.
type Plans = (TransformPlan, TransformPlan, String);

/// One tile to draw: the grid of the area (`area` key), the texture of `tex` and the part of it the
/// area covers.
#[derive(Debug, Clone)]
struct Draw {
    area: TileKey,
    tex: TileKey,
    uv: [f32; 4],
}

/// The runtime side of the map.
pub struct BasemapState {
    service: Option<TileService>,
    textures: HashMap<TileKey, (TextureHandle, u64)>,
    failed: HashMap<TileKey, Instant>,
    /// The model grid of each tile area, for the coordinate system in `grid_key`.
    grids: HashMap<TileKey, (usize, Vec<Pt>)>,
    grid_key: String,
    plans: Option<(String, Result<Plans, String>)>,
    draws: Vec<Draw>,
    frame: u64,
    /// The provider key or token typed in the window. Session only.
    pub key: String,
    /// One line for the window and the status bar.
    pub status: String,
    pub attribution: String,
    pub zoom: u8,
    pub last_error: Option<String>,
    /// Tiles of the last view not shown yet (stand-ins do not count).
    missing: usize,
    /// Frames in a row with every tile of the view shown.
    settled: u32,
    created: Instant,
    /// MAPCAPTURE controls and the capture in flight.
    pub capture: crate::map_capture::CaptureUi,
}

impl Default for BasemapState {
    fn default() -> Self {
        BasemapState {
            service: None,
            textures: HashMap::new(),
            failed: HashMap::new(),
            grids: HashMap::new(),
            grid_key: String::new(),
            plans: None,
            draws: Vec::new(),
            frame: 0,
            key: std::env::var("AFROBIM_TILE_KEY").unwrap_or_default(),
            status: String::new(),
            attribution: String::new(),
            zoom: 0,
            last_error: None,
            missing: 0,
            settled: 0,
            created: Instant::now(),
            capture: Default::default(),
        }
    }
}

impl BasemapState {
    /// Frames in a row in which every tile of the view was shown (0 while the map is off or
    /// cannot be placed).
    pub fn settled(&self) -> u32 {
        self.settled
    }

    /// Seconds since the map state was made (start-up or the last document change).
    pub fn age_s(&self) -> f64 {
        self.created.elapsed().as_secs_f64()
    }

    /// Tiles queued or being fetched.
    pub fn pending(&self) -> usize {
        self.service.as_ref().map(|s| s.pending()).unwrap_or(0)
    }

    fn service(&mut self) -> &TileService {
        self.service.get_or_insert_with(|| {
            let cache = afrobim_raster::DiskCache::default_root().map(afrobim_raster::DiskCache::new);
            #[cfg(not(test))]
            let fetcher: Arc<dyn afrobim_raster::TileFetcher> = Arc::new(afrobim_raster::cache::HttpFetcher::new());
            // Tests never touch the network (and never write the user's tile cache).
            #[cfg(test)]
            let (fetcher, cache): (Arc<dyn afrobim_raster::TileFetcher>, Option<afrobim_raster::DiskCache>) = {
                drop(cache);
                (Arc::new(NoNetwork), None)
            };
            TileService::new(fetcher, cache, 3)
        })
    }

    /// Once a frame, before painting: works out the tiles the view needs, asks for the missing
    /// ones, takes in what arrived. `viewport` is left, top, width, height in points. Returns true
    /// while tiles are on their way (the caller asks for a repaint).
    pub fn sync(&mut self, ctx: &egui::Context, doc: &Document, view: &ViewTransform, viewport: (f64, f64, f64, f64)) -> bool {
        self.frame += 1;
        self.draws.clear();
        // Until the end of this sync the view counts as not complete.
        let settled = std::mem::take(&mut self.settled);
        let s = settings(doc);
        if !s.enabled {
            self.status = "Off.".into();
            self.attribution.clear();
            return false;
        }
        let (template, provider_id, attribution, max_zoom) = match source(&s, &self.key) {
            Ok(v) => v,
            Err(e) => {
                self.status = e;
                self.attribution.clear();
                return false;
            }
        };
        let project = project_crs(doc);
        let pkey = serde_json::to_string(&project).unwrap_or_default();
        if self.plans.as_ref().map(|(k, _)| k) != Some(&pkey) {
            self.plans = Some((pkey.clone(), plans_for(project.as_ref())));
        }
        let (to_geo, from_geo, how) = match &self.plans {
            Some((_, Ok(p))) => (p.0.clone(), p.1.clone(), p.2.clone()),
            Some((_, Err(e))) => {
                self.status = e.clone();
                self.attribution.clear();
                return false;
            }
            None => return false,
        };
        if self.grid_key != pkey {
            self.grids.clear();
            self.grid_key = pkey;
        }
        // The view in WGS 84: corners, edge middles and centre.
        let (l, t, w, h) = viewport;
        let mut lon = (f64::MAX, f64::MIN);
        let mut lat = (f64::MAX, f64::MIN);
        let mut centre_lat = 0.0;
        for (fx, fy) in [(0.0, 0.0), (0.5, 0.0), (1.0, 0.0), (1.0, 0.5), (1.0, 1.0), (0.5, 1.0), (0.0, 1.0), (0.0, 0.5), (0.5, 0.5)] {
            let m = view.to_world(Pt::new(l + w * fx, t + h * fy));
            let Ok(g) = to_geo.apply(m.x / 1000.0, m.y / 1000.0, 0.0) else { continue };
            lat = (lat.0.min(g.first), lat.1.max(g.first));
            lon = (lon.0.min(g.second), lon.1.max(g.second));
            if fx == 0.5 && fy == 0.5 {
                centre_lat = g.first;
            }
        }
        if lon.0 > lon.1 || lat.0 > lat.1 {
            self.status = "The view is outside what the project's projection can compute: zoom in on the project.".into();
            return false;
        }
        let mpp = 0.001 / view.scale.max(1e-12);
        let mut z = tiles::zoom_for(mpp, centre_lat, 1, max_zoom);
        let (mut xs, mut ys) = tiles::tiles_covering(lon.0, lat.0, lon.1, lat.1, z);
        while z > 1 && xs.clone().count() * ys.clone().count() > MAX_TILES {
            z -= 1;
            (xs, ys) = tiles::tiles_covering(lon.0, lat.0, lon.1, lat.1, z);
        }
        self.zoom = z;
        // Take in what arrived.
        let results = self.service().poll();
        for r in results {
            match r.pixels {
                Ok((w, h, px)) => {
                    let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
                    let tex = ctx.load_texture(format!("tile-{}-{}-{}-{}", r.key.provider, r.key.z, r.key.x, r.key.y), img, egui::TextureOptions::LINEAR);
                    self.failed.remove(&r.key);
                    self.textures.insert(r.key, (tex, self.frame));
                }
                Err(e) => {
                    self.last_error = Some(format!("Tile {}/{}/{}: {e}", r.key.z, r.key.x, r.key.y));
                    self.failed.insert(r.key, Instant::now());
                }
            }
        }
        // What to draw and what to ask for.
        let mut wanted: HashSet<TileKey> = HashSet::new();
        let mut asks: Vec<(TileKey, String)> = Vec::new();
        for y in ys.clone() {
            for x in xs.clone() {
                let k = TileKey::new(&provider_id, z, x, y);
                if !self.grids.contains_key(&k) {
                    let n = if z < 12 { 8 } else { 4 };
                    if let Some(g) = tile_grid(&from_geo, z, x, y, n) {
                        self.grids.insert(k.clone(), (n, g));
                    }
                }
                if self.textures.contains_key(&k) {
                    self.draws.push(Draw { area: k.clone(), tex: k.clone(), uv: [0.0, 0.0, 1.0, 1.0] });
                    continue;
                }
                // Until it arrives, the nearest ancestor that is here stands in.
                let mut cur = k.clone();
                let (mut u0, mut v0, mut size) = (0.0f64, 0.0f64, 1.0f64);
                for _ in 0..6 {
                    let Some((p, q)) = cur.parent() else { break };
                    u0 = q[0] + u0 * q[2];
                    v0 = q[1] + v0 * q[2];
                    size *= q[2];
                    if self.textures.contains_key(&p) {
                        self.draws.push(Draw { area: k.clone(), tex: p, uv: [u0 as f32, v0 as f32, (u0 + size) as f32, (v0 + size) as f32] });
                        break;
                    }
                    cur = p;
                }
                wanted.insert(k.clone());
                if self.failed.get(&k).is_none_or(|t| t.elapsed() > RETRY) {
                    asks.push((k.clone(), tiles::expand_url(&template, z, x, y, &self.key)));
                }
            }
        }
        // Stand-ins are drawn first, then the tiles of the zoom on top.
        self.draws.sort_by_key(|d| d.tex.z);
        let svc = self.service();
        svc.retain(|k| wanted.contains(k));
        for (k, url) in asks {
            svc.request(k, url);
        }
        // Keep the textures in use fresh; drop the oldest beyond the cap.
        for d in &self.draws {
            if let Some(e) = self.textures.get_mut(&d.tex) {
                e.1 = self.frame;
            }
        }
        if self.textures.len() > MAX_TEXTURES {
            let mut ages: Vec<(TileKey, u64)> = self.textures.iter().map(|(k, (_, f))| (k.clone(), *f)).collect();
            ages.sort_by_key(|(_, f)| *f);
            for (k, _) in ages.into_iter().take(self.textures.len() - MAX_TEXTURES) {
                self.textures.remove(&k);
            }
        }
        if self.grids.len() > 4 * MAX_TEXTURES {
            self.grids.clear();
        }
        let pending = self.pending();
        self.missing = wanted.len();
        self.settled = if self.missing == 0 { settled + 1 } else { 0 };
        let total = xs.count() * ys.count();
        self.status = format!("Zoom {z}: {} of {total} tiles shown{}. {how}", total.saturating_sub(wanted.len()), if pending > 0 { format!(", {pending} loading") } else { String::new() });
        self.attribution = attribution;
        pending > 0
    }

    /// Draws the map behind everything, then the attribution in the lower right corner.
    pub fn paint(&self, doc: &Document, painter: &Painter, view: &ViewTransform, rect: Rect, dark: bool) {
        if self.draws.is_empty() && self.attribution.is_empty() {
            return;
        }
        let s = settings(doc);
        let tint = Color32::WHITE.gamma_multiply(s.opacity.clamp(0.05, 1.0));
        for d in &self.draws {
            let (Some((tex, _)), Some((n, grid))) = (self.textures.get(&d.tex), self.grids.get(&d.area)) else { continue };
            crate::imagery::paint_grid(painter, view, rect, tex.id(), *n, grid, d.uv, tint);
        }
        if !self.attribution.is_empty() {
            let text = format!("{}  |  AfroBIM map z{}", self.attribution, self.zoom);
            let galley = painter.layout_no_wrap(text, FontId::proportional(10.5), if dark { Color32::from_gray(225) } else { Color32::from_gray(30) });
            let at = rect.right_bottom() - egui::vec2(galley.size().x + 10.0, galley.size().y + 6.0);
            let bg = Rect::from_min_size(at, galley.size()).expand2(egui::vec2(5.0, 2.0));
            painter.rect_filled(bg, 3.0, if dark { Color32::from_rgba_unmultiplied(20, 22, 26, 200) } else { Color32::from_rgba_unmultiplied(255, 255, 255, 210) });
            painter.galley(pos2(at.x, at.y), galley, Color32::WHITE);
        }
    }
}

/// The fetcher of test builds: refuses every request.
#[cfg(test)]
struct NoNetwork;

#[cfg(test)]
impl afrobim_raster::TileFetcher for NoNetwork {
    fn fetch(&self, _url: &str) -> Result<Vec<u8>, String> {
        Err("the network is not used in tests".into())
    }
}

/// The model grid of tile `(z, x, y)`: `(n + 1)^2` points row by row from the north-west corner,
/// moved from WGS 84 to the project with `from_geo`. `None` when a point cannot be projected.
pub fn tile_grid(from_geo: &TransformPlan, z: u8, x: u32, y: u32, n: usize) -> Option<Vec<Pt>> {
    let mut out = Vec::with_capacity((n + 1) * (n + 1));
    for j in 0..=n {
        for i in 0..=n {
            let (lon, lat) = tiles::tile_to_lonlat(x as f64 + i as f64 / n as f64, y as f64 + j as f64 / n as f64, z);
            let p = from_geo.apply(lat, lon, 0.0).ok()?;
            out.push(Pt::new(p.first * 1000.0, p.second * 1000.0));
        }
    }
    Some(out)
}

/// The background map part of the window.
/// The quick map types and the built-in provider each one picks.
pub const MAP_TYPES: &[(&str, &str)] = &[("Satellite", "esri-imagery"), ("Street", "osm"), ("Terrain", "opentopomap")];

/// Which quick map type a provider is, if any.
pub fn map_type(provider: &str) -> Option<usize> {
    MAP_TYPES.iter().position(|(_, id)| *id == provider)
}

pub fn settings_ui(ui: &mut egui::Ui, s: &mut BasemapSettings, st: &mut BasemapState, project: Option<&ProjectCrs>) {
    ui.horizontal(|ui| {
        for (i, (label, id)) in MAP_TYPES.iter().enumerate() {
            if ui.selectable_label(map_type(&s.provider) == Some(i), *label).clicked() {
                s.provider = id.to_string();
                s.enabled = true;
            }
        }
    });
    ui.add_space(4.0);
    egui::Grid::new("basemap-grid").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
        ui.label("Show the map");
        crate::ui_kit::toggle(ui, &mut s.enabled, "Online tiles behind the model (uses the internet)");
        ui.end_row();
        ui.label("Provider");
        let label = if s.provider == CUSTOM { "Custom tile URL".to_string() } else { tiles::provider(&s.provider).map(|p| p.name.to_string()).unwrap_or_else(|| s.provider.clone()) };
        egui::ComboBox::from_id_salt("basemap-provider").selected_text(label).width(300.0).show_ui(ui, |ui| {
            ui.label(egui::RichText::new("Free").small().weak());
            for p in tiles::PROVIDERS.iter().filter(|p| !p.needs_key) {
                ui.selectable_value(&mut s.provider, p.id.to_string(), p.name);
            }
            ui.separator();
            ui.label(egui::RichText::new("Needs your own key or token").small().weak());
            for p in tiles::PROVIDERS.iter().filter(|p| p.needs_key) {
                ui.selectable_value(&mut s.provider, p.id.to_string(), p.name);
            }
            ui.separator();
            ui.selectable_value(&mut s.provider, CUSTOM.to_string(), "Custom tile URL");
        });
        ui.end_row();
        if s.provider == CUSTOM {
            ui.label("Tile URL");
            ui.add(egui::TextEdit::singleline(&mut s.custom_url).desired_width(380.0).hint_text("https://example.com/tiles/{z}/{x}/{y}.png?key={key}"));
            ui.end_row();
            ui.label("Attribution");
            ui.add(egui::TextEdit::singleline(&mut s.custom_attribution).desired_width(380.0).hint_text("As the provider's terms ask"));
            ui.end_row();
        }
        let needs_key = s.provider == CUSTOM && s.custom_url.contains("{key}") || tiles::provider(&s.provider).is_some_and(|p| p.needs_key);
        if needs_key {
            ui.label("Key or token");
            ui.add(egui::TextEdit::singleline(&mut st.key).password(true).desired_width(260.0)).on_hover_text("Kept for this session only: never written to the document or to disk. AFROBIM_TILE_KEY sets it at start-up.");
            ui.end_row();
        }
        ui.label("Opacity");
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut s.opacity, 0.05..=1.0).show_value(false));
            ui.label(format!("{:.0} %", s.opacity * 100.0));
        });
        ui.end_row();
    });
    if s.enabled {
        let colour = if st.status.starts_with("Zoom") { crate::theme::success() } else { crate::theme::warn() };
        ui.label(egui::RichText::new(&st.status).small().color(colour));
        if let Some(e) = &st.last_error {
            ui.label(egui::RichText::new(format!("Last tile error: {e}")).small().weak());
        }
    } else if project.is_none() {
        ui.label(egui::RichText::new("The map needs the project coordinate system (command CRS).").small().weak());
    }
    ui.label(egui::RichText::new("Tiles are fetched only for the view and cached on disk for 30 days. The map is a picture for orientation: it is not survey data, and its position is only as good as the provider's imagery and the datum transformation.").small().weak());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_quick_map_type_names_a_built_in_provider_without_a_key() {
        for (i, (label, id)) in MAP_TYPES.iter().enumerate() {
            let p = tiles::provider(id).unwrap_or_else(|| panic!("{label}: no provider {id}"));
            assert!(!p.needs_key, "{label}");
            assert_eq!(map_type(id), Some(i));
        }
        assert!(tiles::provider(MAP_TYPES[0].1).unwrap().imagery, "Satellite is a photograph");
        assert_eq!(map_type("mapbox-satellite"), None);
    }

    #[test]
    fn the_settings_round_trip_through_the_project_settings_and_start_off() {
        let mut doc = Document::default();
        assert!(!settings(&doc).enabled);
        let s = BasemapSettings { enabled: true, provider: "osm".into(), opacity: 0.6, ..Default::default() };
        set_settings(&mut doc, &s);
        let back = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(settings(&back), s);
    }

    #[test]
    fn the_source_needs_a_key_where_the_provider_does_and_checks_custom_urls() {
        let mut s = BasemapSettings::default();
        let (url, id, attr, _) = source(&s, "").unwrap();
        assert!(url.contains("World_Imagery") && id == "esri-imagery" && attr.contains("Esri"));
        s.provider = "mapbox-satellite".into();
        assert!(source(&s, "").unwrap_err().contains("key"));
        assert!(source(&s, "tok").is_ok());
        s.provider = CUSTOM.into();
        s.custom_url = "http://insecure/{z}/{x}/{y}".into();
        assert!(source(&s, "").is_err());
        s.custom_url = "https://t/{z}/{x}/{y}.png?k={key}".into();
        assert!(source(&s, "").is_err());
        let (_, id, _, _) = source(&s, "k").unwrap();
        assert!(id.starts_with("custom-"));
    }

    #[test]
    fn the_map_needs_a_projected_or_geographic_project_system() {
        assert!(plans_for(None).unwrap_err().contains("CRS"));
        assert!(plans_for(Some(&ProjectCrs::local_grid("Site"))).unwrap_err().contains("local"));
        assert!(plans_for(Some(&ProjectCrs::named_only(None, "Mystery"))).unwrap_err().contains("name only"));
        let (_, _, how) = plans_for(Some(&ProjectCrs::from_epsg(32636).unwrap())).unwrap();
        assert!(how.contains("WGS 84"));
        // Arc 1960 needs a datum step, which is named.
        let (_, _, how) = plans_for(Some(&ProjectCrs::from_epsg(21036).unwrap())).unwrap();
        assert!(how.contains("EPSG:"), "{how}");
    }

    #[test]
    fn a_tile_grid_lands_where_its_corners_project() {
        let utm = ProjectCrs::from_epsg(32636).unwrap();
        let (to, from, _) = plans_for(Some(&utm)).unwrap();
        // The tile over Kampala at zoom 15.
        let (tx, ty) = tiles::lonlat_to_tile(32.5825, 0.3476, 15);
        let g = tile_grid(&from, 15, tx as u32, ty as u32, 4).unwrap();
        assert_eq!(g.len(), 25);
        // Its north-west corner goes back to the same longitude and latitude.
        let (lon, lat) = tiles::tile_to_lonlat(tx.floor(), ty.floor(), 15);
        let back = to.apply(g[0].x / 1000.0, g[0].y / 1000.0, 0.0).unwrap();
        assert!((back.first - lat).abs() < 1e-8 && (back.second - lon).abs() < 1e-8);
        // A zoom 15 tile is about 1.2 km across at the equator.
        let width_m = (g[4].x - g[0].x) / 1000.0;
        assert!((width_m - 1222.0).abs() < 5.0, "{width_m}");
        // North is up: the first row is north of the last.
        assert!(g[0].y > g[20].y);
    }

    #[test]
    fn with_the_map_off_nothing_is_planned_or_fetched() {
        let ctx = egui::Context::default();
        let doc = Document::default();
        let mut st = BasemapState::default();
        let view = ViewTransform::centred_on(Pt::new(0.0, 0.0), Pt::new(400.0, 300.0), 1.0);
        assert!(!st.sync(&ctx, &doc, &view, (0.0, 0.0, 800.0, 600.0)));
        assert!(st.service.is_none());
        assert_eq!(st.status, "Off.");
    }

    #[test]
    fn with_a_project_system_tiles_are_planned_and_asked_for_but_tests_fetch_nothing() {
        let ctx = egui::Context::default();
        let mut doc = Document::default();
        afrobim_kernel::set_project_crs(&mut doc, &ProjectCrs::from_epsg(21036).unwrap());
        set_settings(&mut doc, &BasemapSettings { enabled: true, ..Default::default() });
        let mut st = BasemapState::default();
        // About 1 km of the ACP site, 800 px wide: about 1.25 m per pixel.
        let view = ViewTransform::centred_on(Pt::new(179_700_000.0, 9_909_000_000.0), Pt::new(400.0, 300.0), 0.0008);
        // The refusing fetcher answers at once; a few frames collect the answers.
        for _ in 0..50 {
            st.sync(&ctx, &doc, &view, (0.0, 0.0, 800.0, 600.0));
            if st.last_error.is_some() && st.pending() == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(st.status.starts_with("Zoom 17"), "{}", st.status);
        assert!(st.status.contains("EPSG:1284"), "{}", st.status);
        assert!(st.missing > 0);
        assert!(st.textures.is_empty());
        assert!(st.last_error.as_deref().unwrap_or("").contains("not used in tests"));
        assert!(st.attribution.contains("Esri"));
    }

    #[test]
    fn without_a_project_system_the_map_explains_itself_and_fetches_nothing() {
        let ctx = egui::Context::default();
        let mut doc = Document::default();
        set_settings(&mut doc, &BasemapSettings { enabled: true, ..Default::default() });
        let mut st = BasemapState::default();
        let view = ViewTransform::centred_on(Pt::new(0.0, 0.0), Pt::new(400.0, 300.0), 1.0);
        assert!(!st.sync(&ctx, &doc, &view, (0.0, 0.0, 800.0, 600.0)));
        assert!(st.status.contains("CRS"));
        assert!(st.service.is_none());
    }
}
