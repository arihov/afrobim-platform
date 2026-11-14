//! Period reporting: the monthly table, S-curve data and drawing, the look-ahead list, the delay
//! list, and the CSV text of each.

use afrobim_cost::Decimal;
use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use afrobim_schedule::{Date, Programme, Schedule, Status};
use serde::{Deserialize, Serialize};

use crate::error::{invalid, Result};
use crate::evm::{EvmEngine, EvmResult, Figure};
use crate::record::ProgressLog;

// ---------------------------------------------------------------- period table

/// One calendar month of the table.
///
/// `pv_cum` is the planned value at the end of the month. EV and AC are cumulative to the end of
/// the month, or to the data date in the month that contains it; months after the data date have
/// no EV or AC (`Undefined`). The `*_period` figures are the change from the previous row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeriodRow {
    /// `YYYY-MM`.
    pub label: String,
    pub from: Date,
    pub to: Date,
    /// The date EV and AC are measured at: the end of the month, or the data date if earlier.
    pub as_of: Date,
    pub pv_period: Decimal,
    pub pv_cum: Decimal,
    pub ev_period: Figure<Decimal>,
    pub ev_cum: Figure<Decimal>,
    pub ac_period: Figure<Decimal>,
    pub ac_cum: Figure<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeriodTable {
    pub data_date: Date,
    pub decimals: u32,
    pub rows: Vec<PeriodRow>,
}

/// One point of the S-curve data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    pub date: Date,
    pub pv: Decimal,
    pub ev: Figure<Decimal>,
    pub ac: Figure<Decimal>,
}

/// Planned, earned and actual cumulative values: a zero point on the day before the first month,
/// each month end, and the data date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SCurveData {
    pub data_date: Date,
    pub decimals: u32,
    pub bac: Decimal,
    pub points: Vec<CurvePoint>,
}

fn month_start(d: Date) -> Date {
    let (y, m, _) = d.ymd();
    Date::from_ymd(y, m, 1).expect("first of a month exists")
}

fn next_month_start(d: Date) -> Date {
    let (y, m, _) = d.ymd();
    if m == 12 {
        Date::from_ymd(y + 1, 1, 1).expect("valid")
    } else {
        Date::from_ymd(y, m + 1, 1).expect("valid")
    }
}

impl<'a> EvmEngine<'a> {
    fn pv_total(&self, date: Date) -> Decimal {
        self.setup.budget.by_activity.keys().map(|a| self.planned_value(a, date)).sum()
    }

    /// First and last month the report spans: from the earliest baseline start of a budgeted
    /// activity to the later of the latest baseline finish and the data date.
    fn span(&self, data_date: Date) -> Result<(Date, Date)> {
        let entries: Vec<_> = self.baseline.entries.iter().filter(|e| self.setup.budget.by_activity.contains_key(&e.id)).collect();
        let Some(first) = entries.iter().map(|e| e.start).min() else {
            return invalid("the budget has no activity: there is nothing to report");
        };
        let last = entries.iter().map(|e| e.finish).max().expect("non-empty").max(data_date);
        Ok((month_start(first), month_start(last)))
    }

    fn ev_ac_at(&self, log: &ProgressLog, date: Date, data_date: Date) -> Result<(Figure<Decimal>, Figure<Decimal>)> {
        if date > data_date {
            let why = "after the data date".to_string();
            return Ok((Figure::Undefined(why.clone()), Figure::Undefined(why)));
        }
        let t = self.totals_at(log, date)?;
        let ac = match t.ac {
            Some(v) => Figure::Value(v),
            None => Figure::InputNeeded("actual cost (AC) not entered".into()),
        };
        Ok((Figure::Value(t.ev), ac))
    }

