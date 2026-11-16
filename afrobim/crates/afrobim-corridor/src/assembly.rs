//! Assemblies and subassemblies: named, reusable, parametric parts of a cross-section.
//!
//! A [`Subassembly`] is one part (a lane, a kerb, a ditch ...) with typed parameters. It is
//! attached at the outer end of the part before it and, **drawn in its own coordinates**
//! (`u` outward from the attachment point, `v` up), emits points, links and shapes that carry
//! codes: point codes (`ETW`), link codes (`Top`, `Datum`, `Pave`, `Daylight`) and shape codes
//! (the layer codes the engineer enters: `Pave1`, `Base`, `Subbase`). An [`Assembly`] is a
//! baseline point and an ordered list of subassemblies on each side.
//!
//! **No part has a built-in dimension.** Every width, depth, slope and run is a parameter
//! entered by the engineer. The codes are names, not values. Constructors in [`library`] only
//! gather the parameters.
//!
//! Conventions as in the rest of the crate: offsets are positive to the right looking up-station,
//! slopes positive rising away from the centreline, metres.

use std::collections::BTreeMap;

use afrobim_alignment::{CrossFall, Profile};
use serde::{Deserialize, Serialize};

use crate::section::{interpolate, polygon_area, DaylightPoint, SlopeKind};
use crate::template::{Daylight, Link, Slope, Template, TemplateSide};

/// IFC entity names used by `afrobim-ifc` in this repository (`IfcPavement`, with `IfcCourse`
/// `PAVEMENT` per layer). The other names the BIM brief mentions (kerb, earthworks cut and fill)
/// could not be checked against the IFC 4.3 documentation from here (the site refused the
/// request), so no part sets them: the engineer types them into the classification.
pub mod ifc {
    pub const PAVEMENT: &str = "IfcPavement";
    pub const COURSE: &str = "IfcCourse";
    pub const COURSE_PREDEFINED_PAVEMENT: &str = "PAVEMENT";
}

/// The BIM classification slot of a subassembly, a layer or a shape. All of it is entered; none
/// is checked against a schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Classification {
    /// IFC entity, for example `IfcCourse`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ifc_entity: Option<String>,
    /// IFC predefined type, for example `PAVEMENT`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predefined_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// A cost item code (a bill of quantities line, for example).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_item_code: Option<String>,
}

impl Classification {
    pub fn ifc(entity: &str, predefined_type: &str) -> Classification {
        Classification { ifc_entity: Some(entity.into()), predefined_type: Some(predefined_type.into()), ..Default::default() }
    }
    pub fn with_material(mut self, material: &str) -> Classification {
        self.material = Some(material.into());
        self
    }
    pub fn with_cost_code(mut self, code: &str) -> Classification {
        self.cost_item_code = Some(code.into());
        self
    }
    fn is_empty(&self) -> bool {
        *self == Classification::default()
    }
}

/// The side of the baseline a list of subassemblies is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
}

impl Side {
    /// +1 for the right, −1 for the left: the sign of an outward step in offset.
    pub fn sign(self) -> f64 {
        match self {
            Side::Left => -1.0,
            Side::Right => 1.0,
        }
    }
    fn word(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }
}

// ─── targets ────────────────────────────────────────────────────────────────────────────────

/// A target for an offset: a fixed offset, a table against station or a polyline in plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetTarget {
    /// A signed offset from the alignment, m (positive right).
    Fixed(f64),
    /// (station, signed offset) pairs, stations increasing; linear between them, none outside.
    Table(Vec<(f64, f64)>),
    /// A polyline in plan (x, y), for example another alignment sampled with
    /// [`polyline_from_alignment`]. The offset is where the line square to the baseline through
    /// the station meets the polyline, the nearest such meeting on the side being built.
    Polyline(Vec<(f64, f64)>),
}

/// A target for an elevation: a fixed level, a table against station or a vertical profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElevationTarget {
    Fixed(f64),
    /// (station, level) pairs, stations increasing; linear between them, none outside.
    Table(Vec<(f64, f64)>),
    /// A profile (for example the profile of another alignment) read at the station.
    Profile(Profile),
    /// The profile of another alignment, read at the station of that alignment nearest the
    /// section point (the point the link is at in plan: its attachment point for a link to an
    /// elevation, its end point for a link to an offset and an elevation together), not at the
    /// baseline station. Needs the perpendicular foot of the point on the target alignment and a
    /// profile on it.
    NearestOnAlignment(Box<afrobim_alignment::Alignment>),
}

/// A plan polyline along an alignment at a spacing, for use as an [`OffsetTarget::Polyline`].
/// Straight elements are well represented by any spacing; on curves the polyline cuts the chord,
/// so use a spacing small against the radius.
pub fn polyline_from_alignment(alignment: &afrobim_alignment::Alignment, step_m: f64) -> Result<Vec<(f64, f64)>, String> {
    if !(step_m.is_finite() && step_m > 0.0) {
        return Err(format!("the step must be greater than zero (got {step_m})"));
    }
    let (a, b) = (alignment.start_station_m, alignment.end_station_m());
    let mut out = Vec::new();
    let mut s = a;
    while s < b {
        let p = alignment.point_at(s).ok_or("station off the alignment")?;
        out.push((p.x, p.y));
        s += step_m;
    }
    let p = alignment.point_at(b).ok_or("station off the alignment")?;
    out.push((p.x, p.y));
    Ok(out)
}

fn table_at(table: &[(f64, f64)], station: f64, what: &str) -> Result<f64, String> {
    let (Some(first), Some(last)) = (table.first(), table.last()) else {
        return Err(format!("the {what} table is empty"));
    };
    if station < first.0 - 1e-9 || station > last.0 + 1e-9 {
        return Err(format!("station {station:.3} is outside the {what} table ({:.3} to {:.3})", first.0, last.0));
    }
    Ok(interpolate(table, station).unwrap_or(last.1))
}

impl OffsetTarget {
    fn value(&self, ctx: &EvalContext, side: Side) -> Result<f64, String> {
        match self {
            OffsetTarget::Fixed(o) => Ok(*o),
            OffsetTarget::Table(t) => table_at(t, ctx.station_m, "offset target"),
            OffsetTarget::Polyline(line) => {
                // The section line is p + o n with n = (sin h, -cos h). A segment A-B of the
                // polyline meets it where the component of (point - p) along the heading is 0.
                let (nx, ny) = (ctx.heading_rad.sin(), -ctx.heading_rad.cos());
                let (hx, hy) = (ctx.heading_rad.cos(), ctx.heading_rad.sin());
                let mut best: Option<f64> = None;
                for w in line.windows(2) {
                    let (ax, ay, bx, by) = (w[0].0 - ctx.x, w[0].1 - ctx.y, w[1].0 - ctx.x, w[1].1 - ctx.y);
                    let (da, db) = (ax * hx + ay * hy, bx * hx + by * hy);
                    if da == db || (da > 0.0 && db > 0.0) || (da < 0.0 && db < 0.0) {
                        continue;
                    }
                    let t = da / (da - db);
                    let (px, py) = (ax + t * (bx - ax), ay + t * (by - ay));
                    let o = px * nx + py * ny;
                    if o * side.sign() > 0.0 && best.is_none_or(|b| o.abs() < b.abs()) {
                        best = Some(o);
                    }
                }
                best.ok_or_else(|| format!("the target polyline does not cross the section at station {:.3} on the {}", ctx.station_m, side.word()))
            }
        }
    }
}

