//! IFC reference models: other disciplines' IFC files read for display and clash checking only.
//!
//! A reference model is **not** embedded in the document. The document keeps, in one record
//! element of category `reference`, the path of each file, a hash of its bytes (FNV-1a 64 and the
//! length: it detects a changed file, it is not a security hash), the alignment the user entered
//! (offset and rotation) and the display choices. When a document is opened the files are read
//! again, on a worker thread so the window stays responsive. A file that is missing or whose hash
//! differs is reported and the model is shown as read (when it can be) with the warning kept.
//!
//! The meshes are built once by `afrobim_ifc::import::import_ifc_bytes` and then moved by the
//! entered alignment into world millimetres once per change, not once per frame. Drawing is a
//! software projection, so a model of more than [`DRAW_TRIANGLE_CAP`] triangles is drawn as the
//! boxes of its products instead of its triangles. The import reads the whole file into memory
//! (files of 4 GiB or more are refused by the importer).

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

use afrobim_coordination::Body;
use afrobim_draw::Mesh3;
use afrobim_ifc::import::{import_ifc_bytes, GeometryStatus, ImportOptions, ReferenceModel};
use afrobim_kernel::{Document, ElementId, Pt};
use egui::{Color32, Pos2, Rect, RichText, Shape};
use serde::{Deserialize, Serialize};

use crate::app::AfroBimApp;
use crate::view3d::Camera;

pub const CATEGORY: &str = "reference";
pub const LAYER: &str = "Reference";
const NAME: &str = "Reference models";
#[allow(non_snake_case)]
fn WARN() -> egui::Color32 {
    crate::theme::warn()
}
#[allow(non_snake_case)]
fn BAD() -> egui::Color32 {
    crate::theme::danger()
}

/// Above this many triangles in view, products are drawn as their boxes (the view is a software
/// projection and would stop being responsive).
pub const DRAW_TRIANGLE_CAP: usize = 60_000;

/// One reference model as the document stores it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefEntry {
    /// The name shown in lists and used as the clash category (`reference:<label>`).
    pub label: String,
    pub path: String,
    /// FNV-1a 64 of the file's bytes and its length, as `fnv64:<hex>:<bytes>`.
    pub hash: String,
    /// The position offset entered by the user, world mm, applied after the rotation.
    pub offset_mm: [f64; 3],
    /// The rotation entered by the user, degrees anticlockwise about the world Z axis through the
    /// file's own origin, applied before the offset.
    pub rotation_deg: f64,
    pub visible: bool,
    /// 0 is opaque, 1 invisible.
    pub transparency: f64,
    /// The file's `IfcMapConversion` was applied at import (projected coordinates).
    pub map_conversion: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RefSet {
    pub models: Vec<RefEntry>,
}

/// FNV-1a 64 of the bytes, with the length.
pub fn file_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    format!("fnv64:{h:016x}:{}", bytes.len())
}

/// The reference record of the document and its data.
pub fn load(doc: &Document) -> (Option<ElementId>, RefSet) {
    match doc.elements().iter().find(|e| e.category == CATEGORY) {
        Some(e) => (Some(e.id), serde_json::from_value(e.data.clone()).unwrap_or_default()),
        None => (None, RefSet::default()),
    }
}

/// Stores the set, adding the record element the first time. An empty set removes the record.
pub fn store(doc: &mut Document, set: &RefSet) {
    let value = serde_json::to_value(set).unwrap_or_default();
    if let Some(id) = doc.elements().iter().find(|e| e.category == CATEGORY).map(|e| e.id) {
        if set.models.is_empty() {
            doc.remove(id);
        } else if let Some(e) = doc.find_mut(id) {
            e.data = value;
        }
        return;
    }
    if set.models.is_empty() {
        return;
    }
    doc.ensure_layer(LAYER);
    let at = doc.next_free_origin(2000.0, 500.0);
    let name = doc.unique_name(NAME);
    doc.add(CATEGORY, &name, Pt::new(at.x, at.y), 2000.0, 500.0, LAYER, value);
}

/// The module of the reference record: a plain box on the sheet and a count in the derived values.
pub struct RefModule;

impl afrobim_kernel::Module for RefModule {
    fn category(&self) -> &str {
        CATEGORY
    }
    fn display_name(&self) -> &str {
        "Reference models"
    }
    fn generate(&self, element: &afrobim_kernel::Element) -> Result<afrobim_kernel::Generated, String> {
        use afrobim_draw::{Drawing, Extents, Primitive, Pt2};
        let set: RefSet = serde_json::from_value(element.data.clone()).map_err(|e| format!("not valid reference data: {e}"))?;
        let mut derived = std::collections::BTreeMap::new();
        derived.insert("Reference models".to_string(), set.models.len().to_string());
        let (w, h) = (2000.0, 500.0);
        let c = [Pt2::new(0.0, 0.0), Pt2::new(w, 0.0), Pt2::new(w, h), Pt2::new(0.0, h)];
        let primitives = (0..4).map(|i| Primitive::Line { from: c[i], to: c[(i + 1) % 4], width: 10.0, colour: None }).collect();
        Ok(afrobim_kernel::Generated { drawing: Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(w, h) } }, derived, ..Default::default() })
    }
}

/// Moves a file point by the entered alignment.
pub fn transform(p: [f64; 3], rotation_deg: f64, offset: [f64; 3]) -> [f64; 3] {
    let (s, c) = rotation_deg.to_radians().sin_cos();
    [c * p[0] - s * p[1] + offset[0], s * p[0] + c * p[1] + offset[1], p[2] + offset[2]]
}

