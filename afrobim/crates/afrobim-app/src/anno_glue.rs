// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Annotation commands, without any drawing code: text, dimensions, leaders and measuring.
//!
//! Every command is a small state machine, [`Session`]. The canvas (or the command line) feeds it
//! points and typed text through [`Session::point`], [`Session::text`] and [`Session::enter`]; it
//! says what it wants next in [`Session::prompt`] (AutoCAD style: "Specify first point:", options
//! in brackets) and returns an [`Out`] with the lines for the command history and the element it
//! made. Nothing here needs a pointer, so the tests drive it directly. Pointer interaction on the
//! canvas only turns a click into a call of [`Session::point`].
//!
//! Points are model millimetres. Typed lengths and angles go through the document's
//! `DisplayUnits` parsers. No size here is a drafting-standard value: heights and arrow sizes
//! come from the named styles, which are editable defaults (see `afrobim-annotate`).

use afrobim_alignment::Alignment;
use afrobim_annotate as ann;
use afrobim_annotate::geometry::{add as padd, dist, mid, pt, sub};
use afrobim_annotate::{Ann, AnnGeometry, ChainMode, DimData, DimKind, DisplayUnits, LeaderData, LeaderText, LevelSource, LinearOrient, TextData};
use afrobim_kernel::{Document, ElementId, Pt};

use crate::alignment_glue;

/// The layer new annotation goes on until the layer manager sets another.
pub const LAYER: &str = ann::DEFAULT_LAYER;

/// Every annotation command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    MText,
    /// Linear or aligned, inferred from the picks.
    Dim,
    DimLinear,
    DimAligned,
    DimAngular,
    DimRadius,
    DimDiameter,
    DimArc,
    DimContinue,
    DimBaseline,
    DimStation,
    DimLevel,
    Leader,
    /// A leader that can attach to an element, so its text is the element's mark and follows it.
    MLeader,
    /// Distance, or cumulative distance with more than two points.
    Dist,
    MeasureAngle,
    MeasureRadius,
    Area,
    /// Coordinates of a point, with station, offset and level on an alignment.
    Id,
}

impl Kind {
    pub fn is_measure(self) -> bool {
        matches!(self, Kind::Dist | Kind::MeasureAngle | Kind::MeasureRadius | Kind::Area | Kind::Id)
    }
}

/// What the session is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Points,
    Height,
    Rotation,
    Content,
    Level,
}

/// What a call of a session did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Out {
    /// Lines for the command history.
    pub echo: Vec<String>,
    /// Lines for the measure results window (one entry per result).
    pub results: Vec<String>,
    /// The element made or changed.
    pub created: Option<ElementId>,
    /// The command is over.
    pub finished: bool,
    /// The document changed (take an undo step).
    pub changed: bool,
    /// The command works in site coordinates: the caller should show the alignments layer.
    pub wants_alignments: bool,
}

