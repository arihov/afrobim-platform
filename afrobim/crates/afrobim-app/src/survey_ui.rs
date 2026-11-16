//! SURVEY: a ground surface from a file of survey points.
//!
//! The command asks for a point file (CSV or text), then shows a dialog with what the engineer has
//! to say about it: the order of the columns, the coordinate system, the longest side a triangle
//! may have, and the descriptions (field codes) whose points are not ground. The reading and the
//! triangulation are `afrobim_corridor`'s (`survey.rs`); this file is the dialog, the same import
//! from the command line (`--survey` and its companions), and the words that connect the two.
//!
//! Nothing in the dialog is a design value. The column order starts at PNEZD because that is how
//! surveyors' files usually come, and the first points are shown as they would be read so that
//! the choice can be checked by eye; the coordinate system, the side limit and the list of codes
//! start empty.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use afrobim_corridor::{read_survey_points, ColumnOrder, CoordinateSystem, SurfaceOptions, SurveyFormat, SurveyImportRequest, SurveyPoints};
use afrobim_kernel::{Document, ElementId, SelectMode};
use egui::RichText;

use crate::app::AfroBimApp;
use crate::ui_kit::{self, Col, Dialog, Footer, Level, NumRule};

/// The units a point file can be in, and the metres in one of each (international foot, US survey foot).
const UNITS: [(&str, f64); 3] = [("Metres", 1.0), ("International feet", 0.3048), ("US survey feet", 1200.0 / 3937.0)];

const CRS_HINT: &str = "for example WGS 84 / UTM zone 36S, or EPSG:32736";

/// The request of the dialog or the command line, from what was typed.
///
/// `crs` blank is no coordinate system; `max_side` blank is no limit. `exclude` is a list of
/// descriptions separated by commas.
pub fn request(name: &str, order: ColumnOrder, crs: &str, max_side: &str, exclude: &str) -> Result<SurveyImportRequest, String> {
    request_in(name, SurveyFormat::metres(order), crs, max_side, exclude)
}

/// As [`request`], for a file whose coordinates and levels are in another unit (`format.unit_m`).
pub fn request_in(name: &str, format: SurveyFormat, crs: &str, max_side: &str, exclude: &str) -> Result<SurveyImportRequest, String> {
    let crs = match crs.trim() {
        "" => None,
        typed => Some(CoordinateSystem::parse(typed).ok_or_else(|| {
            format!("'{typed}' is not a coordinate system AfroBIM knows. The WGS 84 / UTM zones are known: type {CRS_HINT}, with N or S for the hemisphere.")
        })?),
    };
    let max_side_m = match max_side.trim() {
        "" => None,
        typed => match typed.parse::<f64>() {
            Ok(v) if v.is_finite() && v > 0.0 => Some(v),
            _ => return Err(format!("The longest triangle side must be a length in metres greater than 0, or blank for no limit; '{typed}' is neither.")),
        },
    };
    let exclude = exclude.split(',').map(str::trim).filter(|c| !c.is_empty()).map(str::to_string).collect();
    if !(format.unit_m.is_finite() && format.unit_m > 0.0) {
        return Err("The file's unit must be a length in metres greater than 0.".into());
    }
    Ok(SurveyImportRequest { name: name.trim().to_string(), format, options: SurfaceOptions { exclude, max_side_m }, crs })
}

/// Imports the file and returns the surface and the account of the import.
pub fn import(doc: &mut Document, path: &Path, request: &SurveyImportRequest) -> Result<(ElementId, Vec<String>), String> {
    let (id, report) = afrobim_corridor::import_survey_surface(doc, path, request)?;
    // At the coordinates of the file, as for LandXML: the element sits at its drawing's origin.
    crate::alignment_glue::place_at_source(doc, id);
    Ok((id, report))
}

