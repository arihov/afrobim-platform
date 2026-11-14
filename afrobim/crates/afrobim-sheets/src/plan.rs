//! The plan view: a station range of an alignment at a scale, clipped to a viewport and turned so
//! the alignment runs along the sheet, with every module's plan drawing, station ticks and
//! labels (displayed stations), element marks and match lines. Also the plan sheet series.
//!
//! Conventions: model coordinates are metres, X east and Y north (an element's drawing is in
//! millimetres and is placed by its site placement, which is also in millimetres, as everywhere in
//! AfroBIM). A point `p` of the model lands on paper at
//!
//! ```text
//!   paper = R(theta) (p - ref) * k + c          k = 1000 / n      (paper mm per metre at 1:n)
//! ```
//!
//! where R(theta) turns anticlockwise by `theta`, `ref` is chosen so that the middle of the
//! turned centreline's bounding box lands on `c`, the middle of the viewport.

use afrobim_alignment::Alignment;
use afrobim_draw::Pt2;
use serde::{Deserialize, Serialize};

use crate::draw::draw_drawing;
use crate::geom::{clip_polyline, clip_segment, dash_segment, Aff, Rect};
use crate::item::{layers, text as mk_text, ComposedSheet};
use crate::model::{station_text, stations_at_interval, Ctx};
use crate::paper::Scale;
use crate::text::{text_width, Align};
use crate::titleblock::{north_arrow, scale_bar};

/// How the plan is turned on the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PlanRotation {
    /// North up the sheet.
    NorthUp,
    /// Turned so the chord from the start to the end of the station range runs along the sheet
    /// (left to right).
    AlignChord,
    /// Turned anticlockwise by this many degrees.
    Degrees(f64),
}

/// A match line: where this sheet meets its neighbour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchLine {
    /// Internal station.
    pub station_m: f64,
    /// Printed beside the line, e.g. "CONTINUED ON SHEET R-003".
    pub text: String,
}

/// A plan view definition. Every number is entered by the user; the defaults are neutral
/// starting values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanView {
    /// The alignment element's name.
    pub alignment: String,
    pub from_station_m: f64,
    pub to_station_m: f64,
    pub scale: Scale,
    pub rotation: PlanRotation,
    /// Spacing of station ticks, displayed stations, m.
    pub tick_interval_m: f64,
    /// Spacing of station labels, displayed stations, m.
    pub label_interval_m: f64,
    pub station_decimals: usize,
    /// Length of a minor tick on paper, mm; a labelled tick is 1.6 times as long.
    pub tick_length_mm: f64,
    /// Distance from the centreline to the start of a station label on paper, mm (at least far
    /// enough to clear the tick). The labels stand on the left of the alignment.
    pub label_offset_mm: f64,
    /// Label elements by their mark (name).
    pub show_marks: bool,
    /// Categories to draw; `None` draws every category that has a module and a site placement,
    /// except those in `exclude_categories`.
    pub categories: Option<Vec<String>>,
    pub exclude_categories: Vec<String>,
    /// Categories drawn as a short footprint line of the element's width instead of their own
    /// drawing (a sign's drawing is its face seen from the front, not a plan).
    pub symbol_categories: Vec<String>,
    /// Skip elements on layers that are switched off in the document. Off by default: modules
    /// whose plan geometry is far from the sign design sheet (alignments, markings, corridors)
    /// are put on layers that start hidden there, and a plan sheet should still show them.
    pub only_visible_layers: bool,
    pub north_arrow: bool,
    pub scale_bar: bool,
    pub match_lines: Vec<MatchLine>,
    /// Caption at the top left of the viewport; empty for the default "PLAN  1:n".
    pub caption: String,
}

impl PlanView {
    pub fn new(alignment: &str, from_station_m: f64, to_station_m: f64, scale: Scale) -> PlanView {
        PlanView {
            alignment: alignment.to_string(),
            from_station_m,
            to_station_m,
            scale,
            rotation: PlanRotation::AlignChord,
            tick_interval_m: 20.0,
            label_interval_m: 100.0,
            station_decimals: 0,
            tick_length_mm: 3.0,
            label_offset_mm: 8.0,
            show_marks: true,
            categories: None,
            exclude_categories: ["alignment", "sheets", "project_info", "schedule", "surface"].iter().map(|s| s.to_string()).collect(),
            symbol_categories: vec!["sign".to_string()],
            only_visible_layers: false,
            north_arrow: true,
            scale_bar: true,
            match_lines: Vec::new(),
            caption: String::new(),
        }
    }
}

