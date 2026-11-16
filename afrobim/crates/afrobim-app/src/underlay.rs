//! DXF underlays: read-only background drawings attached by path and hash.
//!
//! An underlay is a document element (category [`CATEGORY`]) that holds only the file's path, a
//! change-detection hash, the units and placement the engineer chose, and the switches (visible,
//! fade, hidden layers). The drawing is **not** stored in the document: it is read again from the
//! file when the document opens (and on attach), on a worker thread, so a large file does not
//! stop the window. The file is never written.
//!
//! What is drawn is built from the zoom: curves are flattened to a tolerance of about one pixel
//! (a power of two in millimetres), which keeps memory bounded, and the drawing is rebuilt on a
//! worker when the zoom crosses to another tolerance. Lines are drawn as hairlines in one muted
//! ink colour at the chosen fade; the file's own colours, line types and text fonts are not used.
//!
//! The hash is FNV-1a over the file's bytes: it detects that a file changed, it is not a
//! cryptographic hash.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use afrobim_draw::Primitive;
use afrobim_dxf::reader::{self, DisplayOptions, SnapKind, SnapOptions, Underlay};
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, Pt, ViewTransform};
use egui::{pos2, Color32, Painter, Rect, Shape, Stroke};
use serde::{Deserialize, Serialize};

use crate::anno_glue::{Cand, SnapMode};

pub const CATEGORY: &str = "dxf_underlay";
/// The layer of the placeholder elements; hidden, so they are never drawn or picked.
pub const LAYER: &str = "Underlays";

/// The most segments drawn in one frame; beyond it the status says how many were left out.
const DRAW_CAP: usize = 250_000;

/// What the document stores for one underlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnderlayDef {
    /// The file as it was attached.
    pub path: String,
    /// FNV-1a 64 of the file's bytes at attach (or when the change was accepted), hex. Empty
    /// until the first read finishes.
    pub hash: String,
    pub size: u64,
    /// Millimetres per file unit in use: the proposal from the header, or the override.
    pub unit_scale_mm: f64,
    pub proposed_scale_mm: f64,
    pub units_name: String,
    pub units_assumed: bool,
    /// Where the file's origin goes in the model, mm.
    pub offset_mm: [f64; 2],
    /// Turn about the file's origin, degrees anticlockwise.
    pub rotation_deg: f64,
    /// Extra scale on top of the units.
    pub scale: f64,
    pub visible: bool,
    /// Opacity of the drawing, 0 to 1.
    pub opacity: f32,
    /// Layers switched off here (the file's own frozen and off layers are always off).
    pub hidden_layers: Vec<String>,
    /// Show it flat at `drape_level_mm` in the 3D site view.
    pub drape_3d: bool,
    pub drape_level_mm: f64,
}

impl UnderlayDef {
    fn new(path: &Path) -> UnderlayDef {
        UnderlayDef {
            path: path.to_string_lossy().to_string(),
            hash: String::new(),
            size: 0,
            unit_scale_mm: 1.0,
            proposed_scale_mm: 1.0,
            units_name: String::new(),
            units_assumed: false,
            offset_mm: [0.0, 0.0],
            rotation_deg: 0.0,
            scale: 1.0,
            visible: true,
            opacity: 0.55,
            hidden_layers: Vec::new(),
            drape_3d: false,
            drape_level_mm: 0.0,
        }
    }

    fn effective_scale(&self) -> f64 {
        self.unit_scale_mm * self.scale
    }

    fn xf(&self, p: Pt) -> Pt {
        let (s, c) = self.rotation_deg.to_radians().sin_cos();
        Pt::new(p.x * c - p.y * s + self.offset_mm[0], p.x * s + p.y * c + self.offset_mm[1])
    }
}

pub fn def_of(e: &Element) -> Option<UnderlayDef> {
    (e.category == CATEGORY).then(|| serde_json::from_value(e.data.clone()).ok()).flatten()
}

pub fn set_def(doc: &mut Document, id: ElementId, def: &UnderlayDef) {
    if let (Some(e), Ok(v)) = (doc.find_mut(id), serde_json::to_value(def)) {
        e.data = v;
    }
}

/// The module of the placeholder element: it draws nothing.
pub struct UnderlayModule;

impl Module for UnderlayModule {
    fn category(&self) -> &str {
        CATEGORY
    }
    fn display_name(&self) -> &str {
        "DXF underlays"
    }
    fn generate(&self, e: &Element) -> Result<Generated, String> {
        let def = def_of(e).ok_or("The underlay record cannot be read.")?;
        let mut g = Generated::default();
        g.derived.insert("File".into(), def.path.clone());
        g.drawing.extents = afrobim_draw::Extents { min: Pt::new(0.0, 0.0), max: Pt::new(1.0, 1.0) };
        Ok(g)
    }
}

/// FNV-1a 64 of a file, streamed, and its size.
pub fn hash_file(path: &Path) -> std::io::Result<(String, u64)> {
    use std::io::Read;
    let mut f = std::io::BufReader::with_capacity(1 << 20, std::fs::File::open(path)?);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut n: u64 = 0;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let k = f.read(&mut buf)?;
        if k == 0 {
            break;
        }
        for b in &buf[..k] {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        n += k as u64;
    }
    Ok((format!("{h:016x}"), n))
}