/// The open dialog.
pub struct SurveyForm {
    path: PathBuf,
    name: String,
    order: ColumnOrder,
    /// 0 metres, 1 international feet, 2 US survey feet.
    unit: usize,
    /// Descriptions typed as patterns (a trailing * matches any ending), besides the ticked ones.
    patterns: String,
    utm_zone: u8,
    utm_south: bool,
    crs: String,
    max_side: String,
    /// The project's coordinate system when the dialog opened: the default for `crs`.
    project: Option<afrobim_kernel::ProjectCrs>,
    /// With no project system: also make the one entered here the project's.
    set_project: bool,
    /// Descriptions ticked to be left out, in capitals.
    excluded: BTreeSet<String>,
    /// The file as read in `order`, or why it cannot be.
    read: Result<SurveyPoints, String>,
    problem: Option<String>,
}

impl SurveyForm {
    fn new(path: PathBuf, project: Option<afrobim_kernel::ProjectCrs>) -> SurveyForm {
        let order = ColumnOrder::Pnezd;
        // The file is taken to be in the project's system unless it is told otherwise.
        let crs = project.as_ref().map(|p| p.code().unwrap_or_else(|| p.name.clone())).unwrap_or_default();
        SurveyForm {
            name: path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            read: read_survey_points(&path, &SurveyFormat::metres(order)).map_err(|e| e.to_string()),
            path,
            order,
            unit: 0,
            patterns: String::new(),
            utm_zone: 1,
            utm_south: false,
            crs,
            max_side: String::new(),
            project,
            set_project: true,
            excluded: BTreeSet::new(),
            problem: None,
        }
    }

    fn format(&self) -> SurveyFormat {
        SurveyFormat { order: self.order, unit_m: UNITS[self.unit.min(2)].1 }
    }

    fn read_again(&mut self) {
        self.read = read_survey_points(&self.path, &self.format()).map_err(|e| e.to_string());
        self.problem = None;
    }

    fn request(&self) -> Result<SurveyImportRequest, String> {
        let mut exclude: Vec<&str> = self.excluded.iter().map(String::as_str).collect();
        exclude.extend(self.patterns.split(',').map(str::trim).filter(|p| !p.is_empty()));
        let mut r = request_in(&self.name, self.format(), &self.crs, &self.max_side, &exclude.join(","))?;
        // A description may itself hold a comma-free blank entry: the points with none.
        if self.excluded.contains("") {
            r.options.exclude.push(String::new());
        }
        Ok(r)
    }
}

/// The SURVEY dialog, when it is open.
#[derive(Default)]
pub struct SurveyUi {
    form: Option<SurveyForm>,
}

impl AfroBimApp {
    /// Opens the SURVEY dialog on a file.
    pub(crate) fn survey_open(&mut self, path: PathBuf) {
        self.survey.form = Some(SurveyForm::new(path, afrobim_kernel::project_crs(&self.doc)));
    }

    /// Imports a point file with a ready request: the surface is added, selected and reported in
    /// the command history. Returns the account, or the reason nothing was imported.
    pub(crate) fn survey_import(&mut self, ctx: Option<&egui::Context>, path: &Path, request: &SurveyImportRequest) -> Result<Vec<String>, String> {
        let (id, report) = import(&mut self.doc, path, request)?;
        self.sync_generated();
        self.doc.select(&[id], SelectMode::Replace);
        let name = self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
        for line in &report {
            self.log(format!("SURVEY: {line}"));
        }
        self.status = format!("Made the surface '{name}' from {}. {} line(s) in the command history.", path.display(), report.len());
        self.fitted_3d = false;
        if let Some(ctx) = ctx {
            self.schedule_commit(ctx);
        }
        Ok(report)
    }

