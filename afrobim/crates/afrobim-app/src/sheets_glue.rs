//! Drawing sheets in the app: live data for the schedule and chart sheets, composing a stored
//! sheet set from the live model, the exports (PDF, DXF, headless), and the demo sheet set.
//!
//! A sheet set is stored in the document as one `sheets` element (see the `afrobim-sheets`
//! crate). The tables and charts of the schedule and detail sheets carry only the *name of a
//! feed* (`Table::feed`, `DetailView::feed`); the rows and drawings are filled in from the
//! document each time the set is composed, so the sheets always show the model as it is now.
//! What is stored in the document is the definition, never a copy of the data.

use std::path::{Path, PathBuf};

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, ModuleRegistry, Note};
use afrobim_schedule::{Date, GanttOptions, SCurveBasis};
use afrobim_sheets::{
    add_sheet_set, compose_set_with, load_sheet_set, module_quantities, priced_bill_table, project_info, set_project_info, sheet_sets, store_sheet_set, ComposedSet, DetailLabel, DetailSource,
    DetailView, Model, Numbering, Orientation, Page, PaperSize, PlanSeries, PlanView, ProfileView, ProjectInfo, Revision, Scale, SheetSet, SheetSource, SheetSpec, Table, TableSeries, UserField,
    ViewDef, ViewKind, SHEETS_CATEGORY,
};

use crate::check_all::{self, Sources};
use crate::{cost_glue, schedule_glue, signs_glue};

// ─── feed names ─────────────────────────────────────────────────────────────────────────

pub const FEED_SIGNS: &str = "sign_schedule";
pub const FEED_BILL: &str = "priced_bill";
pub const FEED_ACTIVITIES: &str = "activities";
pub const FEED_CLASHES: &str = "clashes";
pub const FEED_CHECKS: &str = "check_summary";
/// `quantities:<category>` for one module's values, `quantities:*` for every module's.
pub const FEED_QUANTITIES: &str = "quantities:";
pub const FEED_GANTT: &str = "gantt";
pub const FEED_SCURVE: &str = "s_curve";
pub const FEED_WBS_GANTT: &str = "wbs_gantt";
pub const FEED_TIME_LOCATION: &str = "time_location";
pub const FEED_COST_SCURVE: &str = "cost_s_curve";
/// `vis_plan:<sign mark>` and `vis_profile:<sign mark>`: the sign visibility drawings of VISIBILITY.
pub const FEED_VIS_PLAN: &str = "vis_plan";
pub const FEED_VIS_PROFILE: &str = "vis_profile";
/// `light_contours:<installation mark>`: iso-illuminance contours of the stored photometry run.
pub const FEED_LIGHT_CONTOURS: &str = "light_contours";

/// The kinds of schedule table a sheet can carry, as (feed name, label, default sheet title).
pub const TABLE_KINDS: [(&str, &str, &str); 6] = [
    (FEED_SIGNS, "Sign schedule", "SIGN SCHEDULE"),
    (FEED_QUANTITIES, "Quantities of a module", "QUANTITIES"),
    (FEED_BILL, "Priced bill of quantities", "BILL OF QUANTITIES"),
    (FEED_ACTIVITIES, "Activity table (programme)", "ACTIVITY TABLE"),
    (FEED_CLASHES, "Clash results", "CLASH RESULTS"),
    (FEED_CHECKS, "Model-wide check summary", "CHECK SUMMARY"),
];

/// The kinds of detail sheet, as (feed name or "element", label, default sheet title).
pub const DETAIL_KINDS: [(&str, &str, &str); 9] = [
    ("element", "A selected sign face", "SIGN FACE"),
    (FEED_GANTT, "The Gantt chart", "CONSTRUCTION PROGRAMME"),
    (FEED_SCURVE, "S-curve of the programme", "PROGRAMME S-CURVE"),
    (FEED_WBS_GANTT, "The Gantt chart with summary tasks (WBS)", "PROGRAMME BY WBS"),
    (FEED_TIME_LOCATION, "Time-location chart", "TIME-LOCATION CHART"),
    (FEED_COST_SCURVE, "S-curve of the cash flow (cost)", "COST S-CURVE"),
    (FEED_VIS_PLAN, "Sign visibility, plan envelope (of a sign)", "SIGN VISIBILITY PLAN"),
    (FEED_VIS_PROFILE, "Sign visibility, sight-line profile (of a sign)", "SIGN VISIBILITY PROFILE"),
    (FEED_LIGHT_CONTOURS, "Lighting, iso-illuminance contours (of an installation)", "LIGHTING CONTOURS"),
];

/// True for the detail feeds that name an element after a colon.
pub fn feed_needs_element(feed: &str) -> bool {
    [FEED_VIS_PLAN, FEED_VIS_PROFILE, FEED_LIGHT_CONTOURS].contains(&feed)
}

