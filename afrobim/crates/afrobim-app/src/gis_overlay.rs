//! GIS import as a read-only overlay: GeoJSON, KML, GPX and CSV (longitude, latitude) files drawn
//! behind the model, the way Spatial Manager brings a GIS layer into a drawing.
//!
//! One overlay is one document element (category [`CATEGORY`]) for one file. It keeps the file's
//! path and hash, the features **as read, in WGS 84** (the source, which never changes), the same
//! features moved into the project coordinate system in model millimetres (derived from the
//! source), the layer names with a colour and a switch for each, and the display switches.
//!
//! The projected copy is never edited by anything else. It carries the key of the project system it
//! was made for, and [`reproject_all`] makes it again from the WGS 84 source whenever that key
//! differs from the project's: a change of the project system (the `CRS` command, `TRANSFORM`)
//! moves no millimetres, it places the overlay again. When the project has no system with a position
//! on Earth the overlay says so and draws nothing; its source is kept.
//!
//! Positions are `[lon, lat, h]` in the source and the kernel's plan from WGS 84 takes `(lat, lon)`,
//! so the call is `plan.apply(lat, lon)` and its first result is the easting. Heights are not used.
//!
//! Like images and DXF underlays an overlay is a backdrop (see `imagery::is_backdrop`): it is kept
//! out of every export, quantity, pick and arrange. It lives on the hidden underlay layer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use afrobim_kernel::{project_crs, BBox, CrsKind, Document, Element, ElementId, Generated, Module, ProjectCrs, Pt, ViewTransform};
use afrobim_raster::vector::{self, Feature, Geometry};
use egui::{pos2, Color32, Mesh, Painter, Rect, Stroke};
use serde::{Deserialize, Serialize};

pub const CATEGORY: &str = "gis_overlay";
/// What the command offers.
pub const EXTENSIONS: &[&str] = &["geojson", "json", "kml", "gpx", "csv"];
/// What a file dropped on the window may be. A CSV is only ever read through the command, since a
/// CSV on the window may as well be survey points.
pub const DROP_EXTENSIONS: &[&str] = &["geojson", "json", "kml", "gpx"];

/// A polygon with more vertices than this is outlined and not filled (the triangulation is
/// quadratic in the ring size).
const FILL_MAX_VERTS: usize = 3000;
/// The fill is this part of the layer's opacity.
const FILL_ALPHA: f32 = 0.22;

/// Colours given to the layers of a file in turn.
pub const PALETTE: [[u8; 3]; 8] = [[230, 120, 20], [30, 130, 220], [40, 160, 80], [200, 50, 110], [130, 80, 200], [190, 160, 20], [20, 160, 170], [150, 90, 50]];

// ─── what the document stores ───────────────────────────────────────────────

/// One layer (a KML folder, a GeoJSON `layer` property, a GPX kind) of the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerDef {
    pub name: String,
    pub colour: [u8; 3],
    pub visible: bool,
}

/// A geometry in WGS 84, `[lon, lat, h]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Geom {
    Point([f64; 3]),
    Line(Vec<[f64; 3]>),
    Area(Vec<Vec<[f64; 3]>>),
}

/// One feature as read, in WGS 84.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SrcFeature {
    pub name: String,
    /// Index into the layers.
    pub layer: usize,
    pub geom: Geom,
}

/// A geometry in the project system, model millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PGeom {
    Point([f64; 2]),
    Line(Vec<[f64; 2]>),
    Area(Vec<Vec<[f64; 2]>>),
}

/// A feature moved into the project system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjFeature {
    /// Index into the source features.
    pub src: usize,
    pub layer: usize,
    pub geom: PGeom,
}

/// What the document stores for one overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayDef {
    pub path: String,
    /// FNV-1a 64 of the file when it was read, hex.
    pub hash: String,
    pub size: u64,
    /// "GeoJSON", "KML", "GPX" or "CSV".
    pub kind: String,
    pub visible: bool,
    pub opacity: f32,
    pub layers: Vec<LayerDef>,
    /// The features as read, WGS 84. Never edited.
    pub source: Vec<SrcFeature>,
    /// The features in the project system, millimetres, made from `source`.
    #[serde(default)]
    pub projected: Vec<ProjFeature>,
    /// The project system `projected` was made for (serialised), "null" for none.
    #[serde(default)]
    pub proj_key: String,
    /// Features left out because a position could not be moved into the project system.
    #[serde(default)]
    pub failed: usize,
    /// Why nothing is placed, or empty.
    #[serde(default)]
    pub error: String,
    /// How the features were placed, in words.
    #[serde(default)]
    pub how: String,
    /// Changes whenever the geometry above is rewritten; the runtime cache is keyed on it.
    #[serde(default)]
    pub rev: u64,
}

/// The switches that change often and are read every frame. Edited in the element's data in
/// place, without touching the geometry or its revision.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub visible: bool,
    pub opacity: f32,
    pub layers: Vec<LayerDef>,
}

pub fn look_of(e: &Element) -> Option<Look> {
    if e.category != CATEGORY {
        return None;
    }
    Some(Look {
        visible: e.data.get("visible").and_then(|v| v.as_bool()).unwrap_or(true),
        opacity: e.data.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
        layers: e.data.get("layers").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default(),
    })
}

/// Changes the display switches of an overlay.
pub fn edit_look(doc: &mut Document, id: ElementId, f: impl FnOnce(&mut Look)) {
    let Some(e) = doc.find_mut(id) else { return };
    let Some(mut look) = look_of(e) else { return };
    f(&mut look);
    if let Some(map) = e.data.as_object_mut() {
        map.insert("visible".into(), look.visible.into());
        map.insert("opacity".into(), (look.opacity as f64).into());
        map.insert("layers".into(), serde_json::to_value(&look.layers).unwrap_or_default());
    }
}

pub fn def_of(e: &Element) -> Option<OverlayDef> {
    (e.category == CATEGORY).then(|| serde_json::from_value(e.data.clone()).ok()).flatten()
}

fn next_rev() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    // Seeded from the clock, so a revision read from a saved document is not issued again.
    let _ = NEXT.compare_exchange(0, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1), Ordering::SeqCst, Ordering::SeqCst);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

/// Writes a whole definition back (new revision).
pub fn set_def(doc: &mut Document, id: ElementId, def: &mut OverlayDef) {
    def.rev = next_rev();
    if let (Some(e), Ok(v)) = (doc.find_mut(id), serde_json::to_value(&*def)) {
        e.data = v;
    }
}

// ─── the module of the placeholder element ──────────────────────────────────

/// The module of the placeholder element: it draws nothing.
pub struct GisModule;

impl Module for GisModule {
    fn category(&self) -> &str {
        CATEGORY
    }
    fn display_name(&self) -> &str {
        "GIS overlays"
    }
    fn generate(&self, e: &Element) -> Result<Generated, String> {
        if e.data.get("source").is_none() {
            return Err("The GIS overlay record cannot be read.".into());
        }
        let mut g = Generated::default();
        g.derived.insert("File".into(), e.data.get("path").and_then(|v| v.as_str()).unwrap_or_default().to_string());
        g.derived.insert("Features".into(), e.data.get("source").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0).to_string());
        g.derived.insert("Layers".into(), e.data.get("layers").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0).to_string());
        g.drawing.extents = afrobim_draw::Extents { min: Pt::new(0.0, 0.0), max: Pt::new(1.0, 1.0) };
        Ok(g)
    }
}

// ─── refusing, reading and projecting ───────────────────────────────────────

/// Why the project cannot hold GIS data, in words for an import; None when it can.
pub fn refusal(project: Option<&ProjectCrs>) -> Option<String> {
    match project {
        None => Some("The project has no coordinate system, so GIS data cannot be placed: AfroBIM needs to know where the model is on Earth. Set the system with CRS first.".into()),
        Some(p) if p.kind == CrsKind::LocalGrid => Some(format!("The project is on a local site grid ({}), which has no position on Earth: GIS data in WGS 84 cannot be placed in it. Set a real system with CRS, or calibrate the site (TRANSFORM), first.", p.label())),
        Some(p) if p.kind == CrsKind::NamedOnly => Some(format!("The project's system ({}) is known by name only: AfroBIM cannot compute positions in it, so GIS data cannot be placed. Choose a system it can compute with (CRS).", p.label())),
        Some(_) => None,
    }
}

/// The key a projected copy is tagged with.
pub fn project_key(project: Option<&ProjectCrs>) -> String {
    serde_json::to_string(&project).unwrap_or_default()
}

fn kind_of(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("kml") => "KML",
        Some("gpx") => "GPX",
        Some("csv") | Some("txt") => "CSV",
        _ => "GeoJSON",
    }
}

