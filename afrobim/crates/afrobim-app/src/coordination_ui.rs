//! The Coordination tab's windows: the clash tests manager, the run, the results window with
//! zoom to clash, and the CSV export. Clash markers are drawn by the 3D view from `CoordUi`.

use std::collections::BTreeSet;

use afrobim_coordination::{Clash, ClashKind, ClashTest, Coordination, Scene, Selection, Status};
use afrobim_kernel::{Document, ElementId};
use egui::{RichText};

use crate::app::AfroBimApp;
use crate::coordination_glue as glue;
use crate::view3d::WorldCtx;

const STATUSES: [Status; 5] = [Status::New, Status::Active, Status::Reviewed, Status::Approved, Status::Resolved];
#[allow(non_snake_case)]
fn BAD() -> egui::Color32 {
    crate::theme::danger()
}

/// A clash test being added or edited. The tolerance is text because nothing is prefilled.
#[derive(Default, Clone)]
pub struct TestForm {
    index: Option<usize>,
    name: String,
    kind: Option<ClashKind>,
    a_categories: BTreeSet<String>,
    a_layers: BTreeSet<String>,
    b_categories: BTreeSet<String>,
    b_layers: BTreeSet<String>,
    tolerance: String,
    touching: bool,
    ignore_host_chain: bool,
    error: String,
}

impl TestForm {
    fn new() -> TestForm {
        TestForm { kind: Some(ClashKind::Hard), ignore_host_chain: true, ..TestForm::default() }
    }

    fn of(index: usize, t: &ClashTest) -> TestForm {
        TestForm {
            index: Some(index),
            name: t.name.clone(),
            kind: Some(t.kind),
            a_categories: t.a.categories.iter().cloned().collect(),
            a_layers: t.a.layers.iter().cloned().collect(),
            b_categories: t.b.categories.iter().cloned().collect(),
            b_layers: t.b.layers.iter().cloned().collect(),
            tolerance: if t.kind == ClashKind::Clearance { t.tolerance_mm.to_string() } else { String::new() },
            touching: t.touching_counts,
            ignore_host_chain: t.ignore_host_chain,
            error: String::new(),
        }
    }

    /// The test this form describes, or what is wrong with it.
    fn build(&self, existing: &[ClashTest]) -> Result<ClashTest, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Give the test a name.".into());
        }
        if existing.iter().enumerate().any(|(i, t)| t.name == name && Some(i) != self.index) {
            return Err(format!("A test called '{name}' already exists. Names are part of each clash id."));
        }
        let a = Selection { categories: self.a_categories.iter().cloned().collect(), layers: self.a_layers.iter().cloned().collect(), ids: Vec::new() };
        let b = Selection { categories: self.b_categories.iter().cloned().collect(), layers: self.b_layers.iter().cloned().collect(), ids: Vec::new() };
        let mut t = match self.kind.unwrap_or(ClashKind::Hard) {
            ClashKind::Hard => ClashTest::hard(name, a, b),
            ClashKind::Clearance => {
                let text = self.tolerance.trim();
                let tolerance = text.parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0).ok_or("Enter the clearance tolerance in millimetres, a number above zero. AfroBIM offers no default: it is your requirement.")?;
                ClashTest::clearance(name, a, b, tolerance)
            }
        };
        t.touching_counts = self.touching && t.kind == ClashKind::Hard;
        t.ignore_host_chain = self.ignore_host_chain;
        if let Some(i) = self.index {
            // Editing keeps the pairs the team has approved.
            if let Some(old) = existing.get(i) {
                t.approved_pairs = old.approved_pairs.clone();
            }
        }
        Ok(t)
    }
}

/// What the app keeps about the Coordination windows. The tests and results themselves are in the
/// document; `co` is a copy read when the element's data changes.
pub struct CoordUi {
    pub tests_open: bool,
    pub results_open: bool,
    form: Option<TestForm>,
    filter_test: Option<String>,
    filter_status: Option<Status>,
    /// Draw a marker at each live clash in the 3D view.
    pub markers: bool,
    /// The two elements of the clash last zoomed to, drawn highlighted.
    pub highlight: Vec<ElementId>,
    pub co: Coordination,
    loaded: serde_json::Value,
    last_run: String,
    zoom: Option<usize>,
}

