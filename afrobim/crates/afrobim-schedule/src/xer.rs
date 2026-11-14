//! Primavera P6 XER exchange.
//!
//! XER is a tab-separated text file: a first line `ERMHDR` with the export header, then for each
//! table a `%T` line (table name), a `%F` line (field names) and one `%R` line per row, and a final
//! `%E` line. Table and field names, the enumeration codes and the encoding of calendar data used
//! here were checked against sources listed in the README (Oracle's XER data map, the `xerparser`
//! and `PyP6XER` Python packages, and sample files exported by P6). Neither the exporter nor the
//! importer has been run against P6 itself.
//!
//! Hours: XER stores durations and lags in hours. A working day is converted with the
//! `day_hr_cnt` of the calendar concerned; the exporter writes calendars of [`HOURS_PER_DAY`]
//! hours (08:00-12:00 and 13:00-17:00), a file-format convention and not a planning value.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::calendar::Calendar;
use crate::cpm::{schedule, Schedule};
use crate::date::{Date, Weekday};
use crate::model::{Activity, Constraint, Dependency, LinkType, MilestoneKind, Programme, Status, SummaryTask};
use crate::mspdi::ImportReport;
use crate::wbs::{outline, OutlineKind};

/// Hours written for one working day by the exporter.
pub const HOURS_PER_DAY: f64 = 8.0;
/// Days from 1899-12-30 (day 0 of the calendar-data date numbers) to 1970-01-01.
const SERIAL_OF_1970: i64 = 25569;

#[derive(Debug, thiserror::Error)]
pub enum XerError {
    #[error("not a P6 XER file: it does not start with ERMHDR")]
    NotXer,
    #[error("the file has no {0}")]
    Missing(String),
    #[error("{0}")]
    Unsupported(String),
}

/// What the export could not carry. Nothing is dropped silently.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExportReport {
    pub warnings: Vec<String>,
}

// ------------------------------------------------------------------ reading the text

struct Table {
    fields: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    fn col(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|f| f == name)
    }
}

/// A row with named access; a missing column or a short row reads as the empty string.
#[derive(Clone, Copy)]
struct Row<'a> {
    t: &'a Table,
    r: &'a [String],
}

impl<'a> Row<'a> {
    fn get(&self, name: &str) -> &'a str {
        self.t.col(name).and_then(|i| self.r.get(i)).map(String::as_str).unwrap_or("")
    }
}

impl Table {
    fn rows(&self) -> impl Iterator<Item = Row<'_>> {
        self.rows.iter().map(move |r| Row { t: self, r })
    }
}

/// Windows-1252 bytes 0x80 to 0x9F (the rest equals Latin-1). Undefined bytes read as U+FFFD.
const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}', '\u{017D}', '\u{FFFD}',
    '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
];

/// Decodes the bytes of an XER file: UTF-8 when valid (a byte order mark is dropped), otherwise
/// Windows-1252, which is the code page the sources say P6 writes.
pub fn decode_xer_bytes(bytes: &[u8]) -> String {
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b
            .iter()
            .map(|&c| match c {
                0x80..=0x9F => CP1252_HIGH[(c - 0x80) as usize],
                _ => c as char,
            })
            .collect(),
    }
}

/// Reads the file bytes with [`decode_xer_bytes`] and imports them.
pub fn import_xer_bytes(bytes: &[u8]) -> Result<(Programme, ImportReport), XerError> {
    import_xer(&decode_xer_bytes(bytes))
}

fn read_tables(text: &str) -> Result<HashMap<String, Table>, XerError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    match lines.next() {
        Some(l) if l.starts_with("ERMHDR") => {}
        _ => return Err(XerError::NotXer),
    }
    let mut tables: HashMap<String, Table> = HashMap::new();
    let mut current: Option<String> = None;
    for line in lines {
        let mut parts = line.split('\t');
        match parts.next().unwrap_or("") {
            "%T" => {
                let name = parts.next().unwrap_or("").trim().to_string();
                tables.entry(name.clone()).or_insert_with(|| Table { fields: Vec::new(), rows: Vec::new() });
                current = Some(name);
            }
            "%F" => {
                if let Some(t) = current.as_ref().and_then(|c| tables.get_mut(c)) {
                    t.fields = parts.map(|f| f.trim().to_string()).collect();
                }
            }
            "%R" => {
                if let Some(t) = current.as_ref().and_then(|c| tables.get_mut(c)) {
                    t.rows.push(parts.map(str::to_string).collect());
                }
            }
            _ => {}
        }
    }
    Ok(tables)
}

// ------------------------------------------------------------------ calendar data

/// A node of the calendar-data text: `(0||Name(value)(children))`.
#[derive(Debug, Default)]
struct CNode {
    name: String,
    value: String,
    children: Vec<CNode>,
}

fn parse_cnodes(b: &[u8], pos: &mut usize) -> Result<Vec<CNode>, String> {
    let mut out = Vec::new();
    while *pos < b.len() && b[*pos] == b'(' {
        *pos += 1;
        let h0 = *pos;
        while *pos < b.len() && b[*pos] != b'(' {
            *pos += 1;
        }
        let header = std::str::from_utf8(&b[h0..*pos]).map_err(|e| e.to_string())?;
        let name = header.rsplit("||").next().unwrap_or("").to_string();
        if *pos >= b.len() {
            return Err("calendar data ends inside a node".into());
        }
        *pos += 1; // '(' of the value
        let v0 = *pos;
        while *pos < b.len() && b[*pos] != b')' {
            *pos += 1;
        }
        let value = std::str::from_utf8(&b[v0..*pos]).map_err(|e| e.to_string())?.to_string();
        *pos += 1; // ')' of the value
        if *pos >= b.len() || b[*pos] != b'(' {
            return Err("calendar data: a node has no child list".into());
        }
        *pos += 1; // '(' of the children
        let children = parse_cnodes(b, pos)?;
        if *pos >= b.len() || b[*pos] != b')' {
            return Err("calendar data: unbalanced parentheses".into());
        }
        *pos += 1; // ')' of the children
        if *pos >= b.len() || b[*pos] != b')' {
            return Err("calendar data: unbalanced parentheses".into());
        }
        *pos += 1; // ')' of the node
        out.push(CNode { name, value, children });
    }
    Ok(out)
}

/// Hours of one shift value `s|08:00|f|12:00`.
fn shift_hours(v: &str) -> Option<f64> {
    let p: Vec<&str> = v.split('|').collect();
    if p.len() != 4 || p[0] != "s" || p[2] != "f" {
        return None;
    }
    let min = |t: &str| -> Option<f64> {
        let (h, m) = t.split_once(':')?;
        Some(h.parse::<f64>().ok()? * 60.0 + m.parse::<f64>().ok()?)
    };
    Some((min(p[3])? - min(p[1])?) / 60.0)
}

