// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The assembly editor (ASSEMBLY): an assembly built from the subassembly library with a live
//! cross-section preview, per-part parameters (every one an input, with an Example values button),
//! left and right sides, mirror, code renames and the BIM classification fields; saved into the
//! document and to a reusable library file.
//!
//! The editor keeps its own model (`EdAssembly`, text fields as typed) in the element's property
//! `civil.editor`, next to the engine assembly in the element's data, so an assembly opens again
//! exactly as it was left. An assembly without that property (imported or made another way) opens
//! with its parts carried unchanged and not editable.

use std::collections::BTreeMap;

use afrobim_corridor::{library, Assembly, Classification, Ditch, DitchShape, Layer, OffsetTarget, Side, Slope, Subassembly};
use afrobim_kernel::{ElementId, SelectMode};
use egui::{pos2, Color32, RichText, Sense, Stroke, Ui};
use serde::{Deserialize, Serialize};

use crate::app::AfroBimApp;
use crate::civil::{self, ASSEMBLY};
use crate::civil_dlg::{self as d};
use crate::civil_targets::{self as tg, DocObjs, TargetRef};
use crate::ui_kit::{self, Dialog, Footer, Level};

pub const PROP_EDITOR: &str = "civil.editor";

/// A library part: its name in the list, its parameters (label, unit) and whether it has layers.
pub struct PartKind {
    pub name: &'static str,
    pub params: &'static [(&'static str, &'static str)],
    pub layers: bool,
    /// Example values, marked in the dialog as not from any standard.
    pub example: &'static [&'static str],
}

