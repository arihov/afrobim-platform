//! Microsoft Project XML (MSPDI) exchange: tasks, predecessor links with type and lag, calendars'
//! working weekdays and holidays, constraints, percent complete and actual dates.
//!
//! Element names and enumerations follow Microsoft's published MSPDI reference (see the README for
//! which were verified). One working day is written as [`MINUTES_PER_DAY`] minutes because MSPDI
//! expresses durations in time, not days; the value is a file-format convention, and an imported
//! file's own `MinutesPerDay` is always used to read it back.

use std::collections::{BTreeSet, HashMap};

use roxmltree::{Document as Xml, Node};

use crate::calendar::Calendar;
use crate::cpm::Schedule;
use crate::date::{Date, Weekday};
use crate::model::{Activity, Constraint, LinkType, MilestoneKind, Programme, Status};

/// Minutes written for one working day (8 hours, 08:00-12:00 and 13:00-17:00).
pub const MINUTES_PER_DAY: i64 = 480;
/// The `Text1` custom field, which carries the activity id (`pjCustomTaskText1`).
const TEXT1_FIELD_ID: &str = "188743731";
const NAMESPACE: &str = "http://schemas.microsoft.com/project";

#[derive(Debug, thiserror::Error)]
pub enum MspdiError {
    #[error("not valid XML: {0}")]
    Xml(String),
    #[error("not an MS Project XML file: the root element is '{0}', expected 'Project'")]
    NotMspdi(String),
    #[error("the file has no {0}")]
    Missing(&'static str),
    #[error("{0}")]
    Unsupported(String),
}

/// What the import could not carry over. Nothing is dropped silently.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportReport {
    pub warnings: Vec<String>,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn dur(days: i64) -> String {
    let m = days * MINUTES_PER_DAY;
    format!("PT{}H{}M0S", m / 60, m % 60)
}

// MSPDI DayType: 1 = Sunday ... 7 = Saturday.
fn day_type(w: Weekday) -> u32 {
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

fn weekday_of(day_type: u32) -> Option<Weekday> {
    Weekday::ALL.iter().copied().find(|w| self::day_type(*w) == day_type)
}

/// Writes the programme as MSPDI. When `schedule` is given, each task also carries its early
/// start and finish so a viewer shows dates without recalculating.
///
/// Not carried: milestone kind, weights, 4D links, baseline, required finish, free-form notes.
pub fn export_mspdi(p: &Programme, schedule: Option<&Schedule>) -> String {
    let mut x = String::new();
    let uid_of: HashMap<&str, usize> = p.activities.iter().enumerate().map(|(i, a)| (a.id.as_str(), i + 1)).collect();
    let cal_uid = |name: &str| p.calendars.iter().position(|c| c.name == name).map(|i| i + 1).unwrap_or(1);
    let pcal = p.calendar_named(&p.calendar);
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    x.push_str(&format!("<Project xmlns=\"{NAMESPACE}\">\n"));
    x.push_str("  <SaveVersion>12</SaveVersion>\n");
    x.push_str(&format!("  <Name>{}</Name>\n  <Title>{}</Title>\n", esc(&p.name), esc(&p.name)));
    x.push_str("  <ScheduleFromStart>1</ScheduleFromStart>\n");
    x.push_str(&format!("  <StartDate>{}T08:00:00</StartDate>\n", p.start));
    if let Some(s) = schedule {
        x.push_str(&format!("  <FinishDate>{}T17:00:00</FinishDate>\n", s.project_finish));
    }
    x.push_str(&format!("  <CalendarUID>{}</CalendarUID>\n", cal_uid(&p.calendar)));
    x.push_str("  <DefaultStartTime>08:00:00</DefaultStartTime>\n  <DefaultFinishTime>17:00:00</DefaultFinishTime>\n");
    x.push_str(&format!("  <MinutesPerDay>{MINUTES_PER_DAY}</MinutesPerDay>\n"));
    x.push_str(&format!("  <MinutesPerWeek>{}</MinutesPerWeek>\n", MINUTES_PER_DAY * pcal.map_or(5, |c| c.working_days.len() as i64)));
    x.push_str("  <DurationFormat>7</DurationFormat>\n");
    if let Some(dd) = p.data_date {
        x.push_str(&format!("  <StatusDate>{dd}T17:00:00</StatusDate>\n"));
    }
    x.push_str(&format!(
        "  <ExtendedAttributes>\n    <ExtendedAttribute>\n      <FieldID>{TEXT1_FIELD_ID}</FieldID>\n      <FieldName>Text1</FieldName>\n      <Alias>Activity ID</Alias>\n    </ExtendedAttribute>\n  </ExtendedAttributes>\n"
    ));
    x.push_str("  <Calendars>\n");
    for (i, c) in p.calendars.iter().enumerate() {
        x.push_str(&format!("    <Calendar>\n      <UID>{}</UID>\n      <Name>{}</Name>\n      <IsBaseCalendar>1</IsBaseCalendar>\n      <WeekDays>\n", i + 1, esc(&c.name)));
        let mut order = Weekday::ALL.to_vec();
        order.sort_by_key(|w| day_type(*w));
        for w in order {
            if c.working_days.contains(&w) {
                x.push_str(&format!(
                    "        <WeekDay>\n          <DayType>{}</DayType>\n          <DayWorking>1</DayWorking>\n          <WorkingTimes>\n            <WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime>\n            <WorkingTime><FromTime>13:00:00</FromTime><ToTime>17:00:00</ToTime></WorkingTime>\n          </WorkingTimes>\n        </WeekDay>\n",
                    day_type(w)
                ));
            } else {
                x.push_str(&format!("        <WeekDay>\n          <DayType>{}</DayType>\n          <DayWorking>0</DayWorking>\n        </WeekDay>\n", day_type(w)));
            }
        }
        x.push_str("      </WeekDays>\n");
        if !c.holidays.is_empty() {
            x.push_str("      <Exceptions>\n");
            for h in &c.holidays {
                x.push_str(&format!(
                    "        <Exception>\n          <EnteredByOccurrences>0</EnteredByOccurrences>\n          <TimePeriod>\n            <FromDate>{h}T00:00:00</FromDate>\n            <ToDate>{h}T23:59:00</ToDate>\n          </TimePeriod>\n          <Occurrences>1</Occurrences>\n          <Name>Non-working day</Name>\n          <Type>1</Type>\n          <DayWorking>0</DayWorking>\n        </Exception>\n"
                ));
            }
            x.push_str("      </Exceptions>\n");
        }
        x.push_str("    </Calendar>\n");
    }
    x.push_str("  </Calendars>\n  <Tasks>\n");
    for (i, a) in p.activities.iter().enumerate() {
        let uid = i + 1;
        let row = schedule.and_then(|s| s.row(&a.id));
        let status = a.status();
        x.push_str(&format!("    <Task>\n      <UID>{uid}</UID>\n      <ID>{uid}</ID>\n      <Name>{}</Name>\n      <Type>1</Type>\n      <IsNull>0</IsNull>\n", esc(&a.name)));
        x.push_str(&format!("      <WBS>{}</WBS>\n      <OutlineNumber>{uid}</OutlineNumber>\n      <OutlineLevel>1</OutlineLevel>\n", esc(&a.wbs)));
        if let Some(r) = row {
            // A finish milestone sits at the end of its day, everything else starts at 08:00.
            let at_end = a.is_milestone() && a.milestone_kind == MilestoneKind::Finish;
            let start_time = if at_end { "17:00:00" } else { "08:00:00" };
            let finish_time = if a.is_milestone() && !at_end { "08:00:00" } else { "17:00:00" };
            x.push_str(&format!("      <Start>{}T{start_time}</Start>\n      <Finish>{}T{finish_time}</Finish>\n", r.early_start, r.early_finish));
        }
        x.push_str(&format!("      <Duration>{}</Duration>\n      <DurationFormat>7</DurationFormat>\n", dur(a.duration as i64)));
        x.push_str(&format!("      <Milestone>{}</Milestone>\n      <Summary>0</Summary>\n", u8::from(a.is_milestone())));
        x.push_str(&format!("      <PercentComplete>{}</PercentComplete>\n", if status == Status::Complete { 100 } else { a.progress.percent_complete.round() as i64 }));
        if let Some(d) = a.progress.actual_start {
            x.push_str(&format!("      <ActualStart>{d}T08:00:00</ActualStart>\n"));
        }
        if let Some(d) = a.progress.actual_finish {
            x.push_str(&format!("      <ActualFinish>{d}T17:00:00</ActualFinish>\n"));
        }
        x.push_str(&format!("      <RemainingDuration>{}</RemainingDuration>\n", dur(a.remaining() as i64)));
        let (ct, cd) = match a.constraint {
            None => (0, None),
            Some(Constraint::MustStartOn(d)) => (2, Some(format!("{d}T08:00:00"))),
            Some(Constraint::StartNoEarlierThan(d)) => (4, Some(format!("{d}T08:00:00"))),
            Some(Constraint::FinishNoLaterThan(d)) => (7, Some(format!("{d}T17:00:00"))),
        };
        x.push_str(&format!("      <ConstraintType>{ct}</ConstraintType>\n"));
        x.push_str(&format!("      <CalendarUID>{}</CalendarUID>\n", a.calendar.as_deref().map_or(-1, |c| cal_uid(c) as i64)));
        if let Some(cd) = cd {
            x.push_str(&format!("      <ConstraintDate>{cd}</ConstraintDate>\n"));
        }
        for d in p.dependencies.iter().filter(|d| d.successor == a.id) {
            let Some(pu) = uid_of.get(d.predecessor.as_str()) else { continue };
            let t = match d.kind {
                LinkType::FF => 0,
                LinkType::FS => 1,
                LinkType::SF => 2,
                LinkType::SS => 3,
            };
            x.push_str(&format!(
                "      <PredecessorLink>\n        <PredecessorUID>{pu}</PredecessorUID>\n        <Type>{t}</Type>\n        <CrossProject>0</CrossProject>\n        <LinkLag>{}</LinkLag>\n        <LagFormat>7</LagFormat>\n      </PredecessorLink>\n",
                d.lag as i64 * MINUTES_PER_DAY * 10
            ));
        }
        x.push_str(&format!("      <ExtendedAttribute>\n        <FieldID>{TEXT1_FIELD_ID}</FieldID>\n        <Value>{}</Value>\n      </ExtendedAttribute>\n", esc(&a.id)));
        x.push_str("    </Task>\n");
    }
    x.push_str("  </Tasks>\n</Project>\n");
    x
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn text<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(n, name).and_then(|c| c.text()).map(str::trim)
}

fn int(n: Node, name: &str) -> Option<i64> {
    text(n, name).and_then(|t| t.parse().ok())
}

/// An `xs:duration` (`PT40H0M0S`, `P1DT8H`) in minutes.
fn duration_minutes(s: &str) -> Option<f64> {
    let s = s.trim().strip_prefix('P')?;
    let (date_part, time_part) = s.split_once('T').unwrap_or((s, ""));
    let mut total = 0.0;
    let mut num = String::new();
    for ch in date_part.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            num.push(ch);
        } else {
            let v: f64 = num.parse().ok()?;
            num.clear();
            match ch {
                'D' => total += v * 24.0 * 60.0,
                'W' => total += v * 7.0 * 24.0 * 60.0,
                _ => return None,
            }
        }
    }
    for ch in time_part.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            num.push(ch);
        } else {
            let v: f64 = num.parse().ok()?;
            num.clear();
            match ch {
                'H' => total += v * 60.0,
                'M' => total += v,
                'S' => total += v / 60.0,
                _ => return None,
            }
        }
    }
    Some(total)
}

