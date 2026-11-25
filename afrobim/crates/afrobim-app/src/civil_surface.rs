// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Surfaces in the application: create, the definition (an ordered list of operations that
//! rebuilds), the style, spot elevation, flow path and the Volumes dashboard.

use std::sync::Arc;

use afrobim_corridor::Tin;
use afrobim_kernel::{ElementId, SelectMode};
use afrobim_terrain::{Breakline, BreaklineKind, BoundaryKind, Operation, PasteSource, PointRef, StagePoint, Surface, SurfacePoint, SurfaceRole, VolumeResult};
use egui::{RichText, Ui};

use crate::app::AfroBimApp;
use crate::civil::{self, Bounds, PointsData, SurfaceData, SurfaceStyle, PARCEL, POINTS, SURFACE};
use crate::civil_build::{C_BORDER, C_DESIGN};
use crate::civil_dlg::{self as d, fmt};
use crate::civil_scene::{SLine, Scene};
use crate::ui_kit::{self, Col, Dialog, Footer, Level};

const ROLES: [&str; 5] = ["Existing ground", "Design", "Subgrade", "Formation", "Other"];

fn role_of(i: usize, other: &str) -> SurfaceRole {
    match i {
        0 => SurfaceRole::ExistingGround,
        1 => SurfaceRole::Design,
        2 => SurfaceRole::Subgrade,
        3 => SurfaceRole::Formation,
        _ => SurfaceRole::Other(other.to_string()),
    }
}

fn role_index(r: &SurfaceRole) -> usize {
    match r {
        SurfaceRole::ExistingGround => 0,
        SurfaceRole::Design => 1,
        SurfaceRole::Subgrade => 2,
        SurfaceRole::Formation => 3,
        SurfaceRole::Other(_) => 4,
    }
}

#[derive(Default)]
pub struct SurfaceUi {
    pub new: Option<NewForm>,
    pub def: Option<DefState>,
    pub style: Option<StyleForm>,
    pub spot: Option<SpotForm>,
    pub flow: Option<FlowForm>,
    pub volumes: Option<VolForm>,
    pub vol_out: Option<VolOut>,
}

/// What the volume job returns.
pub struct VolOut {
    pub rows: Vec<Result<VolumeResult, String>>,
    pub stage: Option<Result<Vec<StagePoint>, String>>,
}

impl AfroBimApp {
    fn surfaces_all(&self) -> Vec<(ElementId, String)> {
        civil::surface_choices(&self.doc)
    }

    fn selected_surface(&self) -> Option<ElementId> {
        self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == SURFACE || e.category == afrobim_corridor::SURFACE_CATEGORY)).or_else(|| self.surfaces_all().first().map(|s| s.0))
    }

    pub(crate) fn civil_surface_cmd(&mut self, cv: crate::civil_cmds::Cv) {
        use crate::civil_cmds::Cv::*;
        let sel = self.selected_surface();
        match cv {
            SurfaceNew => self.civil_ui.surf.new = Some(NewForm::new()),
            _ if sel.is_none() => self.status = "There is no surface yet. Create one (SURFACENEW), import one (SURFACE) or build one from points (SURVEY).".into(),
            SurfaceDef => {
                let id = sel.unwrap();
                match civil::get::<SurfaceData>(&self.doc, id, SURFACE) {
                    Some(data) => self.civil_ui.surf.def = Some(DefState::new(id, data, self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default())),
                    None => {
                        // An imported TIN: offer to turn it into a definition first.
                        self.convert_imported(id);
                    }
                }
            }
            SurfaceStyle => {
                let id = sel.unwrap();
                let name = self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
                match civil::get::<SurfaceData>(&self.doc, id, SURFACE) {
                    Some(data) => self.civil_ui.surf.style = Some(StyleForm::new(id, name, data.style)),
                    None => self.status = "The style applies to a definition-based surface. Convert this imported surface first: open its definition (SURFACEDEF).".into(),
                }
            }
            SpotElev => self.civil_ui.surf.spot = Some(SpotForm { surface: sel, e: String::new(), n: String::new() }),
            FlowPath => self.civil_ui.surf.flow = Some(FlowForm { surface: sel, e: String::new(), n: String::new(), result: None }),
            Volumes => self.civil_ui.surf.volumes = Some(VolForm::new(sel)),
            _ => {}
        }
    }

    /// Turns an imported TIN surface into a definition-based one holding the same triangulation.
    pub(crate) fn convert_imported(&mut self, id: ElementId) -> Option<ElementId> {
        let tin = self.tin_for(id)?;
        let name = self.doc.find(id)?.name.clone();
        let source = self.doc.find(id).and_then(|e| e.properties.get(civil::PROP_SOURCE).cloned()).unwrap_or_else(|| "Imported surface".into());
        let b = tin.bounds();
        let mut surface = Surface::new(format!("{name} (definition)"), SurfaceRole::ExistingGround);
        surface.definition.push(Operation::Paste { source: PasteSource::Tin((*tin).clone()) });
        surface.source.description = format!("Converted from the imported surface {name}");
        surface.source.reference = source.clone();
        let data = SurfaceData { surface, style: SurfaceStyle { border: true, ..Default::default() } };
        let nid = civil::add(&mut self.doc, SURFACE, &format!("{name} (definition)"), serde_json::to_value(&data).ok()?, Some(Bounds { min_e: b.min_x, min_n: b.min_y, max_e: b.max_x, max_n: b.max_y }), &source);
        self.doc.select(&[nid], SelectMode::Replace);
        self.status = format!("Made the definition-based surface '{name} (definition)' from {name}. The imported surface is kept; hide or delete it in the toolspace.");
        self.schedule_commit_now();
        Some(nid)
    }

    pub(crate) fn civil_surface_windows(&mut self, ctx: &egui::Context) {
        self.new_surface_window(ctx);
        self.def_window(ctx);
        self.style_window(ctx);
        self.spot_window(ctx);
        self.flow_window(ctx);
        self.volumes_window(ctx);
    }
}

