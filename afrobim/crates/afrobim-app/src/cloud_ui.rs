//! Point clouds in the application: attach (LAS, LAZ, E57 and text with a mapping preview, header
//! facts, the file's coordinate system against the project's, indexing as a background job),
//! properties and display, clip, section slices, surface from the cloud, comparison with a surface,
//! export of a subset, and the painting of a cloud in plan and in 3D.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use afrobim_kernel::{project_crs, ElementId, Pt};
use afrobim_pointcloud::*;
use egui::{Color32, RichText};
use std::result::Result;

use crate::app::AfroBimApp;
use crate::background::Job;
use crate::cloud::{self as cl, CloudData, Compare, Loaded};
use crate::sdb_ui::{fmt_opt, parse_f};
use crate::ui_kit::{self, Col, Dialog, Footer, Level, NumRule};

/// What a cloud job hands back.
pub enum CloudJob {
    Attach(Result<AttachOut, String>),
    Open(ElementId, Result<cl::OpenOut, String>),
    Surface(Result<SurfOut, String>),
    Compare(ElementId, Result<CmpOut, String>),
    Export(Result<ExportReport, String>),
}

pub struct AttachOut {
    reference: PointCloudReference,
    index: PointIndex,
    status: String,
    hash: String,
}

pub struct SurfOut {
    name: String,
    cloud: String,
    ground: GroundSurface,
    notes: Vec<String>,
    source: String,
}

pub struct CmpOut {
    cmp: Comparison,
    surface: String,
    compare: Compare,
}

#[derive(Default)]
pub struct CloudUi {
    pub loaded: HashMap<ElementId, Loaded>,
    pub failed: HashMap<ElementId, String>,
    pub loading: Option<ElementId>,
    pub render: RefCell<cl::CloudRender>,
    pub compare: HashMap<ElementId, Compare>,
    pub attach: Option<AttachForm>,
    pub props: Option<PropsForm>,
    pub clip: Option<ClipForm>,
    pub slice: Option<SliceForm>,
    pub surf: Option<SurfForm>,
    pub cmp: Option<CmpForm>,
    pub export: Option<ExportForm>,
    pub stage: Arc<Mutex<String>>,
    pub surf_out: Option<SurfOut>,
    pub cmp_out: Option<(ElementId, CmpOut)>,
    pub export_out: Option<String>,
    /// What the last drawn raster did, for the properties window: points, depth, complete, milliseconds.
    pub last_draw: RefCell<HashMap<ElementId, (usize, u32, bool, f64)>>,
}

fn cloud_combo(ui: &mut egui::Ui, id: &str, items: &[(ElementId, String)], sel: &mut Option<ElementId>) {
    crate::civil_dlg::pick_from(ui, id, items, sel);
}

impl AfroBimApp {
    pub(crate) fn cloud_cmd(&mut self, cv: crate::civil_cmds::Cv) {
        use crate::civil_cmds::Cv::*;
        if cv == CloudAttach {
            self.civil_ui.cloud.attach = Some(AttachForm::new(self.options.civil_demo));
            return;
        }
        let list = cl::clouds(&self.doc);
        if list.is_empty() {
            self.status = "There is no point cloud in the document: attach one with CLOUDATTACH.".into();
            return;
        }
        let sel = self.doc.primary().filter(|id| list.iter().any(|c| c.0 == *id)).or_else(|| list.first().map(|c| c.0));
        match cv {
            CloudProps => self.civil_ui.cloud.props = Some(PropsForm { cloud: sel, tab: 0, message: String::new() }),
            CloudClip => self.civil_ui.cloud.clip = Some(ClipForm::new(sel)),
            CloudSlice => self.civil_ui.cloud.slice = Some(SliceForm::new(sel)),
            CloudSurface => self.civil_ui.cloud.surf = Some(SurfForm::new(sel)),
            CloudCompare => self.civil_ui.cloud.cmp = Some(CmpForm::new(sel, self.options.civil_demo)),
            CloudExport => self.civil_ui.cloud.export = Some(ExportForm::new(sel)),
            _ => {}
        }
    }

    pub(crate) fn cloud_windows(&mut self, ctx: &egui::Context) {
        self.cloud_tick();
        self.cloud_attach_window(ctx);
        self.cloud_props_window(ctx);
        self.cloud_clip_window(ctx);
        self.cloud_slice_window(ctx);
        self.cloud_surf_window(ctx);
        self.cloud_cmp_window(ctx);
        self.cloud_export_window(ctx);
    }

    /// Once a frame: collects the finished job, and starts opening a cloud that has no index yet.
    fn cloud_tick(&mut self) {
        if let Some(polled) = crate::background::take(&mut self.bg.cloud, &self.doc) {
            match polled {
                crate::background::Poll::Done(job) => self.cloud_job_done(job),
                other => {
                    self.civil_ui.cloud.loading = None;
                    self.status = crate::background::discard_message("Point cloud job", &other);
                }
            }
        }
        if self.bg.cloud.is_some() {
            return;
        }
        let ids: Vec<ElementId> = cl::clouds(&self.doc).into_iter().map(|c| c.0).collect();
        // Forget what belongs to clouds that are gone.
        self.civil_ui.cloud.loaded.retain(|id, _| ids.contains(id));
        for id in ids {
            if self.civil_ui.cloud.loaded.contains_key(&id) || self.civil_ui.cloud.failed.contains_key(&id) {
                continue;
            }
            let Some(d) = cl::data_of(&self.doc, id) else { continue };
            if d.synthetic {
                let (_, buf) = cl::synthetic_data(crate::civil_demo::E0, crate::civil_demo::N0);
                match PointIndex::build(buf, &IndexOptions::default()) {
                    Ok(index) => {
                        let memory_bytes = index.memory_bytes();
                        self.civil_ui.cloud.loaded.insert(id, Loaded { index: Arc::new(index), status: "generated in memory (synthetic)".into(), hash: "synthetic".into(), memory_bytes });
                    }
                    Err(e) => {
                        self.civil_ui.cloud.failed.insert(id, e.to_string());
                    }
                }
                continue;
            }
            let path = PathBuf::from(&d.reference.source_path);
            if !path.exists() {
                self.civil_ui.cloud.failed.insert(id, format!("The file {} is missing. The cloud is kept in the document and shown as its footprint.", path.display()));
                continue;
            }
            let opts = d.reference.source_options();
            let hash = d.reference.source_hash.clone();
            let stage = self.civil_ui.cloud.stage.clone();
            self.civil_ui.cloud.loading = Some(id);
            let fp = crate::background::fingerprint(&self.doc);
            let cname = d.reference.name.clone();
            self.bg.cloud = Some(Job::spawn(&format!("Opening point cloud {cname}"), fp, move || {
                let set = |s: &str| {
                    if let Ok(mut g) = stage.lock() {
                        *g = s.to_string();
                    }
                };
                set("Checking the file against the stored hash");
                let r = (|| {
                    let reference = PointCloudReference { source_hash: hash, ..d.reference.clone() };
                    let _ = (&opts, &path);
                    set("Reading the index from the cache file beside the source, or building it");
                    let opened = reference.open(&IndexOptions::default()).map_err(|e| e.to_string())?;
                    let status = match &opened.cache {
                        CacheStatus::Hit => "index read from the cache file".to_string(),
                        CacheStatus::Built => "index built; cache file written".to_string(),
                        CacheStatus::Rebuilt { reason } => format!("index rebuilt ({reason})"),
                    };
                    Ok(cl::OpenOut { hash: opened.source_hash.clone(), index: opened.index, status })
                })();
                CloudJob::Open(id, r)
            }));
            self.status = format!("Opening the point cloud {cname} in the background.");
            return;
        }
    }

    fn cloud_job_done(&mut self, job: CloudJob) {
        match job {
            CloudJob::Open(id, r) => {
                self.civil_ui.cloud.loading = None;
                match r {
                    Ok(o) => {
                        let memory_bytes = o.index.memory_bytes();
                        self.status = format!("Opened the point cloud: {} points, {}.", o.index.len(), o.status);
                        self.civil_ui.cloud.loaded.insert(id, Loaded { index: Arc::new(o.index), status: o.status, hash: o.hash, memory_bytes });
                    }
                    Err(e) => {
                        self.status = format!("The point cloud cannot be opened: {e}");
                        self.civil_ui.cloud.failed.insert(id, e);
                    }
                }
            }
            CloudJob::Attach(r) => match r {
                Ok(o) => self.cloud_attach_finish(o),
                Err(e) => {
                    self.status = format!("The point cloud could not be attached: {e}");
                    if let Some(f) = &mut self.civil_ui.cloud.attach {
                        f.message = e;
                    }
                }
            },
            CloudJob::Surface(r) => match r {
                Ok(o) => {
                    self.status = format!("Ground surface fitted from {}: {} ground points, {} after thinning, {} in the surface.", o.cloud, o.ground.ground_points_in, o.ground.after_grid, o.ground.after_decimation);
                    self.civil_ui.cloud.surf_out = Some(o);
                }
                Err(e) => self.status = format!("The surface could not be made: {e}"),
            },
            CloudJob::Compare(id, r) => match r {
                Ok(o) => {
                    self.status = format!("Compared {} points with {}: mean {:+.3} m, RMS {:.3} m.", o.cmp.stats.compared, o.surface, o.cmp.stats.mean, o.cmp.stats.rms);
                    self.civil_ui.cloud.cmp_out = Some((id, o));
                }
                Err(e) => self.status = format!("The comparison failed: {e}"),
            },
            CloudJob::Export(r) => match r {
                Ok(rep) => {
                    self.status = format!("Exported {} point(s); the largest rounding error is {:.6} m.", rep.points_written, rep.max_rounding_error);
                    self.civil_ui.cloud.export_out = Some(format!("{} point(s) written, scale {:?}, offset {:?}, largest rounding error {:.6} m, LAS point format {}. {}", rep.points_written, rep.scale, rep.offset, rep.max_rounding_error, rep.point_format, rep.notes.join(" ")));
                }
                Err(e) => self.status = format!("The export failed: {e}"),
            },
        }
    }

    fn cloud_attach_finish(&mut self, o: AttachOut) {
        let Some(f) = self.civil_ui.cloud.attach.take() else { return };
        let mut reference = o.reference;
        reference.name = if f.name.trim().is_empty() { reference.name.clone() } else { f.name.trim().to_string() };
        reference.display.max_points_per_view = parse_f(&f.budget).map(|v| v.max(1.0) as usize).unwrap_or(250_000);
        if let Some(b) = reference.header.bounds {
            reference.display.color_mode = if reference.header.has_rgb { ColorMode::Rgb { scale: RgbScale::Auto } } else { ColorMode::Elevation { min: b.min[2], max: b.max[2], ramp: ColorRamp::rainbow() } };
        }
        let d = CloudData { extent: cl::header_extent(&reference.header), reference, clip: vec![], corridor: None, synthetic: false, moved: vec![], crs_note: f.findings.iter().map(|x| x.message.clone()).collect::<Vec<_>>().join(" ") };
        let before: Vec<ElementId> = self.doc.elements().iter().map(|e| e.id).collect();
        let id = cl::add(&mut self.doc, &d);
        let memory_bytes = o.index.memory_bytes();
        self.civil_ui.cloud.loaded.insert(id, Loaded { index: Arc::new(o.index), status: o.status.clone(), hash: o.hash, memory_bytes });
        self.civil_ui.cloud.failed.remove(&id);
        self.doc.select(&[id], afrobim_kernel::SelectMode::Replace);
        self.status = format!("Attached the point cloud {}: {} points; {}.", d.reference.name, d.reference.header.point_count, o.status);
        self.finish_import_transform(&f.xform, &before, &d.reference.name);
        self.schedule_commit_now();
        self.zoom_to_element(id);
    }

    // ─── painting ───────────────────────────────────────────────────────

