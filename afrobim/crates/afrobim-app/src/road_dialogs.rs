//! The dialogs of street lighting, road restraint, pavement, pavement design and corridors, and
//! the cross-section window.
//!
//! A dialog collects only what cannot be defaulted. Numbers start at zero, which means "not
//! entered": the crates refuse a definition they cannot use and the reason is shown. Stations are
//! the stations shown on the drawings, converted to internal stations (see `station_input`).

use afrobim_corridor as corridor;
use afrobim_draw::{Colour, Primitive};
use afrobim_kernel::{Document, ElementId};
use afrobim_pavement::design::{dmrb, mowt};
use afrobim_restraint as restraint;
use egui::{Color32, RichText, Ui};

use crate::alignment_glue;
use crate::station_input::{self, station_row};
use crate::road_glue::{
    self, AashtoForm, ClassRow, ColumnFields, ColumnRequest, CorridorRequest, DesignForm, DmrbForm, LayerRow, LightingRequest, MowtForm, PavementRequest, RestraintRequest, ShoulderRequest, SnRow, SpeedInput,
};

/// What a dialog hands back to the app.
pub enum Outcome {
    /// An element was added with this mark; the string is the line for the status bar.
    Added(String, String),
    /// A calculation to show in the calculation window: its subject and its lines.
    Report(String, Vec<String>),
    /// A line for the status bar (a refusal, with its reason).
    Status(String),
}

/// The pavement design dialog.
pub struct DesignDialog {
    form: DesignForm,
    /// The pavement the inputs are kept on (`check.pavement_design`), so the model-wide check can
    /// compare the design with its layers. None: the design is run without being kept.
    element: Option<ElementId>,
    /// The layers the last run arrived at, when the method gives them.
    layers: Option<Vec<afrobim_pavement::Layer>>,
    make: Option<(Option<ElementId>, PavementRequest)>,
}

/// The cross-section window: one corridor, built, and the section being looked at.
pub struct SectionsView {
    pub mark: String,
    corridor: corridor::Corridor,
    index: usize,
    exaggeration: f64,
}

#[derive(Default)]
pub struct RoadDialogs {
    lighting: Option<(Option<ElementId>, LightingRequest)>,
    column: Option<(Option<ElementId>, ColumnRequest)>,
    barrier: Option<(Option<ElementId>, RestraintRequest)>,
    pavement: Option<(Option<ElementId>, PavementRequest)>,
    speed: Option<(ElementId, SpeedInput)>,
    design: Option<DesignDialog>,
    corridor: Option<(Option<ElementId>, CorridorRequest)>,
    sections: Option<SectionsView>,
    lighting_check: Option<crate::lighting_inputs::LightingCheck>,
}

const NO_ALIGNMENT: &str = "There is no alignment yet. Import one with File → Import LandXML.";

/// The first alignment of the document with its first and last internal station.
fn first_alignment(doc: &Document) -> Result<(ElementId, f64, f64), String> {
    let (id, _) = alignment_glue::alignments(doc).into_iter().next().ok_or(NO_ALIGNMENT)?;
    let a = alignment_glue::alignment_of(doc, id).ok_or("That alignment cannot be read.")?;
    Ok((id, a.start_station_m, a.end_station_m()))
}

impl RoadDialogs {
    pub fn open_lighting(&mut self, doc: &Document) -> Result<(), String> {
        let (id, start, end) = first_alignment(doc)?;
        self.lighting = Some((Some(id), LightingRequest::empty(&doc.unique_name("LZ-01"), start, end)));
        Ok(())
    }

    pub fn open_column(&mut self, doc: &Document) -> Result<(), String> {
        let (id, start, _) = first_alignment(doc)?;
        self.column = Some((Some(id), ColumnRequest::empty(&doc.unique_name("LC-001"), start)));
        Ok(())
    }

    pub fn open_barrier(&mut self, doc: &Document) -> Result<(), String> {
        let (id, start, end) = first_alignment(doc)?;
        self.barrier = Some((Some(id), RestraintRequest::empty(&doc.unique_name("VRS-001"), start, end)));
        Ok(())
    }

    pub fn open_pavement(&mut self, doc: &Document) -> Result<(), String> {
        let (id, start, end) = first_alignment(doc)?;
        self.pavement = Some((Some(id), PavementRequest::empty(&doc.unique_name("PV-001"), start, end)));
        Ok(())
    }

    /// The lighting check dialog, with the entries kept on the element when there are some.
    pub fn open_lighting_check(&mut self, doc: &Document, id: ElementId) {
        self.lighting_check = Some(crate::lighting_inputs::LightingCheck::open(doc, id));
    }

    /// The speed limit dialog of a barrier run, with the entry kept on it when there is one.
    pub fn open_speed(&mut self, doc: &Document, id: ElementId) {
        let stored: Option<SpeedInput> = crate::check_inputs::load(doc, id, crate::check_inputs::KEY_RESTRAINT);
        self.speed = Some((id, stored.unwrap_or(SpeedInput { value: 0.0, mph: false, answer: None })));
    }

    /// The pavement design dialog. With a pavement element the entries are kept on it and the
    /// dialog opens with the ones already kept.
    pub fn open_design(&mut self, doc: &Document, element: Option<ElementId>) {
        let element = element.filter(|id| doc.find(*id).is_some_and(|e| e.category == afrobim_pavement::PAVEMENT_CATEGORY));
        let stored: Option<DesignForm> = element.and_then(|id| crate::check_inputs::load(doc, id, crate::check_inputs::KEY_PAVEMENT));
        match &mut self.design {
            Some(_) if element.is_none() => {}
            Some(d) => {
                d.element = element;
                if let Some(form) = stored {
                    d.form = form;
                }
            }
            None => self.design = Some(DesignDialog { form: stored.unwrap_or_default(), element, layers: None, make: None }),
        }
    }

