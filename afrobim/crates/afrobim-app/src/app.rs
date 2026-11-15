//! Application state, panels and input routing.

use std::collections::HashMap;
use std::path::PathBuf;

use afrobim_kernel::{
    CanvasController, CanvasEvent, Document, Element, ElementId, Generated, ModuleRegistry, Pt, SelectMode, Severity,
    UndoHistory, ViewTransform, FILE_EXTENSION,
};
use egui::{Color32, Key, KeyboardShortcut, Modifiers, PointerButton, RichText, Sense};

use crate::commands::{self, Action, COMMANDS};
use crate::icons::{self, Icon};
use crate::render::{ElementMesh, Representation};
use crate::{alignment_glue, markings_glue, modules_glue, road_dialogs, road_glue, signs_glue, supports_glue, Options};
use crate::{check_ui::CheckUi, construction_ui::ConstructionUi, coordination_ui::CoordUi, cost_ui::CostUi, schedule_ui::SchedUi};

/// A module's output for one element, kept until the element's definition changes.
pub(crate) struct Cached {
    key: (String, serde_json::Value),
    pub generated: Result<Generated, String>,
    pub mesh: ElementMesh,
}

/// The 3D surfaces of a corridor, kept until the corridor's data changes: building them decodes
/// the ground surface and cuts every section, which is too much to do each frame.
pub(crate) struct CorridorSurfaces {
    key: serde_json::Value,
    pub solids: Result<road_glue::CorridorSolids, String>,
}

pub struct AfroBimApp {
    pub(crate) doc: Document,
    pub(crate) view: ViewTransform,
    pub(crate) ctl: CanvasController,
    pub(crate) modules: ModuleRegistry,
    /// The drawing sheets: project information, the sheet-set manager, the title block editor and
    /// the preview.
    pub(crate) sheets: crate::sheets_ui::SheetsUi,
    /// Clash tests and results, the programme and the estimate: windows and copies of the
    /// document elements that hold them.
    pub(crate) coord: CoordUi,
    pub(crate) sched: SchedUi,
    pub(crate) cost: CostUi,
    /// Site progress and earned value: the windows and the copy of the progress book.
    pub(crate) construction: ConstructionUi,
    /// The model-wide check, the drain and culvert input dialogs and the rational method.
    pub(crate) checks: CheckUi,
    /// The alignment design tools: the plan editor, the PI table, the profile editor, checks and options.
    pub(crate) design: crate::design_ui::DesignUi,
    /// Issues, BCF, reference models, sign visibility, photometry and the report pack.
    pub(crate) issues: crate::issues_ui::IssuesUi,
    pub(crate) refs: crate::reference_models::RefUi,
    pub(crate) vis: crate::visibility_ui::VisUi,
    pub(crate) photo: crate::photometry_ui::PhotoUi,
    pub(crate) pack: crate::report_pack::PackUi,
    /// Annotation commands, object snap, the layer manager, units and styles.
    pub(crate) anno: crate::anno_ui::AnnoUi,
    /// DXF underlays: what was read from the files and what is drawn.
    pub(crate) under: crate::underlay::UnderlayState,
    /// The sign schedule import windows.
    pub(crate) signsched: crate::signsched::SignSchedUi,
    pub(crate) projset: crate::projset::ProjSetUi,
    /// The name of the last IFC file exported, which BCF topics name as their model.
    pub(crate) last_ifc_name: Option<String>,
    signs_ready: bool,
    pub(crate) module_status: String,
    pub(crate) standard: String,
    pub(crate) engine: Option<std::sync::Arc<afrobim_signs::SignEngine>>,
    fonts_dir: PathBuf,
    pub(crate) cache: HashMap<ElementId, Cached>,

    history: UndoHistory<Vec<Element>>,
    commit_due: Option<f64>,
    clipboard: Vec<Element>,
    paste_count: u32,

    cmd_input: String,
    cmd_log: Vec<String>,
    last_action: Option<Action>,
    ribbon_tab: crate::ribbon::Tab,
    show_help: bool,
    show_about: bool,
    show_library: bool,
    library: Vec<signs_glue::LibraryItem>,
    library_filter: String,
    /// The "Add supports" dialog: sign, mounting height as typed, number of posts, edge inset.
    supports_dialog: Option<(ElementId, String, u32, f64)>,
    /// The "Design supports" dialog: sign, mounting height as typed, entered wind pressure, request.
    design_dialog: Option<(ElementId, String, f64, supports_glue::DesignRequest)>,
    /// The "Road marking" dialog: alignment and request.
    marking_dialog: Option<(Option<ElementId>, markings_glue::MarkingRequest)>,
    /// The second-set marking dialog.
    pub(crate) ext_marking_dialog: Option<crate::ext_markings_ui::ExtDialog>,
    /// Slow work running on worker threads.
    pub(crate) bg: crate::background::Background,
    /// The IFC export options dialog is open.
    pub(crate) ifc_options_open: bool,
    /// The "add a road element" dialog: kind, alignment, start and end station, distance, right side.
    road_dialog: Option<(usize, Option<ElementId>, f64, f64, f64, bool)>,
    /// The "hang a sign on a gantry" dialog: sign, gantry, over a lane, lane number, offset, and
    /// the level of the sign's lower edge above the underside of the boom.
    hang_dialog: Option<(ElementId, Option<ElementId>, bool, usize, f64, f64, bool)>,
    /// The last support calculation: sign mark and its lines.
    pub(crate) design_report: Option<(String, Vec<String>)>,
    /// The dialogs of street lighting, road restraint, pavement, pavement design and corridors,
    /// and the cross-section window.
    road: road_dialogs::RoadDialogs,
    /// The built 3D surfaces of corridors.
    pub(crate) corridor_cache: HashMap<ElementId, CorridorSurfaces>,
    /// The "Place on alignment" dialog: sign, alignment, station (m), offset (m), right side.
    /// Sign, alignment, displayed station, offset, right side, and the stationing region as
    /// typed (0 = not stated, else the region number counted from 1).
    station_dialog: Option<(ElementId, Option<ElementId>, f64, f64, bool, usize)>,
    new_prop_key: String,

    pub(crate) grid_on: bool,
    pub(crate) grid_spacing: f64,
    pub(crate) dark_canvas: bool,
    dark_theme: bool,
    pub(crate) rep: Representation,
    pub(crate) show_labels: bool,

    path: Option<PathBuf>,
    pub(crate) status: String,
    fitted: bool,
    pub(crate) view_3d: bool,
    pub(crate) camera: crate::view3d::Camera,
    pub(crate) fitted_3d: bool,
    canvas_drag: bool,
    quit: bool,

    pub(crate) options: Options,
    frames: u32,
}

const CMD_ID: &str = "afrobim-command-line";

impl AfroBimApp {
    pub fn new(cc: &eframe::CreationContext<'_>, options: Options) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut modules = crate::background::worker_registry();
        let standard = options.standard.clone().unwrap_or_else(|| signs_glue::DEFAULT_STANDARD.to_string());
        let (signs_ready, module_status, library, engine, fonts_dir) = match signs_glue::load_module(&standard) {
            Ok(l) => {
                modules.register(l.module);
                (true, l.label, l.library, Some(l.engine), l.fonts_dir)
            }
            Err(e) => (false, format!("Traffic signs unavailable: {e}"), Vec::new(), None, PathBuf::new()),
        };

        let mut app = Self {
            doc: Document::default(),
            view: ViewTransform::default(),
            ctl: CanvasController::default(),
            modules,
            sheets: crate::sheets_ui::SheetsUi::default(),
            coord: CoordUi::default(),
            sched: SchedUi::default(),
            cost: CostUi::default(),
            construction: ConstructionUi::default(),
            checks: CheckUi::default(),
            design: Default::default(),
            issues: Default::default(),
            refs: Default::default(),
            vis: Default::default(),
            photo: Default::default(),
            pack: Default::default(),
            anno: Default::default(),
            under: Default::default(),
            signsched: Default::default(),
            projset: Default::default(),
            last_ifc_name: None,
            signs_ready,
            module_status,
            standard,
            engine,
            fonts_dir,
            cache: HashMap::new(),
            history: UndoHistory::new(Vec::new(), 100),
            commit_due: None,
            clipboard: Vec::new(),
            paste_count: 0,
            cmd_input: String::new(),
            cmd_log: vec!["AfroBIM model space. Type a command or pick a tool. ? lists commands.".into()],
            last_action: None,
            ribbon_tab: options.tab.as_deref().and_then(crate::ribbon::tab_named).unwrap_or(crate::ribbon::Tab::Home),
            show_help: false,
            show_about: false,
            show_library: false,
            library,
            library_filter: String::new(),
            supports_dialog: None,
            design_dialog: None,
            marking_dialog: None,
            ext_marking_dialog: None,
            bg: Default::default(),
            ifc_options_open: false,
            road_dialog: None,
            hang_dialog: None,
            design_report: None,
            road: road_dialogs::RoadDialogs::default(),
            corridor_cache: HashMap::new(),
            station_dialog: None,
            new_prop_key: String::new(),
            grid_on: true,
            grid_spacing: 100.0,
            dark_canvas: true,
            dark_theme: true,
            rep: Representation::Real,
            show_labels: true,
            path: None,
            status: "Ready.".into(),
            fitted: false,
            view_3d: false,
            camera: crate::view3d::Camera::default(),
            fitted_3d: false,
            canvas_drag: false,
            quit: false,
            options,
            frames: 0,
        };

        if let Some(path) = app.options.open.clone() {
            app.open_path(path);
        } else if app.options.demo && app.signs_ready {
            for (mark, data) in signs_glue::demo_data(&app.standard) {
                let name = app.doc.unique_name(&mark);
                let at = app.doc.next_free_origin(2000.0, 1000.0);
                app.doc.add(signs_glue::CATEGORY, &name, at, 2000.0, 1000.0, signs_glue::LAYER, data);
                app.sync_generated();
            }
            // A few catalogued signs of each class, so the demo shows the standard library too.
            let mut per_class: HashMap<String, usize> = HashMap::new();
            let picks: Vec<(String, serde_json::Value)> = app
                .library
                .iter()
                .filter(|i| matches!(i.class.as_str(), "Warning" | "Regulatory" | "Information"))
                .filter(|i| {
                    let n = per_class.entry(i.class.clone()).or_insert(0);
                    *n += 1;
                    *n <= 4
                })
                .map(|i| (i.key.clone(), i.data.clone()))
                .collect();
            for (key, data) in picks {
                let name = app.doc.unique_name(&key);
                let at = app.doc.next_free_origin(1000.0, 1000.0);
                app.doc.add(signs_glue::CATEGORY, &name, at, 1000.0, 1000.0, signs_glue::LAYER, data);
                app.sync_generated();
            }
            app.doc.arrange();
            // Demo only: supports under a few signs, at an arbitrary 2 m mounting height and with
            // the placeholder dimensions, to show posts and foundations in the model.
            let signs: Vec<ElementId> = app.doc.elements().iter().filter(|e| e.category == signs_glue::CATEGORY).map(|e| e.id).take(4).collect();
            for (i, id) in signs.into_iter().enumerate() {
                let posts = if i == 0 { 2 } else { 1 };
                let _ = supports_glue::add(&mut app.doc, id, 2000.0, posts, 300.0);
                if app.options.design {
                    let mut req = supports_glue::DesignRequest::starting(2000.0, posts);
                    req.edge_inset_mm = 300.0;
                    // The two-post sign shows a spread foundation, the others planted ones.
                    req.spread = posts == 2;
                    req.spread_breadth_mm = 2600.0;
                    let mark = app.doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
                    match supports_glue::design(&mut app.doc, id, &req) {
                        Ok(lines) if app.design_report.is_none() => app.design_report = Some((mark, lines)),
                        Ok(_) => {}
                        Err(e) => app.status = format!("{mark}: supports not designed: {e}"),
                    }
                }
            }
            app.sync_generated();
            if let Some(first) = app.doc.elements().first().map(|e| e.id) {
                app.doc.select(&[first], SelectMode::Replace);
            }
        }
        if let Some(path) = app.options.landxml.clone() {
            match alignment_glue::import_landxml(&mut app.doc, &path) {
                Ok((added, _)) => app.status = format!("Imported {added} alignment(s) from {}.", path.display()),
                Err(e) => app.status = format!("Could not import {}: {e}", path.display()),
            }
            // Demo only: a centre line and two edge lines over the stretch the demo pavement
            // covers, so that the IFC export can attach them to it (see `markings_glue::demo`).
            if app.options.demo {
                if let Some((alignment_id, a)) = alignment_glue::alignments(&app.doc).into_iter().next().and_then(|(id, _)| Some((id, alignment_glue::alignment_of(&app.doc, id)?))) {
                    for problem in markings_glue::demo(&mut app.doc, alignment_id, &a).into_iter().chain(crate::ext_markings_glue::demo(&mut app.doc, alignment_id, &a)) {
                        app.status = problem;
                    }
                }
            }
            // Demo only: a side drain, a pipe culvert and a portal gantry on the first alignment,
            // all with their placeholder dimensions.
            if app.options.demo {
                if let Some((alignment_id, a)) = alignment_glue::alignments(&app.doc).into_iter().next().and_then(|(id, _)| Some((id, alignment_glue::alignment_of(&app.doc, id)?))) {
                    let (s0, len) = (a.start_station_m, a.length());
                    for (kind, from, to, offset) in [(0, 0.05, 0.45, 7.0), (1, 0.25, 0.25, 0.0), (2, 0.35, 0.35, 0.0)] {
                        if let Err(e) = modules_glue::add(&mut app.doc, kind, alignment_id, s0 + len * from, s0 + len * to, offset) {
                            app.status = e;
                        }
                    }
                }
            }
            // Demo only: one each of ground surface, corridor, street lighting, barrier run and
            // pavement. Their values are arbitrary demo values, see `road_glue::demo`.
            if app.options.demo {
                for problem in road_glue::demo(&mut app.doc) {
                    app.status = problem;
                }
            }
            // Demo only: stand the supported signs along the first alignment, alternating sides.
            if app.options.demo {
                if let Some((alignment_id, _)) = alignment_glue::alignments(&app.doc).into_iter().next() {
                    if let Some(a) = alignment_glue::alignment_of(&app.doc, alignment_id) {
                        let signs: Vec<ElementId> = app
                            .doc
                            .elements()
                            .iter()
                            .filter(|e| e.category == signs_glue::CATEGORY)
                            .filter(|e| app.doc.elements().iter().any(|p| p.host == Some(e.id)))
                            .map(|e| e.id)
                            .collect();
                        let step = a.length() / (signs.len() as f64 + 1.0);
                        for (i, id) in signs.into_iter().enumerate() {
                            let internal = a.start_station_m + step * (i as f64 + 1.0);
                            let shown = a.displayed_station(internal);
                            let (station, region) = shown.map(|d| (d.station_m, Some(d.region))).unwrap_or((internal, None));
                            if let Err(e) = alignment_glue::place_sign(&mut app.doc, id, alignment_id, station, region, 4.0, i % 2 == 1) {
                                app.status = e;
                            }
                        }
                    }
                }
            }
        }
        if app.options.demo {
            // Demo only: the made-up programme, estimate and clash tests of `demo_glue`.
            for problem in crate::demo_glue::add_records(&mut app.doc) {
                app.status = problem;
            }
            // Demo only: a small designed alignment with two options (see `design_glue::demo`).
            for problem in crate::design_glue::demo(&mut app.doc) {
                app.status = problem;
            }
            // Demo only: arbitrary visibility inputs on one placed sign, marked as demo.
            crate::demo_glue::add_visibility(&mut app.doc);
            // Demo only: the DEMO project information and a demo sheet set (see `sheets_glue::demo`).
            for problem in crate::sheets_glue::demo(&mut app.doc) {
                app.status = problem;
            }
            // Demo only: a few annotations with arbitrary demo values (see `anno_ui::demo`).
            for problem in crate::anno_ui::demo(&mut app.doc) {
                app.status = problem;
            }
        }
        app.sync_generated();
        if app.options.demo {
            app.run_clash_tests();
            app.coord.results_open = false;
            // Demo only: one made-up issue with a viewpoint (see `demo_glue::add_issue`).
            if crate::demo_glue::add_issue(&mut app.coord.co).is_some() {
                app.store_coordination();
            }
            app.status = "Ready.".into();
        }
        if let Some(c) = app.options.zoom_clash.and_then(|n| app.coord.co.results.clashes.get(n).cloned()) {
            app.zoom_to_clash(&c);
        }
        if let Some(n) = app.options.subtab {
            app.cost.set_subtab(n);
        }
        if let Some(text) = app.options.date.clone() {
            app.sched.sync(&app.doc);
            match afrobim_schedule::Date::parse(&text) {
                Some(d) => {
                    app.sched.set_date(&app.doc, d);
                    app.sched.fourd_on = true;
                }
                None => app.status = format!("--date: '{text}' is not a date (YYYY-MM-DD)."),
            }
        }
        // `--underlay path[,dx,dy[,rotation]]`: attach a DXF at start-up (offsets in mm).
        if let Some(spec) = app.options.underlay.clone() {
            let mut parts = spec.split(',');
            let path = PathBuf::from(parts.next().unwrap_or_default());
            let nums: Vec<f64> = parts.filter_map(|p| p.trim().parse().ok()).collect();
            match app.under.attach(&mut app.doc, &path) {
                Ok(id) => {
                    if let Some(mut def) = app.doc.find(id).and_then(crate::underlay::def_of) {
                        def.offset_mm = [nums.first().copied().unwrap_or(0.0), nums.get(1).copied().unwrap_or(0.0)];
                        def.rotation_deg = nums.get(2).copied().unwrap_or(0.0);
                        crate::underlay::set_def(&mut app.doc, id, &def);
                    }
                    app.under.selected = Some(id);
                }
                Err(e) => app.status = e,
            }
        }
        if let Some(path) = app.options.signschedule.clone() {
            app.signsched_load(&path);
        }
        if let Some(path) = app.options.dxf_out.clone() {
            app.doc.clear_selection();
            app.export_dxf(path);
            eprintln!("AfroBIM: {}", app.status);
            app.quit = true;
        }
        for name in app.options.layers_on.clone().unwrap_or_default().split(',').filter(|n| !n.is_empty()) {
            let _ = afrobim_annotate::layers::set_visible(&mut app.doc, name, true);
        }
        app.history.reset(app.doc.snapshot());
        app.view_3d = app.options.view3d;
        if let Some(dir) = app.options.export_sheets.clone() {
            app.doc.clear_selection(); // an unattended export covers the whole model
            let _ = std::fs::create_dir_all(&dir);
            app.export_quantities(dir.join("sign-quantities.csv"));
            app.write_module_quantities(&dir.join("sign-quantities.csv"));
            eprintln!("AfroBIM: {}", app.status);
            app.export_sheets(dir);
            eprintln!("AfroBIM: {}", app.status);
            app.quit = true;
        }
        if let Some(kind) = app.options.sheet_form {
            let doc = &app.doc;
            app.sheets.open_form_for_screenshot(doc, kind);
        }
        if let Some(page) = app.options.sheet {
            app.sheets.page = page;
        }
        if let Some(path) = app.options.export_set.clone() {
            app.sheets.sync(&app.doc);
            let name = app.options.sheet_set.clone();
            match app.compose_sheets(name.as_deref()).and_then(|c| crate::sheets_glue::export_headless(&c, &path)) {
                Ok(line) => eprintln!("AfroBIM: {line}"),
                Err(e) => eprintln!("AfroBIM: sheet export failed: {e}"),
            }
            app.quit = true;
        }
        if let Some(path) = app.options.export_pack.clone() {
            app.pack_compose();
            if let Some(pack) = app.pack.pack.clone() {
                match crate::report_pack::write_all(&pack, &path) {
                    Ok(w) => eprintln!("AfroBIM: wrote {}", w.join(", ")),
                    Err(e) => eprintln!("AfroBIM: {e}"),
                }
            }
            app.quit = true;
        }
        if let Some(spec) = app.options.ifcref.clone() {
            let mut parts = spec.split(',');
            let path = parts.next().unwrap_or_default().to_string();
            let nums: Vec<f64> = parts.filter_map(|p| p.trim().parse().ok()).collect();
            app.refs.quiet = true;
            app.refs.start_import(std::path::Path::new(&path), &nums);
        }
        if let Some(path) = app.options.photometry.clone() {
            app.photo_open();
            app.photo_load_file(&path);
            if app.options.photometry_run {
                app.photo_demo_run();
            }
        }
        if app.options.capture_viewpoint {
            // Take a viewpoint of the demo issue from the current 3D camera and snapshot it.
            if let Some(g) = app.coord.co.issues.first().map(|i| i.guid.clone()) {
                app.view_3d = true;
                if let Err(e) = app.viewpoint_capture(&g) {
                    app.status = e;
                }
            }
        }
        if let Some(path) = app.options.xer_in.clone() {
            app.import_xer_path(&path);
        }
        if let Some(path) = app.options.xer_out.clone() {
            app.sched.sync(&app.doc);
            if let Some(p) = app.sched.prog.clone() {
                app.export_xer_path(&p, &path);
            }
        }
        if let Some(spec) = app.options.new_alignment.clone() {
            let (name, start) = spec.split_once(',').map(|(n, s)| (n.trim().to_string(), s.trim().parse::<f64>().unwrap_or(0.0))).unwrap_or((spec.clone(), 0.0));
            let points: Vec<(f64, f64)> = app
                .options
                .pis
                .clone()
                .unwrap_or_default()
                .split(';')
                .filter_map(|p| p.split_once(',').and_then(|(x, y)| Some((x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?))))
                .collect();
            app.design.create = Some(crate::design_ui::Create { name, start_m: start, points });
            app.design.plan_on = true;
            app.design.invalidate();
            if app.options.pis_finish {
                app.design_finish_create();
            }
        }
        app.frames_setup();
        if let Some(path) = app.options.export_ifc.clone() {
            app.export_ifc(path);
            eprintln!("AfroBIM: {}", app.status);
            app.quit = true;
        }
        app
    }

