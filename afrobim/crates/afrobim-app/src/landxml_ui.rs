//! The LandXML import dialog. The file is read first and the dialog shows what is in it: the
//! alignments (station range, length, whether the file gave a profile, station equations and
//! superelevation) and the TIN surfaces (points, triangles, extent), with the readers' own notes.
//! The engineer ticks what to bring in; nothing is added until the button is pressed. The reading is
//! `afrobim_alignment::read_landxml` and `afrobim_corridor::read_landxml_surfaces`.

use std::path::PathBuf;

use afrobim_alignment::{read_landxml, LandXmlImport};
use afrobim_corridor::{read_landxml_surfaces, SurfaceImport};
use afrobim_kernel::SelectMode;
use egui::RichText;

use crate::app::AfroBimApp;
use crate::ui_kit::{self, Col, Dialog, Footer, Level};

pub struct LandXmlForm {
    path: PathBuf,
    alignments: Result<LandXmlImport, String>,
    surfaces: Result<SurfaceImport, String>,
    take_alignment: Vec<bool>,
    take_surface: Vec<bool>,
    tab: usize,
}

impl LandXmlForm {
    /// Reads the file both ways. `surfaces_first` opens on the surfaces tab.
    pub fn new(path: PathBuf, surfaces_first: bool) -> LandXmlForm {
        let alignments = read_landxml(&path).map_err(|e| e.to_string());
        let surfaces = read_landxml_surfaces(&path).map_err(|e| e.to_string());
        let take_alignment = alignments.as_ref().map(|a| vec![true; a.alignments.len()]).unwrap_or_default();
        let take_surface = surfaces.as_ref().map(|s| vec![true; s.surfaces.len()]).unwrap_or_default();
        let tab = match (&alignments, &surfaces) {
            (Ok(a), Ok(s)) if surfaces_first && !s.surfaces.is_empty() => 1,
            (Ok(a), Ok(s)) if a.alignments.is_empty() && !s.surfaces.is_empty() => 1,
            _ => 0,
        };
        LandXmlForm { path, alignments, surfaces, take_alignment, take_surface, tab }
    }

    fn picked(&self) -> (usize, usize) {
        (self.take_alignment.iter().filter(|b| **b).count(), self.take_surface.iter().filter(|b| **b).count())
    }
}

#[derive(Default)]
pub struct LandXmlUi {
    form: Option<LandXmlForm>,
}

impl AfroBimApp {
    pub(crate) fn landxml_open(&mut self, path: PathBuf, surfaces_first: bool) {
        self.landxml.form = Some(LandXmlForm::new(path, surfaces_first));
    }

    pub(crate) fn landxml_window(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.landxml.form.take() else { return };
        let units = crate::anno_ui::units_of(&self.doc);
        let file = form.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let (na, ns) = form.picked();
        let nothing_in_file = form.alignments.as_ref().map(|a| a.alignments.is_empty()).unwrap_or(true) && form.surfaces.as_ref().map(|s| s.surfaces.is_empty()).unwrap_or(true);
        let reason = if nothing_in_file { "The file holds no alignment or surface" } else { "Tick something to import" };
        let alignment_count = form.alignments.as_ref().map(|a| a.alignments.len()).unwrap_or(0);
        let surface_count = form.surfaces.as_ref().map(|s| s.surfaces.len()).unwrap_or(0);
        let labels = [format!("Alignments ({alignment_count})"), format!("Surfaces ({surface_count})")];
        let tab_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let button = format!("Import {}", match (na, ns) {
            (0, 0) => "nothing".to_string(),
            (a, 0) => format!("{a} alignment(s)"),
            (0, s) => format!("{s} surface(s)"),
            (a, s) => format!("{a} alignment(s) and {s} surface(s)"),
        });
        let out = Dialog::new("Import LandXML", crate::glyphs::by_name("LANDXML"))
            .subtitle(file)
            .width(640.0)
            .help("LandXML 1.x files are read with lengths converted to metres. Alignments come in with their profile, station equations and superelevation where the file has them; surfaces come in as ground surfaces (TIN). Untick what you do not want. Names already taken get a number.")
            .show(
                ctx,
                |ui| {
                    ui_kit::tabs(ui, &mut form.tab, &tab_refs);
                    if form.tab == 0 {
                        alignments_tab(ui, &mut form, &units);
                    } else {
                        surfaces_tab(ui, &mut form);
                    }
                },
                |ui| Footer::new(&button).enabled(na + ns > 0, reason).icon(crate::glyphs::by_name("LANDXML")).show(ui),
            );
        if out.closed || out.inner.is_some_and(|(_, f)| f.cancel) {
            return;
        }
        if out.inner.is_some_and(|(_, f)| f.primary) {
            self.landxml_apply(ctx, &form);
            return;
        }
        self.landxml.form = Some(form);
    }