pub const KINDS: [PartKind; 14] = [
    PartKind { name: "Lane", params: &[("Width", "m"), ("Cross-slope (negative falls away)", "%")], layers: true, example: &["3.5", "-2"] },
    PartKind { name: "Shoulder", params: &[("Width", "m"), ("Cross-slope", "%")], layers: true, example: &["1.5", "-4"] },
    PartKind { name: "Footway", params: &[("Width", "m"), ("Cross-slope", "%")], layers: true, example: &["1.5", "-2"] },
    PartKind { name: "Verge", params: &[("Width", "m"), ("Cross-slope", "%")], layers: false, example: &["1.0", "-4"] },
    PartKind { name: "Kerb", params: &[("Height", "m"), ("Width", "m"), ("Bed depth", "m")], layers: false, example: &["0.15", "0.15", "0.1"] },
    PartKind { name: "Kerb and channel", params: &[("Channel width", "m"), ("Channel cross-slope", "%"), ("Kerb height", "m"), ("Kerb width", "m"), ("Bed depth", "m")], layers: false, example: &["0.3", "-5", "0.15", "0.15", "0.1"] },
    PartKind { name: "Raised median", params: &[("Height", "m"), ("Half width", "m"), ("Bed depth", "m")], layers: false, example: &["0.15", "1.0", "0.1"] },
    PartKind { name: "Depressed median", params: &[("Half bottom width", "m"), ("Depth", "m"), ("Side run per rise", "")], layers: false, example: &["1.0", "0.5", "4"] },
    PartKind { name: "V ditch", params: &[("Depth", "m"), ("Inner run per rise", ""), ("Outer run per rise", "")], layers: false, example: &["0.6", "4", "3"] },
    PartKind { name: "Trapezoid ditch", params: &[("Depth", "m"), ("Bottom width", "m"), ("Inner run per rise", ""), ("Outer run per rise", "")], layers: false, example: &["0.6", "0.5", "4", "3"] },
    PartKind { name: "Retaining edge", params: &[("Thickness", "m"), ("Depth", "m")], layers: false, example: &["0.3", "1.0"] },
    PartKind { name: "Daylight", params: &[("Cut run per rise", ""), ("Fill run per rise", "")], layers: false, example: &["2", "2"] },
    PartKind { name: "Lined V drain", params: &[("Depth", "m"), ("Inner run per rise", ""), ("Outer run per rise", ""), ("Lining thickness", "m")], layers: false, example: &["0.6", "4", "3", "0.1"] },
    PartKind { name: "Lined trapezoid drain", params: &[("Depth", "m"), ("Bottom width", "m"), ("Inner run per rise", ""), ("Outer run per rise", ""), ("Lining thickness", "m")], layers: false, example: &["0.6", "0.5", "4", "3", "0.1"] },
];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EdLayer {
    pub code: String,
    pub depth: String,
    pub material: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EdPart {
    pub kind: usize,
    pub name: String,
    pub vals: Vec<String>,
    pub layers: Vec<EdLayer>,
    /// 0 none, 1 IfcPavement (the constant the corridor crate provides), 2 free text (unverified).
    pub ifc: usize,
    pub ifc_text: String,
    pub predefined: String,
    pub material: String,
    pub cost: String,
    /// "From=To; From=To": the codes renamed in this part.
    pub renames: String,
    /// A lane or shoulder takes the cross-fall of the alignment's superelevation where one is
    /// stored (the library's superelevated lane and shoulder).
    #[serde(default)]
    pub superelevated: bool,
    /// The lane of the road type a lane part takes its default width from (for example L1).
    #[serde(default)]
    pub lane: String,
    /// Which shoulder element a shoulder part takes its default from (0 hard shoulder, 1 hard strip nearside, 2 offside).
    #[serde(default)]
    pub element: usize,
    /// A lane, shoulder, footway or verge whose width runs to an offset target, not a number.
    #[serde(default)]
    pub width_target: Option<OffsetTarget>,
    /// The objects the targets of this part were taken from, by role (width, offset, elevation).
    #[serde(default)]
    pub refs: BTreeMap<String, TargetRef>,
    /// A part carried unchanged (not made in this editor), or a generic link made here.
    pub raw: Option<Subassembly>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EdAssembly {
    pub name: String,
    pub left: Vec<EdPart>,
    pub right: Vec<EdPart>,
}

impl EdPart {
    pub fn new(kind: usize) -> EdPart {
        EdPart { kind, name: KINDS[kind].name.to_string(), vals: vec![String::new(); KINDS[kind].params.len()], ..Default::default() }
    }

    /// A generic link made in this editor: nothing is entered, so it reads as incomplete until the codes, the width or targets and the slope are.
    pub fn new_link() -> EdPart {
        use afrobim_corridor::{Dim, GenericLink, LinkMode, SubKind};
        let sub = Subassembly::new("Generic link", SubKind::Link(GenericLink { codes: vec![], point_code: String::new(), mode: LinkMode::WidthSlope { width: Dim::Fixed(0.0), slope: Slope::Percent(0.0) } }));
        EdPart { kind: 0, name: "Generic link".into(), raw: Some(sub), ..Default::default() }
    }

    pub fn example(&mut self) {
        let k = &KINDS[self.kind];
        self.vals = k.example.iter().map(|s| s.to_string()).collect();
        if k.layers && self.layers.is_empty() {
            self.layers = vec![EdLayer { code: "Pave1".into(), depth: "0.05".into(), material: "example material A".into() }, EdLayer { code: "Base".into(), depth: "0.15".into(), material: "example material B".into() }];
        }
    }

    /// The engine part, or what is missing in words.
    pub fn build(&self) -> Result<Subassembly, String> {
        if let Some(r) = &self.raw {
            // The two kinds the editor can change are checked, so that a value entered here is
            // refused in words; every other carried part passes through unchanged.
            if matches!(r.kind, afrobim_corridor::SubKind::Daylight(_) | afrobim_corridor::SubKind::Link(_)) {
                r.check().map_err(|e| format!("{}: {e}", self.name))?;
            }
            return Ok(r.clone());
        }
        let k = &KINDS[self.kind];
        let mut v = Vec::new();
        for (i, (label, _)) in k.params.iter().enumerate() {
            let t = self.vals.get(i).map(|s| s.trim()).unwrap_or("");
            if i == 0 && self.width_target.is_some() && self.kind <= 3 {
                // The width is the distance to the target; the library constructor needs a number that is replaced below.
                v.push(1.0);
                continue;
            }
            v.push(t.parse::<f64>().ok().filter(|x| x.is_finite()).ok_or_else(|| format!("{}: enter the {}.", self.name, label.to_lowercase()))?);
        }
        let mut layers = Vec::new();
        for l in &self.layers {
            let depth = l.depth.trim().parse::<f64>().ok().filter(|x| *x > 0.0).ok_or_else(|| format!("{}: layer '{}' needs a depth greater than 0.", self.name, l.code))?;
            if l.code.trim().is_empty() {
                return Err(format!("{}: every layer needs a shape code.", self.name));
            }
            let mut layer = Layer::new(l.code.trim(), depth);
            if !l.material.trim().is_empty() {
                layer = layer.as_course(l.material.trim());
            }
            layers.push(layer);
        }
        let slope = |p: f64| Slope::Percent(p);
        let name = self.name.as_str();
        let mut s = match self.kind {
            0 if self.superelevated => library::lane_superelevated(name, v[0], slope(v[1]), layers),
            0 => library::lane(name, v[0], slope(v[1]), layers),
            1 if self.superelevated => library::shoulder_superelevated(name, v[0], slope(v[1]), layers),
            1 => library::shoulder(name, v[0], slope(v[1]), layers),
            2 => library::footway(name, v[0], slope(v[1]), layers),
            3 => library::verge(name, v[0], slope(v[1])),
            4 => library::kerb(name, v[0], v[1], v[2]),
            5 => library::kerb_and_channel(name, v[0], slope(v[1]), v[2], v[3], v[4]),
            6 => library::median_raised(name, v[0], v[1], v[2]),
            7 => library::median_depressed(name, v[0], v[1], v[2]),
            8 => library::side_ditch_v(name, v[0], v[1], v[2]),
            9 => library::side_ditch_trapezoid(name, v[0], v[1], v[2], v[3]),
            10 => library::retaining_edge(name, v[0], v[1]),
            11 => library::daylight(v[0], v[1]),
            12 => library::lined_drain(name, Ditch { shape: DitchShape::V, depth_m: v[0], inner_run_per_rise: v[1], outer_run_per_rise: v[2] }, v[3]),
            _ => library::lined_drain(name, Ditch { shape: DitchShape::Trapezoid { bottom_width_m: v[1] }, depth_m: v[0], inner_run_per_rise: v[2], outer_run_per_rise: v[3] }, v[4]),
        };
        let mut c = Classification::default();
        match self.ifc {
            1 => {
                c.ifc_entity = Some(afrobim_corridor::ifc::PAVEMENT.to_string());
            }
            2 if !self.ifc_text.trim().is_empty() => c.ifc_entity = Some(self.ifc_text.trim().to_string()),
            _ => {}
        }
        if !self.predefined.trim().is_empty() {
            c.predefined_type = Some(self.predefined.trim().to_string());
        }
        if !self.material.trim().is_empty() {
            c.material = Some(self.material.trim().to_string());
        }
        if !self.cost.trim().is_empty() {
            c.cost_item_code = Some(self.cost.trim().to_string());
        }
        s = s.classified(c);
        if let Some(t) = &self.width_target {
            use afrobim_corridor::{Dim, SubKind};
            if let SubKind::Lane(st) | SubKind::Shoulder(st) | SubKind::Footway(st) | SubKind::Verge(st) = &mut s.kind {
                st.width = Dim::ToOffset(t.clone());
            }
        }
        for pair in self.renames.split(';') {
            if let Some((a, b)) = pair.split_once('=') {
                if !a.trim().is_empty() && !b.trim().is_empty() {
                    s = s.rename_code(a.trim(), b.trim());
                }
            }
        }
        s.check().map_err(|e| format!("{}: {e}", self.name))?;
        Ok(s)
    }
}

impl EdAssembly {
    pub fn side(&self, s: Side) -> &Vec<EdPart> {
        if s == Side::Left {
            &self.left
        } else {
            &self.right
        }
    }
    pub fn side_mut(&mut self, s: Side) -> &mut Vec<EdPart> {
        if s == Side::Left {
            &mut self.left
        } else {
            &mut self.right
        }
    }

    /// The engine assembly of every part that is complete, and what is missing from the rest.
    pub fn build(&self) -> (Assembly, Vec<String>) {
        let mut a = Assembly::new(if self.name.trim().is_empty() { "Assembly" } else { self.name.trim() });
        let mut problems = Vec::new();
        for side in [Side::Left, Side::Right] {
            for p in self.side(side) {
                match p.build() {
                    Ok(s) => {
                        a.add(side, s);
                    }
                    Err(e) => problems.push(e),
                }
            }
        }
        (a, problems)
    }

    pub fn from_assembly(a: &Assembly) -> EdAssembly {
        let part = |s: &Subassembly| EdPart { kind: 0, name: s.name.clone(), raw: Some(s.clone()), ..Default::default() };
        EdAssembly { name: a.name.clone(), left: a.left.iter().map(part).collect(), right: a.right.iter().map(part).collect() }
    }
}

#[derive(Default)]
pub struct AssemblyUi {
    pub form: Option<AsmForm>,
}

pub struct AsmForm {
    id: Option<ElementId>,
    ed: EdAssembly,
    side: usize,
    sel: usize,
    add_kind: usize,
    /// The height of the centreline above the level ground the preview is drawn on.
    preview_height: String,
    message: String,
    /// The road type the defaults are looked up for.
    road: crate::defaults_glue::RoadChoice,
    /// The alignments and feature lines of the document a target can be taken from.
    objs: DocObjs,
    /// The preview: 0 level ground under a made-up straight, otherwise the alignment at this index + 1.
    prev_on: usize,
    prev_station: String,
}

impl AfroBimApp {
    pub(crate) fn civil_assembly_cmd(&mut self) {
        let sel = self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == ASSEMBLY));
        let form = match sel.and_then(|id| self.doc.find(id).map(|e| (id, e))) {
            Some((id, e)) => {
                let ed = e.properties.get(PROP_EDITOR).and_then(|t| serde_json::from_str::<EdAssembly>(t).ok()).or_else(|| civil::get::<Assembly>(&self.doc, id, ASSEMBLY).map(|a| EdAssembly::from_assembly(&a))).unwrap_or_default();
                AsmForm { id: Some(id), ed, side: 1, sel: 0, add_kind: 0, preview_height: "1".into(), message: String::new(), road: Default::default(), objs: DocObjs::gather(&self.doc), prev_on: 0, prev_station: String::new() }
            }
            None => AsmForm { id: None, ed: EdAssembly { name: String::new(), ..Default::default() }, side: 1, sel: 0, add_kind: 0, preview_height: "1".into(), message: String::new(), road: Default::default(), objs: DocObjs::gather(&self.doc), prev_on: 0, prev_station: String::new() },
        };
        self.civil_ui.asm.form = Some(form);
    }

    pub(crate) fn civil_assembly_windows(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.asm.form.take() else { return };
        let side = if f.side == 0 { Side::Left } else { Side::Right };
        let (assembly, problems) = f.ed.build();
        let preview_h = f.preview_height.trim().parse::<f64>().ok();
        let can_save = problems.is_empty() && !f.ed.name.trim().is_empty() && (!f.ed.left.is_empty() || !f.ed.right.is_empty());
        let out = Dialog::new("Assembly editor", crate::glyphs::by_name("ASSEMBLY")).width(1000.0).resizable(true).max_body_height(620.0).help("Parts are listed from the centreline outwards on each side. Every dimension is an input: a part is not drawn until its fields are filled. The Example values button fills demonstration numbers that are not from any standard. The preview puts the assembly on level ground below the centreline so the side slopes show; it is a drawing aid, not a design. IFC entity: only IfcPavement (the constant the corridor crate provides) is offered; anything else is free text and is marked unverified.").show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui_kit::text_field(ui, &mut f.ed.name, "e.g. Two lane road");
                    ui.add_space(8.0);
                    ui.label("Road type").on_hover_text("Pre-fills lane, hard strip and verge widths and the crossfall from the defaults basis");
                    crate::defaults_glue::road_selector(ui, "asm-road", &mut f.road);
                });
                // Side by side when there is room, one under the other in a narrow work area.
                let wide = ui.available_width() > 880.0;
                let col_h = (crate::win::work_rect(ctx).height() - 160.0 - 70.0).clamp(240.0, 620.0);
                let layout = if wide { egui::Layout::left_to_right(egui::Align::Min) } else { egui::Layout::top_down(egui::Align::Min) };
                ui.with_layout(layout, |ui| {
                    ui.vertical(|ui| {
                        if wide {
                            ui.set_width(230.0);
                        }
                        egui::ScrollArea::vertical().id_salt("asm-left").max_height(if wide { col_h } else { f32::INFINITY }).auto_shrink([false, true]).show(ui, |ui| {
                        ui_kit::section(ui, &format!("Parts, {} side, from the centreline outwards", if f.side == 0 { "left" } else { "right" }), |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Side shown");
                                ui_kit::segmented(ui, &mut f.side, &["Left", "Right"]);
                            });
                            let n = f.ed.side(side).len();
                            let mut action: Option<(usize, i32)> = None;
                            for i in 0..n {
                                ui.horizontal(|ui| {
                                    let p = &f.ed.side(side)[i];
                                    let ready = p.build().is_ok();
                                    let label = format!("{}. {}{}", i + 1, p.name, if ready { "" } else { "  (incomplete)" });
                                    if ui.selectable_label(f.sel == i, label).clicked() {
                                        f.sel = i;
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove this part").clicked() {
                                            action = Some((i, 0));
                                        }
                                        ui.add_enabled_ui(i + 1 < n, |ui| {
                                            if ui_kit::icon_button(ui, crate::glyphs::ui::ARROW_DOWN, "Move this part outwards").clicked() {
                                                action = Some((i, 1));
                                            }
                                        });
                                        ui.add_enabled_ui(i > 0, |ui| {
                                            if ui_kit::icon_button(ui, crate::glyphs::ui::ARROW_UP, "Move this part towards the centreline").clicked() {
                                                action = Some((i, -1));
                                            }
                                        });
                                    });
                                });
                            }
                            if n == 0 {
                                ui.label(RichText::new("No parts on this side yet.").size(crate::theme::FONT_SMALL).weak());
                            }
                            if let Some((i, dir)) = action {
                                let v = f.ed.side_mut(side);
                                match dir {
                                    0 => {
                                        v.remove(i);
                                    }
                                    -1 => v.swap(i, i - 1),
                                    _ => v.swap(i, i + 1),
                                }
                                f.sel = f.sel.min(f.ed.side(side).len().saturating_sub(1));
                            }
                            ui.horizontal(|ui| {
                                let mut names: Vec<&str> = KINDS.iter().map(|k| k.name).collect();
                                names.push("Generic link (to targets)");
                                ui_kit::combo(ui, "asm-kind", &mut f.add_kind, &names);
                                if ui.button("Add part").clicked() {
                                    let v = f.ed.side_mut(side);
                                    v.push(if f.add_kind >= KINDS.len() { EdPart::new_link() } else { EdPart::new(f.add_kind) });
                                    f.sel = v.len() - 1;
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.button("Copy right side to left").clicked() {
                                    f.ed.left = f.ed.right.clone();
                                }
                                if ui.button("Copy left side to right").clicked() {
                                    f.ed.right = f.ed.left.clone();
                                }
                            });
                        });
                        });
                    });
                    ui.vertical(|ui| {
                        if wide {
                            ui.set_width(380.0);
                        }
                        egui::ScrollArea::vertical().id_salt("asm-mid").max_height(if wide { col_h } else { f32::INFINITY }).auto_shrink([false, true]).show(ui, |ui| {
                        let sel = f.sel;
                        ui_kit::section(ui, "Selected part", |ui| {
                            if let Some(p) = f.ed.side_mut(side).get_mut(sel) {
                                part_editor(ui, p, &f.road, &f.objs);
                            } else {
                                ui.label(RichText::new("Add a part, then select it to enter its dimensions.").size(crate::theme::FONT_SMALL).weak());
                            }
                        });
                        });
                    });
                    ui.vertical(|ui| {
                        if wide {
                            ui.set_width(330.0);
                        }
                        egui::ScrollArea::vertical().id_salt("asm-right").max_height(if wide { col_h } else { f32::INFINITY }).auto_shrink([false, true]).show(ui, |ui| {
                        ui_kit::section(ui, "Preview", |ui| {
                            let mut names: Vec<String> = vec!["Level ground, made-up straight".into()];
                            names.extend(f.objs.aligns.iter().map(|a| a.0.clone()));
                            ui_kit::form(ui, "asm-preview", |fm| {
                                fm.row("Preview along", |ui| {
                                    ui_kit::combo_owned(ui, "asm-prev-on", &mut f.prev_on, &names);
                                });
                                if f.prev_on > 0 {
                                    let al = &f.objs.aligns[f.prev_on - 1].1;
                                    let unit = format!("m ({:.0} to {:.0})", al.start_station_m, al.end_station_m());
                                    fm.row("Station", |ui| {
                                        d::num(ui, &mut f.prev_station, &unit);
                                    });
                                }
                                fm.row("Centreline height", |ui| {
                                    d::num(ui, &mut f.preview_height, "m above the ground");
                                });
                            });
                            let on = if f.prev_on > 0 { f.prev_station.trim().parse::<f64>().ok().map(|st| (&f.objs.aligns[f.prev_on - 1].1, st)) } else { None };
                            preview(ui, &assembly, preview_h, on, f.prev_on > 0);
                            for p in &problems {
                                ui_kit::callout(ui, Level::Warning, p.clone());
                            }
                        });
                        });
                    });
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
            |ui| Footer::new("Save in the document").icon(crate::glyphs::by_name("ASSEMBLY")).secondary(&["Save to library file...", "Load from library file..."]).enabled(can_save, "Name the assembly and complete every part").cancel(Some("Close")).show(ui),
        );
        let fo = out.inner.as_ref().map(|(_, f)| *f);
        if fo.is_some_and(|x| x.primary) {
            let objs = f.objs.clone();
            let mut notes = Vec::new();
            for side in [Side::Left, Side::Right] {
                for p in f.ed.side_mut(side) {
                    notes.extend(rebind_part(p, &objs));
                }
            }
            let (a, _) = f.ed.build();
            let v = serde_json::to_value(&a).unwrap_or_default();
            let editor = serde_json::to_string(&f.ed).unwrap_or_default();
            let id = match f.id {
                Some(id) => {
                    civil::set(&mut self.doc, id, v, None);
                    if let Some(e) = self.doc.find_mut(id) {
                        e.name = f.ed.name.trim().to_string();
                    }
                    id
                }
                None => civil::add(&mut self.doc, ASSEMBLY, f.ed.name.trim(), v, None, "Built in the assembly editor"),
            };
            if let Some(e) = self.doc.find_mut(id) {
                e.properties.insert(PROP_EDITOR.into(), editor);
            }
            self.doc.select(&[id], SelectMode::Replace);
            self.status = format!("Saved the assembly '{}' in the document.", f.ed.name.trim());
            self.schedule_commit_now();
            f.id = Some(id);
            f.message = if notes.is_empty() { "Saved in the document.".into() } else { format!("Saved in the document. Not refreshed from the document: {}", notes.join("; ")) };
        }
        if fo.is_some_and(|x| x.secondary == Some(0)) {
            let (a, _) = f.ed.build();
            if let Some(path) = rfd::FileDialog::new().add_filter("AfroBIM assembly library", &["json"]).set_file_name(format!("{}.assembly.json", f.ed.name.trim())).save_file() {
                let file = serde_json::json!({ "afrobim_assembly": 1, "editor": f.ed, "assembly": a });
                f.message = match std::fs::write(&path, serde_json::to_string_pretty(&file).unwrap_or_default()) {
                    Ok(()) => format!("Saved to {}.", path.display()),
                    Err(e) => format!("The file could not be written: {e}"),
                };
            }
        }
        if fo.is_some_and(|x| x.secondary == Some(1)) {
            if let Some(path) = rfd::FileDialog::new().add_filter("AfroBIM assembly library", &["json"]).pick_file() {
                match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).map_err(|e| e.to_string())) {
                    Ok(v) => {
                        let ed = v.get("editor").and_then(|e| serde_json::from_value::<EdAssembly>(e.clone()).ok()).or_else(|| v.get("assembly").and_then(|a| serde_json::from_value::<Assembly>(a.clone()).ok()).map(|a| EdAssembly::from_assembly(&a)));
                        match ed {
                            Some(ed) => {
                                f.ed = ed;
                                f.message = format!("Loaded {}.", path.display());
                            }
                            None => f.message = "The file is not an AfroBIM assembly.".into(),
                        }
                    }
                    Err(e) => f.message = format!("The file could not be read: {e}"),
                }
            }
        }
        if out.closed || fo.is_some_and(|x| x.cancel) {
            return;
        }
        self.civil_ui.asm.form = Some(f);
    }
}