    /// Draws every visible cloud in plan: a raster of the level-of-detail query for the view, kept
    /// until the view changes and then rebuilt when it has settled.
    pub(crate) fn cloud_paint_plan(&self, painter: &egui::Painter, rect: egui::Rect) {
        let clouds = cl::clouds(&self.doc);
        if clouds.is_empty() {
            return;
        }
        let ctx = painter.ctx().clone();
        let now = ctx.input(|i| i.time);
        let lo = self.view.to_world(Pt::new(rect.left() as f64, rect.bottom() as f64));
        let hi = self.view.to_world(Pt::new(rect.right() as f64, rect.top() as f64));
        let (vlo, vhi) = ([lo.x.min(hi.x) / 1000.0, lo.y.min(hi.y) / 1000.0], [lo.x.max(hi.x) / 1000.0, lo.y.max(hi.y) / 1000.0]);
        for (id, _) in clouds {
            let Some(e) = self.doc.find(id) else { continue };
            if crate::civil::hidden(e) || !self.doc.layer_visible(&e.layer) {
                continue;
            }
            let Some(d) = cl::data_of(&self.doc, id) else { continue };
            if !d.reference.display.visible {
                continue;
            }
            let cu = &self.civil_ui.cloud;
            let Some(loaded) = cu.loaded.get(&id) else {
                // No index yet: the footprint, so that it can be found and picked.
                if let Some(ext) = d.extent {
                    let (a, b) = (self.view.to_screen(Pt::new(ext[0][0] * 1000.0, ext[1][1] * 1000.0)), self.view.to_screen(Pt::new(ext[1][0] * 1000.0, ext[0][1] * 1000.0)));
                    let r = egui::Rect::from_two_pos(egui::pos2(a.x as f32, a.y as f32), egui::pos2(b.x as f32, b.y as f32));
                    painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(0x9A, 0x80, 0xC8)), egui::StrokeKind::Inside);
                    let why = cu.failed.get(&id).cloned().unwrap_or_else(|| "opening...".into());
                    painter.text(r.left_top() + egui::vec2(4.0, 4.0), egui::Align2::LEFT_TOP, format!("{}: {}", d.reference.name, why.chars().take(70).collect::<String>()), egui::FontId::proportional(10.5), Color32::from_rgb(0xB0, 0xA0, 0xD0));
                }
                continue;
            };
            let plans = cl::chain_plans(&d);
            let span = [(vhi[0] - vlo[0]).max(1.0), (vhi[1] - vlo[1]).max(1.0)];
            let q = |v: f64, s: f64| (v / (s * 0.02)).round() as i64;
            let compare = cu.compare.get(&id);
            let key = cl::hash_of(&[format!("{:?}", (q(vlo[0], span[0]), q(vlo[1], span[1]), q(vhi[0], span[0]), q(vhi[1], span[1]))), serde_json::to_string(&d.reference.display).unwrap_or_default(), serde_json::to_string(&d.reference.filter).unwrap_or_default(), format!("{:?}{:?}{}", d.clip, d.corridor, d.moved.len()), compare.map(|c| c.indices.len()).unwrap_or(0).to_string(), loaded.hash.clone(), format!("{:?}", rect.size())]);
            let mut render = cu.render.borrow_mut();
            let have = render.rasters.get(&id).map(|r| r.key);
            if have != Some(key) {
                let since = *render.changed_at.entry(id).or_insert(now);
                // Rebuild when the view has settled (a drag keeps moving the old raster instead).
                if have.is_none() || now - since >= 0.12 {
                    let size = [(rect.width() as usize).clamp(64, 2400), (rect.height() as usize).clamp(64, 1600)];
                    match cl::build_raster(&ctx, &d, &loaded.index, &plans, vlo, vhi, size, compare) {
                        Some(mut r) => {
                            r.key = key;
                            cu.last_draw.borrow_mut().insert(id, (r.points, r.depth, r.complete, r.ms));
                            render.rasters.insert(id, r);
                        }
                        None => {
                            render.rasters.remove(&id);
                        }
                    }
                    render.changed_at.remove(&id);
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(140));
                }
            } else {
                render.changed_at.remove(&id);
            }
            if let Some(r) = render.rasters.get(&id) {
                let a = self.view.to_screen(Pt::new(r.rect_mm[0], r.rect_mm[3]));
                let b = self.view.to_screen(Pt::new(r.rect_mm[2], r.rect_mm[1]));
                painter.image(r.tex.id(), egui::Rect::from_two_pos(egui::pos2(a.x as f32, a.y as f32), egui::pos2(b.x as f32, b.y as f32)), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            }
        }
    }

    /// The points of a cloud in the 3D view, as small squares in depth order with the other items.
    pub(crate) fn cloud_points_3d(&self, e: &afrobim_kernel::Element, frame: crate::view3d::Frame, rect: egui::Rect, items: &mut Vec<(f64, Vec<egui::Shape>)>) {
        let Some(d) = cl::data_of(&self.doc, e.id) else { return };
        if !d.reference.display.visible {
            return;
        }
        let cu = &self.civil_ui.cloud;
        let Some(loaded) = cu.loaded.get(&e.id) else { return };
        let plans = cl::chain_plans(&d);
        let compare = cu.compare.get(&e.id);
        let budget = d.reference.display.max_points_per_view.min(80_000);
        let key = cl::hash_of(&[serde_json::to_string(&d.reference.display).unwrap_or_default(), serde_json::to_string(&d.reference.filter).unwrap_or_default(), format!("{:?}{:?}{}", d.clip, d.corridor, d.moved.len()), compare.map(|c| c.indices.len()).unwrap_or(0).to_string(), loaded.hash.clone()]);
        let mut render = cu.render.borrow_mut();
        if render.three_d.get(&e.id).map(|x| x.0) != Some(key) {
            let (pos, col, origin) = cl::build_3d(&d, &loaded.index, &plans, budget, compare);
            render.three_d.insert(e.id, (key, pos, col, origin));
        }
        let Some((_, pos, col, origin)) = render.three_d.get(&e.id) else { return };
        let size = (d.reference.display.point_size.max(1.0)).min(6.0);
        let cam = &self.camera;
        let mut shapes: Vec<egui::Shape> = Vec::with_capacity(pos.len());
        let mut depth = 0.0;
        for (p, c) in pos.iter().zip(col.iter()) {
            let (x, y, z) = (origin[0] + p[0] as f64, origin[1] + p[1] as f64, origin[2] + p[2] as f64);
            let world = frame.at(x * 1000.0 - e.origin.x, y * 1000.0 - e.origin.y, z * 1000.0);
            if let Some((s, dz)) = cam.project(world, rect) {
                if rect.contains(s) {
                    shapes.push(egui::Shape::rect_filled(egui::Rect::from_center_size(s, egui::vec2(size, size)), 0.0, Color32::from_rgb(c[0], c[1], c[2])));
                    depth += dz;
                }
            }
        }
        if !shapes.is_empty() {
            // The whole cloud sorts at its mean depth: it is a reference layer, not a solid.
            items.push((depth / shapes.len() as f64 + 1.0e9, shapes));
        }
    }
}

// ─── attach ─────────────────────────────────────────────────────────────

pub struct AttachForm {
    path: Option<PathBuf>,
    header: Option<Result<HeaderInfo, String>>,
    detect: Option<Result<TextDetection, String>>,
    roles: Vec<usize>,
    skip: String,
    rgb_max: String,
    int_scale: String,
    int_offset: String,
    delim: usize,
    name: String,
    budget: String,
    findings: Vec<afrobim_kernel::CrsFinding>,
    xform: crate::transform_ui::ImportCrs,
    file_crs: Option<afrobim_kernel::ProjectCrs>,
    message: String,
    example: bool,
}

const ROLES: [(TextRole, &str); 12] = [
    (TextRole::X, "X (easting)"),
    (TextRole::Y, "Y (northing)"),
    (TextRole::Z, "Z (level)"),
    (TextRole::Intensity, "Intensity"),
    (TextRole::Red, "Red"),
    (TextRole::Green, "Green"),
    (TextRole::Blue, "Blue"),
    (TextRole::Class, "Class"),
    (TextRole::ReturnNumber, "Return number"),
    (TextRole::NumberOfReturns, "Number of returns"),
    (TextRole::GpsTime, "GPS time"),
    (TextRole::Ignore, "(ignore)"),
];
const DELIMS: [(Delimiter, &str); 5] = [(Delimiter::Auto, "Detect"), (Delimiter::Comma, "Comma"), (Delimiter::Semicolon, "Semicolon"), (Delimiter::Tab, "Tab"), (Delimiter::Whitespace, "White space")];

impl AttachForm {
    fn new(example: bool) -> AttachForm {
        AttachForm { path: None, header: None, detect: None, roles: vec![], skip: "0".into(), rgb_max: String::new(), int_scale: "1".into(), int_offset: "0".into(), delim: 0, name: String::new(), budget: "250000".into(), findings: vec![], xform: crate::transform_ui::ImportCrs::default(), file_crs: None, message: String::new(), example }
    }

    fn text_format(&self) -> Result<TextFormat, String> {
        let columns: Vec<TextRole> = self.roles.iter().map(|r| ROLES[(*r).min(11)].0).collect();
        let mut f = TextFormat::with_columns(columns);
        f.delimiter = DELIMS[self.delim].0;
        f.skip_lines = parse_f(&self.skip).filter(|v| *v >= 0.0).ok_or("The lines to skip must be a number.")? as usize;
        f.rgb_max = parse_f(&self.rgb_max).filter(|v| *v > 0.0).ok_or("The largest colour value in the file is an input: type it (255 or 65535 are the usual ones).")?;
        f.intensity_scale = parse_f(&self.int_scale).ok_or("The intensity scale is not a number.")?;
        f.intensity_offset = parse_f(&self.int_offset).ok_or("The intensity offset is not a number.")?;
        Ok(f)
    }

    fn is_text(&self) -> bool {
        self.detect.is_some()
    }

    fn options(&self) -> Result<SourceOptions, String> {
        Ok(SourceOptions { text: if self.is_text() { Some(self.text_format()?) } else { None }, e57_apply_pose: true })
    }

    fn load(&mut self, path: PathBuf, project: Option<&afrobim_kernel::ProjectCrs>) {
        self.name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        self.header = None;
        self.detect = None;
        self.findings.clear();
        self.message.clear();
        match detect_kind(&path) {
            Ok(SourceKind::Text) => {
                let d = detect_text_format(&path, 8).map_err(|e| e.to_string());
                if let Ok(det) = &d {
                    self.roles = det.format.columns.iter().map(|r| ROLES.iter().position(|x| x.0 == *r).unwrap_or(11)).collect();
                    self.skip = det.format.skip_lines.to_string();
                    self.int_scale = format!("{}", det.format.intensity_scale);
                    self.int_offset = format!("{}", det.format.intensity_offset);
                    // The colour maximum is an input the detection only guesses: show the guess, to be confirmed.
                    self.rgb_max = format!("{}", det.format.rgb_max);
                    self.delim = DELIMS.iter().position(|x| x.0 == det.delimiter).unwrap_or(0);
                }
                self.detect = Some(d);
            }
            Ok(_) => {
                let h = read_header(&path, &SourceOptions::default()).map_err(|e| e.to_string());
                if let Ok(h) = &h {
                    self.file_crs = cl::file_crs(h);
                    let extent = h.bounds.map(|b| afrobim_kernel::PlanExtent { min_easting: b.min[0], min_northing: b.min[1], max_easting: b.max[0], max_northing: b.max[1] });
                    self.findings = crate::crs_ui::dataset_check(project, &self.name, self.file_crs.as_ref(), extent);
                    self.xform = crate::transform_ui::ImportCrs::new(self.file_crs.as_ref());
                }
                self.header = Some(h);
            }
            Err(e) => self.header = Some(Err(e.to_string())),
        }
        self.path = Some(path);
    }
}

