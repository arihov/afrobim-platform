// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! EARTHWORKS: the earthworks table of a corridor (average end area or prismoidal), the material
//! volumes by shape code, the mass haul diagram with a free haul distance, CSV and the send to the
//! bill. The calculation runs on a background job.

use afrobim_corridor::{baseline_earthworks, build_baseline, material_volumes, quantity_csv, MassHaul, MaterialStation, MaterialTable, QuantityOptions, QuantityTable, VolumeMethod, VolumeOptions};
use afrobim_kernel::ElementId;
use egui::{pos2, Color32, RichText, Sense, Stroke};

use crate::app::AfroBimApp;
use crate::civil::{self, CorridorData, CORRIDOR};
use crate::civil_dlg::{self as d, fmt};
use crate::ui_kit::{self, Col, Dialog, Footer, Level, NumRule};

pub const PROP_BILL: &str = "civil.bill";

pub struct QtyOut {
    pub corridor: ElementId,
    pub baseline: String,
    pub table: Result<QuantityTable, String>,
    pub materials: Result<MaterialTable, String>,
    pub haul: Option<Result<MassHaul, String>>,
}

#[derive(Default)]
pub struct QtyUi {
    pub form: Option<QtyForm>,
    pub out: Option<QtyOut>,
}

pub struct QtyForm {
    corridor: Option<ElementId>,
    baseline: usize,
    method: usize,
    cut_factor: String,
    fill_factor: String,
    free_haul: String,
    message: String,
    /// Press Calculate on the first frame (the demonstration's screenshots).
    auto: bool,
}

impl AfroBimApp {
    pub(crate) fn civil_quantities_cmd(&mut self) {
        let cors = civil::of_category(&self.doc, CORRIDOR);
        if cors.is_empty() {
            self.status = "There is no corridor yet: make one with CORRIDORMODEL first.".into();
            return;
        }
        let sel = self.doc.primary().filter(|id| cors.iter().any(|c| c.0 == *id)).or_else(|| cors.first().map(|c| c.0));
        self.civil_ui.qty.form = Some(QtyForm { corridor: sel, baseline: 0, method: 0, cut_factor: String::new(), fill_factor: String::new(), free_haul: if self.options.civil_demo { "100".into() } else { String::new() }, message: String::new(), auto: self.options.civil_demo });
    }

