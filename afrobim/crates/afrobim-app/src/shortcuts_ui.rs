// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Data shortcuts in the application: the working project folder, publishing objects of this
//! document, references to objects of other documents (with the dependency closure shown before
//! confirming), state badges, synchronising, promoting, repairing paths, validating, attaching a
//! whole document as an underlay, and the guard that keeps a reference read-only.
//!
//! The engine is `afrobim-shortcuts`. A reference is an ordinary element of its source's own
//! category at the source's own coordinates, so it draws, can be a target (a referenced surface as
//! corridor ground, a referenced alignment for a profile view) and opens without the source. It is
//! marked and locked. The status check on opening reads the manifest only; the full check (which
//! opens the sources) runs on a refresh.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use afrobim_kernel::{project_setting, set_project_setting, Document, Element, ElementId};
use afrobim_shortcuts::*;
use egui::{Color32, RichText};

use crate::app::AfroBimApp;
use crate::ui_kit::{self, Col, Dialog, Level};

pub const FOLDER_KEY: &str = "shortcuts_folder";

#[derive(Default)]
pub struct ShortcutsUi {
    pub root: Option<PathBuf>,
    pub project: Option<ShortcutProject>,
    pub error: String,
    pub statuses: HashMap<ElementId, ReferenceStatus>,
    pub doc_statuses: Vec<DocumentStatus>,
    /// What the status check was last made for (folder and references), so that it runs once.
    pub checked_key: Option<u64>,
    /// Loaded underlay documents: name to (hash, document).
    pub underlays: HashMap<String, (String, Document)>,
    pub win: Option<ShortWin>,
    /// The next commit may change references (a synchronise, a promote): the guard stands aside once.
    pub bypass: bool,
    pub last_sync: Vec<String>,
}

pub struct ShortWin {
    tab: usize,
    publish_sel: HashSet<ElementId>,
    with_deps: bool,
    share: usize,
    note: String,
    ref_key: Option<String>,
    repair_old: String,
    repair_new: Option<PathBuf>,
    layers: String,
    locked: bool,
    lines: Vec<String>,
    confirm_promote: bool,
    pending_folder: Option<PathBuf>,
}

const SHARE: [ShareStatus; 5] = [ShareStatus::WorkInProgress, ShareStatus::Shared, ShareStatus::Published, ShareStatus::Superseded, ShareStatus::Archived];

fn author() -> String {
    std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "unknown".into())
}

/// The badge of a reference state, the words the Prospector shows.
pub fn badge(s: &ReferenceStatus) -> (&'static str, Color32) {
    match &s.state {
        RefState::UpToDate if s.crs_differs => ("coordinate system differs", Color32::from_rgb(0xE0, 0xA0, 0x30)),
        RefState::UpToDate => ("up to date", Color32::from_rgb(0x5F, 0xB0, 0x6A)),
        RefState::OutOfDate { .. } => ("out of date", Color32::from_rgb(0xE0, 0xA0, 0x30)),
        RefState::SourceMissing { .. } => ("source missing", Color32::from_rgb(0xD9, 0x55, 0x4A)),
        RefState::ShortcutRemoved => ("removed", Color32::from_rgb(0xD9, 0x55, 0x4A)),
    }
}

/// The guard: a locked reference that the commit finds changed or erased is put back, and the
/// reason is returned. Every edit path commits, so no command can modify a reference. Called by
/// the commit with the document and the last committed elements.
pub fn guard(doc: &mut Document, committed: &[Element]) -> Vec<String> {
    if !doc.elements().iter().any(is_reference) && !committed.iter().any(is_reference) {
        return Vec::new();
    }
    let mut before = Document::default();
    before.restore(committed.to_vec());
    let mut els = doc.snapshot();
    let mut msgs = Vec::new();
    let mut touched = false;
    for old in committed.iter().filter(|e| is_reference(e)) {
        if check_editable(&before, old.id).is_ok() {
            continue;
        }
        match els.iter().position(|e| e.id == old.id) {
            Some(i) if els[i] != *old => {
                els[i] = old.clone();
                msgs.push(format!("{} is a referenced object (read-only): the change was undone. Promote it to an editable copy first.", old.name));
                touched = true;
            }
            None => {
                els.push(old.clone());
                msgs.push(format!("{} is a referenced object: it cannot be erased here. Remove it from the Shortcuts manager.", old.name));
                touched = true;
            }
            _ => {}
        }
    }
    if touched {
        doc.restore(els);
    }
    msgs
}

fn closure_names(doc: &Document, id: ElementId, table: &DependencyTable) -> Vec<(ElementId, String, String)> {
    local_closure(doc, id, table).into_iter().filter_map(|i| doc.find(i).map(|e| (i, e.name.clone(), e.category.clone()))).collect()
}