impl AfroBimApp {
    fn cloud_attach_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.cloud.attach.take() else { return };
        let project = project_crs(&self.doc);
        let busy = self.bg.cloud.is_some();
        let stage = self.civil_ui.cloud.stage.lock().map(|g| g.clone()).unwrap_or_default();
        let mut pick = false;
        let mut go = false;
        let ready_state = std::cell::Cell::new((false, ""));
        let out = Dialog::new("Attach a point cloud", crate::glyphs::by_name("CLOUDATTACH")).width(780.0).resizable(true).max_body_height(640.0).help("The points stay in their file; the document keeps a reference (path, SHA-256, header facts, display settings). Indexing runs in the background and writes a cache file (.afrocloud) beside the source, so the next opening is fast. LAS, LAZ, E57 and text files are read; text files need a column mapping, which you confirm against the preview.").show(
            ctx,
            |ui| {
                ui_kit::section(ui, "File", |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Choose a file...").clicked() {
                            pick = true;
                        }
                        ui.label(f.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(none)".into()));
                    });
                });
                if f.is_text() {
                    match f.detect.clone() {
                        Some(Err(e)) => {
                            ui_kit::callout(ui, Level::Error, e);
                        }
                        Some(Ok(det)) => {
                            ui_kit::section(ui, "Text file: columns", |ui| {
                                for n in &det.notes {
                                    ui_kit::note(ui, n.clone());
                                }
                                ui_kit::form(ui, "ca-t", |fm| {
                                    fm.row("Delimiter", |ui| {
                                        ui_kit::combo(ui, "ca-dl", &mut f.delim, &DELIMS.map(|d| d.1));
                                    });
                                    fm.row("Lines to skip", |ui| {
                                        crate::civil_dlg::num(ui, &mut f.skip, "");
                                    });
                                    fm.row_note("Largest colour value", "an input: 255 or 65535 are usual; it cannot be known from a few rows", |ui| {
                                        crate::civil_dlg::num(ui, &mut f.rgb_max, "");
                                    });
                                    fm.row_note("Intensity scale and offset", "stored = value x scale + offset", |ui| {
                                        ui.horizontal(|ui| {
                                            crate::civil_dlg::num(ui, &mut f.int_scale, "x");
                                            crate::civil_dlg::num(ui, &mut f.int_offset, "+");
                                        });
                                    });
                                });
                                let items: Vec<&str> = ROLES.iter().map(|r| r.1).collect();
                                egui::Grid::new("ca-map").spacing([6.0, 4.0]).striped(true).show(ui, |ui| {
                                    for (c, role) in f.roles.iter_mut().enumerate() {
                                        ui.label(RichText::new(format!("Column {}", c + 1)).small());
                                        ui_kit::combo(ui, &format!("ca-r{c}"), role, &items);
                                        for row in det.preview.iter().take(4) {
                                            ui.label(RichText::new(row.get(c).cloned().unwrap_or_default()).monospace().size(11.0));
                                        }
                                        ui.end_row();
                                    }
                                });
                                ui_kit::note(ui, "The first rows of the file are shown to the right of each column. Check the mapping by eye.");
                            });
                            ui_kit::callout(ui, Level::Info, "A text file has no header: its facts and the file's coordinate system are not known until it has been read. Say below what system its coordinates are in, if it differs from the project's.");
                        }
                        None => {}
                    }
                }
                if let Some(h) = &f.header {
                    match h {
                        Err(e) => {
                            ui_kit::callout(ui, Level::Error, e.clone());
                        }
                        Ok(h) => header_facts(ui, h),
                    }
                }
                if f.path.is_some() {
                    if !f.findings.is_empty() {
                        ui_kit::section(ui, "Coordinate system check against the project", |ui| {
                            crate::crs_ui::show_findings(ui, &f.findings);
                        });
                    }
                    if let Some(p) = project.as_ref() {
                        let differs = f.is_text() || crate::transform_ui::differs(f.file_crs.as_ref(), Some(p));
                        if differs {
                            let ext = f.header.as_ref().and_then(|h| h.as_ref().ok()).and_then(|h| h.bounds).map(|b| ((b.min[0], b.min[1]), (b.max[0], b.max[1])));
                            f.xform.show(ui, "ca-crs", p, ext);
                        }
                    }
                    ui_kit::form(ui, "ca-n", |fm| {
                        fm.row("Name", |ui| {
                            ui_kit::text_field(ui, &mut f.name, "");
                        });
                        fm.row_note("Points drawn per view", "the level-of-detail budget: a display setting", |ui| {
                            crate::civil_dlg::num_rule(ui, &mut f.budget, "points", NumRule::required().min(1000.0));
                        });
                    });
                }
                if busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(if stage.is_empty() { "Working in the background...".to_string() } else { stage.clone() });
                    });
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Error, f.message.clone());
                }
                let have = f.path.is_some() && (f.is_text() || f.header.as_ref().is_some_and(|h| h.is_ok()));
                ready_state.set(if !have { (false, "Choose a file that can be read") } else if busy { (false, "A point cloud job is running") } else if f.is_text() && f.text_format().is_err() { (false, "Complete the text mapping") } else if f.xform.cancelled_or_unready() { (false, f.xform.why_not()) } else { (true, "") });
                let _ = f.example;
            },
            |ui| {
                let (ok, why) = ready_state.get();
                let o = Footer::new("Attach and index").icon(crate::glyphs::by_name("CLOUDATTACH")).enabled(ok, why).show(ui);
                go = o.primary;
                o
            },
        );
        if pick {
            if let Some(path) = rfd::FileDialog::new().add_filter("Point clouds", &["las", "laz", "e57", "txt", "xyz", "pts", "csv", "asc"]).add_filter("All files", &["*"]).pick_file() {
                f.load(path, project.as_ref());
            }
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if go {
            match (f.path.clone(), f.options()) {
                (Some(path), Ok(opts)) => {
                    let stage = self.civil_ui.cloud.stage.clone();
                    let fp = crate::background::fingerprint(&self.doc);
                    self.bg.cloud = Some(Job::spawn(&format!("Attaching point cloud {}", f.name), fp, move || {
                        let r = cl::open_file(&path, &opts, &|s| {
                            if let Ok(mut g) = stage.lock() {
                                *g = s.to_string();
                            }
                        })
                        .map(|(o, reference)| AttachOut { reference, index: o.index, status: o.status, hash: o.hash });
                        CloudJob::Attach(r)
                    }));
                    self.status = "Attaching the point cloud: indexing in the background (Cancel is in the progress window).".into();
                }
                (_, Err(e)) => f.message = e,
                _ => {}
            }
        }
        self.civil_ui.cloud.attach = Some(f);
    }
}

fn header_facts(ui: &mut egui::Ui, h: &HeaderInfo) {
    ui_kit::section(ui, "Header facts", |ui| {
        let kind = match h.kind {
            SourceKind::Las => "LAS",
            SourceKind::Laz => "LAZ",
            SourceKind::E57 => "E57",
            SourceKind::Text => "text",
        };
        let rows = vec![
            vec!["Format".to_string(), format!("{kind} {}{}", h.version, h.point_format.map(|p| format!(", point format {p}")).unwrap_or_default())],
            vec!["Points".into(), format!("{}{}", h.point_count, if h.from_scan { " (counted)" } else { " (as the header states)" })],
            vec!["Bounds".into(), h.bounds.map(|b| format!("E {:.3} to {:.3}, N {:.3} to {:.3}, Z {:.3} to {:.3}{}", b.min[0], b.max[0], b.min[1], b.max[1], b.min[2], b.max[2], if h.bounds_exact { "" } else { " (the header's, not checked)" })).unwrap_or_else(|| "not stated".into())],
            vec!["Attributes".into(), format!("{}{}{}", if h.has_intensity { "intensity " } else { "" }, if h.has_rgb { "colour " } else { "" }, if h.has_gps_time { "GPS time" } else { "" })],
            vec!["Software".into(), format!("{} {}", h.generating_software, h.system_identifier)],
            vec!["Memory when indexed".into(), format!("about {:.0} MB (the index is held in memory)", h.point_count as f64 * h.bytes_per_point() as f64 / 1.0e6)],
            vec!["Coordinate system".into(), match (&h.crs.epsg, &h.crs.wkt) {
                (Some(e), _) => format!("EPSG:{e}{}", h.crs.epsg_source.as_ref().map(|s| format!(" ({s})")).unwrap_or_default()),
                (None, Some(w)) => format!("WKT: {}", w.chars().take(90).collect::<String>()),
                _ => "the file states none".into(),
            }],
        ];
        ui_kit::results_table(ui, "ca-h", &["Fact", "Value"], &[Col::Text, Col::Text], &rows);
        if let Some(v) = h.crs.vertical_epsg {
            ui_kit::note(ui, format!("Vertical system stated: EPSG:{v}. Heights are used as the file gives them."));
        }
    });
}

// ─── properties and display ─────────────────────────────────────────────

pub struct PropsForm {
    cloud: Option<ElementId>,
    tab: usize,
    message: String,
}

