// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Georeferenced images behind the model: orthophotos from a drone survey, satellite scenes,
//! scanned plans with a world file.
//!
//! An image underlay is a document element (category [`CATEGORY`]) that holds the file's path and
//! a change-detection hash, the file's **own** placement (a six-term affine from pixels to the
//! image's coordinate system) and that system, and the display switches. The pixels are never
//! stored in the document: they are read again from the file when the document opens, on a worker
//! thread, decimated to the detail chosen so that a large orthophoto does not fill the memory.
//!
//! The model placement is derived, not stored: a grid of points over the image is moved from the
//! image's system to the project's (through the kernel's transformation plans when the two
//! differ), and the texture is drawn over that grid. Changing the project coordinate system
//! re-places the image; nothing is invented when there is no transformation between the two (the
//! window says why the image cannot be placed).
//!
//! The placement comes from the GeoTIFF tags or a world file (`.pgw`, `.jgw`, `.tfw`, `.wld`)
//! beside the image, and the system from the GeoTIFF keys or a `.prj` beside it. An image with
//! neither is placed by hand: the top-left corner and the pixel size.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};

use afrobim_kernel::{project_crs, Document, Element, ElementId, Generated, Module, ProjectCrs, Pt, ViewTransform};
use afrobim_raster::georef::{resolve_crs, CrsRelation};
use afrobim_raster::{Affine, DecodeOptions, Decoded};
use egui::{pos2, Color32, Mesh, Painter, Rect, TextureHandle, TextureId};
use serde::{Deserialize, Serialize};

pub const CATEGORY: &str = "image_underlay";
/// The image extensions offered and accepted when dropped.
pub const EXTENSIONS: &[&str] = &["tif", "tiff", "png", "jpg", "jpeg"];

/// The detail choices (the longest side of the texture, pixels).
pub const DETAILS: &[(u32, &str)] = &[(2048, "Draft (2048 px)"), (4096, "Normal (4096 px)"), (8192, "Fine (8192 px)")];

/// What the document stores for one image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageDef {
    pub path: String,
    /// FNV-1a 64 of the file at attach (or when a change was accepted), hex; empty until read.
    pub hash: String,
    pub size: u64,
    /// Full-resolution size of the image, pixels.
    pub src_px: [u32; 2],
    /// Pixel corners to the image's system (a, b, c, d, e, f): see [`Affine`]. `None` until read,
    /// and for an image that carries no placement and has not been placed by hand.
    pub affine: Option<[f64; 6]>,
    /// Where the placement came from: "world file x.pgw", "GeoTIFF tags", "placed by hand".
    pub placed_from: String,
    /// The image's coordinate system, when known.
    pub crs: Option<ProjectCrs>,
    /// Where the system came from: "GeoTIFF keys", "x.prj", "chosen by hand".
    pub crs_from: String,
    pub visible: bool,
    pub opacity: f32,
    /// Longest side of the texture, pixels.
    pub detail: u32,
    /// Pure black pixels are not drawn (the collar of an orthophoto mosaic).
    pub transparent_black: bool,
    /// Draw order among the images: higher is drawn later, over the others. The online map is
    /// always below every image. Absent in documents saved before it existed (read as 0).
    #[serde(default)]
    pub order: i32,
}

impl ImageDef {
    fn new(path: &Path) -> ImageDef {
        ImageDef {
            path: path.to_string_lossy().to_string(),
            hash: String::new(),
            size: 0,
            src_px: [0, 0],
            affine: None,
            placed_from: String::new(),
            crs: None,
            crs_from: String::new(),
            visible: true,
            opacity: 1.0,
            detail: 4096,
            transparent_black: true,
            order: 0,
        }
    }

    fn decode_key(&self) -> (u32, bool) {
        (self.detail, self.transparent_black)
    }
}

/// True for the read-only background records (DXF underlays, images, GIS overlays): they
/// hold a reference and draw behind the model, and no export, quantity or clash test may
/// treat them as objects.
pub fn is_backdrop(category: &str) -> bool {
    category == CATEGORY || category == crate::underlay::CATEGORY || category == crate::gis_overlay::CATEGORY
}

pub fn def_of(e: &Element) -> Option<ImageDef> {
    (e.category == CATEGORY).then(|| serde_json::from_value(e.data.clone()).ok()).flatten()
}

pub fn set_def(doc: &mut Document, id: ElementId, def: &ImageDef) {
    if let (Some(e), Ok(v)) = (doc.find_mut(id), serde_json::to_value(def)) {
        e.data = v;
    }
}

/// Before the project system changes under the drawing (TRANSFORM, site calibration), writes the
/// old system onto every image that has none of its own. An image without a system is read in
/// the project system, so without this it would silently be reread in the new one and land in
/// the wrong place. Returns how many images were pinned.
pub fn pin_crs(doc: &mut Document, old: Option<&ProjectCrs>) -> usize {
    let Some(old) = old else { return 0 };
    let ids: Vec<(ElementId, ImageDef)> = doc.elements().iter().filter_map(|e| def_of(e).filter(|d| d.crs.is_none()).map(|d| (e.id, d))).collect();
    for (id, mut def) in ids.iter().cloned() {
        def.crs = Some(old.clone());
        def.crs_from = "project system before a transform".into();
        set_def(doc, id, &def);
    }
    ids.len()
}

/// Moves the placement of every image without a system of its own by a plane map (a site
/// calibration, which is a similarity, so three corners fix the new affine exactly). Used where
/// the project system changes from a local grid, in which [`pin_crs`] would leave the image in a
/// system nothing can compute with. Returns how many images were moved.
pub fn move_unpinned(doc: &mut Document, f: &dyn Fn(f64, f64) -> (f64, f64)) -> usize {
    let ids: Vec<(ElementId, ImageDef)> = doc.elements().iter().filter_map(|e| def_of(e).filter(|d| d.crs.is_none() && d.affine.is_some()).map(|d| (e.id, d))).collect();
    for (id, mut def) in ids.iter().cloned() {
        let a = Affine::from_array(def.affine.unwrap());
        let (o, x, y) = (f(a.c, a.f), { let p = a.apply(1.0, 0.0); f(p.0, p.1) }, { let p = a.apply(0.0, 1.0); f(p.0, p.1) });
        def.affine = Some([x.0 - o.0, y.0 - o.0, o.0, x.1 - o.1, y.1 - o.1, o.1]);
        def.placed_from = format!("{} (moved by a site calibration)", def.placed_from);
        set_def(doc, id, &def);
    }
    ids.len()
}

/// The images in the order they are drawn: by `order`, then by their place in the document.
pub fn paint_order(doc: &Document) -> Vec<ElementId> {
    let mut v: Vec<(i32, usize, ElementId)> = doc.elements().iter().enumerate().filter_map(|(i, e)| def_of(e).map(|d| (d.order, i, e.id))).collect();
    v.sort();
    v.into_iter().map(|(_, _, id)| id).collect()
}