// ─── new ───────────────────────────────────────────────────────────────────

pub struct NewForm {
    name: String,
    role: usize,
    other: String,
    source: String,
    start: usize,
    set: usize,
    group: usize,
    tin: usize,
}

impl NewForm {
    fn new() -> NewForm {
        NewForm { name: String::new(), role: 0, other: String::new(), source: String::new(), start: 0, set: 0, group: 0, tin: 0 }
    }
}

impl AfroBimApp {
    fn new_surface_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.surf.new.take() else { return };
        let sets = self.point_sets();
        let imported: Vec<(ElementId, String)> = civil::of_category(&self.doc, afrobim_corridor::SURFACE_CATEGORY);
        let groups: Vec<String> = sets.get(f.set).and_then(|s| self.points_data(s.0)).map(|d| d.groups.groups().iter().map(|g| g.name.clone()).collect()).unwrap_or_default();
        let ok = !f.name.trim().is_empty() && (f.role != 4 || !f.other.trim().is_empty()) && match f.start {
            1 => !sets.is_empty() && !groups.is_empty(),
            2 => !imported.is_empty(),
            _ => true,
        };
        let out = Dialog::new("Create surface", crate::glyphs::by_name("SURFACENEW")).width(520.0).help("A surface keeps its definition, an ordered list of operations, and rebuilds from it. Start empty and add operations in SURFACEDEF, or start from a point group or from an imported surface.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "ns", |fm| {
                    fm.row("Name", |ui| d::text(ui, &mut f.name, "e.g. Existing ground"));
                    fm.row("Role", |ui| {
                        ui_kit::combo(ui, "ns-role", &mut f.role, &ROLES);
                    });
                    if f.role == 4 {
                        fm.row("Role, in words", |ui| d::text(ui, &mut f.other, ""));
                    }
                    fm.row_note("Source", "Where the data came from: survey, drawing, date", |ui| d::text(ui, &mut f.source, ""));
                    fm.row("Start from", |ui| {
                        ui_kit::radio_group(ui, &mut f.start, &["An empty definition", "A point group", "An imported surface (its triangulation is kept)"]);
                    });
                    if f.start == 1 {
                        fm.row("Point set", |ui| {
                            let names: Vec<String> = sets.iter().map(|s| s.1.clone()).collect();
                            ui_kit::combo_owned(ui, "ns-set", &mut f.set, &names);
                        });
                        fm.row("Group", |ui| {
                            ui_kit::combo_owned(ui, "ns-grp", &mut f.group, &groups);
                        });
                    }
                    if f.start == 2 {
                        fm.row("Imported surface", |ui| {
                            let names: Vec<String> = imported.iter().map(|s| s.1.clone()).collect();
                            ui_kit::combo_owned(ui, "ns-tin", &mut f.tin, &names);
                        });
                    }
                });
            },
            |ui| Footer::new("Create surface").icon(crate::glyphs::by_name("SURFACENEW")).enabled(ok, "Name the surface and complete the choices").show(ui),
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            let mut surface = Surface::new(f.name.trim(), role_of(f.role, f.other.trim()));
            surface.source.description = f.source.trim().to_string();
            let mut bounds: Option<Bounds> = None;
            match f.start {
                1 => {
                    if let Some(mut data) = sets.get(f.set).and_then(|s| self.points_data(s.0)) {
                        let gname = groups.get(f.group).cloned().unwrap_or_default();
                        let members = data.groups.members_of(&data.store, &gname).unwrap_or_default();
                        let pts: Vec<SurfacePoint> = members.iter().filter_map(|n| data.store.get(*n)).filter_map(|p| p.elevation.map(|z| SurfacePoint::new(p.number.to_string(), p.easting, p.northing, z))).collect();
                        bounds = Bounds::of(pts.iter().map(|p| (p.x, p.y)));
                        surface.definition.push(Operation::AddPoints { group: gname, points: pts });
                    }
                }
                2 => {
                    if let Some(tin) = imported.get(f.tin).and_then(|s| self.tin_for(s.0)) {
                        let b = tin.bounds();
                        bounds = Some(Bounds { min_e: b.min_x, min_n: b.min_y, max_e: b.max_x, max_n: b.max_y });
                        surface.definition.push(Operation::Paste { source: PasteSource::Tin((*tin).clone()) });
                    }
                }
                _ => {}
            }
            let data = SurfaceData { surface, style: SurfaceStyle { border: true, ..Default::default() } };
            let id = civil::add(&mut self.doc, SURFACE, f.name.trim(), serde_json::to_value(&data).unwrap_or_default(), bounds, if f.source.trim().is_empty() { "Created in AfroBIM" } else { f.source.trim() });
            self.doc.select(&[id], SelectMode::Replace);
            self.status = format!("Created the surface '{}'. Open SURFACEDEF to add operations.", f.name.trim());
            self.schedule_commit_now();
            return;
        }
        self.civil_ui.surf.new = Some(f);
    }
}

// ─── definition ────────────────────────────────────────────────────────────

pub struct DefState {
    id: ElementId,
    name: String,
    surface: Surface,
    selected: Option<usize>,
    add_tab: usize,
    set: usize,
    group: usize,
    line_name: String,
    line_text: String,
    line_kind: usize,
    align: usize,
    align_step: String,
    bnd_name: String,
    bnd_kind: usize,
    bnd_text: String,
    parcel: usize,
    edit_kind: usize,
    a: String,
    b: String,
    c: String,
    message: String,
}

impl DefState {
    fn new(id: ElementId, data: SurfaceData, name: String) -> DefState {
        DefState { id, name, surface: data.surface, selected: None, add_tab: 0, set: 0, group: 0, line_name: String::new(), line_text: String::new(), line_kind: 0, align: 0, align_step: String::new(), bnd_name: String::new(), bnd_kind: 0, bnd_text: String::new(), parcel: 0, edit_kind: 0, a: String::new(), b: String::new(), c: String::new(), message: String::new() }
    }
}