/// The features as stored (WGS 84) and their layers, colours given in turn.
fn store_features(features: &[Feature]) -> (Vec<SrcFeature>, Vec<LayerDef>) {
    let mut layers: Vec<LayerDef> = Vec::new();
    let mut source = Vec::with_capacity(features.len());
    for f in features {
        let name = if f.layer.trim().is_empty() { "Features" } else { f.layer.as_str() };
        let layer = match layers.iter().position(|l| l.name == name) {
            Some(i) => i,
            None => {
                layers.push(LayerDef { name: name.to_string(), colour: PALETTE[layers.len() % PALETTE.len()], visible: true });
                layers.len() - 1
            }
        };
        let geom = match &f.geometry {
            Geometry::Point(p) => Geom::Point(*p),
            Geometry::LineString(v) => Geom::Line(v.clone()),
            Geometry::Polygon(r) => Geom::Area(r.clone()),
        };
        source.push(SrcFeature { name: f.name.clone(), layer, geom });
    }
    (source, layers)
}

/// Moves the source of `def` into `project`, and records the key, the count of features that could
/// not be moved, how, or why not. The source is never touched.
pub fn apply_projection(def: &mut OverlayDef, project: Option<&ProjectCrs>) {
    def.proj_key = project_key(project);
    def.projected.clear();
    def.failed = 0;
    def.how.clear();
    def.error.clear();
    if let Some(why) = refusal(project) {
        def.error = why;
        return;
    }
    let (_, from, how) = match crate::basemap::plans_for(project) {
        Ok(p) => p,
        Err(e) => {
            def.error = e;
            return;
        }
    };
    // The plan takes (latitude, longitude) and gives (easting, northing) for a projected system.
    let conv = |p: &[f64; 3]| -> Option<[f64; 2]> {
        let r = from.apply(p[1], p[0], 0.0).ok()?;
        let (x, y) = (r.first * 1000.0, r.second * 1000.0);
        (x.is_finite() && y.is_finite()).then_some([x, y])
    };
    for (i, f) in def.source.iter().enumerate() {
        let geom = match &f.geom {
            Geom::Point(p) => conv(p).map(PGeom::Point),
            Geom::Line(v) => v.iter().map(conv).collect::<Option<Vec<_>>>().map(PGeom::Line),
            Geom::Area(rings) => rings.iter().map(|r| r.iter().map(conv).collect::<Option<Vec<_>>>()).collect::<Option<Vec<_>>>().map(PGeom::Area),
        };
        match geom {
            Some(g) => def.projected.push(ProjFeature { src: i, layer: f.layer, geom: g }),
            None => def.failed += 1,
        }
    }
    if def.projected.is_empty() && !def.source.is_empty() {
        def.error = format!("None of the {} features could be placed in {}: every position is outside what the projection can compute.", def.source.len(), project.map(|p| p.label()).unwrap_or_default());
        return;
    }
    def.how = how;
}

/// What an import or a re-read did, for the status line.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub features: usize,
    pub points: usize,
    pub lines: usize,
    pub areas: usize,
    pub layers: usize,
    pub failed: usize,
    pub how: String,
}

impl Report {
    fn of(def: &OverlayDef) -> Report {
        let (mut points, mut lines, mut areas) = (0, 0, 0);
        for f in &def.source {
            match f.geom {
                Geom::Point(_) => points += 1,
                Geom::Line(_) => lines += 1,
                Geom::Area(_) => areas += 1,
            }
        }
        Report { features: def.source.len(), points, lines, areas, layers: def.layers.len(), failed: def.failed, how: def.how.clone() }
    }

    pub fn line(&self, file: &str) -> String {
        let failed = if self.failed > 0 { format!(" {} feature(s) could not be placed in the project system and are left out.", self.failed) } else { String::new() };
        let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
        format!("{file}: {} ({}, {}, {}) in {}.{failed} {}", n(self.features, "feature", "features"), n(self.points, "point", "points"), n(self.lines, "line", "lines"), n(self.areas, "polygon", "polygons"), n(self.layers, "layer", "layers"), self.how)
    }
}

/// Reads a GIS file and adds it to the document as an overlay in the project system. Nothing is
/// added when the project has no system with a position on Earth, when the file cannot be read, or
/// when it holds nothing that can be placed.
pub fn import(doc: &mut Document, path: &Path) -> Result<(ElementId, Report), String> {
    let project = project_crs(doc);
    if let Some(why) = refusal(project.as_ref()) {
        return Err(format!("Not imported: {why}"));
    }
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !path.is_file() {
        return Err(format!("Not imported: {} does not exist.", path.display()));
    }
    let features = vector::read_any(&path).map_err(|e| format!("Not imported: {e}"))?;
    if features.is_empty() {
        return Err(format!("Not imported: {} holds no features.", path.display()));
    }
    let (hash, size) = crate::underlay::hash_file(&path).map_err(|e| format!("Not imported: {} could not be read: {e}", path.display()))?;
    let (source, layers) = store_features(&features);
    let mut def = OverlayDef { path: path.to_string_lossy().to_string(), hash, size, kind: kind_of(&path).into(), visible: true, opacity: 1.0, layers, source, projected: Vec::new(), proj_key: String::new(), failed: 0, error: String::new(), how: String::new(), rev: 0 };
    apply_projection(&mut def, project.as_ref());
    if !def.error.is_empty() {
        return Err(format!("Not imported: {}", def.error));
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "GIS".into());
    let name = doc.unique_name(&format!("GIS-{stem}"));
    doc.ensure_layer(crate::underlay::LAYER);
    if let Some(l) = doc.layers_mut().iter_mut().find(|l| l.name == crate::underlay::LAYER) {
        l.visible = false;
    }
    def.rev = next_rev();
    let id = doc.add(CATEGORY, &name, Pt::new(0.0, 0.0), 1.0, 1.0, crate::underlay::LAYER, serde_json::to_value(&def).map_err(|e| e.to_string())?);
    Ok((id, Report::of(&def)))
}

/// Reads the file again. The colour and switch of a layer that is still in the file are kept.
pub fn reread(doc: &mut Document, id: ElementId) -> Result<Report, String> {
    let mut def = doc.find(id).and_then(def_of).ok_or("The overlay is no longer in the document.")?;
    let path = PathBuf::from(&def.path);
    if !path.is_file() {
        return Err(format!("{} is missing: the features already read are kept.", def.path));
    }
    let features = vector::read_any(&path).map_err(|e| format!("Not read again: {e} (the features already read are kept)."))?;
    if features.is_empty() {
        return Err(format!("{} now holds no features: the features already read are kept.", def.path));
    }
    let (hash, size) = crate::underlay::hash_file(&path).map_err(|e| format!("{} could not be read: {e}", def.path))?;
    let (source, mut layers) = store_features(&features);
    for l in &mut layers {
        if let Some(old) = def.layers.iter().find(|o| o.name == l.name) {
            l.colour = old.colour;
            l.visible = old.visible;
        }
    }
    (def.source, def.layers, def.hash, def.size, def.kind) = (source, layers, hash, size, kind_of(&path).into());
    apply_projection(&mut def, project_crs(doc).as_ref());
    let report = Report::of(&def);
    set_def(doc, id, &mut def);
    Ok(report)
}

/// Makes the projected copy again, from the WGS 84 source, for every overlay made for another
/// project system than the present one. Returns how many were made again. Idempotent.
pub fn reproject_all(doc: &mut Document) -> usize {
    let project = project_crs(doc);
    let key = project_key(project.as_ref());
    let stale: Vec<ElementId> = doc.elements().iter().filter(|e| e.category == CATEGORY && e.data.get("proj_key").and_then(|v| v.as_str()) != Some(key.as_str())).map(|e| e.id).collect();
    let mut n = 0;
    for id in stale {
        let Some(mut def) = doc.find(id).and_then(def_of) else { continue };
        apply_projection(&mut def, project.as_ref());
        set_def(doc, id, &mut def);
        n += 1;
    }
    n
}

// ─── the runtime side ───────────────────────────────────────────────────────

/// What was found when the file was compared with the hash kept.
#[derive(Debug, Clone, PartialEq)]
pub enum FileStatus {
    Same,
    Changed,
    Missing,
    Unreadable(String),
}

impl FileStatus {
    pub fn line(&self) -> String {
        match self {
            FileStatus::Same => "The file is the one that was read.".into(),
            FileStatus::Changed => "The file has changed since it was read. Re-read it to bring the changes in.".into(),
            FileStatus::Missing => "The file is missing. The features already read are kept and drawn.".into(),
            FileStatus::Unreadable(e) => format!("The file could not be checked: {e}"),
        }
    }
}

pub fn check_file(path: &str, hash: &str) -> FileStatus {
    let p = Path::new(path);
    if !p.is_file() {
        return FileStatus::Missing;
    }
    match crate::underlay::hash_file(p) {
        Ok((h, _)) if h == hash => FileStatus::Same,
        Ok(_) => FileStatus::Changed,
        Err(e) => FileStatus::Unreadable(e.to_string()),
    }
}

/// What is drawn for one projected feature.
struct Prep {
    bbox: [f64; 4],
    /// Triangles of a polygon's fill, model millimetres; empty for points and lines.
    tris: Vec<[[f64; 2]; 3]>,
}

