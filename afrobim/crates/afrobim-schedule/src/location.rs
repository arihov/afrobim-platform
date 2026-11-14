//! Linear works: where along an alignment each activity is, and the time-location (time-chainage)
//! chart that plots station against date.
//!
//! An activity carries an optional [`LocationRange`]. Where it has none, [`resolve_locations`] can
//! derive one from the site placements of the model elements the activity is linked to
//! (`Element::site`, with its alignment name and station). The chart and its CSV then plot every
//! activity that has a range on one alignment.
//!
//! Plot rules (all dates are whole calendar days, as in the Gantt chart): an activity starts at the
//! **start** of its early start day and finishes at the **end** of its early finish day.
//!
//! | Direction | Drawn as |
//! |-----------|----------|
//! | `Increasing` | a line from (low station, start) to (high station, finish); a parallelogram when `dwell_days` is given |
//! | `Decreasing` | a line from (high station, start) to (low station, finish); likewise |
//! | `Fixed`, or a range of one station | a bar from the low to the high station, from start to finish |
//!
//! A parallelogram of dwell `w` days has the corners (a, start), (b, finish - w), (b, finish) and
//! (a, start + w), where a and b are the stations the work front starts and ends at; `w` is cut
//! to the activity's own length.

use std::collections::BTreeMap;

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use afrobim_kernel::Document;
use serde::{Deserialize, Serialize};

use crate::cpm::Schedule;
use crate::date::{Date, Weekday};
use crate::links::selects_element;
use crate::model::{LocationRange, MilestoneKind, Programme, Status, WorkDirection};

/// A location range of an activity, entered or derived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedLocation {
    pub activity: String,
    pub range: LocationRange,
    /// True when the range came from the linked elements' site placements, not from the activity.
    pub derived: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ResolvedLocations {
    /// In programme order.
    pub items: Vec<ResolvedLocation>,
    /// What could not be derived and why.
    pub notes: Vec<String>,
}

impl ResolvedLocations {
    pub fn get(&self, activity: &str) -> Option<&ResolvedLocation> {
        self.items.iter().find(|i| i.activity == activity)
    }

    /// The alignment names present, in order of first appearance.
    pub fn alignments(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for i in &self.items {
            if !out.contains(&i.range.alignment) {
                out.push(i.range.alignment.clone());
            }
        }
        out
    }
}

/// The location of every activity that has one: the range entered on the activity, else, when a
/// document is given, the range of the linked elements' stations.
///
/// Derived range: the linked elements that have a site placement with an alignment name and a
/// station; the range runs from the lowest to the highest of their stations, all on one alignment.
/// Its direction is `Fixed`, because the placements say where the elements are, not in which order
/// they are built. Activities whose elements sit on more than one alignment, or on none, get no
/// derived range and a note.
pub fn resolve_locations(p: &Programme, doc: Option<&Document>) -> ResolvedLocations {
    let mut out = ResolvedLocations::default();
    for a in &p.activities {
        if let Some(r) = &a.location {
            out.items.push(ResolvedLocation { activity: a.id.clone(), range: r.clone(), derived: false });
            continue;
        }
        let Some(doc) = doc else { continue };
        if !p.links.iter().any(|l| l.activity == a.id) {
            continue;
        }
        // alignment -> (lowest station, highest station)
        let mut by_alignment: BTreeMap<String, (f64, f64)> = BTreeMap::new();
        let mut unplaced = 0;
        for e in doc.elements() {
            if e.category == crate::SCHEDULE_CATEGORY || !selects_element(p, &a.id, e) {
                continue;
            }
            match e.site.as_ref().and_then(|s| Some((s.alignment.clone()?, s.station_m?))) {
                Some((al, st)) if st.is_finite() => {
                    let r = by_alignment.entry(al).or_insert((st, st));
                    r.0 = r.0.min(st);
                    r.1 = r.1.max(st);
                }
                _ => unplaced += 1,
            }
        }
        match by_alignment.len() {
            0 => {
                if unplaced > 0 {
                    out.notes.push(format!("activity '{}': none of its {unplaced} linked element(s) has a station on an alignment", a.id));
                }
            }
            1 => {
                let (al, (lo, hi)) = by_alignment.into_iter().next().expect("one entry");
                if unplaced > 0 {
                    out.notes.push(format!("activity '{}': {unplaced} linked element(s) without a station were left out of the derived range", a.id));
                }
                out.items.push(ResolvedLocation { activity: a.id.clone(), range: LocationRange::new(&al, lo, hi, WorkDirection::Fixed), derived: true });
            }
            _ => out.notes.push(format!(
                "activity '{}': its linked elements are on {} alignments ({}); no range was derived, enter one",
                a.id,
                by_alignment.len(),
                by_alignment.keys().cloned().collect::<Vec<_>>().join(", ")
            )),
        }
    }
    out
}

