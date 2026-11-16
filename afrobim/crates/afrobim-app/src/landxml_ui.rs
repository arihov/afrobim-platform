//! The LandXML import dialog. The file is read first and the dialog shows what is in it: the
//! alignments (station range, length, whether the file gave a profile, station equations and
//! superelevation) and the TIN surfaces (points, triangles, extent), with the readers' own notes.
//! The engineer ticks what to bring in; nothing is added until the button is pressed. The reading is
//! `afrobim_alignment::read_landxml` and `afrobim_corridor::read_landxml_surfaces`.

use std::path::PathBuf;

use afrobim_alignment::{read_landxml, read_landxml_file, LandXmlFile, LandXmlImport};
use afrobim_corridor::{read_landxml_surfaces, SurfaceImport};
use afrobim_kernel::{project_crs, PlanExtent, ProjectCrs, SelectMode};
use egui::RichText;

use crate::app::AfroBimApp;
use crate::ui_kit::{self, Col, Dialog, Footer, Level};

pub struct LandXmlForm {
    path: PathBuf,
    /// The whole file, read once for the File tab: coordinate system, units, application, extents.
    file: Result<LandXmlFile, String>,
    /// What the file declares as a coordinate system, in the application's own type.
    file_crs: Option<ProjectCrs>,
    /// The project had no system when the dialog opened and the file declares one: offer it.
    use_file_crs: bool,
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
        // Tabs: 0 the file, 1 alignments, 2 surfaces. The file comes first: it says where the
        // coordinates are, before anything is brought in.
        let tab = match (&alignments, &surfaces) {
            (Ok(_), Ok(s)) if surfaces_first && !s.surfaces.is_empty() => 2,
            _ => 0,
        };
        let file = read_landxml_file(&path).map_err(|e| e.to_string());
        let file_crs = file.as_ref().ok().and_then(|f| f.coordinate_system.as_ref()).and_then(crate::crs_ui::from_landxml);
        LandXmlForm { path, file, file_crs, use_file_crs: true, alignments, surfaces, take_alignment, take_surface, tab }
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
        let project = project_crs(&self.doc);
        let labels = ["The file".to_string(), format!("Alignments ({alignment_count})"), format!("Surfaces ({surface_count})")];
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
                    match form.tab {
                        0 => file_tab(ui, &mut form, project.as_ref()),
                        1 => alignments_tab(ui, &mut form, &units),
                        _ => surfaces_tab(ui, &mut form),
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
        // The project takes the file's coordinate system first (when it had none and that is
        // wanted), so the status bar reads the imported objects in it at once. Nothing moves.
        if project_crs(&self.doc).is_none() && form.use_file_crs {
            if let Some(c) = &form.file_crs {
                self.apply_project_crs(c.clone());
            }
        }
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
                    match crate::alignment_glue::add_surface_at_source(&mut self.doc, &surface.name, &surface.tin, source.as_deref()) {
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

/// The File tab: what the file says about itself and where it sits, and what that means for the
/// project's coordinate system. Nothing here changes any coordinate.
fn file_tab(ui: &mut egui::Ui, form: &mut LandXmlForm, project: Option<&ProjectCrs>) {
    let file = match &form.file {
        Err(e) => {
            ui_kit::callout(ui, Level::Error, format!("The file cannot be read as LandXML: {e}"));
            return;
        }
        Ok(f) => f,
    };
    ui_kit::section(ui, "The file", |ui| {
        ui_kit::form(ui, "lx-file", |f| {
            f.row("Written by", |ui| {
                let app = file.applications.first().map(|a| {
                    let mut t = a.name.clone();
                    if let Some(v) = &a.version {
                        t.push_str(&format!(" {v}"));
                    }
                    if let Some(m) = &a.manufacturer {
                        t.push_str(&format!(" ({m})"));
                    }
                    t
                });
                ui.label(match (file.is_civil_3d(), app) {
                    (true, Some(a)) => format!("Civil 3D: {a}"),
                    (false, Some(a)) => format!("another application: {a}"),
                    (_, None) => "no Application element: not stated".to_string(),
                });
            });
            f.row("LandXML version", |ui| {
                ui.label(file.header.version.clone().unwrap_or_else(|| "not stated".into()));
            });
            f.row_note("Units", "lengths, coordinates and levels are converted to metres on reading", |ui| {
                let u = &file.units;
                ui.label(if !u.declared {
                    "no Units element: metres are assumed".to_string()
                } else if u.imperial {
                    format!("Imperial, linear unit {} ({} m each)", u.linear_unit, u.metres_per_linear_unit)
                } else {
                    format!("Metric, linear unit {}", u.linear_unit)
                });
            });
        });
    });
    ui_kit::section(ui, "Coordinate system in the file", |ui| match (&file.coordinate_system, &form.file_crs) {
        (Some(_), Some(c)) => {
            ui.label(RichText::new(c.label()).strong());
            let line = |label: &str, v: &str| (!v.is_empty()).then(|| format!("{label}: {v}"));
            let rows: Vec<String> = [line("Horizontal datum", &c.horizontal_datum), line("Vertical datum", &c.vertical_datum)].into_iter().flatten().collect();
            if !rows.is_empty() {
                ui.label(RichText::new(rows.join("   ")).small().weak());
            }
            ui.label(RichText::new(crate::crs_ui::kind_text(c.kind)).small().weak());
        }
        _ => {
            ui_kit::callout(ui, Level::Info, "The file does not declare a coordinate system.");
        }
    });
    ui_kit::section(ui, "Extent, as the file gives it (metres)", |ui| {
        let sum = &file.summary;
        let row = |name: &str, c: &afrobim_alignment::LandXmlClassSummary| {
            let ext = |lo: f64, hi: f64| format!("{lo:.3} to {hi:.3}");
            match c.extent {
                Some(e) => vec![name.to_string(), c.count.to_string(), ext(e.min_easting, e.max_easting), ext(e.min_northing, e.max_northing)],
                None => vec![name.to_string(), c.count.to_string(), "-".into(), "-".into()],
            }
        };
        let rows: Vec<Vec<String>> = [("Alignments", &sum.alignments), ("Surfaces", &sum.surfaces), ("Control points", &sum.cg_points), ("Parcels", &sum.parcels), ("Plan features", &sum.plan_features)]
            .into_iter()
            .filter(|(_, c)| c.count > 0)
            .map(|(n, c)| row(n, c))
            .collect();
        if rows.is_empty() {
            ui.label("The file holds nothing with a plan position.");
        } else {
            ui_kit::results_table(ui, "lx-extents", &["Class", "Count", "Easting", "Northing"], &[Col::Text, Col::Num, Col::Num, Col::Num], &rows);
        }
    });
    ui_kit::section(ui, "Against the project coordinate system", |ui| {
        let extent = file.summary.overall_extent().map(|e| PlanExtent { min_easting: e.min_easting, min_northing: e.min_northing, max_easting: e.max_easting, max_northing: e.max_northing });
        let name = form.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "The file".into());
        match project {
            Some(p) => ui.label(RichText::new(format!("Project: {}", p.label())).strong()),
            None => ui.label(RichText::new("The project has no coordinate system.").strong()),
        };
        crate::crs_ui::show_findings(ui, &crate::crs_ui::dataset_check(project, &name, form.file_crs.as_ref(), extent));
        ui_kit::callout(ui, Level::Info, "Objects come in at the coordinates of the file, easting and northing times 1000 in model millimetres. Nothing is reprojected, shifted or re-based, whatever the check says.");
        if project.is_none() && form.file_crs.is_some() {
            ui.add_space(4.0);
            ui_kit::toggle(ui, &mut form.use_file_crs, "Use the file's coordinate system for the project");
        }
    });
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