/// The paper transform of a plan view, see the module documentation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanTransform {
    /// Model metres to paper millimetres.
    pub map: Aff,
    /// The rotation applied, degrees anticlockwise.
    pub rotation_deg: f64,
    /// Size of the turned centreline's bounding box on paper, mm.
    pub extent_mm: (f64, f64),
}

/// The centreline between two stations, sampled finely enough that no chord strays more than
/// 0.03 mm on paper from the true curve at `scale`.
pub fn sample_centreline(a: &Alignment, s0: f64, s1: f64, scale: Scale) -> Vec<(f64, f64, f64)> {
    let sag_m = 0.03 / scale.paper_mm_per_m();
    // Sagitta of a chord c on radius R is c^2 / 8R, so c = sqrt(8 R sag).
    let chord = a.min_radius().map_or(20.0, |r| (8.0 * r * sag_m).sqrt()).clamp(0.1, 20.0);
    let n = (((s1 - s0) / chord).ceil() as usize).clamp(1, 20_000);
    (0..=n)
        .filter_map(|i| {
            let s = s0 + (s1 - s0) * i as f64 / n as f64;
            a.point_at(s).map(|p| (p.x, p.y, s))
        })
        .collect()
}

/// Works out where a plan view puts the model on paper. `None` when the alignment has no points
/// in the range.
pub fn plan_transform(a: &Alignment, s0: f64, s1: f64, scale: Scale, rotation: PlanRotation, rect: &Rect) -> Option<PlanTransform> {
    let pts = sample_centreline(a, s0, s1, scale);
    let (first, last) = (pts.first()?, pts.last()?);
    let theta = match rotation {
        PlanRotation::NorthUp => 0.0,
        PlanRotation::Degrees(d) => d,
        PlanRotation::AlignChord => {
            let (dx, dy) = (last.0 - first.0, last.1 - first.1);
            if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
                0.0
            } else {
                -dy.atan2(dx).to_degrees()
            }
        }
    };
    let k = scale.paper_mm_per_m();
    let turn = Aff::scale_rotate(k, theta);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in &pts {
        let q = turn.apply(Pt2::new(p.0, p.1));
        x0 = x0.min(q.x);
        y0 = y0.min(q.y);
        x1 = x1.max(q.x);
        y1 = y1.max(q.y);
    }
    let c = rect.centre();
    let map = turn.then(&Aff::translate(c.x - (x0 + x1) / 2.0, c.y - (y0 + y1) / 2.0));
    Some(PlanTransform { map, rotation_deg: theta, extent_mm: (x1 - x0, y1 - y0) })
}

/// Normalises a text angle into (-90, 90] so text reads upright or upward; returns the angle and
/// whether it was flipped by 180 degrees.
fn readable(deg: f64) -> (f64, bool) {
    let mut a = (deg + 180.0).rem_euclid(360.0) - 180.0;
    let mut flipped = false;
    if a > 90.0 + 1e-9 {
        a -= 180.0;
        flipped = true;
    } else if a <= -90.0 + 1e-9 {
        a += 180.0;
        flipped = true;
    }
    (a, flipped)
}

