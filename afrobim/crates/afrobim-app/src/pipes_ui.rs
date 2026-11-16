//! The pipe network commands: create by picking structure positions, the parts list manager, network
//! properties and rule set, the edit tools, the checks window, the tables, the quantities with trench
//! volumes, and the profile of a run drawn in the model. The calculations are the `afrobim-pipes`
//! engine's; this module is the dialogs around it.

use std::sync::Arc;

use afrobim_corridor::Tin;
use afrobim_defaults::Conditions;
use afrobim_kernel::{ElementId, SelectMode};
use afrobim_pipes::quantities::{PipeLengthRow, StructureCountRow};
use afrobim_pipes::*;
use egui::RichText;

use crate::app::AfroBimApp;
use crate::background::Job;
use crate::civil::{self, PIPEPROFILE};
use crate::civil_dlg::{self as d, fmt};
use crate::civil_pipes::{self as cp, PipeNetData, PipeProfileData};
use crate::ui_kit::{self, Col, Dialog, Footer, Level, NumRule};

/// The labelled example parts list (the pipes crate's test file): its name says what it is.
pub const EXAMPLE_PARTS_CSV: &str = include_str!("../../afrobim-pipes/tests/data/example_parts.csv");
/// The label every example parts list carries.
pub const EXAMPLE_LABEL: &str = "EXAMPLE parts list: example values, not a standard or a manufacturer's catalogue";

pub fn example_catalogue() -> PartsCatalogue {
    let mut c = PartsCatalogue::from_csv(EXAMPLE_LABEL, EXAMPLE_PARTS_CSV).unwrap_or_default();
    c.source = Some("Example file shipped with AfroBIM (afrobim-pipes tests/data/example_parts.csv)".into());
    c
}

#[derive(Default)]
pub struct PipesUi {
    pub new: Option<NewForm>,
    pub parts: Option<PartsForm>,
    pub props: Option<PropsForm>,
    pub edit: Option<EditForm>,
    pub check: Option<CheckForm>,
    pub check_out: Option<PipeCheckOut>,
    pub qty_out: Option<QtyOut>,
    pub tables: Option<TablesForm>,
    pub qty: Option<QtyForm>,
    pub profile: Option<ProfForm>,
    /// The parts list new networks are made with (loaded in the parts manager).
    pub catalogue: PartsCatalogue,
}

const NET_TYPES: [(NetworkType, &str); 4] = [(NetworkType::Storm, "Storm"), (NetworkType::Sanitary, "Sanitary"), (NetworkType::Water, "Water"), (NetworkType::Other, "Other")];
const STATUSES: [(NetworkStatus, &str); 4] = [(NetworkStatus::Proposed, "Proposed"), (NetworkStatus::Existing, "Existing"), (NetworkStatus::Abandoned, "Abandoned"), (NetworkStatus::Destroyed, "Destroyed")];

fn type_index(t: NetworkType) -> usize {
    NET_TYPES.iter().position(|x| x.0 == t).unwrap_or(0)
}
fn status_index(t: NetworkStatus) -> usize {
    STATUSES.iter().position(|x| x.0 == t).unwrap_or(0)
}

fn net_combo(ui: &mut egui::Ui, id: &str, nets: &[(ElementId, String)], sel: &mut Option<ElementId>) -> bool {
    d::pick_from(ui, id, nets, sel)
}

fn structure_items(net: &PipeNetwork) -> Vec<(u32, String)> {
    net.structures().map(|s| (s.id.0, s.name.clone())).collect()
}
fn pipe_items(net: &PipeNetwork) -> Vec<(u32, String)> {
    net.pipes().map(|p| (p.id.0, format!("{}  ({} to {})", p.name, net.structure(p.start).map(|s| s.name.clone()).unwrap_or_default(), net.structure(p.end).map(|s| s.name.clone()).unwrap_or_default()))).collect()
}

fn id_combo(ui: &mut egui::Ui, id: &str, items: &[(u32, String)], sel: &mut Option<u32>) {
    let shown = sel.and_then(|s| items.iter().find(|i| i.0 == s)).map(|i| i.1.clone()).unwrap_or_else(|| "(choose)".into());
    egui::ComboBox::from_id_salt(("pipes-id", id)).width(260.0).selected_text(shown).show_ui(ui, |ui| {
        for (i, n) in items {
            if ui.selectable_label(*sel == Some(*i), n).clicked() {
                *sel = Some(*i);
            }
        }
    });
}

fn parts_combo(ui: &mut egui::Ui, id: &str, cat: &PartsCatalogue, family: PartFamily, sel: &mut usize) {
    let parts: Vec<&Part> = cat.parts.iter().filter(|p| p.family == family).collect();
    let labels: Vec<String> = parts.iter().map(|p| format!("{}  {}", p.id, p.name)).collect();
    if labels.is_empty() {
        ui.label(RichText::new("(no parts of this family in the list)").weak());
        return;
    }
    *sel = (*sel).min(labels.len() - 1);
    ui_kit::combo_owned(ui, id, sel, &labels);
}

fn nth_part(cat: &PartsCatalogue, family: PartFamily, i: usize) -> Option<Part> {
    cat.parts.iter().filter(|p| p.family == family).nth(i).cloned()
}

// ─── CREATE ─────────────────────────────────────────────────────────────────

pub struct NewForm {
    name: String,
    net_type: usize,
    surface: Option<ElementId>,
    /// Typed rows: easting, northing, rim level (blank: read from the surface).
    rows: Vec<(String, String, String)>,
    use_parts: bool,
    pipe_part: usize,
    struct_part: usize,
    kind: usize,
    diameter: String,
    /// 0 carrier drain, 1 combined drain: the roughness default's condition.
    drain: usize,
    ks_mm: String,
    start_invert: String,
    slope_pct: String,
    message: String,
}

impl AfroBimApp {
    pub(crate) fn pipe_new_cmd(&mut self) {
        let surf = civil::surface_choices(&self.doc).first().map(|s| s.0);
        let example = self.options.civil_demo;
        self.civil_ui.pipes.new = Some(NewForm { name: String::new(), net_type: 0, surface: surf, rows: vec![(String::new(), String::new(), String::new()); 2], use_parts: !self.civil_ui.pipes.catalogue.parts.is_empty() || example, pipe_part: 0, struct_part: 0, kind: 0, diameter: String::new(), drain: 0, ks_mm: String::new(), start_invert: String::new(), slope_pct: String::new(), message: String::new() });
        if example && self.civil_ui.pipes.catalogue.parts.is_empty() {
            self.civil_ui.pipes.catalogue = example_catalogue();
        }
    }

    fn pipe_new_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.pipes.new.take() else { return };
        // Positions picked in the model arrive in the queue.
        for (e, n) in std::mem::take(&mut self.civil_ui.queue) {
            let z = f.surface.and_then(|s| self.tin_for(s)).and_then(|t| t.elevation_at(e, n));
            let row = (format!("{e:.3}"), format!("{n:.3}"), z.map(|z| format!("{z:.3}")).unwrap_or_default());
            match f.rows.iter().position(|r| r.0.trim().is_empty() && r.1.trim().is_empty()) {
                Some(i) => f.rows[i] = row,
                None => f.rows.push(row),
            }
        }
        let surfaces = civil::surface_choices(&self.doc);
        let cat = self.civil_ui.pipes.catalogue.clone();
        let has_parts = !cat.parts.is_empty();
        let dia_rule = NumRule::required().above(0.0);
        let mut pick = false;
        let mut go = false;
        let busy_pick = self.civil_ui.pick.is_some();
        let ready = f.rows.iter().filter(|r| !r.0.trim().is_empty()).count() >= 2;
        let out = Dialog::new("Create pipe network", crate::glyphs::by_name("PIPENEW")).width(700.0).resizable(true).max_body_height(600.0).help("Click the structure positions in the model, in order from upstream to downstream, or type them. Each pair of structures is joined by a pipe. The parts list gives the pipe and structure sizes; without one you type the diameter. The start invert and the slope are inputs.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "pipe-new", |fm| {
                    fm.row("Name", |ui| d::text(ui, &mut f.name, "e.g. Storm drain A"));
                    fm.row("Type", |ui| {
                        ui_kit::combo(ui, "pn-type", &mut f.net_type, &NET_TYPES.map(|t| t.1));
                    });
                    fm.row_note("Ground surface", "Gives the rim levels of picked structures", |ui| {
                        d::pick_from(ui, "pn-surf", &surfaces, &mut f.surface);
                    });
                });
                ui_kit::section(ui, "Parts", |ui| {
                    ui_kit::toggle(ui, &mut f.use_parts, "Use the parts list");
                    if f.use_parts {
                        if !has_parts {
                            ui_kit::callout(ui, Level::Info, "No parts list is loaded. Open PIPEPARTS to load a CSV or JSON list, or the labelled example list.");
                        } else {
                            ui.label(RichText::new(&cat.name).small().weak());
                            ui_kit::form(ui, "pn-parts", |fm| {
                                fm.row("Pipe part", |ui| parts_combo(ui, "pn-pp", &cat, PartFamily::Pipe, &mut f.pipe_part));
                                fm.row("Structure part", |ui| parts_combo(ui, "pn-sp", &cat, PartFamily::Structure, &mut f.struct_part));
                            });
                        }
                    }
                    if !f.use_parts || !has_parts {
                        ui_kit::form(ui, "pn-manual", |fm| {
                            fm.row("Structure kind", |ui| {
                                ui_kit::combo(ui, "pn-kind", &mut f.kind, &StructureKind::ALL.map(|k| k.label()));
                            });
                            fm.row("Pipe diameter", |ui| {
                                d::num_rule(ui, &mut f.diameter, "mm", dia_rule.clone());
                            });
                            fm.row_note("Roughness", "Colebrook-White ks from the defaults basis", |ui| {
                                ui.vertical(|ui| {
                                    ui_kit::segmented(ui, &mut f.drain, &["Carrier drain", "Combined drain"]);
                                    let key = if f.drain == 0 { "drainage.roughness.kc_carrier_drain" } else { "drainage.roughness.kc_combined_drain" };
                                    crate::defaults_glue::field(ui, "pn-ks", &mut f.ks_mm, "mm", key, &Conditions::none(), 1.0, &NumRule::default().above(0.0));
                                });
                            });
                        });
                    }
                });
                ui_kit::section(ui, "Inverts", |ui| {
                    ui_kit::form(ui, "pn-inv", |fm| {
                        fm.row("Start invert", |ui| {
                            d::num(ui, &mut f.start_invert, "m");
                        });
                        fm.row_note("Slope", "Falling downstream; the same for every pipe of the run", |ui| {
                            d::num_rule(ui, &mut f.slope_pct, "%", NumRule::required().above(0.0));
                        });
                    });
                });
                ui_kit::section(ui, "Structure positions, upstream first", |ui| {
                    egui::Grid::new("pn-rows").num_columns(5).spacing([6.0, 4.0]).show(ui, |ui| {
                        for h in ["#", "Easting (m)", "Northing (m)", "Rim level (m)", ""] {
                            ui.label(RichText::new(h).small().strong());
                        }
                        ui.end_row();
                        let mut remove = None;
                        for (i, r) in f.rows.iter_mut().enumerate() {
                            ui.label((i + 1).to_string());
                            ui.add(egui::TextEdit::singleline(&mut r.0).desired_width(100.0));
                            ui.add(egui::TextEdit::singleline(&mut r.1).desired_width(100.0));
                            ui.add(egui::TextEdit::singleline(&mut r.2).desired_width(80.0).hint_text("from surface"));
                            if ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove this structure").clicked() {
                                remove = Some(i);
                            }
                            ui.end_row();
                        }
                        if let Some(i) = remove {
                            f.rows.remove(i);
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Add a row").clicked() {
                            f.rows.push(Default::default());
                        }
                        if ui.selectable_label(busy_pick, "Pick in the model").on_hover_text("Each click adds a structure; press Escape or close the window to stop").clicked() {
                            pick = true;
                        }
                    });
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new("Create network").icon(crate::glyphs::by_name("PIPENEW")).enabled(ready, "Give at least two structure positions").show(ui);
                go = o.primary;
                o
            },
        );
        if pick {
            self.civil_ui.pick = Some(crate::civil_ui::PickFor::Queue);
            self.civil_ui.pick_repeat = true;
            self.status = "Click the structure positions in the model, upstream first.".into();
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            self.civil_ui.pick = None;
            return;
        }
        if go {
            match self.pipe_create(&f, &cat) {
                Ok(()) => {
                    self.civil_ui.pick = None;
                    return;
                }
                Err(e) => f.message = e,
            }
        }
        self.civil_ui.pipes.new = Some(f);
    }

    fn pipe_create(&mut self, f: &NewForm, cat: &PartsCatalogue) -> Result<(), String> {
        let num = |what: &str, t: &str| t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("{what} is not a number."));
        let start_invert = num("The start invert", &f.start_invert)?;
        let slope = num("The slope", &f.slope_pct)? / 100.0;
        let mut points = Vec::new();
        for (i, r) in f.rows.iter().enumerate().filter(|(_, r)| !r.0.trim().is_empty() || !r.1.trim().is_empty()) {
            let (e, n) = (num(&format!("Easting of row {}", i + 1), &r.0)?, num(&format!("Northing of row {}", i + 1), &r.1)?);
            let rim = if r.2.trim().is_empty() {
                f.surface.and_then(|s| self.tin_for(s)).and_then(|t| t.elevation_at(e, n)).ok_or_else(|| format!("Row {} has no rim level and the ground surface does not cover it.", i + 1))?
            } else {
                num(&format!("Rim level of row {}", i + 1), &r.2)?
            };
            points.push(RunPoint { easting_m: e, northing_m: n, rim_level_m: rim });
        }
        if points.len() < 2 {
            return Err("Give at least two structure positions.".into());
        }
        let (pipe_spec, kind, struct_part) = if f.use_parts && !cat.parts.is_empty() {
            let part = nth_part(cat, PartFamily::Pipe, f.pipe_part).ok_or("The parts list has no pipe part.")?;
            let sp = nth_part(cat, PartFamily::Structure, f.struct_part);
            let kind = sp.as_ref().and_then(|p| p.structure_kind).unwrap_or(StructureKind::ALL[f.kind.min(5)]);
            (PipeSpec::from_part(&part).map_err(|e| e.to_string())?, kind, sp)
        } else {
            let dia = num("The pipe diameter", &f.diameter)? / 1000.0;
            if !(dia > 0.0) {
                return Err("The pipe diameter must be greater than zero.".into());
            }
            let mut spec = PipeSpec::new(PipeShape::Circular { diameter_m: dia });
            if !f.ks_mm.trim().is_empty() {
                spec.roughness = Some(Roughness::ColebrookWhite { ks_m: num("The roughness ks", &f.ks_mm)? / 1000.0 });
            }
            (spec, StructureKind::ALL[f.kind.min(5)], None)
        };
        let mut data = PipeNetData::default();
        let name = if f.name.trim().is_empty() { "Pipe network".to_string() } else { f.name.trim().to_string() };
        data.network = PipeNetwork::new(&name);
        data.network.network_type = NET_TYPES[f.net_type.min(3)].0;
        data.catalogue = cat.clone();
        data.surface = f.surface.map(|s| s.0);
        data.network.surface_name = f.surface.and_then(|s| self.doc.find(s).map(|e| e.name.clone()));
        let spec = RunSpec { structure_kind: kind, structure_prefix: "S".into(), pipe_prefix: "P".into(), pipe: pipe_spec, start_invert_m: start_invert, slope };
        let (structs, _) = data.network.add_run(&points, &spec).map_err(|e| e.to_string())?;
        if let Some(sp) = &struct_part {
            for s in structs {
                data.network.swap_structure_part(s, sp).map_err(|e| e.to_string())?;
            }
        }
        let id = cp::add(&mut self.doc, &name, &data, "Picked structure positions");
        self.doc.select(&[id], SelectMode::Replace);
        self.status = format!("Created pipe network '{name}': {} structures, {} pipes.", data.network.structure_count(), data.network.pipe_count());
        self.schedule_commit_now();
        Ok(())
    }
}

