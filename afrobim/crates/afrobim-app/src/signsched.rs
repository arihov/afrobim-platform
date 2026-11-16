//! The sign schedule in the app: SIGNSCHEDULE (CSV in), SIGNSCHEDULEOUT (the model's signs out)
//! and the blank template.
//!
//! The format, the synonyms and the plan are `afrobim_fabrication::sign_schedule`'s. What the
//! app supplies is the sign engine (it measures a sign's face, through [`AppBuilder`]) and the
//! windows: the proposed column mapping with a dropdown for every file column, then the import
//! plan (rows to create, rows that cannot be, duplicates with skip or update) and, after the
//! apply, a report. The apply is one undo step.

use std::path::{Path, PathBuf};

use afrobim_alignment::TrafficSide;
use afrobim_fabrication::sign_schedule::{
    alignments_in, apply_plan, existing_signs, export_csv, plan_import, propose_mapping, template_csv, ApplyReport, ColumnMapping, CsvStyle, CsvTable, DuplicatePolicy, Field, ImportPlan,
    LengthUnit, MappingProposal, PlanContext, RowOutcome, SignBuilder,
};
use afrobim_kernel::{Document, ElementId};
use afrobim_signs::{SignDefinition, SignEngine};
use egui::{Color32, RichText};

use crate::app::AfroBimApp;

/// The app's half of an import: the sign engine lays a face out to measure it, and the supports
/// follow a sign once it is placed.
pub struct AppBuilder<'a> {
    pub engine: &'a SignEngine,
}

impl SignBuilder for AppBuilder<'_> {
    fn measure(&mut self, definition: &SignDefinition) -> Result<(f64, f64), String> {
        let layout = self.engine.layout(definition).map_err(|e| e.to_string())?;
        let e = layout.drawing.scaled_to_origin(layout.sw_mm).extents;
        Ok((e.width(), e.height()))
    }

    fn placed(&mut self, doc: &mut Document, id: ElementId) {
        let _ = afrobim_supports::sync_support_sites(doc, id);
    }
}

/// Reads CSV text and proposes the mapping of its columns.
pub fn read_text(text: &str) -> Result<(CsvTable, MappingProposal), String> {
    let table = afrobim_fabrication::sign_schedule::parse_csv(text).map_err(|e| e.to_string())?;
    let proposal = propose_mapping(&table.headers);
    Ok((table, proposal))
}

/// The plan for a table and a mapping against the document and the engine's catalogue.
pub fn plan(doc: &Document, engine: &SignEngine, table: &CsvTable, mapping: &ColumnMapping) -> Result<ImportPlan, String> {
    let (aligns, existing) = (alignments_in(doc), existing_signs(doc));
    let ctx = PlanContext { catalog: engine.catalog(), alignments: &aligns, existing: &existing, traffic: TrafficSide::Left };
    plan_import(table, mapping, &ctx).map_err(|e| e.to_string())
}

/// Applies the plan's valid rows. The caller wraps it in one undo step.
pub fn apply(doc: &mut Document, engine: &SignEngine, plan: &ImportPlan, policy: DuplicatePolicy) -> ApplyReport {
    apply_plan(doc, plan, &mut AppBuilder { engine }, policy)
}

/// The report of an apply, a line each.
pub fn report_lines(plan: &ImportPlan, r: &ApplyReport) -> Vec<String> {
    let mut v = vec![format!("{} created, {} updated, {} skipped, {} failed ({} rows in the file).", r.created.len(), r.updated.len(), r.skipped.len(), r.failed.len(), plan.rows.len())];
    for (row, mark, why) in &r.skipped {
        v.push(format!("line {row}, {mark}: skipped: {why}"));
    }
    for (row, mark, why) in &r.failed {
        v.push(format!("line {row}, {mark}: failed: {why}"));
    }
    v
}

