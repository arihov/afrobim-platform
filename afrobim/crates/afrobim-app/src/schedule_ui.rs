//! The 4D tab's windows: the activity table with logic, the calendar, the CPM results, the Gantt
//! chart, baseline and variance, progress, element links and their report, the 4D date control and
//! the MS Project exchange. The programme itself is a document element (`schedule_glue`).

use std::collections::HashMap;

use afrobim_draw::{Colour, Primitive};
use afrobim_kernel::{ElementId, SelectMode};
use afrobim_schedule::{
    activity_table_csv, element_states, export_mspdi, import_mspdi, link_report, outline, s_curve, s_curve_csv, schedule_with, variance_csv, OutlineKind, ScheduleOptions, Activity, ActivityLink, Constraint, Date, Dependency,
    ElementRule, ElementSelector, ElementStates, GanttDrawing, GanttLabelKind, LinkAction, LinkType, MilestoneKind, Phase, Programme, SCurveBasis, Schedule, ScheduleError, Weekday,
};
use egui::{pos2, Align2, Color32, FontId, RichText, Sense, Shape, Stroke, Ui};

use crate::app::AfroBimApp;
use crate::schedule_glue as glue;

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

/// How the 3D view draws an element on the 4D date.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FourD {
    Normal,
    Ghost,
    Hidden,
    Active,
}

#[derive(Default)]
struct NewProgramme {
    name: String,
    start: String,
    calendar: String,
    days: [bool; 7],
}

#[derive(Default)]
struct LinkForm {
    activity: String,
    action: Option<LinkAction>,
    category: String,
    layer: String,
    pattern: String,
}

pub struct SchedUi {
    pub prog: Option<Programme>,
    pub id: Option<ElementId>,
    loaded: serde_json::Value,
    /// The last CPM run: the schedule, or the problems in plain words.
    pub result: Option<Result<Schedule, Vec<String>>>,
    gantt: Option<GanttDrawing>,
    pub states: Option<ElementStates>,
    states_for: Option<(Date, usize)>,
    pub fourd_on: bool,
    pub hide_not_started: bool,
    pub date: Option<Date>,
    pub table_open: bool,
    pub calendar_open: bool,
    pub schedule_open: bool,
    pub gantt_open: bool,
    pub variance_open: bool,
    pub progress_open: bool,
    pub link_open: bool,
    pub report_open: bool,
    pub date_open: bool,
    pub baseline_open: bool,
    pub import_report: Option<Vec<String>>,
    new_prog: NewProgramme,
    new_act: (String, String, String, String),
    new_dep: (String, String, LinkType, String),
    holiday: String,
    calendar_pick: usize,
    new_calendar: String,
    cons: HashMap<String, Vec<(u8, String)>>,
    /// The second round of 4D tools: WBS, critical basis, time-location chart, playback (`schedule_ext`).
    pub ext: crate::schedule_ext::SchedExt,
    prog_text: HashMap<(String, u8), String>,
    start_text: String,
    data_date_text: String,
    required_text: String,
    baseline_name: String,
    link_form: LinkForm,
    gantt_scale: f32,
    problem: String,
}

impl Default for SchedUi {
    fn default() -> Self {
        SchedUi {
            prog: None,
            id: None,
            loaded: serde_json::Value::Null,
            result: None,
            gantt: None,
            states: None,
            states_for: None,
            fourd_on: false,
            hide_not_started: false,
            date: None,
            table_open: false,
            calendar_open: false,
            schedule_open: false,
            gantt_open: false,
            variance_open: false,
            progress_open: false,
            link_open: false,
            report_open: false,
            date_open: false,
            baseline_open: false,
            import_report: None,
            new_prog: NewProgramme::default(),
            new_act: Default::default(),
            new_dep: (String::new(), String::new(), LinkType::FS, String::new()),
            holiday: String::new(),
            calendar_pick: 0,
            new_calendar: String::new(),
            cons: HashMap::new(),
            ext: Default::default(),
            prog_text: HashMap::new(),
            start_text: String::new(),
            data_date_text: String::new(),
            required_text: String::new(),
            baseline_name: String::new(),
            link_form: LinkForm::default(),
            gantt_scale: 3.5,
            problem: String::new(),
        }
    }
}

fn date_of(text: &str) -> Option<Date> {
    let t = text.trim();
    if t.len() == 10 {
        Date::parse(t)
    } else {
        None
    }
}

fn messages(e: ScheduleError) -> Vec<String> {
    match e {
        ScheduleError::Invalid(issues) => issues.into_iter().map(|i| i.message).collect(),
        ScheduleError::Other(m) => vec![m],
    }
}

impl SchedUi {
    /// Reads the document's programme when its data changed (open, undo, import).
    pub fn sync(&mut self, doc: &afrobim_kernel::Document) {
        let current = doc.elements().iter().find(|e| e.category == afrobim_schedule::SCHEDULE_CATEGORY).map(|e| e.data.clone()).unwrap_or(serde_json::Value::Null);
        let count = doc.elements().len();
        if current != self.loaded {
            match glue::load(doc) {
                Some((id, p)) => {
                    if !self.ext.settings_loaded {
                        self.ext.settings_loaded = true;
                        self.ext.critical = glue::critical_basis_of(doc);
                        self.ext.rollup = glue::rollup_basis_of(doc);
                        self.ext.tl_alignment = glue::setting(doc, glue::KEY_TL_ALIGNMENT);
                    }
                    self.id = Some(id);
                    self.start_text = p.start.to_string();
                    self.data_date_text = p.data_date.map(|d| d.to_string()).unwrap_or_default();
                    self.required_text = p.required_finish.map(|d| d.to_string()).unwrap_or_default();
                    self.prog = Some(p);
                }
                None => {
                    self.id = None;
                    self.prog = None;
                }
            }
            self.cons.clear();
            self.prog_text.clear();
            self.loaded = current;
            self.recompute(doc);
        } else if self.states_for.is_some_and(|(_, n)| n != count) {
            self.refresh_states(doc);
        }
    }

    /// Runs the critical path method and redraws the Gantt chart.
    pub fn recompute(&mut self, doc: &afrobim_kernel::Document) {
        self.gantt = None;
        self.ext.rev += 1;
        let options = ScheduleOptions { critical: self.ext.critical };
        self.result = self.prog.as_ref().map(|p| schedule_with(p, &options).map_err(messages));
        self.rebuild_gantt();
        if let (Some(_), Some(Ok(s))) = (&self.prog, &self.result) {
            if self.date.is_none() {
                self.date = Some(s.project_start);
            }
        }
        self.states_for = None;
        self.refresh_states(doc);
    }

    /// Draws the Gantt chart again: with summary rows and the roll-up when the programme has them,
    /// without the activities under a collapsed summary task.
    pub fn rebuild_gantt(&mut self) {
        self.gantt = None;
        self.ext.rollup_error = None;
        if let (Some(p), Some(Ok(s))) = (&self.prog, &self.result) {
            let (g, err) = crate::schedule_ext::build_gantt(p, s, self.ext.rollup, &self.ext.collapsed);
            self.ext.rollup_error = err;
            self.gantt = Some(g);
        }
    }

    pub fn refresh_states(&mut self, doc: &afrobim_kernel::Document) {
        self.states = None;
        self.states_for = None;
        if let (Some(p), Some(Ok(s)), Some(date)) = (&self.prog, &self.result, self.date) {
            self.states = Some(element_states(p, s, doc, date));
            self.states_for = Some((date, doc.elements().len()));
        }
    }

    /// Puts a programme made elsewhere (the roll-up of site progress) in place of the edited one,
    /// stores it in the document and reschedules.
    pub fn replace_programme(&mut self, doc: &mut afrobim_kernel::Document, p: Programme) -> Result<(), String> {
        self.start_text = p.start.to_string();
        self.data_date_text = p.data_date.map(|d| d.to_string()).unwrap_or_default();
        self.required_text = p.required_finish.map(|d| d.to_string()).unwrap_or_default();
        self.prog = Some(p);
        self.cons.clear();
        self.prog_text.clear();
        self.commit(doc)
    }