    // ─── module output ───────────────────────────────────────────────────────

    /// Regenerates geometry for elements whose definition changed and drops erased ones.
    pub(crate) fn sync_generated(&mut self) {
        // A leader that shows a mark and a station dimension read the rest of the document:
        // refresh them whenever an element was renamed, erased or edited (an alignment included).
        let mut regenerated = self.sync_generated_once();
        for _ in 0..2 {
            if regenerated && self.anno_refresh() {
                regenerated = self.sync_generated_once();
            } else {
                break;
            }
        }
        if self.view_3d {
            self.sync_corridor_surfaces();
        }
    }

    /// Regenerates what changed; true when any element was regenerated or dropped.
    fn sync_generated_once(&mut self) -> bool {
        let mut any = false;
        let ids: Vec<ElementId> = self.doc.elements().iter().map(|e| e.id).collect();
        let before = self.cache.len();
        self.cache.retain(|id, _| ids.contains(id));
        any |= self.cache.len() != before;
        for id in ids {
            let Some(e) = self.doc.find(id) else { continue };
            let fresh = self.cache.get(&id).is_some_and(|c| c.key.0 == e.name && c.key.1 == e.data);
            if fresh {
                continue;
            }
            let generated = self.modules.generate(e);
            let key = (e.name.clone(), e.data.clone());
            let mesh = generated.as_ref().map(|g| ElementMesh::build(&g.drawing)).unwrap_or_default();
            if let Ok(g) = &generated {
                let (w, h) = (g.drawing.extents.width(), g.drawing.extents.height());
                if w > 0.0 && h > 0.0 {
                    if let Some(e) = self.doc.find_mut(id) {
                        e.width = w;
                        e.height = h;
                    }
                }
            }
            self.cache.insert(id, Cached { key, generated, mesh });
            any = true;
        }
        any
    }

    /// Builds the 3D surfaces of corridors that have none yet or whose data changed.
    pub(crate) fn sync_corridor_surfaces(&mut self) {
        let ids: Vec<ElementId> = self.doc.elements().iter().filter(|e| e.category == afrobim_corridor::CORRIDOR_CATEGORY).map(|e| e.id).collect();
        self.corridor_cache.retain(|id, _| ids.contains(id));
        for id in ids {
            let Some(e) = self.doc.find(id) else { continue };
            if self.corridor_cache.get(&id).is_some_and(|c| c.key == e.data) {
                continue;
            }
            let key = e.data.clone();
            let solids = road_glue::corridor_solids(&self.doc, id);
            self.corridor_cache.insert(id, CorridorSurfaces { key, solids });
        }
    }

    // ─── undo ────────────────────────────────────────────────────────────────

    pub(crate) fn request_quit(&mut self) {
        self.quit = true;
    }

    pub(crate) fn schedule_commit(&mut self, ctx: &egui::Context) {
        self.commit_due = Some(ctx.input(|i| i.time) + 0.3);
        ctx.request_repaint_after(std::time::Duration::from_millis(350));
    }

    pub(crate) fn flush_commit(&mut self) {
        self.commit_due = None;
        self.sync_generated();
        self.history.commit(self.doc.snapshot());
    }

    fn undo(&mut self) {
        self.flush_commit();
        match self.history.undo().cloned() {
            Some(state) => {
                self.doc.restore(state);
                self.design.invalidate();
                self.log("Undo");
            }
            None => self.status = "Nothing to undo.".into(),
        }
    }

    fn redo(&mut self) {
        self.flush_commit();
        match self.history.redo().cloned() {
            Some(state) => {
                self.doc.restore(state);
                self.design.invalidate();
                self.log("Redo");
            }
            None => self.status = "Nothing to redo.".into(),
        }
    }

    pub(crate) fn log(&mut self, line: impl Into<String>) {
        self.cmd_log.push(line.into());
        if self.cmd_log.len() > 200 {
            self.cmd_log.remove(0);
        }
    }

    // ─── files ───────────────────────────────────────────────────────────────