/// Takes the targets of a part that were chosen by name from the document again.
fn rebind_part(p: &mut EdPart, o: &DocObjs) -> Vec<String> {
    let mut out = Vec::new();
    if let (Some(t), Some(r)) = (p.width_target.as_mut(), p.refs.get("width")) {
        if let Err(e) = tg::rebind_offset(t, r, o) {
            out.push(format!("{}: {e}", p.name));
        }
    }
    if let Some(sub) = p.raw.as_mut() {
        out.extend(tg::rebind_sub(sub, &p.refs, o));
    }
    out
}

/// The parameters of any library part as label and value, for a part carried unchanged. Covers
/// every kind the library has, whether or not the editor can make it.
pub fn describe(s: &Subassembly) -> Vec<(String, String)> {
    use afrobim_corridor::{Dim, LinkMode, OffsetTarget, ElevationTarget, SubKind};
    let m = |v: f64| format!("{v} m");
    let slope = |sl: &Slope| match sl {
        Slope::Percent(p) => format!("{p} %"),
        Slope::Ratio { run, rise } => format!("{rise} rise in {run} run"),
    };
    let dim = |d: &Dim| match d {
        Dim::Fixed(w) => m(*w),
        Dim::ToOffset(t) => format!("to {}", offset_target(t)),
    };
    fn offset_target(t: &OffsetTarget) -> String {
        match t {
            OffsetTarget::Fixed(o) => format!("offset {o} m"),
            OffsetTarget::Table(t) => format!("an offset table of {} stations", t.len()),
            OffsetTarget::Polyline(p) => format!("a plan polyline of {} points", p.len()),
        }
    }
    let ditch = |d: &Ditch| {
        let mut v = vec![("Shape".to_string(), match d.shape {
            DitchShape::V => "V".to_string(),
            DitchShape::Trapezoid { .. } => "Trapezoid".to_string(),
        })];
        v.push(("Depth".into(), m(d.depth_m)));
        if let DitchShape::Trapezoid { bottom_width_m } = d.shape {
            v.push(("Bottom width".into(), m(bottom_width_m)));
        }
        v.push(("Inner run per rise".into(), d.inner_run_per_rise.to_string()));
        v.push(("Outer run per rise".into(), d.outer_run_per_rise.to_string()));
        v
    };
    let strip = |st: &afrobim_corridor::Strip| {
        let mut v = vec![("Width".to_string(), dim(&st.width)), ("Cross-slope".into(), slope(&st.slope))];
        if st.follows_superelevation {
            v.push(("Cross-fall".into(), "follows the superelevation where one is stored".into()));
        }
        for (i, l) in st.layers.iter().enumerate() {
            v.push((format!("Layer {} ({})", i + 1, l.shape_code), m(l.depth_m)));
        }
        v.push(("Outer point code".into(), st.point_code.clone()));
        v
    };
    let block = |b: &afrobim_corridor::Block| vec![("Height".to_string(), m(b.height_m)), ("Width".into(), m(b.width_m)), ("Bed depth".into(), m(b.bed_depth_m))];
    let bench = |label: &str, b: &Option<afrobim_corridor::Bench>| match b {
        Some(b) => vec![(label.to_string(), format!("every {} m of height, {} m wide, cross-slope {}", b.step_height_m, b.width_m, slope(&b.slope)))],
        None => vec![],
    };
    match &s.kind {
        SubKind::Lane(st) | SubKind::Shoulder(st) | SubKind::Footway(st) | SubKind::Verge(st) => strip(st),
        SubKind::Kerb(b) | SubKind::MedianRaised(b) => block(b),
        SubKind::KerbAndChannel { channel, kerb } => {
            let mut v: Vec<(String, String)> = strip(channel).into_iter().map(|(a, b)| (format!("Channel {}", a.to_lowercase()), b)).collect();
            v.extend(block(kerb).into_iter().map(|(a, b)| (format!("Kerb {}", a.to_lowercase()), b)));
            v
        }
        SubKind::MedianDepressed(d) => vec![("Half bottom width".to_string(), m(d.half_bottom_width_m)), ("Depth".into(), m(d.depth_m)), ("Side run per rise".into(), d.side_run_per_rise.to_string())],
        SubKind::SideDitch(d) => ditch(d),
        SubKind::LinedDrain { ditch: d, lining_thickness_m } => {
            let mut v = ditch(d);
            v.push(("Lining thickness".into(), m(*lining_thickness_m)));
            v
        }
        SubKind::RetainingEdge(w) => vec![("Thickness".to_string(), m(w.thickness_m)), ("Depth".into(), m(w.depth_m))],
        SubKind::Link(l) => {
            let mut v = vec![("Link codes".to_string(), l.codes.join(", ")), ("Point code".into(), l.point_code.clone())];
            match &l.mode {
                LinkMode::WidthSlope { width, slope: sl } => {
                    v.push(("Width".into(), dim(width)));
                    v.push(("Slope".into(), slope(sl)));
                }
                LinkMode::ToElevation { target, slope: sl } => {
                    v.push(("Runs to".into(), match target {
                        ElevationTarget::Fixed(z) => format!("level {z} m"),
                        ElevationTarget::Table(t) => format!("a level table of {} stations", t.len()),
                        ElevationTarget::Profile(_) => "the level of a profile".into(),
                        ElevationTarget::NearestOnAlignment(a) => format!("the level of alignment '{}' at its station nearest the section point", a.name),
                    }));
                    v.push(("Slope".into(), slope(sl)));
                }
                LinkMode::ToOffset { target, slope: sl } => {
                    v.push(("Runs to".into(), offset_target(target)));
                    v.push(("Slope".into(), slope(sl)));
                }
                LinkMode::ToOffsetAndElevation { offset, elevation } => {
                    v.push(("Runs to the offset".into(), offset_target(offset)));
                    v.push(("and to the level".into(), match elevation {
                        ElevationTarget::Fixed(z) => format!("level {z} m"),
                        ElevationTarget::Table(t) => format!("a level table of {} stations", t.len()),
                        ElevationTarget::Profile(_) => "the level of a profile".into(),
                        ElevationTarget::NearestOnAlignment(a) => format!("the level of alignment '{}' at its station nearest the end point", a.name),
                    }));
                    v.push(("Slope".into(), "whatever joins the two (not entered)".into()));
                }
            }
            v
        }
        SubKind::Daylight(r) => {
            let mut v = vec![("Cut run per rise".to_string(), r.cut_run_per_rise.to_string()), ("Fill run per rise".into(), r.fill_run_per_rise.to_string())];
            v.extend(bench("Bench in cut", &r.bench_in_cut));
            v.extend(bench("Bench in fill", &r.bench_in_fill));
            v
        }
    }
}

