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

use afrobim_corridor::{library, Assembly, Classification, Layer, Side, Slope, Subassembly};
use afrobim_kernel::{ElementId, SelectMode};
use egui::{pos2, Color32, RichText, Sense, Stroke, Ui};
use serde::{Deserialize, Serialize};

use crate::app::AfroBimApp;
use crate::civil::{self, ASSEMBLY};
use crate::civil_dlg::{self as d};
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

pub const KINDS: [PartKind; 12] = [
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
    /// A part carried unchanged (not made in this editor).
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
            return Ok(r.clone());
        }
        let k = &KINDS[self.kind];
        let mut v = Vec::new();
        for (i, (label, _)) in k.params.iter().enumerate() {
            let t = self.vals.get(i).map(|s| s.trim()).unwrap_or("");
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
            0 => library::lane(name, v[0], slope(v[1]), layers),
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
            _ => library::daylight(v[0], v[1]),
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
}

impl AfroBimApp {
    pub(crate) fn civil_assembly_cmd(&mut self) {
        let sel = self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == ASSEMBLY));
        let form = match sel.and_then(|id| self.doc.find(id).map(|e| (id, e))) {
            Some((id, e)) => {
                let ed = e.properties.get(PROP_EDITOR).and_then(|t| serde_json::from_str::<EdAssembly>(t).ok()).or_else(|| civil::get::<Assembly>(&self.doc, id, ASSEMBLY).map(|a| EdAssembly::from_assembly(&a))).unwrap_or_default();
                AsmForm { id: Some(id), ed, side: 1, sel: 0, add_kind: 0, preview_height: "1".into(), message: String::new() }
            }
            None => AsmForm { id: None, ed: EdAssembly { name: String::new(), ..Default::default() }, side: 1, sel: 0, add_kind: 0, preview_height: "1".into(), message: String::new() },
        };
        self.civil_ui.asm.form = Some(form);
    }

    pub(crate) fn civil_assembly_windows(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.asm.form.take() else { return };
        let side = if f.side == 0 { Side::Left } else { Side::Right };
        let (assembly, problems) = f.ed.build();
        let preview_h = f.preview_height.trim().parse::<f64>().ok();
        let can_save = problems.is_empty() && !f.ed.name.trim().is_empty() && (!f.ed.left.is_empty() || !f.ed.right.is_empty());
        let out = Dialog::new("Assembly editor", crate::glyphs::by_name("ASSEMBLY")).width(860.0).resizable(true).max_body_height(620.0).help("Parts are listed from the centreline outwards on each side. Every dimension is an input: a part is not drawn until its fields are filled. The Example values button fills demonstration numbers that are not from any standard. The preview puts the assembly on level ground below the centreline so the side slopes show; it is a drawing aid, not a design. IFC entity: only IfcPavement (the constant the corridor crate provides) is offered; anything else is free text and is marked unverified.").show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui_kit::text_field(ui, &mut f.ed.name, "e.g. Two lane road");
                    ui_kit::segmented(ui, &mut f.side, &["Left side", "Right side"]);
                    if ui.button("Mirror right to left").clicked() {
                        f.ed.left = f.ed.right.clone();
                    }
                    if ui.button("Mirror left to right").clicked() {
                        f.ed.right = f.ed.left.clone();
                    }
                });
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(420.0);
                        ui.label(RichText::new(format!("Parts, {} side", if f.side == 0 { "left" } else { "right" })).strong());
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
                                if ui.add_enabled(i > 0, egui::Button::new("Up").small()).clicked() {
                                    action = Some((i, -1));
                                }
                                if ui.add_enabled(i + 1 < n, egui::Button::new("Down").small()).clicked() {
                                    action = Some((i, 1));
                                }
                                if ui.small_button("x").clicked() {
                                    action = Some((i, 0));
                                }
                            });
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
                        }
                        ui.horizontal(|ui| {
                            let names: Vec<&str> = KINDS.iter().map(|k| k.name).collect();
                            ui_kit::combo(ui, "asm-kind", &mut f.add_kind, &names);
                            if ui.button("Add part").clicked() {
                                let v = f.ed.side_mut(side);
                                v.push(EdPart::new(f.add_kind));
                                f.sel = v.len() - 1;
                            }
                        });
                        ui.separator();
                        let sel = f.sel;
                        if let Some(p) = f.ed.side_mut(side).get_mut(sel) {
                            part_editor(ui, p);
                        } else {
                            ui.label(RichText::new("Add a part, then select it to enter its dimensions.").weak());
                        }
                    });
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Preview").strong());
                        ui.horizontal(|ui| {
                            ui.label("Centreline above the level ground");
                            d::num(ui, &mut f.preview_height, "m");
                        });
                        preview(ui, &assembly, preview_h);
                        for p in &problems {
                            ui.label(RichText::new(p).small().color(crate::theme::warn()));
                        }
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
            f.message = "Saved in the document.".into();
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

