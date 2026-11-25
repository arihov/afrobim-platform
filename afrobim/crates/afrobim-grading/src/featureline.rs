// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Feature lines: 3D polylines with line and arc segments in plan.
//!
//! # Model
//!
//! A feature line is an ordered list of vertices. Each vertex is either a **PI** (a point of
//! intersection: it defines the plan geometry) or an **elevation point** (it lies on the plan
//! geometry between two PIs and only carries an elevation). The plan geometry between two
//! consecutive PIs is a *span*: a straight line, or a circular arc when the first PI of the span
//! has a non-zero **bulge** (the DXF convention: `bulge = tan(theta / 4)` where `theta` is the
//! signed included angle of the arc, positive counter-clockwise from the PI to the next PI).
//! A closed line has a closing span from the last PI back to the first vertex, which must be a PI.
//!
//! The **station** of a vertex is the plan distance along the line from the first vertex (arc
//! length along arcs). Elevation is linear in station between consecutive vertices, so the grade
//! between two vertices is constant: `grade = (z2 - z1) / (s2 - s1)`.
//!
//! All plan arithmetic inside a span is done relative to the span's start point, so true UTM
//! eastings and northings keep their precision, and results are returned in source coordinates.

use afrobim_corridor::Tin;
use serde::{Deserialize, Serialize};

use crate::error::{finite, GradingError, Result};
use crate::geom::*;

const EPS: f64 = 1e-9;

/// What a vertex of a feature line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VertexKind {
    /// Point of intersection: defines the plan geometry.
    Pi,
    /// Elevation point: lies on the span between two PIs, only carries an elevation.
    Elevation,
}

/// One vertex of a feature line, source coordinates in metres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub kind: VertexKind,
    /// For a PI: bulge of the span that starts here (0 = straight). Ignored for elevation points.
    #[serde(default)]
    pub bulge: f64,
}

impl Vertex {
    pub fn pi(x: f64, y: f64, z: f64) -> Vertex {
        Vertex { x, y, z, kind: VertexKind::Pi, bulge: 0.0 }
    }
    fn elev(x: f64, y: f64, z: f64) -> Vertex {
        Vertex { x, y, z, kind: VertexKind::Elevation, bulge: 0.0 }
    }
    fn xy(&self) -> P2 {
        [self.x, self.y]
    }
}

/// The elevation step of a stepped offset.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElevationStep {
    /// The offset line is this much higher (positive) or lower (negative), m.
    Difference(f64),
    /// The offset line rises (positive) or falls (negative) away from the source at this grade in
    /// percent over the plan offset distance: `dz = grade / 100 * |offset|`.
    GradePercent(f64),
}

/// Which end of a graded run stays where it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hold {
    Start,
    End,
}

/// An arc of a span. `centre` is relative to the span start.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ArcGeo {
    pub centre: P2,
    pub radius: f64,
    pub a0: f64,
    pub sweep: f64,
}

/// The plan geometry between two PIs.
#[derive(Debug, Clone)]
pub(crate) struct Span {
    pub a: P2,
    pub b: P2,
    pub length: f64,
    pub arc: Option<ArcGeo>,
}

impl Span {
    /// Builds the span. For an arc: with chord `c` and included angle `theta = 4 atan(bulge)`,
    /// radius `r = |c| / (2 sin(|theta|/2))`, and the centre lies at distance `r cos(theta/2)`
    /// from the chord midpoint on the left of the chord for a counter-clockwise arc and on the
    /// right for a clockwise one.
    pub fn new(a: P2, b: P2, bulge: f64) -> Span {
        let c = sub(b, a);
        let l = len(c);
        if bulge.abs() < 1e-12 || l == 0.0 {
            return Span { a, b, length: l, arc: None };
        }
        let theta = 4.0 * bulge.atan();
        let r = l / (2.0 * (theta.abs() / 2.0).sin());
        let d = r * (theta / 2.0).cos();
        let centre = add(scale(c, 0.5), scale(left(unit(c)), d * theta.signum()));
        let a0 = (-centre[1]).atan2(-centre[0]);
        Span { a, b, length: r * theta.abs(), arc: Some(ArcGeo { centre, radius: r, a0, sweep: theta }) }
    }

    fn angle_at(&self, g: &ArcGeo, s: f64) -> f64 {
        g.a0 + g.sweep.signum() * s / g.radius
    }

    pub fn point_at(&self, s: f64) -> P2 {
        if s <= 1e-12 {
            return self.a;
        }
        if s >= self.length - 1e-12 {
            return self.b;
        }
        match &self.arc {
            None => add(self.a, scale(unit(sub(self.b, self.a)), s)),
            Some(g) => {
                let ang = self.angle_at(g, s);
                add(self.a, add(g.centre, [g.radius * ang.cos(), g.radius * ang.sin()]))
            }
        }
    }

    pub fn tangent_at(&self, s: f64) -> P2 {
        match &self.arc {
            None => unit(sub(self.b, self.a)),
            Some(g) => {
                let ang = self.angle_at(g, s.clamp(0.0, self.length));
                if g.sweep > 0.0 {
                    [-ang.sin(), ang.cos()]
                } else {
                    [ang.sin(), -ang.cos()]
                }
            }
        }
    }

    /// Station along the span of the point of the span nearest to `p`.
    pub fn station_of(&self, p: P2) -> f64 {
        match &self.arc {
            None => dot(sub(p, self.a), unit(sub(self.b, self.a))).clamp(0.0, self.length),
            Some(g) => {
                let l = sub(sub(p, self.a), g.centre);
                let ang = l[1].atan2(l[0]);
                let tau = std::f64::consts::TAU;
                let mut d = (g.sweep.signum() * (ang - g.a0)).rem_euclid(tau);
                let total = g.sweep.abs();
                if d > total + (tau - total) / 2.0 {
                    d = 0.0;
                }
                g.radius * d.min(total)
            }
        }
    }
}

