//! Critical path method over working-day calendars. The formulae are in the README; the helper
//! functions here follow its notation.

use std::collections::{HashMap, HashSet};

use afrobim_kernel::Severity;
use serde::{Deserialize, Serialize};

use crate::calendar::Calendar;
use crate::date::Date;
use crate::model::{Constraint, LinkType, MilestoneKind, Programme, Status};

/// A finding about the programme: an error stops scheduling, a warning does not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub severity: Severity,
    /// Stable machine-readable code, e.g. `cycle`, `unknown_activity`, `open_end`.
    pub code: String,
    pub message: String,
    pub activity: Option<String>,
}

fn issue(severity: Severity, code: &str, message: String, activity: Option<&str>) -> Issue {
    Issue { severity, code: code.to_string(), message, activity: activity.map(str::to_string) }
}

#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error("the programme cannot be scheduled:\n{}", .0.iter().map(|i| format!("  - {}", i.message)).collect::<Vec<_>>().join("\n"))]
    Invalid(Vec<Issue>),
    #[error("{0}")]
    Other(String),
}

/// One activity as scheduled. Float is in working days of the activity's calendar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub id: String,
    pub wbs: String,
    pub name: String,
    pub status: Status,
    pub calendar: String,
    pub is_milestone: bool,
    /// Working days: the entered duration, or for work already started the span from actual start
    /// to the (actual or forecast) finish.
    pub duration: u32,
    /// Working days still to do: duration if not started, 0 if complete.
    pub remaining_duration: u32,
    pub percent_complete: f64,
    /// For work in progress or complete, the actual start; otherwise the early start.
    pub early_start: Date,
    pub early_finish: Date,
    pub late_start: Date,
    pub late_finish: Date,
    pub total_float: i64,
    pub free_float: i64,
    /// Total float of zero or less, and not complete.
    pub critical: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    /// In the programme's activity order.
    pub rows: Vec<Row>,
    pub project_start: Date,
    /// The latest early finish.
    pub project_finish: Date,
    /// The date float is measured against: the required finish when entered, else the project
    /// finish.
    pub finish_limit: Date,
    pub data_date: Option<Date>,
    /// Warnings found while scheduling (open ends, negative float, constraint conflicts).
    pub issues: Vec<Issue>,
}

impl Schedule {
    pub fn row(&self, id: &str) -> Option<&Row> {
        self.rows.iter().find(|r| r.id == id)
    }

    pub fn critical(&self) -> impl Iterator<Item = &Row> {
        self.rows.iter().filter(|r| r.critical)
    }
}