/// The plan position of the point at `offset` on the section line through the baseline station.
fn section_point(ctx: &EvalContext, offset: f64) -> (f64, f64) {
    (ctx.x + offset * ctx.heading_rad.sin(), ctx.y - offset * ctx.heading_rad.cos())
}

impl ElevationTarget {
    /// The level of the target. `at` is the section point in plan, used by
    /// [`ElevationTarget::NearestOnAlignment`] only.
    fn value(&self, ctx: &EvalContext, at: (f64, f64)) -> Result<f64, String> {
        match self {
            ElevationTarget::Fixed(z) => Ok(*z),
            ElevationTarget::Table(t) => table_at(t, ctx.station_m, "elevation target"),
            ElevationTarget::Profile(p) => p.elevation_at(ctx.station_m).map_err(|e| format!("the target profile: {e}")),
            ElevationTarget::NearestOnAlignment(a) => {
                let (station, _) = a.station_offset(at.0, at.1).ok_or_else(|| format!("the section point at station {:.3} has no perpendicular foot on the target alignment '{}'", ctx.station_m, a.name))?;
                let profile = a.profile.as_ref().ok_or_else(|| format!("the target alignment '{}' has no profile", a.name))?;
                profile.elevation_at(station).map_err(|e| format!("the profile of the target alignment '{}' at station {station:.3}: {e}", a.name))
            }
        }
    }
}

/// A width: entered, or the distance to an offset target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dim {
    Fixed(f64),
    /// The width runs from the attachment point to the target (lane widening toward another
    /// alignment). It must be greater than zero where evaluated.
    ToOffset(OffsetTarget),
}

impl Dim {
    fn resolve(&self, ctx: &EvalContext, side: Side, start_offset: f64) -> Result<f64, String> {
        let w = match self {
            Dim::Fixed(w) => *w,
            Dim::ToOffset(t) => side.sign() * (t.value(ctx, side)? - start_offset),
        };
        if w.is_finite() && w > 0.0 {
            Ok(w)
        } else {
            Err(format!("the width must be greater than zero (got {w})"))
        }
    }
}

// ─── parameters ─────────────────────────────────────────────────────────────────────────────

/// One structural layer under a strip, from the top down.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// The shape code: `Pave1`, `Base`, `Subbase` ... Entered.
    pub shape_code: String,
    /// Vertical depth of the layer, m, greater than zero.
    pub depth_m: f64,
    #[serde(default)]
    pub classification: Classification,
    /// Code of the link along the bottom of the layer. Absent: `Pave`, and `Datum` for the
    /// lowest layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom_link_code: Option<String>,
}

impl Layer {
    pub fn new(shape_code: &str, depth_m: f64) -> Layer {
        Layer { shape_code: shape_code.into(), depth_m, classification: Classification::default(), bottom_link_code: None }
    }
    /// The same layer as an `IfcCourse` `PAVEMENT` with the entered material.
    pub fn as_course(mut self, material: &str) -> Layer {
        self.classification = Classification::ifc(ifc::COURSE, ifc::COURSE_PREDEFINED_PAVEMENT).with_material(material);
        self
    }
}

/// A straight strip of finished surface, with layers under it: lane, shoulder, footway, verge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Strip {
    pub width: Dim,
    pub slope: Slope,
    /// Takes the cross-fall of the alignment's superelevation (or the entered crossfall table)
    /// for its side at the station, where one is stored; elsewhere keeps `slope`.
    #[serde(default)]
    pub follows_superelevation: bool,
    /// Top down. Empty: nothing under the strip (its datum is its surface).
    #[serde(default)]
    pub layers: Vec<Layer>,
    /// Code of the point at the outer end.
    pub point_code: String,
}

/// A vertical-faced block: a kerb or a raised median.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    /// Height of the top above the attachment level, m, greater than zero.
    pub height_m: f64,
    /// Width of the top, m, greater than zero.
    pub width_m: f64,
    /// Depth of the bed below the attachment level, m, zero or more.
    pub bed_depth_m: f64,
    pub point_code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DitchShape {
    V,
    Trapezoid { bottom_width_m: f64 },
}

/// A side ditch cut down from the attachment level and back up to the same level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ditch {
    pub shape: DitchShape,
    /// Depth of the invert below the attachment level, m, greater than zero.
    pub depth_m: f64,
    /// Horizontal metres per metre of depth on the inner (road side) face and on the outer face.
    pub inner_run_per_rise: f64,
    pub outer_run_per_rise: f64,
}

/// A median seen from the centreline outward (the assembly's mirror gives the other half). The
/// baseline point is the invert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DepressedMedian {
    /// Half the width of the flat bottom, m, zero or more.
    pub half_bottom_width_m: f64,
    /// Height from the invert to the edge of the carriageway, m, greater than zero.
    pub depth_m: f64,
    pub side_run_per_rise: f64,
}

/// A wall at the edge: top at the attachment level, `depth_m` below it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub thickness_m: f64,
    pub depth_m: f64,
}

/// How a generic link finds its end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkMode {
    WidthSlope { width: Dim, slope: Slope },
    /// Runs at `slope` until it reaches the elevation of the target.
    ToElevation { target: ElevationTarget, slope: Slope },
    /// Runs at `slope` to the offset of the target.
    ToOffset { target: OffsetTarget, slope: Slope },
    /// Runs from the attachment point to the offset of one target and the level of another
    /// together: the end point is fixed in both, so the slope is whatever joins them (a kerb
    /// return's sliver that must meet an edge in plan and in level, a link that ties two
    /// alignments). The offset must be outward of the attachment point.
    ToOffsetAndElevation { offset: OffsetTarget, elevation: ElevationTarget },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenericLink {
    /// Link codes, for example `["Top"]`.
    pub codes: Vec<String>,
    pub point_code: String,
    pub mode: LinkMode,
}

/// Benching of a side slope: after every `step_height_m` of rise (cut) or fall (fill) along the
/// side slope a flat or sloping bench `width_m` wide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bench {
    pub step_height_m: f64,
    pub width_m: f64,
    /// Cross slope of the bench, rising outward.
    pub slope: Slope,
}

/// The side slope to the ground surface (a "to surface" link). Always the last on its side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaylightRule {
    pub cut_run_per_rise: f64,
    pub fill_run_per_rise: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bench_in_cut: Option<Bench>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bench_in_fill: Option<Bench>,
}

/// What a subassembly is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubKind {
    Lane(Strip),
    Shoulder(Strip),
    Footway(Strip),
    Verge(Strip),
    /// A kerb; the next part attaches at the top of its back.
    Kerb(Block),
    /// A channel (a strip) and a kerb.
    KerbAndChannel { channel: Strip, kerb: Block },
    /// A raised median (half); the next part attaches at the foot of its back.
    MedianRaised(Block),
    MedianDepressed(DepressedMedian),
    SideDitch(Ditch),
    /// A ditch with a lining `lining_thickness_m` thick, measured vertically.
    LinedDrain { ditch: Ditch, lining_thickness_m: f64 },
    RetainingEdge(Wall),
    Link(GenericLink),
    Daylight(DaylightRule),
}

