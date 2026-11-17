//! The dialogs of the second marking set: one window, opened by a command of the Roads tab for a
//! group (transverse lines, arrows, hatching and chevrons, yellow lines, studs, worded markings,
//! Uganda lines), with the basis choice, the engine's refusal where the Uganda manual defines
//! nothing, the "use the UK value" choice with its stored reason, and a plan preview of what the
//! engine lays out.

use afrobim_kernel::{Document, ElementId};
use afrobim_markings::{self as markings, Basis, ExtSpec, Family, Kind, Setting, StudColour};
use egui::{Color32, RichText, Ui};

use crate::app::AfroBimApp;
use crate::ext_markings_glue::{self as glue, ExtRequest, Group, Placement, PLACEHOLDER_WORDS};
use crate::{alignment_glue, station_input};

#[allow(non_snake_case)]

fn WARN() -> egui::Color32 {

    crate::theme::warn()

}

/// The state of the window.
pub struct ExtDialog {
    pub group: Group,
    pub alignment: Option<ElementId>,
    pub req: ExtRequest,
    /// The family whose UK types are offered under the Uganda basis, by the engineer's choice.
    fallback_family: Option<Family>,
    /// The speed limit as typed (the basis's own unit), and whether a UK limit is typed in km/h.
    speed_text: String,
    kmh: bool,
    answer: Option<bool>,
    /// Stud row chosen (index into the stud catalogue) and the urban or rural spacing.
    stud_rural: bool,
}

impl ExtDialog {
    pub fn new(doc: &Document, group: Group, uganda: bool) -> Option<ExtDialog> {
        let first = alignment_glue::alignments(doc).first().map(|a| a.0)?;
        let (start, end) = alignment_glue::alignment_of(doc, first).map(|a| (a.start_station_m, a.end_station_m())).unwrap_or((0.0, 0.0));
        // The first type of the group under the UK basis, or of the Uganda basis for the Uganda lines.
        // The basis starts from the document's sign standard; the engineer changes it in the dialog.
        let basis = if group == Group::Longitudinal || uganda { Basis::Uganda } else { Basis::Uk };
        // The first marking of the group that can be laid, then the first of any kind.
        let layable = |s: &&&'static ExtSpec| !matches!(s.geometry, markings::GeometryStatus::CatalogueOnly(_));
        let type_id = group
            .families()
            .iter()
            .find_map(|f| glue::types_of(basis, *f).ok().and_then(|v| v.iter().find(layable).or(v.first()).map(|s| s.id.to_string())))
            .unwrap_or_else(|| markings::STUD_ID.to_string());
        let type_id = if group == Group::Studs { markings::STUD_ID.to_string() } else { type_id };
        Some(ExtDialog { group, alignment: Some(first), req: ExtRequest::new(basis, &type_id, start, end), fallback_family: None, speed_text: String::new(), kmh: false, answer: None, stud_rural: false })
    }
}

fn opt_mm(ui: &mut Ui, value: &mut Option<u32>, speed: f64, suffix: &str) {
    ui.horizontal(|ui| {
        let mut on = value.is_some();
        if ui.checkbox(&mut on, "").changed() {
            *value = on.then_some(0);
        }
        match value {
            Some(v) => {
                ui.add(egui::DragValue::new(v).speed(speed).suffix(suffix));
            }
            None => {
                ui.label(RichText::new("not entered").weak());
            }
        }
    });
}

fn choice_mm(ui: &mut Ui, salt: &str, value: &mut Option<u32>, options: &[u32], what: &str) {
    ui.horizontal_wrapped(|ui| {
        for o in options {
            ui.selectable_value(value, Some(*o), format!("{o} {what}"));
        }
        if options.is_empty() {
            ui.label(RichText::new("the table gives none").weak());
        }
    });
    let _ = salt;
}

