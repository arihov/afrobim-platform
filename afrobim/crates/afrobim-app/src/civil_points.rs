//! COGO points in the application: import, create, the point table, point groups, description
//! keys, export, the COGO tools and traverse entry.
//!
//! One element holds a whole point set (the store, its groups and its description keys). With 100 000
//! points that is one 20 MB JSON value; the alternative, one element per point, would make the
//! document, the undo history and the per-frame change check 100 000 times heavier. The point table
//! works on a copy and writes it back when applied or closed, and the canvas draws from a scene
//! culled to the view (`civil_scene`). A set can be split into several sets by importing into
//! different targets.

use std::path::PathBuf;

use afrobim_cogo::{
    detect_format, export_points, import_into, parse_points, CogoPoint, ColumnOrder, Delimiter, DetectionReport, DuplicatePolicy, ExportOptions, PointFileFormat, PointGroup, Rule, Xy,
};
use afrobim_cogo::pointfile::ParsedPoints;
use afrobim_kernel::{project_crs, ElementId, PlanExtent, SelectMode};
use egui::{RichText, Ui};

use crate::app::AfroBimApp;
use crate::civil::{self, Bounds, PointsData, POINTS};
use crate::civil_dlg::{self as d, fmt};
use crate::ui_kit::{self, Col, Dialog, Footer, Level, NumRule};

pub const UNITS: [(&str, f64); 3] = [("Metres", 1.0), ("International feet", 0.3048), ("US survey feet", 1200.0 / 3937.0)];
const DELIMS: [(&str, Delimiter); 4] = [("Comma", Delimiter::Comma), ("Tab", Delimiter::Tab), ("Semicolon", Delimiter::Semicolon), ("Space", Delimiter::Space)];
const POLICIES: [(&str, DuplicatePolicy); 3] = [
    ("Stop at the first clash and change nothing", DuplicatePolicy::Reject),
    ("Skip points whose number is taken", DuplicatePolicy::Skip),
    ("Give clashing points the next free number", DuplicatePolicy::RenumberToFree),
];

#[derive(Default)]
pub struct PointsUi {
    pub import: Option<ImportForm>,
    pub create: Option<CreateForm>,
    pub table: Option<TableState>,
    pub groups: Option<GroupsState>,
    pub keys: Option<KeysState>,
    pub export: Option<ExportForm>,
    pub cogo: Option<CogoForm>,
    pub traverse: Option<TraverseForm>,
}

// ─── shared ────────────────────────────────────────────────────────────────

impl AfroBimApp {
    pub(crate) fn points_data(&self, id: ElementId) -> Option<PointsData> {
        civil::get::<PointsData>(&self.doc, id, POINTS)
    }

    fn data_bounds(data: &PointsData) -> Option<Bounds> {
        Bounds::of(data.store.iter().map(|p| (p.easting, p.northing)))
    }

    /// Writes a point set back, at its source coordinates.
    pub(crate) fn set_points(&mut self, id: ElementId, mut data: PointsData) {
        data.groups.refresh(&data.store);
        let b = Self::data_bounds(&data);
        if let Ok(v) = serde_json::to_value(&data) {
            civil::set(&mut self.doc, id, v, b);
        }
        self.schedule_commit_now();
    }

    pub(crate) fn new_points(&mut self, name: &str, data: PointsData, source: &str) -> ElementId {
        let b = Self::data_bounds(&data);
        let v = serde_json::to_value(&data).unwrap_or_default();
        let id = civil::add(&mut self.doc, POINTS, name, v, b, source);
        self.schedule_commit_now();
        id
    }

    pub(crate) fn point_sets(&self) -> Vec<(ElementId, String)> {
        civil::of_category(&self.doc, POINTS)
    }

    fn selected_points(&self) -> Option<ElementId> {
        self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == POINTS)).or_else(|| self.point_sets().first().map(|p| p.0))
    }

    pub(crate) fn civil_points_cmd(&mut self, cv: crate::civil_cmds::Cv) {
        use crate::civil_cmds::Cv::*;
        let sets = self.point_sets();
        let sel = self.selected_points();
        match cv {
            Points => self.open_import_points(None),
            PointCreate => self.civil_ui.points.create = Some(CreateForm::new(sel)),
            PointTable | PointGroups | DescKeys | PointExport if sets.is_empty() => self.status = "There are no points yet. Import a point file (POINTS) or create a point (POINTCREATE) first.".into(),
            PointTable => {
                let id = sel.unwrap();
                if let Some(data) = self.points_data(id) {
                    self.civil_ui.points.table = Some(TableState::new(id, data, self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default()));
                }
            }
            PointGroups => self.civil_ui.points.groups = sel.and_then(|id| self.points_data(id).map(|d| GroupsState { id, data: d, selected: 0, new_name: String::new(), message: String::new() })),
            DescKeys => self.civil_ui.points.keys = sel.and_then(|id| self.points_data(id).map(|d| KeysState { id, data: d, message: String::new() })),
            PointExport => self.civil_ui.points.export = Some(ExportForm::new(sel.unwrap())),
            Cogo => self.civil_ui.points.cogo = Some(CogoForm::default()),
            Traverse => self.civil_ui.points.traverse = Some(TraverseForm::default()),
            _ => {}
        }
    }

    pub(crate) fn open_import_points(&mut self, path: Option<PathBuf>) {
        let path = match path.or_else(|| rfd::FileDialog::new().add_filter("Point files", &["csv", "txt", "pnt", "asc", "xyz", "pts"]).add_filter("All files", &["*"]).pick_file()) {
            Some(p) => p,
            None => return,
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let sel = self.selected_points();
                self.civil_ui.points.import = Some(ImportForm::new(path, text, sel.and_then(|id| self.point_sets().iter().position(|s| s.0 == id)).map(|i| i + 1).unwrap_or(0)));
            }
            Err(e) => self.status = format!("The point file cannot be read: {e}"),
        }
    }

    /// The click of the pick mode: easting and northing of the model point.
    pub(crate) fn civil_pick_press(&mut self, screen: afrobim_kernel::Pt) -> bool {
        use crate::civil_ui::PickFor;
        let Some(kind) = self.civil_ui.pick.take() else { return false };
        let w = self.view.to_world(screen);
        let (e, n) = (w.x / 1000.0, w.y / 1000.0);
        match kind {
            PickFor::Create => {
                let surface = self.civil_ui.points.create.as_ref().and_then(|f| f.surface);
                let z = surface.and_then(|sid| self.tin_for(sid)).and_then(|t| t.elevation_at(e, n));
                if let Some(f) = &mut self.civil_ui.points.create {
                    f.e = format!("{e:.3}");
                    f.n = format!("{n:.3}");
                    if let Some(z) = z {
                        f.z = format!("{z:.3}");
                    }
                }
            }
            PickFor::Spot => {
                if let Some(f) = &mut self.civil_ui.surf.spot {
                    f.set(e, n);
                }
            }
            PickFor::Flow => {
                if let Some(f) = &mut self.civil_ui.surf.flow {
                    f.set(e, n);
                }
            }
            PickFor::ParcelCorner => {
                self.parcel_pick_vertex(e, n);
                // Corners are picked one after another until the dialog closes.
                self.civil_ui.pick = Some(PickFor::ParcelCorner);
            }
        }
        true
    }

    pub(crate) fn civil_points_windows(&mut self, ctx: &egui::Context) {
        self.import_points_window(ctx);
        self.create_point_window(ctx);
        self.point_table_window(ctx);
        self.groups_window(ctx);
        self.keys_window(ctx);
        self.export_points_window(ctx);
        self.cogo_window(ctx);
        self.traverse_window(ctx);
    }
}