/// What reading a file ended in.
pub enum ReadOutcome {
    Read { model: Box<ReferenceModel>, hash: String, bytes: usize },
    Failed(String),
}

/// Reads and imports a file (the work of the worker thread).
pub fn read_file(path: &Path, map_conversion: bool) -> ReadOutcome {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => return ReadOutcome::Failed(format!("{}: {e}", path.display())),
    };
    let hash = file_hash(&data);
    let bytes = data.len();
    match import_ifc_bytes(data, &ImportOptions { apply_map_conversion: map_conversion, ..ImportOptions::default() }) {
        Ok(m) => ReadOutcome::Read { model: Box::new(m), hash, bytes },
        Err(e) => ReadOutcome::Failed(format!("{}: {e}", path.display())),
    }
}

/// The import report as lines for a window or a file: schema, units, the map conversion note,
/// what was read and what was not (the "geometry not read" counts by type).
pub fn report_lines(m: &ReferenceModel, applied_map_conversion: bool) -> Vec<String> {
    let r = &m.report;
    let mut out = vec![
        format!("Schema {}, project '{}', {} instances in {} bytes.", r.schema, m.project_name, r.instances, r.file_bytes),
        format!("Length unit: {} ({} mm per unit). Coordinates are converted to millimetres.", if r.length_unit.is_empty() { "not stated" } else { &r.length_unit }, afrobim_draw_num(r.length_unit_mm)),
    ];
    out.push(match (&m.map_conversion, applied_map_conversion) {
        (Some(c), true) => format!("IfcMapConversion found (eastings {}, northings {}, height {}) and APPLIED: coordinates are in the projected system.", c.eastings, c.northings, c.orthogonal_height),
        (Some(c), false) => format!("IfcMapConversion found (eastings {}, northings {}, height {}) and NOT applied: coordinates are the file's own engineering coordinates. Enter the offset and rotation that align the model with this document, or re-import with the map conversion applied.", c.eastings, c.northings, c.orthogonal_height),
        (None, _) => "No IfcMapConversion in the file: its coordinates are its own and are not tied to a map system by the file. Enter an offset and rotation if it must line up with this document.".to_string(),
    });
    out.push(format!(
        "Products: {} ({} spatial). Geometry read for {}, partly for {}, bounding-box placeholder for {}, not read for {}, no representation for {}, placement unresolved for {}.",
        r.products, r.products_spatial, r.products_read, r.products_partial, r.products_placeholder, r.products_not_read, r.products_no_representation, r.products_placement_unresolved
    ));
    out.push(format!("{} triangles. Openings are not subtracted, so walls show without their holes.", r.triangles));
    if r.items_not_read.is_empty() {
        out.push("Geometry not read: none.".to_string());
    } else {
        out.push("Geometry not read, by type:".to_string());
        out.extend(r.items_not_read.iter().map(|(t, n)| format!("  {t}: {n}")));
    }
    if !r.placements_unresolved.is_empty() {
        out.push("Placements not resolved, by type:".to_string());
        out.extend(r.placements_unresolved.iter().map(|(t, n)| format!("  {t}: {n}")));
    }
    if !r.properties_not_read.is_empty() {
        out.push("Properties not read, by type:".to_string());
        out.extend(r.properties_not_read.iter().map(|(t, n)| format!("  {t}: {n}")));
    }
    out.push("Products by type (with geometry / all):".to_string());
    out.extend(r.by_type.iter().map(|(t, c)| format!("  {t}: {} / {}", c.with_geometry, c.products)));
    out.extend(r.warnings.iter().map(|w| format!("Warning: {w}")));
    out
}

fn afrobim_draw_num(v: f64) -> String {
    afrobim_sheets::trim_number(v, 6)
}

/// A model read into memory, with its transformed meshes.
pub struct LoadedRef {
    pub model: Arc<ReferenceModel>,
    /// Meshes by product index in world mm, after the entered alignment. Empty for products that
    /// do not take part (spatial elements, openings, no geometry).
    pub world: Vec<Vec<Mesh3>>,
    pub bounds: Vec<Option<([f64; 3], [f64; 3])>>,
    pub triangles: usize,
    key: ([f64; 3], f64),
    /// What the stored hash says about the file now.
    pub warning: Option<String>,
}

impl LoadedRef {
    fn build(model: Arc<ReferenceModel>, e: &RefEntry, warning: Option<String>) -> LoadedRef {
        let mut l = LoadedRef { model, world: Vec::new(), bounds: Vec::new(), triangles: 0, key: (e.offset_mm, e.rotation_deg), warning };
        l.place(e);
        l
    }

    /// Moves the meshes by the entered alignment (once per change).
    fn place(&mut self, e: &RefEntry) {
        self.key = (e.offset_mm, e.rotation_deg);
        self.triangles = 0;
        self.world = Vec::with_capacity(self.model.products.len());
        self.bounds = Vec::with_capacity(self.model.products.len());
        for p in &self.model.products {
            if !p.clashable() {
                self.world.push(Vec::new());
                self.bounds.push(None);
                continue;
            }
            let meshes: Vec<Mesh3> = p
                .meshes
                .iter()
                .map(|m| Mesh3 { name: m.name.clone(), vertices: m.vertices.iter().map(|v| transform(*v, e.rotation_deg, e.offset_mm)).collect(), triangles: m.triangles.clone(), colour: m.colour })
                .collect();
            self.triangles += meshes.iter().map(|m| m.triangles.len()).sum::<usize>();
            let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            for v in meshes.iter().flat_map(|m| m.vertices.iter()) {
                for i in 0..3 {
                    lo[i] = lo[i].min(v[i]);
                    hi[i] = hi[i].max(v[i]);
                }
            }
            self.bounds.push((lo[0] <= hi[0]).then_some((lo, hi)));
            self.world.push(meshes);
        }
    }
}