    pub(crate) fn survey_window(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.survey.form.take() else { return };
        let file = form.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let max_side = ui_kit::check_number(&form.max_side, &NumRule::default().above(0.0));
        let crs_ok = form.crs.trim().is_empty() || CoordinateSystem::parse(&form.crs).is_some();
        let read_ok = form.read.is_ok();
        let (enabled, reason) = match (read_ok, max_side.is_bad(), crs_ok) {
            (false, _, _) => (false, "The file cannot be read in this column order"),
            (_, true, _) => (false, "Correct the longest triangle side"),
            (_, _, false) => (false, "The coordinate system is not known"),
            _ => (true, ""),
        };
        let out = Dialog::new("Survey points", crate::glyphs::by_name("SURVEY"))
            .id_title(format!("Survey points: {file}"))
            .subtitle(file.clone())
            .width(620.0)
            .help("A ground surface is made from the points by triangulation. Coordinates and levels are read in the unit you choose and kept in metres. Nothing here is a design value: the column order starts at PNEZD because surveyors' files usually come that way, and the first points are shown as they are read so you can check it by eye.")
            .show(
                ctx,
                |ui| survey_body(ui, &mut form),
                |ui| Footer::new("Make the surface").icon(crate::glyphs::by_name("SURVEY")).enabled(enabled, reason).show(ui),
            );
        if out.closed || out.inner.is_some_and(|(_, f)| f.cancel) {
            return;
        }
        if out.inner.is_some_and(|(_, f)| f.primary) {
            match form.request().and_then(|request| self.survey_import(Some(ctx), &form.path, &request)) {
                Ok(_) => {
                    // With no project system, the one stated for this file becomes the project's, when ticked.
                    if form.project.is_none() && form.set_project {
                        if let Some(mut c) = CoordinateSystem::parse(&form.crs) {
                            c.source = afrobim_kernel::CrsSource::Survey;
                            self.apply_project_crs(c);
                        }
                    }
                    return;
                }
                Err(why) => form.problem = Some(why),
            }
        }
        self.survey.form = Some(form);
    }
}