/// The type's pieces of the catalogue entry a dialog needs: which dimensions it asks for.
fn dimension_rows(ui: &mut Ui, spec: &ExtSpec, req: &mut ExtRequest) {
    match spec.kind {
        Kind::Lines(l) => {
            ui.label("Width (mm)");
            choice_mm(ui, "w", &mut req.width_mm, l.widths_mm, "");
            ui.end_row();
            if !l.separations_mm.is_empty() {
                ui.label("Gap between the lines (mm)");
                choice_mm(ui, "s", &mut req.separation_mm, l.separations_mm, "");
                ui.end_row();
            }
            if l.by_setting {
                ui.label("Setting");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut req.setting, Some(Setting::Urban), "urban");
                    ui.selectable_value(&mut req.setting, Some(Setting::Rural), "rural");
                });
                ui.end_row();
            }
        }
        Kind::Transverse(t) => {
            ui.label("Width (mm)");
            choice_mm(ui, "w", &mut req.width_mm, t.widths_mm, "");
            ui.end_row();
        }
        Kind::Arrow(a) => {
            ui.label("Length (mm)");
            let lengths: Vec<u32> = a.sizes.iter().map(|s| s.length_mm).collect();
            choice_mm(ui, "l", &mut req.size_mm, &lengths, "");
            ui.end_row();
        }
        Kind::Word(w) => {
            ui.label("Letter height (mm)");
            let heights: Vec<u32> = w.heights.iter().map(|h| h.1).collect();
            choice_mm(ui, "h", &mut req.size_mm, &heights, "");
            ui.end_row();
        }
        Kind::Hatch(h) => {
            ui.label("Boundary line width (mm)");
            choice_mm(ui, "w", &mut req.width_mm, h.boundary_widths_mm, "");
            ui.end_row();
            ui.label("Clear width between the boundary lines (mm)");
            opt_mm(ui, &mut req.strip_width_mm, 50.0, "");
            ui.end_row();
            if h.strip_min_mm.is_some() || h.strip_max_mm.is_some() {
                ui.label("");
                ui.label(RichText::new(format!("The figure gives {} to {} mm; the width is yours.", h.strip_min_mm.map_or("-".into(), |v| v.to_string()), h.strip_max_mm.map_or("-".into(), |v| v.to_string()))).small().weak());
                ui.end_row();
            }
            ui.label("Lean");
            ui.checkbox(&mut req.mirror, "mirror the diagonals (point chevrons up-station)");
            ui.end_row();
        }
        Kind::ZigZag(z) => {
            ui.label("Lateral extent (mm)");
            choice_mm(ui, "a", &mut req.size_mm, z.amplitudes_mm, "");
            ui.end_row();
            ui.label("Mirror");
            ui.checkbox(&mut req.mirror, "mirror the zig-zag");
            ui.end_row();
        }
        Kind::Crossing(c) => {
            ui.label("Stripe depth or line width (mm)");
            opt_mm(ui, &mut req.width_mm, 50.0, "");
            ui.end_row();
            ui.label("");
            crate::ui_kit::note(ui, format!("The sign page gives stripes {} to {} mm, gaps {} to {} mm, width {} to {} mm.", c.mark_min_mm, c.mark_max_mm, c.gap_min_mm, c.gap_max_mm, c.min_width_mm, c.max_width_mm));
            ui.end_row();
            if c.stripes {
                ui.label("Gap between stripes (mm)");
                opt_mm(ui, &mut req.gap_mm, 50.0, "");
                ui.end_row();
                ui.label("Number of stripes");
                ui.horizontal(|ui| {
                    let mut on = req.count.is_some();
                    if ui.checkbox(&mut on, "").changed() {
                        req.count = on.then_some(1);
                    }
                    if let Some(n) = &mut req.count {
                        ui.add(egui::DragValue::new(n).range(1..=60));
                    }
                });
                ui.end_row();
            } else {
                ui.label("Length of the crossing along the road (mm)");
                opt_mm(ui, &mut req.length_mm, 100.0, "");
                ui.end_row();
            }
        }
        Kind::Triangle(_) => {
            ui.label("Mirror");
            ui.checkbox(&mut req.mirror, "mirror the triangle");
            ui.end_row();
        }
        Kind::None => {}
    }
}

/// True where the chosen entry depends on the speed limit (the engine says so when it cannot go on).
fn wants_speed(spec: Option<&ExtSpec>) -> bool {
    matches!(spec.map(|s| s.kind), Some(Kind::Hatch(_) | Kind::Arrow(_) | Kind::Word(_) | Kind::Transverse(_)))
}