/// A named parametric part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subassembly {
    pub name: String,
    pub kind: SubKind,
    #[serde(default, skip_serializing_if = "Classification::is_empty")]
    pub classification: Classification,
    /// Renames codes in the output: default code to the code wanted.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub code_map: BTreeMap<String, String>,
}

impl Subassembly {
    pub fn new(name: &str, kind: SubKind) -> Subassembly {
        Subassembly { name: name.into(), kind, classification: Classification::default(), code_map: BTreeMap::new() }
    }
    pub fn classified(mut self, c: Classification) -> Subassembly {
        self.classification = c;
        self
    }
    pub fn rename_code(mut self, from: &str, to: &str) -> Subassembly {
        self.code_map.insert(from.into(), to.into());
        self
    }

    /// Checks the entered parameters; the message says which is wrong.
    pub fn check(&self) -> Result<(), String> {
        let at = |m: String| format!("subassembly '{}': {m}", self.name);
        let pos = |what: &str, v: f64| if v.is_finite() && v > 0.0 { Ok(()) } else { Err(at(format!("{what} must be greater than zero (got {v})"))) };
        let nonneg = |what: &str, v: f64| if v.is_finite() && v >= 0.0 { Ok(()) } else { Err(at(format!("{what} must not be negative (got {v})"))) };
        let slope = |s: &Slope| match *s {
            Slope::Ratio { run, .. } if !(run.is_finite() && run > 0.0) => Err(at(format!("the run of a slope ratio must be greater than zero (got {run})"))),
            _ if !s.rise_per_m().is_finite() => Err(at("the slope is not a finite number".into())),
            _ => Ok(()),
        };
        let dim = |d: &Dim| match d {
            Dim::Fixed(w) => pos("the width", *w),
            Dim::ToOffset(_) => Ok(()),
        };
        let strip = |s: &Strip| -> Result<(), String> {
            dim(&s.width)?;
            slope(&s.slope)?;
            for l in &s.layers {
                pos(&format!("the depth of layer '{}'", l.shape_code), l.depth_m)?;
            }
            Ok(())
        };
        let block = |b: &Block| -> Result<(), String> {
            pos("the height", b.height_m)?;
            pos("the width", b.width_m)?;
            nonneg("the bed depth", b.bed_depth_m)
        };
        let ditch = |d: &Ditch| -> Result<(), String> {
            pos("the depth", d.depth_m)?;
            pos("the inner run", d.inner_run_per_rise)?;
            pos("the outer run", d.outer_run_per_rise)?;
            if let DitchShape::Trapezoid { bottom_width_m } = d.shape {
                pos("the bottom width", bottom_width_m)?;
            }
            Ok(())
        };
        match &self.kind {
            SubKind::Lane(s) | SubKind::Shoulder(s) | SubKind::Footway(s) | SubKind::Verge(s) => strip(s),
            SubKind::Kerb(b) | SubKind::MedianRaised(b) => block(b),
            SubKind::KerbAndChannel { channel, kerb } => {
                strip(channel)?;
                block(kerb)
            }
            SubKind::MedianDepressed(m) => {
                nonneg("the half bottom width", m.half_bottom_width_m)?;
                pos("the depth", m.depth_m)?;
                pos("the side run", m.side_run_per_rise)
            }
            SubKind::SideDitch(d) => ditch(d),
            SubKind::LinedDrain { ditch: d, lining_thickness_m } => {
                ditch(d)?;
                pos("the lining thickness", *lining_thickness_m)
            }
            SubKind::RetainingEdge(w) => {
                pos("the thickness", w.thickness_m)?;
                pos("the depth", w.depth_m)
            }
            SubKind::Link(l) => match &l.mode {
                LinkMode::WidthSlope { width, slope: s } => {
                    dim(width)?;
                    slope(s)
                }
                LinkMode::ToElevation { slope: s, .. } | LinkMode::ToOffset { slope: s, .. } => slope(s),
                LinkMode::ToOffsetAndElevation { .. } => Ok(()),
            },
            SubKind::Daylight(d) => {
                pos("the cut run", d.cut_run_per_rise)?;
                pos("the fill run", d.fill_run_per_rise)?;
                for b in [&d.bench_in_cut, &d.bench_in_fill].into_iter().flatten() {
                    pos("the bench step height", b.step_height_m)?;
                    pos("the bench width", b.width_m)?;
                    slope(&b.slope)?;
                }
                Ok(())
            }
        }
    }
}

// ─── evaluation ─────────────────────────────────────────────────────────────────────────────

/// What a subassembly may read at a station.
#[derive(Debug, Clone, Copy)]
pub struct EvalContext<'a> {
    pub station_m: f64,
    /// Plan position and heading of the baseline there.
    pub x: f64,
    pub y: f64,
    pub heading_rad: f64,
    /// The profile level of the baseline, m.
    pub centre_z: f64,
    pub cross_fall: CrossFall,
    /// The ground along the section, (offset, level) with increasing offsets.
    pub ground: Option<&'a [(f64, f64)]>,
}

type P = (f64, f64);

/// A subassembly's output in its own coordinates (`u` outward, `v` up) from its attachment point.
#[derive(Debug, Clone, Default)]
pub(crate) struct Local {
    points: Vec<(P, Vec<String>)>,
    links: Vec<(Vec<String>, Vec<P>)>,
    shapes: Vec<(String, Vec<P>, Classification)>,
    /// The line earthworks are measured to, from the start to the end.
    datum: Vec<P>,
    end: P,
    terminal: Option<DaylightRule>,
}