impl Out {
    fn say(mut self, line: impl Into<String>) -> Out {
        self.echo.push(line.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub kind: Kind,
    /// The layer the new annotation goes on.
    pub layer: String,
    pts: Vec<Pt>,
    phase: Phase,
    height_mm: Option<f64>,
    rotation_deg: f64,
    orient: Option<LinearOrient>,
    alignment: Option<String>,
    level: Option<(f64, LevelSource, String)>,
    sampled: Option<(f64, String)>,
    target: Option<ElementId>,
    base: Option<ElementId>,
    pub undo_marker: bool,
}

/// The first dimension that can be continued: the selection when it is a linear dimension.
fn linear_dim_of(doc: &Document, id: ElementId) -> Option<(DimData, Pt)> {
    let e = doc.find(id)?;
    if e.category != ann::DIM_CATEGORY {
        return None;
    }
    match ann::read_absolute(e).ok()? {
        Ann::Dim(d) if matches!(d.kind, DimKind::Linear { .. } | DimKind::Chain { .. }) => Some((d, e.origin)),
        _ => None,
    }
}

pub fn units(doc: &Document) -> DisplayUnits {
    ann::settings::load(doc).units
}

/// The alignments of the document by name.
pub fn alignment_list(doc: &Document) -> Vec<(String, Alignment)> {
    alignment_glue::alignments(doc).into_iter().filter_map(|(id, name)| alignment_glue::alignment_of(doc, id).map(|a| (name, a))).collect()
}

/// Typed point input with the document's units: `x,y` absolute, `@dx,dy` relative to `last`,
/// `d<a` or `@d<a` polar. Lengths use `DisplayUnits::parse_length`, angles `parse_angle`.
pub fn parse_point(text: &str, last: Option<Pt>, u: &DisplayUnits) -> Option<Pt> {
    let mut t = text.trim();
    if t.is_empty() {
        return None;
    }
    let relative = t.starts_with('@');
    if relative {
        t = &t[1..];
    }
    let origin = if relative { last.unwrap_or_default() } else { Pt::default() };
    if let Some((d, a)) = t.split_once('<') {
        let (d, a) = (u.parse_length(d.trim()).ok()?, u.parse_angle(a.trim()).ok()?);
        let r = a.to_radians();
        return Some(pt(origin.x + d * r.cos(), origin.y + d * r.sin()));
    }
    let mut parts = t.split(',');
    let (x, y) = (u.parse_length(parts.next()?.trim()).ok()?, u.parse_length(parts.next()?.trim()).ok()?);
    if parts.next().is_some() {
        return None;
    }
    Some(pt(origin.x + x, origin.y + y))
}

/// As [`parse_point`], and with a project coordinate system an absolute `x,y` (no `@`, no `<`) is
/// an `E,N` in metres of that system, at true magnitude: `450123.456,9960012.345` is the model
/// point (450 123 456, 9 960 012 345) mm, whatever the display unit. Relative and polar input stays
/// in the display unit.
pub fn parse_point_crs(text: &str, last: Option<Pt>, u: &DisplayUnits, crs: Option<&afrobim_kernel::ProjectCrs>) -> Option<Pt> {
    let t = text.trim();
    if crs.is_some() && !t.starts_with('@') && !t.contains('<') {
        let mut parts = t.split(',');
        let num = |p: Option<&str>| p.and_then(|v| v.trim().trim_end_matches(['m', 'M']).trim().parse::<f64>().ok()).filter(|v| v.is_finite());
        if let (Some(e), Some(n), None) = (num(parts.next()), num(parts.next()), parts.next()) {
            return Some(afrobim_kernel::model_point_from_crs(e, n));
        }
    }
    parse_point(text, last, u)
}

/// The element a click at `p` would attach a leader to: the topmost pickable model element that
/// is not an annotation or a settings record.
pub fn pick_model(doc: &Document, p: Pt, radius: f64) -> Option<ElementId> {
    doc.elements()
        .iter()
        .rev()
        .filter(|e| !ann::is_annotation(e) && e.category != ann::SETTINGS_CATEGORY && e.category != afrobim_kernel::PROJECT_SETTINGS_CATEGORY && !crate::imagery::is_backdrop(&e.category))
        .filter(|e| doc.layer_visible(&e.layer) && !doc.layers().iter().any(|l| l.name == e.layer && l.locked))
        .find(|e| e.bounds().inflate(radius).contains_point(p))
        .map(|e| e.id)
}

/// The level at a site point in metres and where it came from: the first survey surface that
/// covers the point, else the profile of the nearest alignment that has one (the level of its
/// centre line at the point's station).
pub fn level_at(doc: &Document, site_mm: Pt) -> Option<(f64, String)> {
    let (x, y) = (site_mm.x / 1000.0, site_mm.y / 1000.0);
    for (id, name) in afrobim_corridor::surfaces(doc) {
        if let Some(z) = afrobim_corridor::surface_of(doc, id).and_then(|t| t.elevation_at(x, y)) {
            return Some((z, format!("surface {name}")));
        }
    }
    let mut best: Option<(f64, f64, String)> = None;
    for (name, a) in alignment_list(doc) {
        let Some(profile) = &a.profile else { continue };
        let Some((internal, offset)) = a.station_offset(x, y) else { continue };
        let Ok(z) = profile.elevation_at(internal) else { continue };
        if best.as_ref().is_none_or(|b| offset.abs() < b.0) {
            let label = a.station_label(internal, 3);
            best = Some((offset.abs(), z, format!("profile of {name} at station {label} (centre line level)")));
        }
    }
    best.map(|(_, z, s)| (z, s))
}

/// What ID says about a point: coordinates, then station, offset and level on the nearest
/// alignment (or the one named).
pub fn id_report(doc: &Document, p: Pt, alignment: Option<&str>) -> String {
    let u = units(doc);
    let mut text = format!("Point X = {}, Y = {}", u.length_u(p.x), u.length_u(p.y));
    // The same point in the project coordinate system: eastings and northings in metres at true
    // magnitude, latitude and longitude on the datum of the system, convergence and scale.
    if let Some(crs) = afrobim_kernel::project_crs(doc) {
        for line in crate::crs_ui::where_lines(p, Some(&crs)).into_iter().skip(1) {
            text.push_str("; ");
            text.push_str(&line);
        }
    }
    let list = alignment_list(doc);
    let mut best: Option<(f64, String)> = None;
    for (name, a) in &list {
        if alignment.is_some_and(|n| !n.eq_ignore_ascii_case(name)) {
            continue;
        }
        if let Ok(r) = ann::measure::on_alignment(a, p, &u) {
            if let ann::Measurement::OnAlignment { offset_m, .. } = r.measurement {
                if best.as_ref().is_none_or(|b| offset_m.abs() < b.0) {
                    best = Some((offset_m.abs(), format!("{name}: {}", r.text)));
                }
            }
        }
    }
    match best {
        Some((_, line)) => text.push_str(&format!("; {line}")),
        None if list.is_empty() => text.push_str("; no alignment in the document"),
        None => text.push_str("; the point has no foot on an alignment"),
    }
    if let Some((z, source)) = level_at(doc, p) {
        text.push_str(&format!("; ground level {} m from {source}", ann::units::fixed(z, u.length_decimals)));
    }
    text
}

impl Session {
    /// Starts a command. Fails with a reason when it cannot (for example DIMSTATION with no
    /// alignment, or DIMCONTINUE with no dimension to continue).
    pub fn start(kind: Kind, doc: &Document, layer: &str, last_dim: Option<ElementId>) -> Result<Session, String> {
        let mut s = Session {
            kind,
            layer: layer.to_string(),
            pts: Vec::new(),
            phase: Phase::Points,
            height_mm: None,
            rotation_deg: 0.0,
            orient: None,
            alignment: None,
            level: None,
            sampled: None,
            target: None,
            base: None,
            undo_marker: false,
        };
        match kind {
            Kind::DimStation => {
                let list = alignment_list(doc);
                s.alignment = Some(list.first().map(|a| a.0.clone()).ok_or("There is no alignment yet. Import one with LANDXML, then dimension a station.")?);
            }
            Kind::DimContinue | Kind::DimBaseline => {
                let selected = doc.primary().filter(|id| linear_dim_of(doc, *id).is_some());
                let base = selected.or(last_dim.filter(|id| linear_dim_of(doc, *id).is_some()));
                s.base = Some(base.ok_or("There is no linear dimension to continue. Select one, or make one with DIMLINEAR or DIMALIGNED first.")?);
            }
            _ => {}
        }
        Ok(s)
    }

    /// How many points are collected so far.
    pub fn points(&self) -> &[Pt] {
        &self.pts
    }

    fn fixed_points(&self) -> Option<usize> {
        match self.kind {
            Kind::Text | Kind::DimLevel | Kind::Id => Some(1),
            Kind::MText => Some(2),
            Kind::Dim | Kind::DimLinear | Kind::DimAligned | Kind::DimRadius | Kind::DimDiameter | Kind::MeasureAngle | Kind::MeasureRadius => Some(3),
            Kind::DimAngular | Kind::DimArc => Some(4),
            Kind::DimStation => Some(2),
            _ => None,
        }
    }

    /// What the command asks for now, as it would show on the command line.
    pub fn prompt(&self, doc: &Document) -> String {
        let u = units(doc);
        let st = ann::settings::load(doc);
        let n = self.pts.len();
        match self.phase {
            Phase::Height => {
                let h = st.current_text().height_paper_mm * st.annotation_scale;
                format!("Specify height <{}> (style {}):", u.length_u(self.height_mm.unwrap_or(h)), st.current_text_style)
            }
            Phase::Rotation => format!("Specify rotation angle of text <{}>:", u.angle(self.rotation_deg)),
            Phase::Content => match self.kind {
                Kind::MText => "Enter text (type \\n for a new line; Enter to finish):".into(),
                Kind::Leader | Kind::MLeader => "Enter the text of the leader (Enter for none):".into(),
                _ => "Enter text:".into(),
            },
            Phase::Level => match &self.sampled {
                Some((z, src)) => format!("Level <{} m from {src}>: type a level in metres, or Enter to accept:", ann::units::fixed(*z, 3)),
                None => "No surface or profile gives a level here. Type the level in metres:".into(),
            },
            Phase::Points => match self.kind {
                Kind::Text => "Specify start point of text:".into(),
                Kind::MText => if n == 0 { "Specify first corner:" } else { "Specify opposite corner:" }.into(),
                Kind::Dim | Kind::DimLinear | Kind::DimAligned => match n {
                    0 => "Specify first extension line origin:".into(),
                    1 => "Specify second extension line origin:".into(),
                    _ if self.kind == Kind::DimAligned => "Specify dimension line location:".into(),
                    _ => "Specify dimension line location or [Horizontal/Vertical/Aligned]:".into(),
                },
                Kind::DimAngular => ["Specify angle vertex:", "Specify first angle endpoint:", "Specify second angle endpoint:", "Specify dimension arc line location:"][n.min(3)].into(),
                Kind::DimRadius | Kind::DimDiameter => ["Specify centre point:", "Specify a point on the circle:", "Specify dimension line location:"][n.min(2)].into(),
                Kind::DimArc => ["Specify centre point:", "Specify arc start point:", "Specify arc end point (anticlockwise from the start):", "Specify dimension arc line location:"][n.min(3)].into(),
                Kind::DimContinue | Kind::DimBaseline => "Specify next extension line origin (Enter to finish):".into(),
                Kind::DimStation => match n {
                    0 => format!("Specify point on alignment {} or type another alignment name:", self.alignment.as_deref().unwrap_or("?")),
                    _ => "Specify text location:".into(),
                },
                Kind::DimLevel => "Specify point to level:".into(),
                Kind::Leader => if n == 0 { "Specify arrow point:" } else { "Specify next point (Enter to add the text):" }.into(),
                Kind::MLeader => {
                    if n == 0 {
                        "Specify arrow point (click an element to show its mark):".into()
                    } else if self.target.is_some() {
                        "Specify landing point:".into()
                    } else {
                        "Specify next point (Enter to add the text):".into()
                    }
                }
                Kind::Dist => if n == 0 { "Specify first point:" } else { "Specify next point (Enter to finish):" }.into(),
                Kind::MeasureAngle => ["Specify angle vertex:", "Specify first point:", "Specify second point:"][n.min(2)].into(),
                Kind::MeasureRadius => format!("Specify point {} on the arc:", n + 1),
                Kind::Area => if n < 3 { format!("Specify point {}:", n + 1) } else { "Specify next point (Enter to close and measure):".to_string() },
                Kind::Id => format!("Specify point{}:", if self.alignment.is_some() { " (alignment named)" } else { " or type an alignment name" }),
            },
        }
    }

    // ── building the annotation from the points ─────────────────────────────────────────

    fn infer_orient(&self, p1: Pt, p2: Pt, q: Pt) -> LinearOrient {
        match self.kind {
            Kind::DimAligned => LinearOrient::Aligned,
            Kind::DimLinear => self.orient.unwrap_or_else(|| {
                let m = mid(p1, p2);
                let d = sub(q, m);
                if d.y.abs() >= d.x.abs() { LinearOrient::Horizontal } else { LinearOrient::Vertical }
            }),
            _ => self.orient.unwrap_or_else(|| {
                let d = sub(p2, p1);
                let a = d.y.atan2(d.x).to_degrees().rem_euclid(180.0);
                if a < 1.0 || a > 179.0 {
                    LinearOrient::Horizontal
                } else if (a - 90.0).abs() < 1.0 {
                    LinearOrient::Vertical
                } else {
                    LinearOrient::Aligned
                }
            }),
        }
    }

    /// The annotation for `pts` (which may be fewer than the command finally needs, for the
    /// hover preview) and the text, in model coordinates.
    fn assemble(&self, doc: &Document, pts: &[Pt], content: Option<&str>) -> Result<Ann, String> {
        let st = ann::settings::load(doc);
        let style = st.current_dim();
        let need = |n: usize| if pts.len() >= n { Ok(()) } else { Err("not enough points yet".to_string()) };
        match self.kind {
            Kind::Text | Kind::MText => {
                let text = content.unwrap_or("Text");
                let mut t;
                if self.kind == Kind::Text {
                    need(1)?;
                    t = TextData::new(text, pts[0], &st.current_text(), st.annotation_scale);
                } else {
                    need(2)?;
                    let (a, b) = (pts[0], pts[1]);
                    t = TextData::new(&text.replace("\\n", "\n"), pt(a.x.min(b.x), a.y.max(b.y)), &st.current_text(), st.annotation_scale);
                    t.justify = ann::Justify::TopLeft;
                    t.wrap_width = Some((a.x - b.x).abs().max(1.0));
                }
                if let Some(h) = self.height_mm {
                    t.height = ann::HeightSpec::Model(h);
                }
                t.rotation_deg = self.rotation_deg;
                Ok(Ann::Text(t))
            }
            Kind::Dim | Kind::DimLinear | Kind::DimAligned => {
                need(2)?;
                let (p1, p2) = (pts[0], pts[1]);
                let q = pts.get(2).copied().unwrap_or(p2);
                let orient = self.infer_orient(p1, p2, q);
                Ok(Ann::Dim(DimData::new(DimKind::Linear { p1, p2, line_pt: q, orient }, &style)))
            }
            Kind::DimAngular => {
                need(3)?;
                let arc_pt = pts.get(3).copied().unwrap_or_else(|| mid(pts[1], pts[2]));
                Ok(Ann::Dim(DimData::new(DimKind::Angular { vertex: pts[0], p1: pts[1], p2: pts[2], arc_pt }, &style)))
            }
            Kind::DimRadius | Kind::DimDiameter => {
                need(2)?;
                let radius = dist(pts[0], pts[1]);
                let at = pts.get(2).copied().unwrap_or(pts[1]);
                let kind = if self.kind == Kind::DimRadius { DimKind::Radius { centre: pts[0], radius, at } } else { DimKind::Diameter { centre: pts[0], radius, at } };
                Ok(Ann::Dim(DimData::new(kind, &style)))
            }
            Kind::DimArc => {
                need(3)?;
                let c = pts[0];
                let radius = dist(c, pts[1]);
                let ang = |p: Pt| (p.y - c.y).atan2(p.x - c.x).to_degrees();
                let offset = pts.get(3).map(|p| (dist(c, *p) - radius).abs()).unwrap_or(radius * 0.1);
                Ok(Ann::Dim(DimData::new(DimKind::ArcLength { centre: c, radius, start_deg: ang(pts[1]), end_deg: ang(pts[2]), offset }, &style)))
            }
            Kind::DimStation => {
                need(1)?;
                let name = self.alignment.clone().ok_or("no alignment chosen")?;
                let text_h = style.m(style.text_height_mm);
                let text_pt = pts.get(1).copied().unwrap_or(padd(pts[0], pt(text_h * 2.0, text_h * 2.0)));
                Ok(Ann::Dim(DimData::new(DimKind::StationOffset { point: pts[0], alignment: name, text_pt }, &style)))
            }
            Kind::DimLevel => {
                need(1)?;
                let (value_m, source, _) = self.level.clone().or_else(|| self.sampled.clone().map(|(z, s)| (z, LevelSource::Sampled, s))).ok_or("no level yet")?;
                let text_h = style.m(style.text_height_mm);
                let text_pt = pts.get(1).copied().unwrap_or(padd(pts[0], pt(text_h * 2.0, text_h * 2.0)));
                Ok(Ann::Dim(DimData::new(DimKind::Level { point: pts[0], value_m, source, text_pt }, &style)))
            }
            Kind::Leader | Kind::MLeader => {
                need(2)?;
                let text = match (self.target, content) {
                    (Some(id), _) => LeaderText::Mark { element: id.0, cached: doc.find(id).map(|e| e.name.clone()).unwrap_or_default() },
                    (None, Some(t)) if !t.is_empty() => LeaderText::Literal(t.to_string()),
                    _ => LeaderText::None,
                };
                Ok(Ann::Leader(LeaderData::new(pts.to_vec(), text, &style)))
            }
            _ => Err("this command makes no annotation".into()),
        }
    }

    /// The drawing the command would make if the next point were `cursor`: the hover preview.
    pub fn preview(&self, doc: &Document, cursor: Pt) -> Option<AnnGeometry> {
        if self.phase != Phase::Points || self.kind.is_measure() {
            return None;
        }
        match self.kind {
            Kind::DimContinue | Kind::DimBaseline => {
                let (mut d, origin) = linear_dim_of(doc, self.base?)?;
                let mut pts = match &d.kind {
                    DimKind::Linear { p1, p2, .. } => vec![*p1, *p2],
                    DimKind::Chain { pts, .. } => pts.clone(),
                    _ => return None,
                };
                for p in &self.pts {
                    pts.push(*p);
                }
                pts.push(cursor);
                self.rechain(&mut d, pts, origin);
                d.compute().ok().map(|r| r.geometry)
            }
            _ => {
                let mut pts = self.pts.clone();
                pts.push(cursor);
                if matches!(self.kind, Kind::Leader | Kind::MLeader) && pts.len() < 2 {
                    return None;
                }
                self.assemble(doc, &pts, Some("")).ok().and_then(|a| a.geometry().ok())
            }
        }
        .map(|mut g: AnnGeometry| {
            g.notes.clear();
            g
        })
    }

    fn rechain(&self, d: &mut DimData, pts: Vec<Pt>, _origin: Pt) {
        let (line_pt, orient) = match &d.kind {
            DimKind::Linear { line_pt, orient, .. } | DimKind::Chain { line_pt, orient, .. } => (*line_pt, *orient),
            _ => return,
        };
        let mode = if self.kind == Kind::DimBaseline { ChainMode::Baseline } else { ChainMode::Continued };
        d.kind = DimKind::Chain { mode, pts, line_pt, orient };
    }

    // ── input ───────────────────────────────────────────────────────────────────────────

    /// A point was given (picked, snapped or typed). `pick_radius` is the pickbox in model mm.
    pub fn point(&mut self, doc: &mut Document, p: Pt, pick_radius: f64) -> Out {
        if self.phase != Phase::Points {
            return Out::default().say(format!("{} Type it on the command line.", self.prompt(doc)));
        }
        let u = units(doc);
        let mut out = Out::default();
        if matches!(self.kind, Kind::DimStation) {
            out.wants_alignments = true;
        }
        if self.kind == Kind::MLeader && self.pts.is_empty() {
            self.target = pick_model(doc, p, pick_radius);
            if let Some(id) = self.target {
                let name = doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
                out.echo.push(format!("Leader to {name}: its mark will follow the element."));
            }
        }
        self.pts.push(p);
        let n = self.pts.len();
        match self.kind {
            Kind::Dist => {
                if n == 2 {
                    let r = ann::measure::distance(self.pts[0], self.pts[1], &u);
                    out.results.push(r.text.clone());
                    out.echo.push(r.text);
                } else if n > 2 {
                    if let Ok(r) = ann::measure::cumulative(&self.pts, &u) {
                        out.results.push(r.text.clone());
                        out.echo.push(r.text);
                    }
                }
            }
            Kind::Id => {
                let line = id_report(doc, p, self.alignment.as_deref());
                out.results.push(line.clone());
                out.echo.push(line);
                out.finished = true;
            }
            Kind::MeasureAngle if n == 3 => {
                match ann::measure::angle(self.pts[0], self.pts[1], self.pts[2], &u) {
                    Ok(r) => {
                        out.results.push(r.text.clone());
                        out.echo.push(r.text);
                    }
                    Err(e) => out.echo.push(e),
                }
                out.finished = true;
            }
            Kind::MeasureRadius if n == 3 => {
                match ann::measure::radius_three_points(self.pts[0], self.pts[1], self.pts[2], &u) {
                    Ok(r) => {
                        out.results.push(r.text.clone());
                        out.echo.push(r.text);
                    }
                    Err(e) => out.echo.push(e),
                }
                out.finished = true;
            }
            Kind::MLeader if self.target.is_some() && n == 2 => return self.finish_annotation(doc, None, out),
            Kind::DimContinue | Kind::DimBaseline => return self.extend_chain(doc, out),
            Kind::DimLevel => {
                self.sampled = level_at(doc, p);
                if let Some((z, s)) = &self.sampled {
                    out.echo.push(format!("Level at the point: {} m from {s}.", ann::units::fixed(*z, 3)));
                }
                self.phase = Phase::Level;
            }
            Kind::Text => self.phase = Phase::Height,
            Kind::MText if n == 2 => self.phase = Phase::Content,
            _ => {
                if self.fixed_points() == Some(n) {
                    return self.finish_annotation(doc, None, out);
                }
            }
        }
        out
    }

    /// Enter, Space or the right button.
    pub fn enter(&mut self, doc: &mut Document) -> Out {
        let u = units(doc);
        let mut out = Out::default();
        match self.phase {
            Phase::Height => {
                self.phase = Phase::Rotation;
                out
            }
            Phase::Rotation => {
                self.phase = Phase::Content;
                out
            }
            Phase::Level => match self.sampled.clone() {
                Some((z, src)) => {
                    self.level = Some((z, LevelSource::Sampled, src));
                    self.phase = Phase::Points;
                    out.echo.push(format!("Level {} m accepted.", ann::units::fixed(z, 3)));
                    // The text goes beside the point unless a second point is picked; finish now.
                    self.finish_annotation(doc, None, out)
                }
                None => out.say("There is no sampled level: type the level in metres."),
            },
            Phase::Content => match self.kind {
                Kind::Leader | Kind::MLeader => self.finish_annotation(doc, Some(""), out),
                _ => {
                    out.finished = true;
                    out.say("Nothing typed: no text made.")
                }
            },
            Phase::Points => match self.kind {
                Kind::Leader | Kind::MLeader if self.pts.len() >= 2 => {
                    self.phase = Phase::Content;
                    out
                }
                Kind::DimContinue | Kind::DimBaseline => {
                    out.finished = true;
                    out.changed = self.undo_marker;
                    out.say("DIM chain finished.")
                }
                Kind::Dist => {
                    out.finished = true;
                    if self.pts.len() >= 2 {
                        if let Ok(r) = ann::measure::cumulative(&self.pts, &u) {
                            out.results.push(format!("Total of {} legs: {}", self.pts.len() - 1, r.text));
                            out.echo.push(format!("Total {}", r.text));
                        }
                    }
                    out
                }
                Kind::Area => {
                    out.finished = true;
                    match ann::measure::polygon(&self.pts, &u) {
                        Ok(r) => {
                            out.results.push(r.text.clone());
                            out.echo.push(r.text);
                        }
                        Err(e) => out.echo.push(e),
                    }
                    out
                }
                _ => {
                    out.finished = true;
                    out.say("*Cancel*")
                }
            },
        }
    }

    /// Typed text: a point, a number, a keyword or the annotation's text, by what is asked.
    pub fn text(&mut self, doc: &mut Document, t: &str) -> Out {
        let t = t.trim();
        if t.is_empty() {
            return self.enter(doc);
        }
        let u = units(doc);
        let out = Out::default();
        match self.phase {
            Phase::Height => match u.parse_length(t) {
                Ok(h) if h > 0.0 => {
                    self.height_mm = Some(h);
                    self.phase = Phase::Rotation;
                    out
                }
                _ => out.say("Type a height greater than zero, for example 2.5 (in the display unit), or press Enter."),
            },
            Phase::Rotation => match u.parse_angle(t) {
                Ok(a) => {
                    self.rotation_deg = a;
                    self.phase = Phase::Content;
                    out
                }
                Err(e) => out.say(e.to_string()),
            },
            Phase::Content => self.finish_annotation(doc, Some(t), out),
            Phase::Level => match t.trim_end_matches(['m', 'M']).trim().parse::<f64>() {
                Ok(z) if z.is_finite() => {
                    self.level = Some((z, LevelSource::Entered, "typed".into()));
                    self.finish_annotation(doc, None, out)
                }
                _ => out.say("Type the level in metres, for example 112.505."),
            },
            Phase::Points => {
                // Keywords.
                let key = t.to_ascii_uppercase();
                if matches!(self.kind, Kind::Dim | Kind::DimLinear) && self.pts.len() == 2 {
                    let orient = match key.as_str() {
                        "H" | "HORIZONTAL" => Some(LinearOrient::Horizontal),
                        "V" | "VERTICAL" => Some(LinearOrient::Vertical),
                        "A" | "ALIGNED" => Some(LinearOrient::Aligned),
                        _ => None,
                    };
                    if let Some(o) = orient {
                        self.orient = Some(o);
                        return out.say(format!("Dimension orientation: {o:?}."));
                    }
                }
                if matches!(self.kind, Kind::DimStation | Kind::Id) && self.pts.is_empty() {
                    if let Some((name, _)) = alignment_list(doc).into_iter().find(|(n, _)| n.eq_ignore_ascii_case(t)) {
                        self.alignment = Some(name.clone());
                        return out.say(format!("Alignment {name}."));
                    }
                }
                match parse_point_crs(t, self.pts.last().copied(), &u, afrobim_kernel::project_crs(doc).as_ref()) {
                    Some(p) => self.point(doc, p, 0.0),
                    None => {
                        let names = alignment_list(doc).into_iter().map(|a| a.0).collect::<Vec<_>>().join(", ");
                        let hint = if matches!(self.kind, Kind::DimStation | Kind::Id) && !names.is_empty() { format!(" Alignments: {names}.") } else { String::new() };
                        out.say(format!("'{t}' is not a point. Type x,y or @dx,dy or distance<angle, or pick one.{hint}"))
                    }
                }
            }
        }
    }

    fn finish_annotation(&mut self, doc: &mut Document, content: Option<&str>, mut out: Out) -> Out {
        let pts = self.pts.clone();
        let result = self.assemble(doc, &pts, content).and_then(|a| ann::add_annotation(doc, a, &self.layer));
        match result {
            Ok(id) => {
                ann::refresh(doc);
                let (name, summary) = describe(doc, id);
                out.created = Some(id);
                out.changed = true;
                out.finished = true;
                out.echo.push(format!("{name} made{summary}"));
            }
            Err(e) => {
                out.finished = true;
                out.echo.push(format!("Not made: {e}"));
            }
        }
        out
    }

    fn extend_chain(&mut self, doc: &mut Document, mut out: Out) -> Out {
        let Some(base) = self.base else { return out };
        let Some((mut d, origin)) = linear_dim_of(doc, base) else {
            out.finished = true;
            return out.say("The dimension being continued is gone.");
        };
        let mut pts: Vec<Pt> = match &d.kind {
            DimKind::Linear { p1, p2, .. } => vec![*p1, *p2],
            DimKind::Chain { pts, .. } => pts.clone(),
            _ => return out,
        };
        pts.push(*self.pts.last().expect("a point was just added"));
        self.rechain(&mut d, pts, origin);
        match ann::replace_absolute(doc, base, Ann::Dim(d)) {
            Ok(()) => {
                self.undo_marker = true;
                out.created = Some(base);
                out.changed = true;
                out.echo.push(format!("Extended {}: {} extension line origins.", doc.find(base).map(|e| e.name.clone()).unwrap_or_default(), self.pts.len() + 2));
            }
            Err(e) => {
                self.pts.pop();
                out.echo.push(format!("Not added: {e}"));
            }
        }
        out
    }
}

/// The mark of an annotation and a short summary of what it measures or says.
pub fn describe(doc: &Document, id: ElementId) -> (String, String) {
    let Some(e) = doc.find(id) else { return (String::new(), String::new()) };
    let summary = match ann::read(e) {
        Ok(Ann::Dim(d)) => match d.compute() {
            Ok(r) => format!(": {}", r.texts.join(", ")),
            Err(_) => String::new(),
        },
        Ok(Ann::Text(t)) => format!(": \"{}\"", t.text.replace('\n', " / ")),
        Ok(Ann::Leader(l)) => format!(": \"{}\"", l.display_text()),
        Err(_) => String::new(),
    };
    (e.name.clone(), summary)
}

// ─── editing an existing annotation ──────────────────────────────────────────────────────────

/// Moves definition point `index` of an annotation (what a grip drag does). The change is
/// refused with a reason when the annotation can no longer be drawn.
pub fn move_grip(doc: &mut Document, id: ElementId, index: usize, to: Pt) -> Result<(), String> {
    let e = doc.find(id).ok_or("That annotation no longer exists.")?;
    let mut a = ann::read_absolute(e)?;
    if !a.set_grip(index, to) {
        return Err("That grip does not exist.".into());
    }
    ann::replace_absolute(doc, id, a)
}

/// The grips of an annotation in model space.
pub fn grips_of(doc: &Document, id: ElementId) -> Vec<Pt> {
    doc.find(id).and_then(|e| ann::read_absolute(e).ok()).map(|a| a.grips()).unwrap_or_default()
}

/// Sets the text of a text annotation (TEXTEDIT), or the literal text of a leader.
pub fn set_text(doc: &mut Document, id: ElementId, text: &str) -> Result<(), String> {
    let e = doc.find_mut(id).ok_or("That annotation no longer exists.")?;
    let mut a = ann::read(e)?;
    match &mut a {
        Ann::Text(t) => t.text = text.to_string(),
        Ann::Leader(l) => l.text = if text.is_empty() { LeaderText::None } else { LeaderText::Literal(text.to_string()) },
        Ann::Dim(_) => return Err("A dimension's text is its override: set it in Properties.".into()),
    }
    ann::write(e, a)?;
    ann::refresh(doc);
    Ok(())
}

// ─── object snap ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SnapMode {
    Endpoint,
    Midpoint,
    Centre,
    Quadrant,
    Intersection,
    Node,
    Grid,
}

impl SnapMode {
    pub const ALL: [SnapMode; 7] = [SnapMode::Endpoint, SnapMode::Midpoint, SnapMode::Centre, SnapMode::Quadrant, SnapMode::Intersection, SnapMode::Node, SnapMode::Grid];