impl AfroBimApp {
    pub(crate) fn open_ext_marking(&mut self, group: Group) {
        match ExtDialog::new(&self.doc, group, self.standard == "UG-TSM-2004") {
            Some(d) => self.ext_marking_dialog = Some(d),
            None => self.status = "There is no alignment yet. Import one with File → Import LandXML.".into(),
        }
    }

    pub(crate) fn ext_marking_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext_marking_dialog.take() else { return };
        let choices = alignment_glue::alignments(&self.doc);
        let (mut keep, mut lay) = (true, false);
                let footer_ok = d.alignment.is_some();
        let dlg = crate::ui_kit::Dialog::titled(format!("Road markings: {}", d.group.title())).width(560.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(620.0).auto_shrink([false, true]).show(ui, |ui| {
                crate::ui_kit::grid(ui, "extmark", |ui| {
                    ui.label("Alignment");
                    let current = choices.iter().find(|c| Some(c.0) == d.alignment).map(|c| c.1.clone()).unwrap_or_default();
                    egui::ComboBox::from_id_salt("extmark-alignment").selected_text(current).show_ui(ui, |ui| {
                        for (id, name) in &choices {
                            ui.selectable_value(&mut d.alignment, Some(*id), name);
                        }
                    });
                    ui.end_row();
                    ui.label("Basis");
                    ui.horizontal(|ui| {
                        for b in Basis::ALL {
                            if ui.selectable_value(&mut d.req.basis, b, b.label()).clicked() {
                                d.req.uk_fallback = None;
                                d.fallback_family = None;
                                d.speed_text.clear();
                                d.req.speed = None;
                            }
                        }
                    });
                    ui.end_row();
                    let studs = d.group == Group::Studs;
                    // The types the basis defines, and the families it does not.
                    let mut entries: Vec<(&'static ExtSpec, bool)> = Vec::new();
                    let mut missing: Vec<(Family, String)> = Vec::new();
                    if !studs {
                        for fam in d.group.families() {
                            match glue::types_of(d.req.basis, *fam) {
                                Ok(v) => entries.extend(v.into_iter().map(|s| (s, false))),
                                Err(msg) => {
                                    missing.push((*fam, msg));
                                    if d.req.basis == Basis::Uganda && d.fallback_family == Some(*fam) {
                                        if let Ok(uk) = glue::types_of(Basis::Uk, *fam) {
                                            entries.extend(uk.into_iter().map(|s| (s, true)));
                                        }
                                    }
                                }
                            }
                        }
                        let chosen = entries.iter().find(|(s, _)| s.id == d.req.type_id).copied();
                        ui.label("Marking");
                        let shown = chosen.map(|(s, uk)| format!("{} ({}{})", s.name, s.diagram, if uk { ", UK value" } else { "" })).unwrap_or_else(|| "choose a marking".into());
                        egui::ComboBox::from_id_salt("extmark-type").width(380.0).selected_text(shown).show_ui(ui, |ui| {
                            for (s, uk) in &entries {
                                let text = format!("{} ({}{}){}", s.name, s.diagram, if *uk { ", UK value" } else { "" }, if glue::is_placeholder(s) { " [placeholder block]" } else { "" });
                                if ui.selectable_label(d.req.type_id == s.id, text).clicked() {
                                    d.req.type_id = s.id.to_string();
                                    d.req.width_mm = None;
                                    d.req.size_mm = None;
                                    d.req.separation_mm = None;
                                    d.req.uk_fallback = if *uk { Some(d.req.uk_fallback.clone().unwrap_or_default()) } else { None };
                                }
                            }
                        });
                        ui.end_row();
                        if chosen.is_none() && !entries.is_empty() && d.req.type_id != markings::STUD_ID {
                            // The type id no longer belongs to this group or basis: take the first.
                            d.req.type_id = entries[0].0.id.to_string();
                            d.req.uk_fallback = if entries[0].1 { Some(String::new()) } else { None };
                        }
                    }
                    for (fam, msg) in &missing {
                        ui.label("");
                        ui.vertical(|ui| {
                            ui.label(RichText::new(msg).color(WARN()));
                            if d.req.basis == Basis::Uganda && glue::types_of(Basis::Uk, *fam).is_ok() {
                                let on = d.fallback_family == Some(*fam);
                                if ui.selectable_label(on, format!("Use the UK value for {}", glue::family_label(*fam).to_lowercase())).clicked() {
                                    d.fallback_family = if on { None } else { Some(*fam) };
                                    d.req.uk_fallback = None;
                                }
                            }
                        });
                        ui.end_row();
                    }
                    if d.req.uk_fallback.is_some() {
                        ui.label("Reason for the UK value");
                        let reason = d.req.uk_fallback.get_or_insert_with(String::new);
                        ui.add(egui::TextEdit::multiline(reason).desired_rows(2).desired_width(380.0).hint_text("required; stored with the marking"));
                        ui.end_row();
                    }
                    let spec = entries.iter().find(|(s, _)| s.id == d.req.type_id).map(|(s, _)| *s);
                    if let Some(s) = spec {
                        ui.label("");
                        ui.vertical(|ui| {
                            ui.label(RichText::new(s.citation).small());
                            ui.label(RichText::new(s.applies).small().weak());
                            if glue::is_placeholder(s) {
                                ui.label(RichText::new(PLACEHOLDER_WORDS).strong().color(WARN()));
                            }
                            if let markings::GeometryStatus::CatalogueOnly(why) = s.geometry {
                                ui.label(RichText::new(format!("Catalogue only, it cannot be laid: {why}")).color(WARN()));
                            }
                        });
                        ui.end_row();
                    }
                    // Placement.
                    let placement = if studs { Placement::Linear } else { spec.map_or(Placement::Linear, glue::placement_of) };
                    let a = d.alignment;
                    match placement {
                        Placement::Linear => {
                            station_input::station_row(ui, &self.doc, a, "extmark-start", "Start station", &mut d.req.station_m);
                            station_input::station_row(ui, &self.doc, a, "extmark-end", "End station", &mut d.req.end_station_m);
                        }
                        Placement::Across | Placement::Point => {
                            station_input::station_row(ui, &self.doc, a, "extmark-station", "Station", &mut d.req.station_m);
                        }
                    }
                    let (from_label, offset_note) = match placement {
                        Placement::Across => ("From offset (m)", "positive right looking up-station"),
                        Placement::Point => ("Offset of the centre (m)", "positive right looking up-station"),
                        Placement::Linear => ("Offset (m)", "positive right looking up-station; for a kerb or edge item, the edge"),
                    };
                    ui.label(from_label);
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut d.req.offset_m).speed(0.05));
                        ui.label(RichText::new(offset_note).small().weak());
                    });
                    ui.end_row();
                    if placement == Placement::Across {
                        ui.label("To offset (m)");
                        ui.add(egui::DragValue::new(&mut d.req.to_offset_m).speed(0.05));
                        ui.end_row();
                    }
                    if placement != Placement::Linear {
                        ui.label("Rotation (degrees from up-station, anticlockwise)");
                        ui.add(egui::DragValue::new(&mut d.req.rotation_deg).speed(1.0));
                        ui.end_row();
                    }
                    if let Some(s) = spec {
                        dimension_rows(ui, s, &mut d.req);
                    }
                    if studs {
                        stud_rows(ui, &mut d);
                    }
                    // Speed limit.
                    if wants_speed(spec) || studs {
                        ui.label(format!("Speed limit ({})", d.req.basis.speed_unit()));
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut d.speed_text).hint_text("enter the limit").desired_width(90.0));
                                if d.req.basis == Basis::Uk {
                                    ui.selectable_value(&mut d.kmh, false, "mph");
                                    ui.selectable_value(&mut d.kmh, true, "km/h");
                                }
                            });
                            let typed = d.speed_text.trim().parse::<f64>().ok();
                            d.req.speed = None;
                            d.req.speed_note.clear();
                            if let Some(v) = typed {
                                if d.req.basis == Basis::Uk && d.kmh {
                                    // A km/h limit meets the manual's mph tables through the project's rule.
                                    let r = crate::projset::resolve_answered(&self.doc, v, false, &crate::projset::marking_thresholds(), d.answer);
                                    ui.label(RichText::new(&r.band_note).small());
                                    if let Some(ask) = r.ask {
                                        ui.label(RichText::new(format!("The project rule asks each time: treat {:.2} mph as {} mph?", ask.exact_mph, ask.band_mph)).small().color(WARN()));
                                        ui.horizontal(|ui| {
                                            ui.selectable_value(&mut d.answer, Some(true), "Yes, take the band");
                                            ui.selectable_value(&mut d.answer, Some(false), "No, keep the exact value");
                                        });
                                    } else {
                                        d.req.speed = Some(r.mph_used.round().max(1.0) as u32);
                                        d.req.speed_note = format!("{v} km/h: {}", r.band_note);
                                    }
                                } else {
                                    d.req.speed = Some(v.round().max(1.0) as u32);
                                    d.req.speed_note = format!("{} {} as entered", v, d.req.basis.speed_unit());
                                }
                            }
                        });
                        ui.end_row();
                    }
                    ui.label("Material");
                    ui.add(egui::TextEdit::singleline(&mut d.req.material).hint_text("not stated").desired_width(190.0));
                    ui.end_row();
                });
                ui.add_space(4.0);
                preview(ui, &self.doc, &d);
                ui.label(RichText::new(station_input::note(&self.doc, d.alignment)).small().weak());
                ui.label(
                    RichText::new("Dimensions are the manual's, from the catalogue of afrobim-markings; where a manual leaves a figure to the engineer the field starts empty. The marking is shown in the 3D site view and measured under Export quantities and in the 5D estimate. Plan geometry only: no profile or cross-fall.")
                        .small()
                        .weak(),
                );
            });
        }, |ui| crate::ui_kit::Footer::new("Lay the marking").enabled(footer_ok, "Complete the inputs").cancel(Some("Close")).show(ui));
        if let Some((_, f)) = &dlg.inner {
            lay = f.primary;
            if f.cancel {
                keep = false;
            }
        }
        if dlg.closed {
            keep = false;
        }
        if lay {
            if let Some(a) = d.alignment {
                match glue::add(&mut self.doc, a, &d.req) {
                    Ok(mark) => {
                        self.sync_generated();
                        self.status = format!("Laid {mark}.");
                        let notes = self.doc.elements().iter().find(|e| e.name == mark).and_then(|e| self.cache.get(&e.id)).and_then(|c| c.generated.as_ref().ok()).map(|g| g.notes.clone()).unwrap_or_default();
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
        }
        if keep {
            self.ext_marking_dialog = Some(d);
        }
    }
}

/// Stud rows: the engine's tables, one row chosen by the engineer, the spacing and colour taken from it
/// on an explicit press and shown with the table that gives them.
fn stud_rows(ui: &mut Ui, d: &mut ExtDialog) {
    let basis = d.req.basis;
    ui.label("Stud row");
    ui.vertical(|ui| {
        let rows: Vec<&markings::StudRow> = markings::stud_catalogue().iter().filter(|r| r.basis == basis).collect();
        let shown = if d.req.stud_row.is_empty() { "choose the row of the table".to_string() } else { d.req.stud_row.clone() };
        egui::ComboBox::from_id_salt("extmark-studrow").width(420.0).selected_text(shown).show_ui(ui, |ui| {
            for r in &rows {
                ui.selectable_value(&mut d.req.stud_row, r.id.to_string(), format!("{} line {}: {}", r.id, r.line, r.description));
            }
        });
        if let Some(r) = rows.iter().find(|r| r.id == d.req.stud_row) {
            ui.label(RichText::new(r.citation).small());
            if r.urban_m.is_some() && r.urban_m != r.rural_m {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut d.stud_rural, false, format!("urban {} m", r.urban_m.unwrap_or(0.0)));
                    ui.selectable_value(&mut d.stud_rural, true, format!("rural {} m", r.rural_m.unwrap_or(0.0)));
                });
            }
            let spacing = if d.stud_rural { r.rural_m } else { r.urban_m };
            match spacing {
                Some(m) => {
                    if ui.button(format!("Use {} m spacing, {} studs, from this row", m, r.colour.label())).clicked() {
                        d.req.stud_spacing_mm = Some((m * 1000.0).round() as u32);
                        d.req.stud_colour = Some(r.colour);
                    }
                }
                None => {
                    ui.label(RichText::new(format!("The table gives no spacing here: \"{}\".", r.text)).color(WARN()));
                }
            }
        }
        match (d.req.stud_spacing_mm, d.req.stud_colour) {
            (Some(s), Some(c)) => {
                ui.label(RichText::new(format!("Spacing {} m, colour {}.", s as f64 / 1000.0, c.label())).small());
            }
            _ => {
                crate::ui_kit::note(ui, "No spacing chosen yet.");
            }
        }
    });
    ui.end_row();
    ui.label("Line the studs go with");
    crate::ui_kit::note(ui, "The offset above is the line's offset; the studs run along it between the stations.");
    ui.end_row();
    let _ = StudColour::White;
}