/// The parts of an overlay the canvas and the window need, made once for each revision.
pub struct Geo {
    rev: u64,
    proj_key: String,
    pub error: String,
    pub how: String,
    pub failed: usize,
    pub total: usize,
    pub points: usize,
    pub lines: usize,
    pub areas: usize,
    /// Placed features of each layer.
    pub per_layer: Vec<usize>,
    projected: Vec<ProjFeature>,
    prep: Vec<Prep>,
    bounds: Option<[f64; 4]>,
}

fn bbox_of(pts: impl Iterator<Item = [f64; 2]>) -> [f64; 4] {
    pts.fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])
}

/// The fill triangles of a polygon: the outer ring first, then holes.
pub fn fill_triangles(rings: &[Vec<[f64; 2]>]) -> Vec<[[f64; 2]; 3]> {
    let open = |r: &Vec<[f64; 2]>| -> Vec<[f64; 2]> {
        let mut v = r.clone();
        if v.len() > 1 && v.first() == v.last() {
            v.pop();
        }
        v
    };
    let rings: Vec<Vec<[f64; 2]>> = rings.iter().map(open).filter(|r| r.len() >= 3).collect();
    let Some(outer) = rings.first() else { return Vec::new() };
    if rings.iter().map(|r| r.len()).sum::<usize>() > FILL_MAX_VERTS {
        return Vec::new();
    }
    // Relative to a corner, so that the arithmetic works on numbers near zero (UTM millimetres are around 1e10).
    let o = outer[0];
    let rel = |r: &Vec<[f64; 2]>| -> Vec<[f64; 2]> { r.iter().map(|p| [p[0] - o[0], p[1] - o[1]]).collect() };
    let (outer_r, holes): (Vec<[f64; 2]>, Vec<Vec<[f64; 2]>>) = (rel(outer), rings[1..].iter().map(rel).collect());
    let all: Vec<[f64; 2]> = outer_r.iter().cloned().chain(holes.iter().flatten().cloned()).collect();
    afrobim_ifc::import::triangulate(&outer_r, &holes).iter().map(|t| [0, 1, 2].map(|k| [all[t[k] as usize][0] + o[0], all[t[k] as usize][1] + o[1]])).collect()
}

impl Geo {
    fn build(def: OverlayDef) -> Geo {
        let mut per_layer = vec![0usize; def.layers.len()];
        let (mut points, mut lines, mut areas) = (0, 0, 0);
        let mut prep = Vec::with_capacity(def.projected.len());
        let mut bounds: Option<[f64; 4]> = None;
        for f in &def.projected {
            if let Some(n) = per_layer.get_mut(f.layer) {
                *n += 1;
            }
            let (bbox, tris) = match &f.geom {
                PGeom::Point(p) => {
                    points += 1;
                    ([p[0], p[1], p[0], p[1]], Vec::new())
                }
                PGeom::Line(v) => {
                    lines += 1;
                    (bbox_of(v.iter().cloned()), Vec::new())
                }
                PGeom::Area(rings) => {
                    areas += 1;
                    (bbox_of(rings.iter().flatten().cloned()), fill_triangles(rings))
                }
            };
            bounds = Some(match bounds {
                Some(b) => [b[0].min(bbox[0]), b[1].min(bbox[1]), b[2].max(bbox[2]), b[3].max(bbox[3])],
                None => bbox,
            });
            prep.push(Prep { bbox, tris });
        }
        Geo { rev: def.rev, proj_key: def.proj_key, error: def.error, how: def.how, failed: def.failed, total: def.source.len(), points, lines, areas, per_layer, projected: def.projected, prep, bounds }
    }
}

/// The runtime side of the overlays: what each one draws, the window.
#[derive(Default)]
pub struct GisState {
    entries: HashMap<ElementId, Arc<Geo>>,
    pkey: String,
    pub window_open: bool,
    /// Set when a sync changed the document (the caller takes an undo step).
    pub doc_changed: bool,
    status: HashMap<ElementId, FileStatus>,
    /// The window was drawn last frame.
    shown: bool,
    /// The last message of the window: (is an error, text).
    pub message: Option<(bool, String)>,
}

impl GisState {
    /// Forgets everything (a different document).
    pub fn reset(&mut self) {
        let open = self.window_open;
        *self = GisState::default();
        self.window_open = open;
    }

    /// Once a frame: places again the overlays made for another project system, and makes what the
    /// canvas draws for a changed record.
    pub fn sync(&mut self, doc: &mut Document) {
        if reproject_all(doc) > 0 {
            self.doc_changed = true;
        }
        self.pkey = project_key(project_crs(doc).as_ref());
        let ids: Vec<ElementId> = doc.elements().iter().filter(|e| e.category == CATEGORY).map(|e| e.id).collect();
        self.entries.retain(|id, _| ids.contains(id));
        self.status.retain(|id, _| ids.contains(id));
        for e in doc.elements().iter().filter(|e| e.category == CATEGORY) {
            let rev = e.data.get("rev").and_then(|v| v.as_u64()).unwrap_or(0);
            if self.entries.get(&e.id).map(|g| g.rev) != Some(rev) {
                self.status.remove(&e.id);
                if let Some(def) = def_of(e) {
                    self.entries.insert(e.id, Arc::new(Geo::build(def)));
                }
            }
        }
    }

    fn placed(&self, id: ElementId) -> Option<&Arc<Geo>> {
        self.entries.get(&id).filter(|g| g.error.is_empty() && g.proj_key == self.pkey)
    }

    /// What the overlay has to show now, if it is placed.
    pub fn geo(&self, id: ElementId) -> Option<&Geo> {
        self.entries.get(&id).map(|g| g.as_ref())
    }

    pub fn detach(&mut self, doc: &mut Document, id: ElementId) {
        doc.remove(id);
        self.entries.remove(&id);
        self.status.remove(&id);
    }

    /// Model extents of the placed, visible overlays.
    pub fn extents(&self, doc: &Document) -> Option<BBox> {
        doc.elements()
            .iter()
            .filter(|e| look_of(e).is_some_and(|l| l.visible))
            .filter_map(|e| self.placed(e.id).and_then(|g| g.bounds))
            .map(|b| BBox::new(b[0], b[1], b[2], b[3]))
            .reduce(|a, b| a.union(&b))
    }

    /// The extents of one overlay.
    pub fn extents_of(&self, id: ElementId) -> Option<BBox> {
        self.placed(id).and_then(|g| g.bounds).map(|b| BBox::new(b[0], b[1], b[2], b[3]))
    }

    /// Draws the visible overlays: lines, outlines, a light fill and crosses for points, in the
    /// colour of the layer.
    pub fn paint(&self, doc: &Document, painter: &Painter, view: &ViewTransform, rect: Rect) {
        for e in doc.elements().iter().filter(|e| e.category == CATEGORY) {
            let (Some(geo), Some(look)) = (self.placed(e.id), look_of(e)) else { continue };
            if look.visible {
                paint_geo(geo, &look, painter, view, rect);
            }
        }
    }
}