pub fn op_summary(op: &Operation) -> String {
    match op {
        Operation::AddPoints { group, points } => format!("Add points: {group} ({} points)", points.len()),
        Operation::AddBreaklines { lines } => format!("Add breaklines: {} line(s)", lines.len()),
        Operation::AddBoundary { name, kind, vertices } => format!("Add boundary: {name}, {} ({} vertices)", match kind { BoundaryKind::Outer => "outer", BoundaryKind::Hide => "hide", BoundaryKind::Show => "show" }, vertices.len()),
        Operation::AddContourData { name, lines, .. } => format!("Add contour data: {name} ({} lines)", lines.len()),
        Operation::MaxTriangleEdge { length } => format!("Longest triangle edge {length} m"),
        Operation::ExcludeElevations { below, above } => format!("Exclude elevations below {} above {}", below.map(|v| v.to_string()).unwrap_or("-".into()), above.map(|v| v.to_string()).unwrap_or("-".into())),
        Operation::DeleteTriangle { x, y } => format!("Delete the triangle at {x:.3}, {y:.3}"),
        Operation::SwapEdge { a, b, .. } => format!("Swap the edge {:.2},{:.2} to {:.2},{:.2}", a[0], a[1], b[0], b[1]),
        Operation::AddPoint { x, y, z } => format!("Add the point {x:.3}, {y:.3}, {z:.3}"),
        Operation::DeletePoint { target } => match target {
            PointRef::Id(i) => format!("Delete the point {i}"),
            PointRef::Near { x, y, tolerance } => format!("Delete the point near {x:.3}, {y:.3} (within {tolerance} m)"),
        },
        Operation::RaiseLower { delta } => format!("Raise or lower by {delta} m"),
        Operation::Paste { .. } => "Paste a surface (its triangulation)".into(),
    }
}

/// Reads "E N Z" lines (or with commas) into vertices. Returns the line number of a bad line.
fn parse_vertices(text: &str, with_z: bool) -> Result<Vec<[f64; 3]>, String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let nums: Vec<f64> = t.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
        if nums.len() < if with_z { 3 } else { 2 } {
            return Err(format!("Line {}: '{t}' needs {} numbers.", i + 1, if with_z { "an easting, a northing and an elevation" } else { "an easting and a northing" }));
        }
        out.push([nums[0], nums[1], if with_z { nums[2] } else { 0.0 }]);
    }
    Ok(out)
}