    /// Writes the edited programme back into the document and reschedules. True when stored.
    pub fn commit(&mut self, doc: &mut afrobim_kernel::Document) -> Result<(), String> {
        let Some(p) = self.prog.clone() else { return Ok(()) };
        let id = glue::store(doc, self.id, &p)?;
        self.id = Some(id);
        self.loaded = doc.find(id).map(|e| e.data.clone()).unwrap_or_default();
        self.recompute(doc);
        Ok(())
    }

    /// How the 3D view should draw an element at the chosen date.
    pub fn fourd_style(&self, id: ElementId) -> FourD {
        if !self.fourd_on {
            return FourD::Normal;
        }
        let Some(st) = self.states.as_ref().and_then(|s| s.get(id)) else { return FourD::Normal };
        if !st.present {
            return match st.phase {
                Phase::NotStarted if !self.hide_not_started => FourD::Ghost,
                _ => FourD::Hidden,
            };
        }
        match st.phase {
            Phase::InProgress { .. } => FourD::Active,
            _ => FourD::Normal,
        }
    }

    pub fn set_date(&mut self, doc: &afrobim_kernel::Document, d: Date) {
        self.date = Some(d);
        self.refresh_states(doc);
    }

    /// The first and last day the date control offers: the programme's span with a week either side.
    fn span(&self) -> Option<(Date, Date)> {
        match &self.result {
            Some(Ok(s)) => Some((s.project_start.add_days(-7), s.project_finish.add_days(7))),
            _ => None,
        }
    }
}

fn rgb(c: Option<Colour>) -> Color32 {
    let [r, g, b] = c.map(|c| c.display_rgb()).unwrap_or([0x10, 0x10, 0x10]);
    Color32::from_rgb(r, g, b)
}

/// Paints the Gantt chart and its labels on a light sheet (the chart's black lines need one).
fn paint_gantt(ui: &mut Ui, g: &GanttDrawing, scale: f32) {
    let ext = g.drawing.extents;
    let (w, h) = (ext.width() as f32 * scale, ext.height() as f32 * scale);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w + 24.0, h + 24.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(0xF6, 0xF5, 0xF0));
    let map = |x: f64, y: f64| pos2(rect.left() + 12.0 + (x - ext.min.x) as f32 * scale, rect.bottom() - 12.0 - (y - ext.min.y) as f32 * scale);
    for prim in &g.drawing.primitives {
        match prim {
            Primitive::Fill { outline, colour } => {
                let pts: Vec<_> = outline.iter().map(|p| map(p.x, p.y)).collect();
                if pts.len() >= 3 {
                    painter.add(Shape::convex_polygon(pts, rgb(Some(*colour)), Stroke::NONE));
                }
            }
            Primitive::Line { from, to, width, colour } => {
                painter.line_segment([map(from.x, from.y), map(to.x, to.y)], Stroke::new((*width as f32 * scale).max(1.0), rgb(*colour)));
            }
            _ => {}
        }
    }
    for l in &g.labels {
        let colour = match l.kind {
            GanttLabelKind::DataDate => Color32::from_rgb(0x1F, 0x7A, 0x3A),
            GanttLabelKind::RequiredFinish => Color32::from_rgb(0x6B, 0x3A, 0x1E),
            _ => Color32::from_rgb(0x22, 0x22, 0x22),
        };
        let size = match l.kind {
            GanttLabelKind::Title => 13.0,
            GanttLabelKind::Week => 9.0,
            _ => 11.0,
        };
        painter.text(map(l.at.x, l.at.y), Align2::LEFT_CENTER, &l.text, FontId::proportional(size), colour);
    }
}

fn weekday_boxes(ui: &mut Ui, days: &mut [bool; 7]) {
    ui.horizontal(|ui| {
        for (i, w) in Weekday::ALL.iter().enumerate() {
            ui.checkbox(&mut days[i], glue::weekday_label(*w));
        }
    });
}

fn kind_label(k: LinkType) -> &'static str {
    match k {
        LinkType::FS => "FS",
        LinkType::SS => "SS",
        LinkType::FF => "FF",
        LinkType::SF => "SF",
    }
}

fn action_label(a: LinkAction) -> &'static str {
    match a {
        LinkAction::Construct => "construct",
        LinkAction::Demolish => "demolish",
        LinkAction::Temporary => "temporary",
    }
}

fn selector_text(doc: &afrobim_kernel::Document, s: &ElementSelector) -> String {
    match s {
        ElementSelector::Id(id) => format!("element {}", doc.find(*id).map(|e| e.name.clone()).unwrap_or_else(|| format!("{} (missing)", id.0))),
        ElementSelector::Rule(r) => {
            let mut parts = Vec::new();
            if let Some(c) = &r.category {
                parts.push(format!("category {c}"));
            }
            if let Some(l) = &r.layer {
                parts.push(format!("layer {l}"));
            }
            if let Some(m) = &r.mark_pattern {
                parts.push(format!("mark {m}"));
            }
            format!("rule: {}", parts.join(", "))
        }
    }
}

impl AfroBimApp {
    pub(crate) fn sched_mark_changed(&mut self) {
        self.schedule_commit_now();
    }

    /// RUN SCHEDULE: the critical path method, with its dates, floats and findings.
    pub(crate) fn run_schedule(&mut self) {
        self.sched.sync(&self.doc);
        if self.sched.prog.is_none() {
            self.status = "There is no programme yet. Open PROGRAMME to create one.".into();
            self.sched.table_open = true;
            return;
        }
        self.sched.recompute(&self.doc);
        self.sched.schedule_open = true;
        self.status = match &self.sched.result {
            Some(Ok(s)) => format!(
                "Scheduled {} activities: {} to {}, {} critical, {} finding(s).",
                s.rows.len(),
                s.project_start,
                s.project_finish,
                s.critical().count(),
                s.issues.len()
            ),
            Some(Err(m)) => format!("The programme cannot be scheduled: {}", m.join("; ")),
            None => String::new(),
        };
    }

    pub(crate) fn import_mspdi_file(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("MS Project XML", &["xml"]).pick_file() else { return };
        self.import_mspdi_path(&path);
    }

    pub(crate) fn import_mspdi_path(&mut self, path: &std::path::Path) {
        let xml = match std::fs::read_to_string(path) {
            Ok(x) => x,
            Err(e) => {
                self.status = format!("Could not read {}: {e}", path.display());
                return;
            }
        };
        match import_mspdi(&xml) {
            Ok((mut p, report)) => {
                self.sched.sync(&self.doc);
                // Links to model elements are not in the file: keep those whose activity survives.
                if let Some(old) = &self.sched.prog {
                    p.links = old.links.iter().filter(|l| p.activity(&l.activity).is_some()).cloned().collect();
                }
                let n = p.activities.len();
                self.sched.prog = Some(p);
                let mut lines = report.warnings.clone();
                lines.insert(0, format!("Imported {n} activities from {}.", path.display()));
                if lines.len() == 1 {
                    lines.push("Nothing was left out.".into());
                }
                self.sched.import_report = Some(lines);
                self.sched.date = None;
                match self.sched.commit(&mut self.doc) {
                    Ok(()) => self.status = format!("Imported {n} activities from {}; {} item(s) the file could not carry are listed.", path.display(), report.warnings.len()),
                    Err(e) => self.status = format!("Imported but not stored: {e}"),
                }
                self.sched_mark_changed();
            }
            Err(e) => self.status = format!("Could not import {}: {e}", path.display()),
        }
    }

    pub(crate) fn export_mspdi_file(&mut self) {
        self.sched.sync(&self.doc);
        let Some(p) = self.sched.prog.clone() else {
            self.status = "There is no programme to export.".into();
            return;
        };
        let Some(mut path) = rfd::FileDialog::new().add_filter("MS Project XML", &["xml"]).set_file_name("programme.xml").save_file() else { return };
        if path.extension().is_none() {
            path.set_extension("xml");
        }
        self.export_mspdi_path(&p, &path);
    }

