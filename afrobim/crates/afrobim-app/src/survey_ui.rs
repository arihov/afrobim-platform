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

const CRS_HINT: &str = "for example WGS 84 / UTM zone 36S, or EPSG:32736";

/// The request of the dialog or the command line, from what was typed.
///
/// `crs` blank is no coordinate system; `max_side` blank is no limit. `exclude` is a list of
/// descriptions separated by commas.
pub fn request(name: &str, order: ColumnOrder, crs: &str, max_side: &str, exclude: &str) -> Result<SurveyImportRequest, String> {
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
    Ok(SurveyImportRequest { name: name.trim().to_string(), format: SurveyFormat::metres(order), options: SurfaceOptions { exclude, max_side_m }, crs })
}

/// Imports the file and returns the surface and the account of the import.
pub fn import(doc: &mut Document, path: &Path, request: &SurveyImportRequest) -> Result<(ElementId, Vec<String>), String> {
    afrobim_corridor::import_survey_surface(doc, path, request)
}

/// The open dialog.
pub struct SurveyForm {
    path: PathBuf,
    name: String,
    order: ColumnOrder,
    crs: String,
    max_side: String,
    /// Descriptions ticked to be left out, in capitals.
    excluded: BTreeSet<String>,
    /// The file as read in `order`, or why it cannot be.
    read: Result<SurveyPoints, String>,
    problem: Option<String>,
}

impl SurveyForm {
    fn new(path: PathBuf) -> SurveyForm {
        let order = ColumnOrder::Pnezd;
        SurveyForm {
            name: path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            read: read_survey_points(&path, &SurveyFormat::metres(order)).map_err(|e| e.to_string()),
            path,
            order,
            crs: String::new(),
            max_side: String::new(),
            excluded: BTreeSet::new(),
            problem: None,
        }
    }

    fn read_again(&mut self) {
        self.read = read_survey_points(&self.path, &SurveyFormat::metres(self.order)).map_err(|e| e.to_string());
        self.problem = None;
    }

