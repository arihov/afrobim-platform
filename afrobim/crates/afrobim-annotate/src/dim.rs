//! Dimensions: definition points in, measured value and drawn geometry out.
//!
//! A [`DimData`] stores only definition points and a style. The measured value is recomputed from
//! the points every time the dimension is generated, so moving a point updates the dimension. The
//! value last computed is also kept in [`DimData::measured`] (refreshed by
//! [`crate::element::normalize`]) so a text override never loses it.
//!
//! # Geometry rules (this crate's definitions)
//!
//! *Linear and aligned.* The measured direction is `u` (+X for horizontal, +Y for vertical, the
//! given angle for rotated, the direction from the first to the second point for aligned). The
//! value is `|(p2 - p1) . u|`. The dimension line lies along `u` through the dimension line point,
//! at the signed distance `n . line_pt` where `n` is `u` turned 90 degrees anticlockwise. Each
//! extension line runs from the measured point, plus the extension offset, along the normal to
//! the dimension line, to the extension distance beyond the dimension line.
//!
//! *Where the text goes.* With `L` the length of the dimension line, `w` the text width, `g` the
//! text gap and `a` the arrow size (all in model mm):
//!
//! 1. Arrows fit inside when `L >= 2a` (a `None` terminator always fits).
//! 2. The text fits inside when `w + 2g <= L`, and, for centred placement, also `2a` more.
//! 3. Arrows and text both inside: the normal case.
//! 4. Arrows inside, text does not fit: the text moves outside, past the second end of the line,
//!    on an extension of the dimension line.
//! 5. Arrows do not fit: they are drawn outside, pointing at the extension lines from beyond
//!    them with a tail of `a`, and a text that does not fit goes outside past the second end.
//!
//! A note is added to the result whenever something moved outside.

use afrobim_alignment::Alignment;
use afrobim_sheets::{text_width, Align};
use serde::{Deserialize, Serialize};