/// Edits a slope in the form it already has: a percent, or a rise in a run.
fn slope_rows(fm: &mut ui_kit::Form, label: &str, s: &mut Slope) {
    match s {
        Slope::Percent(v) => {
            fm.row(label, |ui| {
                ui_kit::f64_field(ui, v, "%", -1000.0..=1000.0, 0.05);
            });
        }
        Slope::Ratio { run, rise } => {
            fm.row(&format!("{label}, run"), |ui| {
                ui_kit::f64_field(ui, run, "m", 0.0..=1000.0, 0.05);
            });
            fm.row(&format!("{label}, rise"), |ui| {
                ui_kit::f64_field(ui, rise, "m", -1000.0..=1000.0, 0.05);
            });
        }
    }
}

/// The benching of one side of a daylight rule: off, or a step height, a width and a slope. Switching
/// it on starts from zeros, which are not valid: every value is the engineer's.
fn bench_rows(ui: &mut Ui, id: &str, label: &str, b: &mut Option<afrobim_corridor::Bench>) {
    let mut on = b.is_some();
    ui_kit::toggle(ui, &mut on, label);
    if on && b.is_none() {
        *b = Some(afrobim_corridor::Bench { step_height_m: 0.0, width_m: 0.0, slope: Slope::Percent(0.0) });
    } else if !on {
        *b = None;
    }
    if let Some(bench) = b {
        ui_kit::form(ui, id, |fm| {
            fm.row("Step height", |ui| {
                ui_kit::f64_field(ui, &mut bench.step_height_m, "m", 0.0..=1000.0, 0.05);
            });
            fm.row("Bench width", |ui| {
                ui_kit::f64_field(ui, &mut bench.width_m, "m", 0.0..=1000.0, 0.05);
            });
            slope_rows(fm, "Bench cross-slope (rising outward)", &mut bench.slope);
        });
        if bench.step_height_m <= 0.0 || bench.width_m <= 0.0 {
            ui_kit::callout(ui, Level::Warning, "Enter a step height and a width greater than 0.");
        }
    }
}