/// How the file stands against what the document remembers.
#[derive(Debug, Clone, PartialEq)]
pub enum FileStatus {
    Reading,
    Ok,
    Missing,
    /// The file is not what was attached (hash differs): old and new hash.
    Changed { was: String, now: String },
    Failed(String),
}

impl FileStatus {
    pub fn line(&self) -> String {
        match self {
            FileStatus::Reading => "Reading the file in the background...".into(),
            FileStatus::Ok => "The file is as it was attached.".into(),
            FileStatus::Missing => "The file is missing, so the underlay is not drawn. Put it back at the same path, or detach this underlay and attach the file again.".into(),
            FileStatus::Changed { .. } => "The file has CHANGED since it was attached (its hash differs). It is drawn as it is now. Accept the change to stop this warning.".into(),
            FileStatus::Failed(e) => format!("The file could not be read: {e}"),
        }
    }
}

struct Loaded {
    under: Arc<Underlay>,
    hash: String,
    size: u64,
}

/// What is drawn for one underlay at one tolerance.
#[derive(Clone, Default)]
pub struct Built {
    origin: [f64; 2],
    /// Segments relative to `origin`, millimetres.
    segs: Vec<[f32; 4]>,
    pub labels: Vec<(Pt, String, f64, f64)>,
    pub items: usize,
    pub truncated: bool,
    pub warnings: Vec<String>,
    pub bounds: Option<(Pt, Pt)>,
    key: BuildKey,
}

#[derive(Clone, Default, PartialEq, Debug)]
struct BuildKey {
    tol_log2: i32,
    offset: [i64; 2],
    rotation: i64,
    scale: i64,
    hidden: Vec<String>,
    unit: i64,
}

impl BuildKey {
    fn of(def: &UnderlayDef, tol_log2: i32) -> BuildKey {
        let q = |v: f64| (v * 1000.0).round() as i64;
        BuildKey { tol_log2, offset: [q(def.offset_mm[0]), q(def.offset_mm[1])], rotation: q(def.rotation_deg), scale: q(def.scale), hidden: def.hidden_layers.clone(), unit: q(def.unit_scale_mm) }
    }
}

/// The chord tolerance for a zoom: about one pixel, as a power of two in millimetres.
pub fn tolerance_log2(view_scale_px_per_mm: f64) -> i32 {
    ((1.0 / view_scale_px_per_mm.max(1e-9)).log2().round() as i32).clamp(-1, 14)
}