impl Default for CoordUi {
    fn default() -> Self {
        CoordUi {
            tests_open: false,
            results_open: false,
            form: None,
            filter_test: None,
            filter_status: None,
            markers: true,
            highlight: Vec::new(),
            co: Coordination::default(),
            loaded: serde_json::Value::Null,
            last_run: String::new(),
            zoom: None,
        }
    }
}

impl CoordUi {
    /// Reads the document's data when it changed (open, undo, an edit made elsewhere).
    pub fn sync(&mut self, doc: &afrobim_kernel::Document) {
        let current = doc.elements().iter().find(|e| e.category == glue::CATEGORY).map(|e| e.data.clone()).unwrap_or(serde_json::Value::Null);
        if current != self.loaded {
            let (_, co) = glue::load(doc);
            self.co = co;
            self.loaded = current;
        }
    }

    /// Records that the document's coordination data is what this copy holds.
    pub fn mark_loaded(&mut self, doc: &afrobim_kernel::Document) {
        self.loaded = doc.elements().iter().find(|e| e.category == glue::CATEGORY).map(|e| e.data.clone()).unwrap_or_default();
    }

    /// The clashes a marker is drawn for: those still live.
    pub fn marked(&self) -> impl Iterator<Item = &Clash> {
        self.co.results.clashes.iter().filter(|c| matches!(c.status, Status::New | Status::Active | Status::Reviewed))
    }
}

fn selection_text(s: &Selection) -> String {
    let mut parts = Vec::new();
    if !s.categories.is_empty() {
        parts.push(format!("category {}", s.categories.join(", ")));
    }
    if !s.layers.is_empty() {
        parts.push(format!("layer {}", s.layers.join(", ")));
    }
    if !s.ids.is_empty() {
        parts.push(format!("{} named element(s)", s.ids.len()));
    }
    if parts.is_empty() {
        "everything".into()
    } else {
        parts.join("; ")
    }
}

fn pick_set(ui: &mut egui::Ui, title: &str, options: &[String], chosen: &mut BTreeSet<String>) {
    ui.label(RichText::new(title).small().weak());
    ui.horizontal_wrapped(|ui| {
        if options.is_empty() {
            ui.label(RichText::new("none in the model yet").weak());
        }
        for o in options {
            let mut on = chosen.contains(o);
            if ui.checkbox(&mut on, o).changed() {
                if on {
                    chosen.insert(o.clone());
                } else {
                    chosen.remove(o);
                }
            }
        }
    });
}

/// What a clash run on a worker thread hands back: the coordination state with its results, the
/// per-test statistics, the number of bodies tested and the corridor surfaces the run had to
/// build (kept by the app, so the 3D view does not build them again).
pub struct ClashOut {
    pub co: afrobim_coordination::Coordination,
    pub stats: Vec<afrobim_coordination::RunStats>,
    pub bodies: usize,
    pub corridors: Vec<crate::background::CorridorBuilt>,
}

/// What a clash run needs, taken on the UI thread: a copy of the document, the generated solids of
/// every element but the corridors, the corridor surfaces that are up to date in the app's cache
/// and the corridors that are not. The stale corridors are built in the worker (decoding a ground
/// surface and cutting every section is the slow part), then the scene is built and run.
pub struct ClashInput {
    pub doc: Document,
    pub co: afrobim_coordination::Coordination,
    pub solids: std::collections::HashMap<ElementId, Vec<afrobim_draw::Mesh3>>,
    pub corridors: std::collections::HashMap<ElementId, Result<crate::road_glue::CorridorSolids, String>>,
    pub stale: Vec<ElementId>,
    pub site_mode: bool,
    /// Bodies of reference models, already placed.
    pub refs: Vec<afrobim_coordination::Body>,
}