/// The parameters of a carried part that the engine lets the editor change: the benched daylight
/// rule and the generic link. Returns true when it showed them (any other kind is read only).
fn raw_editor(ui: &mut Ui, name: &str, raw: &mut Subassembly, refs: &mut BTreeMap<String, TargetRef>, objs: &DocObjs) -> bool {
    use afrobim_corridor::{Dim, ElevationTarget, LinkMode, OffsetTarget, SubKind};
    match &mut raw.kind {
        SubKind::Daylight(r) => {
            ui_kit::callout(ui, Level::Info, format!("{name}: the side slope to the ground. The run per rise is yours to enter; a bench is optional, and when it is on every value of it is an input."));
            ui_kit::section(ui, "Side slopes", |ui| {
                ui_kit::form(ui, "asm-day", |fm| {
                    fm.row("Cut run per rise", |ui| {
                        ui_kit::f64_field(ui, &mut r.cut_run_per_rise, "", 0.0..=1000.0, 0.05);
                    });
                    fm.row("Fill run per rise", |ui| {
                        ui_kit::f64_field(ui, &mut r.fill_run_per_rise, "", 0.0..=1000.0, 0.05);
                    });
                });
            });
            ui_kit::section(ui, "Bench in cut", |ui| bench_rows(ui, "asm-bench-cut", "Bench the cut slope", &mut r.bench_in_cut));
            ui_kit::section(ui, "Bench in fill", |ui| bench_rows(ui, "asm-bench-fill", "Bench the fill slope", &mut r.bench_in_fill));
            true
        }
        SubKind::Link(l) => {
            let mut codes = l.codes.join(", ");
            let mut mode = match &l.mode {
                LinkMode::WidthSlope { .. } => 0,
                LinkMode::ToElevation { .. } => 1,
                LinkMode::ToOffset { .. } => 2,
                LinkMode::ToOffsetAndElevation { .. } => 3,
            };
            let was = mode;
            ui_kit::form(ui, "asm-link", |fm| {
                fm.row("Link codes", |ui| {
                    if ui_kit::text_field(ui, &mut codes, "comma separated").changed() {
                        l.codes = codes.split(',').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
                    }
                });
                fm.row("Point code", |ui| {
                    ui_kit::text_field(ui, &mut l.point_code, "");
                });
                fm.row("The link ends", |ui| {
                    ui_kit::combo(ui, "asm-link-mode", &mut mode, &["At a width and a slope", "At a level, running at a slope", "At an offset, running at a slope", "At an offset and a level together"]);
                });
            });
            if mode != was {
                let old = match &l.mode {
                    LinkMode::WidthSlope { slope, .. } | LinkMode::ToElevation { slope, .. } | LinkMode::ToOffset { slope, .. } => Some(slope.clone()),
                    LinkMode::ToOffsetAndElevation { .. } => None,
                };
                let sl = old.unwrap_or(Slope::Percent(0.0));
                l.mode = match mode {
                    0 => LinkMode::WidthSlope { width: Dim::Fixed(0.0), slope: sl },
                    1 => LinkMode::ToElevation { target: ElevationTarget::Table(vec![]), slope: sl },
                    2 => LinkMode::ToOffset { target: OffsetTarget::Table(vec![]), slope: sl },
                    _ => LinkMode::ToOffsetAndElevation { offset: OffsetTarget::Table(vec![]), elevation: ElevationTarget::Table(vec![]) },
                };
                refs.clear();
            }
            match &mut l.mode {
                LinkMode::WidthSlope { width, slope } => {
                    ui_kit::section(ui, "Width", |ui| {
                        let r = refs.entry("width".into()).or_default();
                        tg::dim_ui(ui, "asm-link-w", width, r, objs);
                    });
                    ui_kit::section(ui, "Slope", |ui| ui_kit::form(ui, "asm-link-s", |fm| slope_rows(fm, "Slope", slope)));
                }
                LinkMode::ToElevation { target, slope } => {
                    ui_kit::section(ui, "Target level", |ui| {
                        let r = refs.entry("elevation".into()).or_default();
                        tg::elevation_ui(ui, "asm-link-z", target, r, objs);
                    });
                    ui_kit::section(ui, "Slope", |ui| ui_kit::form(ui, "asm-link-s", |fm| slope_rows(fm, "Slope", slope)));
                }
                LinkMode::ToOffset { target, slope } => {
                    ui_kit::section(ui, "Target offset", |ui| {
                        let r = refs.entry("offset".into()).or_default();
                        tg::offset_ui(ui, "asm-link-o", target, r, objs);
                    });
                    ui_kit::section(ui, "Slope", |ui| ui_kit::form(ui, "asm-link-s", |fm| slope_rows(fm, "Slope", slope)));
                }
                LinkMode::ToOffsetAndElevation { offset, elevation } => {
                    ui.label(RichText::new("Fixed in plan and in level together: the slope is whatever joins the two.").size(crate::theme::FONT_SMALL).weak());
                    ui_kit::section(ui, "Target offset", |ui| {
                        let r = refs.entry("offset".into()).or_default();
                        tg::offset_ui(ui, "asm-link-o", offset, r, objs);
                    });
                    ui_kit::section(ui, "Target level", |ui| {
                        let r = refs.entry("elevation".into()).or_default();
                        tg::elevation_ui(ui, "asm-link-z", elevation, r, objs);
                    });
                }
            }
            if l.codes.is_empty() {
                ui_kit::callout(ui, Level::Warning, "A link needs at least one code.");
            }
            true
        }
        _ => false,
    }
}