/// Moves an image one step up (`forward`) or down the draw order, renumbering every image
/// 0, 1, 2 ... so the orders stay distinct. False when it is already at that end.
pub fn shift_order(doc: &mut Document, id: ElementId, forward: bool) -> bool {
    let mut ids = paint_order(doc);
    let Some(i) = ids.iter().position(|x| *x == id) else { return false };
    let j = if forward { i + 1 } else { i.wrapping_sub(1) };
    if j >= ids.len() {
        return false;
    }
    ids.swap(i, j);
    for (k, id) in ids.iter().enumerate() {
        if let Some(mut d) = doc.find(*id).and_then(def_of) {
            d.order = k as i32;
            set_def(doc, *id, &d);
        }
    }
    true
}

/// The module of the placeholder element: it draws nothing.
pub struct ImageModule;

impl Module for ImageModule {
    fn category(&self) -> &str {
        CATEGORY
    }
    fn display_name(&self) -> &str {
        "Image underlays"
    }
    fn generate(&self, e: &Element) -> Result<Generated, String> {
        let def = def_of(e).ok_or("The image underlay record cannot be read.")?;
        let mut g = Generated::default();
        g.derived.insert("File".into(), def.path.clone());
        if def.src_px[0] > 0 {
            g.derived.insert("Pixels".into(), format!("{} x {}", def.src_px[0], def.src_px[1]));
        }
        if let Some(a) = def.affine.map(Affine::from_array) {
            let (sx, sy) = a.pixel_size();
            g.derived.insert("Ground pixel".into(), format!("{sx:.3} x {sy:.3}"));
        }
        if let Some(c) = &def.crs {
            g.derived.insert("Image coordinate system".into(), c.label());
        }
        g.drawing.extents = afrobim_draw::Extents { min: Pt::new(0.0, 0.0), max: Pt::new(1.0, 1.0) };
        Ok(g)
    }
}

// ─── placement ──────────────────────────────────────────────────────────────

/// Where the image is drawn: an `(n + 1)` by `(n + 1)` grid of model points (millimetres), row by
/// row from the top-left pixel corner, and how it was placed, in words.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub n: usize,
    pub pts: Vec<Pt>,
    pub how: String,
    pub lo: Pt,
    pub hi: Pt,
}

/// The grid of an image in the project. One cell when the numbers are used as they are (the
/// mapping is affine); 8 by 8 when they go through a transformation, so that the curvature of a
/// change of projection is followed.
pub fn place(def: &ImageDef, project: Option<&ProjectCrs>) -> Result<Placement, String> {
    let a = def.affine.map(Affine::from_array).ok_or("The image has no placement yet.")?;
    let [w, h] = def.src_px;
    if w == 0 || h == 0 {
        return Err("The image has not been read yet.".into());
    }
    let grid = |n: usize, f: &dyn Fn(f64, f64) -> Result<Pt, String>| -> Result<Vec<Pt>, String> {
        let mut pts = Vec::with_capacity((n + 1) * (n + 1));
        for j in 0..=n {
            for i in 0..=n {
                let (x, y) = a.apply(w as f64 * i as f64 / n as f64, h as f64 * j as f64 / n as f64);
                pts.push(f(x, y)?);
            }
        }
        Ok(pts)
    };
    let (n, pts, how) = match resolve_crs(def.crs.as_ref(), project) {
        CrsRelation::AsIs(how) => (1, grid(1, &|x, y| Ok(Pt::new(x * 1000.0, y * 1000.0)))?, how),
        CrsRelation::Cannot(why) => return Err(why),
        CrsRelation::Transform { from, to } => {
            let cs = a.corners(w, h);
            let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
            for (x, y) in cs {
                lo = (lo.0.min(x), lo.1.min(y));
                hi = (hi.0.max(x), hi.1.max(y));
            }
            let plans = afrobim_kernel::plan_transform(&from, &to, Some(afrobim_kernel::DataExtent::new(lo, hi))).map_err(|e| format!("The image is in {} and the project in {}: {e}. The image cannot be placed.", from.label(), to.label()))?;
            let plan = plans.into_iter().next().ok_or("No transformation was found.")?;
            let pts = grid(8, &|x, y| plan.apply(x, y, 0.0).map(|p| Pt::new(p.first * 1000.0, p.second * 1000.0)).map_err(|e| e.to_string()))?;
            (8, pts, format!("Moved from {} to {}{}.", from.label(), to.label(), plan_words(&plan)))
        }
    };
    let (mut lo, mut hi) = (Pt::new(f64::MAX, f64::MAX), Pt::new(f64::MIN, f64::MIN));
    for p in &pts {
        lo = Pt::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Pt::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    Ok(Placement { n, pts, how, lo, hi })
}

/// " through EPSG:1122, stated accuracy 20 m", or "" for a change of projection only.
pub fn plan_words(plan: &afrobim_kernel::TransformPlan) -> String {
    let codes = plan.transformation_codes();
    if codes.is_empty() {
        return String::new();
    }
    let list = codes.iter().map(|c| format!("EPSG:{c}")).collect::<Vec<_>>().join(" then ");
    match plan.accuracy_m.filter(|a| *a > 0.0) {
        Some(a) => format!(" through {list} (stated accuracy {a:.0} m)"),
        None => format!(" through {list}"),
    }
}

// ─── painting a texture over a grid ─────────────────────────────────────────

/// Draws `tex` over a grid of model points (see [`Placement`]), `uv` being the part of the texture
/// the grid covers (left, top, right, bottom, 0 to 1). A cell that is far larger than the screen
/// (a deep zoom on a large image) is split in four until its pieces are near the screen's size,
/// and pieces off the screen are dropped, so that no vertex is ever millions of pixels away when it
/// is cast to f32 (model millimetres in UTM are around 1e10).
#[allow(clippy::too_many_arguments)]
pub fn paint_grid(painter: &Painter, view: &ViewTransform, rect: Rect, tex: TextureId, n: usize, pts: &[Pt], uv: [f32; 4], tint: Color32) {
    if pts.len() != (n + 1) * (n + 1) {
        return;
    }
    let mut mesh = Mesh::with_texture(tex);
    let limit = 4.0 * rect.width().max(rect.height()).max(64.0) as f64;
    let at = |i: usize, j: usize| pts[j * (n + 1) + i];
    let uv_at = |i: usize, j: usize| [uv[0] + (uv[2] - uv[0]) * i as f32 / n as f32, uv[1] + (uv[3] - uv[1]) * j as f32 / n as f32];
    for j in 0..n {
        for i in 0..n {
            let cell = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
            let uvs = [uv_at(i, j), uv_at(i + 1, j), uv_at(i + 1, j + 1), uv_at(i, j + 1)];
            cell_into(&mut mesh, view, rect, cell, uvs, tint, limit, 0);
        }
    }
    if !mesh.is_empty() {
        painter.add(egui::Shape::mesh(mesh));
    }
}

#[allow(clippy::too_many_arguments)]
fn cell_into(mesh: &mut Mesh, view: &ViewTransform, rect: Rect, m: [Pt; 4], uv: [[f32; 2]; 4], tint: Color32, limit: f64, depth: u32) {
    let s: Vec<Pt> = m.iter().map(|p| view.to_screen(*p)).collect();
    let (x0, x1) = s.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
    let (y0, y1) = s.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
    if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
        return;
    }
    if x1 < rect.left() as f64 - 2.0 || x0 > rect.right() as f64 + 2.0 || y1 < rect.top() as f64 - 2.0 || y0 > rect.bottom() as f64 + 2.0 {
        return;
    }
    if (x1 - x0).max(y1 - y0) > limit && depth < 24 {
        // Split in four at the bilinear midpoints (in f64 model space).
        let mid = |a: Pt, b: Pt| Pt::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        let muv = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let (t, r, b, l) = (mid(m[0], m[1]), mid(m[1], m[2]), mid(m[3], m[2]), mid(m[0], m[3]));
        let c = mid(t, b);
        let (tu, ru, bu, lu) = (muv(uv[0], uv[1]), muv(uv[1], uv[2]), muv(uv[3], uv[2]), muv(uv[0], uv[3]));
        let cu = muv(tu, bu);
        cell_into(mesh, view, rect, [m[0], t, c, l], [uv[0], tu, cu, lu], tint, limit, depth + 1);
        cell_into(mesh, view, rect, [t, m[1], r, c], [tu, uv[1], ru, cu], tint, limit, depth + 1);
        cell_into(mesh, view, rect, [c, r, m[2], b], [cu, ru, uv[2], bu], tint, limit, depth + 1);
        cell_into(mesh, view, rect, [l, c, b, m[3]], [lu, cu, bu, uv[3]], tint, limit, depth + 1);
        return;
    }
    let base = mesh.vertices.len() as u32;
    for k in 0..4 {
        mesh.vertices.push(egui::epaint::Vertex { pos: pos2(s[k].x as f32, s[k].y as f32), uv: pos2(uv[k][0], uv[k][1]), color: tint });
    }
    mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

// ─── the runtime side ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ImgStatus {
    Reading,
    Ok,
    Missing,
    Failed(String),
    Changed { was: String, now: String },
}