impl AfroBimApp {
    fn def_window(&mut self, ctx: &egui::Context) {
        let Some(mut s) = self.civil_ui.surf.def.take() else { return };
        let sets = self.point_sets();
        let aligns = crate::alignment_glue::alignments(&self.doc);
        let parcels = civil::of_category(&self.doc, PARCEL);
        let groups: Vec<String> = sets.get(s.set).and_then(|x| self.points_data(x.0)).map(|d| d.groups.groups().iter().map(|g| g.name.clone()).collect()).unwrap_or_default();
        let report: Vec<String> = self.civil.built.get(&s.id).map(|(_, b)| b.report.clone()).unwrap_or_default();
        let building = self.bg.civil.is_some();
        let mut new_ops: Vec<Operation> = Vec::new();
        let mut message = String::new();
        let mut apply = false;
        let title = format!("Surface definition: {}", s.name);
        let out = Dialog::new("Surface definition", crate::glyphs::by_name("SURFACEDEF")).id_title(title).subtitle(s.name.clone()).width(700.0).resizable(true).max_body_height(560.0).help("The operations run in this order. Data operations (points, breaklines, boundaries, edits of the data) act on the data gathered up to their place in the list; operations on the triangulation (longest edge, boundaries, deleting triangles, swapping edges) act on the triangulation of all the data. Apply rebuilds the surface and the report below says what happened.").show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label("Role");
                    let mut r = role_index(&s.surface.role);
                    if ui_kit::combo(ui, "def-role", &mut r, &ROLES) {
                        s.surface.role = role_of(r, "");
                    }
                });
                ui_kit::section(ui, "Operations", |ui| {
                    if s.surface.definition.operations.is_empty() {
                        ui.label(RichText::new("No operations yet. Add points below.").weak());
                    }
                    let n = s.surface.definition.operations.len();
                    let mut action: Option<(usize, i32)> = None;
                    for (i, op) in s.surface.definition.operations.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let sel = s.selected == Some(i);
                            if ui.selectable_label(sel, format!("{}.  {}", i + 1, op_summary(op))).clicked() {
                                s.selected = Some(i);
                            }
                            if ui.add_enabled_ui(i > 0, |ui| crate::ui_kit::icon_button(ui, crate::glyphs::ui::ARROW_UP, "Move up")).inner.clicked() {
                                action = Some((i, -1));
                            }
                            if ui.add_enabled_ui(i + 1 < n, |ui| crate::ui_kit::icon_button(ui, crate::glyphs::ui::ARROW_DOWN, "Move down")).inner.clicked() {
                                action = Some((i, 1));
                            }
                            if crate::ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove").clicked() {
                                action = Some((i, 0));
                            }
                        });
                    }
                    if let Some((i, dir)) = action {
                        let ops = &mut s.surface.definition.operations;
                        match dir {
                            0 => {
                                ops.remove(i);
                            }
                            -1 => ops.swap(i, i - 1),
                            _ => ops.swap(i, i + 1),
                        }
                    }
                });
                ui_kit::section(ui, "Add an operation", |ui| {
                    ui_kit::tabs(ui, &mut s.add_tab, &["Points", "Breaklines", "Boundary", "Edits"]);
                    match s.add_tab {
                        0 => {
                            if sets.is_empty() {
                                d::needs(ui, "There are no points yet. Import a point file with POINTS.");
                            } else {
                                ui_kit::form(ui, "def-pts", |fm| {
                                    fm.row("Point set", |ui| {
                                        let names: Vec<String> = sets.iter().map(|x| x.1.clone()).collect();
                                        ui_kit::combo_owned(ui, "def-set", &mut s.set, &names);
                                    });
                                    fm.row("Group", |ui| {
                                        ui_kit::combo_owned(ui, "def-grp", &mut s.group, &groups);
                                    });
                                });
                                if ui.button("Add the group's points").clicked() {
                                    if let Some(mut data) = sets.get(s.set).and_then(|x| self.points_data(x.0)) {
                                        let g = groups.get(s.group).cloned().unwrap_or_default();
                                        let members = data.groups.members_of(&data.store, &g).unwrap_or_default();
                                        let pts: Vec<SurfacePoint> = members.iter().filter_map(|n| data.store.get(*n)).filter_map(|p| p.elevation.map(|z| SurfacePoint::new(p.number.to_string(), p.easting, p.northing, z))).collect();
                                        message = format!("Added {} point(s) of the group {g} (points without an elevation are left out).", pts.len());
                                        new_ops.push(Operation::AddPoints { group: g, points: pts });
                                    }
                                }
                            }
                        }
                        1 => {
                            ui_kit::form(ui, "def-bl", |fm| {
                                fm.row("Name", |ui| d::text(ui, &mut s.line_name, "e.g. Edge of road"));
                                fm.row("Kind", |ui| {
                                    ui_kit::segmented(ui, &mut s.line_kind, &["Standard", "Proximity"]);
                                });
                                fm.row_note("Vertices", "One per line: easting northing elevation", |ui| {
                                    ui.add(egui::TextEdit::multiline(&mut s.line_text).desired_rows(4).desired_width(300.0));
                                });
                            });
                            if ui.button("Add the breakline").clicked() {
                                match parse_vertices(&s.line_text, true) {
                                    Ok(v) if v.len() >= 2 => {
                                        new_ops.push(Operation::AddBreaklines { lines: vec![Breakline { name: if s.line_name.trim().is_empty() { "Breakline".into() } else { s.line_name.trim().into() }, kind: if s.line_kind == 0 { BreaklineKind::Standard } else { BreaklineKind::Proximity }, vertices: v }] });
                                        s.line_text.clear();
                                    }
                                    Ok(_) => message = "A breakline needs at least two vertices.".into(),
                                    Err(e) => message = e,
                                }
                            }
                            if !aligns.is_empty() {
                                ui.separator();
                                ui.label("Or along an alignment, at the levels of its design profile");
                                ui.horizontal(|ui| {
                                    let names: Vec<String> = aligns.iter().map(|a| a.1.clone()).collect();
                                    ui_kit::combo_owned(ui, "def-al", &mut s.align, &names);
                                    d::num_opt(ui, &mut s.align_step, "m spacing").ok();
                                    if ui.button("Add").clicked() {
                                        if let (Some(a), Ok(step)) = (aligns.get(s.align).and_then(|a| crate::alignment_glue::alignment_of(&self.doc, a.0)), s.align_step.trim().parse::<f64>()) {
                                            if step > 0.0 && a.profile.is_some() {
                                                let (s0, s1) = (a.start_station_m, a.end_station_m());
                                                let n = (((s1 - s0) / step).ceil() as usize).clamp(1, 20000);
                                                let v: Vec<[f64; 3]> = (0..=n).filter_map(|i| {
                                                    let st = (s0 + step * i as f64).min(s1);
                                                    let p = a.point_at(st)?;
                                                    let z = a.profile.as_ref()?.elevation_at(st).ok()?;
                                                    Some([p.x, p.y, z])
                                                }).collect();
                                                if v.len() >= 2 {
                                                    new_ops.push(Operation::AddBreaklines { lines: vec![Breakline { name: format!("{} centreline", a.name), kind: BreaklineKind::Standard, vertices: v }] });
                                                }
                                            } else {
                                                message = "Enter a spacing greater than 0 for an alignment that has a design profile.".into();
                                            }
                                        }
                                    }
                                });
                            }
                        }
                        2 => {
                            ui_kit::form(ui, "def-bd", |fm| {
                                fm.row("Name", |ui| d::text(ui, &mut s.bnd_name, ""));
                                fm.row("Kind", |ui| {
                                    ui_kit::segmented(ui, &mut s.bnd_kind, &["Outer (keep inside)", "Hide inside", "Show inside"]);
                                });
                                fm.row_note("Vertices", "One per line: easting northing", |ui| {
                                    ui.add(egui::TextEdit::multiline(&mut s.bnd_text).desired_rows(3).desired_width(300.0));
                                });
                            });
                            ui.horizontal(|ui| {
                                if ui.button("Add the boundary").clicked() {
                                    match parse_vertices(&s.bnd_text, false) {
                                        Ok(v) if v.len() >= 3 => new_ops.push(Operation::AddBoundary { name: if s.bnd_name.trim().is_empty() { "Boundary".into() } else { s.bnd_name.trim().into() }, kind: [BoundaryKind::Outer, BoundaryKind::Hide, BoundaryKind::Show][s.bnd_kind.min(2)], vertices: v.iter().map(|p| [p[0], p[1]]).collect() }),
                                        Ok(_) => message = "A boundary needs at least three vertices.".into(),
                                        Err(e) => message = e,
                                    }
                                }
                                if !parcels.is_empty() {
                                    let names: Vec<String> = parcels.iter().map(|p| p.1.clone()).collect();
                                    ui_kit::combo_owned(ui, "def-parcel", &mut s.parcel, &names);
                                    if ui.button("Use the parcel's outline").clicked() {
                                        if let Some(p) = parcels.get(s.parcel).and_then(|p| civil::get::<afrobim_cogo::Parcel>(&self.doc, p.0, PARCEL)) {
                                            if let Ok(ring) = p.outline(crate::civil_build::DISPLAY_FLATNESS_M) {
                                                new_ops.push(Operation::AddBoundary { name: p.name.clone(), kind: [BoundaryKind::Outer, BoundaryKind::Hide, BoundaryKind::Show][s.bnd_kind.min(2)], vertices: ring.iter().map(|q| [q.e, q.n]).collect() });
                                            }
                                        }
                                    }
                                }
                            });
                        }
                        _ => {
                            ui_kit::combo(ui, "def-edit", &mut s.edit_kind, &["Longest triangle edge", "Exclude elevations", "Raise or lower", "Add a point", "Delete the point nearest", "Delete a triangle"]);
                            let labels: [(&str, &str, &str); 6] = [("Length (m)", "", ""), ("Below (m)", "Above (m)", ""), ("Change (m)", "", ""), ("Easting", "Northing", "Elevation"), ("Easting", "Northing", "Within (m)"), ("Easting", "Northing", "")];
                            let l = labels[s.edit_kind.min(5)];
                            ui.horizontal(|ui| {
                                if !l.0.is_empty() {
                                    ui.label(l.0);
                                    d::num_opt(ui, &mut s.a, "").ok();
                                }
                                if !l.1.is_empty() {
                                    ui.label(l.1);
                                    d::num_opt(ui, &mut s.b, "").ok();
                                }
                                if !l.2.is_empty() {
                                    ui.label(l.2);
                                    d::num_opt(ui, &mut s.c, "").ok();
                                }
                            });
                            if ui.button("Add the edit").clicked() {
                                let v = |t: &String| t.trim().parse::<f64>().ok();
                                let op = match s.edit_kind {
                                    0 => v(&s.a).filter(|x| *x > 0.0).map(|length| Operation::MaxTriangleEdge { length }),
                                    1 => (v(&s.a).is_some() || v(&s.b).is_some()).then(|| Operation::ExcludeElevations { below: v(&s.a), above: v(&s.b) }),
                                    2 => v(&s.a).map(|delta| Operation::RaiseLower { delta }),
                                    3 => v(&s.a).zip(v(&s.b)).zip(v(&s.c)).map(|((x, y), z)| Operation::AddPoint { x, y, z }),
                                    4 => v(&s.a).zip(v(&s.b)).zip(v(&s.c)).map(|((x, y), tolerance)| Operation::DeletePoint { target: PointRef::Near { x, y, tolerance } }),
                                    _ => v(&s.a).zip(v(&s.b)).map(|(x, y)| Operation::DeleteTriangle { x, y }),
                                };
                                match op {
                                    Some(op) => new_ops.push(op),
                                    None => message = "Fill in the numbers for this edit.".into(),
                                }
                            }
                        }
                    }
                    if !s.message.is_empty() {
                        ui.label(RichText::new(&s.message).small().color(crate::theme::warn()));
                    }
                });
                ui_kit::section(ui, "Build report", |ui| {
                    if building {
                        ui.label(RichText::new("Building...").weak());
                    }
                    if report.is_empty() && !building {
                        ui.label(RichText::new("Apply to build the surface and see the report.").weak());
                    }
                    for line in &report {
                        ui.label(RichText::new(line).small());
                    }
                });
                apply = false;
            },
            |ui| Footer::new("Apply and rebuild").icon(crate::glyphs::by_name("SURFACEDEF")).secondary(&["Rebuild"]).cancel(Some("Close")).show(ui),
        );
        let _ = apply;
        if !message.is_empty() {
            s.message = message;
        }
        for op in new_ops {
            s.surface.definition.push(op);
            s.message.clear();
        }
        let fo = out.inner.as_ref().map(|(_, f)| *f);
        if fo.is_some_and(|f| f.primary || f.secondary == Some(0)) {
            if let Some(mut data) = civil::get::<SurfaceData>(&self.doc, s.id, SURFACE) {
                data.surface = s.surface.clone();
                let b = s.surface.definition.operations.iter().find_map(|o| match o {
                    Operation::AddPoints { points, .. } => Bounds::of(points.iter().map(|p| (p.x, p.y))),
                    Operation::Paste { source: PasteSource::Tin(t) } => {
                        let b = t.bounds();
                        Some(Bounds { min_e: b.min_x, min_n: b.min_y, max_e: b.max_x, max_n: b.max_y })
                    }
                    _ => None,
                });
                civil::set(&mut self.doc, s.id, serde_json::to_value(&data).unwrap_or_default(), b);
                self.civil.held = None;
                self.schedule_commit_now();
                self.status = format!("Rebuilding {}...", s.name);
            }
            if fo.is_some_and(|f| f.primary) {
                return;
            }
        }
        if out.closed || fo.is_some_and(|f| f.cancel) {
            return;
        }
        self.civil_ui.surf.def = Some(s);
    }
}