/// Reads an MSPDI file into a programme. Summary tasks and null tasks are skipped; links to them
/// are dropped; whatever cannot be represented is listed in the report.
pub fn import_mspdi(xml: &str) -> Result<(Programme, ImportReport), MspdiError> {
    let doc = Xml::parse(xml).map_err(|e| MspdiError::Xml(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "Project" {
        return Err(MspdiError::NotMspdi(root.tag_name().name().to_string()));
    }
    let mut rep = ImportReport::default();
    let mpd = int(root, "MinutesPerDay").ok_or(MspdiError::Missing("MinutesPerDay (needed to turn durations into working days)"))?;
    if mpd <= 0 {
        return Err(MspdiError::Unsupported("MinutesPerDay must be positive".into()));
    }
    if text(root, "ScheduleFromStart") == Some("0") {
        return Err(MspdiError::Unsupported("projects scheduled from the finish date are not supported".into()));
    }
    let start = text(root, "StartDate").and_then(Date::parse).ok_or(MspdiError::Missing("StartDate"))?;
    let proj_cal_uid = int(root, "CalendarUID").ok_or(MspdiError::Missing("project CalendarUID"))?;

    // Calendars: first pass reads own data, second resolves base calendars.
    struct RawCal {
        uid: i64,
        name: String,
        weekdays: Option<BTreeSet<Weekday>>,
        holidays: BTreeSet<Date>,
        base: Option<i64>,
    }
    let mut raw = Vec::new();
    if let Some(cals) = child(root, "Calendars") {
        for c in cals.children().filter(|c| c.is_element() && c.tag_name().name() == "Calendar") {
            let uid = int(c, "UID").ok_or(MspdiError::Missing("Calendar UID"))?;
            let name = text(c, "Name").unwrap_or("").to_string();
            let mut weekdays = None;
            if let Some(wd) = child(c, "WeekDays") {
                let mut set = BTreeSet::new();
                for w in wd.children().filter(|w| w.is_element() && w.tag_name().name() == "WeekDay") {
                    let (Some(t), Some(working)) = (int(w, "DayType"), int(w, "DayWorking")) else { continue };
                    if t == 0 {
                        rep.warnings.push(format!("calendar '{name}': a WeekDay exception entry was skipped"));
                    } else if let (Some(d), true) = (weekday_of(t as u32), working == 1) {
                        set.insert(d);
                    }
                }
                weekdays = Some(set);
            }
            let mut holidays = BTreeSet::new();
            if let Some(ex) = child(c, "Exceptions") {
                for e in ex.children().filter(|e| e.is_element() && e.tag_name().name() == "Exception") {
                    let working = int(e, "DayWorking").unwrap_or(0);
                    let (from, to) = match child(e, "TimePeriod") {
                        Some(tp) => (text(tp, "FromDate").and_then(Date::parse), text(tp, "ToDate").and_then(Date::parse)),
                        None => (None, None),
                    };
                    let (Some(from), Some(to)) = (from, to) else {
                        rep.warnings.push(format!("calendar '{name}': an exception without a date range was skipped"));
                        continue;
                    };
                    if working == 1 {
                        rep.warnings.push(format!("calendar '{name}': a working exception ({from}) was skipped; only non-working days are read"));
                        continue;
                    }
                    if int(e, "Type").is_some_and(|t| t != 1) || int(e, "Occurrences").is_some_and(|o| o != to.days_since(from) + 1 && o != 1) {
                        rep.warnings.push(format!("calendar '{name}': a recurring exception starting {from} was read as the dates {from} to {to} only"));
                    }
                    let mut d = from;
                    while d <= to && d.days_since(from) < 3660 {
                        holidays.insert(d);
                        d = d.add_days(1);
                    }
                }
            }
            if child(c, "WorkWeeks").is_some() {
                rep.warnings.push(format!("calendar '{name}': work weeks (changed working times for a period) are not supported and were ignored"));
            }
            raw.push(RawCal { uid, name, weekdays, holidays, base: int(c, "BaseCalendarUID").filter(|b| *b >= 0) });
        }
    }
    let mut calendars: Vec<Calendar> = Vec::new();
    let mut cal_name_of: HashMap<i64, String> = HashMap::new();
    for rc in &raw {
        let (weekdays, mut holidays) = match &rc.weekdays {
            Some(w) => (w.clone(), rc.holidays.clone()),
            None => match rc.base.and_then(|b| raw.iter().find(|x| x.uid == b)) {
                Some(b) if b.weekdays.is_some() => (b.weekdays.clone().unwrap_or_default(), rc.holidays.clone()),
                _ => {
                    rep.warnings.push(format!("calendar '{}' has no week days and no usable base calendar and was skipped", rc.name));
                    continue;
                }
            },
        };
        if let (None, Some(b)) = (&rc.weekdays, rc.base.and_then(|b| raw.iter().find(|x| x.uid == b))) {
            holidays.extend(b.holidays.iter().copied());
        }
        let mut name = if rc.name.is_empty() { format!("Calendar {}", rc.uid) } else { rc.name.clone() };
        if calendars.iter().any(|c| c.name == name) {
            name = format!("{name} ({})", rc.uid);
        }
        cal_name_of.insert(rc.uid, name.clone());
        calendars.push(Calendar { name, working_days: weekdays, holidays });
    }
    let calendar = cal_name_of.get(&proj_cal_uid).cloned().ok_or(MspdiError::Missing("calendar for the project CalendarUID"))?;
    let mut p = Programme { name: text(root, "Title").or(text(root, "Name")).unwrap_or("").to_string(), start, calendars, calendar, activities: vec![], dependencies: vec![], links: vec![], data_date: text(root, "StatusDate").and_then(Date::parse), required_finish: None, baseline: None };

    let days_of = |minutes: f64, what: &str, id: &str, rep: &mut ImportReport| -> u32 {
        let d = minutes / mpd as f64;
        if (d - d.round()).abs() > 1e-6 {
            rep.warnings.push(format!("task {id}: {what} of {d:.2} working days was rounded up to a whole day"));
            d.ceil().max(0.0) as u32
        } else {
            d.round().max(0.0) as u32
        }
    };

    let mut id_of_uid: HashMap<i64, String> = HashMap::new();
    let mut pending: Vec<(String, i64, LinkType, f64, i64)> = Vec::new(); // successor id, pred uid, type, lag minutes, lag format
    if let Some(tasks) = child(root, "Tasks") {
        for t in tasks.children().filter(|t| t.is_element() && t.tag_name().name() == "Task") {
            let uid = int(t, "UID").ok_or(MspdiError::Missing("Task UID"))?;
            if int(t, "IsNull") == Some(1) {
                continue;
            }
            let name = text(t, "Name").unwrap_or("").to_string();
            if int(t, "Summary") == Some(1) {
                if uid != 0 {
                    rep.warnings.push(format!("summary task '{name}' (UID {uid}) was skipped; its subtasks were imported"));
                }
                continue;
            }
            let id = t
                .children()
                .filter(|c| c.is_element() && c.tag_name().name() == "ExtendedAttribute")
                .find(|e| text(*e, "FieldID") == Some(TEXT1_FIELD_ID))
                .and_then(|e| text(e, "Value"))
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| uid.to_string());
            id_of_uid.insert(uid, id.clone());
            let milestone = int(t, "Milestone") == Some(1);
            let minutes = text(t, "Duration").and_then(duration_minutes).ok_or(MspdiError::Missing("Task Duration"))?;
            let mut a = Activity::new(&id, text(t, "WBS").unwrap_or(""), &name, if milestone { 0 } else { days_of(minutes, "duration", &id, &mut rep) });
            if !milestone && a.duration == 0 {
                rep.warnings.push(format!("task {id} has zero duration but is not marked as a milestone; it is imported as a milestone"));
            }
            if let Some(cu) = int(t, "CalendarUID").filter(|c| *c >= 0) {
                match cal_name_of.get(&cu) {
                    Some(n) if *n != p.calendar => a.calendar = Some(n.clone()),
                    Some(_) => {}
                    None => rep.warnings.push(format!("task {id} refers to calendar UID {cu}, which was not imported; the project calendar is used")),
                }
            }
            let cdate = text(t, "ConstraintDate").and_then(Date::parse);
            match (int(t, "ConstraintType").unwrap_or(0), cdate) {
                (0, _) => {}
                (2, Some(d)) => a.constraint = Some(Constraint::MustStartOn(d)),
                (4, Some(d)) => a.constraint = Some(Constraint::StartNoEarlierThan(d)),
                (7, Some(d)) => a.constraint = Some(Constraint::FinishNoLaterThan(d)),
                (k, _) => rep.warnings.push(format!("task {id}: constraint type {k} is not supported (supported: 2 must start on, 4 start no earlier than, 7 finish no later than) and was ignored")),
            }
            let pct = int(t, "PercentComplete").unwrap_or(0) as f64;
            a.progress.percent_complete = pct.clamp(0.0, 100.0);
            a.progress.actual_start = text(t, "ActualStart").and_then(Date::parse);
            a.progress.actual_finish = text(t, "ActualFinish").and_then(Date::parse);
            if a.progress.percent_complete >= 100.0 && a.progress.actual_finish.is_none() {
                a.progress.actual_finish = text(t, "Finish").and_then(Date::parse);
                rep.warnings.push(format!("task {id} is 100 percent complete without an actual finish; its Finish date was used"));
            }
            if a.progress.percent_complete > 0.0 && a.progress.actual_start.is_none() {
                a.progress.actual_start = text(t, "Start").and_then(Date::parse);
                rep.warnings.push(format!("task {id} has progress without an actual start; its Start date was used"));
            }
            if a.status() == Status::InProgress {
                a.progress.remaining_duration = text(t, "RemainingDuration").and_then(duration_minutes).map(|m| days_of(m, "remaining duration", &id, &mut rep)).filter(|d| *d > 0);
            }
            if milestone {
                a.milestone_kind = MilestoneKind::Finish;
            }
            for l in t.children().filter(|c| c.is_element() && c.tag_name().name() == "PredecessorLink") {
                let Some(pu) = int(l, "PredecessorUID") else { continue };
                let kind = match int(l, "Type").unwrap_or(1) {
                    0 => LinkType::FF,
                    1 => LinkType::FS,
                    2 => LinkType::SF,
                    3 => LinkType::SS,
                    k => {
                        rep.warnings.push(format!("task {id}: link type {k} is not valid and the link was skipped"));
                        continue;
                    }
                };
                if int(l, "CrossProject") == Some(1) {
                    rep.warnings.push(format!("task {id}: a link to another project was skipped"));
                    continue;
                }
                pending.push((id.clone(), pu, kind, int(l, "LinkLag").unwrap_or(0) as f64 / 10.0, int(l, "LagFormat").unwrap_or(7)));
            }
            p.activities.push(a);
        }
    }
    for (succ, pu, kind, lag_min, fmt) in pending {
        let Some(pred) = id_of_uid.get(&pu) else {
            rep.warnings.push(format!("task {succ}: the link from UID {pu} was dropped (the predecessor is a summary, null or unknown task)"));
            continue;
        };
        if matches!(fmt, 4 | 6 | 8 | 10 | 12 | 19 | 20) {
            rep.warnings.push(format!("task {succ}: lag format {fmt} (elapsed time or percent) is not supported; the lag was read as working time"));
        }
        let days = lag_min / mpd as f64;
        if (days - days.round()).abs() > 1e-6 {
            rep.warnings.push(format!("task {succ}: lag of {days:.2} working days was rounded to a whole day"));
        }
        p.dependencies.push(crate::model::Dependency { predecessor: pred.clone(), successor: succ, kind, lag: days.round() as i32 });
    }
    Ok((p, rep))
}