/// One span with the vertices that belong to it.
#[derive(Debug, Clone)]
pub(crate) struct SpanRec {
    pub start: usize,
    /// Index of the PI that ends the span (0 for the closing span of a closed line).
    pub end: usize,
    pub span: Span,
    pub s0: f64,
    /// Indices of the elevation points inside the span.
    pub elev: Vec<usize>,
}

/// Spans and stations of a feature line.
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    pub spans: Vec<SpanRec>,
    pub station: Vec<f64>,
    pub length: f64,
}

/// A feature line. See the module documentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureLine {
    pub name: String,
    /// Priority of the line's style. Where two lines in a site cross, the higher number wins.
    #[serde(default)]
    pub style_priority: i32,
    /// A counter that grows each time the line is edited (the site keeps it). Where two lines
    /// cross and the style priorities are equal, the larger number wins.
    #[serde(default)]
    pub last_edited: u64,
    #[serde(default)]
    pub closed: bool,
    pub vertices: Vec<Vertex>,
}

/// A point of a flattened line.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlatPoint {
    pub station: f64,
    pub p: [f64; 3],
}

/// One row of the grade and elevation table: the segment from vertex `from` to vertex `to`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradeRow {
    pub from: usize,
    pub to: usize,
    pub station_from: f64,
    pub station_to: f64,
    pub z_from: f64,
    pub z_to: f64,
    /// Plan length along the line (arc length on an arc), m.
    pub plan_length: f64,
    pub dz: f64,
    /// Rise over run in percent (positive rising along the line).
    pub grade_percent: f64,
    /// Horizontal metres per vertical metre, or `None` when the segment is level.
    pub slope_h_per_v: Option<f64>,
    /// Length along the slope, m.
    pub length_3d: f64,
    /// True when the segment lies on an arc in plan.
    pub on_arc: bool,
}

/// Result of draping a line on a surface.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DrapeReport {
    pub vertices_set: usize,
    pub points_added: usize,
    pub warnings: Vec<String>,
}

/// Result of joining two lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JoinResult {
    pub line: FeatureLine,
    /// Elevation difference between the two ends at the joint (the first line's value is kept), m.
    pub elevation_mismatch: f64,
    /// Plan distance between the two ends that were joined, m.
    pub gap: f64,
}

/// Result of a fillet.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FilletReport {
    /// Deflection angle at the corner, radians.
    pub deflection: f64,
    /// Tangent length `R tan(deflection / 2)`, m.
    pub tangent_length: f64,
    pub arc_length: f64,
    pub elevation_points_removed: usize,
}

/// Tolerances for weeding. All three are required inputs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeedTolerances {
    /// A PI may be removed when it is within this plan distance of the chord of its neighbours, m.
    pub plan_distance: f64,
    /// ... and its elevation is within this of the elevation interpolated on that chord, m.
    pub elevation: f64,
    /// ... and the resulting span is no longer than this, m.
    pub max_span_length: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WeedReport {
    pub pis_removed: usize,
    pub elevation_points_removed: usize,
}

/// One row of a quick elevation edit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QuickEdit {
    pub vertex: usize,
    /// New elevation of the vertex.
    pub elevation: Option<f64>,
    /// Grade in percent from this vertex to the next one; the next vertex moves, this one is held.
    pub grade_to_next_percent: Option<f64>,
}

impl FeatureLine {
    /// An open line of straight spans through `points` `[x, y, z]`; every point is a PI.
    pub fn from_points(name: impl Into<String>, points: &[[f64; 3]]) -> Result<FeatureLine> {
        FeatureLine::build(name.into(), points, false)
    }

    /// A closed line of straight spans (do not repeat the first point).
    pub fn closed_from_points(name: impl Into<String>, points: &[[f64; 3]]) -> Result<FeatureLine> {
        FeatureLine::build(name.into(), points, true)
    }

    fn build(name: String, points: &[[f64; 3]], closed: bool) -> Result<FeatureLine> {
        let min = if closed { 3 } else { 2 };
        if points.len() < min {
            return Err(GradingError::Invalid(format!("a feature line needs at least {min} points")));
        }
        for p in points {
            for v in p {
                finite("a coordinate", *v)?;
            }
        }
        for w in points.windows(2) {
            if len(sub([w[1][0], w[1][1]], [w[0][0], w[0][1]])) < EPS {
                return Err(GradingError::Invalid("two consecutive points coincide".into()));
            }
        }
        Ok(FeatureLine { name, style_priority: 0, last_edited: 0, closed, vertices: points.iter().map(|p| Vertex::pi(p[0], p[1], p[2])).collect() })
    }

    /// Sets the bulge of each span, in order (one per span: n - 1 for an open line, n for a
    /// closed one, counting PIs only; the line must have no elevation points yet).
    pub fn with_bulges(mut self, bulges: &[f64]) -> Result<FeatureLine> {
        let pis: Vec<usize> = self.pi_indices();
        let need = if self.closed { pis.len() } else { pis.len() - 1 };
        if bulges.len() != need {
            return Err(GradingError::Invalid(format!("{need} bulges are needed, {} given", bulges.len())));
        }
        for (k, b) in bulges.iter().enumerate() {
            finite("a bulge", *b)?;
            self.vertices[pis[k]].bulge = *b;
        }
        Ok(self)
    }

    /// Sets the style priority.
    pub fn with_priority(mut self, priority: i32) -> FeatureLine {
        self.style_priority = priority;
        self
    }

    /// Indices of the PIs.
    pub fn pi_indices(&self) -> Vec<usize> {
        self.vertices.iter().enumerate().filter(|(_, v)| v.kind == VertexKind::Pi).map(|(i, _)| i).collect()
    }