fn target_combo(ui: &mut Ui, id: &str, sets: &[(ElementId, String)], target: &mut usize) {
    let mut items = vec!["A new point set".to_string()];
    items.extend(sets.iter().map(|s| s.1.clone()));
    ui_kit::combo_owned(ui, id, target, &items);
}

// ─── import ────────────────────────────────────────────────────────────────

pub struct ImportForm {
    path: PathBuf,
    file: String,
    text: String,
    report: DetectionReport,
    order: usize,
    delim: usize,
    unit: usize,
    policy: usize,
    target: usize,
    name: String,
    parsed: Result<ParsedPoints, String>,
    key: (usize, usize, usize),
    findings: Vec<afrobim_kernel::CrsFinding>,
}

impl ImportForm {
    fn new(path: PathBuf, text: String, target: usize) -> ImportForm {
        let report = detect_format(&text, 12);
        let order = report.best.as_ref().and_then(|f| ColumnOrder::ALL.iter().position(|o| *o == f.order)).unwrap_or(0);
        let delim = DELIMS.iter().position(|d| d.1 == report.delimiter).unwrap_or(0);
        let file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let name = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Points".into());
        let mut f = ImportForm { path, file, text, report, order, delim, unit: 0, policy: 0, target, name, parsed: Err(String::new()), key: (usize::MAX, 0, 0), findings: vec![] };
        f.reparse(None);
        f
    }

    fn format(&self) -> PointFileFormat {
        PointFileFormat { order: ColumnOrder::ALL[self.order], delimiter: DELIMS[self.delim].1, unit_factor: UNITS[self.unit].1, missing_elevation: None, start_number: 1 }
    }

    fn reparse(&mut self, project: Option<&afrobim_kernel::ProjectCrs>) {
        self.key = (self.order, self.delim, self.unit);
        self.parsed = parse_points(&self.text, &self.format(), Some(&self.file)).map_err(|e| e.to_string());
        self.findings = match &self.parsed {
            Ok(p) if !p.points.is_empty() => {
                let b = Bounds::of(p.points.iter().map(|q| (q.easting, q.northing))).unwrap();
                let extent = PlanExtent { min_easting: b.min_e, min_northing: b.min_n, max_easting: b.max_e, max_northing: b.max_n };
                crate::crs_ui::dataset_check(project, &self.file, None, Some(extent))
            }
            _ => vec![],
        };
    }
}

impl AfroBimApp {
    fn import_points_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.points.import.take() else { return };
        let project = project_crs(&self.doc);
        if f.key != (f.order, f.delim, f.unit) {
            f.reparse(project.as_ref());
        } else if f.findings.is_empty() && f.parsed.is_ok() && f.key.0 != usize::MAX && !f.findings_checked() {
            f.reparse(project.as_ref());
        }
        let sets = self.point_sets();
        let ok = f.parsed.as_ref().is_ok_and(|p| !p.points.is_empty());
        let out = Dialog::new("Import points", crate::glyphs::by_name("POINTS"))
            .id_title(format!("Import points: {}", f.file))
            .subtitle(f.file.clone())
            .width(680.0)
            .help("The file's format is detected and shown as it would be read: check the preview by eye. The order of the columns cannot be known from the numbers alone; when two orders fit, the dialog says so and you choose. Coordinates are kept exactly as given and nothing is reprojected.")
            .show(
                ctx,
                |ui| {
                    ui_kit::section(ui, "Format", |ui| {
                        let r = &f.report;
                        let level = if r.ambiguous { Level::Warning } else { Level::Info };
                        let msg = match &r.best {
                            Some(b) => format!("Detected order {} with {:.0} % confidence{}.", b.order.code(), r.confidence * 100.0, if r.ambiguous { "; two orders fit about equally well, so choose by looking at the preview" } else { "" }),
                            None => "The format could not be detected. Choose the order of the columns.".to_string(),
                        };
                        ui_kit::callout(ui, level, msg);
                        for n in &r.notes {
                            ui.label(RichText::new(n).small().weak());
                        }
                        ui_kit::form(ui, "pf", |fm| {
                            fm.row("Column order", |ui| {
                                let items: Vec<&str> = ColumnOrder::ALL.iter().map(|o| o.code()).collect();
                                ui_kit::combo(ui, "order", &mut f.order, &items);
                            });
                            fm.row("Delimiter", |ui| {
                                let items: Vec<&str> = DELIMS.iter().map(|d| d.0).collect();
                                ui_kit::combo(ui, "delim", &mut f.delim, &items);
                            });
                            fm.row("File unit", |ui| {
                                let items: Vec<&str> = UNITS.iter().map(|d| d.0).collect();
                                ui_kit::combo(ui, "unit", &mut f.unit, &items);
                            });
                        });
                    });
                    ui_kit::section(ui, "Preview", |ui| match &f.parsed {
                        Ok(p) => {
                            ui.label(format!("{} point(s) read, {} line(s) with a problem{}.", p.points.len(), p.issues.len(), if p.header_skipped { ", a heading line skipped" } else { "" }));
                            let rows: Vec<Vec<String>> = p.points.iter().take(8).map(|q| vec![q.number.to_string(), fmt(q.easting, 3), fmt(q.northing, 3), q.elevation.map(|z| fmt(z, 3)).unwrap_or_default(), q.raw_description.clone()]).collect();
                            ui_kit::results_table(ui, "pf-prev", &["No.", "Easting", "Northing", "Elevation", "Description"], &[Col::Num, Col::Num, Col::Num, Col::Num, Col::Text], &rows);
                            for i in p.issues.iter().take(3) {
                                ui.label(RichText::new(format!("Line {}: {}", i.line, i.message)).small().color(crate::theme::warn()));
                            }
                        }
                        Err(e) => { ui_kit::callout(ui, Level::Error, e.clone()); }
                    });
                    ui_kit::section(ui, "Coordinate system check", |ui| {
                        if f.findings.is_empty() {
                            ui.label(RichText::new("Nothing to check yet.").weak());
                        }
                        for fd in &f.findings {
                            ui_kit::callout(ui, if fd.warning { Level::Warning } else { Level::Info }, fd.message.clone());
                        }
                    });
                    ui_kit::section(ui, "Into", |ui| {
                        ui_kit::form(ui, "pt-into", |fm| {
                            fm.row("Point set", |ui| target_combo(ui, "target", &sets, &mut f.target));
                            if f.target == 0 {
                                fm.row("Name", |ui| d::text(ui, &mut f.name, "Name of the new set"));
                            }
                            fm.row("Duplicate numbers", |ui| {
                                let items: Vec<&str> = POLICIES.iter().map(|d| d.0).collect();
                                ui_kit::radio_group(ui, &mut f.policy, &items);
                            });
                        });
                    });
                },
                |ui| Footer::new("Import points").icon(crate::glyphs::by_name("POINTS")).enabled(ok, "No point can be read in this format").show(ui),
            );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            self.finish_import_points(&f, &sets);
            return;
        }
        self.civil_ui.points.import = Some(f);
    }

    fn finish_import_points(&mut self, f: &ImportForm, sets: &[(ElementId, String)]) {
        let Ok(parsed) = &f.parsed else { return };
        let (id, mut data) = if f.target > 0 {
            let id = sets[f.target - 1].0;
            (Some(id), self.points_data(id).unwrap_or_default())
        } else {
            (None, PointsData::default())
        };
        match import_into(&mut data.store, parsed.points.clone(), POLICIES[f.policy].1) {
            Ok(r) => {
                let n = r.added.len();
                let id = match id {
                    Some(id) => {
                        self.set_points(id, data);
                        id
                    }
                    None => {
                        let id = self.new_points(&f.name, data, &f.file);
                        id
                    }
                };
                self.doc.select(&[id], SelectMode::Replace);
                self.status = format!("Imported {n} point(s) from {} ({} skipped, {} renumbered).", f.file, r.skipped.len(), r.renumbered.len());
                self.log(format!("POINTS: {}", self.status));
                self.sync_generated();
                self.zoom_to_element(id);
            }
            Err(e) => {
                self.status = format!("Nothing was imported: {e}");
                self.civil_ui.points.import = None;
            }
        }
    }
}

