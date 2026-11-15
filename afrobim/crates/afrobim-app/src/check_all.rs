//! The model-wide check (`CHECKALL`): every check an engine has, run on every element it applies to,
//! gathered into one report.
//!
//! Each row is one check of one element with a status, a one-line reason and the calculation lines
//! the engine returned, **unchanged**: every warning an engine emits (a superseded edition, a
//! secondary source, a limit of the method) is in the lines of the row it belongs to, as the engine
//! wrote it. The report adds nothing of its own to the lines but a header and, for the culvert
//! freeboard, one line it marks as its own.
//!
//! A check needs inputs that only the engineer has. They are read from the properties the
//! single-element CHECK dialogs keep on the element (`check_inputs`). An element with none reports
//! "input needed" and names the dialog to open. Nothing is assumed in their place.
//!
//! No check is invented: the engines expose the checks below and no others. Categories with no
//! check are listed with the reason, so the report is an honest map of what is and is not checked.
//! A pass means that the engine's own lines passed on the inputs stored; it is a calculation for
//! an engineer to check, not a certification.

use std::collections::BTreeSet;

use afrobim_drainage::design::{mowt, DesignBasis};
use afrobim_drainage::{DrainageError, PipeCulvertDefinition, SideDrainDefinition};
use afrobim_gantry as gantry;
use afrobim_kernel::{Document, Element, ElementId, Note, Severity};
use afrobim_lighting as lighting;
use afrobim_lighting::design::Outcome as LightOutcome;
use afrobim_pavement as pavement;
use afrobim_restraint as restraint;

use crate::check_inputs::{self as inputs, CulvertInputs, DrainInputs, RationalInputs};
use crate::{alignment_glue, lighting_inputs, road_glue, signs_glue, supports_glue};

/// The result of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Status {
    /// The engine's own lines passed on the stored inputs.
    Passes,
    /// A line failed.
    Fails,
    /// The model differs from what the stored inputs call for, or from a reference that does not govern.
    Differs,
    /// An input is missing: the reason names it and the dialog to open.
    InputNeeded,
    /// The check could not be made, and the reason says why.
    NotChecked,
}

impl Status {
    pub const ALL: [Status; 5] = [Status::Passes, Status::Fails, Status::Differs, Status::InputNeeded, Status::NotChecked];

    pub fn label(self) -> &'static str {
        match self {
            Status::Passes => "Passes",
            Status::Fails => "Fails",
            Status::Differs => "Differs",
            Status::InputNeeded => "Input needed",
            Status::NotChecked => "Not checked",
        }
    }
}

/// One check of one element.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub element: ElementId,
    pub mark: String,
    /// The category as the model shows it (the module's display name).
    pub category: String,
    pub check: String,
    pub status: Status,
    /// One line: why the status is what it is, or what to enter and where.
    pub reason: String,
    /// The calculation, line by line, as the engine returned it.
    pub lines: Vec<String>,
    /// The messages the element's module raised, as it wrote them (severity, code, message).
    pub notes: Vec<String>,
}

/// The whole report.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    pub rows: Vec<Row>,
    /// Categories present in the model that have no check, with the reason.
    pub no_check: Vec<(String, String)>,
}

impl Report {
    pub fn count(&self, status: Status) -> usize {
        self.rows.iter().filter(|r| r.status == status).count()
    }

    /// The categories that have rows, sorted.
    pub fn categories(&self) -> Vec<String> {
        let set: BTreeSet<&String> = self.rows.iter().map(|r| &r.category).collect();
        set.into_iter().cloned().collect()
    }

    /// The indices of the rows that pass the filters. `statuses` is indexed as [`Status::ALL`];
    /// `category` None shows every category.
    pub fn visible(&self, statuses: &[bool; 5], category: Option<&str>) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| statuses[Status::ALL.iter().position(|s| *s == r.status).unwrap_or(0)] && category.is_none_or(|c| c == r.category))
            .map(|(i, _)| i)
            .collect()
    }

    /// One CSV row per check: mark, category, check, status, reason, and the calculation lines
    /// joined by line breaks inside one quoted field.
    pub fn to_csv(&self) -> String {
        let mut out = String::from("Mark,Category,Check,Status,Reason,Calculation\n");
        for r in &self.rows {
            let fields = [r.mark.as_str(), r.category.as_str(), r.check.as_str(), r.status.label(), r.reason.as_str(), &r.lines.join("\n")];
            out.push_str(&fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
            out.push('\n');
        }
        out
    }

    /// The calculation pack: plain text, one section per check with its calculation lines as the
    /// engine wrote them, then the categories with no check.
    pub fn to_pack(&self, title: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!("CALCULATION PACK: {title}\n"));
        out.push_str(&"=".repeat(60));
        out.push('\n');
        out.push_str("Every calculation line below is as the engine returned it, with its warnings. A pass means that the engine's lines passed on the inputs stored with the element. These are calculations for an engineer to check, not a certified design.\n\n");
        out.push_str(&format!(
            "Summary: {} check(s): {} pass, {} fail, {} differ, {} need input, {} not checked.\n\n",
            self.rows.len(),
            self.count(Status::Passes),
            self.count(Status::Fails),
            self.count(Status::Differs),
            self.count(Status::InputNeeded),
            self.count(Status::NotChecked)
        ));
        for r in &self.rows {
            out.push_str(&format!("{} ({}): {}\n", r.mark, r.category, r.check));
            out.push_str(&format!("  Status: {}. {}\n", r.status.label(), r.reason));
            for l in &r.lines {
                out.push_str(&format!("  {l}\n"));
            }
            if !r.notes.is_empty() {
                out.push_str("  Messages from the model:\n");
                for n in &r.notes {
                    out.push_str(&format!("    {n}\n"));
                }
            }
            out.push('\n');
        }
        if !self.no_check.is_empty() {
            out.push_str("Categories with no check\n");
            for (category, why) in &self.no_check {
                out.push_str(&format!("  {category}: {why}\n"));
            }
        }
        out
    }
}

fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// The messages of a module, as it wrote them.
fn note_lines(notes: &[Note]) -> Vec<String> {
    notes
        .iter()
        .map(|n| {
            let severity = match n.severity {
                Severity::Info => "Info",
                Severity::Warning => "Warning",
                Severity::Error => "Error",
            };
            format!("[{severity}] {}: {}", n.code, n.message)
        })
        .collect()
}