    pub(crate) fn layout(&self) -> Result<Layout> {
        let n = self.vertices.len();
        if n < 2 || self.vertices[0].kind != VertexKind::Pi || (!self.closed && self.vertices[n - 1].kind != VertexKind::Pi) {
            return Err(GradingError::Invalid("a feature line must start (and, if open, end) with a PI".into()));
        }
        let pis = self.pi_indices();
        let count = if self.closed { pis.len() } else { pis.len() - 1 };
        let mut spans = Vec::with_capacity(count);
        let mut station = vec![0.0; n];
        let mut s0 = 0.0;
        for k in 0..count {
            let (i, j) = (pis[k], if k + 1 < pis.len() { pis[k + 1] } else { 0 });
            let span = Span::new(self.vertices[i].xy(), self.vertices[j].xy(), self.vertices[i].bulge);
            let hi = if k + 1 < pis.len() { j } else { n };
            let elev: Vec<usize> = (i + 1..hi).collect();
            station[i] = s0;
            let mut prev = s0;
            for &e in &elev {
                let st = (s0 + span.station_of(self.vertices[e].xy())).clamp(prev, s0 + span.length);
                station[e] = st;
                prev = st;
            }
            spans.push(SpanRec { start: i, end: j, span: span.clone(), s0, elev });
            s0 += span.length;
        }
        if !self.closed {
            station[n - 1] = s0;
        }
        Ok(Layout { spans, station, length: s0 })
    }

    /// Plan length of the line, m (the closing span included for a closed line).
    pub fn length(&self) -> Result<f64> {
        Ok(self.layout()?.length)
    }

    /// Stations of all vertices, m.
    pub fn stations(&self) -> Result<Vec<f64>> {
        Ok(self.layout()?.station)
    }

    pub(crate) fn z_at_layout(&self, lay: &Layout, s: f64) -> f64 {
        let mut pts: Vec<(f64, f64)> = lay.station.iter().zip(&self.vertices).map(|(s, v)| (*s, v.z)).collect();
        let mut s = s;
        if self.closed {
            pts.push((lay.length, self.vertices[0].z));
            s = s.rem_euclid(lay.length.max(1e-300));
        } else {
            s = s.clamp(0.0, lay.length);
        }
        let k = pts.partition_point(|p| p.0 <= s).clamp(1, pts.len() - 1);
        let (a, b) = (pts[k - 1], pts[k]);
        if b.0 - a.0 <= 0.0 {
            return a.1;
        }
        a.1 + (b.1 - a.1) * (s - a.0) / (b.0 - a.0)
    }

    /// Elevation at a station, m (linear between vertices; the end values outside an open line).
    pub fn z_at(&self, station: f64) -> Result<f64> {
        let lay = self.layout()?;
        Ok(self.z_at_layout(&lay, station))
    }