// ─── PARTS ──────────────────────────────────────────────────────────────────

pub struct PartsForm {
    /// None: the list new networks are made with; Some: the list of that network.
    target: Option<ElementId>,
    message: String,
}

impl AfroBimApp {
    pub(crate) fn pipe_parts_cmd(&mut self) {
        let target = self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == civil::PIPENET));
        self.civil_ui.pipes.parts = Some(PartsForm { target, message: String::new() });
    }

    fn pipe_parts_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.pipes.parts.take() else { return };
        let nets = cp::networks(&self.doc);
        let mut cat = match f.target.and_then(|t| cp::data_of(&self.doc, t)) {
            Some(d) => d.catalogue,
            None => self.civil_ui.pipes.catalogue.clone(),
        };
        let before = cat.clone();
        let mut action = 0u8;
        let mut remove = None;
        let out = Dialog::new("Parts list", crate::glyphs::by_name("PIPEPARTS")).width(900.0).resizable(true).max_body_height(600.0).help("The parts list gives pipe and structure sizes, materials, wall thicknesses and roughness. Load a CSV or JSON file, or the labelled example list. Nothing is built in: the crate holds no pipe size or roughness.").show_body(
            ctx,
            |ui| {
                ui_kit::form(ui, "parts-head", |fm| {
                    fm.row_note("List of", "The list new networks use, or the list kept in a network", |ui| {
                        let mut items = vec![(ElementId(u64::MAX), "New networks (the working list)".to_string())];
                        items.extend(nets.iter().cloned());
                        let mut sel = Some(f.target.unwrap_or(ElementId(u64::MAX)));
                        if net_combo(ui, "parts-target", &items, &mut sel) {
                            f.target = sel.filter(|s| s.0 != u64::MAX);
                        }
                    });
                });
                ui.horizontal(|ui| {
                    if ui.button("Load CSV or JSON...").clicked() {
                        action = 1;
                    }
                    if ui.button("Load the example list").on_hover_text("Example values for trying the tools. Not a standard, not a catalogue.").clicked() {
                        action = 2;
                    }
                    if ui.button("Export CSV...").clicked() {
                        action = 3;
                    }
                    if ui.button("Clear").clicked() {
                        action = 4;
                    }
                });
                if cat.parts.is_empty() {
                    ui_kit::callout(ui, Level::Info, "The list is empty. Pipes and structures are then typed in.");
                } else {
                    ui.label(RichText::new(format!("{}  ({} parts)", cat.name, cat.parts.len())).strong());
                    if let Some(s) = &cat.source {
                        ui.label(RichText::new(s).small().weak());
                    }
                    if cat.name.to_uppercase().contains("EXAMPLE") {
                        ui_kit::callout(ui, Level::Warning, "This is an example list: its sizes and roughness values are examples only.");
                    }
                    if let Err(e) = cat.validate() {
                        ui_kit::callout(ui, Level::Error, e.to_string());
                    }
                    egui::ScrollArea::both().max_height(360.0).show(ui, |ui| {
                        egui::Grid::new("parts-table").striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                            for h in ["Id", "Name", "Family", "Kind or shape", "Size (m)", "Material", "Wall (m)", "Roughness", "IFC entity", "Cost code", ""] {
                                ui_kit::th(ui, h);
                            }
                            ui.end_row();
                            for (i, p) in cat.parts.iter().enumerate() {
                                ui.label(&p.id);
                                ui.label(&p.name);
                                ui.label(if p.family == PartFamily::Pipe { "pipe" } else { "structure" });
                                ui.label(match (&p.pipe_shape, p.structure_kind) {
                                    (Some(s), _) => format!("{s:?}").split(' ').next().unwrap_or("").to_lowercase(),
                                    (None, Some(k)) => k.label().to_string(),
                                    _ => String::new(),
                                });
                                ui.label(match (&p.pipe_shape, &p.structure_inner) {
                                    (Some(s), _) => s.size_label(),
                                    (None, Some(StructureInner::Circular { diameter_m })) => format!("{diameter_m}"),
                                    (None, Some(StructureInner::Rectangular { length_m, width_m })) => format!("{length_m} x {width_m}"),
                                    _ => String::new(),
                                });
                                ui.label(p.material.clone().unwrap_or_default());
                                ui.label(p.wall_thickness_m.map(|w| w.to_string()).unwrap_or_default());
                                ui.label(match p.roughness {
                                    Some(Roughness::Manning { n }) => format!("n {n}"),
                                    Some(Roughness::ColebrookWhite { ks_m }) => format!("ks {ks_m} m"),
                                    None => String::new(),
                                });
                                ui.label(p.ifc_entity.clone().unwrap_or_default());
                                ui.label(p.cost_item_code.clone().unwrap_or_default());
                                if ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove this part").clicked() {
                                    remove = Some(i);
                                }
                                ui.end_row();
                            }
                        });
                    });
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
        );
        if let Some(i) = remove {
            cat.parts.remove(i);
        }
        match action {
            1 => {
                if let Some(p) = rfd::FileDialog::new().add_filter("Parts list", &["csv", "json"]).pick_file() {
                    match std::fs::read_to_string(&p) {
                        Ok(text) => {
                            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            let r = if name.to_lowercase().ends_with(".json") { PartsCatalogue::from_json(&text) } else { PartsCatalogue::from_csv(&name, &text) };
                            match r {
                                Ok(mut c) => {
                                    c.source = Some(p.display().to_string());
                                    f.message = format!("Loaded {} parts from {name}.", c.parts.len());
                                    cat = c;
                                }
                                Err(e) => f.message = format!("The parts list cannot be read: {e}"),
                            }
                        }
                        Err(e) => f.message = format!("The file cannot be read: {e}"),
                    }
                }
            }
            2 => {
                cat = example_catalogue();
                f.message = "Loaded the example list (example values).".into();
            }
            3 => {
                if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("parts.csv").save_file() {
                    f.message = match std::fs::write(&p, cat.to_csv()) {
                        Ok(()) => format!("Wrote {}.", p.display()),
                        Err(e) => format!("The file could not be written: {e}"),
                    };
                }
            }
            4 => cat = PartsCatalogue::default(),
            _ => {}
        }
        if cat != before {
            match f.target {
                Some(t) => {
                    if let Some(mut data) = cp::data_of(&self.doc, t) {
                        data.catalogue = cat.clone();
                        cp::store(&mut self.doc, t, &data);
                        self.schedule_commit_now();
                    }
                }
                None => self.civil_ui.pipes.catalogue = cat,
            }
        }
        if out.closed {
            return;
        }
        self.civil_ui.pipes.parts = Some(f);
    }
}

// ─── PROPERTIES AND RULES ───────────────────────────────────────────────────

pub struct PropsForm {
    net: Option<ElementId>,
    loaded: Option<ElementId>,
    name: String,
    net_type: usize,
    status: usize,
    surface: Option<ElementId>,
    alignment: String,
    partners: Vec<ElementId>,
    tailwater: String,
    viscosity: String,
    clearance: String,
    cover_spacing: String,
    min_slope: String,
    max_slope: String,
    min_cover: String,
    max_cover: String,
    max_length: String,
    drop: String,
    mode: usize,
    min_velocity: String,
    max_velocity: String,
    max_depth_ratio: String,
    /// The defaults basis condition: 0 urban design, 1 rural design, 2 urban deteriorated, 3 rural deteriorated.
    setting: usize,
    diameter_idx: usize,
    message: String,
}

fn opt_text(v: Option<f64>) -> String {
    v.map(afrobim_defaults::fmt_num).unwrap_or_default()
}