fn codes(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

impl Local {
    fn shifted(mut self, du: f64, dv: f64) -> Local {
        let mv = |p: &mut P| {
            p.0 += du;
            p.1 += dv;
        };
        self.points.iter_mut().for_each(|(p, _)| mv(p));
        self.links.iter_mut().for_each(|(_, l)| l.iter_mut().for_each(mv));
        self.shapes.iter_mut().for_each(|(_, l, _)| l.iter_mut().for_each(mv));
        self.datum.iter_mut().for_each(mv);
        mv(&mut self.end);
        self
    }
    fn then(mut self, next: Local) -> Local {
        let n = next.shifted(self.end.0, self.end.1);
        self.points.extend(n.points);
        self.links.extend(n.links);
        self.shapes.extend(n.shapes);
        for p in n.datum {
            if self.datum.last() != Some(&p) {
                self.datum.push(p);
            }
        }
        self.end = n.end;
        self.terminal = n.terminal;
        self
    }
}

fn cross_fall_rise(ctx: &EvalContext, side: Side) -> Option<f64> {
    match side {
        Side::Left => ctx.cross_fall.left_pct,
        Side::Right => ctx.cross_fall.right_pct,
    }
    .map(|p| p / 100.0)
}

fn strip_local(s: &Strip, label: &str, ctx: &EvalContext, side: Side, start: P, own: &Classification) -> Result<Local, String> {
    let w = s.width.resolve(ctx, side, start.0)?;
    let rise = if s.follows_superelevation { cross_fall_rise(ctx, side).unwrap_or(s.slope.rise_per_m()) } else { s.slope.rise_per_m() };
    let end = (w, rise * w);
    let mut l = Local { end, ..Default::default() };
    l.points.push((end, vec![s.point_code.clone()]));
    l.links.push((codes(&["Top", label]), vec![(0.0, 0.0), end]));
    let (mut c0, n) = (0.0, s.layers.len());
    for (i, layer) in s.layers.iter().enumerate() {
        let c1 = c0 + layer.depth_m;
        let poly = vec![(0.0, -c0), (w, end.1 - c0), (w, end.1 - c1), (0.0, -c1)];
        let class = if layer.classification.is_empty() { own.clone() } else { layer.classification.clone() };
        l.shapes.push((layer.shape_code.clone(), poly, class));
        let bottom = layer.bottom_link_code.clone().unwrap_or_else(|| if i + 1 == n { "Datum".into() } else { "Pave".into() });
        let mut bc = vec![bottom];
        if i + 1 == n && bc[0] != "Datum" {
            bc.push("Datum".into());
        }
        l.links.push((bc, vec![(0.0, -c1), (w, end.1 - c1)]));
        c0 = c1;
    }
    l.datum = if n > 0 { vec![(0.0, -c0), (w, end.1 - c0)] } else { vec![(0.0, 0.0), end] };
    Ok(l)
}

fn block_local(b: &Block, own: &Classification, back_to_foot: bool) -> Local {
    let (h, w, bed) = (b.height_m, b.width_m, b.bed_depth_m);
    let mut l = Local::default();
    l.points.push(((0.0, h), codes(&["Kerb_Face_Top"])));
    l.links.push((codes(&["Face"]), vec![(0.0, 0.0), (0.0, h)]));
    l.links.push((codes(&["Top"]), vec![(0.0, h), (w, h)]));
    if back_to_foot {
        l.points.push(((w, h), codes(&["Median_Edge_Top"])));
        l.links.push((codes(&["Back"]), vec![(w, h), (w, 0.0)]));
        l.points.push(((w, 0.0), vec![b.point_code.clone()]));
        l.end = (w, 0.0);
    } else {
        l.points.push(((w, h), vec![b.point_code.clone()]));
        l.end = (w, h);
    }
    l.shapes.push(("Kerb".into(), vec![(0.0, -bed), (0.0, h), (w, h), (w, -bed)], own.clone()));
    l.links.push((codes(&["Datum"]), vec![(0.0, -bed), (w, -bed)]));
    l.datum = vec![(0.0, -bed), (w, -bed)];
    l
}

fn ditch_top(d: &Ditch) -> Vec<P> {
    let (ni, no, h) = (d.inner_run_per_rise, d.outer_run_per_rise, d.depth_m);
    let mut v = vec![(0.0, 0.0), (ni * h, -h)];
    let mut u = ni * h;
    if let DitchShape::Trapezoid { bottom_width_m } = d.shape {
        u += bottom_width_m;
        v.push((u, -h));
    }
    v.push((u + no * h, 0.0));
    v
}

fn ditch_local(d: &Ditch, lining: Option<(f64, &Classification)>) -> Local {
    let top = ditch_top(d);
    let names: Vec<&str> = if top.len() == 3 { vec!["Ditch_In", "Ditch_Out"] } else { vec!["Ditch_In", "Ditch_Bottom", "Ditch_Out"] };
    let mut l = Local { end: top[top.len() - 1], ..Default::default() };
    for (i, w) in top.windows(2).enumerate() {
        l.links.push((codes(&["Top", names[i]]), vec![w[0], w[1]]));
    }
    for (i, p) in top.iter().enumerate().skip(1) {
        let c = if i + 1 == top.len() {
            "Ditch_Top_Out"
        } else if top.len() == 3 {
            "Ditch_Invert"
        } else if i == 1 {
            "Ditch_Invert_In"
        } else {
            "Ditch_Invert_Out"
        };
        l.points.push((*p, codes(&[c])));
    }
    match lining {
        None => l.datum = top,
        Some((t, class)) => {
            let low: Vec<P> = top.iter().map(|p| (p.0, p.1 - t)).collect();
            let mut poly = top.clone();
            poly.extend(low.iter().rev());
            l.shapes.push(("Lining".into(), poly, class.clone()));
            l.links.push((codes(&["Datum"]), low.clone()));
            l.datum = low;
        }
    }
    l
}

fn generic_local(g: &GenericLink, ctx: &EvalContext, side: Side, start: P) -> Result<Local, String> {
    let (w, rise) = match &g.mode {
        LinkMode::WidthSlope { width, slope } => (width.resolve(ctx, side, start.0)?, slope.rise_per_m()),
        LinkMode::ToElevation { target, slope } => {
            let (zt, s) = (target.value(ctx, section_point(ctx, start.0))?, slope.rise_per_m());
            let w = if s == 0.0 { f64::NAN } else { (zt - start.1) / s };
            if !(w.is_finite() && w > 0.0) {
                return Err(format!("the link cannot reach level {zt:.3} from {:.3} at that slope", start.1));
            }
            (w, s)
        }
        LinkMode::ToOffset { target, slope } => {
            let o = target.value(ctx, side)?;
            let w = side.sign() * (o - start.0);
            if !(w.is_finite() && w > 0.0) {
                return Err(format!("the target offset {o:.3} is not outward of the attachment point {:.3}", start.0));
            }
            (w, slope.rise_per_m())
        }
        LinkMode::ToOffsetAndElevation { offset, elevation } => {
            let o = offset.value(ctx, side)?;
            let w = side.sign() * (o - start.0);
            if !(w.is_finite() && w > 0.0) {
                return Err(format!("the target offset {o:.3} is not outward of the attachment point {:.3}", start.0));
            }
            let zt = elevation.value(ctx, section_point(ctx, o))?;
            // The slope is what joins the attachment point to the target level at that offset.
            (w, (zt - start.1) / w)
        }
    };
    let end = (w, rise * w);
    let mut l = Local { end, datum: vec![(0.0, 0.0), end], ..Default::default() };
    l.points.push((end, vec![g.point_code.clone()]));
    l.links.push((g.codes.clone(), vec![(0.0, 0.0), end]));
    Ok(l)
}

impl Subassembly {
    /// Evaluates the part from `start` (absolute offset, level) going outward on `side`.
    pub(crate) fn eval(&self, ctx: &EvalContext, side: Side, start: P) -> Result<Local, String> {
        self.check()?;
        let at = |e: String| format!("subassembly '{}': {e}", self.name);
        let c = &self.classification;
        let mut l = match &self.kind {
            SubKind::Lane(s) => strip_local(s, "Lane", ctx, side, start, c),
            SubKind::Shoulder(s) => strip_local(s, "Shoulder", ctx, side, start, c),
            SubKind::Footway(s) => strip_local(s, "Footway", ctx, side, start, c),
            SubKind::Verge(s) => strip_local(s, "Verge", ctx, side, start, c),
            SubKind::Kerb(b) => Ok(block_local(b, c, false)),
            SubKind::MedianRaised(b) => Ok(block_local(b, c, true)),
            SubKind::KerbAndChannel { channel, kerb } => strip_local(channel, "Channel", ctx, side, start, c).map(|ch| ch.then(block_local(kerb, c, false))),
            SubKind::MedianDepressed(m) => {
                let (hb, d) = (m.half_bottom_width_m, m.depth_m);
                let end = (hb + m.side_run_per_rise * d, d);
                let mut l = Local { end, datum: vec![(0.0, 0.0), (hb, 0.0), end], ..Default::default() };
                if hb > 0.0 {
                    l.links.push((codes(&["Top", "Median_Bottom"]), vec![(0.0, 0.0), (hb, 0.0)]));
                }
                l.points.push(((hb, 0.0), codes(&["Median_Invert"])));
                l.links.push((codes(&["Top", "Median_Slope"]), vec![(hb, 0.0), end]));
                l.points.push((end, codes(&["Median_Edge"])));
                l.datum.dedup();
                Ok(l)
            }
            SubKind::SideDitch(d) => Ok(ditch_local(d, None)),
            SubKind::LinedDrain { ditch, lining_thickness_m } => Ok(ditch_local(ditch, Some((*lining_thickness_m, c)))),
            SubKind::RetainingEdge(w) => {
                let (t, d) = (w.thickness_m, w.depth_m);
                let mut l = Local { end: (t, 0.0), datum: vec![(0.0, -d), (t, -d)], ..Default::default() };
                l.shapes.push(("Wall".into(), vec![(0.0, 0.0), (t, 0.0), (t, -d), (0.0, -d)], c.clone()));
                l.links.push((codes(&["Top", "Wall_Top"]), vec![(0.0, 0.0), (t, 0.0)]));
                l.links.push((codes(&["Datum"]), vec![(0.0, -d), (t, -d)]));
                l.points.push(((t, 0.0), codes(&["Wall_Edge"])));
                Ok(l)
            }
            SubKind::Link(g) => generic_local(g, ctx, side, start),
            SubKind::Daylight(rule) => Ok(Local { terminal: Some(rule.clone()), ..Default::default() }),
        }
        .map_err(at)?;
        if !self.code_map.is_empty() {
            let map = |v: &mut Vec<String>| v.iter_mut().for_each(|c| *c = self.code_map.get(c).cloned().unwrap_or_else(|| c.clone()));
            l.points.iter_mut().for_each(|(_, c)| map(c));
            l.links.iter_mut().for_each(|(c, _)| map(c));
            for (code, _, _) in &mut l.shapes {
                if let Some(n) = self.code_map.get(code) {
                    *code = n.clone();
                }
            }
        }
        Ok(l)
    }
}

// ─── the assembly ───────────────────────────────────────────────────────────────────────────

/// A baseline point and the subassemblies on each side, from the baseline outward.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assembly {
    pub name: String,
    /// Offset of the baseline point from the alignment, m (positive right). Zero: on it.
    #[serde(default)]
    pub baseline_offset_m: f64,
    /// Level of the baseline point above the profile, m. Zero: on it.
    #[serde(default)]
    pub baseline_elevation_m: f64,
    #[serde(default)]
    pub left: Vec<Subassembly>,
    #[serde(default)]
    pub right: Vec<Subassembly>,
}