    pub(crate) fn export_mspdi_path(&mut self, p: &Programme, path: &std::path::Path) {
        let s = self.sched.result.as_ref().and_then(|r| r.as_ref().ok());
        let xml = export_mspdi(p, s);
        match std::fs::write(path, xml) {
            Ok(()) => self.status = format!("Exported {} activities to {}. Milestone kind, weights, element links, baseline and required finish are not carried by MS Project XML.", p.activities.len(), path.display()),
            Err(e) => self.status = format!("Could not write {}: {e}", path.display()),
        }
    }

    /// The activity table, variance and S-curve CSVs into a folder.
    pub(crate) fn export_programme_csvs(&mut self) {
        let Some(dir) = rfd::FileDialog::new().set_title("Folder for the programme reports").pick_folder() else { return };
        self.write_programme_csvs(&dir);
    }

    pub(crate) fn write_programme_csvs(&mut self, dir: &std::path::Path) {
        self.sched.sync(&self.doc);
        let (Some(p), Some(Ok(s))) = (self.sched.prog.clone(), self.sched.result.as_ref().map(|r| r.as_ref().map(|s| s.clone()).map_err(|e| e.clone()))) else {
            self.status = "There is no schedule to export. Create the programme and fix any errors first.".into();
            return;
        };
        let mut files: Vec<(&str, String)> = vec![("activities.csv", activity_table_csv(&p, &s))];
        let mut wrote = Vec::new();
        match p.variance(&s) {
            Ok(v) => files.push(("variance.csv", variance_csv(&v))),
            Err(_) => wrote.push("variance.csv skipped: no baseline".to_string()),
        }
        match s_curve(&p, &s, SCurveBasis::Duration) {
            Ok(c) => files.push(("s_curve.csv", s_curve_csv(&c))),
            Err(e) => wrote.push(format!("s_curve.csv skipped: {e}")),
        }
        for (name, text) in files {
            match std::fs::write(dir.join(name), text) {
                Ok(()) => wrote.push(name.to_string()),
                Err(e) => wrote.push(format!("{name} (not written: {e})")),
            }
        }
        self.status = format!("Programme reports in {}: {}. The S-curve weighs each activity by its working days.", dir.display(), wrote.join(", "));
    }