/// Layout of the time-location chart, in drawing units.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeLocationOptions {
    /// The alignment to plot. `None` plots the first one that appears.
    pub alignment: Option<String>,
    /// Drawing units per metre of station.
    pub metre_scale: f64,
    /// Drawing units per calendar day.
    pub day_scale: f64,
    /// Time runs up the page (station along the bottom) when true, along the page (station up the
    /// side) when false.
    pub time_vertical: bool,
    /// Space left of and below the plot for the scale labels.
    pub margin: f64,
    /// Station grid spacing in metres. `None` picks 1, 2 or 5 times a power of ten giving about
    /// eight divisions.
    pub station_tick_m: Option<f64>,
    /// Narrowest bar, so a culvert at a single station still shows.
    pub min_bar_width: f64,
    /// Width of the diagonal lines.
    pub line_width: f64,
    /// Draw the baseline in a lighter style, when the programme has one.
    pub baseline: bool,
}

impl Default for TimeLocationOptions {
    fn default() -> Self {
        TimeLocationOptions {
            alignment: None,
            metre_scale: 0.05,
            day_scale: 1.0,
            time_vertical: true,
            margin: 16.0,
            station_tick_m: None,
            min_bar_width: 2.0,
            line_width: 0.6,
            baseline: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotKind {
    Line,
    Parallelogram,
    Bar,
    Milestone,
}

impl PlotKind {
    fn text(self) -> &'static str {
        match self {
            PlotKind::Line => "line",
            PlotKind::Parallelogram => "parallelogram",
            PlotKind::Bar => "bar",
            PlotKind::Milestone => "milestone",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlLabelKind {
    Title,
    /// A station value on the station scale.
    Station,
    Month,
    Week,
    /// The activity id, at the middle of its line or bar.
    Activity,
    DataDate,
    RequiredFinish,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TlLabel {
    pub at: Pt2,
    pub text: String,
    pub kind: TlLabelKind,
}

/// One activity as plotted.
#[derive(Debug, Clone, PartialEq)]
pub struct PlottedActivity {
    pub id: String,
    pub name: String,
    pub derived: bool,
    pub kind: PlotKind,
    pub direction: WorkDirection,
    pub critical: bool,
    pub status: Status,
    pub percent_complete: f64,
    /// Station of the work front on the start date (the low station of a bar).
    pub start_station_m: f64,
    /// Station on the finish date (the high station of a bar).
    pub end_station_m: f64,
    /// Early start date (the line begins at the start of this day).
    pub start_date: Date,
    /// Early finish date (the line ends at the end of this day).
    pub finish_date: Date,
    pub dwell_days: Option<u32>,
    pub baseline_start: Option<Date>,
    pub baseline_finish: Option<Date>,
}

/// The chart: drawing primitives, separate labels (the draw crate has no text), and what was
/// plotted.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeLocationChart {
    pub drawing: Drawing,
    pub labels: Vec<TlLabel>,
    pub alignment: String,
    /// Every alignment that has activities, so the app can offer a choice.
    pub alignments: Vec<String>,
    pub plotted: Vec<PlottedActivity>,
    /// Stations at the two ends of the station scale (grid values, metres).
    pub station_min: f64,
    pub station_max: f64,
    /// The date at time zero of the scale (a Monday); a day is `day_scale` long.
    pub origin_date: Date,
    pub metre_scale: f64,
    pub day_scale: f64,
    pub time_vertical: bool,
    pub margin: f64,
}

impl TimeLocationChart {
    /// Drawing position of a station and a time given in days since the start of `origin_date`
    /// (the start of a date is its day number, the end of a date is the day number plus one).
    pub fn pos(&self, station_m: f64, days: f64) -> Pt2 {
        let s = self.margin + (station_m - self.station_min) * self.metre_scale;
        let t = self.margin + days * self.day_scale;
        if self.time_vertical {
            Pt2::new(s, t)
        } else {
            Pt2::new(t, s)
        }
    }

    /// Drawing position of a station and the start (`end_of_day` false) or end of a date.
    pub fn pos_at(&self, station_m: f64, date: Date, end_of_day: bool) -> Pt2 {
        self.pos(station_m, date.days_since(self.origin_date) as f64 + if end_of_day { 1.0 } else { 0.0 })
    }
}

/// A grid step of 1, 2 or 5 times a power of ten that is at least `span / 8`.
fn nice_step(span: f64) -> f64 {
    if !(span.is_finite() && span > 0.0) {
        return 1.0;
    }
    let raw = span / 8.0;
    let mag = 10f64.powf(raw.log10().floor());
    for m in [1.0, 2.0, 5.0, 10.0] {
        if mag * m >= raw {
            return mag * m;
        }
    }
    mag * 10.0
}

/// What the chart and its CSV plot, for one alignment.
fn plot_items(p: &Programme, s: &Schedule, res: &ResolvedLocations, alignment: &str, use_baseline: bool) -> Vec<PlottedActivity> {
    let baseline = if use_baseline { p.baseline.as_ref() } else { None };
    let mut out = Vec::new();
    for r in &s.rows {
        let Some(loc) = res.get(&r.id).filter(|l| l.range.alignment == alignment) else { continue };
        let range = &loc.range;
        let (lo, hi) = (range.low(), range.high());
        let milestone = r.is_milestone;
        let moving = !milestone && lo != hi && range.direction != WorkDirection::Fixed;
        let (sa, sb) = match range.direction {
            _ if !moving => (lo, hi),
            WorkDirection::Decreasing => (hi, lo),
            _ => (lo, hi),
        };
        let dwell = range.dwell_days.filter(|d| *d > 0);
        let kind = if milestone {
            PlotKind::Milestone
        } else if !moving {
            PlotKind::Bar
        } else if dwell.is_some() {
            PlotKind::Parallelogram
        } else {
            PlotKind::Line
        };
        let be = baseline.and_then(|b| b.entries.iter().find(|e| e.id == r.id));
        out.push(PlottedActivity {
            id: r.id.clone(),
            name: r.name.clone(),
            derived: loc.derived,
            kind,
            direction: range.direction,
            critical: r.critical,
            status: r.status,
            percent_complete: r.percent_complete,
            start_station_m: sa,
            end_station_m: sb,
            start_date: r.early_start,
            finish_date: r.early_finish,
            dwell_days: dwell,
            baseline_start: be.map(|e| e.start),
            baseline_finish: be.map(|e| e.finish),
        });
    }
    out
}

fn line(a: Pt2, b: Pt2, w: f64, c: Colour) -> Primitive {
    Primitive::Line { from: a, to: b, width: w, colour: Some(c) }
}

/// The time-location chart for one alignment. See the module documentation for the plot rules;
/// critical activities are red, complete ones grey, the rest blue; the baseline is a thin grey
/// outline of the same shape; the data date is a green line and the required finish a brown one.
/// Returns `None` when no activity has a location on the alignment.
pub fn time_location_chart(p: &Programme, s: &Schedule, res: &ResolvedLocations, o: &TimeLocationOptions) -> Option<TimeLocationChart> {
    let alignments = res.alignments();
    let alignment = match &o.alignment {
        Some(a) => a.clone(),
        None => alignments.first()?.clone(),
    };
    let plotted = plot_items(p, s, res, &alignment, o.baseline);
    if plotted.is_empty() {
        return None;
    }
    // Station scale.
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for it in &plotted {
        lo = lo.min(it.start_station_m.min(it.end_station_m));
        hi = hi.max(it.start_station_m.max(it.end_station_m));
    }
    let step = o.station_tick_m.filter(|t| *t > 0.0).unwrap_or_else(|| nice_step(hi - lo));
    let smin = (lo / step).floor() * step;
    let mut smax = (hi / step).ceil() * step;
    if smax <= smin {
        smax = smin + step;
    }
    // Date scale, starting on a Monday so the week ticks are whole.
    let mut first = plotted.iter().map(|i| i.start_date.min(i.baseline_start.unwrap_or(i.start_date))).min().expect("not empty");
    let mut last = plotted.iter().map(|i| i.finish_date.max(i.baseline_finish.unwrap_or(i.finish_date))).max().expect("not empty");
    for d in [s.data_date, p.required_finish].into_iter().flatten() {
        first = first.min(d);
        last = last.max(d);
    }
    while first.weekday() != Weekday::Mon {
        first = first.add_days(-1);
    }
    let days = last.days_since(first) + 2;
    let (ds, ms, margin) = (o.day_scale, o.metre_scale, o.margin);
    let header = margin;
    let (width, height) = if o.time_vertical {
        (margin + (smax - smin) * ms + 2.0, margin + days as f64 * ds + header)
    } else {
        (margin + days as f64 * ds + 2.0, margin + (smax - smin) * ms + header)
    };
    let mut chart = TimeLocationChart {
        drawing: Drawing { primitives: Vec::new(), extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(width, height) } },
        labels: Vec::new(),
        alignment: alignment.clone(),
        alignments,
        plotted: plotted.clone(),
        station_min: smin,
        station_max: smax,
        origin_date: first,
        metre_scale: ms,
        day_scale: ds,
        time_vertical: o.time_vertical,
        margin,
    };
    let mut prims: Vec<Primitive> = Vec::new();
    let mut labels: Vec<TlLabel> = Vec::new();
    let span_t = days as f64;
    let c = &chart;
    // Lines across the whole plot at a station / at a time.
    let across_station = |st: f64| (c.pos(st, 0.0), c.pos(st, span_t));
    let across_time = |t: f64| (c.pos(smin, t), c.pos(smax, t));
    labels.push(TlLabel { at: Pt2::new(2.0, height - header * 0.5),text: format!("{} - {}", p.name, alignment), kind: TlLabelKind::Title });

    // Station grid.
    let n_ticks = ((smax - smin) / step).round() as i64;
    for k in 0..=n_ticks {
        let st = smin + k as f64 * step;
        let (a, b) = across_station(st);
        prims.push(line(a, b, 0.0, Colour::Grey));
        let at = if o.time_vertical { Pt2::new(a.x, margin * 0.5) } else { Pt2::new(2.0, a.y) };
        labels.push(TlLabel { at, text: format!("{st}"), kind: TlLabelKind::Station });
    }
    // Time grid: week lines on Mondays, month lines on the first.
    for k in 0..days {
        let d = first.add_days(k);
        let (_, _, day) = d.ymd();
        let t = k as f64;
        if d.weekday() == Weekday::Mon {
            let (a, b) = across_time(t);
            prims.push(line(a, b, 0.0, Colour::Grey));
            let at = if o.time_vertical { Pt2::new(margin * 0.62, a.y) } else { Pt2::new(a.x, margin * 0.62) };
            labels.push(TlLabel { at, text: format!("{day:02}"), kind: TlLabelKind::Week });
        }
        if day == 1 || k == 0 {
            let (a, b) = across_time(t);
            prims.push(line(a, b, 0.1, Colour::Black));
            let at = if o.time_vertical { Pt2::new(1.0, a.y) } else { Pt2::new(a.x, margin * 0.25) };
            labels.push(TlLabel { at, text: format!("{} {}", d.month_name(), d.ymd().0), kind: TlLabelKind::Month });
        }
    }
    // Frame.
    let (o0, o1, o2, o3) = (c.pos(smin, 0.0), c.pos(smax, 0.0), c.pos(smax, span_t), c.pos(smin, span_t));
    for (a, b) in [(o0, o1), (o1, o2), (o2, o3), (o3, o0)] {
        prims.push(line(a, b, 0.2, Colour::Black));
    }

    // Shapes of an item, as a closed outline (bar, parallelogram), an open line, or a point.
    let shape = |sa: f64, sb: f64, t0: f64, t1: f64, kind: PlotKind, dwell: Option<u32>| -> (Vec<Pt2>, bool) {
        match kind {
            PlotKind::Line => (vec![c.pos(sa, t0), c.pos(sb, t1)], false),
            PlotKind::Parallelogram => {
                let w = (dwell.unwrap_or(0) as f64).min(t1 - t0).max(0.0);
                (vec![c.pos(sa, t0), c.pos(sb, t1 - w), c.pos(sb, t1), c.pos(sa, t0 + w)], true)
            }
            PlotKind::Bar | PlotKind::Milestone => {
                let (mut a, mut b) = (sa.min(sb), sa.max(sb));
                if (b - a) * ms < o.min_bar_width {
                    let mid = (a + b) / 2.0;
                    a = mid - o.min_bar_width / ms / 2.0;
                    b = mid + o.min_bar_width / ms / 2.0;
                }
                (vec![c.pos(a, t0), c.pos(b, t0), c.pos(b, t1), c.pos(a, t1)], true)
            }
        }
    };
    let outline = |pts: &[Pt2], closed: bool, w: f64, col: Colour, prims: &mut Vec<Primitive>| {
        for k in 0..pts.len().saturating_sub(1) {
            prims.push(line(pts[k], pts[k + 1], w, col));
        }
        if closed && pts.len() > 2 {
            prims.push(line(pts[pts.len() - 1], pts[0], w, col));
        }
    };
    let t_of = |d: Date, end: bool| d.days_since(first) as f64 + if end { 1.0 } else { 0.0 };

    // Baseline first (under), then the plan.
    for it in &plotted {
        if let (Some(bs), Some(bf)) = (it.baseline_start, it.baseline_finish) {
            if it.kind == PlotKind::Milestone {
                continue;
            }
            let (pts, closed) = shape(it.start_station_m, it.end_station_m, t_of(bs, false), t_of(bf, true), it.kind, it.dwell_days);
            outline(&pts, closed, o.line_width * 0.35, Colour::Grey, &mut prims);
        }
    }
    for it in &plotted {
        let col = if it.critical {
            Colour::Red
        } else if it.status == Status::Complete {
            Colour::Grey
        } else {
            Colour::Blue
        };
        let milestone_finish = p.activity(&it.id).is_some_and(|a| a.is_milestone() && a.milestone_kind == MilestoneKind::Finish);
        let (t0, t1) = if it.kind == PlotKind::Milestone {
            let t = t_of(it.start_date, milestone_finish);
            (t, t)
        } else {
            (t_of(it.start_date, false), t_of(it.finish_date, true))
        };
        let mid_station = (it.start_station_m + it.end_station_m) / 2.0;
        match it.kind {
            PlotKind::Line => {
                prims.push(line(c.pos(it.start_station_m, t0), c.pos(it.end_station_m, t1), o.line_width, col));
            }
            PlotKind::Milestone => {
                let h = ms.max(1e-9).recip() * (o.min_bar_width / 2.0);
                let pts = vec![c.pos(mid_station - h, t0), c.pos(mid_station, t0 + o.min_bar_width / 2.0 / ds), c.pos(mid_station + h, t0), c.pos(mid_station, t0 - o.min_bar_width / 2.0 / ds)];
                prims.push(Primitive::Fill { outline: pts, colour: col });
            }
            _ => {
                let (pts, _) = shape(it.start_station_m, it.end_station_m, t0, t1, it.kind, it.dwell_days);
                prims.push(Primitive::Fill { outline: pts, colour: col });
            }
        }
        labels.push(TlLabel { at: c.pos(mid_station, (t0 + t1) / 2.0), text: it.id.clone(), kind: TlLabelKind::Activity });
    }

    if let Some(dd) = s.data_date {
        let t = t_of(dd, true);
        let (a, b) = across_time(t);
        prims.push(line(a, b, 0.3, Colour::Green));
        let at = if o.time_vertical { Pt2::new(b.x - 30.0_f64.min(b.x), b.y + 0.5) } else { Pt2::new(b.x + 0.5, b.y - 2.0) };
        labels.push(TlLabel { at, text: format!("Data date {dd}"), kind: TlLabelKind::DataDate });
    }
    if let Some(rf) = p.required_finish {
        let t = t_of(rf, true);
        let (a, b) = across_time(t);
        prims.push(line(a, b, 0.3, Colour::Brown));
        labels.push(TlLabel { at: Pt2::new(a.x + 0.5, a.y + 0.5), text: format!("Required finish {rf}"), kind: TlLabelKind::RequiredFinish });
    }
    chart.drawing.primitives = prims;
    chart.labels = labels;
    Some(chart)
}

pub const TIME_LOCATION_CSV_HEADER: &str = "id,name,alignment,source,kind,direction,critical,status,percent_complete,start_station_m,start_date,end_station_m,finish_date,dwell_days,baseline_start,baseline_finish";

fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The plotted data as CSV: one row per plotted activity of the alignment (the one in the options,
/// else the first), in programme order. The start date is the day the line begins (at its start),
/// the finish date the day it ends (at its end of day). Stations in metres.
pub fn time_location_csv(p: &Programme, s: &Schedule, res: &ResolvedLocations, o: &TimeLocationOptions) -> String {
    let mut out = String::from(TIME_LOCATION_CSV_HEADER);
    out.push('\n');
    let Some(alignment) = o.alignment.clone().or_else(|| res.alignments().first().cloned()) else { return out };
    for it in plot_items(p, s, res, &alignment, o.baseline) {
        out.push_str(&format!(
            "{},{},{},{},{},{:?},{},{:?},{},{},{},{},{},{},{},{}\n",
            csv(&it.id),
            csv(&it.name),
            csv(&alignment),
            if it.derived { "derived" } else { "entered" },
            it.kind.text(),
            it.direction,
            if it.critical { "yes" } else { "no" },
            it.status,
            it.percent_complete,
            it.start_station_m,
            it.start_date,
            it.end_station_m,
            it.finish_date,
            it.dwell_days.map(|d| d.to_string()).unwrap_or_default(),
            it.baseline_start.map(|d| d.to_string()).unwrap_or_default(),
            it.baseline_finish.map(|d| d.to_string()).unwrap_or_default(),
        ));
    }
    out
}