impl Assembly {
    pub fn new(name: &str) -> Assembly {
        Assembly { name: name.into(), baseline_offset_m: 0.0, baseline_elevation_m: 0.0, left: vec![], right: vec![] }
    }
    /// The same parts on both sides: the right is `parts`, the left is its mirror.
    pub fn symmetric(name: &str, parts: Vec<Subassembly>) -> Assembly {
        let mut a = Assembly::new(name);
        a.right = parts;
        a.mirror_right_to_left();
        a
    }
    /// Replaces the left side with a copy of the right. Parts are described outward from their
    /// attachment, so a copy is a mirror image; a target given as a fixed signed offset is not
    /// mirrored.
    pub fn mirror_right_to_left(&mut self) {
        self.left = self.right.clone();
    }
    pub fn mirror_left_to_right(&mut self) {
        self.right = self.left.clone();
    }
    pub fn add(&mut self, side: Side, part: Subassembly) -> &mut Assembly {
        match side {
            Side::Left => self.left.push(part),
            Side::Right => self.right.push(part),
        }
        self
    }
    pub fn side(&self, side: Side) -> &[Subassembly] {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    pub fn check(&self) -> Result<(), String> {
        if !(self.baseline_offset_m.is_finite() && self.baseline_elevation_m.is_finite()) {
            return Err(format!("assembly '{}': the baseline point is not a finite number", self.name));
        }
        for (name, parts) in [("left", &self.left), ("right", &self.right)] {
            for (i, p) in parts.iter().enumerate() {
                p.check().map_err(|e| format!("assembly '{}', {name}: {e}", self.name))?;
                if matches!(p.kind, SubKind::Daylight(_)) && i + 1 != parts.len() {
                    return Err(format!("assembly '{}', {name}: the daylight part must be the last on its side", self.name));
                }
            }
        }
        Ok(())
    }

    /// The assembly an old [`Template`] stands for. Each link is a strip (a lane with one layer
    /// of the formation depth where the link is `paved`, a verge otherwise) whose outer point
    /// carries the link's code; the daylight rule becomes a [`SubKind::Daylight`]. Sections of
    /// the result have the same earthworks line as the template's.
    pub fn from_template(template: &Template) -> Assembly {
        let side = |s: &TemplateSide| -> Vec<Subassembly> {
            let mut v: Vec<Subassembly> = s
                .links
                .iter()
                .map(|l| {
                    let layers = if l.paved && template.formation_depth_m > 0.0 { vec![Layer::new("Formation", template.formation_depth_m)] } else { vec![] };
                    let strip = Strip { width: Dim::Fixed(l.width_m), slope: l.slope, follows_superelevation: l.follows_superelevation, layers, point_code: l.code.clone() };
                    Subassembly::new(&l.code, if l.paved { SubKind::Lane(strip) } else { SubKind::Verge(strip) })
                })
                .collect();
            v.push(Subassembly::new(
                "Daylight",
                SubKind::Daylight(DaylightRule { cut_run_per_rise: s.daylight.cut_run_per_rise, fill_run_per_rise: s.daylight.fill_run_per_rise, bench_in_cut: None, bench_in_fill: None }),
            ));
            v
        };
        Assembly { name: template.name.clone(), baseline_offset_m: 0.0, baseline_elevation_m: 0.0, left: side(&template.left), right: side(&template.right) }
    }

    /// The old [`Template`] this assembly is equivalent to, when it is one. A template has one
    /// formation depth, links of one slope each, and a daylight rule; so every part must be a
    /// strip, a ditch or a fixed-width generic link, all widths fixed, layered strips must all
    /// have the same total depth, and each side must end in an unbenched daylight rule. Anything
    /// else (kerbs, targets, benches, shapes for individual layers) is refused with the part's
    /// name; build such an assembly through [`crate::build_model`] instead.
    pub fn to_template(&self) -> Result<Template, String> {
        self.check()?;
        if self.baseline_offset_m != 0.0 || self.baseline_elevation_m != 0.0 {
            return Err(format!("assembly '{}': a template starts on the alignment and profile; the baseline point is moved", self.name));
        }
        let mut formation: Option<f64> = None;
        let mut side = |parts: &[Subassembly], which: &str| -> Result<TemplateSide, String> {
            let mut links = Vec::new();
            let mut daylight = None;
            for p in parts {
                let no = |why: &str| Err(format!("assembly '{}', {which}: '{}' {why}", self.name, p.name));
                match &p.kind {
                    SubKind::Lane(s) | SubKind::Shoulder(s) | SubKind::Footway(s) | SubKind::Verge(s) => {
                        let Dim::Fixed(w) = s.width else { return no("has a width that follows a target") };
                        let depth: f64 = s.layers.iter().map(|l| l.depth_m).sum();
                        if depth > 0.0 {
                            if formation.is_some_and(|f| (f - depth).abs() > 1e-12) {
                                return no("has a total layer depth different from another layered part (a template has one formation depth)");
                            }
                            formation = Some(depth);
                        }
                        links.push(Link { code: s.point_code.clone(), width_m: w, slope: s.slope, follows_superelevation: s.follows_superelevation, paved: depth > 0.0 });
                    }
                    SubKind::Link(g) => match &g.mode {
                        LinkMode::WidthSlope { width: Dim::Fixed(w), slope } => links.push(Link::new(&g.point_code, *w, *slope)),
                        _ => return no("is a link to a target"),
                    },
                    SubKind::SideDitch(d) => {
                        let top = ditch_top(d);
                        for (i, w) in top.windows(2).enumerate() {
                            let (du, dv) = (w[1].0 - w[0].0, w[1].1 - w[0].1);
                            links.push(Link::new(&format!("{}_{}", p.name, i + 1), du, Slope::Ratio { run: du, rise: dv }));
                        }
                    }
                    SubKind::Daylight(r) => {
                        if r.bench_in_cut.is_some() || r.bench_in_fill.is_some() {
                            return no("has benching, which a template cannot hold");
                        }
                        daylight = Some(Daylight { cut_run_per_rise: r.cut_run_per_rise, fill_run_per_rise: r.fill_run_per_rise });
                    }
                    _ => return no("is a part a template cannot hold (vertical faces or a structure)"),
                }
            }
            let daylight = daylight.ok_or(format!("assembly '{}', {which}: a template needs a daylight rule on each side", self.name))?;
            Ok(TemplateSide { links, daylight })
        };
        let left = side(&self.left, "left")?;
        let right = side(&self.right, "right")?;
        Ok(Template { name: self.name.clone(), left, right, formation_depth_m: formation.unwrap_or(0.0) })
    }

    /// A corridor definition (the existing, single-assembly kind) from this assembly.
    pub fn to_corridor_definition(&self, name: &str, interval_m: f64) -> Result<crate::template::CorridorDefinition, String> {
        Ok(crate::template::CorridorDefinition::single(name, self.to_template()?, interval_m))
    }
}

// ─── output ─────────────────────────────────────────────────────────────────────────────────

/// A point of the design, with its codes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignPoint {
    pub offset_m: f64,
    pub elevation_m: f64,
    pub codes: Vec<String>,
    /// `None` on the baseline.
    pub side: Option<Side>,
    pub subassembly: String,
}

