//! Drawings of a sign's visibility, with their labels returned separately (the draw crate has no
//! text primitive, so a front end or the sheet composer sets the text).
//!
//! Both drawings are in millimetres at full size: one metre is 1000 units, and a drawing point maps
//! back to the world by the origin fields of [`VisibilityDrawing`]. Lines are hairlines (width 0)
//! unless noted; colours are the named colours of `afrobim-draw`:
//!
//! - eye positions: green where the whole face is visible, yellow where part of it is, red where
//!   none is
//! - sight lines at the limit: blue for the last position that sees the whole face, red up to the
//!   obstruction for the first position that does not
//! - the sign face: a black line of 100 mm
//! - the alignment centreline: grey

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use serde::{Deserialize, Serialize};

use crate::analysis::SignAnalysis;
use crate::scene::ObstacleScene;
use crate::sweep::{SampleState, Sweeper, TravelDirection};

/// What a label says, so a front end can style it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelKind {
    Title,
    /// A distance before the sign.
    Distance,
    /// The continuous clear visibility limit.
    Limit,
    Obstruction,
    Sign,
    /// Levels and heights on the profile.
    Level,
}

/// A piece of text belonging to a drawing, anchored at a point in drawing units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingLabel {
    pub at: Pt2,
    pub text: String,
    pub kind: LabelKind,
}

/// Which of the two drawings this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawingKind {
    Plan,
    Profile,
}

/// A drawing with its labels and the mapping back to the world.
///
/// Plan: `X = origin_a_m + u / 1000`, `Y = origin_b_m + v / 1000` (site metres).
/// Profile: `distance along the sight line from the eye = origin_a_m + u / 1000`,
/// `level = origin_b_m + v / (1000 x vertical_exaggeration)`.
#[derive(Debug, Clone, PartialEq)]
pub struct VisibilityDrawing {
    pub kind: DrawingKind,
    pub drawing: Drawing,
    pub labels: Vec<DrawingLabel>,
    pub origin_a_m: f64,
    pub origin_b_m: f64,
    pub vertical_exaggeration: f64,
}

struct Builder {
    prims: Vec<Primitive>,
    labels: Vec<DrawingLabel>,
    min: (f64, f64),
    max: (f64, f64),
}

impl Builder {
    fn new() -> Builder {
        Builder { prims: Vec::new(), labels: Vec::new(), min: (f64::INFINITY, f64::INFINITY), max: (f64::NEG_INFINITY, f64::NEG_INFINITY) }
    }
    fn grow(&mut self, p: (f64, f64)) {
        self.min = (self.min.0.min(p.0), self.min.1.min(p.1));
        self.max = (self.max.0.max(p.0), self.max.1.max(p.1));
    }
    fn line(&mut self, a: (f64, f64), b: (f64, f64), width: f64, colour: Option<Colour>) {
        self.grow(a);
        self.grow(b);
        self.prims.push(Primitive::Line { from: Pt2::new(a.0, a.1), to: Pt2::new(b.0, b.1), width, colour });
    }
    fn polyline(&mut self, pts: &[(f64, f64)], colour: Option<Colour>) {
        for w in pts.windows(2) {
            self.line(w[0], w[1], 0.0, colour);
        }
    }
    fn square(&mut self, c: (f64, f64), half: f64, colour: Colour) {
        let o = vec![Pt2::new(c.0 - half, c.1 - half), Pt2::new(c.0 + half, c.1 - half), Pt2::new(c.0 + half, c.1 + half), Pt2::new(c.0 - half, c.1 + half)];
        o.iter().for_each(|p| self.grow((p.x, p.y)));
        self.prims.push(Primitive::Fill { outline: o, colour });
    }
    fn label(&mut self, at: (f64, f64), text: String, kind: LabelKind) {
        self.grow(at);
        self.labels.push(DrawingLabel { at: Pt2::new(at.0, at.1), text, kind });
    }
}