impl PropsForm {
    fn load(&mut self, doc: &afrobim_kernel::Document, id: ElementId) {
        let Some(data) = cp::data_of(doc, id) else { return };
        let n = &data.network;
        let r = &data.rules;
        self.loaded = Some(id);
        self.name = doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
        self.net_type = type_index(n.network_type);
        self.status = status_index(n.status);
        self.surface = data.surface.map(ElementId);
        self.alignment = n.alignment_name.clone().unwrap_or_default();
        self.partners = data.partners.iter().map(|p| ElementId(*p)).collect();
        self.tailwater = opt_text(data.tailwater_m);
        self.viscosity = opt_text(data.viscosity_m2_s);
        self.clearance = opt_text(data.clearance_m);
        self.cover_spacing = opt_text(data.cover_spacing_m);
        self.min_slope = r.min_slope.map(|s| afrobim_defaults::fmt_num(1.0 / s)).unwrap_or_default();
        self.max_slope = opt_text(r.max_slope.map(|s| s * 100.0));
        self.min_cover = opt_text(r.min_cover_m);
        self.max_cover = opt_text(r.max_cover_m);
        self.max_length = opt_text(r.max_pipe_length_m);
        self.drop = opt_text(r.structure_drop_m);
        self.mode = match r.match_mode { None => 0, Some(MatchMode::Crown) => 1, Some(MatchMode::Invert) => 2, Some(MatchMode::Centreline) => 3 };
        self.min_velocity = opt_text(r.min_velocity_m_s);
        self.max_velocity = opt_text(r.max_velocity_m_s);
        self.max_depth_ratio = opt_text(r.max_depth_ratio);
    }
}

impl AfroBimApp {
    pub(crate) fn pipe_props_cmd(&mut self) {
        let nets = cp::networks(&self.doc);
        if nets.is_empty() {
            self.status = "There is no pipe network yet: make one with PIPENEW first.".into();
            return;
        }
        let net = self.doc.primary().filter(|id| nets.iter().any(|n| n.0 == *id)).or_else(|| nets.first().map(|n| n.0));
        let mut f = PropsForm { net, loaded: None, name: String::new(), net_type: 0, status: 0, surface: None, alignment: String::new(), partners: vec![], tailwater: String::new(), viscosity: String::new(), clearance: String::new(), cover_spacing: String::new(), min_slope: String::new(), max_slope: String::new(), min_cover: String::new(), max_cover: String::new(), max_length: String::new(), drop: String::new(), mode: 0, min_velocity: String::new(), max_velocity: String::new(), max_depth_ratio: String::new(), setting: 0, diameter_idx: 0, message: String::new() };
        if let Some(id) = net {
            f.load(&self.doc, id);
        }
        self.civil_ui.pipes.props = Some(f);
    }

    fn pipe_props_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.pipes.props.take() else { return };
        let nets = cp::networks(&self.doc);
        if f.net != f.loaded {
            if let Some(id) = f.net {
                f.load(&self.doc, id);
            }
        }
        let surfaces = civil::surface_choices(&self.doc);
        let aligns: Vec<String> = crate::alignment_glue::alignments(&self.doc).into_iter().filter_map(|(id, _)| crate::alignment_glue::alignment_of(&self.doc, id).map(|a| a.name)).collect();
        let diameters = {
            let keys = ["drainage.velocity.min_urban_design", "drainage.velocity.min_rural_design", "drainage.velocity.min_urban_deteriorated", "drainage.velocity.min_rural_deteriorated"];
            let key = keys[f.setting.min(3)];
            crate::defaults_glue::with(|d| d.all().iter().find(|e| e.key == key).and_then(|e| e.table.as_ref()).map(|t| t.rows.iter().filter_map(|r| if let afrobim_defaults::At::Num(n) = r.at { Some(n) } else { None }).collect::<Vec<f64>>()).unwrap_or_default())
        };
        let mut apply = false;
        let has_net = f.net.is_some();
        let pos = NumRule::default().above(0.0);
        let out = Dialog::new("Pipe network properties and rules", crate::glyphs::by_name("PIPEPROPS")).width(720.0).resizable(true).max_body_height(620.0).help("The rule set is checked by PIPECHECK; a blank rule is off and is not checked. Where the defaults basis holds a value for the condition you choose, it is filled in with its citation; the other rules have no default and stay empty.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "pp-head", |fm| {
                    fm.row("Network", |ui| {
                        net_combo(ui, "pp-net", &nets, &mut f.net);
                    });
                    fm.row("Name", |ui| d::text(ui, &mut f.name, ""));
                    fm.row("Type", |ui| {
                        ui_kit::combo(ui, "pp-type", &mut f.net_type, &NET_TYPES.map(|t| t.1));
                    });
                    fm.row("Status", |ui| {
                        ui_kit::combo(ui, "pp-status", &mut f.status, &STATUSES.map(|t| t.1));
                    });
                    fm.row_note("Ground surface", "Cover, trenches and the profile read it", |ui| {
                        d::pick_from(ui, "pp-surf", &surfaces, &mut f.surface);
                    });
                    fm.row_note("Alignment", "Profiles and sections are projected onto it; blank draws along the run", |ui| {
                        let mut i = aligns.iter().position(|a| *a == f.alignment).map(|i| i + 1).unwrap_or(0);
                        let mut labels = vec!["(none)".to_string()];
                        labels.extend(aligns.iter().cloned());
                        if ui_kit::combo_owned(ui, "pp-align", &mut i, &labels) {
                            f.alignment = if i == 0 { String::new() } else { aligns[i - 1].clone() };
                        }
                    });
                });
                ui_kit::section(ui, "Inputs of the checks", |ui| {
                    ui_kit::form(ui, "pp-inputs", |fm| {
                        fm.row_note("Tailwater level", "Starts the grade lines; blank: no grade lines", |ui| {
                            d::num_opt(ui, &mut f.tailwater, "m").ok();
                        });
                        fm.row_note("Kinematic viscosity", "Needed for Colebrook-White roughness; no default", |ui| {
                            d::num_opt_rule(ui, &mut f.viscosity, "m2/s", &pos).ok();
                        });
                        fm.row_note("Interference clearance", "Blank: interference is not checked", |ui| {
                            d::num_opt_rule(ui, &mut f.clearance, "m", &NumRule::default().min(0.0)).ok();
                        });
                        fm.row_note("Cover sample spacing", "Blank: the ends of each pipe only", |ui| {
                            d::num_opt_rule(ui, &mut f.cover_spacing, "m", &pos).ok();
                        });
                    });
                    let others: Vec<(ElementId, String)> = nets.iter().filter(|n| Some(n.0) != f.net).cloned().collect();
                    if !others.is_empty() {
                        ui.label(RichText::new("Compare with these networks for interference").small().weak());
                        for (id, n) in others {
                            let mut on = f.partners.contains(&id);
                            if ui_kit::checkbox(ui, &mut on, &n).changed() {
                                if on {
                                    f.partners.push(id);
                                } else {
                                    f.partners.retain(|p| *p != id);
                                }
                            }
                        }
                    }
                });
                ui_kit::section(ui, "Defaults basis for the rules", |ui| {
                    ui_kit::form(ui, "pp-cond", |fm| {
                        fm.row("Setting", |ui| {
                            ui_kit::combo(ui, "pp-setting", &mut f.setting, &["Urban, design", "Rural, design", "Urban, deteriorated", "Rural, deteriorated"]);
                        });
                        fm.row("Pipe diameter", |ui| {
                            if diameters.is_empty() {
                                ui.label("(no table in the basis)");
                            } else {
                                f.diameter_idx = f.diameter_idx.min(diameters.len() - 1);
                                let labels: Vec<String> = diameters.iter().map(|v| format!("{} mm", afrobim_defaults::fmt_num(*v))).collect();
                                ui_kit::combo_owned(ui, "pp-dia", &mut f.diameter_idx, &labels);
                            }
                        });
                    });
                });
                ui_kit::section(ui, "Rule set (blank is off)", |ui| {
                    let setting = ["urban_design", "rural_design", "urban_deteriorated", "rural_deteriorated"][f.setting.min(3)];
                    let cond = diameters.get(f.diameter_idx).map(|v| Conditions::none().num("pipe_diameter_mm", *v));
                    ui_kit::form(ui, "pp-rules", |fm| {
                        fm.row_note("Minimum gradient", "1 in n", |ui| {
                            ui.vertical(|ui| match &cond {
                                Some(c) => {
                                    crate::defaults_glue::field(ui, ("pp-mg", f.net), &mut f.min_slope, "1:n", &format!("drainage.gradient.min_{setting}"), c, 1.0, &pos);
                                }
                                None => {
                                    d::num_opt_rule(ui, &mut f.min_slope, "1:n", &pos).ok();
                                }
                            });
                        });
                        fm.row_note("Minimum velocity", "", |ui| {
                            ui.vertical(|ui| match &cond {
                                Some(c) => {
                                    crate::defaults_glue::field(ui, ("pp-mv", f.net), &mut f.min_velocity, "m/s", &format!("drainage.velocity.min_{setting}"), c, 1.0, &pos);
                                }
                                None => {
                                    d::num_opt_rule(ui, &mut f.min_velocity, "m/s", &pos).ok();
                                }
                            });
                        });
                        for (label, field, unit, key) in [("Maximum slope", &mut f.max_slope, "%", ""), ("Minimum cover", &mut f.min_cover, "m", "drainage.pipe.min_cover"), ("Maximum cover", &mut f.max_cover, "m", ""), ("Maximum pipe length", &mut f.max_length, "m", "drainage.chamber.max_spacing"), ("Drop across a structure", &mut f.drop, "m", ""), ("Maximum velocity", &mut f.max_velocity, "m/s", "drainage.pipe.max_velocity"), ("Maximum depth ratio", &mut f.max_depth_ratio, "", "")] {
                            fm.row_note(label, if key.is_empty() { "No default in the basis: enter a value or leave blank" } else { "No default in the basis (looked for, not found): enter a value or leave blank" }, |ui| {
                                d::num_opt_rule(ui, field, unit, &NumRule::default().min(0.0)).ok();
                            });
                        }
                        fm.row("Match pipes at a structure", |ui| {
                            ui_kit::combo(ui, "pp-mode", &mut f.mode, &["(not set)", "Crown", "Invert", "Centreline"]);
                        });
                    });
                    ui_kit::note(ui, "Cover is to the outside top of the barrel. A rule that is blank is not checked and the check says it was skipped.");
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new("Apply").icon(crate::glyphs::by_name("PIPEPROPS")).enabled(has_net, "Choose a network").show(ui);
                apply = o.primary;
                o
            },
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if apply {
            match self.pipe_props_apply(&f) {
                Ok(()) => return,
                Err(e) => f.message = e,
            }
        }
        self.civil_ui.pipes.props = Some(f);
    }

    fn pipe_props_apply(&mut self, f: &PropsForm) -> Result<(), String> {
        let id = f.net.ok_or("Choose a network.")?;
        let mut data = cp::data_of(&self.doc, id).ok_or("The network cannot be read.")?;
        let opt = |what: &str, t: &str| -> Result<Option<f64>, String> {
            if t.trim().is_empty() {
                return Ok(None);
            }
            t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite()).map(Some).ok_or_else(|| format!("{what} is not a number."))
        };
        data.tailwater_m = opt("The tailwater level", &f.tailwater)?;
        data.viscosity_m2_s = opt("The viscosity", &f.viscosity)?;
        data.clearance_m = opt("The clearance", &f.clearance)?;
        data.cover_spacing_m = opt("The cover spacing", &f.cover_spacing)?;
        let mut r = RuleSet { name: data.rules.name.clone(), ..Default::default() };
        r.min_slope = opt("The minimum gradient", &f.min_slope)?.map(|n| 1.0 / n);
        r.max_slope = opt("The maximum slope", &f.max_slope)?.map(|p| p / 100.0);
        r.min_cover_m = opt("The minimum cover", &f.min_cover)?;
        r.max_cover_m = opt("The maximum cover", &f.max_cover)?;
        r.max_pipe_length_m = opt("The maximum pipe length", &f.max_length)?;
        r.structure_drop_m = opt("The drop", &f.drop)?;
        r.min_velocity_m_s = opt("The minimum velocity", &f.min_velocity)?;
        r.max_velocity_m_s = opt("The maximum velocity", &f.max_velocity)?;
        r.max_depth_ratio = opt("The depth ratio", &f.max_depth_ratio)?;
        r.match_mode = [None, Some(MatchMode::Crown), Some(MatchMode::Invert), Some(MatchMode::Centreline)][f.mode.min(3)];
        data.rules = r;
        data.surface = f.surface.map(|s| s.0);
        data.network.surface_name = f.surface.and_then(|s| self.doc.find(s).map(|e| e.name.clone()));
        data.network.alignment_name = (!f.alignment.is_empty()).then(|| f.alignment.clone());
        data.network.network_type = NET_TYPES[f.net_type.min(3)].0;
        data.network.status = STATUSES[f.status.min(3)].0;
        data.partners = f.partners.iter().map(|p| p.0).collect();
        if !f.name.trim().is_empty() {
            data.network.name = f.name.trim().to_string();
        }
        cp::store(&mut self.doc, id, &data);
        if let Some(e) = self.doc.find_mut(id) {
            if !f.name.trim().is_empty() {
                e.name = f.name.trim().to_string();
            }
        }
        self.status = "Pipe network properties and rules saved.".into();
        self.schedule_commit_now();
        Ok(())
    }
}

