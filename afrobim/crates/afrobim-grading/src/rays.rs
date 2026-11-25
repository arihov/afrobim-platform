// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Nodes along a footprint and the projection rays that leave them.
//!
//! # Rays
//!
//! A footprint is cut into **nodes**: every vertex of the feature line, the ends of the station
//! range, extra stations asked for by the caller (region boundaries), and points inserted so that
//! no piece is longer than `ray_spacing` and no arc piece turns by more than `facet_angle`.
//!
//! At each node one or more rays leave the footprint on the chosen side. A ray has a unit
//! plan direction `u` and a **miter factor** `m >= 1`; a point at *perpendicular distance* `t`
//! from the footprint lies at `foot + u m t`, with elevation `z0 + s t` for a slope `s`.
//!
//! * Straight run or smooth arc: one ray along the side normal, `m = 1`.
//! * Interior corner (the side the grading goes to is the inside of the turn, the normals of the
//!   two spans converge): one ray along the bisector of the two normals, with `m = 1 / cos(phi/2)`
//!   where `phi` is the angle between the normals. At perpendicular distance `t` from both spans
//!   the point is then on both offset lines (the miter point), so on flat ground the daylight
//!   lines of the two spans meet exactly at the ray's end.
//! * Exterior corner (the normals diverge): a **radial fan**: the normal of the incoming span
//!   is turned to the normal of the outgoing span in `ceil(phi / facet_angle)` equal steps, one
//!   ray per step, `m = 1`. On flat ground the fan is a pyramid approximating the cone.
//!
//! Interior corners can still make the daylight line fold over itself (short spans, large
//! distances); the clean-up of the grading removes such loops.

use serde::{Deserialize, Serialize};

use crate::error::{GradingError, Result};
use crate::featureline::{FeatureLine, Layout, VertexKind};
use crate::geom::*;

const EPS: f64 = 1e-9;

/// Which side of the footprint (looking along its direction) a grading goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub(crate) fn sign(self) -> f64 {
        match self {
            Side::Left => 1.0,
            Side::Right => -1.0,
        }
    }
}

/// Sampling inputs of a projection. Both are required: there is no default.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectionParams {
    /// Largest station interval between rays along the footprint, m.
    pub ray_spacing: f64,
    /// Largest angle between neighbouring rays at an exterior corner or on an arc, radians.
    pub facet_angle: f64,
}

impl ProjectionParams {
    pub fn validate(&self) -> Result<()> {
        if !(self.ray_spacing > 0.0 && self.ray_spacing.is_finite()) {
            return Err(GradingError::Invalid("ray_spacing must be positive".into()));
        }
        if !(self.facet_angle > 0.0 && self.facet_angle <= std::f64::consts::PI) {
            return Err(GradingError::Invalid("facet_angle must be in (0, pi] radians".into()));
        }
        Ok(())
    }
}

/// How a ray leaves its node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RayKind {
    /// Along the normal of a straight run or an arc.
    Normal,
    /// The bisector ray of an interior corner.
    InteriorCorner,
    /// One ray of the radial fan of an exterior corner.
    Fan,
}

/// A node of the footprint.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Node {
    pub station: f64,
    pub foot: [f64; 3],
    pub tin: P2,
    pub tout: P2,
}

/// Geometry of one ray (before any criterion is applied).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct RayGeom {
    pub node: usize,
    pub station: f64,
    pub foot: [f64; 3],
    pub dir: [f64; 2],
    pub miter: f64,
    pub kind: RayKind,
}

/// The rays of a footprint run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RaySet {
    pub rays: Vec<RayGeom>,
    /// True when the run is a whole closed footprint (the last ray joins the first).
    pub ring: bool,
}

fn tangent_before(fl: &FeatureLine, lay: &Layout, s: f64) -> P2 {
    let s = if s <= 1e-12 && fl.closed { lay.length } else { s };
    let rec = lay.spans.iter().rev().find(|r| r.s0 < s - 1e-12).unwrap_or(&lay.spans[0]);
    rec.span.tangent_at((s - rec.s0).min(rec.span.length))
}

fn tangent_after(lay: &Layout, s: f64, closed: bool) -> P2 {
    let s = if s >= lay.length - 1e-12 && closed { 0.0 } else { s };
    let rec = lay.spans.iter().rev().find(|r| r.s0 <= s + 1e-12).unwrap_or(&lay.spans[0]);
    rec.span.tangent_at((s - rec.s0).max(0.0))
}