fn part_editor(ui: &mut Ui, p: &mut EdPart, road: &crate::defaults_glue::RoadChoice, objs: &DocObjs) {
    let k = &KINDS[p.kind.min(KINDS.len() - 1)];
    if let Some(raw) = p.raw.as_mut() {
        if raw_editor(ui, &p.name, raw, &mut p.refs, objs) {
            return;
        }
    }
    if let Some(raw) = &p.raw {
        ui_kit::callout(ui, Level::Info, format!("{}: a part carried unchanged from the assembly it came in. Its parameters are shown below; to change them, remove it and add a library part.", p.name));
        let rows = describe(raw);
        ui_kit::form(ui, "asm-raw", |fm| {
            for (label, value) in &rows {
                fm.row(label, |ui| {
                    ui.label(value);
                });
            }
        });
        return;
    }
    ui_kit::form(ui, "asm-part", |fm| {
        fm.row("Name", |ui| d::text(ui, &mut p.name, ""));
        if p.kind == 0 && road.prefix().is_some() {
            fm.row("Lane", |ui| {
                let lanes: Vec<String> = crate::defaults_glue::labels_of(&road.key("lane_width").unwrap_or_default()).into_iter().filter_map(|l| l.strip_prefix(&format!("{}/", road.road)).map(str::to_string)).collect();
                let mut i = lanes.iter().position(|l| *l == p.lane).unwrap_or(0);
                if ui_kit::combo_owned(ui, "asm-lane", &mut i, &lanes) || p.lane.is_empty() {
                    p.lane = lanes.get(i).cloned().unwrap_or_default();
                }
            });
        }
        if p.kind == 1 && road.prefix().is_some() {
            fm.row("Element", |ui| {
                ui_kit::combo(ui, "asm-elem", &mut p.element, &["Hard shoulder", "Hard strip, nearside", "Hard strip, offside"]);
            });
        }
        if p.kind <= 3 {
            fm.row_note("Width", "A width runs to a target when the part widens toward another alignment or line", |ui| {
                let mut to = p.width_target.is_some() as usize;
                if ui_kit::segmented(ui, &mut to, &["Entered", "To an offset target"]) {
                    p.width_target = if to == 1 { Some(OffsetTarget::Table(vec![])) } else { None };
                    p.refs.remove("width");
                }
            });
        }
        for (i, (label, unit)) in k.params.iter().enumerate() {
            if i == 0 && p.kind <= 3 && p.width_target.is_some() {
                continue;
            }
            fm.row(label, |ui| {
                if p.vals.len() <= i {
                    p.vals.resize(i + 1, String::new());
                }
                // The pack's key and condition for this field, where the part and the road type give one.
                let bind: Option<(String, afrobim_defaults::Conditions, f64)> = match (p.kind, i, road.prefix()) {
                    (0, 0, Some(_)) => road.key("lane_width").map(|k| (k, afrobim_defaults::Conditions::none().label("road_type/lane", &format!("{}/{}", road.road, p.lane)), 1.0)),
                    (0, 1, _) => Some(("link.crossfall.normal".to_string(), afrobim_defaults::Conditions::none(), -1.0)),
                    (1, 0, Some(_)) => road.key(["hard_shoulder_width", "hard_strip_nearside_width", "hard_strip_offside_width"][p.element.min(2)]).map(|k| (k, road.cond(), 1.0)),
                    (3, 0, Some(_)) => road.key("verge_width").map(|k| (k, road.cond(), 1.0)),
                    _ => None,
                };
                match bind {
                    Some((key, cond, factor)) => {
                        ui_kit::default_num_field(ui, ("asm", p.kind, i), &mut p.vals[i], unit, &key, &cond, factor, &ui_kit::NumRule::required());
                    }
                    None => {
                        d::num(ui, &mut p.vals[i], unit);
                    }
                }
            });
        }
        if p.kind <= 1 {
            fm.row_note("Cross-fall", "Takes the superelevation of the alignment where one is stored", |ui| {
                ui_kit::toggle(ui, &mut p.superelevated, "Follows the superelevation");
            });
        }
    });
    if let Some(t) = p.width_target.as_mut() {
        ui_kit::section(ui, "Width target", |ui| {
            let r = p.refs.entry("width".into()).or_default();
            tg::offset_ui(ui, "asm-w", t, r, objs);
        });
    }
    if k.layers {
        ui_kit::section(ui, "Layers, top down", |ui| {
            let mut remove = None;
            egui::Grid::new("asm-layers").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
                for h in ["Shape code", "Depth (m)", "Material (free text)", ""] {
                    ui.label(RichText::new(h).size(crate::theme::FONT_SMALL).strong());
                }
                ui.end_row();
                for (i, l) in p.layers.iter_mut().enumerate() {
                    ui.add(egui::TextEdit::singleline(&mut l.code).desired_width(70.0));
                    ui.add(egui::TextEdit::singleline(&mut l.depth).desired_width(60.0));
                    ui.add(egui::TextEdit::singleline(&mut l.material).desired_width(130.0));
                    if ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove this layer").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
            if let Some(i) = remove {
                p.layers.remove(i);
            }
            if ui.button("Add layer").clicked() {
                p.layers.push(EdLayer::default());
            }
        });
    }
    ui_kit::section(ui, "BIM classification", |ui| {
        ui_kit::form(ui, "asm-bim", |fm| {
            fm.row("IFC entity", |ui| {
                ui_kit::combo(ui, "asm-ifc", &mut p.ifc, &["(none)", "IfcPavement", "Other (free text, unverified)"]);
            });
            if p.ifc == 2 {
                fm.row_note("Entity name", "Unverified: not checked against the schema", |ui| d::text(ui, &mut p.ifc_text, ""));
            }
            fm.row_note("Predefined type", "Free text, unverified", |ui| d::text(ui, &mut p.predefined, ""));
            fm.row("Material", |ui| d::text(ui, &mut p.material, ""));
            fm.row("Cost item code", |ui| d::text(ui, &mut p.cost, ""));
            fm.row_note("Code renames", "From=To; From=To", |ui| d::text(ui, &mut p.renames, ""));
        });
    });
    ui.horizontal(|ui| {
        if ui.button("Example values").on_hover_text("Demonstration numbers, not from any standard").clicked() {
            p.example();
        }
        ui.label(RichText::new("Example values are for demonstration only.").size(crate::theme::FONT_SMALL).weak());
    });
}

/// The preview: the assembly on level ground, drawn from the engine's own section at a station of
/// a straight made-up alignment. Returns nothing; an assembly that cannot be drawn says why.
fn preview(ui: &mut Ui, a: &Assembly, height: Option<f64>, on: Option<(&afrobim_alignment::Alignment, f64)>, wants_alignment: bool) {
    let size = egui::vec2(ui.available_width().clamp(240.0, 300.0), 170.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(0x1B, 0x1F, 0x25));
    let say = |t: &str| {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, t, egui::FontId::proportional(12.0), Color32::from_gray(160));
    };
    let Some(h) = height else {
        return say("Enter the height of the centreline above the ground.");
    };
    if a.left.is_empty() && a.right.is_empty() {
        return say("No parts yet.");
    }
    if a.check().is_err() {
        return say("The assembly does not pass its check yet.");
    }
    if wants_alignment && on.is_none() {
        return say("Enter a station on the chosen alignment.");
    }
    match preview_geometry(a, h, on) {
        Ok((links, shapes, ground)) => {
            let all: Vec<(f64, f64)> = links.iter().chain(shapes.iter()).chain(std::iter::once(&ground)).flatten().copied().collect();
            let (x0, x1) = all.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.0), b.max(p.0)));
            let (y0, y1) = all.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.1), b.max(p.1)));
            let k = ((rect.width() as f64 - 24.0) / (x1 - x0).max(1e-6)).min((rect.height() as f64 - 24.0) / (y1 - y0).max(1e-6));
            let map = |p: &(f64, f64)| pos2(rect.left() + 12.0 + ((p.0 - x0) * k) as f32, rect.bottom() - 12.0 - ((p.1 - y0) * k) as f32);
            painter.add(egui::Shape::line(ground.iter().map(map).collect(), Stroke::new(1.5_f32, Color32::from_rgb(0x4F, 0xA0, 0x62))));
            for s in &shapes {
                let mut pts: Vec<_> = s.iter().map(map).collect();
                if let Some(f) = pts.first().copied() {
                    pts.push(f);
                }
                painter.add(egui::Shape::line(pts, Stroke::new(1.0_f32, Color32::from_rgb(0x7A, 0xB0, 0x9A))));
            }
            for l in &links {
                painter.add(egui::Shape::line(l.iter().map(map).collect(), Stroke::new(1.8_f32, Color32::from_rgb(0xD9, 0x55, 0x4A))));
            }
            painter.text(rect.left_top() + egui::vec2(8.0, 6.0), egui::Align2::LEFT_TOP, format!("Width {:.2} m, to the daylight points", x1 - x0), egui::FontId::proportional(11.0), Color32::from_gray(170));
        }
        Err(e) => say(&e),
    }
}