// ─── EDIT TOOLS ─────────────────────────────────────────────────────────────

pub struct EditForm {
    net: Option<ElementId>,
    tool: usize,
    pipe: Option<u32>,
    structure: Option<u32>,
    name: String,
    kind: usize,
    e: String,
    n: String,
    rim: String,
    at_chainage: String,
    merge: bool,
    part: usize,
    inv_mode: usize,
    anchor_end: usize,
    anchor_invert: String,
    slope_pct: String,
    drop: String,
    match_mode: usize,
    cover: String,
    flow: String,
    inflow: String,
    loss_k: String,
    sump: String,
    message: String,
}

const EDIT_TOOLS: [&str; 6] = ["Insert structure", "Move structure", "Delete structure", "Swap part", "Set inverts", "Properties"];

impl AfroBimApp {
    pub(crate) fn pipe_edit_cmd(&mut self) {
        let nets = cp::networks(&self.doc);
        if nets.is_empty() {
            self.status = "There is no pipe network yet: make one with PIPENEW first.".into();
            return;
        }
        let net = self.doc.primary().filter(|id| nets.iter().any(|n| n.0 == *id)).or_else(|| nets.first().map(|n| n.0));
        self.civil_ui.pipes.edit = Some(EditForm { net, tool: 0, pipe: None, structure: None, name: String::new(), kind: 0, e: String::new(), n: String::new(), rim: String::new(), at_chainage: String::new(), merge: false, part: 0, inv_mode: 0, anchor_end: 0, anchor_invert: String::new(), slope_pct: String::new(), drop: String::new(), match_mode: 0, cover: String::new(), flow: String::new(), inflow: String::new(), loss_k: String::new(), sump: String::new(), message: String::new() });
    }

    fn pipe_edit_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.pipes.edit.take() else { return };
        if let Some((e, n)) = std::mem::take(&mut self.civil_ui.queue).pop() {
            f.e = format!("{e:.3}");
            f.n = format!("{n:.3}");
        }
        let nets = cp::networks(&self.doc);
        let data = f.net.and_then(|id| cp::data_of(&self.doc, id));
        let structures = data.as_ref().map(|d| structure_items(&d.network)).unwrap_or_default();
        let pipes = data.as_ref().map(|d| pipe_items(&d.network)).unwrap_or_default();
        let cat = data.as_ref().map(|d| d.catalogue.clone()).unwrap_or_default();
        let mut apply = false;
        let has_net = f.net.is_some();
        let mut pick = false;
        let tool = f.tool;
        let out = Dialog::new("Edit pipe network", crate::glyphs::by_name("PIPEEDIT")).width(660.0).resizable(true).max_body_height(560.0).help("Each tool changes the network at once and can be undone with UNDO. Moving a structure or swapping a part leaves the inverts alone, so slopes and crowns change: set the inverts again afterwards.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "pe-head", |fm| {
                    fm.row("Network", |ui| {
                        net_combo(ui, "pe-net", &nets, &mut f.net);
                    });
                });
                ui_kit::tabs(ui, &mut f.tool, &EDIT_TOOLS);
                ui_kit::form(ui, "pe-tool", |fm| match tool {
                    0 => {
                        fm.row("Split the pipe", |ui| id_combo(ui, "pe-ins-pipe", &pipes, &mut f.pipe));
                        fm.row("Structure name", |ui| d::text(ui, &mut f.name, "blank: numbered"));
                        fm.row("Kind", |ui| {
                            ui_kit::combo(ui, "pe-kind", &mut f.kind, &StructureKind::ALL.map(|k| k.label()));
                        });
                        fm.row_note("Position", "Easting and northing, or pick in the model", |ui| {
                            ui.horizontal(|ui| {
                                d::text(ui, &mut f.e, "easting");
                                d::text(ui, &mut f.n, "northing");
                                if ui.button("Pick").clicked() {
                                    pick = true;
                                }
                            });
                        });
                        fm.row_note("Rim level", "Blank: read from the network's ground surface", |ui| {
                            d::num_opt(ui, &mut f.rim, "m").ok();
                        });
                    }
                    1 => {
                        fm.row("Structure", |ui| id_combo(ui, "pe-mv", &structures, &mut f.structure));
                        fm.row_note("New position", "Easting and northing, or pick", |ui| {
                            ui.horizontal(|ui| {
                                d::text(ui, &mut f.e, "easting");
                                d::text(ui, &mut f.n, "northing");
                                if ui.button("Pick").clicked() {
                                    pick = true;
                                }
                            });
                        });
                    }
                    2 => {
                        fm.row("Structure", |ui| id_combo(ui, "pe-del", &structures, &mut f.structure));
                        fm.row_note("Pipes", "", |ui| {
                            ui_kit::toggle(ui, &mut f.merge, "Join the pipes either side into one");
                        });
                    }
                    3 => {
                        fm.row("Pipe to change", |ui| id_combo(ui, "pe-sw-pipe", &pipes, &mut f.pipe));
                        fm.row("Pipe part", |ui| parts_combo(ui, "pe-sw-pp", &cat, PartFamily::Pipe, &mut f.part));
                        fm.row("Or structure", |ui| id_combo(ui, "pe-sw-st", &structures, &mut f.structure));
                        fm.row("Structure part", |ui| parts_combo(ui, "pe-sw-sp", &cat, PartFamily::Structure, &mut f.kind));
                        fm.full(|ui| ui_kit::note(ui, "Choose a pipe or a structure; the pipe wins when both are chosen."));
                    }
                    4 => {
                        fm.row("By", |ui| {
                            ui_kit::segmented(ui, &mut f.inv_mode, &["Slope", "Drop across a structure", "Cover under the surface"]);
                        });
                        match f.inv_mode {
                            0 => {
                                fm.row("Pipe", |ui| id_combo(ui, "pe-inv-pipe", &pipes, &mut f.pipe));
                                fm.row("Anchored at its", |ui| {
                                    ui_kit::segmented(ui, &mut f.anchor_end, &["Start", "End"]);
                                });
                                fm.row("Anchor invert", |ui| {
                                    d::num(ui, &mut f.anchor_invert, "m");
                                });
                                fm.row("Slope", |ui| {
                                    d::num(ui, &mut f.slope_pct, "%");
                                });
                            }
                            1 => {
                                fm.row("Structure", |ui| id_combo(ui, "pe-inv-st", &structures, &mut f.structure));
                                fm.row("Drop", |ui| {
                                    d::num_rule(ui, &mut f.drop, "m", NumRule::required().min(0.0));
                                });
                                fm.row("Match at the structure", |ui| {
                                    ui_kit::combo(ui, "pe-inv-mm", &mut f.match_mode, &["Crown", "Invert", "Centreline"]);
                                });
                            }
                            _ => {
                                fm.row("Pipe", |ui| id_combo(ui, "pe-inv-cpipe", &pipes, &mut f.pipe));
                                fm.row("Cover", |ui| {
                                    d::num_rule(ui, &mut f.cover, "m", NumRule::required().min(0.0));
                                });
                                fm.full(|ui| ui_kit::note(ui, "The cover is to the outside top of the barrel under the network's ground surface."));
                            }
                        }
                    }
                    _ => {
                        fm.row("Pipe", |ui| id_combo(ui, "pe-pr-pipe", &pipes, &mut f.pipe));
                        fm.row_note("Design flow", "m3/s, blank to leave", |ui| {
                            d::num_opt(ui, &mut f.flow, "m3/s").ok();
                        });
                        fm.row("Structure", |ui| id_combo(ui, "pe-pr-st", &structures, &mut f.structure));
                        fm.row_note("Inflow", "m3/s into the structure, blank to leave", |ui| {
                            d::num_opt(ui, &mut f.inflow, "m3/s").ok();
                        });
                        fm.row_note("Loss coefficient K", "No method for choosing K is built in", |ui| {
                            d::num_opt(ui, &mut f.loss_k, "").ok();
                        });
                        fm.row_note("Rim level", "blank to leave", |ui| {
                            d::num_opt(ui, &mut f.rim, "m").ok();
                        });
                        fm.row_note("Sump level", "blank to leave", |ui| {
                            d::num_opt(ui, &mut f.sump, "m").ok();
                        });
                    }
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new("Apply").icon(crate::glyphs::by_name("PIPEEDIT")).enabled(has_net, "Choose a network").show(ui);
                apply = o.primary;
                o
            },
        );
        if pick {
            self.civil_ui.pick = Some(crate::civil_ui::PickFor::Queue);
            self.civil_ui.pick_repeat = false;
            self.status = "Click the position in the model.".into();
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            self.civil_ui.pick = None;
            return;
        }
        if apply {
            f.message = match self.pipe_edit_apply(&f) {
                Ok(m) => m,
                Err(e) => format!("Refused: {e}"),
            };
        }
        self.civil_ui.pipes.edit = Some(f);
    }

    fn pipe_edit_apply(&mut self, f: &EditForm) -> Result<String, String> {
        let id = f.net.ok_or("Choose a network.")?;
        let mut data = cp::data_of(&self.doc, id).ok_or("The network cannot be read.")?;
        let num = |what: &str, t: &str| t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("{what} is not a number."));
        let opt = |what: &str, t: &str| -> Result<Option<f64>, String> { if t.trim().is_empty() { Ok(None) } else { num(what, t).map(Some) } };
        let tin: Option<Arc<Tin>> = data.surface.and_then(|s| self.tin_for(ElementId(s)));
        let net = &mut data.network;
        let msg = match f.tool {
            0 => {
                let pipe = PipeId(f.pipe.ok_or("Choose the pipe to split.")?);
                let (e, n) = (num("The easting", &f.e)?, num("The northing", &f.n)?);
                let rim = match opt("The rim level", &f.rim)? {
                    Some(r) => r,
                    None => tin.as_ref().and_then(|t| t.elevation_at(e, n)).ok_or("No rim level: type it or set a ground surface that covers the position.")?,
                };
                let (s, p) = net.split_pipe(pipe, f.name.trim(), StructureKind::ALL[f.kind.min(5)], e, n, rim).map_err(|x| x.to_string())?;
                format!("Inserted structure {s} and pipe {p}. The new pipe has no inverts of its own change: set them with Set inverts.")
            }
            1 => {
                let s = StructureId(f.structure.ok_or("Choose the structure.")?);
                net.move_structure(s, num("The easting", &f.e)?, num("The northing", &f.n)?).map_err(|x| x.to_string())?;
                "Moved the structure. The inverts were left alone.".into()
            }
            2 => {
                let s = StructureId(f.structure.ok_or("Choose the structure.")?);
                let o = if f.merge { net.delete_structure_merge(s) } else { net.delete_structure(s) }.map_err(|x| x.to_string())?;
                format!("Deleted the structure; {} pipe(s) removed{}.", o.removed_pipes.len(), if o.merged_into.is_some() { ", the two sides joined" } else { "" })
            }
            3 => {
                if let Some(p) = f.pipe {
                    let part = nth_part(&data.catalogue, PartFamily::Pipe, f.part).ok_or("Choose a pipe part.")?;
                    net.swap_pipe_part(PipeId(p), &part).map_err(|x| x.to_string())?;
                    format!("Swapped the pipe to part {}. The inverts were left alone.", part.id)
                } else if let Some(s) = f.structure {
                    let part = nth_part(&data.catalogue, PartFamily::Structure, f.kind).ok_or("Choose a structure part.")?;
                    net.swap_structure_part(StructureId(s), &part).map_err(|x| x.to_string())?;
                    format!("Swapped the structure to part {}.", part.id)
                } else {
                    return Err("Choose a pipe or a structure.".into());
                }
            }
            4 => match f.inv_mode {
                0 => {
                    let p = PipeId(f.pipe.ok_or("Choose the pipe.")?);
                    net.set_inverts_by_slope(p, if f.anchor_end == 0 { PipeEnd::Start } else { PipeEnd::End }, num("The anchor invert", &f.anchor_invert)?, num("The slope", &f.slope_pct)? / 100.0).map_err(|x| x.to_string())?;
                    "Set the inverts by slope.".into()
                }
                1 => {
                    let s = StructureId(f.structure.ok_or("Choose the structure.")?);
                    net.set_drop_across_structure(s, num("The drop", &f.drop)?, [MatchMode::Crown, MatchMode::Invert, MatchMode::Centreline][f.match_mode.min(2)]).map_err(|x| x.to_string())?;
                    "Set the drop across the structure.".into()
                }
                _ => {
                    let p = PipeId(f.pipe.ok_or("Choose the pipe.")?);
                    let t = tin.as_ref().ok_or("The network has no ground surface: set one in the properties.")?;
                    net.set_inverts_by_cover(p, t.as_ref(), num("The cover", &f.cover)?).map_err(|x| x.to_string())?;
                    "Set the inverts by cover under the surface.".into()
                }
            },
            _ => {
                let mut done = Vec::new();
                if let Some(p) = f.pipe {
                    if let Some(q) = opt("The design flow", &f.flow)? {
                        net.pipe_mut(PipeId(p)).map_err(|x| x.to_string())?.design_flow_m3_s = Some(q);
                        done.push("design flow");
                    }
                }
                if let Some(s) = f.structure {
                    let st = net.structure_mut(StructureId(s)).map_err(|x| x.to_string())?;
                    if let Some(q) = opt("The inflow", &f.inflow)? {
                        st.inflow_m3_s = q;
                        done.push("inflow");
                    }
                    if let Some(k) = opt("K", &f.loss_k)? {
                        st.loss_coefficient = Some(k);
                        done.push("loss coefficient");
                    }
                    if let Some(r) = opt("The rim level", &f.rim)? {
                        st.rim_level_m = r;
                        done.push("rim level");
                    }
                    if let Some(r) = opt("The sump level", &f.sump)? {
                        st.sump_level_m = Some(r);
                        done.push("sump level");
                    }
                }
                if done.is_empty() {
                    return Err("Nothing to change: choose a pipe or a structure and fill a value.".into());
                }
                format!("Changed: {}.", done.join(", "))
            }
        };
        cp::store(&mut self.doc, id, &data);
        self.schedule_commit_now();
        Ok(msg)
    }
}