/// A link: a polyline of (offset, level) with codes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignLink {
    pub codes: Vec<String>,
    pub points: Vec<(f64, f64)>,
    pub side: Option<Side>,
    pub subassembly: String,
}

/// A closed shape (a pavement layer, a kerb, a lining).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignShape {
    pub code: String,
    pub polygon: Vec<(f64, f64)>,
    pub area_m2: f64,
    pub side: Option<Side>,
    pub subassembly: String,
    pub classification: Classification,
}

/// The points, links and shapes of an assembly at one station.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DesignGeometry {
    /// From the left outer end through the baseline to the right outer end; daylight points last.
    pub points: Vec<DesignPoint>,
    pub links: Vec<DesignLink>,
    pub shapes: Vec<DesignShape>,
    /// The line earthworks are measured to, left to right.
    pub earthworks: Vec<(f64, f64)>,
}

impl DesignGeometry {
    /// Points carrying a code.
    pub fn points_with_code<'a>(&'a self, code: &'a str) -> impl Iterator<Item = &'a DesignPoint> {
        self.points.iter().filter(move |p| p.codes.iter().any(|c| c == code))
    }
    pub fn links_with_code<'a>(&'a self, code: &'a str) -> impl Iterator<Item = &'a DesignLink> {
        self.links.iter().filter(move |l| l.codes.iter().any(|c| c == code))
    }
    /// Total area of the shapes with a code, m².
    pub fn shape_area(&self, code: &str) -> f64 {
        self.shapes.iter().filter(|s| s.code == code).map(|s| s.area_m2).sum()
    }
}

/// The result of laying an assembly on a station.
#[derive(Debug, Clone)]
pub(crate) struct Assembled {
    pub design: DesignGeometry,
    pub left_daylight: Option<DaylightPoint>,
    pub right_daylight: Option<DaylightPoint>,
    /// Whether each side has a daylight part (a side without one is closed at its last point).
    pub left_open: bool,
    pub right_open: bool,
    pub notes: Vec<String>,
}

fn slope_polyline(rule: &DaylightRule, cut: bool, reach: f64) -> Vec<P> {
    let (run, bench, sign) = if cut { (rule.cut_run_per_rise, &rule.bench_in_cut, 1.0) } else { (rule.fill_run_per_rise, &rule.bench_in_fill, -1.0) };
    match bench {
        None => vec![(0.0, 0.0), (reach, sign * reach / run)],
        Some(b) => {
            let mut v = vec![(0.0, 0.0)];
            let (mut u, mut z) = (0.0, 0.0);
            let mut guard = 0;
            while u < reach && guard < 10_000 {
                u += run * b.step_height_m;
                z += sign * b.step_height_m;
                v.push((u, z));
                u += b.width_m;
                z += b.slope.rise_per_m() * b.width_m;
                v.push((u, z));
                guard += 1;
            }
            v
        }
    }
}