/// The live data the feeds read, collected from the document.
#[derive(Default)]
pub struct Live {
    pub sign_csv: Option<String>,
    pub bill_csv: Option<String>,
    pub activity_csv: Option<String>,
    pub clash_csv: Option<String>,
    /// Mark, category, check, status, reason of each check of the model-wide check.
    pub check_rows: Option<Vec<[String; 5]>>,
    pub gantt: Option<(Drawing, Vec<DetailLabel>)>,
    pub scurve: Option<(Drawing, Vec<DetailLabel>)>,
    pub wbs_gantt: Option<(Drawing, Vec<DetailLabel>)>,
    pub time_location: Option<(Drawing, Vec<DetailLabel>)>,
    pub cost_scurve: Option<(Drawing, Vec<DetailLabel>)>,
    /// Drawings fed by an element, by `kind:mark` (visibility plan and profile, lighting contours).
    pub by_element: std::collections::HashMap<String, (Drawing, Vec<DetailLabel>)>,
    /// Why a feed has no data, by feed name.
    pub missing: Vec<(&'static str, String)>,
}

impl Live {
    /// Reads every feed's data from the document. `engine` is the sign engine when the traffic
    /// signs module is loaded (the sign schedule and the sign lines of the bill need it).
    pub fn from_doc(doc: &Document, registry: &ModuleRegistry, engine: Option<&afrobim_signs::SignEngine>) -> Live {
        let mut live = Live::default();

        // The sign schedule.
        match engine {
            Some(engine) => {
                let elements: Vec<&Element> = doc.elements().iter().collect();
                let signs = signs_glue::definitions(&elements);
                if signs.is_empty() {
                    live.missing.push((FEED_SIGNS, "There are no signs in the document.".into()));
                } else {
                    live.sign_csv = Some(signs_glue::quantities(engine, &signs).0);
                }
            }
            None => live.missing.push((FEED_SIGNS, "The traffic signs module is not loaded.".into())),
        }

        // The programme: activity table, Gantt chart, S-curve.
        let programme = schedule_glue::load(doc).map(|(_, p)| p);
        match &programme {
            Some(p) => match afrobim_schedule::schedule_with(p, &afrobim_schedule::ScheduleOptions { critical: schedule_glue::critical_basis_of(doc) }) {
                Ok(s) => {
                    live.activity_csv = Some(afrobim_schedule::activity_table_csv(p, &s));
                    // The Gantt chart with summary tasks, rolled up on the basis the user chose.
                    if p.summaries.is_empty() {
                        live.missing.push((FEED_WBS_GANTT, "The programme has no summary tasks (WBS): add some in the WBS window.".into()));
                    } else {
                        match afrobim_schedule::wbs_rollup(p, &s, schedule_glue::rollup_basis_of(doc)) {
                            Ok(ru) => {
                                let g = afrobim_schedule::gantt_drawing_wbs(p, &s, &GanttOptions::default(), &ru);
                                live.wbs_gantt = Some((g.drawing, g.labels.iter().map(|l| DetailLabel::new(l.at, &l.text)).collect()));
                            }
                            Err(e) => live.missing.push((FEED_WBS_GANTT, format!("The roll-up cannot be made: {}", crate::schedule_ext::error_text(e)))),
                        }
                    }
                    match crate::schedule_ext::time_location_for_sheet(p, &s, doc) {
                        Ok((d, labels)) => live.time_location = Some((d, labels.iter().map(|(at, text)| DetailLabel::new(*at, text)).collect())),
                        Err(e) => live.missing.push((FEED_TIME_LOCATION, e)),
                    }
                    let g = afrobim_schedule::gantt_drawing(p, &s, &GanttOptions::default());
                    live.gantt = Some((g.drawing, g.labels.iter().map(|l| DetailLabel::new(l.at, &l.text)).collect()));
                    match afrobim_schedule::s_curve(p, &s, SCurveBasis::Duration) {
                        Ok(c) => {
                            let (d, labels) = afrobim_schedule::s_curve_drawing(&c, 100.0, 60.0);
                            live.scurve = Some((d, labels.iter().map(|l| DetailLabel::new(l.at, &l.text)).collect()));
                        }
                        Err(e) => live.missing.push((FEED_SCURVE, format!("The S-curve cannot be made: {e}"))),
                    }
                }
                Err(e) => {
                    let why = match e {
                        afrobim_schedule::ScheduleError::Invalid(issues) => issues.into_iter().map(|i| i.message).collect::<Vec<_>>().join("; "),
                        afrobim_schedule::ScheduleError::Other(m) => m,
                    };
                    for f in [FEED_ACTIVITIES, FEED_GANTT, FEED_SCURVE, FEED_WBS_GANTT, FEED_TIME_LOCATION] {
                        live.missing.push((f, format!("The programme does not schedule: {why}")));
                    }
                }
            },
            None => {
                for f in [FEED_ACTIVITIES, FEED_GANTT, FEED_SCURVE, FEED_WBS_GANTT, FEED_TIME_LOCATION] {
                    live.missing.push((f, "There is no programme in the document.".into()));
                }
            }
        }

        // The estimate: priced bill and cash flow.
        match cost_glue::list(doc).first().and_then(|(id, _)| cost_glue::load(doc, *id)) {
            Some(est) => {
                let signs = cost_glue::sign_rows(engine, doc);
                let lines = cost_glue::collect(doc, &signs);
                match est.compute(&lines) {
                    Ok(result) => {
                        live.bill_csv = Some(afrobim_cost::priced_boq_csv(&result));
                        match est.cash_flow(&result) {
                            Ok(cf) if !cf.months.is_empty() => live.cost_scurve = Some(cost_curve_drawing(&cf)),
                            Ok(_) => live.missing.push((FEED_COST_SCURVE, "The estimate has no phasing, so there is no cash flow.".into())),
                            Err(e) => live.missing.push((FEED_COST_SCURVE, format!("The cash flow cannot be made: {e}"))),
                        }
                    }
                    Err(e) => {
                        live.missing.push((FEED_BILL, format!("The estimate does not price: {e}")));
                        live.missing.push((FEED_COST_SCURVE, format!("The estimate does not price: {e}")));
                    }
                }
            }
            None => {
                live.missing.push((FEED_BILL, "There is no estimate in the document (open ESTIMATE to start one).".into()));
                live.missing.push((FEED_COST_SCURVE, "There is no estimate in the document.".into()));
            }
        }

        live.add_element_drawings(doc, registry);

        // Clash results.
        let (_, co) = crate::coordination_glue::load(doc);
        if co.results.clashes.is_empty() {
            live.missing.push((FEED_CLASHES, "There are no clash results (run CLASH first).".into()));
        } else {
            live.clash_csv = Some(co.results.to_csv());
        }

        // The model-wide check, from each element's own module messages.
        let report = check_report(doc, registry);
        if report.rows.is_empty() {
            live.missing.push((FEED_CHECKS, "No element in the document has a check.".into()));
        } else {
            live.check_rows = Some(report.rows.iter().map(|r| [r.mark.clone(), r.category.clone(), r.check.clone(), r.status.label().to_string(), r.reason.clone()]).collect());
        }
        live
    }