/// Checks the programme without scheduling it. Errors come first in the result.
pub fn validate(p: &Programme) -> Vec<Issue> {
    let mut out = Vec::new();
    let err = |out: &mut Vec<Issue>, code: &str, msg: String, a: Option<&str>| out.push(issue(Severity::Error, code, msg, a));

    if p.calendar_named(&p.calendar).is_none() {
        err(&mut out, "unknown_calendar", format!("the programme calendar '{}' is not defined", p.calendar), None);
    }
    for c in &p.calendars {
        if c.working_days.is_empty() {
            err(&mut out, "no_working_days", format!("calendar '{}' has no working weekday", c.name), None);
        }
    }
    let mut ids: HashMap<&str, usize> = HashMap::new();
    for (i, a) in p.activities.iter().enumerate() {
        if a.id.trim().is_empty() {
            err(&mut out, "empty_id", format!("activity {} has an empty id", i + 1), None);
        }
        if ids.insert(a.id.as_str(), i).is_some() {
            err(&mut out, "duplicate_activity", format!("activity id '{}' is used more than once", a.id), Some(&a.id));
        }
        if let Some(c) = &a.calendar {
            if p.calendar_named(c).is_none() {
                err(&mut out, "unknown_calendar", format!("activity '{}' uses calendar '{c}', which is not defined", a.id), Some(&a.id));
            }
        }
        if let Some(w) = a.weight {
            if !(w.is_finite() && w > 0.0) {
                err(&mut out, "bad_weight", format!("activity '{}' has a weight that is not a positive number", a.id), Some(&a.id));
            }
        }
        validate_progress(p, a, &mut out);
    }
    let mut edges: Vec<(usize, usize)> = Vec::new();
    let mut seen = HashSet::new();
    for (k, d) in p.dependencies.iter().enumerate() {
        let (pi, si) = (ids.get(d.predecessor.as_str()), ids.get(d.successor.as_str()));
        if pi.is_none() {
            err(&mut out, "unknown_activity", format!("dependency {} names predecessor '{}', which is not an activity (dangling logic)", k + 1, d.predecessor), Some(&d.successor));
        }
        if si.is_none() {
            err(&mut out, "unknown_activity", format!("dependency {} names successor '{}', which is not an activity (dangling logic)", k + 1, d.successor), Some(&d.predecessor));
        }
        if let (Some(&pi), Some(&si)) = (pi, si) {
            if pi == si {
                err(&mut out, "self_link", format!("activity '{}' is linked to itself", d.predecessor), Some(&d.predecessor));
            } else {
                edges.push((pi, si));
            }
            if !seen.insert((pi, si, d.kind)) {
                out.push(issue(Severity::Warning, "duplicate_link", format!("{:?} link {} -> {} is entered twice", d.kind, d.predecessor, d.successor), Some(&d.successor)));
            }
        }
    }
    if let Some(cycle) = find_cycle(p.activities.len(), &edges) {
        let path: Vec<&str> = cycle.iter().map(|&i| p.activities[i].id.as_str()).collect();
        err(&mut out, "cycle", format!("logic loop: {}", path.join(" -> ")), Some(path[0]));
    }
    if let Some(dd) = p.data_date {
        if dd < p.start && p.activities.iter().any(|a| a.status() != Status::NotStarted) {
            out.push(issue(Severity::Warning, "data_date_before_start", "the data date is before the programme start".into(), None));
        }
    }
    for l in &p.links {
        if !ids.contains_key(l.activity.as_str()) {
            err(&mut out, "unknown_activity", format!("an element link names activity '{}', which does not exist", l.activity), None);
        }
    }
    // Open ends: only when there is a network to speak of.
    if p.activities.len() > 1 {
        let mut has_pred = vec![false; p.activities.len()];
        let mut has_succ = vec![false; p.activities.len()];
        for &(a, b) in &edges {
            has_succ[a] = true;
            has_pred[b] = true;
        }
        for (i, a) in p.activities.iter().enumerate() {
            if !has_pred[i] {
                out.push(issue(Severity::Warning, "open_start", format!("activity '{}' has no predecessor (open start)", a.id), Some(&a.id)));
            }
            if !has_succ[i] {
                out.push(issue(Severity::Warning, "open_end", format!("activity '{}' has no successor (open end)", a.id), Some(&a.id)));
            }
        }
    }
    out.sort_by_key(|i| match i.severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    });
    out
}

fn validate_progress(p: &Programme, a: &crate::model::Activity, out: &mut Vec<Issue>) {
    let pr = &a.progress;
    let mut bad = |msg: String| out.push(issue(Severity::Error, "bad_progress", format!("activity '{}': {msg}", a.id), Some(&a.id)));
    if !(pr.percent_complete.is_finite() && (0.0..=100.0).contains(&pr.percent_complete)) {
        bad("percent complete must be between 0 and 100".into());
        return;
    }
    let status = a.status();
    if status == Status::NotStarted {
        return;
    }
    let Some(dd) = p.data_date else {
        bad("progress is entered but the programme has no data date".into());
        return;
    };
    if a.is_milestone() {
        if pr.actual_finish.is_none() {
            bad("a milestone is either achieved (actual finish) or not".into());
        }
    } else if pr.actual_start.is_none() {
        bad("progress needs an actual start".into());
    }
    if let (Some(s), Some(f)) = (pr.actual_start, pr.actual_finish) {
        if f < s {
            bad("the actual finish is before the actual start".into());
        }
    }
    for (what, d) in [("actual start", pr.actual_start), ("actual finish", pr.actual_finish)] {
        if let Some(d) = d {
            if d > dd {
                bad(format!("the {what} {d} is after the data date {dd}"));
            }
        }
    }
    if pr.actual_finish.is_some() && pr.percent_complete < 100.0 && pr.percent_complete > 0.0 {
        bad("an actual finish is entered but percent complete is not 100".into());
    }
    if pr.percent_complete >= 100.0 && pr.actual_finish.is_none() {
        bad("100 percent complete needs an actual finish".into());
    }
    if status == Status::InProgress && pr.remaining_duration == Some(0) {
        bad("remaining duration is 0 but there is no actual finish".into());
    }
}