/// Composes a plan view into `rect`.
pub fn compose_plan(sheet: &mut ComposedSheet, ctx: &Ctx, view: &PlanView, rect: &Rect) {
    let styles = ctx.styles;
    if !view.scale.is_valid() {
        sheet.warn("A plan view has no valid scale.");
        return;
    }
    let alignment = match ctx.model.alignment(&view.alignment) {
        Ok(a) => a,
        Err(e) => {
            sheet.warn(e);
            return;
        }
    };
    let (a_start, a_end) = (alignment.start_station_m, alignment.end_station_m());
    let (s0, s1) = (view.from_station_m.max(a_start), view.to_station_m.min(a_end));
    if s1.is_nan() || s1 <= s0 {
        sheet.warn(format!("The plan range {:.3} to {:.3} is outside alignment '{}' ({:.3} to {:.3}).", view.from_station_m, view.to_station_m, view.alignment, a_start, a_end));
        return;
    }
    let Some(tf) = plan_transform(&alignment, s0, s1, view.scale, view.rotation, rect) else {
        sheet.warn("The plan range has no points on the alignment.");
        return;
    };
    let k = view.scale.paper_mm_per_m();
    if tf.extent_mm.0 > rect.width() + 0.5 || tf.extent_mm.1 > rect.height() + 0.5 {
        sheet.warn(format!(
            "The plan of '{}' from {} to {} needs {:.0} x {:.0} mm at {} but the viewport is {:.0} x {:.0} mm: it is cut at the edges. Use a smaller station range or a smaller scale.",
            view.alignment,
            station_text(&alignment, s0, 0),
            station_text(&alignment, s1, 0),
            tf.extent_mm.0,
            tf.extent_mm.1,
            view.scale.label(),
            rect.width(),
            rect.height()
        ));
    }
    let pen = |l: &str| styles.pens.pen(l);

    // Model elements. `marks` collects where each one's mark goes: the middle of the part of the
    // element that is inside the viewport.
    let doc = ctx.model.doc;
    let mut marks: Vec<(String, Pt2)> = Vec::new();
    for e in doc.elements() {
        let Some(site) = &e.site else { continue };
        if view.exclude_categories.contains(&e.category) || view.categories.as_ref().is_some_and(|c| !c.contains(&e.category)) {
            continue;
        }
        if view.only_visible_layers && !doc.layer_visible(&e.layer) {
            continue;
        }
        // Local millimetres to model metres to paper.
        let local = Aff::scale_rotate(1.0, site.rotation_deg).then(&Aff::translate(site.x, site.y)).then(&Aff::scale_rotate(0.001, 0.0)).then(&tf.map);
        if view.symbol_categories.contains(&e.category) {
            let (a, b) = (local.apply(Pt2::new(0.0, 0.0)), local.apply(Pt2::new(e.width.max(1.0), 0.0)));
            let weight = pen(&e.layer).weight_mm * 3.0;
            if let Some((p, q)) = clip_segment(a, b, rect) {
                sheet.line(&e.layer, p, q, weight, pen(&e.layer).rgb);
                marks.push((e.name.clone(), Pt2::new((p.x + q.x) / 2.0, (p.y + q.y) / 2.0)));
            }
            continue;
        }
        if ctx.model.registry.get(&e.category).is_none() {
            continue;
        }
        match ctx.model.registry.generate(e) {
            Ok(g) => {
                draw_drawing(sheet, styles, &g.drawing, &local, rect, &e.layer);
                let ext = g.drawing.extents;
                let corners = [Pt2::new(ext.min.x, ext.min.y), Pt2::new(ext.max.x, ext.min.y), Pt2::new(ext.max.x, ext.max.y), Pt2::new(ext.min.x, ext.max.y)].map(|c| local.apply(c));
                let (x0, x1) = (corners.iter().map(|c| c.x).fold(f64::INFINITY, f64::min).max(rect.x0), corners.iter().map(|c| c.x).fold(f64::NEG_INFINITY, f64::max).min(rect.x1));
                let (y0, y1) = (corners.iter().map(|c| c.y).fold(f64::INFINITY, f64::min).max(rect.y0), corners.iter().map(|c| c.y).fold(f64::NEG_INFINITY, f64::max).min(rect.y1));
                if x1 > x0 || y1 > y0 {
                    marks.push((e.name.clone(), Pt2::new((x0 + x1) / 2.0, (y0 + y1) / 2.0)));
                }
            }
            Err(err) => sheet.warn(format!("Element '{}' is not drawn: {err}", e.name)),
        }
    }

    // The alignment, its ticks and labels.
    let pts = sample_centreline(&alignment, s0, s1, view.scale);
    let line: Vec<Pt2> = pts.iter().map(|p| tf.map.apply(Pt2::new(p.0, p.1))).collect();
    let ap = pen(layers::PLAN_ALIGNMENT);
    for piece in clip_polyline(&line, rect) {
        sheet.stroke(layers::PLAN_ALIGNMENT, piece, false, ap.weight_mm, ap.rgb);
    }
    let sp = pen(layers::PLAN_STATIONS);
    let label_h = styles.text.label;
    let major = stations_at_interval(&alignment, s0, s1, view.label_interval_m);
    let is_major = |s: f64| major.iter().any(|m| (m - s).abs() < 1e-6);
    let mut ticks = stations_at_interval(&alignment, s0, s1, view.tick_interval_m);
    ticks.extend(major.iter().copied());
    ticks.sort_by(f64::total_cmp);
    ticks.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    for s in ticks {
        let Some(p) = alignment.point_at(s) else { continue };
        let len_mm = if is_major(s) { view.tick_length_mm * 1.6 } else { view.tick_length_mm };
        let half_m = len_mm / 2.0 / k;
        let (l, r) = (tf.map.apply(point(p.offset(-half_m))), tf.map.apply(point(p.offset(half_m))));
        if let Some((a, b)) = clip_segment(l, r, rect) {
            sheet.line(layers::PLAN_STATIONS, a, b, sp.weight_mm, sp.rgb);
        }
        if is_major(s) {
            // The label stands at the outer end of the tick on the left, turned square to the
            // alignment so it reads upward (or upright).
            let anchor_m = (len_mm / 2.0 + 0.8).max(view.label_offset_mm) / k;
            let at = tf.map.apply(point(p.offset(-anchor_m)));
            let away = p.heading_rad.to_degrees() + tf.rotation_deg + 90.0;
            let (rot, flipped) = readable(away);
            if rect.inset(label_h).contains(at) {
                let text = station_text(&alignment, s, view.station_decimals);
                sheet.text(mk_text(layers::PLAN_STATIONS, at, label_h, text, if flipped { Align::Right } else { Align::Left }, rot, false, sp.rgb));
            }
        }
    }

    // Element marks, skipping a mark that would sit on one already placed.
    if view.show_marks {
        let mut placed: Vec<Rect> = Vec::new();
        let mp = pen(layers::PLAN_MARKS);
        for (name, mid) in &marks {
            if name.is_empty() {
                continue;
            }
            let at = Pt2::new(mid.x + 0.8, mid.y + 0.8);
            let w = text_width(name, label_h, false);
            let box_ = Rect::new(at.x, at.y, at.x + w, at.y + label_h);
            let inside = rect.inset(0.5);
            if !inside.contains(Pt2::new(box_.x0, box_.y0)) || !inside.contains(Pt2::new(box_.x1, box_.y1)) {
                continue;
            }
            if placed.iter().any(|p| p.x0 < box_.x1 && p.x1 > box_.x0 && p.y0 < box_.y1 && p.y1 > box_.y0) {
                continue;
            }
            placed.push(box_);
            sheet.text(mk_text(layers::PLAN_MARKS, at, label_h, name.clone(), Align::Left, 0.0, false, mp.rgb));
        }
    }

    // Match lines.
    let mp = pen(layers::PLAN_MATCH);
    for m in &view.match_lines {
        if m.station_m < s0 - 1e-6 || m.station_m > s1 + 1e-6 {
            continue;
        }
        let Some(p) = alignment.point_at(m.station_m) else { continue };
        let reach = (rect.width() + rect.height()) / k;
        let (l, r) = (tf.map.apply(point(p.offset(-reach))), tf.map.apply(point(p.offset(reach))));
        let Some((a, b)) = clip_segment(l, r, rect) else { continue };
        for (d0, d1) in dash_segment(a, b, 6.0, 2.0) {
            sheet.line(layers::PLAN_MATCH, d0, d1, mp.weight_mm, mp.rgb);
        }
        // Text beside the line, reading along it, starting 6 mm from where the line crosses the
        // alignment and running away from it along the longer side.
        let crossing = tf.map.apply(point((p.x, p.y)));
        let far = if a.distance_to(crossing) >= b.distance_to(crossing) { a } else { b };
        let len = far.distance_to(crossing).max(1e-9);
        let dir = ((far.x - crossing.x) / len, (far.y - crossing.y) / len);
        let (rot, flipped) = readable(dir.1.atan2(dir.0).to_degrees());
        let start = Pt2::new(crossing.x + dir.0 * 6.0, crossing.y + dir.1 * 6.0);
        let (s, c) = rot.to_radians().sin_cos();
        let h = styles.text.normal;
        let lines = [format!("MATCH LINE STA {}", station_text(&alignment, m.station_m, view.station_decimals)), m.text.clone()];
        for (i, text) in lines.iter().enumerate().filter(|(_, t)| !t.is_empty()) {
            let off = 2.2 + i as f64 * h * 1.5;
            let at = Pt2::new(start.x - off * s, start.y + off * c);
            sheet.text(mk_text(layers::PLAN_MATCH, at, h, text.clone(), if flipped { Align::Right } else { Align::Left }, rot, i == 0, mp.rgb));
        }
    }

    // Furniture and caption.
    let fp = pen(layers::VIEWPORT);
    sheet.rect(layers::VIEWPORT, rect, fp.weight_mm, fp.rgb);
    let caption = if view.caption.is_empty() { format!("PLAN  {}", view.scale.label()) } else { view.caption.clone() };
    sheet.text(mk_text(layers::TEXT, Pt2::new(rect.x0 + 2.0, rect.y1 - 2.0 - styles.text.heading), styles.text.heading, caption, Align::Left, 0.0, true, fp.rgb));
    if view.north_arrow {
        let north = 90.0 + tf.rotation_deg;
        north_arrow(sheet, Pt2::new(rect.x1 - 12.0, rect.y1 - 14.0), 12.0, north, styles);
    }
    if view.scale_bar {
        scale_bar(sheet, Pt2::new(rect.x0 + 4.0, rect.y0 + 4.0), view.scale, (rect.width() * 0.25).min(60.0), styles);
    }
}