    /// The drawings that belong to one element: the sign visibility plan and profile of each sign
    /// with accepted inputs, and the contours of each lighting installation with a stored run.
    fn add_element_drawings(&mut self, doc: &Document, registry: &ModuleRegistry) {
        if crate::visibility_ui::any_stored(doc) {
            let solids = |e: &Element| registry.generate(e).map(|g| g.solids).unwrap_or_default();
            let x: std::collections::HashMap<ElementId, f64> = doc.elements().iter().filter_map(|e| registry.generate(e).ok().and_then(|g| afrobim_visibility::derived_x_height(&g.derived)).map(|v| (e.id, v))).collect();
            let src = crate::visibility_ui::source_for(doc, &solids, x);
            for e in doc.elements().iter().filter(|e| e.category == afrobim_visibility::SIGN_CATEGORY && e.site.is_some()) {
                if afrobim_visibility::load(doc, e.id).is_none() || !crate::visibility_ui::eye_height_accepted(doc, e.id) {
                    continue;
                }
                let a = crate::visibility_ui::analyse(doc, &src, e.id);
                let (plan, profile) = crate::visibility_ui::drawings_of(&a, Some(&src.scene), 5.0);
                for (feed, d) in [(FEED_VIS_PLAN, plan), (FEED_VIS_PROFILE, profile)] {
                    if let Some(d) = d {
                        let labels = d.labels.iter().map(|l| DetailLabel::new(l.at, &l.text)).collect();
                        self.by_element.insert(format!("{feed}:{}", e.name), (d.drawing, labels));
                    }
                }
            }
        }
        for e in doc.elements().iter().filter(|e| e.category == afrobim_lighting::CATEGORY) {
            if !matches!(afrobim_lighting::calc::photometry_settings_of(doc, e.id), Ok(Some(_))) {
                continue;
            }
            let Ok(data) = afrobim_lighting::lighting_of(doc, e.id) else { continue };
            let name = match &data.definition {
                afrobim_lighting::LightingDefinition::Installation(i) => i.alignment.clone(),
                afrobim_lighting::LightingDefinition::Column(c) => c.alignment.clone(),
            };
            let alignment = crate::alignment_glue::alignments(doc).into_iter().find(|a| a.1 == name).and_then(|a| crate::alignment_glue::alignment_of(doc, a.0));
            let Ok(run) = afrobim_lighting::calc::run_stored_photometry(doc, e.id, alignment.as_ref(), None) else { continue };
            let class = data.definition.lighting_class().to_string();
            let Some(area) = run.primary_area_for_class(&class).map(|s| s.to_string()).or_else(|| run.result.areas.first().map(|a| a.name.clone())) else { continue };
            let levels = crate::photometry_ui::levels_for(&run, &area, "");
            if let (Some(c), _) = crate::photometry_ui::drawings_of(&run, &data, &area, &levels) {
                let labels = c.labels.iter().map(|l| DetailLabel::new(Pt2::new(l.x, l.y), &l.text)).collect();
                self.by_element.insert(format!("{FEED_LIGHT_CONTOURS}:{}", e.name), (c.drawing, labels));
            }
        }
    }