impl ImportForm {
    fn findings_checked(&self) -> bool {
        true
    }
}

// ─── create ────────────────────────────────────────────────────────────────

pub struct CreateForm {
    target: usize,
    name: String,
    e: String,
    n: String,
    z: String,
    desc: String,
    number: String,
    surface: Option<ElementId>,
    message: String,
}

impl CreateForm {
    fn new(sel: Option<ElementId>) -> CreateForm {
        CreateForm { target: 0, name: "Points".into(), e: String::new(), n: String::new(), z: String::new(), desc: String::new(), number: String::new(), surface: None, message: String::new() }.with_target(sel)
    }
    fn with_target(mut self, _sel: Option<ElementId>) -> CreateForm {
        self.target = 0;
        self
    }
}

impl AfroBimApp {
    fn create_point_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.points.create.take() else { return };
        let sets = self.point_sets();
        if f.target == 0 && !sets.is_empty() && f.message.is_empty() {
            f.target = self.selected_points().and_then(|id| sets.iter().position(|s| s.0 == id)).map(|i| i + 1).unwrap_or(1);
        }
        let vals: std::cell::Cell<(Option<f64>, Option<f64>, Option<f64>)> = std::cell::Cell::new((None, None, None));
        let mut pick = false;
        let surfaces: Vec<(ElementId, String)> = civil::surface_choices(&self.doc);
        let out = Dialog::new("Create point", crate::glyphs::by_name("POINTCREATE"))
            .width(480.0)
            .help("Type the easting, northing and elevation, or press Pick and click in the model. A picked point on a chosen surface takes that surface's elevation. The number is the next free one unless you type one.")
            .show(
                ctx,
                |ui| {
                    ui_kit::form(ui, "pc", |fm| {
                        fm.row("Point set", |ui| target_combo(ui, "pc-target", &sets, &mut f.target));
                        if f.target == 0 {
                            fm.row("Name", |ui| d::text(ui, &mut f.name, "Name of the new set"));
                        }
                        fm.row("Easting", |ui| vals.set((d::num(ui, &mut f.e, "m"), vals.get().1, vals.get().2)));
                        fm.row("Northing", |ui| vals.set((vals.get().0, d::num(ui, &mut f.n, "m"), vals.get().2)));
                        fm.row_note("Elevation", "Leave blank for a point with no elevation", |ui| vals.set((vals.get().0, vals.get().1, d::num_opt(ui, &mut f.z, "m").ok().flatten())));
                        fm.row("Description", |ui| d::text(ui, &mut f.desc, "Field code, e.g. EP"));
                        fm.row_note("Number", "Blank takes the next free number", |ui| d::text(ui, &mut f.number, ""));
                        fm.row("Level from surface", |ui| {
                            let mut sel = f.surface;
                            d::pick_from(ui, "pc-surf", &surfaces, &mut sel);
                            f.surface = sel;
                        });
                        fm.row("", |ui| {
                            if ui.button(format!("{}  Pick in the model", egui_phosphor::regular::CROSSHAIR)).clicked() {
                                pick = true;
                            }
                        });
                    });
                    if !f.message.is_empty() {
                        ui.label(RichText::new(&f.message).small());
                    }
                },
                |ui| Footer::new("Add point").icon(crate::glyphs::by_name("POINTCREATE")).cancel(Some("Close")).enabled(vals.get().0.is_some() && vals.get().1.is_some(), "Type an easting and a northing").show(ui),
            );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            self.civil_ui.pick = None;
            return;
        }
        if pick {
            self.civil_ui.pick = Some(crate::civil_ui::PickFor::Create);
            self.status = "Click the point in the model.".into();
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            let vals = vals.get();
            if let (Some(e), Some(n)) = (vals.0, vals.1) {
                let z = vals.2.or_else(|| f.surface.and_then(|s| self.civil.tin_of(s)).and_then(|t| t.elevation_at(e, n)));
                let (id, mut data) = if f.target > 0 {
                    let id = sets[f.target - 1].0;
                    (Some(id), self.points_data(id).unwrap_or_default())
                } else {
                    (None, PointsData::default())
                };
                let p = CogoPoint::new(f.number.trim().parse().unwrap_or(0), e, n, z, f.desc.trim());
                let added = match f.number.trim().parse::<u32>() {
                    Ok(num) => data.store.add(CogoPoint { number: num, ..p }).map(|_| num),
                    Err(_) => data.store.add_next_free(CogoPoint { number: 1, ..p }),
                };
                match added {
                    Ok(num) => {
                        let id = match id {
                            Some(id) => {
                                self.set_points(id, data);
                                id
                            }
                            None => self.new_points(&f.name.clone(), data, "Typed or picked"),
                        };
                        f.message = format!("Added point {num}.");
                        f.number.clear();
                        if f.target == 0 {
                            f.target = self.point_sets().iter().position(|s| s.0 == id).map(|i| i + 1).unwrap_or(0);
                        }
                        self.status = f.message.clone();
                    }
                    Err(x) => f.message = format!("Not added: {x}"),
                }
            }
        }
        self.civil_ui.points.create = Some(f);
    }
}

// ─── point table ───────────────────────────────────────────────────────────

pub struct TableState {
    id: ElementId,
    name: String,
    data: PointsData,
    order: Vec<u32>,
    key: (usize, bool, String, u64),
    sort: usize,
    asc: bool,
    filter: String,
    dirty: bool,
    edit: Option<(u32, usize, String)>,
    selected: Option<u32>,
    message: String,
}

const COLS: [&str; 7] = ["No.", "Easting", "Northing", "Elevation", "Description", "Full description", "Locked"];

impl TableState {
    fn new(id: ElementId, data: PointsData, name: String) -> TableState {
        TableState { id, name, data, order: vec![], key: (usize::MAX, true, String::new(), 0), sort: 0, asc: true, filter: String::new(), dirty: false, edit: None, selected: None, message: String::new() }
    }

    fn refresh(&mut self) {
        let key = (self.sort, self.asc, self.filter.clone(), self.data.store.revision());
        if key == self.key {
            return;
        }
        self.key = key;
        let f = self.filter.trim().to_lowercase();
        let mut pts: Vec<&CogoPoint> = self
            .data
            .store
            .iter()
            .filter(|p| f.is_empty() || p.number.to_string() == f || p.raw_description.to_lowercase().contains(&f) || p.full_description.to_lowercase().contains(&f))
            .collect();
        let s = self.sort;
        pts.sort_by(|a, b| {
            let o = match s {
                1 => a.easting.total_cmp(&b.easting),
                2 => a.northing.total_cmp(&b.northing),
                3 => a.elevation.unwrap_or(f64::NEG_INFINITY).total_cmp(&b.elevation.unwrap_or(f64::NEG_INFINITY)),
                4 => a.raw_description.cmp(&b.raw_description),
                5 => a.full_description.cmp(&b.full_description),
                6 => a.locked.cmp(&b.locked),
                _ => a.number.cmp(&b.number),
            };
            if self.asc {
                o
            } else {
                o.reverse()
            }
        });
        self.order = pts.iter().map(|p| p.number).collect();
    }