impl AfroBimApp {
    fn cloud_props_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.cloud.props.take() else { return };
        let list = cl::clouds(&self.doc);
        let Some(id) = f.cloud.filter(|c| list.iter().any(|l| l.0 == *c)) else { return };
        let Some(mut d) = cl::data_of(&self.doc, id) else { return };
        let before = d.clone();
        let loaded_info = self.civil_ui.cloud.loaded.get(&id).map(|l| (l.index.len(), l.status.clone(), l.memory_bytes, cl::class_counts(&l.index), l.index.bounds()));
        let failed = self.civil_ui.cloud.failed.get(&id).cloned();
        let draw = self.civil_ui.cloud.last_draw.borrow().get(&id).copied();
        let mut check = false;
        let mut retry = false;
        let table = d.reference.header.class_table;
        let out = Dialog::new("Point cloud properties and display", crate::glyphs::by_name("CLOUDPROPS")).width(700.0).resizable(true).max_body_height(620.0).help("Display settings apply at once and are saved in the document. The class filter hides classes from drawing, surface making, comparison and export; class names are those of the LAS specification table the file's point format uses.").show_body(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    cloud_combo(ui, "cp-sel", &list, &mut f.cloud);
                });
                if d.synthetic {
                    ui_kit::callout(ui, Level::Info, cl::SYNTHETIC_LABEL);
                }
                ui_kit::tabs(ui, &mut f.tab, &["Display", "Classes", "Facts"]);
                match f.tab {
                    0 => {
                        let disp = &mut d.reference.display;
                        ui_kit::form(ui, "cp-d", |fm| {
                            fm.row("Visible", |ui| {
                                ui.checkbox(&mut disp.visible, "");
                            });
                            fm.row("Colour by", |ui| {
                                let mut m = match disp.color_mode {
                                    ColorMode::Rgb { .. } => 0,
                                    ColorMode::Elevation { .. } => 1,
                                    ColorMode::Intensity { .. } => 2,
                                    ColorMode::Class { .. } => 3,
                                    ColorMode::Fixed(_) => 4,
                                };
                                let was = m;
                                ui_kit::combo(ui, "cp-m", &mut m, &["Colour of the points (RGB)", "Elevation", "Intensity", "Class", "One colour"]);
                                if m != was {
                                    let b = loaded_info.as_ref().map(|l| l.4).or(d.reference.header.bounds);
                                    disp.color_mode = match m {
                                        0 => ColorMode::Rgb { scale: RgbScale::Auto },
                                        1 => ColorMode::Elevation { min: b.map(|b| b.min[2]).unwrap_or(0.0), max: b.map(|b| b.max[2]).unwrap_or(1.0), ramp: ColorRamp::rainbow() },
                                        2 => ColorMode::Intensity { min: 0, max: 65535, ramp: ColorRamp::grey() },
                                        3 => ColorMode::Class { palette: ClassPalette::standard() },
                                        _ => ColorMode::Fixed([200, 200, 200]),
                                    };
                                }
                            });
                            match &mut disp.color_mode {
                                ColorMode::Elevation { min, max, .. } => {
                                    fm.row_note("Elevation range", "metres, from the data's own range until you change it", |ui| {
                                        ui.horizontal(|ui| {
                                            crate::civil_dlg::f64_edit(ui, min);
                                            crate::civil_dlg::f64_edit(ui, max);
                                        });
                                    });
                                }
                                ColorMode::Intensity { min, max, .. } => {
                                    fm.row("Intensity range", |ui| {
                                        ui.horizontal(|ui| {
                                            let (mut a, mut b) = (*min as f64, *max as f64);
                                            crate::civil_dlg::f64_edit(ui, &mut a);
                                            crate::civil_dlg::f64_edit(ui, &mut b);
                                            *min = a.clamp(0.0, 65535.0) as u16;
                                            *max = b.clamp(0.0, 65535.0) as u16;
                                        });
                                    });
                                }
                                ColorMode::Rgb { scale } => {
                                    fm.row_note("Colour values are", "Auto reads 8-bit when no value is above 255", |ui| {
                                        let mut s = match scale {
                                            RgbScale::Auto => 0,
                                            RgbScale::Bits8 => 1,
                                            RgbScale::Bits16 => 2,
                                        };
                                        ui_kit::combo(ui, "cp-s", &mut s, &["Auto", "8-bit", "16-bit"]);
                                        *scale = [RgbScale::Auto, RgbScale::Bits8, RgbScale::Bits16][s];
                                    });
                                }
                                _ => {}
                            }
                            fm.row("Point size", |ui| {
                                let mut s = disp.point_size as f64;
                                ui_kit::f64_field(ui, &mut s, "px", 1.0..=12.0, 0.1);
                                disp.point_size = s as f32;
                            });
                            fm.row_note("Points drawn per view", "the level-of-detail budget; fewer is faster", |ui| {
                                let mut n = disp.max_points_per_view as f64;
                                ui_kit::f64_field(ui, &mut n, "points", 1000.0..=5_000_000.0, 1000.0);
                                disp.max_points_per_view = n as usize;
                            });
                        });
                        if let Some((n, depth, complete, ms)) = draw {
                            ui_kit::note(ui, format!("The last view drew {n} points{}; level of detail depth {depth}; {ms:.0} ms.", if complete { " (all of them in view)" } else { "" }));
                        }
                    }
                    1 => match &loaded_info {
                        None => ui_kit::note(ui, "The cloud is not open yet."),
                        Some((_, _, _, counts, _)) => {
                            ui_kit::note(ui, "Tick the classes to show. With none ticked every class is shown.");
                            let shown: Vec<u8> = d.reference.filter.classes.clone().unwrap_or_default();
                            let mut new_shown = shown.clone();
                            for (code, n) in counts {
                                let mut on = shown.contains(code);
                                let label = format!("{}   ({n} points)", class_label(*code, table));
                                if ui.checkbox(&mut on, label).changed() {
                                    if on {
                                        new_shown.push(*code);
                                    } else {
                                        new_shown.retain(|c| c != code);
                                    }
                                }
                            }
                            d.reference.filter.classes = if new_shown.is_empty() { None } else { Some(new_shown) };
                            if ui.button("Show every class").clicked() {
                                d.reference.filter.classes = None;
                            }
                        }
                    },
                    _ => {
                        header_facts(ui, &d.reference.header);
                        ui_kit::form(ui, "cp-f", |fm| {
                            fm.row("Source", |ui| {
                                ui.label(&d.reference.source_path);
                            });
                            fm.row("SHA-256", |ui| {
                                ui.label(RichText::new(&d.reference.source_hash).monospace().size(10.0));
                            });
                            fm.row("Index", |ui| {
                                ui.label(match (&loaded_info, &failed) {
                                    (Some((n, s, m, _, _)), _) => format!("{n} points held, {:.0} MB; {s}", *m as f64 / 1.0e6),
                                    (None, Some(e)) => format!("not open: {e}"),
                                    _ => "opening".into(),
                                });
                            });
                            fm.row("Clip", |ui| {
                                ui.label(if d.clip.len() >= 3 { format!("polygon of {} vertices", d.clip.len()) } else if let Some(c) = &d.corridor { format!("corridor swath, {} m either side", c.half_width_m) } else { "none".into() });
                            });
                            fm.row("Moved by TRANSFORM", |ui| {
                                ui.label(if d.moved.is_empty() { "no: the points are at the coordinates of the file".to_string() } else { format!("{} transformation(s): the points are drawn through them; the file is not changed", d.moved.len()) });
                            });
                        });
                        if !d.crs_note.is_empty() {
                            ui_kit::note(ui, d.crs_note.clone());
                        }
                        ui.horizontal(|ui| {
                            if !d.synthetic && ui.button("Check the file against the stored hash").clicked() {
                                check = true;
                            }
                            if failed.is_some() && ui.button("Try opening again").clicked() {
                                retry = true;
                            }
                        });
                        if !f.message.is_empty() {
                            ui_kit::callout(ui, Level::Info, f.message.clone());
                        }
                    }
                }
            },
        );
        if d != before {
            cl::store(&mut self.doc, id, &d);
            self.schedule_commit_now();
        }
        if check {
            f.message = match d.reference.check_source() {
                Ok(SourceCheck::Unchanged) => "The file is unchanged since the reference was made.".into(),
                Ok(SourceCheck::Changed) => "The file CHANGED since the reference was made (its hash differs). Drawing and queries are refused until a new reference is made.".into(),
                Ok(SourceCheck::Missing) => "The file is MISSING.".into(),
                Err(e) => e.to_string(),
            };
        }
        if retry {
            self.civil_ui.cloud.failed.remove(&id);
        }
        if out.closed {
            return;
        }
        self.civil_ui.cloud.props = Some(f);
    }
}

// ─── clip ───────────────────────────────────────────────────────────────

pub struct ClipForm {
    cloud: Option<ElementId>,
    mode: usize,
    rows: Vec<(String, String)>,
    alignment: Option<ElementId>,
    half_width: String,
    step: String,
    message: String,
}

impl ClipForm {
    fn new(cloud: Option<ElementId>) -> ClipForm {
        ClipForm { cloud, mode: 0, rows: vec![(String::new(), String::new()); 3], alignment: None, half_width: String::new(), step: String::new(), message: String::new() }
    }
}

impl AfroBimApp {
    fn cloud_clip_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.cloud.clip.take() else { return };
        for (e, n) in std::mem::take(&mut self.civil_ui.queue) {
            let row = (format!("{e:.3}"), format!("{n:.3}"));
            match f.rows.iter().position(|r| r.0.trim().is_empty() && r.1.trim().is_empty()) {
                Some(i) => f.rows[i] = row,
                None => f.rows.push(row),
            }
        }
        let list = cl::clouds(&self.doc);
        let aligns = civil_alignments(&self.doc);
        let (mut pick, mut apply, mut clear) = (false, false, false);
        let out = Dialog::new("Clip a point cloud", crate::glyphs::by_name("CLOUDCLIP")).width(620.0).resizable(true).max_body_height(560.0).help("Clips to a polygon (typed or picked in the plan) or to a corridor swath along an alignment. The clip hides the points outside from drawing and keeps them out of surface making, comparison and export; the file is not changed.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "cc-h", |fm| {
                    fm.row("Cloud", |ui| cloud_combo(ui, "cc-c", &list, &mut f.cloud));
                    fm.row("Clip to", |ui| {
                        ui_kit::segmented(ui, &mut f.mode, &["A polygon", "A corridor swath"]);
                    });
                });
                if f.mode == 0 {
                    egui::Grid::new("cc-rows").num_columns(3).spacing([6.0, 4.0]).show(ui, |ui| {
                        for h in ["#", "Easting (m)", "Northing (m)"] {
                            ui_kit::th(ui, h);
                        }
                        ui.end_row();
                        for (i, r) in f.rows.iter_mut().enumerate() {
                            ui.label((i + 1).to_string());
                            ui.add(egui::TextEdit::singleline(&mut r.0).desired_width(110.0));
                            ui.add(egui::TextEdit::singleline(&mut r.1).desired_width(110.0));
                            ui.end_row();
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Add a row").clicked() {
                            f.rows.push(Default::default());
                        }
                        if ui.button("Pick in the plan").clicked() {
                            pick = true;
                        }
                    });
                } else {
                    ui_kit::form(ui, "cc-k", |fm| {
                        fm.row("Alignment", |ui| cloud_combo(ui, "cc-a", &aligns, &mut f.alignment));
                        fm.row_note("Half width either side", "an input", |ui| {
                            crate::civil_dlg::num_rule(ui, &mut f.half_width, "m", NumRule::required().above(0.0));
                        });
                        fm.row_note("Centreline step", "an input: the polyline is sampled this often", |ui| {
                            crate::civil_dlg::num_rule(ui, &mut f.step, "m", NumRule::required().above(0.0));
                        });
                    });
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new("Apply the clip").icon(crate::glyphs::by_name("CLOUDCLIP")).secondary(&["Remove the clip"]).show(ui);
                apply = o.primary;
                clear = o.secondary == Some(0);
                o
            },
        );
        if pick {
            self.civil_ui.pick = Some(crate::civil_ui::PickFor::Queue);
            self.civil_ui.pick_repeat = true;
            self.status = "Click the polygon corners in the plan.".into();
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            self.civil_ui.pick = None;
            return;
        }
        if let Some(id) = f.cloud {
            if let Some(mut d) = cl::data_of(&self.doc, id) {
                if clear {
                    d.clip.clear();
                    d.corridor = None;
                    cl::store(&mut self.doc, id, &d);
                    self.schedule_commit_now();
                    f.message = "The clip was removed.".into();
                }
                if apply {
                    if f.mode == 0 {
                        let pts: Vec<[f64; 2]> = f.rows.iter().filter_map(|r| Some([parse_f(&r.0)?, parse_f(&r.1)?])).collect();
                        if pts.len() < 3 {
                            f.message = "A polygon needs at least three corners.".into();
                        } else {
                            d.clip = pts;
                            d.corridor = None;
                            cl::store(&mut self.doc, id, &d);
                            self.schedule_commit_now();
                            self.status = "The cloud is clipped to the polygon.".into();
                            f.message.clear();
                        }
                    } else {
                        match (f.alignment.and_then(|a| crate::alignment_glue::alignment_of(&self.doc, a)), parse_f(&f.half_width).filter(|v| *v > 0.0), parse_f(&f.step).filter(|v| *v > 0.0)) {
                            (Some(al), Some(hw), Some(step)) => {
                                d.corridor = Some(cl::Corridor { line: centreline(&al, step), half_width_m: hw });
                                d.clip.clear();
                                cl::store(&mut self.doc, id, &d);
                                self.schedule_commit_now();
                                self.status = format!("The cloud is clipped to a swath {hw} m either side of {}.", al.name);
                                f.message.clear();
                            }
                            _ => f.message = "Choose an alignment and type the half width and the step.".into(),
                        }
                    }
                }
            }
        }
        self.civil_ui.cloud.clip = Some(f);
    }
}

pub fn civil_alignments(doc: &afrobim_kernel::Document) -> Vec<(ElementId, String)> {
    crate::civil::of_category(doc, afrobim_alignment::CATEGORY)
}

fn centreline(al: &afrobim_alignment::Alignment, step: f64) -> Vec<[f64; 2]> {
    let (s0, s1) = (al.start_station_m, al.end_station_m());
    let n = (((s1 - s0) / step).ceil() as usize).clamp(1, 200_000);
    (0..=n).filter_map(|i| al.point_at(s0 + (s1 - s0) * i as f64 / n as f64).map(|p| [p.x, p.y])).collect()
}

// ─── section slices ─────────────────────────────────────────────────────

pub struct SliceForm {
    cloud: Option<ElementId>,
    target: usize,
    group: Option<ElementId>,
    view: Option<ElementId>,
    thickness: String,
    step: String,
    message: String,
}