    pub fn open_corridor(&mut self, doc: &Document) -> Result<(), String> {
        let (id, _, _) = first_alignment(doc)?;
        let surface = afrobim_corridor::surfaces(doc).first().map(|s| s.0);
        if surface.is_none() {
            return Err("There is no ground surface yet. Import one with SURFACE.".into());
        }
        let mut req = CorridorRequest::empty(&doc.unique_name("Corridor"));
        req.surface = surface;
        self.corridor = Some((Some(id), req));
        Ok(())
    }

    pub fn open_sections(&mut self, doc: &Document, id: ElementId) -> Result<(), String> {
        let built = road_glue::built_corridor(doc, id)?;
        if built.sections.is_empty() {
            return Err("The corridor has no sections.".into());
        }
        let mark = doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
        self.sections = Some(SectionsView { mark, corridor: built, index: 0, exaggeration: 1.0 });
        Ok(())
    }

    /// Draws every open dialog and returns what they produced this frame.
    pub fn show(&mut self, ctx: &egui::Context, doc: &mut Document) -> Vec<Outcome> {
        let mut out = Vec::new();
        lighting_dialog(ctx, doc, &mut self.lighting, &mut out);
        column_dialog(ctx, doc, &mut self.column, &mut out);
        barrier_dialog(ctx, doc, &mut self.barrier, &mut out);
        pavement_dialog(ctx, doc, &mut self.pavement, &mut self.design, &mut out);
        speed_dialog(ctx, doc, &mut self.speed, &mut out);
        design_dialog(ctx, doc, &mut self.design, &mut out);
        corridor_dialog(ctx, doc, &mut self.corridor, &mut out);
        sections_window(ctx, &mut self.sections);
        crate::lighting_inputs::show(ctx, doc, &mut self.lighting_check, &mut out);
        out
    }
}

// ─── small widgets ──────────────────────────────────────────────────────────────────────

const NOTE: Color32 = Color32::from_rgb(0xD9, 0x8A, 0x2B);

fn number(ui: &mut Ui, value: &mut f64, speed: f64, suffix: &str) {
    ui.add(egui::DragValue::new(value).speed(speed).suffix(suffix));
}

fn whole(ui: &mut Ui, value: &mut u32, suffix: &str) {
    ui.add(egui::DragValue::new(value).speed(1.0).suffix(suffix));
}

/// A number the engineer may leave out: a tick box and the value beside it.
fn optional(ui: &mut Ui, value: &mut Option<f64>, speed: f64, suffix: &str) {
    ui.horizontal(|ui| {
        let mut on = value.is_some();
        if ui.checkbox(&mut on, "").changed() {
            *value = on.then_some(0.0);
        }
        if let Some(v) = value {
            number(ui, v, speed, suffix);
        } else {
            ui.label(RichText::new("not entered").weak());
        }
    });
}

fn text(ui: &mut Ui, value: &mut String, hint: &str, width: f32) {
    ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(width));
}

fn alignment_row(ui: &mut Ui, salt: &str, choices: &[(ElementId, String)], alignment: &mut Option<ElementId>) {
    ui.label("Alignment");
    let current = choices.iter().find(|c| Some(c.0) == *alignment).map(|c| c.1.clone()).unwrap_or_default();
    egui::ComboBox::from_id_salt(salt).selected_text(current).show_ui(ui, |ui| {
        for (id, name) in choices {
            ui.selectable_value(alignment, Some(*id), name);
        }
    });
    ui.end_row();
}

fn pick<T: PartialEq + Copy>(ui: &mut Ui, salt: &str, current: &mut T, options: &[(T, String)]) {
    let shown = options.iter().find(|o| o.0 == *current).map(|o| o.1.clone()).unwrap_or_default();
    egui::ComboBox::from_id_salt(salt).selected_text(shown).show_ui(ui, |ui| {
        for (value, label) in options {
            ui.selectable_value(current, *value, label);
        }
    });
}

fn index_options(labels: Vec<String>) -> Vec<(usize, String)> {
    labels.into_iter().enumerate().collect()
}

fn buttons(ui: &mut Ui, ok: &str, enabled: bool, add: &mut bool, keep: &mut bool) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.add_enabled(enabled, egui::Button::new(ok)).clicked() {
            *add = true;
        }
        if ui.button("Cancel").clicked() {
            *keep = false;
        }
    });
}

fn column_fields(ui: &mut Ui, f: &mut ColumnFields) {
    ui.label("Mounting height");
    number(ui, &mut f.mounting_height_m, 0.1, " m");
    ui.end_row();
    ui.label("Bracket outreach");
    number(ui, &mut f.bracket_outreach_m, 0.05, " m");
    ui.end_row();
    ui.label("Tilt");
    number(ui, &mut f.tilt_deg, 0.5, "°");
    ui.end_row();
    ui.label("Luminaire product");
    text(ui, &mut f.product, "make and model", 200.0);
    ui.end_row();
    ui.label("Lumen output");
    number(ui, &mut f.lumen_output_lm, 100.0, " lm");
    ui.end_row();
    ui.label("Power");
    number(ui, &mut f.power_w, 1.0, " W");
    ui.end_row();
}

fn shoulder_row(ui: &mut Ui, label: &str, shoulder: &mut Option<ShoulderRequest>) {
    ui.label(label);
    ui.horizontal(|ui| {
        let mut on = shoulder.is_some();
        if ui.checkbox(&mut on, "").changed() {
            *shoulder = on.then(ShoulderRequest::default);
        }
        if let Some(s) = shoulder {
            number(ui, &mut s.width_m, 0.1, " m wide");
            number(ui, &mut s.crossfall_percent, 0.1, " % fall");
        } else {
            ui.label(RichText::new("none").weak());
        }
    });
    ui.end_row();
}