    fn request(&self) -> Result<SurveyImportRequest, String> {
        let exclude: Vec<&str> = self.excluded.iter().map(String::as_str).collect();
        let mut r = request(&self.name, self.order, &self.crs, &self.max_side, &exclude.join(","))?;
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
        self.survey.form = Some(SurveyForm::new(path));
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
        let mut open = true;
        let (mut go, mut cancel) = (false, false);
        let title = format!("Survey points: {}", form.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
        crate::win::new(ctx, title).id(egui::Id::new("survey-points")).open(&mut open).default_width(560.0).show(ctx, |ui| {
            ui.label(RichText::new("A ground surface is made from the points by triangulation. Coordinates and levels are read as metres.").small().weak());
            ui.add_space(4.0);
            egui::Grid::new("survey-form").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Surface name");
                ui.add(egui::TextEdit::singleline(&mut form.name).desired_width(260.0));
                ui.end_row();

                ui.label("Columns");
                let before = form.order;
                egui::ComboBox::from_id_salt("survey-order").selected_text(format!("{}: {}", form.order.code(), form.order.label())).show_ui(ui, |ui| {
                    for order in ColumnOrder::ALL {
                        ui.selectable_value(&mut form.order, order, format!("{}: {}", order.code(), order.label()));
                    }
                });
                if form.order != before {
                    form.read_again();
                }
                ui.end_row();

                ui.label("Coordinate system");
                ui.add(egui::TextEdit::singleline(&mut form.crs).hint_text(CRS_HINT).desired_width(260.0));
                ui.end_row();

                ui.label("Longest triangle side (m)");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut form.max_side).hint_text("blank: no limit").desired_width(90.0));
                    ui.label(RichText::new("Triangles with a longer side are removed, so the surface does not span ground that was not surveyed.").small().weak());
                });
                ui.end_row();
            });

            ui.add_space(6.0);
            match &form.read {
                Err(why) => {
                    ui.colored_label(crate::theme::warn(), format!("The file cannot be read in this column order: {why}"));
                }
                Ok(survey) => {
                    ui.label(RichText::new(format!("{} points. The first as they are read:", survey.points.len())).strong());
                    for p in survey.points.iter().take(3) {
                        ui.label(
                            RichText::new(format!(
                                "{}  easting {:.3}  northing {:.3}  level {:.3}  {}",
                                if p.id.is_empty() { format!("line {}", p.line) } else { p.id.clone() },
                                p.easting,
                                p.northing,
                                p.level,
                                p.code
                            ))
                            .monospace(),
                        );
                    }
                    for note in &survey.notes {
                        ui.label(RichText::new(note).small().weak());
                    }
                    // The coordinate system, as understood, and whether the coordinates can be in it.
                    if !form.crs.trim().is_empty() {
                        match CoordinateSystem::parse(&form.crs) {
                            None => {
                                ui.colored_label(crate::theme::warn(), "That coordinate system is not known. The WGS 84 / UTM zones are, with N or S.");
                            }
                            Some(crs) => {
                                let extent = survey.plan_extent();
                                let doubts = extent.map(|(min, max)| crs.doubts(min, max)).unwrap_or_default();
                                if doubts.is_empty() {
                                    let centre = extent.and_then(|(min, max)| crs.to_geographic((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0));
                                    let place = centre.map(|(lat, lon)| format!(" The middle of the points is at latitude {lat:.5}, longitude {lon:.5}.")).unwrap_or_default();
                                    ui.label(RichText::new(format!("{}.{place}", crs.label())).small());
                                } else {
                                    for doubt in doubts {
                                        ui.colored_label(crate::theme::warn(), doubt);
                                    }
                                }
                            }
                        }
                    }

                    ui.add_space(6.0);
                    let codes = survey.codes();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Descriptions ({})", codes.len())).strong());
                        ui.label(RichText::new("Tick those that are not ground levels (inverts, tops of walls) to leave their points out.").small().weak());
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button("Untick all").clicked() {
                            form.excluded.clear();
                        }
                        let out: usize = codes.iter().filter(|(c, _)| form.excluded.contains(&c.to_ascii_uppercase())).map(|(_, n)| n).sum();
                        ui.label(RichText::new(format!("{out} point(s) left out.")).small());
                    });
                    egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
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
                }
            }

            if let Some(problem) = &form.problem {
                ui.add_space(4.0);
                ui.colored_label(crate::theme::danger(), problem);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(form.read.is_ok(), egui::Button::new("Make the surface")).clicked() {
                    go = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if go {
            match form.request().and_then(|request| self.survey_import(Some(ctx), &form.path, &request)) {
                Ok(_) => return,
                Err(why) => form.problem = Some(why),
            }
        }
        if open && !cancel {
            self.survey.form = Some(form);
        }
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
        assert_eq!(r.crs.as_ref().map(|c| c.epsg), Some(32736));
        assert_eq!(r.options, SurfaceOptions { exclude: vec!["CULV-INV".into(), "clvt-*".into()], max_side_m: Some(40.0) });
        // Blank is none, for both.
        let r = request("", ColumnOrder::Enzd, "  ", "", "").unwrap();
        assert!(r.crs.is_none() && r.options.max_side_m.is_none() && r.options.exclude.is_empty());
        // What cannot be understood is said, never guessed.
        assert!(request("", ColumnOrder::Pnezd, "UTM 36", "", "").unwrap_err().contains("with N or S"));
        assert!(request("", ColumnOrder::Pnezd, "Arc 1960 / UTM zone 36S", "", "").unwrap_err().contains("not a coordinate system AfroBIM knows"));
        for bad in ["0", "-5", "ten", "inf"] {
            assert!(request("", ColumnOrder::Pnezd, "", bad, "").unwrap_err().contains("greater than 0"), "{bad}");
        }
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
