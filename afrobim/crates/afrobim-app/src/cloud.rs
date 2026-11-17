//! Point clouds in the document: the element that holds a `PointCloudReference` (no points), the
//! cache of opened indexes, the synthetic cloud of the demonstration, and the drawing of a cloud in
//! plan (a raster of the level-of-detail query for the current view, kept until the view changes)
//! and in 3D (a display buffer with a local origin, budgeted).
//!
//! The points stay in their file (and in the `.afrocloud` cache beside it). The document keeps the
//! reference, the display settings, a clip polygon or corridor, the footprint and, when the
//! document was transformed, the record of the transformations the points are drawn through.

use std::collections::HashMap;
use std::sync::Arc;

use afrobim_kernel::crs_plan::{plan_transform, TransformPlan};
use afrobim_kernel::{Document, ElementId, ProjectCrs};
use afrobim_pointcloud::*;
use serde::{Deserialize, Serialize};
use std::result::Result;

use crate::civil;

pub const CATEGORY: &str = civil::POINTCLOUD;
pub const SYNTHETIC_PATH: &str = "synthetic:demonstration";
pub const SYNTHETIC_LABEL: &str = "SYNTHETIC cloud generated on the fly for the demonstration: not a scan";

/// A transformation a document was moved through while it held this cloud: the cloud's points stay
/// in their file and are drawn through the same plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Moved {
    pub from_epsg: Option<u32>,
    pub to_epsg: Option<u32>,
    pub codes: Vec<u32>,
    pub zero_height: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Corridor {
    /// Plan polyline (project coordinates) and the half width either side, an input.
    pub line: Vec<[f64; 2]>,
    pub half_width_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudData {
    pub reference: PointCloudReference,
    /// Plan footprint (least and greatest easting and northing) in the project system.
    pub extent: Option<[[f64; 2]; 2]>,
    /// A clip polygon in plan (project coordinates); empty for none.
    #[serde(default)]
    pub clip: Vec<[f64; 2]>,
    #[serde(default)]
    pub corridor: Option<Corridor>,
    #[serde(default)]
    pub synthetic: bool,
    #[serde(default)]
    pub moved: Vec<Moved>,
    /// What `CrsCheck` said when the cloud was attached.
    #[serde(default)]
    pub crs_note: String,
}

pub fn data_of(doc: &Document, id: ElementId) -> Option<CloudData> {
    civil::get(doc, id, CATEGORY)
}

pub fn clouds(doc: &Document) -> Vec<(ElementId, String)> {
    civil::of_category(doc, CATEGORY)
}

pub fn store(doc: &mut Document, id: ElementId, d: &CloudData) {
    let b = d.extent.map(|e| civil::Bounds { min_e: e[0][0], min_n: e[0][1], max_e: e[1][0], max_n: e[1][1] });
    civil::set(doc, id, serde_json::to_value(d).unwrap_or_default(), b);
}

pub fn add(doc: &mut Document, d: &CloudData) -> ElementId {
    let b = d.extent.map(|e| civil::Bounds { min_e: e[0][0], min_n: e[0][1], max_e: e[1][0], max_n: e[1][1] });
    civil::add(doc, CATEGORY, &d.reference.name, serde_json::to_value(d).unwrap_or_default(), b, if d.synthetic { SYNTHETIC_LABEL } else { "Point cloud file" })
}

/// The coordinate system a file states, as the application's type, when it states one.
pub fn file_crs(h: &HeaderInfo) -> Option<ProjectCrs> {
    if let Some(e) = h.crs.epsg {
        if let Some(c) = ProjectCrs::from_epsg(e).or_else(|| ProjectCrs::geographic(e)) {
            return Some(c);
        }
    }
    h.crs.wkt.as_deref().and_then(ProjectCrs::from_wkt).or_else(|| h.crs.epsg.map(|e| ProjectCrs::named_only(Some(e), &format!("EPSG:{e}"))))
}

pub fn header_extent(h: &HeaderInfo) -> Option<[[f64; 2]; 2]> {
    h.bounds.map(|b| [[b.min[0], b.min[1]], [b.max[0], b.max[1]]])
}

// ─── the synthetic cloud ────────────────────────────────────────────────

/// A small cloud over the demonstration ground: ground points (class 2) on a grid with a little
/// deterministic noise, a few trees (class 5) and a shed (class 6), with colour and intensity.
pub fn synthetic_points(e0: f64, n0: f64) -> PointBuffer {
    let mut b = PointBuffer::new(true, false);
    let (w, h, step) = (300.0, 300.0, 1.0);
    let mut k = 0u32;
    let noise = |k: u32| (((k as f64) * 12.9898).sin() * 43758.5453).fract() - 0.5;
    let mut x = 0.0;
    while x <= w {
        let mut y = 0.0;
        while y <= h {
            k += 1;
            let (e, n) = (e0 + x + 0.2 * noise(k), n0 + y + 0.2 * noise(k + 7));
            let z = crate::civil_demo::ground_z(e, n) + 0.03 * noise(k + 13);
            let g = (90.0 + 60.0 * ((x / 40.0).sin() * (y / 33.0).cos())) as u16;
            b.push(&PointRecord { xyz: [e, n, z], intensity: 1000 + (noise(k + 3) * 400.0 + 200.0) as u16, class: 2, return_number: 1, number_of_returns: 1, rgb: Some([(g + 40) * 256, (g + 70) * 256, g * 256]), gps_time: None });
            y += step;
        }
        x += step;
    }
    // Trees: a crown of points above the ground.
    for (i, (tx, ty)) in [(40.0, 60.0), (95.0, 210.0), (180.0, 90.0), (250.0, 240.0), (60.0, 140.0), (210.0, 30.0)].into_iter().enumerate() {
        for j in 0..220u32 {
            k += 1;
            let a = j as f64 * 2.399963;
            let r = 1.0 + 3.0 * ((j as f64) / 220.0).sqrt();
            let (e, n) = (e0 + tx + r * a.cos(), n0 + ty + r * a.sin());
            let z = crate::civil_demo::ground_z(e, n) + 4.0 + 4.0 * (1.0 - r / 4.0) + 0.3 * noise(k + i as u32);
            b.push(&PointRecord { xyz: [e, n, z], intensity: 600, class: 5, return_number: 1, number_of_returns: 2, rgb: Some([40 * 256, 120 * 256, 50 * 256]), gps_time: None });
        }
    }
    // A shed: roof and walls.
    for ix in 0..24 {
        for iy in 0..14 {
            let (e, n) = (e0 + 120.0 + ix as f64 * 0.5, n0 + 20.0 + iy as f64 * 0.5);
            let z = crate::civil_demo::ground_z(e0 + 126.0, n0 + 23.0) + 3.5;
            b.push(&PointRecord { xyz: [e, n, z], intensity: 3000, class: 6, return_number: 1, number_of_returns: 1, rgb: Some([180 * 256, 170 * 256, 160 * 256]), gps_time: None });
        }
    }
    b
}

pub fn synthetic_header(buf: &PointBuffer) -> HeaderInfo {
    let bounds = Bounds3::of(&buf.xyz);
    HeaderInfo {
        kind: SourceKind::Text,
        version: String::new(),
        point_format: None,
        point_count: buf.len() as u64,
        bounds: Some(bounds),
        bounds_exact: true,
        scale: None,
        offset: None,
        has_rgb: true,
        has_gps_time: false,
        has_intensity: true,
        compressed: false,
        crs: CrsInfo::default(),
        class_table: ClassTable::for_las_format(1),
        generating_software: "AfroBIM demonstration generator".into(),
        system_identifier: "synthetic".into(),
        from_scan: true,
    }
}

pub fn synthetic_data(e0: f64, n0: f64) -> (CloudData, PointBuffer) {
    let buf = synthetic_points(e0, n0);
    let header = synthetic_header(&buf);
    let mut display = DisplaySettings::starting();
    display.max_points_per_view = 250_000;
    let reference = PointCloudReference { name: "Demo point cloud (synthetic)".into(), source_path: SYNTHETIC_PATH.into(), source_hash: "synthetic".into(), crs: header.crs.clone(), header: header.clone(), text_format: None, e57_apply_pose: true, filter: PointFilter::default(), display };
    (CloudData { reference, extent: header_extent(&header), clip: vec![], corridor: None, synthetic: true, moved: vec![], crs_note: "Generated at the project's coordinates: no file, no coordinate system to check.".into() }, buf)
}

// ─── the opened index ───────────────────────────────────────────────────

pub struct Loaded {
    pub index: Arc<PointIndex>,
    pub status: String,
    /// The hash the index was made for.
    pub hash: String,
    pub memory_bytes: usize,
}

/// What a finished open or attach hands back.
pub struct OpenOut {
    pub index: PointIndex,
    pub status: String,
    pub hash: String,
}

/// Opens (and indexes through the cache file beside the source) a file. `stage` is told what is
/// being done, for the progress window.
pub fn open_file(path: &std::path::Path, opts: &SourceOptions, stage: &dyn Fn(&str)) -> Result<(OpenOut, PointCloudReference), String> {
    stage("Reading the header and hashing the file");
    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Point cloud".into());
    let reference = PointCloudReference::from_file(&name, path, opts).map_err(|e| e.to_string())?;
    stage("Indexing (or reading the cache file beside the source)");
    let opened = reference.open(&IndexOptions::default()).map_err(|e| e.to_string())?;
    let status = match &opened.cache {
        CacheStatus::Hit => format!("index read from the cache file {}", opened.cache_path.display()),
        CacheStatus::Built => format!("index built and written to {}", opened.cache_path.display()),
        CacheStatus::Rebuilt { reason } => format!("index rebuilt ({reason}) and written to {}", opened.cache_path.display()),
    } + &opened.cache_write_error.as_ref().map(|e| format!("; the cache could not be written: {e}")).unwrap_or_default();
    Ok((OpenOut { hash: opened.source_hash.clone(), index: opened.index, status }, reference))
}

// ─── transformations the points are drawn through ───────────────────────

pub fn chain_plans(d: &CloudData) -> Vec<TransformPlan> {
    let sys = |e: Option<u32>| e.and_then(|c| ProjectCrs::from_epsg(c).or_else(|| ProjectCrs::geographic(c)));
    d.moved
        .iter()
        .filter_map(|m| {
            let (from, to) = (sys(m.from_epsg)?, sys(m.to_epsg)?);
            let plans = plan_transform(&from, &to, None).ok()?;
            let p = plans.iter().find(|p| p.transformation_codes() == m.codes).or_else(|| plans.first())?.clone();
            Some(if m.zero_height { p.with_height_assumption(afrobim_kernel::crs_plan::HeightAssumption::Zero) } else { p })
        })
        .collect()
}

pub fn map_chain(plans: &[TransformPlan], e: f64, n: f64, z: f64) -> Option<(f64, f64)> {
    let mut p = (e, n);
    for plan in plans {
        let r = plan.apply(p.0, p.1, z).ok()?;
        p = (r.first, r.second);
    }
    Some(p)
}

pub fn map_chain_back(plans: &[TransformPlan], e: f64, n: f64, z: f64) -> Option<(f64, f64)> {
    let mut p = (e, n);
    for plan in plans.iter().rev() {
        let r = plan.reversed().apply(p.0, p.1, z).ok()?;
        p = (r.first, r.second);
    }
    Some(p)
}

/// Records, on every cloud in scope, that the document was moved through `plan`.
pub fn note_move(doc: &mut Document, ids: &[ElementId], plan: &TransformPlan) {
    for id in ids {
        let Some(mut d) = data_of(doc, *id) else { continue };
        d.moved.push(Moved { from_epsg: plan.from.epsg, to_epsg: plan.to.epsg, codes: plan.transformation_codes(), zero_height: plan.height == afrobim_kernel::crs_plan::HeightAssumption::Zero });
        if let Some(e) = doc.find_mut(*id) {
            e.data = serde_json::to_value(&d).unwrap_or_default();
        }
    }
}

// ─── clip tests ─────────────────────────────────────────────────────────

fn point_in_ring(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let n = ring.len();
    if n < 3 {
        return true;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (ring[i], ring[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn dist_to_polyline(p: [f64; 2], line: &[[f64; 2]]) -> f64 {
    line.windows(2)
        .map(|w| {
            let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
            let l2 = dx * dx + dy * dy;
            let t = if l2 > 0.0 { (((p[0] - w[0][0]) * dx + (p[1] - w[0][1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
            (p[0] - (w[0][0] + t * dx)).hypot(p[1] - (w[0][1] + t * dy))
        })
        .fold(f64::INFINITY, f64::min)
}

/// Whether a plan position (project coordinates) is inside the clip polygon and the corridor, where set.
pub fn inside_clip(d: &CloudData, e: f64, n: f64) -> bool {
    (d.clip.len() < 3 || point_in_ring([e, n], &d.clip)) && d.corridor.as_ref().is_none_or(|c| c.line.len() < 2 || dist_to_polyline([e, n], &c.line) <= c.half_width_m)
}

/// Indices of the points that pass the cloud's class filter and clip, in the index's buffer.
pub fn selected_indices(d: &CloudData, index: &PointIndex, plans: &[TransformPlan]) -> Vec<u32> {
    let buf = index.points();
    let idx = if d.reference.filter.is_empty() { (0..buf.len() as u32).collect() } else { d.reference.filter.apply(buf) };
    if d.clip.len() < 3 && d.corridor.is_none() {
        return idx;
    }
    idx.into_iter()
        .filter(|&i| {
            let p = buf.xyz[i as usize];
            match map_chain(plans, p[0], p[1], p[2]) {
                Some((e, n)) => inside_clip(d, e, n),
                None => false,
            }
        })
        .collect()
}

// ─── drawing in plan: a raster of the view ──────────────────────────────

pub struct Raster {
    pub key: u64,
    /// The world rectangle the texture covers, model millimetres: min x, min y, max x, max y.
    pub rect_mm: [f64; 4],
    pub tex: egui::TextureHandle,
    pub points: usize,
    pub depth: u32,
    pub complete: bool,
    pub ms: f64,
}

#[derive(Default)]
pub struct CloudRender {
    pub rasters: HashMap<ElementId, Raster>,
    /// When the view last changed, per cloud (seconds), so a raster is rebuilt when it settles.
    pub changed_at: HashMap<ElementId, f64>,
    /// Display buffers for the 3D view: element to (key, positions relative to origin, colours, origin).
    pub three_d: HashMap<ElementId, (u64, Vec<[f32; 3]>, Vec<[u8; 3]>, [f64; 3])>,
}

pub fn hash_of(parts: &[String]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        p.hash(&mut h);
    }
    h.finish()
}

/// What colours the points: the stored mode, or the comparison bands when one is shown.
pub fn colours_for(buf: &PointBuffer, indices: &[u32], mode: &ColorMode, compare: Option<&Compare>) -> Vec<[u8; 3]> {
    match compare {
        Some(c) => indices
            .iter()
            .map(|i| match c.indices.binary_search(i) {
                Ok(k) => c.colors[k],
                Err(_) => [90, 90, 90],
            })
            .collect(),
        None => colorize(buf, Some(indices), mode),
    }
}

/// A cloud-against-surface comparison to show on the cloud: colour per compared point.
#[derive(Clone)]
pub struct Compare {
    pub indices: Vec<u32>,
    pub colors: Vec<[u8; 3]>,
}

/// The raster for the current view of a cloud. `view_lo` and `view_hi` are the visible corners in
/// project coordinates (metres). The result is `None` when the view is outside the cloud.
#[allow(clippy::too_many_arguments)]
pub fn build_raster(ctx: &egui::Context, d: &CloudData, index: &PointIndex, plans: &[TransformPlan], view_lo: [f64; 2], view_hi: [f64; 2], size_px: [usize; 2], compare: Option<&Compare>) -> Option<Raster> {
    let started = std::time::Instant::now();
    let (w, h) = (size_px[0].max(8), size_px[1].max(8));
    // The view in the cloud's own coordinates.
    let corners = [[view_lo[0], view_lo[1]], [view_hi[0], view_lo[1]], [view_hi[0], view_hi[1]], [view_lo[0], view_hi[1]]];
    let mut b = Bounds3::EMPTY;
    for c in corners {
        let (e, n) = if plans.is_empty() { (c[0], c[1]) } else { map_chain_back(plans, c[0], c[1], 0.0)? };
        b.include([e, n, 0.0]);
    }
    let ib = index.bounds();
    let view = Bounds3::new([b.min[0], b.min[1], ib.min[2] - 1.0], [b.max[0], b.max[1], ib.max[2] + 1.0]);
    if !view.intersects(&ib) {
        return None;
    }
    let budget = d.reference.display.max_points_per_view.max(1);
    let lod = index.lod(&view, budget);
    let buf = index.points();
    let filter = d.reference.filter.compile();
    let keep: Vec<u32> = lod
        .indices
        .iter()
        .copied()
        .filter(|&i| d.reference.filter.is_empty() || filter.matches(buf, i as usize))
        .filter(|&i| {
            if d.clip.len() < 3 && d.corridor.is_none() {
                return true;
            }
            let p = buf.xyz[i as usize];
            map_chain(plans, p[0], p[1], p[2]).is_some_and(|(e, n)| inside_clip(d, e, n))
        })
        .collect();
    let colours = colours_for(buf, &keep, &d.reference.display.color_mode, compare);
    let disp = index.display_buffer(&keep, &colours);
    // The raster: one pixel per view pixel; a point is a square of the display size.
    let (sx, sy) = (w as f64 / (view_hi[0] - view_lo[0]).max(1e-9), h as f64 / (view_hi[1] - view_lo[1]).max(1e-9));
    let mut pixels = vec![egui::Color32::TRANSPARENT; w * h];
    let size = d.reference.display.point_size.max(1.0) as i64;
    for (k, p) in disp.positions.iter().enumerate() {
        let (e0, n0) = (disp.origin[0] + p[0] as f64, disp.origin[1] + p[1] as f64);
        let (e, n) = if plans.is_empty() { (e0, n0) } else { match map_chain(plans, e0, n0, disp.origin[2] + p[2] as f64) { Some(v) => v, None => continue } };
        let (px, py) = (((e - view_lo[0]) * sx) as i64, ((view_hi[1] - n) * sy) as i64);
        let c = disp.colors[k];
        let col = egui::Color32::from_rgb(c[0], c[1], c[2]);
        for dy in 0..size {
            for dx in 0..size {
                let (x, y) = (px + dx - size / 2, py + dy - size / 2);
                if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                    pixels[y as usize * w + x as usize] = col;
                }
            }
        }
    }
    let image = egui::ColorImage { size: [w, h], pixels };
    let tex = ctx.load_texture("afrobim-cloud", image, egui::TextureOptions::NEAREST);
    Some(Raster { key: 0, rect_mm: [view_lo[0] * 1000.0, view_lo[1] * 1000.0, view_hi[0] * 1000.0, view_hi[1] * 1000.0], tex, points: keep.len(), depth: lod.depth, complete: lod.complete, ms: started.elapsed().as_secs_f64() * 1000.0 })
}

/// The display buffer for the 3D view: a budgeted level-of-detail sample of the whole cloud, in
/// project coordinates relative to a local origin.
pub fn build_3d(d: &CloudData, index: &PointIndex, plans: &[TransformPlan], budget: usize, compare: Option<&Compare>) -> (Vec<[f32; 3]>, Vec<[u8; 3]>, [f64; 3]) {
    let lod = index.lod(&index.bounds(), budget.max(1));
    let buf = index.points();
    let filter = d.reference.filter.compile();
    let keep: Vec<u32> = lod.indices.iter().copied().filter(|&i| d.reference.filter.is_empty() || filter.matches(buf, i as usize)).collect();
    let colours = colours_for(buf, &keep, &d.reference.display.color_mode, compare);
    let disp = index.display_buffer(&keep, &colours);
    if plans.is_empty() && d.clip.len() < 3 && d.corridor.is_none() {
        return (disp.positions, disp.colors, disp.origin);
    }
    // Moved or clipped: go through project coordinates, with a new local origin.
    let mut pts: Vec<([f64; 3], [u8; 3])> = Vec::with_capacity(keep.len());
    for (k, p) in disp.positions.iter().enumerate() {
        let (e0, n0, z) = (disp.origin[0] + p[0] as f64, disp.origin[1] + p[1] as f64, disp.origin[2] + p[2] as f64);
        let Some((e, n)) = map_chain(plans, e0, n0, z) else { continue };
        if inside_clip(d, e, n) {
            pts.push(([e, n, z], disp.colors[k]));
        }
    }
    let origin = pts.first().map(|p| p.0).unwrap_or([0.0; 3]);
    (pts.iter().map(|p| [(p.0[0] - origin[0]) as f32, (p.0[1] - origin[1]) as f32, (p.0[2] - origin[2]) as f32]).collect(), pts.iter().map(|p| p.1).collect(), origin)
}

/// The class codes present in an index, with counts, for the class filter.
pub fn class_counts(index: &PointIndex) -> Vec<(u8, u64)> {
    let mut h = ClassHistogram::default();
    h.add_buffer(index.points());
    h.counts.iter().map(|(c, n)| (*c, *n)).collect()
}