fn layer_list(ui: &mut Ui, salt: &str, rows: &mut Vec<LayerRow>) {
    let materials: Vec<(usize, String)> = index_options(road_glue::materials().iter().map(|m| m.label().to_string()).collect());
    let mut remove = None;
    egui::Grid::new(format!("{salt}-layers")).num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
        for (i, row) in rows.iter_mut().enumerate() {
            text(ui, &mut row.name, "layer name", 150.0);
            pick(ui, &format!("{salt}-material-{i}"), &mut row.material, &materials);
            number(ui, &mut row.thickness_mm, 5.0, " mm");
            if ui.small_button("x").on_hover_text("Remove this layer").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        rows.remove(i);
    }
    if ui.button("Add a layer").clicked() {
        rows.push(LayerRow::empty());
    }
}

fn not_a_design(ui: &mut Ui, what: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(what).small().color(NOTE));
}

// ─── street lighting ────────────────────────────────────────────────────────────────────

fn lighting_dialog(ctx: &egui::Context, doc: &mut Document, slot: &mut Option<(Option<ElementId>, LightingRequest)>, out: &mut Vec<Outcome>) {
    let Some((mut alignment, mut req)) = slot.take() else { return };
    let choices = alignment_glue::alignments(doc);
    let (mut keep, mut add) = (true, false);
    crate::win::new(ctx, "Street lighting installation").collapsible(false).resizable(false).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
            egui::Grid::new("lighting").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                alignment_row(ui, "lighting-alignment", &choices, &mut alignment);
                ui.label("Reference");
                text(ui, &mut req.reference, "e.g. LZ-01", 120.0);
                ui.end_row();
                station_row(ui, doc, alignment, "lighting-start", "Start station", &mut req.start_station_m);
                station_row(ui, doc, alignment, "lighting-end", "End station", &mut req.end_station_m);
                ui.label("Arrangement");
                let arrangements = index_options(road_glue::ARRANGEMENTS.iter().map(|a| a.label().to_string()).collect());
                pick(ui, "lighting-arrangement", &mut req.arrangement, &arrangements);
                ui.end_row();
                ui.label("First column on");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut req.first_side_right, false, "Left");
                    ui.selectable_value(&mut req.first_side_right, true, "Right");
                });
                ui.end_row();
                ui.label("Spacing");
                number(ui, &mut req.spacing_m, 0.5, " m");
                ui.end_row();
                ui.label("At the end");
                let ends = index_options(road_glue::END_TREATMENTS.iter().map(|e| e.1.to_string()).collect());
                pick(ui, "lighting-end", &mut req.end_treatment, &ends);
                ui.end_row();
                ui.label("Carriageway width");
                number(ui, &mut req.carriageway_width_m, 0.1, " m");
                ui.end_row();
                ui.label("Carriageway centre offset");
                number(ui, &mut req.reference_offset_m, 0.1, " m");
                ui.end_row();
                ui.label("Set-back from the edge");
                number(ui, &mut req.setback_m, 0.1, " m");
                ui.end_row();
                column_fields(ui, &mut req.column);
                ui.label("Lighting class");
                text(ui, &mut req.lighting_class, "as the engineer enters it", 160.0);
                ui.end_row();
            });
        });
        not_a_design(ui, "Nothing here is a design value: spacing, set-back, mounting height and the class are the engineer's. AfroBIM does no photometric, structural or electrical design. The offset is positive to the right.");
        ui.label(RichText::new(station_input::note(doc, alignment)).small().weak());
        buttons(ui, "Add", alignment.is_some(), &mut add, &mut keep);
    });
    if add {
        match alignment.ok_or_else(|| "Choose an alignment.".to_string()).and_then(|a| road_glue::add_lighting(doc, a, &req)) {
            Ok(mark) => out.push(Outcome::Added(mark.clone(), format!("Added {mark}. Select it and run CHECK to see which requirements need your input."))),
            Err(e) => {
                out.push(Outcome::Status(format!("Not added: {e}")));
                *slot = Some((alignment, req));
            }
        }
    } else if keep {
        *slot = Some((alignment, req));
    }
}

fn column_dialog(ctx: &egui::Context, doc: &mut Document, slot: &mut Option<(Option<ElementId>, ColumnRequest)>, out: &mut Vec<Outcome>) {
    let Some((mut alignment, mut req)) = slot.take() else { return };
    let choices = alignment_glue::alignments(doc);
    let (mut keep, mut add) = (true, false);
    crate::win::new(ctx, "Lighting column").collapsible(false).resizable(false).show(ctx, |ui| {
        egui::Grid::new("column").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            alignment_row(ui, "column-alignment", &choices, &mut alignment);
            ui.label("Mark");
            text(ui, &mut req.mark, "e.g. LC-001", 120.0);
            ui.end_row();
            station_row(ui, doc, alignment, "column-station", "Station", &mut req.station_m);
            ui.label("Offset from the alignment");
            ui.horizontal(|ui| {
                number(ui, &mut req.distance_m, 0.1, " m");
                ui.selectable_value(&mut req.right, false, "Left");
                ui.selectable_value(&mut req.right, true, "Right");
            });
            ui.end_row();
            ui.label("Bracket points");
            ui.horizontal(|ui| {
                let mut toward = req.bearing_deg.is_none();
                if ui.radio_value(&mut toward, true, "square to the alignment").changed() && toward {
                    req.bearing_deg = None;
                }
                if ui.radio_value(&mut toward, false, "on a bearing").changed() && !toward {
                    req.bearing_deg = Some(0.0);
                }
                if let Some(b) = &mut req.bearing_deg {
                    number(ui, b, 1.0, "°");
                }
            });
            ui.end_row();
            column_fields(ui, &mut req.column);
            ui.label("Lighting class");
            text(ui, &mut req.lighting_class, "as the engineer enters it", 160.0);
            ui.end_row();
        });
        not_a_design(ui, "For a junction or conflict area. Mounting height, outreach and the luminaire are entered, not designed.");
        ui.label(RichText::new(station_input::note(doc, alignment)).small().weak());
        buttons(ui, "Add", alignment.is_some(), &mut add, &mut keep);
    });
    if add {
        match alignment.ok_or_else(|| "Choose an alignment.".to_string()).and_then(|a| road_glue::add_light_column(doc, a, &req)) {
            Ok(mark) => out.push(Outcome::Added(mark.clone(), format!("Added {mark}."))),
            Err(e) => {
                out.push(Outcome::Status(format!("Not added: {e}")));
                *slot = Some((alignment, req));
            }
        }
    } else if keep {
        *slot = Some((alignment, req));
    }
}