impl AfroBimApp {
    /// Opens the manager on a tab.
    pub(crate) fn shortcuts_cmd(&mut self, cv: crate::civil_cmds::Cv) {
        use crate::civil_cmds::Cv::*;
        let tab = match cv {
            ShortcutsManager | ShortcutFolder => 0,
            ShortcutPublish => 1,
            ShortcutReference => 2,
            ShortcutSync => 3,
            ShortcutPromote | ShortcutRepair | ShortcutValidate => 4,
            _ => 5,
        };
        if cv == ShortcutsManager {
            // A refresh: the full check, which opens the sources.
            self.shortcuts_refresh(true);
        }
        let sel = self.doc.selection().iter().copied().collect::<HashSet<_>>();
        self.civil_ui.shortcuts.win = Some(ShortWin { tab, publish_sel: sel, with_deps: true, share: 2, note: String::new(), ref_key: None, repair_old: String::new(), repair_new: None, layers: String::new(), locked: true, lines: vec![], confirm_promote: false, pending_folder: None });
    }

    fn shortcuts_root_setting(&self) -> Option<PathBuf> {
        let s = project_setting(&self.doc, FOLDER_KEY)?.as_str()?.to_string();
        let p = Path::new(&s);
        Some(if p.is_absolute() { p.to_path_buf() } else { self.doc_path().and_then(Path::parent).map(|d| d.join(p)).unwrap_or_else(|| p.to_path_buf()) })
    }

    fn shortcuts_set_folder(&mut self, folder: &Path) {
        let stored = match self.doc_path().and_then(Path::parent).and_then(|d| folder.strip_prefix(d).ok()) {
            Some(rel) if !rel.as_os_str().is_empty() => rel.to_string_lossy().replace('\\', "/"),
            _ => folder.to_string_lossy().replace('\\', "/"),
        };
        set_project_setting(&mut self.doc, FOLDER_KEY, serde_json::Value::String(stored));
        self.schedule_commit_now();
        self.civil_ui.shortcuts.checked_key = None;
    }

    /// Loads the project of the folder and checks the references. `full` opens the sources.
    pub(crate) fn shortcuts_refresh(&mut self, full: bool) {
        let root = self.shortcuts_root_setting();
        let key = self.shortcuts_key();
        let su = &mut self.civil_ui.shortcuts;
        su.error.clear();
        su.statuses.clear();
        su.doc_statuses.clear();
        su.project = None;
        su.root = root.clone();
        let Some(root) = root else { return };
        match ShortcutProject::open(&root) {
            Ok(p) => {
                let list = status(&self.doc, &p, &StatusOptions { check_sources: full });
                su.statuses = list.into_iter().map(|s| (s.element, s)).collect();
                su.doc_statuses = document_status(&self.doc, &root, true);
                su.project = Some(p);
            }
            Err(e) => su.error = format!("The shortcut project in {} cannot be opened: {e}", root.display()),
        }
        su.checked_key = Some(key);
    }

    fn shortcuts_key(&self) -> u64 {
        let refs: Vec<String> = recorded_references(&self.doc).iter().map(|r| format!("{}{}{}", r.key, r.revision, r.hash)).collect();
        crate::cloud::hash_of(&[format!("{:?}", self.shortcuts_root_setting()), refs.join("|"), format!("{}", document_references(&self.doc).len())])
    }

    /// Once a frame: the cheap check (the manifest alone) when the document or its references changed.
    pub(crate) fn shortcuts_tick(&mut self) {
        let key = self.shortcuts_key();
        if self.civil_ui.shortcuts.checked_key != Some(key) {
            let has = !recorded_references(&self.doc).is_empty() || !document_references(&self.doc).is_empty() || self.shortcuts_root_setting().is_some();
            if has {
                self.shortcuts_refresh(false);
            } else {
                self.civil_ui.shortcuts.checked_key = Some(key);
            }
        }
    }

    pub(crate) fn shortcuts_windows(&mut self, ctx: &egui::Context) {
        self.shortcuts_tick();
        self.shortcuts_window(ctx);
    }

