//! The toolspace: a Prospector-style tree of the civil objects (and the other element categories
//! below them), with a Settings tab. Collapsible nodes with icons and counts, a right-click menu
//! on every object, double-click to zoom, rename, properties, delete, show and hide.

use afrobim_kernel::{ElementId, SelectMode};
use egui::{RichText, Ui};
use egui_phosphor::regular as ph;

use crate::app::AfroBimApp;
use crate::civil::{self, *};
use crate::civil_cmds::Cv;
use crate::commands::Action;

/// What a row of the tree asks for once the frame is drawn.
#[derive(Debug, Clone)]
enum Act {
    Select(ElementId),
    Zoom(ElementId),
    Rename(ElementId),
    Properties(ElementId),
    Delete(ElementId),
    ToggleHidden(ElementId),
    Run(Cv, Option<ElementId>),
}

/// One object row: what the tree needs of an element.
struct Row {
    id: ElementId,
    name: String,
    status: String,
    hidden: bool,
    detail: String,
    category: String,
}

#[derive(Default)]
pub struct TreeState {
    /// 0 Prospector, 1 Settings.
    pub tab: usize,
    pub renaming: Option<(ElementId, String)>,
    pub filter: String,
}

fn icon_of(category: &str) -> &'static str {
    match category {
        POINTS => ph::DOTS_NINE,
        SURFACE | afrobim_corridor::SURFACE_CATEGORY => ph::MOUNTAINS,
        PROFILE => ph::CHART_LINE_UP,
        PROFILE_VIEW => ph::CHART_LINE,
        ASSEMBLY => ph::STACK_SIMPLE,
        CORRIDOR => ph::ROAD_HORIZON,
        SAMPLES => ph::LINE_SEGMENTS,
        PARCEL => ph::POLYGON,
        c if c == afrobim_alignment::CATEGORY => ph::PATH,
        _ => ph::SQUARE,
    }
}

impl AfroBimApp {
    fn rows(&self, category: &str) -> Vec<Row> {
        self.doc
            .elements()
            .iter()
            .filter(|e| e.category == category)
            .map(|e| {
                let detail = self.civil.built.get(&e.id).and_then(|(_, b)| b.scene.derived.first().map(|d| format!("{} {}", d.1, d.0.to_lowercase()))).unwrap_or_default();
                Row { id: e.id, name: e.name.clone(), status: status_of(e), hidden: hidden(e), detail, category: e.category.clone() }
            })
            .collect()
    }