fn weekday_of_xer(n: u32) -> Option<Weekday> {
    // Day 1 is Sunday (the sources list Sunday first and the sample's days 1 and 7 are the weekend).
    match n {
        1 => Some(Weekday::Sun),
        2 => Some(Weekday::Mon),
        3 => Some(Weekday::Tue),
        4 => Some(Weekday::Wed),
        5 => Some(Weekday::Thu),
        6 => Some(Weekday::Fri),
        7 => Some(Weekday::Sat),
        _ => None,
    }
}

fn xer_weekday_number(w: Weekday) -> u32 {
    match w {
        Weekday::Sun => 1,
        Weekday::Mon => 2,
        Weekday::Tue => 3,
        Weekday::Wed => 4,
        Weekday::Thu => 5,
        Weekday::Fri => 6,
        Weekday::Sat => 7,
    }
}

pub(crate) fn date_of_serial(serial: i64) -> Option<Date> {
    let epoch = Date::from_ymd(1970, 1, 1)?;
    Some(epoch.add_days(serial - SERIAL_OF_1970))
}

fn serial_of_date(d: Date) -> i64 {
    d.days_since(Date::from_ymd(1970, 1, 1).expect("a real date")) + SERIAL_OF_1970
}

/// What was read from one calendar's data.
pub(crate) struct CalData {
    /// `None` when the calendar lists no week days at all (it then inherits from its base).
    pub(crate) weekdays: Option<BTreeSet<Weekday>>,
    pub(crate) hours: HashMap<Weekday, f64>,
    pub(crate) holidays: BTreeSet<Date>,
    working_exceptions: usize,
}

pub(crate) fn read_calendar_data(data: &str) -> Result<CalData, String> {
    let b = data.trim().as_bytes();
    let mut pos = 0;
    let roots = parse_cnodes(b, &mut pos)?;
    if pos != b.len() || roots.is_empty() {
        return Err("calendar data is not in the expected nested form".into());
    }
    let mut out = CalData { weekdays: None, hours: HashMap::new(), holidays: BTreeSet::new(), working_exceptions: 0 };
    let find = |nodes: &'_ [CNode], name: &str| -> Option<usize> { nodes.iter().position(|n| n.name == name) };
    let top = &roots[0].children;
    if let Some(i) = find(top, "DaysOfWeek") {
        let days = &top[i].children;
        if !days.is_empty() {
            let mut set = BTreeSet::new();
            for d in days {
                let Some(w) = d.name.parse::<u32>().ok().and_then(weekday_of_xer) else { continue };
                let hrs: f64 = d.children.iter().filter_map(|s| shift_hours(&s.value)).sum();
                if !d.children.is_empty() && hrs > 0.0 {
                    set.insert(w);
                    out.hours.insert(w, hrs);
                }
            }
            out.weekdays = Some(set);
        }
    }
    if let Some(i) = find(top, "Exceptions") {
        for e in &top[i].children {
            let Some(serial) = e.value.strip_prefix("d|").and_then(|v| v.trim().parse::<i64>().ok()) else { continue };
            if e.children.is_empty() {
                if let Some(d) = date_of_serial(serial) {
                    out.holidays.insert(d);
                }
            } else {
                out.working_exceptions += 1;
            }
        }
    }
    Ok(out)
}

pub(crate) fn write_calendar_data(c: &Calendar) -> String {
    let mut s = String::from("(0||CalendarData()((0||DaysOfWeek()(");
    for n in 1..=7u32 {
        let w = Weekday::ALL.iter().copied().find(|w| xer_weekday_number(*w) == n).expect("1..=7");
        if c.working_days.contains(&w) {
            s.push_str(&format!("(0||{n}()((0||0(s|08:00|f|12:00)())(0||1(s|13:00|f|17:00)())))"));
        } else {
            s.push_str(&format!("(0||{n}()())"));
        }
    }
    s.push_str("))(0||Exceptions()(");
    for (k, h) in c.holidays.iter().enumerate() {
        s.push_str(&format!("(0||{k}(d|{})())", serial_of_date(*h)));
    }
    s.push_str("))))");
    s
}

// ------------------------------------------------------------------ import

fn parse_f(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        t.replace(',', ".").parse().ok()
    }
}

fn date_part(s: &str) -> Option<Date> {
    if s.trim().len() < 10 {
        return None;
    }
    Date::parse(s)
}

/// Hours of the time of day in `YYYY-MM-DD HH:MM`, when present.
fn hour_of(s: &str) -> Option<f64> {
    let t = s.trim().get(11..)?;
    let (h, rest) = t.split_once(':')?;
    let m = rest.get(..2)?;
    Some(h.parse::<f64>().ok()? + m.parse::<f64>().ok()? / 60.0)
}

struct Cal {
    name: String,
    day_hr: f64,
}

