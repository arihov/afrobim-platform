//! The Issues window (`ISSUES`) and the BCF exchange windows (`BCFOUT`, `BCFIN`).
//!
//! Issues live in the coordination element (`Coordination::issues`), so they are saved, opened
//! and undone with the document. An issue can be free-standing or raised from a clash or from a
//! group of clashes (the grouping distance is entered, none is offered). A viewpoint is taken from
//! the current 3D camera together with a PNG of the 3D view, and "Go to viewpoint" puts the camera
//! and the selection back.
//!
//! BCF files are BCF 2.1. They have been written and read back by this software and checked
//! against the 2.1 schemas, but never opened in another BCF tool; BCF 3.0 is refused.

use std::collections::HashMap;

use afrobim_coordination::{export_bcf, group_clashes, import_bcf, values, BcfOptions, ClashGroup, ElementRef, GroupOptions, ImportOptions, Issue};
use afrobim_kernel::{ElementId, SelectMode};
use egui::{RichText};

use crate::app::AfroBimApp;
use crate::coordination_glue as glue;

#[allow(non_snake_case)]

fn BAD() -> egui::Color32 {

    crate::theme::danger()

}
#[allow(non_snake_case)]
fn WARN() -> egui::Color32 {
    crate::theme::warn()
}

/// What the BCF dialogs say about the files, as the engine's README states it.
pub const BCF_LIMITS: &str = "AfroBIM writes and reads BCF 2.1 only; BCF 3.0 is not supported and a 3.0 file is refused. The files have been checked against the BCF 2.1 schemas and read back by AfroBIM, but they have not been opened in any other BCF tool, so how another tool shows the viewpoints and the selected objects is not known.";

const STATUSES: [&str; 5] = [values::STATUS_OPEN, values::STATUS_IN_PROGRESS, values::STATUS_RESOLVED, values::STATUS_CLOSED, values::STATUS_REOPENED];
const PRIORITIES: [&str; 4] = [values::PRIORITY_LOW, values::PRIORITY_NORMAL, values::PRIORITY_HIGH, values::PRIORITY_CRITICAL];
const TYPES: [&str; 2] = [values::TYPE_ISSUE, values::TYPE_CLASH];

/// A viewpoint snapshot waiting for the screenshot of the 3D view.
pub struct SnapRequest {
    issue: String,
    viewpoint: String,
    /// Frames to wait before the screenshot is requested (the 3D view has to be on screen).
    wait: u8,
    requested: bool,
}

/// What the Issues windows keep between frames.
pub struct IssuesUi {
    pub open: bool,
    pub bcf_out_open: bool,
    pub bcf_in_open: bool,
    pub author: String,
    new_title: String,
    pub selected: Option<String>,
    clash_pick: Option<String>,
    group_distance: String,
    group_share: bool,
    groups: Vec<ClashGroup>,
    group_pick: Option<usize>,
    comment: String,
    due_text: String,
    due_for: Option<String>,
    labels_text: String,
    labels_for: Option<String>,
    pub bcf_report: Vec<String>,
    message: String,
    pub snap: Option<SnapRequest>,
    textures: HashMap<String, egui::TextureHandle>,
}

impl Default for IssuesUi {
    fn default() -> Self {
        IssuesUi {
            open: false,
            bcf_out_open: false,
            bcf_in_open: false,
            author: std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default(),
            new_title: String::new(),
            selected: None,
            clash_pick: None,
            group_distance: String::new(),
            group_share: true,
            groups: Vec::new(),
            group_pick: None,
            comment: String::new(),
            due_text: String::new(),
            due_for: None,
            labels_text: String::new(),
            labels_for: None,
            bcf_report: Vec::new(),
            message: String::new(),
            snap: None,
            textures: HashMap::new(),
        }
    }
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn date_text(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    afrobim_schedule::Date::from_ymd(1970, 1, 1).map(|d| d.add_days(days).to_string()).unwrap_or_default()
}

fn parse_due(text: &str) -> Result<Option<i64>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let d = afrobim_schedule::Date::parse(t).ok_or_else(|| format!("'{t}' is not a date (write YYYY-MM-DD)."))?;
    let epoch = afrobim_schedule::Date::from_ymd(1970, 1, 1).ok_or("no epoch")?;
    Ok(Some(d.days_since(epoch) * 86_400))
}

fn combo(ui: &mut egui::Ui, salt: &str, value: &mut String, options: &[&str]) -> bool {
    let before = value.clone();
    egui::ComboBox::from_id_salt(salt).width(110.0).selected_text(if value.is_empty() { "not set" } else { value.as_str() }).show_ui(ui, |ui| {
        for o in options {
            ui.selectable_value(value, o.to_string(), *o);
        }
    });
    *value != before
}