// ─── style ─────────────────────────────────────────────────────────────────

pub struct StyleForm {
    id: ElementId,
    name: String,
    triangles: bool,
    border: bool,
    minor: String,
    major: String,
    band: String,
    slope: String,
    arrow_len: String,
    arrow_min: String,
}

impl StyleForm {
    fn new(id: ElementId, name: String, s: SurfaceStyle) -> StyleForm {
        let t = |v: Option<f64>| v.map(|x| x.to_string()).unwrap_or_default();
        StyleForm { id, name, triangles: s.triangles, border: s.border, minor: t(s.minor_interval), major: t(s.major_interval), band: t(s.elevation_band_step), slope: t(s.slope_band_step_percent), arrow_len: t(s.arrow_length), arrow_min: t(s.arrow_min_slope_percent) }
    }
}

impl AfroBimApp {
    fn style_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.surf.style.take() else { return };
        let positive = ui_kit::NumRule::default().above(0.0);
        let mut ok = std::cell::Cell::new(true);
        let _ = &mut ok;
        let out = Dialog::new("Surface style", crate::glyphs::by_name("SURFACESTYLE")).id_title(format!("Surface style: {}", f.name)).subtitle(f.name.clone()).width(520.0).help("Every interval is yours to enter: AfroBIM does not assume a contour interval. Leave a field blank to leave that display off. The Example values button fills demonstration numbers that are not from any standard.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "ss", |fm| {
                    fm.row("Triangles", |ui| {
                        ui.checkbox(&mut f.triangles, "Draw the triangle edges");
                    });
                    fm.row("Border", |ui| {
                        ui.checkbox(&mut f.border, "Draw the outer edge");
                    });
                    fm.row_note("Minor contour interval", "Blank draws no contours", |ui| ok.set(ok.get() & !d::num_opt_rule(ui, &mut f.minor, "m", &positive).is_err()));
                    fm.row_note("Major contour interval", "Labelled, drawn heavier; blank for none", |ui| ok.set(ok.get() & !d::num_opt_rule(ui, &mut f.major, "m", &positive).is_err()));
                    fm.row_note("Elevation bands, step", "Blank for no bands", |ui| ok.set(ok.get() & !d::num_opt_rule(ui, &mut f.band, "m", &positive).is_err()));
                    fm.row_note("Slope bands, step", "Percent; blank for no bands", |ui| ok.set(ok.get() & !d::num_opt_rule(ui, &mut f.slope, "%", &positive).is_err()));
                    fm.row_note("Slope arrows, length", "Both arrow fields are needed for arrows", |ui| ok.set(ok.get() & !d::num_opt_rule(ui, &mut f.arrow_len, "m", &positive).is_err()));
                    fm.row("Slope arrows, from slope", |ui| ok.set(ok.get() & !d::num_opt(ui, &mut f.arrow_min, "%").is_err()));
                });
                ui.horizontal(|ui| {
                    if ui.button("Example values").on_hover_text("Demonstration numbers, not from any standard").clicked() {
                        let e = SurfaceStyle::example();
                        f.minor = e.minor_interval.map(|v| v.to_string()).unwrap_or_default();
                        f.major = e.major_interval.map(|v| v.to_string()).unwrap_or_default();
                        f.border = true;
                    }
                    crate::ui_kit::note(ui, "Example values are for demonstration only.");
                });
            },
            |ui| Footer::new("Apply style").icon(crate::glyphs::by_name("SURFACESTYLE")).enabled(ok.get(), "Correct the intervals").show(ui),
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            let p = |t: &String| t.trim().parse::<f64>().ok();
            if let Some(mut data) = civil::get::<SurfaceData>(&self.doc, f.id, SURFACE) {
                data.style = SurfaceStyle { triangles: f.triangles, border: f.border, minor_interval: p(&f.minor), major_interval: p(&f.major), elevation_band_step: p(&f.band), slope_band_step_percent: p(&f.slope), arrow_length: p(&f.arrow_len), arrow_min_slope_percent: p(&f.arrow_min) };
                civil::set(&mut self.doc, f.id, serde_json::to_value(&data).unwrap_or_default(), None);
                self.schedule_commit_now();
                self.status = format!("Applied the style to {}.", f.name);
            }
            return;
        }
        self.civil_ui.surf.style = Some(f);
    }
}