/// The model's signs as a schedule, and what it could not carry.
pub fn export(doc: &Document, engine: &SignEngine, style: &CsvStyle) -> afrobim_fabrication::sign_schedule::Export {
    export_csv(doc, &alignments_in(doc), engine.catalog(), style)
}

pub fn template(style: &CsvStyle) -> String {
    template_csv(style)
}

/// The windows' state.
#[derive(Default)]
pub struct SignSchedUi {
    pub mapping_open: bool,
    pub plan_open: bool,
    pub path: Option<PathBuf>,
    pub table: Option<CsvTable>,
    pub proposal: Option<MappingProposal>,
    pub mapping: ColumnMapping,
    pub plan: Option<Result<ImportPlan, String>>,
    pub policy: DuplicatePolicy,
    pub report: Vec<String>,
    pub note: String,
}

#[allow(non_snake_case)]

fn BAD() -> egui::Color32 {

    crate::theme::danger()

}
#[allow(non_snake_case)]
fn WARN() -> egui::Color32 {
    crate::theme::warn()
}
#[allow(non_snake_case)]
fn GOOD() -> egui::Color32 {
    crate::theme::success()
}

fn field_label(f: &Field) -> String {
    match f {
        Field::Param(p) => format!("param:{p}"),
        other => other.canonical_header(),
    }
}

fn unit_label(u: Option<LengthUnit>) -> &'static str {
    match u {
        None => "default unit",
        Some(LengthUnit::Millimetres) => "mm",
        Some(LengthUnit::Metres) => "m",
    }
}

impl AfroBimApp {
    /// SIGNSCHEDULE: choose a CSV and open the mapping.
    pub(crate) fn signsched_pick(&mut self) {
        if let Some(path) = rfd::FileDialog::new().add_filter("CSV or text", &["csv", "txt", "tsv"]).set_title("Sign schedule (CSV)").pick_file() {
            self.signsched_load(&path);
        }
    }