    fn why(&self, feed: &str) -> String {
        if let Some((kind, mark)) = feed.split_once(':') {
            return match kind {
                FEED_LIGHT_CONTOURS => format!("There is no stored photometry run for '{mark}': run PHOTOMETRY for it first."),
                _ => format!("There is no visibility drawing for '{mark}': store accepted visibility inputs for the sign (VISIBILITY) and place it on an alignment."),
            };
        }
        self.missing.iter().find(|(f, _)| *f == feed).map(|(_, w)| w.clone()).unwrap_or_else(|| "No data.".into())
    }
}

/// The model-wide check of the document, from each element's own module messages and, when any
/// sign has stored visibility inputs, the obstruction scene built from the modules' solids.
pub fn check_report(doc: &Document, registry: &ModuleRegistry) -> check_all::Report {
    let notes: std::collections::HashMap<ElementId, Result<Vec<Note>, String>> =
        doc.elements().iter().filter(|e| registry.get(&e.category).is_some()).map(|e| (e.id, registry.generate(e).map(|g: Generated| g.notes))).collect();
    let labels: std::collections::HashMap<String, String> = registry.categories().map(|(c, n)| (c.to_string(), n.to_string())).collect();
    let notes_of = |id: ElementId| notes.get(&id).cloned();
    let label_of = |c: &str| labels.get(c).cloned().unwrap_or_else(|| c.to_string());
    let vis_src = crate::visibility_ui::any_stored(doc).then(|| {
        let solids = |e: &Element| registry.generate(e).map(|g| g.solids).unwrap_or_default();
        let x: std::collections::HashMap<ElementId, f64> = doc.elements().iter().filter_map(|e| registry.generate(e).ok().and_then(|g| afrobim_visibility::derived_x_height(&g.derived)).map(|v| (e.id, v))).collect();
        crate::visibility_ui::source_for(doc, &solids, x)
    });
    check_all::run_all(doc, &Sources { notes_of: &notes_of, label_of: &label_of, vis: vis_src.as_ref() })
}

/// The cost S-curve (cumulative percent of the total against date) in a box 100 by 60 units, in
/// the style of the programme S-curve.
fn cost_curve_drawing(cf: &afrobim_cost::CashFlow) -> (Drawing, Vec<DetailLabel>) {
    let (width, height) = (100.0, 60.0);
    let line = |a: Pt2, b: Pt2, w: f64, c: Colour| Primitive::Line { from: a, to: b, width: w, colour: Some(c) };
    let mut prims = Vec::new();
    let mut labels = Vec::new();
    for q in 0..=4 {
        let y = height * q as f64 / 4.0;
        prims.push(line(Pt2::new(0.0, y), Pt2::new(width, y), 0.0, Colour::Grey));
        labels.push(DetailLabel::new(Pt2::new(0.0, y), &format!("{}%", q * 25)));
    }
    prims.push(line(Pt2::new(0.0, 0.0), Pt2::new(0.0, height), 0.2, Colour::Black));
    prims.push(line(Pt2::new(0.0, 0.0), Pt2::new(width, 0.0), 0.2, Colour::Black));
    let pts = cf.s_curve();
    let day = |d: &afrobim_cost::Date| Date::parse(&d.to_string()).map(|x| x.days_since(Date::from_ymd(1970, 1, 1).expect("a real date")) as f64);
    let (first, last) = (pts.first().and_then(|p| day(&p.date)), pts.last().and_then(|p| day(&p.date)));
    if let (Some(f), Some(l)) = (first, last) {
        let span = (l - f).max(1.0);
        let at = |p: &afrobim_cost::SCurvePoint| Pt2::new(width * (day(&p.date).unwrap_or(f) - f) / span, height * (p.percent.to_f64() / 100.0).clamp(0.0, 1.0));
        for w in pts.windows(2) {
            prims.push(line(at(&w[0]), at(&w[1]), 0.4, Colour::Blue));
        }
        labels.push(DetailLabel::new(Pt2::new(0.0, 0.0), &pts[0].date.to_string()));
        let mut end = DetailLabel::new(Pt2::new(width, 0.0), &pts[pts.len() - 1].date.to_string());
        end.align = afrobim_sheets::Align::Right;
        labels.push(end);
    }
    (Drawing { primitives: prims, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(width, height) } }, labels)
}

// ─── feeding a set ──────────────────────────────────────────────────────────────────────

fn note_table(title: &str, message: &str) -> Table {
    Table::new(&["Note"], vec![vec![message.to_string()]]).with_title(title)
}

fn table_for(feed: &str, title: &str, model: &Model, live: &Live) -> Table {
    let csv_table = |csv: &Option<String>| match csv {
        Some(c) => Table::from_csv(title, c),
        None => note_table(title, &live.why(feed)),
    };
    match feed {
        FEED_SIGNS => csv_table(&live.sign_csv),
        FEED_BILL => match &live.bill_csv {
            Some(c) => {
                let mut t = priced_bill_table(c);
                t.title = Some(title.to_string());
                t
            }
            None => note_table(title, &live.why(feed)),
        },
        FEED_ACTIVITIES => match &live.activity_csv {
            Some(c) => {
                let mut t = afrobim_sheets::activity_table(c);
                t.title = Some(title.to_string());
                t
            }
            None => note_table(title, &live.why(feed)),
        },
        FEED_CLASHES => match &live.clash_csv {
            Some(c) => {
                let mut t = afrobim_sheets::clash_table(c);
                t.title = Some(title.to_string());
                t
            }
            None => note_table(title, &live.why(feed)),
        },
        FEED_CHECKS => match &live.check_rows {
            Some(rows) => {
                let mut t = Table::new(&["Mark", "Category", "Check", "Status", "Reason"], rows.iter().map(|r| r.to_vec()).collect()).with_title(title);
                t.col_widths_mm = vec![28.0, 40.0, 70.0, 24.0, 0.0];
                t
            }
            None => note_table(title, &live.why(feed)),
        },
        f if f.starts_with(FEED_QUANTITIES) => {
            let category = f.trim_start_matches(FEED_QUANTITIES);
            let cat = if category == "*" || category.is_empty() { None } else { Some(category) };
            let mut t = module_quantities(model, title, cat, &["plan_origin_x_m", "plan_origin_y_m"]);
            if t.rows.is_empty() {
                t = note_table(title, "No element of that category reports quantities.");
            }
            t
        }
        other => note_table(title, &format!("Unknown data feed '{other}'.")),
    }
}