    /// The reference marker and the underlay outlines, painted in plan.
    pub(crate) fn shortcuts_paint(&self, painter: &egui::Painter, rect: egui::Rect) {
        let su = &self.civil_ui.shortcuts;
        // Underlay documents: outline geometry at their own coordinates.
        for (name, (_, d)) in &su.underlays {
            let _ = name;
            for e in d.elements() {
                if e.category == afrobim_kernel::PROJECT_SETTINGS_CATEGORY {
                    continue;
                }
                let (a, b) = (self.view.to_screen(afrobim_kernel::Pt::new(e.origin.x, e.origin.y + e.height)), self.view.to_screen(afrobim_kernel::Pt::new(e.origin.x + e.width, e.origin.y)));
                let r = egui::Rect::from_two_pos(egui::pos2(a.x as f32, a.y as f32), egui::pos2(b.x as f32, b.y as f32));
                if !r.intersects(rect) {
                    continue;
                }
                let ink = Color32::from_rgba_unmultiplied(0x6F, 0xA8, 0xDC, 140);
                if e.category == afrobim_alignment::CATEGORY {
                    if let Ok(al) = serde_json::from_value::<afrobim_alignment::Alignment>(e.data.clone()) {
                        let n = (((al.end_station_m() - al.start_station_m) / 5.0).ceil() as usize).clamp(2, 4000);
                        let pts: Vec<egui::Pos2> = (0..=n).filter_map(|i| al.point_at(al.start_station_m + (al.end_station_m() - al.start_station_m) * i as f64 / n as f64)).map(|p| {
                            let s = self.view.to_screen(afrobim_kernel::Pt::new(p.x * 1000.0, p.y * 1000.0));
                            egui::pos2(s.x as f32, s.y as f32)
                        }).collect();
                        painter.add(egui::Shape::line(pts, egui::Stroke::new(1.2_f32, ink)));
                        continue;
                    }
                }
                if r.width() > 2.0 && r.height() > 2.0 {
                    painter.rect_stroke(r, 0.0, egui::Stroke::new(0.8_f32, ink), egui::StrokeKind::Inside);
                }
            }
        }
        // The small reference marker at the corner of every referenced object.
        for e in self.doc.elements() {
            let Some(info) = reference_info(e) else { continue };
            if !self.doc.layer_visible(&e.layer) {
                continue;
            }
            let (a, b) = (self.view.to_screen(afrobim_kernel::Pt::new(e.origin.x, e.origin.y + e.height)), self.view.to_screen(afrobim_kernel::Pt::new(e.origin.x + e.width, e.origin.y)));
            let r = egui::Rect::from_two_pos(egui::pos2(a.x as f32, a.y as f32), egui::pos2(b.x as f32, b.y as f32));
            if !r.intersects(rect) {
                continue;
            }
            let colour = su.statuses.get(&e.id).map(|s| badge(s).1).unwrap_or(Color32::from_rgb(0x6F, 0xA8, 0xDC));
            let at = egui::pos2(r.left().max(rect.left() + 2.0), r.top().max(rect.top() + 2.0));
            painter.text(at, egui::Align2::LEFT_TOP, egui_phosphor::regular::LINK_SIMPLE, crate::theme::icon_font(13.0), colour);
            let _ = info;
        }
    }

    fn shortcuts_load_underlays(&mut self) {
        let Some(root) = self.civil_ui.shortcuts.root.clone() else { return };
        for r in document_references(&self.doc) {
            let have = self.civil_ui.shortcuts.underlays.get(&r.name).map(|(h, _)| h == &r.hash).unwrap_or(false);
            if !have {
                if let Ok(d) = load_underlay(&root, &r) {
                    self.civil_ui.shortcuts.underlays.insert(r.name.clone(), (r.hash.clone(), d));
                }
            }
        }
        let names: Vec<String> = document_references(&self.doc).into_iter().map(|r| r.name).collect();
        self.civil_ui.shortcuts.underlays.retain(|n, _| names.contains(n));
    }