impl SliceForm {
    fn new(cloud: Option<ElementId>) -> SliceForm {
        SliceForm { cloud, target: 0, group: None, view: None, thickness: String::new(), step: String::new(), message: String::new() }
    }
}

impl AfroBimApp {
    fn cloud_slice_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.cloud.slice.take() else { return };
        let list = cl::clouds(&self.doc);
        let groups = crate::civil::of_category(&self.doc, crate::civil::SAMPLES);
        let views = crate::civil::of_category(&self.doc, crate::civil::PROFILE_VIEW);
        if f.group.is_none() {
            f.group = groups.first().map(|g| g.0);
        }
        if f.view.is_none() {
            f.view = views.first().map(|g| g.0);
        }
        let mut go = false;
        let mut clear = false;
        let out = Dialog::new("Point cloud slices", crate::glyphs::by_name("CLOUDSLICE")).width(600.0).help("Cuts the cloud at a thickness: the points within half the thickness of each sample line are shown in the section views of the group; the points within half the thickness of the alignment are shown in the profile view. The thickness is an input.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "cs-h", |fm| {
                    fm.row("Cloud", |ui| cloud_combo(ui, "cs-c", &list, &mut f.cloud));
                    fm.row("Show in", |ui| {
                        ui_kit::segmented(ui, &mut f.target, &["Section views", "A profile view"]);
                    });
                    if f.target == 0 {
                        fm.row("Sample line group", |ui| cloud_combo(ui, "cs-g", &groups, &mut f.group));
                    } else {
                        fm.row("Profile view", |ui| cloud_combo(ui, "cs-v", &views, &mut f.view));
                        fm.row_note("Centreline step", "an input", |ui| {
                            crate::civil_dlg::num_rule(ui, &mut f.step, "m", NumRule::required().above(0.0));
                        });
                    }
                    fm.row_note("Slice thickness", "total, centred on the line: an input", |ui| {
                        crate::civil_dlg::num_rule(ui, &mut f.thickness, "m", NumRule::required().above(0.0));
                    });
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new("Cut the slices").icon(crate::glyphs::by_name("CLOUDSLICE")).secondary(&["Remove the slices"]).show(ui);
                go = o.primary;
                clear = o.secondary == Some(0);
                o
            },
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if clear || go {
            f.message = self.cloud_do_slices(&f, clear).unwrap_or_else(|e| e);
        }
        self.civil_ui.cloud.slice = Some(f);
    }

    fn cloud_do_slices(&mut self, f: &SliceForm, clear: bool) -> Result<String, String> {
        let id = f.cloud.ok_or("Choose a cloud.")?;
        let d = cl::data_of(&self.doc, id).ok_or("The cloud cannot be read.")?;
        let name = d.reference.name.clone();
        let thickness = parse_f(&f.thickness).filter(|v| *v > 0.0);
        if clear {
            if let Some(g) = f.group.filter(|_| f.target == 0) {
                let mut sd = crate::civil::get::<crate::civil::SampleData>(&self.doc, g, crate::civil::SAMPLES).ok_or("Choose a group.")?;
                sd.slices.retain(|s| s.cloud != name);
                crate::civil::set(&mut self.doc, g, serde_json::to_value(&sd).unwrap_or_default(), None);
            } else if let Some(v) = f.view {
                let mut pv = crate::civil::get::<crate::civil::ProfileViewData>(&self.doc, v, crate::civil::PROFILE_VIEW).ok_or("Choose a view.")?;
                pv.cloud.retain(|s| s.cloud != name);
                crate::civil::set(&mut self.doc, v, serde_json::to_value(&pv).unwrap_or_default(), None);
            }
            self.schedule_commit_now();
            return Ok("The slices were removed.".into());
        }
        let thickness = thickness.ok_or("Type the slice thickness (an input, above zero).")?;
        let loaded = self.civil_ui.cloud.loaded.get(&id).ok_or("The cloud is not open yet.")?;
        let index = loaded.index.clone();
        let plans = cl::chain_plans(&d);
        let filter = (!d.reference.filter.is_empty()).then(|| d.reference.filter.clone());
        let back = |e: f64, n: f64| if plans.is_empty() { Some([e, n]) } else { cl::map_chain_back(&plans, e, n, 0.0).map(|p| [p.0, p.1]) };
        let fwd = |e: f64, n: f64, z: f64| if plans.is_empty() { Some((e, n)) } else { cl::map_chain(&plans, e, n, z) };
        if f.target == 0 {
            let g = f.group.ok_or("Choose a sample line group.")?;
            let mut sd = crate::civil::get::<crate::civil::SampleData>(&self.doc, g, crate::civil::SAMPLES).ok_or("The group cannot be read.")?;
            let al = crate::alignment_glue::alignment_by_name(&self.doc, &sd.group.alignment).ok_or("The alignment of the group is missing.")?;
            let mut lines = Vec::new();
            let mut total = 0;
            for l in &sd.group.lines {
                let p = al.point_at(l.station_m).ok_or("A sample line is off the alignment.")?;
                let (a, b) = (p.offset(-l.left_swath_m), p.offset(l.right_swath_m));
                let (a, b) = (back(a.0, a.1).ok_or("A section line cannot be moved to the file's coordinates.")?, back(b.0, b.1).ok_or("A section line cannot be moved to the file's coordinates.")?);
                let slice = section_slice(&index, &[a, b], thickness, filter.as_ref()).map_err(|e| e.to_string())?;
                let pts: Vec<[f64; 2]> = slice.points.iter().filter(|_| true).map(|p| [p.offset - l.left_swath_m, p.z]).collect();
                let _ = fwd;
                total += pts.len();
                lines.push(crate::civil::SliceLine { name: l.name.clone(), points: pts });
            }
            sd.slices.retain(|s| s.cloud != name);
            sd.slices.push(crate::civil::SliceSet { cloud: name.clone(), thickness_m: thickness, lines });
            crate::civil::set(&mut self.doc, g, serde_json::to_value(&sd).unwrap_or_default(), None);
            self.schedule_commit_now();
            Ok(format!("Cut {total} points into the section views of the group at a thickness of {thickness} m."))
        } else {
            let v = f.view.ok_or("Choose a profile view.")?;
            let step = parse_f(&f.step).filter(|v| *v > 0.0).ok_or("Type the centreline step (an input, above zero).")?;
            let mut pv = crate::civil::get::<crate::civil::ProfileViewData>(&self.doc, v, crate::civil::PROFILE_VIEW).ok_or("The profile view cannot be read.")?;
            let al = crate::alignment_glue::alignment_by_name(&self.doc, &pv.alignment).ok_or("The alignment of the view is missing.")?;
            let line = centreline(&al, step);
            let line_back: Vec<[f64; 2]> = line.iter().filter_map(|p| back(p[0], p[1])).collect();
            let slice = section_slice(&index, &line_back, thickness, filter.as_ref()).map_err(|e| e.to_string())?;
            // The offset along the sampled centreline is the distance from the start: station = start + offset (to the step's accuracy).
            let total_len = slice.length.max(1e-9);
            let al_len = al.end_station_m() - al.start_station_m;
            let pts: Vec<[f64; 2]> = slice.points.iter().map(|p| [al.start_station_m + p.offset / total_len * al_len, p.z]).collect();
            let n = pts.len();
            pv.cloud.retain(|s| s.cloud != name);
            pv.cloud.push(crate::civil::CloudProfile { cloud: name, thickness_m: thickness, points: pts });
            crate::civil::set(&mut self.doc, v, serde_json::to_value(&pv).unwrap_or_default(), None);
            self.schedule_commit_now();
            Ok(format!("Cut {n} points into the profile view at a thickness of {thickness} m."))
        }
    }
}

// ─── surface from the cloud ─────────────────────────────────────────────

pub struct SurfForm {
    cloud: Option<ElementId>,
    /// 0 from classes, 1 progressive morphological filter, 2 grid minimum and slope.
    method: usize,
    classes: String,
    cell: String,
    window_start: String,
    window_step: String,
    window_max: String,
    slope: String,
    initial: String,
    max_dist: String,
    search: String,
    tol: String,
    thin: usize,
    thin_cell: String,
    stat: usize,
    decim: String,
    name: String,
    message: String,
}

impl SurfForm {
    fn new(cloud: Option<ElementId>) -> SurfForm {
        let _ = &cloud;
        SurfForm { cloud, method: 0, classes: "2".into(), cell: String::new(), window_start: String::new(), window_step: String::new(), window_max: String::new(), slope: String::new(), initial: String::new(), max_dist: String::new(), search: String::new(), tol: String::new(), thin: 0, thin_cell: String::new(), stat: 0, decim: String::new(), name: String::new(), message: String::new() }
    }

    /// The ground selection as a closure over the index's buffer.
    fn spec(&self) -> Result<(GroundSel, SurfaceSpec), String> {
        let need = |what: &str, t: &str| parse_f(t).filter(|v| *v > 0.0).ok_or_else(|| format!("{what} is an input above zero: type it."));
        let sel = match self.method {
            0 => {
                let classes: Vec<u8> = self.classes.split([',', ' ']).filter(|s| !s.is_empty()).map(|s| s.parse::<u8>().map_err(|_| format!("'{s}' is not a class code."))).collect::<Result<_, _>>()?;
                if classes.is_empty() {
                    return Err("Name the class codes that are ground (the LAS class for ground is listed in the class table).".into());
                }
                GroundSel::Classes(classes)
            }
            1 => {
                let (cell, s, st, mx) = (need("The cell size", &self.cell)?, need("The first window", &self.window_start)? as usize, need("The window step", &self.window_step)? as usize, need("The last window", &self.window_max)? as usize);
                GroundSel::Pmf(PmfParams::from_series(cell, linear_windows(s, st, mx), need("The slope", &self.slope)?, need("The initial distance", &self.initial)?, need("The largest distance", &self.max_dist)?))
            }
            _ => GroundSel::GridSlope(GridMinSlopeParams { cell_size: need("The cell size", &self.cell)?, max_slope: need("The largest slope", &self.slope)?, search_cells: need("The search window in cells", &self.search)? as usize, tolerance: need("The tolerance above the cell minimum", &self.tol)? }),
        };
        let grid = if self.thin == 0 { Some((need("The thinning cell size", &self.thin_cell)?, [CellStat::Min, CellStat::Mean, CellStat::Median][self.stat.min(2)])) } else { None };
        let tolerance = if self.decim.trim().is_empty() { None } else { Some(need("The decimation tolerance", &self.decim)?) };
        Ok((sel, SurfaceSpec { grid, tolerance }))
    }
}

pub enum GroundSel {
    Classes(Vec<u8>),
    Pmf(PmfParams),
    GridSlope(GridMinSlopeParams),
}