impl ClashInput {
    /// Builds the stale corridors, the bodies of the scene exactly as the 3D view places them, and
    /// runs every test.
    pub fn compute(self) -> ClashOut {
        let ClashInput { doc, mut co, solids, mut corridors, stale, site_mode, refs } = self;
        let mut built = Vec::new();
        for id in stale {
            let Some(key) = doc.find(id).map(|e| e.data.clone()) else { continue };
            let result = crate::road_glue::corridor_solids(&doc, id);
            corridors.insert(id, result.clone());
            built.push(crate::background::CorridorBuilt { id, key, solids: result });
        }
        let mut bodies = {
            let solids_of = |id: ElementId| corridors.get(&id).and_then(|r| r.as_ref().ok()).map(|s| s.meshes.as_slice()).or_else(|| solids.get(&id).map(|v| v.as_slice()));
            let origin = |id: ElementId| corridors.get(&id)?.as_ref().ok().map(|s| s.origin);
            glue::bodies(&WorldCtx { doc: &doc, solids_of: &solids_of, corridor_origin: &origin }, site_mode)
        };
        bodies.extend(refs);
        let n = bodies.len();
        let scene = Scene::new(bodies);
        let stats = co.run(&scene);
        ClashOut { co, stats, bodies: n, corridors: built }
    }
}

/// Starts [`ClashInput::compute`] on a worker thread, to be applied only while the document is
/// unchanged from the copy the input holds.
pub fn spawn_clash(input: ClashInput) -> crate::background::Job<ClashOut> {
    let fp = crate::background::fingerprint(&input.doc);
    crate::background::Job::spawn("Clash run", fp, move || input.compute())
}

impl AfroBimApp {
    /// What a clash run needs (see [`ClashInput`]); None (with the reason in the status line) when
    /// there is no test to run.
    pub(crate) fn clash_input(&mut self) -> Option<ClashInput> {
        self.sync_generated();
        self.coord.sync(&self.doc);
        if self.coord.co.tests.is_empty() {
            self.status = "There are no clash tests yet. Add one in CLASHTESTS, then run.".into();
            self.coord.tests_open = true;
            return None;
        }
        let is_corridor = |id: ElementId| self.doc.find(id).is_some_and(|e| e.category == afrobim_corridor::CORRIDOR_CATEGORY);
        let solids = self.doc.elements().iter().filter(|e| e.category != afrobim_corridor::CORRIDOR_CATEGORY).filter_map(|e| self.meshes_by_id(e.id).filter(|m| !m.is_empty()).map(|m| (e.id, m.to_vec()))).collect();
        let stale = crate::background::stale_corridors(&self.doc, &|id| self.corridor_cache.get(&id).map(|c| c.key().clone()));
        let corridors = self
            .corridor_cache
            .iter()
            .filter(|(id, _)| is_corridor(**id) && !stale.contains(id))
            .map(|(id, c)| (*id, c.solids.clone()))
            .collect();
        Some(ClashInput { doc: self.doc.clone(), co: self.coord.co.clone(), solids, corridors, stale, site_mode: self.site_mode(), refs: self.refs.bodies() })
    }

    /// Runs every test on the UI thread (headless runs and tests).
    pub(crate) fn run_clash_tests(&mut self) {
        let Some(input) = self.clash_input() else { return };
        let out = input.compute();
        self.apply_clash_out(out);
    }

    /// Runs every test on a worker thread, with a progress indicator and Cancel. The corridor
    /// surfaces the scene needs are built there too. The results are applied only if the document
    /// has not changed since the run started.
    pub(crate) fn run_clash_tests_bg(&mut self) {
        if self.bg.clash.is_some() {
            self.status = "A clash run is already in progress.".into();
            return;
        }
        let Some(input) = self.clash_input() else { return };
        self.bg.clash = Some(spawn_clash(input));
        self.status = "Clash run started on a worker thread; the model stays editable, but an edit discards the result.".into();
    }

    /// Takes the finished worker's results in.
    pub(crate) fn apply_clash_out(&mut self, out: ClashOut) {
        self.apply_corridors(out.corridors);
        self.coord.co = out.co;
        self.apply_clash_run(out.stats, out.bodies);
    }

    fn apply_clash_run(&mut self, stats: Vec<afrobim_coordination::RunStats>, n: usize) {
        let open_before = self.coord.co.results.clashes.len();
        let found: usize = stats.iter().map(|s| s.clashes).sum();
        let new = self.coord.co.results.count(Status::New);
        match glue::store(&mut self.doc, &self.coord.co) {
            Ok(_) => {}
            Err(e) => {
                self.status = format!("Clash results not stored: {e}");
                return;
            }
        }
        self.coord.loaded = self.doc.elements().iter().find(|e| e.category == glue::CATEGORY).map(|e| e.data.clone()).unwrap_or_default();
        self.coord.last_run = format!("{} test(s) on {n} element(s) with solids: {found} clash(es) found, {new} new, {} on record.", stats.len(), open_before);
        self.status = format!("Clash run: {}", self.coord.last_run);
        if n == 0 {
            self.status.push_str(" No element has solids in the current view, so there was nothing to test.");
        }
        self.coord.results_open = true;
        self.schedule_commit_now();
    }