// ─── road restraint ─────────────────────────────────────────────────────────────────────

fn barrier_dialog(ctx: &egui::Context, doc: &mut Document, slot: &mut Option<(Option<ElementId>, RestraintRequest)>, out: &mut Vec<Outcome>) {
    let Some((mut alignment, mut req)) = slot.take() else { return };
    let choices = alignment_glue::alignments(doc);
    let (mut keep, mut add) = (true, false);
    crate::win::new(ctx, "Safety barrier run").collapsible(false).resizable(false).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
            egui::Grid::new("barrier").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                alignment_row(ui, "barrier-alignment", &choices, &mut alignment);
                ui.label("Mark");
                text(ui, &mut req.name, "e.g. VRS-001", 120.0);
                ui.end_row();
                station_row(ui, doc, alignment, "barrier-start", "Start station", &mut req.start_station_m);
                station_row(ui, doc, alignment, "barrier-end", "End station", &mut req.end_station_m);
                ui.label("Traffic face at");
                ui.horizontal(|ui| {
                    number(ui, &mut req.distance_m, 0.1, " m");
                    ui.selectable_value(&mut req.right, false, "Left");
                    ui.selectable_value(&mut req.right, true, "Right");
                });
                ui.end_row();
                ui.label("Traffic on the");
                let faces = index_options(road_glue::FACES.iter().map(|f| f.1.to_string()).collect());
                pick(ui, "barrier-face", &mut req.face, &faces);
                ui.end_row();
                ui.label("Containment level");
                let levels = index_options(restraint::ContainmentLevel::ALL.iter().map(|c| c.label().to_string()).collect());
                pick(ui, "barrier-containment", &mut req.containment, &levels);
                ui.end_row();
                ui.label("Working width class");
                let widths = index_options(restraint::WorkingWidthClass::ALL.iter().map(|c| c.label().to_string()).collect());
                pick(ui, "barrier-width", &mut req.working_width, &widths);
                ui.end_row();
                ui.label("Impact severity level");
                let severities = index_options(["A", "B", "C"].iter().map(|s| s.to_string()).collect());
                pick(ui, "barrier-severity", &mut req.impact_severity, &severities);
                ui.end_row();
                ui.label("Beam height");
                number(ui, &mut req.beam_height_m, 0.01, " m");
                ui.end_row();
                ui.label("Post spacing");
                optional(ui, &mut req.post_spacing_m, 0.1, " m");
                ui.end_row();
                ui.label("Set-back");
                optional(ui, &mut req.set_back_m, 0.1, " m");
                ui.end_row();
                ui.label("System or product");
                text(ui, &mut req.system, "named from a product list", 200.0);
                ui.end_row();
                let terminals = restraint_terminal_options();
                for (label, salt, which) in [("Start terminal", "barrier-start", &mut req.start_terminal), ("End terminal", "barrier-end", &mut req.end_terminal)] {
                    ui.label(label);
                    let mut choice: i64 = which.map(|i| i as i64).unwrap_or(-1);
                    egui::ComboBox::from_id_salt(salt).selected_text(terminals.iter().find(|t| t.0 == choice).map(|t| t.1.clone()).unwrap_or_default()).show_ui(ui, |ui| {
                        for (value, name) in &terminals {
                            ui.selectable_value(&mut choice, *value, name);
                        }
                    });
                    *which = (choice >= 0).then_some(choice as usize);
                    ui.end_row();
                }
            });
        });
        not_a_design(ui, "The class, working width and severity are what the engineer chooses (CD 377 and the risk assessment). Run CHECK for the requirement checks.");
        ui.label(RichText::new(station_input::note(doc, alignment)).small().weak());
        buttons(ui, "Add", alignment.is_some(), &mut add, &mut keep);
    });
    if add {
        match alignment.ok_or_else(|| "Choose an alignment.".to_string()).and_then(|a| road_glue::add_restraint(doc, a, &req)) {
            Ok(mark) => out.push(Outcome::Added(mark.clone(), format!("Added {mark}. Run CHECK for its requirement checks."))),
            Err(e) => {
                out.push(Outcome::Status(format!("Not added: {e}")));
                *slot = Some((alignment, req));
            }
        }
    } else if keep {
        *slot = Some((alignment, req));
    }
}

fn restraint_terminal_options() -> Vec<(i64, String)> {
    let mut v = vec![(-1, "not stated".to_string())];
    v.extend(road_glue::terminal_classes().iter().enumerate().map(|(i, t)| (i as i64, t.label())));
    v
}