/// A shortest-looking loop through the graph, as node indices with the first repeated at the
/// end, or `None`.
fn find_cycle(n: usize, edges: &[(usize, usize)]) -> Option<Vec<usize>> {
    let mut adj = vec![Vec::new(); n];
    for &(a, b) in edges {
        adj[a].push(b);
    }
    // 0 = unseen, 1 = on the stack, 2 = done
    let mut state = vec![0u8; n];
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        state[root] = 1;
        while let Some(&mut (node, ref mut next)) = stack.last_mut() {
            if *next < adj[node].len() {
                let to = adj[node][*next];
                *next += 1;
                match state[to] {
                    0 => {
                        state[to] = 1;
                        stack.push((to, 0));
                    }
                    1 => {
                        let at = stack.iter().position(|&(x, _)| x == to).expect("on stack");
                        let mut path: Vec<usize> = stack[at..].iter().map(|&(x, _)| x).collect();
                        path.push(to);
                        return Some(path);
                    }
                    _ => {}
                }
            } else {
                state[node] = 2;
                stack.pop();
            }
        }
    }
    None
}

#[derive(Clone, Copy)]
struct Edge {
    other: usize,
    kind: LinkType,
    lag: i64,
}

struct Net<'a> {
    cals: Vec<&'a Calendar>,
    ms: Vec<bool>,
    mk: Vec<MilestoneKind>,
    /// Duration still to schedule: the duration, or the remaining duration for work in progress.
    dur: Vec<i64>,
    status: Vec<Status>,
    preds: Vec<Vec<Edge>>,
    succs: Vec<Vec<Edge>>,
}

impl Net<'_> {
    /// The start of the activity as a relationship sees it (`S` in the README).
    fn s_eff(&self, i: usize, es: Date) -> Date {
        if self.ms[i] && self.mk[i] == MilestoneKind::Finish {
            self.cals[i].first_on_or_after(es.add_days(1))
        } else {
            es
        }
    }

    /// The finish as a relationship sees it (`F` in the README).
    fn f_eff(&self, i: usize, es: Date, ef: Date) -> Date {
        if self.ms[i] && self.mk[i] == MilestoneKind::Start {
            self.cals[i].last_on_or_before(es.add_days(-1))
        } else {
            ef
        }
    }

    /// The earliest date of activity `i` that one relationship allows, given the predecessor's
    /// effective start `ps` and finish `pf`. For work this is the start; for a milestone, its date.
    fn implied(&self, i: usize, kind: LinkType, lag: i64, ps: Date, pf: Date) -> Date {
        let cal = self.cals[i];
        let (finish_type, x, l) = match kind {
            LinkType::FS => (false, pf.add_days(1), lag),
            LinkType::SS => (false, ps, lag),
            LinkType::FF => (true, pf, lag),
            LinkType::SF => (true, ps, lag - 1),
        };
        let lb = cal.shift(cal.first_on_or_after(x), l);
        let d = self.dur[i];
        match (finish_type, d > 0, self.mk[i]) {
            (false, true, _) | (false, false, MilestoneKind::Start) => lb,
            (false, false, MilestoneKind::Finish) => cal.shift(lb, -1),
            (true, true, _) => cal.shift(lb, -(d - 1)),
            (true, false, MilestoneKind::Start) => cal.first_on_or_after(lb.add_days(1)),
            (true, false, MilestoneKind::Finish) => lb,
        }
    }
}