fn survey_body(ui: &mut egui::Ui, form: &mut SurveyForm) {
    ui_kit::section(ui, "Surface", |ui| {
        ui_kit::form(ui, "survey-name", |f| {
            f.row("Name", |ui| {
                let r = ui_kit::text_field(ui, &mut form.name, "the file name when blank");
                ui_kit::focus_once(ui, &r, "survey-name");
            });
        });
    });
    ui_kit::section(ui, "The file", |ui| {
        ui_kit::form(ui, "survey-file", |f| {
            f.row("Columns", |ui| {
                let before = form.order;
                let labels: Vec<String> = ColumnOrder::ALL.iter().map(|o| format!("{}: {}", o.code(), o.label())).collect();
                let mut i = ColumnOrder::ALL.iter().position(|o| *o == form.order).unwrap_or(0);
                let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
                if ui_kit::combo(ui, "survey-order", &mut i, &refs) {
                    form.order = ColumnOrder::ALL[i];
                }
                if form.order != before {
                    form.read_again();
                }
            });
            f.row_note("Units of the file", "coordinates and levels are converted to metres", |ui| {
                let labels: Vec<&str> = UNITS.iter().map(|u| u.0).collect();
                let before = form.unit;
                ui_kit::segmented(ui, &mut form.unit, &labels);
                if form.unit != before {
                    form.read_again();
                }
            });
        });
        ui.add_space(4.0);
        match &form.read {
            Err(why) => {
                ui_kit::callout(ui, Level::Warning, format!("The file cannot be read in this column order: {why}"));
            }
            Ok(survey) => {
                ui.label(RichText::new(format!("{} points. The first, as they are read:", survey.points.len())).strong());
                let rows: Vec<Vec<String>> = survey
                    .points
                    .iter()
                    .take(3)
                    .map(|p| vec![if p.id.is_empty() { format!("line {}", p.line) } else { p.id.clone() }, format!("{:.3}", p.easting), format!("{:.3}", p.northing), format!("{:.3}", p.level), p.code.clone()])
                    .collect();
                ui_kit::results_table(ui, "survey-first", &["Point", "Easting", "Northing", "Level", "Description"], &[Col::Text, Col::Num, Col::Num, Col::Num, Col::Text], &rows);
                for note in &survey.notes {
                    ui.label(RichText::new(note).small().weak());
                }
            }
        }
    });
    ui_kit::section(ui, "Coordinate system", |ui| {
        ui_kit::form(ui, "survey-crs", |f| {
            f.row_note("System", CRS_HINT, |ui| {
                ui_kit::text_field(ui, &mut form.crs, "blank: not stated");
            });
            f.row("Or a WGS 84 UTM zone", |ui| {
                ui.horizontal(|ui| {
                    let mut zone = form.utm_zone as u32;
                    ui.add(egui::DragValue::new(&mut zone).range(1..=60).prefix("zone "));
                    form.utm_zone = zone as u8;
                    let mut i = usize::from(form.utm_south);
                    if ui_kit::segmented(ui, &mut i, &["North", "South"]) {
                        form.utm_south = i == 1;
                    }
                    if ui.button("Use").clicked() {
                        if let Some(c) = CoordinateSystem::utm_wgs84(form.utm_zone, form.utm_south) {
                            form.crs = c.code().unwrap_or_default();
                        }
                    }
                });
            });
        });
        if let (false, Ok(survey)) = (form.crs.trim().is_empty(), &form.read) {
            match CoordinateSystem::parse(&form.crs) {
                None => {
                    ui_kit::callout(ui, Level::Error, "That coordinate system is not known. The WGS 84 / UTM zones are, with N or S.");
                }
                Some(crs) => {
                    let extent = survey.plan_extent();
                    let doubts = extent.map(|(min, max)| crs.doubts(min, max)).unwrap_or_default();
                    if doubts.is_empty() {
                        let centre = extent.and_then(|(min, max)| crs.to_geographic((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0));
                        let place = centre.map(|(lat, lon)| format!(" The middle of the points is at latitude {lat:.5}, longitude {lon:.5}.")).unwrap_or_default();
                        ui_kit::callout(ui, Level::Success, format!("{}.{place}", crs.label()));
                    } else {
                        for doubt in doubts {
                            ui_kit::callout(ui, Level::Warning, doubt);
                        }
                    }
                }
            }
        }
    });
    if let (Ok(survey), Some(crs)) = (&form.read, CoordinateSystem::parse(&form.crs)) {
        match &form.project {
            Some(p) => {
                // Same or different from the project: said plainly. The file comes in at its own coordinates either way.
                let extent = survey.plan_extent().map(|(min, max)| afrobim_kernel::PlanExtent { min_easting: min.0, min_northing: min.1, max_easting: max.0, max_northing: max.1 });
                let findings: Vec<_> = crate::crs_ui::dataset_check(Some(p), &form.name, Some(&crs), extent)
                    .into_iter()
                    .filter(|f| matches!(f.kind, afrobim_kernel::FindingKind::SameSystem | afrobim_kernel::FindingKind::DifferentSystem))
                    .collect();
                crate::crs_ui::show_findings(ui, &findings);
            }
            None => {
                ui_kit::toggle(ui, &mut form.set_project, &format!("Also use {} for the project", crs.label()));
            }
        }
    }
    ui_kit::section(ui, "Triangulation", |ui| {
        ui_kit::form(ui, "survey-tri", |f| {
            f.row_note("Longest triangle side", "triangles with a longer side are removed, so the surface does not span ground that was not surveyed; blank keeps every triangle", |ui| {
                ui_kit::num_field(ui, &mut form.max_side, "m", &NumRule::default().above(0.0));
            });
        });
    });
    if let Ok(survey) = &form.read {
        let codes = survey.codes();
        ui_kit::section(ui, &format!("Points left out ({} descriptions)", codes.len()), |ui| {
            crate::ui_kit::note(ui, "Tick the descriptions that are not ground levels (inverts, tops of walls) to leave their points out.");
            ui.horizontal(|ui| {
                if ui.small_button("Untick all").clicked() {
                    form.excluded.clear();
                }
                let out: usize = codes.iter().filter(|(c, _)| form.excluded.contains(&c.to_ascii_uppercase())).map(|(_, n)| n).sum();
                ui.label(RichText::new(format!("{out} point(s) left out by ticks.")).small());
            });
            egui::ScrollArea::vertical().max_height(150.0).auto_shrink([false, true]).show(ui, |ui| {
                egui::Grid::new("survey-codes").num_columns(4).spacing([18.0, 2.0]).show(ui, |ui| {
                    for (i, (code, count)) in codes.iter().enumerate() {
                        let key = code.to_ascii_uppercase();
                        let mut out = form.excluded.contains(&key);
                        let shown = if code.is_empty() { "(no description)" } else { code.as_str() };
                        if ui.checkbox(&mut out, format!("{shown}  ({count})")).changed() {
                            if out {
                                form.excluded.insert(key);
                            } else {
                                form.excluded.remove(&key);
                            }
                        }
                        if i % 4 == 3 {
                            ui.end_row();
                        }
                    }
                });
            });
            ui_kit::advanced(ui, "survey-patterns", |ui| {
                ui_kit::form(ui, "survey-patterns-form", |f| {
                    f.row_note("Also leave out", "descriptions or patterns, separated by commas; a trailing * matches any ending (CULV-*)", |ui| {
                        ui_kit::text_field_wide(ui, &mut form.patterns, "for example CULV-*, WW");
                    });
                });
            });
        });
    }
    if let Some(problem) = &form.problem {
        ui.add_space(4.0);
        ui_kit::callout(ui, Level::Error, problem.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_values_become_a_request_or_a_reason() {
        let r = request(" EG ", ColumnOrder::Pnezd, "wgs 84 / utm zone 36s", "40", "CULV-INV, clvt-*,,").unwrap();
        assert_eq!(r.name, "EG");
        assert_eq!(r.format, SurveyFormat::metres(ColumnOrder::Pnezd));
        assert_eq!(r.crs.as_ref().and_then(|c| c.epsg), Some(32736));
        assert_eq!(r.options, SurfaceOptions { exclude: vec!["CULV-INV".into(), "clvt-*".into()], max_side_m: Some(40.0) });
        // Blank is none, for both.
        let r = request("", ColumnOrder::Enzd, "  ", "", "").unwrap();
        assert!(r.crs.is_none() && r.options.max_side_m.is_none() && r.options.exclude.is_empty());
        // What cannot be understood is said, never guessed.
        assert!(request("", ColumnOrder::Pnezd, "UTM 36", "", "").unwrap_err().contains("with N or S"));
        assert!(request("", ColumnOrder::Pnezd, "Arc 1960 / UTM zone 38S", "", "").unwrap_err().contains("not a coordinate system AfroBIM knows"));
        for bad in ["0", "-5", "ten", "inf"] {
            assert!(request("", ColumnOrder::Pnezd, "", bad, "").unwrap_err().contains("greater than 0"), "{bad}");
        }
    }

    #[test]
    fn a_file_in_feet_is_read_with_its_unit_and_a_bad_unit_is_refused() {
        let r = request_in("", SurveyFormat { order: ColumnOrder::Enzd, unit_m: UNITS[1].1 }, "", "", "").unwrap();
        assert_eq!(r.format.unit_m, 0.3048);
        assert!((UNITS[2].1 - 0.304_800_609_6).abs() < 1e-9);
        assert!(request_in("", SurveyFormat { order: ColumnOrder::Enzd, unit_m: 0.0 }, "", "", "").is_err());
    }

    #[test]
    fn a_point_file_is_imported_as_a_selected_surface() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-corridor/tests/survey/points.csv");
        let mut doc = Document::default();
        let r = request("", ColumnOrder::Pnezd, "EPSG:32736", "25", "CULV-*").unwrap();
        let (id, report) = import(&mut doc, &path, &r).unwrap();
        let e = doc.find(id).unwrap();
        assert_eq!((e.name.as_str(), e.category.as_str()), ("points", afrobim_corridor::SURFACE_CATEGORY));
        assert!(report[0].starts_with("26 point(s) read in the order PNEZD; 25 are in the surface"), "{report:?}");
        // The wrong order against a stated system is refused with the reason.
        let wrong = request("", ColumnOrder::Penzd, "EPSG:32736", "", "").unwrap();
        assert!(import(&mut doc, &path, &wrong).unwrap_err().contains("check the column order"));
    }
}
