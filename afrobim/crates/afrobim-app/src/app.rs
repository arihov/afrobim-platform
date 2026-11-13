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
use crate::{signs_glue, Options};

/// A module's output for one element, kept until the element's definition changes.
pub(crate) struct Cached {
    key: (String, serde_json::Value),
    pub generated: Result<Generated, String>,
    pub mesh: ElementMesh,
}

pub struct AfroBimApp {
    pub(crate) doc: Document,
    pub(crate) view: ViewTransform,
    pub(crate) ctl: CanvasController,
    modules: ModuleRegistry,
    signs_ready: bool,
    module_status: String,
    pub(crate) cache: HashMap<ElementId, Cached>,

    history: UndoHistory<Vec<Element>>,
    commit_due: Option<f64>,
    clipboard: Vec<Element>,
    paste_count: u32,

    cmd_input: String,
    cmd_log: Vec<String>,
    last_action: Option<Action>,
    show_help: bool,
    show_about: bool,
    show_library: bool,
    library: Vec<signs_glue::LibraryItem>,
    library_filter: String,
    new_prop_key: String,

    pub(crate) grid_on: bool,
    pub(crate) grid_spacing: f64,
    pub(crate) dark_canvas: bool,
    dark_theme: bool,
    pub(crate) rep: Representation,
    pub(crate) show_labels: bool,

    path: Option<PathBuf>,
    status: String,
    fitted: bool,
    canvas_drag: bool,
    quit: bool,

    options: Options,
    frames: u32,
}

const CMD_ID: &str = "afrobim-command-line";

impl AfroBimApp {
    pub fn new(cc: &eframe::CreationContext<'_>, options: Options) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut modules = ModuleRegistry::default();
        let (signs_ready, module_status, library) = match signs_glue::load_module() {
            Ok((module, label, library)) => {
                modules.register(module);
                (true, label, library)
            }
            Err(e) => (false, format!("Traffic signs unavailable: {e}"), Vec::new()),
        };

        let mut app = Self {
            doc: Document::default(),
            view: ViewTransform::default(),
            ctl: CanvasController::default(),
            modules,
            signs_ready,
            module_status,
            cache: HashMap::new(),
            history: UndoHistory::new(Vec::new(), 100),
            commit_due: None,
            clipboard: Vec::new(),
            paste_count: 0,
            cmd_input: String::new(),
            cmd_log: vec!["AfroBIM model space. Type a command or pick a tool. ? lists commands.".into()],
            last_action: None,
            show_help: false,
            show_about: false,
            show_library: false,
            library,
            library_filter: String::new(),
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
            canvas_drag: false,
            quit: false,
            options,
            frames: 0,
        };

        if let Some(path) = app.options.open.clone() {
            app.open_path(path);
        } else if app.options.demo && app.signs_ready {
            for (mark, data) in signs_glue::demo_data() {
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
            if let Some(first) = app.doc.elements().first().map(|e| e.id) {
                app.doc.select(&[first], SelectMode::Replace);
            }
        }
        app.sync_generated();
        app.history.reset(app.doc.snapshot());
        app
    }

    // ─── module output ───────────────────────────────────────────────────────

    /// Regenerates geometry for elements whose definition changed and drops erased ones.
    fn sync_generated(&mut self) {
        let ids: Vec<ElementId> = self.doc.elements().iter().map(|e| e.id).collect();
        self.cache.retain(|id, _| ids.contains(id));
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
        }
    }

    // ─── undo ────────────────────────────────────────────────────────────────

    fn schedule_commit(&mut self, ctx: &egui::Context) {
        self.commit_due = Some(ctx.input(|i| i.time) + 0.3);
        ctx.request_repaint_after(std::time::Duration::from_millis(350));
    }

    fn flush_commit(&mut self) {
        self.commit_due = None;
        self.sync_generated();
        self.history.commit(self.doc.snapshot());
    }

    fn undo(&mut self) {
        self.flush_commit();
        match self.history.undo().cloned() {
            Some(state) => {
                self.doc.restore(state);
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
                self.log("Redo");
            }
            None => self.status = "Nothing to redo.".into(),
        }
    }