    pub fn label(self) -> &'static str {
        match self {
            SnapMode::Endpoint => "Endpoint",
            SnapMode::Midpoint => "Midpoint",
            SnapMode::Centre => "Centre",
            SnapMode::Quadrant => "Quadrant",
            SnapMode::Intersection => "Intersection",
            SnapMode::Node => "Node",
            SnapMode::Grid => "Grid",
        }
    }

    /// Which element types provide this mode. Stated plainly in the OSNAP dialog.
    pub fn provided_by(self) -> &'static str {
        match self {
            SnapMode::Endpoint => "alignments (the ends of each line, curve and spiral); the ends of the lines and arcs of an element's drawing, or the corners of its bounding rectangle where the drawing has none; DXF underlays",
            SnapMode::Midpoint => "alignments (the middle of each element); the middle of each line and arc of an element's drawing, or of each edge of its bounding rectangle where the drawing has none; DXF underlays",
            SnapMode::Centre => "alignment arcs; the centres of the arcs and circles of an element's drawing, or the centre of its bounding rectangle where the drawing has none; arcs and circles of DXF underlays",
            SnapMode::Quadrant => "circles of an element's drawing; circles of DXF underlays",
            SnapMode::Intersection => "straight alignment elements, the drawn lines of elements (or their bounding rectangle edges) with each other; straight segments of DXF underlays",
            SnapMode::Node => "the definition points of annotations (dimension points, text insertion, leader points); points, text and block inserts of DXF underlays",
            SnapMode::Grid => "the grid, when it is shown",
        }
    }

    fn rank(self) -> u8 {
        match self {
            SnapMode::Endpoint => 0,
            SnapMode::Intersection => 1,
            SnapMode::Midpoint => 2,
            SnapMode::Centre => 3,
            SnapMode::Quadrant => 4,
            SnapMode::Node => 5,
            SnapMode::Grid => 6,
        }
    }
}