    fn cell(p: &CogoPoint, c: usize) -> String {
        match c {
            0 => p.number.to_string(),
            1 => format!("{:.4}", p.easting),
            2 => format!("{:.4}", p.northing),
            3 => p.elevation.map(|z| format!("{z:.4}")).unwrap_or_default(),
            4 => p.raw_description.clone(),
            5 => p.full_description.clone(),
            _ => if p.locked { "yes" } else { "" }.to_string(),
        }
    }

    /// Applies an edited cell. Returns a problem in words.
    fn commit(&mut self, number: u32, col: usize, text: &str) -> Result<(), String> {
        let p = self.data.store.get(number).cloned().ok_or("That point no longer exists.")?;
        let t = text.trim();
        let num = |t: &str| t.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("'{t}' is not a number."));
        match col {
            0 => {
                let to: u32 = t.parse().map_err(|_| format!("'{t}' is not a whole point number."))?;
                self.data.store.renumber(number, to).map_err(|e| e.to_string())?;
            }
            1 => self.data.store.move_point(number, num(t)?, p.northing, p.elevation, "edited in the point table").map_err(|e| e.to_string())?,
            2 => self.data.store.move_point(number, p.easting, num(t)?, p.elevation, "edited in the point table").map_err(|e| e.to_string())?,
            3 => {
                let z = if t.is_empty() { None } else { Some(num(t)?) };
                self.data.store.move_point(number, p.easting, p.northing, z, "edited in the point table").map_err(|e| e.to_string())?
            }
            4 => self.data.store.edit(number, |p| p.raw_description = t.to_string()).map_err(|e| e.to_string())?,
            5 => self.data.store.edit(number, |p| p.full_description = t.to_string()).map_err(|e| e.to_string())?,
            _ => {
                let lock = !p.locked;
                self.data.store.set_locked(number, lock).map_err(|e| e.to_string())?
            }
        }
        self.dirty = true;
        Ok(())
    }
}

impl AfroBimApp {
    fn point_table_window(&mut self, ctx: &egui::Context) {
        let Some(mut t) = self.civil_ui.points.table.take() else { return };
        t.refresh();
        let total = t.data.store.len();
        let mut apply = false;
        let mut add = false;
        let mut del = false;
        let title = format!("Point table: {}", t.name);
        let out = Dialog::new("Point table", crate::glyphs::by_name("POINTTABLE")).id_title(title).subtitle(t.name.clone()).width(820.0).resizable(true).max_body_height(560.0).show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label("Filter");
                    ui_kit::text_field(ui, &mut t.filter, "number or text in the description");
                    ui.label(RichText::new(format!("{} of {total} point(s) shown", t.order.len())).small().weak());
                });
                ui.horizontal(|ui| {
                    if ui.button(format!("{}  Add point", egui_phosphor::regular::PLUS)).clicked() {
                        add = true;
                    }
                    if ui.add_enabled(t.selected.is_some(), egui::Button::new(format!("{}  Delete selected", egui_phosphor::regular::TRASH_SIMPLE))).clicked() {
                        del = true;
                    }
                    if ui.add_enabled(t.dirty, egui::Button::new(format!("{}  Apply to the document", egui_phosphor::regular::CHECK))).clicked() {
                        apply = true;
                    }
                    ui.label(RichText::new(if t.dirty { "Edited, not applied yet" } else { "Up to date" }).small().weak());
                });
                if !t.message.is_empty() {
                    ui.label(RichText::new(&t.message).small().color(crate::theme::warn()));
                }
                ui.separator();
                let widths = [60.0, 110.0, 110.0, 80.0, 140.0, 180.0, 50.0];
                ui.horizontal(|ui| {
                    for (i, c) in COLS.iter().enumerate() {
                        let arrow = if t.sort == i { if t.asc { " ▲" } else { " ▼" } } else { "" };
                        let (rect, r) = ui.allocate_exact_size(egui::vec2(widths[i], 20.0), egui::Sense::click());
                        ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, format!("{c}{arrow}"), egui::FontId::proportional(12.0), ui.visuals().strong_text_color());
                        if r.clicked() {
                            if t.sort == i {
                                t.asc = !t.asc;
                            } else {
                                t.sort = i;
                                t.asc = true;
                            }
                        }
                    }
                });
                let row_h = 22.0;
                let n = t.order.len();
                egui::ScrollArea::vertical().id_salt("pt-rows").max_height(440.0).auto_shrink([false, false]).show_rows(ui, row_h, n, |ui, range| {
                    for i in range {
                        let number = t.order[i];
                        let Some(p) = t.data.store.get(number).cloned() else { continue };
                        ui.horizontal(|ui| {
                            for (c, w) in widths.iter().enumerate() {
                                let editing = t.edit.as_ref().is_some_and(|(n, col, _)| *n == number && *col == c);
                                if editing {
                                    let mut done = None;
                                    if let Some((_, _, buf)) = &mut t.edit {
                                        let r = ui.add_sized([*w, 20.0], egui::TextEdit::singleline(buf));
                                        r.request_focus();
                                        if r.lost_focus() {
                                            done = Some(!ui.input(|i| i.key_pressed(egui::Key::Escape)));
                                        }
                                    }
                                    if let Some(save) = done {
                                        let (n, col, buf) = t.edit.take().unwrap();
                                        if save {
                                            if let Err(e) = t.commit(n, col, &buf) {
                                                t.message = e;
                                            } else {
                                                t.message.clear();
                                            }
                                        }
                                    }
                                } else {
                                    let text = TableState::cell(&p, c);
                                    let sel = t.selected == Some(number);
                                    let r = table_cell(ui, *w, &text, sel);
                                    if r.clicked() {
                                        t.selected = Some(number);
                                        if c == 6 {
                                            if let Err(e) = t.commit(number, 6, "") {
                                                t.message = e;
                                            }
                                        }
                                    }
                                    if r.double_clicked() && c != 6 {
                                        t.edit = Some((number, c, text));
                                    }
                                }
                            }
                        });
                    }
                });
            },
            |ui| Footer::new("Apply and close").icon(crate::glyphs::by_name("POINTTABLE")).cancel(Some("Close without applying")).show(ui),
        );
        if add {
            if let Ok(n) = t.data.store.add_next_free(CogoPoint::new(1, 0.0, 0.0, None, "")) {
                t.selected = Some(n);
                t.dirty = true;
                t.message = format!("Added point {n} at 0, 0: edit its coordinates.");
            }
        }
        if del {
            if let Some(n) = t.selected.take() {
                if let Err(e) = t.data.store.remove(n, "deleted in the point table") {
                    t.message = e.to_string();
                } else {
                    t.dirty = true;
                }
            }
        }
        if apply || out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            if t.dirty {
                self.set_points(t.id, t.data.clone());
                t.dirty = false;
                self.status = format!("Applied the edits to {}.", t.name);
            }
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel || fo.primary) {
            return;
        }
        self.civil_ui.points.table = Some(t);
    }
}

// ─── point groups ──────────────────────────────────────────────────────────

pub struct GroupsState {
    id: ElementId,
    data: PointsData,
    selected: usize,
    new_name: String,
    message: String,
}