    /// Frames the clash point in the 3D view and highlights the two elements.
    pub(crate) fn zoom_to_clash(&mut self, c: &Clash) {
        self.view_3d = true;
        self.fitted_3d = true;
        self.coord.highlight = vec![c.element_a, c.element_b];
        let size = [c.overlap_box.max[0] - c.overlap_box.min[0], c.overlap_box.max[1] - c.overlap_box.min[1], c.overlap_box.max[2] - c.overlap_box.min[2]];
        let diag = (size[0] * size[0] + size[1] * size[1] + size[2] * size[2]).sqrt();
        self.camera.yaw = (-60.0f64).to_radians();
        self.camera.pitch = 30.0f64.to_radians();
        self.camera.fit_sphere(c.point, (diag * 3.0).max(1500.0));
        self.status = format!("{}: {} and {} highlighted.", c.id, c.mark_a, c.mark_b);
    }

    pub(crate) fn export_clashes_csv(&mut self) {
        self.coord.sync(&self.doc);
        if self.coord.co.results.clashes.is_empty() {
            self.status = "There are no clashes to export. Run the tests first.".into();
            return;
        }
        let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("clashes.csv").save_file() else { return };
        if path.extension().is_none() {
            path.set_extension("csv");
        }
        self.write_clashes_csv(&path);
    }

    pub(crate) fn write_clashes_csv(&mut self, path: &std::path::Path) {
        match std::fs::write(path, self.coord.co.results.to_csv()) {
            Ok(()) => self.status = format!("Wrote {} clash(es) to {}.", self.coord.co.results.clashes.len(), path.display()),
            Err(e) => self.status = format!("Could not write {}: {e}", path.display()),
        }
    }