/// Which modes are on, and whether object snap is on at all (F3).
#[derive(Debug, Clone, PartialEq)]
pub struct Osnap {
    pub on: bool,
    pub modes: Vec<SnapMode>,
}

impl Default for Osnap {
    /// Default: on, with every mode but quadrant and grid.
    fn default() -> Self {
        Osnap { on: true, modes: vec![SnapMode::Endpoint, SnapMode::Midpoint, SnapMode::Centre, SnapMode::Intersection, SnapMode::Node] }
    }
}

impl Osnap {
    pub fn has(&self, m: SnapMode) -> bool {
        self.modes.contains(&m)
    }

    pub fn set(&mut self, m: SnapMode, on: bool) {
        self.modes.retain(|x| *x != m);
        if on {
            self.modes.push(m);
        }
    }
}

/// One snap candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct Cand {
    pub p: Pt,
    pub mode: SnapMode,
    /// What provides it, for the tooltip.
    pub from: String,
    /// The element it belongs to (none for the grid and the underlays), so that an element being
    /// moved does not snap to itself.
    pub owner: Option<ElementId>,
}

fn segment_intersection(a: (Pt, Pt), b: (Pt, Pt)) -> Option<Pt> {
    let (p, r) = (a.0, sub(a.1, a.0));
    let (q, s) = (b.0, sub(b.1, b.0));
    let denom = r.x * s.y - r.y * s.x;
    if denom.abs() < 1e-12 {
        return None;
    }
    let qp = sub(q, p);
    let t = (qp.x * s.y - qp.y * s.x) / denom;
    let u = (qp.x * r.y - qp.y * r.x) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| pt(p.x + t * r.x, p.y + t * r.y))
}

/// The most lines and arcs of one element's drawing that are examined for one query. A drawing
/// with more (a long run of markings, a surface) is read only as far as this and the rest offers
/// nothing; an element with no line or arc in the part read falls back to its bounding rectangle.
pub const DRAWN_BUDGET: usize = 2000;

/// The most drawn straight segments kept, over every element, for intersections. Pairs are
/// compared, so this bounds the work of a query at about `SEGMENT_BUDGET` squared over two.
pub const SEGMENT_BUDGET: usize = 400;

/// Candidates of the model within `radius` of `cursor` (model mm). `grid` is the grid spacing
/// when the grid is shown. `extra` carries what the underlays provide. Elements are offered by
/// their bounding rectangle; see [`candidates_with`] for elements with a drawing.
#[cfg(test)]
pub fn candidates(doc: &Document, cursor: Pt, radius: f64, osnap: &Osnap, grid: Option<f64>, extra: &[Cand]) -> Vec<Cand> {
    candidates_with(doc, cursor, radius, osnap, grid, extra, &|_| None)
}