/// Where the side slope from `hinge` meets the ground going outward on `side`. Returns the
/// vertices of the slope line between the hinge and the daylight point, then the point.
fn trace_daylight(ground: &[P], hinge: P, side: Side, rule: &DaylightRule) -> Result<(Vec<P>, DaylightPoint), String> {
    let sg = side.sign();
    let g = interpolate(ground, hinge.0).ok_or("the outer end of the template is off the ground surface")?;
    let d = hinge.1 - g;
    if d.abs() <= 1e-9 {
        return Ok((vec![], DaylightPoint { offset_m: hinge.0, elevation_m: hinge.1, kind: SlopeKind::AtGround }));
    }
    let cut = d < 0.0;
    let kind = if cut { SlopeKind::Cut } else { SlopeKind::Fill };
    let extent = ground.iter().map(|p| sg * (p.0 - hinge.0)).fold(0.0, f64::max);
    let poly = slope_polyline(rule, cut, extent + 1.0);
    let gz = |u: f64| interpolate(ground, hinge.0 + sg * u);
    let pz = |u: f64| -> f64 {
        let i = poly.partition_point(|p| p.0 <= u).clamp(1, poly.len() - 1);
        let (p, q) = (poly[i - 1], poly[i]);
        p.1 + (q.1 - p.1) * ((u - p.0) / (q.0 - p.0))
    };
    let mut us: Vec<f64> = poly.iter().map(|p| p.0).chain(ground.iter().map(|p| sg * (p.0 - hinge.0))).filter(|u| *u >= 0.0 && *u <= extent).collect();
    us.sort_by(f64::total_cmp);
    us.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    let mut prev = (0.0, d);
    for &u in us.iter().skip(1) {
        let Some(gv) = gz(u) else { continue };
        let gap = hinge.1 + pz(u) - gv;
        if gap == 0.0 || (gap > 0.0) != (d > 0.0) {
            let t = prev.1 / (prev.1 - gap);
            let ux = prev.0 + t * (u - prev.0);
            let mut pts: Vec<P> = poly.iter().filter(|p| p.0 > 1e-12 && p.0 < ux - 1e-12).map(|p| (hinge.0 + sg * p.0, hinge.1 + p.1)).collect();
            let end = (hinge.0 + sg * ux, hinge.1 + pz(ux));
            pts.push(end);
            return Ok((pts, DaylightPoint { offset_m: end.0, elevation_m: end.1, kind }));
        }
        prev = (u, gap);
    }
    Err(format!("the {} slope leaves the ground surface before meeting it", if cut { "cut" } else { "fill" }))
}

struct SideOut {
    points: Vec<DesignPoint>,
    links: Vec<DesignLink>,
    shapes: Vec<DesignShape>,
    /// Earthworks line from the baseline outward.
    chain: Vec<P>,
    daylight: Option<DaylightPoint>,
    open: bool,
}

fn push_dedup(chain: &mut Vec<P>, p: P) {
    if chain.last() != Some(&p) {
        chain.push(p);
    }
}

fn lay_side(parts: &[Subassembly], side: Side, ctx: &EvalContext, base: P, notes: &mut Vec<String>) -> Result<SideOut, String> {
    let sg = side.sign();
    let abs = |start: P, p: P| (start.0 + sg * p.0, start.1 + p.1);
    let mut out = SideOut { points: vec![], links: vec![], shapes: vec![], chain: vec![], daylight: None, open: false };
    let mut cur = base;
    for p in parts {
        let l = p.eval(ctx, side, cur)?;
        if let Some(rule) = &l.terminal {
            out.open = true;
            // The side slope starts from the finished outer end; the step from the datum up to
            // it is vertical.
            push_dedup(&mut out.chain, cur);
            match ctx.ground {
                Some(ground) => match trace_daylight(ground, cur, side, rule) {
                    Ok((pts, dp)) => {
                        let mut line = vec![cur];
                        line.extend(pts.iter().copied());
                        for q in &pts {
                            push_dedup(&mut out.chain, *q);
                        }
                        if dp.kind != SlopeKind::AtGround {
                            let kc = if dp.kind == SlopeKind::Cut { "Daylight_Cut" } else { "Daylight_Fill" };
                            out.links.push(DesignLink { codes: codes(&["Daylight", kc]), points: line, side: Some(side), subassembly: p.name.clone() });
                        }
                        out.points.push(DesignPoint { offset_m: dp.offset_m, elevation_m: dp.elevation_m, codes: codes(&["Daylight"]), side: Some(side), subassembly: p.name.clone() });
                        out.daylight = Some(dp);
                    }
                    Err(why) => notes.push(format!("No daylight on the {}: {why}.", side.word())),
                },
                None => notes.push("The centreline is off the ground surface.".to_string()),
            }
            break;
        }
        for q in &l.datum {
            push_dedup(&mut out.chain, abs(cur, *q));
        }
        for (q, c) in l.points {
            let a = abs(cur, q);
            out.points.push(DesignPoint { offset_m: a.0, elevation_m: a.1, codes: c, side: Some(side), subassembly: p.name.clone() });
        }
        for (c, pts) in l.links {
            out.links.push(DesignLink { codes: c, points: pts.iter().map(|q| abs(cur, *q)).collect(), side: Some(side), subassembly: p.name.clone() });
        }
        for (code, poly, class) in l.shapes {
            let polygon: Vec<P> = poly.iter().map(|q| abs(cur, *q)).collect();
            out.shapes.push(DesignShape { area_m2: polygon_area(&polygon), code, polygon, side: Some(side), subassembly: p.name.clone(), classification: class });
        }
        cur = abs(cur, l.end);
    }
    if out.chain.is_empty() {
        out.chain.push(cur);
    }
    if !out.open {
        push_dedup(&mut out.chain, cur);
    }
    Ok(out)
}

/// Lays the assembly on a station.
pub(crate) fn assemble(asm: &Assembly, ctx: &EvalContext) -> Result<Assembled, String> {
    asm.check()?;
    let base = (asm.baseline_offset_m, ctx.centre_z + asm.baseline_elevation_m);
    let mut notes = Vec::new();
    let left = lay_side(&asm.left, Side::Left, ctx, base, &mut notes)?;
    let right = lay_side(&asm.right, Side::Right, ctx, base, &mut notes)?;
    notes.dedup();
    let mut design = DesignGeometry::default();
    let centre = DesignPoint { offset_m: base.0, elevation_m: base.1, codes: codes(&["CL"]), side: None, subassembly: asm.name.clone() };
    let mut lp = left.points;
    lp.reverse();
    let is_daylight = |p: &DesignPoint| p.codes.iter().any(|c| c == "Daylight");
    let (ld, lk): (Vec<_>, Vec<_>) = lp.into_iter().partition(is_daylight);
    let (rd, rk): (Vec<_>, Vec<_>) = right.points.into_iter().partition(is_daylight);
    design.points.extend(lk);
    design.points.push(centre);
    design.points.extend(rk);
    design.points.extend(ld);
    design.points.extend(rd);
    design.links.extend(left.links);
    design.links.extend(right.links);
    design.shapes.extend(left.shapes);
    design.shapes.extend(right.shapes);
    let mut line: Vec<P> = left.chain.into_iter().rev().collect();
    line.extend(right.chain);
    line.dedup();
    design.earthworks = line;
    Ok(Assembled { design, left_daylight: left.daylight, right_daylight: right.daylight, left_open: left.open, right_open: right.open, notes })
}

// ─── the library ────────────────────────────────────────────────────────────────────────────

/// Constructors for the generic parts. Each takes **every dimension as an argument**; none has a
/// default. Codes are names fixed by the constructor and can be renamed with
/// [`Subassembly::rename_code`].
pub mod library {
    use super::*;