/// Fills every fed table and chart of `set` from the live data. What it could not fill is said
/// in the returned notes and in the sheet itself (a one-line table).
pub fn feed_set(set: &mut SheetSet, model: &Model, live: &Live) -> Vec<String> {
    let mut notes = Vec::new();
    for source in &mut set.sources {
        let spec_title = source.template().title.clone();
        for view in &mut source.template_mut().views {
            match &mut view.kind {
                ViewKind::Table(t) => {
                    if let Some(feed) = t.feed.clone() {
                        let title = t.title.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| spec_title.clone());
                        let fresh = table_for(&feed, &title, model, live);
                        if fresh.headers == ["Note"] {
                            notes.push(format!("{title}: {}", fresh.rows[0][0]));
                        }
                        *t = Table { feed: Some(feed), ..fresh };
                    }
                }
                ViewKind::Detail(d) => {
                    if let Some(feed) = d.feed.clone() {
                        let data = match feed.as_str() {
                            FEED_GANTT => live.gantt.clone(),
                            FEED_SCURVE => live.scurve.clone(),
                            FEED_WBS_GANTT => live.wbs_gantt.clone(),
                            FEED_TIME_LOCATION => live.time_location.clone(),
                            FEED_COST_SCURVE => live.cost_scurve.clone(),
                            other if other.contains(':') => live.by_element.get(other).cloned(),
                            _ => None,
                        };
                        match data {
                            Some((drawing, labels)) => d.source = DetailSource::Inline { drawing, labels },
                            None => {
                                notes.push(format!("{spec_title}: {}", live.why(&feed)));
                                d.source = DetailSource::Inline { drawing: Drawing { primitives: vec![], extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(1.0, 1.0) } }, labels: vec![DetailLabel::new(Pt2::new(0.0, 0.0), &live.why(&feed))] };
                                d.fit = true;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    notes
}

/// A composed set with what it was made from and when.
pub struct Composed {
    pub set_name: String,
    pub set: SheetSet,
    pub composed: ComposedSet,
    /// What the live data could not give (also shown on the sheets themselves).
    pub feed_notes: Vec<String>,
    /// The time it was composed, as text, in UTC.
    pub at: String,
}

/// The time now as `2026-11-14 10:32 UTC`. The system clock is UTC; the machine's time zone is not
/// consulted.
pub fn now_text() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    time_text(secs)
}

/// `secs` since 1970-01-01 00:00 UTC as `YYYY-MM-DD HH:MM UTC`.
pub fn time_text(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let date = Date::from_ymd(1970, 1, 1).expect("a real date").add_days(days);
    format!("{date} {:02}:{:02} UTC", rest / 3600, rest % 3600 / 60)
}

/// Feeds and composes the stored set `id` from the live model.
pub fn compose_stored(doc: &Document, registry: &ModuleRegistry, id: ElementId, live: &Live) -> Result<Composed, String> {
    let mut set = load_sheet_set(doc, id)?;
    let name = doc.find(id).map(|e| e.name.clone()).unwrap_or_else(|| set.name.clone());
    let model = Model { doc, registry };
    let feed_notes = feed_set(&mut set, &model, live);
    let info = project_info(doc);
    let composed = compose_set_with(&model, &set, &info);
    Ok(Composed { set_name: name, set, composed, feed_notes, at: now_text() })
}

// ─── exports ────────────────────────────────────────────────────────────────────────────

/// File name made safe from a sheet number or title.
pub fn file_stem(text: &str) -> String {
    let s: String = text.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if s.is_empty() {
        "sheet".into()
    } else {
        s
    }
}

/// Writes the whole set as one multi-page PDF. Returns the number of pages.
pub fn write_pdf(c: &Composed, path: &Path) -> Result<usize, String> {
    std::fs::write(path, c.composed.pdf(&c.set_name)).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    Ok(c.composed.sheets.len())
}

/// Writes one DXF per sheet into `dir`, named by sheet number. Returns the files written.
pub fn write_dxf_per_sheet(c: &Composed, dir: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Could not make {}: {e}", dir.display()))?;
    let mut out = Vec::new();
    for sheet in &c.composed.sheets {
        let path = dir.join(format!("{}.dxf", file_stem(&sheet.number)));
        std::fs::write(&path, afrobim_sheets::write_dxf(sheet, &c.set.styles.pens)).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
        out.push(path);
    }
    Ok(out)
}

/// Writes every sheet side by side in one DXF (a gap of 50 mm between sheets).
pub fn write_dxf_one_file(c: &Composed, path: &Path) -> Result<(), String> {
    std::fs::write(path, c.composed.dxf(&c.set.styles, 50.0)).map_err(|e| format!("Could not write {}: {e}", path.display()))
}

/// The headless export (`--export-set <path>`): a path ending in `.pdf` gets the PDF; one ending in
/// `.dxf` gets one DXF with every sheet; anything else is a folder that gets `sheets.pdf` and one
/// DXF per sheet. Returns a line saying what was written.
pub fn export_headless(c: &Composed, path: &Path) -> Result<String, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
    match ext.as_deref() {
        Some("pdf") => {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                let _ = std::fs::create_dir_all(parent);
            }
            let pages = write_pdf(c, path)?;
            Ok(format!("Wrote {pages} sheet(s) of '{}' to {}.", c.set_name, path.display()))
        }
        Some("dxf") => {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                let _ = std::fs::create_dir_all(parent);
            }
            write_dxf_one_file(c, path)?;
            Ok(format!("Wrote {} sheet(s) of '{}' to {}.", c.composed.sheets.len(), c.set_name, path.display()))
        }
        _ => {
            std::fs::create_dir_all(path).map_err(|e| format!("Could not make {}: {e}", path.display()))?;
            let pages = write_pdf(c, &path.join("sheets.pdf"))?;
            let files = write_dxf_per_sheet(c, path)?;
            Ok(format!("Wrote sheets.pdf ({pages} page(s)) and {} DXF file(s) of '{}' into {}.", files.len(), c.set_name, path.display()))
        }
    }
}

// ─── the set in the document ────────────────────────────────────────────────────────────

/// Adds an empty sheet set with this name. The numbering and styles are the crate's neutral
/// defaults, shown in the manager as editable defaults.
pub fn new_set(doc: &mut Document, name: &str) -> ElementId {
    let name = doc.unique_name(name);
    add_sheet_set(doc, &name, &SheetSet::new(&name))
}

/// The sheet sets of the document: id and name.
pub fn sets(doc: &Document) -> Vec<(ElementId, String)> {
    sheet_sets(doc)
}

/// Stores the set (definition only: fed tables and charts keep just their feed names and no data).
pub fn store(doc: &mut Document, id: ElementId, set: &SheetSet) -> Result<(), String> {
    let mut clean = set.clone();
    for source in &mut clean.sources {
        for view in &mut source.template_mut().views {
            match &mut view.kind {
                ViewKind::Table(t) if t.feed.is_some() => {
                    t.headers.clear();
                    t.rows.clear();
                    t.col_widths_mm.clear();
                    t.align.clear();
                }
                ViewKind::Detail(d) if d.feed.is_some() => {
                    d.source = DetailSource::Inline { drawing: Drawing { primitives: vec![], extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(1.0, 1.0) } }, labels: vec![] };
                }
                _ => {}
            }
        }
    }
    store_sheet_set(doc, id, &clean)
}