    /// The table by calendar month, from the first month of the baseline to the later of the last
    /// baseline month and the month of the data date.
    pub fn period_table(&self, log: &ProgressLog, data_date: Date) -> Result<PeriodTable> {
        let (first, last) = self.span(data_date)?;
        let mut rows = Vec::new();
        let mut from = first;
        let (mut prev_pv, mut prev_ev, mut prev_ac) = (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
        while from <= last {
            let to = next_month_start(from).add_days(-1);
            let as_of = to.min(data_date);
            let pv_cum = self.pv_total(to);
            let (ev_cum, ac_cum) = self.ev_ac_at(log, as_of, data_date)?;
            let (ev_cum, ac_cum) = if from > data_date {
                let why = "after the data date".to_string();
                (Figure::Undefined(why.clone()), Figure::Undefined(why))
            } else {
                (ev_cum, ac_cum)
            };
            let diff = |now: &Figure<Decimal>, prev: Decimal| match now {
                Figure::Value(v) => Figure::Value(*v - prev),
                Figure::InputNeeded(w) => Figure::InputNeeded(w.clone()),
                Figure::Undefined(w) => Figure::Undefined(w.clone()),
            };
            let row = PeriodRow {
                label: format!("{:04}-{:02}", from.ymd().0, from.ymd().1),
                from,
                to,
                as_of,
                pv_period: pv_cum - prev_pv,
                pv_cum,
                ev_period: diff(&ev_cum, prev_ev),
                ac_period: diff(&ac_cum, prev_ac),
                ev_cum: ev_cum.clone(),
                ac_cum: ac_cum.clone(),
            };
            prev_pv = pv_cum;
            if let Figure::Value(v) = ev_cum {
                prev_ev = v;
            }
            if let Figure::Value(v) = ac_cum {
                prev_ac = v;
            }
            rows.push(row);
            from = next_month_start(from);
        }
        Ok(PeriodTable { data_date, decimals: self.setup.rounding.decimals, rows })
    }

    /// S-curve data: planned against earned against actual, cumulative.
    pub fn s_curve(&self, log: &ProgressLog, data_date: Date) -> Result<SCurveData> {
        let (first, last) = self.span(data_date)?;
        let mut dates = vec![first.add_days(-1)];
        let mut m = first;
        while m <= last {
            dates.push(next_month_start(m).add_days(-1));
            m = next_month_start(m);
        }
        if !dates.contains(&data_date) && data_date > dates[0] {
            dates.push(data_date);
        }
        dates.sort();
        dates.dedup();
        let mut points = Vec::new();
        for d in dates {
            let (ev, ac) = self.ev_ac_at(log, d, data_date)?;
            points.push(CurvePoint { date: d, pv: self.pv_total(d), ev, ac });
        }
        Ok(SCurveData { data_date, decimals: self.setup.rounding.decimals, bac: self.setup.budget.bac(), points })
    }

    /// The whole report at a date: figures, table and curve.
    pub fn report(&self, log: &ProgressLog, data_date: Date) -> Result<(EvmResult, PeriodTable, SCurveData)> {
        Ok((self.evm(log, data_date)?, self.period_table(log, data_date)?, self.s_curve(log, data_date)?))
    }
}

// ---------------------------------------------------------------- look-ahead and delay

/// What an activity does in the look-ahead window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    Starts,
    Finishes,
    StartsAndFinishes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LookAheadRow {
    pub id: String,
    pub wbs: String,
    pub name: String,
    pub status: Status,
    pub event: Event,
    /// Forecast dates from the schedule.
    pub start: Date,
    pub finish: Date,
    pub total_float: i64,
    pub critical: bool,
}

/// Activities due to start or finish in the `days` calendar days after `from`: dates `from + 1`
/// to `from + days` inclusive (the data date is the end of its day, so `from` itself is past).
/// Taken from a schedule computed after the progress was applied. An activity is listed as
/// starting when it is not started and its early start is in the window, and as finishing when it
/// is not complete and its early finish is in the window. Sorted by the first date in the window.
pub fn look_ahead(schedule: &Schedule, from: Date, days: u32) -> Vec<LookAheadRow> {
    let to = from.add_days(days as i64);
    let inside = |d: Date| d > from && d <= to;
    let mut rows: Vec<LookAheadRow> = schedule
        .rows
        .iter()
        .filter_map(|r| {
            let starts = r.status == Status::NotStarted && inside(r.early_start);
            let finishes = r.status != Status::Complete && inside(r.early_finish);
            let event = match (starts, finishes) {
                (true, true) => Event::StartsAndFinishes,
                (true, false) => Event::Starts,
                (false, true) => Event::Finishes,
                (false, false) => return None,
            };
            Some(LookAheadRow {
                id: r.id.clone(),
                wbs: r.wbs.clone(),
                name: r.name.clone(),
                status: r.status,
                event,
                start: r.early_start,
                finish: r.early_finish,
                total_float: r.total_float,
                critical: r.critical,
            })
        })
        .collect();
    rows.sort_by_key(|r| if matches!(r.event, Event::Finishes) { r.finish } else { r.start });
    rows
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DelayRow {
    pub id: String,
    pub name: String,
    pub status: Status,
    pub baseline_start: Date,
    pub baseline_finish: Date,
    /// Forecast (or actual, when complete) dates from the schedule.
    pub start: Date,
    pub finish: Date,
    /// Working days of the activity's calendar from the baseline finish to the forecast finish.
    pub finish_variance: i64,
    pub start_variance: i64,
    pub critical: bool,
}

/// Activities whose forecast finish is later than the baseline finish, worst first. The variance
/// is working days of the activity's own calendar, as in the schedule crate's variance report.
/// Activities without a baseline entry are not listed.
pub fn delay_list(programme: &Programme, schedule: &Schedule) -> Result<Vec<DelayRow>> {
    let Some(b) = programme.baseline.as_ref() else {
        return invalid("the programme has no baseline: a delay is measured against it");
    };
    let pcal = programme.calendar_named(&programme.calendar).ok_or_else(|| crate::error::ProgressError::Schedule("the programme calendar is not defined".into()))?;
    let mut out = Vec::new();
    for r in &schedule.rows {
        let Some(e) = b.entries.iter().find(|e| e.id == r.id) else { continue };
        let cal = programme.calendar_named(&r.calendar).unwrap_or(pcal);
        let fv = cal.wd_between(e.finish, r.early_finish);
        if fv > 0 {
            out.push(DelayRow {
                id: r.id.clone(),
                name: r.name.clone(),
                status: r.status,
                baseline_start: e.start,
                baseline_finish: e.finish,
                start: r.early_start,
                finish: r.early_finish,
                finish_variance: fv,
                start_variance: cal.wd_between(e.start, r.early_start),
                critical: r.critical,
            });
        }
    }
    out.sort_by(|a, b| b.finish_variance.cmp(&a.finish_variance).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

// ---------------------------------------------------------------- CSV

fn field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn money(f: &Figure<Decimal>, decimals: u32) -> String {
    field(&f.text(|v| v.to_fixed(decimals)))
}

fn index(f: &Figure<Decimal>) -> String {
    field(&f.text(|v| v.to_string()))
}

pub const PERIOD_CSV_HEADER: &str = "period,from,to,as_of,pv_period,pv_cumulative,ev_period,ev_cumulative,ac_period,ac_cumulative";
pub const S_CURVE_CSV_HEADER: &str = "date,pv,ev,ac";
pub const EVM_CSV_HEADER: &str = "measure,value,definition";
pub const EVM_ACTIVITY_CSV_HEADER: &str = "activity,name,bac,pv,percent_complete,ev,ac,has_progress_source";
pub const LOOK_AHEAD_CSV_HEADER: &str = "activity,wbs,name,status,event,start,finish,total_float_wd,critical";
pub const DELAY_CSV_HEADER: &str = "activity,name,status,baseline_start,baseline_finish,forecast_start,forecast_finish,finish_variance_wd,start_variance_wd,critical";

fn status_text(s: Status) -> &'static str {
    match s {
        Status::NotStarted => "not started",
        Status::InProgress => "in progress",
        Status::Complete => "complete",
    }
}

pub fn period_csv(t: &PeriodTable) -> String {
    let d = t.decimals;
    let mut s = format!("{PERIOD_CSV_HEADER}\n");
    for r in &t.rows {
        s.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            r.label,
            r.from,
            r.to,
            r.as_of,
            r.pv_period.to_fixed(d),
            r.pv_cum.to_fixed(d),
            money(&r.ev_period, d),
            money(&r.ev_cum, d),
            money(&r.ac_period, d),
            money(&r.ac_cum, d)
        ));
    }
    s
}

pub fn s_curve_csv(c: &SCurveData) -> String {
    let d = c.decimals;
    let mut s = format!("{S_CURVE_CSV_HEADER}\n");
    for p in &c.points {
        s.push_str(&format!("{},{},{},{}\n", p.date, p.pv.to_fixed(d), money(&p.ev, d), money(&p.ac, d)));
    }
    s
}

/// The earned value figures, one per row, each with its definition and, for EAC, the formula name.
pub fn evm_csv(r: &EvmResult) -> String {
    let d = r.decimals;
    let mut s = format!("{EVM_CSV_HEADER}\n");
    let mut row = |name: &str, value: String, def: &str| s.push_str(&format!("{},{},{}\n", field(name), value, field(def)));
    row("data date", r.data_date.to_string(), "");
    row("BAC", r.bac.to_fixed(d), "sum of the activity budgets");
    row("PV", r.pv.to_fixed(d), "sum over activities of BAC * working days elapsed / working days of the baseline span");
    row("EV", r.ev.to_fixed(d), "sum over activities of BAC * percent complete / 100");
    row("AC", money(&r.ac, d), "entered actual cost to the data date (input)");
    row("SV", r.sv.to_fixed(d), "EV - PV");
    row("CV", money(&r.cv, d), "EV - AC");
    row("SPI", index(&r.spi), "EV / PV");
    row("CPI", index(&r.cpi), "EV / AC");
    row(&format!("EAC ({})", r.eac.formula), money(&r.eac.value, d), &r.eac.definition);
    row("ETC", money(&r.etc, d), "EAC - AC");
    row("VAC", money(&r.vac, d), "BAC - EAC");
    row("TCPI to BAC", index(&r.tcpi_bac), "(BAC - EV) / (BAC - AC)");
    row("TCPI to EAC", index(&r.tcpi_eac), "(BAC - EV) / (EAC - AC)");
    s
}

pub fn evm_activity_csv(r: &EvmResult) -> String {
    let d = r.decimals;
    let mut s = format!("{EVM_ACTIVITY_CSV_HEADER}\n");
    for a in &r.activities {
        s.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            field(&a.id),
            field(&a.name),
            a.bac.to_fixed(d),
            a.pv.to_fixed(d),
            a.percent_complete.to_fixed(2),
            a.ev.to_fixed(d),
            money(&a.ac, d),
            a.measured
        ));
    }
    s
}