/// Plan of the visibility envelope of a sign: the eye positions along the approach coloured by
/// what they see, the sight lines at the limit, the obstruction, the entered lateral offsets and
/// the sign face. `None` when the sign has no sweep.
///
/// `scene` is only needed to redraw the sight lines at the limit with the same obstructions as the
/// sweep; without it they are drawn against the road profile and the entered offsets alone.
pub fn plan_envelope(a: &SignAnalysis, scene: Option<&ObstacleScene>) -> Option<VisibilityDrawing> {
    let (sweep, approach, face) = (a.sweep.as_ref()?, a.approach.as_ref()?, a.face?);
    if sweep.samples.is_empty() {
        return None;
    }
    let mut sweeper = Sweeper::new(face, approach, sweep.params).with_lateral(&a.inputs.lateral);
    if let Some(s) = scene {
        sweeper = sweeper.with_scene(s, &a.ignore);
    }
    let all: Vec<[f64; 3]> = sweep.samples.iter().map(|s| s.eye).chain(face.corners()).collect();
    let ox = all.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min) - 20.0;
    let oy = all.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min) - 20.0;
    let mm = |x: f64, y: f64| ((x - ox) * 1000.0, (y - oy) * 1000.0);
    let mut b = Builder::new();

    // The alignment centreline between the ends of the sweep.
    let (s0, s1) = (sweep.sign_station_m, sweep.sign_station_m - sweep.direction.sign() * (sweep.swept_to_m + 10.0));
    let (lo, hi) = (s0.min(s1) - 10.0, s0.max(s1) + 10.0);
    let n = (((hi - lo) / 5.0).ceil() as usize).max(1);
    let centre: Vec<(f64, f64)> = (0..=n)
        .filter_map(|i| {
            let s = lo + (hi - lo) * i as f64 / n as f64;
            approach.alignment.point_at(s).map(|p| mm(p.x, p.y))
        })
        .collect();
    b.polyline(&centre, Some(Colour::Grey));

    // Entered lateral obstructions, drawn at their offset over the swept stations.
    for o in &a.inputs.lateral {
        let off = match o.side {
            crate::sweep::ObstructionSide::Right => o.offset_m,
            crate::sweep::ObstructionSide::Left => -o.offset_m,
        };
        let (from, to) = o.station_range.map_or((lo, hi), |(x, y)| (x.min(y).max(lo), x.max(y).min(hi)));
        let k = (((to - from) / 5.0).ceil() as usize).max(1);
        let pts: Vec<(f64, f64)> = (0..=k)
            .filter_map(|i| approach.alignment.point_at_offset(from + (to - from) * i as f64 / k as f64, off).map(|(x, y)| mm(x, y)))
            .collect();
        b.polyline(&pts, None);
        if let Some(p) = pts.first() {
            b.label(*p, format!("{} (offset {:.1} m {})", if o.label.is_empty() { "lateral obstruction" } else { &o.label }, o.offset_m, match o.side { crate::sweep::ObstructionSide::Right => "right", crate::sweep::ObstructionSide::Left => "left" }), LabelKind::Obstruction);
        }
    }

    // Eye positions.
    let half = (sweep.params.step_m * 0.2).clamp(0.3, 2.0) * 1000.0;
    for s in &sweep.samples {
        let c = mm(s.eye[0], s.eye[1]);
        let colour = match s.state {
            SampleState::Full => Colour::Green,
            SampleState::Partial => Colour::Yellow,
            SampleState::Blocked => Colour::Red,
        };
        b.line((c.0 - half, c.1), (c.0 + half, c.1), 0.0, Some(colour));
        b.line((c.0, c.1 - half), (c.0, c.1 + half), 0.0, Some(colour));
    }
    // Distance ticks every 50 m of sweep, on the nearest eye position.
    for s in &sweep.samples {
        let k = (s.distance_m / 50.0).round();
        if k >= 1.0 && (s.distance_m - k * 50.0).abs() <= sweep.params.step_m / 2.0 && sweep.samples.iter().filter(|o| (o.distance_m - k * 50.0).abs() < (s.distance_m - k * 50.0).abs()).count() == 0 {
            b.label(mm(s.eye[0], s.eye[1]), format!("{:.0} m", k * 50.0), LabelKind::Distance);
        }
    }

    // The sign face.
    let c = face.corners();
    for i in 0..4 {
        let (p, q) = (mm(c[i][0], c[i][1]), mm(c[(i + 1) % 4][0], c[(i + 1) % 4][1]));
        b.line(p, q, 100.0, Some(Colour::Black));
    }
    b.label(mm(face.centre()[0], face.centre()[1]), format!("{} (face normal {:.0} deg)", a.mark, {
        let n = face.normal();
        n[1].atan2(n[0]).to_degrees().rem_euclid(360.0)
    }), LabelKind::Sign);

    // Sight lines at the limit.
    let draw_lines = |b: &mut Builder, d: f64, blocked_colour: Colour, clear_colour: Colour| {
        if let Some(lines) = sweeper.lines_at(d) {
            if let Some(s) = sweeper.sample_at(d) {
                let e = mm(s.eye[0], s.eye[1]);
                for l in lines {
                    let end = mm(l.end[0], l.end[1]);
                    b.line(e, end, 0.0, Some(if l.blocker.is_some() { blocked_colour } else { clear_colour }));
                    if l.blocker.is_some() {
                        b.square(end, 150.0, Colour::Red);
                    }
                }
            }
        }
    };
    let limit = &sweep.full;
    if let Some(at) = &limit.at_limit {
        draw_lines(&mut b, at.distance_m, Colour::Red, Colour::Blue);
    }
    let mut title = format!("{} visibility envelope: whole face visible continuously from {}{:.1} m before the sign", a.mark, if limit.reaches_start { "at least " } else { "" }, limit.distance_m);
    if let Some(beyond) = &limit.beyond {
        draw_lines(&mut b, beyond.distance_m, Colour::Red, Colour::Blue);
        if let Some(bl) = beyond.blockers.first() {
            let p = mm(bl.point[0], bl.point[1]);
            let text = format!("{} '{}'{} hides the face beyond {:.1} m", bl.kind.label(), bl.label, bl.station_m.map_or(String::new(), |s| format!(" at station {s:.1}")), limit.distance_m);
            b.label(p, text, LabelKind::Obstruction);
        }
    }
    if let Some(at) = &limit.at_limit {
        b.label(mm(at.eye[0], at.eye[1]), format!("limit {:.1} m (obliquity {:.1} deg)", limit.distance_m, at.obliquity_deg), LabelKind::Limit);
    }
    if sweep.partial.distance_m > limit.distance_m {
        title.push_str(&format!("; part of it from {:.1} m", sweep.partial.distance_m));
    }
    let top = (b.max.0 + b.min.0) / 2.0;
    let at = (top, b.max.1);
    b.label(at, title, LabelKind::Title);
    // The mapping above puts the lower-left of the data 20 m (margin) inside the origin; the
    // extents run from the origin to the largest coordinate.
    let extents = Extents { min: Pt2::new(b.min.0.min(0.0), b.min.1.min(0.0)), max: Pt2::new(b.max.0, b.max.1) };
    Some(VisibilityDrawing { kind: DrawingKind::Plan, drawing: Drawing { primitives: b.prims, extents }, labels: b.labels, origin_a_m: ox, origin_b_m: oy, vertical_exaggeration: 1.0 })
}