fn rule_ui(ui: &mut Ui, r: &mut Rule, salt: &str) -> bool {
    let mut remove = false;
    ui.horizontal(|ui| {
        let kinds = ["Number range", "Name", "Raw description", "Elevation range"];
        let mut k = match r {
            Rule::NumberRange { .. } => 0,
            Rule::Name { .. } => 1,
            Rule::RawDescription { .. } => 2,
            Rule::ElevationRange { .. } => 3,
        };
        if ui_kit::combo(ui, &format!("rk-{salt}"), &mut k, &kinds) {
            *r = match k {
                0 => Rule::NumberRange { from: 1, to: 1 },
                1 => Rule::Name { pattern: "*".into() },
                2 => Rule::RawDescription { pattern: "*".into() },
                _ => Rule::ElevationRange { min: 0.0, max: 0.0 },
            };
        }
        match r {
            Rule::NumberRange { from, to } => {
                ui.add(egui::DragValue::new(from));
                ui.label("to");
                ui.add(egui::DragValue::new(to));
            }
            Rule::Name { pattern } | Rule::RawDescription { pattern } => {
                ui.add(egui::TextEdit::singleline(pattern).desired_width(150.0));
            }
            Rule::ElevationRange { min, max } => {
                ui.add(egui::DragValue::new(min).speed(0.1));
                ui.label("to");
                ui.add(egui::DragValue::new(max).speed(0.1));
            }
        }
        if ui.small_button("x").clicked() {
            remove = true;
        }
    });
    remove
}

impl AfroBimApp {
    fn groups_window(&mut self, ctx: &egui::Context) {
        let Some(mut g) = self.civil_ui.points.groups.take() else { return };
        let mut apply = false;
        let names: Vec<String> = g.data.groups.groups().iter().map(|x| x.name.clone()).collect();
        g.selected = g.selected.min(names.len().saturating_sub(1));
        let store = g.data.store.clone();
        let out = Dialog::new("Point groups", crate::glyphs::by_name("POINTGROUPS")).width(620.0).help("A point group takes the points that match its include rules and not its exclude rules. Wildcards: * for any text, ? for one character. A group marked as surface points is what a surface definition uses.").show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label("Groups");
                    ui_kit::combo_owned(ui, "grp-sel", &mut g.selected, &names);
                    ui_kit::text_field(ui, &mut g.new_name, "New group name");
                    if ui.add_enabled(!g.new_name.trim().is_empty(), egui::Button::new("Add group")).clicked() {
                        match g.data.groups.add(PointGroup::new(g.new_name.trim())) {
                            Ok(()) => {
                                g.new_name.clear();
                                g.selected = g.data.groups.groups().len() - 1;
                                g.message.clear();
                            }
                            Err(e) => g.message = e.to_string(),
                        }
                    }
                });
                if !g.message.is_empty() {
                    ui.label(RichText::new(&g.message).small().color(crate::theme::warn()));
                }
                let Some(name) = names.get(g.selected).cloned() else { return };
                let is_all = name == afrobim_cogo::ALL_POINTS;
                let members = g.data.groups.members_of(&store, &name).map(|m| m.len()).unwrap_or(0);
                ui.label(RichText::new(format!("{name}: {members} point(s)")).strong());
                if is_all {
                    ui.label(RichText::new("The built-in group of every point cannot be edited.").small().weak());
                    return;
                }
                let mut edit: Option<PointGroup> = g.data.groups.get(&name).cloned();
                let mut remove = false;
                let mut changed = false;
                if let Some(grp) = &mut edit {
                    changed |= ui.checkbox(&mut grp.include_all, "Include every point").changed();
                    changed |= ui.checkbox(&mut grp.surface_points, "Surface points (used by surface definitions)").changed();
                    ui.label("Include rules");
                    let mut i = 0;
                    while i < grp.include.len() {
                        if rule_ui(ui, &mut grp.include[i], &format!("i{i}")) {
                            grp.include.remove(i);
                        } else {
                            i += 1;
                        }
                        changed = true;
                    }
                    if ui.small_button("Add include rule").clicked() {
                        grp.include.push(Rule::NumberRange { from: 1, to: 1 });
                        changed = true;
                    }
                    ui.label("Exclude rules");
                    let mut i = 0;
                    while i < grp.exclude.len() {
                        if rule_ui(ui, &mut grp.exclude[i], &format!("e{i}")) {
                            grp.exclude.remove(i);
                        } else {
                            i += 1;
                        }
                        changed = true;
                    }
                    if ui.small_button("Add exclude rule").clicked() {
                        grp.exclude.push(Rule::NumberRange { from: 1, to: 1 });
                        changed = true;
                    }
                    ui.horizontal(|ui| {
                        if ui.small_button("Move up").clicked() && g.selected > 0 {
                            let _ = g.data.groups.move_to(&name, g.selected - 1);
                            g.selected -= 1;
                        }
                        if ui.small_button("Move down").clicked() {
                            let _ = g.data.groups.move_to(&name, g.selected + 1);
                            g.selected += 1;
                        }
                        if ui.button(RichText::new("Delete group").color(crate::theme::danger())).clicked() {
                            remove = true;
                        }
                    });
                }
                if remove {
                    let _ = g.data.groups.remove(&name);
                    g.selected = 0;
                } else if let (true, Some(grp)) = (changed, edit) {
                    let _ = g.data.groups.edit(&name, |x| *x = grp);
                }
                apply = true;
            },
            |ui| Footer::new("Apply").icon(crate::glyphs::by_name("POINTGROUPS")).cancel(Some("Close without applying")).show(ui),
        );
        let _ = apply;
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            self.set_points(g.id, g.data.clone());
            self.status = "Point groups applied.".into();
            return;
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        self.civil_ui.points.groups = Some(g);
    }
}

// ─── description keys ──────────────────────────────────────────────────────

pub struct KeysState {
    id: ElementId,
    data: PointsData,
    message: String,
}

impl AfroBimApp {
    fn keys_window(&mut self, ctx: &egui::Context) {
        let Some(mut k) = self.civil_ui.points.keys.take() else { return };
        let mut apply_now = false;
        let out = Dialog::new("Description keys", crate::glyphs::by_name("DESCKEYS")).width(900.0).resizable(true).help("A key matches the raw description of a point (TR* matches TREE and TR1; EP $1 takes the next word as a parameter) and gives it a full description, a point style, a label style and a layer. The classification columns are free text: they are not checked against any published scheme.").show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label("Set name");
                    ui_kit::text_field(ui, &mut k.data.keys.name, "");
                    ui.checkbox(&mut k.data.keys.case_sensitive, "Case sensitive");
                });
                egui::Grid::new("keys-grid").striped(true).num_columns(8).spacing([6.0, 4.0]).show(ui, |ui| {
                    for h in ["Code", "Full description", "Point style", "Label style", "Layer", "Class scheme", "Class code", ""] {
                        ui.label(RichText::new(h).small().strong());
                    }
                    ui.end_row();
                    let mut remove = None;
                    for (i, key) in k.data.keys.keys.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(&mut key.code).desired_width(70.0));
                        ui.add(egui::TextEdit::singleline(&mut key.full_description).desired_width(170.0));
                        ui.add(egui::TextEdit::singleline(&mut key.point_style).desired_width(90.0));
                        ui.add(egui::TextEdit::singleline(&mut key.label_style).desired_width(90.0));
                        ui.add(egui::TextEdit::singleline(&mut key.layer).desired_width(90.0));
                        let c = key.classification.get_or_insert_with(|| afrobim_cogo::Classification { scheme: String::new(), code: String::new(), title: None });
                        ui.add(egui::TextEdit::singleline(&mut c.scheme).desired_width(80.0));
                        ui.add(egui::TextEdit::singleline(&mut c.code).desired_width(80.0));
                        if ui.small_button("x").clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                    if let Some(i) = remove {
                        k.data.keys.keys.remove(i);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Add key").clicked() {
                        k.data.keys.keys.push(afrobim_cogo::DescriptionKey { code: String::new(), full_description: String::new(), point_style: String::new(), label_style: String::new(), layer: String::new(), classification: None });
                    }
                    if ui.button("Apply keys to the points").on_hover_text("Fills the full description, styles and layer of every point that matches a key").clicked() {
                        apply_now = true;
                    }
                });
                if !k.message.is_empty() {
                    ui.label(RichText::new(&k.message).small());
                }
            },
            |ui| Footer::new("Save keys").icon(crate::glyphs::by_name("DESCKEYS")).cancel(Some("Close without saving")).show(ui),
        );
        if apply_now {
            for key in &mut k.data.keys.keys {
                if key.classification.as_ref().is_some_and(|c| c.scheme.is_empty() && c.code.is_empty()) {
                    key.classification = None;
                }
            }
            let rep = k.data.keys.clone().apply(&mut k.data.store);
            k.message = format!("{} point(s) matched a key, {} did not, {} locked point(s) were left alone, {} warning(s).", rep.matched.len(), rep.unmatched.len(), rep.locked_skipped.len(), rep.warnings.len());
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            self.set_points(k.id, k.data.clone());
            self.status = "Description keys saved.".into();
            return;
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        self.civil_ui.points.keys = Some(k);
    }
}