    fn title(&self) -> String {
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string());
        format!("{} - AfroBIM", name.unwrap_or_else(|| "Untitled".into()))
    }

    /// A different document: the coordination, programme and estimate state is read afresh.
    fn reset_records(&mut self) {
        let markers = self.coord.markers;
        self.coord = CoordUi::default();
        self.coord.markers = markers;
        self.sched = SchedUi::default();
        self.cost = CostUi::default();
        self.construction = ConstructionUi::default();
        self.checks = CheckUi::default();
        self.design = Default::default();
        self.sheets = crate::sheets_ui::SheetsUi::default();
        self.corridor_cache.clear();
        // Underlays are read again from their files, and a command in progress is dropped.
        self.under.reset();
        let osnap = self.anno.osnap.clone();
        self.anno = Default::default();
        self.anno.osnap = osnap;
    }

    /// Commits to the undo history on the next frame, for edits made outside a pointer drag.
    pub(crate) fn schedule_commit_now(&mut self) {
        self.commit_due = Some(0.0);
    }

    fn open_path(&mut self, path: PathBuf) {
        match Document::load(&path) {
            Ok(doc) => {
                self.doc = doc;
                self.cache.clear();
                self.reset_records();
                let wanted = self.doc.standard.clone();
                if !wanted.is_empty() && wanted != self.standard && signs_glue::STANDARDS.iter().any(|(id, _, _)| *id == wanted) {
                    self.set_standard(&wanted);
                }
                self.sync_generated();
                self.history.reset(self.doc.snapshot());
                self.status = format!("Opened {} ({} elements).", path.display(), self.doc.elements().len());
                self.path = Some(path);
                self.fitted = false;
            }
            Err(e) => self.status = format!("Could not open {}: {e}", path.display()),
        }
    }

    fn save_to(&mut self, path: PathBuf) {
        self.flush_commit();
        match self.doc.save(&path) {
            Ok(()) => {
                self.status = format!("Saved {}.", path.display());
                self.path = Some(path);
            }
            Err(e) => self.status = format!("Could not save {}: {e}", path.display()),
        }
    }

    /// Writes the selected elements (or every element when nothing is selected) to one DXF, each
    /// at its model position, in millimetres.
    fn export_dxf(&mut self, path: PathBuf) {
        self.sync_generated();
        let selected_only = !self.doc.selection().is_empty();
        let mut dxf = afrobim_dxf::DxfDocument::new();
        let (mut written, mut skipped) = (0, 0);
        for e in self.doc.elements() {
            if selected_only && !self.doc.is_selected(e.id) {
                continue;
            }
            // Annotation is written by its own engine (real TEXT, arrows as SOLID); underlay and
            // settings records have no drawing.
            if afrobim_annotate::is_annotation(e) || e.category == crate::underlay::CATEGORY || e.category == afrobim_annotate::SETTINGS_CATEGORY || e.category == afrobim_kernel::PROJECT_SETTINGS_CATEGORY {
                continue;
            }
            match self.cache.get(&e.id).and_then(|c| c.generated.as_ref().ok()) {
                Some(g) => {
                    dxf.add(&g.drawing, 1.0, e.origin);
                    written += 1;
                }
                None => skipped += 1,
            }
        }
        // With a selection only the selected annotation is written: a copy of the document
        // without the others goes to the engine.
        let mut ann_doc = Document::from_json(&self.doc.to_json()).unwrap_or_default();
        if selected_only {
            let drop: Vec<ElementId> = ann_doc.elements().iter().filter(|e| afrobim_annotate::is_annotation(e) && !self.doc.is_selected(e.id)).map(|e| e.id).collect();
            for id in drop {
                ann_doc.remove(id);
            }
        }
        let ann_report = afrobim_annotate::export_dxf(&ann_doc, &mut dxf);
        written += ann_report.annotations;
        match std::fs::write(&path, dxf.finish()) {
            Ok(()) => {
                self.status = format!("Exported {written} element(s) to {}.", path.display());
                if skipped > 0 {
                    self.status.push_str(&format!(" {skipped} without geometry were left out."));
                }
                for w in ann_report.warnings.iter().take(3) {
                    self.status.push_str(&format!(" {w}"));
                }
            }
            Err(e) => self.status = format!("Could not write {}: {e}", path.display()),
        }
    }

    /// Writes the whole model as IFC 4.3: signs with their artwork, posts and foundations as
    /// solids aggregated with their sign, elements at their site positions, and alignments.
    /// Elements not placed on site stand upright as on the sheet (see the afrobim-ifc README).
    pub(crate) fn export_ifc(&mut self, path: PathBuf) {
        self.sync_generated();
        let inputs: Vec<afrobim_ifc::IfcElementInput> = self
            .doc
            .elements()
            .iter()
            .filter(|e| e.category != crate::reference_models::CATEGORY)
            .map(|e| afrobim_ifc::IfcElementInput { element: e, generated: self.cache.get(&e.id).and_then(|c| c.generated.as_ref().ok()) })
            .collect();
        // The project name scopes the GlobalIds, so use the document's file name: re-exporting
        // the same saved document keeps every element's identity.
        let project = self.ifc_project_name();
        let options = afrobim_ifc::IfcOptions {
            project_name: project,
            application: format!("AfroBIM {}", env!("CARGO_PKG_VERSION")),
            alignment: crate::ifc_options::mode_of(&self.doc),
            ..afrobim_ifc::IfcOptions::default()
        };
        let export = afrobim_ifc::export_ifc_model(&self.doc, &inputs, &options);
        match std::fs::write(&path, export.text) {
            Ok(()) => {
                self.last_ifc_name = path.file_name().map(|n| n.to_string_lossy().to_string());
                self.status = format!("Exported {} element(s) to {}.", inputs.len(), path.display());
                for note in export.notes.iter().take(5) {
                    self.log(format!("IFC: {note}"));
                }
                if !export.notes.is_empty() {
                    self.status.push_str(&format!(" {} note(s) in the command history.", export.notes.len()));
                }
            }
            Err(e) => self.status = format!("Could not write {}: {e}", path.display()),
        }
    }

    /// The project name the IFC export uses (it scopes the GlobalIds), also the one BCF files use.
    pub(crate) fn ifc_project_name(&self) -> String {
        self.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "AfroBIM model".into())
    }

    /// The signs an export covers: the selection, or every sign when nothing is selected.
    fn export_signs(&self) -> Vec<afrobim_signs::SignDefinition> {
        let selected_only = !self.doc.selection().is_empty();
        let elements: Vec<&Element> = self.doc.elements().iter().filter(|e| !selected_only || self.doc.is_selected(e.id)).collect();
        signs_glue::definitions(&elements)
    }

    /// Writes the quantity CSVs of the road modules (drainage, gantries, lighting, restraint,
    /// pavement, corridors) beside `path`, and says in the status line what was written and what
    /// could not be measured.
    pub(crate) fn write_module_quantities(&mut self, path: &std::path::Path) {
        let (files, problems) = modules_glue::quantities(&self.doc);
        for (suffix, csv) in files {
            match std::fs::write(path.with_extension(&suffix), csv) {
                Ok(()) => self.status.push_str(&format!(" Wrote {}.", path.with_extension(&suffix).display())),
                Err(e) => self.status.push_str(&format!(" {suffix} not written: {e}.")),
            }
        }
        for problem in problems {
            self.status.push_str(&format!(" {problem}."));
        }
    }

    /// Writes the bill-of-quantities CSV and, beside it, `<name>.summary.csv` by ground colour.
    pub(crate) fn export_quantities(&mut self, path: PathBuf) {
        let Some(engine) = self.engine.clone() else {
            self.status = self.module_status.clone();
            return;
        };
        let signs = self.export_signs();
        if signs.is_empty() {
            self.status = "Nothing to measure.".into();
            return;
        }
        let (csv, summary, count, area) = signs_glue::quantities(&engine, &signs);
        let summary_path = path.with_extension("summary.csv");
        let mut supports_note = String::new();
        match supports_glue::quantities(&self.doc) {
            Some(Ok((posts, foundations, totals))) => {
                let written = std::fs::write(path.with_extension("posts.csv"), posts)
                    .and_then(|()| std::fs::write(path.with_extension("foundations.csv"), foundations))
                    .and_then(|()| std::fs::write(path.with_extension("supports.csv"), totals));
                supports_note = match written {
                    Ok(()) => " Posts and foundations measured beside it.".into(),
                    Err(e) => format!(" Support quantities not written: {e}."),
                };
            }
            Some(Err(e)) => supports_note = format!(" Support quantities not written: {e}."),
            None => {}
        }
        match std::fs::write(&path, csv).and_then(|()| std::fs::write(&summary_path, summary)) {
            Ok(()) => self.status = format!("Measured {count} sign face(s), {area:.3} m², to {} and {}.{supports_note}", path.display(), summary_path.display()),
            Err(e) => self.status = format!("Could not write {}: {e}", path.display()),
        }
    }

    /// Writes an A3 fabrication sheet per sign (DXF and PDF) and the schedule PDF into a folder.
    pub(crate) fn export_sheets(&mut self, dir: PathBuf) {
        let Some(engine) = self.engine.clone() else {
            self.status = self.module_status.clone();
            return;
        };
        let signs = self.export_signs();
        if signs.is_empty() {
            self.status = "Nothing to draw.".into();
            return;
        }
        let drawn_by = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default();
        self.status = match signs_glue::fabrication_sheets(&engine, &self.fonts_dir, signs, &dir, &drawn_by) {
            Ok(summary) => summary,
            Err(e) => format!("Fabrication sheets failed: {e}"),
        };
    }

    fn file_dialog() -> rfd::FileDialog {
        rfd::FileDialog::new().add_filter("AfroBIM document", &[FILE_EXTENSION])
    }

    // ─── commands ────────────────────────────────────────────────────────────

    pub(crate) fn run(&mut self, ctx: &egui::Context, action: Action) {
        self.last_action = Some(action);
        match action {
            Action::New => {
                self.doc = Document::default();
                self.cache.clear();
                self.reset_records();
                self.history.reset(Vec::new());
                self.path = None;
                self.fitted = false;
                self.status = "New document.".into();
            }
            Action::Open => {
                if let Some(path) = Self::file_dialog().pick_file() {
                    self.open_path(path);
                }
            }
            Action::Save => match self.path.clone() {
                Some(path) => self.save_to(path),
                None => self.run(ctx, Action::SaveAs),
            },
            Action::SaveAs => {
                if let Some(mut path) = Self::file_dialog().set_file_name("Untitled.afrobim").save_file() {
                    if path.extension().is_none() {
                        path.set_extension(FILE_EXTENSION);
                    }
                    self.save_to(path);
                }
            }
            Action::ExportDxf => {
                if self.doc.elements().is_empty() {
                    self.status = "Nothing to export.".into();
                } else if let Some(mut path) = rfd::FileDialog::new().add_filter("AutoCAD DXF (R12)", &["dxf"]).set_file_name("signs.dxf").save_file() {
                    if path.extension().is_none() {
                        path.set_extension("dxf");
                    }
                    self.export_dxf(path);
                }
            }
            Action::ExportIfc => {
                if self.doc.elements().is_empty() {
                    self.status = "Nothing to export.".into();
                } else {
                    // The options come first (how alignments are written); the file is chosen from there.
                    self.ifc_options_open = true;
                }
            }
            Action::ExportQuantities => {
                if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("sign-quantities.csv").save_file() {
                    if path.extension().is_none() {
                        path.set_extension("csv");
                    }
                    self.export_quantities(path.clone());
                    self.write_module_quantities(&path);
                    if let Some((schedule, rows)) = crate::ext_markings_glue::quantities(&self.doc) {
                        match std::fs::write(path.with_extension("markings2.csv"), schedule) {
                            Ok(()) => self.status.push_str(&format!(" {rows} second-set road marking(s) measured to {}.", path.with_extension("markings2.csv").display())),
                            Err(e) => self.status.push_str(&format!(" Second-set marking quantities not written: {e}.")),
                        }
                    }
                    if let Some((schedule, summary, rows)) = markings_glue::quantities(&self.doc) {
                        let written = std::fs::write(path.with_extension("markings.csv"), schedule).and_then(|()| std::fs::write(path.with_extension("markings.summary.csv"), summary));
                        match written {
                            Ok(()) => self.status.push_str(&format!(" {rows} road marking(s) measured to {}.", path.with_extension("markings.csv").display())),
                            Err(e) => self.status.push_str(&format!(" Road marking quantities not written: {e}.")),
                        }
                    }
                }
            }
            Action::ExportSheets => {
                if let Some(dir) = rfd::FileDialog::new().set_title("Folder for the fabrication sheets").pick_folder() {
                    self.export_sheets(dir);
                }
            }
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::CopyClip => {
                self.clipboard = self.doc.selection().iter().filter_map(|id| self.doc.find(*id).cloned()).collect();
                self.paste_count = 0;
                self.status = format!("{} element(s) copied.", self.clipboard.len());
            }
            Action::PasteClip => {
                if self.clipboard.is_empty() {
                    self.status = "The clipboard is empty.".into();
                } else {
                    self.paste_count += 1;
                    let shift = 300.0 * self.paste_count as f64;
                    let mut made = Vec::new();
                    for src in self.clipboard.clone() {
                        let name = self.doc.unique_name(&src.name);
                        made.push(self.doc.add_copy(&src, &name, Pt::new(src.origin.x + shift, src.origin.y - shift)));
                    }
                    self.doc.select(&made, SelectMode::Replace);
                    self.schedule_commit(ctx);
                }
            }
            Action::Move => self.ctl.start_move(&mut self.doc),
            Action::Copy => self.ctl.start_copy(&mut self.doc),
            Action::Erase => self.ctl.start_erase(&mut self.doc),
            Action::SelectAll => self.doc.select_all(),
            Action::ZoomExtents if self.view_3d => self.fit_3d(),
            Action::ZoomExtents => self.zoom_extents_visible(),
            Action::ZoomWindow => self.ctl.start_zoom_window(),
            Action::ZoomIn if self.view_3d => self.camera.zoom(1.5),
            Action::ZoomOut if self.view_3d => self.camera.zoom(1.0 / 1.5),
            Action::ZoomIn => self.ctl.zoom_by(&mut self.view, 1.5),
            Action::ZoomOut => self.ctl.zoom_by(&mut self.view, 1.0 / 1.5),
            Action::ToggleGrid => self.grid_on = !self.grid_on,
            Action::ToggleSnap => self.ctl.snap_on = !self.ctl.snap_on,
            Action::ToggleOrtho => self.ctl.ortho_on = !self.ctl.ortho_on,
            Action::RepReal => self.rep = Representation::Real,
            Action::RepOutlined => self.rep = Representation::Outlined,
            Action::ToggleCanvas => self.dark_canvas = !self.dark_canvas,
            Action::ToggleMarks => self.show_labels = !self.show_labels,
            Action::ToggleTheme => {
                self.dark_theme = !self.dark_theme;
                ctx.set_visuals(if self.dark_theme { egui::Visuals::dark() } else { egui::Visuals::light() });
            }
            Action::Toggle3d => {
                self.view_3d = !self.view_3d;
                if self.view_3d {
                    self.design.plan_on = false;
                }
                self.ctl.escape(&mut self.doc);
                if self.view_3d {
                    self.fit_3d();
                }
            }
            Action::Arrange => {
                self.doc.arrange();
                self.zoom_extents_visible();
                self.schedule_commit(ctx);
            }
            Action::InsertSign(index) => {
                if !self.signs_ready {
                    self.status = self.module_status.clone();
                } else if let Some(t) = signs_glue::templates(&self.standard).into_iter().nth(index as usize) {
                    self.insert_sign(ctx, t.mark, t.data);
                }
            }
            Action::Library => self.show_library = true,
            Action::ImportLandXml => {
                if let Some(path) = rfd::FileDialog::new().add_filter("LandXML", &["xml"]).pick_file() {
                    self.import_landxml(ctx, path);
                }
            }
            Action::PlaceOnAlignment => match (self.selected_sign(), alignment_glue::alignments(&self.doc).first().map(|a| a.0)) {
                (Some(sign), Some(first)) => {
                    let known = self.doc.find(sign).and_then(|e| e.site.as_ref()).map(|s| (s.station_m.unwrap_or(0.0), s.offset_m.unwrap_or(3.0)));
                    let (station, offset) = known.unwrap_or((0.0, 3.0));
                    self.station_dialog = Some((sign, Some(first), station, offset.abs(), offset > 0.0, 0));
                }
                (None, _) => self.status = "Select one sign first, then place it on an alignment.".into(),
                (_, None) => self.status = "There is no alignment yet. Import one with File → Import LandXML.".into(),
            },
            Action::AddSupports | Action::DesignSupports if self.selected_sign().is_some_and(|id| modules_glue::on_gantry(&self.doc, id)) => {
                self.status = "This sign hangs on a gantry, so it takes no posts. Take it off the gantry first (UNHANG).".into();
            }
            Action::AddSupports => match self.selected_sign() {
                Some(id) => self.supports_dialog = Some((id, String::new(), 2, 150.0)),
                None => self.status = "Select one sign first, then add its supports.".into(),
            },
            Action::DesignSupports => match self.selected_sign() {
                Some(id) => {
                    // The entries kept on the sign by an earlier design open again.
                    let stored = crate::check_inputs::load::<supports_glue::StoredSupportDesign>(&self.doc, id, crate::check_inputs::KEY_SUPPORT).and_then(|s| s.request().ok());
                    if let Some(req) = stored {
                        let entered = if let supports_glue::WindChoice::Entered(v) = req.wind { v } else { 1.0 };
                        self.design_dialog = Some((id, req.mounting_height_mm.to_string(), entered, req));
                    } else {
                        let (height, posts) = supports_glue::present(&self.doc, id).map(|(h, n)| (h.to_string(), n)).unwrap_or((String::new(), 1));
                        self.design_dialog = Some((id, height, 1.0, supports_glue::DesignRequest::starting(0.0, posts)));
                    }
                }
                None => self.status = "Select one sign first, then design its supports.".into(),
            },
            Action::AddExtMarking(g) => self.open_ext_marking(crate::ext_markings_glue::Group::ALL[(g as usize).min(6)]),
            Action::AddMarking => match alignment_glue::alignments(&self.doc).first().map(|a| a.0) {
                Some(first) => {
                    let (start, end) = alignment_glue::alignment_of(&self.doc, first).map(|a| (a.start_station_m, a.end_station_m())).unwrap_or((0.0, 0.0));
                    let request = markings_glue::MarkingRequest { line: 0, start_station_m: start, end_station_m: end, distance_m: 0.0, right: false, speed_mph: 30, speed_kmh: None, width_mm: None, material: String::new(), basis_uganda: false, uk_fallback: None };
                    self.marking_dialog = Some((Some(first), request));
                }
                None => self.status = "There is no alignment yet. Import one with File → Import LandXML.".into(),
            },
            Action::AddRoadElement(kind) => match alignment_glue::alignments(&self.doc).first().map(|a| a.0) {
                Some(first) => {
                    let (start, end) = alignment_glue::alignment_of(&self.doc, first).map(|a| (a.start_station_m, a.end_station_m())).unwrap_or((0.0, 0.0));
                    self.road_dialog = Some((kind as usize, Some(first), start, end, 5.0, false));
                }
                None => self.status = "There is no alignment yet. Import one with File → Import LandXML.".into(),
            },
            Action::AddLighting => {
                if let Err(e) = self.road.open_lighting(&self.doc) {
                    self.status = e;
                }
            }
            Action::AddLightColumn => {
                if let Err(e) = self.road.open_column(&self.doc) {
                    self.status = e;
                }
            }
            Action::AddBarrier => {
                if let Err(e) = self.road.open_barrier(&self.doc) {
                    self.status = e;
                }
            }
            Action::AddPavement => {
                if let Err(e) = self.road.open_pavement(&self.doc) {
                    self.status = e;
                }
            }
            Action::PavementDesign => {
                let selected = self.doc.primary();
                self.road.open_design(&self.doc, selected);
            }
            Action::AddCorridor => {
                if let Err(e) = self.road.open_corridor(&self.doc) {
                    self.status = e;
                }
            }
            Action::ShowSections => match self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == afrobim_corridor::CORRIDOR_CATEGORY)) {
                Some(id) => {
                    if let Err(e) = self.road.open_sections(&self.doc, id) {
                        self.status = format!("No sections to show: {e}");
                    }
                }
                None => self.status = "Select a corridor first. Build one with CORRIDOR.".into(),
            },
            Action::ImportSurface => {
                if let Some(path) = rfd::FileDialog::new().add_filter("LandXML", &["xml"]).pick_file() {
                    self.import_surface(ctx, path);
                }
            }
            Action::CheckElement => match self.doc.primary() {
                Some(id) => match modules_glue::check(&self.doc, id) {
                    modules_glue::Check::Lines(Ok(lines)) => {
                        let mark = self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
                        self.status = format!("Checked {mark}. Read the calculation.");
                        self.design_report = Some((mark, lines));
                    }
                    modules_glue::Check::Lines(Err(e)) => self.status = format!("Not checked: {e}"),
                    modules_glue::Check::DrainFlow => self.checks.open_drain(&self.doc, id),
                    modules_glue::Check::CulvertInputs => self.checks.open_culvert(&self.doc, id),
                    modules_glue::Check::RestraintSpeed => self.road.open_speed(&self.doc, id),
                    modules_glue::Check::LightingInputs => self.road.open_lighting_check(&self.doc, id),
                    modules_glue::Check::PavementDesign => self.road.open_design(&self.doc, Some(id)),
                    modules_glue::Check::None => self.status = "Select a gantry, side drain, culvert, street lighting, barrier run or pavement to check. Sign supports are checked with DESIGN. CHECKALL checks the whole model.".into(),
                },
                None => self.status = "Select a gantry, side drain, culvert, street lighting, barrier run or pavement first.".into(),
            },
            Action::HangSign => match (self.selected_sign(), modules_glue::gantries(&self.doc).first().map(|g| g.0)) {
                (Some(sign), Some(first)) => self.hang_dialog = Some((sign, Some(first), false, 1, 0.0, 0.0, true)),
                (None, _) => self.status = "Select one sign first, then hang it on a gantry.".into(),
                (_, None) => self.status = "There is no gantry yet. Add one from the Structures tab.".into(),
            },
            Action::UnhangSign => match self.selected_sign() {
                Some(sign) => match modules_glue::unhang_sign(&mut self.doc, sign) {
                    Ok(()) => {
                        self.status = "The sign is off its gantry.".into();
                        self.schedule_commit(ctx);
                    }
                    Err(e) => self.status = e,
                },
                None => self.status = "Select the sign to take off its gantry.".into(),
            },
            Action::RemoveSupports => match self.selected_sign() {
                Some(id) => {
                    let n = supports_glue::remove(&mut self.doc, id);
                    self.status = format!("Removed {n} support element(s).");
                    self.schedule_commit(ctx);
                }
                None => self.status = "Select the sign whose supports are to be removed.".into(),
            },
            Action::Standard(index) => {
                if let Some((id, label, _)) = signs_glue::STANDARDS.get(index as usize) {
                    self.set_standard(id);
                    self.log(format!("Standard: {label}"));
                }
            }
            Action::ClashTests => {
                self.coord.sync(&self.doc);
                self.coord.tests_open = true;
            }
            Action::ClashRun => self.run_clash_tests_bg(),
            Action::ClashResults => {
                self.coord.sync(&self.doc);
                self.coord.results_open = true;
                self.status = format!("{} clash(es) on record.", self.coord.co.results.clashes.len());
            }
            Action::ClashMarkers => {
                self.coord.markers = !self.coord.markers;
                self.status = format!("Clash markers {}.", if self.coord.markers { "shown in the 3D view" } else { "hidden" });
            }
            Action::ClashCsv => self.export_clashes_csv(),
            Action::Issues => self.open_issues(),
            Action::BcfOut => {
                self.coord.sync(&self.doc);
                self.issues.bcf_out_open = true;
            }
            Action::BcfIn => self.issues.bcf_in_open = true,
            Action::Visibility => self.vis_open_inputs(false),
            Action::VisibilityRun => {
                let sign = self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == afrobim_visibility::SIGN_CATEGORY));
                self.vis_run(sign.map(|id| vec![id]));
            }
            Action::Photometry => self.photo_open(),
            Action::ReportPack => self.pack_open(),
            Action::IfcRef => self.ifcref_pick(),
            Action::RefModels => self.refs.list_open = true,
            Action::Programme => {
                self.sched.sync(&self.doc);
                self.sched.table_open = true;
            }
            Action::Calendar => {
                self.sched.sync(&self.doc);
                self.sched.calendar_open = true;
            }
            Action::ScheduleRun => self.run_schedule(),
            Action::Gantt => {
                self.sched.sync(&self.doc);
                self.sched.gantt_open = true;
                if self.sched.prog.is_none() {
                    self.status = "There is no programme yet (PROGRAMME).".into();
                }
            }
            Action::SetBaseline => {
                self.sched.sync(&self.doc);
                self.sched.baseline_open = true;
            }
            Action::Variance => {
                self.sched.sync(&self.doc);
                self.sched.variance_open = true;
            }
            Action::Progress => {
                self.sched.sync(&self.doc);
                self.sched.progress_open = true;
            }
            Action::Link4d => {
                self.sched.sync(&self.doc);
                self.sched.link_open = true;
            }
            Action::LinkReport => {
                self.sched.sync(&self.doc);
                self.sched.report_open = true;
            }
            Action::FourD => crate::schedule_ui::open_date_control(self),
            Action::MspIn => self.import_mspdi_file(),
            Action::MspOut => self.export_mspdi_file(),
            Action::ProgCsv => self.export_programme_csvs(),
            Action::Bill => {
                if self.cost_open() {
                    self.cost.bill_open = true;
                }
            }
            Action::Rates => {
                if self.cost_open() {
                    self.cost.rates_open = true;
                }
            }
            Action::Currency => {
                if self.cost_open() {
                    self.cost.currency_open = true;
                }
            }
            Action::Estimate => {
                if self.cost_open() {
                    self.cost.estimate_open = true;
                }
            }
            Action::CashFlow => {
                if self.cost_open() {
                    self.cost.cash_open = true;
                    self.sched.sync(&self.doc);
                }
            }
            Action::Certificate => {
                if self.cost_open() {
                    self.cost.cert_open = true;
                    self.sched.sync(&self.doc);
                    if self.options.demo && self.options.subtab == Some(1) {
                        self.cert_demo_run();
                    }
                }
            }
            Action::CostCsv => self.export_cost_reports(),
            Action::CheckAll => self.run_check_all(),
            Action::Rational => {
                let selected = self.doc.primary();
                self.checks.open_rational(&self.doc, selected);
            }
            Action::SiteRecord | Action::SiteLog | Action::RollUp | Action::Evm | Action::LookAhead => self.construction_open(action),
            Action::SiteView => self.toggle_site_colour(),
            Action::EvmCsv => self.export_evm_csvs(),
            Action::SheetSets | Action::SheetPreview | Action::TitleBlock | Action::ProjectInfo | Action::SheetPdf | Action::SheetDxf => self.sheets_action(action),
            Action::AlignDesign => self.design_start_new(),
            Action::AlignEdit => self.design_edit_plan(),
            Action::PiTable => self.design_open_table(),
            Action::AlignLabels => {
                self.design.labels_on = !self.design.labels_on;
                self.status = format!("Curve data labels {}.", if self.design.labels_on { "shown" } else { "hidden" });
            }
            Action::ProfileDesign => self.design_open_profile(),
            Action::AlignCheck => self.design_open_check(),
            Action::LandXmlOut => self.design_landxml_out(),
            Action::Options => self.design_open_options(),
            Action::XerIn => self.import_xer_file(),
            Action::XerOut => self.export_xer_file(),
            Action::Wbs => {
                self.sched.sync(&self.doc);
                self.sched.ext.wbs_open = true;
            }
            Action::CriticalBasis => {
                self.sched.sync(&self.doc);
                self.sched.ext.critical_open = true;
            }
            Action::TimeLocation => {
                self.sched.sync(&self.doc);
                self.sched.ext.tl_open = true;
            }
            Action::Play4d => self.play_toggle(ctx),
            Action::Anno(kind) => self.anno_start(kind),
            Action::TextEdit => match self.doc.primary() {
                Some(id) => {
                    self.anno_open_text_edit(id);
                }
                None => self.status = "Select a text or a leader first, then TEXTEDIT.".into(),
            },
            Action::MeasureResults => self.anno.results_open = true,
            Action::TextStyles => self.anno.text_styles_open = true,
            Action::DimStyles => self.anno.dim_styles_open = true,
            Action::ToggleOsnap => {
                self.anno.osnap.on = !self.anno.osnap.on;
                self.status = format!("Object snap {}.", if self.anno.osnap.on { "on" } else { "off" });
            }
            Action::OsnapSettings => self.anno.osnap_open = true,
            Action::Layers => self.anno.layers_open = true,
            Action::Units => self.anno.units_open = true,
            Action::ProjectSettings => self.projset.open = true,
            Action::Underlay => self.under.window_open = true,
            Action::SignSchedule => self.signsched_pick(),
            Action::SignScheduleOut => self.signsched_out(),
            Action::SignScheduleTemplate => self.signsched_template(),
            Action::Help => self.show_help = true,
            Action::About => self.show_about = true,
            Action::Quit => self.quit = true,
        }
    }

    /// Zoom to the elements on visible layers (hidden ones, such as the alignments, which sit in
    /// site coordinates far from the sign sheet, and the records that draw nothing, are left out).
    pub(crate) fn zoom_extents_visible(&mut self) {
        let ext = self
            .doc
            .elements()
            .iter()
            .filter(|e| self.doc.layer_visible(&e.layer) && e.category != afrobim_annotate::SETTINGS_CATEGORY && e.category != crate::underlay::CATEGORY)
            .map(|e| e.bounds())
            .reduce(|a, b| a.union(&b))
            .unwrap_or(afrobim_kernel::BBox::new(0.0, 0.0, 5000.0, 3000.0));
        let (l, t, w, h) = self.ctl.viewport;
        self.view.fit(&ext, l, t, w, h, 60.0);
    }

    pub(crate) fn import_landxml(&mut self, ctx: &egui::Context, path: PathBuf) {
        match alignment_glue::import_landxml(&mut self.doc, &path) {
            Ok((added, notes)) => {
                self.design.invalidate();
                markings_glue::sync(&mut self.doc);
                for problem in modules_glue::sync_all(&mut self.doc) {
                    self.log(format!("Not brought up to date: {problem}"));
                }
                self.status = format!("Imported {added} alignment(s) from {}.", path.display());
                for note in notes.iter().take(6) {
                    self.log(format!("LandXML: {note}"));
                }
                if !notes.is_empty() {
                    self.status.push_str(&format!(" {} note(s) in the command history.", notes.len()));
                }
                self.schedule_commit(ctx);
            }
            Err(e) => self.status = format!("Could not import {}: {e}", path.display()),
        }
    }

    /// Imports the TIN surfaces of a LandXML file as ground surface elements.
    pub(crate) fn import_surface(&mut self, ctx: &egui::Context, path: PathBuf) {
        match road_glue::import_surfaces(&mut self.doc, &path) {
            Ok((added, notes)) => {
                self.sync_generated();
                self.status = format!("Imported {added} surface(s) from {}.", path.display());
                for note in notes.iter().take(6) {
                    self.log(format!("LandXML: {note}"));
                }
                if !notes.is_empty() {
                    self.status.push_str(&format!(" {} note(s) in the command history.", notes.len()));
                }
                self.fitted_3d = false;
                self.schedule_commit(ctx);
            }
            Err(e) => self.status = format!("Could not import {}: {e}", path.display()),
        }
    }

    /// The primary selection when it is a sign.
    fn selected_sign(&self) -> Option<ElementId> {
        self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == signs_glue::CATEGORY))
    }

    /// Switches the design standard: reloads the rule pack and lays every sign out again under
    /// it. A sign the new standard cannot lay out keeps its place and shows the reason.
    fn set_standard(&mut self, id: &str) {
        match signs_glue::load_module(id) {
            Ok(l) => {
                self.modules.register(l.module);
                self.signs_ready = true;
                self.module_status = l.label;
                self.library = l.library;
                self.engine = Some(l.engine);
                self.fonts_dir = l.fonts_dir;
                self.standard = id.to_string();
                self.doc.standard = id.to_string();
                self.cache.clear();
                self.sync_generated();
                self.status = format!("Standard set to {id}.");
            }
            Err(e) => self.status = format!("Could not load {id}: {e}"),
        }
    }

    /// Adds a sign with a unique mark and hangs it from the cursor for placing.
    fn insert_sign(&mut self, ctx: &egui::Context, mark: &str, data: serde_json::Value) {
        let name = self.doc.unique_name(mark);
        let at = self.doc.next_free_origin(2000.0, 1000.0);
        let id = self.doc.add(signs_glue::CATEGORY, &name, at, 2000.0, 1000.0, signs_glue::LAYER, data);
        self.sync_generated();
        self.doc.select(&[id], SelectMode::Replace);
        self.ctl.start_place(&self.doc, &self.view, id);
        self.schedule_commit(ctx);
    }

    fn submit_command_line(&mut self, ctx: &egui::Context) {
        let text = std::mem::take(&mut self.cmd_input);
        if self.design.wants_text() {
            self.log(format!("{} {}", self.design.prompt().unwrap_or_default(), text));
            self.design_take_text(&text);
            return;
        }
        if self.anno.session.is_some() {
            let prompt = self.anno.session.as_ref().map(|s| s.prompt(&self.doc)).unwrap_or_default();
            self.log(format!("{prompt} {text}"));
            self.anno_text(&text);
            return;
        }
        if self.ctl.is_busy() {
            self.log(format!("{} {}", self.ctl.prompt(), text));
            self.ctl.submit_text(&mut self.doc, &mut self.view, &text);
            return;
        }
        if text.trim().is_empty() {
            if let Some(a) = self.last_action {
                self.run(ctx, a);
            }
            return;
        }
        self.log(format!("Command: {}", text.trim()));
        match commands::resolve(&text) {
            Some(c) => self.run(ctx, c.action),
            None => self.log(format!("Unknown command \"{}\". Type ? for the list.", text.trim())),
        }
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        for event in self.ctl.take_events() {
            match event {
                CanvasEvent::Echo(s) => self.log(s),
                CanvasEvent::Committed => self.schedule_commit(ctx),
                CanvasEvent::Erase(ids) => {
                    for id in ids {
                        self.doc.remove(id);
                    }
                }
                CanvasEvent::Copy(ids, dx, dy) => {
                    for id in ids {
                        if let Some(src) = self.doc.find(id).cloned() {
                            let name = self.doc.unique_name(&src.name);
                            self.doc.add_copy(&src, &name, Pt::new(src.origin.x + dx, src.origin.y + dy));
                        }
                    }
                }
                CanvasEvent::Details(_) => {}
            }
        }
    }

    // ─── keyboard ────────────────────────────────────────────────────────────

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd_id = egui::Id::new(CMD_ID);
        let focused = ctx.memory(|m| m.focused());
        let in_field = focused.is_some_and(|f| f != cmd_id);

        let table = [
            (Modifiers::COMMAND, Key::N, Action::New),
            (Modifiers::COMMAND, Key::O, Action::Open),
            (Modifiers::COMMAND | Modifiers::SHIFT, Key::S, Action::SaveAs),
            (Modifiers::COMMAND, Key::S, Action::Save),
            (Modifiers::COMMAND, Key::Y, Action::Redo),
            (Modifiers::NONE, Key::F7, Action::ToggleGrid),
            (Modifiers::NONE, Key::F8, Action::ToggleOrtho),
            (Modifiers::NONE, Key::F9, Action::ToggleSnap),
            (Modifiers::NONE, Key::F3, Action::ToggleOsnap),
        ];
        for (m, k, a) in table {
            if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k))) {
                self.run(ctx, a);
            }
        }
        // These belong to a text field while one is being edited.
        if !in_field {
            for (k, a) in [(Key::Z, Action::Undo), (Key::A, Action::SelectAll)] {
                if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, k))) {
                    self.run(ctx, a);
                }
            }
            if self.cmd_input.is_empty() {
                let (copy, paste, delete) = ctx.input(|i| {
                    (
                        i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
                        i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))),
                        i.key_pressed(Key::Delete),
                    )
                });
                if copy {
                    self.run(ctx, Action::CopyClip);
                }
                if paste {
                    self.run(ctx, Action::PasteClip);
                }
                if delete && !self.ctl.is_busy() && !self.doc.selection().is_empty() {
                    self.ctl.start_erase(&mut self.doc);
                }
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.show_help = false;
            self.show_about = false;
            self.show_library = false;
            // Esc in a Properties field belongs to that field; it only cancels an active command.
            if !in_field || self.ctl.is_busy() {
                self.cmd_input.clear();
                if !self.anno_cancel() {
                    self.ctl.escape(&mut self.doc);
                }
                self.design_escape();
            }
        }
    }

    // ─── panels ──────────────────────────────────────────────────────────────

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                let mut pick: Option<Action> = None;
                let mut item = |ui: &mut egui::Ui, label: &str, shortcut: &str, a: Action| {
                    if ui.add(egui::Button::new(label).shortcut_text(shortcut)).clicked() {
                        pick = Some(a);
                        ui.close_menu();
                    }
                };
                ui.menu_button("File", |ui| {
                    item(ui, "New", "Ctrl+N", Action::New);
                    item(ui, "Open…", "Ctrl+O", Action::Open);
                    item(ui, "Save", "Ctrl+S", Action::Save);
                    item(ui, "Save as…", "Ctrl+Shift+S", Action::SaveAs);
                    ui.separator();
                    item(ui, "Import LandXML alignments…", "LANDXML", Action::ImportLandXml);
                    ui.separator();
                    item(ui, "Export DXF…", "DXFOUT", Action::ExportDxf);
                    item(ui, "Export IFC 4.3…", "IFCOUT", Action::ExportIfc);
                    item(ui, "Export quantities (BoQ)…", "QUANTITIES", Action::ExportQuantities);
                    item(ui, "Export fabrication sheets…", "SHEETS", Action::ExportSheets);
                    ui.separator();
                    item(ui, "Quit", "", Action::Quit);
                });
                ui.menu_button("Edit", |ui| {
                    item(ui, "Undo", "Ctrl+Z", Action::Undo);
                    item(ui, "Redo", "Ctrl+Y", Action::Redo);
                    ui.separator();
                    item(ui, "Copy", "Ctrl+C", Action::CopyClip);
                    item(ui, "Paste", "Ctrl+V", Action::PasteClip);
                    item(ui, "Erase", "Del", Action::Erase);
                    ui.separator();
                    item(ui, "Select all", "Ctrl+A", Action::SelectAll);
                });
                ui.menu_button("View", |ui| {
                    item(ui, "Zoom extents", "Z", Action::ZoomExtents);
                    item(ui, "Zoom window", "ZW", Action::ZoomWindow);
                    item(ui, "Zoom in", "", Action::ZoomIn);
                    item(ui, "Zoom out", "", Action::ZoomOut);
                    ui.separator();
                    item(ui, "Grid", "F7", Action::ToggleGrid);
                    item(ui, "Snap", "F9", Action::ToggleSnap);
                    item(ui, "Ortho", "F8", Action::ToggleOrtho);
                    ui.separator();
                    item(ui, "2D model space / 3D view", "3D", Action::Toggle3d);
                    ui.separator();
                    item(ui, "Real (full colour)", "", Action::RepReal);
                    item(ui, "Outlined", "", Action::RepOutlined);
                    ui.separator();
                    item(ui, "Dark / light model space", "", Action::ToggleCanvas);
                    item(ui, "Dark / light theme", "", Action::ToggleTheme);
                });
                ui.menu_button("Insert", |ui| {
                    for (i, t) in signs_glue::templates(&self.standard).iter().enumerate() {
                        item(ui, t.label, t.command, Action::InsertSign(i as u8));
                    }
                    ui.separator();
                    item(ui, "Standard signs…", "LIBRARY", Action::Library);
                    ui.separator();
                    item(ui, "Sign schedule from CSV…", "SIGNSCHEDULE", Action::SignSchedule);
                    item(ui, "Export the sign schedule (CSV)…", "SIGNSCHEDULEOUT", Action::SignScheduleOut);
                    item(ui, "Blank sign schedule template…", "SIGNSCHEDULETEMPLATE", Action::SignScheduleTemplate);
                    ui.separator();
                    item(ui, "DXF underlay…", "UNDERLAY", Action::Underlay);
                });
                ui.menu_button("Annotate", |ui| {
                    item(ui, "Show / hide element marks", "MARKS", Action::ToggleMarks);
                    ui.separator();
                    item(ui, "Text", "TEXT", Action::Anno(crate::anno_glue::Kind::Text));
                    item(ui, "Multiline text", "MTEXT", Action::Anno(crate::anno_glue::Kind::MText));
                    item(ui, "Edit text", "TEXTEDIT", Action::TextEdit);
                    ui.separator();
                    item(ui, "Dimension", "DIM", Action::Anno(crate::anno_glue::Kind::Dim));
                    item(ui, "Linear", "DIMLINEAR", Action::Anno(crate::anno_glue::Kind::DimLinear));
                    item(ui, "Aligned", "DIMALIGNED", Action::Anno(crate::anno_glue::Kind::DimAligned));
                    item(ui, "Angular", "DIMANGULAR", Action::Anno(crate::anno_glue::Kind::DimAngular));
                    item(ui, "Radius", "DIMRADIUS", Action::Anno(crate::anno_glue::Kind::DimRadius));
                    item(ui, "Diameter", "DIMDIAMETER", Action::Anno(crate::anno_glue::Kind::DimDiameter));
                    item(ui, "Arc length", "DIMARC", Action::Anno(crate::anno_glue::Kind::DimArc));
                    item(ui, "Continue", "DIMCONTINUE", Action::Anno(crate::anno_glue::Kind::DimContinue));
                    item(ui, "Baseline", "DIMBASELINE", Action::Anno(crate::anno_glue::Kind::DimBaseline));
                    item(ui, "Station and offset", "DIMSTATION", Action::Anno(crate::anno_glue::Kind::DimStation));
                    item(ui, "Level", "DIMLEVEL", Action::Anno(crate::anno_glue::Kind::DimLevel));
                    ui.separator();
                    item(ui, "Leader", "LEADER", Action::Anno(crate::anno_glue::Kind::Leader));
                    item(ui, "Mark leader", "MLEADER", Action::Anno(crate::anno_glue::Kind::MLeader));
                    ui.separator();
                    item(ui, "Distance", "DIST", Action::Anno(crate::anno_glue::Kind::Dist));
                    item(ui, "Angle", "MEASUREANGLE", Action::Anno(crate::anno_glue::Kind::MeasureAngle));
                    item(ui, "Radius of three points", "MEASURERADIUS", Action::Anno(crate::anno_glue::Kind::MeasureRadius));
                    item(ui, "Area", "AREA", Action::Anno(crate::anno_glue::Kind::Area));
                    item(ui, "ID point", "ID", Action::Anno(crate::anno_glue::Kind::Id));
                    item(ui, "Measure results…", "MEASURERESULTS", Action::MeasureResults);
                    ui.separator();
                    item(ui, "Text styles…", "STYLE", Action::TextStyles);
                    item(ui, "Dimension styles…", "DIMSTYLE", Action::DimStyles);
                    item(ui, "Object snap on / off", "OSNAP", Action::ToggleOsnap);
                    item(ui, "Object snap settings…", "OSNAPSETTINGS", Action::OsnapSettings);
                });
                ui.menu_button("Format", |ui| {
                    for (i, (id, label, _)) in signs_glue::STANDARDS.iter().enumerate() {
                        let mark = if *id == self.standard { "● " } else { "   " };
                        item(ui, &format!("{mark}{label}"), id, Action::Standard(i as u8));
                    }
                });
                ui.menu_button("Modify", |ui| {
                    item(ui, "Place the sign on an alignment…", "STATION", Action::PlaceOnAlignment);
                    item(ui, "Add supports to the sign…", "SUPPORTS", Action::AddSupports);
                    item(ui, "Design the sign's supports…", "DESIGN", Action::DesignSupports);
                    item(ui, "Remove the sign's supports", "NOSUPPORTS", Action::RemoveSupports);
                    ui.separator();
                    item(ui, "Move", "M", Action::Move);
                    item(ui, "Copy", "CO", Action::Copy);
                    item(ui, "Erase", "E", Action::Erase);
                    ui.separator();
                    item(ui, "Arrange", "", Action::Arrange);
                });
                ui.menu_button("Manage", |ui| {
                    for (i, (id, label, _)) in signs_glue::STANDARDS.iter().enumerate() {
                        let mark = if *id == self.standard { "● " } else { "   " };
                        item(ui, &format!("{mark}Design standard: {label}"), id, Action::Standard(i as u8));
                    }
                    ui.separator();
                    item(ui, "Check the whole model…", "CHECKALL", Action::CheckAll);
                    item(ui, "Rational method…", "RATIONAL", Action::Rational);
                    ui.separator();
                    item(ui, "Layer manager…", "LAYER", Action::Layers);
                    item(ui, "Units…", "UNITS", Action::Units);
                    item(ui, "Project settings…", "PROJECTSETTINGS", Action::ProjectSettings);
                    ui.separator();
                    item(ui, "Dark / light theme", "THEME", Action::ToggleTheme);
                    item(ui, "Dark / light model space", "CANVAS", Action::ToggleCanvas);
                });
                ui.menu_button("Coordination", |ui| {
                    item(ui, "Clash tests…", "CLASHTESTS", Action::ClashTests);
                    item(ui, "Run the clash tests", "CLASH", Action::ClashRun);
                    item(ui, "Clash results…", "CLASHES", Action::ClashResults);
                    item(ui, "Show / hide clash markers", "CLASHMARKERS", Action::ClashMarkers);
                    item(ui, "Export clashes (CSV)…", "CLASHCSV", Action::ClashCsv);
                    ui.separator();
                    item(ui, "Issues…", "ISSUES", Action::Issues);
                    item(ui, "Export issues as BCF…", "BCFOUT", Action::BcfOut);
                    item(ui, "Import issues from BCF…", "BCFIN", Action::BcfIn);
                    ui.separator();
                    item(ui, "IFC reference model…", "IFCREF", Action::IfcRef);
                    item(ui, "Reference models…", "REFMODELS", Action::RefModels);
                });
                ui.menu_button("4D", |ui| {
                    item(ui, "Activities and logic…", "PROGRAMME", Action::Programme);
                    item(ui, "Calendar…", "CALENDAR", Action::Calendar);
                    item(ui, "Schedule (critical path)", "SCHEDULE", Action::ScheduleRun);
                    item(ui, "Gantt chart", "GANTT", Action::Gantt);
                    ui.separator();
                    item(ui, "Set baseline…", "BASELINE", Action::SetBaseline);
                    item(ui, "Variance", "VARIANCE", Action::Variance);
                    item(ui, "Progress and data date…", "PROGRESS", Action::Progress);
                    ui.separator();
                    item(ui, "Link elements to activities…", "LINK4D", Action::Link4d);
                    item(ui, "Link report", "LINKREPORT", Action::LinkReport);
                    item(ui, "4D date control", "FOURD", Action::FourD);
                    ui.separator();
                    item(ui, "Import MS Project XML…", "MSPIN", Action::MspIn);
                    item(ui, "Export MS Project XML…", "MSPOUT", Action::MspOut);
                    item(ui, "Export programme reports (CSV)…", "PROGCSV", Action::ProgCsv);
                });
                ui.menu_button("5D", |ui| {
                    item(ui, "Bill and mapping rules…", "BILL", Action::Bill);
                    item(ui, "Rate library…", "RATES", Action::Rates);
                    item(ui, "Currency…", "CURRENCY", Action::Currency);
                    item(ui, "Estimate…", "ESTIMATE", Action::Estimate);
                    item(ui, "Cash flow…", "CASHFLOW", Action::CashFlow);
                    item(ui, "Interim payment certificate…", "CERTIFICATE", Action::Certificate);
                    ui.separator();
                    item(ui, "Export cost reports (CSV)…", "COSTCSV", Action::CostCsv);
                });
                ui.menu_button("Construction", |ui| {
                    item(ui, "Record site progress…", "SITEREC", Action::SiteRecord);
                    item(ui, "Site records…", "SITELOG", Action::SiteLog);
                    ui.separator();
                    item(ui, "Roll up and update the programme…", "ROLLUP", Action::RollUp);
                    item(ui, "Look-ahead and delays…", "LOOKAHEAD", Action::LookAhead);
                    item(ui, "Earned value…", "EVM", Action::Evm);
                    ui.separator();
                    item(ui, "Colour the 3D view by recorded progress", "SITEVIEW", Action::SiteView);
                    item(ui, "Export earned value tables (CSV)…", "EVMCSV", Action::EvmCsv);
                });
                ui.menu_button("Help", |ui| {
                    item(ui, "Command list", "?", Action::Help);
                    item(ui, "About AfroBIM", "", Action::About);
                });
                if let Some(a) = pick {
                    self.run(ctx, a);
                }
            });
        });
    }

    /// The ribbon: every command as an icon button, grouped in tabs and panels.
    fn toolbar(&mut self, ctx: &egui::Context) {
        let (grid, snap, ortho, marks, three_d, real) = (self.grid_on, self.ctl.snap_on, self.ctl.ortho_on, self.show_labels, self.view_3d, self.rep == Representation::Real);
        let (clash_markers, fourd) = (self.coord.markers, self.sched.fourd_on);
        let playing = self.sched.ext.play.playing;
        let site_colour = self.construction.colour_by_record;
        let align_labels = self.design.labels_on;
        let standard = self.standard.clone();
        let is_on = move |a: Action| match a {
            Action::ToggleGrid => grid,
            Action::ToggleSnap => snap,
            Action::ToggleOrtho => ortho,
            Action::ToggleMarks => marks,
            Action::Toggle3d => three_d,
            Action::RepReal => real,
            Action::RepOutlined => !real,
            Action::ClashMarkers => clash_markers,
            Action::FourD => fourd,
            Action::Play4d => playing,
            Action::SiteView => site_colour,
            Action::AlignLabels => align_labels,
            Action::Standard(i) => signs_glue::STANDARDS.get(i as usize).is_some_and(|s| s.0 == standard),
            _ => false,
        };
        let mut tab = self.ribbon_tab;
        let mut pick = None;
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(2.0);
            pick = crate::ribbon::show(ui, &mut tab, &self.standard, self.signs_ready, &is_on);
            ui.add_space(1.0);
        });
        self.ribbon_tab = tab;
        if let Some(a) = pick {
            self.run(ctx, a);
        }
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let w = self.ctl.cursor_world();
                let units = crate::anno_ui::units_of(&self.doc);
                ui.add_sized([210.0, 16.0], egui::Label::new(RichText::new(format!("{} , {}", units.length(w.x), units.length_u(w.y))).monospace()));
                ui.separator();
                if ui.selectable_label(self.grid_on, "GRID").on_hover_text("Grid display (F7)").clicked() {
                    self.grid_on = !self.grid_on;
                }
                if ui.selectable_label(self.ctl.snap_on, "SNAP").on_hover_text("Snap picked points to a 50 mm grid (F9)").clicked() {
                    self.ctl.snap_on = !self.ctl.snap_on;
                }
                if ui.selectable_label(self.ctl.ortho_on, "ORTHO").on_hover_text("Hold moves to the X or Y axis (F8)").clicked() {
                    self.ctl.ortho_on = !self.ctl.ortho_on;
                }
                if ui.selectable_label(self.anno.osnap.on, "OSNAP").on_hover_text("Object snap for annotation commands (F3). OSNAPSETTINGS says which modes and what provides them").clicked() {
                    self.anno.osnap.on = !self.anno.osnap.on;
                }
                if crate::anno_ui::scale_control(ui, &mut self.doc) {
                    self.schedule_commit_now();
                }
                ui.separator();
                if ui.selectable_label(!self.view_3d, "2D").on_hover_text("Model space: select, move, edit").clicked() && self.view_3d {
                    self.view_3d = false;
                }
                if ui.selectable_label(self.view_3d, "3D").on_hover_text("3D view: orbit, pan, zoom").clicked() && !self.view_3d {
                    self.view_3d = true;
                    self.fit_3d();
                }
                ui.separator();
                ui.selectable_value(&mut self.rep, Representation::Real, "Real");
                ui.selectable_value(&mut self.rep, Representation::Outlined, "Outlined");
                ui.separator();
                ui.label(RichText::new(&self.status).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&self.module_status).weak());
                    ui.separator();
                    ui.label(RichText::new(format!("1 px = {:.1} mm", 1.0 / self.view.scale)).monospace().weak());
                    ui.separator();
                    let sel = self.doc.selection();
                    let text = match (sel.len(), self.doc.primary().and_then(|id| self.doc.find(id))) {
                        (0, _) => "No selection".to_string(),
                        (1, Some(e)) => format!("{}  {} × {}", e.name, units.length(e.width), units.length_u(e.height)),
                        (n, _) => format!("{n} elements selected"),
                    };
                    ui.label(text);
                });
            });
        });
    }

    fn left_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("tools").resizable(true).default_width(220.0).min_width(180.0).show(ctx, |ui| {
            let mut pick = None;
            ui.add_space(4.0);
            ui.label(RichText::new("TOOL SETS").small().strong().weak());
            ui.separator();
            ui.label(RichText::new("Traffic signs").small().weak());
            ui.add_enabled_ui(self.signs_ready, |ui| {
                for (i, t) in signs_glue::templates(&self.standard).iter().enumerate() {
                    if icons::palette_button(ui, t.icon, t.label, t.tip).clicked() {
                        pick = Some(Action::InsertSign(i as u8));
                    }
                }
            });
            ui.add_enabled_ui(self.signs_ready, |ui| {
                if icons::palette_button(ui, Icon::Warning, "Standard signs…", "Warning, regulatory, information and other catalogued signs").clicked() {
                    pick = Some(Action::Library);
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Modify").small().weak());
            for (icon, label, tip, a) in [
                (Icon::Move, "Move", "MOVE (M)", Action::Move),
                (Icon::Copy, "Copy", "COPY (CO)", Action::Copy),
                (Icon::Erase, "Erase", "ERASE (E or Delete)", Action::Erase),
                (Icon::Arrange, "Arrange", "Arrange every element in rows", Action::Arrange),
            ] {
                if icons::palette_button(ui, icon, label, tip).clicked() {
                    pick = Some(a);
                }
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Zoom").small().weak());
            for (icon, label, tip, a) in [
                (Icon::ZoomExtents, "Extents", "Zoom extents (Z)", Action::ZoomExtents),
                (Icon::ZoomWindow, "Window", "Zoom window (ZW)", Action::ZoomWindow),
            ] {
                if icons::palette_button(ui, icon, label, tip).clicked() {
                    pick = Some(a);
                }
            }

            ui.add_space(10.0);
            ui.label(RichText::new("MODEL").small().strong().weak());
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let categories: Vec<(String, String)> = self.modules.categories().map(|(c, n)| (c.to_string(), n.to_string())).collect();
                let mut select = None;
                for (category, title) in categories {
                    let count = self.doc.elements().iter().filter(|e| e.category == category).count();
                    egui::CollapsingHeader::new(format!("{title} ({count})")).default_open(true).show(ui, |ui| {
                        for e in self.doc.elements().iter().filter(|e| e.category == category) {
                            if ui.selectable_label(self.doc.is_selected(e.id), &e.name).clicked() {
                                select = Some(e.id);
                            }
                        }
                    });
                }
                if let Some(id) = select {
                    self.doc.select(&[id], SelectMode::Replace);
                }
            });
            if let Some(a) = pick {
                self.run(ctx, a);
            }
        });
    }

    fn right_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("properties").resizable(true).default_width(330.0).min_width(260.0).show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("PROPERTIES").small().strong().weak());
            ui.separator();
            let mut definition_edited: Option<ElementId> = None;
            let mut changed = false;
            egui::ScrollArea::vertical().id_salt("props").max_height((ui.available_height() - 170.0).max(120.0)).auto_shrink([false, false]).show(ui, |ui| {
                let selected = self.doc.selection().len();
                let Some(id) = self.doc.primary() else {
                    ui.label(RichText::new("Nothing selected. Click an element in the model.").weak());
                    return;
                };
                if selected > 1 {
                    ui.label(RichText::new(format!("{selected} elements selected. Showing the last one picked.")).weak());
                }
                let layers: Vec<String> = self.doc.layers().iter().map(|l| l.name.clone()).filter(|n| !crate::anno_ui::hidden_layer(n)).collect();
                let units = crate::anno_ui::units_of(&self.doc);
                let ann_settings = afrobim_annotate::settings::load(&self.doc);
                let align_names = crate::anno_ui::alignment_names(&self.doc);
                let module_name = self.doc.find(id).and_then(|e| self.modules.get(&e.category)).map(|m| m.display_name().to_string());
                let derived = self.cache.get(&id).and_then(|c| c.generated.as_ref().ok()).map(|g| (g.derived.clone(), g.notes.clone()));
                let error = self.cache.get(&id).and_then(|c| c.generated.as_ref().err().cloned());
                let Some(e) = self.doc.find_mut(id) else { return };

                egui::Grid::new("identity").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Mark");
                    changed |= ui.text_edit_singleline(&mut e.name).changed();
                    ui.end_row();
                    ui.label("Category");
                    ui.label(module_name.unwrap_or_else(|| e.category.clone()));
                    ui.end_row();
                    ui.label("Layer");
                    egui::ComboBox::from_id_salt("layer").selected_text(e.layer.clone()).show_ui(ui, |ui| {
                        for l in &layers {
                            changed |= ui.selectable_value(&mut e.layer, l.clone(), l).changed();
                        }
                    });
                    ui.end_row();
                    ui.label("Position X");
                    changed |= ui.add(egui::DragValue::new(&mut e.origin.x).speed(10.0).custom_formatter(|v, _| units.length_u(v)).custom_parser(|t| units.parse_length(t).ok())).changed();
                    ui.end_row();
                    ui.label("Position Y");
                    changed |= ui.add(egui::DragValue::new(&mut e.origin.y).speed(10.0).custom_formatter(|v, _| units.length_u(v)).custom_parser(|t| units.parse_length(t).ok())).changed();
                    ui.end_row();
                    ui.label("Size");
                    ui.label(format!("{} × {}", units.length(e.width), units.length_u(e.height)));
                    ui.end_row();
                });

                ui.add_space(8.0);
                ui.label(RichText::new("Definition").strong());
                if e.category == signs_glue::CATEGORY {
                    changed |= signs_glue::edit_ui(ui, &mut e.data);
                } else if afrobim_annotate::is_annotation(e) {
                    changed |= crate::anno_ui::edit_ui(ui, e, &ann_settings, &align_names);
                } else if supports_glue::is_support(&e.category) {
                    changed |= supports_glue::edit_ui(ui, &mut e.data);
                } else if modules_glue::owns(&e.category) {
                    if supports_glue::edit_ui(ui, &mut e.data) {
                        changed = true;
                        definition_edited = Some(id);
                    }
                } else if modules_glue::is_surface(&e.category) {
                    ui.label(RichText::new("An imported ground surface. Its triangles are not edited here: import it again to change it. Its figures are under Derived.").small().weak());
                }

                if let Some((derived, notes)) = derived {
                    if !derived.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Derived").strong());
                        egui::Grid::new("derived").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                            for (k, v) in &derived {
                                ui.label(k);
                                ui.label(v);
                                ui.end_row();
                            }
                        });
                    }
                    if !notes.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Checks").strong());
                        for n in &notes {
                            let colour = match n.severity {
                                Severity::Error => Color32::from_rgb(0xEF, 0x6C, 0x63),
                                Severity::Warning => Color32::from_rgb(0xD9, 0x8A, 0x2B),
                                Severity::Info => ui.visuals().weak_text_color(),
                            };
                            ui.label(RichText::new(format!("{}  {}", n.code, n.message)).color(colour).small());
                        }
                    }
                }
                if let Some(err) = error {
                    ui.add_space(8.0);
                    ui.label(RichText::new(err).color(Color32::from_rgb(0xEF, 0x6C, 0x63)));
                }

                ui.add_space(8.0);
                ui.label(RichText::new("Property set").strong());
                let mut remove = None;
                egui::Grid::new("pset").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
                    // The entries the check dialogs and the construction windows keep are theirs.
                    for (k, v) in e.properties.iter_mut().filter(|(k, _)| !crate::check_inputs::is_check_key(k) && !k.starts_with("construction.")) {
                        ui.label(k);
                        changed |= ui.text_edit_singleline(v).changed();
                        if ui.small_button("x").on_hover_text("Remove this property").clicked() {
                            remove = Some(k.clone());
                        }
                        ui.end_row();
                    }
                });
                if let Some(k) = remove {
                    e.properties.remove(&k);
                    changed = true;
                }
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.new_prop_key).hint_text("New property, e.g. Material").desired_width(170.0));
                    let key = self.new_prop_key.trim().to_string();
                    if ui.add_enabled(!key.is_empty() && !e.properties.contains_key(&key), egui::Button::new("Add")).clicked() {
                        e.properties.insert(key, String::new());
                        self.new_prop_key.clear();
                        changed = true;
                    }
                });
            });
            if let Some(id) = definition_edited {
                if let Some(problem) = modules_glue::after_edit(&mut self.doc, id) {
                    self.status = format!("Not applied yet: {problem}");
                }
            }
            if changed {
                // A rename or an edit can change what a mark leader or a station dimension shows.
                self.sync_generated();
                self.schedule_commit(ctx);
            }

            ui.add_space(6.0);
            ui.label(RichText::new("LAYERS").small().strong().weak());
            ui.separator();
            egui::ScrollArea::vertical().id_salt("layers").show(ui, |ui| {
                for layer in self.doc.layers_mut().iter_mut().filter(|l| !crate::anno_ui::hidden_layer(&l.name)) {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut layer.visible, "").on_hover_text("Visible");
                        ui.toggle_value(&mut layer.locked, "Lock").on_hover_text("Locked layers cannot be selected");
                        ui.label(&layer.name);
                    });
                }
                ui.checkbox(&mut self.show_labels, "Show element marks");
            });
        });
    }

    fn command_line(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("command").show(ctx, |ui| {
            let tail = self.cmd_log.iter().rev().take(3).rev().cloned().collect::<Vec<_>>().join("\n");
            ui.label(RichText::new(tail).monospace().small().weak());
            ui.horizontal(|ui| {
                let design_prompt = self.design.prompt();
                let anno_prompt = self.anno.session.as_ref().map(|s| s.prompt(&self.doc));
                let prompt = match (&design_prompt, &anno_prompt, self.ctl.prompt().is_empty()) {
                    (Some(p), _, _) => p.as_str(),
                    (None, Some(p), _) => p.as_str(),
                    (None, None, true) => "Command:",
                    (None, None, false) => self.ctl.prompt(),
                };
                ui.label(RichText::new(prompt).monospace().strong().color(Color32::from_rgb(0x4D, 0xC0, 0x7A)));
                let id = egui::Id::new(CMD_ID);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.cmd_input)
                        .id(id)
                        .font(egui::TextStyle::Monospace)
                        .frame(false)
                        .desired_width(f32::INFINITY)
                        .hint_text("? commands  ·  Enter repeats  ·  Esc cancels"),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    self.submit_command_line(ctx);
                }
                // Typing over the drawing goes to the command line, as in any CAD package.
                if ctx.memory(|m| m.focused().is_none()) {
                    ctx.memory_mut(|m| m.request_focus(id));
                }
            });
        });
    }

    fn canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            // Every window opens inside this area, below the menu and the ribbon (see `win`).
            crate::win::set_work_rect(ctx, ui.max_rect());
            if self.view_3d {
                self.canvas_3d(ui);
                return;
            }
            if self.design.plan_on {
                self.canvas_plan(ui);
                return;
            }
            let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let rect = response.rect;
            self.ctl.viewport = (rect.left() as f64, rect.top() as f64, rect.width() as f64, rect.height() as f64);
            if !self.fitted && rect.width() > 50.0 {
                self.fitted = true;
                self.zoom_extents_visible();
                // `--zoom-to x,y,width`: the centre and the width to show, millimetres.
                if let Some(spec) = self.options.zoom_to.take() {
                    let v: Vec<f64> = spec.split(',').filter_map(|t| t.trim().parse().ok()).collect();
                    if let [x, y, w] = v[..] {
                        let scale = rect.width() as f64 / w.max(1.0);
                        self.view.scale = scale;
                        self.view.origin_x = rect.center().x as f64 - x * scale;
                        self.view.origin_y = rect.center().y as f64 + y * scale;
                    }
                }
            }

            let was_busy = self.ctl.is_busy();
            let anno_active = self.anno.active();
            let over = response.contains_pointer();
            let input = ui.input(|i| {
                (
                    i.pointer.latest_pos(),
                    i.pointer.button_pressed(PointerButton::Primary),
                    i.pointer.button_released(PointerButton::Primary),
                    i.pointer.button_double_clicked(PointerButton::Primary),
                    i.pointer.button_down(PointerButton::Middle),
                    i.pointer.button_double_clicked(PointerButton::Middle),
                    i.pointer.button_pressed(PointerButton::Secondary),
                    i.pointer.delta(),
                    i.raw_scroll_delta.y,
                    i.modifiers.shift,
                )
            });
            let (pos, pressed, released, double, mid_down, mid_double, secondary, delta, scroll, shift) = input;

            if let Some(pos) = pos {
                let s = Pt::new(pos.x as f64, pos.y as f64);
                if over || self.canvas_drag {
                    if mid_down {
                        self.view.pan(delta.x as f64, delta.y as f64);
                    }
                    self.ctl.pointer_move(&mut self.doc, &self.view, s);
                    self.anno_pointer(s);
                }
                if over {
                    if scroll != 0.0 {
                        self.view.zoom_about(s, (scroll as f64 * 0.0025).exp());
                        self.ctl.pointer_move(&mut self.doc, &self.view, s);
                    }
                    if mid_double {
                        self.zoom_extents_visible();
                    }
                    if pressed {
                        if self.anno_press(s, shift) {
                            // An annotation command took the click (or a grip was picked up).
                            self.canvas_drag = self.anno.grip_drag.is_some();
                        } else {
                            if self.anno.grip_hover.is_some() {
                                self.ctl.pointer_leave();
                            }
                            self.ctl.left_down(&mut self.doc, &mut self.view, s, shift, false);
                            self.canvas_drag = true;
                        }
                        ctx.memory_mut(|m| m.request_focus(egui::Id::new(CMD_ID)));
                    }
                    if double && !was_busy && !anno_active {
                        // Double-click isolates the element so Properties shows it alone; a text
                        // or leader also opens for editing.
                        if let Some(id) = self.doc.pick(self.view.to_world(s), self.ctl.pick_box_px / self.view.scale) {
                            self.doc.select(&[id], SelectMode::Replace);
                            if self.doc.find(id).is_some_and(|e| e.category == afrobim_annotate::TEXT_CATEGORY || e.category == afrobim_annotate::LEADER_CATEGORY) {
                                self.anno_open_text_edit(id);
                            }
                        }
                    }
                    // During a command the right button is Enter.
                    if secondary && was_busy {
                        self.ctl.enter(&mut self.doc);
                    } else if secondary && anno_active {
                        self.anno_enter();
                    }
                }
                if released && self.anno.grip_drag.is_some() {
                    self.canvas_drag = false;
                    self.anno_release(s);
                } else if released && self.canvas_drag {
                    self.canvas_drag = false;
                    self.ctl.left_up(&mut self.doc, &mut self.view, s);
                }
            }
            if !over && !self.canvas_drag {
                self.ctl.pointer_leave();
            }
            self.drain_events(ctx);
            self.sync_generated();

            if over {
                ctx.set_cursor_icon(egui::CursorIcon::None); // the canvas draws its own crosshair
            }
            self.paint_scene(&painter, rect);
            self.paint_overlay(&painter, rect, over && !mid_down);
            self.paint_anno(&painter, over && !mid_down);

            if !was_busy {
                let mut pick = None;
                response.context_menu(|ui| {
                    for (label, a) in [
                        ("Move", Action::Move),
                        ("Copy", Action::Copy),
                        ("Erase", Action::Erase),
                        ("Paste", Action::PasteClip),
                        ("Select all", Action::SelectAll),
                        ("Zoom extents", Action::ZoomExtents),
                        ("Zoom window", Action::ZoomWindow),
                    ] {
                        if ui.button(label).clicked() {
                            pick = Some(a);
                            ui.close_menu();
                        }
                    }
                });
                if let Some(a) = pick {
                    self.run(ctx, a);
                }
            }
        });
    }

    fn windows(&mut self, ctx: &egui::Context) {
        self.snapshot_tick(ctx);
        if self.hide_windows_for_snapshot() {
            return;
        }
        self.reference_tick(ctx);
        self.reference_windows(ctx);
        self.anno_windows(ctx);
        self.underlay_window(ctx);
        self.signsched_windows(ctx);
        self.projset_window(ctx);
        self.issues_windows(ctx);
        self.visibility_windows(ctx);
        self.photometry_windows(ctx);
        self.pack_windows(ctx);
        self.coordination_windows(ctx);
        self.schedule_windows(ctx);
        self.cost_windows(ctx);
        self.construction_windows(ctx);
        self.check_windows(ctx);
        self.sheets_windows(ctx);
        self.design_windows(ctx);
        let mut help = self.show_help;
        crate::win::new(ctx, "Commands").open(&mut help).default_width(520.0).show(ctx, |ui| {
            egui::Grid::new("cmds").num_columns(3).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                for c in COMMANDS {
                    ui.label(RichText::new(c.name).monospace().strong());
                    ui.label(RichText::new(c.aliases.join(" ")).monospace().weak());
                    ui.label(c.help);
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Points: x,y absolute  ·  @dx,dy relative  ·  @distance<angle polar. MOVE, COPY and the signs commands take millimetres; the annotation, dimension and measure commands take the document units (UNITS).").weak());
        });
        self.show_help = help;

        let mut library_open = self.show_library;
        let mut insert: Option<(String, serde_json::Value)> = None;
        crate::win::new(ctx, "Standard signs").open(&mut library_open).default_width(420.0).default_height(520.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Find");
                ui.add(egui::TextEdit::singleline(&mut self.library_filter).hint_text("key or name").desired_width(f32::INFINITY));
            });
            ui.separator();
            let filter = self.library_filter.trim().to_lowercase();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let mut classes: Vec<&str> = self.library.iter().map(|i| i.class.as_str()).collect();
                classes.dedup();
                classes.sort();
                classes.dedup();
                for class in classes {
                    let items: Vec<&signs_glue::LibraryItem> = self
                        .library
                        .iter()
                        .filter(|i| i.class == class)
                        .filter(|i| filter.is_empty() || i.key.to_lowercase().contains(&filter) || i.name.to_lowercase().contains(&filter))
                        .collect();
                    if items.is_empty() {
                        continue;
                    }
                    egui::CollapsingHeader::new(format!("{class} ({})", items.len())).default_open(true).show(ui, |ui| {
                        for item in items {
                            if ui.selectable_label(false, format!("{}   {}", item.key, item.name)).on_hover_text("Click to insert, then click in the model to place").clicked() {
                                insert = Some((item.key.clone(), item.data.clone()));
                            }
                        }
                    });
                }
            });
        });
        self.show_library = library_open;
        if let Some((key, data)) = insert {
            self.show_library = false;
            self.insert_sign(ctx, &key, data);
        }

        if let Some((sign, mut height, mut posts, mut inset)) = self.supports_dialog.take() {
            let mark = self.doc.find(sign).map(|e| e.name.clone()).unwrap_or_default();
            let (mut keep, mut add) = (true, false);
            crate::win::new(ctx, format!("Supports for {mark}")).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::Grid::new("supports").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Mounting height (mm)");
                    ui.add(egui::TextEdit::singleline(&mut height).hint_text("ground to the sign's lower edge").desired_width(190.0));
                    ui.end_row();
                    ui.label("Number of posts");
                    ui.add(egui::DragValue::new(&mut posts).range(1..=6));
                    ui.end_row();
                    ui.label("Edge inset (mm)");
                    ui.add(egui::DragValue::new(&mut inset).speed(5.0).range(0.0..=5000.0));
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Posts and foundations are added with placeholder dimensions. Set every dimension in Properties, or size them for wind with Modify → Design the sign's supports (DESIGN).")
                        .small()
                        .weak(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let valid = height.trim().parse::<f64>().is_ok_and(|h| h > 0.0);
                    if ui.add_enabled(valid, egui::Button::new("Add supports")).clicked() {
                        add = true;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
            if add {
                let h: f64 = height.trim().parse().unwrap_or(0.0);
                self.sync_generated(); // the sign's size must be current
                match supports_glue::add(&mut self.doc, sign, h, posts, inset) {
                    Ok(n) => {
                        self.status = format!("Added {n} support element(s) to {mark} with placeholder dimensions.");
                        self.schedule_commit(ctx);
                    }
                    Err(e) => self.status = format!("Supports not added: {e}"),
                }
            } else if keep {
                self.supports_dialog = Some((sign, height, posts, inset));
            }
        }

        if let Some((sign, mut height, mut entered, mut req)) = self.design_dialog.take() {
            use supports_glue::WindChoice;
            let mark = self.doc.find(sign).map(|e| e.name.clone()).unwrap_or_default();
            let (mut keep, mut run) = (true, false);
            crate::win::new(ctx, format!("Design supports for {mark}")).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::Grid::new("design").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Mounting height (mm)");
                    ui.add(egui::TextEdit::singleline(&mut height).hint_text("ground to the sign's lower edge").desired_width(190.0));
                    ui.end_row();
                    ui.label("Number of posts");
                    ui.add(egui::DragValue::new(&mut req.posts).range(1..=6));
                    ui.end_row();
                    ui.label("Edge inset (mm)");
                    ui.add(egui::DragValue::new(&mut req.edge_inset_mm).speed(5.0).range(0.0..=5000.0));
                    ui.end_row();
                    ui.label("Basic wind pressure");
                    ui.vertical(|ui| {
                        ui.radio_value(&mut req.wind, WindChoice::MowtOpen, "MoWT Vol. 4 Table 2, open terrain");
                        ui.radio_value(&mut req.wind, WindChoice::MowtBuiltUp, "MoWT Vol. 4 Table 2, built-up area");
                        ui.horizontal(|ui| {
                            let is_entered = matches!(req.wind, WindChoice::Entered(_));
                            if ui.radio(is_entered, "Entered (kN/m²)").clicked() || is_entered {
                                req.wind = WindChoice::Entered(entered);
                            }
                            if ui.add(egui::DragValue::new(&mut entered).speed(0.01).range(0.05..=5.0)).changed() {
                                req.wind = WindChoice::Entered(entered);
                            }
                        });
                    });
                    ui.end_row();
                    ui.label("Foundation");
                    ui.vertical(|ui| {
                        ui.radio_value(&mut req.spread, false, "Planted, one per post");
                        ui.radio_value(&mut req.spread, true, "Spread, one under the sign");
                    });
                    ui.end_row();
                    if req.spread {
                        use afrobim_supports::design::AssumedGround;
                        ui.label("Assumed ground (Table 5.2)");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut req.spread_ground, AssumedGround::Poor, "Poor");
                            ui.radio_value(&mut req.spread_ground, AssumedGround::Average, "Average");
                            ui.radio_value(&mut req.spread_ground, AssumedGround::Good, "Good");
                        });
                        ui.end_row();
                        ui.label("Breadth along the sign (mm)");
                        ui.add(egui::DragValue::new(&mut req.spread_breadth_mm).speed(10.0).range(300.0..=10000.0));
                        ui.end_row();
                        ui.label("Thickness (mm)");
                        ui.add(egui::DragValue::new(&mut req.spread_thickness_mm).speed(10.0).range(200.0..=3000.0));
                        ui.end_row();
                        ui.label("Top below ground (mm)");
                        ui.add(egui::DragValue::new(&mut req.spread_cover_mm).speed(5.0).range(0.0..=1000.0));
                        ui.end_row();
                    } else {
                        ui.label("Method");
                        ui.vertical(|ui| {
                            ui.radio_value(&mut req.cd354, false, "PD 6547 (concrete surround resists)");
                            ui.radio_value(&mut req.cd354, true, "CD 354 rev 1 (post alone resists)");
                        });
                        ui.end_row();
                        ui.label("Foundation diameter (mm)");
                        ui.add(egui::DragValue::new(&mut req.foundation_diameter_mm).speed(10.0).range(100.0..=2000.0));
                        ui.end_row();
                        ui.label("Ground factor G (kN/m² per m)");
                        ui.add(egui::DragValue::new(&mut req.ground_factor).speed(5.0).range(1.0..=2000.0));
                        ui.end_row();
                        ui.label("Minimum planting depth (mm)");
                        ui.add(egui::DragValue::new(&mut req.min_planting_depth_mm).speed(10.0).range(100.0..=3000.0));
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Steel S355 CHS posts on level ground, to the IHE Sign Structures Guide 2021 (BS EN 12899-1). \
                         Planted: G = 230 is the guide's value for poor ground, and the minimum planting depth is from BS EN 40-2 Table 7, which AfroBIM does not hold. \
                         Spread: the length is sized for the breadth and thickness given; the ground is assumed and must be proved by testing; concrete and reinforcement are not designed. \
                         The result is a calculation for an engineer to check.",
                    )
                    .small()
                    .weak(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let valid = height.trim().parse::<f64>().is_ok_and(|h| h >= 0.0);
                    if ui.add_enabled(valid, egui::Button::new("Design")).clicked() {
                        run = true;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
            if run {
                req.mounting_height_mm = height.trim().parse().unwrap_or(0.0);
                self.sync_generated(); // the sign's size must be current
                match supports_glue::design(&mut self.doc, sign, &req) {
                    Ok(lines) => {
                        self.status = format!("Designed the supports of {mark}. Check the calculation.");
                        self.design_report = Some((mark, lines));
                        self.schedule_commit(ctx);
                    }
                    Err(e) => {
                        self.status = format!("Supports not designed: {e}");
                        self.design_dialog = Some((sign, height, entered, req));
                    }
                }
            } else if keep {
                self.design_dialog = Some((sign, height, entered, req));
            }
        }

        self.ext_marking_window(ctx);
        self.background_tick(ctx);
        self.ifc_options_window(ctx);

        if let Some((mut alignment, mut req)) = self.marking_dialog.take() {
            let choices = alignment_glue::alignments(&self.doc);
            let types = markings_glue::line_types();
            let (mut keep, mut lay) = (true, false);
            crate::win::new(ctx, "Road marking").collapsible(false).resizable(false).show(ctx, |ui| {
                let spec = &types[req.line.min(types.len() - 1)];
                egui::Grid::new("marking").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Alignment");
                    let current = choices.iter().find(|c| Some(c.0) == alignment).map(|c| c.1.clone()).unwrap_or_default();
                    egui::ComboBox::from_id_salt("marking-alignment").selected_text(current).show_ui(ui, |ui| {
                        for (id, name) in &choices {
                            ui.selectable_value(&mut alignment, Some(*id), name);
                        }
                    });
                    ui.end_row();
                    ui.label("Line type");
                    egui::ComboBox::from_id_salt("marking-type").width(330.0).selected_text(format!("{} (diagram {})", spec.name, spec.diagram)).show_ui(ui, |ui| {
                        for (i, t) in types.iter().enumerate() {
                            if ui.selectable_value(&mut req.line, i, format!("{} (diagram {}, {})", t.name, t.diagram, t.speed.label())).clicked() {
                                req.width_mm = None;
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("Basis");
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut req.basis_uganda, false, afrobim_markings::Basis::Uk.label());
                            ui.selectable_value(&mut req.basis_uganda, true, afrobim_markings::Basis::Uganda.label());
                        });
                        if req.basis_uganda {
                            ui.label(RichText::new(format!("{}: not defined in the Uganda manual (the longitudinal catalogue here is the UK diagrams; the Uganda lines are under Uganda lines).", spec.name)).small().color(Color32::from_rgb(0xD9, 0x8A, 0x2B)));
                            let mut on = req.uk_fallback.is_some();
                            if ui.checkbox(&mut on, "use the UK value").changed() {
                                req.uk_fallback = on.then(String::new);
                            }
                            if let Some(reason) = &mut req.uk_fallback {
                                ui.add(egui::TextEdit::multiline(reason).desired_rows(2).desired_width(330.0).hint_text("required: the reason is stored with the marking"));
                            }
                        } else {
                            req.uk_fallback = None;
                        }
                    });
                    ui.end_row();
                    ui.label("Pattern");
                    ui.label(format!("{}; {}", spec.pattern.label(), spec.citation));
                    ui.end_row();
                    ui.label("Speed limit");
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let mut kmh = req.speed_kmh.is_some();
                            ui.selectable_value(&mut kmh, false, "mph");
                            ui.selectable_value(&mut kmh, true, "km/h");
                            if kmh != req.speed_kmh.is_some() {
                                req.speed_kmh = kmh.then(|| (afrobim_markings::kmh(req.speed_mph).round(), None));
                            }
                            match &mut req.speed_kmh {
                                Some((v, _)) => {
                                    ui.add(egui::DragValue::new(v).range(5.0..=130.0).suffix(" km/h"));
                                }
                                None => {
                                    ui.add(egui::DragValue::new(&mut req.speed_mph).range(5..=70));
                                    ui.label(RichText::new(afrobim_markings::speed_label(req.speed_mph)).small().weak());
                                }
                            }
                        });
                        // A km/h limit meets the manual's mph tables through the project's rule.
                        if let Some((v, answer)) = &mut req.speed_kmh {
                            let r = crate::projset::resolve_answered(&self.doc, *v, false, &crate::projset::marking_thresholds(), *answer);
                            ui.label(RichText::new(&r.band_note).small());
                            if let Some(ask) = r.ask {
                                ui.label(RichText::new(format!("The project rule asks each time: treat {:.2} mph as {} mph?", ask.exact_mph, ask.band_mph)).small().color(Color32::from_rgb(0xD9, 0x8A, 0x2B)));
                                ui.horizontal(|ui| {
                                    ui.selectable_value(answer, Some(true), "Yes, take the band");
                                    ui.selectable_value(answer, Some(false), "No, keep the exact value");
                                });
                            } else {
                                ui.label(RichText::new(format!("The tables are read at {} mph.", r.mph_used.round())).small().weak());
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("Width (mm)");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut req.width_mm, None, "From the table");
                        for w in spec.widths_mm {
                            ui.selectable_value(&mut req.width_mm, Some(*w), w.to_string());
                        }
                    });
                    ui.end_row();
                    crate::station_input::station_row(ui, &self.doc, alignment, "marking-start", "Start station", &mut req.start_station_m);
                    crate::station_input::station_row(ui, &self.doc, alignment, "marking-end", "End station", &mut req.end_station_m);
                    ui.label("Offset from the alignment (m)");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut req.distance_m).speed(0.05).range(0.0..=50.0));
                        ui.selectable_value(&mut req.right, false, "Left");
                        ui.selectable_value(&mut req.right, true, "Right");
                    });
                    ui.end_row();
                    ui.label("Material");
                    ui.add(egui::TextEdit::singleline(&mut req.material).hint_text("not stated").desired_width(190.0));
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Patterns and widths are from the Traffic Signs Manual Chapter 5 (2018). Its tables are in mph: how a km/h limit maps to them is the engineer's decision. \
                         The line is shown in the 3D site view and measured under Export quantities.",
                    )
                    .small()
                    .weak(),
                );
                ui.label(RichText::new(crate::station_input::note(&self.doc, alignment)).small().weak());
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(alignment.is_some(), egui::Button::new("Lay the line")).clicked() {
                        lay = true;
                    }
                    if ui.button("Close").clicked() {
                        keep = false;
                    }
                });
            });
            if lay {
                if let Some(a) = alignment {
                    match markings_glue::add(&mut self.doc, a, &req) {
                        Ok(mark) => {
                            self.sync_generated();
                            let notes = self.doc.elements().iter().find(|e| e.name == mark).and_then(|e| self.cache.get(&e.id)).and_then(|c| c.generated.as_ref().ok()).map(|g| g.notes.clone()).unwrap_or_default();
                            self.status = format!("Laid {mark}.");
                            for n in notes.iter().take(4) {
                                self.log(format!("{mark}: {}", n.message));
                            }
                            if !notes.is_empty() {
                                self.status.push_str(&format!(" {} note(s) in the command history.", notes.len()));
                            }
                            self.fitted_3d = false;
                            self.schedule_commit(ctx);
                        }
                        Err(e) => self.status = format!("Not laid: {e}"),
                    }
                }
                // Stay open: several lines are usually laid in a row.
                self.marking_dialog = Some((alignment, req));
            } else if keep {
                self.marking_dialog = Some((alignment, req));
            }
        }

        if let Some((kind, mut alignment, mut start, mut end, mut distance, mut right)) = self.road_dialog.take() {
            let info = &modules_glue::KINDS[kind.min(modules_glue::KINDS.len() - 1)];
            let choices = alignment_glue::alignments(&self.doc);
            let (mut keep, mut add) = (true, false);
            crate::win::new(ctx, format!("Add a {}", info.label.to_lowercase())).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::Grid::new("road-element").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Alignment");
                    let current = choices.iter().find(|c| Some(c.0) == alignment).map(|c| c.1.clone()).unwrap_or_default();
                    egui::ComboBox::from_id_salt("road-alignment").selected_text(current).show_ui(ui, |ui| {
                        for (id, name) in &choices {
                            ui.selectable_value(&mut alignment, Some(*id), name);
                        }
                    });
                    ui.end_row();
                    crate::station_input::station_row(ui, &self.doc, alignment, "road-start", if info.span { "Start station" } else { "Station" }, &mut start);
                    if info.span {
                        crate::station_input::station_row(ui, &self.doc, alignment, "road-end", "End station", &mut end);
                    }
                    if info.offset {
                        ui.label("Offset from the alignment (m)");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut distance).speed(0.1).range(0.0..=200.0));
                            ui.selectable_value(&mut right, false, "Left");
                            ui.selectable_value(&mut right, true, "Right");
                        });
                        ui.end_row();
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new("It is added with placeholder dimensions that are not design values: select it and set every dimension in Properties, then run CHECK.")
                        .small()
                        .weak(),
                );
                ui.label(RichText::new(crate::station_input::note(&self.doc, alignment)).small().weak());
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(alignment.is_some(), egui::Button::new("Add")).clicked() {
                        add = true;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
            if add {
                let offset = if right { distance } else { -distance };
                match alignment.ok_or_else(|| "Choose an alignment.".to_string()).and_then(|a| modules_glue::add(&mut self.doc, kind, a, start, end, offset)) {
                    Ok(mark) => {
                        self.sync_generated();
                        if let Some(id) = self.doc.elements().iter().find(|e| e.name == mark).map(|e| e.id) {
                            self.doc.select(&[id], SelectMode::Replace);
                        }
                        self.status = format!("Added {mark} with placeholder dimensions. Set them in Properties.");
                        self.fitted_3d = false;
                        self.schedule_commit(ctx);
                    }
                    Err(e) => {
                        self.status = format!("Not added: {e}");
                        self.road_dialog = Some((kind, alignment, start, end, distance, right));
                    }
                }
            } else if keep {
                self.road_dialog = Some((kind, alignment, start, end, distance, right));
            }
        }

        for outcome in self.road.show(ctx, &mut self.doc) {
            match outcome {
                road_dialogs::Outcome::Added(mark, status) => {
                    self.sync_generated();
                    if let Some(id) = self.doc.elements().iter().find(|e| e.name == mark).map(|e| e.id) {
                        self.doc.select(&[id], SelectMode::Replace);
                    }
                    self.status = status;
                    self.fitted_3d = false;
                    self.schedule_commit(ctx);
                }
                road_dialogs::Outcome::Report(subject, lines) => {
                    self.status = format!("Calculated {subject}. Read the calculation.");
                    self.design_report = Some((subject, lines));
                }
                road_dialogs::Outcome::Status(line) => self.status = line,
            }
        }

        if let Some((sign, mut gantry, mut by_lane, mut lane, mut offset, mut bottom, mut up)) = self.hang_dialog.take() {
            let mark = self.doc.find(sign).map(|e| e.name.clone()).unwrap_or_default();
            let choices = modules_glue::gantries(&self.doc);
            let (mut keep, mut hang) = (true, false);
            crate::win::new(ctx, format!("Hang {mark} on a gantry")).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::Grid::new("hang").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Gantry");
                    let current = choices.iter().find(|c| Some(c.0) == gantry).map(|c| c.1.clone()).unwrap_or_default();
                    egui::ComboBox::from_id_salt("hang-gantry").selected_text(current).show_ui(ui, |ui| {
                        for (id, name) in &choices {
                            ui.selectable_value(&mut gantry, Some(*id), name);
                        }
                    });
                    ui.end_row();
                    ui.label("Position");
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut by_lane, false, "Offset from the centreline (m)");
                            ui.add_enabled(!by_lane, egui::DragValue::new(&mut offset).speed(0.1));
                        });
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut by_lane, true, "Over lane number");
                            ui.add_enabled(by_lane, egui::DragValue::new(&mut lane).range(1..=12));
                        });
                    });
                    ui.end_row();
                    if !by_lane {
                        ui.label("Faces traffic travelling");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut up, true, "Up-station");
                            ui.selectable_value(&mut up, false, "Down-station");
                        });
                        ui.end_row();
                    }
                    ui.label("Lower edge above the boom underside (mm)");
                    ui.add(egui::DragValue::new(&mut bottom).speed(10.0));
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.label(RichText::new("Lanes are those entered on the gantry, numbered from 1 at the left. A negative level hangs the sign below the boom. Run CHECK on the gantry afterwards for headroom.").small().weak());
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(gantry.is_some(), egui::Button::new("Hang")).clicked() {
                        hang = true;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
            if hang {
                match gantry.ok_or_else(|| "Choose a gantry.".to_string()).and_then(|g| modules_glue::hang_sign(&mut self.doc, g, sign, by_lane.then_some(lane), offset, bottom, up)) {
                    Ok(note) => {
                        self.status = note;
                        self.fitted_3d = false;
                        self.schedule_commit(ctx);
                    }
                    Err(e) => {
                        self.status = format!("Not hung: {e}");
                        self.hang_dialog = Some((sign, gantry, by_lane, lane, offset, bottom, up));
                    }
                }
            } else if keep {
                self.hang_dialog = Some((sign, gantry, by_lane, lane, offset, bottom, up));
            }
        }

        if let Some(command) = self.options.preview.clone() {
            crate::win::new(ctx, "Command preview").collapsible(false).resizable(false).title_bar(false).default_pos([420.0, 190.0]).show(ctx, |ui| {
                if !crate::ribbon::preview_of(ui, &command) {
                    ui.label(format!("No ribbon button runs {command}."));
                }
            });
        }

        if let Some((mark, lines)) = self.design_report.take() {
            let mut open = true;
            crate::win::new(ctx, format!("Calculation for {mark}")).open(&mut open).default_width(640.0).show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                    for line in &lines {
                        let text = RichText::new(line).small();
                        let flagged = line.contains("FAILS") || line.contains("NOT ENOUGH") || ["Check:", "Note:", "WARNING"].iter().any(|p| line.starts_with(p));
                        ui.label(if flagged { text.color(egui::Color32::from_rgb(0xD9, 0x8A, 0x2B)) } else { text });
                    }
                });
                if ui.button("Copy").clicked() {
                    ctx.copy_text(lines.join("\n"));
                }
            });
            if open {
                self.design_report = Some((mark, lines));
            }
        }

        if let Some((sign, mut alignment, mut station, mut offset, mut right, mut region)) = self.station_dialog.take() {
            let mark = self.doc.find(sign).map(|e| e.name.clone()).unwrap_or_default();
            let choices = alignment_glue::alignments(&self.doc);
            let (mut keep, mut place) = (true, false);
            crate::win::new(ctx, format!("Place {mark} on an alignment")).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::Grid::new("station").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Alignment");
                    let current = choices.iter().find(|c| Some(c.0) == alignment).map(|c| c.1.clone()).unwrap_or_default();
                    egui::ComboBox::from_id_salt("alignment").selected_text(current).show_ui(ui, |ui| {
                        for (id, name) in &choices {
                            ui.selectable_value(&mut alignment, Some(*id), name);
                        }
                    });
                    ui.end_row();
                    ui.label("Station (m)");
                    ui.add(egui::DragValue::new(&mut station).speed(1.0));
                    ui.end_row();
                    let regions = alignment.map(|a| alignment_glue::region_count(&self.doc, a)).unwrap_or(1);
                    if regions > 1 {
                        ui.label("Stationing region");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut region).range(0..=regions));
                            ui.label(RichText::new(format!("1 to {regions}; 0 if the station occurs once")).small().weak());
                        });
                        ui.end_row();
                    } else {
                        region = 0;
                    }
                    ui.label("Offset from centreline (m)");
                    ui.add(egui::DragValue::new(&mut offset).speed(0.1).range(0.0..=200.0));
                    ui.end_row();
                    ui.label("Side, looking up-station");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut right, false, "Left");
                        ui.selectable_value(&mut right, true, "Right");
                    });
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.label(RichText::new("The sign faces the traffic on its own side of the road, for traffic keeping left.").small().weak());
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(alignment.is_some(), egui::Button::new("Place")).clicked() {
                        place = true;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
            if place {
                if let Some(a) = alignment {
                    match alignment_glue::place_sign(&mut self.doc, sign, a, station, region.checked_sub(1), offset, right) {
                        Ok(note) => {
                            self.status = note;
                            self.fitted_3d = false;
                            self.schedule_commit(ctx);
                        }
                        Err(e) => {
                            self.status = format!("Not placed: {e}");
                            self.station_dialog = Some((sign, alignment, station, offset, right, region));
                        }
                    }
                }
            } else if keep {
                self.station_dialog = Some((sign, alignment, station, offset, right, region));
            }
        }

        let mut about = self.show_about;
        crate::win::new(ctx, "About AfroBIM").open(&mut about).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.heading("AfroBIM");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.add_space(6.0);
            ui.label("Building information modelling for roads and infrastructure.");
            ui.label("Traffic signs module: UK Traffic Signs Manual Chapter 7.");
            ui.add_space(6.0);
            ui.label(RichText::new(&self.module_status).weak());
        });
        self.show_about = about;
    }

    /// The categories the registered modules own, with their display names.
    pub(crate) fn module_categories(&self) -> Vec<(String, String)> {
        self.modules.categories().map(|(c, n)| (c.to_string(), n.to_string())).collect()
    }

    /// The label of the sign design standard in use, for the report pack.
    pub(crate) fn standard_label(&self) -> String {
        signs_glue::STANDARDS.iter().find(|(id, _, _)| *id == self.standard).map(|(id, label, _)| format!("{id} ({label})")).unwrap_or_else(|| self.standard.clone())
    }

    /// The title of the calculation pack: the document's file name, else "Untitled".
    pub(crate) fn title_for_pack(&self) -> String {
        self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled".into())
    }

    fn screenshot_mode(&mut self, ctx: &egui::Context) {
        let Some(path) = self.options.screenshot.clone() else { return };
        // Wait for a viewpoint snapshot or a running import before the picture is taken.
        if self.issues.snap.is_some() || self.refs.busy() || self.under.busy() || self.bg.busy() {
            ctx.request_repaint();
            return;
        }
        self.frames += 1;
        if self.frames == 8 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, user_data, .. } if user_data.data.is_none() => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            if let Err(e) = image::save_buffer(&path, img.as_raw(), img.width() as u32, img.height() as u32, image::ColorType::Rgba8) {
                eprintln!("AfroBIM: could not write {}: {e}", path.display());
            }
            self.quit = true;
        }
        ctx.request_repaint();
    }
}