struct Job {
    label: String,
    path: String,
    rx: Receiver<ReadOutcome>,
    /// Adds the model to the document when it has been read (an import), or only loads it (an open).
    add: Option<(bool, Vec<f64>)>,
}

/// What the app keeps about reference models.
#[derive(Default)]
pub struct RefUi {
    pub list_open: bool,
    pub report_open: bool,
    pub set: RefSet,
    loaded_json: serde_json::Value,
    pub loaded: HashMap<String, LoadedRef>,
    /// Why a model could not be read, by label.
    pub failed: HashMap<String, String>,
    requested: BTreeSet<String>,
    jobs: Vec<Job>,
    /// The model whose import report is shown.
    pub report_for: Option<String>,
    pub apply_map_conversion: bool,
    /// Picked product: model label and product index.
    pub picked: Option<(String, usize)>,
    pub filter: String,
    /// Offset and rotation as typed, by label.
    typed: HashMap<String, [String; 4]>,
    pub message: String,
    /// Set by `--ifcref`: do not open the report window when the read finishes.
    pub quiet: bool,
}

fn unique_label(set: &RefSet, wanted: &str) -> String {
    let base = if wanted.is_empty() { "Reference".to_string() } else { wanted.to_string() };
    let mut label = base.clone();
    let mut n = 2;
    while set.models.iter().any(|m| m.label == label) {
        label = format!("{base} ({n})");
        n += 1;
    }
    label
}

impl RefUi {
    /// Reads the record when it changed, and starts reading the files of models not yet loaded.
    pub fn sync(&mut self, doc: &Document) {
        let current = doc.elements().iter().find(|e| e.category == CATEGORY).map(|e| e.data.clone()).unwrap_or(serde_json::Value::Null);
        if current != self.loaded_json {
            self.set = load(doc).1;
            self.loaded_json = current;
            let labels: BTreeSet<String> = self.set.models.iter().map(|m| m.label.clone()).collect();
            self.loaded.retain(|l, _| labels.contains(l));
            self.requested.retain(|l| labels.contains(l));
            self.failed.retain(|l, _| labels.contains(l));
            // The alignment may have changed.
            for e in &self.set.models {
                if let Some(l) = self.loaded.get_mut(&e.label) {
                    if l.key != (e.offset_mm, e.rotation_deg) {
                        l.place(e);
                    }
                }
            }
        }
        for e in self.set.models.clone() {
            if !self.loaded.contains_key(&e.label) && !self.requested.contains(&e.label) {
                self.requested.insert(e.label.clone());
                self.spawn(&e.label, &e.path, e.map_conversion, None);
            }
        }
    }

    fn spawn(&mut self, label: &str, path: &str, map_conversion: bool, add: Option<(bool, Vec<f64>)>) {
        let (tx, rx) = channel();
        let p = PathBuf::from(path);
        std::thread::spawn(move || {
            let _ = tx.send(read_file(&p, map_conversion));
        });
        self.jobs.push(Job { label: label.to_string(), path: path.to_string(), rx, add });
    }