// ─── spot elevation and flow path ──────────────────────────────────────────

pub struct SpotForm {
    surface: Option<ElementId>,
    e: String,
    n: String,
}

impl SpotForm {
    pub fn set(&mut self, e: f64, n: f64) {
        self.e = format!("{e:.3}");
        self.n = format!("{n:.3}");
    }
}

impl FlowForm {
    pub fn set(&mut self, e: f64, n: f64) {
        self.e = format!("{e:.3}");
        self.n = format!("{n:.3}");
    }
}

pub struct FlowForm {
    surface: Option<ElementId>,
    e: String,
    n: String,
    result: Option<afrobim_terrain::FlowPath>,
}

impl AfroBimApp {
    fn spot_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.surf.spot.take() else { return };
        let surfaces = self.surfaces_all();
        let tin = f.surface.and_then(|s| self.tin_for(s));
        let mut pick = false;
        let out = Dialog::new("Spot elevation", crate::glyphs::by_name("SPOT")).width(440.0).show(
            ctx,
            |ui| {
                ui_kit::form(ui, "sp", |fm| {
                    fm.row("Surface", |ui| {
                        d::pick_from(ui, "sp-s", &surfaces, &mut f.surface);
                    });
                    fm.row("Easting", |ui| {
                        d::num(ui, &mut f.e, "m");
                    });
                    fm.row("Northing", |ui| {
                        d::num(ui, &mut f.n, "m");
                    });
                    fm.row("", |ui| {
                        if ui.button(format!("{}  Pick in the model", egui_phosphor::regular::CROSSHAIR)).clicked() {
                            pick = true;
                        }
                    });
                });
                if let (Some(t), Ok(e), Ok(n)) = (&tin, f.e.trim().parse::<f64>(), f.n.trim().parse::<f64>()) {
                    match afrobim_terrain::spot_elevation(t, e, n) {
                        Some(z) => {
                            ui.label(RichText::new(format!("Elevation {z:.3} m")).strong());
                        }
                        None => {
                            ui.label(RichText::new("That point is outside the surface.").color(crate::theme::warn()));
                        }
                    }
                } else if tin.is_none() {
                    d::needs(ui, "The surface is still building or has no triangulation.");
                }
            },
            |ui| Footer::new("Close").cancel(None).show(ui),
        );
        if pick {
            self.civil_ui.pick = Some(crate::civil_ui::PickFor::Spot);
            self.status = "Click a point in the model.".into();
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            return;
        }
        self.civil_ui.surf.spot = Some(f);
    }

    fn flow_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.surf.flow.take() else { return };
        let surfaces = self.surfaces_all();
        let tin = f.surface.and_then(|s| self.tin_for(s));
        let mut pick = false;
        let mut trace = false;
        let mut draw = false;
        let out = Dialog::new("Flow path", crate::glyphs::by_name("FLOWPATH")).width(480.0).help("The path of steepest descent over the triangles from the start point. It stops at a pit, on flat ground or where it leaves the surface, and says which.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "fp", |fm| {
                    fm.row("Surface", |ui| {
                        d::pick_from(ui, "fp-s", &surfaces, &mut f.surface);
                    });
                    fm.row("Easting", |ui| {
                        d::num(ui, &mut f.e, "m");
                    });
                    fm.row("Northing", |ui| {
                        d::num(ui, &mut f.n, "m");
                    });
                });
                ui.horizontal(|ui| {
                    if ui.button(format!("{}  Pick in the model", egui_phosphor::regular::CROSSHAIR)).clicked() {
                        pick = true;
                    }
                    if ui.button("Trace the path").clicked() {
                        trace = true;
                    }
                    if ui.add_enabled(f.result.is_some(), egui::Button::new("Draw it in the model")).clicked() {
                        draw = true;
                    }
                });
                if let Some(r) = &f.result {
                    ui_kit::results_table(ui, "fp-r", &["Quantity", "Value"], &[Col::Text, Col::Num], &[vec!["Ends".into(), format!("{:?}", r.stop)], vec!["Plan length (m)".into(), fmt(r.plan_length, 3)], vec!["Length on the surface (m)".into(), fmt(r.length_3d, 3)], vec!["Vertices".into(), r.points.len().to_string()]]);
                }
                if tin.is_none() {
                    d::needs(ui, "The surface is still building or has no triangulation.");
                }
            },
            |ui| Footer::new("Close").cancel(None).show(ui),
        );
        if pick {
            self.civil_ui.pick = Some(crate::civil_ui::PickFor::Flow);
            self.status = "Click the start point in the model.".into();
        }
        if trace {
            if let (Some(t), Ok(e), Ok(n)) = (&tin, f.e.trim().parse::<f64>(), f.n.trim().parse::<f64>()) {
                f.result = Some(afrobim_terrain::flow_path(t, e, n));
            }
        }
        if draw {
            if let Some(r) = &f.result {
                let pts: Vec<[f64; 2]> = r.points.iter().map(|p| [p[0] * 1000.0, p[1] * 1000.0]).collect();
                self.civil_ui.overlay.lines.push(SLine::new(pts, [0x3B, 0x8E, 0xE0], 2.4));
                self.status = "The flow path is drawn in the model (it is a temporary overlay and is not saved).".into();
            }
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            return;
        }
        self.civil_ui.surf.flow = Some(f);
    }
}