fn speed_dialog(ctx: &egui::Context, doc: &mut Document, slot: &mut Option<(ElementId, SpeedInput)>, out: &mut Vec<Outcome>) {
    let Some((id, mut speed)) = slot.take() else { return };
    let mark = doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
    let (mut keep, mut run) = (true, false);
    crate::win::new(ctx, format!("Check {mark}")).collapsible(false).resizable(false).show(ctx, |ui| {
        egui::Grid::new("speed").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            ui.label("Speed limit");
            ui.horizontal(|ui| {
                number(ui, &mut speed.value, 1.0, "");
                ui.selectable_value(&mut speed.mph, false, "km/h");
                ui.selectable_value(&mut speed.mph, true, "mph");
            });
            ui.end_row();
        });
        // How the limit meets the 50 mph step is the project's rule (PROJECTSETTINGS).
        if speed.value > 0.0 {
            let r = crate::projset::resolve_answered(doc, speed.value, speed.mph, &crate::projset::restraint_thresholds(), speed.answer);
            ui.label(egui::RichText::new(format!("Speed band: {}", r.band_note)).small());
            if let Some(ask) = r.ask {
                ui.label(egui::RichText::new(format!("The project rule asks each time. Treat {:.2} mph as the {} mph band?", ask.exact_mph, ask.band_mph)).color(egui::Color32::from_rgb(0xD9, 0x8A, 0x2B)));
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut speed.answer, Some(true), format!("Yes, treat it as {} mph", ask.band_mph));
                    ui.selectable_value(&mut speed.answer, Some(false), "No, keep the exact value");
                });
            }
        }
        not_a_design(ui, "The checks need the speed limit. Every other input is left not entered, and the report says which checks it stops.");
        buttons(ui, "Check", true, &mut run, &mut keep);
    });
    if run {
        match road_glue::check_restraint(doc, id, speed) {
            Ok(lines) => {
                // The speed limit stays on the run for the model-wide check.
                crate::check_inputs::save(doc, id, crate::check_inputs::KEY_RESTRAINT, &speed);
                out.push(Outcome::Report(mark, lines));
            }
            Err(e) => {
                out.push(Outcome::Status(format!("Not checked: {e}")));
                *slot = Some((id, speed));
            }
        }
    } else if keep {
        *slot = Some((id, speed));
    }
}

// ─── pavement ───────────────────────────────────────────────────────────────────────────

fn pavement_dialog(ctx: &egui::Context, doc: &mut Document, slot: &mut Option<(Option<ElementId>, PavementRequest)>, design: &mut Option<DesignDialog>, out: &mut Vec<Outcome>) {
    let Some((mut alignment, mut req)) = slot.take() else { return };
    let choices = alignment_glue::alignments(doc);
    let (mut keep, mut add, mut design_first) = (true, false, false);
    crate::win::new(ctx, "Pavement").collapsible(false).resizable(false).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
            egui::Grid::new("pavement").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                alignment_row(ui, "pavement-alignment", &choices, &mut alignment);
                ui.label("Mark");
                text(ui, &mut req.name, "e.g. PV-001", 120.0);
                ui.end_row();
                station_row(ui, doc, alignment, "pavement-start", "Start station", &mut req.start_station_m);
                station_row(ui, doc, alignment, "pavement-end", "End station", &mut req.end_station_m);
                ui.label("Carriageway width");
                number(ui, &mut req.carriageway_width_m, 0.1, " m");
                ui.end_row();
                ui.label("Cross-fall");
                number(ui, &mut req.crossfall_percent, 0.1, " %");
                ui.end_row();
                ui.label("Shape");
                let shapes = index_options(vec!["Crowned, falling to both sides".into(), "One way, falling to the left".into(), "One way, falling to the right".into()]);
                pick(ui, "pavement-shape", &mut req.shape, &shapes);
                ui.end_row();
                shoulder_row(ui, "Left shoulder", &mut req.left_shoulder);
                shoulder_row(ui, "Right shoulder", &mut req.right_shoulder);
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Layers, top first").strong());
            layer_list(ui, "pavement", &mut req.layers);
        });
        not_a_design(ui, "Enter the layers, or run a design method first. Shoulders take the layers of the carriageway; edit them in Properties.");
        ui.label(RichText::new(station_input::note(doc, alignment)).small().weak());
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(alignment.is_some(), egui::Button::new("Add")).clicked() {
                add = true;
            }
            if ui.button("Design the layers first…").clicked() {
                design_first = true;
            }
            if ui.button("Cancel").clicked() {
                keep = false;
            }
        });
    });
    if design_first {
        if design.is_none() {
            *design = Some(DesignDialog { form: DesignForm::default(), element: None, layers: None, make: None });
        }
        keep = true;
    }
    if add {
        match alignment.ok_or_else(|| "Choose an alignment.".to_string()).and_then(|a| road_glue::add_pavement(doc, a, &req)) {
            Ok(mark) => out.push(Outcome::Added(mark.clone(), format!("Added {mark}."))),
            Err(e) => {
                out.push(Outcome::Status(format!("Not added: {e}")));
                *slot = Some((alignment, req));
            }
        }
    } else if keep {
        *slot = Some((alignment, req));
    }
}

// ─── pavement design ────────────────────────────────────────────────────────────────────