pub fn look_ahead_csv(rows: &[LookAheadRow]) -> String {
    let mut s = format!("{LOOK_AHEAD_CSV_HEADER}\n");
    for r in rows {
        let ev = match r.event {
            Event::Starts => "starts",
            Event::Finishes => "finishes",
            Event::StartsAndFinishes => "starts and finishes",
        };
        s.push_str(&format!("{},{},{},{},{},{},{},{},{}\n", field(&r.id), field(&r.wbs), field(&r.name), status_text(r.status), ev, r.start, r.finish, r.total_float, r.critical));
    }
    s
}

pub fn delay_csv(rows: &[DelayRow]) -> String {
    let mut s = format!("{DELAY_CSV_HEADER}\n");
    for r in rows {
        s.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            field(&r.id),
            field(&r.name),
            status_text(r.status),
            r.baseline_start,
            r.baseline_finish,
            r.start,
            r.finish,
            r.finish_variance,
            r.start_variance,
            r.critical
        ));
    }
    s
}

// ---------------------------------------------------------------- drawing

/// A text label of the S-curve drawing, as the schedule crate does for its charts.
#[derive(Debug, Clone, PartialEq)]
pub struct CurveLabel {
    pub at: Pt2,
    pub text: String,
}

fn line(a: Pt2, b: Pt2, w: f64, c: Colour) -> Primitive {
    Primitive::Line { from: a, to: b, width: w, colour: Some(c) }
}