impl AfroBimApp {
    // ─── the data ────────────────────────────────────────────────────────────────

    fn author(&self) -> String {
        let a = self.issues.author.trim();
        if a.is_empty() { "AfroBIM user".to_string() } else { a.to_string() }
    }

    /// Writes the coordination data (tests, results, issues) back to the document element.
    pub(crate) fn store_coordination(&mut self) {
        if let Err(e) = glue::store(&mut self.doc, &self.coord.co) {
            self.status = format!("Not stored: {e}");
        }
        self.coord.mark_loaded(&self.doc);
        self.schedule_commit_now();
    }

    /// A free-standing issue. Returns its GUID.
    pub(crate) fn issue_new(&mut self, title: &str) -> Result<String, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("Give the issue a title.".into());
        }
        self.coord.sync(&self.doc);
        let (author, now) = (self.author(), now_unix());
        let guid = self.coord.co.new_issue(title, &author, now).guid.clone();
        self.store_coordination();
        Ok(guid)
    }

    /// Raises an issue from a clash. Returns its GUID (the same one when raised twice).
    pub(crate) fn issue_from_clash(&mut self, clash_id: &str) -> Result<String, String> {
        self.coord.sync(&self.doc);
        let (author, now) = (self.author(), now_unix());
        let guid = self.coord.co.raise_issue_from_clash(clash_id, &author, now).ok_or_else(|| format!("There is no clash {clash_id}."))?;
        self.store_coordination();
        Ok(guid)
    }

    /// Groups the live clashes by the entered distance and shared elements. No distance is offered:
    /// an empty entry groups by shared element only when "share an element" is ticked.
    pub(crate) fn group_clashes_entered(&mut self, distance_text: &str, share: bool) -> Result<Vec<ClashGroup>, String> {
        self.coord.sync(&self.doc);
        let t = distance_text.trim();
        let distance = if t.is_empty() { None } else { Some(t.replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0).ok_or_else(|| format!("'{t}' is not a distance in millimetres."))?) };
        if distance.is_none() && !share {
            return Err("Enter a grouping distance in millimetres, or tick 'share an element'. AfroBIM offers no distance.".into());
        }
        let options = GroupOptions { share_element: share, distance_mm: distance, ignore_shared: Vec::new() };
        let live: Vec<_> = self.coord.marked().cloned().collect();
        Ok(group_clashes(&live, &options))
    }

    /// Raises one issue for a group.
    pub(crate) fn issue_from_group(&mut self, group: &ClashGroup) -> String {
        let (author, now) = (self.author(), now_unix());
        let issue = Issue::from_group(group, &self.coord.co.results.clashes, &author, now);
        let guid = issue.guid.clone();
        if !self.coord.co.issues.iter().any(|i| i.guid == guid) {
            self.coord.co.issues.push(issue);
        }
        self.store_coordination();
        guid
    }

    // ─── viewpoints ─────────────────────────────────────────────────────────────

    /// Adds a viewpoint to the issue from the current 3D camera and the current selection, and asks
    /// for a snapshot of the 3D view (it arrives a frame or two later).
    pub(crate) fn viewpoint_capture(&mut self, issue_guid: &str) -> Result<(), String> {
        self.coord.sync(&self.doc);
        let camera = glue::viewpoint_camera(&self.camera);
        let selected: Vec<ElementRef> = self.doc.selection().iter().filter_map(|id| self.doc.find(*id)).map(|e| ElementRef::element(e.id, &e.name)).collect();
        let issue = self.coord.co.issue_mut(issue_guid).ok_or("That issue is gone.")?;
        let vp = issue.add_viewpoint(Some(camera));
        vp.selected = selected;
        let vp_guid = vp.guid.clone();
        self.store_coordination();
        if !self.view_3d {
            self.view_3d = true;
            self.status = "Viewpoint saved from the camera. Switched to the 3D view for the snapshot.".into();
        } else {
            self.status = "Viewpoint saved from the current 3D camera; taking the snapshot.".into();
        }
        self.issues.snap = Some(SnapRequest { issue: issue_guid.to_string(), viewpoint: vp_guid, wait: 3, requested: false });
        Ok(())
    }

    /// Restores the camera and the selection of a viewpoint.
    pub(crate) fn viewpoint_go(&mut self, issue_guid: &str, viewpoint_guid: &str) -> Result<(), String> {
        let issue = self.coord.co.issue(issue_guid).ok_or("That issue is gone.")?.clone();
        let vp = issue.viewpoints.iter().find(|v| v.guid == viewpoint_guid).ok_or("That viewpoint is gone.")?.clone();
        let cam = vp.camera.as_ref().ok_or("This viewpoint has no camera.")?;
        if !cam.is_valid() {
            return Err("This viewpoint's camera has no usable direction.".into());
        }
        // The orbit camera needs a target: the issue's location, else the middle of the model.
        let mut depth = issue.location.map(|p| cam.depth_of(p)).filter(|d| *d > 100.0);
        if depth.is_none() {
            let points = self.shown_points(self.site_mode());
            if !points.is_empty() {
                let mut c = [0.0; 3];
                for p in &points {
                    for i in 0..3 {
                        c[i] += p[i] / points.len() as f64;
                    }
                }
                depth = Some(cam.depth_of(c)).filter(|d| *d > 100.0);
            }
        }
        let distance = depth.unwrap_or(10_000.0);
        self.camera.set_look(cam.position, cam.direction, distance);
        self.view_3d = true;
        self.fitted_3d = true;
        let ids: Vec<ElementId> = vp.selected.iter().filter_map(|r| r.id).filter(|id| self.doc.find(*id).is_some()).collect();
        self.doc.select(&ids, SelectMode::Replace);
        self.status = format!("Viewpoint restored: camera and {} selected element(s).", ids.len());
        Ok(())
    }

    /// Called every frame: requests the screenshot for a pending viewpoint and stores it when it arrives.
    pub(crate) fn snapshot_tick(&mut self, ctx: &egui::Context) {
        let Some(snap) = self.issues.snap.as_mut() else { return };
        if !snap.requested {
            if snap.wait > 0 {
                snap.wait -= 1;
            } else {
                snap.requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(format!("viewpoint:{}", snap.viewpoint))));
            }
            ctx.request_repaint();
            return;
        }
        let tag = format!("viewpoint:{}", snap.viewpoint);
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, user_data, .. } if user_data.data.as_ref().and_then(|d| d.downcast_ref::<String>()).is_some_and(|s| *s == tag) => Some(image.clone()),
                _ => None,
            })
        });
        ctx.request_repaint();
        let Some(image) = shot else { return };
        let (issue, vp) = (snap.issue.clone(), snap.viewpoint.clone());
        self.issues.snap = None;
        let ppp = ctx.pixels_per_point();
        let area = crate::win::work_rect(ctx);
        let png = crop_to_png(&image, area, ppp);
        match png {
            Some(bytes) => {
                if let Some(v) = self.coord.co.issue_mut(&issue).and_then(|i| i.viewpoints.iter_mut().find(|v| v.guid == vp)) {
                    v.snapshot_png = Some(bytes);
                }
                self.issues.textures.remove(&vp);
                self.store_coordination();
                self.status = "Viewpoint snapshot stored with the issue.".into();
            }
            None => self.status = "The snapshot could not be encoded; the viewpoint keeps its camera only.".into(),
        }
    }

    /// True while a snapshot is being taken: the windows are left out so they are not in the picture.
    pub(crate) fn hide_windows_for_snapshot(&self) -> bool {
        self.issues.snap.as_ref().is_some_and(|s| s.requested || s.wait == 0)
    }

    // ─── BCF ────────────────────────────────────────────────────────────────────

    /// The BCF options: the project name is the one the IFC export uses, and the IFC file name is
    /// the last IFC export's, when there was one.
    pub(crate) fn bcf_options(&self) -> BcfOptions {
        BcfOptions { project_name: self.ifc_project_name(), ifc_file_name: self.last_ifc_name.clone(), ..BcfOptions::default() }
    }

    /// Writes the issues as BCF 2.1. Returns the report lines.
    pub(crate) fn bcf_write(&mut self, path: &std::path::Path) -> Result<Vec<String>, String> {
        self.coord.sync(&self.doc);
        if self.coord.co.issues.is_empty() {
            return Err("There are no issues to export.".into());
        }
        let out = bcf_bytes(&self.coord.co.issues, &self.bcf_options())?;
        std::fs::write(path, &out.bytes).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
        let mut report = vec![format!("Wrote {} issue(s) as BCF 2.1 to {} ({} bytes).", self.coord.co.issues.len(), path.display(), out.bytes.len())];
        report.push(match &self.last_ifc_name {
            Some(n) => format!("The IFC file named in the topics is '{n}' (the last IFC export)."),
            None => "No IFC file has been exported in this session, so the topics name no IFC file.".to_string(),
        });
        report.extend(out.notes);
        Ok(report)
    }

    /// Reads a BCF file and merges its issues (same GUID replaces). Returns the report lines.
    pub(crate) fn bcf_read(&mut self, path: &std::path::Path) -> Result<Vec<String>, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        let known: Vec<ElementId> = self.doc.elements().iter().map(|e| e.id).collect();
        self.coord.sync(&self.doc);
        let project = self.ifc_project_name();
        let mut report = bcf_merge(&mut self.coord.co, &bytes, &project, known)?;
        report[0] = format!("{} ({})", report[0], path.display());
        self.store_coordination();
        Ok(report)
    }

    // ─── windows ────────────────────────────────────────────────────────────────

    pub(crate) fn issues_windows(&mut self, ctx: &egui::Context) {
        self.coord.sync(&self.doc);
        if self.issues.bcf_out_open {
            self.bcf_out_window(ctx);
        }
        if self.issues.bcf_in_open {
            self.bcf_in_window(ctx);
        }
        if !self.issues.open {
            return;
        }
        let mut open = true;
        let mut changed = false;
        let mut go: Option<(String, String)> = None;
        let mut capture: Option<String> = None;
        let mut raise_clash = false;
        let mut create = false;
        let mut group = false;
        let mut raise_group: Option<usize> = None;
        let mut raise_all_groups = false;
        let mut delete: Option<String> = None;
        let mut message: Option<String> = None;
        crate::win::new(ctx, "Issues").open(&mut open).default_width(960.0).default_height(640.0).show(ctx, |ui| {
            ui.label(RichText::new("Issues are kept in the document with the clash tests. Raise one from a clash or a group of clashes, or write a free-standing one; add comments and viewpoints; exchange them as BCF 2.1 (BCFOUT, BCFIN).").small().weak());
            ui.horizontal(|ui| {
                ui.label("Your name");
                ui.add(egui::TextEdit::singleline(&mut self.issues.author).desired_width(140.0).hint_text("author of what you add"));
                ui.separator();
                ui.add(egui::TextEdit::singleline(&mut self.issues.new_title).desired_width(260.0).hint_text("title of a new issue"));
                if ui.button("New issue").clicked() {
                    create = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("From a clash");
                let live: Vec<(String, String)> = self.coord.co.results.clashes.iter().map(|c| (c.id.clone(), format!("{}: {} / {}", c.id, c.mark_a, c.mark_b))).collect();
                let shown = live.iter().find(|(id, _)| Some(id) == self.issues.clash_pick.as_ref()).map(|(_, t)| t.clone()).unwrap_or_else(|| if live.is_empty() { "no clashes (run CLASH)".into() } else { "choose a clash".into() });
                egui::ComboBox::from_id_salt("issue-clash-pick").width(300.0).selected_text(shown).show_ui(ui, |ui| {
                    for (id, text) in &live {
                        ui.selectable_value(&mut self.issues.clash_pick, Some(id.clone()), text);
                    }
                });
                if ui.add_enabled(self.issues.clash_pick.is_some(), egui::Button::new("Raise issue")).clicked() {
                    raise_clash = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Group clashes within");
                ui.add(egui::TextEdit::singleline(&mut self.issues.group_distance).desired_width(80.0).hint_text("mm, yours"));
                ui.checkbox(&mut self.issues.group_share, "or share an element");
                if ui.button("Group").clicked() {
                    group = true;
                }
                if !self.issues.groups.is_empty() {
                    let n = self.issues.groups.len();
                    egui::ComboBox::from_id_salt("issue-group-pick").width(260.0).selected_text(self.issues.group_pick.map(|i| format!("group {} ({} clashes)", i + 1, self.issues.groups[i].clash_ids.len())).unwrap_or_else(|| format!("{n} group(s)"))).show_ui(ui, |ui| {
                        for (i, g) in self.issues.groups.iter().enumerate() {
                            ui.selectable_value(&mut self.issues.group_pick, Some(i), format!("group {} ({} clashes, {} elements)", i + 1, g.clash_ids.len(), g.elements.len()));
                        }
                    });
                    if ui.add_enabled(self.issues.group_pick.is_some(), egui::Button::new("Raise issue for the group")).clicked() {
                        raise_group = self.issues.group_pick;
                    }
                    if ui.button("One issue per group").clicked() {
                        raise_all_groups = true;
                    }
                }
            });
            if !self.issues.message.is_empty() {
                ui.label(RichText::new(&self.issues.message).small().color(WARN()));
            }
            ui.separator();
            let n = self.coord.co.issues.len();
            if n == 0 {
                ui.label(RichText::new("No issues yet.").weak());
            }
            {
                // The list.
                egui::ScrollArea::vertical().id_salt("issue-list").max_height(150.0).auto_shrink([false, true]).show(ui, |ui| {
                    egui::Grid::new("issue-list-grid").num_columns(7).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                        for h in ["Title", "Status", "Priority", "Type", "Assignee", "Due", "Labels"] {
                            ui.label(RichText::new(h).strong());
                        }
                        ui.end_row();
                        for i in &self.coord.co.issues {
                            let on = self.issues.selected.as_deref() == Some(i.guid.as_str());
                            if ui.selectable_label(on, &i.title).clicked() {
                                self.issues.selected = Some(i.guid.clone());
                            }
                            ui.label(&i.status);
                            ui.label(&i.priority);
                            ui.label(&i.issue_type);
                            ui.label(&i.assignee);
                            ui.label(i.due.map(date_text).unwrap_or_default());
                            ui.label(i.labels.join(", "));
                            ui.end_row();
                        }
                    });
                });
                // The selected issue.
                ui.separator();
                let Some(guid) = self.issues.selected.clone().filter(|g| self.coord.co.issue(g).is_some()) else {
                    ui.label(RichText::new("Select an issue to edit it.").weak());
                    return;
                };
                if self.issues.due_for.as_deref() != Some(guid.as_str()) {
                    self.issues.due_text = self.coord.co.issue(&guid).and_then(|i| i.due).map(date_text).unwrap_or_default();
                    self.issues.due_for = Some(guid.clone());
                }
                if self.issues.labels_for.as_deref() != Some(guid.as_str()) {
                    self.issues.labels_text = self.coord.co.issue(&guid).map(|i| i.labels.join(", ")).unwrap_or_default();
                    self.issues.labels_for = Some(guid.clone());
                }
                let author = self.author();
                let now = now_unix();
                let textures = &mut self.issues.textures;
                let (due_text, labels_text, comment) = (&mut self.issues.due_text, &mut self.issues.labels_text, &mut self.issues.comment);
                let units = crate::anno_ui::units_of(&self.doc);
                let Some(issue) = self.coord.co.issue_mut(&guid) else { return };
                egui::ScrollArea::vertical().id_salt("issue-detail").auto_shrink([false, false]).show(ui, |ui| {
                    changed |= ui.add(egui::TextEdit::singleline(&mut issue.title).desired_width(f32::INFINITY)).changed();
                    egui::Grid::new("issue-fields").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                        ui.label("Status");
                        let mut status = issue.status.clone();
                        if combo(ui, "issue-status", &mut status, &STATUSES) {
                            issue.set_status(&status, &author, now);
                            changed = true;
                        }
                        ui.end_row();
                        ui.label("Priority");
                        changed |= combo(ui, "issue-priority", &mut issue.priority, &PRIORITIES);
                        ui.end_row();
                        ui.label("Type");
                        changed |= combo(ui, "issue-type", &mut issue.issue_type, &TYPES);
                        ui.end_row();
                        ui.label("Assignee");
                        changed |= ui.add(egui::TextEdit::singleline(&mut issue.assignee).desired_width(160.0)).changed();
                        ui.end_row();
                        ui.label("Due date");
                        if ui.add(egui::TextEdit::singleline(due_text).desired_width(110.0).hint_text("YYYY-MM-DD")).changed() {
                            match parse_due(due_text) {
                                Ok(d) => {
                                    issue.due = d;
                                    changed = true;
                                }
                                Err(e) => message = Some(e),
                            }
                        }
                        ui.end_row();
                        ui.label("Labels");
                        if ui.add(egui::TextEdit::singleline(labels_text).desired_width(200.0).hint_text("comma separated")).changed() {
                            issue.labels = labels_text.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                            changed = true;
                        }
                        ui.end_row();
                    });
                    ui.label(RichText::new("Description").small().weak());
                    changed |= ui.add(egui::TextEdit::multiline(&mut issue.description).desired_rows(3).desired_width(f32::INFINITY)).changed();
                    if !issue.elements.is_empty() {
                        ui.label(RichText::new(format!("Elements: {}", issue.elements.iter().map(|e| if e.mark.is_empty() { e.ifc_guid.clone().unwrap_or_default() } else { e.mark.clone() }).collect::<Vec<_>>().join(", "))).small());
                    }
                    if let Some(p) = issue.location {
                        ui.label(RichText::new(format!("Location: {}, {}, {}", units.length_u(p[0]), units.length_u(p[1]), units.length_u(p[2]))).small().weak());
                    }
                    ui.separator();
                    ui.label(RichText::new("Comments").strong());
                    for c in &issue.comments {
                        ui.label(RichText::new(format!("{} ({}):", c.author, date_text(c.date))).small().weak());
                        ui.label(&c.text);
                    }
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(comment).desired_width(240.0).hint_text("add a comment"));
                        if ui.button("Add").clicked() && !comment.trim().is_empty() {
                            issue.add_comment(&author, now, comment.trim());
                            comment.clear();
                            changed = true;
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Viewpoints").strong());
                        if ui.button("Capture the 3D view").on_hover_text("Saves the current 3D camera and selection, with a snapshot of the 3D view").clicked() {
                            capture = Some(guid.clone());
                        }
                    });
                    let mut remove_vp: Option<usize> = None;
                    for (n, vp) in issue.viewpoints.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if let Some(png) = &vp.snapshot_png {
                                let tex = textures.entry(vp.guid.clone()).or_insert_with(|| {
                                    let img = image::load_from_memory(png).map(|i| i.to_rgba8()).unwrap_or_else(|_| image::RgbaImage::new(1, 1));
                                    let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                                    ui.ctx().load_texture(format!("vp-{}", vp.guid), ci, egui::TextureOptions::LINEAR)
                                });
                                let w = 180.0;
                                let h = w * tex.size()[1] as f32 / tex.size()[0].max(1) as f32;
                                ui.image((tex.id(), egui::vec2(w, h)));
                            } else {
                                ui.label(RichText::new("(no snapshot yet)").weak());
                            }
                            ui.vertical(|ui| {
                                ui.label(format!("Viewpoint {}: {} selected", n + 1, vp.selected.len()));
                                if ui.button("Go to viewpoint").clicked() {
                                    go = Some((guid.clone(), vp.guid.clone()));
                                }
                                if ui.small_button("Delete").clicked() {
                                    remove_vp = Some(n);
                                }
                            });
                        });
                    }
                    if let Some(n) = remove_vp {
                        issue.viewpoints.remove(n);
                        changed = true;
                    }
                    ui.separator();
                    if ui.button(RichText::new("Delete this issue").color(BAD())).clicked() {
                        delete = Some(guid.clone());
                    }
                });
            }
        });
        self.issues.open = open;
        if let Some(m) = message {
            self.issues.message = m;
        }
        if create {
            let t = self.issues.new_title.clone();
            match self.issue_new(&t) {
                Ok(g) => {
                    self.issues.selected = Some(g);
                    self.issues.new_title.clear();
                    self.issues.message.clear();
                }
                Err(e) => self.issues.message = e,
            }
        }
        if raise_clash {
            if let Some(id) = self.issues.clash_pick.clone() {
                match self.issue_from_clash(&id) {
                    Ok(g) => {
                        self.issues.selected = Some(g);
                        self.issues.message.clear();
                    }
                    Err(e) => self.issues.message = e,
                }
            }
        }
        if group {
            let (d, s) = (self.issues.group_distance.clone(), self.issues.group_share);
            match self.group_clashes_entered(&d, s) {
                Ok(g) => {
                    self.issues.message = format!("{} group(s) from {} live clash(es).", g.len(), self.coord.marked().count());
                    self.issues.groups = g;
                    self.issues.group_pick = None;
                }
                Err(e) => self.issues.message = e,
            }
        }
        if let Some(i) = raise_group {
            if let Some(g) = self.issues.groups.get(i).cloned() {
                let guid = self.issue_from_group(&g);
                self.issues.selected = Some(guid);
            }
        }
        if raise_all_groups {
            for g in self.issues.groups.clone() {
                let guid = self.issue_from_group(&g);
                self.issues.selected = Some(guid);
            }
        }
        if let Some(g) = delete {
            self.coord.co.issues.retain(|i| i.guid != g);
            self.issues.selected = None;
            changed = true;
        }
        if let Some(g) = capture {
            if let Err(e) = self.viewpoint_capture(&g) {
                self.issues.message = e;
            }
        }
        if let Some((g, v)) = go {
            if let Err(e) = self.viewpoint_go(&g, &v) {
                self.issues.message = e;
            }
        }
        if changed {
            self.store_coordination();
        }
    }

    fn bcf_out_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut export = false;
        crate::win::new(ctx, "Export issues as BCF").open(&mut open).default_width(560.0).show(ctx, |ui| {
            ui.label(RichText::new(BCF_LIMITS).small().color(WARN()));
            ui.add_space(4.0);
            ui.label(format!("{} issue(s) in the document.", self.coord.co.issues.len()));
            ui.label(match &self.last_ifc_name {
                Some(n) => format!("IFC file named in the topics: {n} (the last IFC export)."),
                None => "No IFC file has been exported yet, so the topics will name none. Export the IFC first (IFCOUT) if the BCF should point at it.".to_string(),
            });
            if ui.button("Choose a file and export").clicked() {
                export = true;
            }
            for l in &self.issues.bcf_report {
                ui.label(RichText::new(l).small());
            }
        });
        self.issues.bcf_out_open = open;
        if export {
            if let Some(mut path) = rfd::FileDialog::new().add_filter("BCF 2.1", &["bcf", "bcfzip"]).set_file_name("issues.bcf").save_file() {
                if path.extension().is_none() {
                    path.set_extension("bcf");
                }
                match self.bcf_write(&path) {
                    Ok(r) => {
                        self.status = r[0].clone();
                        self.issues.bcf_report = r;
                    }
                    Err(e) => self.issues.bcf_report = vec![e],
                }
            }
        }
    }

    fn bcf_in_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut import = false;
        crate::win::new(ctx, "Import issues from BCF").open(&mut open).default_width(560.0).show(ctx, |ui| {
            ui.label(RichText::new(BCF_LIMITS).small().color(WARN()));
            ui.label(RichText::new("An issue with the same GUID replaces the one in the document; the others are added. What the model cannot hold is listed as not imported.").small().weak());
            if ui.button("Choose a file and import").clicked() {
                import = true;
            }
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for l in &self.issues.bcf_report {
                    ui.label(RichText::new(l).small());
                }
            });
        });
        self.issues.bcf_in_open = open;
        if import {
            if let Some(path) = rfd::FileDialog::new().add_filter("BCF", &["bcf", "bcfzip"]).pick_file() {
                match self.bcf_read(&path) {
                    Ok(r) => {
                        self.status = r[0].clone();
                        self.issues.bcf_report = r;
                    }
                    Err(e) => self.issues.bcf_report = vec![e],
                }
            }
        }
    }

    /// Opens the issue window with the demo clash zoomed to nothing in particular.
    pub(crate) fn open_issues(&mut self) {
        self.coord.sync(&self.doc);
        self.issues.open = true;
        if self.issues.selected.is_none() {
            self.issues.selected = self.coord.co.issues.first().map(|i| i.guid.clone());
        }
    }
}