/// Profile-style drawing of one sight line against the ground and the road level: from the eye
/// at the first position beyond the limit (so it shows what blocks), or from the start of the
/// sweep when nothing blocks, to the centre of the face. The horizontal axis is distance along the
/// line in plan from the eye; levels are stretched by `vertical_exaggeration` (1.0 is natural
/// scale). `None` when the sign has no sweep.
///
/// Drawn: the sight line (blue where clear, red up to its obstruction, grey beyond), the ground
/// along the line from the TIN of `scene` (green; omitted without a TIN), the road level of the
/// profile at the stations under the line (black), the driver's eye height at the start and the
/// sign face at the end.
pub fn sight_line_profile(a: &SignAnalysis, scene: Option<&ObstacleScene>, vertical_exaggeration: f64) -> Option<VisibilityDrawing> {
    let k = if vertical_exaggeration.is_finite() && vertical_exaggeration > 0.0 { vertical_exaggeration } else { 1.0 };
    let (sweep, approach, face) = (a.sweep.as_ref()?, a.approach.as_ref()?, a.face?);
    let mut sweeper = Sweeper::new(face, approach, sweep.params).with_lateral(&a.inputs.lateral);
    if let Some(s) = scene {
        sweeper = sweeper.with_scene(s, &a.ignore);
    }
    let eye_sample = sweep.full.beyond.clone().or_else(|| sweep.samples.last().cloned())?;
    let eye = eye_sample.eye;
    let target = face.centre();
    let blocker = sweeper.trace(eye, target);
    let plan_len = ((target[0] - eye[0]).powi(2) + (target[1] - eye[1]).powi(2)).sqrt();
    if plan_len <= 0.0 {
        return None;
    }
    let n = (plan_len.ceil() as usize).clamp(8, 2000);
    let at = |t: f64| (eye[0] + (target[0] - eye[0]) * t, eye[1] + (target[1] - eye[1]) * t);
    let line_z = |t: f64| eye[2] + (target[2] - eye[2]) * t;
    let road: Vec<(f64, f64)> = (0..=n)
        .filter_map(|i| {
            let t = i as f64 / n as f64;
            let (x, y) = at(t);
            approach.alignment.station_offset(x, y).map(|(s, _)| (t * plan_len, approach.road_level(s)))
        })
        .collect();
    let ground: Vec<(f64, f64)> = scene
        .map(|sc| {
            (0..=n)
                .filter_map(|i| {
                    let t = i as f64 / n as f64;
                    let (x, y) = at(t);
                    sc.ground_z(x, y).map(|z| (t * plan_len, z))
                })
                .collect()
        })
        .unwrap_or_default();

    let mut levels: Vec<f64> = vec![eye[2], target[2], face.origin[2]];
    levels.extend(road.iter().map(|p| p.1));
    levels.extend(ground.iter().map(|p| p.1));
    let zmin = levels.iter().cloned().fold(f64::INFINITY, f64::min) - 0.5;
    let pt = |d: f64, z: f64| (d * 1000.0, (z - zmin) * 1000.0 * k);
    let mut b = Builder::new();
    b.polyline(&ground.iter().map(|p| pt(p.0, p.1)).collect::<Vec<_>>(), Some(Colour::Green));
    b.polyline(&road.iter().map(|p| pt(p.0, p.1)).collect::<Vec<_>>(), None);
    // The sight line.
    let stop_t = blocker.as_ref().map_or(1.0, |bl| (bl.along_line_m / ((target[0] - eye[0]).powi(2) + (target[1] - eye[1]).powi(2) + (target[2] - eye[2]).powi(2)).sqrt()).clamp(0.0, 1.0));
    let (e, mid, end) = (pt(0.0, eye[2]), pt(stop_t * plan_len, line_z(stop_t)), pt(plan_len, target[2]));
    match &blocker {
        None => b.line(e, end, 0.0, Some(Colour::Blue)),
        Some(_) => {
            b.line(e, mid, 0.0, Some(Colour::Red));
            b.line(mid, end, 0.0, Some(Colour::Grey));
            b.square(mid, 100.0, Colour::Red);
        }
    }
    // Eye and face.
    let road0 = road.first().map_or(eye[2] - sweep.params.eye_height_m, |p| p.1);
    b.line(pt(0.0, road0), e, 0.0, Some(Colour::Blue));
    let (f0, f1) = (pt(plan_len, face.origin[2]), pt(plan_len, face.origin[2] + face.height_m));
    b.line(f0, f1, 100.0, Some(Colour::Black));
    let road_end = road.last().map_or(face.origin[2], |p| p.1);
    b.line(pt(plan_len, road_end), f0, 0.0, Some(Colour::Grey));

    b.label(e, format!("eye {:.2} m above the road, {:.1} m before the sign ({})", sweep.params.eye_height_m, eye_sample.distance_m, if sweep.direction == TravelDirection::UpStation { "up-station" } else { "down-station" }), LabelKind::Level);
    b.label(f1, format!("{} face, lower edge {:.2} m, top {:.2} m", a.mark, face.origin[2], face.origin[2] + face.height_m), LabelKind::Sign);
    if let Some(&(d, z)) = road.get(road.len() / 2) {
        b.label(pt(d, z), "road level (profile)".to_string(), LabelKind::Level);
    }
    if let Some(&(d, z)) = ground.get(ground.len() / 2) {
        b.label(pt(d, z), "ground (TIN)".to_string(), LabelKind::Level);
    }
    let title = match &blocker {
        Some(bl) => {
            let text = format!("{} '{}'{} blocks the sight line", bl.kind.label(), bl.label, bl.station_m.map_or(String::new(), |s| format!(" at station {s:.1}")));
            b.label(mid, text.clone(), LabelKind::Obstruction);
            format!("{}: sight line from {:.1} m before the sign; {text}", a.mark, eye_sample.distance_m)
        }
        None => format!("{}: sight line from {:.1} m before the sign is clear", a.mark, eye_sample.distance_m),
    };
    b.label(((b.max.0 + b.min.0) / 2.0, b.max.1), title, LabelKind::Title);
    let extents = Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(b.max.0, b.max.1) };
    Some(VisibilityDrawing { kind: DrawingKind::Profile, drawing: Drawing { primitives: b.prims, extents }, labels: b.labels, origin_a_m: 0.0, origin_b_m: zmin, vertical_exaggeration: k })
}