/// The S-curve in a box `width` by `height`, lower-left on the origin: planned (blue), earned
/// (green) and actual (red) over a grey grid. X is calendar days over the whole range, Y runs from
/// zero to the largest value drawn (or BAC). Only points with a value are drawn, so an AC that is
/// not entered leaves out the red line, and EV stops at the data date.
pub fn s_curve_drawing(c: &SCurveData, width: f64, height: f64) -> (Drawing, Vec<CurveLabel>) {
    let mut prims = Vec::new();
    let mut labels = Vec::new();
    let max = c
        .points
        .iter()
        .flat_map(|p| [Some(p.pv), p.ev.value().copied(), p.ac.value().copied()])
        .flatten()
        .chain(std::iter::once(c.bac))
        .max()
        .map(|m| m.to_f64())
        .filter(|m| *m > 0.0)
        .unwrap_or(1.0);
    for q in 0..=4 {
        let y = height * q as f64 / 4.0;
        prims.push(line(Pt2::new(0.0, y), Pt2::new(width, y), 0.0, Colour::Grey));
        labels.push(CurveLabel { at: Pt2::new(0.0, y), text: format!("{:.0}", max * q as f64 / 4.0) });
    }
    prims.push(line(Pt2::new(0.0, 0.0), Pt2::new(0.0, height), 0.2, Colour::Black));
    prims.push(line(Pt2::new(0.0, 0.0), Pt2::new(width, 0.0), 0.2, Colour::Black));
    let (d0, d1) = match (c.points.first(), c.points.last()) {
        (Some(a), Some(b)) => (a.date, b.date),
        _ => return (Drawing { primitives: prims, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(width, height) } }, labels),
    };
    let span = d1.days_since(d0).max(1) as f64;
    let at = |d: Date, v: f64| Pt2::new(width * d.days_since(d0) as f64 / span, height * v / max);
    let mut draw = |colour: Colour, name: &str, get: &dyn Fn(&CurvePoint) -> Option<f64>| {
        let pts: Vec<(Date, f64)> = c.points.iter().filter_map(|p| get(p).map(|v| (p.date, v))).collect();
        for w in pts.windows(2) {
            prims.push(line(at(w[0].0, w[0].1), at(w[1].0, w[1].1), 0.4, colour));
        }
        if let Some((d, v)) = pts.last() {
            labels.push(CurveLabel { at: at(*d, *v), text: name.to_string() });
        }
    };
    draw(Colour::Blue, "PV", &|p| Some(p.pv.to_f64()));
    draw(Colour::Green, "EV", &|p| p.ev.value().map(|v| v.to_f64()));
    draw(Colour::Red, "AC", &|p| p.ac.value().map(|v| v.to_f64()));
    labels.push(CurveLabel { at: Pt2::new(0.0, 0.0), text: d0.to_string() });
    labels.push(CurveLabel { at: Pt2::new(width, 0.0), text: d1.to_string() });
    (Drawing { primitives: prims, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(width, height) } }, labels)
}