fn design_dialog(ctx: &egui::Context, doc: &mut Document, slot: &mut Option<DesignDialog>, out: &mut Vec<Outcome>) {
    let Some(mut d) = slot.take() else { return };
    let choices = alignment_glue::alignments(doc);
    let (mut keep, mut run, mut create) = (true, false, false);
    crate::win::new(ctx, "Pavement design").collapsible(false).resizable(true).default_width(560.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Method");
            for (i, m) in road_glue::METHODS.iter().enumerate() {
                ui.selectable_value(&mut d.form.method, i, m.name());
            }
        });
        ui.label(RichText::new(road_glue::METHODS[d.form.method.min(2)].reference()).small().weak());
        ui.horizontal(|ui| {
            ui.label("Keep the inputs on pavement");
            let pavements = afrobim_pavement::pavements(doc);
            let current = pavements.iter().find(|p| Some(p.0) == d.element).map(|p| p.1.clone()).unwrap_or_else(|| "none (run only)".to_string());
            egui::ComboBox::from_id_salt("design-element").selected_text(current).show_ui(ui, |ui| {
                ui.selectable_value(&mut d.element, None, "none (run only)");
                for (id, name, _) in &pavements {
                    ui.selectable_value(&mut d.element, Some(*id), name);
                }
            });
        });
        ui.label(RichText::new("Inputs kept on a pavement are used by CHECKALL, which compares the method's layers with the pavement's own.").small().weak());
        ui.separator();
        egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| match d.form.method {
            0 => mowt_form(ui, &mut d.form.mowt),
            1 => dmrb_form(ui, &mut d.form.dmrb),
            _ => aashto_form(ui, &mut d.form.aashto),
        });
        not_a_design(ui, "Every figure is the engineer's: traffic, strength, climate and the AASHTO coefficients are not defaulted. The methods are not interchangeable. The result is a calculation for an engineer to check, not a certified design; the warnings of the method are in the calculation.");
        if let Some(layers) = &d.layers {
            ui.separator();
            ui.label(RichText::new("The method arrives at these layers").strong());
            for l in layers {
                ui.label(format!("{}: {} mm", l.name, l.thickness_mm));
            }
            if let Some((alignment, req)) = &mut d.make {
                egui::Grid::new("design-make").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    alignment_row(ui, "design-alignment", &choices, alignment);
                    ui.label("Mark");
                    text(ui, &mut req.name, "e.g. PV-001", 120.0);
                    ui.end_row();
                    ui.label("Carriageway width");
                    number(ui, &mut req.carriageway_width_m, 0.1, " m");
                    ui.end_row();
                    ui.label("Cross-fall");
                    number(ui, &mut req.crossfall_percent, 0.1, " %");
                    ui.end_row();
                });
                if ui.add_enabled(alignment.is_some(), egui::Button::new("Make a pavement from these layers")).clicked() {
                    create = true;
                }
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Run the method").clicked() {
                run = true;
            }
            if ui.button("Close").clicked() {
                keep = false;
            }
        });
    });
    if run {
        match d.form.run() {
            Ok(result) => {
                if let Some(element) = d.element {
                    crate::check_inputs::save(doc, element, crate::check_inputs::KEY_PAVEMENT, &d.form);
                }
                let title = road_glue::METHODS[d.form.method.min(2)].name().to_string();
                out.push(Outcome::Report(title, result.lines));
                d.make = result.layers.as_ref().and_then(|layers| {
                    let (id, start, end) = first_alignment(doc).ok()?;
                    let mut req = PavementRequest::empty(&doc.unique_name("PV-001"), start, end);
                    req.layers = LayerRow::from_layers(layers);
                    Some((Some(id), req))
                });
                d.layers = result.layers;
            }
            Err(e) => {
                out.push(Outcome::Status(format!("Not designed: {e}")));
                d.layers = None;
                d.make = None;
            }
        }
    }
    if create {
        if let Some((Some(a), req)) = &d.make {
            match road_glue::add_pavement(doc, *a, req) {
                Ok(mark) => out.push(Outcome::Added(mark.clone(), format!("Added {mark} from the designed layers."))),
                Err(e) => out.push(Outcome::Status(format!("Not added: {e}"))),
            }
        }
    }
    if keep {
        *slot = Some(d);
    }
}

fn mowt_form(ui: &mut Ui, f: &mut MowtForm) {
    ui.label(RichText::new("Traffic, one direction (the other is taken to carry the same)").strong());
    let mut remove = None;
    egui::Grid::new("mowt-classes").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label(RichText::new("Vehicle category").small());
        ui.label(RichText::new("Per day, first year").small());
        ui.label(RichText::new("ESAs per vehicle").small());
        ui.label("");
        ui.end_row();
        for (i, c) in f.classes.iter_mut().enumerate() {
            text(ui, &mut c.name, "e.g. buses", 120.0);
            number(ui, &mut c.daily_first_year, 1.0, "");
            number(ui, &mut c.esa_per_vehicle, 0.01, "");
            if ui.small_button("x").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        f.classes.remove(i);
    }
    if ui.button("Add a vehicle category").clicked() {
        f.classes.push(ClassRow { name: String::new(), daily_first_year: 0.0, esa_per_vehicle: 0.0 });
    }
    ui.label(RichText::new("The damage factor is entered as ESAs per vehicle from an axle load survey. Entering axle loads is not offered here.").small().weak());
    egui::Grid::new("mowt").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("Growth rate");
        number(ui, &mut f.growth_percent, 0.1, " % a year");
        ui.end_row();
        ui.label("Design period");
        whole(ui, &mut f.design_period_years, " years");
        ui.end_row();
        ui.label("Road");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut f.dual_carriageway, false, "Single carriageway");
            ui.selectable_value(&mut f.dual_carriageway, true, "Dual carriageway");
        });
        ui.end_row();
        if f.dual_carriageway {
            ui.label("Commercial vehicles per day, one direction");
            number(ui, &mut f.commercial_vehicles_per_day_one_direction, 1.0, "");
        } else {
            ui.label("Paved width");
            number(ui, &mut f.paved_width_m, 0.1, " m");
            ui.end_row();
            ui.label("Narrow road factor");
            optional(ui, &mut f.narrow_road_factor, 0.05, "");
        }
        ui.end_row();
        ui.label("Design subgrade CBR");
        number(ui, &mut f.subgrade_cbr_percent, 0.5, " %");
        ui.end_row();
        ui.label("Climate");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut f.dry, true, "Predominantly dry");
            ui.selectable_value(&mut f.dry, false, "Predominantly wet");
        });
        ui.end_row();
        if !f.dry {
            ui.label("Drainage");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut f.drainage_good, true, "Good");
                ui.selectable_value(&mut f.drainage_good, false, "Deficient");
            });
            ui.end_row();
            ui.label("Maintenance");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut f.maintenance_good, true, "Good");
                ui.selectable_value(&mut f.maintenance_good, false, "Deficient");
            });
            ui.end_row();
        }
        ui.label("Base and subbase");
        let types = index_options(mowt::PavementType::ALL.iter().map(|t| t.label().to_string()).collect());
        pick(ui, "mowt-type", &mut f.pavement, &types);
        ui.end_row();
    });
}