// ─── CHECKS ─────────────────────────────────────────────────────────────────

pub struct CheckForm {
    net: Option<ElementId>,
    only_problems: bool,
}

pub struct PipeCheckOut {
    pub network: ElementId,
    pub name: String,
    pub findings: Vec<Finding>,
    pub hydraulics: Option<HydraulicReport>,
    pub grade: Option<GradeReport>,
    pub notes: Vec<String>,
    /// The structure and pipe names and positions the findings zoom to.
    pub positions: Vec<(ElementRef, String, [f64; 2])>,
}

fn run_checks(id: ElementId, d: PipeNetData, tin: Option<Arc<Tin>>, partners: Vec<(String, PipeNetData)>) -> PipeCheckOut {
    let net = &d.network;
    let mut notes = Vec::new();
    let mut findings = net.topology_findings();
    let ground: Option<&dyn Ground> = tin.as_deref().map(|t| t as &dyn Ground);
    if ground.is_none() {
        notes.push("Cover was not checked: the network has no ground surface.".to_string());
    } else if d.rules.min_cover_m.is_none() && d.rules.max_cover_m.is_none() {
        notes.push("Cover was not checked: no cover rule is set.".to_string());
    }
    if d.rules.min_slope.is_none() && d.rules.max_slope.is_none() {
        notes.push("Slope was not checked: no slope rule is set.".to_string());
    }
    if d.rules.max_pipe_length_m.is_none() {
        notes.push("Pipe length was not checked: no maximum length is set.".to_string());
    }
    if d.rules.structure_drop_m.is_none() {
        notes.push("Drops across structures were not checked: no drop rule is set.".to_string());
    }
    findings.extend(net.check_all_geometry(&d.rules, ground, d.cover_spacing_m));
    let hyd = net.check_hydraulics(&d.rules, &d.settings());
    findings.extend(hyd.findings.clone());
    let grade = d.tailwater_m.map(|t| net.grade_lines(t, &d.settings()));
    match &grade {
        Some(g) => findings.extend(g.findings.clone()),
        None => notes.push("Grade lines were not computed: no tailwater level is set.".to_string()),
    }
    match d.clearance_m {
        Some(c) => {
            findings.extend(net.check_interference(c));
            for (name, other) in &partners {
                let mut f = net.check_interference_with(&other.network, c);
                for x in &mut f {
                    x.message = format!("With network {name}: {}", x.message);
                }
                findings.extend(f);
            }
        }
        None => notes.push("Interference was not checked: no clearance is set.".to_string()),
    }
    let mut positions: Vec<(ElementRef, String, [f64; 2])> = net.structures().map(|s| (ElementRef::Structure(s.id), s.name.clone(), [s.easting_m, s.northing_m])).collect();
    for p in net.pipes() {
        if let Ok(path) = net.pipe_path(p.id) {
            positions.push((ElementRef::Pipe(p.id), p.name.clone(), path.point_at(path.length_m / 2.0)));
        }
    }
    PipeCheckOut { network: id, name: net.name.clone(), findings, hydraulics: Some(hyd), grade, notes, positions }
}

impl AfroBimApp {
    pub(crate) fn pipe_check_cmd(&mut self) {
        let nets = cp::networks(&self.doc);
        if nets.is_empty() {
            self.status = "There is no pipe network yet: make one with PIPENEW first.".into();
            return;
        }
        let net = self.doc.primary().filter(|id| nets.iter().any(|n| n.0 == *id)).or_else(|| nets.first().map(|n| n.0));
        self.civil_ui.pipes.check = Some(CheckForm { net, only_problems: false });
        if self.options.civil_demo {
            self.pipe_check_run();
        }
    }

    fn pipe_check_run(&mut self) {
        let Some(id) = self.civil_ui.pipes.check.as_ref().and_then(|f| f.net) else { return };
        let Some(d) = cp::data_of(&self.doc, id) else { return };
        let tin = d.surface.and_then(|s| self.tin_for(ElementId(s)));
        let partners: Vec<(String, PipeNetData)> = d.partners.iter().filter_map(|p| cp::data_of(&self.doc, ElementId(*p)).map(|x| (self.doc.find(ElementId(*p)).map(|e| e.name.clone()).unwrap_or_default(), x))).collect();
        let fp = crate::background::fingerprint(&self.doc);
        let label = format!("Checking pipe network {}", d.network.name);
        self.bg.pipecheck = Some(Job::spawn(&label, fp, move || run_checks(id, d, tin, partners)));
    }

    fn pipe_check_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.pipes.check.take() else { return };
        let nets = cp::networks(&self.doc);
        let busy = self.bg.pipecheck.is_some();
        let mut run = false;
        let mut csv = false;
        let mut zoom: Option<(ElementId, [f64; 2])> = None;
        let out_ref = self.civil_ui.pipes.check_out.as_ref();
        let out = Dialog::new("Pipe network checks", crate::glyphs::by_name("PIPECHECK")).width(900.0).resizable(true).max_body_height(620.0).help("Cover, slope, length, drops, hydraulics at the design flows, grade lines from the tailwater level and interference within and between networks, against the rule set and inputs of the network (PIPEPROPS). A check whose input is blank is skipped and says so. These are calculations for an engineer to check, not a certified design.").show_body(
            ctx,
            |ui| {
                ui_kit::form(ui, "pc-head", |fm| {
                    fm.row("Network", |ui| {
                        if net_combo(ui, "pc-net", &nets, &mut f.net) {
                            run = false;
                        }
                    });
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy && f.net.is_some(), egui::Button::new("Run the checks")).clicked() {
                        run = true;
                    }
                    ui_kit::toggle(ui, &mut f.only_problems, "Warnings and errors only");
                    if ui.add_enabled(out_ref.is_some(), egui::Button::new("Findings as CSV...")).clicked() {
                        csv = true;
                    }
                });
                if busy {
                    ui.label(RichText::new("Checking...").weak());
                }
                let Some(o) = out_ref else { return };
                ui.separator();
                ui.label(RichText::new(format!("Network {}", o.name)).strong());
                for n in &o.notes {
                    ui.label(RichText::new(n).small().weak());
                }
                let shown: Vec<&Finding> = o.findings.iter().filter(|x| !f.only_problems || x.severity != Severity::Info).collect();
                let (w, e) = (o.findings.iter().filter(|x| x.severity == Severity::Warning).count(), o.findings.iter().filter(|x| x.severity == Severity::Error).count());
                ui.label(format!("{} finding(s): {e} error(s), {w} warning(s).", o.findings.len()));
                egui::ScrollArea::vertical().id_salt("pc-find").max_height(230.0).show(ui, |ui| {
                    egui::Grid::new("pc-findings").striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                        for h in ["Check", "Severity", "Element", "Finding", ""] {
                            ui_kit::th(ui, h);
                        }
                        ui.end_row();
                        for fd in &shown {
                            ui.label(&fd.check);
                            let (txt, col) = match fd.severity {
                                Severity::Info => ("info", ui.visuals().weak_text_color()),
                                Severity::Warning => ("warning", crate::theme::warn()),
                                Severity::Error => ("error", crate::theme::danger()),
                            };
                            ui.label(RichText::new(txt).color(col));
                            let names: Vec<String> = fd.elements.iter().map(|r| o.positions.iter().find(|p| p.0 == *r).map(|p| p.1.clone()).unwrap_or_else(|| r.to_string())).collect();
                            ui.label(names.join(", "));
                            ui.vertical(|ui| {
                                ui.set_width(420.0);
                                ui.add(egui::Label::new(RichText::new(&fd.message).small()).wrap());
                            });
                            if let Some(first) = fd.elements.first().and_then(|r| o.positions.iter().find(|p| p.0 == *r)) {
                                if ui_kit::icon_button(ui, crate::glyphs::by_name("ZOOMSELECTION"), "Zoom to the element").clicked() {
                                    zoom = Some((o.network, first.2));
                                }
                            } else {
                                ui.label("");
                            }
                            ui.end_row();
                        }
                    });
                });
                if let Some(h) = &o.hydraulics {
                    ui_kit::heading(ui, "Hydraulics at the design flows");
                    let rows: Vec<Vec<String>> = h.pipes.iter().map(|p| {
                        let name = o.positions.iter().find(|x| x.0 == ElementRef::Pipe(p.pipe)).map(|x| x.1.clone()).unwrap_or_default();
                        vec![name, fmt(p.flow_m3_s, 4), fmt(p.slope * 100.0, 3), p.flow.as_ref().map(|n| fmt(n.velocity_m_s, 2)).unwrap_or_default(), p.capacity_ratio.map(|c| fmt(c, 2)).unwrap_or_default(), p.flow.as_ref().map(|n| fmt(n.depth_m, 3)).unwrap_or_default()]
                    }).collect();
                    egui::ScrollArea::vertical().id_salt("pc-hyd").max_height(130.0).show(ui, |ui| {
                        ui_kit::results_table(ui, "pc-hyd", &["Pipe", "Flow m3/s", "Slope %", "Velocity m/s", "Flow / full", "Depth m"], &[Col::Text, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num], &rows);
                    });
                }
                if let Some(g) = &o.grade {
                    ui_kit::heading(ui, &format!("Grade lines from a tailwater level of {} m", fmt(g.tailwater_level_m, 3)));
                    ui.label(RichText::new(&g.method).small().weak());
                    let rows: Vec<Vec<String>> = g.structures.iter().map(|s| {
                        let name = o.positions.iter().find(|x| x.0 == ElementRef::Structure(s.structure)).map(|x| x.1.clone()).unwrap_or_default();
                        vec![name, fmt(s.hgl_m, 3), fmt(s.egl_m, 3), fmt(s.rim_level_m, 3), if s.above_rim { "above the rim".into() } else { "below".into() }]
                    }).collect();
                    egui::ScrollArea::vertical().id_salt("pc-grade").max_height(130.0).show(ui, |ui| {
                        ui_kit::results_table(ui, "pc-grade", &["Structure", "HGL m", "EGL m", "Rim m", "HGL against rim"], &[Col::Text, Col::Num, Col::Num, Col::Num, Col::Text], &rows);
                    });
                }
            },
        );
        if run {
            self.civil_ui.pipes.check = Some(CheckForm { net: f.net, only_problems: f.only_problems });
            self.pipe_check_run();
            f = self.civil_ui.pipes.check.take().unwrap_or(CheckForm { net: None, only_problems: false });
        }
        if csv {
            if let Some(o) = &self.civil_ui.pipes.check_out {
                let mut text = String::from("check,severity,elements,message\n");
                for fd in &o.findings {
                    text.push_str(&format!("{},{:?},\"{}\",\"{}\"\n", fd.check, fd.severity, fd.elements.iter().map(|r| r.to_string()).collect::<Vec<_>>().join(" "), fd.message.replace('"', "'")));
                }
                if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("pipe-findings.csv").save_file() {
                    self.status = match std::fs::write(&p, text) {
                        Ok(()) => format!("Wrote {}.", p.display()),
                        Err(e) => format!("The file could not be written: {e}"),
                    };
                }
            }
        }
        if let Some((id, at)) = zoom {
            self.doc.select(&[id], SelectMode::Replace);
            self.civil_zoom_to(at[0], at[1], 12.0);
        }
        if out.closed {
            return;
        }
        self.civil_ui.pipes.check = Some(f);
    }

    /// Centres the plan view on an easting and northing with a window of about `span_m` across.
    pub(crate) fn civil_zoom_to(&mut self, e: f64, n: f64, span_m: f64) {
        let (left, top, w, h) = self.ctl.viewport;
        if w > 50.0 {
            let s = span_m * 500.0;
            self.view.fit(&afrobim_kernel::BBox::new(e * 1000.0 - s, n * 1000.0 - s, e * 1000.0 + s, n * 1000.0 + s), left, top, w, h, 40.0);
        }
    }

    pub(crate) fn pipe_tick(&mut self) {
        if let Some(polled) = crate::background::take(&mut self.bg.pipecheck, &self.doc) {
            match polled {
                crate::background::Poll::Done(out) => {
                    self.status = format!("Checked pipe network {}: {} finding(s).", out.name, out.findings.len());
                    self.civil_ui.pipes.check_out = Some(out);
                }
                other => self.status = crate::background::discard_message("Pipe network check", &other),
            }
        }
    }
}