/// The BCF 2.1 bytes of the issues and the engine's notes on what was left out.
pub fn bcf_bytes(issues: &[Issue], options: &BcfOptions) -> Result<afrobim_coordination::BcfExport, String> {
    export_bcf(issues, options).map_err(|e| e.to_string())
}

/// Reads BCF bytes and merges the issues into `co` (the same GUID replaces). Returns the report
/// lines, the first being the summary of what was read.
pub fn bcf_merge(co: &mut afrobim_coordination::Coordination, bytes: &[u8], project: &str, known: Vec<ElementId>) -> Result<Vec<String>, String> {
    let got = import_bcf(bytes, &ImportOptions { project_name: project.to_string(), known_ids: known }).map_err(|e| e.to_string())?;
    let (added, replaced) = co.merge_issues(got.issues);
    let r = got.report;
    let mut report = vec![
        format!("Read BCF {}: {} topic(s), {} comment(s), {} viewpoint(s), {} snapshot(s)", r.version, r.topics, r.comments, r.viewpoints, r.snapshots),
        format!("{added} issue(s) added, {replaced} replaced (same GUID). {} component(s) matched elements of this document, {} are foreign.", r.components_matched, r.components_foreign),
    ];
    report.extend(r.skipped.iter().map(|s| format!("Not imported: {s}")));
    report.extend(r.warnings.iter().map(|w| format!("Warning: {w}")));
    Ok(report)
}