    fn log(&mut self, line: impl Into<String>) {
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

    fn open_path(&mut self, path: PathBuf) {
        match Document::load(&path) {
            Ok(doc) => {
                self.doc = doc;
                self.cache.clear();
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
            match self.cache.get(&e.id).and_then(|c| c.generated.as_ref().ok()) {
                Some(g) => {
                    dxf.add(&g.drawing, 1.0, e.origin);
                    written += 1;
                }
                None => skipped += 1,
            }
        }
        match std::fs::write(&path, dxf.finish()) {
            Ok(()) => {
                self.status = format!("Exported {written} element(s) to {}.", path.display());
                if skipped > 0 {
                    self.status.push_str(&format!(" {skipped} without geometry were left out."));
                }
            }
            Err(e) => self.status = format!("Could not write {}: {e}", path.display()),
        }
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
            Action::ZoomExtents => self.ctl.zoom_extents(&self.doc, &mut self.view),
            Action::ZoomWindow => self.ctl.start_zoom_window(),
            Action::ZoomIn => self.ctl.zoom_by(&mut self.view, 1.5),
            Action::ZoomOut => self.ctl.zoom_by(&mut self.view, 1.0 / 1.5),
            Action::ToggleGrid => self.grid_on = !self.grid_on,
            Action::ToggleSnap => self.ctl.snap_on = !self.ctl.snap_on,
            Action::ToggleOrtho => self.ctl.ortho_on = !self.ctl.ortho_on,
            Action::RepReal => self.rep = Representation::Real,
            Action::RepOutlined => self.rep = Representation::Outlined,
            Action::ToggleCanvas => self.dark_canvas = !self.dark_canvas,
            Action::ToggleTheme => {
                self.dark_theme = !self.dark_theme;
                ctx.set_visuals(if self.dark_theme { egui::Visuals::dark() } else { egui::Visuals::light() });
            }
            Action::Arrange => {
                self.doc.arrange();
                self.ctl.zoom_extents(&self.doc, &mut self.view);
                self.schedule_commit(ctx);
            }
            Action::InsertSign(index) => {
                if !self.signs_ready {
                    self.status = self.module_status.clone();
                } else if let Some(t) = signs_glue::templates().into_iter().nth(index as usize) {
                    self.insert_sign(ctx, t.mark, t.data);
                }
            }
            Action::Library => self.show_library = true,
            Action::Help => self.show_help = true,
            Action::About => self.show_about = true,
            Action::Quit => self.quit = true,
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
                self.ctl.escape(&mut self.doc);
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
                    item(ui, "Export DXF…", "DXFOUT", Action::ExportDxf);
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
                    item(ui, "Real (full colour)", "", Action::RepReal);
                    item(ui, "Outlined", "", Action::RepOutlined);
                    ui.separator();
                    item(ui, "Dark / light model space", "", Action::ToggleCanvas);
                    item(ui, "Dark / light theme", "", Action::ToggleTheme);
                });
                ui.menu_button("Insert", |ui| {
                    for (i, t) in signs_glue::templates().iter().enumerate() {
                        item(ui, t.label, t.command, Action::InsertSign(i as u8));
                    }
                    ui.separator();
                    item(ui, "Standard signs…", "LIBRARY", Action::Library);
                });
                ui.menu_button("Modify", |ui| {
                    item(ui, "Move", "M", Action::Move);
                    item(ui, "Copy", "CO", Action::Copy);
                    item(ui, "Erase", "E", Action::Erase);
                    ui.separator();
                    item(ui, "Arrange", "", Action::Arrange);
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

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let groups: [&[(Icon, &str, Action)]; 5] = [
                    &[(Icon::New, "New (Ctrl+N)", Action::New), (Icon::Open, "Open (Ctrl+O)", Action::Open), (Icon::Save, "Save (Ctrl+S)", Action::Save)],
                    &[(Icon::Undo, "Undo (Ctrl+Z)", Action::Undo), (Icon::Redo, "Redo (Ctrl+Y)", Action::Redo)],
                    &[(Icon::Move, "MOVE (M)", Action::Move), (Icon::Copy, "COPY (CO)", Action::Copy), (Icon::Erase, "ERASE (E or Delete)", Action::Erase)],
                    &[
                        (Icon::ZoomExtents, "Zoom extents (Z)", Action::ZoomExtents),
                        (Icon::ZoomWindow, "Zoom window (ZW)", Action::ZoomWindow),
                        (Icon::ZoomIn, "Zoom in", Action::ZoomIn),
                        (Icon::ZoomOut, "Zoom out", Action::ZoomOut),
                    ],
                    &[(Icon::Arrange, "Arrange in rows", Action::Arrange), (Icon::Theme, "Dark / light theme", Action::ToggleTheme)],
                ];
                let mut pick = None;
                for (n, group) in groups.iter().enumerate() {
                    if n > 0 {
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);
                    }
                    for (icon, tip, action) in group.iter() {
                        if icons::button(ui, *icon, tip).clicked() {
                            pick = Some(*action);
                        }
                    }
                }
                if let Some(a) = pick {
                    self.run(ctx, a);
                }
            });
        });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let w = self.ctl.cursor_world();
                ui.add_sized([150.0, 16.0], egui::Label::new(RichText::new(format!("{:.0} , {:.0} mm", w.x, w.y)).monospace()));
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
                        (1, Some(e)) => format!("{}  {:.0} × {:.0} mm", e.name, e.width, e.height),
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
                for (i, t) in signs_glue::templates().iter().enumerate() {
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
                let layers: Vec<String> = self.doc.layers().iter().map(|l| l.name.clone()).collect();
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
                    changed |= ui.add(egui::DragValue::new(&mut e.origin.x).speed(10.0).suffix(" mm")).changed();
                    ui.end_row();
                    ui.label("Position Y");
                    changed |= ui.add(egui::DragValue::new(&mut e.origin.y).speed(10.0).suffix(" mm")).changed();
                    ui.end_row();
                    ui.label("Size");
                    ui.label(format!("{:.0} × {:.0} mm", e.width, e.height));
                    ui.end_row();
                });

                ui.add_space(8.0);
                ui.label(RichText::new("Definition").strong());
                if e.category == signs_glue::CATEGORY {
                    changed |= signs_glue::edit_ui(ui, &mut e.data);
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
                    for (k, v) in e.properties.iter_mut() {
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
            if changed {
                self.schedule_commit(ctx);
            }

            ui.add_space(6.0);
            ui.label(RichText::new("LAYERS").small().strong().weak());
            ui.separator();
            egui::ScrollArea::vertical().id_salt("layers").show(ui, |ui| {
                for layer in self.doc.layers_mut().iter_mut() {
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
                let prompt = if self.ctl.prompt().is_empty() { "Command:" } else { self.ctl.prompt() };
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
            let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let rect = response.rect;
            self.ctl.viewport = (rect.left() as f64, rect.top() as f64, rect.width() as f64, rect.height() as f64);
            if !self.fitted && rect.width() > 50.0 {
                self.fitted = true;
                self.ctl.zoom_extents(&self.doc, &mut self.view);
            }

            let was_busy = self.ctl.is_busy();
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
                }
                if over {
                    if scroll != 0.0 {
                        self.view.zoom_about(s, (scroll as f64 * 0.0025).exp());
                        self.ctl.pointer_move(&mut self.doc, &self.view, s);
                    }
                    if mid_double {
                        self.ctl.zoom_extents(&self.doc, &mut self.view);
                    }
                    if pressed {
                        self.ctl.left_down(&mut self.doc, &mut self.view, s, shift, false);
                        self.canvas_drag = true;
                        ctx.memory_mut(|m| m.request_focus(egui::Id::new(CMD_ID)));
                    }
                    if double && !was_busy {
                        // Double-click isolates the element so Properties shows it alone.
                        if let Some(id) = self.doc.pick(self.view.to_world(s), self.ctl.pick_box_px / self.view.scale) {
                            self.doc.select(&[id], SelectMode::Replace);
                        }
                    }
                    // During a command the right button is Enter.
                    if secondary && was_busy {
                        self.ctl.enter(&mut self.doc);
                    }
                }
                if released && self.canvas_drag {
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
        let mut help = self.show_help;
        egui::Window::new("Commands").open(&mut help).default_width(520.0).show(ctx, |ui| {
            egui::Grid::new("cmds").num_columns(3).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                for c in COMMANDS {
                    ui.label(RichText::new(c.name).monospace().strong());
                    ui.label(RichText::new(c.aliases.join(" ")).monospace().weak());
                    ui.label(c.help);
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Points: x,y absolute  ·  @dx,dy relative  ·  @distance<angle polar. Millimetres.").weak());
        });
        self.show_help = help;

        let mut library_open = self.show_library;
        let mut insert: Option<(String, serde_json::Value)> = None;
        egui::Window::new("Standard signs").open(&mut library_open).default_width(420.0).default_height(520.0).show(ctx, |ui| {
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

        let mut about = self.show_about;
        egui::Window::new("About AfroBIM").open(&mut about).collapsible(false).resizable(false).show(ctx, |ui| {
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

    fn screenshot_mode(&mut self, ctx: &egui::Context) {
        let Some(path) = self.options.screenshot.clone() else { return };
        self.frames += 1;
        if self.frames == 8 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
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