    pub(crate) fn coordination_windows(&mut self, ctx: &egui::Context) {
        self.coord.sync(&self.doc);
        let mut changed = false;

        // ── the tests manager ──
        if self.coord.tests_open {
            let mut open = true;
            let mut run = false;
            let mut edit: Option<usize> = None;
            let mut delete: Option<usize> = None;
            let mut add = false;
            crate::win::new(ctx, "Clash tests").open(&mut open).default_width(760.0).show(ctx, |ui| {
                ui.set_min_width(720.0);
                ui.label(RichText::new("Reference models appear in the selections as categories 'reference:<model>' (pick one model) and by their IFC entity type under layers (for example IFCWALL).").small().weak());
                ui.label(RichText::new("A test checks one selection of elements against another. Hard tests find solids that overlap; clearance tests find solids closer than the tolerance you enter. AfroBIM offers no tolerances: they come from your project requirements.").small().weak());
                ui.add_space(4.0);
                if self.coord.co.tests.is_empty() {
                    ui.label(RichText::new("No tests yet.").weak());
                }
                egui::Grid::new("clash-tests-grid").num_columns(6).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                    for h in ["Name", "Type", "Selection A", "Selection B", "Tolerance", ""] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (i, t) in self.coord.co.tests.iter().enumerate() {
                        ui.label(&t.name);
                        ui.label(t.kind.label());
                        ui.label(selection_text(&t.a));
                        ui.label(selection_text(&t.b));
                        ui.label(if t.kind == ClashKind::Clearance { format!("{} mm", t.tolerance_mm) } else if t.touching_counts { "touching counts".to_string() } else { String::new() });
                        ui.horizontal(|ui| {
                            if ui.small_button("Edit").clicked() {
                                edit = Some(i);
                            }
                            if ui.small_button("Delete").clicked() {
                                delete = Some(i);
                            }
                        });
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Add a test").clicked() {
                        add = true;
                    }
                    if ui.add_enabled(!self.coord.co.tests.is_empty(), egui::Button::new("Run the tests")).clicked() {
                        run = true;
                    }
                });
            });
            self.coord.tests_open = open;
            if add {
                self.coord.form = Some(TestForm::new());
            }
            if let Some(i) = edit {
                self.coord.form = Some(TestForm::of(i, &self.coord.co.tests[i]));
            }
            if let Some(i) = delete {
                let name = self.coord.co.tests.remove(i).name;
                self.coord.co.results.clashes.retain(|c| c.test != name);
                self.status = format!("Deleted test '{name}' and its clashes.");
                changed = true;
            }
            if run {
                self.run_clash_tests_bg();
            }
        }

        // ── the add / edit form ──
        if let Some(mut form) = self.coord.form.take() {
            let (mut cats, mut layers) = glue::selectable(&self.doc);
            let (ref_cats, ref_layers) = self.refs.selectable();
            cats.extend(ref_cats);
            for l in ref_layers {
                if !layers.contains(&l) {
                    layers.push(l);
                }
            }
            let (mut keep, mut save) = (true, false);
            crate::win::new(ctx, if form.index.is_some() { "Edit clash test" } else { "Add a clash test" }).collapsible(false).resizable(false).default_width(560.0).show(ctx, |ui| {
                egui::Grid::new("clash-form").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Name");
                    ui.add_sized([300.0, 20.0], egui::TextEdit::singleline(&mut form.name).hint_text("e.g. Services against structures"));
                    ui.end_row();
                    ui.label("Type");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut form.kind, Some(ClashKind::Hard), "Hard (solids overlap)");
                        ui.radio_value(&mut form.kind, Some(ClashKind::Clearance), "Clearance (closer than a tolerance)");
                    });
                    ui.end_row();
                    ui.label("Selection A");
                    ui.vertical(|ui| {
                        pick_set(ui, "Categories (none ticked means all)", &cats, &mut form.a_categories);
                        pick_set(ui, "Layers (none ticked means all)", &layers, &mut form.a_layers);
                    });
                    ui.end_row();
                    ui.label("Selection B");
                    ui.vertical(|ui| {
                        pick_set(ui, "Categories (none ticked means all)", &cats, &mut form.b_categories);
                        pick_set(ui, "Layers (none ticked means all)", &layers, &mut form.b_layers);
                    });
                    ui.end_row();
                    if form.kind == Some(ClashKind::Clearance) {
                        ui.label("Tolerance (mm)");
                        ui.add_sized([160.0, 20.0], egui::TextEdit::singleline(&mut form.tolerance).hint_text("your requirement, in mm"));
                        ui.end_row();
                    } else {
                        ui.label("Touching");
                        ui.checkbox(&mut form.touching, "faces that only touch count as a clash");
                        ui.end_row();
                    }
                    ui.label("Host chain");
                    ui.checkbox(&mut form.ignore_host_chain, "ignore pairs where one element hosts the other (a post and its sign or foundation)");
                    ui.end_row();
                });
                if !form.error.is_empty() {
                    ui.label(RichText::new(&form.error).color(BAD()));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Save the test").clicked() {
                        save = true;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
            if save {
                match form.build(&self.coord.co.tests) {
                    Ok(t) => {
                        let old_name = form.index.and_then(|i| self.coord.co.tests.get(i)).map(|t| t.name.clone());
                        match form.index {
                            Some(i) if i < self.coord.co.tests.len() => {
                                // A renamed test changes every clash id, so its old results are dropped.
                                if old_name.as_deref() != Some(t.name.as_str()) {
                                    let old = old_name.unwrap_or_default();
                                    self.coord.co.results.clashes.retain(|c| c.test != old);
                                }
                                self.coord.co.tests[i] = t;
                            }
                            _ => self.coord.co.tests.push(t),
                        }
                        self.status = "Clash test saved. Run the tests to see its clashes.".into();
                        changed = true;
                    }
                    Err(e) => {
                        form.error = e;
                        self.coord.form = Some(form);
                    }
                }
            } else if keep {
                self.coord.form = Some(form);
            }
        }

        // ── the results ──
        if self.coord.results_open {
            let mut open = true;
            let mut export = false;
            let mut run = false;
            let mut zoom: Option<usize> = None;
            let names: Vec<String> = self.coord.co.tests.iter().map(|t| t.name.clone()).collect();
            crate::win::new(ctx, "Clash results").open(&mut open).default_width(1060.0).default_height(420.0).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Test");
                    let shown = self.coord.filter_test.clone().unwrap_or_else(|| "all".to_string());
                    egui::ComboBox::from_id_salt("clash-filter-test").selected_text(shown).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.coord.filter_test, None, "all");
                        for n in &names {
                            ui.selectable_value(&mut self.coord.filter_test, Some(n.clone()), n);
                        }
                    });
                    ui.label("Status");
                    egui::ComboBox::from_id_salt("clash-filter-status").selected_text(self.coord.filter_status.map(|s| s.label()).unwrap_or("all")).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.coord.filter_status, None, "all");
                        for s in STATUSES {
                            ui.selectable_value(&mut self.coord.filter_status, Some(s), s.label());
                        }
                    });
                    ui.checkbox(&mut self.coord.markers, "Markers in 3D");
                    if ui.button("Run again").clicked() {
                        run = true;
                    }
                    if ui.button("Export CSV").clicked() {
                        export = true;
                    }
                });
                let counts: Vec<String> = STATUSES.iter().map(|s| format!("{} {}", self.coord.co.results.count(*s), s.label())).collect();
                ui.label(RichText::new(format!("{} clash(es): {}.", self.coord.co.results.clashes.len(), counts.join(", "))).small().weak());
                if !self.coord.last_run.is_empty() {
                    ui.label(RichText::new(format!("Last run: {}", self.coord.last_run)).small().weak());
                }
                let rows: Vec<usize> = self
                    .coord
                    .co
                    .results
                    .clashes
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| self.coord.filter_test.as_ref().is_none_or(|t| &c.test == t) && self.coord.filter_status.is_none_or(|s| c.status == s))
                    .map(|(i, _)| i)
                    .collect();
                if rows.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(if self.coord.co.results.clashes.is_empty() { "No clashes on record." } else { "No clash matches the filters." }).weak());
                }
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("clash-results-grid").num_columns(10).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                        for h in ["", "Id", "Test", "Element A", "Element B", "Kind", "Depth or distance", "Status", "Assignee", "Comment"] {
                            ui.label(RichText::new(h).strong());
                        }
                        ui.end_row();
                        for i in rows {
                            let c = &mut self.coord.co.results.clashes[i];
                            if ui.small_button("Zoom").on_hover_text("Frame this clash in the 3D view and highlight its two elements").clicked() {
                                zoom = Some(i);
                            }
                            ui.label(RichText::new(&c.id).monospace().small());
                            ui.label(&c.test);
                            ui.label(&c.mark_a);
                            ui.label(&c.mark_b);
                            ui.label(c.kind.label());
                            ui.label(match (c.depth_mm, c.distance_mm) {
                                (Some(d), _) if c.containment => format!("{d:.1} mm (one inside the other)"),
                                (Some(d), _) => format!("{d:.1} mm deep"),
                                (_, Some(d)) => format!("{d:.1} mm apart"),
                                _ => String::new(),
                            });
                            let before = c.status;
                            egui::ComboBox::from_id_salt(("clash-status", i)).width(90.0).selected_text(c.status.label()).show_ui(ui, |ui| {
                                for s in STATUSES {
                                    ui.selectable_value(&mut c.status, s, s.label());
                                }
                            });
                            changed |= before != c.status;
                            changed |= ui.add_sized([90.0, 20.0], egui::TextEdit::singleline(&mut c.assignee).hint_text("who")).changed();
                            changed |= ui.add_sized([180.0, 20.0], egui::TextEdit::singleline(&mut c.comment).hint_text("comment")).changed();
                            ui.end_row();
                        }
                    });
                });
            });
            self.coord.results_open = open;
            self.coord.zoom = zoom;
            if let Some(i) = self.coord.zoom.take() {
                let c = self.coord.co.results.clashes[i].clone();
                self.zoom_to_clash(&c);
            }
            if export {
                self.export_clashes_csv();
            }
            if run {
                self.run_clash_tests_bg();
            }
        }

        if changed {
            if let Err(e) = glue::store(&mut self.doc, &self.coord.co) {
                self.status = format!("Not stored: {e}");
            }
            self.coord.loaded = self.doc.elements().iter().find(|e| e.category == glue::CATEGORY).map(|e| e.data.clone()).unwrap_or_default();
            self.schedule_commit_now();
        }
    }
}