fn part_editor(ui: &mut Ui, p: &mut EdPart) {
    let k = &KINDS[p.kind.min(KINDS.len() - 1)];
    if p.raw.is_some() {
        ui.label(RichText::new(format!("{}: a part carried unchanged from the assembly it came in. It is not editable here; remove it and add a library part to change it.", p.name)).small().weak());
        return;
    }
    ui_kit::form(ui, "asm-part", |fm| {
        fm.row("Name", |ui| d::text(ui, &mut p.name, ""));
        for (i, (label, unit)) in k.params.iter().enumerate() {
            fm.row(label, |ui| {
                if p.vals.len() <= i {
                    p.vals.resize(i + 1, String::new());
                }
                d::num(ui, &mut p.vals[i], unit);
            });
        }
    });
    if k.layers {
        ui.label(RichText::new("Layers, top down").strong());
        let mut remove = None;
        egui::Grid::new("asm-layers").num_columns(4).show(ui, |ui| {
            for h in ["Shape code", "Depth (m)", "Material (free text)", ""] {
                ui.label(RichText::new(h).small().strong());
            }
            ui.end_row();
            for (i, l) in p.layers.iter_mut().enumerate() {
                ui.add(egui::TextEdit::singleline(&mut l.code).desired_width(70.0));
                ui.add(egui::TextEdit::singleline(&mut l.depth).desired_width(60.0));
                ui.add(egui::TextEdit::singleline(&mut l.material).desired_width(130.0));
                if ui.small_button("x").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove {
            p.layers.remove(i);
        }
        if ui.small_button("Add layer").clicked() {
            p.layers.push(EdLayer::default());
        }
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
        ui.label(RichText::new("Example values are for demonstration only.").small().weak());
    });
}

/// The preview: the assembly on level ground, drawn from the engine's own section at a station of
/// a straight made-up alignment. Returns nothing; an assembly that cannot be drawn says why.
fn preview(ui: &mut Ui, a: &Assembly, height: Option<f64>) {
    let size = egui::vec2(430.0, 260.0);
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
    match preview_geometry(a, h) {
        Ok((links, shapes, ground)) => {
            let all: Vec<(f64, f64)> = links.iter().chain(shapes.iter()).chain(std::iter::once(&ground)).flatten().copied().collect();
            let (x0, x1) = all.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.0), b.max(p.0)));
            let (y0, y1) = all.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.1), b.max(p.1)));
            let k = ((rect.width() as f64 - 24.0) / (x1 - x0).max(1e-6)).min((rect.height() as f64 - 24.0) / (y1 - y0).max(1e-6));
            let map = |p: &(f64, f64)| pos2(rect.left() + 12.0 + ((p.0 - x0) * k) as f32, rect.bottom() - 12.0 - ((p.1 - y0) * k) as f32);
            painter.add(egui::Shape::line(ground.iter().map(map).collect(), Stroke::new(1.5, Color32::from_rgb(0x4F, 0xA0, 0x62))));
            for s in &shapes {
                let mut pts: Vec<_> = s.iter().map(map).collect();
                if let Some(f) = pts.first().copied() {
                    pts.push(f);
                }
                painter.add(egui::Shape::line(pts, Stroke::new(1.0, Color32::from_rgb(0x7A, 0xB0, 0x9A))));
            }
            for l in &links {
                painter.add(egui::Shape::line(l.iter().map(map).collect(), Stroke::new(1.8, Color32::from_rgb(0xD9, 0x55, 0x4A))));
            }
            painter.text(rect.left_top() + egui::vec2(8.0, 6.0), egui::Align2::LEFT_TOP, format!("Width {:.2} m, to the daylight points", x1 - x0), egui::FontId::proportional(11.0), Color32::from_gray(170));
        }
        Err(e) => say(&e),
    }
}

type Poly = Vec<(f64, f64)>;

fn preview_geometry(a: &Assembly, height: f64) -> Result<(Vec<Poly>, Vec<Poly>, Poly), String> {
    use afrobim_alignment::{Alignment, AlignmentElement, Profile, Pvi};
    use afrobim_corridor::{design_section_at, AssemblyRegion, Baseline, Frequency, Tin};
    // A straight alignment of 100 m, level at `height`, over ground at zero.
    let al = Alignment::from_elements("preview", 0.0, vec![AlignmentElement::line(0.0, 0.0, 0.0, 100.0)]).map_err(|e| e.to_string())?;
    let mut al = al;
    al.profile = Profile::new("preview", vec![Pvi::point(0.0, height), Pvi::point(100.0, height)]).ok();
    let ground = Tin::from_grid(-100.0, -100.0, 50.0, 50.0, 5, 5, |_, _| 0.0).map_err(|e| e.to_string())?;
    let baseline = Baseline { name: "preview".into(), alignment: "preview".into(), regions: vec![AssemblyRegion { name: "r".into(), start_station_m: 0.0, end_station_m: 100.0, assembly: a.clone(), frequency: Frequency::uniform(50.0) }], crossfall_table: vec![] };
    let ms = design_section_at(&baseline, 0, &al, &ground, 50.0).map_err(|e| e.to_string())?;
    let links: Vec<Poly> = ms.design.links.iter().map(|l| l.points.clone()).filter(|p| p.len() >= 2).collect();
    let shapes: Vec<Poly> = ms.design.shapes.iter().map(|s| s.polygon.clone()).collect();
    let xs: Vec<f64> = links.iter().flatten().map(|p| p.0).collect();
    let (lo, hi) = (xs.iter().cloned().fold(f64::INFINITY, f64::min), xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    Ok((links, shapes, vec![(lo - 1.0, 0.0), (hi + 1.0, 0.0)]))
}

#[allow(dead_code)]
fn _unused(_: BTreeMap<u8, u8>) {}