fn dmrb_form(ui: &mut Ui, f: &mut DmrbForm) {
    ui.label(RichText::new("Commercial vehicle flows, one direction, at opening").strong());
    let groups = index_options(dmrb::VehicleGroup::ALL.iter().map(|g| g.label().to_string()).collect());
    let mut remove = None;
    egui::Grid::new("dmrb-flows").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
        for (i, (group, aadf)) in f.flows.iter_mut().enumerate() {
            pick(ui, &format!("dmrb-group-{i}"), group, &groups);
            number(ui, aadf, 1.0, " cv/day");
            if ui.small_button("x").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        f.flows.remove(i);
    }
    if ui.button("Add a flow").clicked() {
        f.flows.push((0, 0.0));
    }
    egui::Grid::new("dmrb").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("Scheme");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut f.maintenance, false, "New (WN)");
            ui.selectable_value(&mut f.maintenance, true, "Maintenance (WM)");
        });
        ui.end_row();
        ui.label("Design period");
        whole(ui, &mut f.design_period_years, " years");
        ui.end_row();
        ui.label("Lanes in one direction");
        whole(ui, &mut f.lanes_one_direction, "");
        ui.end_row();
        ui.label("Past traffic");
        ui.checkbox(&mut f.past_traffic, "the figures are for past traffic");
        ui.end_row();
        ui.label("Foundation class (from CD 225)");
        ui.add(egui::DragValue::new(&mut f.foundation_class).range(1..=4));
        ui.end_row();
        ui.label("Flexible pavement");
        let kinds = index_options(road_glue::DMRB_PAVEMENTS.iter().map(|s| s.to_string()).collect());
        pick(ui, "dmrb-pavement", &mut f.pavement, &kinds);
        ui.end_row();
    });
    ui.label(RichText::new("This method gives design traffic and, over an HBGM base, the asphalt thickness. It gives no layer stack, so no pavement can be made from it.").small().weak());
}

fn aashto_form(ui: &mut Ui, f: &mut AashtoForm) {
    egui::Grid::new("aashto").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("Design lane 18-kip ESALs (W18)");
        number(ui, &mut f.w18, 1000.0, "");
        ui.end_row();
        ui.label("Reliability");
        number(ui, &mut f.reliability_percent, 0.5, " %");
        ui.end_row();
        ui.label("Overall standard deviation (S0)");
        number(ui, &mut f.overall_standard_deviation, 0.01, "");
        ui.end_row();
        ui.label("Serviceability loss (delta PSI)");
        number(ui, &mut f.delta_psi, 0.1, "");
        ui.end_row();
        ui.label("Effective roadbed modulus (MR)");
        number(ui, &mut f.roadbed_modulus_psi, 100.0, " psi");
        ui.end_row();
    });
    ui.add_space(4.0);
    ui.label(RichText::new("Structure to check, top first").strong());
    let materials = index_options(road_glue::materials().iter().map(|m| m.label().to_string()).collect());
    let mut remove = None;
    egui::Grid::new("aashto-layers").num_columns(7).spacing([8.0, 4.0]).show(ui, |ui| {
        for h in ["Layer", "Material", "mm", "a (per inch)", "m", "Modulus (psi)", ""] {
            ui.label(RichText::new(h).small());
        }
        ui.end_row();
        for (i, l) in f.layers.iter_mut().enumerate() {
            text(ui, &mut l.name, "layer name", 110.0);
            pick(ui, &format!("aashto-material-{i}"), &mut l.material, &materials);
            number(ui, &mut l.thickness_mm, 5.0, "");
            number(ui, &mut l.layer_coefficient, 0.005, "");
            number(ui, &mut l.drainage_coefficient, 0.05, "");
            optional(ui, &mut l.resilient_modulus_psi, 100.0, "");
            if ui.small_button("x").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        f.layers.remove(i);
    }
    if ui.button("Add a layer").clicked() {
        f.layers.push(SnRow { name: String::new(), material: 0, thickness_mm: 0.0, layer_coefficient: 0.0, drainage_coefficient: 1.0, resilient_modulus_psi: None });
    }
    ui.label(RichText::new("The layer and drainage coefficients are entered; drainage coefficient 1 applies to bound layers. With no layers the required structural number is all that is computed.").small().weak());
}

// ─── corridor ───────────────────────────────────────────────────────────────────────────

fn corridor_dialog(ctx: &egui::Context, doc: &mut Document, slot: &mut Option<(Option<ElementId>, CorridorRequest)>, out: &mut Vec<Outcome>) {
    let Some((mut alignment, mut req)) = slot.take() else { return };
    let choices = alignment_glue::alignments(doc);
    let surfaces = afrobim_corridor::surfaces(doc);
    let (mut keep, mut add) = (true, false);
    crate::win::new(ctx, "Corridor").collapsible(false).resizable(false).show(ctx, |ui| {
        egui::Grid::new("corridor").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            alignment_row(ui, "corridor-alignment", &choices, &mut alignment);
            ui.label("Ground surface");
            let current = surfaces.iter().find(|s| Some(s.0) == req.surface).map(|s| s.1.clone()).unwrap_or_default();
            egui::ComboBox::from_id_salt("corridor-surface").selected_text(current).show_ui(ui, |ui| {
                for (id, name) in &surfaces {
                    ui.selectable_value(&mut req.surface, Some(*id), name);
                }
            });
            ui.end_row();
            ui.label("Name");
            text(ui, &mut req.name, "e.g. Corridor", 140.0);
            ui.end_row();
            ui.label("Lane width, each side");
            number(ui, &mut req.lane_width_m, 0.05, " m");
            ui.end_row();
            ui.label("Lane cross-fall (falling away)");
            ui.horizontal(|ui| {
                number(ui, &mut req.lane_crossfall_percent, 0.1, " %");
                ui.checkbox(&mut req.follow_superelevation, "take the alignment's stored cross-fall where there is one");
            });
            ui.end_row();
            shoulder_row(ui, "Shoulder, each side", &mut req.shoulder);
            if req.shoulder.is_some() {
                ui.label("Formation under the shoulder");
                ui.checkbox(&mut req.shoulder_paved, "the shoulder is paved");
                ui.end_row();
            }
            ui.label("Formation depth below paved links");
            number(ui, &mut req.formation_depth_m, 0.01, " m");
            ui.end_row();
            ui.label("Cut slope (horizontal for 1 vertical)");
            number(ui, &mut req.cut_run_per_rise, 0.05, "");
            ui.end_row();
            ui.label("Fill slope (horizontal for 1 vertical)");
            number(ui, &mut req.fill_run_per_rise, 0.05, "");
            ui.end_row();
            ui.label("Section interval");
            number(ui, &mut req.interval_m, 1.0, " m");
            ui.end_row();
            ui.label("Stretch");
            ui.horizontal(|ui| {
                let mut limited = req.stretch.is_some();
                if ui.checkbox(&mut limited, "limit to a stretch of the alignment").changed() {
                    req.stretch = if limited { alignment.and_then(|a| alignment_glue::alignment_of(doc, a)).map(|a| (a.start_station_m, a.end_station_m())) } else { None };
                }
                if !limited {
                    ui.label(RichText::new("the whole alignment").weak());
                }
            });
            ui.end_row();
            if let Some((start, end)) = &mut req.stretch {
                station_row(ui, doc, alignment, "corridor-start", "Stretch start station", start);
                station_row(ui, doc, alignment, "corridor-end", "Stretch end station", end);
            }
        });
        ui.label(RichText::new(station_input::note(doc, alignment)).small().weak());
        not_a_design(ui, "The corridor holds no design values: every width, slope, depth and interval is yours. It needs an alignment with a vertical profile, and the same template is used both sides over the whole length. More regions are edited in Properties. Volumes are by average end area, with no bulking or shrinkage factor (both 1).");
        buttons(ui, "Build", alignment.is_some() && req.surface.is_some(), &mut add, &mut keep);
    });
    if add {
        match alignment.ok_or_else(|| "Choose an alignment.".to_string()).and_then(|a| road_glue::add_corridor(doc, a, &req)) {
            Ok((mark, summary)) => out.push(Outcome::Added(mark.clone(), format!("Built {mark}: {summary}. SECTIONS shows the cross-sections."))),
            Err(e) => {
                out.push(Outcome::Status(format!("Not built: {e}")));
                *slot = Some((alignment, req));
            }
        }
    } else if keep {
        *slot = Some((alignment, req));
    }
}