    fn fixed_strip(width_m: f64, slope: Slope, layers: Vec<Layer>, point_code: &str) -> Strip {
        Strip { width: Dim::Fixed(width_m), slope, follows_superelevation: false, layers, point_code: point_code.into() }
    }

    /// A lane with layers from the top down (the shape codes and depths are entered). Its outer
    /// point is coded `ETW`.
    pub fn lane(name: &str, width_m: f64, slope: Slope, layers: Vec<Layer>) -> Subassembly {
        Subassembly::new(name, SubKind::Lane(fixed_strip(width_m, slope, layers, "ETW")))
    }
    /// A lane that follows the superelevation of its side.
    pub fn lane_superelevated(name: &str, width_m: f64, slope: Slope, layers: Vec<Layer>) -> Subassembly {
        let mut s = fixed_strip(width_m, slope, layers, "ETW");
        s.follows_superelevation = true;
        Subassembly::new(name, SubKind::Lane(s))
    }
    /// A lane whose width runs to an offset target (lane widening).
    pub fn lane_to_offset(name: &str, target: OffsetTarget, slope: Slope, layers: Vec<Layer>) -> Subassembly {
        let mut s = fixed_strip(1.0, slope, layers, "ETW");
        s.width = Dim::ToOffset(target);
        Subassembly::new(name, SubKind::Lane(s))
    }
    pub fn shoulder(name: &str, width_m: f64, slope: Slope, layers: Vec<Layer>) -> Subassembly {
        Subassembly::new(name, SubKind::Shoulder(fixed_strip(width_m, slope, layers, "ESh")))
    }
    pub fn shoulder_superelevated(name: &str, width_m: f64, slope: Slope, layers: Vec<Layer>) -> Subassembly {
        let mut s = fixed_strip(width_m, slope, layers, "ESh");
        s.follows_superelevation = true;
        Subassembly::new(name, SubKind::Shoulder(s))
    }
    pub fn footway(name: &str, width_m: f64, slope: Slope, layers: Vec<Layer>) -> Subassembly {
        Subassembly::new(name, SubKind::Footway(fixed_strip(width_m, slope, layers, "EFw")))
    }
    pub fn verge(name: &str, width_m: f64, slope: Slope) -> Subassembly {
        Subassembly::new(name, SubKind::Verge(fixed_strip(width_m, slope, vec![], "EVg")))
    }
    pub fn kerb(name: &str, height_m: f64, width_m: f64, bed_depth_m: f64) -> Subassembly {
        Subassembly::new(name, SubKind::Kerb(Block { height_m, width_m, bed_depth_m, point_code: "Kerb_Back".into() }))
    }
    pub fn kerb_and_channel(name: &str, channel_width_m: f64, channel_slope: Slope, kerb_height_m: f64, kerb_width_m: f64, bed_depth_m: f64) -> Subassembly {
        Subassembly::new(
            name,
            SubKind::KerbAndChannel {
                channel: fixed_strip(channel_width_m, channel_slope, vec![], "Channel_Edge"),
                kerb: Block { height_m: kerb_height_m, width_m: kerb_width_m, bed_depth_m, point_code: "Kerb_Back".into() },
            },
        )
    }
    pub fn median_raised(name: &str, height_m: f64, half_width_m: f64, bed_depth_m: f64) -> Subassembly {
        Subassembly::new(name, SubKind::MedianRaised(Block { height_m, width_m: half_width_m, bed_depth_m, point_code: "Median_Edge".into() }))
    }
    pub fn median_depressed(name: &str, half_bottom_width_m: f64, depth_m: f64, side_run_per_rise: f64) -> Subassembly {
        Subassembly::new(name, SubKind::MedianDepressed(DepressedMedian { half_bottom_width_m, depth_m, side_run_per_rise }))
    }
    pub fn side_ditch_v(name: &str, depth_m: f64, inner_run_per_rise: f64, outer_run_per_rise: f64) -> Subassembly {
        Subassembly::new(name, SubKind::SideDitch(Ditch { shape: DitchShape::V, depth_m, inner_run_per_rise, outer_run_per_rise }))
    }
    pub fn side_ditch_trapezoid(name: &str, depth_m: f64, bottom_width_m: f64, inner_run_per_rise: f64, outer_run_per_rise: f64) -> Subassembly {
        Subassembly::new(name, SubKind::SideDitch(Ditch { shape: DitchShape::Trapezoid { bottom_width_m }, depth_m, inner_run_per_rise, outer_run_per_rise }))
    }
    pub fn lined_drain(name: &str, ditch: Ditch, lining_thickness_m: f64) -> Subassembly {
        Subassembly::new(name, SubKind::LinedDrain { ditch, lining_thickness_m })
    }
    pub fn retaining_edge(name: &str, thickness_m: f64, depth_m: f64) -> Subassembly {
        Subassembly::new(name, SubKind::RetainingEdge(Wall { thickness_m, depth_m }))
    }
    pub fn link_width_slope(name: &str, codes_: &[&str], point_code: &str, width_m: f64, slope: Slope) -> Subassembly {
        Subassembly::new(name, SubKind::Link(GenericLink { codes: codes(codes_), point_code: point_code.into(), mode: LinkMode::WidthSlope { width: Dim::Fixed(width_m), slope } }))
    }
    pub fn link_to_elevation(name: &str, codes_: &[&str], point_code: &str, target: ElevationTarget, slope: Slope) -> Subassembly {
        Subassembly::new(name, SubKind::Link(GenericLink { codes: codes(codes_), point_code: point_code.into(), mode: LinkMode::ToElevation { target, slope } }))
    }
    pub fn link_to_offset(name: &str, codes_: &[&str], point_code: &str, target: OffsetTarget, slope: Slope) -> Subassembly {
        Subassembly::new(name, SubKind::Link(GenericLink { codes: codes(codes_), point_code: point_code.into(), mode: LinkMode::ToOffset { target, slope } }))
    }
    /// A link whose end is fixed in plan and in level by two targets together.
    pub fn link_to_offset_and_elevation(name: &str, codes_: &[&str], point_code: &str, offset: OffsetTarget, elevation: ElevationTarget) -> Subassembly {
        Subassembly::new(name, SubKind::Link(GenericLink { codes: codes(codes_), point_code: point_code.into(), mode: LinkMode::ToOffsetAndElevation { offset, elevation } }))
    }
    /// The side slope to the ground, without benching.
    pub fn daylight(cut_run_per_rise: f64, fill_run_per_rise: f64) -> Subassembly {
        Subassembly::new("Daylight", SubKind::Daylight(DaylightRule { cut_run_per_rise, fill_run_per_rise, bench_in_cut: None, bench_in_fill: None }))
    }
    /// The side slope to the ground with benches in cut, in fill, or both.
    pub fn daylight_benched(cut_run_per_rise: f64, fill_run_per_rise: f64, bench_in_cut: Option<Bench>, bench_in_fill: Option<Bench>) -> Subassembly {
        Subassembly::new("Daylight", SubKind::Daylight(DaylightRule { cut_run_per_rise, fill_run_per_rise, bench_in_cut, bench_in_fill }))
    }
}