    /// Starts importing a file as a new reference model. `offset_rotation` is x, y, z (mm) and
    /// rotation (degrees) as entered, or empty.
    pub fn start_import(&mut self, path: &Path, offset_rotation: &[f64]) {
        let label = unique_label(&self.set, &path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
        let label = if self.jobs.iter().any(|j| j.label == label) { format!("{label} (again)") } else { label };
        self.message = format!("Reading {} on a worker thread...", path.display());
        let map = self.apply_map_conversion;
        self.spawn(&label, &path.to_string_lossy(), map, Some((map, offset_rotation.to_vec())));
    }

    pub fn busy(&self) -> bool {
        !self.jobs.is_empty()
    }

    /// Collects finished reads. Returns true when the document's record must be stored.
    pub fn poll(&mut self) -> bool {
        let mut store = false;
        let mut i = 0;
        while i < self.jobs.len() {
            let got = self.jobs[i].rx.try_recv();
            match got {
                Err(std::sync::mpsc::TryRecvError::Empty) => i += 1,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let j = self.jobs.remove(i);
                    self.failed.insert(j.label.clone(), "the reading thread stopped without a result".into());
                    self.message = format!("{}: reading stopped without a result.", j.label);
                }
                Ok(outcome) => {
                    let j = self.jobs.remove(i);
                    match outcome {
                        ReadOutcome::Failed(why) => {
                            self.failed.insert(j.label.clone(), why.clone());
                            self.message = format!("Not read: {why}");
                        }
                        ReadOutcome::Read { model, hash, bytes } => {
                            let model = Arc::new(*model);
                            match j.add {
                                Some((map, off)) => {
                                    let e = RefEntry {
                                        label: j.label.clone(),
                                        path: j.path.clone(),
                                        hash,
                                        offset_mm: [off.first().copied().unwrap_or(0.0), off.get(1).copied().unwrap_or(0.0), off.get(2).copied().unwrap_or(0.0)],
                                        rotation_deg: off.get(3).copied().unwrap_or(0.0),
                                        visible: true,
                                        transparency: 0.0,
                                        map_conversion: map,
                                    };
                                    self.loaded.insert(e.label.clone(), LoadedRef::build(model.clone(), &e, None));
                                    self.requested.insert(e.label.clone());
                                    self.message = format!("Read {} ({} products, {} bytes).", e.label, model.report.products, bytes);
                                    self.report_for = Some(e.label.clone());
                                    self.report_open = !self.quiet;
                                    self.set.models.push(e);
                                    store = true;
                                }
                                None => {
                                    if let Some(e) = self.set.models.iter().find(|m| m.label == j.label).cloned() {
                                        let warning = (e.hash != hash).then(|| format!("The file has changed since it was added: stored {}, now {}. The model shown is the file as it is now; accept it to store the new hash, or restore the old file.", e.hash, hash));
                                        if warning.is_some() {
                                            self.report_open = true;
                                        }
                                        self.loaded.insert(e.label.clone(), LoadedRef::build(model, &e, warning));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        store
    }

    /// The category names and entity names reference products offer to a clash test.
    pub fn selectable(&self) -> (Vec<String>, Vec<String>) {
        let mut cats = Vec::new();
        let mut layers: BTreeSet<String> = BTreeSet::new();
        for e in &self.set.models {
            if let Some(l) = self.loaded.get(&e.label) {
                cats.push(format!("reference:{}", e.label));
                for p in l.model.products.iter().filter(|p| p.clashable()) {
                    layers.insert(p.entity.clone());
                }
            }
        }
        (cats, layers.into_iter().collect())
    }

    /// One clash body per product with geometry of every model shown or not (a hidden model can
    /// still be tested). Ids are stable for a file: they come from the label and the product index.
    pub fn bodies(&self) -> Vec<Body> {
        let mut out = Vec::new();
        for e in &self.set.models {
            let Some(l) = self.loaded.get(&e.label) else { continue };
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for b in e.label.bytes() {
                h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01B3);
            }
            let base = (1u64 << 40) | ((h & 0xF_FFFF) << 24);
            for (i, p) in l.model.products.iter().enumerate() {
                if l.world[i].is_empty() {
                    continue;
                }
                let mark = if p.name.is_empty() { format!("{}#{}", p.entity, p.step_id) } else { p.name.clone() };
                out.push(Body::new(ElementId(base | i as u64), &mark, &format!("reference:{}", e.label), &p.entity, None, l.world[i].clone()));
            }
        }
        out
    }

    /// Draws the visible models into the 3D view's item list (view depth, shapes).
    pub fn draw(&self, cam: &Camera, rect: Rect, dark: bool, items: &mut Vec<(f64, Vec<Shape>)>) {
        let visible: Vec<(&RefEntry, &LoadedRef)> = self.set.models.iter().filter(|e| e.visible).filter_map(|e| self.loaded.get(&e.label).map(|l| (e, l))).collect();
        let total: usize = visible.iter().map(|(_, l)| l.triangles).sum();
        let as_boxes = total > DRAW_TRIANGLE_CAP;
        let eye = cam.eye();
        let light = [-0.35, -0.75, 0.55f64];
        let ln = (light[0] * light[0] + light[1] * light[1] + light[2] * light[2]).sqrt();
        let base = if dark { [0x9A, 0xA1, 0xA8] } else { [0x7C, 0x83, 0x8A] };
        for (e, l) in visible {
            let alpha = ((1.0 - e.transparency.clamp(0.0, 1.0)) * 255.0) as u8;
            if alpha == 0 {
                continue;
            }
            for (pi, meshes) in l.world.iter().enumerate() {
                if meshes.is_empty() {
                    continue;
                }
                let picked = self.picked.as_ref().is_some_and(|(lab, i)| *lab == e.label && *i == pi);
                if as_boxes {
                    if let Some((lo, hi)) = l.bounds[pi] {
                        box_edges(cam, rect, lo, hi, Color32::from_rgba_unmultiplied(base[0], base[1], base[2], alpha.max(60)), items);
                    }
                    continue;
                }
                for m in meshes {
                    for t in &m.triangles {
                        let p: Vec<[f64; 3]> = t.iter().map(|i| m.vertices[*i as usize]).collect();
                        let n = cross3(sub3(p[1], p[0]), sub3(p[2], p[0]));
                        let nl = dot3(n, n).sqrt().max(1e-12);
                        let mut n = [n[0] / nl, n[1] / nl, n[2] / nl];
                        // Reference geometry is drawn lit from both sides: openings are not cut and
                        // surface models have no sense of outside.
                        if dot3(n, sub3(eye, p[0])) < 0.0 {
                            n = [-n[0], -n[1], -n[2]];
                        }
                        let proj: Option<Vec<(Pos2, f64)>> = p.iter().map(|q| cam.project(*q, rect)).collect();
                        let Some(proj) = proj else { continue };
                        let depth = proj.iter().map(|(_, d)| *d).sum::<f64>() / 3.0;
                        let shade = (0.45 + 0.55 * (dot3(n, light) / ln).max(0.0)) as f32;
                        let mut c = [(base[0] as f32 * shade) as u8, (base[1] as f32 * shade) as u8, (base[2] as f32 * shade) as u8];
                        if picked {
                            c = [0x4D, 0xA3, 0xFF];
                        }
                        let colour = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], alpha);
                        let mut mesh = egui::Mesh::default();
                        for (q, _) in &proj {
                            mesh.vertices.push(egui::epaint::Vertex { pos: *q, uv: egui::epaint::WHITE_UV, color: colour });
                        }
                        mesh.indices = vec![0, 1, 2];
                        items.push((depth, vec![Shape::mesh(mesh)]));
                    }
                }
            }
        }
    }

    /// The product under a screen point: the nearest visible one whose triangle contains it.
    pub fn pick(&self, cam: &Camera, rect: Rect, at: Pos2) -> Option<(String, usize)> {
        let mut best: Option<(f64, String, usize)> = None;
        for e in self.set.models.iter().filter(|e| e.visible && e.transparency < 1.0) {
            let Some(l) = self.loaded.get(&e.label) else { continue };
            for (pi, meshes) in l.world.iter().enumerate() {
                for m in meshes {
                    for t in &m.triangles {
                        let proj: Option<Vec<(Pos2, f64)>> = t.iter().map(|i| cam.project(m.vertices[*i as usize], rect)).collect();
                        let Some(proj) = proj else { continue };
                        if inside(at, proj[0].0, proj[1].0, proj[2].0) {
                            let d = (proj[0].1 + proj[1].1 + proj[2].1) / 3.0;
                            if best.as_ref().is_none_or(|b| d < b.0) {
                                best = Some((d, e.label.clone(), pi));
                            }
                        }
                    }
                }
            }
        }
        best.map(|(_, l, i)| (l, i))
    }

    pub fn world_bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        let mut acc: Option<([f64; 3], [f64; 3])> = None;
        for e in self.set.models.iter().filter(|e| e.visible) {
            if let Some(l) = self.loaded.get(&e.label) {
                for b in l.bounds.iter().flatten() {
                    acc = Some(match acc {
                        None => *b,
                        Some(a) => ([a.0[0].min(b.0[0]), a.0[1].min(b.0[1]), a.0[2].min(b.0[2])], [a.1[0].max(b.1[0]), a.1[1].max(b.1[1]), a.1[2].max(b.1[2])]),
                    });
                }
            }
        }
        acc
    }
}

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn inside(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let s = |p1: Pos2, p2: Pos2, p3: Pos2| (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y);
    let (d1, d2, d3) = (s(p, a, b), s(p, b, c), s(p, c, a));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

fn box_edges(cam: &Camera, rect: Rect, lo: [f64; 3], hi: [f64; 3], colour: Color32, items: &mut Vec<(f64, Vec<Shape>)>) {
    let c: Vec<[f64; 3]> = (0..8).map(|i| [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }]).collect();
    let mut shapes = Vec::new();
    let mut depth = 0.0;
    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
        if let (Some((p, d1)), Some((q, d2))) = (cam.project(c[a], rect), cam.project(c[b], rect)) {
            shapes.push(Shape::line_segment([p, q], egui::Stroke::new(1.0_f32, colour)));
            depth = (d1 + d2) / 2.0;
        }
    }
    if !shapes.is_empty() {
        items.push((depth, shapes));
    }
}

// ─── windows ────────────────────────────────────────────────────────────────────────────

impl AfroBimApp {
    /// Asks for IFC files and starts reading each on a worker thread.
    pub(crate) fn ifcref_pick(&mut self) {
        let Some(paths) = rfd::FileDialog::new().add_filter("IFC", &["ifc"]).pick_files() else { return };
        for p in paths {
            self.refs.start_import(&p, &[]);
        }
        self.refs.list_open = true;
    }

    /// Every frame: sync with the document, collect finished reads.
    pub(crate) fn reference_tick(&mut self, ctx: &egui::Context) {
        self.refs.sync(&self.doc);
        if self.refs.poll() {
            store(&mut self.doc, &self.refs.set);
            self.refs.loaded_json = self.doc.elements().iter().find(|e| e.category == CATEGORY).map(|e| e.data.clone()).unwrap_or_default();
            self.fitted_3d = false;
            self.schedule_commit_now();
        }
        if self.refs.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn refs_changed(&mut self) {
        store(&mut self.doc, &self.refs.set);
        self.refs.loaded_json = self.doc.elements().iter().find(|e| e.category == CATEGORY).map(|e| e.data.clone()).unwrap_or_default();
        let set = self.refs.set.clone();
        for e in &set.models {
            if let Some(l) = self.refs.loaded.get_mut(&e.label) {
                if l.key != (e.offset_mm, e.rotation_deg) {
                    l.place(e);
                }
            }
        }
        self.schedule_commit_now();
    }

    pub(crate) fn reference_windows(&mut self, ctx: &egui::Context) {
        if self.refs.list_open {
            self.reference_list_window(ctx);
        }
        if self.refs.report_open {
            self.reference_report_window(ctx);
        }
        self.reference_product_window(ctx);
    }

    fn reference_list_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut changed = false;
        let mut remove: Option<usize> = None;
        let mut import = false;
        let mut show_report: Option<String> = None;
        let mut accept: Option<String> = None;
        let mut zoom: Option<String> = None;
        crate::win::new(ctx, "Reference models").open(&mut open).default_width(900.0).default_height(520.0).show(ctx, |ui| {
            ui.label(RichText::new("Reference models are read-only IFC files. The document stores each file's path and a hash, not its contents; the file is read again when the document opens. A model in another coordinate frame needs the offset and rotation that line it up: enter them below (rotation about the file's own origin, then the offset).").small().weak());
            ui.label(RichText::new(format!("Limits: the importer reads the whole file into memory and refuses files of 4 GiB or more. Models above {DRAW_TRIANGLE_CAP} triangles in view are drawn as the boxes of their products.")).small().weak());
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.refs.apply_map_conversion, "apply the file's IfcMapConversion when it has one (default: use the file's own coordinates)");
                if ui.button("Import IFC files...").clicked() {
                    import = true;
                }
            });
            if self.refs.busy() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("Reading {} file(s) in the background...", self.refs.jobs.len()));
                });
            }
            if !self.refs.message.is_empty() {
                ui.label(RichText::new(&self.refs.message).small());
            }
            ui.separator();
            if self.refs.set.models.is_empty() {
                ui.label(RichText::new("No reference models.").weak());
            }
            let labels: Vec<String> = self.refs.set.models.iter().map(|m| m.label.clone()).collect();
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                for (i, label) in labels.iter().enumerate() {
                    let typed = self.refs.typed.entry(label.clone()).or_insert_with(|| {
                        let e = &self.refs.set.models[i];
                        [afrobim_sheets::trim_number(e.offset_mm[0], 6), afrobim_sheets::trim_number(e.offset_mm[1], 6), afrobim_sheets::trim_number(e.offset_mm[2], 6), afrobim_sheets::trim_number(e.rotation_deg, 6)]
                    });
                    ui.group(|ui| {
                        let loaded = self.refs.loaded.get(label);
                        let e = &mut self.refs.set.models[i];
                        ui.horizontal(|ui| {
                            changed |= ui.checkbox(&mut e.visible, RichText::new(&e.label).strong()).changed();
                            ui.label(RichText::new(&e.path).small().weak());
                            if ui.small_button("Report").clicked() {
                                show_report = Some(label.clone());
                            }
                            if ui.small_button("Show").on_hover_text("Frame this model in the 3D site view").clicked() {
                                zoom = Some(label.clone());
                            }
                            if ui.small_button("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                        match (loaded, self.refs.failed.get(label)) {
                            (Some(l), _) => {
                                ui.label(RichText::new(format!("{} products, {} with geometry, {} triangles{}.", l.model.report.products, l.model.clashable().len(), l.triangles, if l.triangles > DRAW_TRIANGLE_CAP { " (drawn as boxes)" } else { "" })).small());
                                if let Some(w) = &l.warning {
                                    ui.label(RichText::new(w).small().color(WARN()));
                                    if ui.small_button("Accept the file as it is now").clicked() {
                                        accept = Some(label.clone());
                                    }
                                }
                            }
                            (None, Some(why)) => {
                                ui.label(RichText::new(format!("File not read: {why}. The stored path and hash are kept; restore the file and reopen the document, or remove the model.")).small().color(BAD()));
                            }
                            (None, None) => {
                                ui.label(RichText::new("Reading...").small().weak());
                            }
                        }
                        ui.horizontal(|ui| {
                            ui.label("Transparency");
                            changed |= ui.add(egui::Slider::new(&mut e.transparency, 0.0..=1.0)).changed();
                        });
                        ui.horizontal(|ui| {
                            for (n, name) in ["Offset X (mm)", "Y (mm)", "Z (mm)", "Rotation (deg)"].iter().enumerate() {
                                ui.label(*name);
                                if ui.add(egui::TextEdit::singleline(&mut typed[n]).desired_width(70.0)).changed() {
                                    if let Ok(v) = typed[n].trim().replace(',', ".").parse::<f64>() {
                                        if v.is_finite() {
                                            if n < 3 {
                                                e.offset_mm[n] = v;
                                            } else {
                                                e.rotation_deg = v;
                                            }
                                            changed = true;
                                        }
                                    }
                                }
                            }
                        });
                    });
                }
                if !self.refs.set.models.is_empty() {
                    ui.separator();
                    ui.label(RichText::new("Products").strong());
                    ui.add(egui::TextEdit::singleline(&mut self.refs.filter).hint_text("filter by name or type").desired_width(240.0));
                    let f = self.refs.filter.trim().to_lowercase();
                    let mut shown = 0;
                    'outer: for e in &self.refs.set.models {
                        let Some(l) = self.refs.loaded.get(&e.label) else { continue };
                        for (pi, p) in l.model.products.iter().enumerate() {
                            if !p.clashable() {
                                continue;
                            }
                            if !f.is_empty() && !p.name.to_lowercase().contains(&f) && !p.entity.to_lowercase().contains(&f) {
                                continue;
                            }
                            shown += 1;
                            if shown > 200 {
                                ui.label(RichText::new("More than 200 match: narrow the filter.").small().weak());
                                break 'outer;
                            }
                            let on = self.refs.picked.as_ref().is_some_and(|(lab, i)| *lab == e.label && *i == pi);
                            if ui.selectable_label(on, format!("{}  {}  ({})", p.entity, if p.name.is_empty() { "unnamed" } else { &p.name }, e.label)).clicked() {
                                self.refs.picked = Some((e.label.clone(), pi));
                            }
                        }
                    }
                }
            });
        });
        self.refs.list_open = open;
        if import {
            self.ifcref_pick();
        }
        if let Some(l) = show_report {
            self.refs.report_for = Some(l);
            self.refs.report_open = true;
        }
        if let Some(l) = accept {
            if let Ok(bytes) = std::fs::read(self.refs.set.models.iter().find(|m| m.label == l).map(|m| m.path.clone()).unwrap_or_default()) {
                let h = file_hash(&bytes);
                if let Some(e) = self.refs.set.models.iter_mut().find(|m| m.label == l) {
                    e.hash = h;
                }
                if let Some(x) = self.refs.loaded.get_mut(&l) {
                    x.warning = None;
                }
                changed = true;
            }
        }
        if let Some(l) = zoom {
            if let Some(x) = self.refs.loaded.get(&l) {
                if let Some(b) = x.bounds.iter().flatten().fold(None::<([f64; 3], [f64; 3])>, |acc, b| Some(acc.map_or(*b, |a| ([a.0[0].min(b.0[0]), a.0[1].min(b.0[1]), a.0[2].min(b.0[2])], [a.1[0].max(b.1[0]), a.1[1].max(b.1[1]), a.1[2].max(b.1[2])])))) {
                    let c = [(b.0[0] + b.1[0]) / 2.0, (b.0[1] + b.1[1]) / 2.0, (b.0[2] + b.1[2]) / 2.0];
                    let d = sub3(b.1, b.0);
                    self.camera.yaw = (-60.0f64).to_radians();
                    self.camera.pitch = 30.0f64.to_radians();
                    self.camera.fit_sphere(c, dot3(d, d).sqrt() / 2.0);
                    self.view_3d = true;
                    self.fitted_3d = true;
                }
            }
        }
        if let Some(i) = remove {
            let m = self.refs.set.models.remove(i);
            self.refs.loaded.remove(&m.label);
            self.refs.typed.remove(&m.label);
            changed = true;
        }
        if changed {
            self.refs_changed();
        }
    }

    fn reference_report_window(&mut self, ctx: &egui::Context) {
        let Some(label) = self.refs.report_for.clone() else {
            self.refs.report_open = false;
            return;
        };
        let mut open = true;
        let lines = self.refs.loaded.get(&label).map(|l| report_lines(&l.model, self.refs.set.models.iter().find(|m| m.label == label).is_some_and(|m| m.map_conversion))).unwrap_or_else(|| vec!["This model is not read.".to_string()]);
        let warning = self.refs.loaded.get(&label).and_then(|l| l.warning.clone());
        crate::win::new(ctx, format!("Import report: {label}")).open(&mut open).default_width(640.0).default_height(420.0).show(ctx, |ui| {
            if let Some(w) = warning {
                ui.label(RichText::new(w).color(WARN()));
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for l in &lines {
                    ui.label(RichText::new(l).monospace().small());
                }
            });
            if ui.button("Copy").clicked() {
                ctx.copy_text(lines.join("\n"));
            }
        });
        self.refs.report_open = open;
    }

    fn reference_product_window(&mut self, ctx: &egui::Context) {
        let Some((label, pi)) = self.refs.picked.clone() else { return };
        let Some(l) = self.refs.loaded.get(&label) else { return };
        let Some(p) = l.model.products.get(pi) else { return };
        let mut open = true;
        let tree_path = {
            // The container chain, as names.
            let mut names = Vec::new();
            let mut at = p.container;
            let mut guard = 0;
            while let Some(n) = at {
                let Some(node) = l.model.tree.get(n) else { break };
                names.push(format!("{} {}", node.entity, node.name));
                at = l.model.tree.iter().position(|t| t.children.contains(&n));
                guard += 1;
                if guard > 20 {
                    break;
                }
            }
            names.reverse();
            names.join(" > ")
        };
        crate::win::new(ctx, "Reference product").open(&mut open).default_width(420.0).default_height(420.0).show(ctx, |ui| {
            ui.label(RichText::new("Read-only: the model is a reference.").small().weak());
            egui::Grid::new("ref-product").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                for (k, v) in [
                    ("Model", label.clone()),
                    ("Name", p.name.clone()),
                    ("Type", format!("{}{}", p.entity, p.predefined_type.as_ref().map(|t| format!(" ({t})")).unwrap_or_default())),
                    ("Object type", p.object_type.clone().unwrap_or_default()),
                    ("GlobalId", p.global_id.clone()),
                    ("Contained in", tree_path.clone()),
                    ("Geometry", format!("{}{}", geometry_text(p.geometry), if p.unread_items.is_empty() { String::new() } else { format!("; not read: {}", p.unread_items.join(", ")) })),
                ] {
                    ui.label(RichText::new(k).weak());
                    ui.label(v);
                    ui.end_row();
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let mut sets: Vec<&str> = p.properties.iter().map(|x| x.set.as_str()).collect();
                sets.dedup();
                let mut seen = BTreeSet::new();
                if p.properties.is_empty() {
                    ui.label(RichText::new("No property sets read for this product.").weak());
                }
                for s in sets {
                    if !seen.insert(s) {
                        continue;
                    }
                    ui.label(RichText::new(if s.is_empty() { "(no set name)" } else { s }).strong());
                    egui::Grid::new(("ref-set", s)).num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
                        for x in p.properties.iter().filter(|x| x.set == s) {
                            ui.label(&x.name);
                            ui.label(format!("{} ({})", x.value, x.value_type));
                            ui.end_row();
                        }
                    });
                }
            });
        });
        if !open {
            self.refs.picked = None;
        }
    }
}