/// What a drawing offers: its snap points (point, mode, what it is) and its straight segments,
/// in model millimetres, for the part of it within `window` (x0, y0, x1, y1). `origin` is the
/// element's origin (its drawing is in the element's own frame). Reads at most `budget` lines and
/// arcs. Lines give their ends (Endpoint) and middle (Midpoint) and are also segments; an arc
/// gives its ends, its middle (Midpoint) and its centre (Centre); a circle (an arc of a full
/// turn) gives its centre and its four quadrant points. The second result is true when the
/// drawing has any line or arc in the part read.
pub fn drawn_snaps(origin: Pt, d: &afrobim_draw::Drawing, window: (f64, f64, f64, f64), budget: usize) -> (Vec<(Pt, SnapMode, &'static str)>, Vec<(Pt, Pt)>, bool) {
    use afrobim_draw::Primitive;
    let mut points = Vec::new();
    let mut segments = Vec::new();
    let mut read = 0usize;
    let inside = |a: Pt, b: Pt| a.x.max(b.x) >= window.0 && a.x.min(b.x) <= window.2 && a.y.max(b.y) >= window.1 && a.y.min(b.y) <= window.3;
    for prim in &d.primitives {
        match prim {
            Primitive::Line { from, to, .. } => {
                read += 1;
                let (a, b) = (pt(origin.x + from.x, origin.y + from.y), pt(origin.x + to.x, origin.y + to.y));
                if inside(a, b) {
                    points.push((a, SnapMode::Endpoint, "line"));
                    points.push((b, SnapMode::Endpoint, "line"));
                    points.push((mid(a, b), SnapMode::Midpoint, "line"));
                    segments.push((a, b));
                }
            }
            Primitive::Arc { centre, radius, start_deg, end_deg, .. } => {
                read += 1;
                let c = pt(origin.x + centre.x, origin.y + centre.y);
                if inside(pt(c.x - radius, c.y - radius), pt(c.x + radius, c.y + radius)) {
                    let at = |deg: f64| pt(c.x + radius * deg.to_radians().cos(), c.y + radius * deg.to_radians().sin());
                    points.push((c, SnapMode::Centre, "arc"));
                    if (end_deg - start_deg).abs() >= 360.0 - 1e-6 {
                        for q in 0..4 {
                            points.push((at(q as f64 * 90.0), SnapMode::Quadrant, "circle"));
                        }
                    } else {
                        points.push((at(*start_deg), SnapMode::Endpoint, "arc"));
                        points.push((at(*end_deg), SnapMode::Endpoint, "arc"));
                        points.push((at((start_deg + end_deg) / 2.0), SnapMode::Midpoint, "arc"));
                    }
                }
            }
            _ => {}
        }
        if read >= budget {
            break;
        }
    }
    (points, segments, read > 0)
}

/// As [`candidates`], where `drawn` gives an element's generated drawing when it has one. An
/// element with a drawing that has lines or arcs is offered by them ([`drawn_snaps`]: ends and
/// middles of lines, centres, ends and middles of arcs, centres and quadrants of circles, at most
/// [`DRAWN_BUDGET`] per element and [`SEGMENT_BUDGET`] segments for intersections), not by its
/// bounding rectangle. An element with no drawing, or none with a line or an arc (a sign whose
/// face is filled shapes and lettering), keeps the corners, edge middles and centre of its
/// bounding rectangle.
#[allow(clippy::too_many_arguments)]
pub fn candidates_with<'d>(doc: &Document, cursor: Pt, radius: f64, osnap: &Osnap, grid: Option<f64>, extra: &[Cand], drawn: &dyn Fn(ElementId) -> Option<&'d afrobim_draw::Drawing>) -> Vec<Cand> {
    let mut out: Vec<Cand> = Vec::new();
    let near = |p: Pt| (p.x - cursor.x).abs() <= radius && (p.y - cursor.y).abs() <= radius && dist(p, cursor) <= radius;
    let push = |out: &mut Vec<Cand>, owner: Option<ElementId>, p: Pt, mode: SnapMode, from: &str| {
        if osnap.has(mode) && near(p) {
            out.push(Cand { p, mode, from: from.to_string(), owner });
        }
    };
    let window = (cursor.x - radius * 4.0, cursor.y - radius * 4.0, cursor.x + radius * 4.0, cursor.y + radius * 4.0);
    let mut segments: Vec<((Pt, Pt), String)> = Vec::new();
    for e in doc.elements() {
        if !doc.layer_visible(&e.layer) || e.category == ann::SETTINGS_CATEGORY || e.category == afrobim_kernel::PROJECT_SETTINGS_CATEGORY {
            continue;
        }
        let b = e.bounds();
        let touches = b.max.x >= window.0 && b.min.x <= window.2 && b.max.y >= window.1 && b.min.y <= window.3;
        let owner = Some(e.id);
        if ann::is_annotation(e) {
            if touches || e.width == 0.0 {
                for g in grips_of(doc, e.id) {
                    push(&mut out, owner, g, SnapMode::Node, &format!("definition point of {}", e.name));
                }
            }
            continue;
        }
        if e.category == afrobim_alignment::CATEGORY {
            // The alignment's elements, in site coordinates (its element sits at its site origin).
            let Some(a) = alignment_glue::alignment_of(doc, e.id) else { continue };
            for el in &a.elements {
                let (sx, sy) = (el.x * 1000.0, el.y * 1000.0);
                let (ex, ey, _) = el.end();
                let (ex, ey) = (ex * 1000.0, ey * 1000.0);
                let from = format!("alignment {}", e.name);
                push(&mut out, owner, pt(sx, sy), SnapMode::Endpoint, &from);
                push(&mut out, owner, pt(ex, ey), SnapMode::Endpoint, &from);
                let (mx, my) = el.point_at(el.length / 2.0);
                push(&mut out, owner, pt(mx * 1000.0, my * 1000.0), SnapMode::Midpoint, &from);
                if let (Some(r), Some(turn)) = (el.radius_start(), el.turn()) {
                    if el.kind() == afrobim_alignment::ElementKind::Arc {
                        let h = el.heading_rad;
                        let side = if matches!(turn, afrobim_alignment::Turn::Left) { 1.0 } else { -1.0 };
                        push(&mut out, owner, pt(sx - side * r * 1000.0 * h.sin(), sy + side * r * 1000.0 * h.cos()), SnapMode::Centre, &from);
                    }
                }
                if el.kind() == afrobim_alignment::ElementKind::Line && osnap.has(SnapMode::Intersection) {
                    let (a, b2) = (pt(sx, sy), pt(ex, ey));
                    if a.x.max(b2.x) >= window.0 && a.x.min(b2.x) <= window.2 && a.y.max(b2.y) >= window.1 && a.y.min(b2.y) <= window.3 {
                        segments.push(((a, b2), from));
                    }
                }
            }
            continue;
        }
        if !touches {
            continue;
        }
        // The drawing, where the element has one with lines or arcs in it.
        if let Some(d) = drawn(e.id) {
            let (points, segs, any) = drawn_snaps(e.origin, d, window, DRAWN_BUDGET);
            if any {
                for (p, mode, what) in points {
                    push(&mut out, owner, p, mode, &format!("{what} of {}", e.name));
                }
                if osnap.has(SnapMode::Intersection) {
                    for (a, b2) in segs {
                        if segments.len() < SEGMENT_BUDGET {
                            segments.push(((a, b2), format!("line of {}", e.name)));
                        }
                    }
                }
                continue;
            }
        }
        let from = format!("bounding rectangle of {}", e.name);
        let (x0, y0, x1, y1) = (b.min.x, b.min.y, b.max.x, b.max.y);
        let corners = [pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)];
        for c in corners {
            push(&mut out, owner, c, SnapMode::Endpoint, &from);
        }
        for i in 0..4 {
            let (a, b2) = (corners[i], corners[(i + 1) % 4]);
            push(&mut out, owner, mid(a, b2), SnapMode::Midpoint, &from);
            if segments.len() < SEGMENT_BUDGET {
                segments.push(((a, b2), from.clone()));
            }
        }
        push(&mut out, owner, pt((x0 + x1) / 2.0, (y0 + y1) / 2.0), SnapMode::Centre, &from);
    }
    if osnap.has(SnapMode::Intersection) {
        for i in 0..segments.len() {
            for j in i + 1..segments.len() {
                if let Some(p) = segment_intersection(segments[i].0, segments[j].0) {
                    push(&mut out, None, p, SnapMode::Intersection, &format!("{} and {}", segments[i].1, segments[j].1));
                }
            }
        }
    }
    for c in extra {
        if osnap.has(c.mode) && near(c.p) {
            out.push(c.clone());
        }
    }
    if let (Some(step), true) = (grid, osnap.has(SnapMode::Grid)) {
        if step > 0.0 {
            let g = pt((cursor.x / step).round() * step, (cursor.y / step).round() * step);
            push(&mut out, None, g, SnapMode::Grid, "grid");
        }
    }
    out
}