impl ImgStatus {
    pub fn line(&self) -> String {
        match self {
            ImgStatus::Reading => "Reading the file...".into(),
            ImgStatus::Ok => "Read.".into(),
            ImgStatus::Missing => "The file is missing: nothing is drawn. Attach it again or put the file back.".into(),
            ImgStatus::Failed(e) => format!("The file could not be read: {e}"),
            ImgStatus::Changed { .. } => "The file has changed since it was attached.".into(),
        }
    }
}

struct Loaded {
    hash: String,
    size: u64,
    georef: Result<Option<afrobim_raster::Georef>, String>,
    decoded: Decoded,
    key: (u32, bool),
}

type Msg = (ElementId, Result<Loaded, (bool, String)>);

struct Entry {
    status: ImgStatus,
    tex: Option<TextureHandle>,
    key: Option<(u32, bool)>,
    loading: Option<(u32, bool)>,
    notes: Vec<String>,
    /// Placement and the inputs it was made from.
    placed: Option<(String, Result<Placement, String>)>,
    texture_px: [u32; 2],
}

/// The runtime side of the image underlays: textures, placements, the reads in flight.
pub struct ImageryState {
    entries: HashMap<ElementId, Entry>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub window_open: bool,
    /// Set when a worker result changed the document (the caller takes an undo step).
    pub doc_changed: bool,
    /// The texture limit of the GPU, from the context.
    max_texture: u32,
    /// Hand placement fields of the window, per image: left, top (metres) and pixel size.
    hand: HashMap<ElementId, [String; 3]>,
    /// EPSG code typed in the window, per image.
    epsg_text: HashMap<ElementId, String>,
}

impl Default for ImageryState {
    fn default() -> Self {
        let (tx, rx) = channel();
        ImageryState { entries: HashMap::new(), tx, rx, window_open: false, doc_changed: false, max_texture: 8192, hand: HashMap::new(), epsg_text: HashMap::new() }
    }
}

impl ImageryState {
    /// Forgets everything (a different document).
    pub fn reset(&mut self) {
        let open = self.window_open;
        *self = ImageryState::default();
        self.window_open = open;
    }

    pub fn busy(&self) -> bool {
        self.entries.values().any(|e| e.loading.is_some())
    }