// ─── cross-sections ─────────────────────────────────────────────────────────────────────

fn colour(c: Option<Colour>, dark: bool) -> Color32 {
    match c {
        // Uncoloured hairlines are the finished surface: black on paper, light on the dark theme.
        None => if dark { Color32::from_rgb(0xE6, 0xE6, 0xE6) } else { Color32::BLACK },
        Some(c) => {
            let [r, g, b] = c.display_rgb();
            Color32::from_rgb(r, g, b)
        }
    }
}

fn sections_window(ctx: &egui::Context, slot: &mut Option<SectionsView>) {
    let Some(mut v) = slot.take() else { return };
    let mut open = true;
    let dark = ctx.style().visuals.dark_mode;
    crate::win::new(ctx, format!("Cross-sections of {}", v.mark)).open(&mut open).default_width(700.0).show(ctx, |ui| {
        let last = v.corridor.sections.len() - 1;
        ui.horizontal(|ui| {
            ui.label("Station");
            let current = v.corridor.sections[v.index.min(last)].label.clone();
            egui::ComboBox::from_id_salt("section-station").selected_text(current).show_ui(ui, |ui| {
                for (i, s) in v.corridor.sections.iter().enumerate() {
                    ui.selectable_value(&mut v.index, i, &s.label);
                }
            });
            if ui.add_enabled(v.index > 0, egui::Button::new("Previous")).clicked() {
                v.index -= 1;
            }
            if ui.add_enabled(v.index < last, egui::Button::new("Next")).clicked() {
                v.index += 1;
            }
            ui.label("Vertical exaggeration");
            ui.add(egui::DragValue::new(&mut v.exaggeration).speed(0.1).range(1.0..=20.0));
        });
        let section = &v.corridor.sections[v.index.min(last)];
        let drawn = corridor::section_drawing(section, v.exaggeration);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(400.0), 300.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, if dark { Color32::from_rgb(0x1B, 0x1F, 0x25) } else { Color32::from_rgb(0xF6, 0xF6, 0xF4) });
        let (w, h) = (drawn.drawing.extents.width().max(1.0), drawn.drawing.extents.height().max(1.0));
        let inner = rect.shrink(18.0);
        let k = (inner.width() as f64 / w).min(inner.height() as f64 / h);
        let map = |p: afrobim_draw::Pt2| egui::pos2(inner.left() + (p.x * k) as f32 + (inner.width() as f64 - w * k) as f32 / 2.0, inner.bottom() - (p.y * k) as f32 - (inner.height() as f64 - h * k) as f32 / 2.0);
        for p in &drawn.drawing.primitives {
            if let Primitive::Line { from, to, colour: c, .. } = p {
                painter.line_segment([map(*from), map(*to)], egui::Stroke::new(1.5_f32, colour(*c, dark)));
            }
        }
        for l in &drawn.labels {
            if matches!(l.kind, corridor::LabelKind::Design | corridor::LabelKind::Daylight) {
                painter.circle_filled(map(l.at), 2.5, colour(None, dark));
            }
        }
        ui.label(RichText::new("Ground green, earthworks line red, finished surface black, centreline grey. Looking up-station, left on the left.").small().weak());
        for l in &drawn.labels {
            match l.kind {
                corridor::LabelKind::Title | corridor::LabelKind::Areas => {
                    ui.label(RichText::new(&l.text).strong());
                }
                _ => {
                    ui.label(RichText::new(&l.text).small());
                }
            }
        }
        for n in &section.notes {
            ui.label(RichText::new(n).small().color(NOTE));
        }
    });
    if open {
        *slot = Some(v);
    }
}