/// Runs the critical path method.
///
/// Fails with [`ScheduleError::Invalid`] when [`validate`] finds an error. See the README for
/// every formula.
pub fn schedule(p: &Programme) -> Result<Schedule, ScheduleError> {
    let mut issues = validate(p);
    if issues.iter().any(|i| i.severity == Severity::Error) {
        issues.retain(|i| i.severity == Severity::Error);
        return Err(ScheduleError::Invalid(issues));
    }
    let n = p.activities.len();
    if n == 0 {
        return Err(ScheduleError::Other("the programme has no activities".into()));
    }
    let idx: HashMap<&str, usize> = p.activities.iter().enumerate().map(|(i, a)| (a.id.as_str(), i)).collect();
    let mut net = Net {
        cals: p.activities.iter().map(|a| p.calendar_of(a).expect("validated")).collect(),
        ms: p.activities.iter().map(|a| a.is_milestone()).collect(),
        mk: p.activities.iter().map(|a| a.milestone_kind).collect(),
        dur: p.activities.iter().map(|a| match a.status() {
            Status::Complete => 0,
            Status::InProgress => a.remaining() as i64,
            Status::NotStarted => a.duration as i64,
        }).collect(),
        status: p.activities.iter().map(|a| if a.is_milestone() && a.progress.actual_finish.is_none() { Status::NotStarted } else { a.status() }).collect(),
        preds: vec![Vec::new(); n],
        succs: vec![Vec::new(); n],
    };
    for d in &p.dependencies {
        let (a, b) = (idx[d.predecessor.as_str()], idx[d.successor.as_str()]);
        net.preds[b].push(Edge { other: a, kind: d.kind, lag: d.lag as i64 });
        net.succs[a].push(Edge { other: b, kind: d.kind, lag: d.lag as i64 });
    }
    let order = topological_order(n, &net.preds);

    // Forward pass.
    let mut es = vec![p.start; n];
    let mut ef = vec![p.start; n];
    let base_floor = match p.data_date {
        Some(dd) => dd.add_days(1).max(p.start),
        None => p.start,
    };
    for &i in &order {
        let a = &p.activities[i];
        let cal = net.cals[i];
        if net.status[i] == Status::Complete {
            let fin = a.progress.actual_finish.expect("validated");
            es[i] = a.progress.actual_start.unwrap_or(fin);
            ef[i] = fin;
            continue;
        }
        let started = net.status[i] == Status::InProgress;
        let mut start = cal.first_on_or_after(base_floor);
        for e in &net.preds[i] {
            if started && e.kind == LinkType::SS {
                continue; // already started: the start-driven relationship is spent
            }
            let (ps, pf) = (net.s_eff(e.other, es[e.other]), net.f_eff(e.other, es[e.other], ef[e.other]));
            start = start.max(net.implied(i, e.kind, e.lag, ps, pf));
        }
        if !started {
            match a.constraint {
                Some(Constraint::StartNoEarlierThan(d)) => start = start.max(cal.first_on_or_after(d)),
                Some(Constraint::MustStartOn(d)) => {
                    let m = cal.first_on_or_after(d);
                    if m < start {
                        issues.push(issue(
                            Severity::Warning,
                            "mso_overrides_logic",
                            format!("activity '{}' must start on {m} but its logic needs {start}: the constraint overrides the logic", a.id),
                            Some(&a.id),
                        ));
                    }
                    start = m;
                }
                _ => {}
            }
        }
        let d = net.dur[i];
        ef[i] = if d > 0 { cal.shift(start, d - 1) } else { start };
        es[i] = if started { a.progress.actual_start.expect("validated") } else { start };
    }
    let project_finish = (0..n).map(|i| ef[i]).max().expect("n > 0");
    let finish_limit = p.required_finish.unwrap_or(project_finish);

    // Backward pass. Complete activities hold their dates and constrain nothing.
    let mut ls = es.clone();
    let mut lf = ef.clone();
    for &i in order.iter().rev() {
        if net.status[i] == Status::Complete {
            continue;
        }
        let a = &p.activities[i];
        let cal = net.cals[i];
        let started = net.status[i] == Status::InProgress;
        let d = net.dur[i];
        let mk = net.mk[i];
        let mut cap: Option<Date> = None;
        let mut put = |x: Date| cap = Some(cap.map_or(x, |c| c.min(x)));
        let s_bound = |x: Date, put: &mut dyn FnMut(Date)| {
            if started {
                return;
            }
            if d > 0 || mk == MilestoneKind::Start {
                put(x)
            } else {
                put(cal.shift(x, -1))
            }
        };
        let f_bound = |x: Date, put: &mut dyn FnMut(Date)| {
            if d > 0 {
                put(cal.shift(x, -(d - 1)))
            } else if mk == MilestoneKind::Start {
                put(cal.first_on_or_after(x.add_days(1)))
            } else {
                put(x)
            }
        };
        let live: Vec<&Edge> = net.succs[i].iter().filter(|e| net.status[e.other] != Status::Complete).collect();
        if live.is_empty() {
            f_bound(cal.last_on_or_before(finish_limit), &mut put);
        }
        for e in live {
            let s = e.other;
            let cs = net.cals[s];
            let (ss, sf) = (net.s_eff(s, ls[s]), net.f_eff(s, ls[s], lf[s]));
            match e.kind {
                LinkType::FS => f_bound(cal.last_on_or_before(cs.shift(ss, -e.lag).add_days(-1)), &mut put),
                LinkType::SS => s_bound(cal.last_on_or_before(cs.shift(ss, -e.lag)), &mut put),
                LinkType::FF => f_bound(cal.last_on_or_before(cs.shift(sf, -e.lag)), &mut put),
                LinkType::SF => s_bound(cal.last_on_or_before(cs.shift(sf, 1 - e.lag)), &mut put),
            }
        }
        if !started {
            match a.constraint {
                Some(Constraint::FinishNoLaterThan(c)) => f_bound(cal.last_on_or_before(c), &mut put),
                Some(Constraint::MustStartOn(c)) => put(cal.first_on_or_after(c)),
                _ => {}
            }
        }
        let late_start = cap.expect("an activity is terminal or has a live successor");
        ls[i] = late_start;
        lf[i] = if d > 0 { cal.shift(late_start, d - 1) } else { late_start };
    }

    // Float.
    let mut rows = Vec::with_capacity(n);
    let mut negative = Vec::new();
    for i in 0..n {
        let a = &p.activities[i];
        let cal = net.cals[i];
        let complete = net.status[i] == Status::Complete;
        let tf = if complete { 0 } else { cal.wd_between(ef[i], lf[i]) };
        let mut ff: Option<i64> = None;
        if !complete {
            let (ps, pf) = (net.s_eff(i, es[i]), net.f_eff(i, es[i], ef[i]));
            for e in &net.succs[i] {
                if net.status[e.other] != Status::NotStarted {
                    continue;
                }
                let need = net.implied(e.other, e.kind, e.lag, ps, pf);
                let slack = net.cals[e.other].wd_between(need, es[e.other]);
                ff = Some(ff.map_or(slack, |f| f.min(slack)));
            }
        }
        let ff = if complete { 0 } else { ff.unwrap_or(tf).min(tf) };
        if tf < 0 {
            negative.push(a.id.clone());
        }
        let duration = match net.status[i] {
            Status::NotStarted => a.duration,
            _ if net.ms[i] => 0,
            _ => cal.wd_between(es[i], ef[i].add_days(1)).max(0) as u32,
        };
        rows.push(Row {
            id: a.id.clone(),
            wbs: a.wbs.clone(),
            name: a.name.clone(),
            status: net.status[i],
            calendar: cal.name.clone(),
            is_milestone: net.ms[i],
            duration,
            remaining_duration: if complete { 0 } else { net.dur[i] as u32 },
            percent_complete: if complete { 100.0 } else { a.progress.percent_complete },
            early_start: es[i],
            early_finish: ef[i],
            late_start: ls[i],
            late_finish: lf[i],
            total_float: tf,
            free_float: ff,
            critical: !complete && tf <= 0,
        });
    }
    if !negative.is_empty() {
        issues.push(issue(
            Severity::Warning,
            "negative_float",
            format!("{} activities have negative total float and miss a constraint or the required finish: {}", negative.len(), negative.join(", ")),
            None,
        ));
    }
    Ok(Schedule { rows, project_start: p.start, project_finish, finish_limit, data_date: p.data_date, issues })
}

/// Kahn's algorithm; ties keep the programme order so results are deterministic.
fn topological_order(n: usize, preds: &[Vec<Edge>]) -> Vec<usize> {
    let mut indeg: Vec<usize> = preds.iter().map(|p| p.len()).collect();
    let mut succ = vec![Vec::new(); n];
    for (b, ps) in preds.iter().enumerate() {
        for e in ps {
            succ[e.other].push(b);
        }
    }
    let mut ready: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while !ready.is_empty() {
        let i = ready.remove(0);
        order.push(i);
        for &s in &succ[i] {
            indeg[s] -= 1;
            if indeg[s] == 0 {
                ready.push(s);
                ready.sort_unstable();
            }
        }
    }
    order
}