    fn shortcuts_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.civil_ui.shortcuts.win.take() else { return };
        let su_root = self.civil_ui.shortcuts.root.clone();
        let error = self.civil_ui.shortcuts.error.clone();
        let list: Vec<Shortcut> = self.civil_ui.shortcuts.project.as_ref().map(|p| p.list().to_vec()).unwrap_or_default();
        let statuses: Vec<ReferenceStatus> = {
            let mut v: Vec<ReferenceStatus> = self.civil_ui.shortcuts.statuses.values().cloned().collect();
            v.sort_by_key(|s| s.element.0);
            v
        };
        let docs = document_references(&self.doc);
        let doc_status = self.civil_ui.shortcuts.doc_statuses.clone();
        let table = DependencyTable::civil();
        let mine: Vec<(ElementId, String, String)> = self.doc.elements().iter().filter(|e| e.category != afrobim_kernel::PROJECT_SETTINGS_CATEGORY && !is_reference(e) && e.category != afrobim_annotate::SETTINGS_CATEGORY).map(|e| (e.id, e.name.clone(), e.category.clone())).collect();
        let referenced_keys: HashSet<String> = statuses.iter().map(|s| s.key.clone()).collect();
        let mut act: Vec<Act> = Vec::new();
        let out = Dialog::new("Data shortcuts", crate::glyphs::by_name("SHORTCUTS")).width(860.0).resizable(true).max_body_height(660.0).help("A shortcut project is a folder with one manifest file. Publish objects of this document, and reference published objects from other documents. A reference is read-only and keeps the source's own coordinates; it is told when the source is published again. Nothing is moved or transformed, and there is no file locking: two people publishing at once are not coordinated.").show_body(
            ctx,
            |ui| {
                ui_kit::tabs(ui, &mut w.tab, &["Project", "Publish", "Reference", "Synchronise", "Promote, repair, validate", "Documents"]);
                if !error.is_empty() {
                    ui_kit::callout(ui, Level::Error, error.clone());
                }
                match w.tab {
                    0 => {
                        ui_kit::section(ui, "Working project folder", |ui| {
                            ui.label(su_root.as_ref().map(|r| r.display().to_string()).unwrap_or_else(|| "(not set)".into()));
                            ui.horizontal(|ui| {
                                if ui.button("Choose the folder...").clicked() {
                                    act.push(Act::PickFolder);
                                }
                                if ui.button("Refresh the status of the references").on_hover_text("Opens the source documents").clicked() {
                                    act.push(Act::Refresh);
                                }
                            });
                            if let Some(p) = &w.pending_folder {
                                ui_kit::callout(ui, Level::Info, format!("{} holds no shortcut project.", p.display()));
                                if ui.button("Make a shortcut project here").clicked() {
                                    act.push(Act::CreateProject);
                                }
                            }
                        });
                        ui_kit::section(ui, &format!("Published shortcuts ({})", list.len()), |ui| {
                            let rows: Vec<Vec<String>> = list.iter().map(|s| vec![s.key.clone(), s.category.clone(), s.revision.to_string(), s.status.label().to_string(), s.published_by.clone(), s.published_at.clone(), s.note.clone().unwrap_or_default(), s.source_path.clone()]).collect();
                            ui_kit::scroll_x(ui, "sc-list", |ui| ui_kit::results_table(ui, "sc-t", &["Key", "Category", "Rev", "Status", "By", "When", "Note", "Source"], &[Col::Text, Col::Text, Col::Num, Col::Text, Col::Text, Col::Text, Col::Text, Col::Text], &rows));
                        });
                    }
                    1 => {
                        if su_root.is_none() {
                            ui_kit::callout(ui, Level::Warning, "Set the working project folder first (Project tab).");
                        }
                        ui_kit::note(ui, "Tick the objects to publish. The shortcut points at the SAVED file of this document: it is saved when you publish.");
                        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                            for (id, name, cat) in &mine {
                                let mut on = w.publish_sel.contains(id);
                                if ui.checkbox(&mut on, format!("{name}  ({cat})")).changed() {
                                    if on {
                                        w.publish_sel.insert(*id);
                                    } else {
                                        w.publish_sel.remove(id);
                                    }
                                }
                            }
                        });
                        ui_kit::form(ui, "sc-p", |fm| {
                            fm.row("Status label", |ui| {
                                ui_kit::combo(ui, "sc-share", &mut w.share, &SHARE.map(|s| s.label()));
                            });
                            fm.row("Note", |ui| {
                                ui_kit::text_field(ui, &mut w.note, "optional");
                            });
                            fm.row_note("Publish what they depend on", "the objects their fields name, so a reference can be made", |ui| {
                                ui.checkbox(&mut w.with_deps, "");
                            });
                        });
                        if w.with_deps {
                            let mut shown: BTreeMap<u64, (String, String)> = BTreeMap::new();
                            for id in &w.publish_sel {
                                for (i, n, c) in closure_names(&self.doc, *id, &table) {
                                    shown.insert(i.0, (n, c));
                                }
                            }
                            if !shown.is_empty() {
                                ui_kit::callout(ui, Level::Info, format!("These {} object(s) will be published: {}", shown.len(), shown.values().map(|(n, c)| format!("{n} ({c})")).collect::<Vec<_>>().join(", ")));
                            }
                        }
                        if ui.add_enabled(su_root.is_some() && !w.publish_sel.is_empty(), egui::Button::new("Publish")).clicked() {
                            act.push(Act::Publish);
                        }
                    }
                    2 => {
                        if list.is_empty() {
                            ui_kit::note(ui, "No shortcut is published in the working project yet.");
                        }
                        let own = self.doc_path().map(|p| crate::survey_db::stored_path(su_root.as_deref().map(|r| r.join("x")).as_deref(), p));
                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            for s in &list {
                                let already = referenced_keys.contains(&s.key);
                                let label = format!("{}  (revision {}, {}){}", s.key, s.revision, s.status.label(), if already { "  [already referenced]" } else { "" });
                                if ui.selectable_label(w.ref_key.as_deref() == Some(&s.key), label).clicked() {
                                    w.ref_key = Some(s.key.clone());
                                }
                            }
                        });
                        let _ = own;
                        if let (Some(key), Some(p)) = (&w.ref_key, &self.civil_ui.shortcuts.project) {
                            if let Some(s) = p.get(key) {
                                if let Some(file) = p.source_file(key) {
                                    if let Ok(src) = Document::load(&file) {
                                        let names = closure_names(&src, ElementId(s.source_element_id), &table);
                                        ui_kit::section(ui, "Dependency closure: what the reference brings in", |ui| {
                                            for (i, n, c) in &names {
                                                let published = p.list().iter().find(|x| x.source_element_id == i.0 && p.source_file(&x.key) == Some(file.clone())).map(|x| x.key.clone());
                                                ui.label(match published {
                                                    Some(k) => format!("{n} ({c}): published as {k}"),
                                                    None => format!("{n} ({c}): NOT PUBLISHED: the reference will be refused until the source publishes it"),
                                                });
                                            }
                                        });
                                    }
                                }
                                if let Some(crs) = &s.source_crs {
                                    ui_kit::note(ui, format!("The source declares {}. A reference is never moved; a different system is reported, not transformed.", crs.label()));
                                }
                            }
                        }
                        if ui.add_enabled(w.ref_key.is_some(), egui::Button::new("Create the reference")).clicked() {
                            act.push(Act::Reference);
                        }
                    }
                    3 => {
                        if statuses.is_empty() {
                            ui_kit::note(ui, "This document holds no references.");
                        }
                        let rows: Vec<Vec<String>> = statuses.iter().map(|s| {
                            let (b, _) = badge(s);
                            let detail = match &s.state {
                                RefState::OutOfDate { old_revision, new_revision, .. } => format!("revision {old_revision} to {new_revision}"),
                                RefState::SourceMissing { what } => what.clone(),
                                _ => String::new(),
                            };
                            vec![s.name.clone(), s.key.clone(), b.to_string(), detail, if s.unpublished_changes { "the source has unpublished edits".into() } else { String::new() }, s.crs_messages.join(" ")]
                        }).collect();
                        ui_kit::scroll_x(ui, "sc-st", |ui| ui_kit::results_table(ui, "sc-state", &["Object", "Shortcut", "State", "Detail", "Source", "Coordinate system"], &[Col::Text; 6], &rows));
                        ui.horizontal(|ui| {
                            if ui.button("Check the sources now").clicked() {
                                act.push(Act::Refresh);
                            }
                            if ui.add_enabled(!statuses.is_empty(), egui::Button::new("Synchronise all")).clicked() {
                                act.push(Act::Sync(None));
                            }
                        });
                        for s in statuses.iter().filter(|s| matches!(s.state, RefState::OutOfDate { .. })) {
                            if ui.small_button(format!("Synchronise {}", s.name)).clicked() {
                                act.push(Act::Sync(Some(s.key.clone())));
                            }
                        }
                        for l in &self.civil_ui.shortcuts.last_sync {
                            ui.label(RichText::new(l).monospace().size(11.0));
                        }
                    }
                    4 => {
                        ui_kit::section(ui, "Promote a reference to an editable copy", |ui| {
                            ui_kit::note(ui, "The selected referenced object becomes an independent element: it is no longer updated from its source, and it can be edited.");
                            ui.checkbox(&mut w.confirm_promote, "Promote the selected referenced object(s)");
                            if ui.add_enabled(w.confirm_promote, egui::Button::new("Promote")).clicked() {
                                act.push(Act::Promote);
                            }
                            if ui.button("Remove the selected reference(s) from this document").clicked() {
                                act.push(Act::RemoveRef);
                            }
                        });
                        ui_kit::section(ui, "Repair a moved source", |ui| {
                            ui_kit::form(ui, "sc-r", |fm| {
                                fm.row_note("Old path", "as the manifest has it", |ui| {
                                    ui_kit::text_field(ui, &mut w.repair_old, "survey/ground.afrobim");
                                });
                                fm.row("New file", |ui| {
                                    ui.horizontal(|ui| {
                                        if ui.button("Choose...").clicked() {
                                            act.push(Act::PickRepair);
                                        }
                                        ui.label(w.repair_new.as_ref().map(|p| p.display().to_string()).unwrap_or_default());
                                    });
                                });
                            });
                            if ui.add_enabled(w.repair_new.is_some() && !w.repair_old.trim().is_empty(), egui::Button::new("Repair")).clicked() {
                                act.push(Act::Repair);
                            }
                        });
                        ui_kit::section(ui, "Validate the project", |ui| {
                            if ui.button("Validate").clicked() {
                                act.push(Act::Validate);
                            }
                        });
                    }
                    _ => {
                        ui_kit::note(ui, "A whole document can be attached as an underlay: drawn as outlines at its own coordinates, never copied into this document and never modified.");
                        let rows: Vec<Vec<String>> = docs.iter().map(|d| {
                            let st = doc_status.iter().find(|s| s.name == d.name).map(|s| match &s.state {
                                DocState::UpToDate => "up to date".to_string(),
                                DocState::OutOfDate { .. } => "out of date".into(),
                                DocState::SourceMissing { what } => format!("source missing: {what}"),
                            }).unwrap_or_default();
                            vec![d.name.clone(), d.path.clone(), d.element_count.to_string(), if d.layers.is_empty() { "all".into() } else { d.layers.join(", ") }, st]
                        }).collect();
                        ui_kit::results_table(ui, "sc-docs", &["Name", "Path", "Elements", "Layers", "State"], &[Col::Text, Col::Text, Col::Num, Col::Text, Col::Text], &rows);
                        ui_kit::form(ui, "sc-u", |fm| {
                            fm.row_note("Layers to show", "names separated by commas; empty shows all", |ui| {
                                ui_kit::text_field(ui, &mut w.layers, "");
                            });
                            fm.row("Locked", |ui| {
                                ui.checkbox(&mut w.locked, "");
                            });
                        });
                        ui.horizontal(|ui| {
                            if ui.add_enabled(su_root.is_some(), egui::Button::new("Attach a document...")).clicked() {
                                act.push(Act::Attach);
                            }
                            for d in &docs {
                                if ui.small_button(format!("Refresh {}", d.name)).clicked() {
                                    act.push(Act::RefreshDoc(d.name.clone()));
                                }
                                if ui.small_button(format!("Detach {}", d.name)).clicked() {
                                    act.push(Act::Detach(d.name.clone()));
                                }
                            }
                        });
                    }
                }
                for l in &w.lines {
                    ui_kit::note(ui, l.clone());
                }
            },
        );
        if out.closed {
            return;
        }
        for a in act {
            self.shortcuts_act(ctx, &mut w, a);
        }
        self.civil_ui.shortcuts.win = Some(w);
    }

    fn shortcuts_act(&mut self, ctx: &egui::Context, w: &mut ShortWin, a: Act) {
        let at = crate::transform_doc::now_iso();
        let by = author();
        w.lines.clear();
        match a {
            Act::PickFolder => {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    if dir.join(MANIFEST_FILE).exists() {
                        self.shortcuts_set_folder(&dir);
                        self.shortcuts_refresh(false);
                        self.status = format!("Working shortcut project: {}.", dir.display());
                    } else {
                        w.pending_folder = Some(dir);
                    }
                }
            }
            Act::CreateProject => {
                if let Some(dir) = w.pending_folder.take() {
                    let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Project".into());
                    match ShortcutProject::create(&dir, &name) {
                        Ok(_) => {
                            self.shortcuts_set_folder(&dir);
                            self.shortcuts_refresh(false);
                            self.status = format!("Made the shortcut project {name} in {}.", dir.display());
                        }
                        Err(e) => w.lines.push(e.to_string()),
                    }
                }
            }
            Act::Refresh => {
                self.shortcuts_refresh(true);
                self.shortcuts_load_underlays();
                let n = self.civil_ui.shortcuts.statuses.values().filter(|s| !matches!(s.state, RefState::UpToDate)).count();
                w.lines.push(format!("Checked {} reference(s); {n} need attention.", self.civil_ui.shortcuts.statuses.len()));
            }
            Act::Publish => {
                self.run(ctx, crate::commands::Action::Save);
                let Some(path) = self.doc_path().map(Path::to_path_buf) else {
                    w.lines.push("The document has no file yet: save it first.".into());
                    return;
                };
                let Some(project) = self.civil_ui.shortcuts.project.as_mut() else { return };
                let table = DependencyTable::civil();
                let mut ids: Vec<ElementId> = w.publish_sel.iter().copied().collect();
                ids.sort_by_key(|i| i.0);
                for id in ids {
                    let mut req = PublishRequest::new(path.clone(), id, &by, &at);
                    req.status = Some(SHARE[w.share.min(4)]);
                    req.note = (!w.note.trim().is_empty()).then(|| w.note.trim().to_string());
                    let result = if w.with_deps { project.publish_closure(&req, &table) } else { project.publish(&req).map(|o| vec![o]) };
                    match result {
                        Ok(outs) => {
                            for o in outs {
                                w.lines.push(format!("{}: {}, revision {}{}", o.key, match o.action { PublishAction::Created => "created".to_string(), PublishAction::Updated { old_revision } => format!("updated from revision {old_revision}"), PublishAction::Unchanged => "unchanged".into() }, o.revision, o.previous_source.map(|p| format!("; it used to point at {p}")).unwrap_or_default()));
                            }
                        }
                        Err(e) => w.lines.push(format!("Not published: {e}")),
                    }
                }
                self.shortcuts_refresh(false);
            }
            Act::Reference => {
                let (Some(key), Some(project)) = (w.ref_key.clone(), self.civil_ui.shortcuts.project.take()) else { return };
                let opts = ReferenceOptions { by: by.clone(), at: at.clone(), ..ReferenceOptions::default() };
                self.civil_ui.shortcuts.bypass = true;
                match create_reference(&mut self.doc, &project, &key, &opts) {
                    Ok(rep) => {
                        for i in &rep.items {
                            w.lines.push(format!("{}: {}", i.key, if i.created { "referenced" } else { "already referenced" }));
                        }
                        for n in rep.notes.iter().chain(rep.crs_messages.iter()) {
                            w.lines.push(n.clone());
                        }
                        self.doc.select(&[rep.root], afrobim_kernel::SelectMode::Replace);
                        self.status = format!("Created the reference {key} (read-only; it keeps the source's coordinates).");
                        self.schedule_commit_now();
                    }
                    Err(e) => {
                        self.civil_ui.shortcuts.bypass = false;
                        w.lines.push(format!("Not created: {e}"));
                    }
                }
                self.civil_ui.shortcuts.project = Some(project);
                self.shortcuts_refresh(false);
            }
            Act::Sync(key) => {
                let Some(project) = self.civil_ui.shortcuts.project.take() else { return };
                let opts = ReferenceOptions { by, at, ..ReferenceOptions::default() };
                let target = match key {
                    Some(k) => SyncTarget::Keys(vec![k]),
                    None => SyncTarget::All,
                };
                self.civil_ui.shortcuts.bypass = true;
                let rep = synchronise(&mut self.doc, &project, &target, &opts);
                let mut lines = Vec::new();
                for it in &rep.items {
                    lines.push(match &it.outcome {
                        SyncOutcome::AlreadyUpToDate => format!("{}: already up to date", it.key),
                        SyncOutcome::Skipped { reason } => format!("{}: skipped: {reason}", it.key),
                        SyncOutcome::Refreshed { old_revision, new_revision, changes } => {
                            let mut s = format!("{}: refreshed, revision {old_revision} to {new_revision}; {} field(s) changed{}{}", it.key, changes.fields.len(), if changes.placement_changed { "; placement changed" } else { "" }, changes.renamed.as_ref().map(|(a, b)| format!("; renamed {a} to {b}")).unwrap_or_default());
                            for f in changes.fields.iter().take(6) {
                                s.push_str(&format!("\n    {} {:?}: {} to {}", f.path, f.kind, f.before.clone().unwrap_or_default(), f.after.clone().unwrap_or_default()));
                            }
                            if !changes.missing_dependencies.is_empty() {
                                s.push_str(&format!("\n    new dependencies not brought in: {}", changes.missing_dependencies.join(", ")));
                            }
                            s
                        }
                    });
                }
                let changed = rep.changed_elements();
                self.status = format!("Synchronised: {} reference(s) refreshed.", changed.len());
                self.civil_ui.shortcuts.last_sync = lines.iter().flat_map(|l| l.lines().map(str::to_string).collect::<Vec<_>>()).collect();
                if !changed.is_empty() {
                    self.schedule_commit_now();
                } else {
                    self.civil_ui.shortcuts.bypass = false;
                }
                self.civil_ui.shortcuts.project = Some(project);
                self.shortcuts_refresh(false);
            }
            Act::Promote => {
                let ids: Vec<ElementId> = self.doc.selection().to_vec();
                self.civil_ui.shortcuts.bypass = true;
                let mut n = 0;
                for id in ids {
                    match promote(&mut self.doc, id, &by, &at) {
                        Ok(_) => n += 1,
                        Err(e) => w.lines.push(e.to_string()),
                    }
                }
                self.status = format!("Promoted {n} referenced object(s) to editable copies.");
                w.confirm_promote = false;
                self.schedule_commit_now();
                self.shortcuts_refresh(false);
            }
            Act::RemoveRef => {
                let ids: Vec<ElementId> = self.doc.selection().iter().copied().filter(|id| self.doc.find(*id).is_some_and(is_reference)).collect();
                self.civil_ui.shortcuts.bypass = true;
                for id in &ids {
                    self.doc.remove(*id);
                }
                self.status = format!("Removed {} reference(s) from this document. The sources are untouched.", ids.len());
                self.schedule_commit_now();
                self.shortcuts_refresh(false);
            }
            Act::PickRepair => {
                w.repair_new = rfd::FileDialog::new().add_filter("AfroBIM documents", &["afrobim"]).pick_file();
            }
            Act::Repair => {
                if let (Some(new), Some(p)) = (w.repair_new.clone(), self.civil_ui.shortcuts.project.as_mut()) {
                    match p.repair_source_path(w.repair_old.trim(), &new) {
                        Ok(r) => {
                            w.lines.push(format!("Repaired {}: {}", r.repaired.len(), r.repaired.join(", ")));
                            if !r.content_differs.is_empty() {
                                w.lines.push(format!("The content differs from what was published for: {}. Publish again from the new file.", r.content_differs.join(", ")));
                            }
                        }
                        Err(e) => w.lines.push(e.to_string()),
                    }
                }
                self.shortcuts_refresh(false);
            }
            Act::Validate => match self.civil_ui.shortcuts.project.as_ref() {
                Some(p) => {
                    let issues = p.validate();
                    if issues.is_empty() {
                        w.lines.push("The project is consistent: every shortcut's file and element exist and match.".into());
                    }
                    for i in issues {
                        w.lines.push(format!("{:?} {:?} {}: {}", i.level, i.kind, i.key, i.message));
                    }
                }
                None => w.lines.push("Set the working project folder first.".into()),
            },
            Act::Attach => {
                let Some(root) = self.civil_ui.shortcuts.root.clone() else { return };
                if let Some(path) = rfd::FileDialog::new().add_filter("AfroBIM documents", &["afrobim"]).set_directory(&root).pick_file() {
                    let name = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Document".into());
                    let layers: Vec<String> = w.layers.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
                    match attach_document(&mut self.doc, &root, &name, &path, &layers, w.locked, &by, &at) {
                        Ok(r) => {
                            w.lines.push(format!("Attached {} ({} elements). It is drawn at its own coordinates and never changed.", r.name, r.element_count));
                            self.schedule_commit_now();
                        }
                        Err(e) => w.lines.push(e.to_string()),
                    }
                    self.shortcuts_refresh(false);
                    self.shortcuts_load_underlays();
                }
            }
            Act::RefreshDoc(name) => {
                if let Some(root) = self.civil_ui.shortcuts.root.clone() {
                    match refresh_document(&mut self.doc, &root, &name) {
                        Ok(r) => w.lines.push(format!("{name}: {} elements before, {} now{}.", r.elements_before, r.elements_after, if r.hash_changed { "; the content changed" } else { "" })),
                        Err(e) => w.lines.push(e.to_string()),
                    }
                    self.civil_ui.shortcuts.underlays.remove(&name);
                    self.schedule_commit_now();
                    self.shortcuts_load_underlays();
                    self.shortcuts_refresh(false);
                }
            }
            Act::Detach(name) => {
                detach_document(&mut self.doc, &name);
                self.civil_ui.shortcuts.underlays.remove(&name);
                self.schedule_commit_now();
                self.shortcuts_refresh(false);
            }
        }
    }
}

enum Act {
    PickFolder,
    CreateProject,
    Refresh,
    Publish,
    Reference,
    Sync(Option<String>),
    Promote,
    RemoveRef,
    PickRepair,
    Repair,
    Validate,
    Attach,
    RefreshDoc(String),
    Detach(String),
}

/// Whether the elements of a selection may be edited; the message names the first that may not.
pub fn refuse_if_referenced(doc: &Document, ids: &[ElementId]) -> Option<String> {
    let locked = locked_among(doc, ids);
    let first = locked.first()?;
    check_editable(doc, *first).err().map(|e| e.to_string())
}

/// The state words for the Properties and the tree: a reference's source and state.
pub fn reference_line(e: &Element, status: Option<&ReferenceStatus>) -> Option<String> {
    let i = reference_info(e)?;
    Some(format!("Referenced from {} (shortcut {}, revision {}){}", i.source_path, i.shortcut_key, i.source_revision, status.map(|s| format!(": {}", badge(s).0)).unwrap_or_default()))
}