/// What the report asks of the app: the messages the module of an element raised (or why it could
/// not generate), and the display name of a category.
pub struct Sources<'a> {
    pub notes_of: &'a dyn Fn(ElementId) -> Option<Result<Vec<Note>, String>>,
    pub label_of: &'a dyn Fn(&str) -> String,
    /// The obstruction scene and legend sizes for the sign visibility check; `None` when the
    /// caller did not build them (the visibility row then says it was not checked).
    pub vis: Option<&'a crate::visibility_ui::VisSource>,
}

struct Maker<'a> {
    src: &'a Sources<'a>,
    rows: Vec<Row>,
    noted: BTreeSet<ElementId>,
}

impl Maker<'_> {
    fn push(&mut self, e: &Element, check: &str, status: Status, reason: impl Into<String>, lines: Vec<String>) {
        // The module's messages are attached to the first row of the element.
        let notes = if self.noted.insert(e.id) {
            match (self.src.notes_of)(e.id) {
                Some(Ok(n)) => note_lines(&n),
                Some(Err(why)) => vec![format!("[Error] the element could not be generated: {why}")],
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };
        self.rows.push(Row { element: e.id, mark: e.name.clone(), category: (self.src.label_of)(&e.category), check: check.to_string(), status, reason: reason.into(), lines, notes });
    }
}

/// The checks of one element, in the order the report lists them.
fn check_element(doc: &Document, e: &Element, m: &mut Maker) {
    match e.category.as_str() {
        c if c == signs_glue::CATEGORY => sign(doc, e, m),
        c if c == afrobim_drainage::DRAIN_CATEGORY => drain(doc, e, m),
        c if c == afrobim_drainage::CULVERT_CATEGORY => culvert(doc, e, m),
        c if c == gantry::GANTRY_CATEGORY => gantry_check(doc, e, m),
        c if c == lighting::CATEGORY => lighting_check(doc, e, m),
        c if c == restraint::CATEGORY => restraint_check(doc, e, m),
        c if c == pavement::PAVEMENT_CATEGORY => pavement_check(doc, e, m),
        c if c == afrobim_markings::CATEGORY => rules(e, "Marking rules (Traffic Signs Manual Chapter 5 checks of the marking engine)", m),
        c if c == alignment_glue::CATEGORY => alignment_check(doc, e, m),
        _ => {}
    }
}

/// The rows of one element's checks, as the single-element dialogs use them.
pub fn run_one(doc: &Document, id: ElementId, src: &Sources) -> Vec<Row> {
    let mut m = Maker { src, rows: Vec::new(), noted: BTreeSet::new() };
    if let Some(e) = doc.find(id) {
        check_element(doc, e, &mut m);
    }
    m.rows
}

/// Runs every check on every element of the document.
pub fn run_all(doc: &Document, src: &Sources) -> Report {
    let mut m = Maker { src, rows: Vec::new(), noted: BTreeSet::new() };
    for e in doc.elements() {
        check_element(doc, e, &mut m);
    }
    let checked: BTreeSet<String> = m.rows.iter().map(|r| r.category.clone()).collect();
    let mut no_check: Vec<(String, String)> = Vec::new();
    for e in doc.elements() {
        let label = (src.label_of)(&e.category);
        if checked.contains(&label) || no_check.iter().any(|(c, _)| *c == label) {
            continue;
        }
        no_check.push((label, no_check_reason(&e.category)));
    }
    no_check.sort();
    Report { rows: m.rows, no_check }
}

/// Why a category has no check.
fn no_check_reason(category: &str) -> String {
    match category {
        c if c == alignment_glue::CATEGORY => "an imported alignment is geometry with no PIs to check; a designed alignment is checked against your limits table (ALIGNCHECK)".into(),
        c if c == afrobim_design::DESIGN_CATEGORY => "this holds the PIs and PVIs of a designed alignment; the alignment element it built carries the check".into(),
        c if c == afrobim_design::OPTIONS_CATEGORY => "an option set is a comparison of designs, not a check".into(),
        c if c == afrobim_corridor::CORRIDOR_CATEGORY => "the corridor crate builds sections and earthworks volumes and holds no design check".into(),
        c if c == afrobim_corridor::SURFACE_CATEGORY => "a ground surface is survey data; there is nothing to check".into(),
        c if c == supports_glue::POST_CATEGORY || c == supports_glue::FOUNDATION_CATEGORY => "posts and foundations are checked with the sign they support (sign support design)".into(),
        c if c == afrobim_schedule::SCHEDULE_CATEGORY => "the programme has its own findings: run SCHEDULE".into(),
        c if c == afrobim_cost::ESTIMATE_CATEGORY => "the estimate has its own warnings and audit trail: see ESTIMATE".into(),
        c if c == crate::coordination_glue::CATEGORY => "clash tests are run with CLASH, not here".into(),
        c if c == afrobim_progress::PROGRESS_CATEGORY => "site progress is a record, not a check".into(),
        _ => "the engines hold no check for this category".into(),
    }
}

// ─── designed alignments ────────────────────────────────────────────────────────────────

/// The geometric checks of a designed alignment against the limits table whose path and design
/// speed row were stored on it by ALIGNCHECK. An imported alignment has no design and no row.
fn alignment_check(doc: &Document, e: &Element, m: &mut Maker) {
    use crate::design_glue::{check_for_report, report_lines, AlignmentCheck};
    use afrobim_design::CheckStatus;
    let check = "Alignment geometry against the limits table (ALIGNCHECK)";
    match check_for_report(doc, e.id) {
        None => {}
        Some(AlignmentCheck::InputNeeded(why)) => m.push(e, check, Status::InputNeeded, why, Vec::new()),
        Some(AlignmentCheck::NotChecked(why)) => m.push(e, check, Status::NotChecked, why, Vec::new()),
        Some(AlignmentCheck::Failed(why)) => m.push(e, check, Status::NotChecked, format!("the design could not be read: {why}"), Vec::new()),
        Some(AlignmentCheck::Report(r)) => {
            let (pass, fail, none) = (r.count(CheckStatus::Pass), r.count(CheckStatus::Fail), r.count(CheckStatus::NotChecked));
            let lines = report_lines(&r);
            if fail > 0 {
                m.push(e, check, Status::Fails, format!("{fail} check(s) fail against the limits of row {}; {pass} pass, {none} have no limit in the table and were not checked", r.row_label), lines);
            } else if pass == 0 {
                m.push(e, check, Status::NotChecked, format!("none of the {none} check(s) has a limit in the table, so nothing was checked"), lines);
            } else {
                m.push(e, check, Status::Passes, format!("{pass} check(s) pass against row {}; {none} have no limit in the table and were not checked (a pass is not a certification)", r.row_label), lines);
            }
        }
    }
}