// ─── the demo ───────────────────────────────────────────────────────────────────────────

/// The demo project information. Every field says DEMO: none of it belongs to a real project.
pub fn demo_project_info() -> ProjectInfo {
    ProjectInfo {
        project: "DEMO: made-up road project".into(),
        client: "DEMO client (not a real client)".into(),
        contract_number: "DEMO-000".into(),
        drawn_by: "DEMO".into(),
        checked_by: "DEMO".into(),
        approved_by: "DEMO".into(),
        date: "DEMO".into(),
        revisions: vec![Revision { rev: "D0".into(), date: "DEMO".into(), description: "DEMO revision, not real".into(), by: "D".into(), checked: "D".into() }],
        user_fields: vec![UserField { label: "Status".into(), value: "DEMO, not for construction".into() }],
        logo_text: "DEMO".into(),
    }
}

/// Adds the demo project information and a demo sheet set: a plan series, one profile and one
/// schedule, on the first alignment. **Every number below is an arbitrary demo value** chosen so
/// that the sheets show something on the demo alignment; none is a scale, interval, length or
/// layout from a standard or from any organisation. Returns what could not be added.
pub fn demo(doc: &mut Document) -> Vec<String> {
    set_project_info(doc, &demo_project_info());
    let Some((alignment_id, name)) = crate::alignment_glue::alignments(doc).into_iter().next() else { return vec!["Demo sheets: there is no alignment.".to_string()] };
    let Some(a) = crate::alignment_glue::alignment_of(doc, alignment_id) else { return vec!["Demo sheets: the alignment cannot be read.".to_string()] };
    let page = Page::new(PaperSize::A3, Orientation::Landscape);
    let mut set = SheetSet::new("DEMO sheets");
    // Demo numbering: prefix "DEMO-", three digits (the crate's default).
    set.numbering = Numbering { prefix: "DEMO-".into(), ..Numbering::default() };

    // Plan series: demo scale 1:2000, 300 m of alignment a sheet, 30 m overlap, ticks every 20 m
    // and labels every 100 m, the whole alignment.
    let mut plan = PlanView::new(&name, a.start_station_m, a.end_station_m(), Scale(2000.0));
    plan.tick_interval_m = 20.0;
    plan.label_interval_m = 100.0;
    let mut template = SheetSpec::new("DEMO PLAN", page, vec![ViewDef::full(ViewKind::Plan(plan))]);
    template.number_prefix = Some("DEMO-PL-".into());
    set.sources.push(SheetSource::PlanSeries(PlanSeries { template, plan_view: 0, from_station_m: None, to_station_m: None, length_m: 300.0, overlap_m: 30.0 }));

    // One profile over the whole profile: demo horizontal scale 1:6000, vertical scale 1:300 (so an
    // exaggeration of 20), grid every 100 m and every 1 m of level, with the demo ground if present.
    if let Some((lo, hi)) = a.profile.as_ref().and_then(|p| p.range()) {
        let mut profile = ProfileView::new(&name, lo, hi, Scale(6000.0), 20.0);
        profile.grid_station_interval_m = 100.0;
        profile.grid_level_interval_m = 1.0;
        if doc.elements().iter().any(|e| e.category == afrobim_corridor::SURFACE_CATEGORY && e.name == "Demo ground") {
            profile.surface = Some("Demo ground".into());
        }
        let mut spec = SheetSpec::new("DEMO PROFILE", page, vec![ViewDef::full(ViewKind::Profile(profile))]);
        spec.number_prefix = Some("DEMO-PR-".into());
        set.sources.push(SheetSource::Single(spec));
    }

    // One schedule: the programme's activity table, as many sheets as it needs.
    let mut table = Table::default();
    table.title = Some("DEMO ACTIVITY TABLE".into());
    table.feed = Some(FEED_ACTIVITIES.into());
    let mut spec = SheetSpec::new("DEMO ACTIVITY TABLE", page, vec![ViewDef::full(ViewKind::Table(table))]);
    spec.number_prefix = Some("DEMO-SC-".into());
    set.sources.push(SheetSource::TableSeries(TableSeries { template: spec, table_view: 0 }));

    let id = new_set(doc, "DEMO sheets");
    match store(doc, id, &set) {
        Ok(()) => Vec::new(),
        Err(e) => vec![format!("Demo sheets: {e}")],
    }
}

/// The module for the `sheets` element in the application: the crate's module with its page
/// outlines taken out of the drawing, so that a sheet set does not draw its sheets over the model
/// in model space (they belong on the sheets, not in the model). Its derived values and messages
/// are kept.
#[derive(Debug, Clone, Copy, Default)]
pub struct QuietSheetsModule;

impl Module for QuietSheetsModule {
    fn category(&self) -> &str {
        SHEETS_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Drawing sheets"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let mut g = afrobim_sheets::SheetsModule.generate(element)?;
        g.drawing.primitives.clear();
        g.drawing.extents = Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(1.0, 1.0) };
        Ok(g)
    }
}