    pub(crate) fn civil_quantities_windows(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.qty.form.take() else { return };
        let cors = civil::of_category(&self.doc, CORRIDOR);
        let cdata = f.corridor.and_then(|id| civil::get::<CorridorData>(&self.doc, id, CORRIDOR));
        let baselines: Vec<String> = cdata.as_ref().map(|c| c.model.baselines.iter().map(|b| b.name.clone()).collect()).unwrap_or_default();
        let pos = NumRule::default().above(0.0);
        let cf = d::num_rule_peek(&f.cut_factor, &pos).or(if f.cut_factor.trim().is_empty() { Some(1.0) } else { None });
        let ff = d::num_rule_peek(&f.fill_factor, &pos).or(if f.fill_factor.trim().is_empty() { Some(1.0) } else { None });
        let haul = if f.free_haul.trim().is_empty() { Some(None) } else { d::num_rule_peek(&f.free_haul, &NumRule::default().min(0.0)).map(Some) };
        let busy = self.bg.quantities.is_some();
        let ok = f.corridor.is_some() && cf.is_some() && ff.is_some() && haul.is_some() && !busy;
        let mut csv = false;
        let mut bill = false;
        let out_ref = self.civil_ui.qty.out.as_ref();
        let out = Dialog::new("Earthworks and mass haul", crate::glyphs::by_name("EARTHWORKS")).width(860.0).resizable(true).max_body_height(620.0).help("Volumes between sections by the method you choose. Cut and fill factors are yours: leave them blank for none (1). The mass ordinate is the running sum of cut times its factor less fill times its factor. Free haul is the distance within which material moves without extra charge: it is your contract's figure, so enter it to see the overhaul; blank gives no haul analysis.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "eq", |fm| {
                    fm.row("Corridor", |ui| {
                        let before = f.corridor;
                        d::pick_from(ui, "eq-c", &cors, &mut f.corridor);
                        if before != f.corridor {
                            f.baseline = 0;
                        }
                    });
                    if baselines.len() > 1 {
                        fm.row("Baseline", |ui| {
                            ui_kit::combo_owned(ui, "eq-b", &mut f.baseline, &baselines);
                        });
                    }
                    fm.row("Method", |ui| {
                        ui_kit::segmented(ui, &mut f.method, &["Average end area", "Prismoidal"]);
                    });
                    fm.row_note("Cut factor", "Blank for none", |ui| {
                        d::num_opt_rule(ui, &mut f.cut_factor, "", &pos).ok();
                    });
                    fm.row_note("Fill factor", "Blank for none", |ui| {
                        d::num_opt_rule(ui, &mut f.fill_factor, "", &pos).ok();
                    });
                    fm.row_note("Free haul distance", "Blank for no haul analysis", |ui| {
                        d::num_opt(ui, &mut f.free_haul, "m").ok();
                    });
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
                if busy {
                    ui.label(RichText::new("Calculating...").weak());
                }
                if let Some(o) = out_ref {
                    ui.separator();
                    ui.label(RichText::new(format!("Baseline {}", o.baseline)).strong());
                    match &o.table {
                        Ok(t) => {
                            ui_kit::results_table(ui, "eq-tot", &["Total", "m3"], &[Col::Text, Col::Num], &[vec!["Cut".into(), fmt(t.total_cut_m3, 1)], vec!["Fill".into(), fmt(t.total_fill_m3, 1)], vec!["Net (cut less fill)".into(), fmt(t.total_cut_m3 - t.total_fill_m3, 1)]]);
                            if t.unmeasured_length_m > 0.0 {
                                ui_kit::callout(ui, Level::Warning, format!("{:.1} m of the corridor could not be measured (a section is missing a figure the method needs).", t.unmeasured_length_m));
                            }
                            mass_diagram(ui, t);
                            let rows: Vec<Vec<String>> = t.rows.iter().map(|r| vec![r.label.clone(), r.cut_area_m2.map(|v| fmt(v, 2)).unwrap_or_default(), r.fill_area_m2.map(|v| fmt(v, 2)).unwrap_or_default(), r.cut_volume_m3.map(|v| fmt(v, 1)).unwrap_or_default(), r.fill_volume_m3.map(|v| fmt(v, 1)).unwrap_or_default(), fmt(r.mass_ordinate_m3, 1)]).collect();
                            egui::ScrollArea::vertical().id_salt("eq-rows").max_height(130.0).show(ui, |ui| {
                                ui_kit::results_table(ui, "eq-rows", &["Station", "Cut area", "Fill area", "Cut m3", "Fill m3", "Mass ordinate"], &[Col::Text, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num], &rows);
                            });
                        }
                        Err(e) => {
                            ui_kit::callout(ui, Level::Error, e.clone());
                        }
                    }
                    if let Some(h) = &o.haul {
                        match h {
                            Ok(h) => ui_kit::results_table(ui, "eq-haul", &["Mass haul", "Value"], &[Col::Text, Col::Num], &[vec!["Free haul distance (m)".into(), fmt(h.free_haul_m, 1)], vec!["Balanced volume (m3)".into(), fmt(h.balanced_volume_m3, 1)], vec!["Free haul volume (m3)".into(), fmt(h.free_haul_volume_m3, 1)], vec!["Overhaul volume (m3)".into(), fmt(h.overhaul_volume_m3, 1)], vec!["Total haul moment (m3 m)".into(), fmt(h.total_haul_moment_m3m, 0)], vec!["Overhaul moment (m3 m)".into(), fmt(h.overhaul_moment_m3m, 0)], vec!["Residual (m3)".into(), fmt(h.residual_m3, 1)]]),
                            Err(e) => {
                                ui.label(RichText::new(format!("Mass haul: {e}")).color(crate::theme::warn()));
                            }
                        }
                    }
                    match &o.materials {
                        Ok(m) => {
                            let rows: Vec<Vec<String>> = m.totals.iter().map(|t| vec![t.shape_code.clone(), fmt(t.volume_m3, 2), t.classification.material.clone().unwrap_or_default(), t.classification.ifc_entity.clone().unwrap_or_default()]).collect();
                            if !rows.is_empty() {
                                ui_kit::results_table(ui, "eq-mat", &["Shape code", "Volume (m3)", "Material", "IFC entity"], &[Col::Text, Col::Num, Col::Text, Col::Text], &rows);
                            }
                            for n in &m.notes {
                                ui.label(RichText::new(n).small().weak());
                            }
                        }
                        Err(e) => {
                            ui.label(RichText::new(format!("Material volumes: {e}")).color(crate::theme::warn()));
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Export CSV...").clicked() {
                            csv = true;
                        }
                        if ui.button("Send to the bill").on_hover_text("Adds the earthworks and material volumes as quantity lines the bill (BILL) collects").clicked() {
                            bill = true;
                        }
                    });
                }
            },
            |ui| Footer::new("Calculate").icon(crate::glyphs::by_name("EARTHWORKS")).enabled(ok, "Complete the fields; wait for a running calculation").cancel(Some("Close")).show(ui),
        );
        if csv {
            if let Some(Ok(t)) = self.civil_ui.qty.out.as_ref().map(|o| &o.table) {
                let mut text = quantity_csv(t);
                if let Some(Ok(m)) = self.civil_ui.qty.out.as_ref().map(|o| &o.materials) {
                    text.push_str("\nshape_code,volume_m3,material,ifc_entity\n");
                    for t in &m.totals {
                        text.push_str(&format!("{},{:.3},{},{}\n", t.shape_code, t.volume_m3, t.classification.material.clone().unwrap_or_default(), t.classification.ifc_entity.clone().unwrap_or_default()));
                    }
                }
                if let Some(path) = rfd::FileDialog::new().set_file_name("earthworks.csv").save_file() {
                    self.status = match std::fs::write(&path, text) {
                        Ok(()) => format!("Wrote {}.", path.display()),
                        Err(e) => format!("The file could not be written: {e}"),
                    };
                }
            }
        }
        if bill {
            self.send_civil_to_bill();
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) || (f.auto && ok) {
            f.auto = false;
            if let (Some(cd), Some(cid)) = (cdata, f.corridor) {
                match cd.model.baselines.get(f.baseline).cloned() {
                    Some(baseline) => {
                        let alignment = crate::alignment_glue::alignments(&self.doc).into_iter().filter_map(|a| crate::alignment_glue::alignment_of(&self.doc, a.0)).find(|a| a.name == baseline.alignment);
                        let ground = self.tin_for(ElementId(cd.ground));
                        match (alignment, ground) {
                            (Some(al), Some(g)) => {
                                let opts = QuantityOptions { volumes: VolumeOptions { cut_factor: cf.unwrap_or(1.0), fill_factor: ff.unwrap_or(1.0), curvature_correction: false }, method: if f.method == 0 { VolumeMethod::AverageEndArea } else { VolumeMethod::Prismoidal } };
                                let free = haul.flatten();
                                let fp = crate::background::fingerprint(&self.doc);
                                self.bg.quantities = Some(crate::background::Job::spawn("Calculating earthworks", fp, move || run_quantities(cid, baseline, al, g, opts, free)));
                                self.civil_ui.qty.out = None;
                                f.message.clear();
                            }
                            _ => f.message = "The alignment or the ground surface of this corridor is missing.".into(),
                        }
                    }
                    None => f.message = "The corridor has no baseline.".into(),
                }
            }
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        self.civil_ui.qty.form = Some(f);
    }

    /// Writes the quantity lines into the corridor's property `civil.bill`, where `cost_glue::collect`
    /// finds them. The bill then lists them as quantities; mapping them to rates needs rate rules
    /// for the categories "Civil earthworks" and "Civil materials" (BILL, Rates).
    fn send_civil_to_bill(&mut self) {
        let Some(o) = &self.civil_ui.qty.out else { return };
        let mut lines: Vec<(String, String, f64, String)> = Vec::new();
        if let Ok(t) = &o.table {
            lines.push(("Civil earthworks".into(), format!("Cut, {}", o.baseline), t.total_cut_m3, "m3".into()));
            lines.push(("Civil earthworks".into(), format!("Fill, {}", o.baseline), t.total_fill_m3, "m3".into()));
        }
        if let Ok(m) = &o.materials {
            for t in &m.totals {
                lines.push(("Civil materials".into(), format!("{}, {}", t.shape_code, o.baseline), t.volume_m3, "m3".into()));
            }
        }
        let n = lines.len();
        if let Some(e) = self.doc.find_mut(o.corridor) {
            e.properties.insert(PROP_BILL.into(), serde_json::to_string(&lines).unwrap_or_default());
        }
        self.status = format!("Sent {n} quantity line(s) to the bill. Open BILL to see them; they need rate rules for the categories Civil earthworks and Civil materials to be priced.");
        self.schedule_commit_now();
    }
}