use crate::geometry::*;
use crate::styles::{DimStyle, TextPlacement, Terminator};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LinearOrient {
    /// Along the line from the first point to the second.
    Aligned,
    Horizontal,
    Vertical,
    /// The measured direction, degrees anticlockwise from +X.
    Rotated(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChainMode {
    /// Each dimension starts where the one before ended, all on one dimension line.
    Continued,
    /// Each dimension starts at the first point; the lines are stacked.
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LevelSource {
    /// Typed by the user.
    Entered,
    /// Sampled from a surface by the caller.
    Sampled,
}

/// What is dimensioned. All points are in the element's local frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DimKind {
    Linear { p1: Pt, p2: Pt, line_pt: Pt, orient: LinearOrient },
    Chain { mode: ChainMode, pts: Vec<Pt>, line_pt: Pt, orient: LinearOrient },
    /// The angle at `vertex` from the arm through `p1` to the arm through `p2`; `arc_pt` places
    /// the arc and picks which of the two possible angles is dimensioned (the one containing it).
    Angular { vertex: Pt, p1: Pt, p2: Pt, arc_pt: Pt },
    /// The angle between line a1-a2 and line b1-b2; `arc_pt` picks the sector.
    AngularLines { a1: Pt, a2: Pt, b1: Pt, b2: Pt, arc_pt: Pt },
    /// `at` gives the direction from the centre in which the dimension line is drawn.
    Radius { centre: Pt, radius: f64, at: Pt },
    Diameter { centre: Pt, radius: f64, at: Pt },
    /// Arc length of the arc from `start_deg` to `end_deg` anticlockwise; the dimension arc lies
    /// `offset` (model mm) outside the arc.
    ArcLength { centre: Pt, radius: f64, start_deg: f64, end_deg: f64, offset: f64 },
    /// Station and offset of `point` on the named alignment, with the text at `text_pt`. The
    /// point is in site millimetres: the alignment's metres are the point's millimetres / 1000.
    StationOffset { point: Pt, alignment: String, text_pt: Pt },
    /// A level at `point`, supplied by the caller (typed or sampled); the text goes at `text_pt`.
    Level { point: Pt, value_m: f64, source: LevelSource, text_pt: Pt },
}

/// What was looked up in an alignment, kept so a dimension can be drawn without the document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DimCache {
    /// Displayed station, m.
    pub station_m: Option<f64>,
    /// Offset, m, positive to the right looking up-station.
    pub offset_m: Option<f64>,
    /// Station equation region of the displayed station.
    pub region: Option<usize>,
    /// Why there is no station: "no such alignment", "off the alignment".
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimData {
    pub kind: DimKind,
    pub style: DimStyle,
    /// Text overrides by value index (`<>` stands for the measured text). Missing means none.
    #[serde(default)]
    pub overrides: Vec<Option<String>>,
    /// The measured values at the last normalisation: lengths in model mm, angles in degrees,
    /// levels and stations in metres.
    #[serde(default)]
    pub measured: Vec<f64>,
    #[serde(default)]
    pub cache: DimCache,
}

/// A dimension's computed form.
#[derive(Debug, Clone, PartialEq)]
pub struct DimResult {
    pub geometry: AnnGeometry,
    pub measured: Vec<f64>,
    /// The text of each value as drawn (after formatting and overrides).
    pub texts: Vec<String>,
}

impl DimData {
    pub fn new(kind: DimKind, style: &DimStyle) -> DimData {
        DimData { kind, style: style.clone(), overrides: Vec::new(), measured: Vec::new(), cache: DimCache::default() }
    }

    /// Every definition point, for grips and for moving.
    pub fn points_mut(&mut self) -> Vec<&mut Pt> {
        match &mut self.kind {
            DimKind::Linear { p1, p2, line_pt, .. } => vec![p1, p2, line_pt],
            DimKind::Chain { pts, line_pt, .. } => {
                let mut v: Vec<&mut Pt> = pts.iter_mut().collect();
                v.push(line_pt);
                v
            }
            DimKind::Angular { vertex, p1, p2, arc_pt } => vec![vertex, p1, p2, arc_pt],
            DimKind::AngularLines { a1, a2, b1, b2, arc_pt } => vec![a1, a2, b1, b2, arc_pt],
            DimKind::Radius { centre, at, .. } | DimKind::Diameter { centre, at, .. } => vec![centre, at],
            DimKind::ArcLength { centre, .. } => vec![centre],
            DimKind::StationOffset { point, text_pt, .. } | DimKind::Level { point, text_pt, .. } => vec![point, text_pt],
        }
    }

    pub fn points(&self) -> Vec<Pt> {
        let mut c = self.clone();
        c.points_mut().into_iter().map(|p| *p).collect()
    }

    pub fn translate(&mut self, d: Pt) {
        for p in self.points_mut() {
            *p = add(*p, d);
        }
    }

    fn override_for(&self, i: usize) -> Option<&str> {
        self.overrides.get(i).and_then(|o| o.as_deref())
    }

    /// Computes the measured values, the text and the geometry.
    pub fn compute(&self) -> Result<DimResult, String> {
        let st = &self.style;
        let mut g = AnnGeometry::default();
        let (measured, texts): (Vec<f64>, Vec<String>) = match &self.kind {
            DimKind::Linear { p1, p2, line_pt, orient } => {
                let u = orient_dir(*orient, *p1, *p2)?;
                let m = dot(sub(*p2, *p1), u).abs();
                if m < 1e-9 {
                    return Err("The two points are the same distance apart along the dimension direction: zero length.".into());
                }
                let t = DimStyle::with_override(&st.format_length(m), self.override_for(0));
                linear_dim(&mut g, st, *p1, *p2, u, dot(*line_pt, perp(u)), &t, true, true);
                (vec![m], vec![t])
            }
            DimKind::Chain { mode, pts, line_pt, orient } => {
                if pts.len() < 3 {
                    return Err("A chain needs a first point and at least two more.".into());
                }
                let u = orient_dir(*orient, pts[0], pts[1])?;
                let n = perp(u);
                let c0 = dot(*line_pt, n);
                let side = if c0 - dot(pts[0], n) >= 0.0 { 1.0 } else { -1.0 };
                let step = st.m(st.baseline_spacing_mm);
                let (mut ms, mut ts) = (Vec::new(), Vec::new());
                for i in 1..pts.len() {
                    let (a, b) = match mode {
                        ChainMode::Continued => (pts[i - 1], pts[i]),
                        ChainMode::Baseline => (pts[0], pts[i]),
                    };
                    let m = dot(sub(b, a), u).abs();
                    let t = DimStyle::with_override(&st.format_length(m), self.override_for(i - 1));
                    match mode {
                        ChainMode::Continued => linear_dim(&mut g, st, a, b, u, c0, &t, i == 1, true),
                        ChainMode::Baseline => {
                            let c = c0 + side * (i as f64 - 1.0) * step;
                            linear_dim(&mut g, st, a, b, u, c, &t, i == pts.len() - 1, true);
                        }
                    }
                    ms.push(m);
                    ts.push(t);
                }
                (ms, ts)
            }
            DimKind::Angular { vertex, p1, p2, arc_pt } => {
                let (a0, sweep) = angular_sector(*vertex, *p1, *p2, *arc_pt)?;
                let t = DimStyle::with_override(&st.format_angle(sweep), self.override_for(0));
                angular_dim(&mut g, st, *vertex, a0, sweep, dist(*vertex, *arc_pt), *p1, *p2, &t);
                (vec![sweep], vec![t])
            }
            DimKind::AngularLines { a1, a2, b1, b2, arc_pt } => {
                let v = line_intersection(*a1, *a2, *b1, *b2).ok_or("The two lines are parallel: they have no angle between them.")?;
                let ac = angle_of(sub(*arc_pt, v));
                let da = unit(sub(*a2, *a1)).ok_or("The first line has no length.")?;
                let db = unit(sub(*b2, *b1)).ok_or("The second line has no length.")?;
                // Choose the ray of each line so the sector between them (under 180 degrees) contains the arc point.
                let mut chosen = None;
                for sa in [1.0, -1.0] {
                    for sb in [1.0, -1.0] {
                        let (ra, rb) = (angle_of(mul(da, sa)), angle_of(mul(db, sb)));
                        let sweep = (rb - ra).rem_euclid(360.0);
                        let (start, sw) = if sweep <= 180.0 { (ra, sweep) } else { (rb, 360.0 - sweep) };
                        if (ac - start).rem_euclid(360.0) <= sw + 1e-9 {
                            chosen = Some((ra, rb, start, sw));
                        }
                    }
                }
                let (ra, rb, start, sw) = chosen.ok_or("The arc point does not lie in any angle between the lines.")?;
                let r = dist(v, *arc_pt);
                // The arm points: the end of each line on the chosen ray, or a point on the ray.
                let arm = |ang: f64, p: Pt, q: Pt| {
                    let d = dir(ang);
                    let far = if dot(sub(p, v), d) > dot(sub(q, v), d) { p } else { q };
                    if dot(sub(far, v), d) > 0.0 { far } else { add(v, mul(d, r)) }
                };
                let (pa, pb) = (arm(ra, *a1, *a2), arm(rb, *b1, *b2));
                let (s_pt, e_pt) = if (start - ra).abs() < 1e-9 { (pa, pb) } else { (pb, pa) };
                let t = DimStyle::with_override(&st.format_angle(sw), self.override_for(0));
                angular_dim(&mut g, st, v, start, sw, r, s_pt, e_pt, &t);
                (vec![sw], vec![t])
            }
            DimKind::Radius { centre, radius, at } => {
                if radius.is_nan() || *radius <= 0.0 {
                    return Err("The radius must be positive.".into());
                }
                let d = unit(sub(*at, *centre)).unwrap_or(pt(1.0, 0.0));
                let t = DimStyle::with_override(&format!("{}{}", st.radius_prefix, st.format_length(*radius)), self.override_for(0));
                radial_dim(&mut g, st, *centre, *radius, d, &t, false);
                (vec![*radius], vec![t])
            }
            DimKind::Diameter { centre, radius, at } => {
                if radius.is_nan() || *radius <= 0.0 {
                    return Err("The radius must be positive.".into());
                }
                let d = unit(sub(*at, *centre)).unwrap_or(pt(1.0, 0.0));
                let t = DimStyle::with_override(&format!("{}{}", st.diameter_prefix, st.format_length(2.0 * radius)), self.override_for(0));
                radial_dim(&mut g, st, *centre, *radius, d, &t, true);
                (vec![2.0 * radius], vec![t])
            }
            DimKind::ArcLength { centre, radius, start_deg, end_deg, offset } => {
                if radius.is_nan() || *radius <= 0.0 {
                    return Err("The radius must be positive.".into());
                }
                let sweep = (end_deg - start_deg).rem_euclid(360.0);
                if sweep < 1e-9 {
                    return Err("The arc has no length.".into());
                }
                let len = radius * sweep.to_radians();
                let t = DimStyle::with_override(&format!("{}{}", st.arc_prefix, st.format_length(len)), self.override_for(0));
                arc_length_dim(&mut g, st, *centre, *radius, *start_deg, sweep, *offset, &t);
                (vec![len], vec![t])
            }
            DimKind::StationOffset { point, text_pt, .. } => {
                let (l1, l2, ms) = match (self.cache.station_m, self.cache.offset_m) {
                    (Some(s), Some(o)) => {
                        let side = if o >= 0.0 { &st.right_letter } else { &st.left_letter };
                        let sta = crate::units::DisplayUnits { station_decimals: st.station_decimals, ..Default::default() }.station(s);
                        (
                            DimStyle::with_override(&format!("{}{}", st.station_prefix, sta), self.override_for(0)),
                            DimStyle::with_override(&format!("{}{} {}", st.offset_prefix, crate::units::fixed(o.abs(), st.station_decimals), side), self.override_for(1)),
                            vec![s, o],
                        )
                    }
                    _ => {
                        let why = self.cache.problem.clone().unwrap_or_else(|| "not yet computed".into());
                        g.notes.push(format!("No station: {why}."));
                        (format!("{}?", st.station_prefix), format!("{}?", st.offset_prefix), vec![])
                    }
                };
                callout(&mut g, st, *point, *text_pt, &[l1.clone(), l2.clone()]);
                (ms, vec![l1, l2])
            }
            DimKind::Level { point, value_m, text_pt, .. } => {
                let t = DimStyle::with_override(&format!("{}{}", st.level_prefix, crate::units::fixed(*value_m, st.level_decimals)), self.override_for(0));
                callout(&mut g, st, *point, *text_pt, std::slice::from_ref(&t));
                (vec![*value_m], vec![t])
            }
        };
        Ok(DimResult { geometry: g, measured, texts })
    }
}

fn orient_dir(o: LinearOrient, p1: Pt, p2: Pt) -> Result<Pt, String> {
    Ok(match o {
        LinearOrient::Horizontal => pt(1.0, 0.0),
        LinearOrient::Vertical => pt(0.0, 1.0),
        LinearOrient::Rotated(deg) => dir(deg),
        LinearOrient::Aligned => unit(sub(p2, p1)).ok_or("The two points coincide: an aligned dimension has no direction.")?,
    })
}

/// The intersection of two infinite lines, or `None` when they are parallel.
pub fn line_intersection(a1: Pt, a2: Pt, b1: Pt, b2: Pt) -> Option<Pt> {
    let (r, s) = (sub(a2, a1), sub(b2, b1));
    let den = cross(r, s);
    if den.abs() < 1e-12 * len(r).max(1.0) * len(s).max(1.0) {
        return None;
    }
    let t = cross(sub(b1, a1), s) / den;
    Some(add(a1, mul(r, t)))
}

/// Start angle and sweep (degrees, anticlockwise) of the angle at `v` between the rays through
/// `p1` and `p2` that contains the direction to `arc_pt`.
pub fn angular_sector(v: Pt, p1: Pt, p2: Pt, arc_pt: Pt) -> Result<(f64, f64), String> {
    if dist(v, p1) < 1e-9 || dist(v, p2) < 1e-9 {
        return Err("An arm of the angle has no length.".into());
    }
    if dist(v, arc_pt) < 1e-9 {
        return Err("The arc point is at the vertex.".into());
    }
    let (a1, a2, ac) = (angle_of(sub(p1, v)), angle_of(sub(p2, v)), angle_of(sub(arc_pt, v)));
    let sweep12 = (a2 - a1).rem_euclid(360.0);
    if sweep12 < 1e-9 || (360.0 - sweep12) < 1e-9 {
        return Err("The two arms point the same way: there is no angle.".into());
    }
    if (ac - a1).rem_euclid(360.0) <= sweep12 {
        Ok((a1, sweep12))
    } else {
        Ok((a2, 360.0 - sweep12))
    }
}

fn label_w(st: &DimStyle, text: &str) -> f64 {
    text_width(text, st.m(st.text_height_mm), st.bold) * st.width_factor
}

/// A label whose cap box is centred on `centre`, reading along `rotation_deg`.
fn centred_label(st: &DimStyle, text: &str, centre: Pt, rotation_deg: f64) -> Label {
    let h = st.m(st.text_height_mm);
    let up = perp(dir(rotation_deg));
    Label {
        text: text.to_string(),
        at: sub(centre, mul(up, h / 2.0)),
        height: h,
        rotation_deg,
        align: Align::Centre,
        width_factor: st.width_factor,
        bold: st.bold,
        style: st.text_style.clone(),
    }
}

fn terminate(g: &mut AnnGeometry, st: &DimStyle, tip: Pt, pointing: Pt) {
    let a = st.m(st.arrow_size_mm);
    match st.terminator {
        Terminator::Arrow => g.fill(FillKind::Arrow, arrow_head(tip, pointing, a)),
        Terminator::Tick => {
            let d = rot(pointing, 45.0);
            g.line(sub(tip, mul(d, a / 2.0)), add(tip, mul(d, a / 2.0)));
        }
        Terminator::Dot => g.fill(FillKind::Dot, disc(tip, a / 3.0)),
        Terminator::None => {}
    }
}

fn arrow_space(st: &DimStyle) -> f64 {
    if st.terminator == Terminator::None {
        0.0
    } else {
        st.m(st.arrow_size_mm)
    }
}

/// A dimension line from `a` to `b` with terminators and text, placed by the rules in the module
/// documentation. Returns whether the text went outside.
fn dim_line(g: &mut AnnGeometry, st: &DimStyle, a: Pt, b: Pt, text: &str) -> bool {
    let Some(u) = unit(sub(b, a)) else { return false };
    let (l, h, gap, asz) = (dist(a, b), st.m(st.text_height_mm), st.m(st.text_gap_mm), arrow_space(st));
    let w = label_w(st, text);
    let arrows_inside = l >= 2.0 * asz - 1e-9;
    let centred = st.text_placement == TextPlacement::Centred;
    let text_inside = w + 2.0 * gap + if centred { 2.0 * asz } else { 0.0 } <= l + 1e-9;
    let rotation = readable(angle_of(u));
    let up = perp(dir(rotation));

    if arrows_inside {
        terminate(g, st, a, mul(u, -1.0));
        terminate(g, st, b, u);
    } else {
        terminate(g, st, a, u);
        terminate(g, st, b, mul(u, -1.0));
        g.line(a, sub(a, mul(u, 2.0 * asz)));
        g.line(b, add(b, mul(u, 2.0 * asz)));
        g.notes.push("The dimension is too short for its arrows: they are drawn outside.".into());
    }
    if text_inside {
        let m = mid(a, b);
        if centred {
            let half = w / 2.0 + gap;
            g.line(a, sub(m, mul(u, half)));
            g.line(add(m, mul(u, half)), b);
            g.labels.push(centred_label(st, text, m, rotation));
        } else {
            g.line(a, b);
            g.labels.push(centred_label(st, text, add(m, mul(up, gap + h / 2.0)), rotation));
        }
        false
    } else {
        g.line(a, b);
        let start = if arrows_inside { 0.0 } else { 2.0 * asz };
        let tail = start + gap;
        let c = add(b, mul(u, tail + w / 2.0));
        g.line(b, add(b, mul(u, tail + w + gap)));
        g.labels.push(centred_label(st, text, add(c, mul(up, gap + h / 2.0)), rotation));
        g.notes.push("The text does not fit between the extension lines: it is placed outside, past the second one.".into());
        true
    }
}

/// A linear dimension between `p1` and `p2` measured along `u`, with the dimension line at
/// normal coordinate `c` (that is, `n . x == c` with `n = perp(u)`).
#[allow(clippy::too_many_arguments)]
fn linear_dim(g: &mut AnnGeometry, st: &DimStyle, p1: Pt, p2: Pt, u: Pt, c: f64, text: &str, ext1: bool, ext2: bool) {
    let n = perp(u);
    let a = add(p1, mul(n, c - dot(p1, n)));
    let b = add(p2, mul(n, c - dot(p2, n)));
    let (off, extend) = (st.m(st.ext_offset_mm), st.m(st.ext_extend_mm));
    for (p, d, want) in [(p1, a, ext1), (p2, b, ext2)] {
        if !want {
            continue;
        }
        let e = unit(sub(d, p)).unwrap_or(n);
        let (s, t) = (add(p, mul(e, off)), add(d, mul(e, extend)));
        if dot(sub(t, s), e) > 1e-9 {
            g.line(s, t);
        }
    }
    // Run the line from the first point toward the second along +u so "second end" is consistent.
    let (a, b) = if dot(sub(b, a), u) >= 0.0 { (a, b) } else { (b, a) };
    dim_line(g, st, a, b, text);
}

#[allow(clippy::too_many_arguments)]
fn angular_dim(g: &mut AnnGeometry, st: &DimStyle, v: Pt, a0: f64, sweep: f64, r: f64, p_start: Pt, p_end: Pt, text: &str) {
    let (h, gap, asz) = (st.m(st.text_height_mm), st.m(st.text_gap_mm), arrow_space(st));
    let (off, extend) = (st.m(st.ext_offset_mm), st.m(st.ext_extend_mm));
    let a1 = a0 + sweep;
    // Extension lines along the rays, where the arc lies beyond the arm point.
    for (p, ang) in [(p_start, a0), (p_end, a1)] {
        let d = dir(ang);
        if dist(v, p) < r {
            g.line(add(p, mul(d, off)), add(v, mul(d, r + extend)));
        }
    }
    g.arc(v, r, a0, a1);
    let arc_len = r * sweep.to_radians();
    let arrows_inside = arc_len >= 2.0 * asz - 1e-9;
    let (t0, t1) = (mul(perp(dir(a0)), -1.0), perp(dir(a1)));
    let (e0, e1) = (add(v, mul(dir(a0), r)), add(v, mul(dir(a1), r)));
    if arrows_inside {
        terminate(g, st, e0, t0);
        terminate(g, st, e1, t1);
    } else {
        terminate(g, st, e0, mul(t0, -1.0));
        terminate(g, st, e1, mul(t1, -1.0));
        g.notes.push("The angle is too small for its arrows: they are drawn outside.".into());
    }
    let w = label_w(st, text);
    let mid_ang = a0 + sweep / 2.0;
    if w + 2.0 * gap <= arc_len + 1e-9 {
        let rotation = readable(mid_ang + 90.0);
        let c = add(v, mul(dir(mid_ang), r + gap + h / 2.0));
        g.labels.push(centred_label(st, text, c, rotation));
    } else {
        let rotation = readable(mid_ang);
        let c = add(v, mul(dir(mid_ang), r + 2.0 * asz + gap + w / 2.0));
        g.labels.push(centred_label(st, text, c, rotation));
        g.notes.push("The text does not fit along the arc: it is placed outside, along the bisector.".into());
    }
}

#[allow(clippy::too_many_arguments)]
fn arc_length_dim(g: &mut AnnGeometry, st: &DimStyle, c: Pt, r: f64, a0: f64, sweep: f64, offset: f64, text: &str) {
    // The dimension arc, its terminators, text and radial extension lines are those of an angular
    // dimension on the larger radius, with the arc's own end points as the arm points.
    let pa = add(c, mul(dir(a0), r));
    let pb = add(c, mul(dir(a0 + sweep), r));
    angular_dim(g, st, c, a0, sweep, r + offset, pa, pb, text);
}

fn radial_dim(g: &mut AnnGeometry, st: &DimStyle, centre: Pt, r: f64, d: Pt, text: &str, diameter: bool) {
    let (h, gap, asz) = (st.m(st.text_height_mm), st.m(st.text_gap_mm), arrow_space(st));
    let cm = st.m(st.centre_mark_mm);
    if cm > 0.0 {
        g.line(sub(centre, pt(cm / 2.0, 0.0)), add(centre, pt(cm / 2.0, 0.0)));
        g.line(sub(centre, pt(0.0, cm / 2.0)), add(centre, pt(0.0, cm / 2.0)));
    }
    let w = label_w(st, text);
    let rotation = readable(angle_of(d));
    let up = perp(dir(rotation));
    let edge = add(centre, mul(d, r));
    let start = if diameter { sub(centre, mul(d, r)) } else { centre };
    g.line(start, edge);
    terminate(g, st, edge, d);
    if diameter {
        terminate(g, st, start, mul(d, -1.0));
    }
    let span = if diameter { 2.0 * r } else { r };
    let fits = w + 2.0 * gap + asz <= span + 1e-9;
    if fits {
        let m = if diameter { centre } else { mid(centre, edge) };
        g.labels.push(centred_label(st, text, add(m, mul(up, gap + h / 2.0)), rotation));
    } else {
        let tail = asz.max(gap) + gap;
        let c = add(edge, mul(d, tail + w / 2.0));
        g.line(edge, add(edge, mul(d, tail + w + gap)));
        g.labels.push(centred_label(st, text, add(c, mul(up, gap + h / 2.0)), rotation));
        g.notes.push("The text does not fit on the dimension line: it is placed outside the circle.".into());
    }
}

/// A marker at `point`, a leader to `text_pt`, a landing line and the text lines on it.
fn callout(g: &mut AnnGeometry, st: &DimStyle, point: Pt, text_pt: Pt, lines: &[String]) {
    let (h, gap, asz) = (st.m(st.text_height_mm), st.m(st.text_gap_mm), arrow_space(st));
    // A cross at the point.
    let k = (asz.max(h) / 2.0).max(1e-9);
    g.line(sub(point, pt(k, 0.0)), add(point, pt(k, 0.0)));
    g.line(sub(point, pt(0.0, k)), add(point, pt(0.0, k)));
    if dist(point, text_pt) > 1e-9 {
        g.line(point, text_pt);
    }
    let side = if text_pt.x >= point.x { 1.0 } else { -1.0 };
    let width = lines.iter().map(|l| label_w(st, l)).fold(0.0, f64::max);
    g.line(text_pt, add(text_pt, pt(side * (width + 2.0 * gap), 0.0)));
    let align = if side > 0.0 { Align::Left } else { Align::Right };
    let x = text_pt.x + side * gap;
    let ys: Vec<f64> = match lines.len() {
        1 => vec![text_pt.y + gap],
        _ => vec![text_pt.y + gap, text_pt.y - gap - h],
    };
    for (l, y) in lines.iter().zip(ys) {
        g.labels.push(Label {
            text: l.clone(),
            at: pt(x, y),
            height: h,
            rotation_deg: 0.0,
            align,
            width_factor: st.width_factor,
            bold: st.bold,
            style: st.text_style.clone(),
        });
    }
}

/// Station and offset of a point (site millimetres) on an alignment, as a cache for a
/// [`DimKind::StationOffset`] dimension.
pub fn station_offset_cache(alignment: &Alignment, site_point_mm: Pt) -> DimCache {
    let (x, y) = (site_point_mm.x / 1000.0, site_point_mm.y / 1000.0);
    match alignment.station_offset(x, y) {
        None => DimCache { problem: Some("the point has no perpendicular foot on the alignment".into()), ..Default::default() },
        Some((internal, offset)) => match alignment.displayed_station(internal) {
            Some(d) => DimCache { station_m: Some(d.station_m), offset_m: Some(offset), region: Some(d.region), problem: None },
            None => DimCache { problem: Some("the station is off the alignment".into()), ..Default::default() },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> DimStyle {
        // Hand-picked round sizes (model mm, scale 1): text 10, gap 2, arrow 10, ext offset 3, extend 4.
        DimStyle {
            text_height_mm: 10.0,
            text_gap_mm: 2.0,
            arrow_size_mm: 10.0,
            ext_offset_mm: 3.0,
            ext_extend_mm: 4.0,
            baseline_spacing_mm: 50.0,
            centre_mark_mm: 10.0,
            length_unit: crate::units::LengthUnit::Millimetre,
            length_decimals: 1,
            ..DimStyle::default()
        }
    }

    fn close(a: Pt, b: Pt) -> bool {
        (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
    }

    fn has_line(g: &AnnGeometry, a: Pt, b: Pt) -> bool {
        g.segs.iter().any(|s| matches!(s, Seg::Line { a: x, b: y } if (close(*x, a) && close(*y, b)) || (close(*x, b) && close(*y, a))))
    }

    #[test]
    fn horizontal_linear_value_and_key_coordinates() {
        let d = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(1000.0, 300.0), line_pt: pt(500.0, 400.0), orient: LinearOrient::Horizontal }, &st());
        let r = d.compute().unwrap();
        assert_eq!(r.measured, vec![1000.0]); // |dx|
        assert_eq!(r.texts, vec!["1000.0"]);
        // Dimension line at y = 400 from x = 0 to 1000; ext line 1 from (0,3) to (0,404);
        // ext line 2 from (1000,303) to (1000,404).
        assert!(has_line(&r.geometry, pt(0.0, 3.0), pt(0.0, 404.0)));
        assert!(has_line(&r.geometry, pt(1000.0, 303.0), pt(1000.0, 404.0)));
        assert!(has_line(&r.geometry, pt(0.0, 400.0), pt(1000.0, 400.0)));
        // Text above the line: baseline at 400 + gap (2) centred on x = 500.
        let l = &r.geometry.labels[0];
        assert!((l.at.x - 500.0).abs() < 1e-9 && (l.at.y - 402.0).abs() < 1e-9 && l.rotation_deg == 0.0);
        assert_eq!(l.align, Align::Centre);
        // Two filled arrows.
        assert_eq!(r.geometry.fills.iter().filter(|f| f.kind == FillKind::Arrow).count(), 2);
        assert!(r.geometry.notes.is_empty());
    }

    #[test]
    fn vertical_and_rotated_values() {
        let v = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(1000.0, 300.0), line_pt: pt(1200.0, 100.0), orient: LinearOrient::Vertical }, &st());
        let r = v.compute().unwrap();
        assert_eq!(r.measured, vec![300.0]);
        // Dimension line at x = 1200; the text reads upward (rotation 90).
        assert_eq!(r.geometry.labels[0].rotation_deg, 90.0);
        // Rotated 30 degrees: measured length of (1000, 300) along (cos30, sin30).
        let rot30 = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(1000.0, 300.0), line_pt: pt(0.0, 0.0), orient: LinearOrient::Rotated(30.0) }, &st());
        let m = rot30.compute().unwrap().measured[0];
        assert!((m - (1000.0 * 30f64.to_radians().cos() + 300.0 * 30f64.to_radians().sin())).abs() < 1e-9);
    }

    #[test]
    fn aligned_is_the_true_distance_and_the_line_sits_on_the_picked_side() {
        // 3-4-5 triangle scaled by 100: distance 500.
        let d = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(300.0, 400.0), line_pt: pt(-100.0, 200.0), orient: LinearOrient::Aligned }, &st());
        let r = d.compute().unwrap();
        assert!((r.measured[0] - 500.0).abs() < 1e-9);
        // u = (0.6, 0.8); n = (-0.8, 0.6); c = n.line_pt = 80 + 120 = 200: the line is 200 to the left of p1.
        // a = p1 + n*200 = (-160, 120); b = (300,400) + n*(200 - 0) = (140, 520).
        assert!(has_line(&r.geometry, pt(-160.0, 120.0), pt(140.0, 520.0)));
        // Rotation 53.13 degrees, readable as is.
        assert!((r.geometry.labels[0].rotation_deg - 53.13010235415598).abs() < 1e-9);
    }

    #[test]
    fn text_moves_outside_when_it_does_not_fit_and_arrows_flip_when_too_short() {
        // Length 40: arrows (2 x 10 = 20) fit, text "40.0" is about 4 chars * 10/0.718*0.556 ~ 31 > 40 - 4? width = 4 glyph advances.
        let mut s = st();
        s.text_height_mm = 20.0; // text wider than the line
        let d = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(40.0, 0.0), line_pt: pt(0.0, 100.0), orient: LinearOrient::Horizontal }, &s);
        let r = d.compute().unwrap();
        let l = &r.geometry.labels[0];
        assert!(l.at.x > 40.0, "text must be past the second end, at {}", l.at.x);
        assert!(r.geometry.notes.iter().any(|n| n.contains("outside")));
        // Length 15: arrows do not fit (needs 20).
        let d = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(15.0, 0.0), line_pt: pt(0.0, 100.0), orient: LinearOrient::Horizontal }, &st());
        let r = d.compute().unwrap();
        assert!(r.geometry.notes.iter().any(|n| n.contains("arrows")));
        // Outside arrows point inward: the first arrow's tip is at x = 0 and its body at x < 0.
        let arrow = r.geometry.fills.iter().find(|f| f.kind == FillKind::Arrow).unwrap();
        assert!(close(arrow.pts[0], pt(0.0, 100.0)) && arrow.pts[1].x < 0.0);
    }

    #[test]
    fn centred_text_breaks_the_line() {
        let mut s = st();
        s.text_placement = TextPlacement::Centred;
        let d = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(1000.0, 0.0), line_pt: pt(0.0, 100.0), orient: LinearOrient::Horizontal }, &s);
        let r = d.compute().unwrap();
        let w = label_w(&s, "1000.0");
        let half = w / 2.0 + 2.0;
        assert!(has_line(&r.geometry, pt(0.0, 100.0), pt(500.0 - half, 100.0)));
        assert!(has_line(&r.geometry, pt(500.0 + half, 100.0), pt(1000.0, 100.0)));
        // Text centred on the line: its box centre is at y = 100, so the baseline is half a height below.
        assert!((r.geometry.labels[0].at.y - 95.0).abs() < 1e-9);
    }

    #[test]
    fn continued_and_baseline_chains() {
        let pts = vec![pt(0.0, 0.0), pt(1000.0, 0.0), pt(2500.0, 0.0), pt(3000.0, 0.0)];
        let c = DimData::new(DimKind::Chain { mode: ChainMode::Continued, pts: pts.clone(), line_pt: pt(0.0, 200.0), orient: LinearOrient::Horizontal }, &st());
        let r = c.compute().unwrap();
        assert_eq!(r.measured, vec![1000.0, 1500.0, 500.0]);
        assert_eq!(r.texts, vec!["1000.0", "1500.0", "500.0"]);
        // One shared dimension line level: all dimension-line segments at y = 200.
        assert!(has_line(&r.geometry, pt(1000.0, 200.0), pt(2500.0, 200.0)) || r.geometry.segs.iter().any(|s| matches!(s, Seg::Line{a,b} if (a.y-200.0).abs()<1e-9 && (b.y-200.0).abs()<1e-9 && a.x>=1000.0 && b.x<=2500.0 + 1e-9)));
        let b = DimData::new(DimKind::Chain { mode: ChainMode::Baseline, pts, line_pt: pt(0.0, 200.0), orient: LinearOrient::Horizontal }, &st());
        let r = b.compute().unwrap();
        assert_eq!(r.measured, vec![1000.0, 2500.0, 3000.0]);
        // Lines stacked 50 apart, away from the measured points: y = 200, 250, 300.
        for (y, x1) in [(200.0, 1000.0), (250.0, 2500.0), (300.0, 3000.0)] {
            assert!(r.geometry.segs.iter().any(|s| matches!(s, Seg::Line { a, b } if (a.y - y).abs() < 1e-9 && (b.y - y).abs() < 1e-9 && (a.x.max(b.x) - x1).abs() < 1e-6 || false)), "line at y={y}");
        }
        assert!(DimData::new(DimKind::Chain { mode: ChainMode::Baseline, pts: vec![pt(0.0, 0.0), pt(1.0, 0.0)], line_pt: pt(0.0, 1.0), orient: LinearOrient::Horizontal }, &st()).compute().is_err());
    }

    #[test]
    fn angular_three_point_picks_the_sector_of_the_arc_point() {
        let s = st();
        // Right angle at the origin between +X and +Y arms.
        let k = |arc: Pt| DimData::new(DimKind::Angular { vertex: pt(0.0, 0.0), p1: pt(100.0, 0.0), p2: pt(0.0, 100.0), arc_pt: arc }, &s).compute().unwrap();
        let r = k(pt(300.0, 300.0));
        assert!((r.measured[0] - 90.0).abs() < 1e-9);
        assert_eq!(r.texts, vec!["90.00\u{b0}"]);
        // Arc radius |arc_pt| = 424.26; arm points are at 100 so extension lines are drawn.
        let arc = r.geometry.segs.iter().find_map(|s| if let Seg::Arc { radius, start_deg, end_deg, .. } = s { Some((*radius, *start_deg, *end_deg)) } else { None }).unwrap();
        assert!((arc.0 - 300.0 * 2f64.sqrt()).abs() < 1e-9 && arc.1.abs() < 1e-9 && (arc.2 - 90.0).abs() < 1e-9);
        // On the other side the reflex angle 270 is measured.
        let r = k(pt(-300.0, -300.0));
        assert!((r.measured[0] - 270.0).abs() < 1e-9);
        // A 30 degree angle.
        let d = DimData::new(DimKind::Angular { vertex: pt(0.0, 0.0), p1: pt(100.0, 0.0), p2: rot(pt(100.0, 0.0), 30.0), arc_pt: pt(500.0, 100.0) }, &s).compute().unwrap();
        assert!((d.measured[0] - 30.0).abs() < 1e-9);
        assert!(DimData::new(DimKind::Angular { vertex: pt(0.0, 0.0), p1: pt(100.0, 0.0), p2: pt(200.0, 0.0), arc_pt: pt(5.0, 5.0) }, &s).compute().is_err());
    }

    #[test]
    fn angular_two_lines_measures_the_chosen_sector() {
        let s = st();
        // Line A along the X axis, line B along y = x; they meet at the origin.
        let mk = |arc: Pt| {
            DimData::new(DimKind::AngularLines { a1: pt(-500.0, 0.0), a2: pt(500.0, 0.0), b1: pt(-300.0, -300.0), b2: pt(300.0, 300.0), arc_pt: arc }, &s).compute().unwrap()
        };
        assert!((mk(pt(400.0, 100.0)).measured[0] - 45.0).abs() < 1e-9); // between +X and the diagonal
        assert!((mk(pt(-100.0, 400.0)).measured[0] - 135.0).abs() < 1e-9); // between the diagonal and -X
        assert!((mk(pt(-400.0, -100.0)).measured[0] - 45.0).abs() < 1e-9);
        let par = DimData::new(DimKind::AngularLines { a1: pt(0.0, 0.0), a2: pt(1.0, 0.0), b1: pt(0.0, 1.0), b2: pt(1.0, 1.0), arc_pt: pt(0.5, 0.5) }, &s);
        assert!(par.compute().is_err());
    }

    #[test]
    fn radius_and_diameter() {
        let s = st();
        let r = DimData::new(DimKind::Radius { centre: pt(0.0, 0.0), radius: 250.0, at: pt(100.0, 100.0) }, &s).compute().unwrap();
        assert_eq!(r.measured, vec![250.0]);
        assert_eq!(r.texts, vec!["R250.0"]);
        // The arrow tip is on the circle at 45 degrees.
        let tip = r.geometry.fills.iter().find(|f| f.kind == FillKind::Arrow).unwrap().pts[0];
        assert!(close(tip, pt(250.0 / 2f64.sqrt(), 250.0 / 2f64.sqrt())));
        // Centre mark: two lines through the centre (size 10).
        assert!(has_line(&r.geometry, pt(-5.0, 0.0), pt(5.0, 0.0)));
        let d = DimData::new(DimKind::Diameter { centre: pt(0.0, 0.0), radius: 250.0, at: pt(1.0, 0.0) }, &s).compute().unwrap();
        assert_eq!(d.measured, vec![500.0]);
        assert_eq!(d.texts, vec!["\u{d8}500.0"]);
        assert!(has_line(&d.geometry, pt(-250.0, 0.0), pt(250.0, 0.0)));
        assert_eq!(d.geometry.fills.iter().filter(|f| f.kind == FillKind::Arrow).count(), 2);
        // A tiny circle: the text is outside.
        let t = DimData::new(DimKind::Radius { centre: pt(0.0, 0.0), radius: 20.0, at: pt(1.0, 0.0) }, &s).compute().unwrap();
        assert!(t.geometry.labels[0].at.x > 20.0);
        assert!(DimData::new(DimKind::Radius { centre: pt(0.0, 0.0), radius: 0.0, at: pt(1.0, 0.0) }, &s).compute().is_err());
    }

    #[test]
    fn arc_length_is_radius_times_sweep() {
        let s = st();
        // Quarter circle of radius 1000: 1000 * pi / 2 = 1570.796...
        let d = DimData::new(DimKind::ArcLength { centre: pt(0.0, 0.0), radius: 1000.0, start_deg: 0.0, end_deg: 90.0, offset: 100.0 }, &s).compute().unwrap();
        assert!((d.measured[0] - 1570.7963267948967).abs() < 1e-9);
        assert_eq!(d.texts, vec!["arc 1570.8"]);
        // The dimension arc is 100 outside.
        assert!(d.geometry.segs.iter().any(|s| matches!(s, Seg::Arc { radius, .. } if (radius - 1100.0).abs() < 1e-9)));
        // End before start wraps: 350 to 10 is a 20 degree sweep.
        let w = DimData::new(DimKind::ArcLength { centre: pt(0.0, 0.0), radius: 100.0, start_deg: 350.0, end_deg: 10.0, offset: 10.0 }, &s).compute().unwrap();
        assert!((w.measured[0] - 100.0 * 20f64.to_radians()).abs() < 1e-9);
    }

    #[test]
    fn level_and_station_offset_text() {
        let s = st();
        let l = DimData::new(DimKind::Level { point: pt(0.0, 0.0), value_m: 101.2504, source: LevelSource::Entered, text_pt: pt(50.0, 50.0) }, &s).compute().unwrap();
        assert_eq!(l.texts, vec!["101.250"]);
        assert_eq!(l.measured, vec![101.2504]);
        // Station and offset on a straight alignment along +X from station 0: point (1250.5 m, -4.5 m)
        // is 4.5 m to the right (offset positive right looking up-station, i.e. towards -Y).
        let a = afrobim_alignment::AlignmentBuilder::new("A", 0.0, 0.0, 0.0, 0.0).line(2000.0).build().unwrap();
        let cache = station_offset_cache(&a, pt(1_250_500.0, -4500.0));
        assert!((cache.station_m.unwrap() - 1250.5).abs() < 1e-9 && (cache.offset_m.unwrap() - 4.5).abs() < 1e-9);
        let mut d = DimData::new(DimKind::StationOffset { point: pt(1_250_500.0, -4500.0), alignment: "A".into(), text_pt: pt(1_251_000.0, -2000.0) }, &s);
        d.cache = cache;
        let r = d.compute().unwrap();
        assert_eq!(r.texts, vec!["STA 1+250.500", "OFF 4.500 R"]);
        // Left of the alignment reads L; off the end has no station.
        let left = station_offset_cache(&a, pt(500_000.0, 3000.0));
        assert!((left.offset_m.unwrap() + 3.0).abs() < 1e-9);
        assert!(station_offset_cache(&a, pt(-5000.0, 0.0)).station_m.is_none());
        let none = DimData::new(DimKind::StationOffset { point: pt(0.0, 0.0), alignment: "A".into(), text_pt: pt(1.0, 1.0) }, &s).compute().unwrap();
        assert!(none.geometry.notes.iter().any(|n| n.contains("No station")));
    }

    #[test]
    fn station_dimension_reads_through_station_equations() {
        use afrobim_alignment::{StationEquation, StationEquations};
        let mut a = afrobim_alignment::AlignmentBuilder::new("A", 0.0, 0.0, 0.0, 0.0).line(1000.0).build().unwrap();
        // At internal 500 the displayed station becomes 1000.
        a.station_equations = StationEquations::new(vec![StationEquation::new(500.0, 1000.0)]).unwrap();
        let c = station_offset_cache(&a, pt(600_000.0, 0.0));
        assert!((c.station_m.unwrap() - 1100.0).abs() < 1e-9);
        assert_eq!(c.region, Some(1));
        let before = station_offset_cache(&a, pt(100_000.0, 0.0));
        assert_eq!((before.station_m, before.region), (Some(100.0), Some(0)));
    }

    #[test]
    fn override_keeps_the_measured_value() {
        let mut d = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(500.0, 0.0), line_pt: pt(0.0, 50.0), orient: LinearOrient::Horizontal }, &st());
        d.overrides = vec![Some("<> TYP".into())];
        let r = d.compute().unwrap();
        assert_eq!(r.texts, vec!["500.0 TYP"]);
        assert_eq!(r.measured, vec![500.0]);
    }

    #[test]
    fn moving_a_point_updates_the_value() {
        let mut d = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(500.0, 0.0), line_pt: pt(0.0, 50.0), orient: LinearOrient::Horizontal }, &st());
        assert_eq!(d.compute().unwrap().measured, vec![500.0]);
        *d.points_mut()[1] = pt(750.0, 0.0);
        assert_eq!(d.compute().unwrap().measured, vec![750.0]);
        d.translate(pt(10.0, 10.0));
        assert_eq!(d.points()[0], pt(10.0, 10.0));
        assert_eq!(d.compute().unwrap().measured, vec![750.0]); // a translation does not change it
    }

    #[test]
    fn ticks_dots_and_no_terminator() {
        let mut s = st();
        s.terminator = Terminator::Tick;
        let d = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(1000.0, 0.0), line_pt: pt(0.0, 100.0), orient: LinearOrient::Horizontal }, &s).compute().unwrap();
        assert!(d.geometry.fills.is_empty());
        // The tick at (0,100) crosses at 45 degrees, half-length 5.
        let h = 5.0 / 2f64.sqrt();
        assert!(has_line(&d.geometry, pt(-h, 100.0 - h), pt(h, 100.0 + h)) || has_line(&d.geometry, pt(h, 100.0 + h), pt(-h, 100.0 - h)) || d.geometry.segs.iter().any(|s| matches!(s, Seg::Line { a, b } if (a.x + h).abs() < 1e-6 || (b.x + h).abs() < 1e-6)));
        s.terminator = Terminator::Dot;
        let d = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(1000.0, 0.0), line_pt: pt(0.0, 100.0), orient: LinearOrient::Horizontal }, &s).compute().unwrap();
        assert_eq!(d.geometry.fills.iter().filter(|f| f.kind == FillKind::Dot).count(), 2);
        s.terminator = Terminator::None;
        let d = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(1000.0, 0.0), line_pt: pt(0.0, 100.0), orient: LinearOrient::Horizontal }, &s).compute().unwrap();
        assert!(d.geometry.fills.is_empty());
    }
}