// ─── export ────────────────────────────────────────────────────────────────

pub struct ExportForm {
    id: ElementId,
    order: usize,
    delim: usize,
    decimals: u8,
    group: usize,
    use_full: bool,
    header: bool,
    message: String,
}

impl ExportForm {
    fn new(id: ElementId) -> ExportForm {
        ExportForm { id, order: 0, delim: 0, decimals: 3, group: 0, use_full: false, header: true, message: String::new() }
    }
}

impl AfroBimApp {
    fn export_points_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.points.export.take() else { return };
        let sets = self.point_sets();
        let Some(data) = self.points_data(f.id) else { return };
        let groups: Vec<String> = data.groups.groups().iter().map(|g| g.name.clone()).collect();
        let mut set_index = sets.iter().position(|s| s.0 == f.id).unwrap_or(0);
        let out = Dialog::new("Export points", crate::glyphs::by_name("POINTEXPORT")).width(480.0).show(
            ctx,
            |ui| {
                ui_kit::form(ui, "pe", |fm| {
                    fm.row("Point set", |ui| {
                        let names: Vec<String> = sets.iter().map(|s| s.1.clone()).collect();
                        ui_kit::combo_owned(ui, "pe-set", &mut set_index, &names);
                    });
                    fm.row("Group", |ui| {
                        ui_kit::combo_owned(ui, "pe-grp", &mut f.group, &groups);
                    });
                    fm.row("Column order", |ui| {
                        let items: Vec<&str> = ColumnOrder::ALL.iter().map(|o| o.code()).collect();
                        ui_kit::combo(ui, "pe-order", &mut f.order, &items);
                    });
                    fm.row("Delimiter", |ui| {
                        let items: Vec<&str> = DELIMS.iter().map(|d| d.0).collect();
                        ui_kit::combo(ui, "pe-delim", &mut f.delim, &items);
                    });
                    fm.row("Decimal places", |ui| {
                        ui_kit::decimals_field(ui, &mut f.decimals, 0..=6);
                    });
                    fm.row("Description", |ui| {
                        ui.checkbox(&mut f.use_full, "Write the full description");
                    });
                    fm.row("Heading line", |ui| {
                        ui.checkbox(&mut f.header, "Write the column order as a heading");
                    });
                });
                if !f.message.is_empty() {
                    ui.label(RichText::new(&f.message).small());
                }
            },
            |ui| Footer::new("Export...").icon(crate::glyphs::by_name("POINTEXPORT")).cancel(Some("Close")).show(ui),
        );
        if let Some(s) = sets.get(set_index) {
            f.id = s.0;
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            let mut data = self.points_data(f.id).unwrap_or_default();
            let members = groups.get(f.group).and_then(|g| data.groups.members_of(&data.store, g).ok()).unwrap_or_default();
            let pts: Vec<&CogoPoint> = members.iter().filter_map(|n| data.store.get(*n)).collect();
            let opts = ExportOptions { order: ColumnOrder::ALL[f.order], delimiter: DELIMS[f.delim].1, decimals: f.decimals as usize, missing_elevation: Some(String::new()), header: f.header };
            match export_points(pts, &opts, f.use_full) {
                Ok(text) => {
                    if let Some(path) = rfd::FileDialog::new().set_file_name("points.csv").save_file() {
                        match std::fs::write(&path, text) {
                            Ok(()) => f.message = format!("Wrote {} point(s) to {}.", members.len(), path.display()),
                            Err(e) => f.message = format!("The file could not be written: {e}"),
                        }
                    }
                }
                Err(e) => f.message = format!("Not exported: {e}"),
            }
            self.status = f.message.clone();
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        self.civil_ui.points.export = Some(f);
    }
}

// ─── COGO tools ────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct PtIn {
    e: String,
    n: String,
    z: String,
}

fn pt_in(ui: &mut Ui, label: &str, p: &mut PtIn) -> Option<Xy> {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.label(label);
        let e = d::num(ui, &mut p.e, "E");
        let n = d::num(ui, &mut p.n, "N");
        out = match (e, n) {
            (Some(e), Some(n)) => Some(Xy::new(e, n)),
            _ => None,
        };
    });
    out
}

#[derive(Default)]
pub struct CogoForm {
    tab: usize,
    a: PtIn,
    b: PtIn,
    bearing1: String,
    bearing2: String,
    dist: String,
    r1: String,
    r2: String,
    kind: usize,
    alignment: Option<ElementId>,
    station: String,
    offset: String,
    target: usize,
    message: String,
}