/// The nodes of a footprint run. `range` is `None` for the whole line (a closed line is then a ring
/// and its first node is not repeated at the end).
pub(crate) fn nodes(fl: &FeatureLine, range: Option<(f64, f64)>, params: &ProjectionParams, extra: &[f64]) -> Result<(Vec<Node>, bool)> {
    params.validate()?;
    let lay = fl.layout()?;
    let ring = range.is_none() && fl.closed;
    let (s0, s1) = range.unwrap_or((0.0, lay.length));
    if !(s0 >= -EPS && s1 <= lay.length + EPS && s1 - s0 > 1e-6) {
        return Err(GradingError::Invalid("the station range must lie inside the footprint and have a length".into()));
    }
    let (s0, s1) = (s0.max(0.0), s1.min(lay.length));
    let mut marks: Vec<f64> = vec![s0, s1];
    for (i, v) in fl.vertices.iter().enumerate() {
        let _ = v;
        let st = lay.station[i];
        if st > s0 + EPS && st < s1 - EPS {
            marks.push(st);
        }
    }
    marks.extend(extra.iter().cloned().filter(|e| *e > s0 + EPS && *e < s1 - EPS));
    marks.sort_by(|a, b| a.partial_cmp(b).unwrap());
    marks.dedup_by(|a, b| (*a - *b).abs() < EPS);
    let mut stations: Vec<f64> = vec![];
    for w in marks.windows(2) {
        let (a, b) = (w[0], w[1]);
        let rec = lay.spans.iter().rev().find(|r| r.s0 <= 0.5 * (a + b)).unwrap_or(&lay.spans[0]);
        let mut n = ((b - a) / params.ray_spacing).ceil().max(1.0) as usize;
        if let Some(g) = &rec.span.arc {
            n = n.max((((b - a) / g.radius) / params.facet_angle).ceil() as usize);
        }
        for k in 0..n {
            stations.push(a + (b - a) * k as f64 / n as f64);
        }
    }
    if !ring {
        stations.push(s1);
    }
    let mut out = Vec::with_capacity(stations.len());
    let last = stations.len() - 1;
    for (k, &s) in stations.iter().enumerate() {
        let xy = fl.xy_at_layout(&lay, s);
        let z = fl.z_at_layout(&lay, s);
        let is_pi = fl.vertices.iter().enumerate().any(|(i, v)| v.kind == VertexKind::Pi && (lay.station[i] - s).abs() < EPS);
        let (tin, tout) = if !ring && (k == 0 || k == last) {
            let t = if k == 0 { tangent_after(&lay, s, fl.closed) } else { tangent_before(fl, &lay, s) };
            (t, t)
        } else if is_pi {
            (tangent_before(fl, &lay, s), tangent_after(&lay, s, fl.closed))
        } else {
            let t = tangent_after(&lay, s, fl.closed);
            (t, t)
        };
        out.push(Node { station: s, foot: [xy[0], xy[1], z], tin, tout });
    }
    Ok((out, ring))
}

/// The rays of a footprint run on one side. `extra` are stations that must be nodes.
pub fn ray_geometry(fl: &FeatureLine, range: Option<(f64, f64)>, side: Side, params: &ProjectionParams, extra: &[f64]) -> Result<RaySet> {
    let (nodes, ring) = nodes(fl, range, params, extra)?;
    let sg = side.sign();
    let mut rays = Vec::new();
    for (k, n) in nodes.iter().enumerate() {
        let turn = cross(n.tin, n.tout);
        let cs = dot(n.tin, n.tout);
        let (n_in, n_out) = (scale(left(n.tin), sg), scale(left(n.tout), sg));
        if turn.abs() < 1e-9 && cs > 0.0 {
            rays.push(RayGeom { node: k, station: n.station, foot: n.foot, dir: n_in, miter: 1.0, kind: RayKind::Normal });
            continue;
        }
        if cs < 0.0 && turn.abs() < 1e-6 {
            return Err(GradingError::Geometry(format!("the footprint reverses on itself at station {:.3}", n.station)));
        }
        if sg * turn > 0.0 {
            let bis = add(n_in, n_out);
            let c2 = len(bis) / 2.0;
            if c2 < 1e-6 {
                return Err(GradingError::Geometry(format!("interior corner at station {:.3} is too sharp for a miter", n.station)));
            }
            rays.push(RayGeom { node: k, station: n.station, foot: n.foot, dir: unit(bis), miter: 1.0 / c2, kind: RayKind::InteriorCorner });
        } else {
            let phi = cross(n_in, n_out).atan2(dot(n_in, n_out));
            let steps = (phi.abs() / params.facet_angle).ceil().max(1.0) as usize;
            for j in 0..=steps {
                rays.push(RayGeom { node: k, station: n.station, foot: n.foot, dir: rot(n_in, phi * j as f64 / steps as f64), miter: 1.0, kind: RayKind::Fan });
            }
        }
    }
    Ok(RaySet { rays, ring })
}