/// Builds the drawing of an underlay for a placement and a tolerance (a pure function: the worker
/// thread calls it, and so do the tests).
pub fn build(under: &Underlay, def: &UnderlayDef, tol_log2: i32) -> Built {
    let tol = 2f64.powi(tol_log2);
    let opts = DisplayOptions { unit_scale_mm: Some(def.effective_scale()), offset_mm: Pt::new(0.0, 0.0), tolerance_mm: tol, hidden_layers: def.hidden_layers.clone(), ..DisplayOptions::default() };
    let view = under.to_drawing(&opts);
    let mut pts: Vec<(Pt, Pt)> = Vec::with_capacity(view.drawing.primitives.len());
    let (mut lo, mut hi) = (Pt::new(f64::INFINITY, f64::INFINITY), Pt::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    let mut seg = |a: Pt, b: Pt| {
        let (a, b) = (def.xf(a), def.xf(b));
        for p in [a, b] {
            lo = Pt::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Pt::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        pts.push((a, b));
    };
    for prim in &view.drawing.primitives {
        match prim {
            Primitive::Line { from, to, .. } => seg(*from, *to),
            Primitive::Arc { centre, radius, start_deg, end_deg, .. } => {
                let sweep = if end_deg >= start_deg { end_deg - start_deg } else { end_deg - start_deg + 360.0 };
                let step = if *radius > tol { 2.0 * (1.0 - tol / radius).clamp(-1.0, 1.0).acos().to_degrees() } else { 90.0 };
                let n = ((sweep / step.max(0.25)).ceil() as usize).clamp(4, 720);
                let at = |i: usize| {
                    let a = (start_deg + sweep * i as f64 / n as f64).to_radians();
                    Pt::new(centre.x + radius * a.cos(), centre.y + radius * a.sin())
                };
                for i in 0..n {
                    seg(at(i), at(i + 1));
                }
            }
            Primitive::Fill { outline, .. } => {
                let n = outline.len();
                for i in 0..n {
                    seg(outline[i], outline[(i + 1) % n]);
                }
            }
            _ => {}
        }
    }
    let origin = if lo.x.is_finite() { [lo.x, lo.y] } else { [0.0, 0.0] };
    let segs = pts.iter().map(|(a, b)| [(a.x - origin[0]) as f32, (a.y - origin[1]) as f32, (b.x - origin[0]) as f32, (b.y - origin[1]) as f32]).collect();
    let labels = view.labels.iter().take(20_000).map(|l| (def.xf(l.at), l.text.clone(), l.height_mm * def.scale, l.rotation_deg + def.rotation_deg)).collect();
    Built {
        origin,
        segs,
        labels,
        items: view.items,
        truncated: view.truncated,
        warnings: view.warnings,
        bounds: lo.x.is_finite().then_some((lo, hi)),
        key: BuildKey::of(def, tol_log2),
    }
}

fn snap_points(under: &Underlay, def: &UnderlayDef) -> Vec<(Pt, SnapMode)> {
    let opts = DisplayOptions { unit_scale_mm: Some(def.effective_scale()), offset_mm: Pt::new(0.0, 0.0), hidden_layers: def.hidden_layers.clone(), ..DisplayOptions::default() };
    under
        .snap_points(&opts, &SnapOptions::default())
        .points
        .iter()
        .map(|s| {
            let mode = match s.kind {
                SnapKind::End => SnapMode::Endpoint,
                SnapKind::Mid => SnapMode::Midpoint,
                SnapKind::Centre => SnapMode::Centre,
                SnapKind::Quadrant => SnapMode::Quadrant,
                SnapKind::Node => SnapMode::Node,
                SnapKind::Intersection => SnapMode::Intersection,
            };
            (def.xf(s.p), mode)
        })
        .collect()
}

enum Msg {
    Read(ElementId, Result<Loaded, (bool, String)>),
    Built(ElementId, Box<Built>, Arc<Vec<(Pt, SnapMode)>>),
}

struct Entry {
    status: FileStatus,
    data: Option<Arc<Underlay>>,
    built: Option<Built>,
    snaps: Arc<Vec<(Pt, SnapMode)>>,
    snap_key: Option<BuildKey>,
    building: Option<BuildKey>,
    /// What the file said when it was read, for the window.
    hash_now: String,
}

/// The runtime side of the underlays: what was read, what is drawn, the jobs in flight.
pub struct UnderlayState {
    entries: HashMap<ElementId, Entry>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub window_open: bool,
    /// Set when a worker result changed the document (the caller takes an undo step).
    pub doc_changed: bool,
    pub selected: Option<ElementId>,
    /// Drawn segments left out of the last frame by the cap.
    pub left_out: std::cell::Cell<usize>,
}

impl Default for UnderlayState {
    fn default() -> Self {
        let (tx, rx) = channel();
        UnderlayState { entries: HashMap::new(), tx, rx, window_open: false, doc_changed: false, selected: None, left_out: std::cell::Cell::new(0) }
    }
}

impl UnderlayState {
    /// Forgets everything (a different document).
    pub fn reset(&mut self) {
        let open = self.window_open;
        *self = UnderlayState::default();
        self.window_open = open;
    }

    pub fn entry_status(&self, id: ElementId) -> Option<&FileStatus> {
        self.entries.get(&id).map(|e| &e.status)
    }

    pub fn underlay(&self, id: ElementId) -> Option<&Arc<Underlay>> {
        self.entries.get(&id).and_then(|e| e.data.as_ref())
    }

    pub fn built(&self, id: ElementId) -> Option<&Built> {
        self.entries.get(&id).and_then(|e| e.built.as_ref())
    }

    /// Attaches a DXF: adds the record to the document and starts reading. DWG is refused by the
    /// reader with its message (convert to DXF first).
    pub fn attach(&mut self, doc: &mut Document, path: &Path) -> Result<ElementId, String> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !path.exists() {
            return Err(format!("{} does not exist.", path.display()));
        }
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Underlay".into());
        let name = doc.unique_name(&format!("XREF-{stem}"));
        doc.ensure_layer(LAYER);
        if let Some(l) = doc.layers_mut().iter_mut().find(|l| l.name == LAYER) {
            l.visible = false;
        }
        let def = UnderlayDef::new(&path);
        let id = doc.add(CATEGORY, &name, Pt::new(0.0, 0.0), 1.0, 1.0, LAYER, serde_json::to_value(&def).map_err(|e| e.to_string())?);
        self.start_read(id, &path);
        Ok(id)
    }

    /// Removes an underlay record.
    pub fn detach(&mut self, doc: &mut Document, id: ElementId) {
        doc.remove(id);
        self.entries.remove(&id);
        if self.selected == Some(id) {
            self.selected = None;
        }
    }

    fn start_read(&mut self, id: ElementId, path: &Path) {
        self.entries.insert(id, Entry { status: FileStatus::Reading, data: None, built: None, snaps: Arc::new(Vec::new()), snap_key: None, building: None, hash_now: String::new() });
        let (tx, path) = (self.tx.clone(), path.to_path_buf());
        std::thread::spawn(move || {
            let result = if !path.exists() {
                Err((true, "missing".to_string()))
            } else {
                hash_file(&path).map_err(|e| (false, e.to_string())).and_then(|(hash, size)| {
                    let under = reader::read_file(&path).map_err(|e| (false, e.to_string()))?;
                    Ok(Loaded { under: Arc::new(under), hash, size })
                })
            };
            let _ = tx.send(Msg::Read(id, result));
        });
    }

    /// Once a frame: starts reads for records that have none (a document just opened), collects
    /// what the workers finished and starts a rebuild when the zoom needs one. `view_scale` is
    /// pixels per millimetre.
    pub fn sync(&mut self, doc: &mut Document, view_scale: f64) {
        let ids: Vec<ElementId> = doc.elements().iter().filter(|e| e.category == CATEGORY).map(|e| e.id).collect();
        self.entries.retain(|id, _| ids.contains(id));
        for id in &ids {
            if !self.entries.contains_key(id) {
                if let Some(def) = doc.find(*id).and_then(def_of) {
                    self.start_read(*id, Path::new(&def.path));
                }
            }
        }
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Read(id, result) => self.finish_read(doc, id, result),
                Msg::Built(id, built, snaps) => {
                    if let Some(en) = self.entries.get_mut(&id) {
                        en.snap_key = Some(built.key.clone());
                        en.snaps = snaps;
                        en.building = None;
                        en.built = Some(*built);
                    }
                }
            }
        }
        let tol = tolerance_log2(view_scale);
        for id in ids {
            let Some(def) = doc.find(id).and_then(def_of) else { continue };
            let Some(en) = self.entries.get_mut(&id) else { continue };
            let Some(data) = en.data.clone() else { continue };
            let want = BuildKey::of(&def, tol);
            let have = en.built.as_ref().map(|b| &b.key);
            if have == Some(&want) || en.building.as_ref() == Some(&want) || !def.visible {
                continue;
            }
            en.building = Some(want);
            let (tx, def2) = (self.tx.clone(), def.clone());
            // Snap points do not depend on the zoom: reuse them while only the tolerance changes.
            let reuse = en.snap_key.as_ref().is_some_and(|k| BuildKey { tol_log2: 0, ..k.clone() } == BuildKey { tol_log2: 0, ..BuildKey::of(&def, 0) }).then(|| en.snaps.clone());
            std::thread::spawn(move || {
                let built = build(&data, &def2, tol);
                let snaps = reuse.unwrap_or_else(|| Arc::new(snap_points(&data, &def2)));
                let _ = tx.send(Msg::Built(id, Box::new(built), snaps));
            });
        }
    }

    fn finish_read(&mut self, doc: &mut Document, id: ElementId, result: Result<Loaded, (bool, String)>) {
        let Some(en) = self.entries.get_mut(&id) else { return };
        match result {
            Err((true, _)) => en.status = FileStatus::Missing,
            Err((false, e)) => en.status = FileStatus::Failed(e),
            Ok(l) => {
                en.hash_now = l.hash.clone();
                let Some(mut def) = doc.find(id).and_then(def_of) else { return };
                if def.hash.is_empty() {
                    // First read after an attach: record what the file is.
                    def.hash = l.hash.clone();
                    def.size = l.size;
                    def.proposed_scale_mm = l.under.units.mm_per_unit;
                    def.unit_scale_mm = l.under.unit_scale_mm;
                    def.units_name = l.under.units.name.clone();
                    def.units_assumed = l.under.units.assumed;
                    set_def(doc, id, &def);
                    self.doc_changed = true;
                    en.status = FileStatus::Ok;
                } else if def.hash == l.hash {
                    en.status = FileStatus::Ok;
                } else {
                    en.status = FileStatus::Changed { was: def.hash.clone(), now: l.hash.clone() };
                }
                en.data = Some(l.under);
            }
        }
    }

    /// Accepts a changed file: the new hash becomes the one remembered.
    pub fn accept_change(&mut self, doc: &mut Document, id: ElementId) {
        let Some(en) = self.entries.get_mut(&id) else { return };
        if let (FileStatus::Changed { now, .. }, Some(mut def)) = (en.status.clone(), doc.find(id).and_then(def_of)) {
            def.hash = now;
            if let Ok((_, size)) = hash_file(Path::new(&def.path)) {
                def.size = size;
            }
            set_def(doc, id, &def);
            en.status = FileStatus::Ok;
            self.doc_changed = true;
        }
    }

    /// Blocks until every read and build in flight has finished (tests).
    #[cfg(test)]
    pub fn wait(&mut self, doc: &mut Document, view_scale: f64) {
        for _ in 0..4000 {
            self.sync(doc, view_scale);
            let busy = self.entries.values().any(|e| e.status == FileStatus::Reading || e.building.is_some()) || {
                doc.elements().iter().filter(|e| e.category == CATEGORY).any(|e| {
                    self.entries.get(&e.id).is_some_and(|en| en.data.is_some() && en.built.is_none()) && def_of(e).is_some_and(|d| d.visible)
                })
            };
            if !busy {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// Snap candidates of the visible underlays within `radius` of `cursor`.
    pub fn snaps_near(&self, doc: &Document, cursor: Pt, radius: f64) -> Vec<Cand> {
        let mut out = Vec::new();
        for e in doc.elements().iter().filter(|e| e.category == CATEGORY) {
            if !def_of(e).is_some_and(|d| d.visible) {
                continue;
            }
            let Some(en) = self.entries.get(&e.id) else { continue };
            for (p, mode) in en.snaps.iter() {
                if (p.x - cursor.x).abs() <= radius && (p.y - cursor.y).abs() <= radius && afrobim_annotate::geometry::dist(*p, cursor) <= radius {
                    out.push(Cand { p: *p, mode: *mode, from: format!("underlay {}", e.name), owner: None });
                }
            }
        }
        out
    }

    /// Draws the visible underlays behind the model. `ink` is the canvas foreground colour.
    pub fn paint(&self, doc: &Document, painter: &Painter, view: &ViewTransform, rect: Rect, ink: Color32) {
        self.left_out.set(0);
        for e in doc.elements().iter().filter(|e| e.category == CATEGORY) {
            let Some(def) = def_of(e) else { continue };
            if !def.visible {
                continue;
            }
            let Some(b) = self.entries.get(&e.id).and_then(|en| en.built.as_ref()) else { continue };
            let colour = ink.gamma_multiply(def.opacity.clamp(0.05, 1.0) * 0.8);
            let stroke = Stroke::new(1.0_f32, colour);
            let mut shapes: Vec<Shape> = Vec::new();
            // Placed from the difference between the underlay origin and the view anchor, in f64,
            // then clipped to the screen in f64 before the f32 conversion (see `render::CLIP_PX`).
            let at = |x: f32, y: f32| view.to_screen_local(Pt::new(b.origin[0], b.origin[1]), x as f64, y as f64);
            let (lo, hi) = (Pt::new(rect.left() as f64 - 50.0, rect.top() as f64 - 50.0), Pt::new(rect.right() as f64 + 50.0, rect.bottom() as f64 + 50.0));
            let mut drawn = 0usize;
            for s in &b.segs {
                let Some((p, q)) = crate::render::clip_segment_box(at(s[0], s[1]), at(s[2], s[3]), lo, hi) else { continue };
                let (p, q) = (pos2(p.x as f32, p.y as f32), pos2(q.x as f32, q.y as f32));
                if (p.x < rect.left() && q.x < rect.left()) || (p.x > rect.right() && q.x > rect.right()) || (p.y < rect.top() && q.y < rect.top()) || (p.y > rect.bottom() && q.y > rect.bottom()) {
                    continue;
                }
                if drawn >= DRAW_CAP {
                    self.left_out.set(self.left_out.get() + 1);
                    continue;
                }
                drawn += 1;
                shapes.push(Shape::line_segment([p, q], stroke));
            }
            painter.extend(shapes);
            for (p, text, height, rot) in &b.labels {
                let s = view.to_screen(*p);
                let px = (height * view.scale) as f32;
                if px < 4.0 || !rect.expand(40.0).contains(pos2(s.x as f32, s.y as f32)) {
                    continue;
                }
                let galley = painter.layout_no_wrap(text.replace('\n', " "), egui::FontId::proportional(px.min(60.0)), colour);
                let shape = egui::epaint::TextShape::new(pos2(s.x as f32, s.y as f32), galley, colour).with_angle(-(*rot as f32).to_radians());
                painter.add(shape);
            }
        }
    }

    /// True while a read or a build is in flight.
    pub fn busy(&self) -> bool {
        self.entries.values().any(|e| e.status == FileStatus::Reading || e.building.is_some())
    }

    /// Draws the underlays that are draped in the 3D site view, flat at their level.
    pub fn paint_3d(&self, doc: &Document, painter: &Painter, cam: &crate::view3d::Camera, rect: Rect, ink: Color32) {
        for e in doc.elements().iter().filter(|e| e.category == CATEGORY) {
            let Some(def) = def_of(e) else { continue };
            if !def.visible || !def.drape_3d {
                continue;
            }
            let Some(b) = self.entries.get(&e.id).and_then(|en| en.built.as_ref()) else { continue };
            let stroke = Stroke::new(1.0_f32, ink.gamma_multiply(def.opacity.clamp(0.05, 1.0) * 0.8));
            // Of a very large drawing the first 40 000 segments are draped.
            for s in b.segs.iter().take(40_000) {
                let a = [b.origin[0] + s[0] as f64, b.origin[1] + s[1] as f64, def.drape_level_mm];
                let c = [b.origin[0] + s[2] as f64, b.origin[1] + s[3] as f64, def.drape_level_mm];
                if let (Some((p, _)), Some((q, _))) = (cam.project(a, rect), cam.project(c, rect)) {
                    painter.line_segment([p, q], stroke);
                }
            }
        }
    }

}

impl crate::app::AfroBimApp {
    /// The UNDERLAY window: attach, and for each underlay its status, units, placement, layers
    /// and the reader's report.
    pub(crate) fn underlay_window(&mut self, ctx: &egui::Context) {
        if !self.under.window_open {
            return;
        }
        let mut open = true;
        let (mut attach, mut detach, mut accept, mut zoom) = (false, None, None, None);
        let mut changes: Vec<(ElementId, UnderlayDef)> = Vec::new();
        crate::win::new(ctx, "DXF underlays").open(&mut open).default_width(660.0).default_height(560.0).show(ctx, |ui| {
            ui.label(egui::RichText::new("A DXF drawing attached here is a read-only background reference. The document stores its path and a hash, not the drawing: it is read again whenever the document opens, with a warning if the file is missing or has changed. AfroBIM ships no underlay: you supply the DXF. A DWG must be converted to DXF first (save it as DXF from AutoCAD or Civil 3D).").small());
            if ui.button("Attach a DXF...").clicked() {
                attach = true;
            }
            ui.separator();
            let ids: Vec<(ElementId, String)> = self.doc.elements().iter().filter(|e| e.category == CATEGORY).map(|e| (e.id, e.name.clone())).collect();
            if ids.is_empty() {
                ui.label(egui::RichText::new("No underlay attached.").weak());
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (id, name) in ids {
                    let Some(mut def) = self.doc.find(id).and_then(def_of) else { continue };
                    let before = def.clone();
                    let status = self.under.entry_status(id).cloned();
                    let under = self.under.underlay(id).cloned();
                    egui::CollapsingHeader::new(egui::RichText::new(format!("{name}   {}", Path::new(&def.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())).strong()).id_salt(("underlay", id.0)).default_open(true).show(ui, |ui| {
                        ui.label(egui::RichText::new(&def.path).small().weak());
                        if let Some(s) = &status {
                            let colour = match s {
                                FileStatus::Ok => crate::theme::success(),
                                FileStatus::Reading => egui::Color32::GRAY,
                                _ => crate::theme::danger(),
                            };
                            ui.label(egui::RichText::new(s.line()).color(colour));
                            if let FileStatus::Changed { was, now } = s {
                                ui.label(egui::RichText::new(format!("Hash was {was}, now {now}.")).small().weak());
                                if ui.button("Accept the changed file").clicked() {
                                    accept = Some(id);
                                }
                            }
                        }
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut def.visible, "Visible");
                            ui.label("Fade");
                            ui.add(egui::Slider::new(&mut def.opacity, 0.05..=1.0).show_value(false)).on_hover_text("Opacity of the drawing");
                            ui.label(format!("{:.0} % opaque", def.opacity * 100.0));
                        });
                        egui::Grid::new(("underlay-grid", id.0)).num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                            ui.label("File units");
                            ui.label(if def.units_name.is_empty() { "not read yet".to_string() } else { format!("{}{}", def.units_name, if def.units_assumed { " (assumed: the file does not say)" } else { "" }) });
                            ui.end_row();
                            ui.label("Millimetres per unit");
                            ui.horizontal(|ui| {
                                ui.add(egui::DragValue::new(&mut def.unit_scale_mm).speed(0.1).range(1e-6..=1e9));
                                ui.label(egui::RichText::new(format!("proposed {}", def.proposed_scale_mm)).small().weak());
                                if ui.small_button("Use the proposal").clicked() {
                                    def.unit_scale_mm = def.proposed_scale_mm;
                                }
                            });
                            ui.end_row();
                            ui.label("Insertion offset (mm)");
                            ui.horizontal(|ui| {
                                ui.add(egui::DragValue::new(&mut def.offset_mm[0]).speed(100.0).prefix("X "));
                                ui.add(egui::DragValue::new(&mut def.offset_mm[1]).speed(100.0).prefix("Y "));
                            });
                            ui.end_row();
                            ui.label("Rotation (degrees)");
                            ui.add(egui::DragValue::new(&mut def.rotation_deg).speed(0.5));
                            ui.end_row();
                            ui.label("Scale");
                            ui.add(egui::DragValue::new(&mut def.scale).speed(0.01).range(1e-6..=1e6));
                            ui.end_row();
                            ui.label("3D site view");
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut def.drape_3d, "Draped flat at");
                                ui.add(egui::DragValue::new(&mut def.drape_level_mm).speed(100.0).suffix(" mm"));
                            });
                            ui.end_row();
                        });
                        if let Some(u) = &under {
                            ui.label(egui::RichText::new("Layers of the file").strong());
                            egui::ScrollArea::vertical().id_salt(("underlay-layers", id.0)).max_height(120.0).show(ui, |ui| {
                                for l in &u.layers {
                                    let mut on = !def.hidden_layers.iter().any(|h| h.eq_ignore_ascii_case(&l.name));
                                    let file_off = !l.visible();
                                    let label = if file_off { format!("{} (off or frozen in the file)", l.name) } else { l.name.clone() };
                                    if ui.checkbox(&mut on, label).changed() {
                                        def.hidden_layers.retain(|h| !h.eq_ignore_ascii_case(&l.name));
                                        if !on {
                                            def.hidden_layers.push(l.name.clone());
                                        }
                                    }
                                }
                            });
                            egui::CollapsingHeader::new(format!("Reader report: {} entities read, {} not read", u.report.total_read(), u.report.total_unread())).id_salt(("underlay-report", id.0)).default_open(true).show(ui, |ui| {
                                ui.label(format!("DXF version: {}", u.report.dxf_version.clone().unwrap_or_else(|| "not stated".into())));
                                ui.label(egui::RichText::new("Read, by DXF type").strong());
                                ui.label(u.report.read.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(",  "));
                                if !u.report.unread.is_empty() {
                                    ui.label(egui::RichText::new("Not read, by type").strong());
                                    for (k, v) in &u.report.unread {
                                        ui.label(egui::RichText::new(format!("{k}: {v}")).color(crate::theme::warn()));
                                    }
                                }
                                if u.report.paper_space_skipped > 0 {
                                    ui.label(format!("Paper space entities skipped: {}", u.report.paper_space_skipped));
                                }
                                for w in u.report.warnings.iter().take(10) {
                                    ui.label(egui::RichText::new(w).small());
                                }
                                ui.label(egui::RichText::new("Not drawn from the file: line types, line weights, hatch patterns, fonts, styles, xrefs. Z is dropped (plan view).").small().weak());
                            });
                        }
                        if let Some(b) = self.under.built(id) {
                            ui.label(egui::RichText::new(format!("Drawn at this zoom: {} segments from {} items, {} texts. {}", b.segs.len(), b.items, b.labels.len(), if b.truncated { "The file hit the item limit: part of it is not drawn." } else { "" })).small().weak());
                            for w in b.warnings.iter().take(5) {
                                ui.label(egui::RichText::new(w).small().color(crate::theme::warn()));
                            }
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
            });
            if self.under.left_out.get() > 0 {
                ui.label(egui::RichText::new(format!("{} segments are left out of the last picture (the cap is {DRAW_CAP}): zoom in.", self.under.left_out.get())).small().color(crate::theme::warn()));
            }
        });
        for (id, def) in changes {
            set_def(&mut self.doc, id, &def);
            self.schedule_commit_now();
        }
        if attach {
            if let Some(path) = rfd::FileDialog::new().add_filter("DXF drawing", &["dxf"]).add_filter("any file", &["*"]).set_title("Attach a DXF underlay").pick_file() {
                match self.under.attach(&mut self.doc, &path) {
                    Ok(id) => {
                        self.under.selected = Some(id);
                        self.status = format!("Attached {}: reading in the background.", path.display());
                        self.schedule_commit_now();
                    }
                    Err(e) => self.status = e,
                }
            }
        }
        if let Some(id) = accept {
            self.under.accept_change(&mut self.doc, id);
            self.schedule_commit_now();
        }
        if let Some(id) = detach {
            self.under.detach(&mut self.doc, id);
            self.schedule_commit_now();
        }
        if let Some(id) = zoom {
            if let Some((lo, hi)) = self.under.built(id).and_then(|b| b.bounds) {
                let (l, t, w, h) = self.ctl.viewport;
                self.view.fit(&afrobim_kernel::BBox::new(lo.x, lo.y, hi.x, hi.y), l, t, w, h, 60.0);
            }
        }
        self.under.window_open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dxf(name: &str, lines: usize) -> PathBuf {
        let mut dxf = afrobim_dxf::DxfDocument::new();
        for i in 0..lines {
            dxf.add_line("ROAD", 7, Pt::new(i as f64 * 1000.0, 0.0), Pt::new(i as f64 * 1000.0, 2000.0));
        }
        dxf.add_arc("KERB", 3, Pt::new(0.0, 0.0), 5000.0, 0.0, 90.0);
        let dir = std::env::temp_dir().join(format!("afrobim-u1-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, dxf.finish()).unwrap();
        path
    }

    #[test]
    fn an_underlay_attaches_from_the_apps_own_dxf_with_the_expected_entities_and_detects_a_changed_file() {
        let path = temp_dxf("site.dxf", 5);
        let mut doc = Document::default();
        let mut st = UnderlayState::default();
        let id = st.attach(&mut doc, &path).unwrap();
        st.wait(&mut doc, 0.1);
        let under = st.underlay(id).expect("the file was read");
        assert_eq!(under.report.read.get("LINE"), Some(&5));
        assert_eq!(under.report.read.get("ARC"), Some(&1));
        assert_eq!(st.entry_status(id), Some(&FileStatus::Ok));
        let def = def_of(doc.find(id).unwrap()).unwrap();
        assert!(!def.hash.is_empty() && def.size > 0);
        // Units are the proposal from the header and can be overridden; the drawing follows.
        let drawn = st.built(id).unwrap();
        assert!(drawn.segs.len() >= 5 + 4, "{} segments", drawn.segs.len());
        // The record is saved with the document and the drawing is not.
        let json = doc.to_json();
        assert!(json.contains(&def.hash) && !json.contains("segs"));
        // A different session: the document is opened, the file read again, same hash: fine.
        let mut doc2 = Document::from_json(&json).unwrap();
        let mut st2 = UnderlayState::default();
        st2.wait(&mut doc2, 0.1);
        let id2 = doc2.elements().iter().find(|e| e.category == CATEGORY).unwrap().id;
        assert_eq!(st2.entry_status(id2), Some(&FileStatus::Ok));
        // The file changes: the next open says so, and accepting clears it.
        let mut again = afrobim_dxf::DxfDocument::new();
        again.add_line("ROAD", 7, Pt::new(0.0, 0.0), Pt::new(10.0, 10.0));
        std::fs::write(&path, again.finish()).unwrap();
        let mut doc3 = Document::from_json(&json).unwrap();
        let mut st3 = UnderlayState::default();
        st3.wait(&mut doc3, 0.1);
        let id3 = doc3.elements().iter().find(|e| e.category == CATEGORY).unwrap().id;
        assert!(matches!(st3.entry_status(id3), Some(FileStatus::Changed { .. })), "{:?}", st3.entry_status(id3));
        st3.accept_change(&mut doc3, id3);
        assert_eq!(st3.entry_status(id3), Some(&FileStatus::Ok));
        // Missing files are named, not fatal.
        std::fs::remove_file(&path).unwrap();
        let mut doc4 = Document::from_json(&json).unwrap();
        let mut st4 = UnderlayState::default();
        st4.wait(&mut doc4, 0.1);
        let id4 = doc4.elements().iter().find(|e| e.category == CATEGORY).unwrap().id;
        assert_eq!(st4.entry_status(id4), Some(&FileStatus::Missing));
    }

    #[test]
    fn placement_scale_rotation_and_layer_switches_move_the_drawing_and_the_snaps() {
        let path = temp_dxf("place.dxf", 2);
        let under = reader::read_file(&path).unwrap();
        let mut def = UnderlayDef::new(&path);
        def.unit_scale_mm = 1.0;
        let plain = build(&under, &def, 0);
        let (lo, hi) = plain.bounds.unwrap();
        assert!(lo.x.abs() < 1e-6 && lo.y.abs() < 1e-6 && (hi.x - 5000.0).abs() < 1.0, "{lo:?} {hi:?}");
        // Metres in the file: a thousand times larger.
        def.unit_scale_mm = 1000.0;
        let (_, hi_m) = build(&under, &def, 4).bounds.unwrap();
        assert!((hi_m.x - 5_000_000.0).abs() < 1000.0);
        def.unit_scale_mm = 1.0;
        // Offset and a quarter turn: the point (1000, 0) goes to (offset.x, offset.y + 1000).
        def.offset_mm = [10_000.0, 20_000.0];
        def.rotation_deg = 90.0;
        let snaps = snap_points(&under, &def);
        assert!(snaps.iter().any(|(p, m)| *m == SnapMode::Endpoint && (p.x - 10_000.0).abs() < 1e-6 && (p.y - 21_000.0).abs() < 1e-6), "{:?}", &snaps[..snaps.len().min(6)]);
        // A layer switched off leaves the drawing: KERB is the only arc.
        let with = build(&under, &def, 0).segs.len();
        def.hidden_layers = vec!["KERB".into()];
        let without = build(&under, &def, 0).segs.len();
        assert!(without < with, "{without} {with}");
        // The tolerance follows the zoom: more pixels per millimetre, a finer tolerance.
        assert!(tolerance_log2(1.0) < tolerance_log2(0.01));
    }

    #[test]
    fn dwg_is_refused_with_the_readers_message() {
        let dir = std::env::temp_dir().join(format!("afrobim-u1-dwg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("site.dwg");
        std::fs::write(&path, b"AC1032\0\0\0\0\0\0\0\0\0\0").unwrap();
        let mut doc = Document::default();
        let mut st = UnderlayState::default();
        let id = st.attach(&mut doc, &path).unwrap();
        st.wait(&mut doc, 0.1);
        match st.entry_status(id) {
            Some(FileStatus::Failed(m)) => assert!(m.contains("Convert it to DXF first"), "{m}"),
            other => panic!("{other:?}"),
        }
    }
}

#[cfg(test)]
mod true_coordinate_tests {
    use super::*;

    /// A DXF written in metres at UTM size (easting 450 123.456, northing 9 960 012.345) is placed
    /// at those numbers times 1000, with no offset added and nothing re-based.
    #[test]
    fn a_dxf_underlay_in_metres_lands_at_source_coordinates() {
        let mut dxf = afrobim_dxf::DxfDocument::new();
        dxf.add_line("ROAD", 7, Pt::new(450_123.456, 9_960_012.345), Pt::new(450_223.456, 9_960_112.345));
        let dir = std::env::temp_dir().join(format!("afrobim-u2-underlay-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("true.dxf");
        std::fs::write(&path, dxf.finish()).unwrap();
        let mut doc = Document::default();
        let mut st = UnderlayState::default();
        let id = st.attach(&mut doc, &path).unwrap();
        st.wait(&mut doc, 0.1);
        let mut def = def_of(doc.find(id).unwrap()).unwrap();
        assert_eq!(def.offset_mm, [0.0, 0.0], "attaching adds no offset");
        // The file is in metres: say so (1000 mm per unit), as the dialog's unit choice does.
        def.unit_scale_mm = 1000.0;
        let built = build(st.underlay(id).unwrap(), &def, 0);
        let (lo, hi) = built.bounds.expect("the line is drawn");
        for (got, want) in [(lo.x, 450_123_456.0), (lo.y, 9_960_012_345.0), (hi.x, 450_223_456.0), (hi.y, 9_960_112_345.0)] {
            assert!((got - want).abs() < 0.001, "{got} != {want}");
        }
        assert_eq!(built.origin, [lo.x, lo.y]);
    }
}