fn clip_to_rect(poly: &[[f64; 2]], lo: [f64; 2], hi: [f64; 2]) -> Vec<[f64; 2]> {
    // Sutherland-Hodgman against the four sides of the rectangle.
    let mut out: Vec<[f64; 2]> = poly.to_vec();
    for side in 0..4 {
        let inside = |p: [f64; 2]| match side {
            0 => p[0] >= lo[0],
            1 => p[0] <= hi[0],
            2 => p[1] >= lo[1],
            _ => p[1] <= hi[1],
        };
        let cut = |a: [f64; 2], b: [f64; 2]| -> [f64; 2] {
            match side {
                0 | 1 => {
                    let x = if side == 0 { lo[0] } else { hi[0] };
                    [x, a[1] + (b[1] - a[1]) * (x - a[0]) / (b[0] - a[0])]
                }
                _ => {
                    let y = if side == 2 { lo[1] } else { hi[1] };
                    [a[0] + (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]), y]
                }
            }
        };
        let input = std::mem::take(&mut out);
        for i in 0..input.len() {
            let (a, b) = (input[i], input[(i + 1) % input.len()]);
            match (inside(a), inside(b)) {
                (true, true) => out.push(b),
                (true, false) => out.push(cut(a, b)),
                (false, true) => {
                    out.push(cut(a, b));
                    out.push(b);
                }
                (false, false) => {}
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

fn paint_geo(geo: &Geo, look: &Look, painter: &Painter, view: &ViewTransform, rect: Rect) {
    // The view in model millimetres, with room for the size of a cross.
    let (a, b) = (view.to_world(Pt::new(rect.left() as f64, rect.bottom() as f64)), view.to_world(Pt::new(rect.right() as f64, rect.top() as f64)));
    let pad = 12.0 / view.scale.max(1e-12);
    let (lo, hi) = ([a.x.min(b.x) - pad, a.y.min(b.y) - pad], [a.x.max(b.x) + pad, a.y.max(b.y) + pad]);
    // Screen clip box.
    let (slo, shi) = ([rect.left() as f64 - 2.0, rect.top() as f64 - 2.0], [rect.right() as f64 + 2.0, rect.bottom() as f64 + 2.0]);
    let opacity = look.opacity.clamp(0.05, 1.0);
    let alpha = (255.0 * opacity) as u8;
    let fill_alpha = (255.0 * opacity * FILL_ALPHA) as u8;
    let to_screen = |p: [f64; 2]| -> [f64; 2] {
        let s = view.to_screen(Pt::new(p[0], p[1]));
        [s.x, s.y]
    };
    let mut fill = Mesh::default();
    for (f, prep) in geo.projected.iter().zip(&geo.prep) {
        let Some(layer) = look.layers.get(f.layer).filter(|l| l.visible) else { continue };
        if prep.bbox[2] < lo[0] || prep.bbox[0] > hi[0] || prep.bbox[3] < lo[1] || prep.bbox[1] > hi[1] {
            continue;
        }
        let [r, g, bl] = layer.colour;
        let ink = Color32::from_rgba_unmultiplied(r, g, bl, alpha);
        let stroke = Stroke::new(1.5_f32, ink);
        let polyline = |pts: &[[f64; 2]]| {
            let mut run: Vec<egui::Pos2> = Vec::new();
            let mut end: Option<[f64; 2]> = None;
            for w in pts.windows(2) {
                let (sa, sb) = (to_screen(w[0]), to_screen(w[1]));
                match crate::render::clip_segment_box(Pt::new(sa[0], sa[1]), Pt::new(sb[0], sb[1]), Pt::new(slo[0], slo[1]), Pt::new(shi[0], shi[1])) {
                    Some((ca, cb)) => {
                        if end.is_none_or(|e| (e[0] - ca.x).abs() > 1e-6 || (e[1] - ca.y).abs() > 1e-6) {
                            if run.len() >= 2 {
                                painter.add(egui::Shape::line(std::mem::take(&mut run), stroke));
                            }
                            run.clear();
                            run.push(pos2(ca.x as f32, ca.y as f32));
                        }
                        run.push(pos2(cb.x as f32, cb.y as f32));
                        end = Some([cb.x, cb.y]);
                    }
                    None => {
                        if run.len() >= 2 {
                            painter.add(egui::Shape::line(std::mem::take(&mut run), stroke));
                        }
                        run.clear();
                        end = None;
                    }
                }
            }
            if run.len() >= 2 {
                painter.add(egui::Shape::line(run, stroke));
            }
        };
        match &f.geom {
            PGeom::Point(p) => {
                let s = to_screen(*p);
                if s[0] >= slo[0] && s[0] <= shi[0] && s[1] >= slo[1] && s[1] <= shi[1] {
                    let c = pos2(s[0] as f32, s[1] as f32);
                    painter.line_segment([c - egui::vec2(4.5, 0.0), c + egui::vec2(4.5, 0.0)], stroke);
                    painter.line_segment([c - egui::vec2(0.0, 4.5), c + egui::vec2(0.0, 4.5)], stroke);
                }
            }
            PGeom::Line(v) => polyline(v),
            PGeom::Area(rings) => {
                for ring in rings {
                    polyline(ring);
                }
                let tint = Color32::from_rgba_unmultiplied(r, g, bl, fill_alpha);
                for t in &prep.tris {
                    let s = [to_screen(t[0]), to_screen(t[1]), to_screen(t[2])];
                    let (x0, x1) = (s.iter().map(|p| p[0]).fold(f64::MAX, f64::min), s.iter().map(|p| p[0]).fold(f64::MIN, f64::max));
                    let (y0, y1) = (s.iter().map(|p| p[1]).fold(f64::MAX, f64::min), s.iter().map(|p| p[1]).fold(f64::MIN, f64::max));
                    if x1 < slo[0] || x0 > shi[0] || y1 < slo[1] || y0 > shi[1] {
                        continue;
                    }
                    let poly = if x0 >= slo[0] && x1 <= shi[0] && y0 >= slo[1] && y1 <= shi[1] { s.to_vec() } else { clip_to_rect(&s, slo, shi) };
                    if poly.len() < 3 {
                        continue;
                    }
                    let base = fill.vertices.len() as u32;
                    for p in &poly {
                        fill.vertices.push(egui::epaint::Vertex { pos: pos2(p[0] as f32, p[1] as f32), uv: egui::epaint::WHITE_UV, color: tint });
                    }
                    for k in 1..poly.len() as u32 - 1 {
                        fill.indices.extend_from_slice(&[base, base + k, base + k + 1]);
                    }
                }
            }
        }
    }
    if !fill.is_empty() {
        painter.add(egui::Shape::mesh(fill));
    }
}

// ─── the window ─────────────────────────────────────────────────────────────

enum Edit {
    Visible(ElementId, bool),
    Opacity(ElementId, f32),
    LayerVisible(ElementId, usize, bool),
    LayerColour(ElementId, usize, [u8; 3]),
}

impl crate::app::AfroBimApp {
    /// The GISIMPORT window: each overlay with its file, how it was placed, its layers (switch,
    /// colour), opacity, re-read and detach.
    pub(crate) fn gis_window(&mut self, ctx: &egui::Context) {
        if !self.gis.window_open {
            self.gis.shown = false;
            return;
        }
        if !std::mem::replace(&mut self.gis.shown, true) {
            // Opened now: the files are compared with their hashes again.
            self.gis.status.clear();
        }
        let mut open = true;
        let (mut import, mut detach, mut reread, mut zoom) = (false, None, None, None);
        let mut edits: Vec<Edit> = Vec::new();
        let project = project_crs(&self.doc);
        let refused = refusal(project.as_ref());
        let ids: Vec<(ElementId, String)> = self.doc.elements().iter().filter(|e| e.category == CATEGORY).map(|e| (e.id, e.name.clone())).collect();
        for (id, _) in &ids {
            if !self.gis.status.contains_key(id) {
                if let Some(e) = self.doc.find(*id) {
                    let st = check_file(e.data.get("path").and_then(|v| v.as_str()).unwrap_or_default(), e.data.get("hash").and_then(|v| v.as_str()).unwrap_or_default());
                    self.gis.status.insert(*id, st);
                }
            }
        }
        let message = self.gis.message.clone();
        let dlg = crate::ui_kit::Dialog::titled("GIS overlays").subtitle("GeoJSON, KML, GPX and CSV files drawn behind the model").width(640.0).show(
            ctx,
            |ui| {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "An overlay is a read-only background. The document keeps the file's path and hash, the features as read in WGS 84 and the same features in the project coordinate system. Change the project system and the overlay is placed again from WGS 84; nothing is moved. A CSV needs longitude and latitude columns in degrees.");
                match &refused {
                    Some(why) => {
                        crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, why.clone());
                    }
                    None => {
                        if let Some(p) = &project {
                            crate::ui_kit::note(ui, format!("Project system: {}", p.label()));
                        }
                    }
                }
                if let Some((is_err, text)) = &message {
                    ui.label(egui::RichText::new(text).color(if *is_err { crate::theme::danger() } else { crate::theme::success() }));
                }
                crate::ui_kit::heading(ui, "Overlays");
                if ids.is_empty() {
                    ui.label(egui::RichText::new("No GIS file imported.").weak());
                }
                for (id, name) in &ids {
                    let id = *id;
                    let Some(e) = self.doc.find(id) else { continue };
                    let (Some(look), path) = (look_of(e), e.data.get("path").and_then(|v| v.as_str()).unwrap_or_default().to_string()) else { continue };
                    let file = Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let kind = e.data.get("kind").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                    let status = self.gis.status.get(&id).cloned();
                    let geo = self.gis.geo(id);
                    egui::CollapsingHeader::new(egui::RichText::new(format!("{name}   {file}")).strong()).id_salt(("gis-overlay", id.0)).default_open(true).show(ui, |ui| {
                        ui.label(egui::RichText::new(&path).small().weak());
                        if let Some(s) = &status {
                            ui.label(egui::RichText::new(s.line()).color(if *s == FileStatus::Same { crate::theme::success() } else { crate::theme::warn() }));
                        }
                        if let Some(g) = geo {
                            ui.label(format!("{kind}: {} features ({} points, {} lines, {} polygons) placed of {}.", g.points + g.lines + g.areas, g.points, g.lines, g.areas, g.total));
                            if g.failed > 0 {
                                ui.label(egui::RichText::new(format!("{} feature(s) could not be placed in the project system and are left out.", g.failed)).color(crate::theme::warn()));
                            }
                            if !g.error.is_empty() {
                                ui.label(egui::RichText::new(format!("Not drawn: {}", g.error)).color(crate::theme::danger()));
                            } else {
                                ui.label(egui::RichText::new(&g.how).small());
                            }
                        }
                        let mut visible = look.visible;
                        let mut opacity = look.opacity;
                        egui::Grid::new(("gis-look", id.0)).num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                            ui.label("Visible");
                            crate::ui_kit::toggle(ui, &mut visible, "Shown behind the model");
                            ui.end_row();
                            ui.label("Opacity");
                            ui.horizontal(|ui| {
                                ui.add(egui::Slider::new(&mut opacity, 0.05..=1.0).show_value(false));
                                ui.label(format!("{:.0} %", opacity * 100.0));
                            });
                            ui.end_row();
                        });
                        if visible != look.visible {
                            edits.push(Edit::Visible(id, visible));
                        }
                        if opacity != look.opacity {
                            edits.push(Edit::Opacity(id, opacity));
                        }
                        crate::ui_kit::heading(ui, "Layers");
                        for (i, layer) in look.layers.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let mut on = layer.visible;
                                if ui.checkbox(&mut on, "").changed() {
                                    edits.push(Edit::LayerVisible(id, i, on));
                                }
                                let mut c = layer.colour;
                                if ui.color_edit_button_srgb(&mut c).changed() {
                                    edits.push(Edit::LayerColour(id, i, c));
                                }
                                let n = geo.and_then(|g| g.per_layer.get(i)).copied().unwrap_or(0);
                                ui.label(format!("{}  ({n})", layer.name));
                            });
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Zoom to it").clicked() {
                                zoom = Some(id);
                            }
                            if ui.button("Re-read").on_hover_text("Reads the file again; the colour and switch of each layer still in the file are kept").clicked() {
                                reread = Some(id);
                            }
                            if ui.button("Detach").on_hover_text("Removes the overlay from the document; the file is not touched").clicked() {
                                detach = Some(id);
                            }
                        });
                    });
                }
            },
            |ui| crate::ui_kit::Footer::new("Import a file...").enabled(refused.is_none(), "Set the project coordinate system first (CRS).").cancel(Some("Close")).show(ui),
        );
        if dlg.closed {
            open = false;
        }
        if let Some((_, out)) = &dlg.inner {
            if out.cancel {
                open = false;
            }
            if out.primary {
                import = true;
            }
        }
        let had_edits = !edits.is_empty();
        for edit in edits {
            match edit {
                Edit::Visible(id, v) => edit_look(&mut self.doc, id, |l| l.visible = v),
                Edit::Opacity(id, o) => edit_look(&mut self.doc, id, |l| l.opacity = o),
                Edit::LayerVisible(id, i, v) => edit_look(&mut self.doc, id, |l| {
                    if let Some(x) = l.layers.get_mut(i) {
                        x.visible = v;
                    }
                }),
                Edit::LayerColour(id, i, c) => edit_look(&mut self.doc, id, |l| {
                    if let Some(x) = l.layers.get_mut(i) {
                        x.colour = c;
                    }
                }),
            }
        }
        if had_edits {
            self.schedule_commit_now();
        }
        if import {
            if let Some(path) = rfd::FileDialog::new().add_filter("GIS files (GeoJSON, KML, GPX, CSV)", EXTENSIONS).add_filter("any file", &["*"]).set_title("Import a GIS file as an overlay").pick_file() {
                self.import_gis(&path);
            }
        }
        if let Some(id) = reread {
            match crate::gis_overlay::reread(&mut self.doc, id) {
                Ok(r) => {
                    self.gis.message = Some((false, format!("Read again. {}", r.line(&self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default()))));
                    self.schedule_commit_now();
                }
                Err(e) => self.gis.message = Some((true, e)),
            }
        }
        if let Some(id) = detach {
            self.gis.detach(&mut self.doc, id);
            self.schedule_commit_now();
        }
        if let Some(id) = zoom {
            if let Some(b) = self.gis.extents_of(id) {
                let (l, t, w, h) = self.ctl.viewport;
                self.view.fit(&b, l, t, w, h, 60.0);
            }
        }
        self.gis.window_open = open;
    }

    /// Imports a GIS file as an overlay and opens the window so that it can be seen.
    pub(crate) fn import_gis(&mut self, path: &Path) {
        match import(&mut self.doc, path) {
            Ok((id, report)) => {
                let name = self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
                let text = format!("Imported {}", report.line(&name));
                self.status = text.clone();
                self.gis.message = Some((false, text));
                self.gis.window_open = true;
                self.schedule_commit_now();
            }
            Err(e) => {
                self.status = e.clone();
                self.gis.message = Some((true, e));
                self.gis.window_open = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("afrobim-gis-overlay-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn doc_in(epsg: u32) -> Document {
        let mut doc = Document::default();
        afrobim_kernel::set_project_crs(&mut doc, &ProjectCrs::from_epsg(epsg).unwrap());
        doc
    }

    /// Kampala: about 0.3476 N, 32.5825 E.
    const LINE: &str = r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{"name":"Road","layer":"Roads"},"geometry":{"type":"LineString","coordinates":[[32.5825,0.3476],[32.5900,0.3500],[32.6000,0.3550]]}}]}"#;

    fn write(d: &Path, name: &str, text: &str) -> PathBuf {
        let p = d.join(name);
        std::fs::write(&p, text).unwrap();
        p
    }

    fn projected_line(doc: &Document, id: ElementId) -> Vec<[f64; 2]> {
        let def = def_of(doc.find(id).unwrap()).unwrap();
        match &def.projected[0].geom {
            PGeom::Line(v) => v.clone(),
            other => panic!("not a line: {other:?}"),
        }
    }

    #[test]
    fn a_geojson_line_near_kampala_lands_where_the_plan_puts_it_with_no_lat_lon_swap() {
        let d = dir("kampala");
        let p = write(&d, "road.geojson", LINE);
        let mut doc = doc_in(32636);
        let (id, report) = import(&mut doc, &p).unwrap();
        assert_eq!((report.features, report.lines, report.failed), (1, 1, 0));
        let got = projected_line(&doc, id);
        assert_eq!(got.len(), 3);
        // The expected positions are written from the literal latitude and longitude, with the
        // plan called as the kernel documents it: (lat, lon) in, (easting, northing) out.
        let crs = ProjectCrs::from_epsg(32636).unwrap();
        let wgs = ProjectCrs::geographic(4326).unwrap();
        let plan = afrobim_kernel::plan_transform(&wgs, &crs, None).unwrap().into_iter().next().unwrap();
        for (g, (lat, lon)) in got.iter().zip([(0.3476, 32.5825), (0.3500, 32.5900), (0.3550, 32.6000)]) {
            let want = plan.apply(lat, lon, 0.0).unwrap();
            assert!((g[0] / 1000.0 - want.first).abs() < 0.01 && (g[1] / 1000.0 - want.second).abs() < 0.01, "{g:?} vs {want:?}");
            // Kampala in UTM 36N is about 453 km east and 38 km north: a swap would put it 38 km east.
            assert!(g[0] / 1000.0 > 440_000.0 && g[0] / 1000.0 < 470_000.0 && g[1] / 1000.0 > 30_000.0 && g[1] / 1000.0 < 45_000.0, "{g:?}");
        }
        // And back: the project to WGS 84 plan of the app returns the file's longitude and latitude.
        let (to, _, _) = crate::basemap::plans_for(Some(&crs)).unwrap();
        let [lon, lat, _] = crate::gis_io::to_lonlat(&to, Pt::new(got[0][0], got[0][1])).unwrap();
        assert!((lon - 32.5825).abs() < 1e-7 && (lat - 0.3476).abs() < 1e-7, "{lon} {lat}");
        // The record is on the hidden underlay layer, with the file's hash.
        let e = doc.find(id).unwrap();
        assert_eq!(e.layer, crate::underlay::LAYER);
        assert!(!def_of(e).unwrap().hash.is_empty());
        assert_eq!(def_of(e).unwrap().layers[0].name, "Roads");
    }

    const HOLE_KML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2"><Document><Folder><name>Plots</name>