    pub(crate) fn signsched_load(&mut self, path: &Path) {
        let ui = &mut self.signsched;
        match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|t| read_text(&t)) {
            Ok((table, proposal)) => {
                ui.mapping = proposal.mapping.clone();
                ui.proposal = Some(proposal);
                ui.table = Some(table);
                ui.path = Some(path.to_path_buf());
                ui.report.clear();
                ui.note.clear();
                ui.mapping_open = true;
                self.signsched_replan();
                self.status = format!("Read {}. Check the column mapping, then the plan.", path.display());
            }
            Err(e) => self.status = format!("Could not read {}: {e}", path.display()),
        }
    }

    pub(crate) fn signsched_replan(&mut self) {
        let Some(engine) = self.engine.clone() else {
            self.signsched.plan = Some(Err(self.module_status.clone()));
            return;
        };
        if let Some(table) = &self.signsched.table {
            let result = plan(&self.doc, &engine, table, &self.signsched.mapping);
            self.signsched.plan_open = true;
            self.signsched.plan = Some(result);
        }
    }

    /// SIGNSCHEDULEOUT.
    pub(crate) fn signsched_out(&mut self) {
        let Some(engine) = self.engine.clone() else {
            self.status = self.module_status.clone();
            return;
        };
        if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("sign-schedule.csv").save_file() {
            if path.extension().is_none() {
                path.set_extension("csv");
            }
            let ex = export(&self.doc, &engine, &CsvStyle::default());
            match std::fs::write(&path, &ex.csv) {
                Ok(()) => {
                    self.status = format!("Wrote {} sign(s) to {}.", ex.signs, path.display());
                    for w in ex.warnings.iter().take(8) {
                        self.log(format!("Sign schedule: {w}"));
                    }
                }
                Err(e) => self.status = format!("Could not write {}: {e}", path.display()),
            }
        }
    }

    /// The blank template.
    pub(crate) fn signsched_template(&mut self) {
        if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("sign-schedule-template.csv").save_file() {
            if path.extension().is_none() {
                path.set_extension("csv");
            }
            match std::fs::write(&path, template(&CsvStyle::default())) {
                Ok(()) => self.status = format!("Wrote the blank sign schedule to {}. Its example line starts with # so an import skips it.", path.display()),
                Err(e) => self.status = format!("Could not write {}: {e}", path.display()),
            }
        }
    }

    pub(crate) fn signsched_apply(&mut self, ctx: &egui::Context) {
        let Some(engine) = self.engine.clone() else { return };
        let Some(Ok(plan)) = self.signsched.plan.take() else { return };
        // One undo step: what came before is committed first, then the whole import.
        self.flush_commit();
        let report = apply(&mut self.doc, &engine, &plan, self.signsched.policy);
        let lines = report_lines(&plan, &report);
        self.status = lines[0].clone();
        self.signsched.report = lines;
        self.signsched.plan = Some(Ok(plan));
        self.sync_generated();
        self.schedule_commit(ctx);
        self.signsched_replan();
    }

    pub(crate) fn signsched_windows(&mut self, ctx: &egui::Context) {
        if self.signsched.mapping_open {
            self.signsched_mapping_window(ctx);
        }
        if self.signsched.plan_open {
            self.signsched_plan_window(ctx);
        }
    }

    fn signsched_mapping_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let (mut changed, mut load_mapping, mut save_mapping, mut other_file) = (false, false, false, false);
        let work = crate::win::work_rect(ctx);
        let dlg = crate::ui_kit::Dialog::titled("Sign schedule: columns").subtitle("Match the columns of the file to the schedule fields").width(620.0).default_pos(crate::win::first_position(work)).show_body(ctx, |ui| {
            let ui_state = &mut self.signsched;
            ui.label(RichText::new(ui_state.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default()).small().weak());
            let Some(table) = ui_state.table.clone() else {
                ui.label("No file chosen.");
                return;
            };
            ui.label(format!("{} columns, {} data rows, delimiter '{}'.", table.headers.len(), table.rows.len(), if table.delimiter == '\t' { "tab".to_string() } else { table.delimiter.to_string() }));
            crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "Each column of the file is matched to a field of the schedule by its header (synonyms such as Chainage for Station). Correct any that are wrong.");
            ui.add_space(4.0);
            let fields: Vec<Field> = {
                let mut v: Vec<Field> = Field::FIXED.to_vec();
                v.extend(afrobim_fabrication::sign_schedule::PARAMS.iter().map(|p| Field::Param(p.name.to_string())));
                v
            };
            egui::ScrollArea::vertical().max_height(130.0).show(ui, |ui| {
                egui::Grid::new("sched-map").num_columns(4).spacing([10.0, 4.0]).striped(true).show(ui, |ui| {
                    crate::ui_kit::heading(ui, "File column");
                    crate::ui_kit::heading(ui, "Schedule field");
                    crate::ui_kit::heading(ui, "Unit");
                    crate::ui_kit::heading(ui, "First value");
                    ui.end_row();
                    for (i, h) in table.headers.iter().enumerate() {
                        ui.label(h);
                        let current = ui_state.mapping.entry_for(h).cloned();
                        let shown = current.as_ref().map(|c| field_label(&c.field)).unwrap_or_else(|| "not read".to_string());
                        let mut pick: Option<Option<Field>> = None;
                        egui::ComboBox::from_id_salt(("sched-field", i)).selected_text(shown).width(190.0).show_ui(ui, |ui| {
                            if ui.selectable_label(current.is_none(), "not read").clicked() {
                                pick = Some(None);
                            }
                            for f in &fields {
                                let required = Field::REQUIRED.contains(f);
                                let label = format!("{}{}", field_label(f), if required { " (required)" } else { "" });
                                if ui.selectable_label(current.as_ref().is_some_and(|c| &c.field == f), label).clicked() {
                                    pick = Some(Some(f.clone()));
                                }
                            }
                        });
                        match pick {
                            Some(None) => {
                                ui_state.mapping.columns.retain(|c| !c.header.trim().eq_ignore_ascii_case(h.trim()));
                                changed = true;
                            }
                            Some(Some(f)) => {
                                let unit = current.as_ref().and_then(|c| c.unit);
                                ui_state.mapping.set(h, f, unit);
                                changed = true;
                            }
                            None => {}
                        }
                        match current.as_ref().filter(|c| c.field.default_unit().is_some()) {
                            Some(c) => {
                                let mut unit = c.unit;
                                egui::ComboBox::from_id_salt(("sched-unit", i)).selected_text(unit_label(unit)).width(100.0).show_ui(ui, |ui| {
                                    ui.selectable_value(&mut unit, None, "default unit");
                                    ui.selectable_value(&mut unit, Some(LengthUnit::Metres), "m");
                                    ui.selectable_value(&mut unit, Some(LengthUnit::Millimetres), "mm");
                                });
                                if unit != c.unit {
                                    ui_state.mapping.set(h, c.field.clone(), unit);
                                    changed = true;
                                }
                            }
                            None => {
                                ui.label("");
                            }
                        }
                        ui.label(RichText::new(table.rows.first().and_then(|r| r.cells.get(i)).cloned().unwrap_or_default()).small().weak());
                        ui.end_row();
                    }
                });
            });
            if let Some(p) = &ui_state.proposal {
                let missing: Vec<String> = Field::REQUIRED.iter().filter(|f| !ui_state.mapping.has(f)).map(field_label).collect();
                if missing.is_empty() {
                    ui.label(RichText::new("Every required field is mapped.").color(GOOD()).small());
                } else {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, format!("Required fields not mapped: {}", missing.join(", ")));
                }
                if !p.unmapped_headers.is_empty() {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, format!("Not matched by the synonyms (and so not read unless you map them): {}", p.unmapped_headers.join(", ")));
                }
                if !p.duplicate_headers.is_empty() {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, format!("Headers that repeat a field another column took: {}", p.duplicate_headers.iter().map(|(h, f)| format!("{h} ({})", field_label(f))).collect::<Vec<_>>().join(", ")));
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Save mapping...").clicked() {
                    save_mapping = true;
                }
                if ui.button("Load mapping...").clicked() {
                    load_mapping = true;
                }
                if ui.button("Other file...").clicked() {
                    other_file = true;
                }
            });
            if !ui_state.note.is_empty() {
                ui.label(RichText::new(&ui_state.note).small());
            }
        });
        if dlg.closed {
            open = false;
        }
        if save_mapping {
            if let Some(path) = rfd::FileDialog::new().add_filter("Column mapping (JSON)", &["json"]).set_file_name("sign-schedule-mapping.json").save_file() {
                self.signsched.note = match std::fs::write(&path, self.signsched.mapping.to_json()) {
                    Ok(()) => format!("Mapping saved to {}.", path.display()),
                    Err(e) => format!("Could not save: {e}"),
                };
            }
        }
        if load_mapping {
            if let Some(path) = rfd::FileDialog::new().add_filter("Column mapping (JSON)", &["json"]).pick_file() {
                match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| ColumnMapping::from_json(&t)) {
                    Ok(m) => {
                        self.signsched.mapping = m;
                        self.signsched.note = format!("Mapping loaded from {}.", path.display());
                        changed = true;
                    }
                    Err(e) => self.signsched.note = e,
                }
            }
        }
        if other_file {
            self.signsched_pick();
        }
        if changed {
            self.signsched_replan();
        }
        self.signsched.mapping_open = open;
    }

    fn signsched_plan_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let (mut apply_now, mut replan) = (false, false);
        let work = crate::win::work_rect(ctx);
        let dlg = crate::ui_kit::Dialog::titled("Sign schedule: import plan").subtitle("What the import will add and change").width(700.0).default_pos(crate::win::first_position(work) + egui::vec2(0.0, 392.0)).show_body(ctx, |ui| {
            let st = &mut self.signsched;
            let Some(plan) = &st.plan else {
                ui.label("No plan yet: choose a file with SIGNSCHEDULE.");
                return;
            };
            let plan = match plan {
                Ok(p) => p,
                Err(e) => {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, e);
                    return;
                }
            };
            let dups = plan.duplicates().count();
            ui.horizontal_wrapped(|ui| {
                crate::ui_kit::heading(ui, &format!("{} rows to create or update", plan.valid_count()));
                ui.label(RichText::new(format!("{} rows with problems", plan.invalid_count())).color(if plan.invalid_count() > 0 { BAD() } else { Color32::GRAY }).strong());
                ui.label(RichText::new(format!("{dups} duplicate marks")).color(if dups > 0 { WARN() } else { Color32::GRAY }).strong());
            });
            if dups > 0 {
                ui.horizontal(|ui| {
                    ui.label("Duplicate marks:");
                    ui.radio_value(&mut st.policy, DuplicatePolicy::Skip, "skip (keep the existing sign)");
                    ui.radio_value(&mut st.policy, DuplicatePolicy::Update, "update (replace its definition and site placement)");
                });
            }
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(110.0).show(ui, |ui| {
                egui::Grid::new("sched-plan").num_columns(5).spacing([10.0, 3.0]).striped(true).show(ui, |ui| {
                    for h in ["Line", "Mark", "What", "Where", "Result"] {
                        crate::ui_kit::th(ui, &h);
                    }
                    ui.end_row();
                    for r in &plan.rows {
                        ui.label(r.row.to_string());
                        ui.label(&r.mark);
                        match &r.outcome {
                            RowOutcome::Valid(p) => {
                                ui.label(&p.catalog_name);
                                ui.label(format!("{} {}, {:.2} m {}", p.placement.alignment, afrobim_alignment::format_station(p.placement.station_m, 3), p.placement.offset_m, if matches!(p.placement.side, afrobim_alignment::SignSide::Right) { "right" } else { "left" }));
                                match p.existing {
                                    Some(_) => ui.label(RichText::new(if st.policy == DuplicatePolicy::Update { "duplicate: will update" } else { "duplicate: will skip" }).color(WARN())),
                                    None => ui.label(RichText::new("will create").color(GOOD())),
                                };
                            }
                            RowOutcome::Invalid(issues) => {
                                ui.label("");
                                ui.label("");
                                crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, issues.iter().map(|i| i.message.as_str()).collect::<Vec<_>>().join("; "));
                            }
                        }
                        ui.end_row();
                    }
                });
            });
            if !plan.unread_headers.is_empty() {
                ui.label(RichText::new(format!("Columns not read: {}", plan.unread_headers.join(", "))).small().weak());
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(plan.valid_count() > 0, egui::Button::new(format!("Apply {} row(s)", plan.valid_count()))).on_hover_text("One undo step").clicked() {
                    apply_now = true;
                }
                if ui.button("Plan again").clicked() {
                    replan = true;
                }
            });
            if !st.report.is_empty() {
                ui.separator();
                crate::ui_kit::heading(ui, "Report");
                for l in &st.report {
                    ui.label(RichText::new(l).small());
                }
            }
        });
        if dlg.closed {
            open = false;
        }
        if apply_now {
            self.signsched_apply(ctx);
        }
        if replan {
            self.signsched_replan();
        }
        self.signsched.plan_open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_kernel::Pt;

    fn engine() -> Option<std::sync::Arc<SignEngine>> {
        crate::signs_glue::load_module(crate::signs_glue::DEFAULT_STANDARD).ok().map(|l| l.engine)
    }

    fn doc_with_alignment() -> Document {
        let mut doc = Document::default();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/corridor.xml");
        crate::alignment_glue::import_landxml(&mut doc, &path).unwrap();
        let _ = Pt::new(0.0, 0.0);
        doc
    }

    #[test]
    fn export_then_import_through_the_glue_recreates_the_signs_and_a_foreign_header_file_maps_by_synonyms() {
        let engine = engine().expect("the rule pack and fonts are found from the source tree");
        let mut doc = doc_with_alignment();
        let align_name = crate::alignment_glue::alignments(&doc)[0].1.clone();
        let stack = engine.catalog().iter().find(|e| e.kind == afrobim_signs::SignKind::StackType).unwrap().key.clone();
        // A schedule with foreign headers (semicolons, Chainage for Station, L/R for Side).
        let foreign = format!(
            "Sign No.;Diagram;Road Name;Chainage;L/R;Offset (m);Remarks;Legend Text;Contractor\n\
             ADS-1;{stack};{align_name};1+100;R;3.5;first;Kasese@Ahead;ACME\n\
             ADS-2;{stack};{align_name};1+200;L;4;second;Masaka@Left;ACME\n\
             BAD()-1;NOPE;{align_name};1+300;L;4;a row with an unknown code;Kasese@Ahead;ACME\n"
        );
        let (table, proposal) = read_text(&foreign).unwrap();
        assert!(proposal.missing_required.is_empty(), "{:?}", proposal.missing_required);
        assert_eq!(proposal.unmapped_headers, ["Contractor"]);
        let p = plan(&doc, &engine, &table, &proposal.mapping).unwrap();
        assert_eq!((p.valid_count(), p.invalid_count()), (2, 1), "{:?}", p.error_lines());
        assert!(p.error_lines()[0].starts_with("row 4"), "{:?}", p.error_lines());
        let r = apply(&mut doc, &engine, &p, DuplicatePolicy::Skip);
        assert_eq!((r.created.len(), r.skipped.len(), r.failed.len()), (2, 1, 0), "{:?}", report_lines(&p, &r));
        let signs = || doc.elements().iter().filter(|e| e.category == "sign").map(|e| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(signs(), ["ADS-1", "ADS-2"]);
        let ads1 = doc.elements().iter().find(|e| e.name == "ADS-1").unwrap();
        let site = ads1.site.as_ref().expect("placed on the alignment");
        assert_eq!(site.alignment.as_deref(), Some(align_name.as_str()));
        assert!(ads1.width > 100.0, "the sign engine measured the face");
        // Export, import into a fresh document: the same signs come back.
        let ex = export(&doc, &engine, &CsvStyle::default());
        assert_eq!(ex.signs, 2);
        let mut fresh = doc_with_alignment();
        let (table2, proposal2) = read_text(&ex.csv).unwrap();
        let p2 = plan(&fresh, &engine, &table2, &proposal2.mapping).unwrap();
        assert_eq!(p2.valid_count(), 2, "{:?}", p2.error_lines());
        let r2 = apply(&mut fresh, &engine, &p2, DuplicatePolicy::Skip);
        assert_eq!(r2.created.len(), 2);
        let back = fresh.elements().iter().find(|e| e.name == "ADS-1").unwrap();
        let (a, b) = (site, back.site.as_ref().unwrap());
        assert!((a.x - b.x).abs() < 1.0 && (a.y - b.y).abs() < 1.0 && (a.rotation_deg - b.rotation_deg).abs() < 1e-6, "{a:?} {b:?}");
        // Applying again to the same document: duplicates skip, or update when asked.
        let again = plan(&fresh, &engine, &table2, &proposal2.mapping).unwrap();
        assert_eq!(again.duplicates().count(), 2);
        assert_eq!(apply(&mut fresh, &engine, &again, DuplicatePolicy::Skip).skipped.len(), 2);
        assert_eq!(apply(&mut fresh, &engine, &again, DuplicatePolicy::Update).updated.len(), 2);
        // The blank template is a header and a commented example.
        let (blank, _) = read_text(&template(&CsvStyle::default())).unwrap();
        assert!(blank.rows.is_empty() && !blank.comment_lines.is_empty(), "the template has a header and a commented example, no data rows");
    }
}