impl AfroBimApp {
    fn cloud_surf_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.cloud.surf.take() else { return };
        let list = cl::clouds(&self.doc);
        let busy = self.bg.cloud.is_some();
        let mut run = false;
        let mut make = false;
        let out_ref = self.civil_ui.cloud.surf_out.as_ref().map(|o| (o.name.clone(), o.cloud.clone(), o.ground.ground_points_in, o.ground.after_grid, o.ground.after_decimation, o.ground.fit.clone(), o.notes.clone(), o.ground.tin.triangles().len()));
        let out = Dialog::new("Surface from a point cloud", crate::glyphs::by_name("CLOUDSURFACE")).width(700.0).resizable(true).max_body_height(640.0).help("Ground points are chosen from the classes the file already has, or by a ground filter whose parameters are your inputs. They are thinned (grid, then tolerance) and triangulated. The fit report gives the difference between ALL the input ground points and the surface. The result is a normal surface whose source record names the cloud.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "cf-h", |fm| {
                    fm.row("Cloud", |ui| cloud_combo(ui, "cf-c", &list, &mut f.cloud));
                    fm.row("Surface name", |ui| {
                        ui_kit::text_field(ui, &mut f.name, "defaults to the cloud's name");
                    });
                });
                ui_kit::section(ui, "Ground points", |ui| {
                    ui_kit::radio_group(ui, &mut f.method, &["From the classes the file has", "Progressive morphological filter (Zhang et al. 2003, as PDAL describes it)", "Grid minimum and slope filter (this crate's own method)"]);
                    ui_kit::form(ui, "cf-g", |fm| match f.method {
                        0 => {
                            fm.row_note("Ground class codes", "numbers separated by commas", |ui| {
                                ui_kit::text_field(ui, &mut f.classes, "");
                            });
                        }
                        1 => {
                            for (label, unit, text) in [("Cell size", "m", &mut f.cell), ("First window", "cells", &mut f.window_start), ("Window step", "cells", &mut f.window_step), ("Last window", "cells", &mut f.window_max), ("Slope", "rise over run", &mut f.slope), ("Initial distance", "m", &mut f.initial), ("Largest distance", "m", &mut f.max_dist)] {
                                fm.row(label, |ui| {
                                    crate::civil_dlg::num_rule(ui, text, unit, NumRule::required().above(0.0));
                                });
                            }
                        }
                        _ => {
                            for (label, unit, text) in [("Cell size", "m", &mut f.cell), ("Largest slope", "rise over run", &mut f.slope), ("Search window", "cells", &mut f.search), ("Tolerance above the cell minimum", "m", &mut f.tol)] {
                                fm.row(label, |ui| {
                                    crate::civil_dlg::num_rule(ui, text, unit, NumRule::required().above(0.0));
                                });
                            }
                        }
                    });
                });
                ui_kit::section(ui, "Thinning", |ui| {
                    ui_kit::radio_group(ui, &mut f.thin, &["Thin to a grid first", "Do not thin to a grid"]);
                    ui_kit::form(ui, "cf-t", |fm| {
                        if f.thin == 0 {
                            fm.row("Cell size", |ui| {
                                crate::civil_dlg::num_rule(ui, &mut f.thin_cell, "m", NumRule::required().above(0.0));
                            });
                            fm.row("Point kept in each cell", |ui| {
                                ui_kit::combo(ui, "cf-s", &mut f.stat, &["Lowest", "Mean", "Median"]);
                            });
                        }
                        fm.row_note("Then decimate to a tolerance", "optional; vertical tolerance at the dropped points", |ui| {
                            crate::civil_dlg::num_opt(ui, &mut f.decim, "m").ok();
                        });
                    });
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new("Fit the surface")).clicked() {
                        run = true;
                    }
                    if busy {
                        ui.spinner();
                    }
                });
                if let Some((name, cloud, g, a, b, fit, notes, tris)) = &out_ref {
                    ui_kit::section(ui, "Fit report", |ui| {
                        ui.label(format!("{name} from {cloud}: {g} ground points, {a} after the grid, {b} in the surface, {tris} triangles."));
                        let rows = vec![vec!["Points compared".into(), fit.compared.to_string()], vec!["Outside the surface".into(), fit.outside.to_string()], vec!["Mean (point minus surface)".into(), format!("{:+.4} m", fit.mean)], vec!["RMS".into(), format!("{:.4} m", fit.rms)], vec!["Largest absolute".into(), format!("{:.4} m", fit.max_abs)], vec!["Lowest".into(), format!("{:+.4} m", fit.min)], vec!["Highest".into(), format!("{:+.4} m", fit.max)]];
                        ui_kit::results_table(ui, "cf-fit", &["Measure", "Value"], &[Col::Text, Col::Num], &rows);
                        for n in notes {
                            ui_kit::note(ui, n.clone());
                        }
                        if ui.button("Make the surface in the document").clicked() {
                            make = true;
                        }
                    });
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
            },
            |ui| Footer::new("Close").cancel(None).show(ui),
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            return;
        }
        if run {
            match (f.cloud, f.spec()) {
                (Some(id), Ok((sel, spec))) => match (cl::data_of(&self.doc, id), self.civil_ui.cloud.loaded.get(&id).map(|l| l.index.clone())) {
                    (Some(d), Some(index)) => {
                        let plans = cl::chain_plans(&d);
                        let name = if f.name.trim().is_empty() { format!("Ground from {}", d.reference.name) } else { f.name.trim().to_string() };
                        let cloud_name = d.reference.name.clone();
                        let source = format!("Point cloud {} (SHA-256 {})", d.reference.source_path, d.reference.source_hash);
                        let fp = crate::background::fingerprint(&self.doc);
                        self.bg.cloud = Some(Job::spawn("Fitting a surface to the ground points", fp, move || {
                            let r = (|| {
                                let chosen = cl::selected_indices(&d, &index, &plans);
                                let buf = index.points();
                                let ground = match &sel {
                                    GroundSel::Classes(c) => ground_from_classes(buf, Some(&chosen), c),
                                    GroundSel::Pmf(p) => progressive_morphological_filter(buf, Some(&chosen), p).map_err(|e| e.to_string())?.ground,
                                    GroundSel::GridSlope(p) => grid_minimum_slope_filter(buf, Some(&chosen), p).map_err(|e| e.to_string())?.ground,
                                };
                                if ground.len() < 3 {
                                    return Err(format!("Only {} ground point(s) were found: the surface needs at least three.", ground.len()));
                                }
                                let gs = ground_surface(buf, &ground, &spec).map_err(|e| e.to_string())?;
                                let mut notes = vec![format!("{} of {} selected points are ground.", ground.len(), chosen.len())];
                                if !plans.is_empty() {
                                    notes.push("The cloud was moved by TRANSFORM: the surface takes the points as they are in the file, not in the moved coordinates. Make the surface before transforming, or transform it as well.".into());
                                }
                                Ok(SurfOut { name, cloud: cloud_name, ground: gs, notes, source })
                            })();
                            CloudJob::Surface(r)
                        }));
                    }
                    _ => f.message = "The cloud is not open yet.".into(),
                },
                (_, Err(e)) => f.message = e,
                _ => f.message = "Choose a cloud.".into(),
            }
        }
        if make {
            if let Some(o) = self.civil_ui.cloud.surf_out.take() {
                let mut surface = afrobim_terrain::Surface::new(&o.name, afrobim_terrain::SurfaceRole::ExistingGround);
                surface.source.description = o.source.clone();
                surface.definition.push(afrobim_terrain::Operation::AddPoints { group: o.cloud.clone(), points: surface_points(&o.ground.points, "C") });
                let b = crate::civil::Bounds::of(o.ground.points.iter().map(|p| (p[0], p[1])));
                let data = crate::civil::SurfaceData { surface, style: crate::civil::SurfaceStyle::example() };
                let id = crate::civil::add(&mut self.doc, crate::civil::SURFACE, &o.name, serde_json::to_value(&data).unwrap_or_default(), b, &o.source);
                self.doc.select(&[id], afrobim_kernel::SelectMode::Replace);
                self.status = format!("Made the surface {} from the point cloud {}: {} points; fit RMS {:.3} m.", o.name, o.cloud, o.ground.after_decimation, o.ground.fit.rms);
                self.schedule_commit_now();
                f.message = self.status.clone();
            }
        }
        self.civil_ui.cloud.surf = Some(f);
    }
}

// ─── compare with a surface ─────────────────────────────────────────────

pub struct CmpForm {
    cloud: Option<ElementId>,
    surface: Option<ElementId>,
    breaks: String,
    hist_min: String,
    bin_width: String,
    bins: String,
    message: String,
    example: bool,
}

impl CmpForm {
    fn new(cloud: Option<ElementId>, example: bool) -> CmpForm {
        let mut f = CmpForm { cloud, surface: None, breaks: String::new(), hist_min: String::new(), bin_width: String::new(), bins: String::new(), message: String::new(), example };
        if example {
            // Example values for the demonstration, marked as such in the dialog.
            f.breaks = "-1, -0.5, -0.1, 0.1, 0.5, 1".into();
            f.hist_min = "-2".into();
            f.bin_width = "0.25".into();
            f.bins = "16".into();
        }
        f
    }
}

impl AfroBimApp {
    fn cloud_cmp_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.cloud.cmp.take() else { return };
        let list = cl::clouds(&self.doc);
        let surfaces = crate::civil::surface_choices(&self.doc);
        if f.surface.is_none() {
            f.surface = surfaces.first().map(|s| s.0);
        }
        let busy = self.bg.cloud.is_some();
        let (mut run, mut show, mut hide) = (false, false, false);
        let shown = f.cloud.is_some_and(|c| self.civil_ui.cloud.compare.contains_key(&c));
        let result = self.civil_ui.cloud.cmp_out.as_ref().filter(|(c, _)| Some(*c) == f.cloud).map(|(_, o)| (o.cmp.clone(), o.surface.clone()));
        let out = Dialog::new("Compare a point cloud with a surface", crate::glyphs::by_name("CLOUDCOMPARE")).width(720.0).resizable(true).max_body_height(660.0).help("Each point's level minus the surface level at its position: for as-built checks. The bands, the histogram and the statistics are yours to define; points outside the surface are counted separately. The bands can colour the cloud in the plan and in 3D.").show(
            ctx,
            |ui| {
                if f.example {
                    ui_kit::callout(ui, Level::Info, "EXAMPLE VALUES for the demonstration: the bands and the histogram are not tolerances. Type your own.");
                }
                ui_kit::form(ui, "cm-h", |fm| {
                    fm.row("Cloud", |ui| cloud_combo(ui, "cm-c", &list, &mut f.cloud));
                    fm.row("Surface", |ui| cloud_combo(ui, "cm-s", &surfaces, &mut f.surface));
                    fm.row_note("Band breaks", "metres, increasing, separated by commas", |ui| {
                        ui_kit::text_field(ui, &mut f.breaks, "");
                    });
                    fm.row("Histogram: first bin starts at", |ui| {
                        crate::civil_dlg::num(ui, &mut f.hist_min, "m");
                    });
                    fm.row("Bin width", |ui| {
                        crate::civil_dlg::num_rule(ui, &mut f.bin_width, "m", NumRule::required().above(0.0));
                    });
                    fm.row("Number of bins", |ui| {
                        crate::civil_dlg::num_rule(ui, &mut f.bins, "", NumRule::required().above(0.0));
                    });
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new("Compare")).clicked() {
                        run = true;
                    }
                    if busy {
                        ui.spinner();
                    }
                });
                if let Some((c, sname)) = &result {
                    ui_kit::section(ui, &format!("Against {sname}"), |ui| {
                        let s = &c.stats;
                        let rows = vec![vec!["Points compared".into(), s.compared.to_string()], vec!["Outside the surface".into(), c.outside.to_string()], vec!["Mean (point minus surface)".into(), format!("{:+.4} m", s.mean)], vec!["RMS".into(), format!("{:.4} m", s.rms)], vec!["Largest absolute".into(), format!("{:.4} m", s.max_abs)], vec!["Lowest".into(), format!("{:+.4} m", s.min)], vec!["Highest".into(), format!("{:+.4} m", s.max)]];
                        ui_kit::results_table(ui, "cm-st", &["Statistic", "Value"], &[Col::Text, Col::Num], &rows);
                        histogram_plot(ui, &c.histogram);
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!shown, egui::Button::new("Colour the cloud by the bands")).clicked() {
                                show = true;
                            }
                            if ui.add_enabled(shown, egui::Button::new("Back to the cloud's own colours")).clicked() {
                                hide = true;
                            }
                        });
                    });
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
            },
            |ui| Footer::new("Close").cancel(None).show(ui),
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            return;
        }
        if hide {
            if let Some(c) = f.cloud {
                self.civil_ui.cloud.compare.remove(&c);
            }
        }
        if show {
            if let Some((c, o)) = &self.civil_ui.cloud.cmp_out {
                self.civil_ui.cloud.compare.insert(*c, o.compare.clone());
            }
        }
        if run {
            let parse_breaks = || -> Result<Vec<f64>, String> { f.breaks.split(',').map(|s| parse_f(s).ok_or_else(|| format!("'{}' is not a number.", s.trim()))).collect() };
            match (f.cloud, f.surface, parse_breaks(), parse_f(&f.hist_min), parse_f(&f.bin_width), parse_f(&f.bins)) {
                (Some(id), Some(sid), Ok(breaks), Some(hmin), Some(bw), Some(bins)) => {
                    let tin = self.tin_for(sid);
                    let sname = self.doc.find(sid).map(|e| e.name.clone()).unwrap_or_default();
                    match (cl::data_of(&self.doc, id), self.civil_ui.cloud.loaded.get(&id).map(|l| l.index.clone()), tin, DifferenceBands::new(breaks)) {
                        (Some(d), Some(index), Some(tin), Ok(bands)) => {
                            let plans = cl::chain_plans(&d);
                            let fp = crate::background::fingerprint(&self.doc);
                            self.bg.cloud = Some(Job::spawn("Comparing the cloud with the surface", fp, move || {
                                let r = (|| {
                                    let idx = cl::selected_indices(&d, &index, &plans);
                                    // The points are compared where the surface is: in the project's coordinates.
                                    let mut mapped = PointBuffer::new(false, false);
                                    for &i in &idx {
                                        let p = index.points().xyz[i as usize];
                                        let (e, n) = cl::map_chain(&plans, p[0], p[1], p[2]).ok_or("A point could not be moved to the project's coordinates.")?;
                                        mapped.push(&PointRecord { xyz: [e, n, p[2]], ..Default::default() });
                                    }
                                    let cmp = compare_to_tin(&mapped, None, &tin, hmin, bw, bins as usize, &bands).map_err(|e| e.to_string())?;
                                    let colors = cmp.colors(&BandColors::blue_white_red(bands.band_count()));
                                    let compare = Compare { indices: idx, colors };
                                    Ok(CmpOut { cmp, surface: sname, compare })
                                })();
                                CloudJob::Compare(id, r)
                            }));
                        }
                        (_, _, None, _) => f.message = "That surface has no built TIN yet.".into(),
                        (_, None, _, _) => f.message = "The cloud is not open yet.".into(),
                        (_, _, _, Err(e)) => f.message = e.to_string(),
                        _ => {}
                    }
                }
                (_, _, Err(e), ..) => f.message = e,
                _ => f.message = "Choose a cloud and a surface and fill the histogram and band fields.".into(),
            }
        }
        self.civil_ui.cloud.cmp = Some(f);
    }
}