impl AfroBimApp {
    fn cogo_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.points.cogo.take() else { return };
        let aligns = crate::alignment_glue::alignments(&self.doc);
        if f.alignment.is_none() {
            f.alignment = aligns.first().map(|a| a.0);
        }
        let sets = self.point_sets();
        let add: std::cell::Cell<Option<(f64, f64)>> = std::cell::Cell::new(None);
        let doc_align = f.alignment.and_then(|id| crate::alignment_glue::alignment_of(&self.doc, id));
        let out = Dialog::new("COGO tools", crate::glyphs::by_name("COGO")).width(620.0).help("Plane coordinate geometry on grid coordinates: no scale factor, convergence or curvature is applied. Bearings are whole-circle (degrees clockwise from grid north) or quadrant (N45-30-00E).").show(
            ctx,
            |ui| {
                ui_kit::tabs(ui, &mut f.tab, &["Inverse", "Bearing and distance", "Intersections", "Station and offset"]);
                match f.tab {
                    0 => {
                        let a = pt_in(ui, "From", &mut f.a);
                        let b = pt_in(ui, "To", &mut f.b);
                        if let (Some(a), Some(b)) = (a, b) {
                            let r = afrobim_cogo::inverse(a, None, b, None);
                            ui_kit::results_table(ui, "inv", &["Quantity", "Value"], &[Col::Text, Col::Num], &[vec!["Distance (m)".into(), fmt(r.distance, 4)], vec!["Bearing".into(), r.bearing_deg.map(|b| afrobim_cogo::format_bearing(b, 1)).unwrap_or_default()], vec!["Quadrant".into(), r.bearing_deg.map(|b| afrobim_cogo::format_quadrant(b, 1)).unwrap_or_default()], vec!["dE (m)".into(), fmt(b.e - a.e, 4)], vec!["dN (m)".into(), fmt(b.n - a.n, 4)]]);
                        }
                    }
                    1 => {
                        let a = pt_in(ui, "From", &mut f.a);
                        ui_kit::form(ui, "bd", |fm| {
                            fm.row("Bearing", |ui| d::text(ui, &mut f.bearing1, "e.g. 123.4567 or N45-30-00E"));
                            fm.row("Distance", |ui| {
                                d::num(ui, &mut f.dist, "m");
                            });
                        });
                        if let (Some(a), Ok(b), Ok(dd)) = (a, afrobim_cogo::parse_bearing(&f.bearing1), f.dist.trim().parse::<f64>()) {
                            let p = afrobim_cogo::point_by_bearing_distance(a, b, dd);
                            ui.label(format!("Easting {}  Northing {}", fmt(p.e, 4), fmt(p.n, 4)));
                            add.set(Some((p.e, p.n)));
                        }
                    }
                    2 => {
                        ui_kit::segmented(ui, &mut f.kind, &["Bearing, bearing", "Bearing, distance", "Distance, distance"]);
                        let p1 = pt_in(ui, if f.kind == 2 { "Centre 1" } else { "Point 1" }, &mut f.a);
                        let p2 = pt_in(ui, if f.kind == 0 { "Point 2" } else { "Centre 2 / centre" }, &mut f.b);
                        ui_kit::form(ui, "ix", |fm| {
                            if f.kind != 2 {
                                fm.row("Bearing 1", |ui| d::text(ui, &mut f.bearing1, ""));
                            }
                            if f.kind == 0 {
                                fm.row("Bearing 2", |ui| d::text(ui, &mut f.bearing2, ""));
                            } else {
                                fm.row("Radius", |ui| {
                                    d::num(ui, &mut f.r1, "m");
                                });
                            }
                            if f.kind == 2 {
                                fm.row("Radius 2", |ui| {
                                    d::num(ui, &mut f.r2, "m");
                                });
                            }
                        });
                        let mut lines: Vec<(f64, f64)> = vec![];
                        if let (Some(p1), Some(p2)) = (p1, p2) {
                            let res: Result<Vec<Xy>, String> = match f.kind {
                                0 => match (afrobim_cogo::parse_bearing(&f.bearing1), afrobim_cogo::parse_bearing(&f.bearing2)) {
                                    // 1e-9 degrees: a numerical guard against dividing by zero for parallel lines.
                                    (Ok(b1), Ok(b2)) => afrobim_cogo::intersect_bearing_bearing(p1, b1, p2, b2, 1e-9).map(|h| vec![h.point]).map_err(|e| e.to_string()),
                                    _ => Err("Enter both bearings.".into()),
                                },
                                1 => match (afrobim_cogo::parse_bearing(&f.bearing1), f.r1.trim().parse::<f64>()) {
                                    (Ok(b), Ok(r)) => afrobim_cogo::intersect_bearing_distance(p1, b, p2, r).map(|v| v.into_iter().map(|h| h.point).collect()).map_err(|e| e.to_string()),
                                    _ => Err("Enter the bearing and the radius.".into()),
                                },
                                _ => match (f.r1.trim().parse::<f64>(), f.r2.trim().parse::<f64>()) {
                                    (Ok(r1), Ok(r2)) => afrobim_cogo::intersect_distance_distance(p1, r1, p2, r2).map_err(|e| e.to_string()),
                                    _ => Err("Enter both radii.".into()),
                                },
                            };
                            match res {
                                Ok(v) if v.is_empty() => {
                                    ui.label("No intersection.");
                                }
                                Ok(v) => {
                                    for (i, p) in v.iter().enumerate() {
                                        ui.label(format!("Solution {}: easting {}  northing {}", i + 1, fmt(p.e, 4), fmt(p.n, 4)));
                                        lines.push((p.e, p.n));
                                    }
                                }
                                Err(e) => {
                                    ui.label(RichText::new(e).color(crate::theme::warn()));
                                }
                            }
                        }
                        add.set(lines.first().copied());
                    }
                    _ => {
                        if aligns.is_empty() {
                            d::needs(ui, "There is no alignment in the document. Import one with LANDXML or draw one with ALIGNDESIGN.");
                        } else {
                            ui_kit::form(ui, "so", |fm| {
                                fm.row("Alignment", |ui| {
                                    d::pick_from(ui, "cg-al", &aligns, &mut f.alignment);
                                });
                            });
                            let a = pt_in(ui, "Point", &mut f.a);
                            if let (Some(a), Some(al)) = (a, &doc_align) {
                                // Curves are approximated by chords of 0.05 m (a sagitta under 0.00003 m for a radius of 10 m or more).
                                let pl: Vec<Xy> = al.polyline(0.05).iter().map(|p| Xy::new(p.0, p.1)).collect();
                                match afrobim_cogo::station_offset(&pl, a, al.start_station_m) {
                                    Ok(r) => {
                                        ui.label(format!("Station {}  Offset {} m ({})", fmt(r.station, 3), fmt(r.offset.abs(), 3), if r.offset >= 0.0 { "right" } else { "left" }));
                                        if r.beyond_end {
                                            ui.label(RichText::new("The point is beyond the end of the alignment.").small().color(crate::theme::warn()));
                                        }
                                    }
                                    Err(e) => {
                                        ui.label(RichText::new(e.to_string()).color(crate::theme::warn()));
                                    }
                                }
                            }
                            ui.separator();
                            ui_kit::form(ui, "so2", |fm| {
                                fm.row("Station", |ui| {
                                    d::num(ui, &mut f.station, "m");
                                });
                                fm.row_note("Offset", "Positive is to the right looking up-station", |ui| {
                                    d::num(ui, &mut f.offset, "m");
                                });
                            });
                            if let (Some(al), Ok(s), Ok(o)) = (&doc_align, f.station.trim().parse::<f64>(), f.offset.trim().parse::<f64>()) {
                                match al.point_at_offset(s, o) {
                                    Some(p) => {
                                        ui.label(format!("Easting {}  Northing {}", fmt(p.0, 4), fmt(p.1, 4)));
                                        add.set(Some(p));
                                    }
                                    None => {
                                        ui.label(RichText::new("That station is outside the alignment.").color(crate::theme::warn()));
                                    }
                                }
                            }
                        }
                    }
                }
                if f.tab != 0 {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Add the result to");
                        target_combo(ui, "cg-target", &sets, &mut f.target);
                    });
                }
                if !f.message.is_empty() {
                    ui.label(RichText::new(&f.message).small());
                }
            },
            |ui| Footer::new("Add result as a point").enabled(add.get().is_some(), "There is no result to add").cancel(Some("Close")).show(ui),
        );
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            if let Some((e, n)) = add.get() {
                let (id, mut data) = if f.target > 0 { (Some(sets[f.target - 1].0), self.points_data(sets[f.target - 1].0).unwrap_or_default()) } else { (None, PointsData::default()) };
                match data.store.add_next_free(CogoPoint::new(1, e, n, None, "COGO")) {
                    Ok(num) => {
                        match id {
                            Some(id) => self.set_points(id, data),
                            None => {
                                let id = self.new_points("COGO points", data, "COGO tools");
                                f.target = self.point_sets().iter().position(|s| s.0 == id).map(|i| i + 1).unwrap_or(0);
                            }
                        }
                        f.message = format!("Added point {num}.");
                    }
                    Err(e) => f.message = e.to_string(),
                }
            }
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        self.civil_ui.points.cogo = Some(f);
    }
}

// ─── traverse ──────────────────────────────────────────────────────────────