impl eframe::App for AfroBimApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
        self.shortcuts(ctx);
        // `--run COMMAND`: runs a command once at start-up (on the element named by `--focus`, when
        // given), to look at a dialog or window without a pointer.
        if let Some(command) = self.options.run.take() {
            if let Some(mark) = self.options.focus.clone() {
                if let Some(id) = self.doc.elements().iter().find(|e| e.name == mark).map(|e| e.id) {
                    self.doc.select(&[id], SelectMode::Replace);
                }
            }
            // Several commands may be given, separated by commas.
            for part in command.split(',') {
                match commands::resolve(part) {
                    Some(info) => self.run(ctx, info.action),
                    None => self.status = format!("--run: {part} is not a command."),
                }
            }
        }

        self.sched_tick(ctx);
        self.under.sync(&mut self.doc, self.view.scale);
        if std::mem::take(&mut self.under.doc_changed) {
            self.schedule_commit_now();
        }
        if self.under.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
        self.menu_bar(ctx);
        self.toolbar(ctx);
        self.status_bar(ctx);
        self.left_panel(ctx);
        self.right_panel(ctx);
        self.command_line(ctx);
        self.canvas(ctx);
        self.windows(ctx);

        // Commit once input has settled, so a drag or a burst of typing is one undo step.
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        if let Some(due) = self.commit_due {
            if !pointer_down && ctx.input(|i| i.time) >= due {
                self.flush_commit();
            }
        }
        self.screenshot_mode(ctx);
        if self.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