    /// Adds the record and starts reading.
    pub fn attach(&mut self, doc: &mut Document, path: &Path) -> Result<ElementId, String> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !path.is_file() {
            return Err(format!("{} does not exist.", path.display()));
        }
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Image".into());
        let name = doc.unique_name(&format!("IMG-{stem}"));
        doc.ensure_layer(crate::underlay::LAYER);
        if let Some(l) = doc.layers_mut().iter_mut().find(|l| l.name == crate::underlay::LAYER) {
            l.visible = false;
        }
        let mut def = ImageDef::new(&path);
        // A new image goes on top of the ones already attached.
        def.order = doc.elements().iter().filter_map(def_of).map(|d| d.order + 1).max().unwrap_or(0);
        let id = doc.add(CATEGORY, &name, Pt::new(0.0, 0.0), 1.0, 1.0, crate::underlay::LAYER, serde_json::to_value(&def).map_err(|e| e.to_string())?);
        self.start(id, &def, true);
        Ok(id)
    }

    pub fn detach(&mut self, doc: &mut Document, id: ElementId) {
        doc.remove(id);
        self.entries.remove(&id);
    }

    fn start(&mut self, id: ElementId, def: &ImageDef, read_georef: bool) {
        let key = def.decode_key();
        let opts = DecodeOptions { max_side: def.detail.min(self.max_texture).max(256), transparent_black: def.transparent_black };
        let en = self.entries.entry(id).or_insert_with(|| Entry { status: ImgStatus::Reading, tex: None, key: None, loading: None, notes: Vec::new(), placed: None, texture_px: [0, 0] });
        en.loading = Some(key);
        if en.tex.is_none() {
            en.status = ImgStatus::Reading;
        }
        let (tx, path) = (self.tx.clone(), std::path::PathBuf::from(&def.path));
        std::thread::spawn(move || {
            let result = if !path.is_file() {
                Err((true, "missing".to_string()))
            } else {
                crate::underlay::hash_file(&path).map_err(|e| (false, e.to_string())).and_then(|(hash, size)| {
                    let georef = if read_georef { afrobim_raster::read_georef(&path) } else { Ok(None) };
                    let decoded = afrobim_raster::decode(&path, opts).map_err(|e| (false, e))?;
                    Ok(Loaded { hash, size, georef, decoded, key })
                })
            };
            let _ = tx.send((id, result));
        });
    }

    /// Once a frame: starts reads for records that have none, applies what the workers finished,
    /// reloads at a new detail, and re-places images whose inputs changed.
    pub fn sync(&mut self, ctx: &egui::Context, doc: &mut Document) {
        self.max_texture = (ctx.input(|i| i.max_texture_side) as u32).clamp(1024, 16384);
        let ids: Vec<ElementId> = doc.elements().iter().filter(|e| e.category == CATEGORY).map(|e| e.id).collect();
        self.entries.retain(|id, _| ids.contains(id));
        for id in &ids {
            let Some(def) = doc.find(*id).and_then(def_of) else { continue };
            match self.entries.get(id) {
                None => self.start(*id, &def, def.affine.is_none()),
                Some(en) if en.loading.is_none() && en.key.is_some_and(|k| k != def.decode_key()) && def.visible => self.start(*id, &def, false),
                _ => {}
            }
        }
        while let Ok((id, result)) = self.rx.try_recv() {
            self.finish(ctx, doc, id, result);
        }
        let project = project_crs(doc);
        let pkey = serde_json::to_string(&project).unwrap_or_default();
        for id in ids {
            let Some(def) = doc.find(id).and_then(def_of) else { continue };
            let Some(en) = self.entries.get_mut(&id) else { continue };
            let key = format!("{:?}|{:?}|{}|{:?}", def.affine, def.src_px, serde_json::to_string(&def.crs).unwrap_or_default(), pkey);
            if en.placed.as_ref().map(|(k, _)| k) != Some(&key) && def.src_px[0] > 0 {
                let p = if def.affine.is_none() { Err("The image carries no placement (no GeoTIFF tags and no world file beside it). Place it by hand below.".to_string()) } else { place(&def, project.as_ref()) };
                en.placed = Some((key, p));
            }
        }
    }

    fn finish(&mut self, ctx: &egui::Context, doc: &mut Document, id: ElementId, result: Result<Loaded, (bool, String)>) {
        let Some(en) = self.entries.get_mut(&id) else { return };
        let asked = en.loading.take();
        match result {
            Err((true, _)) => en.status = ImgStatus::Missing,
            Err((false, e)) => en.status = ImgStatus::Failed(e),
            Ok(l) => {
                let Some(mut def) = doc.find(id).and_then(def_of) else { return };
                let mut changed = false;
                if def.hash.is_empty() {
                    def.hash = l.hash.clone();
                    def.size = l.size;
                    changed = true;
                    en.status = ImgStatus::Ok;
                } else if def.hash == l.hash {
                    en.status = ImgStatus::Ok;
                } else {
                    en.status = ImgStatus::Changed { was: def.hash.clone(), now: l.hash.clone() };
                }
                if def.src_px != [l.decoded.src_width, l.decoded.src_height] {
                    def.src_px = [l.decoded.src_width, l.decoded.src_height];
                    changed = true;
                }
                en.notes = l.decoded.notes.clone();
                match &l.georef {
                    Ok(Some(g)) if def.affine.is_none() => {
                        def.affine = Some(g.affine.to_array());
                        def.placed_from = g.from.clone();
                        if def.crs.is_none() {
                            def.crs = g.crs.clone();
                            def.crs_from = g.crs_from.clone();
                        }
                        en.notes.extend(g.notes.iter().cloned());
                        changed = true;
                    }
                    Err(e) => en.notes.push(format!("The placement could not be read: {e}")),
                    _ => {}
                }
                let img = egui::ColorImage::from_rgba_unmultiplied([l.decoded.width as usize, l.decoded.height as usize], &l.decoded.rgba);
                en.tex = Some(ctx.load_texture(format!("image-underlay-{}", id.0), img, egui::TextureOptions::LINEAR));
                en.texture_px = [l.decoded.width, l.decoded.height];
                en.key = Some(l.key);
                if asked.is_some_and(|a| a != def.decode_key()) {
                    // The detail changed while this read ran: the next sync reads again.
                    en.key = asked;
                }
                if changed {
                    set_def(doc, id, &def);
                    self.doc_changed = true;
                }
            }
        }
    }

    /// Accepts a changed file: its hash becomes the one remembered.
    pub fn accept_change(&mut self, doc: &mut Document, id: ElementId) {
        let Some(en) = self.entries.get_mut(&id) else { return };
        if let (ImgStatus::Changed { now, .. }, Some(mut def)) = (en.status.clone(), doc.find(id).and_then(def_of)) {
            def.hash = now;
            set_def(doc, id, &def);
            en.status = ImgStatus::Ok;
            self.doc_changed = true;
        }
    }

    pub fn placement(&self, id: ElementId) -> Option<&Result<Placement, String>> {
        self.entries.get(&id).and_then(|e| e.placed.as_ref().map(|(_, p)| p))
    }

    /// Model extents of the placed, visible images.
    pub fn extents(&self, doc: &Document) -> Option<afrobim_kernel::BBox> {
        doc.elements()
            .iter()
            .filter(|e| e.category == CATEGORY && def_of(e).is_some_and(|d| d.visible))
            .filter_map(|e| self.placement(e.id).and_then(|p| p.as_ref().ok()))
            .map(|p| afrobim_kernel::BBox::new(p.lo.x, p.lo.y, p.hi.x, p.hi.y))
            .reduce(|a, b| a.union(&b))
    }

    /// Draws the visible images behind the model, in document order.
    pub fn paint(&self, doc: &Document, painter: &Painter, view: &ViewTransform, rect: Rect) {
        for id in paint_order(doc) {
            let Some(e) = doc.find(id) else { continue };
            let Some(def) = def_of(e) else { continue };
            if !def.visible {
                continue;
            }
            let Some(en) = self.entries.get(&e.id) else { continue };
            let (Some(tex), Some((_, Ok(p)))) = (&en.tex, &en.placed) else { continue };
            paint_grid(painter, view, rect, tex.id(), p.n, &p.pts, [0.0, 0.0, 1.0, 1.0], Color32::WHITE.gamma_multiply(def.opacity.clamp(0.05, 1.0)));
        }
    }

    /// Blocks until every read in flight has finished (tests).
    #[cfg(test)]
    pub fn wait(&mut self, ctx: &egui::Context, doc: &mut Document) {
        for _ in 0..4000 {
            self.sync(ctx, doc);
            if !self.busy() && doc.elements().iter().filter(|e| e.category == CATEGORY).all(|e| self.entries.contains_key(&e.id)) {
                self.sync(ctx, doc);
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

// ─── the window ─────────────────────────────────────────────────────────────

impl crate::app::AfroBimApp {
    /// The IMAGE window: the background map, then each attached image with its status, placement,
    /// coordinate system and display switches.
    pub(crate) fn imagery_window(&mut self, ctx: &egui::Context) {
        if !self.imagery.window_open {
            return;
        }
        let mut open = true;
        let (mut attach, mut detach, mut accept, mut zoom) = (false, None, None, None);
        let mut reorder: Option<(ElementId, bool)> = None;
        let mut capture = false;
        let mut changes: Vec<(ElementId, ImageDef)> = Vec::new();
        let mut map_changed: Option<crate::basemap::BasemapSettings> = None;
        let project = project_crs(&self.doc);
        let dlg = crate::ui_kit::Dialog::titled("Images and background map").subtitle("Orthophotos, satellite scenes and an online map behind the model").width(680.0).show(
            ctx,
            |ui| {
                // The background map.
                crate::ui_kit::heading(ui, "Background map");
                let mut s = crate::basemap::settings(&self.doc);
                let before = s.clone();
                crate::basemap::settings_ui(ui, &mut s, &mut self.basemap, project.as_ref());
                let can_capture = s.enabled && crate::basemap::plans_for(project.as_ref()).is_ok();
                let refusal = crate::basemap::capture_refusal(&s);
                if s != before {
                    map_changed = Some(s);
                }
                ui.add_space(6.0);
                capture = crate::map_capture::capture_ui(ui, &mut self.basemap.capture, can_capture, refusal.as_deref());
                ui.add_space(6.0);
                crate::ui_kit::heading(ui, "Attached images");
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "An image attached here is a read-only background. The document keeps its path, a hash and its placement, not the pixels: it is read again when the document opens. The placement comes from GeoTIFF tags or a world file (.pgw, .jgw, .tfw, .wld) beside the image, and its coordinate system from the GeoTIFF or a .prj file. When the image's system is not the project's, it is moved through the same datum transformations the CRS command uses.");
                let ids: Vec<(ElementId, String)> = self.doc.elements().iter().filter(|e| e.category == CATEGORY).map(|e| (e.id, e.name.clone())).collect();
                if ids.is_empty() {
                    ui.label(egui::RichText::new("No image attached.").weak());
                }
                for (id, name) in ids {
                    let Some(mut def) = self.doc.find(id).and_then(def_of) else { continue };
                    let before = def.clone();
                    let en = self.imagery.entries.get(&id);
                    let status = en.map(|e| e.status.clone());
                    let notes = en.map(|e| e.notes.clone()).unwrap_or_default();
                    let texture_px = en.map(|e| e.texture_px).unwrap_or([0, 0]);
                    let placed = self.imagery.placement(id).cloned();
                    let file = Path::new(&def.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    egui::CollapsingHeader::new(egui::RichText::new(format!("{name}   {file}")).strong()).id_salt(("image", id.0)).default_open(true).show(ui, |ui| {
                        ui.label(egui::RichText::new(&def.path).small().weak());
                        if let Some(s) = &status {
                            let colour = match s {
                                ImgStatus::Ok => crate::theme::success(),
                                ImgStatus::Reading => Color32::GRAY,
                                _ => crate::theme::danger(),
                            };
                            ui.label(egui::RichText::new(s.line()).color(colour));
                            if let ImgStatus::Changed { .. } = s {
                                if ui.button("Accept the changed file").clicked() {
                                    accept = Some(id);
                                }
                            }
                        }
                        egui::Grid::new(("image-look", id.0)).num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                            ui.label("Visible");
                            crate::ui_kit::toggle(ui, &mut def.visible, "Shown behind the model");
                            ui.end_row();
                            ui.label("Opacity");
                            ui.horizontal(|ui| {
                                ui.add(egui::Slider::new(&mut def.opacity, 0.05..=1.0).show_value(false));
                                ui.label(format!("{:.0} %", def.opacity * 100.0));
                            });
                            ui.end_row();
                            ui.label("Draw order");
                            ui.horizontal(|ui| {
                                if ui.button("Send back").on_hover_text("Drawn under the other images (the online map stays below all of them)").clicked() {
                                    reorder = Some((id, false));
                                }
                                if ui.button("Bring forward").on_hover_text("Drawn over the other images").clicked() {
                                    reorder = Some((id, true));
                                }
                            });
                            ui.end_row();
                            ui.label("Detail");
                            egui::ComboBox::from_id_salt(("image-detail", id.0)).selected_text(DETAILS.iter().find(|d| d.0 == def.detail).map(|d| d.1).unwrap_or("Custom")).show_ui(ui, |ui| {
                                for (v, l) in DETAILS {
                                    ui.selectable_value(&mut def.detail, *v, *l);
                                }
                            });
                            ui.end_row();
                            ui.label("Black collar");
                            crate::ui_kit::toggle(ui, &mut def.transparent_black, "Pure black pixels are not drawn");
                            ui.end_row();
                        });
                        crate::ui_kit::heading(ui, "Placement");
                        egui::Grid::new(("image-place", id.0)).num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                            ui.label("Pixels");
                            ui.label(if def.src_px[0] == 0 { "not read yet".to_string() } else { format!("{} x {} (texture {} x {})", def.src_px[0], def.src_px[1], texture_px[0], texture_px[1]) });
                            ui.end_row();
                            ui.label("Placed from");
                            ui.label(if def.placed_from.is_empty() { "nothing yet" } else { &def.placed_from });
                            ui.end_row();
                            if let Some(a) = def.affine.map(Affine::from_array) {
                                let (sx, sy) = a.pixel_size();
                                let (x, y) = a.apply(0.0, 0.0);
                                ui.label("Ground pixel");
                                ui.label(format!("{sx:.4} x {sy:.4} (image units), turned {:.4} degrees", a.rotation_deg()));
                                ui.end_row();
                                ui.label("Top-left corner");
                                ui.label(format!("{x:.3}, {y:.3}"));
                                ui.end_row();
                            }
                            ui.label("Image system");
                            ui.label(match &def.crs {
                                Some(c) => format!("{} ({})", c.label(), if def.crs_from.is_empty() { "given" } else { &def.crs_from }),
                                None => "not stated: taken to be the project's".into(),
                            });
                            ui.end_row();
                            ui.label("Choose the system");
                            ui.horizontal(|ui| {
                                let t = self.imagery.epsg_text.entry(id).or_insert_with(|| def.crs.as_ref().and_then(|c| c.epsg).map(|c| c.to_string()).unwrap_or_default());
                                ui.add(egui::TextEdit::singleline(t).desired_width(90.0).hint_text("EPSG code"));
                                if ui.button("Use").on_hover_text("The image's own coordinate system, for example 21036 (Arc 1960 / UTM 36S) or 32636 (WGS 84 / UTM 36N)").clicked() {
                                    match ProjectCrs::parse(t) {
                                        Some(c) => {
                                            def.crs = Some(c);
                                            def.crs_from = "chosen by hand".into();
                                        }
                                        None => self.status = format!("'{t}' is not a coordinate system AfroBIM knows."),
                                    }
                                }
                                if def.crs.is_some() && ui.button("Clear").clicked() {
                                    def.crs = None;
                                    def.crs_from.clear();
                                }
                            });
                            ui.end_row();
                        });
                        match &placed {
                            Some(Ok(p)) => {
                                ui.label(egui::RichText::new(&p.how).small());
                            }
                            Some(Err(e)) => {
                                ui.label(egui::RichText::new(e).small().color(crate::theme::warn()));
                            }
                            None => {}
                        }
                        if def.affine.is_none() && def.src_px[0] > 0 || def.placed_from == "placed by hand" {
                            ui.label(egui::RichText::new("Place by hand: the top-left corner of the image and the size of one pixel, in the image's system (metres for a projected system).").small());
                            let f = self.imagery.hand.entry(id).or_insert_with(|| [String::new(), String::new(), String::new()]);
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut f[0]).desired_width(110.0).hint_text("Easting"));
                                ui.add(egui::TextEdit::singleline(&mut f[1]).desired_width(110.0).hint_text("Northing"));
                                ui.add(egui::TextEdit::singleline(&mut f[2]).desired_width(70.0).hint_text("Pixel"));
                                if ui.button("Place").clicked() {
                                    match (f[0].trim().parse::<f64>(), f[1].trim().parse::<f64>(), f[2].trim().parse::<f64>()) {
                                        (Ok(x), Ok(y), Ok(s)) if s > 0.0 && x.is_finite() && y.is_finite() => {
                                            def.affine = Some(Affine::north_up(x, y, s, s).to_array());
                                            def.placed_from = "placed by hand".into();
                                        }
                                        _ => self.status = "Give the easting and northing of the top-left corner and a pixel size greater than zero.".into(),
                                    }
                                }
                            });
                        }
                        for n in notes.iter().take(6) {
                            ui.label(egui::RichText::new(n).small().weak());
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Zoom to it").clicked() {
                                zoom = Some(id);
                            }
                            if ui.button("Detach").on_hover_text("Removes the record from the document; the file is not touched").clicked() {
                                detach = Some(id);
                            }
                        });
                    });
                    if def != before {
                        changes.push((id, def));
                    }
                }
            },
            |ui| crate::ui_kit::Footer::new("Attach an image...").cancel(Some("Close")).show(ui),
        );
        if dlg.closed {
            open = false;
        }
        if let Some((_, out)) = &dlg.inner {
            if out.cancel {
                open = false;
            }
            if out.primary {
                attach = true;
            }
        }
        if let Some(s) = map_changed {
            crate::basemap::set_settings(&mut self.doc, &s);
            self.schedule_commit_now();
        }
        for (id, def) in changes {
            set_def(&mut self.doc, id, &def);
            self.schedule_commit_now();
        }
        if attach {
            if let Some(path) = rfd::FileDialog::new().add_filter("Georeferenced image", EXTENSIONS).add_filter("any file", &["*"]).set_title("Attach a georeferenced image").pick_file() {
                self.attach_image(&path);
            }
        }
        if capture {
            self.run(ctx, crate::commands::Action::MapCapture);
        }
        if let Some((id, forward)) = reorder {
            if shift_order(&mut self.doc, id, forward) {
                self.schedule_commit_now();
            }
        }
        if let Some(id) = accept {
            self.imagery.accept_change(&mut self.doc, id);
            self.schedule_commit_now();
        }
        if let Some(id) = detach {
            self.imagery.detach(&mut self.doc, id);
            self.schedule_commit_now();
        }
        if let Some(id) = zoom {
            if let Some(Ok(p)) = self.imagery.placement(id) {
                let (l, t, w, h) = self.ctl.viewport;
                self.view.fit(&afrobim_kernel::BBox::new(p.lo.x, p.lo.y, p.hi.x, p.hi.y), l, t, w, h, 60.0);
            }
        }
        self.imagery.window_open = open;
    }

    /// Attaches an image and opens the window so its placement can be seen.
    pub(crate) fn attach_image(&mut self, path: &Path) {
        match self.imagery.attach(&mut self.doc, path) {
            Ok(_) => {
                self.status = format!("Attached {}: reading in the background.", path.display());
                self.imagery.window_open = true;
                self.schedule_commit_now();
            }
            Err(e) => self.status = e,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("afrobim-img-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn png_with_world(d: &Path, w: u32, h: u32, world: &str, prj: Option<&str>) -> PathBuf {
        let p = d.join("ortho.png");
        let px: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8, 90, 160]).collect();
        image::save_buffer(&p, &px, w, h, image::ExtendedColorType::Rgb8).unwrap();
        std::fs::write(d.join("ortho.pgw"), world).unwrap();
        if let Some(t) = prj {
            std::fs::write(d.join("ortho.prj"), t).unwrap();
        }
        p
    }

    #[test]
    fn an_image_with_a_world_file_lands_at_its_coordinates_in_millimetres() {
        let d = dir("plain");
        let p = png_with_world(&d, 40, 20, "0.5\n0\n0\n-0.5\n450000.25\n40000.75\n", None);
        let ctx = egui::Context::default();
        let mut doc = Document::default();
        afrobim_kernel::set_project_crs(&mut doc, &ProjectCrs::from_epsg(32636).unwrap());
        let mut st = ImageryState::default();
        let id = st.attach(&mut doc, &p).unwrap();
        st.wait(&ctx, &mut doc);
        let def = doc.find(id).and_then(def_of).unwrap();
        assert_eq!(def.src_px, [40, 20]);
        assert!(def.placed_from.contains("ortho.pgw"));
        assert!(!def.hash.is_empty());
        let pl = st.placement(id).unwrap().as_ref().unwrap();
        assert_eq!(pl.n, 1);
        // Top-left corner: the world file's centre minus half a pixel, in mm.
        assert_eq!(pl.pts[0], Pt::new(450_000_000.0, 40_001_000.0));
        // Bottom-right: 40 x 0.5 m east, 20 x 0.5 m south.
        assert_eq!(pl.pts[3], Pt::new(450_020_000.0, 39_991_000.0));
        assert!(pl.how.contains("taken to be the project's"));
        let ext = st.extents(&doc).unwrap();
        assert_eq!((ext.min.x, ext.max.y), (450_000_000.0, 40_001_000.0));
    }

    #[test]
    fn an_arc_1960_image_is_moved_into_a_wgs84_project_through_a_datum_shift() {
        let d = dir("arc");
        let prj = r#"PROJCS["Arc_1960_UTM_Zone_36S",GEOGCS["GCS_Arc_1960",DATUM["D_Arc_1960",SPHEROID["Clarke_1880_RGS",6378249.145,293.465]],PRIMEM["Greenwich",0.0],UNIT["Degree",0.0174532925199433]],PROJECTION["Transverse_Mercator"],PARAMETER["False_Easting",500000.0],PARAMETER["False_Northing",10000000.0],PARAMETER["Central_Meridian",33.0],PARAMETER["Scale_Factor",0.9996],PARAMETER["Latitude_Of_Origin",0.0],UNIT["Meter",1.0]]"#;
        let p = png_with_world(&d, 16, 16, "1\n0\n0\n-1\n180000.5\n9909000.5\n", Some(prj));
        let ctx = egui::Context::default();
        let mut doc = Document::default();
        afrobim_kernel::set_project_crs(&mut doc, &ProjectCrs::from_epsg(32736).unwrap());
        let mut st = ImageryState::default();
        let id = st.attach(&mut doc, &p).unwrap();
        st.wait(&ctx, &mut doc);
        let def = doc.find(id).and_then(def_of).unwrap();
        assert_eq!(def.crs.as_ref().and_then(|c| c.epsg), Some(21036));
        let pl = st.placement(id).unwrap().as_ref().unwrap();
        assert_eq!(pl.n, 8);
        assert!(pl.how.contains("EPSG:"), "{}", pl.how);
        // Arc 1960 to WGS 84 moves positions in Uganda by a few hundred metres, never kilometres.
        let shift = ((pl.pts[0].x - 180_000_000.0).powi(2) + (pl.pts[0].y - 9_909_001_000.0).powi(2)).sqrt() / 1000.0;
        assert!(shift > 50.0 && shift < 1000.0, "shift {shift} m");
        // Same image in an Arc 1960 project: used as it is.
        afrobim_kernel::set_project_crs(&mut doc, &ProjectCrs::from_epsg(21036).unwrap());
        st.sync(&ctx, &mut doc);
        let pl = st.placement(id).unwrap().as_ref().unwrap();
        assert_eq!((pl.n, pl.pts[0]), (1, Pt::new(180_000_000.0, 9_909_001_000.0)));
    }

    #[test]
    fn an_image_with_no_placement_says_so_and_can_be_placed_by_hand() {
        let d = dir("bare");
        let p = d.join("scan.png");
        image::save_buffer(&p, &[200u8; 8 * 8 * 3], 8, 8, image::ExtendedColorType::Rgb8).unwrap();
        let ctx = egui::Context::default();
        let mut doc = Document::default();
        let mut st = ImageryState::default();
        let id = st.attach(&mut doc, &p).unwrap();
        st.wait(&ctx, &mut doc);
        assert!(st.placement(id).unwrap().as_ref().unwrap_err().contains("by hand"));
        let mut def = doc.find(id).and_then(def_of).unwrap();
        def.affine = Some(Affine::north_up(100.0, 200.0, 2.0, 2.0).to_array());
        set_def(&mut doc, id, &def);
        st.sync(&ctx, &mut doc);
        let pl = st.placement(id).unwrap().as_ref().unwrap();
        assert_eq!(pl.pts[3], Pt::new(116_000.0, 184_000.0));
    }

    #[test]
    fn a_missing_file_is_reported_and_a_changed_file_is_detected() {
        let d = dir("change");
        let p = png_with_world(&d, 4, 4, "1\n0\n0\n-1\n0.5\n3.5\n", None);
        let ctx = egui::Context::default();
        let mut doc = Document::default();
        let mut st = ImageryState::default();
        let id = st.attach(&mut doc, &p).unwrap();
        st.wait(&ctx, &mut doc);
        image::save_buffer(&p, &[9u8; 4 * 4 * 3], 4, 4, image::ExtendedColorType::Rgb8).unwrap();
        let mut st2 = ImageryState::default();
        st2.wait(&ctx, &mut doc);
        assert!(matches!(st2.entries[&id].status, ImgStatus::Changed { .. }));
        st2.accept_change(&mut doc, id);
        assert_eq!(st2.entries[&id].status, ImgStatus::Ok);
        std::fs::remove_file(&p).unwrap();
        let mut st3 = ImageryState::default();
        st3.wait(&ctx, &mut doc);
        assert_eq!(st3.entries[&id].status, ImgStatus::Missing);
    }

    #[test]
    fn an_image_without_a_system_stays_on_the_ground_through_a_transform() {
        use afrobim_kernel::crs_plan::{plan_transform, DataExtent};
        let d = dir("pin");
        // Arc 1960 / UTM 36S near Ntungamo, 1 m pixels, no .prj: read in the project system.
        let p = png_with_world(&d, 4, 4, "1\n0\n0\n-1\n200000.5\n9900003.5\n", None);
        let mut doc = Document::default();
        let from = ProjectCrs::from_epsg(21036).unwrap();
        let to = ProjectCrs::from_epsg(32736).unwrap();
        afrobim_kernel::set_project_crs(&mut doc, &from);
        let mut st = ImageryState::default();
        let id = st.attach(&mut doc, &p).unwrap();
        let ctx = egui::Context::default();
        st.wait(&ctx, &mut doc);
        assert!(def_of(doc.find(id).unwrap()).unwrap().crs.is_none());
        let before = place(&def_of(doc.find(id).unwrap()).unwrap(), Some(&from)).unwrap();
        let plan = plan_transform(&from, &to, Some(DataExtent::new((199_990.0, 9_899_990.0), (200_010.0, 9_900_010.0)))).unwrap().into_iter().next().unwrap();
        crate::transform_doc::apply_plan(&mut doc, None, &plan, &to, "Whole document", "t".into()).unwrap();
        let def = def_of(doc.find(id).unwrap()).unwrap();
        assert_eq!(def.crs.as_ref().and_then(|c| c.epsg), Some(21036), "the old system is pinned onto the image");
        let after = place(&def, Some(&to)).unwrap();
        let want = plan.apply(before.lo.x / 1000.0, before.lo.y / 1000.0, 0.0).unwrap();
        let (dx, dy) = (after.lo.x / 1000.0 - want.first, after.lo.y / 1000.0 - want.second);
        assert!(dx.hypot(dy) < 0.5, "image moved with the datum: off by {dx:.3}, {dy:.3} m");
        let (sx, sy) = (after.lo.x - before.lo.x, after.lo.y - before.lo.y);
        assert!(sx.hypot(sy) > 50_000.0, "the datum shift is tens of metres, so the check is not vacuous");
    }

    #[test]
    fn images_draw_in_their_order_and_an_old_document_without_one_still_loads() {
        let mut doc = Document::default();
        let mk = |doc: &mut Document, name: &str| {
            let def = ImageDef::new(Path::new(&format!("{name}.png")));
            doc.add(CATEGORY, name, Pt::new(0.0, 0.0), 1.0, 1.0, crate::underlay::LAYER, serde_json::to_value(&def).unwrap())
        };
        let (a, b, c) = (mk(&mut doc, "a"), mk(&mut doc, "b"), mk(&mut doc, "c"));
        assert_eq!(paint_order(&doc), vec![a, b, c]);
        assert!(shift_order(&mut doc, a, true));
        assert_eq!(paint_order(&doc), vec![b, a, c]);
        assert!(shift_order(&mut doc, c, false));
        assert_eq!(paint_order(&doc), vec![b, c, a]);
        assert!(!shift_order(&mut doc, a, true), "already on top");
        assert!(!shift_order(&mut doc, b, false), "already at the bottom");
        // An attached image goes on top.
        let d = dir("order");
        let p = png_with_world(&d, 2, 2, "1\n0\n0\n-1\n0.5\n1.5\n", None);
        let mut st = ImageryState::default();
        let top = st.attach(&mut doc, &p).unwrap();
        assert_eq!(paint_order(&doc).last(), Some(&top));
        // A record written before the field existed has no "order" key.
        let mut v = serde_json::to_value(ImageDef::new(Path::new("old.png"))).unwrap();
        v.as_object_mut().unwrap().remove("order");
        let old: ImageDef = serde_json::from_value(v).unwrap();
        assert_eq!(old.order, 0);
    }

    #[test]
    fn a_site_calibration_moves_an_image_on_the_local_grid_with_the_drawing() {
        use afrobim_kernel::crs_plan::{ControlPair, SiteCalibration};
        let d = dir("cal");
        let p = png_with_world(&d, 4, 4, "1\n0\n0\n-1\n100.5\n203.5\n", None);
        let mut doc = Document::default();
        afrobim_kernel::set_project_crs(&mut doc, &ProjectCrs::local_grid("Site grid"));
        let mut st = ImageryState::default();
        let id = st.attach(&mut doc, &p).unwrap();
        let ctx = egui::Context::default();
        st.wait(&ctx, &mut doc);
        let target = ProjectCrs::from_epsg(21036).unwrap();
        // A rotated, slightly scaled site grid.
        let (s, c) = (0.3f64.sin() * 1.0002, 0.3f64.cos() * 1.0002);
        let pairs: Vec<ControlPair> = [(0.0, 0.0), (500.0, 0.0), (0.0, 500.0), (500.0, 500.0)].iter().enumerate().map(|(i, (x, y))| ControlPair { label: format!("C{i}"), local: (*x, *y), target: (179_000.0 + x * c + y * s, 9_909_000.0 - x * s + y * c) }).collect();
        let cal = SiteCalibration::fit("Site", &target, &pairs, None).unwrap();
        crate::transform_doc::apply_calibration(&mut doc, &cal, &target, "t".into()).unwrap();
        let def = def_of(doc.find(id).unwrap()).unwrap();
        assert!(def.crs.is_none(), "still read in the project system");
        let placed = place(&def, Some(&target)).expect("the image can still be placed");
        let a = Affine::from_array(def.affine.unwrap());
        for (col, row) in [(0.0, 0.0), (4.0, 0.0), (0.0, 4.0), (4.0, 4.0)] {
            let got = a.apply(col, row);
            let want = cal.to_target(100.0 + col, 204.0 - row);
            assert!((got.0 - want.0).hypot(got.1 - want.1) < 1e-6, "corner {col},{row}: {got:?} vs {want:?}");
        }
        assert!(placed.lo.x > 170_000_000.0, "placed in the target system, millimetres");
    }

    #[test]
    fn an_image_underlay_is_never_written_to_an_ifc_export() {
        let d = dir("ifc");
        let p = png_with_world(&d, 4, 4, "1\n0\n0\n-1\n0.5\n3.5\n", None);
        let mut doc = Document::default();
        let mut st = ImageryState::default();
        st.attach(&mut doc, &p).unwrap();
        // The export takes only what the app's filter lets through; the image must not pass.
        assert!(doc.elements().iter().any(|e| e.category == CATEGORY));
        assert!(doc.elements().iter().all(|e| !crate::app::ifc_exportable(&e.category)));
        assert!(!crate::app::ifc_exportable(crate::underlay::CATEGORY));
        assert!(crate::app::ifc_exportable("wall"));
    }

    #[test]
    fn a_cell_far_larger_than_the_screen_is_split_and_clipped_before_the_f32_cast() {
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        // A 1 km image in UTM millimetres, at 1000 px per mm: the image is 1e9 px across.
        let view = ViewTransform::centred_on(Pt::new(450_500_000.0, 9_900_500_000.0), Pt::new(400.0, 300.0), 1000.0);
        let pts = vec![Pt::new(450_000_000.0, 9_901_000_000.0), Pt::new(451_000_000.0, 9_901_000_000.0), Pt::new(450_000_000.0, 9_900_000_000.0), Pt::new(451_000_000.0, 9_900_000_000.0)];
        let mut shapes = Vec::new();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            paint_grid(&painter, &view, rect, TextureId::default(), 1, &pts, [0.0, 0.0, 1.0, 1.0], Color32::WHITE);
        });
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            paint_grid(&painter, &view, rect, TextureId::default(), 1, &pts, [0.0, 0.0, 1.0, 1.0], Color32::WHITE);
        });
        for s in out.shapes {
            shapes.push(s.shape);
        }
        let mesh = shapes.iter().find_map(|s| if let egui::Shape::Mesh(m) = s { Some(m.clone()) } else { None }).expect("a mesh");
        assert!(!mesh.vertices.is_empty());
        for v in &mesh.vertices {
            assert!(v.pos.x.abs() < 1e5 && v.pos.y.abs() < 1e5, "vertex at {:?}", v.pos);
        }
        // The middle of the screen shows the middle of the image.
        let near = mesh.vertices.iter().min_by(|a, b| (a.pos - pos2(400.0, 300.0)).length().total_cmp(&(b.pos - pos2(400.0, 300.0)).length())).unwrap();
        assert!((near.uv.x - 0.5).abs() < 0.01 && (near.uv.y - 0.5).abs() < 0.01, "{:?}", near.uv);
    }
}