    fn row_ui(&self, ui: &mut Ui, r: &Row, acts: &mut Vec<Act>, menu: &[(&str, Cv)]) {
        ui.horizontal(|ui| {
            let eye = if r.hidden { ph::EYE_SLASH } else { ph::EYE };
            if ui.add(egui::Button::new(RichText::new(eye).small()).frame(false)).on_hover_text(if r.hidden { "Show" } else { "Hide" }).clicked() {
                acts.push(Act::ToggleHidden(r.id));
            }
            let renaming = self.civil_ui.tree.renaming.as_ref().is_some_and(|(id, _)| *id == r.id);
            if renaming {
                return;
            }
            let selected = self.doc.is_selected(r.id);
            let text = RichText::new(format!("{}  {}", icon_of(&r.category), r.name));
            let text = if r.hidden { text.weak() } else { text };
            let resp = ui.selectable_label(selected, text);
            if resp.clicked() {
                acts.push(Act::Select(r.id));
            }
            if resp.double_clicked() {
                acts.push(Act::Zoom(r.id));
            }
            let tip = format!("{}{}{}", r.name, if r.status.is_empty() { String::new() } else { format!("\nStatus: {}", r.status) }, if r.detail.is_empty() { String::new() } else { format!("\n{}", r.detail) });
            let resp = resp.on_hover_text(tip);
            resp.context_menu(|ui| {
                if ui.button(format!("{}  Zoom to", ph::CROSSHAIR)).clicked() {
                    acts.push(Act::Zoom(r.id));
                    ui.close_menu();
                }
                if ui.button(format!("{}  Rename", ph::PENCIL_SIMPLE)).clicked() {
                    acts.push(Act::Rename(r.id));
                    ui.close_menu();
                }
                if ui.button(format!("{}  Properties", ph::LIST_BULLETS)).clicked() {
                    acts.push(Act::Properties(r.id));
                    ui.close_menu();
                }
                if ui.button(if r.hidden { format!("{}  Show", ph::EYE) } else { format!("{}  Hide", ph::EYE_SLASH) }).clicked() {
                    acts.push(Act::ToggleHidden(r.id));
                    ui.close_menu();
                }
                if !menu.is_empty() {
                    ui.separator();
                }
                for (label, cv) in menu {
                    if ui.button(*label).clicked() {
                        acts.push(Act::Run(*cv, Some(r.id)));
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button(RichText::new(format!("{}  Delete", ph::TRASH_SIMPLE)).color(crate::theme::danger())).clicked() {
                    acts.push(Act::Delete(r.id));
                    ui.close_menu();
                }
            });
            if !r.detail.is_empty() {
                ui.label(RichText::new(&r.detail).small().weak());
            }
        });
    }

    /// A group node: header with an icon and a count, a "new" button, rows inside.
    fn node(&self, ui: &mut Ui, id: &str, icon: &str, title: &str, count: usize, new: Option<(&str, Cv)>, acts: &mut Vec<Act>, body: impl FnOnce(&mut Ui, &mut Vec<Act>)) {
        let header = egui::CollapsingHeader::new(RichText::new(format!("{icon}  {title} ({count})")).strong()).id_salt(id).default_open(count > 0 && count < 40);
        header
            .show(ui, |ui| {
                body(ui, acts);
                if count == 0 {
                    ui.label(RichText::new("None").small().weak());
                }
            })
            .header_response
            .context_menu(|ui| {
                if let Some((label, cv)) = new {
                    if ui.button(label).clicked() {
                        acts.push(Act::Run(cv, None));
                        ui.close_menu();
                    }
                }
            });
    }

    pub(crate) fn toolspace(&mut self, ui: &mut Ui) {
        let mut tab = self.civil_ui.tree.tab;
        ui.horizontal(|ui| {
            crate::ui_kit::tabs(ui, &mut tab, &["Prospector", "Settings"]);
        });
        self.civil_ui.tree.tab = tab;
        ui.separator();
        if tab == 1 {
            self.toolspace_settings(ui);
            return;
        }
        let mut acts: Vec<Act> = Vec::new();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // A rename in progress.
            if let Some((id, mut text)) = self.civil_ui.tree.renaming.take() {
                let mut keep = true;
                ui.horizontal(|ui| {
                    ui.label("Name");
                    let r = ui.text_edit_singleline(&mut text);
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        acts.push(Act::Select(id));
                        if let Some(e) = self.doc.find_mut(id) {
                            if !text.trim().is_empty() {
                                e.name = text.trim().to_string();
                            }
                        }
                        keep = false;
                        self.status = "Renamed.".into();
                        self.schedule_commit_now();
                    } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        keep = false;
                    }
                });
                if keep {
                    self.civil_ui.tree.renaming = Some((id, text));
                }
            }
            let points = self.rows(POINTS);
            self.node(ui, "ts-points", ph::DOTS_NINE, "Points", points.len(), Some(("Import points...", Cv::Points)), &mut acts, |ui, acts| {
                for r in &points {
                    self.row_ui(ui, r, acts, &[("Point table...", Cv::PointTable), ("Point groups...", Cv::PointGroups), ("Description keys...", Cv::DescKeys), ("Export points...", Cv::PointExport)]);
                }
            });
            let groups: Vec<(ElementId, String, usize)> = self
                .doc
                .elements()
                .iter()
                .filter(|e| e.category == POINTS)
                .flat_map(|e| {
                    let mut out = Vec::new();
                    if let Some(d) = civil::get::<PointsData>(&self.doc, e.id, POINTS) {
                        let mut set = d.groups.clone();
                        for g in d.groups.groups() {
                            let n = set.members_of(&d.store, &g.name).map(|m| m.len()).unwrap_or(0);
                            out.push((e.id, g.name.clone(), n));
                        }
                    }
                    out
                })
                .collect();
            self.node(ui, "ts-groups", ph::STACK, "Point Groups", groups.len(), Some(("Point groups...", Cv::PointGroups)), &mut acts, |ui, acts| {
                for (id, name, n) in &groups {
                    let resp = ui.selectable_label(false, format!("{}  {name}  ({n})", ph::STACK));
                    if resp.double_clicked() {
                        acts.push(Act::Run(Cv::PointGroups, Some(*id)));
                    }
                }
            });
            let mut surfaces = self.rows(SURFACE);
            surfaces.extend(self.rows(afrobim_corridor::SURFACE_CATEGORY));
            let mut corridor_surfaces: Vec<(ElementId, String)> = Vec::new();
            for e in self.doc.elements().iter().filter(|e| e.category == CORRIDOR) {
                if let Some((_, b)) = self.civil.built.get(&e.id) {
                    for (n, _) in &b.extra {
                        corridor_surfaces.push((e.id, format!("{} - {n}", e.name)));
                    }
                }
            }
            let total = surfaces.len() + corridor_surfaces.len();
            self.node(ui, "ts-surfaces", ph::MOUNTAINS, "Surfaces", total, Some(("Create surface...", Cv::SurfaceNew)), &mut acts, |ui, acts| {
                for r in &surfaces {
                    let menu: &[(&str, Cv)] = &[("Definition...", Cv::SurfaceDef), ("Surface style...", Cv::SurfaceStyle), ("Volumes...", Cv::Volumes)];
                    self.row_ui(ui, r, acts, menu);
                }
                for (id, n) in &corridor_surfaces {
                    if ui.selectable_label(false, format!("{}  {n}", ph::ROAD_HORIZON)).double_clicked() {
                        acts.push(Act::Zoom(*id));
                    }
                }
            });
            let aligns = self.rows(afrobim_alignment::CATEGORY);
            let profiles = self.rows(PROFILE);
            let views = self.rows(PROFILE_VIEW);
            let samples = self.rows(SAMPLES);
            self.node(ui, "ts-align", ph::PATH, "Alignments", aligns.len(), None, &mut acts, |ui, acts| {
                for r in &aligns {
                    let name = crate::alignment_glue::alignment_of(&self.doc, r.id).map(|a| a.name).unwrap_or_default();
                    egui::CollapsingHeader::new(RichText::new(format!("{}  {}", ph::PATH, r.name))).id_salt(("align", r.id.0)).default_open(true).show(ui, |ui| {
                        let mine = |rows: &[Row], f: &dyn Fn(&afrobim_kernel::Element) -> bool| -> Vec<usize> { rows.iter().enumerate().filter(|(_, row)| self.doc.find(row.id).is_some_and(f)).map(|(i, _)| i).collect() };
                        let ps = mine(&profiles, &|e| civil::get::<ProfileData>(&self.doc, e.id, PROFILE).is_some_and(|d| d.alignment == name));
                        let vs = mine(&views, &|e| civil::get::<ProfileViewData>(&self.doc, e.id, PROFILE_VIEW).is_some_and(|d| d.alignment == name));
                        let ss = mine(&samples, &|e| civil::get::<SampleData>(&self.doc, e.id, SAMPLES).is_some_and(|d| d.group.alignment == name));
                        let has_design = crate::alignment_glue::alignment_of(&self.doc, r.id).is_some_and(|a| a.profile.is_some());
                        let mut sub = |title: &str, icon: &str, rows: &[Row], idx: Vec<usize>, extra: Option<&str>| {
                            egui::CollapsingHeader::new(RichText::new(format!("{icon}  {title} ({})", idx.len() + extra.is_some() as usize))).id_salt((title, r.id.0)).default_open(true).show(ui, |ui| {
                                if let Some(x) = extra {
                                    ui.label(RichText::new(format!("{}  {x}", ph::CHART_LINE_UP)).small());
                                }
                                for i in idx {
                                    self.row_ui(ui, &rows[i], acts, &[]);
                                }
                            });
                        };
                        sub("Profiles", ph::CHART_LINE_UP, &profiles, ps, has_design.then_some("Design profile"));
                        sub("Profile Views", ph::CHART_LINE, &views, vs, None);
                        sub("Sample Line Groups", ph::LINE_SEGMENTS, &samples, ss, None);
                    });
                    let _ = r;
                }
            });
            let corridors = self.rows(CORRIDOR);
            self.node(ui, "ts-corr", ph::ROAD_HORIZON, "Corridors", corridors.len(), Some(("New corridor...", Cv::CorridorModel)), &mut acts, |ui, acts| {
                for r in &corridors {
                    self.row_ui(ui, r, acts, &[("Section views...", Cv::SectionViews), ("Earthworks...", Cv::Earthworks)]);
                }
            });
            let asms = self.rows(ASSEMBLY);
            self.node(ui, "ts-asm", ph::STACK_SIMPLE, "Assemblies", asms.len(), Some(("Assembly editor...", Cv::Assembly)), &mut acts, |ui, acts| {
                for r in &asms {
                    self.row_ui(ui, r, acts, &[("Edit assembly...", Cv::Assembly)]);
                }
            });
            let parcels = self.rows(PARCEL);
            self.node(ui, "ts-parcels", ph::POLYGON, "Parcels (Site)", parcels.len(), Some(("Create parcel...", Cv::ParcelPick)), &mut acts, |ui, acts| {
                for r in &parcels {
                    self.row_ui(ui, r, acts, &[("Parcel properties...", Cv::ParcelProps)]);
                }
            });
            ui.separator();
            // The existing categories (signs, markings, drainage and the rest).
            let categories: Vec<(String, String)> = self.modules.categories().filter(|(c, _)| !civil::is_civil(c) && *c != afrobim_alignment::CATEGORY && *c != afrobim_corridor::SURFACE_CATEGORY).map(|(c, n)| (c.to_string(), n.to_string())).collect();
            for (category, title) in categories {
                let rows = self.rows(&category);
                if rows.is_empty() {
                    continue;
                }
                self.node(ui, &format!("ts-{category}"), ph::SQUARE, &title, rows.len(), None, &mut acts, |ui, acts| {
                    for r in &rows {
                        self.row_ui(ui, r, acts, &[]);
                    }
                });
            }
        });
        for a in acts {
            self.apply_act(a);
        }
    }

    fn apply_act(&mut self, a: Act) {
        match a {
            Act::Select(id) => self.doc.select(&[id], SelectMode::Replace),
            Act::Zoom(id) => self.zoom_to_element(id),
            Act::Rename(id) => {
                let name = self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
                self.civil_ui.tree.renaming = Some((id, name));
            }
            Act::Properties(id) => {
                self.doc.select(&[id], SelectMode::Replace);
                self.show_right = true;
            }
            Act::Delete(id) => {
                let name = self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
                self.doc.remove(id);
                self.status = format!("Deleted {name}. UNDO brings it back.");
                self.schedule_commit_now();
            }
            Act::ToggleHidden(id) => {
                if let Some(e) = self.doc.find_mut(id) {
                    let now = hidden(e);
                    e.properties.insert(PROP_HIDDEN.into(), if now { "0".into() } else { "1".into() });
                }
                self.schedule_commit_now();
            }
            Act::Run(cv, id) => {
                if let Some(id) = id {
                    self.doc.select(&[id], SelectMode::Replace);
                }
                self.civil_run_pending = Some(cv);
            }
        }
    }

    fn toolspace_settings(&mut self, ui: &mut Ui) {
        let crs = afrobim_kernel::project_crs(&self.doc);
        let units = crate::anno_ui::units_of(&self.doc);
        ui.add_space(4.0);
        ui.label(RichText::new("Project coordinate system").strong());
        match &crs {
            Some(c) => {
                ui.label(c.label());
                ui.label(RichText::new("Objects sit at their source coordinates. Nothing is reprojected or shifted.").small().weak());
            }
            None => {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, "No coordinate system is set for this project.");
            }
        }
        if ui.button(format!("{}  Edit coordinate system...", ph::GLOBE)).clicked() {
            self.civil_run_pending = None;
            self.pending_action = Some(Action::CoordSystem);
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Units").strong());
        ui.label(format!("Length shown as {}", units.length_u(1000.0)));
        if ui.button(format!("{}  Edit units...", ph::RULER)).clicked() {
            self.pending_action = Some(Action::Units);
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Design standard").strong());
        ui.label(self.standard.clone());
        if ui.button(format!("{}  Edit project settings...", ph::GEAR)).clicked() {
            self.pending_action = Some(Action::ProjectSettings);
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Civil objects").strong());
        for (c, _, plural) in KINDS {
            ui.label(format!("{plural}: {}", self.doc.elements().iter().filter(|e| e.category == c).count()));
        }
    }
}