// ─── signs ──────────────────────────────────────────────────────────────────────────────

/// A rule check taken from the notes the element's engine raises when it generates it: an error
/// fails, a warning differs from a rule's preference, and a clean run passes.
fn rules(e: &Element, check: &str, m: &mut Maker) {
    match (m.src.notes_of)(e.id) {
        Some(Ok(notes)) => {
            let errors = notes.iter().filter(|n| n.severity == Severity::Error).count();
            let warnings = notes.iter().filter(|n| n.severity == Severity::Warning).count();
            let (status, reason) = if errors > 0 {
                (Status::Fails, format!("{errors} error(s) and {warnings} warning(s) raised by the rules"))
            } else if warnings > 0 {
                (Status::Differs, format!("{warnings} warning(s) raised by the rules; no error"))
            } else {
                (Status::Passes, "no error or warning raised by the rules".to_string())
            };
            m.push(e, check, status, reason, note_lines(&notes));
        }
        Some(Err(why)) => m.push(e, check, Status::Fails, format!("the element could not be generated: {why}"), vec![why]),
        None => m.push(e, check, Status::NotChecked, "the element has not been generated, so its rules were not run", Vec::new()),
    }
}

fn sign(doc: &Document, e: &Element, m: &mut Maker) {
    rules(e, "Sign layout rules (the rule pack's checks of the sign engine)", m);
    visibility(doc, e, m);
    let supports = afrobim_supports::supports_of(doc, e.id);
    if supports.is_empty() || crate::modules_glue::on_gantry(doc, e.id) {
        return;
    }
    let check = "Sign support design (post and foundation)";
    let Some(stored) = inputs::load::<supports_glue::StoredSupportDesign>(doc, e.id, inputs::KEY_SUPPORT) else {
        m.push(e, check, Status::InputNeeded, "no design inputs are stored for this sign: select it and run DESIGN, and enter the mounting height, the wind basis and the ground values", Vec::new());
        return;
    };
    let req = match stored.request() {
        Ok(r) => r,
        Err(why) => return m.push(e, check, Status::InputNeeded, format!("the stored design inputs cannot be read ({why}): run DESIGN again"), Vec::new()),
    };
    match supports_glue::solve(doc, e.id, &req) {
        Err(supports_glue::DesignFailure::Input(why)) => m.push(e, check, Status::InputNeeded, format!("{why}: change the entries in DESIGN"), Vec::new()),
        Err(supports_glue::DesignFailure::NoDesign(why)) => m.push(e, check, Status::Fails, why.clone(), vec![why]),
        Ok(solved) => {
            let lines = solved.result.calculation(&solved.wind_source);
            let designed = solved.result.post.section;
            let modelled = modelled_section(doc, &supports);
            let (status, reason) = match modelled {
                Some((od, wall)) if (od - designed.outside_diameter_mm).abs() < 0.5 && (wall - designed.wall_mm).abs() < 0.05 => {
                    (Status::Passes, format!("the stored inputs size a CHS {} x {} post and the model has the same", fmt(od), fmt(wall)))
                }
                Some((od, wall)) => (
                    Status::Differs,
                    format!("the stored inputs size a CHS {} x {} post; the model has CHS {} x {}: run DESIGN to replace the supports", fmt(designed.outside_diameter_mm), fmt(designed.wall_mm), fmt(od), fmt(wall)),
                ),
                None => (Status::Differs, format!("the stored inputs size a CHS {} x {} post; the model's posts are not circular hollow sections", fmt(designed.outside_diameter_mm), fmt(designed.wall_mm))),
            };
            m.push(e, check, status, reason, lines);
        }
    }
}

/// Sign visibility along the alignment (`VISIBILITY`): the engine's check lines on the inputs stored
/// with the sign. A sign that is not placed on an alignment has nothing to sweep and no row.
fn visibility(doc: &Document, e: &Element, m: &mut Maker) {
    use crate::visibility_ui as vis;
    use afrobim_visibility::Status as V;
    if e.site.is_none() {
        return;
    }
    let check = "Sign visibility along the alignment (VISIBILITY, an analysis aid)";
    if afrobim_visibility::load(doc, e.id).is_none() {
        return m.push(e, check, Status::InputNeeded, "no visibility inputs are stored for this sign: select it and run VISIBILITY, and enter the driver's lateral position, the traffic side and the rest", Vec::new());
    }
    if !vis::eye_height_accepted(doc, e.id) {
        return m.push(e, check, Status::InputNeeded, "the stored inputs use the engine's default driver's eye height, which has not been confirmed for this sign: open VISIBILITY for it and tick to accept the value or enter your own", Vec::new());
    }
    let Some(src) = m.src.vis else {
        return m.push(e, check, Status::NotChecked, "the obstruction scene was not built for this report, so the visibility was not analysed", Vec::new());
    };
    let a = vis::analyse(doc, src, e.id);
    if let Some(why) = &a.skipped {
        return m.push(e, check, Status::NotChecked, format!("not analysed: {why}"), Vec::new());
    }
    let mut lines: Vec<String> = a.lines.iter().map(|l| l.text()).collect();
    if let Some(d) = doc.find(e.id).and_then(|x| x.properties.get(crate::demo_glue::KEY_DEMO_VISIBILITY)) {
        lines.insert(0, format!("DEMO inputs: {d}"));
    }
    if let Some(l) = vis::band_line(doc, e.id) {
        lines.insert(0, l);
    }
    let (ok, fail, need, differ) = (a.count(V::Ok), a.count(V::Fails), a.count(V::InputNeeded), a.count(V::Differs));
    let status = match a.overall() {
        V::Fails => Status::Fails,
        V::Differs => Status::Differs,
        V::InputNeeded => Status::InputNeeded,
        V::Ok => Status::Passes,
        V::Information => Status::NotChecked,
    };
    let clear = a.clear_full().map(|(d, lower)| format!("; continuous clear visibility of the whole face {}{d:.1} m", if lower { "at least " } else { "" })).unwrap_or_default();
    m.push(e, check, status, format!("{ok} OK, {fail} fail, {differ} differ, {need} need input{clear}. The check compares and cites; the engineer decides"), lines);
}