/// Crops the screenshot of the window to `area` (points) and encodes it as a PNG at most 900 px wide.
fn crop_to_png(image: &egui::ColorImage, area: egui::Rect, ppp: f32) -> Option<Vec<u8>> {
    let [w, h] = image.size;
    let x0 = ((area.min.x * ppp).max(0.0) as usize).min(w.saturating_sub(1));
    let y0 = ((area.min.y * ppp).max(0.0) as usize).min(h.saturating_sub(1));
    let x1 = ((area.max.x * ppp).max(0.0) as usize).clamp(x0 + 1, w);
    let y1 = ((area.max.y * ppp).max(0.0) as usize).clamp(y0 + 1, h);
    let (cw, ch) = (x1 - x0, y1 - y0);
    let mut buf = Vec::with_capacity(cw * ch * 4);
    for y in y0..y1 {
        for x in x0..x1 {
            let c = image.pixels[y * w + x];
            buf.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
        }
    }
    let img = image::RgbaImage::from_raw(cw as u32, ch as u32, buf)?;
    let img = if cw > 900 { image::imageops::resize(&img, 900, ((ch as f64) * 900.0 / cw as f64).max(1.0) as u32, image::imageops::FilterType::Triangle) } else { img };
    let mut out = Vec::new();
    use image::ImageEncoder;
    image::codecs::png::PngEncoder::new(&mut out).write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8).ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_coordination::{Body, ClashTest, Coordination, Projection, Scene, Selection};
    use afrobim_draw::Mesh3;
    use afrobim_kernel::Document;

    fn two_boxes() -> Coordination {
        let cube = |min: [f64; 3], max: [f64; 3]| Mesh3::cuboid("T", min, max, [128, 128, 128]);
        let a = Body::new(ElementId(1), "A-1", "drain", "Drains", None, vec![cube([0.0; 3], [1000.0; 3])]);
        let b = Body::new(ElementId(2), "B-1", "structure", "Structures", None, vec![cube([600.0, 0.0, 0.0], [1600.0, 1000.0, 1000.0])]);
        let mut co = Coordination::default();
        co.tests.push(ClashTest::hard("Drains vs structures", Selection::all(), Selection::all()));
        co.run(&Scene::new(vec![a, b]));
        co
    }

    #[test]
    fn an_issue_survives_the_document_and_a_bcf_round_trip_through_the_glue() {
        let mut co = two_boxes();
        assert_eq!(co.results.clashes.len(), 1);
        let clash = co.results.clashes[0].id.clone();
        let guid = co.raise_issue_from_clash(&clash, "ann", 1_800_000_000).unwrap();
        {
            let i = co.issue_mut(&guid).unwrap();
            i.assignee = "bob".into();
            i.due = parse_due("2026-12-01").unwrap();
            i.labels = vec!["drainage".into()];
            i.add_comment("bob", 1_800_000_100, "Moving the drain");
            let vp = i.add_location_viewpoint([0.0, -1.0, 0.3], 4000.0, Projection::Perspective { fov_deg: 50.0 }).unwrap();
            let mut png = Vec::new();
            use image::ImageEncoder;
            image::codecs::png::PngEncoder::new(&mut png).write_image(&[10, 20, 30, 255, 40, 50, 60, 255], 2, 1, image::ExtendedColorType::Rgba8).unwrap();
            vp.snapshot_png = Some(png);
        }
        // Kept in the document with the coordination element, and read back.
        let mut doc = Document::default();
        glue::store(&mut doc, &co).unwrap();
        let reopened = Document::from_json(&doc.to_json()).unwrap();
        let (_, back) = glue::load(&reopened);
        // The document's JSON may move a float by one unit in its last place, so the camera is
        // compared within a micron and the rest exactly.
        let mut same = back.issues.clone();
        let (want, got) = (co.issues[0].viewpoints[0].camera.clone().unwrap(), same[0].viewpoints[0].camera.clone().unwrap());
        for i in 0..3 {
            assert!((want.position[i] - got.position[i]).abs() < 1e-6 && (want.direction[i] - got.direction[i]).abs() < 1e-9);
        }
        same[0].viewpoints[0].camera = co.issues[0].viewpoints[0].camera.clone();
        assert_eq!(same, co.issues);
        assert_eq!(date_text(back.issues[0].due.unwrap()), "2026-12-01");
        // BCF out, then in, into an empty coordination: the same issue comes back.
        let opts = BcfOptions { project_name: "Demo".into(), ifc_file_name: Some("model.ifc".into()), ..BcfOptions::default() };
        let out = bcf_bytes(&back.issues, &opts).unwrap();
        let mut other = Coordination::default();
        let report = bcf_merge(&mut other, &out.bytes, "Demo", vec![ElementId(1), ElementId(2)]).unwrap();
        assert!(report[1].contains("1 issue(s) added"), "{report:?}");
        assert_eq!(other.issues.len(), 1);
        let (a, b) = (&back.issues[0], &other.issues[0]);
        assert_eq!((a.guid.as_str(), a.title.as_str(), a.assignee.as_str(), a.due), (b.guid.as_str(), b.title.as_str(), b.assignee.as_str(), b.due));
        assert_eq!(a.comments, b.comments);
        assert_eq!(a.viewpoints.len(), b.viewpoints.len());
        assert_eq!(a.viewpoints[0].snapshot_png, b.viewpoints[0].snapshot_png);
        // Importing again replaces, it does not duplicate.
        let again = bcf_merge(&mut other, &out.bytes, "Demo", vec![]).unwrap();
        assert!(again[1].contains("0 issue(s) added, 1 replaced"), "{again:?}");
        assert!(BCF_LIMITS.contains("3.0") && BCF_LIMITS.contains("not been opened"));
    }

    #[test]
    fn grouping_needs_an_entered_distance_or_a_shared_element() {
        let co = two_boxes();
        // No distance and no shared element: nothing to group by, and the engine offers none.
        let options = GroupOptions { share_element: false, distance_mm: None, ignore_shared: Vec::new() };
        let groups = group_clashes(&co.results.clashes, &options);
        assert!(groups.iter().all(|g| g.clash_ids.len() == 1));
        assert_eq!(parse_due("not a date").unwrap_err().contains("YYYY-MM-DD"), true);
    }
}