fn run_quantities(corridor: ElementId, baseline: afrobim_corridor::Baseline, al: afrobim_alignment::Alignment, ground: std::sync::Arc<afrobim_corridor::Tin>, opts: QuantityOptions, free_haul: Option<f64>) -> QtyOut {
    let name = baseline.name.clone();
    let built = match build_baseline(&baseline, &al, &ground) {
        Ok(b) => b,
        Err(e) => return QtyOut { corridor, baseline: name, table: Err(e.to_string()), materials: Err("not calculated".into()), haul: None },
    };
    let table = baseline_earthworks(&baseline, &al, &ground, &built, &opts).map_err(|e| e.to_string());
    let stations: Vec<MaterialStation> = built.sections.iter().map(|s| MaterialStation { station_m: s.section.station_m, label: &s.section.label, design: &s.design }).collect();
    let materials = Ok(material_volumes(&stations));
    let haul = free_haul.map(|h| match &table {
        Ok(t) => {
            let (s, o) = t.mass_curve();
            afrobim_corridor::mass_haul(&s, &o, h)
        }
        Err(e) => Err(e.clone()),
    });
    QtyOut { corridor, baseline: name, table, materials, haul }
}

/// The mass haul diagram: cumulative mass against station, drawn with its zero line.
fn mass_diagram(ui: &mut egui::Ui, t: &QuantityTable) {
    let (s, o) = t.mass_curve();
    if s.len() < 2 {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(780.0), 170.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, Color32::from_rgb(0x1B, 0x1F, 0x25));
    let (x0, x1) = (s[0], *s.last().unwrap());
    let (mut y0, mut y1) = (o.iter().cloned().fold(0.0, f64::min), o.iter().cloned().fold(0.0, f64::max));
    if (y1 - y0).abs() < 1e-9 {
        y1 = y0 + 1.0;
        y0 -= 1.0;
    }
    let m = 28.0;
    let map = |x: f64, y: f64| pos2(rect.left() + m + ((x - x0) / (x1 - x0).max(1e-9)) as f32 * (rect.width() - m - 8.0), rect.bottom() - 18.0 - ((y - y0) / (y1 - y0)) as f32 * (rect.height() - 30.0));
    p.line_segment([map(x0, 0.0), map(x1, 0.0)], Stroke::new(1.0_f32, Color32::from_gray(110)));
    p.add(egui::Shape::line(s.iter().zip(&o).map(|(a, b)| map(*a, *b)).collect(), Stroke::new(1.8_f32, Color32::from_rgb(0x4D, 0xA3, 0xFF))));
    let f = egui::FontId::proportional(10.0);
    p.text(rect.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, format!("Mass haul diagram: ordinate from {y0:.0} to {y1:.0} m3"), f.clone(), Color32::from_gray(180));
    p.text(pos2(rect.left() + m, rect.bottom() - 4.0), egui::Align2::LEFT_BOTTOM, format!("{x0:.0} m"), f.clone(), Color32::from_gray(150));
    p.text(pos2(rect.right() - 8.0, rect.bottom() - 4.0), egui::Align2::RIGHT_BOTTOM, format!("{x1:.0} m"), f, Color32::from_gray(150));
}