/// Imports one project of an XER file. The first project with `export_flag` `Y` is read (the first
/// project when the field is absent); further projects are listed in a warning and skipped.
/// WBS nodes become summary tasks, activities keep their WBS node as parent. See the README for
/// what is read and what is reported as skipped.
pub fn import_xer(text: &str) -> Result<(Programme, ImportReport), XerError> {
    let tables = read_tables(text)?;
    let mut rep = ImportReport::default();
    let need = |n: &str| tables.get(n).ok_or_else(|| XerError::Missing(format!("{n} table")));
    let proj_t = need("PROJECT")?;
    let cal_t = need("CALENDAR")?;
    let wbs_t = need("PROJWBS")?;
    let task_t = need("TASK")?;

    // The project.
    let mut projects = proj_t.rows().filter(|r| r.t.col("export_flag").is_none() || r.get("export_flag") == "Y");
    let proj = projects.next().ok_or_else(|| XerError::Missing("project with export_flag Y".into()))?;
    let others: Vec<String> = projects.map(|r| r.get("proj_short_name").to_string()).collect();
    if !others.is_empty() {
        rep.warnings.push(format!("the file holds {} more project(s) ({}); only the first was imported", others.len(), others.join(", ")));
    }
    let proj_id = proj.get("proj_id").to_string();
    let sep = proj.get("name_sep_char");
    let sep = if sep.is_empty() { "." } else { sep };

    // Calendars: the project's own and the global ones, resolved below.
    struct RawCal {
        id: String,
        name: String,
        base: Option<String>,
        day_hr: Option<f64>,
        data: Result<CalData, String>,
        kind: String,
        is_default: bool,
    }
    let mut raw: Vec<RawCal> = Vec::new();
    for r in cal_t.rows() {
        let owner = r.get("proj_id");
        if !owner.is_empty() && owner != proj_id {
            continue;
        }
        raw.push(RawCal {
            id: r.get("clndr_id").to_string(),
            name: r.get("clndr_name").to_string(),
            base: Some(r.get("base_clndr_id").to_string()).filter(|b| !b.is_empty()),
            day_hr: parse_f(r.get("day_hr_cnt")),
            data: read_calendar_data(r.get("clndr_data")),
            kind: r.get("clndr_type").to_string(),
            is_default: r.get("default_flag") == "Y",
        });
    }
    let find_raw = |id: &str| raw.iter().find(|c| c.id == id);

    // Tasks decide which calendars are needed.
    let mut default_cal_id = proj.get("clndr_id").to_string();
    if find_raw(&default_cal_id).is_none() {
        // No default calendar on the project: fall back to the calendar flagged as the default.
        if let Some(c) = raw.iter().find(|c| c.is_default) {
            rep.warnings.push(format!("the project names no usable default calendar; the calendar flagged as default ('{}') was used", c.name));
            default_cal_id = c.id.clone();
        }
    }
    let mut needed: Vec<String> = vec![default_cal_id.clone()];
    for r in task_t.rows().filter(|r| r.get("proj_id") == proj_id) {
        let c = r.get("clndr_id").to_string();
        if !needed.contains(&c) {
            needed.push(c);
        }
    }
    let mut calendars: Vec<Calendar> = Vec::new();
    let mut cal_of: HashMap<String, Cal> = HashMap::new();
    for id in needed.iter().filter(|i| !i.is_empty()) {
        let Some(rc) = find_raw(id) else {
            rep.warnings.push(format!("calendar {id} is used but not in the file"));
            continue;
        };
        let data = match &rc.data {
            Ok(d) => d,
            Err(e) => {
                rep.warnings.push(format!("calendar '{}' ({id}) was skipped: {e}", rc.name));
                continue;
            }
        };
        // Resolve inheritance: a calendar that lists no week days takes its base calendar's.
        let mut weekdays = data.weekdays.clone();
        let mut hours = data.hours.clone();
        let mut holidays = data.holidays.clone();
        if weekdays.is_none() {
            let base = rc.base.as_deref().and_then(find_raw);
            match base.and_then(|b| b.data.as_ref().ok()) {
                Some(bd) if bd.weekdays.is_some() => {
                    weekdays = bd.weekdays.clone();
                    hours = bd.hours.clone();
                    holidays.extend(bd.holidays.iter().copied());
                    rep.warnings.push(format!("calendar '{}' lists no week days; it was given those of its base calendar '{}' (assumed to be what P6 means)", rc.name, base.map(|b| b.name.as_str()).unwrap_or("")));
                }
                _ => {
                    rep.warnings.push(format!("calendar '{}' ({id}) lists no week days and has no usable base calendar; it was skipped", rc.name));
                    continue;
                }
            }
        }
        let weekdays = weekdays.unwrap_or_default();
        if weekdays.is_empty() {
            rep.warnings.push(format!("calendar '{}' ({id}) has no working weekday and was skipped", rc.name));
            continue;
        }
        let day_hr = rc.day_hr.filter(|h| *h > 0.0).or_else(|| weekdays.iter().next().and_then(|w| hours.get(w).copied()));
        let Some(day_hr) = day_hr else {
            rep.warnings.push(format!("calendar '{}' ({id}) has no hours per day and was skipped", rc.name));
            continue;
        };
        if hours.values().any(|h| (h - day_hr).abs() > 1e-6) {
            rep.warnings.push(format!("calendar '{}': working days differ in length; durations and lags were converted to days with the {day_hr} hours per day of the calendar", rc.name));
        }
        if data.working_exceptions > 0 {
            rep.warnings.push(format!("calendar '{}': {} working exception(s) (a normally non-working day made working) were skipped; only non-working days are read", rc.name, data.working_exceptions));
        }
        if rc.kind == "CA_Rsrc" {
            rep.warnings.push(format!("calendar '{}' is a resource calendar and was read as an ordinary calendar", rc.name));
        }
        let mut name = if rc.name.is_empty() { format!("Calendar {id}") } else { rc.name.clone() };
        if calendars.iter().any(|c| c.name == name) {
            name = format!("{name} ({id})");
        }
        cal_of.insert(id.clone(), Cal { name: name.clone(), day_hr });
        calendars.push(Calendar { name, working_days: weekdays, holidays });
    }
    let default_cal = cal_of.get(&default_cal_id).ok_or_else(|| XerError::Missing("usable default calendar of the project".into()))?;
    let default_cal_name = default_cal.name.clone();

    // WBS.
    struct Wbs {
        id: String,
        parent: Option<String>,
        short: String,
        name: String,
        root: bool,
        seq: i64,
    }
    let mut wbs: Vec<Wbs> = wbs_t
        .rows()
        .filter(|r| r.get("proj_id") == proj_id)
        .enumerate()
        .map(|(k, r)| Wbs {
            id: r.get("wbs_id").to_string(),
            parent: Some(r.get("parent_wbs_id").to_string()).filter(|p| !p.is_empty()),
            short: r.get("wbs_short_name").to_string(),
            name: r.get("wbs_name").to_string(),
            root: r.get("proj_node_flag") == "Y",
            seq: r.get("seq_num").trim().parse().unwrap_or(k as i64),
        })
        .collect();
    wbs.sort_by_key(|w| w.seq); // stable: equal keys keep file order
    let root_id = wbs.iter().find(|w| w.root).map(|w| w.id.clone());
    let by_id: HashMap<&str, &Wbs> = wbs.iter().map(|w| (w.id.as_str(), w)).collect();
    let code_of = |id: &str| -> String {
        let mut parts = Vec::new();
        let mut cur = by_id.get(id).copied();
        let mut guard = 0;
        while let Some(w) = cur {
            if w.root {
                break;
            }
            parts.push(w.short.clone());
            cur = w.parent.as_deref().and_then(|p| by_id.get(p).copied());
            guard += 1;
            if guard > wbs.len() {
                break;
            }
        }
        parts.reverse();
        parts.join(sep)
    };
    let name = root_id.as_deref().and_then(|r| by_id.get(r)).map(|w| w.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| proj.get("proj_short_name").to_string());
    let start = date_part(proj.get("plan_start_date")).ok_or_else(|| XerError::Missing("plan_start_date of the project".into()))?;
    let mut p = Programme::new(&name, start, calendars.iter().find(|c| c.name == default_cal_name).cloned().expect("default calendar was added"));
    p.calendars = calendars;
    p.calendar = default_cal_name.clone();
    p.required_finish = date_part(proj.get("plan_end_date"));
    for w in wbs.iter().filter(|w| !w.root) {
        let parent = w.parent.clone().filter(|q| Some(q) != root_id.as_ref() && by_id.contains_key(q.as_str()));
        p.summaries.push(SummaryTask { id: w.id.clone(), wbs: code_of(&w.id), name: w.name.clone(), parent });
    }

    // Schedule options.
    if let Some(so) = tables.get("SCHEDOPTIONS").and_then(|t| t.rows().find(|r| r.get("proj_id") == proj_id)) {
        if so.get("sched_retained_logic") == "N" || so.get("sched_progress_override") == "Y" {
            rep.warnings.push("the project schedules with progress override or without retained logic; this crate always uses retained logic, so forecast dates of work in progress can differ".into());
        }
        let lag = so.get("sched_calendar_on_relationship_lag");
        if !lag.is_empty() && lag != "rcal_Successor" {
            rep.warnings.push(format!("the project computes lag on calendar '{lag}'; this crate always uses the successor's calendar"));
        }
        let ft = so.get("sched_float_type");
        if !ft.is_empty() && ft != "FT_FF" {
            rep.warnings.push(format!("the project's float type is '{ft}'; total float here is always finish float"));
        }
    }
    let cpt = proj.get("critical_path_type");
    if !cpt.is_empty() && cpt != "CT_TotFloat" {
        rep.warnings.push(format!("the project marks critical activities by '{cpt}' in P6; this crate marks by total float unless schedule_with is asked for the longest path"));
    }

    // Activities.
    let hours_to_days = |hours: f64, day_hr: f64, what: &str, id: &str, rep: &mut ImportReport| -> i64 {
        let d = hours / day_hr;
        if (d - d.round()).abs() > 1e-6 {
            rep.warnings.push(format!("activity {id}: {what} of {d:.2} working days was rounded to a whole day"));
        }
        d.round() as i64
    };
    let mut kept: HashMap<String, String> = HashMap::new(); // task_id -> activity id
    let mut skipped_tasks: HashSet<String> = HashSet::new();
    let mut max_actual: Option<Date> = None;
    let mut any_progress = false;
    let mut blank_calendar = 0usize;
    for r in task_t.rows().filter(|r| r.get("proj_id") == proj_id) {
        let tid = r.get("task_id").to_string();
        let code = r.get("task_code").to_string();
        let ttype = r.get("task_type");
        match ttype {
            "TT_LOE" | "TT_WBS" => {
                rep.warnings.push(format!("activity {code}: type {ttype} ({}) is not supported; it and its relationships were skipped", if ttype == "TT_LOE" { "level of effort" } else { "WBS summary" }));
                skipped_tasks.insert(tid);
                continue;
            }
            "TT_Rsrc" => rep.warnings.push(format!("activity {code}: resource dependent type was read as task dependent")),
            _ => {}
        }
        let cal = match cal_of.get(r.get("clndr_id")) {
            Some(c) => c,
            None if r.get("clndr_id").is_empty() => {
                blank_calendar += 1;
                &cal_of[&default_cal_id]
            }
            None => {
                rep.warnings.push(format!("activity {code} uses calendar {} which could not be imported; the activity was skipped", r.get("clndr_id")));
                skipped_tasks.insert(tid);
                continue;
            }
        };
        let milestone = matches!(ttype, "TT_Mile" | "TT_FinMile");
        let target_h = parse_f(r.get("target_drtn_hr_cnt")).unwrap_or(0.0);
        let dur = if milestone { 0 } else { hours_to_days(target_h, cal.day_hr, "duration", &code, &mut rep).max(0) as u32 };
        let mut a = Activity::new(&code, &code_of(r.get("wbs_id")), r.get("task_name"), dur);
        if !milestone && dur == 0 {
            rep.warnings.push(format!("activity {code} has zero duration but is not a milestone; it is imported as a milestone"));
        }
        if milestone || dur == 0 {
            a.milestone_kind = if ttype == "TT_Mile" { MilestoneKind::Start } else { MilestoneKind::Finish };
        }
        if cal.name != default_cal_name {
            a.calendar = Some(cal.name.clone());
        }
        let wid = r.get("wbs_id");
        if Some(wid) != root_id.as_deref() && p.summaries.iter().any(|s| s.id == wid) {
            a.parent = Some(wid.to_string());
        }
        // Constraints: the primary and the secondary one.
        let mut cons: Vec<Constraint> = Vec::new();
        for (ty, dt) in [(r.get("cstr_type"), r.get("cstr_date")), (r.get("cstr_type2"), r.get("cstr_date2"))] {
            if ty.is_empty() {
                continue;
            }
            let d = date_part(dt);
            match (ty, d) {
                ("CS_MSOA", Some(d)) => cons.push(Constraint::StartNoEarlierThan(d)),
                ("CS_MEOB", Some(d)) => cons.push(Constraint::FinishNoLaterThan(d)),
                ("CS_MANDSTART", Some(d)) => cons.push(Constraint::MustStartOn(d)),
                ("CS_MSO", Some(d)) => {
                    cons.push(Constraint::StartNoEarlierThan(d));
                    rep.warnings.push(format!("activity {code}: constraint CS_MSO (start on) was read as start no earlier than {d}; the start-no-later-than half is not modelled"));
                }
                (_, None) => rep.warnings.push(format!("activity {code}: constraint {ty} has no date and was ignored")),
                _ => rep.warnings.push(format!("activity {code}: constraint {ty} is not supported (supported: CS_MSOA, CS_MEOB, CS_MANDSTART; CS_MSO as start no earlier than) and was ignored")),
            }
        }
        let mut it = cons.into_iter();
        a.constraint = it.next();
        a.extra_constraints = it.collect();
        if !r.get("expect_end_date").is_empty() {
            rep.warnings.push(format!("activity {code}: an expected finish date is not supported and was ignored"));
        }
        // Progress.
        let status = r.get("status_code");
        let act_start = date_part(r.get("act_start_date"));
        let act_end = date_part(r.get("act_end_date"));
        match status {
            "TK_Complete" => {
                any_progress = true;
                let fin = act_end.or(act_start);
                let fin = match fin {
                    Some(f) => f,
                    None => {
                        rep.warnings.push(format!("activity {code} is complete without actual dates; its planned finish was used"));
                        date_part(r.get("target_end_date")).unwrap_or(start)
                    }
                };
                a.progress.actual_finish = Some(fin);
                a.progress.actual_start = if milestone { None } else { Some(act_start.unwrap_or(fin).min(fin)) };
                a.progress.percent_complete = 100.0;
                max_actual = max_actual.max(Some(fin));
            }
            "TK_Active" => {
                any_progress = true;
                let s0 = act_start.unwrap_or_else(|| {
                    rep.warnings.push(format!("activity {code} is in progress without an actual start; its planned start was used"));
                    date_part(r.get("target_start_date")).unwrap_or(start)
                });
                a.progress.actual_start = Some(s0);
                max_actual = max_actual.max(Some(s0));
                let remain_h = parse_f(r.get("remain_drtn_hr_cnt"));
                let pct_type = r.get("complete_pct_type");
                let pct = match pct_type {
                    "CP_Phys" => parse_f(r.get("phys_complete_pct")).unwrap_or(0.0),
                    other => {
                        if other == "CP_Units" {
                            rep.warnings.push(format!("activity {code}: percent complete by units is not supported; the duration percent was used"));
                        }
                        match (remain_h, target_h) {
                            (Some(rm), t) if t > 0.0 => 100.0 * (1.0 - rm / t),
                            _ => 0.0,
                        }
                    }
                };
                a.progress.percent_complete = pct.clamp(0.0, 99.99);
                if let Some(rm) = remain_h {
                    let d = hours_to_days(rm, cal.day_hr, "remaining duration", &code, &mut rep);
                    if !milestone {
                        a.progress.remaining_duration = Some(d.max(1) as u32);
                    }
                }
            }
            _ => {}
        }
        kept.insert(tid, code);
        p.activities.push(a);
    }
    if blank_calendar > 0 {
        rep.warnings.push(format!("{blank_calendar} activity(ies) name no calendar; the project's default calendar was used"));
    }
    // Data date: the file's last recalculated date, as xerparser and the sample files use it. A
    // time of day before noon means nothing of that day was worked, so the data date is the day
    // before (this crate takes the data date as the end of a day).
    if any_progress {
        let lr = proj.get("last_recalc_date");
        let mut dd = date_part(lr);
        if let Some(d) = dd {
            if hour_of(lr).is_some_and(|h| h < 12.0) {
                dd = Some(d.add_days(-1));
                rep.warnings.push(format!("data date {lr} is before noon; it was read as the end of the day before ({})", d.add_days(-1)));
            }
        }
        let dd = match (dd, max_actual) {
            (Some(d), Some(m)) if m > d => {
                rep.warnings.push(format!("an actual date ({m}) is after the data date ({d}); the data date was moved to it"));
                m
            }
            (Some(d), _) => d,
            (None, Some(m)) => {
                rep.warnings.push(format!("the project has no data date; the latest actual date ({m}) was used"));
                m
            }
            (None, None) => start,
        };
        p.data_date = Some(dd);
    }
    // Relationships.
    let mut dropped = 0usize;
    for r in tables.get("TASKPRED").into_iter().flat_map(|t| t.rows()) {
        if r.get("proj_id") != proj_id {
            continue;
        }
        let (succ, pred) = (r.get("task_id"), r.get("pred_task_id"));
        if r.get("pred_proj_id") != proj_id {
            rep.warnings.push(format!("a relationship to a predecessor in another project (task {pred}) was skipped"));
            continue;
        }
        let (Some(sa), Some(pa)) = (kept.get(succ), kept.get(pred)) else {
            dropped += 1;
            continue;
        };
        let kind = match r.get("pred_type") {
            "PR_FS" => LinkType::FS,
            "PR_SS" => LinkType::SS,
            "PR_FF" => LinkType::FF,
            "PR_SF" => LinkType::SF,
            other => {
                rep.warnings.push(format!("relationship {pa} -> {sa}: type '{other}' is not valid and the relationship was skipped"));
                continue;
            }
        };
        let succ_act = p.activities.iter().find(|a| &a.id == sa).expect("kept");
        let day_hr = succ_act.calendar.as_deref().or(Some(default_cal_name.as_str())).and_then(|n| cal_of.values().find(|c| c.name == n)).map(|c| c.day_hr).unwrap_or(HOURS_PER_DAY);
        let lag_h = parse_f(r.get("lag_hr_cnt")).unwrap_or(0.0);
        let days = hours_to_days(lag_h, day_hr, "lag", sa, &mut rep);
        p.dependencies.push(Dependency { predecessor: pa.clone(), successor: sa.clone(), kind, lag: days as i32 });
    }
    if dropped > 0 {
        rep.warnings.push(format!("{dropped} relationship(s) to or from skipped activities were dropped"));
    }
    Ok((p, rep))
}