    fn landxml_apply(&mut self, ctx: &egui::Context, form: &LandXmlForm) {
        let mut added_a = 0;
        let mut added_s = 0;
        let mut notes: Vec<String> = Vec::new();
        if let Ok(a) = &form.alignments {
            for (alignment, take) in a.alignments.iter().zip(&form.take_alignment) {
                if *take && crate::alignment_glue::add(&mut self.doc, alignment).is_some() {
                    added_a += 1;
                }
            }
            if form.take_alignment.iter().any(|t| *t) {
                notes.extend(a.notes.iter().map(|n| format!("LandXML: {n}")));
            }
        }
        if added_a > 0 {
            self.design.invalidate();
            crate::markings_glue::sync(&mut self.doc);
            for problem in crate::modules_glue::sync_all(&mut self.doc) {
                self.log(format!("Not brought up to date: {problem}"));
            }
        }
        let mut last_surface = None;
        if let Ok(s) = &form.surfaces {
            let source = form.path.file_name().map(|n| n.to_string_lossy().to_string());
            for (surface, take) in s.surfaces.iter().zip(&form.take_surface) {
                if *take {
                    match afrobim_corridor::add_surface(&mut self.doc, &surface.name, &surface.tin, source.as_deref()) {
                        Ok(id) => {
                            added_s += 1;
                            last_surface = Some(id);
                        }
                        Err(e) => notes.push(format!("LandXML: surface {} not added: {e}", surface.name)),
                    }
                }
            }
            if form.take_surface.iter().any(|t| *t) {
                notes.extend(s.notes.iter().map(|n| format!("LandXML: {n}")));
            }
        }
        if added_s > 0 {
            self.sync_generated();
            self.fitted_3d = false;
            if let Some(id) = last_surface {
                self.doc.select(&[id], SelectMode::Replace);
            }
        }
        self.status = format!("Imported {added_a} alignment(s) and {added_s} surface(s) from {}.", form.path.display());
        for n in notes.iter().take(6) {
            self.log(n.clone());
        }
        if !notes.is_empty() {
            self.status.push_str(&format!(" {} note(s) in the command history.", notes.len()));
        }
        self.schedule_commit(ctx);
    }
}

fn alignments_tab(ui: &mut egui::Ui, form: &mut LandXmlForm, units: &afrobim_annotate::DisplayUnits) {
    match &form.alignments {
        Err(e) => {
            ui_kit::callout(ui, Level::Error, format!("The file cannot be read as LandXML: {e}"));
        }
        Ok(import) => {
            if import.alignments.is_empty() {
                ui_kit::callout(ui, Level::Info, "The file has no alignments.");
            } else {
                ui_kit::section(ui, "Alignments in the file", |ui| {
                    ui.horizontal(|ui| {
                        if ui.small_button("Tick all").clicked() {
                            form.take_alignment.iter_mut().for_each(|t| *t = true);
                        }
                        if ui.small_button("Untick all").clicked() {
                            form.take_alignment.iter_mut().for_each(|t| *t = false);
                        }
                    });
                    egui::Grid::new("lx-alignments").num_columns(7).spacing([14.0, 6.0]).striped(true).show(ui, |ui| {
                        for h in ["", "Name", "From", "To", "Length", "Elements", "Also in the file"] {
                            ui.label(RichText::new(h).small().strong());
                        }
                        ui.end_row();
                        for (i, a) in import.alignments.iter().enumerate() {
                            ui.checkbox(&mut form.take_alignment[i], "");
                            ui.label(&a.name);
                            ui.label(RichText::new(crate::disp::station(units, a.start_station_m)).monospace());
                            ui.label(RichText::new(crate::disp::station(units, a.end_station_m())).monospace());
                            ui.label(RichText::new(crate::disp::len_u(units, a.length())).monospace());
                            ui.label(a.elements.len().to_string());
                            let mut also = Vec::new();
                            if let Some(p) = &a.profile {
                                also.push(format!("profile ({} PVIs)", p.pvis.len()));
                            }
                            if !a.station_equations.is_empty() {
                                also.push(format!("{} station equation(s)", a.station_equations.equations.len()));
                            }
                            if let Some(s) = &a.superelevation {
                                also.push(format!("superelevation ({} sections)", s.sections.len()));
                            }
                            ui.label(if also.is_empty() { "plan only".to_string() } else { also.join(", ") });
                            ui.end_row();
                        }
                    });
                });
            }
            notes(ui, &import.notes, "What the reader says about the alignments");
        }
    }
}

fn surfaces_tab(ui: &mut egui::Ui, form: &mut LandXmlForm) {
    match &form.surfaces {
        Err(e) => {
            ui_kit::callout(ui, Level::Error, format!("The file cannot be read as LandXML: {e}"));
        }
        Ok(import) => {
            if import.surfaces.is_empty() {
                ui_kit::callout(ui, Level::Info, "The file has no TIN surfaces.");
            } else {
                ui_kit::section(ui, "Surfaces in the file", |ui| {
                    let rows: Vec<Vec<String>> = import
                        .surfaces
                        .iter()
                        .map(|s| {
                            let b = s.tin.bounds();
                            vec![s.name.clone(), s.tin.points().len().to_string(), s.tin.triangles().len().to_string(), format!("{:.1} to {:.1} m", b.min_z, b.max_z), format!("{:.0} m2", s.tin.plan_area()), s.invisible_faces.to_string()]
                        })
                        .collect();
                    ui.horizontal(|ui| {
                        if ui.small_button("Tick all").clicked() {
                            form.take_surface.iter_mut().for_each(|t| *t = true);
                        }
                        if ui.small_button("Untick all").clicked() {
                            form.take_surface.iter_mut().for_each(|t| *t = false);
                        }
                    });
                    for (i, row) in rows.iter().enumerate() {
                        ui.checkbox(&mut form.take_surface[i], format!("{}   {} points, {} triangles", row[0], row[1], row[2]));
                    }
                    ui.add_space(4.0);
                    ui_kit::results_table(ui, "lx-surfaces", &["Surface", "Points", "Triangles", "Levels", "Plan area", "Hidden faces left out"], &[Col::Text, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num], &rows);
                });
            }
            notes(ui, &import.notes, "What the reader says about the surfaces");
        }
    }
}

fn notes(ui: &mut egui::Ui, notes: &[String], heading: &str) {
    if notes.is_empty() {
        return;
    }
    ui_kit::section(ui, heading, |ui| {
        for n in notes {
            ui_kit::callout(ui, Level::Warning, n.as_str());
            ui.add_space(2.0);
        }
    });
}