<Placemark><name>Plot with a pond</name><Polygon>
<outerBoundaryIs><LinearRing><coordinates>32.5800,0.3400,0 32.5900,0.3400,0 32.5900,0.3500,0 32.5800,0.3500,0 32.5800,0.3400,0</coordinates></LinearRing></outerBoundaryIs>
<innerBoundaryIs><LinearRing><coordinates>32.5840,0.3440,0 32.5840,0.3460,0 32.5860,0.3460,0 32.5860,0.3440,0 32.5840,0.3440,0</coordinates></LinearRing></innerBoundaryIs>
</Polygon></Placemark></Folder></Document></kml>"#;

    fn area(t: &[[f64; 2]; 3]) -> f64 {
        ((t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[2][0] - t[0][0]) * (t[1][1] - t[0][1])).abs() / 2.0
    }

    #[test]
    fn a_kml_polygon_with_a_hole_keeps_the_hole_and_the_fill_leaves_it_empty() {
        let d = dir("hole");
        let p = write(&d, "plots.kml", HOLE_KML);
        let mut doc = doc_in(32636);
        let (id, report) = import(&mut doc, &p).unwrap();
        assert_eq!((report.areas, report.layers, report.failed), (1, 1, 0));
        let def = def_of(doc.find(id).unwrap()).unwrap();
        assert_eq!(def.layers[0].name, "Plots");
        let PGeom::Area(rings) = &def.projected[0].geom else { panic!("not a polygon") };
        assert_eq!(rings.len(), 2, "outer ring and one hole");
        let (outer, hole) = (bbox_of(rings[0].iter().cloned()), bbox_of(rings[1].iter().cloned()));
        assert!(hole[0] > outer[0] && hole[2] < outer[2] && hole[1] > outer[1] && hole[3] < outer[3], "the hole is inside the outer ring");
        // The fill covers the outer ring less the hole.
        let shoelace = |r: &Vec<[f64; 2]>| (0..r.len()).map(|i| r[i][0] * r[(i + 1) % r.len()][1] - r[(i + 1) % r.len()][0] * r[i][1]).sum::<f64>().abs() / 2.0;
        let want = shoelace(&rings[0]) - shoelace(&rings[1]);
        let got: f64 = fill_triangles(rings).iter().map(area).sum();
        assert!((got - want).abs() / want < 1e-6, "fill {got} vs {want}");
        // The canvas cache holds the same triangles.
        let mut st = GisState::default();
        st.sync(&mut doc);
        assert_eq!(st.geo(id).unwrap().areas, 1);
    }

    #[test]
    fn the_fill_of_a_concave_polygon_has_its_area() {
        // An L shape, 3 by 3 with a 2 by 2 corner taken out: area 5.
        let l = vec![vec![[0.0, 0.0], [3.0, 0.0], [3.0, 1.0], [1.0, 1.0], [1.0, 3.0], [0.0, 3.0], [0.0, 0.0]]];
        let a: f64 = fill_triangles(&l).iter().map(area).sum();
        assert!((a - 5.0).abs() < 1e-9, "{a}");
        // Far from the origin, as UTM millimetres are.
        let far: Vec<Vec<[f64; 2]>> = l.iter().map(|r| r.iter().map(|p| [p[0] * 1000.0 + 4.5e8, p[1] * 1000.0 + 3.8e7]).collect()).collect();
        let a: f64 = fill_triangles(&far).iter().map(area).sum();
        assert!((a - 5.0e6).abs() < 1.0, "{a}");
    }

    #[test]
    fn a_local_grid_a_named_only_system_and_no_system_are_refused_with_a_reason_and_add_nothing() {
        let d = dir("refuse");
        let p = write(&d, "road.geojson", LINE);
        let mut doc = Document::default();
        let e = import(&mut doc, &p).unwrap_err();
        assert!(e.contains("no coordinate system") && e.contains("CRS"), "{e}");
        afrobim_kernel::set_project_crs(&mut doc, &ProjectCrs::local_grid("Site grid"));
        let e = import(&mut doc, &p).unwrap_err();
        assert!(e.contains("local site grid") && e.contains("Site grid") && e.starts_with("Not imported"), "{e}");
        afrobim_kernel::set_project_crs(&mut doc, &ProjectCrs::named_only(Some(4326), "WGS 84"));
        let e = import(&mut doc, &p).unwrap_err();
        assert!(e.contains("known by name only"), "{e}");
        assert!(doc.elements().iter().all(|e| e.category != CATEGORY), "nothing was added");
    }

    #[test]
    fn a_file_that_is_missing_empty_or_not_gis_is_reported_and_adds_nothing() {
        let d = dir("bad");
        let mut doc = doc_in(32636);
        assert!(import(&mut doc, &d.join("nothing.geojson")).unwrap_err().contains("does not exist"));
        let p = write(&d, "empty.geojson", r#"{"type":"FeatureCollection","features":[]}"#);
        assert!(import(&mut doc, &p).unwrap_err().contains("no features"));
        // Projected coordinates in a GeoJSON are rejected by the reader with a hint.
        let p = write(&d, "utm.geojson", r#"{"type":"Feature","properties":{},"geometry":{"type":"Point","coordinates":[453000.0,38000.0]}}"#);
        assert!(import(&mut doc, &p).is_err());
        assert!(doc.elements().iter().all(|e| e.category != CATEGORY));
    }

    #[test]
    fn a_feature_whose_position_cannot_be_moved_is_counted_not_hidden() {
        let crs = ProjectCrs::from_epsg(32636).unwrap();
        let mut def = OverlayDef {
            path: String::new(),
            hash: String::new(),
            size: 0,
            kind: "GeoJSON".into(),
            visible: true,
            opacity: 1.0,
            layers: vec![LayerDef { name: "L".into(), colour: [1, 2, 3], visible: true }],
            source: vec![
                SrcFeature { name: "ok".into(), layer: 0, geom: Geom::Point([32.58, 0.35, 0.0]) },
                SrcFeature { name: "bad point".into(), layer: 0, geom: Geom::Point([f64::NAN, 0.35, 0.0]) },
                SrcFeature { name: "bad vertex".into(), layer: 0, geom: Geom::Line(vec![[32.58, 0.35, 0.0], [32.59, f64::NAN, 0.0]]) },
                SrcFeature { name: "bad ring".into(), layer: 0, geom: Geom::Area(vec![vec![[32.58, 0.35, 0.0], [32.59, 0.35, 0.0], [f64::INFINITY, 0.36, 0.0], [32.58, 0.35, 0.0]]]) },
            ],
            projected: Vec::new(),
            proj_key: String::new(),
            failed: 0,
            error: String::new(),
            how: String::new(),
            rev: 0,
        };
        apply_projection(&mut def, Some(&crs));
        assert_eq!((def.projected.len(), def.failed), (1, 3));
        assert!(def.error.is_empty());
        assert_eq!(Report::of(&def).failed, 3);
        assert!(Report::of(&def).line("x").contains("3 feature(s) could not be placed"));
        // When none can be placed the overlay says so and keeps the source.
        def.source.remove(0);
        apply_projection(&mut def, Some(&crs));
        assert!(def.projected.is_empty() && def.error.contains("None of the 3 features"), "{}", def.error);
        assert_eq!(def.source.len(), 3);
    }

    #[test]
    fn a_gis_overlay_stays_out_of_ifc_cost_quantities_picking_and_gis_export() {
        let d = dir("backdrop");
        let p = write(&d, "road.geojson", LINE);
        let mut doc = doc_in(32636);
        let wall = doc.add("wall", "W1", Pt::new(0.0, 0.0), 1000.0, 200.0, "0", serde_json::json!({}));
        let before = crate::cost_glue::collect(&doc, &[]).lines.len();
        let (id, _) = import(&mut doc, &p).unwrap();
        assert!(crate::imagery::is_backdrop(CATEGORY));
        assert!(!crate::app::ifc_exportable(CATEGORY));
        assert!(crate::app::ifc_exportable("wall"));
        assert!(!crate::gis_io::exportable(doc.find(id).unwrap()));
        let ifc_set: Vec<ElementId> = doc.elements().iter().filter(|e| crate::app::ifc_exportable(&e.category) && e.category != afrobim_kernel::PROJECT_SETTINGS_CATEGORY).map(|e| e.id).collect();
        assert_eq!(ifc_set, vec![wall], "only the wall goes to the IFC export");
        let after = crate::cost_glue::collect(&doc, &[]);
        assert_eq!(after.lines.len(), before);
        assert!(after.lines.iter().all(|l| l.element != Some(id)));
        // A leader cannot attach to it, and it is on the hidden layer, which nothing can pick.
        assert_eq!(crate::anno_glue::pick_model(&doc, Pt::new(0.5, 0.5), 5.0), Some(wall));
        assert!(!doc.layer_visible(crate::underlay::LAYER));
    }

    #[test]
    fn saving_and_loading_keeps_the_overlay_and_draws_it_the_same() {
        let d = dir("save");
        let p = write(&d, "plots.kml", HOLE_KML);
        let mut doc = doc_in(32636);
        let (id, _) = import(&mut doc, &p).unwrap();
        edit_look(&mut doc, id, |l| {
            l.opacity = 0.5;
            l.layers[0].colour = [10, 20, 30];
            l.layers[0].visible = false;
        });
        let file = d.join("site.afrobim");
        doc.save(&file).unwrap();
        let mut back = Document::load(&file).unwrap();
        let (a, b) = (def_of(doc.find(id).unwrap()).unwrap(), def_of(back.find(id).unwrap()).unwrap());
        assert_eq!((&a.path, &a.hash, a.size, &a.kind, a.rev), (&b.path, &b.hash, b.size, &b.kind, b.rev));
        assert_eq!(a.source.len(), b.source.len());
        assert_eq!(a.proj_key, b.proj_key);
        // Floats survive the text round trip to the last digit that matters.
        let (PGeom::Area(ra), PGeom::Area(rb)) = (&a.projected[0].geom, &b.projected[0].geom) else { panic!() };
        for (x, y) in ra.iter().flatten().zip(rb.iter().flatten()) {
            assert!((x[0] - y[0]).abs() < 1e-3 && (x[1] - y[1]).abs() < 1e-3);
        }
        let look = look_of(back.find(id).unwrap()).unwrap();
        assert_eq!((look.opacity, look.layers[0].colour, look.layers[0].visible), (0.5, [10, 20, 30], false));
        // Opening it draws it without any change to the document.
        let mut st = GisState::default();
        st.sync(&mut back);
        assert!(!st.doc_changed, "a document in its own system is not rewritten on opening");
        assert_eq!(st.geo(id).unwrap().areas, 1);
        assert!(st.extents(&back).is_some());
    }

    #[test]
    fn a_change_of_project_system_places_the_overlay_again_from_wgs84() {
        let d = dir("crs");
        let p = write(&d, "road.geojson", LINE);
        let mut doc = doc_in(32636);
        let (id, _) = import(&mut doc, &p).unwrap();
        let src_before = def_of(doc.find(id).unwrap()).unwrap().source;
        let north = projected_line(&doc, id)[0][1] / 1000.0;
        // The same place in UTM 36S: the northing gains the false northing of the southern zone.
        afrobim_kernel::set_project_crs(&mut doc, &ProjectCrs::from_epsg(32736).unwrap());
        let mut st = GisState::default();
        st.sync(&mut doc);
        assert!(st.doc_changed);
        let got = projected_line(&doc, id);
        let crs = ProjectCrs::from_epsg(32736).unwrap();
        let wgs = ProjectCrs::geographic(4326).unwrap();
        let plan = afrobim_kernel::plan_transform(&wgs, &crs, None).unwrap().into_iter().next().unwrap();
        let want = plan.apply(0.3476, 32.5825, 0.0).unwrap();
        assert!((got[0][0] / 1000.0 - want.first).abs() < 0.01 && (got[0][1] / 1000.0 - want.second).abs() < 0.01);
        assert!((got[0][1] / 1000.0 - (north + 10_000_000.0)).abs() < 1.0, "northing {} from {north}", got[0][1] / 1000.0);
        assert_eq!(def_of(doc.find(id).unwrap()).unwrap().source, src_before, "the WGS 84 source is never touched");
        // Idempotent: nothing more to do.
        assert_eq!(reproject_all(&mut doc), 0);
        // An Arc 1960 project moves it by the datum shift (tens to hundreds of metres), not zero.
        afrobim_kernel::set_project_crs(&mut doc, &ProjectCrs::from_epsg(21036).unwrap());
        assert_eq!(reproject_all(&mut doc), 1);
        let arc = projected_line(&doc, id);
        let (to, _, how) = crate::basemap::plans_for(Some(&ProjectCrs::from_epsg(21036).unwrap())).unwrap();
        assert!(how.contains("EPSG:"), "{how}");
        let [lon, lat, _] = crate::gis_io::to_lonlat(&to, Pt::new(arc[0][0], arc[0][1])).unwrap();
        assert!((lon - 32.5825).abs() < 1e-7 && (lat - 0.3476).abs() < 1e-7, "{lon} {lat}");
        // A local grid places nothing, says why, and keeps the source; going back restores it.
        afrobim_kernel::set_project_crs(&mut doc, &ProjectCrs::local_grid("Site"));
        st.sync(&mut doc);
        let def = def_of(doc.find(id).unwrap()).unwrap();
        assert!(def.projected.is_empty() && def.error.contains("local site grid") && def.source == src_before);
        assert!(st.extents(&doc).is_none(), "nothing is drawn");
        afrobim_kernel::set_project_crs(&mut doc, &ProjectCrs::from_epsg(32636).unwrap());
        st.sync(&mut doc);
        assert!((projected_line(&doc, id)[0][1] / 1000.0 - north).abs() < 1e-6);
        assert!(st.extents(&doc).is_some());
    }

    #[test]
    fn transform_places_the_overlay_again_from_wgs84() {
        use afrobim_kernel::crs_plan::{plan_transform, DataExtent};
        let d = dir("transform");
        let p = write(&d, "road.geojson", LINE);
        let from = ProjectCrs::from_epsg(32636).unwrap();
        let to = ProjectCrs::from_epsg(21036).unwrap();
        let mut doc = doc_in(32636);
        let (id, _) = import(&mut doc, &p).unwrap();
        assert_eq!(crate::transform_doc::how(CATEGORY), crate::transform_doc::How::Reprojected);
        let e0 = projected_line(&doc, id)[0];
        let plan = plan_transform(&from, &to, Some(DataExtent::new((440_000.0, 30_000.0), (470_000.0, 45_000.0)))).unwrap().into_iter().next().unwrap();
        crate::transform_doc::apply_plan(&mut doc, None, &plan, &to, "Whole document", "t".into()).unwrap();
        let def = def_of(doc.find(id).unwrap()).unwrap();
        assert_eq!(def.proj_key, project_key(Some(&to)), "placed again at once, not on the next frame");
        // Still the same place on Earth: back to WGS 84 gives the file's position.
        let (back, _, _) = crate::basemap::plans_for(Some(&to)).unwrap();
        let g = projected_line(&doc, id)[0];
        let [lon, lat, _] = crate::gis_io::to_lonlat(&back, Pt::new(g[0], g[1])).unwrap();
        assert!((lon - 32.5825).abs() < 1e-7 && (lat - 0.3476).abs() < 1e-7);
        // And it is not the old millimetres moved: the datum shift makes it differ from them.
        assert!((g[0] - e0[0]).hypot(g[1] - e0[1]) > 1000.0, "the projected numbers changed with the system");
    }

    #[test]
    fn reading_again_brings_in_changes_and_keeps_the_layer_colours() {
        let d = dir("reread");
        let p = write(&d, "road.geojson", LINE);
        let mut doc = doc_in(32636);
        let (id, _) = import(&mut doc, &p).unwrap();
        edit_look(&mut doc, id, |l| l.layers[0].colour = [9, 9, 9]);
        assert_eq!(check_file(&p.to_string_lossy(), &def_of(doc.find(id).unwrap()).unwrap().hash), FileStatus::Same);
        let two = r#"{"type":"FeatureCollection","features":[
{"type":"Feature","properties":{"name":"Road","layer":"Roads"},"geometry":{"type":"LineString","coordinates":[[32.5825,0.3476],[32.5900,0.3500]]}},
{"type":"Feature","properties":{"name":"Bridge","layer":"Roads"},"geometry":{"type":"Point","coordinates":[32.5850,0.3480]}}]}"#;
        std::fs::write(&p, two).unwrap();
        assert_eq!(check_file(&p.to_string_lossy(), &def_of(doc.find(id).unwrap()).unwrap().hash), FileStatus::Changed);
        let r = reread(&mut doc, id).unwrap();
        assert_eq!((r.features, r.points, r.lines), (2, 1, 1));
        let def = def_of(doc.find(id).unwrap()).unwrap();
        assert_eq!(def.layers[0].colour, [9, 9, 9]);
        assert_eq!(check_file(&p.to_string_lossy(), &def.hash), FileStatus::Same);
        std::fs::remove_file(&p).unwrap();
        assert_eq!(check_file(&p.to_string_lossy(), &def.hash), FileStatus::Missing);
        assert!(reread(&mut doc, id).unwrap_err().contains("missing"));
        assert_eq!(def_of(doc.find(id).unwrap()).unwrap().source.len(), 2, "the features already read are kept");
    }

    #[test]
    fn kml_gpx_and_a_csv_of_longitude_and_latitude_all_come_in() {
        let d = dir("formats");
        let mut doc = doc_in(32636);
        let gpx = r#"<?xml version="1.0"?><gpx version="1.1" creator="t" xmlns="http://www.topografix.com/GPX/1/1"><wpt lat="0.3476" lon="32.5825"><name>A</name></wpt><trk><name>T</name><trkseg><trkpt lat="0.3476" lon="32.5825"/><trkpt lat="0.3500" lon="32.5900"/></trkseg></trk></gpx>"#;
        let (_, r) = import(&mut doc, &write(&d, "walk.gpx", gpx)).unwrap();
        assert_eq!((r.points, r.lines), (1, 1));
        let (_, r) = import(&mut doc, &write(&d, "pts.csv", "name,lon,lat\nA,32.5825,0.3476\nB,32.59,0.35\n")).unwrap();
        assert_eq!((r.points, r.layers), (2, 1));
        assert_eq!(doc.elements().iter().filter(|e| e.category == CATEGORY).count(), 2);
        // Two files with the same name get two names.
        let names: Vec<String> = doc.elements().iter().filter(|e| e.category == CATEGORY).map(|e| e.name.clone()).collect();
        assert_ne!(names[0], names[1]);
    }

    #[test]
    fn the_overlay_paints_inside_the_screen_even_zoomed_far_into_a_large_polygon() {
        let d = dir("paint");
        let p = write(&d, "plots.kml", HOLE_KML);
        let mut doc = doc_in(32636);
        let (id, _) = import(&mut doc, &p).unwrap();
        let mut st = GisState::default();
        st.sync(&mut doc);
        let geo = st.geo(id).unwrap();
        let look = look_of(doc.find(id).unwrap()).unwrap();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let b = st.extents(&doc).unwrap();
        let mid = Pt::new((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0);
        // 1000 px per millimetre: the plot is 1e10 px across.
        // Once on the middle (which is in the pond), once on a corner of the plot.
        let corner = match &geo.projected[0].geom {
            PGeom::Area(r) => Pt::new(r[0][0][0] + 5.0, r[0][0][1] + 5.0),
            _ => unreachable!(),
        };
        for (scale, at) in [(0.0005, mid), (1000.0, corner)] {
            let view = ViewTransform::centred_on(at, Pt::new(400.0, 300.0), scale);
            let ctx = egui::Context::default();
            let out = ctx.run(egui::RawInput::default(), |ctx| {
                let painter = ctx.layer_painter(egui::LayerId::background());
                paint_geo(geo, &look, &painter, &view, rect);
            });
            let mut n = 0;
            for s in out.shapes {
                match s.shape {
                    egui::Shape::Mesh(m) => {
                        for v in &m.vertices {
                            assert!(v.pos.x.abs() < 1e5 && v.pos.y.abs() < 1e5, "vertex at {:?} (scale {scale})", v.pos);
                        }
                        n += m.vertices.len();
                    }
                    egui::Shape::Path(path) => {
                        for q in &path.points {
                            assert!(q.x.abs() < 1e5 && q.y.abs() < 1e5, "{q:?}");
                        }
                        n += path.points.len();
                    }
                    _ => {}
                }
            }
            assert!(n > 0, "something is drawn at scale {scale}");
        }
    }

    #[test]
    fn a_layer_switched_off_or_an_overlay_hidden_is_not_drawn() {
        let d = dir("switch");
        let p = write(&d, "road.geojson", LINE);
        let mut doc = doc_in(32636);
        let (id, _) = import(&mut doc, &p).unwrap();
        let mut st = GisState::default();
        st.sync(&mut doc);
        let first = projected_line(&doc, id)[0];
        let view = ViewTransform::centred_on(Pt::new(first[0] + 1000.0, first[1] + 1000.0), Pt::new(400.0, 300.0), 0.1);
        let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let shapes = |doc: &Document, st: &GisState| {
            let ctx = egui::Context::default();
            ctx.run(egui::RawInput::default(), |ctx| st.paint(doc, &ctx.layer_painter(egui::LayerId::background()), &view, rect)).shapes.len()
        };
        assert!(shapes(&doc, &st) > 0);
        edit_look(&mut doc, id, |l| l.layers[0].visible = false);
        st.sync(&mut doc);
        assert_eq!(shapes(&doc, &st), 0);
        edit_look(&mut doc, id, |l| {
            l.layers[0].visible = true;
            l.visible = false;
        });
        st.sync(&mut doc);
        assert_eq!(shapes(&doc, &st), 0);
        assert!(st.extents(&doc).is_none());
    }

    #[test]
    fn the_command_opens_the_window_which_lists_an_imported_overlay_and_draws_it() {
        let (ctx, mut app) = crate::u4_dialogs_test::app_for("dark-glass", false);
        crate::u4_dialogs_test::frame(&ctx, &mut app, 2);
        afrobim_kernel::set_project_crs(&mut app.doc, &ProjectCrs::from_epsg(32636).unwrap());
        let d = dir("window");
        let p = write(&d, "plots.kml", HOLE_KML);
        app.run(&ctx, crate::commands::Action::GisImport);
        assert!(app.gis.window_open);
        app.import_gis(&p);
        assert!(app.status.contains("Imported"), "{}", app.status);
        let shapes = crate::u4_dialogs_test::frame(&ctx, &mut app, 3);
        assert!(shapes > 0 && crate::u4_dialogs_test::windows(&ctx) >= 1, "the window is open");
        // The command is on the command line by name and by alias.
        for name in ["GISIMPORT", "importgis", "GEOJSONIN", "KMLIN", "GPXIN"] {
            assert_eq!(crate::commands::resolve(name).unwrap().action, crate::commands::Action::GisImport, "{name}");
        }
        // Refused on a local grid: the message is kept for the window and nothing is added.
        afrobim_kernel::set_project_crs(&mut app.doc, &ProjectCrs::local_grid("Site"));
        let n = app.doc.elements().iter().filter(|e| e.category == CATEGORY).count();
        app.import_gis(&p);
        assert!(app.status.contains("local site grid"), "{}", app.status);
        assert_eq!(app.doc.elements().iter().filter(|e| e.category == CATEGORY).count(), n);
        crate::u4_dialogs_test::frame(&ctx, &mut app, 2);
    }

    #[test]
    fn undo_after_a_change_of_system_restores_the_old_system_and_the_old_placement() {
        let (ctx, mut app) = crate::u4_dialogs_test::app_for("dark-glass", false);
        let frames = |app: &mut crate::app::AfroBimApp| {
            crate::u4_dialogs_test::frame(&ctx, app, 4);
        };
        frames(&mut app);
        app.apply_project_crs(ProjectCrs::from_epsg(32636).unwrap());
        frames(&mut app);
        let d = dir("undo");
        let p = write(&d, "road.geojson", LINE);
        app.import_gis(&p);
        frames(&mut app);
        let id = app.doc.elements().iter().find(|e| e.category == CATEGORY).map(|e| e.id).unwrap();
        let key36n = project_key(Some(&ProjectCrs::from_epsg(32636).unwrap()));
        let north = projected_line(&app.doc, id)[0][1];
        // The CRS command: the system is stored, the next frames place the overlay again.
        app.apply_project_crs(ProjectCrs::from_epsg(32736).unwrap());
        frames(&mut app);
        assert!((projected_line(&app.doc, id)[0][1] - north - 1.0e10).abs() < 1000.0, "placed again in UTM 36S");
        app.undo();
        frames(&mut app);
        assert_eq!(afrobim_kernel::project_crs(&app.doc).and_then(|c| c.epsg), Some(32636), "the old system is back");
        assert_eq!(def_of(app.doc.find(id).unwrap()).unwrap().proj_key, key36n);
        assert!((projected_line(&app.doc, id)[0][1] - north).abs() < 1e-6, "and so is the old placement");
        // And it stays there: a further frame does not place it again.
        frames(&mut app);
        assert_eq!(afrobim_kernel::project_crs(&app.doc).and_then(|c| c.epsg), Some(32636));
        app.run(&ctx, crate::commands::Action::Redo);
        frames(&mut app);
        assert_eq!(afrobim_kernel::project_crs(&app.doc).and_then(|c| c.epsg), Some(32736));
        assert!((projected_line(&app.doc, id)[0][1] - north - 1.0e10).abs() < 1000.0);
    }

    #[test]
    fn the_window_compares_the_files_again_each_time_it_opens() {
        let (ctx, mut app) = crate::u4_dialogs_test::app_for("dark-glass", false);
        crate::u4_dialogs_test::frame(&ctx, &mut app, 2);
        app.apply_project_crs(ProjectCrs::from_epsg(32636).unwrap());
        let d = dir("reopen");
        let p = write(&d, "road.geojson", LINE);
        app.import_gis(&p);
        let id = app.doc.elements().iter().find(|e| e.category == CATEGORY).map(|e| e.id).unwrap();
        crate::u4_dialogs_test::frame(&ctx, &mut app, 3);
        assert_eq!(app.gis.status.get(&id), Some(&FileStatus::Same));
        app.gis.window_open = false;
        crate::u4_dialogs_test::frame(&ctx, &mut app, 2);
        std::fs::write(&p, r#"{"type":"Feature","properties":{},"geometry":{"type":"Point","coordinates":[32.58,0.35]}}"#).unwrap();
        app.run(&ctx, crate::commands::Action::GisImport);
        crate::u4_dialogs_test::frame(&ctx, &mut app, 3);
        assert_eq!(app.gis.status.get(&id), Some(&FileStatus::Changed));
    }

    #[test]
    fn the_report_reads_in_the_singular_and_the_plural() {
        let mut def = def_of_str();
        def.source.truncate(1);
        let r = Report::of(&def);
        assert!(r.line("f").starts_with("f: 1 feature (1 point, 0 lines, 0 polygons) in 1 layer."), "{}", r.line("f"));
    }

    fn def_of_str() -> OverlayDef {
        OverlayDef {
            path: String::new(),
            hash: String::new(),
            size: 0,
            kind: "GeoJSON".into(),
            visible: true,
            opacity: 1.0,
            layers: vec![LayerDef { name: "L".into(), colour: [1, 2, 3], visible: true }],
            source: vec![SrcFeature { name: "a".into(), layer: 0, geom: Geom::Point([32.58, 0.35, 0.0]) }, SrcFeature { name: "b".into(), layer: 0, geom: Geom::Point([32.59, 0.35, 0.0]) }],
            projected: Vec::new(),
            proj_key: String::new(),
            failed: 0,
            error: String::new(),
            how: String::new(),
            rev: 0,
        }
    }
}
