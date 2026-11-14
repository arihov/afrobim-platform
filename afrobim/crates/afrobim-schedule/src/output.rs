//! Outputs: Gantt chart, activity table, S-curve.

use std::collections::HashMap;

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use serde::{Deserialize, Serialize};

use crate::cpm::{Row, Schedule, ScheduleError};
use crate::date::{Date, Weekday};
use crate::model::{Constraint, LinkType, Programme, Status};

// ---------------------------------------------------------------- Gantt

/// Layout of the Gantt chart, in drawing units (the module draws them as millimetres).
#[derive(Debug, Clone, PartialEq)]
pub struct GanttOptions {
    /// Width of one calendar day.
    pub day_width: f64,
    pub row_height: f64,
    pub bar_height: f64,
    /// Width of the left column that holds the WBS code and name labels.
    pub label_width: f64,
    pub header_height: f64,
    pub dependencies: bool,
    /// Draw the baseline as a thin grey bar under each bar, when the programme has one.
    pub baseline: bool,
}

impl Default for GanttOptions {
    fn default() -> Self {
        GanttOptions { day_width: 2.0, row_height: 8.0, bar_height: 4.0, label_width: 120.0, header_height: 14.0, dependencies: true, baseline: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GanttLabelKind {
    Title,
    /// First of a month, in the upper band of the time scale.
    Month,
    /// Monday, in the lower band.
    Week,
    Wbs,
    Name,
    DataDate,
    RequiredFinish,
}

/// Text for the front end to set: the draw crate has no text primitive.
#[derive(Debug, Clone, PartialEq)]
pub struct GanttLabel {
    /// Left end of the text, vertically centred on this point, in drawing units.
    pub at: Pt2,
    pub text: String,
    pub kind: GanttLabelKind,
}

/// The Gantt chart as primitives with its labels and the date scale.
///
/// A date's x position is `label_width + (date - origin_date) * day_width`, the start of that
/// day; a bar runs from the start of its early start to the end of its early finish. Row `k` of
/// `rows` (programme order) is centred at `y = height - header_height - (k + 0.5) * row_height`.
/// Bars are blue, critical bars red, completed bars grey; progress is a black strip; milestones
/// are diamonds; dependency lines are grey, red between two critical activities; the data date is
/// a green line and the required finish a brown one.
#[derive(Debug, Clone, PartialEq)]
pub struct GanttDrawing {
    pub drawing: Drawing,
    pub labels: Vec<GanttLabel>,
    pub origin_date: Date,
    pub day_width: f64,
    pub label_width: f64,
    pub row_height: f64,
    pub header_height: f64,
    /// Activity ids, top row first.
    pub rows: Vec<String>,
}

impl GanttDrawing {
    pub fn x_of(&self, d: Date) -> f64 {
        self.label_width + d.days_since(self.origin_date) as f64 * self.day_width
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64, colour: Colour) -> Primitive {
    Primitive::Fill { outline: vec![Pt2::new(x0, y0), Pt2::new(x1, y0), Pt2::new(x1, y1), Pt2::new(x0, y1)], colour }
}

fn line(a: Pt2, b: Pt2, w: f64, c: Colour) -> Primitive {
    Primitive::Line { from: a, to: b, width: w, colour: Some(c) }
}

pub fn gantt_drawing(p: &Programme, s: &Schedule, o: &GanttOptions) -> GanttDrawing {
    let baseline = if o.baseline { p.baseline.as_ref() } else { None };
    let mut first = s.rows.iter().map(|r| r.early_start).min().unwrap_or(s.project_start).min(s.project_start);
    let mut last = s.project_finish;
    if let Some(b) = baseline {
        for e in &b.entries {
            first = first.min(e.start);
            last = last.max(e.finish);
        }
    }
    for d in [s.data_date, p.required_finish].into_iter().flatten() {
        first = first.min(d);
        last = last.max(d);
    }
    // Start the scale on a Monday so week ticks are whole.
    while first.weekday() != Weekday::Mon {
        first = first.add_days(-1);
    }
    let days = last.days_since(first) + 2;
    let n = s.rows.len();
    let width = o.label_width + days as f64 * o.day_width;
    let height = o.header_height + n as f64 * o.row_height;
    let x = |d: Date| o.label_width + d.days_since(first) as f64 * o.day_width;
    let row_y = |k: usize| height - o.header_height - (k as f64 + 0.5) * o.row_height;
    let mut prims = Vec::new();
    let mut labels = Vec::new();

    // Time scale and grid.
    labels.push(GanttLabel { at: Pt2::new(2.0, height - o.header_height * 0.5), text: p.name.clone(), kind: GanttLabelKind::Title });
    for k in 0..days {
        let d = first.add_days(k);
        let (_, _, day) = d.ymd();
        let monday = d.weekday() == Weekday::Mon;
        if monday {
            prims.push(line(Pt2::new(x(d), 0.0), Pt2::new(x(d), height - o.header_height * 0.5), 0.0, Colour::Grey));
            labels.push(GanttLabel { at: Pt2::new(x(d) + 0.5, height - o.header_height * 0.75), text: format!("{day:02}"), kind: GanttLabelKind::Week });
        }
        if day == 1 || k == 0 {
            prims.push(line(Pt2::new(x(d), 0.0), Pt2::new(x(d), height), 0.1, Colour::Black));
            labels.push(GanttLabel { at: Pt2::new(x(d) + 0.5, height - o.header_height * 0.25), text: format!("{} {}", d.month_name(), d.ymd().0), kind: GanttLabelKind::Month });
        }
    }
    prims.push(line(Pt2::new(0.0, height - o.header_height), Pt2::new(width, height - o.header_height), 0.1, Colour::Black));

    // Bars and milestones.
    let mut anchors: HashMap<&str, (f64, f64, f64, bool)> = HashMap::new(); // start x, finish x, y, critical
    let half = o.bar_height / 2.0;
    for (k, r) in s.rows.iter().enumerate() {
        let y = row_y(k);
        let colour = if r.critical {
            Colour::Red
        } else if r.status == Status::Complete {
            Colour::Grey
        } else {
            Colour::Blue
        };
        let (xs, xf);
        if r.is_milestone {
            let a = p.activity(&r.id).expect("row from programme");
            let cx = if a.milestone_kind == crate::MilestoneKind::Start { x(r.early_start) } else { x(r.early_start.add_days(1)) };
            prims.push(Primitive::Fill {
                outline: vec![Pt2::new(cx - half, y), Pt2::new(cx, y + half), Pt2::new(cx + half, y), Pt2::new(cx, y - half)],
                colour: if r.critical { Colour::Red } else { Colour::Black },
            });
            xs = cx - half;
            xf = cx + half;
        } else {
            xs = x(r.early_start);
            xf = x(r.early_finish.add_days(1));
            prims.push(rect(xs, y - half, xf, y + half, colour));
            if r.percent_complete > 0.0 {
                let w = (xf - xs) * (r.percent_complete / 100.0).min(1.0);
                prims.push(rect(xs, y - half, xs + w, y - half + o.bar_height / 3.0, Colour::Black));
            }
        }
        if let Some(e) = baseline.and_then(|b| b.entries.iter().find(|e| e.id == r.id)) {
            let (bx0, bx1) = (x(e.start), x(e.finish.add_days(1)));
            prims.push(rect(bx0.min(bx1 - 0.2), y - half - o.row_height * 0.18, bx1.max(bx0 + 0.2), y - half - o.row_height * 0.06, Colour::Grey));
        }
        anchors.insert(r.id.as_str(), (xs, xf, y, r.critical));
        let wbs_w = o.label_width * 0.25;
        labels.push(GanttLabel { at: Pt2::new(2.0, y), text: r.wbs.clone(), kind: GanttLabelKind::Wbs });
        labels.push(GanttLabel { at: Pt2::new(wbs_w, y), text: r.name.clone(), kind: GanttLabelKind::Name });
    }

    // Dependency lines: out of the predecessor's start or finish, into the successor's start or
    // finish, routed through the gap between rows.
    if o.dependencies {
        let gap = (o.day_width * 0.5).max(1.0);
        let arrow = (o.row_height * 0.15).min(1.2);
        for d in &p.dependencies {
            let (Some(&(ps, pf, py, pc)), Some(&(ss, sf, sy, sc))) = (anchors.get(d.predecessor.as_str()), anchors.get(d.successor.as_str())) else {
                continue;
            };
            let from_finish = matches!(d.kind, LinkType::FS | LinkType::FF);
            let into_start = matches!(d.kind, LinkType::FS | LinkType::SS);
            let (px, tx) = (if from_finish { pf } else { ps }, if into_start { ss } else { sf });
            let ox = if from_finish { px + gap } else { px - gap };
            let ax = if into_start { tx - gap } else { tx + gap };
            let my = if sy < py { py - o.row_height / 2.0 } else { py + o.row_height / 2.0 };
            let colour = if pc && sc { Colour::Red } else { Colour::Grey };
            let pts = [Pt2::new(px, py), Pt2::new(ox, py), Pt2::new(ox, my), Pt2::new(ax, my), Pt2::new(ax, sy), Pt2::new(tx, sy)];
            for w in pts.windows(2) {
                prims.push(line(w[0], w[1], 0.0, colour));
            }
            let dir = if into_start { 1.0 } else { -1.0 };
            prims.push(Primitive::Fill { outline: vec![Pt2::new(tx, sy), Pt2::new(tx - dir * arrow, sy + arrow / 2.0), Pt2::new(tx - dir * arrow, sy - arrow / 2.0)], colour });
        }
    }

    if let Some(dd) = s.data_date {
        let xd = x(dd.add_days(1));
        prims.push(line(Pt2::new(xd, 0.0), Pt2::new(xd, height - o.header_height), 0.3, Colour::Green));
        labels.push(GanttLabel { at: Pt2::new(xd + 0.5, 1.5), text: format!("Data date {dd}"), kind: GanttLabelKind::DataDate });
    }
    if let Some(rf) = p.required_finish {
        let xr = x(rf.add_days(1));
        prims.push(line(Pt2::new(xr, 0.0), Pt2::new(xr, height - o.header_height), 0.3, Colour::Brown));
        labels.push(GanttLabel { at: Pt2::new(xr + 0.5, 4.5), text: format!("Required finish {rf}"), kind: GanttLabelKind::RequiredFinish });
    }

    GanttDrawing {
        drawing: Drawing { primitives: prims, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(width, height) } },
        labels,
        origin_date: first,
        day_width: o.day_width,
        label_width: o.label_width,
        row_height: o.row_height,
        header_height: o.header_height,
        rows: s.rows.iter().map(|r| r.id.clone()).collect(),
    }
}

// ---------------------------------------------------------------- CSV

fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub const ACTIVITY_CSV_HEADER: &str = "id,wbs,name,calendar,duration_wd,remaining_wd,percent_complete,constraint,early_start,early_finish,late_start,late_finish,total_float_wd,free_float_wd,critical,status";

fn constraint_text(c: Option<Constraint>) -> String {
    match c {
        None => String::new(),
        Some(Constraint::StartNoEarlierThan(d)) => format!("SNET {d}"),
        Some(Constraint::FinishNoLaterThan(d)) => format!("FNLT {d}"),
        Some(Constraint::MustStartOn(d)) => format!("MSO {d}"),
    }
}

/// The activity table, one row per activity in programme order, dates as `YYYY-MM-DD`, float in
/// working days. Lines end with `\n`.
pub fn activity_table_csv(p: &Programme, s: &Schedule) -> String {
    let mut out = String::from(ACTIVITY_CSV_HEADER);
    out.push('\n');
    for r in &s.rows {
        let a = p.activity(&r.id);
        let fields = [
            csv(&r.id),
            csv(&r.wbs),
            csv(&r.name),
            csv(&r.calendar),
            r.duration.to_string(),
            r.remaining_duration.to_string(),
            format!("{}", r.percent_complete),
            constraint_text(a.and_then(|a| a.constraint)),
            r.early_start.to_string(),
            r.early_finish.to_string(),
            r.late_start.to_string(),
            r.late_finish.to_string(),
            r.total_float.to_string(),
            r.free_float.to_string(),
            if r.critical { "yes" } else { "no" }.to_string(),
            format!("{:?}", r.status),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out
}

pub const VARIANCE_CSV_HEADER: &str = "id,name,status,start_variance_wd,finish_variance_wd,duration_variance_wd,float_variance_wd";

pub fn variance_csv(report: &crate::VarianceReport) -> String {
    let mut out = String::from(VARIANCE_CSV_HEADER);
    out.push('\n');
    let f = |v: Option<i64>| v.map(|x| x.to_string()).unwrap_or_default();
    for v in &report.rows {
        out.push_str(&format!(
            "{},{},{:?},{},{},{},{}\n",
            csv(&v.id),
            csv(&v.name),
            v.status,
            f(v.start_variance),
            f(v.finish_variance),
            f(v.duration_variance),
            f(v.float_variance)
        ));
    }
    out
}

// ---------------------------------------------------------------- S-curve

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SCurveBasis {
    /// Each activity weighs its duration in working days; milestones weigh nothing.
    Duration,
    /// Each activity weighs its entered weight. Every activity must have one.
    Weight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SPoint {
    pub date: Date,
    /// Cumulative planned weight at the end of this day.
    pub cumulative: f64,
    /// `cumulative / total`, 0 to 1.
    pub fraction: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SCurve {
    pub basis: SCurveBasis,
    pub total: f64,
    /// One point per calendar day from the first early start to the project finish.
    pub points: Vec<SPoint>,
}

/// Planned progress by early dates. An activity earns its weight evenly over its working days (in
/// its own calendar); a milestone with a weight earns it on its date.
pub fn s_curve(p: &Programme, s: &Schedule, basis: SCurveBasis) -> Result<SCurve, ScheduleError> {
    let weight_of = |r: &Row| -> Option<f64> {
        let a = p.activity(&r.id)?;
        match basis {
            SCurveBasis::Duration => Some(r.duration as f64),
            SCurveBasis::Weight => a.weight,
        }
    };
    if basis == SCurveBasis::Weight {
        let missing: Vec<&str> = s.rows.iter().filter(|r| weight_of(r).is_none()).map(|r| r.id.as_str()).collect();
        if !missing.is_empty() {
            return Err(ScheduleError::Other(format!("the S-curve by weight needs a weight on every activity; missing on: {}", missing.join(", "))));
        }
    }
    let first = s.rows.iter().map(|r| r.early_start).min().unwrap_or(s.project_start);
    let last = s.project_finish;
    let days = (last.days_since(first) + 1).max(1) as usize;
    let mut earned = vec![0.0; days];
    let mut total = 0.0;
    for r in &s.rows {
        let w = weight_of(r).unwrap_or(0.0);
        total += w;
        let cal = p.calendar_named(&r.calendar).ok_or_else(|| ScheduleError::Other(format!("calendar '{}' is not defined", r.calendar)))?;
        let days_of = cal.working_days_inclusive(r.early_start, r.early_finish);
        if r.is_milestone || days_of.is_empty() {
            earned[(r.early_finish.days_since(first)).clamp(0, days as i64 - 1) as usize] += w;
        } else {
            let each = w / days_of.len() as f64;
            for d in days_of {
                earned[d.days_since(first) as usize] += each;
            }
        }
    }
    let mut cum = 0.0;
    let points = (0..days)
        .map(|k| {
            cum += earned[k];
            SPoint { date: first.add_days(k as i64), cumulative: cum, fraction: if total > 0.0 { (cum / total).min(1.0) } else { 0.0 } }
        })
        .collect();
    Ok(SCurve { basis, total, points })
}

pub fn s_curve_csv(c: &SCurve) -> String {
    let mut out = String::from("date,cumulative,fraction\n");
    for p in &c.points {
        out.push_str(&format!("{},{:.4},{:.5}\n", p.date, p.cumulative, p.fraction));
    }
    out
}

/// Text of an S-curve drawing for the front end to set.
#[derive(Debug, Clone, PartialEq)]
pub struct SCurveLabel {
    pub at: Pt2,
    pub text: String,
}

/// The S-curve as a polyline (blue) on a grid of 0, 25, 50, 75 and 100 percent (grey) in a box
/// `width` by `height`, lower-left on the origin. X is calendar days over the whole range.
pub fn s_curve_drawing(c: &SCurve, width: f64, height: f64) -> (Drawing, Vec<SCurveLabel>) {
    let mut prims = Vec::new();
    let mut labels = Vec::new();
    for q in 0..=4 {
        let y = height * q as f64 / 4.0;
        prims.push(line(Pt2::new(0.0, y), Pt2::new(width, y), 0.0, Colour::Grey));
        labels.push(SCurveLabel { at: Pt2::new(0.0, y), text: format!("{}%", q * 25) });
    }
    prims.push(line(Pt2::new(0.0, 0.0), Pt2::new(0.0, height), 0.2, Colour::Black));
    prims.push(line(Pt2::new(0.0, 0.0), Pt2::new(width, 0.0), 0.2, Colour::Black));
    let n = c.points.len().max(2) as f64 - 1.0;
    let pt = |k: usize, f: f64| Pt2::new(width * k as f64 / n, height * f);
    for k in 1..c.points.len() {
        prims.push(line(pt(k - 1, c.points[k - 1].fraction), pt(k, c.points[k].fraction), 0.4, Colour::Blue));
    }
    if let (Some(a), Some(b)) = (c.points.first(), c.points.last()) {
        labels.push(SCurveLabel { at: Pt2::new(0.0, 0.0), text: a.date.to_string() });
        labels.push(SCurveLabel { at: Pt2::new(width, 0.0), text: b.date.to_string() });
    }
    (Drawing { primitives: prims, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(width, height) } }, labels)
}