// ─── TABLES ─────────────────────────────────────────────────────────────────

pub struct TablesForm {
    net: Option<ElementId>,
    tab: usize,
}

impl AfroBimApp {
    pub(crate) fn pipe_tables_cmd(&mut self) {
        let nets = cp::networks(&self.doc);
        if nets.is_empty() {
            self.status = "There is no pipe network yet: make one with PIPENEW first.".into();
            return;
        }
        let net = self.doc.primary().filter(|id| nets.iter().any(|n| n.0 == *id)).or_else(|| nets.first().map(|n| n.0));
        self.civil_ui.pipes.tables = Some(TablesForm { net, tab: 0 });
    }

    fn pipe_tables_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.pipes.tables.take() else { return };
        let nets = cp::networks(&self.doc);
        let data = f.net.and_then(|id| cp::data_of(&self.doc, id));
        let mut csv = false;
        let out = Dialog::new("Structure and pipe tables", crate::glyphs::by_name("PIPETABLES")).width(980.0).resizable(true).max_body_height(600.0).help("Every structure and pipe of the network with its levels, sizes and connections. Lengths are centre to centre of structures.").show_body(
            ctx,
            |ui| {
                ui_kit::form(ui, "pt-head", |fm| {
                    fm.row("Network", |ui| {
                        net_combo(ui, "pt-net", &nets, &mut f.net);
                    });
                });
                ui_kit::tabs(ui, &mut f.tab, &["Structures", "Pipes"]);
                let Some(d) = &data else { return };
                if ui.button("Export this table as CSV...").clicked() {
                    csv = true;
                }
                let n = &d.network;
                if f.tab == 0 {
                    let rows: Vec<Vec<String>> = n.structure_table().iter().map(|r| vec![r.name.clone(), r.kind.label().to_string(), fmt(r.easting_m, 3), fmt(r.northing_m, 3), fmt(r.rim_level_m, 3), r.sump_level_m.map(|v| fmt(v, 3)).unwrap_or_default(), r.inner_size.clone(), r.part.clone().unwrap_or_default(), r.pipes_in.join(" "), r.pipes_out.join(" "), r.lowest_invert_m.map(|v| fmt(v, 3)).unwrap_or_default(), r.depth_to_lowest_invert_m.map(|v| fmt(v, 3)).unwrap_or_default()]).collect();
                    egui::ScrollArea::both().max_height(430.0).show(ui, |ui| {
                        ui_kit::results_table(ui, "pt-struct", &["Name", "Kind", "Easting", "Northing", "Rim", "Sump", "Inner size", "Part", "In", "Out", "Lowest invert", "Depth"], &[Col::Text, Col::Text, Col::Num, Col::Num, Col::Num, Col::Num, Col::Text, Col::Text, Col::Text, Col::Text, Col::Num, Col::Num], &rows);
                    });
                } else {
                    let rows: Vec<Vec<String>> = n.pipe_table().iter().map(|r| vec![r.name.clone(), r.start.clone(), r.end.clone(), r.size.clone(), r.material.clone().unwrap_or_default(), r.part.clone().unwrap_or_default(), r.roughness.clone(), fmt(r.plan_length_m, 3), fmt(r.start_invert_m, 3), fmt(r.end_invert_m, 3), fmt(r.start_crown_m, 3), fmt(r.end_crown_m, 3), fmt(r.slope * 100.0, 3), r.flow_m3_s.map(|v| fmt(v, 4)).unwrap_or_default()]).collect();
                    egui::ScrollArea::both().max_height(430.0).show(ui, |ui| {
                        ui_kit::results_table(ui, "pt-pipe", &["Name", "Start", "End", "Size", "Material", "Part", "Roughness", "Length", "Start inv.", "End inv.", "Start crown", "End crown", "Slope %", "Flow"], &[Col::Text, Col::Text, Col::Text, Col::Text, Col::Text, Col::Text, Col::Text, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num], &rows);
                    });
                }
            },
        );
        if csv {
            if let Some(d) = &data {
                let (text, name) = if f.tab == 0 { (d.network.structure_table_csv(), "pipe-structures.csv") } else { (d.network.pipe_table_csv(), "pipe-pipes.csv") };
                if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(name).save_file() {
                    self.status = match std::fs::write(&p, text) {
                        Ok(()) => format!("Wrote {}.", p.display()),
                        Err(e) => format!("The file could not be written: {e}"),
                    };
                }
            }
        }
        if out.closed {
            return;
        }
        self.civil_ui.pipes.tables = Some(f);
    }
}

// ─── QUANTITIES ─────────────────────────────────────────────────────────────

pub struct QtyForm {
    net: Option<ElementId>,
    /// 0 fixed width, 1 outside width plus each side.
    width_mode: usize,
    width: String,
    bedding: String,
    side_slope: String,
    bands: String,
    spacing: String,
    message: String,
}

pub struct QtyOut {
    pub network: ElementId,
    pub name: String,
    pub lengths: Vec<PipeLengthRow>,
    pub counts: Vec<StructureCountRow>,
    pub trench: Result<TrenchQuantities, String>,
    pub inputs: String,
}

impl AfroBimApp {
    pub(crate) fn pipe_qty_cmd(&mut self) {
        let nets = cp::networks(&self.doc);
        if nets.is_empty() {
            self.status = "There is no pipe network yet: make one with PIPENEW first.".into();
            return;
        }
        let net = self.doc.primary().filter(|id| nets.iter().any(|n| n.0 == *id)).or_else(|| nets.first().map(|n| n.0));
        let mut f = QtyForm { net, width_mode: 0, width: String::new(), bedding: String::new(), side_slope: String::new(), bands: String::new(), spacing: String::new(), message: String::new() };
        if let Some(t) = net.and_then(|id| cp::data_of(&self.doc, id)).and_then(|d| d.trench) {
            match t.width {
                TrenchWidth::Fixed { width_m } => {
                    f.width_mode = 0;
                    f.width = afrobim_defaults::fmt_num(width_m);
                }
                TrenchWidth::OutsideWidthPlus { each_side_m } => {
                    f.width_mode = 1;
                    f.width = afrobim_defaults::fmt_num(each_side_m);
                }
            }
            f.bedding = afrobim_defaults::fmt_num(t.bedding_thickness_m);
            f.side_slope = afrobim_defaults::fmt_num(t.side_slope);
            f.bands = t.depth_bands_m.iter().map(|b| afrobim_defaults::fmt_num(*b)).collect::<Vec<_>>().join(", ");
            f.spacing = afrobim_defaults::fmt_num(t.station_spacing_m);
        }
        self.civil_ui.pipes.qty = Some(f);
    }