    fn rec_at<'a>(&self, lay: &'a Layout, s: f64) -> &'a SpanRec {
        let s = if self.closed { s.rem_euclid(lay.length.max(1e-300)) } else { s.clamp(0.0, lay.length) };
        lay.spans.iter().rev().find(|r| r.s0 <= s + 1e-12).unwrap_or(&lay.spans[0])
    }

    /// Plan position at a station, source coordinates.
    pub fn xy_at(&self, station: f64) -> Result<[f64; 2]> {
        let lay = self.layout()?;
        Ok(self.xy_at_layout(&lay, station))
    }

    pub(crate) fn xy_at_layout(&self, lay: &Layout, station: f64) -> P2 {
        let r = self.rec_at(lay, station);
        let s = if self.closed { station.rem_euclid(lay.length.max(1e-300)) } else { station.clamp(0.0, lay.length) };
        r.span.point_at(s - r.s0)
    }

    /// 3D point at a station.
    pub fn point_at(&self, station: f64) -> Result<[f64; 3]> {
        let lay = self.layout()?;
        let p = self.xy_at_layout(&lay, station);
        Ok([p[0], p[1], self.z_at_layout(&lay, station)])
    }

    /// The line as a polyline: every vertex, and extra points on arcs so that no piece turns by
    /// more than `facet_angle` (radians). A closed line is not closed by repeating its first point.
    pub fn flatten(&self, facet_angle: f64) -> Result<Vec<FlatPoint>> {
        if !(facet_angle > 0.0 && facet_angle.is_finite()) {
            return Err(GradingError::Invalid("facet_angle must be positive".into()));
        }
        let lay = self.layout()?;
        let mut out = Vec::new();
        for rec in &lay.spans {
            let mut marks: Vec<f64> = vec![rec.s0];
            marks.extend(rec.elev.iter().map(|&e| lay.station[e]));
            marks.push(rec.s0 + rec.span.length);
            for w in marks.windows(2) {
                let n = match &rec.span.arc {
                    Some(g) => (((w[1] - w[0]) / g.radius) / facet_angle).ceil().max(1.0) as usize,
                    None => 1,
                };
                for k in 0..n {
                    let s = w[0] + (w[1] - w[0]) * k as f64 / n as f64;
                    let p = self.xy_at_layout(&lay, s);
                    out.push(FlatPoint { station: s, p: [p[0], p[1], self.z_at_layout(&lay, s)] });
                }
            }
        }
        if !self.closed {
            let p = self.xy_at_layout(&lay, lay.length);
            out.push(FlatPoint { station: lay.length, p: [p[0], p[1], self.z_at_layout(&lay, lay.length)] });
        }
        Ok(out)
    }

    /// Grade and elevation table, one row per segment between consecutive vertices.
    pub fn grade_table(&self) -> Result<Vec<GradeRow>> {
        let lay = self.layout()?;
        let n = self.vertices.len();
        let segs = if self.closed { n } else { n - 1 };
        let mut rows = Vec::with_capacity(segs);
        for i in 0..segs {
            let j = (i + 1) % n;
            let (s1, s2) = (lay.station[i], if j == 0 { lay.length } else { lay.station[j] });
            let (z1, z2) = (self.vertices[i].z, self.vertices[j].z);
            let pl = s2 - s1;
            let dz = z2 - z1;
            let on_arc = lay.spans.iter().any(|r| r.span.arc.is_some() && s1 >= r.s0 - 1e-9 && s2 <= r.s0 + r.span.length + 1e-9);
            rows.push(GradeRow {
                from: i,
                to: j,
                station_from: s1,
                station_to: s2,
                z_from: z1,
                z_to: z2,
                plan_length: pl,
                dz,
                grade_percent: if pl > 0.0 { 100.0 * dz / pl } else { 0.0 },
                slope_h_per_v: if dz != 0.0 { Some(pl / dz.abs()) } else { None },
                length_3d: pl.hypot(dz),
                on_arc,
            });
        }
        Ok(rows)
    }

    // ---------------------------------------------------------------- creation from data

    /// A line draped on a surface: plan polyline `[x, y]` (straight spans), elevations from the
    /// surface at each vertex, and (when `add_points`) an elevation point wherever a span crosses
    /// a triangle edge of the surface, so the line follows the ground between the vertices.
    pub fn from_polyline_on_surface(name: impl Into<String>, plan: &[[f64; 2]], tin: &Tin, add_points: bool) -> Result<(FeatureLine, DrapeReport)> {
        let pts: Vec<[f64; 3]> = plan.iter().map(|p| [p[0], p[1], 0.0]).collect();
        let mut fl = FeatureLine::from_points(name, &pts)?;
        let rep = fl.set_elevations_from_surface(tin, add_points)?;
        Ok((fl, rep))
    }

    /// A line from sampled alignment and profile data (plain arrays).
    ///
    /// `plan` are points `[x, y]` along the alignment (already sampled, for example at a
    /// spacing and at the ends of curves), `stations` the chainage of each, ascending, and
    /// `profile` the vertical profile as `[station, elevation]` points joined by straight
    /// grades (a profile with vertical curves must be sampled finely by the caller). Each plan
    /// sample becomes a PI whose elevation is read from the profile; each profile point that
    /// falls strictly between two plan samples becomes an elevation point on that span.
    pub fn from_alignment_profile(name: impl Into<String>, plan: &[[f64; 2]], stations: &[f64], profile: &[[f64; 2]]) -> Result<FeatureLine> {
        if plan.len() != stations.len() || plan.len() < 2 || profile.len() < 2 {
            return Err(GradingError::Invalid("plan and stations must match and profile needs two points".into()));
        }
        if stations.windows(2).any(|w| w[1] <= w[0]) || profile.windows(2).any(|w| w[1][0] <= w[0][0]) {
            return Err(GradingError::Invalid("stations must be strictly ascending".into()));
        }
        if stations[0] < profile[0][0] - EPS || stations[stations.len() - 1] > profile[profile.len() - 1][0] + EPS {
            return Err(GradingError::Invalid("the profile does not cover the stations of the plan samples".into()));
        }
        let zp = |s: f64| -> f64 {
            let k = profile.partition_point(|p| p[0] <= s).clamp(1, profile.len() - 1);
            let (a, b) = (profile[k - 1], profile[k]);
            a[1] + (b[1] - a[1]) * (s - a[0]) / (b[0] - a[0])
        };
        let mut vertices = Vec::new();
        for i in 0..plan.len() {
            vertices.push(Vertex::pi(plan[i][0], plan[i][1], zp(stations[i])));
            if i + 1 < plan.len() {
                for pv in profile.iter().filter(|p| p[0] > stations[i] + EPS && p[0] < stations[i + 1] - EPS) {
                    let f = (pv[0] - stations[i]) / (stations[i + 1] - stations[i]);
                    let d = sub(plan[i + 1], plan[i]);
                    let q = add(plan[i], scale(d, f));
                    vertices.push(Vertex::elev(q[0], q[1], pv[1]));
                }
            }
        }
        Ok(FeatureLine { name: name.into(), style_priority: 0, last_edited: 0, closed: false, vertices })
    }

    // ---------------------------------------------------------------- elevation edits

    /// Sets the elevation of a vertex.
    pub fn set_elevation(&mut self, vertex: usize, z: f64) -> Result<()> {
        finite("elevation", z)?;
        self.vertices.get_mut(vertex).ok_or_else(|| GradingError::NotFound(format!("vertex {vertex}")))?.z = z;
        Ok(())
    }

    /// Sets the grade (percent, rise over run along the line) between vertices `a < b`. With
    /// `Hold::Start` vertex `b` moves, with `Hold::End` vertex `a` moves. Vertices between them are
    /// re-interpolated on the new straight grade.
    pub fn set_grade(&mut self, a: usize, b: usize, grade_percent: f64, hold: Hold) -> Result<()> {
        finite("grade", grade_percent)?;
        if a >= b || b >= self.vertices.len() {
            return Err(GradingError::Invalid("need a < b, both vertices of the line".into()));
        }
        let lay = self.layout()?;
        let ds = lay.station[b] - lay.station[a];
        if ds <= 0.0 {
            return Err(GradingError::Geometry("the two vertices are at the same station".into()));
        }
        match hold {
            Hold::Start => self.vertices[b].z = self.vertices[a].z + grade_percent / 100.0 * ds,
            Hold::End => self.vertices[a].z = self.vertices[b].z - grade_percent / 100.0 * ds,
        }
        let (za, zb) = (self.vertices[a].z, self.vertices[b].z);
        for k in a + 1..b {
            self.vertices[k].z = za + (zb - za) * (lay.station[k] - lay.station[a]) / ds;
        }
        Ok(())
    }

    /// Sets the slope between two vertices as a ratio of `h_per_v` horizontal to 1 vertical.
    /// `rising` says whether the line rises from `a` to `b`.
    pub fn set_slope_ratio(&mut self, a: usize, b: usize, h_per_v: f64, rising: bool, hold: Hold) -> Result<()> {
        if !(h_per_v > 0.0 && h_per_v.is_finite()) {
            return Err(GradingError::Invalid("the slope ratio must be positive".into()));
        }
        let g = 100.0 / h_per_v;
        self.set_grade(a, b, if rising { g } else { -g }, hold)
    }

    /// Raises (positive) or lowers every vertex by `delta`, m.
    pub fn raise_lower(&mut self, delta: f64) -> Result<()> {
        finite("delta", delta)?;
        self.vertices.iter_mut().for_each(|v| v.z += delta);
        Ok(())
    }

    /// Raises or lowers the vertices `from..=to` by `delta`, m.
    pub fn raise_lower_range(&mut self, from: usize, to: usize, delta: f64) -> Result<()> {
        finite("delta", delta)?;
        if from > to || to >= self.vertices.len() {
            return Err(GradingError::Invalid("vertex range out of bounds".into()));
        }
        self.vertices[from..=to].iter_mut().for_each(|v| v.z += delta);
        Ok(())
    }

    /// Quick elevation edit: a list of rows applied in order, each setting a vertex elevation
    /// and/or the grade to the next vertex. Returns the grade table afterwards.
    pub fn quick_edit(&mut self, rows: &[QuickEdit]) -> Result<Vec<GradeRow>> {
        for r in rows {
            if let Some(z) = r.elevation {
                self.set_elevation(r.vertex, z)?;
            }
            if let Some(g) = r.grade_to_next_percent {
                self.set_grade(r.vertex, r.vertex + 1, g, Hold::Start)?;
            }
        }
        self.grade_table()
    }

    /// Sets every vertex elevation from a surface (elevation points included). With `add_points`,
    /// elevation points are added where straight spans cross triangle edges. Arc spans are not
    /// densified (limit stated in the README). A vertex outside the surface is an error.
    pub fn set_elevations_from_surface(&mut self, tin: &Tin, add_points: bool) -> Result<DrapeReport> {
        let mut rep = DrapeReport::default();
        for v in self.vertices.iter_mut() {
            v.z = tin.elevation_at(v.x, v.y).ok_or(GradingError::OutsideSurface(v.x, v.y))?;
            rep.vertices_set += 1;
        }
        if add_points {
            let lay = self.layout()?;
            let mut adds: Vec<(f64, f64)> = vec![];
            for rec in &lay.spans {
                if rec.span.arc.is_some() {
                    rep.warnings.push(format!("arc span at station {:.3} was not densified", rec.s0));
                    continue;
                }
                let runs = tin.section((rec.span.a[0], rec.span.a[1]), (rec.span.b[0], rec.span.b[1]));
                if runs.len() != 1 {
                    rep.warnings.push(format!("span at station {:.3} leaves the surface or crosses a gap", rec.s0));
                }
                for run in &runs {
                    for &(d, z) in run {
                        if d > 1e-6 && d < rec.span.length - 1e-6 {
                            adds.push((rec.s0 + d, z));
                        }
                    }
                }
            }
            for (s, z) in adds {
                let lay = self.layout()?;
                let near = lay.station.iter().any(|t| (t - s).abs() < 1e-6);
                if !near {
                    self.insert_elevation_point(s, z)?;
                    rep.points_added += 1;
                }
            }
        }
        Ok(rep)
    }

    // ---------------------------------------------------------------- vertex edits

    /// Inserts or sets an elevation point at a station. If a vertex is within a micrometre the
    /// vertex elevation is set instead. Returns the vertex index.
    pub fn insert_elevation_point(&mut self, station: f64, z: f64) -> Result<usize> {
        finite("station", station)?;
        finite("elevation", z)?;
        let lay = self.layout()?;
        if station < -EPS || station > lay.length + EPS {
            return Err(GradingError::Invalid("station is outside the line".into()));
        }
        if let Some(i) = lay.station.iter().position(|t| (t - station).abs() < 1e-6) {
            self.vertices[i].z = z;
            return Ok(i);
        }
        if self.closed && (lay.length - station).abs() < 1e-6 {
            self.vertices[0].z = z;
            return Ok(0);
        }
        let rec = self.rec_at(&lay, station).clone();
        let p = rec.span.point_at(station - rec.s0);
        let mut at = rec.start;
        for &e in &rec.elev {
            if lay.station[e] < station {
                at = e;
            }
        }
        self.vertices.insert(at + 1, Vertex::elev(p[0], p[1], z));
        Ok(at + 1)
    }

    /// Inserts a new PI after vertex `after`, at plan position `(x, y)`. The span that contains
    /// `after` is replaced by two straight spans, so the elevation points that were on it are
    /// removed (their plan position no longer lies on the line). `z` defaults to the elevation
    /// of the old line at the nearest station. Returns the new vertex index.
    pub fn insert_pi(&mut self, after: usize, x: f64, y: f64, z: Option<f64>) -> Result<usize> {
        finite("x", x)?;
        finite("y", y)?;
        let lay = self.layout()?;
        let rec = lay
            .spans
            .iter()
            .find(|r| r.start == after || r.elev.contains(&after))
            .ok_or_else(|| GradingError::Invalid("vertex has no span after it (the last PI of an open line)".into()))?
            .clone();
        let zz = match z {
            Some(z) => z,
            None => {
                let s = rec.s0 + rec.span.station_of([x, y]);
                self.z_at_layout(&lay, s)
            }
        };
        let (start, end_excl) = (rec.start, rec.start + 1 + rec.elev.len());
        self.vertices.drain(start + 1..end_excl);
        self.vertices[start].bulge = 0.0;
        self.vertices.insert(start + 1, Vertex::pi(x, y, zz));
        Ok(start + 1)
    }

    /// Deletes a vertex. Deleting a PI joins its two spans by a straight span and removes the
    /// elevation points of both; at least two PIs (three if closed) must remain.
    pub fn delete_vertex(&mut self, index: usize) -> Result<()> {
        let v = self.vertices.get(index).ok_or_else(|| GradingError::NotFound(format!("vertex {index}")))?;
        if v.kind == VertexKind::Elevation {
            self.vertices.remove(index);
            return Ok(());
        }
        let pis = self.pi_indices();
        if pis.len() <= if self.closed { 3 } else { 2 } {
            return Err(GradingError::Invalid("too few PIs would remain".into()));
        }
        let k = pis.iter().position(|&p| p == index).unwrap();
        let prev = if k == 0 { *pis.last().unwrap() } else { pis[k - 1] };
        let next = if k + 1 < pis.len() { pis[k + 1] } else { pis[0] };
        let mut remove: Vec<usize> = vec![index];
        let span_elev = |from: usize, to: usize| -> Vec<usize> { if to > from { (from + 1..to).collect() } else { (from + 1..self.vertices.len()).collect() } };
        if k > 0 || self.closed {
            remove.extend(span_elev(prev, index));
        }
        if k + 1 < pis.len() || self.closed {
            remove.extend(span_elev(index, next));
        }
        self.vertices[prev].bulge = 0.0;
        remove.sort_unstable();
        remove.dedup();
        for i in remove.into_iter().rev() {
            self.vertices.remove(i);
        }
        if self.vertices[0].kind != VertexKind::Pi {
            return Err(GradingError::Geometry("the line no longer starts with a PI".into()));
        }
        Ok(())
    }

    // ---------------------------------------------------------------- whole-line edits

    /// Reverses the direction of the line. The bulge of each span changes sign and moves to the
    /// new start of the span.
    pub fn reverse(&mut self) {
        let pis_old = self.pi_indices();
        let n = pis_old.len();
        let b_old: Vec<f64> = pis_old.iter().map(|&i| self.vertices[i].bulge).collect();
        self.vertices.reverse();
        if self.closed {
            if let Some(k) = self.vertices.iter().position(|v| v.kind == VertexKind::Pi) {
                self.vertices.rotate_left(k);
            }
        }
        let pis_new = self.pi_indices();
        for (k, &i) in pis_new.iter().enumerate() {
            self.vertices[i].bulge = if self.closed {
                -b_old[(n + n - 2 - k) % n]
            } else if k + 1 < n {
                -b_old[n - 2 - k]
            } else {
                0.0
            };
        }
    }

    /// The part of the line between two stations as a new open line.
    pub fn extract(&self, s0: f64, s1: f64) -> Result<FeatureLine> {
        let lay = self.layout()?;
        if !(s0 >= -EPS && s1 <= lay.length + EPS && s1 - s0 > 1e-6) {
            return Err(GradingError::Invalid("station range must lie inside the line and have a length".into()));
        }
        let (s0, s1) = (s0.max(0.0), s1.min(lay.length));
        let p = self.xy_at_layout(&lay, s0);
        let mut out = vec![Vertex::pi(p[0], p[1], self.z_at_layout(&lay, s0))];
        let mut last_pi = 0usize;
        for rec in &lay.spans {
            let a = s0.max(rec.s0);
            let b = s1.min(rec.s0 + rec.span.length);
            if b - a <= 1e-9 {
                continue;
            }
            out[last_pi].bulge = match &rec.span.arc {
                Some(g) => (g.sweep.signum() * (b - a) / g.radius / 4.0).tan(),
                None => 0.0,
            };
            for &e in &rec.elev {
                let st = lay.station[e];
                if st > a + 1e-9 && st < b - 1e-9 {
                    out.push(Vertex::elev(self.vertices[e].x, self.vertices[e].y, self.vertices[e].z));
                }
            }
            let q = if (b - (rec.s0 + rec.span.length)).abs() < 1e-12 { rec.span.b } else { rec.span.point_at(b - rec.s0) };
            out.push(Vertex::pi(q[0], q[1], self.z_at_layout(&lay, b)));
            last_pi = out.len() - 1;
        }
        Ok(FeatureLine { name: self.name.clone(), style_priority: self.style_priority, last_edited: self.last_edited, closed: false, vertices: out })
    }

    /// Trims the line to the part between two stations.
    pub fn trim(&self, s0: f64, s1: f64) -> Result<FeatureLine> {
        self.extract(s0, s1)
    }

    /// Breaks the line at a station into two lines (named `name-1` and `name-2`). A closed line
    /// becomes one open line that starts and ends at the station.
    pub fn break_at(&self, station: f64) -> Result<Vec<FeatureLine>> {
        let len = self.length()?;
        if self.closed {
            if !(station > 1e-6 && station < len - 1e-6) {
                return Err(GradingError::Invalid("break station must be inside the line".into()));
            }
            let mut a = self.extract(station, len)?;
            let b = self.extract(0.0, station)?;
            let n = a.vertices.len();
            a.vertices[n - 1].bulge = b.vertices[0].bulge;
            a.vertices.extend(b.vertices.into_iter().skip(1));
            a.name = format!("{}-1", self.name);
            return Ok(vec![a]);
        }
        if !(station > 1e-6 && station < len - 1e-6) {
            return Err(GradingError::Invalid("break station must be inside the line".into()));
        }
        let mut a = self.extract(0.0, station)?;
        let mut b = self.extract(station, len)?;
        a.name = format!("{}-1", self.name);
        b.name = format!("{}-2", self.name);
        Ok(vec![a, b])
    }

    /// Joins two open lines. The nearest pair of end points is joined, reversing a line if
    /// needed; the gap must not exceed `tolerance` (plan, m). The first line's elevation and
    /// name are kept at the joint and the mismatch is reported.
    pub fn join(&self, other: &FeatureLine, tolerance: f64) -> Result<JoinResult> {
        if self.closed || other.closed {
            return Err(GradingError::Invalid("a closed line cannot be joined".into()));
        }
        if !(tolerance >= 0.0 && tolerance.is_finite()) {
            return Err(GradingError::Invalid("tolerance must be a non-negative number".into()));
        }
        let ends = |f: &FeatureLine| {
            let (a, b) = (&f.vertices[0], &f.vertices[f.vertices.len() - 1]);
            ([a.x, a.y], [b.x, b.y])
        };
        let (s_start, s_end) = ends(self);
        let (o_start, o_end) = ends(other);
        let cands = [(len(sub(s_end, o_start)), 0), (len(sub(s_end, o_end)), 1), (len(sub(s_start, o_start)), 2), (len(sub(s_start, o_end)), 3)];
        let (gap, case) = cands.iter().cloned().fold((f64::INFINITY, 0), |m, c| if c.0 < m.0 { c } else { m });
        if gap > tolerance {
            return Err(GradingError::Geometry(format!("the nearest ends are {gap:.4} m apart, more than the tolerance")));
        }
        let (mut a, mut b) = (self.clone(), other.clone());
        match case {
            0 => {}
            1 => b.reverse(),
            2 => a.reverse(),
            _ => std::mem::swap(&mut a, &mut b),
        }
        let mismatch = b.vertices[0].z - a.vertices[a.vertices.len() - 1].z;
        let first_bulge = b.vertices[0].bulge;
        let n = a.vertices.len();
        a.vertices[n - 1].bulge = first_bulge;
        a.vertices.extend(b.vertices.into_iter().skip(1));
        a.name = self.name.clone();
        a.style_priority = self.style_priority;
        a.last_edited = self.last_edited.max(other.last_edited);
        Ok(JoinResult { line: a, elevation_mismatch: mismatch, gap })
    }

    /// Replaces the PI at `pi_index` by a circular arc of the given radius tangent to both
    /// adjacent spans (which must be straight). The tangent length is `R tan(delta / 2)` for
    /// deflection angle `delta`; it must be shorter than both adjacent spans. Elevations at the
    /// new tangent points are read from the old line; elevation points inside the arc are removed.
    pub fn fillet(&mut self, pi_index: usize, radius: f64) -> Result<FilletReport> {
        if !(radius > 0.0 && radius.is_finite()) {
            return Err(GradingError::Invalid("radius must be positive".into()));
        }
        let lay = self.layout()?;
        let v = self.vertices.get(pi_index).ok_or_else(|| GradingError::NotFound(format!("vertex {pi_index}")))?;
        if v.kind != VertexKind::Pi {
            return Err(GradingError::Invalid("a fillet needs a PI".into()));
        }
        let rin = lay.spans.iter().find(|r| r.end == pi_index && (self.closed || r.end != 0)).ok_or_else(|| GradingError::Invalid("the first and last PI of an open line cannot be filleted".into()))?;
        let rout = lay.spans.iter().find(|r| r.start == pi_index).ok_or_else(|| GradingError::Invalid("the first and last PI of an open line cannot be filleted".into()))?;
        if rin.span.arc.is_some() || rout.span.arc.is_some() {
            return Err(GradingError::Geometry("both spans at the corner must be straight".into()));
        }
        let b = rout.span.a;
        let (d1, d2) = (unit(sub(b, rin.span.a)), unit(sub(rout.span.b, b)));
        let delta = dot(d1, d2).clamp(-1.0, 1.0).acos();
        if delta < 1e-9 {
            return Err(GradingError::Geometry("the two spans are collinear".into()));
        }
        let t = radius * (delta / 2.0).tan();
        if t >= rin.span.length - 1e-9 || t >= rout.span.length - 1e-9 {
            return Err(GradingError::Geometry(format!("tangent length {t:.3} m does not fit in the adjacent spans")));
        }
        let s_b = rout.s0;
        let wrap_in = if pi_index == 0 { lay.length } else { s_b };
        let (z1, z2) = (self.z_at_layout(&lay, wrap_in - t), self.z_at_layout(&lay, s_b + t));
        let p1 = add(b, scale(d1, -t));
        let p2 = add(b, scale(d2, t));
        let bulge = (delta / 4.0).tan() * cross(d1, d2).signum();
        let sb = if pi_index == 0 { 0.0 } else { s_b };
        let near = |st: f64| {
            let mut d = (st - sb).abs();
            if self.closed {
                d = d.min(lay.length - d);
            }
            d < t
        };
        let mut new = Vec::with_capacity(self.vertices.len() + 1);
        let mut removed = 0;
        for (i, v) in self.vertices.iter().enumerate() {
            if i == pi_index {
                if pi_index == 0 {
                    continue;
                }
                let mut a = Vertex::pi(p1[0], p1[1], z1);
                a.bulge = bulge;
                new.push(a);
                new.push(Vertex::pi(p2[0], p2[1], z2));
            } else if v.kind == VertexKind::Elevation && near(lay.station[i]) {
                removed += 1;
            } else {
                new.push(v.clone());
            }
        }
        if pi_index == 0 {
            // Rotate so the line still starts with a PI: P2 first, P1 (with the arc) last.
            new.insert(0, Vertex::pi(p2[0], p2[1], z2));
            let mut a = Vertex::pi(p1[0], p1[1], z1);
            a.bulge = bulge;
            new.push(a);
        }
        self.vertices = new;
        Ok(FilletReport { deflection: delta, tangent_length: t, arc_length: radius * delta, elevation_points_removed: removed })
    }

    /// Weeds the line: removes PIs that lie within the plan and elevation tolerances of the chord
    /// of their neighbours (greedy, from the start, only between straight spans, and only when the
    /// resulting span is not longer than `max_span_length`), and then elevation points whose
    /// elevation is within `tolerances.elevation` of the grade between their neighbours.
    pub fn weed(&mut self, tol: WeedTolerances) -> Result<WeedReport> {
        for (n, v) in [("plan_distance", tol.plan_distance), ("elevation", tol.elevation), ("max_span_length", tol.max_span_length)] {
            if !(v >= 0.0 && v.is_finite()) {
                return Err(GradingError::Invalid(format!("{n} must be a non-negative number")));
            }
        }
        let mut rep = WeedReport::default();
        let mut k = 1;
        loop {
            let pis = self.pi_indices();
            let lay = self.layout()?;
            let n = pis.len();
            let limit = if self.closed { n } else { n.saturating_sub(1) };
            if k >= limit || n <= if self.closed { 3 } else { 2 } {
                break;
            }
            let (ip, ic, inx) = (pis[k - 1], pis[k], pis[(k + 1) % n]);
            let rec_a = lay.spans.iter().find(|r| r.start == ip).unwrap();
            let rec_b = lay.spans.iter().find(|r| r.start == ic).unwrap();
            if rec_a.span.arc.is_some() || rec_b.span.arc.is_some() {
                k += 1;
                continue;
            }
            let (a, c, b) = (self.vertices[ip].xy(), self.vertices[ic].xy(), self.vertices[inx].xy());
            let chord = len(sub(b, a));
            let (sa, sc, sb) = (lay.station[ip], lay.station[ic], if inx == 0 { lay.length } else { lay.station[inx] });
            let z_chord = self.vertices[ip].z + (self.vertices[inx].z - self.vertices[ip].z) * (sc - sa) / (sb - sa);
            let ok = dist_to_line(c, a, b) <= tol.plan_distance && (self.vertices[ic].z - z_chord).abs() <= tol.elevation && chord <= tol.max_span_length;
            if !ok {
                k += 1;
                continue;
            }
            // Remove the PI; elevation points of both spans are projected onto the new chord.
            let mut moved: Vec<(P2, f64)> = vec![];
            for &e in rec_a.elev.iter().chain(rec_b.elev.iter()) {
                let t = (dot(sub(self.vertices[e].xy(), a), unit(sub(b, a)))).clamp(0.0, chord);
                moved.push((add(a, scale(unit(sub(b, a)), t)), self.vertices[e].z));
            }
            let mut idx: Vec<usize> = rec_a.elev.iter().chain(rec_b.elev.iter()).cloned().collect();
            idx.push(ic);
            idx.sort_unstable();
            for i in idx.into_iter().rev() {
                self.vertices.remove(i);
            }
            for (j, (p, z)) in moved.into_iter().enumerate() {
                self.vertices.insert(ip + 1 + j, Vertex::elev(p[0], p[1], z));
            }
            rep.pis_removed += 1;
        }
        // Elevation points.
        let mut i = 1;
        while i + 1 < self.vertices.len() {
            if self.vertices[i].kind == VertexKind::Elevation {
                let lay = self.layout()?;
                let (sa, sc, sb) = (lay.station[i - 1], lay.station[i], lay.station[i + 1]);
                if sb > sa {
                    let zc = self.vertices[i - 1].z + (self.vertices[i + 1].z - self.vertices[i - 1].z) * (sc - sa) / (sb - sa);
                    if (self.vertices[i].z - zc).abs() <= tol.elevation {
                        self.vertices.remove(i);
                        rep.elevation_points_removed += 1;
                        continue;
                    }
                }
            }
            i += 1;
        }
        Ok(rep)
    }

    /// A stepped offset of the line: a new line at plan distance `offset` (positive to the left of
    /// the direction of the line, negative to the right) with an elevation step.
    ///
    /// Arcs are flattened with `facet_angle` (radians) first, so the result has straight spans only.
    /// At each vertex the two offset spans are joined at their intersection (a miter), which keeps
    /// the plan distance exact on both spans; a very sharp corner therefore gives a long miter, and
    /// an inside corner of a large offset can fold over itself (check the result, or use
    /// `sweep_template`, which cleans such corners in a grading). The elevation of each new vertex
    /// is the elevation of the source line at the same station plus the step.
    pub fn offset_stepped(&self, offset: f64, step: ElevationStep, facet_angle: f64) -> Result<FeatureLine> {
        finite("offset", offset)?;
        let flat = self.flatten(facet_angle)?;
        let n = flat.len();
        let o = [flat[0].p[0], flat[0].p[1]];
        let pts: Vec<P2> = flat.iter().map(|f| sub([f.p[0], f.p[1]], o)).collect();
        let dz = match step {
            ElevationStep::Difference(d) => {
                finite("elevation difference", d)?;
                d
            }
            ElevationStep::GradePercent(g) => {
                finite("grade", g)?;
                g / 100.0 * offset.abs()
            }
        };
        let lay = self.layout()?;
        let t_start = lay.spans[0].span.tangent_at(0.0);
        let t_end = lay.spans[lay.spans.len() - 1].span.tangent_at(lay.spans[lay.spans.len() - 1].span.length);
        let segs = if self.closed { n } else { n - 1 };
        let dirs: Vec<P2> = (0..segs).map(|i| unit(sub(pts[(i + 1) % n], pts[i]))).collect();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let (d_in, d_out) = if self.closed {
                (dirs[(i + n - 1) % n], dirs[i])
            } else if i == 0 {
                (t_start, t_start)
            } else if i == n - 1 {
                (t_end, t_end)
            } else {
                (dirs[i - 1], dirs[i])
            };
            let (n_in, n_out) = (scale(left(d_in), offset), scale(left(d_out), offset));
            let p = pts[i];
            let q = match line_line(add(p, n_in), d_in, add(p, n_out), d_out) {
                Some(q) if dot(d_in, d_out) > -0.999 => q,
                _ => add(p, n_in),
            };
            out.push(Vertex::pi(o[0] + q[0], o[1] + q[1], flat[i].p[2] + dz));
        }
        Ok(FeatureLine { name: format!("{}-offset", self.name), style_priority: self.style_priority, last_edited: self.last_edited, closed: self.closed, vertices: out })
    }

    /// A fingerprint of the geometry (for rebuild data).
    pub fn fingerprint(&self) -> u64 {
        let mut h = Fnv::new();
        h.bytes(&[self.closed as u8]);
        for v in &self.vertices {
            h.f64(v.x);
            h.f64(v.y);
            h.f64(v.z);
            h.f64(v.bulge);
            h.bytes(&[v.kind as u8]);
        }
        h.0
    }

    /// The plan polygon of a closed line (flattened with `facet_angle`), `[x, y]`.
    pub fn plan_ring(&self, facet_angle: f64) -> Result<Vec<[f64; 2]>> {
        Ok(self.flatten(facet_angle)?.iter().map(|f| [f.p[0], f.p[1]]).collect())
    }
}