fn point(p: (f64, f64)) -> Pt2 {
    Pt2::new(p.0, p.1)
}

/// The station windows of a plan sheet series: sheets of `length_m` of alignment, each starting
/// `overlap_m` before the previous one ends, covering `from_m` to `to_m` with no gap. The last
/// window ends exactly at `to_m` (and so may be shorter). `None` when the numbers cannot make a
/// series: a non-positive length or an overlap that is not smaller than the length.
pub fn plan_windows(from_m: f64, to_m: f64, length_m: f64, overlap_m: f64) -> Option<Vec<(f64, f64)>> {
    if !(length_m > 0.0 && overlap_m >= 0.0 && overlap_m < length_m && to_m > from_m && from_m.is_finite() && to_m.is_finite()) {
        return None;
    }
    let step = length_m - overlap_m;
    let mut out = Vec::new();
    let mut start = from_m;
    loop {
        let end = (start + length_m).min(to_m);
        out.push((start, end));
        if end >= to_m - 1e-9 {
            break;
        }
        start += step;
    }
    Some(out)
}

/// The match lines of window `i` of `windows`: at the middle of the overlap with each neighbour,
/// worded with the neighbour's sheet number (`numbers[j]`).
pub fn match_lines_for(windows: &[(f64, f64)], i: usize, numbers: &[String]) -> Vec<MatchLine> {
    let mut out = Vec::new();
    if i > 0 {
        let seam = (windows[i].0 + windows[i - 1].1) / 2.0;
        out.push(MatchLine { station_m: seam, text: format!("CONTINUED ON SHEET {}", numbers[i - 1]) });
    }
    if i + 1 < windows.len() {
        let seam = (windows[i].1 + windows[i + 1].0) / 2.0;
        out.push(MatchLine { station_m: seam, text: format!("CONTINUED ON SHEET {}", numbers[i + 1]) });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_alignment::AlignmentBuilder;

    #[test]
    fn windows_cover_the_range_with_the_overlap_and_no_gap() {
        // 0 to 1000 m in sheets of 400 m overlapping 50 m: starts step by 350: 0, 350, 700 (ends 400, 750, 1000).
        let w = plan_windows(0.0, 1000.0, 400.0, 50.0).unwrap();
        assert_eq!(w, vec![(0.0, 400.0), (350.0, 750.0), (700.0, 1000.0)]);
        for pair in w.windows(2) {
            assert!((pair[0].1 - pair[1].0 - 50.0).abs() < 1e-9); // the entered overlap
        }
        assert_eq!(w[0].0, 0.0);
        assert_eq!(w.last().unwrap().1, 1000.0);
        // A range shorter than a sheet is one sheet; an exact fit has no empty tail sheet.
        assert_eq!(plan_windows(100.0, 300.0, 400.0, 50.0).unwrap(), vec![(100.0, 300.0)]);
        assert_eq!(plan_windows(0.0, 750.0, 400.0, 50.0).unwrap(), vec![(0.0, 400.0), (350.0, 750.0)]);
        assert!(plan_windows(0.0, 100.0, 50.0, 50.0).is_none());
        assert!(plan_windows(0.0, 100.0, 0.0, 0.0).is_none());
        // Zero overlap: windows touch.
        assert_eq!(plan_windows(0.0, 300.0, 100.0, 0.0).unwrap(), vec![(0.0, 100.0), (100.0, 200.0), (200.0, 300.0)]);
    }

    #[test]
    fn every_station_of_the_range_is_on_some_sheet() {
        let (from, to) = (123.4, 987.6);
        let w = plan_windows(from, to, 175.0, 25.0).unwrap();
        let mut s = from;
        while s <= to {
            assert!(w.iter().any(|(a, b)| s >= *a - 1e-9 && s <= *b + 1e-9), "station {s} is on no sheet");
            s += 0.5;
        }
    }

    #[test]
    fn match_lines_sit_in_the_middle_of_the_overlap_and_name_the_neighbour() {
        let w = plan_windows(0.0, 1000.0, 400.0, 50.0).unwrap();
        let numbers: Vec<String> = ["S1", "S2", "S3"].iter().map(|s| s.to_string()).collect();
        let m = match_lines_for(&w, 1, &numbers);
        // Overlap of sheets 1 and 2 is 350 to 400 (seam 375); of 2 and 3 is 700 to 750 (seam 725).
        assert_eq!((m[0].station_m, m[0].text.as_str()), (375.0, "CONTINUED ON SHEET S1"));
        assert_eq!((m[1].station_m, m[1].text.as_str()), (725.0, "CONTINUED ON SHEET S3"));
        assert_eq!(match_lines_for(&w, 0, &numbers).len(), 1);
        assert_eq!(match_lines_for(&w, 2, &numbers).len(), 1);
    }

    #[test]
    fn the_transform_centres_the_range_at_the_scale() {
        // East-running line from (1000, 2000), 1:1000 so 1 m is 1 mm. Stations 100 to 300 span
        // x = 1100 to 1300 (middle 1200). In a viewport centred on (150, 50) station 100 lands at
        // 150 + (1100 - 1200) = 50 and station 300 at 250.
        let a = AlignmentBuilder::new("A", 0.0, 1000.0, 2000.0, 0.0).line(400.0).build().unwrap();
        let rect = Rect::new(0.0, 0.0, 300.0, 100.0);
        let tf = plan_transform(&a, 100.0, 300.0, Scale(1000.0), PlanRotation::AlignChord, &rect).unwrap();
        assert!(tf.rotation_deg.abs() < 1e-9);
        let p = tf.map.apply(Pt2::new(1100.0, 2000.0));
        assert!((p.x - 50.0).abs() < 1e-9 && (p.y - 50.0).abs() < 1e-9, "{p:?}");
        let q = tf.map.apply(Pt2::new(1300.0, 2000.0));
        assert!((q.x - 250.0).abs() < 1e-9);
        assert!((tf.extent_mm.0 - 200.0).abs() < 1e-9);
        // 1:500 doubles the lengths: 200 m is 400 mm.
        let tf = plan_transform(&a, 100.0, 300.0, Scale(500.0), PlanRotation::NorthUp, &rect).unwrap();
        assert!((tf.extent_mm.0 - 400.0).abs() < 1e-9);
    }

    #[test]
    fn a_north_running_alignment_is_turned_to_run_along_the_sheet() {
        // Heading 90 degrees (north). The chord angle is 90, so the plan is turned by -90: a point
        // 100 m further north lands 100 mm (at 1:1000) further right.
        let a = AlignmentBuilder::new("A", 0.0, 0.0, 0.0, std::f64::consts::FRAC_PI_2).line(400.0).build().unwrap();
        let rect = Rect::new(0.0, 0.0, 300.0, 100.0);
        let tf = plan_transform(&a, 0.0, 200.0, Scale(1000.0), PlanRotation::AlignChord, &rect).unwrap();
        assert!((tf.rotation_deg + 90.0).abs() < 1e-9);
        let (p, q) = (tf.map.apply(Pt2::new(0.0, 0.0)), tf.map.apply(Pt2::new(0.0, 100.0)));
        assert!((q.x - p.x - 100.0).abs() < 1e-9 && (q.y - p.y).abs() < 1e-9);
        // North then points along +X on the sheet: the north arrow angle is 90 - 90 = 0.
        assert!(((90.0 + tf.rotation_deg) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn text_angles_are_made_readable() {
        assert_eq!(readable(90.0), (90.0, false));
        assert_eq!(readable(0.0), (0.0, false));
        let (a, f) = readable(135.0);
        assert!((a + 45.0).abs() < 1e-9 && f);
        let (a, f) = readable(-90.0);
        assert!((a - 90.0).abs() < 1e-9 && f);
    }
}