/// Registers the project information and sheet-set modules.
pub fn register(modules: &mut ModuleRegistry) {
    modules.register(Box::new(afrobim_sheets::ProjectInfoModule));
    modules.register(Box::new(QuietSheetsModule));
}

/// A detail of one element (a sign face), as the view stores it: the scale 1:`n`, or fitted when
/// `n` is `None`.
pub fn sign_face_detail(name: &str, scale: Option<f64>) -> DetailView {
    let mut d = DetailView::element(name, Scale(scale.unwrap_or(1.0)), &format!("{name}"));
    d.fit = scale.is_none();
    d
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// The registry the sheets are composed with in tests: alignments, markings, the road modules,
    /// coordination and the programme, estimate and progress records.
    pub fn registry() -> ModuleRegistry {
        let mut modules = ModuleRegistry::default();
        crate::alignment_glue::register(&mut modules);
        crate::markings_glue::register(&mut modules);
        crate::modules_glue::register(&mut modules);
        crate::coordination_glue::register(&mut modules);
        modules.register(Box::new(afrobim_schedule::ScheduleModule::default()));
        modules.register(Box::new(afrobim_cost::EstimateModule));
        modules.register(Box::new(afrobim_progress::ProgressModule));
        register(&mut modules);
        modules
    }

    /// The demo document: the corridor test alignment with the demo records and the demo sheets.
    pub fn demo_doc() -> Document {
        let mut doc = Document::default();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/corridor.xml");
        crate::alignment_glue::import_landxml(&mut doc, &path).expect("the corridor test file imports");
        assert!(crate::road_glue::demo(&mut doc).is_empty());
        assert!(crate::demo_glue::add_records(&mut doc).is_empty());
        assert!(demo(&mut doc).is_empty());
        doc
    }

    pub fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("afrobim-sheets-glue-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use afrobim_sheets::{project_info, ProjectInfo};

    #[test]
    fn time_text_is_utc_date_and_minute() {
        assert_eq!(time_text(0), "1970-01-01 00:00 UTC");
        // 1 000 000 000 s is 2001-09-09 01:46:40 UTC.
        assert_eq!(time_text(1_000_000_000), "2001-09-09 01:46 UTC");
    }

    #[test]
    fn project_information_starts_empty_and_survives_save_and_open() {
        // A new document says nothing: every field starts empty.
        let blank = project_info(&Document::default());
        assert_eq!(blank, ProjectInfo::default());
        assert!(blank.project.is_empty() && blank.client.is_empty() && blank.contract_number.is_empty() && blank.revisions.is_empty() && blank.user_fields.is_empty());
        // Entered, saved to a file and opened again.
        let mut doc = Document::default();
        let info = demo_project_info();
        set_project_info(&mut doc, &info);
        let path = scratch("project-info.afrobim");
        doc.save(&path).unwrap();
        let back = Document::load(&path).unwrap();
        assert_eq!(project_info(&back), info);
        assert!(project_info(&back).project.starts_with("DEMO"));
        // Setting it again replaces it: one element only.
        let mut changed = info.clone();
        changed.client = "DEMO other client".into();
        set_project_info(&mut doc, &changed);
        assert_eq!(doc.elements().iter().filter(|e| e.category == afrobim_sheets::PROJECT_INFO_CATEGORY).count(), 1);
    }

    #[test]
    fn demo_project_information_says_demo_in_every_field() {
        let i = demo_project_info();
        for text in [&i.project, &i.client, &i.contract_number, &i.drawn_by, &i.checked_by, &i.approved_by, &i.date, &i.logo_text] {
            assert!(text.contains("DEMO") || text == &i.drawn_by || text == &i.checked_by, "{text}");
        }
        assert!(i.revisions.iter().all(|r| r.description.contains("DEMO")));
    }

    #[test]
    fn a_sheet_set_survives_save_and_open_and_stores_feed_names_only() {
        let doc = demo_doc();
        let (id, name) = sets(&doc).into_iter().next().unwrap();
        assert_eq!(name, "DEMO sheets");
        let set = load_sheet_set(&doc, id).unwrap();
        assert_eq!(set.sources.len(), 3);
        // The schedule holds a feed name and no data.
        match &set.sources[2] {
            SheetSource::TableSeries(t) => match &t.template.views[0].kind {
                ViewKind::Table(table) => assert!(table.feed.as_deref() == Some(FEED_ACTIVITIES) && table.rows.is_empty() && table.headers.is_empty()),
                _ => panic!("not a table"),
            },
            _ => panic!("not a table series"),
        }
        // Saved and opened again, the set is the same.
        let path = scratch("sheet-set.afrobim");
        doc.save(&path).unwrap();
        let back = Document::load(&path).unwrap();
        let (bid, _) = sets(&back).into_iter().next().unwrap();
        let again = load_sheet_set(&back, bid).unwrap();
        // The file's decimal text can differ from the stored float in its last digit, so the first
        // round trip is compared piece by piece and the next one exactly.
        assert!(again.numbering == set.numbering && again.styles == set.styles && again.sources.len() == set.sources.len());
        for (a, b) in again.sources.iter().zip(&set.sources) {
            assert!(a.template().title == b.template().title && a.template().number_prefix == b.template().number_prefix && a.template().page == b.template().page);
        }
        if let (SheetSource::PlanSeries(a), SheetSource::PlanSeries(b)) = (&again.sources[0], &set.sources[0]) {
            assert!((a.length_m - b.length_m).abs() < 1e-9 && a.overlap_m == b.overlap_m);
        }
        let path2 = scratch("sheet-set-2.afrobim");
        back.save(&path2).unwrap();
        let back2 = Document::load(&path2).unwrap();
        assert!(load_sheet_set(&back2, sets(&back2)[0].0).unwrap() == again);
        // A change made to the working copy and stored comes back the same.
        let mut doc2 = back;
        let mut edited = set.clone();
        edited.numbering.prefix = "ALT-".into();
        edited.sources.remove(1);
        store(&mut doc2, bid, &edited).unwrap();
        assert!(load_sheet_set(&doc2, bid).unwrap() == edited);
    }

    #[test]
    fn the_demo_set_composes_to_six_sheets_and_a_pdf_with_six_pages() {
        let doc = demo_doc();
        let registry = registry();
        let (id, _) = sets(&doc).into_iter().next().unwrap();
        let live = Live::from_doc(&doc, &registry, None);
        let c = compose_stored(&doc, &registry, id, &live).unwrap();
        // Hand count. The plan series covers the alignment, internal stations 1000 to 1914.159, in
        // sheets of 300 m overlapping 30 m: starts step by 270 m, at 1000, 1270, 1540 and 1810, the
        // last one running to the end: four sheets. One profile and one activity table (7 rows, which
        // fit one A3 sheet): six in all.
        assert_eq!(c.composed.sheets.len(), 6, "{:?}", c.composed.sheets.iter().map(|s| (&s.number, &s.title)).collect::<Vec<_>>());
        let numbers: Vec<&str> = c.composed.sheets.iter().map(|s| s.number.as_str()).collect();
        assert_eq!(numbers, ["DEMO-PL-001", "DEMO-PL-002", "DEMO-PL-003", "DEMO-PL-004", "DEMO-PR-001", "DEMO-SC-001"]);
        // The composer had nothing to warn about, and the data feeds were all there.
        assert!(c.composed.warnings.is_empty(), "{:?}", c.composed.warnings);
        assert!(c.feed_notes.is_empty(), "{:?}", c.feed_notes);
        // The activity table is the programme's: seven activities, A20 to A80.
        let texts: Vec<String> = c.composed.sheets[5].texts().map(|t| t.text.clone()).collect();
        assert!(texts.iter().any(|t| t == "A20") && texts.iter().any(|t| t == "A80"), "{texts:?}");
        // The profile is annotated: the PVI at internal 1200 (displayed 1+200) at level 104.
        let profile: Vec<String> = c.composed.sheets[4].texts().map(|t| t.text.clone()).collect();
        assert!(profile.iter().any(|t| t.starts_with("PVI 1+200.000   EL 104.000")), "{profile:?}");
        // Every sheet carries the DEMO project information in its title block.
        for s in &c.composed.sheets {
            assert!(s.texts().any(|t| t.text.starts_with("DEMO: made-up road project")), "{}", s.number);
        }
        // The PDF has one page per sheet (and one media box each).
        let pdf = c.composed.pdf(&c.set_name);
        let text: String = pdf.iter().map(|&b| if b < 128 { b as char } else { '?' }).collect();
        assert_eq!(text.matches("/MediaBox [0 0 ").count(), 6);
        // The headless export writes it, and one DXF per sheet beside it.
        let dir = scratch("export-set");
        let line = export_headless(&c, &dir).unwrap();
        assert!(line.contains("6 DXF"), "{line}");
        assert!(dir.join("sheets.pdf").exists() && dir.join("DEMO-PL-001.dxf").exists() && dir.join("DEMO-SC-001.dxf").exists());
        let one = scratch("one.dxf");
        export_headless(&c, &one).unwrap();
        assert!(std::fs::metadata(&one).unwrap().len() > 1000);
        let pdf_path = scratch("one.pdf");
        export_headless(&c, &pdf_path).unwrap();
        if let Ok(out) = std::process::Command::new("pdfinfo").arg(&pdf_path).output() {
            if out.status.success() {
                assert!(String::from_utf8_lossy(&out.stdout).contains("Pages:           6"));
            }
        }
    }

    #[test]
    fn sheets_regenerate_from_the_live_model_each_time() {
        // Rename an activity: the next compose shows the new name, and the stored set is unchanged.
        let mut doc = demo_doc();
        let registry = registry();
        let (id, _) = sets(&doc).into_iter().next().unwrap();
        let before = compose_stored(&doc, &registry, id, &Live::from_doc(&doc, &registry, None)).unwrap();
        let (pid, mut p) = schedule_glue::load(&doc).unwrap();
        p.activity_mut("A20").unwrap().name = "Earthworks, renamed after the first compose".into();
        schedule_glue::store(&mut doc, Some(pid), &p).unwrap();
        let after = compose_stored(&doc, &registry, id, &Live::from_doc(&doc, &registry, None)).unwrap();
        let has = |c: &Composed, needle: &str| c.composed.sheets[5].texts().any(|t| t.text.contains(needle));
        assert!(!has(&before, "renamed after") && has(&after, "renamed after"));
        assert_eq!(load_sheet_set(&doc, id).unwrap().sources.len(), 3);
    }

    #[test]
    fn a_feed_with_no_data_says_so_on_the_sheet_and_in_the_notes() {
        // A document with an alignment but no programme: the activity table is a one-line note.
        let mut doc = Document::default();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/corridor.xml");
        crate::alignment_glue::import_landxml(&mut doc, &path).unwrap();
        assert!(demo(&mut doc).is_empty());
        let registry = registry();
        let (id, _) = sets(&doc).into_iter().next().unwrap();
        let c = compose_stored(&doc, &registry, id, &Live::from_doc(&doc, &registry, None)).unwrap();
        assert!(c.feed_notes.iter().any(|n| n.contains("There is no programme")), "{:?}", c.feed_notes);
        let last = c.composed.sheets.last().unwrap();
        assert!(last.texts().any(|t| t.text.contains("There is no programme")));
    }
}