fn histogram_plot(ui: &mut egui::Ui, h: &Histogram) {
    let t = crate::theme::tokens_ui(ui);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(660.0), 120.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, t.field);
    let max = h.counts.iter().copied().max().unwrap_or(1).max(1) as f32;
    let n = h.counts.len().max(1) as f32;
    let bw = rect.width() / n;
    for (i, c) in h.counts.iter().enumerate() {
        let hh = (*c as f32 / max) * (rect.height() - 16.0);
        let r = egui::Rect::from_min_max(egui::pos2(rect.left() + i as f32 * bw + 1.0, rect.bottom() - 14.0 - hh), egui::pos2(rect.left() + (i as f32 + 1.0) * bw - 1.0, rect.bottom() - 14.0));
        p.rect_filled(r, 0.0, t.accent);
    }
    p.text(rect.left_bottom() + egui::vec2(2.0, -1.0), egui::Align2::LEFT_BOTTOM, format!("{:+.2} m", h.min), egui::FontId::proportional(10.0), t.text_muted);
    p.text(rect.right_bottom() + egui::vec2(-2.0, -1.0), egui::Align2::RIGHT_BOTTOM, format!("{:+.2} m", h.min + h.bin_width * h.counts.len() as f64), egui::FontId::proportional(10.0), t.text_muted);
    p.text(rect.center_bottom() + egui::vec2(0.0, -1.0), egui::Align2::CENTER_BOTTOM, format!("below {}, above {}", h.below, h.above), egui::FontId::proportional(10.0), t.text_muted);
}

// ─── export a subset ────────────────────────────────────────────────────

pub struct ExportForm {
    cloud: Option<ElementId>,
    /// 0 LAS 1.2, 1 LAS 1.4, 2 LAZ 1.2, 3 LAZ 1.4.
    format: usize,
    like_source: bool,
    scale: String,
    message: String,
}

impl ExportForm {
    fn new(cloud: Option<ElementId>) -> ExportForm {
        ExportForm { cloud, format: 1, like_source: true, scale: String::new(), message: String::new() }
    }
}

impl AfroBimApp {
    fn cloud_export_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.cloud.export.take() else { return };
        let list = cl::clouds(&self.doc);
        let busy = self.bg.cloud.is_some();
        let mut go = false;
        let done = self.civil_ui.cloud.export_out.clone();
        let info = f.cloud.and_then(|c| cl::data_of(&self.doc, c).zip(self.civil_ui.cloud.loaded.get(&c).map(|l| l.index.clone()))).map(|(d, idx)| (cl::selected_indices(&d, &idx, &cl::chain_plans(&d)).len(), d.moved.len(), d.reference.header.scale.is_some()));
        let out = Dialog::new("Export a point cloud subset", crate::glyphs::by_name("CLOUDEXPORT")).width(600.0).help("Writes the points that pass the class filter and the clip to a new LAS or LAZ file. Like the source, the scale and offset of the source are kept and every coordinate is written back exactly; otherwise the scale is your input and the report gives the largest rounding error.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "ce-h", |fm| {
                    fm.row("Cloud", |ui| cloud_combo(ui, "ce-c", &list, &mut f.cloud));
                    fm.row("Format", |ui| {
                        ui_kit::combo(ui, "ce-f", &mut f.format, &["LAS 1.2", "LAS 1.4", "LAZ (LAS 1.2)", "LAZ (LAS 1.4)"]);
                    });
                    fm.row_note("Scale and offset", "like the source keeps coordinates exactly", |ui| {
                        ui.checkbox(&mut f.like_source, "Like the source file");
                    });
                    if !f.like_source || !info.is_some_and(|i| i.2) {
                        fm.row_note("Coordinate scale", "metres per unit, an input", |ui| {
                            crate::civil_dlg::num_rule(ui, &mut f.scale, "m", NumRule::required().above(0.0));
                        });
                    }
                });
                if let Some((n, moved, _)) = info {
                    ui_kit::note(ui, format!("{n} point(s) pass the class filter and the clip."));
                    if moved > 0 {
                        ui_kit::callout(ui, Level::Warning, "The cloud was moved by TRANSFORM. The export writes the points as they are in the file (its own coordinates), not moved.");
                    }
                }
                if busy {
                    ui.spinner();
                }
                if let Some(t) = &done {
                    ui_kit::callout(ui, Level::Success, t.clone());
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new("Export...").icon(crate::glyphs::by_name("CLOUDEXPORT")).enabled(!busy, "A point cloud job is running").show(ui);
                go = o.primary;
                o
            },
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if go {
            if let (Some(id), Some(path)) = (f.cloud, rfd::FileDialog::new().add_filter("LAS or LAZ", &["las", "laz"]).set_file_name(if f.format >= 2 { "subset.laz" } else { "subset.las" }).save_file()) {
                match (cl::data_of(&self.doc, id), self.civil_ui.cloud.loaded.get(&id).map(|l| l.index.clone())) {
                    (Some(d), Some(index)) => {
                        let version = if f.format % 2 == 1 { LasVersion::V1_4 } else { LasVersion::V1_2 };
                        let laz = f.format >= 2;
                        let opts = if f.like_source && d.reference.header.scale.is_some() {
                            Ok(ExportOptions::like_source(&d.reference.header, version, laz))
                        } else {
                            parse_f(&f.scale).filter(|s| *s > 0.0).map(|s| ExportOptions::with_scale([s; 3], version, laz)).ok_or("Type the coordinate scale (an input, above zero).")
                        };
                        match opts {
                            Ok(opts) => {
                                let plans = cl::chain_plans(&d);
                                let fp = crate::background::fingerprint(&self.doc);
                                self.civil_ui.cloud.export_out = None;
                                self.bg.cloud = Some(Job::spawn("Exporting a point cloud subset", fp, move || {
                                    let idx = cl::selected_indices(&d, &index, &plans);
                                    CloudJob::Export(export_las(&path, index.points(), Some(&idx), &opts).map_err(|e| e.to_string()))
                                }));
                            }
                            Err(e) => f.message = e.into(),
                        }
                    }
                    _ => f.message = "The cloud is not open yet.".into(),
                }
            }
        }
        self.civil_ui.cloud.export = Some(f);
    }
}

#[allow(dead_code)]
fn _unused(_: Option<&str>) -> String {
    fmt_opt(None, 0)
}