    pub(crate) fn schedule_windows(&mut self, ctx: &egui::Context) {
        self.sched.sync(&self.doc);
        let mut changed = false;
        if self.sched.table_open {
            changed |= self.table_window(ctx);
        }
        if self.sched.calendar_open {
            changed |= self.calendar_window(ctx);
        }
        if self.sched.schedule_open {
            self.schedule_window(ctx);
        }
        if self.sched.gantt_open {
            self.gantt_window(ctx);
        }
        if self.sched.baseline_open {
            changed |= self.baseline_window(ctx);
        }
        if self.sched.variance_open {
            self.variance_window(ctx);
        }
        if self.sched.progress_open {
            changed |= self.progress_window(ctx);
        }
        if self.sched.link_open {
            changed |= self.link_window(ctx);
        }
        if self.sched.report_open {
            self.report_window(ctx);
        }
        if self.sched.date_open {
            self.date_window(ctx);
        }
        changed |= self.schedule_ext_windows(ctx);
        if let Some(lines) = self.sched.import_report.clone() {
            let mut open = true;
            crate::win::new(ctx, "MS Project import report").open(&mut open).default_width(560.0).show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for l in &lines {
                        ui.label(l);
                    }
                });
            });
            if !open {
                self.sched.import_report = None;
            }
        }
        if changed {
            if let Err(e) = self.sched.commit(&mut self.doc) {
                self.status = format!("Programme not stored: {e}");
            }
            self.sched_mark_changed();
        }
    }

    // ─── activities and logic ────────────────────────────────────────────────────────────

    fn table_window(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        let mut changed = false;
        let s = &mut self.sched;
        crate::win::new(ctx, "Programme: activities and logic").open(&mut open).default_width(900.0).default_height(560.0).show(ctx, |ui| {
            let Some(p) = s.prog.as_mut() else {
                ui.label("There is no programme yet. Enter its name, its start date and its working weekdays. None are assumed.");
                egui::Grid::new("new-prog").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Programme name");
                    ui.add_sized([240.0, 20.0], egui::TextEdit::singleline(&mut s.new_prog.name));
                    ui.end_row();
                    ui.label("Project start date");
                    ui.add_sized([120.0, 20.0], egui::TextEdit::singleline(&mut s.new_prog.start).hint_text("YYYY-MM-DD"));
                    ui.end_row();
                    ui.label("Calendar name");
                    ui.add_sized([160.0, 20.0], egui::TextEdit::singleline(&mut s.new_prog.calendar));
                    ui.end_row();
                    ui.label("Working weekdays");
                    weekday_boxes(ui, &mut s.new_prog.days);
                    ui.end_row();
                });
                ui.label(RichText::new("Holidays are added in the calendar editor (CALENDAR). None are built in.").small().weak());
                if !s.problem.is_empty() {
                    ui.label(RichText::new(&s.problem).color(BAD()));
                }
                if ui.button("Create the programme").clicked() {
                    let days: Vec<Weekday> = Weekday::ALL.iter().enumerate().filter(|(i, _)| s.new_prog.days[*i]).map(|(_, w)| *w).collect();
                    match date_of(&s.new_prog.start).ok_or_else(|| "Enter the start date as YYYY-MM-DD.".to_string()).and_then(|d| glue::new_programme(&s.new_prog.name, d, &s.new_prog.calendar, &days)) {
                        Ok(p) => {
                            s.start_text = p.start.to_string();
                            s.prog = Some(p);
                            s.problem.clear();
                            changed = true;
                        }
                        Err(e) => s.problem = e,
                    }
                }
                return;
            };

            ui.horizontal(|ui| {
                ui.label(RichText::new(&p.name).strong());
                ui.label("start");
                if ui.add_sized([90.0, 20.0], egui::TextEdit::singleline(&mut s.start_text)).lost_focus() {
                    if let Some(d) = date_of(&s.start_text) {
                        if d != p.start {
                            p.start = d;
                            changed = true;
                        }
                    }
                }
                ui.label("required finish");
                let r = ui.add_sized([90.0, 20.0], egui::TextEdit::singleline(&mut s.required_text).hint_text("optional"));
                if r.lost_focus() {
                    let new = if s.required_text.trim().is_empty() { None } else { date_of(&s.required_text) };
                    if (s.required_text.trim().is_empty() || new.is_some()) && new != p.required_finish {
                        p.required_finish = new;
                        changed = true;
                    }
                }
            });
            if date_of(&s.start_text).is_none() {
                ui.label(RichText::new("The start date must be YYYY-MM-DD.").small().color(BAD()));
            }
            ui.separator();
            ui.label(RichText::new("Activities").strong());
            let ids: Vec<String> = p.activities.iter().map(|a| a.id.clone()).collect();
            let many_calendars = p.calendars.len() > 1;
            let has_summaries = !p.summaries.is_empty();
            let calendar_names: Vec<String> = p.calendars.iter().map(|c| c.name.clone()).collect();
            let summary_ids: Vec<(String, String)> = p.summaries.iter().map(|x| (x.id.clone(), format!("{} {}", x.wbs, x.name))).collect();
            // Programme order, or the outline (summary tasks with what is under them) when there are summary tasks.
            let order: Vec<(OutlineKind, usize)> = if has_summaries { outline(p).into_iter().map(|o| (o.kind, o.level)).collect() } else { (0..p.activities.len()).map(|i| (OutlineKind::Activity(i), 1)).collect() };
            let mut delete: Option<usize> = None;
            egui::ScrollArea::both().id_salt("acts").max_height(280.0).auto_shrink([false, true]).show(ui, |ui| {
                let mut heads = vec!["Id", "WBS", "Name", "Days", "Milestone", "Constraints"];
                if has_summaries {
                    heads.push("Under");
                }
                if many_calendars {
                    heads.push("Calendar");
                }
                heads.push("");
                egui::Grid::new("activities").num_columns(heads.len()).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                    let columns = heads.len();
                    for h in heads {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (kind, level) in order {
                        let i = match kind {
                            OutlineKind::Summary(k) => {
                                let sm = &p.summaries[k];
                                let indent = "    ".repeat(level.saturating_sub(1));
                                ui.label("");
                                ui.label(RichText::new(format!("{indent}{}", sm.wbs)).strong());
                                ui.label(RichText::new(&sm.name).strong());
                                for _ in 3..columns {
                                    ui.label("");
                                }
                                ui.end_row();
                                continue;
                            }
                            OutlineKind::Activity(i) => i,
                        };
                        let a = &mut p.activities[i];
                        ui.label(format!("{}{}", "    ".repeat(level.saturating_sub(1)), a.id));
                        changed |= ui.add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut a.wbs)).changed();
                        changed |= ui.add_sized([220.0, 20.0], egui::TextEdit::singleline(&mut a.name)).changed();
                        changed |= ui.add(egui::DragValue::new(&mut a.duration).speed(0.2).range(0..=5000)).changed();
                        if a.duration == 0 {
                            egui::ComboBox::from_id_salt(("ms", i)).width(70.0).selected_text(if a.milestone_kind == MilestoneKind::Start { "start" } else { "finish" }).show_ui(ui, |ui| {
                                changed |= ui.selectable_value(&mut a.milestone_kind, MilestoneKind::Start, "start").changed();
                                changed |= ui.selectable_value(&mut a.milestone_kind, MilestoneKind::Finish, "finish").changed();
                            });
                        } else {
                            ui.label("");
                        }
                        // Several constraints: the engine has three kinds (start no earlier than, finish no later than, must start on).
                        let mut list = s.cons.remove(&a.id).unwrap_or_else(|| {
                            a.constraints()
                                .map(|c| match c {
                                    Constraint::StartNoEarlierThan(d) => (1, d.to_string()),
                                    Constraint::FinishNoLaterThan(d) => (2, d.to_string()),
                                    Constraint::MustStartOn(d) => (3, d.to_string()),
                                })
                                .collect()
                        });
                        let before = list.clone();
                        ui.vertical(|ui| {
                            ui.set_min_width(300.0);
                            let mut remove: Option<usize> = None;
                            for (k, (kind, text)) in list.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    egui::ComboBox::from_id_salt(("cons", i, k)).width(150.0).selected_text(["choose the kind", "start no earlier than", "finish no later than", "must start on"][*kind as usize]).show_ui(ui, |ui| {
                                        for (n, l) in ["start no earlier than", "finish no later than", "must start on"].iter().enumerate() {
                                            ui.selectable_value(kind, n as u8 + 1, *l);
                                        }
                                    });
                                    ui.add_sized([86.0, 20.0], egui::TextEdit::singleline(text).hint_text("YYYY-MM-DD"));
                                    if *kind != 0 && date_of(text).is_none() {
                                        ui.label(RichText::new("date needed").small().color(WARN()));
                                    }
                                    if ui.small_button("x").on_hover_text("Remove this constraint").clicked() {
                                        remove = Some(k);
                                    }
                                });
                            }
                            if let Some(k) = remove {
                                list.remove(k);
                            }
                            if ui.add(egui::Button::new(RichText::new("Add a constraint").small()).wrap_mode(egui::TextWrapMode::Extend)).clicked() {
                                list.push((0, String::new()));
                            }
                        });
                        if list != before {
                            // Only complete entries reach the programme; the first is the primary constraint.
                            let valid: Vec<Constraint> = list
                                .iter()
                                .filter_map(|(kind, text)| {
                                    let d = date_of(text)?;
                                    match kind {
                                        1 => Some(Constraint::StartNoEarlierThan(d)),
                                        2 => Some(Constraint::FinishNoLaterThan(d)),
                                        3 => Some(Constraint::MustStartOn(d)),
                                        _ => None,
                                    }
                                })
                                .collect();
                            let new_primary = valid.first().copied();
                            let new_extra: Vec<Constraint> = valid.iter().skip(1).copied().collect();
                            if a.constraint != new_primary || a.extra_constraints != new_extra {
                                a.constraint = new_primary;
                                a.extra_constraints = new_extra;
                                changed = true;
                            }
                        }
                        s.cons.insert(a.id.clone(), list);
                        if has_summaries {
                            let shown = a.parent.clone().unwrap_or_else(|| "none".to_string());
                            egui::ComboBox::from_id_salt(("aparent", i)).width(150.0).selected_text(shown).show_ui(ui, |ui| {
                                changed |= ui.selectable_value(&mut a.parent, None, "none").changed();
                                for (id, label) in &summary_ids {
                                    changed |= ui.selectable_value(&mut a.parent, Some(id.clone()), label).changed();
                                }
                            });
                        }
                        if many_calendars {
                            let shown = a.calendar.clone().unwrap_or_else(|| "programme calendar".to_string());
                            egui::ComboBox::from_id_salt(("acal", i)).width(120.0).selected_text(shown).show_ui(ui, |ui| {
                                changed |= ui.selectable_value(&mut a.calendar, None, "programme calendar").changed();
                                for n in &calendar_names {
                                    changed |= ui.selectable_value(&mut a.calendar, Some(n.clone()), n).changed();
                                }
                            });
                        }
                        if ui.small_button("Delete").clicked() {
                            delete = Some(i);
                        }
                        ui.end_row();
                    }
                });
            });
            if let Some(i) = delete {
                let id = p.activities.remove(i).id;
                p.dependencies.retain(|d| d.predecessor != id && d.successor != id);
                p.links.retain(|l| l.activity != id);
                changed = true;
            }
            ui.horizontal(|ui| {
                ui.add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut s.new_act.0).hint_text("id"));
                ui.add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut s.new_act.1).hint_text("WBS"));
                ui.add_sized([220.0, 20.0], egui::TextEdit::singleline(&mut s.new_act.2).hint_text("name"));
                ui.add_sized([90.0, 20.0], egui::TextEdit::singleline(&mut s.new_act.3).hint_text("working days"));
                if ui.button("Add activity").clicked() {
                    let id = s.new_act.0.trim().to_string();
                    match s.new_act.3.trim().parse::<u32>() {
                        _ if id.is_empty() => s.problem = "Give the activity an id.".into(),
                        _ if p.activity(&id).is_some() => s.problem = format!("There is already an activity '{id}'."),
                        Ok(days) => {
                            p.add_activity(Activity::new(&id, s.new_act.1.trim(), s.new_act.2.trim(), days));
                            s.new_act = Default::default();
                            s.problem.clear();
                            changed = true;
                        }
                        Err(_) => s.problem = "Enter the duration as a whole number of working days (0 makes a milestone).".into(),
                    }
                }
            });
            ui.separator();
            ui.label(RichText::new("Dependencies").strong());
            let mut delete_dep: Option<usize> = None;
            egui::ScrollArea::vertical().id_salt("deps").max_height(150.0).auto_shrink([false, true]).show(ui, |ui| {
                egui::Grid::new("dependencies").num_columns(5).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                    for h in ["Predecessor", "Successor", "Type", "Lag (days)", ""] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (i, d) in p.dependencies.iter_mut().enumerate() {
                        for (salt, field) in [("dp", &mut d.predecessor), ("ds", &mut d.successor)] {
                            egui::ComboBox::from_id_salt((salt, i)).width(90.0).selected_text(field.clone()).show_ui(ui, |ui| {
                                for id in &ids {
                                    changed |= ui.selectable_value(field, id.clone(), id).changed();
                                }
                            });
                        }
                        egui::ComboBox::from_id_salt(("dk", i)).width(60.0).selected_text(kind_label(d.kind)).show_ui(ui, |ui| {
                            for k in [LinkType::FS, LinkType::SS, LinkType::FF, LinkType::SF] {
                                changed |= ui.selectable_value(&mut d.kind, k, kind_label(k)).changed();
                            }
                        });
                        changed |= ui.add(egui::DragValue::new(&mut d.lag).speed(0.2)).on_hover_text("Working days of the successor's calendar. Negative means overlap.").changed();
                        if ui.small_button("Delete").clicked() {
                            delete_dep = Some(i);
                        }
                        ui.end_row();
                    }
                });
            });
            if let Some(i) = delete_dep {
                p.dependencies.remove(i);
                changed = true;
            }
            ui.horizontal(|ui| {
                for (salt, field) in [("np", &mut s.new_dep.0), ("ns", &mut s.new_dep.1)] {
                    egui::ComboBox::from_id_salt(salt).width(90.0).selected_text(if field.is_empty() { "choose".to_string() } else { field.clone() }).show_ui(ui, |ui| {
                        for id in &ids {
                            ui.selectable_value(field, id.clone(), id);
                        }
                    });
                }
                egui::ComboBox::from_id_salt("nk").width(60.0).selected_text(kind_label(s.new_dep.2)).show_ui(ui, |ui| {
                    for k in [LinkType::FS, LinkType::SS, LinkType::FF, LinkType::SF] {
                        ui.selectable_value(&mut s.new_dep.2, k, kind_label(k));
                    }
                });
                ui.add_sized([130.0, 20.0], egui::TextEdit::singleline(&mut s.new_dep.3).hint_text("lag, days (blank is 0)"));
                if ui.button("Add dependency").clicked() {
                    let lag = if s.new_dep.3.trim().is_empty() { Some(0) } else { s.new_dep.3.trim().parse::<i32>().ok() };
                    match lag {
                        _ if s.new_dep.0.is_empty() || s.new_dep.1.is_empty() => s.problem = "Choose the predecessor and the successor.".into(),
                        None => s.problem = "Enter the lag as a whole number of working days.".into(),
                        Some(lag) => {
                            p.dependencies.push(Dependency { predecessor: s.new_dep.0.clone(), successor: s.new_dep.1.clone(), kind: s.new_dep.2, lag });
                            s.new_dep.3.clear();
                            s.problem.clear();
                            changed = true;
                        }
                    }
                }
            });
            if !s.problem.is_empty() {
                ui.label(RichText::new(&s.problem).color(BAD()));
            }
            ui.add_space(4.0);
            match &s.result {
                Some(Err(m)) => {
                    for l in m {
                        ui.label(RichText::new(l).small().color(BAD()));
                    }
                }
                Some(Ok(sch)) => {
                    ui.label(RichText::new(format!("Scheduled: {} to {}.", sch.project_start, sch.project_finish)).small().color(GOOD()));
                }
                None => {}
            }
        });
        self.sched.table_open = open;
        changed
    }

    // ─── calendar ────────────────────────────────────────────────────────────────────────

    fn calendar_window(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        let mut changed = false;
        let s = &mut self.sched;
        crate::win::new(ctx, "Programme calendar").open(&mut open).default_width(460.0).show(ctx, |ui| {
            let Some(p) = s.prog.as_mut() else {
                ui.label("Create the programme first (PROGRAMME).");
                return;
            };
            s.calendar_pick = s.calendar_pick.min(p.calendars.len().saturating_sub(1));
            let names: Vec<String> = p.calendars.iter().map(|c| c.name.clone()).collect();
            ui.horizontal(|ui| {
                ui.label("Calendar");
                egui::ComboBox::from_id_salt("cal-pick").selected_text(names.get(s.calendar_pick).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut s.calendar_pick, i, n);
                    }
                });
                let current = names.get(s.calendar_pick).cloned().unwrap_or_default();
                if p.calendar == current {
                    ui.label(RichText::new("the programme calendar").small().weak());
                } else if ui.small_button("Make it the programme calendar").clicked() {
                    p.calendar = current;
                    changed = true;
                }
            });
            let used: Vec<String> = p.activities.iter().filter_map(|a| a.calendar.clone()).collect();
            let default_name = p.calendar.clone();
            if let Some(cal) = p.calendars.get_mut(s.calendar_pick) {
                ui.add_space(4.0);
                ui.label("Working weekdays");
                ui.horizontal(|ui| {
                    for w in Weekday::ALL {
                        let mut on = cal.working_days.contains(&w);
                        if ui.checkbox(&mut on, glue::weekday_label(w)).changed() {
                            if on {
                                cal.working_days.insert(w);
                            } else {
                                cal.working_days.remove(&w);
                            }
                            changed = true;
                        }
                    }
                });
                if cal.working_days.is_empty() {
                    ui.label(RichText::new("A calendar needs at least one working weekday.").small().color(BAD()));
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Holidays and other days off (entered by you; none are built in)").strong());
                let mut remove = None;
                egui::ScrollArea::vertical().id_salt("holidays").max_height(160.0).show(ui, |ui| {
                    if cal.holidays.is_empty() {
                        ui.label(RichText::new("none entered").weak());
                    }
                    for h in &cal.holidays {
                        ui.horizontal(|ui| {
                            ui.label(format!("{h}  {}", glue::weekday_label(h.weekday())));
                            if ui.small_button("Remove").clicked() {
                                remove = Some(*h);
                            }
                        });
                    }
                });
                if let Some(h) = remove {
                    cal.holidays.remove(&h);
                    changed = true;
                }
                ui.horizontal(|ui| {
                    ui.add_sized([110.0, 20.0], egui::TextEdit::singleline(&mut s.holiday).hint_text("YYYY-MM-DD"));
                    if ui.button("Add day off").clicked() {
                        match date_of(&s.holiday) {
                            Some(d) => {
                                cal.holidays.insert(d);
                                s.holiday.clear();
                                s.problem.clear();
                                changed = true;
                            }
                            None => s.problem = "Enter the date as YYYY-MM-DD.".into(),
                        }
                    }
                });
                let name = cal.name.clone();
                let in_use = name == default_name || used.contains(&name);
                if names.len() > 1 && ui.add_enabled(!in_use, egui::Button::new("Delete this calendar")).on_disabled_hover_text("It is the programme calendar or an activity uses it.").clicked() {
                    let idx = s.calendar_pick;
                    p.calendars.remove(idx);
                    changed = true;
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.add_sized([190.0, 20.0], egui::TextEdit::singleline(&mut s.new_calendar).hint_text("name of a further calendar"));
                if ui.button("Add calendar").clicked() && !s.new_calendar.trim().is_empty() && p.calendar_named(s.new_calendar.trim()).is_none() {
                    p.calendars.push(afrobim_schedule::Calendar::new(s.new_calendar.trim(), &[]));
                    s.calendar_pick = p.calendars.len() - 1;
                    s.new_calendar.clear();
                    changed = true;
                }
            });
            if !s.problem.is_empty() {
                ui.label(RichText::new(&s.problem).color(BAD()));
            }
        });
        self.sched.calendar_open = open;
        changed
    }

    // ─── CPM results ─────────────────────────────────────────────────────────────────────

    fn schedule_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut rerun = false;
        let mut open_critical = false;
        let s = &self.sched;
        crate::win::new(ctx, "Schedule").open(&mut open).default_width(1000.0).default_height(480.0).show(ctx, |ui| {
            match &s.result {
                None => {
                    ui.label("There is no programme yet (PROGRAMME).");
                }
                Some(Err(m)) => {
                    ui.label(RichText::new("The programme cannot be scheduled until these are fixed:").color(BAD()).strong());
                    for l in m {
                        ui.label(RichText::new(format!("  {l}")).color(BAD()));
                    }
                }
                Some(Ok(sch)) => {
                    ui.horizontal(|ui| {
                        ui.label(format!("Start {}   Finish {}   {} activities, {} critical", sch.project_start, sch.project_finish, sch.rows.len(), sch.critical().count()));
                        if let Some(d) = sch.data_date {
                            ui.label(format!("   Data date {d}"));
                        }
                        if ui.button("Schedule again").clicked() {
                            rerun = true;
                        }
                        ui.label(format!("   Critical by {}", if s.ext.critical == afrobim_schedule::CriticalBasis::TotalFloat { "total float" } else { "the longest path" }));
                        if ui.small_button("Change").on_hover_text("CRITICAL: choose the basis").clicked() {
                            open_critical = true;
                        }
                    });
                    ui.label(RichText::new(format!("Dates are whole days. Float is in working days of each activity's calendar. Critical: {}", if s.ext.critical == afrobim_schedule::CriticalBasis::TotalFloat { "total float of zero or less and not complete." } else { "on the longest path (the chain of driving relationships that ends at the project finish)." })).small().weak());
                    egui::ScrollArea::both().id_salt("sched-rows").max_height(280.0).auto_shrink([false, false]).show(ui, |ui| {
                        egui::Grid::new("sched-grid").num_columns(12).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                            for h in ["Id", "Name", "Days", "Early start", "Early finish", "Late start", "Late finish", "Total float", "Free float", "Critical", "Status", "Done"] {
                                ui.label(RichText::new(h).strong());
                            }
                            ui.end_row();
                            for r in &sch.rows {
                                let colour = if r.critical { BAD() } else { ui.visuals().text_color() };
                                ui.label(RichText::new(&r.id).color(colour));
                                ui.label(RichText::new(&r.name).color(colour));
                                ui.label(if r.is_milestone { "milestone".to_string() } else { r.duration.to_string() });
                                for d in [r.early_start, r.early_finish, r.late_start, r.late_finish] {
                                    ui.label(RichText::new(d.to_string()).color(colour));
                                }
                                ui.label(RichText::new(r.total_float.to_string()).color(if r.total_float < 0 { BAD() } else { colour }));
                                ui.label(r.free_float.to_string());
                                ui.label(if r.critical { "critical" } else { "" });
                                ui.label(format!("{:?}", r.status));
                                ui.label(format!("{:.0}%", r.percent_complete));
                                ui.end_row();
                            }
                        });
                    });
                    ui.separator();
                    ui.label(RichText::new(format!("Findings ({})", sch.issues.len())).strong());
                    egui::ScrollArea::vertical().id_salt("sched-issues").max_height(120.0).show(ui, |ui| {
                        if sch.issues.is_empty() {
                            ui.label(RichText::new("None.").weak());
                        }
                        for i in &sch.issues {
                            let colour = match i.severity {
                                afrobim_kernel::Severity::Error => BAD(),
                                afrobim_kernel::Severity::Warning => WARN(),
                                afrobim_kernel::Severity::Info => ui.visuals().weak_text_color(),
                            };
                            ui.label(RichText::new(&i.message).color(colour));
                        }
                    });
                }
            }
        });
        self.sched.schedule_open = open;
        if open_critical {
            self.sched.ext.critical_open = true;
        }
        if rerun {
            self.run_schedule();
        }
    }

    // ─── Gantt ───────────────────────────────────────────────────────────────────────────

    fn gantt_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut rebuild = false;
        let s = &mut self.sched;
        crate::win::new(ctx, "Gantt chart").open(&mut open).default_width(1100.0).default_height(520.0).show(ctx, |ui| {
            match (&s.result, &s.gantt) {
                (Some(Ok(_)), Some(g)) => {
                    ui.horizontal(|ui| {
                        ui.label("Scale");
                        ui.add(egui::Slider::new(&mut s.gantt_scale, 1.0..=8.0).show_value(false));
                        ui.label(RichText::new("Critical activities red, others blue, finished grey; black strip is progress; thin grey bar is the baseline; green line the data date.").small().weak());
                    });
                    if let Some(p) = &s.prog {
                        if !p.summaries.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(format!("Summary rows roll up on {}.", if s.ext.rollup == afrobim_schedule::RollupBasis::Duration { "duration" } else { "weight" })).small());
                                ui.label(RichText::new("Fold:").small());
                                for sm in &p.summaries {
                                    let folded = s.ext.collapsed.contains(&sm.id);
                                    if ui.selectable_label(folded, format!("{} {}", sm.wbs, sm.name)).on_hover_text("Fold or unfold: hides the rows under this summary task").clicked() {
                                        if folded {
                                            s.ext.collapsed.remove(&sm.id);
                                        } else {
                                            s.ext.collapsed.insert(sm.id.clone());
                                        }
                                        rebuild = true;
                                    }
                                }
                            });
                            if let Some(e) = &s.ext.rollup_error {
                                ui.label(RichText::new(format!("The roll-up could not be made on this basis ({e}); activities are drawn without summary rows.")).small().color(WARN()));
                            }
                        }
                    }
                    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| paint_gantt(ui, g, s.gantt_scale));
                }
                (Some(Err(m)), _) => {
                    ui.label(RichText::new(format!("The programme cannot be scheduled: {}", m.join("; "))).color(BAD()));
                }
                _ => {
                    ui.label("There is no programme yet (PROGRAMME).");
                }
            };
        });
        self.sched.gantt_open = open;
        if rebuild {
            self.sched.rebuild_gantt();
        }
    }

    // ─── baseline and variance ───────────────────────────────────────────────────────────

    fn baseline_window(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        let mut set = false;
        let s = &mut self.sched;
        crate::win::new(ctx, "Set baseline").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            match (&s.prog, &s.result) {
                (Some(p), Some(Ok(sch))) => {
                    if let Some(b) = &p.baseline {
                        ui.label(format!("Current baseline: '{}', finish {}.", b.name, b.project_finish));
                    } else {
                        ui.label("There is no baseline yet.");
                    }
                    ui.label(format!("The baseline will keep the dates, durations and float of the {} activities as scheduled now (finish {}).", sch.rows.len(), sch.project_finish));
                    ui.add_sized([260.0, 20.0], egui::TextEdit::singleline(&mut s.baseline_name).hint_text("name of the baseline"));
                    if ui.add_enabled(!s.baseline_name.trim().is_empty(), egui::Button::new("Set the baseline")).clicked() {
                        set = true;
                    }
                }
                _ => {
                    ui.label("Schedule the programme first: it must have no errors.");
                }
            };
        });
        self.sched.baseline_open = open;
        if set {
            let name = self.sched.baseline_name.trim().to_string();
            if let (Some(p), Some(Ok(sch))) = (self.sched.prog.as_mut(), self.sched.result.as_ref()) {
                p.set_baseline(&name, sch);
                self.status = format!("Baseline '{name}' set. VARIANCE compares the programme with it after changes.");
                self.sched.baseline_open = false;
                return true;
            }
        }
        false
    }

    fn variance_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut csv = false;
        let s = &self.sched;
        crate::win::new(ctx, "Variance against the baseline").open(&mut open).default_width(760.0).show(ctx, |ui| {
            let (Some(p), Some(Ok(sch))) = (&s.prog, &s.result) else {
                ui.label("There is no schedule yet.");
                return;
            };
            match p.variance(sch) {
                Err(e) => {
                    ui.label(format!("{e}. Set one with BASELINE."));
                }
                Ok(v) => {
                    ui.horizontal(|ui| {
                        ui.label(format!("Baseline '{}'. Project finish variance: {} working days (positive is later).", v.baseline, v.project_finish_variance));
                        if ui.button("Export CSV").clicked() {
                            csv = true;
                        }
                    });
                    egui::ScrollArea::both().auto_shrink([false, true]).max_height(360.0).show(ui, |ui| {
                        egui::Grid::new("variance-grid").num_columns(7).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                            for h in ["Id", "Name", "Status", "Start", "Finish", "Duration", "Float"] {
                                ui.label(RichText::new(h).strong());
                            }
                            ui.end_row();
                            let n = |x: Option<i64>| x.map(|v| format!("{v:+}")).unwrap_or_default();
                            for r in &v.rows {
                                ui.label(&r.id);
                                ui.label(&r.name);
                                ui.label(format!("{:?}", r.status));
                                ui.label(n(r.start_variance));
                                ui.label(n(r.finish_variance));
                                ui.label(n(r.duration_variance));
                                ui.label(n(r.float_variance));
                                ui.end_row();
                            }
                        });
                    });
                    ui.label(RichText::new("Variances are current minus baseline, in working days of each activity's calendar.").small().weak());
                }
            }
        });
        self.sched.variance_open = open;
        if csv {
            let (Some(p), Some(Ok(sch))) = (&self.sched.prog, &self.sched.result) else { return };
            if let Ok(v) = p.variance(sch) {
                if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("variance.csv").save_file() {
                    if path.extension().is_none() {
                        path.set_extension("csv");
                    }
                    self.status = match std::fs::write(&path, variance_csv(&v)) {
                        Ok(()) => format!("Wrote the variance table to {}.", path.display()),
                        Err(e) => format!("Could not write {}: {e}", path.display()),
                    };
                }
            }
        }
    }

    // ─── progress ────────────────────────────────────────────────────────────────────────

    fn progress_window(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        let mut changed = false;
        let mut reschedule = false;
        let s = &mut self.sched;
        crate::win::new(ctx, "Progress").open(&mut open).default_width(820.0).default_height(460.0).show(ctx, |ui| {
            let Some(p) = s.prog.as_mut() else {
                ui.label("There is no programme yet (PROGRAMME).");
                return;
            };
            ui.horizontal(|ui| {
                ui.label("Data date");
                let r = ui.add_sized([100.0, 20.0], egui::TextEdit::singleline(&mut s.data_date_text).hint_text("YYYY-MM-DD"));
                if r.lost_focus() {
                    let new = if s.data_date_text.trim().is_empty() { None } else { date_of(&s.data_date_text) };
                    if (s.data_date_text.trim().is_empty() || new.is_some()) && new != p.data_date {
                        p.data_date = new;
                        changed = true;
                    }
                }
                if ui.button("Reschedule").clicked() {
                    reschedule = true;
                }
            });
            ui.label(RichText::new("Progress is as of the end of the data date. Work done keeps its actual dates; work left is scheduled from the next working day. Actual dates may not be after the data date.").small().weak());
            egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(300.0).show(ui, |ui| {
                egui::Grid::new("progress-grid").num_columns(6).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                    for h in ["Id", "Name", "Percent complete", "Actual start", "Actual finish", "Status"] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for a in p.activities.iter_mut() {
                        ui.label(&a.id);
                        ui.label(&a.name);
                        changed |= ui.add(egui::DragValue::new(&mut a.progress.percent_complete).speed(1.0).range(0.0..=100.0).suffix(" %")).changed();
                        for (k, which) in [(0u8, true), (1u8, false)] {
                            let current = if which { a.progress.actual_start } else { a.progress.actual_finish };
                            let text = s.prog_text.entry((a.id.clone(), k)).or_insert_with(|| current.map(|d| d.to_string()).unwrap_or_default());
                            let r = ui.add_sized([90.0, 20.0], egui::TextEdit::singleline(text).hint_text("YYYY-MM-DD"));
                            if r.lost_focus() {
                                let new = if text.trim().is_empty() { None } else { date_of(text) };
                                if (text.trim().is_empty() || new.is_some()) && new != current {
                                    if which {
                                        a.progress.actual_start = new;
                                    } else {
                                        a.progress.actual_finish = new;
                                    }
                                    changed = true;
                                }
                            }
                        }
                        ui.label(format!("{:?}", a.status()));
                        ui.end_row();
                    }
                });
            });
            match &s.result {
                Some(Err(m)) => {
                    for l in m {
                        ui.label(RichText::new(l).small().color(BAD()));
                    }
                }
                Some(Ok(sch)) => {
                    ui.label(RichText::new(format!("Forecast finish {}.", sch.project_finish)).small().color(GOOD()));
                }
                None => {}
            }
        });
        self.sched.progress_open = open;
        if reschedule {
            self.run_schedule();
        }
        changed
    }

    // ─── links ───────────────────────────────────────────────────────────────────────────

    fn link_window(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        let mut changed = false;
        let mut status: Option<String> = None;
        let selection: Vec<ElementId> = self.doc.selection().to_vec();
        let (mut cats, mut layers): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
        for e in self.doc.elements().iter().filter(|e| glue::linkable(e)) {
            cats.push(e.category.clone());
            layers.push(e.layer.clone());
        }
        cats.sort();
        cats.dedup();
        layers.sort();
        layers.dedup();
        let doc = &self.doc;
        let s = &mut self.sched;
        crate::win::new(ctx, "Link elements to activities").open(&mut open).default_width(760.0).default_height(520.0).show(ctx, |ui| {
            let Some(p) = s.prog.as_mut() else {
                ui.label("There is no programme yet (PROGRAMME).");
                return;
            };
            if s.link_form.action.is_none() {
                s.link_form.action = Some(LinkAction::Construct);
            }
            let ids: Vec<(String, String)> = p.activities.iter().map(|a| (a.id.clone(), format!("{}  {}", a.id, a.name))).collect();
            egui::Grid::new("link-form").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Activity");
                let shown = ids.iter().find(|i| i.0 == s.link_form.activity).map(|i| i.1.clone()).unwrap_or_else(|| "choose".into());
                egui::ComboBox::from_id_salt("lk-act").width(300.0).selected_text(shown).show_ui(ui, |ui| {
                    for (id, label) in &ids {
                        ui.selectable_value(&mut s.link_form.activity, id.clone(), label);
                    }
                });
                ui.end_row();
                ui.label("Action");
                ui.horizontal(|ui| {
                    for a in [LinkAction::Construct, LinkAction::Demolish, LinkAction::Temporary] {
                        ui.radio_value(&mut s.link_form.action, Some(a), action_label(a));
                    }
                });
                ui.end_row();
            });
            let (activity, action) = (s.link_form.activity.clone(), s.link_form.action.unwrap_or(LinkAction::Construct));
            ui.add_space(4.0);
            ui.label(RichText::new("By the current selection").strong());
            let selected: Vec<ElementId> = selection.iter().copied().filter(|id| doc.find(*id).is_some_and(glue::linkable)).collect();
            if ui.add_enabled(!activity.is_empty() && !selected.is_empty(), egui::Button::new(format!("Link the {} selected element(s)", selected.len()))).clicked() {
                for id in &selected {
                    p.link_elements(&activity, ElementSelector::Id(*id), action);
                }
                status = Some(format!("Linked {} element(s) to {activity} ({}).", selected.len(), action_label(action)));
                changed = true;
            }
            ui.add_space(4.0);
            ui.label(RichText::new("By rule (every field you give must match; a mark pattern uses * and ?)").strong());
            egui::Grid::new("link-rule").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Category");
                egui::ComboBox::from_id_salt("lk-cat").selected_text(if s.link_form.category.is_empty() { "any".to_string() } else { s.link_form.category.clone() }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.link_form.category, String::new(), "any");
                    for c in &cats {
                        ui.selectable_value(&mut s.link_form.category, c.clone(), c);
                    }
                });
                ui.end_row();
                ui.label("Layer");
                egui::ComboBox::from_id_salt("lk-layer").selected_text(if s.link_form.layer.is_empty() { "any".to_string() } else { s.link_form.layer.clone() }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.link_form.layer, String::new(), "any");
                    for l in &layers {
                        ui.selectable_value(&mut s.link_form.layer, l.clone(), l);
                    }
                });
                ui.end_row();
                ui.label("Mark pattern");
                ui.add_sized([160.0, 20.0], egui::TextEdit::singleline(&mut s.link_form.pattern).hint_text("e.g. SD-*"));
                ui.end_row();
            });
            let opt = |t: &str| if t.trim().is_empty() { None } else { Some(t.trim().to_string()) };
            let rule = ElementRule { category: opt(&s.link_form.category), layer: opt(&s.link_form.layer), mark_pattern: opt(&s.link_form.pattern) };
            let matched = doc.elements().iter().filter(|e| glue::linkable(e) && rule.matches(e)).count();
            ui.label(RichText::new(format!("The rule matches {matched} element(s) now. Rules are read again each time, so elements added later are picked up.")).small().weak());
            if ui.add_enabled(!activity.is_empty() && matched > 0, egui::Button::new("Link by this rule")).clicked() {
                p.link_elements(&activity, ElementSelector::Rule(rule), action);
                status = Some(format!("Linked {matched} element(s) to {activity} by rule ({}).", action_label(action)));
                s.link_form.pattern.clear();
                changed = true;
            }
            ui.separator();
            ui.label(RichText::new(format!("Links ({})", p.links.len())).strong());
            let mut delete: Option<usize> = None;
            egui::ScrollArea::vertical().id_salt("links-list").max_height(170.0).show(ui, |ui| {
                egui::Grid::new("links-grid").num_columns(5).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                    for (i, l) in p.links.iter().enumerate() {
                        let n = match &l.selector {
                            ElementSelector::Id(id) => usize::from(doc.find(*id).is_some()),
                            ElementSelector::Rule(r) => doc.elements().iter().filter(|e| glue::linkable(e) && r.matches(e)).count(),
                        };
                        ui.label(&l.activity);
                        ui.label(selector_text(doc, &l.selector));
                        ui.label(action_label(l.action));
                        ui.label(RichText::new(format!("{n} element(s)")).color(if n == 0 { WARN() } else { ui.visuals().text_color() }));
                        if ui.small_button("Delete").clicked() {
                            delete = Some(i);
                        }
                        ui.end_row();
                    }
                });
            });
            if let Some(i) = delete {
                let removed: ActivityLink = p.links.remove(i);
                status = Some(format!("Removed the link of {}.", removed.activity));
                changed = true;
            }
        });
        self.sched.link_open = open;
        if let Some(t) = status {
            self.status = t;
        }
        changed
    }

    fn report_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut select: Option<ElementId> = None;
        let doc = &self.doc;
        let s = &self.sched;
        crate::win::new(ctx, "Link report").open(&mut open).default_width(620.0).default_height(480.0).show(ctx, |ui| {
            let Some(p) = &s.prog else {
                ui.label("There is no programme yet (PROGRAMME).");
                return;
            };
            let r = link_report(p, doc);
            let unlinked: Vec<ElementId> = r.unlinked_elements.iter().copied().filter(|id| doc.find(*id).is_some_and(glue::linkable)).collect();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::CollapsingHeader::new(format!("Elements no link reaches ({})", unlinked.len())).default_open(true).show(ui, |ui| {
                    ui.label(RichText::new("These stay as drawn on every date. Click one to select it.").small().weak());
                    ui.horizontal_wrapped(|ui| {
                        for id in &unlinked {
                            if let Some(e) = doc.find(*id) {
                                if ui.small_button(&e.name).clicked() {
                                    select = Some(*id);
                                }
                            }
                        }
                    });
                });
                egui::CollapsingHeader::new(format!("Activities with no elements ({})", r.activities_without_links.len())).default_open(true).show(ui, |ui| {
                    for a in &r.activities_without_links {
                        let name = p.activity(a).map(|x| x.name.clone()).unwrap_or_default();
                        ui.label(format!("{a}  {name}"));
                    }
                });
                egui::CollapsingHeader::new(format!("Links to elements that no longer exist ({})", r.missing_elements.len())).default_open(true).show(ui, |ui| {
                    for (a, id) in &r.missing_elements {
                        ui.label(RichText::new(format!("{a}: element {} is not in the document", id.0)).color(WARN()));
                    }
                });
                egui::CollapsingHeader::new(format!("Rules that match nothing ({})", r.empty_rules.len())).default_open(true).show(ui, |ui| {
                    for k in &r.empty_rules {
                        if let Some(l) = p.links.get(*k) {
                            ui.label(RichText::new(format!("{}: {}", l.activity, selector_text(doc, &l.selector))).color(WARN()));
                        }
                    }
                });
            });
        });
        self.sched.report_open = open;
        if let Some(id) = select {
            self.doc.select(&[id], SelectMode::Replace);
        }
    }

    // ─── the 4D date ─────────────────────────────────────────────────────────────────────

    fn date_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut toggle_play = false;
        let mut new_date: Option<Date> = None;
        let span = self.sched.span();
        let mut counts = [0usize; 4];
        if let Some(st) = &self.sched.states {
            for e in &st.states {
                let i = match e.phase {
                    Phase::NotStarted => 0,
                    Phase::InProgress { .. } => 1,
                    Phase::Complete => 2,
                    Phase::Removed => 3,
                };
                counts[i] += 1;
            }
        }
        let doc = &self.doc;
        let s = &mut self.sched;
        let by_record = &mut self.construction.colour_by_record;
        crate::win::new(ctx, "4D date").open(&mut open).default_width(560.0).show(ctx, |ui| {
            let (Some((lo, hi)), Some(date)) = (span, s.date) else {
                ui.label("Create and schedule a programme to use the date control (PROGRAMME).");
                return;
            };
            ui.horizontal(|ui| {
                ui.label("Colour by");
                ui.radio_value(by_record, false, "programme state");
                ui.radio_value(by_record, true, "recorded progress (SITEREC)");
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.fourd_on, "Colour the 3D view by this date");
                ui.checkbox(&mut s.hide_not_started, "hide elements not started (else ghosted)");
            });
            ui.horizontal(|ui| {
                for (label, step) in [("-1 week", -7), ("-1 day", -1), ("+1 day", 1), ("+1 week", 7)] {
                    if ui.button(label).clicked() {
                        new_date = Some(date.add_days(step));
                    }
                }
                let mut text = date.to_string();
                let r = ui.add_sized([90.0, 20.0], egui::TextEdit::singleline(&mut text));
                if r.changed() {
                    if let Some(d) = date_of(&text) {
                        new_date = Some(d);
                    }
                }
                ui.label(glue::weekday_label(date.weekday()));
            });
            let mut offset = date.days_since(lo);
            let max = hi.days_since(lo);
            let slider = egui::Slider::new(&mut offset, 0..=max).custom_formatter(move |v, _| lo.add_days(v as i64).to_string()).show_value(true);
            if ui.add_sized([ui.available_width() - 90.0, 20.0], slider).changed() {
                new_date = Some(lo.add_days(offset));
            }
            ui.horizontal(|ui| {
                let label = if s.ext.play.playing { "Pause" } else { "Play" };
                if ui.button(label).clicked() {
                    toggle_play = true;
                }
                ui.label("speed");
                for v in [1.0, 2.0, 5.0, 10.0, 30.0] {
                    ui.selectable_value(&mut s.ext.play.speed, v, format!("{v} days/s"));
                }
            });
            ui.label(RichText::new("Playback moves the date forward in the 3D view; drag the bar to scrub. Speed is how many days of the programme pass each second of viewing.").small().weak());
            ui.add_space(4.0);
            ui.label(RichText::new(format!(
                "State at the end of the day: {} not started, {} in progress, {} complete, {} removed. Elements no link reaches stay as drawn.",
                counts[0], counts[1], counts[2], counts[3]
            ))
            .small());
            ui.label(RichText::new("Not started: ghosted or hidden. In progress: amber. Complete: normal. Demolished or temporary work that is over: removed.").small().weak());
            let _ = doc;
        });
        self.sched.date_open = open;
        if toggle_play {
            self.play_toggle(ctx);
        }
        if let Some(d) = new_date {
            // Scrubbing reads the playback's timelines: no element is evaluated again.
            self.sched.ext.play.playing = false;
            self.play_to(d);
            self.status = format!("4D date {} ({}).", d, glue::weekday_label(d.weekday()));
        }
    }
}

/// Where the demo and the commands open the 4D tab's windows.
pub fn open_date_control(app: &mut AfroBimApp) {
    app.sched.sync(&app.doc);
    if app.sched.prog.is_none() {
        app.status = "There is no programme yet (PROGRAMME).".into();
        app.sched.table_open = true;
        return;
    }
    app.sched.date_open = true;
    app.sched.fourd_on = true;
    app.view_3d = true;
    app.status = "4D date control open.".into();
}