    fn pipe_qty_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.pipes.qty.take() else { return };
        let nets = cp::networks(&self.doc);
        let mut calc = false;
        let mut csv = false;
        let mut bill = false;
        let out_ref = self.civil_ui.pipes.qty_out.as_ref();
        let pos = NumRule::required().above(0.0);
        let out = Dialog::new("Pipe network quantities", crate::glyphs::by_name("PIPEQTY")).width(860.0).resizable(true).max_body_height(640.0).help("Pipe lengths by size and material, structure counts, and the trench: excavation, bedding and pipe displacement by depth band. Every trench input is yours: nothing is built in. The trench has no structure pits, working space, support, rock, de-watering, swell or compaction.").show_body(
            ctx,
            |ui| {
                ui_kit::form(ui, "pq-head", |fm| {
                    fm.row("Network", |ui| {
                        net_combo(ui, "pq-net", &nets, &mut f.net);
                    });
                });
                ui_kit::section(ui, "Trench inputs", |ui| {
                    ui_kit::form(ui, "pq-in", |fm| {
                        fm.row("Width", |ui| {
                            ui_kit::segmented(ui, &mut f.width_mode, &["Fixed", "Outside width plus each side"]);
                        });
                        fm.row(if f.width_mode == 0 { "Trench width" } else { "Allowance each side" }, |ui| {
                            d::num_rule(ui, &mut f.width, "m", pos.clone());
                        });
                        fm.row("Bedding thickness", |ui| {
                            d::num_rule(ui, &mut f.bedding, "m", NumRule::required().min(0.0));
                        });
                        fm.row_note("Side slope", "Horizontal per vertical; 0 for vertical sides", |ui| {
                            d::num_rule(ui, &mut f.side_slope, "", NumRule::required().min(0.0));
                        });
                        fm.row_note("Depth bands", "Band edges in metres, increasing, for example 1.5, 3", |ui| d::text(ui, &mut f.bands, "1.5, 3"));
                        fm.row_note("Section spacing", "Along each pipe", |ui| {
                            d::num_rule(ui, &mut f.spacing, "m", pos.clone());
                        });
                    });
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
                ui.horizontal(|ui| {
                    if ui.button("Calculate").clicked() {
                        calc = true;
                    }
                    if ui.add_enabled(out_ref.is_some(), egui::Button::new("Export CSV...")).clicked() {
                        csv = true;
                    }
                    if ui.add_enabled(out_ref.is_some(), egui::Button::new("Send to the bill")).on_hover_text("Adds the quantities as lines the bill (BILL) collects").clicked() {
                        bill = true;
                    }
                });
                let Some(o) = out_ref else { return };
                ui.separator();
                ui.label(RichText::new(format!("Network {}", o.name)).strong());
                let rows: Vec<Vec<String>> = o.lengths.iter().map(|r| vec![r.size.clone(), r.material.clone(), r.count.to_string(), fmt(r.plan_length_m, 2), fmt(r.sloping_length_m, 2)]).collect();
                ui_kit::results_table(ui, "pq-len", &["Size", "Material", "Pipes", "Plan length m", "Sloping length m"], &[Col::Text, Col::Text, Col::Num, Col::Num, Col::Num], &rows);
                let rows: Vec<Vec<String>> = o.counts.iter().map(|r| vec![r.kind.label().to_string(), r.part.clone(), r.count.to_string()]).collect();
                ui_kit::results_table(ui, "pq-cnt", &["Structure", "Part", "Count"], &[Col::Text, Col::Text, Col::Num], &rows);
                match &o.trench {
                    Ok(t) => {
                        ui_kit::heading(ui, "Trench");
                        ui.label(RichText::new(format!("Inputs: {}", o.inputs)).small().weak());
                        let rows: Vec<Vec<String>> = t.rows.iter().map(|r| vec![r.name.clone(), fmt(r.plan_length_m, 2), fmt(r.trench_width_m, 2), fmt(r.min_depth_m, 2), fmt(r.max_depth_m, 2), fmt(r.excavation_m3, 2), fmt(r.bedding_m3, 2), fmt(r.pipe_displacement_m3, 2)]).collect();
                        ui_kit::results_table(ui, "pq-tr", &["Pipe", "Length m", "Width m", "Min depth", "Max depth", "Excavation m3", "Bedding m3", "Pipe displacement m3"], &[Col::Text, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num], &rows);
                        let rows: Vec<Vec<String>> = t.band_totals.iter().map(|b| vec![format!("{} to {}", fmt(b.from_m, 2), b.to_m.map(|v| fmt(v, 2)).unwrap_or_else(|| "deeper".into())), fmt(b.volume_m3, 2)]).collect();
                        ui_kit::results_table(ui, "pq-band", &["Depth band m", "Excavation m3"], &[Col::Text, Col::Num], &rows);
                        ui.label(format!("Total excavation {} m3, bedding {} m3, pipe displacement {} m3.", fmt(t.total_excavation_m3, 2), fmt(t.total_bedding_m3, 2), fmt(t.total_pipe_displacement_m3, 2)));
                        ui.label(RichText::new(&t.method).small().weak());
                        for fd in &t.findings {
                            ui.label(RichText::new(&fd.message).small().color(crate::theme::warn()));
                        }
                    }
                    Err(e) => {
                        ui_kit::callout(ui, Level::Warning, e.clone());
                    }
                }
            },
        );
        if calc {
            match self.pipe_qty_calc(&f) {
                Ok(o) => {
                    f.message.clear();
                    self.civil_ui.pipes.qty_out = Some(o);
                }
                Err(e) => f.message = e,
            }
        }
        if csv {
            if let (Some(o), Some(id)) = (&self.civil_ui.pipes.qty_out, f.net) {
                if let Some(d) = cp::data_of(&self.doc, id) {
                    let mut text = d.network.pipe_length_csv();
                    text.push('\n');
                    text.push_str(&d.network.structure_count_csv());
                    if let Ok(t) = &o.trench {
                        text.push('\n');
                        text.push_str(&t.to_csv());
                    }
                    if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("pipe-quantities.csv").save_file() {
                        self.status = match std::fs::write(&p, text) {
                            Ok(()) => format!("Wrote {}.", p.display()),
                            Err(e) => format!("The file could not be written: {e}"),
                        };
                    }
                }
            }
        }
        if bill {
            self.pipe_qty_to_bill();
        }
        if out.closed {
            return;
        }
        self.civil_ui.pipes.qty = Some(f);
    }

    fn pipe_qty_calc(&mut self, f: &QtyForm) -> Result<QtyOut, String> {
        let id = f.net.ok_or("Choose a network.")?;
        let mut d = cp::data_of(&self.doc, id).ok_or("The network cannot be read.")?;
        let num = |what: &str, t: &str| t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("{what} is not a number."));
        let w = num("The width", &f.width)?;
        let bands: Result<Vec<f64>, String> = f.bands.split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()).map(|s| num("A depth band", s)).collect();
        let spec = TrenchSpec { width: if f.width_mode == 0 { TrenchWidth::Fixed { width_m: w } } else { TrenchWidth::OutsideWidthPlus { each_side_m: w } }, bedding_thickness_m: num("The bedding thickness", &f.bedding)?, side_slope: num("The side slope", &f.side_slope)?, depth_bands_m: bands?, station_spacing_m: num("The section spacing", &f.spacing)? };
        let inputs = format!("{} {} m, bedding {} m, side slope {}, bands [{}] m, spacing {} m", if f.width_mode == 0 { "width" } else { "outside width plus" }, afrobim_defaults::fmt_num(w), afrobim_defaults::fmt_num(spec.bedding_thickness_m), afrobim_defaults::fmt_num(spec.side_slope), f.bands.trim(), afrobim_defaults::fmt_num(spec.station_spacing_m));
        let tin = d.surface.and_then(|s| self.tin_for(ElementId(s)));
        let trench = match &tin {
            Some(t) => d.network.trench_quantities(t.as_ref(), &spec).map_err(|e| e.to_string()),
            None => Err("The trench needs a ground surface: set one in the network properties.".into()),
        };
        d.trench = Some(spec);
        let out = QtyOut { network: id, name: d.network.name.clone(), lengths: d.network.pipe_length_quantities(), counts: d.network.structure_count_quantities(), trench, inputs };
        cp::store(&mut self.doc, id, &d);
        self.schedule_commit_now();
        Ok(out)
    }

    fn pipe_qty_to_bill(&mut self) {
        let Some(o) = &self.civil_ui.pipes.qty_out else { return };
        let mut lines: Vec<(String, String, f64, String)> = Vec::new();
        for r in &o.lengths {
            lines.push(("Pipe networks".into(), format!("Pipe {} {}, {}", r.size, r.material, o.name), r.plan_length_m, "m".into()));
        }
        for r in &o.counts {
            lines.push(("Pipe networks".into(), format!("{} {}, {}", r.kind.label(), r.part, o.name), r.count as f64, "no".into()));
        }
        if let Ok(t) = &o.trench {
            lines.push(("Pipe networks".into(), format!("Trench excavation, {} ({})", o.name, o.inputs), t.total_excavation_m3, "m3".into()));
            lines.push(("Pipe networks".into(), format!("Pipe bedding, {}", o.name), t.total_bedding_m3, "m3".into()));
        }
        let n = lines.len();
        if let Some(e) = self.doc.find_mut(o.network) {
            e.properties.insert(crate::civil_quantities::PROP_BILL.into(), serde_json::to_string(&lines).unwrap_or_default());
        }
        self.status = format!("Sent {n} quantity line(s) to the bill (category Pipe networks). They need a rate rule for that category to be priced.");
        self.schedule_commit_now();
    }
}

// ─── PROFILE OF A RUN ───────────────────────────────────────────────────────

pub struct ProfForm {
    edit: Option<ElementId>,
    net: Option<ElementId>,
    start: Option<u32>,
    pipes: Vec<u32>,
    exaggeration: String,
    spacing: String,
    tailwater: String,
    use_alignment: bool,
    e: String,
    n: String,
    message: String,
}

impl AfroBimApp {
    pub(crate) fn pipe_profile_cmd(&mut self) {
        let nets = cp::networks(&self.doc);
        if nets.is_empty() {
            self.status = "There is no pipe network yet: make one with PIPENEW first.".into();
            return;
        }
        let sel = self.doc.primary();
        if let Some(pp) = sel.and_then(|id| civil::get::<PipeProfileData>(&self.doc, id, PIPEPROFILE).map(|d| (id, d))) {
            self.civil_ui.pipes.profile = Some(ProfForm { edit: Some(pp.0), net: Some(ElementId(pp.1.network)), start: pp.1.pipes.first().copied(), pipes: pp.1.pipes.clone(), exaggeration: pp.1.vertical_exaggeration.to_string(), spacing: pp.1.ground_spacing_m.to_string(), tailwater: opt_text(pp.1.tailwater_m), use_alignment: pp.1.alignment.is_some(), e: format!("{:.3}", pp.1.insertion[0]), n: format!("{:.3}", pp.1.insertion[1]), message: String::new() });
            return;
        }
        let net = sel.filter(|id| nets.iter().any(|n| n.0 == *id)).or_else(|| nets.first().map(|n| n.0));
        let (e, n) = net.and_then(|id| cp::data_of(&self.doc, id)).and_then(|d| cp::bounds_of(&d.network)).map(|b| (b.min_e, b.max_n + 20.0)).unwrap_or((0.0, 0.0));
        self.civil_ui.pipes.profile = Some(ProfForm { edit: None, net, start: None, pipes: vec![], exaggeration: String::new(), spacing: String::new(), tailwater: String::new(), use_alignment: false, e: format!("{e:.3}"), n: format!("{n:.3}"), message: String::new() });
    }