/// The scene of a cloud element: its footprint, the clip, and what it is. The points are drawn
/// separately (`cloud_paint_plan`, `cloud_points_3d`) from the opened index.
pub fn build_cloud_scene(e: &afrobim_kernel::Element) -> Result<crate::civil_build::Built, String> {
    use crate::civil_scene::{SLine, SText};
    let d: CloudData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The point cloud data cannot be read: {x}"))?;
    let mut scene = crate::civil_scene::Scene::default();
    if let Some(x) = d.extent {
        let ring = [[x[0][0], x[0][1]], [x[1][0], x[0][1]], [x[1][0], x[1][1]], [x[0][0], x[1][1]], [x[0][0], x[0][1]]];
        scene.lines.push(SLine::new(ring.iter().map(|p| crate::civil_build::mm(p[0], p[1])).collect(), [0x9A, 0x80, 0xC8], 0.8));
        scene.texts.push(SText { at: crate::civil_build::mm(x[0][0], x[1][1]), text: d.reference.name.clone(), rgb: [0xB0, 0xA0, 0xD0], height_mm: 0.0, px: 10.5, anchor: 4, prio: 2, halo: true });
    }
    if d.clip.len() >= 3 {
        let mut ring: Vec<[f64; 2]> = d.clip.iter().map(|p| crate::civil_build::mm(p[0], p[1])).collect();
        ring.push(ring[0]);
        scene.lines.push(SLine::new(ring, [0xE0, 0x6C, 0x2C], 1.4));
    }
    if let Some(c) = &d.corridor {
        scene.lines.push(SLine::new(c.line.iter().map(|p| crate::civil_build::mm(p[0], p[1])).collect(), [0xE0, 0x6C, 0x2C], 1.0));
    }
    let h = &d.reference.header;
    scene.derived.push(("Points".into(), format!("{}", h.point_count)));
    scene.derived.push(("Source".into(), if d.synthetic { "synthetic".into() } else { d.reference.source_path.clone() }));
    scene.derived.push(("Colour".into(), format!("{:?}", d.reference.display.color_mode).chars().take(24).collect()));
    Ok(crate::civil_build::Built { scene, ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::u4_dialogs_test::{app_for, frame};

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("afrobim-cloud-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn demo_app() -> (egui::Context, AfroBimApp) {
        let (ctx, mut app) = app_for("dark-glass", true);
        frame(&ctx, &mut app, 3);
        (ctx, app)
    }

    #[test]
    fn the_demonstration_cloud_is_a_reference_with_no_points_and_survives_a_save() {
        let (_ctx, app) = demo_app();
        let (id, _) = cl::clouds(&app.doc).into_iter().next().expect("the demonstration has a cloud");
        let d = cl::data_of(&app.doc, id).unwrap();
        assert!(d.synthetic && d.reference.name.contains("synthetic"));
        let json = app.doc.to_json();
        assert!(!json.contains("\"xyz\""), "no points are stored in the document");
        let back = afrobim_kernel::Document::from_json(&json).unwrap();
        assert_eq!(cl::data_of(&back, id).unwrap(), d);
        assert!(app.civil_ui.cloud.loaded.contains_key(&id), "the synthetic index is generated on the fly");
    }

    #[test]
    fn a_las_file_is_attached_indexed_with_a_cache_file_and_checked_against_the_project() {
        let dir = temp_dir("las");
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../afrobim-pointcloud/tests/data/autzen.las");
        let path = dir.join("autzen.las");
        std::fs::copy(&src, &path).unwrap();
        let (out, reference) = cl::open_file(&path, &SourceOptions::default(), &|_| {}).expect("opens");
        assert!(out.index.len() > 100 && reference.header.point_count as usize == out.index.len());
        assert!(out.status.contains("built"), "{}", out.status);
        assert!(cache_path_for(&path).exists(), "the cache file sits beside the source");
        let (again, _) = cl::open_file(&path, &SourceOptions::default(), &|_| {}).unwrap();
        assert!(again.status.contains("cache file"), "{}", again.status);
        // The file's own system is compared with the project's: a plain sentence either way.
        let project = afrobim_kernel::ProjectCrs::from_epsg(32736);
        let findings = crate::crs_ui::dataset_check(project.as_ref(), "autzen", cl::file_crs(&reference.header).as_ref(), cl::header_extent(&reference.header).map(|e| afrobim_kernel::PlanExtent { min_easting: e[0][0], min_northing: e[0][1], max_easting: e[1][0], max_northing: e[1][1] }));
        assert!(!findings.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_text_file_is_read_through_a_mapping_the_user_confirms() {
        let dir = temp_dir("txt");
        let path = dir.join("scan.xyz");
        let mut t = String::from("x y z intensity\n");
        for i in 0..200 {
            t.push_str(&format!("{} {} {} {}\n", 450_000.0 + i as f64 * 0.5, 9_960_000.0 + (i % 10) as f64, 1150.0 + (i % 7) as f64 * 0.1, 100 + i));
        }
        std::fs::write(&path, t).unwrap();
        let det = detect_text_format(&path, 5).unwrap();
        let mut fmt = det.format.clone();
        fmt.rgb_max = 255.0;
        let opts = SourceOptions { text: Some(fmt), e57_apply_pose: true };
        let (out, reference) = cl::open_file(&path, &opts, &|_| {}).unwrap();
        assert_eq!(out.index.len(), 200);
        assert!(reference.header.bounds.unwrap().min[1] >= 9_960_000.0, "kept at UTM magnitude");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_display_buffer_keeps_utm_coordinates_exact_in_f32_and_the_raster_draws_the_view() {
        let (ctx, app) = demo_app();
        let (id, _) = cl::clouds(&app.doc).into_iter().next().unwrap();
        let d = cl::data_of(&app.doc, id).unwrap();
        let index = &app.civil_ui.cloud.loaded[&id].index;
        let (pos, col, origin) = cl::build_3d(&d, index, &[], 5000, None);
        assert!(!pos.is_empty() && pos.len() == col.len());
        assert!(origin[1] > 9.0e6, "the origin carries the magnitude");
        let b = index.bounds();
        for p in &pos {
            let n = origin[1] + p[1] as f64;
            assert!(n >= b.min[1] - 1e-3 && n <= b.max[1] + 1e-3);
        }
        // The f32 positions are small numbers, so a millimetre survives.
        assert!(pos.iter().all(|p| p[0].abs() < 1000.0 && p[1].abs() < 1000.0));
        let r = cl::build_raster(&ctx, &d, index, &[], [450_000.0, 9_960_000.0], [450_300.0, 9_960_300.0], [300, 300], None).expect("a raster");
        assert!(r.points > 0 && r.points <= d.reference.display.max_points_per_view);
    }

    #[test]
    fn the_budget_limits_the_points_and_a_clip_removes_those_outside() {
        let (ctx, app) = demo_app();
        let (id, _) = cl::clouds(&app.doc).into_iter().next().unwrap();
        let mut d = cl::data_of(&app.doc, id).unwrap();
        let index = &app.civil_ui.cloud.loaded[&id].index;
        d.reference.display.max_points_per_view = 2000;
        let r = cl::build_raster(&ctx, &d, index, &[], [450_000.0, 9_960_000.0], [450_300.0, 9_960_300.0], [300, 300], None).unwrap();
        assert!(r.points <= 2000 && !r.complete);
        // A clip to a 50 m square keeps about its share of the points.
        d.clip = vec![[450_100.0, 9_960_100.0], [450_150.0, 9_960_100.0], [450_150.0, 9_960_150.0], [450_100.0, 9_960_150.0]];
        let sel = cl::selected_indices(&d, index, &[]);
        assert!(!sel.is_empty() && sel.len() < index.len() / 20, "{} of {}", sel.len(), index.len());
        assert!(sel.iter().all(|&i| {
            let p = index.points().xyz[i as usize];
            (450_100.0..=450_150.0).contains(&p[0]) && (9_960_100.0..=9_960_150.0).contains(&p[1])
        }));
        // A class filter drops the trees.
        d.clip.clear();
        d.reference.filter.classes = Some(vec![2]);
        let ground = cl::selected_indices(&d, index, &[]);
        assert!(ground.len() < index.len() && ground.iter().all(|&i| index.points().class[i as usize] == 2));
    }

    #[test]
    fn a_surface_from_the_cloud_fits_the_ground_and_names_the_cloud_as_its_source() {
        let (_ctx, app) = demo_app();
        let (id, _) = cl::clouds(&app.doc).into_iter().next().unwrap();
        let d = cl::data_of(&app.doc, id).unwrap();
        let index = &app.civil_ui.cloud.loaded[&id].index;
        let buf = index.points();
        let ground = ground_from_classes(buf, None, &[2]);
        assert!(ground.len() > 50_000);
        // The test's own thinning numbers: the application supplies none.
        let gs = ground_surface(buf, &ground, &SurfaceSpec { grid: Some((4.0, CellStat::Median)), tolerance: Some(0.05) }).unwrap();
        assert!(gs.after_decimation < gs.after_grid && gs.fit.rms < 0.5, "{} {}", gs.after_decimation, gs.fit.rms);
        let mut surface = afrobim_terrain::Surface::new("From cloud", afrobim_terrain::SurfaceRole::ExistingGround);
        surface.source.description = format!("Point cloud {}", d.reference.source_path);
        surface.definition.push(surface_operation("cloud", &gs.points));
        let built = surface.build().expect("a normal surface");
        assert!(built.tin.triangles().len() > 100);
    }

    #[test]
    fn the_cloud_against_a_surface_shows_ground_near_zero_and_trees_above() {
        let (_ctx, mut app) = demo_app();
        let (id, _) = cl::clouds(&app.doc).into_iter().next().unwrap();
        app.civil_build_now();
        let sid = crate::civil::surface_choices(&app.doc).into_iter().find(|s| s.1.contains("existing ground")).unwrap().0;
        let tin = app.tin_for(sid).expect("a built surface");
        let index = app.civil_ui.cloud.loaded[&id].index.clone();
        let bands = DifferenceBands::new(vec![-0.5, -0.1, 0.1, 0.5, 2.0]).unwrap();
        let c = compare_to_tin(index.points(), None, &tin, -2.0, 0.5, 40, &bands).unwrap();
        assert!(c.stats.compared > 50_000);
        // Most points are ground: the middle band holds the most.
        let mid = c.band_counts[2];
        assert!(mid as f64 > 0.7 * c.stats.compared as f64, "{:?}", c.band_counts);
        assert!(c.stats.max > 3.0, "the trees stand above the ground: {}", c.stats.max);
    }

    #[test]
    fn slices_are_cut_into_the_section_views_and_the_profile_view() {
        let (_ctx, mut app) = demo_app();
        let (id, _) = cl::clouds(&app.doc).into_iter().next().unwrap();
        let group = crate::civil::of_category(&app.doc, crate::civil::SAMPLES).first().unwrap().0;
        let view = crate::civil::of_category(&app.doc, crate::civil::PROFILE_VIEW).first().unwrap().0;
        let mut f = SliceForm::new(Some(id));
        f.group = Some(group);
        f.view = Some(view);
        f.thickness = "2".into();
        f.step = "2".into();
        let msg = app.cloud_do_slices(&f, false).expect("sections");
        assert!(msg.contains("points"), "{msg}");
        let sd = crate::civil::get::<crate::civil::SampleData>(&app.doc, group, crate::civil::SAMPLES).unwrap();
        assert!(sd.slices[0].lines.iter().any(|l| !l.points.is_empty()));
        f.target = 1;
        app.cloud_do_slices(&f, false).expect("profile");
        let pv = crate::civil::get::<crate::civil::ProfileViewData>(&app.doc, view, crate::civil::PROFILE_VIEW).unwrap();
        assert!(!pv.cloud[0].points.is_empty());
        // The scenes draw them, and an old document without the fields still loads.
        app.civil_build_now();
        assert!(app.civil.built[&group].0.len() > 0);
        let old = serde_json::json!({"alignment": "A", "profiles": [], "show_design": true, "vertical_exaggeration": 1.0, "datum": null, "insertion": [0.0, 0.0], "show_bands": false, "band_height_m": 1.0});
        assert!(serde_json::from_value::<crate::civil::ProfileViewData>(old).is_ok());
        // A thickness of zero or none is refused.
        f.thickness.clear();
        assert!(app.cloud_do_slices(&f, false).is_err());
    }

    #[test]
    fn a_transformed_document_keeps_the_cloud_and_draws_its_points_through_the_plan() {
        let (_ctx, mut app) = demo_app();
        let (id, _) = cl::clouds(&app.doc).into_iter().next().unwrap();
        afrobim_kernel::set_project_crs(&mut app.doc, &afrobim_kernel::ProjectCrs::from_epsg(21036).unwrap());
        let from = afrobim_kernel::project_crs(&app.doc).unwrap();
        let to = afrobim_kernel::ProjectCrs::from_epsg(32736).unwrap();
        let ext = crate::transform_doc::extent_of(&app.doc, None).unwrap();
        let plan = afrobim_kernel::crs_plan::plan_transform(&from, &to, Some(afrobim_kernel::crs_plan::DataExtent::new(ext.0, ext.1))).unwrap().remove(0);
        crate::transform_doc::apply_plan(&mut app.doc, None, &plan, &to, "x", "t".into()).expect("a document with a cloud transforms");
        let d = cl::data_of(&app.doc, id).unwrap();
        assert_eq!(d.moved.len(), 1);
        let plans = cl::chain_plans(&d);
        assert_eq!(plans.len(), 1);
        let (e, n) = cl::map_chain(&plans, 450_100.0, 9_960_100.0, 1150.0).unwrap();
        assert!((e - 450_100.0).hypot(n - 9_960_100.0) > 50.0, "the points move with the document");
        // The footprint was moved by the rule, the file's header was not.
        assert_ne!(d.extent.unwrap()[0], cl::header_extent(&d.reference.header).unwrap()[0]);
    }

    #[test]
    fn the_cloud_draws_in_3d_with_a_budget_when_the_camera_looks_at_it() {
        let (_ctx, mut app) = demo_app();
        let (id, _) = cl::clouds(&app.doc).into_iter().next().unwrap();
        let e = app.doc.find(id).unwrap().clone();
        let site = app.site_mode();
        let frame = app.frame_of(&e, site).expect("a frame");
        let pts = app.element_points(&e, site);
        assert_eq!(pts.len(), 2, "the cloud frames the view with its box");
        let centre = [(pts[0][0] + pts[1][0]) / 2.0, (pts[0][1] + pts[1][1]) / 2.0, (pts[0][2] + pts[1][2]) / 2.0];
        app.camera.fit_sphere(centre, 250_000.0);
        let mut d = cl::data_of(&app.doc, id).unwrap();
        d.reference.display.max_points_per_view = 3000;
        cl::store(&mut app.doc, id, &d);
        let mut items = Vec::new();
        app.cloud_points_3d(&e, frame, egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0)), &mut items);
        let n: usize = items.iter().map(|i| i.1.len()).sum();
        assert!(n > 500 && n <= 3000, "{n} points drawn");
    }
}