// ─── volumes ───────────────────────────────────────────────────────────────

pub struct VolRow {
    base: Option<ElementId>,
    comp: Option<ElementId>,
    /// 0 surface, 1 level.
    mode: usize,
    level: String,
}

pub struct VolForm {
    rows: Vec<VolRow>,
    region: usize,
    stage_surface: Option<ElementId>,
    stage_from: String,
    stage_step: String,
    stage_count: String,
    message: String,
}

impl VolForm {
    fn new(sel: Option<ElementId>) -> VolForm {
        VolForm { rows: vec![VolRow { base: sel, comp: None, mode: 0, level: String::new() }], region: 0, stage_surface: sel, stage_from: String::new(), stage_step: String::new(), stage_count: String::new(), message: String::new() }
    }
}

impl AfroBimApp {
    fn volumes_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.surf.volumes.take() else { return };
        let surfaces = self.surfaces_all();
        let parcels = civil::of_category(&self.doc, PARCEL);
        let busy = self.bg.volumes.is_some();
        let out_rows = self.civil_ui.surf.vol_out.as_ref().map(|o| o.rows.len()).unwrap_or(0);
        let mut run = false;
        let mut csv = false;
        let res = self.civil_ui.surf.vol_out.as_ref();
        let out = Dialog::new("Volumes", crate::glyphs::by_name("VOLUMES")).width(760.0).resizable(true).max_body_height(560.0).help("Cut is where the base surface is above the comparison surface, fill where it is below. Net is cut minus fill. Volumes are exact for the triangulated surfaces. A polygon limits the volume to the inside of a parcel outline. Stage-storage gives the volume held below each level of a surface; keep the levels below the spill level.").show(
            ctx,
            |ui| {
                ui_kit::section(ui, "Pairs of surfaces", |ui| {
                    let mut remove = None;
                    for (i, r) in f.rows.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{}.", i + 1));
                            d::pick_from(ui, &format!("vb{i}"), &surfaces, &mut r.base);
                            ui.label("against");
                            ui_kit::segmented(ui, &mut r.mode, &["Surface", "Level"]);
                            if r.mode == 0 {
                                d::pick_from(ui, &format!("vc{i}"), &surfaces, &mut r.comp);
                            } else {
                                d::num(ui, &mut r.level, "m");
                            }
                            if crate::ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        f.rows.remove(i);
                    }
                    if ui.button("Add a pair").clicked() {
                        f.rows.push(VolRow { base: f.rows.last().and_then(|r| r.base), comp: None, mode: 0, level: String::new() });
                    }
                    ui.horizontal(|ui| {
                        ui.label("Limit to the inside of");
                        let mut names = vec!["(no polygon)".to_string()];
                        names.extend(parcels.iter().map(|p| p.1.clone()));
                        ui_kit::combo_owned(ui, "vol-region", &mut f.region, &names);
                    });
                });
                ui_kit::section(ui, "Stage-storage", |ui| {
                    ui_kit::form(ui, "vol-ss", |fm| {
                        fm.row("Surface", |ui| {
                            d::pick_from(ui, "vs-s", &surfaces, &mut f.stage_surface);
                        });
                        fm.row("First level", |ui| {
                            d::num_opt(ui, &mut f.stage_from, "m").ok();
                        });
                        fm.row("Level step", |ui| {
                            d::num_opt(ui, &mut f.stage_step, "m").ok();
                        });
                        fm.row("Number of levels", |ui| {
                            d::num_opt(ui, &mut f.stage_count, "").ok();
                        });
                    });
                    crate::ui_kit::note(ui, "Leave these blank for no stage-storage table.");
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
                if busy {
                    ui.label(RichText::new("Calculating...").weak());
                }
                if let Some(o) = res {
                    ui_kit::section(ui, "Results", |ui| {
                        let rows: Vec<Vec<String>> = o
                            .rows
                            .iter()
                            .enumerate()
                            .map(|(i, r)| match r {
                                Ok(v) => vec![(i + 1).to_string(), fmt(v.cut, 2), fmt(v.fill, 2), fmt(v.net, 2), fmt(v.overlap_area, 1)],
                                Err(e) => vec![(i + 1).to_string(), e.clone(), String::new(), String::new(), String::new()],
                            })
                            .collect();
                        ui_kit::results_table(ui, "vol-r", &["Pair", "Cut (m3)", "Fill (m3)", "Net (m3)", "Overlap (m2)"], &[Col::Num, Col::Num, Col::Num, Col::Num, Col::Num], &rows);
                        if let Some(st) = &o.stage {
                            match st {
                                Ok(pts) => {
                                    let rows: Vec<Vec<String>> = pts.iter().map(|p| vec![fmt(p.elevation, 3), fmt(p.volume, 2), fmt(p.water_area, 1), fmt(p.incremental_volume, 2)]).collect();
                                    ui_kit::results_table(ui, "vol-st", &["Level (m)", "Volume (m3)", "Water area (m2)", "Added (m3)"], &[Col::Num, Col::Num, Col::Num, Col::Num], &rows);
                                }
                                Err(e) => {
                                    ui.label(RichText::new(e).color(crate::theme::warn()));
                                }
                            }
                        }
                        if ui.button("Export results as CSV...").clicked() {
                            csv = true;
                        }
                    });
                }
            },
            |ui| Footer::new("Calculate").icon(crate::glyphs::by_name("VOLUMES")).enabled(!busy, "A calculation is running").secondary(&[]).cancel(Some("Close")).show(ui),
        );
        let _ = out_rows;
        if csv {
            if let Some(o) = &self.civil_ui.surf.vol_out {
                let mut s = String::from("pair,cut_m3,fill_m3,net_m3,overlap_m2\n");
                for (i, r) in o.rows.iter().enumerate() {
                    if let Ok(v) = r {
                        s.push_str(&format!("{},{:.3},{:.3},{:.3},{:.2}\n", i + 1, v.cut, v.fill, v.net, v.overlap_area));
                    }
                }
                if let Some(Ok(pts)) = &o.stage {
                    s.push_str("\nlevel_m,volume_m3,water_area_m2,added_m3\n");
                    for p in pts {
                        s.push_str(&format!("{:.3},{:.3},{:.2},{:.3}\n", p.elevation, p.volume, p.water_area, p.incremental_volume));
                    }
                }
                if let Some(path) = rfd::FileDialog::new().set_file_name("volumes.csv").save_file() {
                    self.status = match std::fs::write(&path, s) {
                        Ok(()) => format!("Wrote {}.", path.display()),
                        Err(e) => format!("The file could not be written: {e}"),
                    };
                }
            }
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            run = true;
        }
        if run {
            f.message.clear();
            let region: Option<Vec<[f64; 2]>> = if f.region > 0 {
                parcels.get(f.region - 1).and_then(|p| civil::get::<afrobim_cogo::Parcel>(&self.doc, p.0, PARCEL)).and_then(|p| p.outline(crate::civil_build::DISPLAY_FLATNESS_M).ok()).map(|r| r.iter().map(|q| [q.e, q.n]).collect())
            } else {
                None
            };
            let mut jobs: Vec<(Option<Arc<Tin>>, Option<Arc<Tin>>, Option<f64>)> = Vec::new();
            for r in &f.rows {
                let base = r.base.and_then(|b| self.tin_for(b));
                let comp = if r.mode == 0 { r.comp.and_then(|c| self.tin_for(c)) } else { None };
                jobs.push((base, comp, if r.mode == 1 { r.level.trim().parse().ok() } else { None }));
            }
            let stage = match (f.stage_surface.and_then(|s| self.tin_for(s)), f.stage_from.trim().parse::<f64>(), f.stage_step.trim().parse::<f64>(), f.stage_count.trim().parse::<usize>()) {
                (Some(t), Ok(a), Ok(s), Ok(n)) if s > 0.0 && n > 0 && n < 5000 => Some((t, (0..n).map(|i| a + s * i as f64).collect::<Vec<f64>>())),
                _ => None,
            };
            let fp = crate::background::fingerprint(&self.doc);
            self.bg.volumes = Some(crate::background::Job::spawn("Calculating volumes", fp, move || {
                let rows = jobs
                    .iter()
                    .map(|(b, c, l)| match (b, c, l) {
                        (Some(b), Some(c), _) => afrobim_terrain::volume_between(b, c, region.as_deref()).map_err(|e| e.to_string()),
                        (Some(b), None, Some(l)) => afrobim_terrain::volume_against_level(b, *l, region.as_deref()).map_err(|e| e.to_string()),
                        _ => Err("Choose both surfaces, or a surface and a level.".to_string()),
                    })
                    .collect();
                let stage = stage.map(|(t, levels)| afrobim_terrain::stage_storage(&t, region.as_deref(), &levels).map_err(|e| e.to_string()));
                VolOut { rows, stage }
            }));
            self.civil_ui.surf.vol_out = None;
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        self.civil_ui.surf.volumes = Some(f);
    }
}

#[allow(dead_code)]
fn _unused(_: &Ui, _: &PointsData, _: &str, _: &Scene, _: [u8; 3], _: [u8; 3]) {
    let _ = (POINTS, C_BORDER, C_DESIGN);
}