    fn pipe_profile_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.pipes.profile.take() else { return };
        let nets = cp::networks(&self.doc);
        let data = f.net.and_then(|id| cp::data_of(&self.doc, id));
        let structures = data.as_ref().map(|d| structure_items(&d.network)).unwrap_or_default();
        let mut make = false;
        let (has_pipes, editing) = (!f.pipes.is_empty(), f.edit.is_some());
        let out = Dialog::new(if f.edit.is_some() { "Edit pipe profile" } else { "Pipe profile" }, crate::glyphs::by_name("PIPEPROFILE")).width(640.0).resizable(true).max_body_height(560.0).help("Draws a run of pipes in a profile view in the model: ground, structures, pipes by invert and crown, and the hydraulic grade line when a tailwater level is given. The run follows the pipes downstream from the structure you choose. The vertical exaggeration and the spacing of the ground samples are inputs.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "pf-head", |fm| {
                    fm.row("Network", |ui| {
                        if net_combo(ui, "pf-net", &nets, &mut f.net) {
                            f.pipes.clear();
                            f.start = None;
                        }
                    });
                    fm.row_note("Run from structure", "The run follows the pipes downstream", |ui| {
                        let before = f.start;
                        id_combo(ui, "pf-start", &structures, &mut f.start);
                        if before != f.start {
                            f.pipes = f.start.zip(data.as_ref()).and_then(|(s, d)| d.network.trace_run(StructureId(s)).ok()).map(|r| r.iter().map(|p| p.0).collect()).unwrap_or_default();
                        }
                    });
                    fm.row("Pipes in the run", |ui| {
                        ui.label(if f.pipes.is_empty() { "none yet".to_string() } else { f.pipes.iter().filter_map(|p| data.as_ref().and_then(|d| d.network.pipe(PipeId(*p)).ok().map(|x| x.name.clone()))).collect::<Vec<_>>().join(", ") });
                    });
                    fm.row_note("Along the alignment", "Uses the network's alignment (PIPEPROPS)", |ui| {
                        let has = data.as_ref().is_some_and(|d| d.network.alignment_name.is_some());
                        ui.add_enabled_ui(has, |ui| {
                            ui_kit::toggle(ui, &mut f.use_alignment, "Project onto the alignment");
                        });
                    });
                    fm.row("Vertical exaggeration", |ui| {
                        d::num_rule(ui, &mut f.exaggeration, "", NumRule::required().above(0.0));
                    });
                    fm.row("Ground sample spacing", |ui| {
                        d::num_rule(ui, &mut f.spacing, "m", NumRule::required().above(0.0));
                    });
                    fm.row_note("Tailwater level", "Draws the hydraulic grade line; blank for none", |ui| {
                        d::num_opt(ui, &mut f.tailwater, "m").ok();
                    });
                    fm.row("Lower left corner", |ui| {
                        ui.horizontal(|ui| {
                            d::num(ui, &mut f.e, "E");
                            d::num(ui, &mut f.n, "N");
                        });
                    });
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new(if editing { "Update" } else { "Draw the profile" }).icon(crate::glyphs::by_name("PIPEPROFILE")).enabled(has_pipes, "Choose the structure the run starts from").show(ui);
                make = o.primary;
                o
            },
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if make {
            match self.pipe_profile_make(&f, data.as_ref()) {
                Ok(()) => return,
                Err(e) => f.message = e,
            }
        }
        self.civil_ui.pipes.profile = Some(f);
    }

    fn pipe_profile_make(&mut self, f: &ProfForm, data: Option<&PipeNetData>) -> Result<(), String> {
        let net = f.net.ok_or("Choose a network.")?;
        let d = data.ok_or("The network cannot be read.")?;
        let num = |what: &str, t: &str| t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("{what} is not a number."));
        let pp = PipeProfileData {
            network: net.0,
            pipes: f.pipes.clone(),
            alignment: if f.use_alignment { d.network.alignment_name.clone() } else { None },
            surface: d.surface,
            insertion: [num("The easting", &f.e)?, num("The northing", &f.n)?],
            vertical_exaggeration: num("The vertical exaggeration", &f.exaggeration)?,
            ground_spacing_m: num("The ground spacing", &f.spacing)?,
            tailwater_m: if f.tailwater.trim().is_empty() { None } else { Some(num("The tailwater level", &f.tailwater)?) },
        };
        let v = serde_json::to_value(&pp).unwrap_or_default();
        let bounds = Some(civil::Bounds::point(pp.insertion[0], pp.insertion[1]));
        let id = match f.edit {
            Some(id) => {
                civil::set(&mut self.doc, id, v, bounds);
                id
            }
            None => civil::add(&mut self.doc, PIPEPROFILE, &format!("{} profile", d.network.name), v, bounds, "Pipe network run"),
        };
        self.doc.select(&[id], SelectMode::Replace);
        self.status = "Pipe profile drawn in the model.".into();
        self.schedule_commit_now();
        Ok(())
    }

    pub(crate) fn pipes_windows(&mut self, ctx: &egui::Context) {
        self.pipe_tick();
        self.pipe_new_window(ctx);
        self.pipe_parts_window(ctx);
        self.pipe_props_window(ctx);
        self.pipe_edit_window(ctx);
        self.pipe_check_window(ctx);
        self.pipe_tables_window(ctx);
        self.pipe_qty_window(ctx);
        self.pipe_profile_window(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::civil_demo::{build, E0, N0};
    use afrobim_kernel::Document;

    fn demo() -> (Document, ElementId) {
        let mut doc = Document::default();
        let notes = build(&mut doc);
        assert!(notes.is_empty(), "{notes:?}");
        let id = doc.elements().iter().find(|e| e.category == civil::PIPENET).expect("the demonstration has a pipe network").id;
        (doc, id)
    }

    fn app() -> AfroBimApp {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx);
        AfroBimApp::new(&cc, crate::Options { civil_demo: false, screenshot: Some(std::env::temp_dir().join("afrobim-a1-test.png")), ..crate::Options::default() })
    }

    #[test]
    fn a_network_survives_save_and_load_and_sits_at_source_coordinates() {
        let (doc, id) = demo();
        let back = Document::from_json(&doc.to_json()).expect("the document loads");
        let (a, b) = (doc.find(id).unwrap(), back.find(id).unwrap());
        assert_eq!(a.data, b.data);
        assert_eq!(a.origin, b.origin);
        let d = cp::data_of(&doc, id).unwrap();
        assert!(d.example);
        // The origin is the least easting and northing of the structures, times 1000, to a thousandth of a millimetre.
        let bounds = cp::bounds_of(&d.network).unwrap();
        assert!((a.origin.x - bounds.min_e * 1000.0).abs() < 1e-3 && (a.origin.y - bounds.min_n * 1000.0).abs() < 1e-3);
        assert!(bounds.min_e > E0 && bounds.min_n > N0, "true UTM size: {bounds:?}");
        // The scene: centrelines at the source coordinates times 1000, and the 3D solids at origin plus local.
        let ctx = crate::civil_build::Ctx { elements: doc.elements().to_vec(), tins: Default::default() };
        let built = cp::build_pipenet(a, &ctx).unwrap();
        assert!(built.scene.notes.is_empty(), "{:?}", built.scene.notes);
        let first = d.network.pipes().next().unwrap();
        let path = d.network.pipe_path(first.id).unwrap();
        assert!(built.scene.lines.iter().any(|l| (l.pts[0][0] - path.points[0][0] * 1000.0).abs() < 1e-3 && (l.pts[0][1] - path.points[0][1] * 1000.0).abs() < 1e-3));
        let s0 = d.network.structures().next().unwrap();
        let m = built.scene.meshes.iter().find(|m| m.name == s0.name).unwrap();
        let cx = m.vertices.iter().map(|v| v[0]).sum::<f64>() / m.vertices.len() as f64;
        assert!((a.origin.x + cx - s0.easting_m * 1000.0).abs() < 1.0, "the solid is at the structure");
        // The element reads back through its module.
        let mut reg = afrobim_kernel::ModuleRegistry::default();
        civil::register(&mut reg);
        reg.generate(a).unwrap();
    }

    #[test]
    fn the_pipe_profile_draws_ground_structures_pipes_and_the_grade_line() {
        let (doc, _) = demo();
        let pp = doc.elements().iter().find(|e| e.category == PIPEPROFILE).expect("a pipe profile");
        let ctx = crate::civil_build::Ctx { elements: doc.elements().to_vec(), tins: Default::default() };
        let built = crate::civil_pipes::build_pipe_profile(pp, &ctx).unwrap();
        assert!(built.scene.derived.iter().any(|d| d.0 == "Grade lines"), "{:?}", built.scene.derived);
        assert!(built.scene.lines.len() > 10 && !built.scene.fills.is_empty());
        // The view sits at its insertion point.
        let data: PipeProfileData = serde_json::from_value(pp.data.clone()).unwrap();
        let b = crate::civil_build::scene_bounds(&built.scene).unwrap();
        assert!(b.min_n <= data.insertion[1] + 1.0 && b.max_n > data.insertion[1]);
    }

    #[test]
    fn pipes_crossing_a_section_line_are_drawn_in_the_section_views() {
        let (doc, _) = demo();
        let ctx = crate::civil_build::Ctx { elements: doc.elements().to_vec(), tins: Default::default() };
        // The ground surface of the demonstration is needed by the sample lines: build it as the application does.
        let mut tins = std::collections::HashMap::new();
        for e in doc.elements().iter().filter(|e| e.category == civil::SURFACE) {
            if let Ok(b) = std::panic::catch_unwind(|| crate::civil_build::build(e, &ctx)) {
                if let Some(t) = b.tin {
                    tins.insert(e.id.0, t);
                }
            }
        }
        let ctx = crate::civil_build::Ctx { elements: doc.elements().to_vec(), tins };
        let samples = doc.elements().iter().find(|e| e.category == civil::SAMPLES).unwrap();
        let built = crate::civil_build2::build_samples(samples, &ctx).unwrap();
        let blue = built.scene.lines.iter().filter(|l| l.rgb == [0x4D, 0xA3, 0xFF]).count();
        assert!(blue >= 2, "the run (stations 8 to 83 m of the first straight) crosses the section lines at 30 and 60 m: {blue}");
    }

    #[test]
    fn the_landxml_export_carries_the_network_and_reads_back_at_the_same_coordinates() {
        let (doc, id) = demo();
        let ctx = crate::civil_build::Ctx { elements: doc.elements().to_vec(), tins: Default::default() };
        let (text, _) = crate::civil_export::landxml_text(&doc, &ctx);
        assert!(text.contains("<PipeNetworks"));
        let back = afrobim_pipes::parse_landxml_pipes(&text).expect("the pipes read back");
        let d = cp::data_of(&doc, id).unwrap();
        assert_eq!(back.networks.len(), 1);
        let n = &back.networks[0];
        assert_eq!((n.structure_count(), n.pipe_count()), (d.network.structure_count(), d.network.pipe_count()));
        for (a, b) in d.network.structures().zip(n.structures()) {
            assert!((a.easting_m - b.easting_m).abs() < 1e-9 && (a.northing_m - b.northing_m).abs() < 1e-9 && (a.rim_level_m - b.rim_level_m).abs() < 1e-9);
        }
        // The civil reader still reads the whole file.
        assert!(afrobim_alignment::parse_landxml_file(&text).is_ok());
    }

    #[test]
    fn the_checks_run_on_the_demonstration_network_and_say_what_they_skipped() {
        let (doc, id) = demo();
        let d = cp::data_of(&doc, id).unwrap();
        let out = run_checks(id, d, None, vec![]);
        assert!(out.notes.iter().any(|n| n.contains("Cover was not checked")), "no surface was given: {:?}", out.notes);
        assert!(out.hydraulics.as_ref().is_some_and(|h| h.pipes.len() == 3));
        assert!(out.grade.is_some(), "the tailwater level was given");
        assert!(out.positions.len() >= 7, "a position for every structure and pipe, for the zoom");
    }

    #[test]
    fn the_edit_tools_change_the_network_and_refuse_what_cannot_be_done() {
        let (doc, id) = demo();
        let mut app = app();
        app.doc = doc;
        let form = |tool: usize| EditForm { net: Some(id), tool, pipe: None, structure: None, name: String::new(), kind: 0, e: String::new(), n: String::new(), rim: String::new(), at_chainage: String::new(), merge: false, part: 0, inv_mode: 0, anchor_end: 0, anchor_invert: String::new(), slope_pct: String::new(), drop: String::new(), match_mode: 0, cover: String::new(), flow: String::new(), inflow: String::new(), loss_k: String::new(), sump: String::new(), message: String::new() };
        let before = cp::data_of(&app.doc, id).unwrap().network;
        let (p1, s2) = (before.pipes().next().unwrap().id, before.structures().nth(1).unwrap().id);
        // Insert a structure in the first pipe at a typed position with a typed rim level.
        let mid = before.pipe_path(p1).unwrap().point_at(before.plan_length_m(p1).unwrap() / 2.0);
        let mut f = form(0);
        f.pipe = Some(p1.0);
        f.e = format!("{}", mid[0]);
        f.n = format!("{}", mid[1]);
        f.rim = "1151.0".into();
        app.pipe_edit_apply(&f).unwrap();
        let d = cp::data_of(&app.doc, id).unwrap();
        assert_eq!((d.network.structure_count(), d.network.pipe_count()), (before.structure_count() + 1, before.pipe_count() + 1));
        // Move a structure; its pipes follow.
        let mut f = form(1);
        f.structure = Some(s2.0);
        f.e = format!("{}", before.structure(s2).unwrap().easting_m + 1.0);
        f.n = format!("{}", before.structure(s2).unwrap().northing_m);
        app.pipe_edit_apply(&f).unwrap();
        let d = cp::data_of(&app.doc, id).unwrap();
        assert!((d.network.structure(s2).unwrap().easting_m - before.structure(s2).unwrap().easting_m - 1.0).abs() < 1e-9);
        // Set the inverts of a pipe by slope from its start.
        let mut f = form(4);
        f.pipe = Some(p1.0);
        f.anchor_invert = "1149.5".into();
        f.slope_pct = "2".into();
        app.pipe_edit_apply(&f).unwrap();
        let d = cp::data_of(&app.doc, id).unwrap();
        let p = d.network.pipe(p1).unwrap();
        assert!((p.start_invert_m - 1149.5).abs() < 1e-9);
        assert!((d.network.slope(p1).unwrap() - 0.02).abs() < 1e-9);
        // Cover under the surface needs the surface the network has; a missing slope is refused with the reason.
        let mut f = form(4);
        f.inv_mode = 0;
        f.pipe = Some(p1.0);
        assert!(app.pipe_edit_apply(&f).unwrap_err().contains("not a number"));
        // Delete a structure and join the pipes either side.
        let mut f = form(2);
        f.structure = Some(s2.0);
        f.merge = true;
        let n0 = cp::data_of(&app.doc, id).unwrap().network.pipe_count();
        app.pipe_edit_apply(&f).unwrap();
        assert!(cp::data_of(&app.doc, id).unwrap().network.pipe_count() < n0);
        // Flows and losses.
        let mut f = form(5);
        f.pipe = Some(p1.0);
        f.flow = "0.2".into();
        app.pipe_edit_apply(&f).unwrap();
        assert_eq!(cp::data_of(&app.doc, id).unwrap().network.pipe(p1).ok().and_then(|p| p.design_flow_m3_s), Some(0.2));
    }
}