/// The candidate that wins: the nearest, ties by mode.
pub fn best(cands: Vec<Cand>, cursor: Pt) -> Option<Cand> {
    cands.into_iter().min_by(|a, b| {
        let (da, db) = (dist(a.p, cursor), dist(b.p, cursor));
        // Within a millimetre the mode decides (an endpoint beats a midpoint at the same spot).
        if (da - db).abs() < 1.0 { a.mode.rank().cmp(&b.mode.rank()) } else { da.total_cmp(&db) }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ann::{Ann, DimKind};

    fn doc_with_alignment() -> (Document, ElementId) {
        let mut doc = Document::default();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/corridor.xml");
        alignment_glue::import_landxml(&mut doc, &path).unwrap();
        let (id, _) = alignment_glue::alignments(&doc).remove(0);
        (doc, id)
    }

    fn run(kind: Kind, doc: &mut Document, steps: &[&str]) -> (Session, Vec<Out>) {
        let mut s = Session::start(kind, doc, LAYER, None).unwrap();
        let mut outs = Vec::new();
        for st in steps {
            outs.push(if st.is_empty() { s.enter(doc) } else { s.text(doc, st) });
        }
        (s, outs)
    }

    fn the_ann(doc: &Document, id: ElementId) -> Ann {
        ann::read_absolute(doc.find(id).unwrap()).unwrap()
    }

    #[test]
    fn text_asks_point_height_rotation_then_the_text_and_uses_the_style_height_by_default() {
        let mut doc = Document::default();
        let mut s = Session::start(Kind::Text, &doc, LAYER, None).unwrap();
        assert_eq!(s.prompt(&doc), "Specify start point of text:");
        s.point(&mut doc, pt(10_000.0, 5000.0), 5.0);
        assert!(s.prompt(&doc).starts_with("Specify height <"));
        s.enter(&mut doc); // accept the style height
        assert!(s.prompt(&doc).starts_with("Specify rotation angle"));
        s.text(&mut doc, "30");
        assert_eq!(s.prompt(&doc), "Enter text:");
        let out = s.text(&mut doc, "Existing kerb");
        assert!(out.finished && out.changed);
        let Ann::Text(t) = the_ann(&doc, out.created.unwrap()) else { panic!() };
        assert_eq!((t.text.as_str(), t.rotation_deg), ("Existing kerb", 30.0));
        assert!(matches!(t.height, ann::HeightSpec::Paper { .. }), "the height follows the style unless typed");
        assert!((t.at.x - 10_000.0).abs() < 1e-9);
        // Typed height overrides: 0.4 m.
        let mut s = Session::start(Kind::Text, &doc, LAYER, None).unwrap();
        s.text(&mut doc, "0,0");
        s.text(&mut doc, "0.4");
        s.enter(&mut doc);
        let out = s.text(&mut doc, "Big");
        let Ann::Text(t) = the_ann(&doc, out.created.unwrap()) else { panic!() };
        assert_eq!(t.height.model_mm(), 400.0);
    }

    #[test]
    fn mtext_takes_two_corners_a_width_and_several_lines() {
        let mut doc = Document::default();
        let (_, outs) = run(Kind::MText, &mut doc, &["0,0", "6,-2", "Line one\\nLine two"]);
        let id = outs.last().unwrap().created.unwrap();
        let Ann::Text(t) = the_ann(&doc, id) else { panic!() };
        assert_eq!(t.wrap_width, Some(6000.0));
        assert_eq!(t.justify, ann::Justify::TopLeft);
        assert_eq!(t.lines().len(), 2);
        assert_eq!(t.at, pt(0.0, 0.0));
    }

    #[test]
    fn the_dimension_family_makes_each_kind_from_its_picks() {
        let mut doc = Document::default();
        // Aligned: 3 4 5 triangle in metres gives 5.000.
        let (_, o) = run(Kind::DimAligned, &mut doc, &["0,0", "3,4", "-1,2"]);
        let Ann::Dim(d) = the_ann(&doc, o[2].created.unwrap()) else { panic!() };
        assert!(matches!(d.kind, DimKind::Linear { orient: LinearOrient::Aligned, .. }));
        assert_eq!(d.compute().unwrap().texts, vec!["5.000".to_string()]);
        // Linear: the line placed above the points is horizontal and measures dx only.
        let (_, o) = run(Kind::DimLinear, &mut doc, &["0,0", "3,4", "1,6"]);
        let Ann::Dim(d) = the_ann(&doc, o[2].created.unwrap()) else { panic!() };
        assert_eq!(d.compute().unwrap().texts, vec!["3.000".to_string()]);
        // ...and placed to the side it is vertical and measures dy.
        let (_, o) = run(Kind::DimLinear, &mut doc, &["0,0", "3,4", "6,2"]);
        let Ann::Dim(d) = the_ann(&doc, o[2].created.unwrap()) else { panic!() };
        assert_eq!(d.compute().unwrap().texts, vec!["4.000".to_string()]);
        // The keyword overrides the inference.
        let (_, o) = run(Kind::DimLinear, &mut doc, &["0,0", "3,4", "H", "6,2"]);
        let Ann::Dim(d) = the_ann(&doc, o[3].created.unwrap()) else { panic!() };
        assert_eq!(d.compute().unwrap().texts, vec!["3.000".to_string()]);
        // DIM infers aligned for a sloping pair, horizontal for a level pair.
        let (_, o) = run(Kind::Dim, &mut doc, &["0,0", "3,4", "0,6"]);
        let Ann::Dim(d) = the_ann(&doc, o[2].created.unwrap()) else { panic!() };
        assert_eq!(d.compute().unwrap().texts, vec!["5.000".to_string()]);
        let (_, o) = run(Kind::Dim, &mut doc, &["0,0", "3,0", "0,2"]);
        let Ann::Dim(d) = the_ann(&doc, o[2].created.unwrap()) else { panic!() };
        assert!(matches!(d.kind, DimKind::Linear { orient: LinearOrient::Horizontal, .. }));
        // Angular: 90 degrees at the origin, arc picked in the first quadrant.
        let (_, o) = run(Kind::DimAngular, &mut doc, &["0,0", "4,0", "0,4", "2,2"]);
        let Ann::Dim(d) = the_ann(&doc, o[3].created.unwrap()) else { panic!() };
        assert_eq!(d.compute().unwrap().texts, vec!["90.00\u{b0}".to_string()]);
        // Radius and diameter of a circle of radius 2.5 m.
        let (_, o) = run(Kind::DimRadius, &mut doc, &["0,0", "2.5,0", "1.5,1.5"]);
        let Ann::Dim(d) = the_ann(&doc, o[2].created.unwrap()) else { panic!() };
        assert_eq!(d.compute().unwrap().texts, vec!["R2.500".to_string()]);
        let (_, o) = run(Kind::DimDiameter, &mut doc, &["0,0", "2.5,0", "1.5,1.5"]);
        let Ann::Dim(d) = the_ann(&doc, o[2].created.unwrap()) else { panic!() };
        assert!(d.compute().unwrap().texts[0].ends_with("5.000"));
        // Arc length of a quarter of radius 2: 2 * pi / 2 = 3.142.
        let (_, o) = run(Kind::DimArc, &mut doc, &["0,0", "2,0", "0,2", "2.6,2.6"]);
        let Ann::Dim(d) = the_ann(&doc, o[3].created.unwrap()) else { panic!() };
        assert!(d.compute().unwrap().texts[0].ends_with("3.142"), "{:?}", d.compute().unwrap().texts);
    }

    #[test]
    fn continue_and_baseline_extend_the_linear_dimension_in_one_element() {
        let mut doc = Document::default();
        let (_, o) = run(Kind::DimLinear, &mut doc, &["0,0", "3,0", "0,2"]);
        let base = o[2].created.unwrap();
        let mut s = Session::start(Kind::DimContinue, &doc, LAYER, Some(base)).unwrap();
        s.text(&mut doc, "7,0");
        let out = s.text(&mut doc, "12,0");
        assert_eq!(out.created, Some(base));
        s.enter(&mut doc);
        let Ann::Dim(d) = the_ann(&doc, base) else { panic!() };
        assert!(matches!(d.kind, DimKind::Chain { mode: ChainMode::Continued, .. }));
        assert_eq!(d.compute().unwrap().texts, vec!["3.000", "4.000", "5.000"]);
        let mut s = Session::start(Kind::DimBaseline, &doc, LAYER, Some(base)).unwrap();
        s.text(&mut doc, "15,0");
        let Ann::Dim(d) = the_ann(&doc, base) else { panic!() };
        assert!(matches!(d.kind, DimKind::Chain { mode: ChainMode::Baseline, .. }));
        // With no dimension to continue the command says so.
        let empty = Document::default();
        assert!(Session::start(Kind::DimContinue, &empty, LAYER, None).is_err());
    }

    #[test]
    fn station_dimension_shows_the_displayed_station_through_the_equations() {
        let (mut doc, id) = doc_with_alignment();
        let a = alignment_glue::alignment_of(&doc, id).unwrap();
        // Internal station 1000 + 400 is past the first equation (displayed jumps 1300 to 1350):
        // the displayed station there is 1450.
        let internal = a.start_station_m + 400.0;
        let p = a.point_at(internal).unwrap();
        let site = pt(p.x * 1000.0, p.y * 1000.0);
        assert_eq!(a.displayed_station(internal).unwrap().station_m.round(), 1450.0);
        let mut s = Session::start(Kind::DimStation, &doc, LAYER, None).unwrap();
        assert!(s.prompt(&doc).contains(&a.name));
        let o1 = s.point(&mut doc, site, 5.0);
        assert!(o1.wants_alignments);
        let out = s.point(&mut doc, padd(site, pt(3000.0, 3000.0)), 5.0);
        let (_, summary) = describe(&doc, out.created.unwrap());
        assert!(summary.contains("1+450"), "{summary}");
        let Ann::Dim(d) = the_ann(&doc, out.created.unwrap()) else { panic!() };
        assert!((d.cache.station_m.unwrap() - 1450.0).abs() < 0.5);
        // No alignment, no command.
        assert!(Session::start(Kind::DimStation, &Document::default(), LAYER, None).is_err());
    }

    #[test]
    fn level_is_typed_or_sampled_from_the_profile() {
        let (mut doc, id) = doc_with_alignment();
        let a = alignment_glue::alignment_of(&doc, id).unwrap();
        let p = a.point_at(a.start_station_m + 100.0).unwrap();
        let site = pt(p.x * 1000.0, p.y * 1000.0);
        // Typed.
        let mut s = Session::start(Kind::DimLevel, &doc, LAYER, None).unwrap();
        s.point(&mut doc, pt(0.0, 0.0), 5.0);
        let out = s.text(&mut doc, "112.5");
        let Ann::Dim(d) = the_ann(&doc, out.created.unwrap()) else { panic!() };
        assert!(matches!(d.kind, DimKind::Level { value_m, source: LevelSource::Entered, .. } if (value_m - 112.5).abs() < 1e-9));
        // Sampled from the profile: Enter accepts.
        let mut s = Session::start(Kind::DimLevel, &doc, LAYER, None).unwrap();
        let o = s.point(&mut doc, site, 5.0);
        assert!(o.echo.iter().any(|l| l.contains("profile of")), "{:?}", o.echo);
        let out = s.enter(&mut doc);
        let Ann::Dim(d) = the_ann(&doc, out.created.unwrap()) else { panic!() };
        let DimKind::Level { value_m, source, .. } = d.kind else { panic!() };
        assert_eq!(source, LevelSource::Sampled);
        assert!((value_m - a.profile.as_ref().unwrap().elevation_at(a.start_station_m + 100.0).unwrap()).abs() < 1e-6);
    }

    #[test]
    fn a_leader_to_an_element_shows_its_mark_and_follows_a_rename() {
        let mut doc = Document::default();
        let target = doc.add("sign", "ADS-001", pt(0.0, 0.0), 2000.0, 1000.0, "Signs", serde_json::json!({}));
        let mut s = Session::start(Kind::MLeader, &doc, LAYER, None).unwrap();
        let o = s.point(&mut doc, pt(500.0, 500.0), 5.0);
        assert!(o.echo[0].contains("ADS-001"));
        let out = s.point(&mut doc, pt(3000.0, 2000.0), 5.0);
        let id = out.created.unwrap();
        let Ann::Leader(l) = the_ann(&doc, id) else { panic!() };
        assert_eq!(l.display_text(), "ADS-001");
        doc.find_mut(target).unwrap().name = "ADS-007".into();
        ann::refresh(&mut doc);
        let Ann::Leader(l) = the_ann(&doc, id) else { panic!() };
        assert_eq!(l.display_text(), "ADS-007");
        // A plain leader takes literal text after the points.
        let (_, o) = run(Kind::Leader, &mut doc, &["10,10", "12,12", "", "R61"]);
        let Ann::Leader(l) = the_ann(&doc, o[3].created.unwrap()) else { panic!() };
        assert_eq!(l.display_text(), "R61");
    }

    #[test]
    fn measuring_reports_in_the_document_units_and_ids_a_point_on_an_alignment() {
        let (mut doc, id) = doc_with_alignment();
        let (_, o) = run(Kind::Dist, &mut doc, &["0,0", "3,4", ""]);
        assert!(o[1].results[0].starts_with("Distance = 5.000 m"), "{:?}", o[1].results);
        let (_, o) = run(Kind::Area, &mut doc, &["0,0", "4,0", "4,3", "0,3", ""]);
        assert!(o[4].results[0].starts_with("Area = 12.00 m2, perimeter = 14.000 m"), "{:?}", o[4].results);
        let (_, o) = run(Kind::MeasureAngle, &mut doc, &["0,0", "4,0", "0,4"]);
        assert!(o[2].results[0].contains("90.00"), "{:?}", o[2].results);
        let (_, o) = run(Kind::MeasureRadius, &mut doc, &["2.5,0", "0,2.5", "-2.5,0"]);
        assert!(o[2].results[0].contains("2.500"), "{:?}", o[2].results);
        let a = alignment_glue::alignment_of(&doc, id).unwrap();
        let p = a.point_at(a.start_station_m + 100.0).unwrap();
        let line = id_report(&doc, pt(p.x * 1000.0, p.y * 1000.0), None);
        assert!(line.contains("Station 1+100.000") && line.contains("offset 0.000 m"), "{line}");
        // The document's units change what is shown.
        let mut st = ann::settings::load(&doc);
        st.units.length = ann::LengthUnit::Millimetre;
        st.units.length_decimals = 0;
        ann::settings::store(&mut doc, &st);
        let (_, o) = run(Kind::Dist, &mut doc, &["0,0", "3000,4000"]);
        assert!(o[1].results[0].starts_with("Distance = 5000 mm"), "{:?}", o[1].results);
    }

    #[test]
    fn grips_move_a_definition_point_and_a_bad_move_is_refused() {
        let mut doc = Document::default();
        let (_, o) = run(Kind::DimAligned, &mut doc, &["0,0", "3,4", "-1,2"]);
        let id = o[2].created.unwrap();
        assert_eq!(grips_of(&doc, id).len(), 3);
        move_grip(&mut doc, id, 1, pt(6000.0, 8000.0)).unwrap();
        let Ann::Dim(d) = the_ann(&doc, id) else { panic!() };
        assert_eq!(d.compute().unwrap().texts, vec!["10.000".to_string()]);
        assert!(move_grip(&mut doc, id, 9, pt(0.0, 0.0)).is_err());
        // Typed points use the display units.
        let u = units(&doc);
        assert_eq!(parse_point("@2<90", Some(pt(1000.0, 1000.0)), &u).map(|p| (p.x.round(), p.y.round())), Some((1000.0, 3000.0)));
    }

    #[test]
    fn object_snap_finds_endpoints_midpoints_centres_and_intersections() {
        let (mut doc, _) = doc_with_alignment();
        doc.add("sign", "S1", pt(0.0, 0.0), 2000.0, 1000.0, "Signs", serde_json::json!({}));
        doc.add("sign", "S2", pt(1500.0, 600.0), 2000.0, 1000.0, "Signs", serde_json::json!({}));
        let o = Osnap::default();
        let c = best(candidates(&doc, pt(5.0, 3.0), 50.0, &o, None, &[]), pt(5.0, 3.0)).unwrap();
        assert_eq!((c.p, c.mode), (pt(0.0, 0.0), SnapMode::Endpoint));
        // S1 spans x 0 to 2000 and y 0 to 1000, S2 x 1500 to 3500 and y 600 to 1600: S2's left edge
        // (x = 1500) crosses S1's top edge (y = 1000) at (1500, 1000), which is neither's midpoint.
        let c = best(candidates(&doc, pt(1510.0, 1010.0), 50.0, &o, None, &[]), pt(1510.0, 1010.0)).unwrap();
        assert_eq!((c.p, c.mode), (pt(1500.0, 1000.0), SnapMode::Intersection));
        // The middle of S2's left edge is a midpoint.
        let c = best(candidates(&doc, pt(1505.0, 1095.0), 30.0, &Osnap { on: true, modes: vec![SnapMode::Midpoint] }, None, &[]), pt(1505.0, 1095.0)).unwrap();
        assert_eq!((c.p, c.mode), (pt(1500.0, 1100.0), SnapMode::Midpoint));
        // Switched off, nothing.
        let mut off = Osnap::default();
        off.modes.clear();
        assert!(candidates(&doc, pt(5.0, 3.0), 50.0, &off, None, &[]).is_empty());
        // The grid is a mode of its own.
        let mut g = Osnap::default();
        g.set(SnapMode::Grid, true);
        let c = best(candidates(&doc, pt(5200.0, 5100.0), 300.0, &g, Some(500.0), &[]), pt(5200.0, 5100.0)).unwrap();
        assert_eq!((c.p, c.mode), (pt(5000.0, 5000.0), SnapMode::Grid));
        // An annotation's definition points are nodes.
        let (_, o2) = run(Kind::DimAligned, &mut doc, &["20,20", "23,24", "19,22"]);
        assert!(o2[2].created.is_some());
        let c = best(candidates(&doc, pt(23_010.0, 24_010.0), 100.0, &Osnap::default(), None, &[]), pt(23_010.0, 24_010.0)).unwrap();
        assert_eq!((c.p, c.mode), (pt(23_000.0, 24_000.0), SnapMode::Node));
    }

    #[test]
    fn alignment_elements_give_endpoints_and_arc_centres() {
        let (mut doc, id) = doc_with_alignment();
        // A hidden layer gives no snaps; the alignments layer is off until a command shows it.
        assert!(candidates(&doc, pt(0.0, 0.0), 1.0e9, &Osnap::default(), None, &[]).iter().all(|c| !c.from.starts_with("alignment")));
        doc.layers_mut().iter_mut().find(|l| l.name == alignment_glue::LAYER).unwrap().visible = true;
        let a = alignment_glue::alignment_of(&doc, id).unwrap();
        let first = &a.elements[0];
        let at = pt(first.x * 1000.0, first.y * 1000.0);
        let c = candidates(&doc, at, 100.0, &Osnap::default(), None, &[]);
        assert!(c.iter().any(|c| c.mode == SnapMode::Endpoint && dist(c.p, at) < 1.0 && c.from.starts_with("alignment")), "{c:?}");
        let arc = a.elements.iter().find(|e| e.kind() == afrobim_alignment::ElementKind::Arc).expect("the corridor has an arc");
        let r = arc.radius_start().unwrap() * 1000.0;
        let centre = c_of(arc, r);
        let cs = candidates(&doc, centre, 100.0, &Osnap::default(), None, &[]);
        assert!(cs.iter().any(|c| c.mode == SnapMode::Centre && dist(c.p, centre) < 1.0), "{cs:?}");
        // The centre is the radius away from the arc's start.
        assert!((dist(centre, pt(arc.x * 1000.0, arc.y * 1000.0)) - r).abs() < 1.0);
    }

    fn c_of(arc: &afrobim_alignment::AlignmentElement, r: f64) -> Pt {
        let side = if matches!(arc.turn(), Some(afrobim_alignment::Turn::Left)) { 1.0 } else { -1.0 };
        let h = arc.heading_rad;
        pt(arc.x * 1000.0 - side * r * h.sin(), arc.y * 1000.0 + side * r * h.cos())
    }

    // ─── object snap on the drawing of a model element ────────────────────────────────────────

    fn sign_at(doc: &mut Document, name: &str, at: (f64, f64), size: (f64, f64)) -> ElementId {
        doc.add("sign", name, pt(at.0, at.1), size.0, size.1, "0", serde_json::Value::Null)
    }

    fn line(a: (f64, f64), b: (f64, f64)) -> afrobim_draw::Primitive {
        afrobim_draw::Primitive::Line { from: afrobim_draw::Pt2::new(a.0, a.1), to: afrobim_draw::Pt2::new(b.0, b.1), width: 10.0, colour: None }
    }

    fn arc(c: (f64, f64), r: f64, from: f64, to: f64) -> afrobim_draw::Primitive {
        afrobim_draw::Primitive::Arc { centre: afrobim_draw::Pt2::new(c.0, c.1), radius: r, start_deg: from, end_deg: to, colour: None }
    }

    fn drawing(primitives: Vec<afrobim_draw::Primitive>) -> afrobim_draw::Drawing {
        afrobim_draw::Drawing { primitives, extents: afrobim_draw::Extents { min: afrobim_draw::Pt2::new(0.0, 0.0), max: afrobim_draw::Pt2::new(1000.0, 1000.0) } }
    }

    fn all_modes() -> Osnap {
        Osnap { on: true, modes: SnapMode::ALL.iter().copied().filter(|m| *m != SnapMode::Grid).collect() }
    }

    fn near_pt(c: &Cand, x: f64, y: f64) -> bool {
        dist(c.p, pt(x, y)) < 1e-6
    }

    #[test]
    fn an_element_with_a_drawing_is_snapped_by_its_lines_arcs_and_circles() {
        let mut doc = Document::default();
        let id = sign_at(&mut doc, "S1", (10_000.0, 20_000.0), (1000.0, 1000.0));
        // In the element's own frame; the element's origin is (10000, 20000).
        let d = drawing(vec![line((100.0, 100.0), (500.0, 100.0)), arc((700.0, 700.0), 200.0, 0.0, 90.0), arc((300.0, 700.0), 100.0, 0.0, 360.0)]);
        let drawn = |i: ElementId| (i == id).then_some(&d);
        let at = |x: f64, y: f64, r: f64| candidates_with(&doc, pt(x, y), r, &all_modes(), None, &[], &drawn);
        // The ends and the middle of the line.
        let c = at(10_100.0, 20_100.0, 50.0);
        assert!(c.iter().any(|c| c.mode == SnapMode::Endpoint && near_pt(c, 10_100.0, 20_100.0) && c.from.starts_with("line of S1")), "{c:?}");
        let c = at(10_300.0, 20_100.0, 50.0);
        assert!(c.iter().any(|c| c.mode == SnapMode::Midpoint && near_pt(c, 10_300.0, 20_100.0)), "{c:?}");
        // The bounding rectangle's corner (10000, 20000) is not offered where the drawing has lines.
        let c = at(10_000.0, 20_000.0, 60.0);
        assert!(c.is_empty(), "{c:?}");
        // An arc: centre, both ends (at 0 and 90 degrees) and the middle (45 degrees).
        let arc_at = |deg: f64| (10_700.0 + 200.0 * deg.to_radians().cos(), 20_700.0 + 200.0 * deg.to_radians().sin());
        let c = at(10_700.0, 20_700.0, 50.0);
        assert!(c.iter().any(|c| c.mode == SnapMode::Centre && near_pt(c, 10_700.0, 20_700.0) && c.from.starts_with("arc of S1")), "{c:?}");
        let (x, y) = arc_at(0.0);
        assert!(at(x, y, 20.0).iter().any(|c| c.mode == SnapMode::Endpoint && near_pt(c, x, y)));
        let (x, y) = arc_at(90.0);
        assert!(at(x, y, 20.0).iter().any(|c| c.mode == SnapMode::Endpoint && near_pt(c, x, y)));
        let (x, y) = arc_at(45.0);
        assert!(at(x, y, 20.0).iter().any(|c| c.mode == SnapMode::Midpoint && near_pt(c, x, y)));
        // A circle (a full turn): its centre and four quadrant points, no ends.
        let c = at(10_300.0, 20_700.0, 30.0);
        assert!(c.iter().any(|c| c.mode == SnapMode::Centre && near_pt(c, 10_300.0, 20_700.0)));
        for (x, y) in [(10_400.0, 20_700.0), (10_300.0, 20_800.0), (10_200.0, 20_700.0), (10_300.0, 20_600.0)] {
            let c = at(x, y, 20.0);
            assert!(c.iter().any(|c| c.mode == SnapMode::Quadrant && near_pt(c, x, y) && c.from.starts_with("circle of S1")), "{c:?}");
            assert!(c.iter().all(|c| c.mode != SnapMode::Endpoint));
        }
        // A mode that is off gives nothing; every candidate names its element.
        let off = Osnap { on: true, modes: vec![SnapMode::Midpoint] };
        let c = candidates_with(&doc, pt(10_100.0, 20_100.0), 50.0, &off, None, &[], &drawn);
        assert!(c.is_empty());
        assert!(at(10_100.0, 20_100.0, 50.0).iter().all(|c| c.owner == Some(id)));
    }

    #[test]
    fn an_element_whose_drawing_has_no_lines_or_arcs_keeps_its_bounding_rectangle() {
        let mut doc = Document::default();
        let id = sign_at(&mut doc, "S1", (0.0, 0.0), (1000.0, 500.0));
        let fills = drawing(vec![afrobim_draw::Primitive::Fill { outline: vec![afrobim_draw::Pt2::new(0.0, 0.0), afrobim_draw::Pt2::new(100.0, 0.0), afrobim_draw::Pt2::new(0.0, 100.0)], colour: afrobim_draw::Colour::White }]);
        let drawn = |i: ElementId| (i == id).then_some(&fills);
        let c = candidates_with(&doc, pt(1000.0, 500.0), 50.0, &all_modes(), None, &[], &drawn);
        assert!(c.iter().any(|c| c.mode == SnapMode::Endpoint && near_pt(c, 1000.0, 500.0) && c.from.starts_with("bounding rectangle of S1")), "{c:?}");
        let c = candidates_with(&doc, pt(500.0, 250.0), 50.0, &all_modes(), None, &[], &drawn);
        assert!(c.iter().any(|c| c.mode == SnapMode::Centre && near_pt(c, 500.0, 250.0)));
        // No drawing at all: the same.
        let c = candidates_with(&doc, pt(1000.0, 500.0), 50.0, &all_modes(), None, &[], &|_| None);
        assert!(c.iter().any(|c| c.mode == SnapMode::Endpoint && near_pt(c, 1000.0, 500.0)));
    }

    #[test]
    fn the_drawn_snaps_are_bounded_per_element_and_in_segments() {
        // Budget: at most DRAWN_BUDGET lines and arcs of one drawing are read.
        let many: Vec<afrobim_draw::Primitive> = (0..DRAWN_BUDGET * 3).map(|i| line((i as f64, 0.0), (i as f64, 10.0))).collect();
        let (points, segments, any) = drawn_snaps(pt(0.0, 0.0), &drawing(many.clone()), (-1.0e9, -1.0e9, 1.0e9, 1.0e9), DRAWN_BUDGET);
        assert!(any);
        assert_eq!(segments.len(), DRAWN_BUDGET);
        assert_eq!(points.len(), DRAWN_BUDGET * 3);
        // A line past the budget offers nothing.
        let mut doc = Document::default();
        let id = sign_at(&mut doc, "S1", (0.0, 0.0), (10_000.0, 100.0));
        let d = drawing(many);
        let drawn = |i: ElementId| (i == id).then_some(&d);
        let early = candidates_with(&doc, pt(5.0, 5.0), 3.0, &all_modes(), None, &[], &drawn);
        assert!(early.iter().any(|c| c.mode == SnapMode::Midpoint && near_pt(c, 5.0, 5.0)), "{early:?}");
        let late = candidates_with(&doc, pt((DRAWN_BUDGET + 500) as f64, 5.0), 3.0, &all_modes(), None, &[], &drawn);
        assert!(late.is_empty(), "{late:?}");
        // Window: lines outside it are not read into the result.
        let (points, segments, any) = drawn_snaps(pt(0.0, 0.0), &drawing(vec![line((0.0, 0.0), (10.0, 0.0)), line((5000.0, 0.0), (5010.0, 0.0))]), (-100.0, -100.0, 100.0, 100.0), DRAWN_BUDGET);
        assert!(any && segments.len() == 1 && points.len() == 3);
        // The segments kept for intersections are capped over every element.
        let mut doc = Document::default();
        let lines: Vec<_> = (0..SEGMENT_BUDGET).map(|i| line((i as f64 * 2.0, -50.0), (i as f64 * 2.0, 50.0))).collect();
        let a = sign_at(&mut doc, "A", (0.0, 0.0), (1000.0, 100.0));
        let b = sign_at(&mut doc, "B", (0.0, 0.0), (1000.0, 100.0));
        let (da, db) = (drawing(lines.clone()), drawing(vec![line((0.0, 0.0), (1000.0, 0.0))]));
        let drawn = |i: ElementId| if i == a { Some(&da) } else if i == b { Some(&db) } else { None };
        let only_x = Osnap { on: true, modes: vec![SnapMode::Intersection] };
        // A's lines fill the budget, so B's line is not kept and no intersection with it is found;
        // the call returns at once either way.
        let c = candidates_with(&doc, pt(100.0, 0.0), 1.0e6, &only_x, None, &[], &drawn);
        assert!(c.is_empty() || c.iter().all(|c| c.mode == SnapMode::Intersection));
    }

    #[test]
    fn two_drawn_lines_give_their_intersection() {
        let mut doc = Document::default();
        let a = sign_at(&mut doc, "A", (0.0, 0.0), (1000.0, 1000.0));
        let b = sign_at(&mut doc, "B", (0.0, 0.0), (1000.0, 1000.0));
        let (da, db) = (drawing(vec![line((0.0, 0.0), (1000.0, 1000.0))]), drawing(vec![line((0.0, 1000.0), (1000.0, 0.0))]));
        let drawn = |i: ElementId| if i == a { Some(&da) } else if i == b { Some(&db) } else { None };
        let c = candidates_with(&doc, pt(510.0, 495.0), 40.0, &all_modes(), None, &[], &drawn);
        assert!(c.iter().any(|c| c.mode == SnapMode::Intersection && near_pt(c, 500.0, 500.0) && c.from.contains("line of A") && c.from.contains("line of B")), "{c:?}");
        // Both lines' midpoints are also at (500, 500): the endpoint/intersection ranking keeps an intersection first.
        let winner = best(c, pt(500.0, 500.0)).unwrap();
        assert_eq!(winner.mode, SnapMode::Intersection);
    }

    #[test]
    fn a_moved_element_does_not_snap_to_itself_by_owner() {
        let mut doc = Document::default();
        let a = sign_at(&mut doc, "A", (0.0, 0.0), (1000.0, 1000.0));
        let b = sign_at(&mut doc, "B", (1200.0, 0.0), (1000.0, 1000.0));
        let (da, db) = (drawing(vec![line((0.0, 0.0), (1000.0, 0.0))]), drawing(vec![line((0.0, 0.0), (1000.0, 0.0))]));
        let drawn = |i: ElementId| if i == a { Some(&da) } else if i == b { Some(&db) } else { None };
        let mut c = candidates_with(&doc, pt(1000.0, 0.0), 300.0, &all_modes(), None, &[], &drawn);
        assert!(c.iter().any(|c| c.owner == Some(a)) && c.iter().any(|c| c.owner == Some(b)));
        // The caller drops the moving elements' own candidates (as `ctl_object_snap` does).
        let moving = [a];
        c.retain(|c| !c.owner.is_some_and(|o| moving.contains(&o)));
        assert!(!c.is_empty() && c.iter().all(|c| c.owner == Some(b)));
        let winner = best(c, pt(1000.0, 0.0)).unwrap();
        assert!(near_pt(&winner, 1200.0, 0.0), "{winner:?}");
    }
}