fn geometry_text(g: GeometryStatus) -> &'static str {
    match g {
        GeometryStatus::Read => "read",
        GeometryStatus::Partial => "partly read",
        GeometryStatus::BoundingBoxPlaceholder => "bounding-box placeholder",
        GeometryStatus::NotRead => "not read",
        GeometryStatus::NoRepresentation => "no representation",
        GeometryStatus::PlacementUnresolved => "placement not resolved",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheets_glue::test_support::{demo_doc, registry, scratch};
    use afrobim_coordination::{ClashTest, Coordination, Scene, Selection};
    use afrobim_kernel::Generated;

    fn export_demo() -> (Document, std::path::PathBuf, usize) {
        let doc = demo_doc();
        let reg = registry();
        let generated: HashMap<ElementId, Generated> = doc.elements().iter().filter_map(|e| reg.generate(e).ok().map(|g| (e.id, g))).collect();
        let inputs: Vec<afrobim_ifc::IfcElementInput> = doc.elements().iter().filter(|e| e.category != CATEGORY).map(|e| afrobim_ifc::IfcElementInput { element: e, generated: generated.get(&e.id) }).collect();
        let options = afrobim_ifc::IfcOptions { project_name: "Demo".into(), ..afrobim_ifc::IfcOptions::default() };
        let export = afrobim_ifc::export_ifc_model(&doc, &inputs, &options);
        let path = scratch("own-export.ifc");
        std::fs::write(&path, &export.text).unwrap();
        let n = inputs.len();
        drop(inputs);
        (doc, path, n)
    }

    #[test]
    fn a_reference_model_from_the_apps_own_ifc_export_comes_back_and_takes_part_in_a_clash_test() {
        let (doc, path, exported) = export_demo();
        let ReadOutcome::Read { model, hash, bytes } = read_file(&path, false) else { panic!("the export must import") };
        eprintln!("exported {exported} elements; products {} clashable {} triangles {} by_type {:?}", model.report.products, model.clashable().len(), model.report.triangles, model.report.by_type);
        assert!(hash.starts_with("fnv64:") && hash.ends_with(&format!(":{bytes}")));
        assert!(model.report.products >= exported);
        assert_eq!(model.clashable().len(), model.report.by_type.values().map(|t| t.with_geometry).sum::<usize>());
        assert!(!model.clashable().is_empty());
        let lines = report_lines(&model, false);
        assert!(lines.iter().any(|l| l.starts_with("Geometry not read")));
        assert!(lines.iter().any(|l| l.contains("IfcMapConversion")));
        let entry = RefEntry { label: "Demo export".into(), path: path.to_string_lossy().to_string(), hash: hash.clone(), offset_mm: [0.0, 0.0, 500.0], rotation_deg: 0.0, visible: true, transparency: 0.0, map_conversion: false };
        let mut doc = doc;
        store(&mut doc, &RefSet { models: vec![entry.clone()] });
        let json = doc.to_json();
        assert!(json.contains(&hash) && !json.contains("IFC4"), "the document holds the path and hash only");
        let reopened = Document::from_json(&json).unwrap();
        assert_eq!(load(&reopened).1.models, vec![entry.clone()]);
        let loaded = LoadedRef::build(Arc::new(*model), &entry, None);
        let mut ui = RefUi::default();
        ui.set.models.push(entry.clone());
        ui.loaded.insert(entry.label.clone(), loaded);
        let bodies = ui.bodies();
        assert_eq!(bodies.len(), ui.loaded[&entry.label].model.clashable().len());
        assert!(bodies.iter().all(|b| b.category == "reference:Demo export"));
        let (cats, layers) = ui.selectable();
        assert_eq!(cats, vec!["reference:Demo export".to_string()]);
        assert!(!layers.is_empty() && layers.iter().all(|l| l.starts_with("IFC")));
        let reg = registry();
        let gen: HashMap<ElementId, Vec<Mesh3>> = doc.elements().iter().filter_map(|e| reg.generate(e).ok().map(|g| (e.id, g.solids))).collect();
        let get = |id: ElementId| gen.get(&id).map(|v| v.as_slice()).filter(|v| !v.is_empty());
        let none = |_: ElementId| None;
        let mut all = crate::coordination_glue::bodies(&crate::view3d::WorldCtx { doc: &doc, solids_of: &get, corridor_origin: &none }, crate::view3d::site_mode_of(&doc));
        let own = all.len();
        all.extend(bodies);
        assert!(all.len() > own, "the reference bodies join the scene");
        let mut co = Coordination::default();
        let mut reference = Selection::default();
        reference.categories.push("reference:Demo export".into());
        // A clearance test with a tolerance chosen for this test only.
        co.tests.push(ClashTest::clearance("Reference against model", reference, Selection::all(), 50.0));
        co.run(&Scene::new(all));
        assert!(co.results.clashes.iter().any(|c| c.category_a.starts_with("reference:") || c.category_b.starts_with("reference:")), "no clash involved a reference product ({} clashes)", co.results.clashes.len());
    }

    #[test]
    fn the_alignment_moves_the_meshes_by_the_entered_rotation_then_offset() {
        let p = transform([1000.0, 0.0, 5.0], 90.0, [10.0, 20.0, 30.0]);
        assert!((p[0] - 10.0).abs() < 1e-9 && (p[1] - 1020.0).abs() < 1e-9 && (p[2] - 35.0).abs() < 1e-9);
        assert_eq!(file_hash(b"abc"), file_hash(b"abc"));
        assert_ne!(file_hash(b"abc"), file_hash(b"abd"));
    }
}