/// The circular hollow section of the first post among the supports, (outside diameter, wall), mm.
fn modelled_section(doc: &Document, supports: &[ElementId]) -> Option<(f64, f64)> {
    let post = supports.iter().filter_map(|id| doc.find(*id)).find(|e| e.category == supports_glue::POST_CATEGORY)?;
    let def: afrobim_supports::PostDefinition = serde_json::from_value(post.data.clone()).ok()?;
    match def.section {
        afrobim_supports::Section::Chs { outside_diameter_mm, wall_mm } => Some((outside_diameter_mm, wall_mm)),
        _ => None,
    }
}

fn fmt(v: f64) -> String {
    let s = format!("{v:.1}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

// ─── drainage ───────────────────────────────────────────────────────────────────────────

const PLACEHOLDER: &str = "the dimensions are still placeholders, not design values: set them in Properties first";

fn drain(doc: &Document, e: &Element, m: &mut Maker) {
    let check = "Side drain capacity";
    let def = match SideDrainDefinition::from_value(&e.data) {
        Ok(d) => d,
        Err(why) => return m.push(e, check, Status::NotChecked, why.to_string(), Vec::new()),
    };
    if def.placeholder {
        return m.push(e, check, Status::InputNeeded, PLACEHOLDER, Vec::new());
    }
    let Some(stored) = inputs::load::<DrainInputs>(doc, e.id, inputs::KEY_DRAIN) else {
        return m.push(e, check, Status::InputNeeded, "no design flow is stored for this drain: select it and run CHECK, then enter the design flow (or open RATIONAL)", Vec::new());
    };
    let rational: Option<RationalInputs> = inputs::load(doc, e.id, inputs::KEY_RATIONAL);
    let params = match stored.params(rational.as_ref()) {
        Ok(p) => p,
        Err(why) => return m.push(e, check, Status::InputNeeded, format!("{why}: run CHECK on this drain"), Vec::new()),
    };
    let basis = if params.dmrb { DesignBasis::Dmrb } else { DesignBasis::Mowt };
    match def.check_capacity(basis, params.flow_m3_s, params.freeboard_m) {
        Err(DrainageError::InvalidInput(why)) => m.push(e, check, Status::InputNeeded, format!("{why}: set it in Properties"), Vec::new()),
        Err(other) => m.push(e, check, Status::NotChecked, other.to_string(), Vec::new()),
        Ok(c) => {
            let mut lines = params.rational_lines.clone();
            lines.extend(c.calculation());
            let (status, reason) = if c.passes() {
                (Status::Passes, format!("Q = {:.3} m3/s is carried: normal depth {:.3} m in a drain {:.3} m deep, freeboard {:.3} m", params.flow_m3_s, c.normal_depth_m, c.input.depth_m, c.freeboard_m))
            } else {
                let mut why = Vec::new();
                if !c.capacity_ok {
                    why.push(format!("the drain overtops at Q = {:.3} m3/s (normal depth {:.3} m, depth {:.3} m)", params.flow_m3_s, c.normal_depth_m, c.input.depth_m));
                }
                if c.velocity_ok == Some(false) {
                    why.push(format!("the velocity {:.2} m/s is above the permissible velocity", c.velocity_m_s));
                }
                if c.freeboard_ok == Some(false) {
                    why.push(format!("the freeboard {:.3} m is below the required freeboard", c.freeboard_m));
                }
                (Status::Fails, why.join("; "))
            };
            m.push(e, check, status, reason, lines);
        }
    }
}

fn culvert(doc: &Document, e: &Element, m: &mut Maker) {
    let def = match PipeCulvertDefinition::from_value(&e.data) {
        Ok(d) => d,
        Err(why) => return m.push(e, "Culvert layout limits", Status::NotChecked, why.to_string(), Vec::new()),
    };
    let stored: Option<CulvertInputs> = inputs::load(doc, e.id, inputs::KEY_CULVERT);
    let rational: Option<RationalInputs> = inputs::load(doc, e.id, inputs::KEY_RATIONAL);
    if def.placeholder {
        m.push(e, "Culvert layout limits", Status::InputNeeded, PLACEHOLDER, Vec::new());
        m.push(e, "Culvert outlet control (MoWT DDM 9.4.2)", Status::InputNeeded, PLACEHOLDER, Vec::new());
        return;
    }
    // Layout limits: they need no entry but the basis, which is the engineer's choice (stored with the culvert's other inputs).
    let dmrb = stored.as_ref().is_some_and(|s| s.dmrb);
    let basis = if dmrb { DesignBasis::Dmrb } else { DesignBasis::Mowt };
    let notes = def.layout_notes(basis);
    let mut lines = vec![format!("Layout limits of the {} basis. {}", basis.name(), basis.basis_line())];
    lines.extend(note_lines(&notes));
    let warnings = notes.iter().filter(|n| n.severity == Severity::Warning).count();
    let basis_note = if stored.is_some() { String::new() } else { " (MoWT basis: choose another in CHECK)".to_string() };
    if warnings > 0 {
        m.push(e, "Culvert layout limits", Status::Fails, format!("{warnings} layout limit(s) of the {} basis are not met{basis_note}", basis.name()), lines);
    } else {
        m.push(e, "Culvert layout limits", Status::Passes, format!("no layout limit of the {} basis is breached{basis_note}", basis.name()), lines);
    }

    // Outlet control.
    let check = "Culvert outlet control (MoWT DDM 9.4.2)";
    let Some(stored) = stored else {
        return m.push(e, check, Status::InputNeeded, "no flow, Manning's n, entrance loss or tailwater is stored for this culvert: select it and run CHECK and enter them", Vec::new());
    };
    let p = match stored.params(rational.as_ref()) {
        Ok(p) => p,
        Err(why) => return m.push(e, check, Status::InputNeeded, format!("{why}: run CHECK on this culvert"), Vec::new()),
    };
    match def.outlet_control(p.flow_m3_s, p.manning_n, p.entrance_ke, p.tailwater_m) {
        Err(DrainageError::InvalidInput(why)) => m.push(e, check, Status::InputNeeded, why, Vec::new()),
        Err(other) => m.push(e, check, Status::NotChecked, other.to_string(), Vec::new()),
        Ok(oc) => {
            let mut lines = p.rational_lines.clone();
            lines.extend(oc.calculation());
            let mut fails = Vec::new();
            if oc.headwater_ratio > mowt::HEADWATER_RATIO_LIMIT {
                fails.push(format!("HWoi/D = {:.2} exceeds the limit of {} (DDM Section 9.3)", oc.headwater_ratio, mowt::HEADWATER_RATIO_LIMIT));
            }
            if let (Some(level), Some(major)) = (p.shoulder_level_m, p.major_road) {
                let need = if major { mowt::CULVERT_FREEBOARD_MAJOR_ROAD_M } else { mowt::CULVERT_FREEBOARD_MINOR_ROAD_M };
                let free = oc.freeboard_m(def.invert_level_inlet_m, level);
                let ok = free >= need;
                lines.push(format!(
                    "Freeboard (this report's arithmetic on DDM Section 9.6) = shoulder level {level:.3} m less headwater level {:.3} m = {free:.3} m against {need} m for a {} road: {}.",
                    def.invert_level_inlet_m + oc.headwater_m,
                    if major { "major" } else { "minor" },
                    if ok { "OK" } else { "FAILS" }
                ));
                if !ok {
                    fails.push(format!("the freeboard {free:.3} m is below {need} m"));
                }
            }
            if fails.is_empty() {
                m.push(e, check, Status::Passes, format!("outlet control only: HWoi/D = {:.2} within the limit of {}; inlet control is not computed, so this does not size the culvert", oc.headwater_ratio, mowt::HEADWATER_RATIO_LIMIT), lines);
            } else {
                m.push(e, check, Status::Fails, fails.join("; "), lines);
            }
        }
    }
}

// ─── gantry ─────────────────────────────────────────────────────────────────────────────

fn gantry_check(doc: &Document, e: &Element, m: &mut Maker) {
    let check = "Gantry requirement checks (CD 127 headroom and CD 365 requirements)";
    let def = match gantry::definition_of(doc, e.id) {
        Ok(d) => d,
        Err(why) => return m.push(e, check, Status::NotChecked, why.to_string(), Vec::new()),
    };
    if def.placeholder {
        return m.push(e, check, Status::InputNeeded, PLACEHOLDER, Vec::new());
    }
    let alignment = doc.elements().iter().find(|a| a.category == alignment_glue::CATEGORY && a.name == def.alignment).and_then(|a| alignment_glue::alignment_of(doc, a.id));
    match gantry::design::GantryChecks::run(&e.name, &def, alignment.as_ref()) {
        Err(why) => m.push(e, check, Status::NotChecked, why.to_string(), Vec::new()),
        Ok(c) => {
            use gantry::design::Status as G;
            let incomplete = c.lines.iter().filter(|l| l.status == G::Incomplete).count();
            let fails = c.lines.iter().filter(|l| l.status == G::Fails).count();
            let lines = c.calculation();
            let (status, reason) = if fails > 0 {
                (Status::Fails, format!("{fails} requirement line(s) fail"))
            } else if incomplete > 0 {
                (Status::InputNeeded, format!("{incomplete} line(s) are incomplete: an entry they need (for example the SLS deflection from the structural analysis) is not in the gantry's definition"))
            } else {
                (Status::Passes, "no requirement line fails; this is not a structural design".to_string())
            };
            m.push(e, check, status, reason, lines);
        }
    }
}

// ─── lighting ───────────────────────────────────────────────────────────────────────────

fn lighting_check(doc: &Document, e: &Element, m: &mut Maker) {
    let check = "Lighting requirement checks (TD 501 and the class checks)";
    let Some(form) = inputs::load::<lighting_inputs::Form>(doc, e.id, inputs::KEY_LIGHTING) else {
        return m.push(e, check, Status::InputNeeded, "no appraisal, class or photometric results are stored for this lighting: select it and run CHECK and enter what you have", Vec::new());
    };
    let entered = match form.inputs() {
        Ok(i) => i,
        Err(why) => return m.push(e, check, Status::InputNeeded, format!("the stored entries cannot be used ({why}): run CHECK again"), Vec::new()),
    };
    let data = match lighting::lighting_of(doc, e.id) {
        Ok(d) => d,
        Err(why) => return m.push(e, check, Status::NotChecked, why.to_string(), Vec::new()),
    };
    let c = lighting::design::check_lighting(&data, &entered);
    let (ok, fail, need, differ) = (c.count(LightOutcome::Ok), c.count(LightOutcome::Fails), c.count(LightOutcome::InputNeeded), c.count(LightOutcome::Differs));
    let status = if fail > 0 {
        Status::Fails
    } else if need > 0 {
        Status::InputNeeded
    } else if differ > 0 {
        Status::Differs
    } else {
        Status::Passes
    };
    m.push(e, check, status, format!("{ok} OK, {fail} fail, {need} need input, {differ} differ from a reference that does not govern"), c.calculation());
}

// ─── restraint ──────────────────────────────────────────────────────────────────────────

fn restraint_check(doc: &Document, e: &Element, m: &mut Maker) {
    use restraint::design::Status as R;
    let check = "Barrier run requirement checks (CD 377)";
    let Some(speed) = inputs::load::<road_glue::SpeedInput>(doc, e.id, inputs::KEY_RESTRAINT) else {
        return m.push(e, check, Status::InputNeeded, "no speed limit is stored for this run: select it and run CHECK and enter the speed limit", Vec::new());
    };
    match road_glue::check_restraint_design(doc, e.id, speed) {
        Err(why) => m.push(e, check, Status::InputNeeded, why, Vec::new()),
        Ok(d) => {
            let (fails, not_checked, relaxed, review) = (d.count(R::Fails), d.count(R::NotChecked), d.count(R::Relaxation), d.count(R::Review));
            let (status, reason) = if fails > 0 {
                (Status::Fails, format!("{fails} requirement(s) fail"))
            } else if not_checked > 0 {
                (Status::InputNeeded, format!("{not_checked} line(s) are not checked: the dialog takes the speed limit only, and the engine's other inputs (hazard distances, lengths of need, terminal contexts) are not offered in the app"))
            } else if relaxed > 0 {
                (Status::Differs, format!("{relaxed} requirement(s) are met only by a relaxation the standard permits"))
            } else {
                (Status::Passes, format!("no requirement fails; {review} line(s) are for the engineer's review"))
            };
            m.push(e, check, status, reason, d.calculation());
        }
    }
}

// ─── pavement ───────────────────────────────────────────────────────────────────────────

fn pavement_check(doc: &Document, e: &Element, m: &mut Maker) {
    use pavement::design::DesignMethod;
    let check = "Pavement design check against the modelled layers";
    let Some(form) = inputs::load::<road_glue::DesignForm>(doc, e.id, inputs::KEY_PAVEMENT) else {
        return m.push(e, check, Status::InputNeeded, "no design inputs are stored for this pavement: run PAVEDESIGN, choose this pavement to keep the inputs on, and enter the traffic and the subgrade", Vec::new());
    };
    let method = road_glue::METHODS[form.method.min(road_glue::METHODS.len() - 1)];
    let outcome = match form.run() {
        Ok(o) => o,
        Err(why) => return m.push(e, check, Status::InputNeeded, format!("{}: {why}", method.name()), Vec::new()),
    };
    let def = match pavement::PavementDefinition::from_value(&e.data) {
        Ok(d) => d,
        Err(why) => return m.push(e, check, Status::NotChecked, why.to_string(), outcome.lines),
    };
    let Some(designed) = outcome.layers else {
        let why = if method == DesignMethod::DmrbCd226 {
            "this method gives design traffic and, over a bound base, an asphalt thickness, but no layer stack to compare with the modelled layers"
        } else {
            "the method arrives at no layer stack on these inputs (its lines say why)"
        };
        return m.push(e, check, Status::NotChecked, why, outcome.lines);
    };
    let modelled = &def.layers;
    let same_shape = designed.len() == modelled.len() && designed.iter().zip(modelled).all(|(d, l)| d.material == l.material);
    let (status, reason) = if !same_shape {
        (Status::Differs, format!("{} designed layer(s) of {}; the pavement has {} layer(s) of other materials or in another order", designed.len(), method.name(), modelled.len()))
    } else {
        let thin: Vec<String> = designed
            .iter()
            .zip(modelled)
            .filter(|(d, l)| l.thickness_mm + 1e-9 < d.thickness_mm)
            .map(|(d, l)| format!("{} {} mm against {} mm designed", l.name, fmt(l.thickness_mm), fmt(d.thickness_mm)))
            .collect();
        if !thin.is_empty() {
            (Status::Fails, format!("thinner than the method's layers: {}", thin.join("; ")))
        } else if designed.iter().zip(modelled).any(|(d, l)| l.thickness_mm > d.thickness_mm + 1e-9) {
            (Status::Differs, format!("no layer is thinner than {}'s, but at least one is thicker", method.name()))
        } else {
            (Status::Passes, format!("the layers equal those of {}", method.name()))
        }
    };
    m.push(e, check, status, reason, outcome.lines);
}

// ─── tests ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_alignment::{Alignment, Pi};
    use afrobim_kernel::ModuleRegistry;

    // The numbers in these tests are arbitrary test values, not design values.

    fn doc_with_road() -> (Document, ElementId) {
        let mut doc = Document::default();
        let mut a = Alignment::from_pis("MC10", 0.0, &[Pi::point(0.0, 0.0), Pi::point(500.0, 0.0)]).unwrap();
        a.profile = Some(afrobim_alignment::Profile::new("FG", vec![afrobim_alignment::Pvi::point(0.0, 50.0), afrobim_alignment::Pvi::point(500.0, 50.0)]).unwrap());
        let id = alignment_glue::add(&mut doc, &a).unwrap();
        (doc, id)
    }

    fn report(doc: &Document) -> Report {
        let mut modules = ModuleRegistry::default();
        crate::modules_glue::register(&mut modules);
        crate::markings_glue::register(&mut modules);
        supports_glue::register(&mut modules);
        let notes = |id: ElementId| doc.find(id).map(|e| modules.generate(e).map(|g| g.notes));
        let label = |c: &str| c.to_string();
        run_all(doc, &Sources { notes_of: &notes, label_of: &label, vis: None })
    }

    fn real_drain(doc: &mut Document, a: ElementId, mark: &str) -> ElementId {
        // A trapezoid of 1 m bed, 1.5 : 1 slopes and 0.6 m deep, gradient 0.004 and n 0.025 (test values).
        let mut def = afrobim_drainage::SideDrainDefinition::placeholder("MC10", 100.0, 200.0, 5.0);
        def.section = afrobim_drainage::ChannelSection::Trapezoidal { bottom_width_mm: 1000.0, depth_mm: 600.0, left_side_slope: 1.5, right_side_slope: 1.5 };
        def.gradient_m_per_m = 0.004;
        def.manning_n = Some(0.025);
        def.placeholder = false;
        let _ = a;
        afrobim_drainage::add_side_drain(doc, mark, &def).unwrap()
    }

    #[test]
    fn a_designed_alignment_is_checked_against_its_stored_limits_and_an_imported_one_has_no_row() {
        use crate::design_glue as dg;
        let (mut doc, _imported) = doc_with_road();
        let (_, a) = dg::create(&mut doc, "Designed", 0.0, &[(0.0, 1000.0), (400.0, 1000.0), (400.0, 1300.0)]).unwrap();
        let rows = |doc: &Document| report(doc).rows.into_iter().filter(|r| r.check.starts_with("Alignment geometry")).collect::<Vec<_>>();
        // Nothing stored: input needed, naming the dialog. The imported alignment has no row.
        let r = rows(&doc);
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].status, r[0].mark.as_str()), (Status::InputNeeded, "Designed"));
        assert!(r[0].reason.contains("ALIGNCHECK"));
        // A stored path with no limits: not checked.
        dg::store_check(&mut doc, a, &dg::AlignCheckInputs::default());
        assert_eq!(rows(&doc)[0].status, Status::NotChecked);
        // A limits file (arbitrary test numbers): a radius needs entering first, then it fails or passes by the table.
        let d = dg::design_of_alignment(&doc, a).unwrap();
        dg::edit_horizontal(&mut doc, d, dg::HEdit::SetRadius(1, 100.0)).unwrap();
        let dir = std::env::temp_dir().join(format!("afrobim-chk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("limits.csv");
        std::fs::write(&path, "#name,test\n#source,test fixture\nlabel,min_radius_m\nrow,150\n").unwrap();
        dg::store_check(&mut doc, a, &dg::AlignCheckInputs { limits_path: path.display().to_string(), row_label: "row".into() });
        assert_eq!(rows(&doc)[0].status, Status::Fails);
        std::fs::write(&path, "#name,test\n#source,test fixture\nlabel,min_radius_m\nrow,50\n").unwrap();
        assert_eq!(rows(&doc)[0].status, Status::Passes);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_drain_passes_fails_or_needs_input_by_its_stored_flow() {
        let (mut doc, a) = doc_with_road();
        let small = real_drain(&mut doc, a, "SD-S");
        let large = real_drain(&mut doc, a, "SD-L");
        let none = real_drain(&mut doc, a, "SD-N");
        inputs::save(&mut doc, small, inputs::KEY_DRAIN, &DrainInputs { flow_m3_s: "0.1".into(), ..Default::default() });
        inputs::save(&mut doc, large, inputs::KEY_DRAIN, &DrainInputs { flow_m3_s: "50".into(), ..Default::default() });
        let r = report(&doc);
        let status = |mark: &str| r.rows.iter().find(|row| row.mark == mark && row.check == "Side drain capacity").unwrap().status;
        assert_eq!(status("SD-S"), Status::Passes);
        assert_eq!(status("SD-L"), Status::Fails);
        assert_eq!(status("SD-N"), Status::InputNeeded);
        let none_row = r.rows.iter().find(|row| row.element == none).unwrap();
        assert!(none_row.reason.contains("CHECK") && none_row.lines.is_empty());
        // The calculation lines are the engine's, unchanged.
        let def = afrobim_drainage::SideDrainDefinition::from_value(&doc.find(small).unwrap().data).unwrap();
        let expected = def.check_capacity(DesignBasis::Mowt, 0.1, None).unwrap().calculation();
        assert_eq!(r.rows.iter().find(|row| row.element == small).unwrap().lines, expected);
    }

    #[test]
    fn a_placeholder_element_is_input_needed_and_the_report_lists_what_has_no_check() {
        let (mut doc, a) = doc_with_road();
        crate::modules_glue::add(&mut doc, 0, a, 100.0, 200.0, 5.0).unwrap();
        let r = report(&doc);
        let row = r.rows.iter().find(|row| row.category == "drain").unwrap();
        assert_eq!(row.status, Status::InputNeeded);
        assert!(row.reason.contains("placeholders"));
        // The alignment has no check, with the reason.
        let (category, why) = r.no_check.iter().find(|(c, _)| c == "alignment").unwrap();
        assert_eq!(category, "alignment");
        assert!(why.contains("geometry"));
    }

    #[test]
    fn a_culvert_has_layout_limits_and_an_outlet_control_that_needs_its_inputs() {
        let (mut doc, _) = doc_with_road();
        // A real, level-free culvert: 900 mm bore, 12 m long, inverts 10.24 m and 10.0 m (test values).
        let def = afrobim_drainage::PipeCulvertDefinition {
            alignment: "MC10".into(),
            station_m: 250.0,
            skew_deg: 0.0,
            barrels: 1,
            internal_diameter_mm: 900.0,
            wall_thickness_mm: 100.0,
            length_mm: 12_000.0,
            barrel_spacing_mm: None,
            invert_level_inlet_m: 10.24,
            invert_level_outlet_m: 10.0,
            inlet_side: afrobim_drainage::Side::Left,
            centre_offset_m: 0.0,
            road_level_m: None,
            material: "as specified".into(),
            placeholder: false,
        };
        let id = afrobim_drainage::add_pipe_culvert(&mut doc, "CU-T", &def).unwrap();
        let r = report(&doc);
        let layout = r.rows.iter().find(|row| row.element == id && row.check == "Culvert layout limits").unwrap();
        let outlet = r.rows.iter().find(|row| row.element == id && row.check.starts_with("Culvert outlet control")).unwrap();
        assert!(matches!(layout.status, Status::Passes | Status::Fails));
        assert_eq!(outlet.status, Status::InputNeeded);
        // With a small flow and entered Manning's n, entrance loss and tailwater it is calculated; the warnings are the engine's.
        let stored = CulvertInputs { flow_m3_s: "0.3".into(), manning_n: "0.013".into(), entrance_ke: "0.5".into(), tailwater_m: "0.2".into(), ..Default::default() };
        inputs::save(&mut doc, id, inputs::KEY_CULVERT, &stored);
        let r = report(&doc);
        let outlet = r.rows.iter().find(|row| row.element == id && row.check.starts_with("Culvert outlet control")).unwrap();
        assert!(matches!(outlet.status, Status::Passes | Status::Fails), "{outlet:?}");
        let expected = def.outlet_control(0.3, 0.013, 0.5, 0.2).unwrap().calculation();
        assert_eq!(outlet.lines, expected);
        assert!(outlet.lines.iter().any(|l| l.starts_with("Check: Inlet control is not computed")));
    }

    #[test]
    fn restraint_and_lighting_report_input_needed_then_their_engine_lines() {
        let (mut doc, a) = doc_with_road();
        let mut req = road_glue::RestraintRequest::empty("VRS-001", 100.0, 160.0);
        req.distance_m = 5.0;
        req.containment = 4;
        req.beam_height_m = 0.7;
        req.post_spacing_m = Some(2.0);
        let mark = road_glue::add_restraint(&mut doc, a, &req).unwrap();
        let id = doc.elements().iter().find(|e| e.name == mark).unwrap().id;
        let r = report(&doc);
        assert_eq!(r.rows.iter().find(|row| row.element == id).unwrap().status, Status::InputNeeded);
        inputs::save(&mut doc, id, inputs::KEY_RESTRAINT, &road_glue::SpeedInput { value: 100.0, mph: false, answer: None });
        let r = report(&doc);
        let row = r.rows.iter().find(|row| row.element == id).unwrap();
        assert!(row.lines.iter().any(|l| l.contains("CD 377")), "{:?}", row.lines);
        assert_ne!(row.status, Status::NotChecked);
        // Lighting: the superseded edition's warning appears unchanged in the report.
        let mut l = road_glue::LightingRequest::empty("LZ-01", 50.0, 250.0);
        l.arrangement = 2;
        l.spacing_m = 40.0;
        l.carriageway_width_m = 7.0;
        l.setback_m = 1.0;
        l.column = road_glue::ColumnFields { mounting_height_m: 8.0, bracket_outreach_m: 1.0, tilt_deg: 0.0, product: "Test luminaire".into(), lumen_output_lm: 5000.0, power_w: 50.0 };
        let lmark = road_glue::add_lighting(&mut doc, a, &l).unwrap();
        let lid = doc.elements().iter().find(|e| e.name == lmark).unwrap().id;
        assert_eq!(report(&doc).rows.iter().find(|row| row.element == lid).unwrap().status, Status::InputNeeded);
        inputs::save(&mut doc, lid, inputs::KEY_LIGHTING, &lighting_inputs::Form::default());
        let r = report(&doc);
        let row = r.rows.iter().find(|row| row.element == lid).unwrap();
        assert!(row.lines.iter().any(|x| x.starts_with("WARNING") && x.contains("superseded")), "{:?}", row.lines);
        let expected = lighting::design::check_lighting(&lighting::lighting_of(&doc, lid).unwrap(), &Default::default()).calculation();
        assert_eq!(row.lines, expected);
    }

    #[test]
    fn a_gantry_is_checked_without_stored_inputs_and_an_alignment_is_not() {
        let (mut doc, a) = doc_with_road();
        // The placeholder gantry is not checked: its dimensions are not the engineer's.
        crate::modules_glue::add(&mut doc, 2, a, 250.0, 250.0, 0.0).unwrap();
        let r = report(&doc);
        let row = r.rows.iter().find(|row| row.category == "gantry").unwrap();
        assert_eq!(row.status, Status::InputNeeded);
        // A gantry whose dimensions were set is checked by the engine on what its definition holds.
        let id = doc.elements().iter().find(|e| e.category == "gantry").unwrap().id;
        let mut def = gantry::definition_of(&doc, id).unwrap();
        def.placeholder = false;
        let alignment = alignment_glue::alignment_of(&doc, a);
        gantry::set_definition(&mut doc, id, &def, alignment.as_ref()).unwrap();
        let r = report(&doc);
        let row = r.rows.iter().find(|row| row.category == "gantry").unwrap();
        assert!(row.lines.iter().any(|l| l.contains("not a structural design")), "{:?}", row.lines);
        assert!(matches!(row.status, Status::Passes | Status::Fails | Status::InputNeeded));
    }

    #[test]
    fn the_export_quotes_every_field_and_the_pack_carries_the_lines() {
        let report = Report {
            rows: vec![Row {
                element: ElementId(1),
                mark: "SD-1".into(),
                category: "drain".into(),
                check: "Side drain capacity".into(),
                status: Status::Fails,
                reason: "overtops, at 5 m3/s".into(),
                lines: vec!["Line one".into(), "Line \"two\"".into()],
                notes: vec!["[Warning] CODE: message".into()],
            }],
            no_check: vec![("alignment".into(), "geometry".into())],
        };
        let csv = report.to_csv();
        assert!(csv.starts_with("Mark,Category,Check,Status,Reason,Calculation\n"));
        assert!(csv.contains("\"overtops, at 5 m3/s\"") && csv.contains("\"Line one\nLine \"\"two\"\"\""), "{csv}");
        let pack = report.to_pack("Test");
        assert!(pack.contains("SD-1 (drain): Side drain capacity") && pack.contains("  Line \"two\"") && pack.contains("[Warning] CODE: message") && pack.contains("alignment: geometry"));
        assert!(!pack.contains('\u{2014}'));
        // Filters.
        let mut statuses = [true; 5];
        assert_eq!(report.visible(&statuses, None), vec![0]);
        statuses[Status::ALL.iter().position(|s| *s == Status::Fails).unwrap()] = false;
        assert!(report.visible(&statuses, None).is_empty());
        assert!(report.visible(&[true; 5], Some("pavement")).is_empty());
    }

    #[test]
    fn a_sign_support_design_is_input_needed_then_checked_against_the_model() {
        let mut doc = Document::default();
        let sign = doc.add(signs_glue::CATEGORY, "S1", afrobim_kernel::Pt::new(0.0, 0.0), 900.0, 900.0, "Signs", serde_json::json!({}));
        supports_glue::add(&mut doc, sign, 2000.0, 1, 150.0).unwrap();
        let r = report(&doc);
        let row = r.rows.iter().find(|row| row.element == sign && row.check.starts_with("Sign support design")).unwrap();
        assert_eq!(row.status, Status::InputNeeded);
        // The DESIGN dialog designs the supports and keeps its entries on the sign.
        let mut req = supports_glue::DesignRequest::starting(2000.0, 1);
        req.wind = supports_glue::WindChoice::Entered(1.0);
        supports_glue::design(&mut doc, sign, &req).unwrap();
        let r = report(&doc);
        let row = r.rows.iter().find(|row| row.element == sign && row.check.starts_with("Sign support design")).unwrap();
        assert_eq!(row.status, Status::Passes, "{}", row.reason);
        assert!(row.lines.iter().any(|l| l.contains("CHS 76.1 × 3.2")), "{:?}", row.lines);
        // The model's post is then replaced by a placeholder: the report says it differs.
        supports_glue::remove(&mut doc, sign);
        supports_glue::add(&mut doc, sign, 2000.0, 1, 150.0).unwrap();
        let r = report(&doc);
        let row = r.rows.iter().find(|row| row.element == sign && row.check.starts_with("Sign support design")).unwrap();
        assert_eq!(row.status, Status::Differs, "{}", row.reason);
    }
}