type Poly = Vec<(f64, f64)>;

fn preview_geometry(a: &Assembly, height: f64, on: Option<(&afrobim_alignment::Alignment, f64)>) -> Result<(Vec<Poly>, Vec<Poly>, Poly), String> {
    use afrobim_alignment::{Alignment, AlignmentElement, Profile, Pvi};
    use afrobim_corridor::{design_section_at, AssemblyRegion, Baseline, Frequency, Tin};
    // Either a straight alignment of 100 m, level at `height`, over ground at zero; or the chosen
    // alignment at the chosen station, over flat ground `height` below its profile there, so that
    // targets that depend on the plan or the station (a table, a plan line, a profile) are read as
    // the corridor would read them.
    let (al, station, ground_z, ground) = match on {
        None => {
            let mut al = Alignment::from_elements("preview", 0.0, vec![AlignmentElement::line(0.0, 0.0, 0.0, 100.0)]).map_err(|e| e.to_string())?;
            al.profile = Profile::new("preview", vec![Pvi::point(0.0, height), Pvi::point(100.0, height)]).ok();
            let g = Tin::from_grid(-100.0, -100.0, 50.0, 50.0, 5, 5, |_, _| 0.0).map_err(|e| e.to_string())?;
            (al, 50.0, 0.0, g)
        }
        Some((al, st)) => {
            let p = al.profile.as_ref().ok_or_else(|| format!("The alignment '{}' has no design profile.", al.name))?;
            let z = p.elevation_at(st).map_err(|_| format!("Station {st:.3} is off the profile."))? - height;
            let pl = al.polyline(5.0);
            let (x0, x1) = pl.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| (lo.min(q.0), hi.max(q.0)));
            let (y0, y1) = pl.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| (lo.min(q.1), hi.max(q.1)));
            let pad = 300.0;
            let g = Tin::from_grid(x0 - pad, y0 - pad, (x1 - x0 + 2.0 * pad) / 4.0, (y1 - y0 + 2.0 * pad) / 4.0, 5, 5, |_, _| z).map_err(|e| e.to_string())?;
            (al.clone(), st, z, g)
        }
    };
    let baseline = Baseline { name: "preview".into(), alignment: al.name.clone(), regions: vec![AssemblyRegion { name: "r".into(), start_station_m: al.start_station_m, end_station_m: al.end_station_m(), assembly: a.clone(), frequency: Frequency::uniform(50.0) }], crossfall_table: vec![] };
    let ms = design_section_at(&baseline, 0, &al, &ground, station).map_err(|e| e.to_string())?;
    let links: Vec<Poly> = ms.design.links.iter().map(|l| l.points.clone()).filter(|p| p.len() >= 2).collect();
    let shapes: Vec<Poly> = ms.design.shapes.iter().map(|s| s.polygon.clone()).collect();
    let xs: Vec<f64> = links.iter().flatten().map(|p| p.0).collect();
    let (lo, hi) = (xs.iter().cloned().fold(f64::INFINITY, f64::min), xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    // Heights are drawn relative to the ground under the centreline.
    let shift = |p: &Poly| -> Poly { p.iter().map(|q| (q.0, q.1 - ground_z)).collect() };
    Ok((links.iter().map(shift).collect(), shapes.iter().map(shift).collect(), vec![(lo - 1.0, 0.0), (hi + 1.0, 0.0)]))
}

#[allow(dead_code)]
fn _unused(_: BTreeMap<u8, u8>) {}