/// A plan preview of what the engine lays out for the request, scaled to fit. Shown only when the
/// request is complete; otherwise the engine's reason is shown.
fn preview(ui: &mut Ui, doc: &Document, d: &ExtDialog) {
    let outcome = d.req.definition().and_then(|mut def| {
        let a = d.alignment.and_then(|id| alignment_glue::alignment_of(doc, id)).ok_or("Choose an alignment.")?;
        def.alignment = a.name.clone();
        def.geometry = Some(a);
        let (laid, notes) = def.lay_out();
        match laid {
            Some((resolved, laid)) => Ok((resolved, laid)),
            None => Err(notes.iter().filter(|n| n.severity == afrobim_kernel::Severity::Error).map(|n| n.message.clone()).collect::<Vec<_>>().join(" ")),
        }
    });
    crate::ui_kit::heading(ui, "Preview in plan");
    let (resolved, laid) = match outcome {
        Ok(v) => v,
        Err(why) => {
            ui.label(RichText::new(if why.is_empty() { "Not laid out yet.".to_string() } else { why }).small().color(WARN()));
            return;
        }
    };
    let size = egui::vec2(ui.available_width().clamp(260.0, 520.0), 120.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_gray(52));
    // Every piece as polygons (a strip as quads between its edge points).
    let mut polys: Vec<Vec<(f64, f64)>> = Vec::new();
    for p in &laid.pieces {
        match &p.shape {
            markings::Shape::Poly(points) => polys.push(points.clone()),
            markings::Shape::Strip { left, right, .. } => {
                for i in 1..left.len().min(right.len()) {
                    polys.push(vec![left[i - 1], left[i], right[i], right[i - 1]]);
                }
            }
        }
    }
    let (mut lo, mut hi) = ((f64::INFINITY, f64::INFINITY), (f64::NEG_INFINITY, f64::NEG_INFINITY));
    for (x, y) in polys.iter().flatten() {
        lo = (lo.0.min(*x), lo.1.min(*y));
        hi = (hi.0.max(*x), hi.1.max(*y));
    }
    if polys.is_empty() || !lo.0.is_finite() {
        return;
    }
    let (w, h) = ((hi.0 - lo.0).max(0.01), (hi.1 - lo.1).max(0.01));
    let scale = ((rect.width() as f64 - 16.0) / w).min((rect.height() as f64 - 16.0) / h);
    let origin = rect.center() - egui::vec2(((lo.0 + hi.0) / 2.0 * scale) as f32, -((lo.1 + hi.1) / 2.0 * scale) as f32);
    let colour = match resolved.colour.label() {
        c if c.eq_ignore_ascii_case("yellow") => Color32::from_rgb(0xF2, 0xC9, 0x1B),
        c if c.eq_ignore_ascii_case("red") => Color32::from_rgb(0xE0, 0x4B, 0x3C),
        _ => Color32::from_gray(235),
    };
    for poly in &polys {
        let points: Vec<egui::Pos2> = poly.iter().map(|(x, y)| egui::pos2(origin.x + (*x * scale) as f32, origin.y - (*y * scale) as f32)).collect();
        painter.add(egui::Shape::convex_polygon(points, colour, egui::Stroke::new(0.5_f32, colour)));
    }
    ui.label(RichText::new(format!("{} shown {:.1} m by {:.1} m, scaled to fit.{}", resolved.name, w, h, if laid.placeholder { format!(" {PLACEHOLDER_WORDS}.") } else { String::new() })).small().weak());
}