// ------------------------------------------------------------------ export

fn clean(s: &str) -> String {
    s.replace(['\t', '\r', '\n'], " ")
}

fn num(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 {
        format!("{}", x.round() as i64)
    } else {
        let s = format!("{x:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn stamp(d: Date, hhmm: &str) -> String {
    format!("{d} {hhmm}")
}

struct Out {
    s: String,
}

impl Out {
    fn table(&mut self, name: &str, fields: &[&str], rows: &[Vec<String>]) {
        self.s.push_str(&format!("%T\t{name}\n%F\t{}\n", fields.join("\t")));
        for r in rows {
            debug_assert_eq!(r.len(), fields.len(), "table {name}");
            self.s.push_str("%R\t");
            self.s.push_str(&r.iter().map(|v| clean(v)).collect::<Vec<_>>().join("\t"));
            self.s.push('\n');
        }
    }
}

/// A row from `(field, value)` pairs in the order of `fields`; fields not given are empty.
fn row_of(fields: &[&str], values: &[(&str, String)]) -> Vec<String> {
    fields.iter().map(|f| values.iter().find(|(k, _)| k == f).map(|(_, v)| v.clone()).unwrap_or_default()).collect()
}

const PROJECT_FIELDS: [&str; 66] = [
    "proj_id", "fy_start_month_num", "rsrc_self_add_flag", "allow_complete_flag", "rsrc_multi_assign_flag", "checkout_flag", "project_flag", "step_complete_flag", "cost_qty_recalc_flag", "batch_sum_flag",
    "name_sep_char", "def_complete_pct_type", "proj_short_name", "acct_id", "orig_proj_id", "source_proj_id", "base_type_id", "clndr_id", "sum_base_proj_id", "task_code_base", "task_code_step",
    "priority_num", "wbs_max_sum_level", "strgy_priority_num", "last_checksum", "critical_drtn_hr_cnt", "def_cost_per_qty", "last_recalc_date", "plan_start_date", "plan_end_date", "scd_end_date",
    "add_date", "last_tasksum_date", "fcst_start_date", "def_duration_type", "task_code_prefix", "guid", "def_qty_type", "add_by_name", "web_local_root_path", "proj_url", "def_rate_type",
    "add_act_remain_flag", "act_this_per_link_flag", "def_task_type", "act_pct_link_flag", "critical_path_type", "task_code_prefix_flag", "def_rollup_dates_flag", "use_project_baseline_flag",
    "rem_target_link_flag", "reset_planned_flag", "allow_neg_act_flag", "sum_assign_level", "last_fin_dates_id", "last_baseline_update_date", "cr_external_key", "apply_actuals_date", "location_id",
    "loaded_scope_level", "export_flag", "new_fin_dates_id", "next_data_date", "close_period_flag", "sum_refresh_date", "trsrcsum_loaded",
];
const CALENDAR_FIELDS: [&str; 13] = ["clndr_id", "default_flag", "clndr_name", "proj_id", "base_clndr_id", "last_chng_date", "clndr_type", "day_hr_cnt", "week_hr_cnt", "month_hr_cnt", "year_hr_cnt", "rsrc_private", "clndr_data"];
const SCHEDOPTIONS_FIELDS: [&str; 25] = [
    "schedoptions_id", "proj_id", "sched_outer_depend_type", "sched_open_critical_flag", "sched_lag_early_start_flag", "sched_retained_logic", "sched_setplantoforecast", "sched_float_type",
    "sched_calendar_on_relationship_lag", "sched_use_expect_end_flag", "sched_progress_override", "level_float_thrs_cnt", "level_outer_assign_flag", "level_outer_assign_priority", "level_over_alloc_pct",
    "level_within_float_flag", "level_keep_sched_date_flag", "level_all_rsrc_flag", "sched_use_project_end_date_for_float", "enable_multiple_longest_path_calc", "limit_multiple_longest_path_calc",
    "max_multiple_longest_path", "use_total_float_multiple_longest_paths", "key_activity_for_multiple_longest_paths", "LevelPriorityList",
];
const PROJWBS_FIELDS: [&str; 25] = [
    "wbs_id", "proj_id", "obs_id", "seq_num", "proj_node_flag", "sum_data_flag", "status_code", "wbs_short_name", "wbs_name", "phase_id", "parent_wbs_id", "ev_user_pct", "ev_etc_user_value", "orig_cost",
    "indep_remain_total_cost", "ann_dscnt_rate_pct", "dscnt_period_type", "indep_remain_work_qty", "anticip_start_date", "anticip_end_date", "ev_compute_type", "ev_etc_compute_type", "guid", "tmpl_guid",
    "plan_open_state",
];
const TASK_FIELDS: [&str; 59] = [
    "task_id", "proj_id", "wbs_id", "clndr_id", "phys_complete_pct", "rev_fdbk_flag", "lock_plan_flag", "auto_compute_act_flag", "complete_pct_type", "task_type", "duration_type", "status_code", "task_code",
    "task_name", "rsrc_id", "total_float_hr_cnt", "free_float_hr_cnt", "remain_drtn_hr_cnt", "act_work_qty", "remain_work_qty", "target_work_qty", "target_drtn_hr_cnt", "target_equip_qty", "act_equip_qty",
    "remain_equip_qty", "cstr_date", "act_start_date", "act_end_date", "late_start_date", "late_end_date", "expect_end_date", "early_start_date", "early_end_date", "restart_date", "reend_date",
    "target_start_date", "target_end_date", "rem_late_start_date", "rem_late_end_date", "cstr_type", "priority_type", "suspend_date", "resume_date", "float_path", "float_path_order", "guid", "tmpl_guid",
    "cstr_date2", "cstr_type2", "driving_path_flag", "act_this_per_work_qty", "act_this_per_equip_qty", "external_early_start_date", "external_late_end_date", "create_date", "update_date", "create_user",
    "update_user", "location_id",
];
const TASKPRED_FIELDS: [&str; 10] = ["task_pred_id", "task_id", "pred_task_id", "proj_id", "pred_proj_id", "pred_type", "lag_hr_cnt", "float_path", "aref", "arls"];

fn cstr_code(c: Constraint) -> (&'static str, Date, &'static str) {
    match c {
        Constraint::StartNoEarlierThan(d) => ("CS_MSOA", d, "08:00"),
        Constraint::FinishNoLaterThan(d) => ("CS_MEOB", d, "17:00"),
        Constraint::MustStartOn(d) => ("CS_MANDSTART", d, "08:00"),
    }
}

/// Writes the programme as XER, as [`export_xer_with_report`] but without the report.
pub fn export_xer(p: &Programme, s: Option<&Schedule>) -> String {
    export_xer_with_report(p, s).0
}

/// Writes the programme as an XER file: one project with its calendars, WBS (the summary tasks),
/// activities and relationships. When `s` is `None` the programme is scheduled for the date fields;
/// a programme that cannot be scheduled is written with empty dates and a warning.
///
/// Tables: PROJECT, CALENDAR, SCHEDOPTIONS, PROJWBS, TASK, TASKPRED. The report lists what is not
/// carried (see the README). Not opened in P6.
pub fn export_xer_with_report(p: &Programme, s: Option<&Schedule>) -> (String, ExportReport) {
    let mut rep = ExportReport::default();
    let computed;
    let sched: Option<&Schedule> = match s {
        Some(s) => Some(s),
        None => {
            computed = schedule(p).ok();
            if computed.is_none() {
                rep.warnings.push("the programme cannot be scheduled, so the date fields of the file are empty".into());
            }
            computed.as_ref()
        }
    };
    let hpd = HOURS_PER_DAY;
    let cal_id = |name: &str| p.calendars.iter().position(|c| c.name == name).map(|i| i + 1).unwrap_or(1);
    let proj_id = "1".to_string();
    let finish = sched.map(|s| s.project_finish).unwrap_or(p.start);
    let dd = p.data_date;

    let mut out = Out { s: String::new() };
    out.s.push_str("ERMHDR\t8.2.0\t");
    out.s.push_str(&format!("{}\tProject\tadmin\tAfroBIM\tdbxDatabaseNoName\tProject Management\tUSD\n", p.start));

    // PROJECT: flags and options copied from a P6 8.2.0 export (see the README); dates from the programme.
    let mut v: Vec<(&str, String)> = vec![
        ("proj_id", proj_id.clone()),
        ("fy_start_month_num", "1".into()),
        ("rsrc_self_add_flag", "Y".into()),
        ("allow_complete_flag", "Y".into()),
        ("rsrc_multi_assign_flag", "Y".into()),
        ("checkout_flag", "N".into()),
        ("project_flag", "Y".into()),
        ("step_complete_flag", "N".into()),
        ("cost_qty_recalc_flag", "N".into()),
        ("batch_sum_flag", "Y".into()),
        ("name_sep_char", ".".into()),
        ("def_complete_pct_type", "CP_Phys".into()),
        ("proj_short_name", p.name.clone()),
        ("clndr_id", cal_id(&p.calendar).to_string()),
        ("task_code_base", "1000".into()),
        ("task_code_step", "10".into()),
        ("priority_num", "10".into()),
        ("critical_drtn_hr_cnt", "0".into()),
        ("def_cost_per_qty", "0.00".into()),
        ("last_recalc_date", stamp(dd.unwrap_or(p.start), if dd.is_some() { "17:00" } else { "08:00" })),
        ("plan_start_date", stamp(p.start, "08:00")),
        ("scd_end_date", stamp(finish, "17:00")),
        ("add_date", stamp(p.start, "08:00")),
        ("def_duration_type", "DT_FixedDrtn".into()),
        ("task_code_prefix", "A".into()),
        ("def_qty_type", "QT_Hour".into()),
        ("add_by_name", "admin".into()),
        ("def_rate_type", "COST_PER_QTY".into()),
        ("add_act_remain_flag", "N".into()),
        ("act_this_per_link_flag", "Y".into()),
        ("def_task_type", "TT_Task".into()),
        ("act_pct_link_flag", "Y".into()),
        ("critical_path_type", "CT_TotFloat".into()),
        ("task_code_prefix_flag", "Y".into()),
        ("def_rollup_dates_flag", "Y".into()),
        ("use_project_baseline_flag", "Y".into()),
        ("rem_target_link_flag", "Y".into()),
        ("reset_planned_flag", "N".into()),
        ("allow_neg_act_flag", "N".into()),
        ("sum_assign_level", "SL_Taskrsrc".into()),
        ("loaded_scope_level", "7".into()),
        ("export_flag", "Y".into()),
    ];
    if let Some(rf) = p.required_finish {
        v.push(("plan_end_date", stamp(rf, "17:00")));
    }
    out.table("PROJECT", &PROJECT_FIELDS, &[row_of(&PROJECT_FIELDS, &v)]);

    // CALENDAR.
    let mut cal_rows = Vec::new();
    for (i, c) in p.calendars.iter().enumerate() {
        let week = hpd * c.working_days.len() as f64;
        cal_rows.push(row_of(
            &CALENDAR_FIELDS,
            &[
                ("clndr_id", (i + 1).to_string()),
                ("default_flag", if c.name == p.calendar { "Y" } else { "N" }.into()),
                ("clndr_name", c.name.clone()),
                ("proj_id", proj_id.clone()),
                ("clndr_type", "CA_Project".into()),
                ("day_hr_cnt", num(hpd)),
                ("week_hr_cnt", num(week)),
                ("month_hr_cnt", num((week * 52.0 / 12.0).round())),
                ("year_hr_cnt", num(week * 52.0)),
                ("rsrc_private", "N".into()),
                ("clndr_data", write_calendar_data(c)),
            ],
        ));
    }
    out.table("CALENDAR", &CALENDAR_FIELDS, &cal_rows);

    // SCHEDOPTIONS: values of a P6 8.2.0 export, with the options this crate's engine implies.
    let so = [
        ("schedoptions_id", "1".to_string()),
        ("proj_id", proj_id.clone()),
        ("sched_outer_depend_type", "SD_Both".into()),
        ("sched_open_critical_flag", "N".into()),
        ("sched_lag_early_start_flag", "Y".into()),
        ("sched_retained_logic", "Y".into()),
        ("sched_setplantoforecast", "N".into()),
        ("sched_float_type", "FT_FF".into()),
        ("sched_calendar_on_relationship_lag", "rcal_Successor".into()),
        ("sched_use_expect_end_flag", "N".into()),
        ("sched_progress_override", "N".into()),
        ("level_float_thrs_cnt", "0".into()),
        ("level_outer_assign_flag", "N".into()),
        ("level_outer_assign_priority", "5".into()),
        ("level_over_alloc_pct", "25".into()),
        ("level_within_float_flag", "N".into()),
        ("level_keep_sched_date_flag", "Y".into()),
        ("level_all_rsrc_flag", "Y".into()),
        ("sched_use_project_end_date_for_float", if p.required_finish.is_some() { "Y" } else { "N" }.into()),
        ("enable_multiple_longest_path_calc", "N".into()),
        ("limit_multiple_longest_path_calc", "Y".into()),
        ("max_multiple_longest_path", "10".into()),
        ("use_total_float_multiple_longest_paths", "Y".into()),
    ];
    out.table("SCHEDOPTIONS", &SCHEDOPTIONS_FIELDS, &[row_of(&SCHEDOPTIONS_FIELDS, &so)]);

    // PROJWBS: the project node, then the summary tasks in outline order.
    let wbs_id_of: HashMap<&str, usize> = p.summaries.iter().enumerate().map(|(i, s)| (s.id.as_str(), i + 2)).collect();
    let wbs_row = |id: usize, seq: usize, root: bool, short: &str, name: &str, parent: Option<usize>| -> Vec<String> {
        row_of(
            &PROJWBS_FIELDS,
            &[
                ("wbs_id", id.to_string()),
                ("proj_id", proj_id.clone()),
                ("seq_num", (seq * 10).to_string()),
                ("proj_node_flag", if root { "Y" } else { "N" }.into()),
                ("sum_data_flag", "N".into()),
                ("status_code", "WS_Open".into()),
                ("wbs_short_name", short.to_string()),
                ("wbs_name", name.to_string()),
                ("parent_wbs_id", parent.map(|x| x.to_string()).unwrap_or_default()),
                ("orig_cost", "0.00".into()),
                ("indep_remain_total_cost", "0.00".into()),
                ("ev_compute_type", "EC_Cmp_pct".into()),
                ("ev_etc_compute_type", "EE_Rem_hr".into()),
            ],
        )
    };
    let mut wbs_rows = vec![wbs_row(1, 1, true, &p.name, &p.name, None)];
    let mut seq = 2;
    for it in outline(p) {
        if let OutlineKind::Summary(k) = it.kind {
            let sm: &SummaryTask = &p.summaries[k];
            let parent_idx = sm.parent.as_deref().and_then(|q| wbs_id_of.get(q).copied());
            // The short name is the last component of the code when the code extends the parent's.
            let parent_code = sm.parent.as_deref().and_then(|q| p.summary(q)).map(|q| q.wbs.as_str());
            let short = match parent_code {
                Some(pc) if sm.wbs.starts_with(&format!("{pc}.")) => &sm.wbs[pc.len() + 1..],
                _ => sm.wbs.as_str(),
            };
            wbs_rows.push(wbs_row(wbs_id_of[sm.id.as_str()], seq, false, short, &sm.name, Some(parent_idx.unwrap_or(1))));
            seq += 1;
        }
    }
    out.table("PROJWBS", &PROJWBS_FIELDS, &wbs_rows);

    // TASK.
    let task_id_of: HashMap<&str, usize> = p.activities.iter().enumerate().map(|(i, a)| (a.id.as_str(), i + 1)).collect();
    let mut task_rows = Vec::new();
    for a in &p.activities {
        let row = sched.and_then(|s| s.row(&a.id));
        let status = a.status();
        let wbs_id = a.parent.as_deref().and_then(|q| wbs_id_of.get(q).copied()).unwrap_or(1);
        let ms = a.is_milestone();
        let at_end = ms && a.milestone_kind == MilestoneKind::Finish;
        let mut vals: Vec<(&str, String)> = vec![
            ("task_id", task_id_of[a.id.as_str()].to_string()),
            ("proj_id", proj_id.clone()),
            ("wbs_id", wbs_id.to_string()),
            ("clndr_id", cal_id(a.calendar.as_deref().unwrap_or(&p.calendar)).to_string()),
            ("phys_complete_pct", num(if status == Status::Complete { 100.0 } else { a.progress.percent_complete })),
            ("rev_fdbk_flag", "N".into()),
            ("lock_plan_flag", "N".into()),
            ("auto_compute_act_flag", "N".into()),
            ("complete_pct_type", "CP_Phys".into()),
            ("task_type", if !ms { "TT_Task" } else if at_end { "TT_FinMile" } else { "TT_Mile" }.into()),
            ("duration_type", "DT_FixedDrtn".into()),
            ("status_code", match status {
                Status::NotStarted => "TK_NotStart",
                Status::InProgress => "TK_Active",
                Status::Complete => "TK_Complete",
            }.into()),
            ("task_code", a.id.clone()),
            ("task_name", a.name.clone()),
            ("remain_drtn_hr_cnt", num(a.remaining() as f64 * hpd)),
            ("act_work_qty", "0".into()),
            ("remain_work_qty", "0".into()),
            ("target_work_qty", "0".into()),
            ("target_drtn_hr_cnt", num(a.duration as f64 * hpd)),
            ("target_equip_qty", "0".into()),
            ("act_equip_qty", "0".into()),
            ("remain_equip_qty", "0".into()),
            ("priority_type", "PT_Normal".into()),
            ("act_this_per_work_qty", "0".into()),
            ("act_this_per_equip_qty", "0".into()),
        ];
        if let Some(d) = a.progress.actual_start {
            vals.push(("act_start_date", stamp(d, "08:00")));
        }
        if let Some(d) = a.progress.actual_finish {
            vals.push(("act_end_date", stamp(d, "17:00")));
            if ms && a.progress.actual_start.is_none() && !at_end {
                vals.push(("act_start_date", stamp(d, "08:00")));
            }
        }
        let all: Vec<Constraint> = a.constraints().collect();
        if all.len() > 2 {
            rep.warnings.push(format!("activity {}: XER holds two constraints and it has {}; only the first two were written", a.id, all.len()));
        }
        if let Some(c) = all.first() {
            let (code, d, t) = cstr_code(*c);
            vals.push(("cstr_type", code.into()));
            vals.push(("cstr_date", stamp(d, t)));
        }
        if let Some(c) = all.get(1) {
            let (code, d, t) = cstr_code(*c);
            vals.push(("cstr_type2", code.into()));
            vals.push(("cstr_date2", stamp(d, t)));
        }
        if let Some(r) = row {
            let (st, ft) = if ms { if at_end { ("17:00", "17:00") } else { ("08:00", "08:00") } } else { ("08:00", "17:00") };
            let (ls, lf) = (stamp(r.late_start, st), stamp(r.late_finish, ft));
            vals.push(("early_start_date", stamp(r.early_start, st)));
            vals.push(("early_end_date", stamp(r.early_finish, ft)));
            vals.push(("late_start_date", ls));
            vals.push(("late_end_date", lf));
            vals.push(("target_start_date", stamp(r.early_start, st)));
            vals.push(("target_end_date", stamp(r.early_finish, ft)));
            vals.push(("total_float_hr_cnt", num(r.total_float as f64 * hpd)));
            vals.push(("free_float_hr_cnt", num(r.free_float as f64 * hpd)));
            vals.push(("driving_path_flag", if r.on_longest_path { "Y" } else { "N" }.into()));
        }
        task_rows.push(row_of(&TASK_FIELDS, &vals));
    }
    out.table("TASK", &TASK_FIELDS, &task_rows);

    // TASKPRED.
    let mut pred_rows = Vec::new();
    for (k, d) in p.dependencies.iter().enumerate() {
        let (Some(ps), Some(ss)) = (task_id_of.get(d.predecessor.as_str()), task_id_of.get(d.successor.as_str())) else { continue };
        let t = match d.kind {
            LinkType::FS => "PR_FS",
            LinkType::SS => "PR_SS",
            LinkType::FF => "PR_FF",
            LinkType::SF => "PR_SF",
        };
        pred_rows.push(row_of(
            &TASKPRED_FIELDS,
            &[
                ("task_pred_id", (k + 1).to_string()),
                ("task_id", ss.to_string()),
                ("pred_task_id", ps.to_string()),
                ("proj_id", proj_id.clone()),
                ("pred_proj_id", proj_id.clone()),
                ("pred_type", t.into()),
                ("lag_hr_cnt", num(d.lag as f64 * hpd)),
            ],
        ));
    }
    out.table("TASKPRED", &TASKPRED_FIELDS, &pred_rows);
    out.s.push_str("%E\n");

    // What is not carried.
    if p.baseline.is_some() {
        rep.warnings.push("the baseline is not written (XER baselines are separate projects)".into());
    }
    if !p.links.is_empty() {
        rep.warnings.push("the links to model elements are not written".into());
    }
    if p.activities.iter().any(|a| a.weight.is_some()) {
        rep.warnings.push("activity weights are not written".into());
    }
    if p.activities.iter().any(|a| a.location.is_some()) {
        rep.warnings.push("activity location ranges (time-location chart) are not written".into());
    }
    if p.summaries.is_empty() && p.activities.iter().any(|a| !a.wbs.is_empty()) {
        rep.warnings.push("the programme has WBS codes on activities but no summary tasks; XER holds the WBS only as nodes, so the codes are not written (give the programme summary tasks)".into());
    }
    (out.s, rep)
}