pub struct TraverseForm {
    start: PtIn,
    start_number: String,
    courses: Vec<(String, String)>,
    closure: usize,
    end: PtIn,
    method: usize,
    ratio: String,
    target: usize,
    message: String,
}

impl Default for TraverseForm {
    fn default() -> Self {
        TraverseForm { start: PtIn::default(), start_number: String::new(), courses: vec![(String::new(), String::new()); 4], closure: 1, end: PtIn::default(), method: 0, ratio: String::new(), target: 0, message: String::new() }
    }
}

impl AfroBimApp {
    fn traverse_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.points.traverse.take() else { return };
        let sets = self.point_sets();
        let mut result: Option<afrobim_cogo::traverse::AdjustedTraverse> = None;
        let out = Dialog::new("Traverse", crate::glyphs::by_name("TRAVERSE")).width(680.0).resizable(true).help("Enter the start point and the courses (bearing and distance). The misclosure is the gap between where the traverse ends and where it should. The compass (Bowditch) rule spreads it in proportion to the course lengths, the transit rule in proportion to the changes in easting and northing. Whether the precision is good enough is for you to decide: enter the ratio you must meet to see a pass or fail.").show(
            ctx,
            |ui| {
                let start = pt_in(ui, "Start", &mut f.start);
                ui_kit::form(ui, "tv", |fm| {
                    fm.row("Closure", |ui| {
                        ui_kit::segmented(ui, &mut f.closure, &["Open", "Loop", "To a known point"]);
                    });
                    fm.row("Adjustment", |ui| {
                        ui_kit::segmented(ui, &mut f.method, &["Compass (Bowditch)", "Transit"]);
                    });
                });
                let end = if f.closure == 2 { pt_in(ui, "Known end", &mut f.end) } else { None };
                ui.label(RichText::new("Courses").strong());
                let mut remove = None;
                egui::Grid::new("tv-courses").num_columns(3).show(ui, |ui| {
                    ui.label("Bearing");
                    ui.label("Distance (m)");
                    ui.label("");
                    ui.end_row();
                    for (i, (b, dist)) in f.courses.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(b).desired_width(150.0).hint_text("123.4567 or N45-30-00E"));
                        ui.add(egui::TextEdit::singleline(dist).desired_width(90.0));
                        if ui.small_button("x").clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = remove {
                    f.courses.remove(i);
                }
                if ui.button("Add course").clicked() {
                    f.courses.push((String::new(), String::new()));
                }
                let courses: Vec<afrobim_cogo::Course> = f.courses.iter().filter_map(|(b, dd)| Some(afrobim_cogo::Course { bearing_deg: afrobim_cogo::parse_bearing(b).ok()?, distance: dd.trim().parse().ok()? })).collect();
                let closure = match f.closure {
                    0 => Some(afrobim_cogo::Closure::Open),
                    1 => Some(afrobim_cogo::Closure::Loop),
                    _ => end.map(afrobim_cogo::Closure::ToPoint),
                };
                if let (Some(s), Some(cl), false) = (start, closure, courses.is_empty()) {
                    match afrobim_cogo::Traverse::new(s, courses, cl).and_then(|t| t.adjust(if f.method == 0 { afrobim_cogo::AdjustMethod::Bowditch } else { afrobim_cogo::AdjustMethod::Transit })) {
                        Ok(adj) => {
                            let m = &adj.misclosure;
                            ui.separator();
                            ui.label(RichText::new("Misclosure").strong());
                            let ratio_text = m.precision_denominator.map(|r| format!("1 in {r:.0}")).unwrap_or_else(|| "not defined".into());
                            ui_kit::results_table(ui, "tv-m", &["Quantity", "Value"], &[Col::Text, Col::Num], &[vec!["Error in easting (m)".into(), fmt(m.error_e, 4)], vec!["Error in northing (m)".into(), fmt(m.error_n, 4)], vec!["Linear misclosure (m)".into(), fmt(m.linear, 4)], vec!["Perimeter (m)".into(), fmt(m.perimeter, 3)], vec!["Precision".into(), ratio_text]]);
                            ui_kit::form(ui, "tv-r", |fm| {
                                fm.row_note("Required precision", "1 in this number: your own requirement", |ui| {
                                    d::num_opt(ui, &mut f.ratio, "").ok();
                                });
                            });
                            if let Ok(req) = f.ratio.trim().parse::<f64>() {
                                let pass = m.meets_ratio(req);
                                ui_kit::callout(ui, if pass { Level::Info } else { Level::Warning }, if pass { format!("Meets 1 in {req:.0}.") } else { format!("Does not meet 1 in {req:.0}.") });
                            }
                            let rows: Vec<Vec<String>> = adj.points.iter().enumerate().map(|(i, p)| vec![(i + 1).to_string(), fmt(p.e, 4), fmt(p.n, 4), adj.corrections.get(i).map(|c| format!("{:+.4} / {:+.4}", c.0, c.1)).unwrap_or_default()]).collect();
                            ui_kit::results_table(ui, "tv-pts", &["Station", "Easting", "Northing", "Correction dE / dN"], &[Col::Num, Col::Num, Col::Num, Col::Num], &rows);
                            result = Some(adj);
                        }
                        Err(e) => {
                            ui.label(RichText::new(e.to_string()).color(crate::theme::warn()));
                        }
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Create the adjusted points in");
                    target_combo(ui, "tv-target", &sets, &mut f.target);
                });
                ui.horizontal(|ui| {
                    ui.label("First point number");
                    d::num_opt(ui, &mut f.start_number, "").ok();
                });
                if !f.message.is_empty() {
                    ui.label(RichText::new(&f.message).small());
                }
            },
            |ui| Footer::new("Create adjusted points").icon(crate::glyphs::by_name("TRAVERSE")).enabled(true, "").cancel(Some("Close")).show(ui),
        );
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            match result {
                Some(adj) => {
                    let first: u32 = f.start_number.trim().parse().unwrap_or(1);
                    let (id, mut data) = if f.target > 0 { (Some(sets[f.target - 1].0), self.points_data(sets[f.target - 1].0).unwrap_or_default()) } else { (None, PointsData::default()) };
                    let mut n = first;
                    let mut added = 0;
                    for p in &adj.points {
                        n = data.store.next_free(n);
                        if data.store.add(CogoPoint::new(n, p.e, p.n, None, "TRAV")).is_ok() {
                            added += 1;
                        }
                    }
                    match id {
                        Some(id) => self.set_points(id, data),
                        None => {
                            let id = self.new_points("Traverse points", data, "Traverse entry");
                            f.target = self.point_sets().iter().position(|s| s.0 == id).map(|i| i + 1).unwrap_or(0);
                        }
                    }
                    f.message = format!("Created {added} point(s).");
                }
                None => f.message = "Complete the start point, the courses and the closure first.".into(),
            }
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        self.civil_ui.points.traverse = Some(f);
    }
}

#[allow(dead_code)]
fn _unused(_: NumRule) {}

/// A table cell: the text left-aligned in a fixed width, highlighted when selected.
fn table_cell(ui: &mut Ui, width: f32, text: &str, selected: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(egui::vec2(width, 20.0), egui::Sense::click());
    if selected || r.hovered() {
        ui.painter().rect_filled(rect, 3.0, if selected { ui.visuals().selection.bg_fill } else { ui.visuals().widgets.hovered.weak_bg_fill });
    }
    ui.painter().with_clip_rect(rect).text(rect.left_center() + egui::vec2(3.0, 0.0), egui::Align2::LEFT_CENTER, text, egui::FontId::monospace(12.0), ui.visuals().text_color());
    r
}
