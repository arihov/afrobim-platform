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
    /// Civil data of the file: point groups, parcels and ground profiles to bring in, and whether
    /// a surface that has source data comes in as a definition that keeps it.
    take_points: Vec<bool>,
    take_parcels: Vec<bool>,
    take_profiles: Vec<bool>,
    /// The alignments of the file that have cross-sections (the names, in file order) and which to bring in.
    section_alignments: Vec<(String, usize)>,
    take_sections: Vec<bool>,
    /// The pipe networks of the file (`PipeNetworks`), read by the pipes crate, and which to bring in.
    pipes: Result<afrobim_pipes::LandXmlPipes, String>,
    take_pipes: Vec<bool>,
    as_definition: bool,
    tab: usize,
    /// The offer shown when the file declares a system that differs from the project's.
    crs: crate::transform_ui::ImportCrs,
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
        let pipes = afrobim_pipes::read_landxml_pipes(&path).map_err(|e| e.to_string());
        let take_pipes = pipes.as_ref().map(|p| vec![true; p.networks.len()]).unwrap_or_default();
        let file_crs = file.as_ref().ok().and_then(|f| f.coordinate_system.as_ref()).and_then(crate::crs_ui::from_landxml);
        let crs = crate::transform_ui::ImportCrs::new(file_crs.as_ref());
        let (np, nr, nf) = file.as_ref().map(|f| (crate::civil_landxml::point_groups(f).len(), crate::civil_landxml::all_parcels(f).len(), f.ground_profiles.len())).unwrap_or((0, 0, 0));
        let mut section_alignments: Vec<(String, usize)> = Vec::new();
        if let Ok(f) = &file {
            for s in &f.cross_sections {
                match section_alignments.iter_mut().find(|(n, _)| *n == s.alignment) {
                    Some((_, c)) => *c += 1,
                    None => section_alignments.push((s.alignment.clone(), 1)),
                }
            }
        }
        let take_sections = vec![true; section_alignments.len()];
        // A file written by Civil 3D carries its own TIN, which is what its sections and profiles were
        // cut from: it comes in as that TIN, not as source data that AfroBIM would triangulate again
        // (a different triangulation, and a stray source point would stretch it over kilometres).
        let as_definition = !file.as_ref().is_ok_and(|f| f.is_civil_3d());
        LandXmlForm { path, file, file_crs, use_file_crs: true, alignments, surfaces, take_alignment, take_surface, take_points: vec![true; np], take_parcels: vec![true; nr], take_profiles: vec![true; nf], section_alignments, take_sections, pipes, take_pipes, as_definition, tab, crs }
    }

    fn picked(&self) -> (usize, usize) {
        (self.take_alignment.iter().filter(|b| **b).count(), self.take_surface.iter().filter(|b| **b).count())
    }

    fn civil_picked(&self) -> usize {
        [&self.take_points, &self.take_parcels, &self.take_profiles, &self.take_sections, &self.take_pipes].iter().map(|v| v.iter().filter(|b| **b).count()).sum()
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
        let nothing_in_file = form.alignments.as_ref().map(|a| a.alignments.is_empty()).unwrap_or(true) && form.surfaces.as_ref().map(|s| s.surfaces.is_empty()).unwrap_or(true) && form.take_points.is_empty() && form.take_parcels.is_empty() && form.take_profiles.is_empty() && form.take_sections.is_empty() && form.take_pipes.is_empty();
        let reason = if nothing_in_file { "The file holds no alignment or surface" } else { "Tick something to import" };
        let alignment_count = form.alignments.as_ref().map(|a| a.alignments.len()).unwrap_or(0);
        let surface_count = form.surfaces.as_ref().map(|s| s.surfaces.len()).unwrap_or(0);
        let project = project_crs(&self.doc);
        let labels = ["The file".to_string(), format!("Alignments ({alignment_count})"), format!("Surfaces ({surface_count})"), format!("Points ({})", form.take_points.len()), format!("Parcels ({})", form.take_parcels.len()), format!("Profiles ({})", form.take_profiles.len()), format!("Sections ({})", form.take_sections.len()), format!("Pipe networks ({})", form.take_pipes.len())];
        let tab_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let nc = form.civil_picked();
        let button = format!("Import {}{}", if nc > 0 { format!("{nc} civil object(s) and ") } else { String::new() }, match (na, ns) {
            (0, 0) => "nothing".to_string(),
            (a, 0) => format!("{a} alignment(s)"),
            (0, s) => format!("{s} surface(s)"),
            (a, s) => format!("{a} alignment(s) and {s} surface(s)"),
        });
        let crs_state = std::cell::Cell::new((true, ""));
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
                        2 => surfaces_tab(ui, &mut form),
                        3 => civil_tab(ui, &mut form, 0),
                        4 => civil_tab(ui, &mut form, 1),
                        5 => civil_tab(ui, &mut form, 2),
                        6 => sections_tab(ui, &mut form),
                        _ => pipes_tab(ui, &mut form),
                    }
                    crs_state.set((form.crs.ready(), form.crs.why_not()));
                },
                |ui| Footer::new(&button).enabled(na + ns + nc > 0 && crs_state.get().0, if crs_state.get().0 { reason } else { crs_state.get().1 }).icon(crate::glyphs::by_name("LANDXML")).show(ui),
            );
        if out.closed || out.inner.is_some_and(|(_, f)| f.cancel) {
            return;
        }
        if out.inner.is_some_and(|(_, f)| f.primary) {
            let before: Vec<afrobim_kernel::ElementId> = self.doc.elements().iter().map(|e| e.id).collect();
            self.landxml_apply(Some(ctx), &form);
            self.finish_import_transform(&form.crs, &before, &form.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
            return;
        }
        self.landxml.form = Some(form);
    }

    /// Imports everything the file holds, as the dialog does with every box ticked, with no dialog
    /// (the command line and the tests). Prints the time it took when AFROBIM_FRAME_STATS is set.
    pub(crate) fn landxml_import_all(&mut self, path: &std::path::Path) -> String {
        let t0 = std::time::Instant::now();
        let form = LandXmlForm::new(path.to_path_buf(), false);
        let read = t0.elapsed();
        self.landxml_apply(None, &form);
        if self.options.landxml_compare {
            let ids: Vec<afrobim_kernel::ElementId> = self.doc.elements().iter().filter(|e| e.category == crate::civil::SAMPLES).map(|e| e.id).collect();
            for id in ids {
                if let Some(mut d) = crate::civil::get::<crate::civil::SampleData>(&self.doc, id, crate::civil::SAMPLES) {
                    d.compare = true;
                    crate::civil::set(&mut self.doc, id, serde_json::to_value(&d).unwrap_or_default(), None);
                }
            }
        }
        if std::env::var_os("AFROBIM_FRAME_STATS").is_some() {
            for e in self.doc.elements().iter().filter(|e| e.category == crate::civil::SAMPLES || e.category == crate::civil::PROFILE_VIEW).take(40) {
                eprintln!("PLACE {} {} at E {:.0} N {:.0}", e.category, e.id.0, e.origin.x / 1000.0, e.origin.y / 1000.0);
            }
            eprintln!("IMPORTSTATS read={:.0}ms total={:.0}ms elements={}", read.as_secs_f64() * 1000.0, t0.elapsed().as_secs_f64() * 1000.0, self.doc.elements().len());
        }
        self.status.clone()
    }

    fn landxml_apply(&mut self, ctx: Option<&egui::Context>, form: &LandXmlForm) {
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
        let mut names: Vec<(String, String)> = Vec::new();
        if let Ok(a) = &form.alignments {
            for (alignment, take) in a.alignments.iter().zip(&form.take_alignment) {
                if !*take {
                    continue;
                }
                if let Some(id) = crate::alignment_glue::add(&mut self.doc, alignment) {
                    added_a += 1;
                    if let Some(name) = self.doc.find(id).map(|e| e.name.clone()) {
                        names.push((alignment.name.clone(), name));
                    }
                    crate::civil_super::keep_table(&mut self.doc, id, form.file.as_ref().ok(), &alignment.name);
                }
            }
            if form.take_alignment.iter().any(|t| *t) {
                notes.extend(a.notes.iter().map(|n| format!("LandXML: {n}")));
            }
        }
        if added_a > 0 {
            // A civil project looks at its alignments in plan: the layer the sign design hides them
            // on is shown (the Layers panel still switches it).
            if let Some(l) = self.doc.layers_mut().iter_mut().find(|l| l.name == crate::alignment_glue::LAYER) {
                l.visible = true;
            }
            self.design.invalidate();
            crate::markings_glue::sync(&mut self.doc);
            for problem in crate::modules_glue::sync_all(&mut self.doc) {
                self.log(format!("Not brought up to date: {problem}"));
            }
        }
        let mut last_surface = None;
        // A surface that has source data comes in as a definition that keeps it (the engine rebuilds
        // it), when the dialog says so; its TIN is then not added a second time.
        let mut as_definition: Vec<bool> = vec![false; form.take_surface.len()];
        if form.as_definition {
            if let Ok(f) = &form.file {
                for (i, ls) in f.surfaces.iter().enumerate() {
                    if form.take_surface.get(i).copied().unwrap_or(false) && !ls.source_data.is_empty() {
                        let src = form.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        if let Some(def) = crate::civil_landxml::surface_definition(ls, &src) {
                            let id = crate::civil_landxml::add_surface_def(&mut self.doc, def, &src);
                            last_surface = Some(id);
                            added_s += 1;
                            if let Some(slot) = as_definition.get_mut(i) {
                                *slot = true;
                            }
                        }
                    }
                }
            }
        }
        if let Ok(s) = &form.surfaces {
            let source = form.path.file_name().map(|n| n.to_string_lossy().to_string());
            for (i, (surface, take)) in s.surfaces.iter().zip(&form.take_surface).enumerate() {
                if *take && !as_definition.get(i).copied().unwrap_or(false) {
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
            if let Some(l) = self.doc.layers_mut().iter_mut().find(|l| l.name == "Surfaces") {
                l.visible = true;
            }
            self.sync_generated();
            self.fitted_3d = false;
            if let Some(id) = last_surface {
                self.doc.select(&[id], SelectMode::Replace);
            }
        }
        let added_c = self.landxml_apply_civil(form, &names, &mut notes);
        self.status = format!("Imported {added_a} alignment(s), {added_s} surface(s) and {added_c} civil object(s) from {}.", form.path.display());
        for n in notes.iter().take(6) {
            self.log(n.clone());
        }
        if !notes.is_empty() {
            self.status.push_str(&format!(" {} note(s) in the command history.", notes.len()));
        }
        self.zoom_extents_pending = true;
        if let Some(ctx) = ctx {
            self.schedule_commit(ctx);
        }
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
        ui_kit::callout(ui, Level::Info, "Objects come in at the coordinates of the file, easting and northing times 1000 in model millimetres, unless you choose below to transform them. Nothing is reprojected by default.");
        if crate::transform_ui::differs(form.file_crs.as_ref(), project) {
            if let Some(p) = project {
                form.crs.show(ui, "lx-crs", p, extent.map(|e| ((e.min_easting, e.min_northing), (e.max_easting, e.max_northing))));
            }
        }
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
                    ui_kit::toggle(ui, &mut form.as_definition, "A surface with source data comes in as that data, to be triangulated again by AfroBIM");
                    ui_kit::note(ui, "Off (the default for a Civil 3D file): the surface comes in with the triangles of the file, the ones its sections and ground profiles were cut from.");
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

/// The Points, Parcels and Profiles tabs: what the file holds beyond alignments and surfaces.
fn civil_tab(ui: &mut egui::Ui, form: &mut LandXmlForm, which: usize) {
    let Ok(file) = &form.file else {
        ui_kit::callout(ui, Level::Error, "The file cannot be read as LandXML.");
        return;
    };
    match which {
        0 => {
            let groups = crate::civil_landxml::point_groups(file);
            if groups.is_empty() {
                ui_kit::callout(ui, Level::Info, "The file has no control points (CgPoints).");
                return;
            }
            ui_kit::section(ui, "Point groups in the file (each becomes a point set)", |ui| {
                for (i, (name, g)) in groups.iter().enumerate() {
                    ui.checkbox(&mut form.take_points[i], format!("{name}  ({} points)", g.points.len()));
                }
            });
        }
        1 => {
            let parcels = crate::civil_landxml::all_parcels(file);
            if parcels.is_empty() {
                ui_kit::callout(ui, Level::Info, "The file has no parcels.");
                return;
            }
            ui_kit::section(ui, "Parcels in the file", |ui| {
                for (i, p) in parcels.iter().enumerate() {
                    let area = p.area_m2.map(|a| format!("{a:.2} m2 in the file")).unwrap_or_else(|| "no area in the file".into());
                    ui.checkbox(&mut form.take_parcels[i], format!("{}  ({area}{})", p.name, if p.coord_geoms.is_empty() { ", no geometry" } else { "" }));
                }
            });
            crate::ui_kit::note(ui, "A spiral in a parcel boundary is carried as its chord. The area is recomputed from the boundary.");
        }
        _ => {
            if file.ground_profiles.is_empty() {
                ui_kit::callout(ui, Level::Info, "The file has no ground profiles (ProfSurf).");
                return;
            }
            ui_kit::section(ui, "Ground profiles in the file (shown in profile views)", |ui| {
                for (i, p) in file.ground_profiles.iter().enumerate() {
                    ui.checkbox(&mut form.take_profiles[i], format!("{}  on {}  ({} line(s))", p.name, p.alignment, p.point_lists.len()));
                }
            });
        }
    }
}

/// The Pipe networks tab: the networks of the file with their structures and pipes, and the notes
/// of the reader. A network comes in with no parts list (the file carries sizes, not parts).
fn pipes_tab(ui: &mut egui::Ui, form: &mut LandXmlForm) {
    match &form.pipes {
        Err(e) => {
            ui_kit::callout(ui, Level::Warning, format!("The pipe networks of the file cannot be read: {e}"));
        }
        Ok(p) if p.networks.is_empty() => {
            ui_kit::callout(ui, Level::Info, "The file has no pipe networks (PipeNetworks).");
        }
        Ok(p) => {
            ui_kit::section(ui, "Pipe networks in the file (each becomes a pipe network)", |ui| {
                for (i, n) in p.networks.iter().enumerate() {
                    ui.checkbox(&mut form.take_pipes[i], format!("{}  ({:?}, {} structures, {} pipes)", n.name, n.network_type, n.structure_count(), n.pipe_count()));
                }
            });
            for n in &p.notes {
                ui.label(egui::RichText::new(n).small().weak());
            }
            ui_kit::note(ui, "Coordinates come in as they are in the file. The file carries no ground surface or rules: set them in the network properties.");
        }
    }
}

impl AfroBimApp {
    /// The points, parcels and ground profiles ticked in the dialog. Returns how many were added.
    fn landxml_apply_civil(&mut self, form: &LandXmlForm, names: &[(String, String)], notes: &mut Vec<String>) -> usize {
        let Ok(file) = &form.file else { return 0 };
        let source = form.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let mut added = 0;
        for ((name, g), take) in crate::civil_landxml::point_groups(file).into_iter().zip(&form.take_points) {
            if *take {
                let (_, n) = crate::civil_landxml::add_points(&mut self.doc, &name, g, &source);
                notes.push(format!("LandXML: {n} point(s) from {name}."));
                added += 1;
            }
        }
        let mut number = self.doc.elements().iter().filter(|e| e.category == crate::civil::PARCEL).count() as u32 + 1;
        for (p, take) in crate::civil_landxml::all_parcels(file).into_iter().zip(&form.take_parcels) {
            if *take {
                match crate::civil_landxml::parcel_from(p, number).map(|pc| (pc.area().ok(), crate::civil_landxml::add_parcel(&mut self.doc, &pc, &source))) {
                    Ok((area, Some(_))) => {
                        added += 1;
                        number += 1;
                        if let (Some(a), Some(fa)) = (area, p.area_m2) {
                            if (a - fa).abs() > 0.01 * fa.max(1.0) {
                                notes.push(format!("LandXML: parcel {} has {fa:.2} m2 in the file and {a:.2} m2 from its boundary.", p.name));
                            }
                        }
                    }
                    Ok((_, None)) => notes.push(format!("LandXML: parcel {} could not be drawn.", p.name)),
                    Err(e) => notes.push(format!("LandXML: parcel {} not added: {e}", p.name)),
                }
            }
        }
        let mut profile_ids = Vec::new();
        for (ps, take) in file.ground_profiles.iter().zip(&form.take_profiles) {
            if *take {
                let ids = crate::civil_landxml::add_profile(&mut self.doc, ps, &source);
                added += ids.len();
                profile_ids.extend(ids);
            }
        }
        notes.extend(crate::civil_sections::assign_profile_surfaces(&mut self.doc, &profile_ids).into_iter().map(|n| format!("LandXML: {n}")));
        // A profile view for each alignment that has a ground profile, stacked south of the project,
        // showing the alignment's own design profile where the file gave one.
        if !profile_ids.is_empty() {
            let ext = project_extent(&self.doc);
            let (x0, y0) = ext.map(|e| (e.0, e.1 - 400.0)).unwrap_or((0.0, 0.0));
            let mut row = 0.0;
            let mut by_alignment: Vec<(String, Vec<u64>)> = Vec::new();
            for id in &profile_ids {
                if let Some(d) = crate::civil::get::<crate::civil::ProfileData>(&self.doc, *id, crate::civil::PROFILE) {
                    match by_alignment.iter_mut().find(|(a, _)| *a == d.alignment) {
                        Some((_, v)) => v.push(id.0),
                        None => by_alignment.push((d.alignment.clone(), vec![id.0])),
                    }
                }
            }
            for (al, ids) in by_alignment {
                let design = names.iter().find(|(f, _)| *f == al).and_then(|(_, n)| crate::alignment_glue::alignment_by_name(&self.doc, n)).is_some_and(|a| a.profile.is_some());
                let data = crate::civil::ProfileViewData { alignment: al.clone(), profiles: ids, show_design: design, vertical_exaggeration: 5.0, datum: None, insertion: [x0, y0 - row], show_bands: false, band_height_m: 3.0, cloud: vec![] };
                let name = self.doc.unique_name(&format!("Profile view {al}"));
                crate::civil::add(&mut self.doc, crate::civil::PROFILE_VIEW, &name, serde_json::to_value(&data).unwrap_or_default(), Some(crate::civil::Bounds::point(x0, y0 - row)), &source);
                added += 1;
                row += 300.0;
            }
        }
        // The cross-sections, as sample line groups on the alignments that came in, laid out in an
        // array east of everything the file holds.
        let wanted: Vec<(String, String)> = form.section_alignments.iter().zip(&form.take_sections).filter(|(_, t)| **t).filter_map(|((n, _), _)| names.iter().find(|(f, _)| f == n).cloned()).collect();
        if !wanted.is_empty() {
            let ext = project_extent(&self.doc);
            let origin = ext.map(|e| (e.2 + 500.0, e.1)).unwrap_or((0.0, 0.0));
            let r = crate::civil_sections::import_sections(&mut self.doc, file, &wanted, &source, origin);
            added += r.groups.len();
            notes.push(format!("LandXML: {} section line(s) in {} group(s), each with the file's own section surfaces.", r.lines, r.groups.len()));
            notes.extend(r.notes.into_iter().map(|n| format!("LandXML: {n}")));
        } else if form.take_sections.iter().any(|t| *t) {
            notes.push("LandXML: cross-sections were ticked but their alignments were not imported: no section lines made.".into());
        }
        if let Ok(p) = &form.pipes {
            for (net, take) in p.networks.iter().zip(&form.take_pipes) {
                if *take {
                    let mut data = crate::civil_pipes::PipeNetData::default();
                    data.network = net.clone();
                    let id = crate::civil_pipes::add(&mut self.doc, &net.name, &data, &source);
                    let _ = id;
                    added += 1;
                    notes.push(format!("LandXML: pipe network {} with {} structures and {} pipes.", net.name, net.structure_count(), net.pipe_count()));
                }
            }
            if form.take_pipes.iter().any(|t| *t) {
                notes.extend(p.notes.iter().map(|n| format!("LandXML: {n}")));
            }
        }
        added
    }
}

/// The Sections tab: the alignments of the file that have cross-sections.
fn sections_tab(ui: &mut egui::Ui, form: &mut LandXmlForm) {
    if form.section_alignments.is_empty() {
        ui_kit::callout(ui, Level::Info, "The file has no cross-sections (CrossSects).");
        return;
    }
    let Ok(file) = &form.file else { return };
    ui_kit::section(ui, "Cross-sections in the file (each alignment gets a sample line group)", |ui| {
        for (i, (name, n)) in form.section_alignments.iter().enumerate() {
            let surfaces: Vec<&str> = {
                let mut v: Vec<&str> = Vec::new();
                for s in file.cross_sections.iter().filter(|s| &s.alignment == name).take(1) {
                    v.extend(s.surfaces.iter().map(|x| x.name.as_str()));
                }
                v
            };
            ui.checkbox(&mut form.take_sections[i], format!("{name}  ({n} section(s), {} section surface(s) at the first)", surfaces.len()));
        }
    });
    ui_kit::note(ui, "Each section line keeps the file's named section surfaces and draws them in the section view array and the section sheets. A surface of the document with the name of a section surface can be compared with them: tick Compare with AfroBIM's own cut in the group's Properties.");
}

/// The plan extent in metres (least easting, least northing, greatest easting, greatest northing)
/// of the alignments and surfaces of a document: where the sections and profile views are laid out
/// beside them. Read from the elements, so a stray source point of a file does not stretch it.
fn project_extent(doc: &afrobim_kernel::Document) -> Option<(f64, f64, f64, f64)> {
    doc.elements()
        .iter()
        .filter(|e| e.category == afrobim_alignment::CATEGORY || e.category == afrobim_corridor::SURFACE_CATEGORY || e.category == crate::civil::SURFACE)
        .map(|e| (e.origin.x / 1000.0, e.origin.y / 1000.0, (e.origin.x + e.width) / 1000.0, (e.origin.y + e.height) / 1000.0))
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
}
