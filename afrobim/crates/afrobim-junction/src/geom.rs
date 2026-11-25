// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Plan geometry helpers: vectors, slicing an alignment, offsetting an alignment.
//!
//! # Offsetting an element (derivation)
//!
//! Take a plane curve with heading `theta(s)` and curvature `k(s) = theta'(s)` (positive turning
//! left). The point `d` to the right is `q(s) = p(s) + d n(s)` with `n = (sin theta, -cos theta)`.
//! Differentiating, `q'(s) = (cos theta, sin theta) + d k (cos theta, sin theta) = (1 + k d) t(s)`.
//! So the offset curve has the same heading at the same `s`, its speed is `1 + k d`, hence
//!
//! ```text
//!   ds'     = (1 + k d) ds
//!   length' = L + d (theta(L) - theta(0))          exact for every element kind
//!   k'      = k / (1 + k d)                        curvature of the offset curve
//! ```
//!
//! A straight offsets to a straight of the same length. A circular arc of radius R offsets to an
//! arc of radius `R + d` (turning left) on the same centre. A clothoid does not offset to a
//! clothoid; it is cut into pieces that each turn at most [`SPIRAL_PIECE_RAD`], and each piece
//! becomes one element that starts at the exact offset point on the exact heading with curvature
//! varying linearly between the exact offset curvatures at its ends. The joints between pieces
//! are then not exact (a gap of a few micrometres at the piece size used); the largest gap is
//! reported by [`max_joint_gap`] and carried into the findings.

use afrobim_alignment::{Alignment, AlignmentElement, ElementKind};

use crate::error::JunctionError;

pub type Pt = (f64, f64);

/// Largest heading change of one piece when a spiral is offset, radians.
pub(crate) const SPIRAL_PIECE_RAD: f64 = 0.01;

pub(crate) fn sub(a: Pt, b: Pt) -> Pt {
    (a.0 - b.0, a.1 - b.1)
}
pub(crate) fn add(a: Pt, b: Pt) -> Pt {
    (a.0 + b.0, a.1 + b.1)
}
pub(crate) fn scale(a: Pt, k: f64) -> Pt {
    (a.0 * k, a.1 * k)
}
pub(crate) fn dot(a: Pt, b: Pt) -> f64 {
    a.0 * b.0 + a.1 * b.1
}
pub(crate) fn cross(a: Pt, b: Pt) -> f64 {
    a.0 * b.1 - a.1 * b.0
}
pub(crate) fn norm(a: Pt) -> f64 {
    a.0.hypot(a.1)
}
pub(crate) fn dist(a: Pt, b: Pt) -> f64 {
    norm(sub(a, b))
}
pub(crate) fn dir(heading: f64) -> Pt {
    (heading.cos(), heading.sin())
}
pub(crate) fn heading_of(v: Pt) -> f64 {
    v.1.atan2(v.0)
}

/// The part of an alignment between two internal stations, as an alignment with the **same
/// internal stations** (its start station is `s0`), and the same profile, superelevation and
/// station equations. Elements are cut exactly: a piece of an element keeps its start point,
/// heading and the curvature at its two ends.
pub fn slice_alignment(a: &Alignment, s0: f64, s1: f64, name: &str) -> Result<Alignment, JunctionError> {
    let (lo, hi) = (a.start_station_m, a.end_station_m());
    let (s0, s1) = (s0.max(lo), s1.min(hi));
    if !(s1 - s0 > 1e-9) {
        return Err(JunctionError::geometry(format!("the part of alignment '{}' between {s0:.3} and {s1:.3} has no length", a.name)));
    }
    let mut out = Vec::new();
    let mut es = lo;
    for e in &a.elements {
        let (b0, b1) = (es.max(s0), (es + e.length).min(s1));
        if b1 - b0 > 1e-12 {
            let (la, lb) = (b0 - es, b1 - es);
            let (x, y) = e.point_at(la);
            out.push(AlignmentElement {
                x,
                y,
                heading_rad: e.heading_at(la),
                length: lb - la,
                curvature_start: e.curvature_at(la),
                curvature_end: e.curvature_at(lb),
            });
        }
        es += e.length;
    }
    let mut r = Alignment::from_elements(name, s0, out).map_err(|e| JunctionError::geometry(e.to_string()))?;
    r.profile = a.profile.clone();
    r.superelevation = a.superelevation.clone();
    r.station_equations = a.station_equations.clone();
    Ok(r)
}

/// Appends the elements of the part `[a, b]` (distances from the element's start) of `e`, offset
/// `d` to the right (negative: left). See the module documentation.
pub(crate) fn offset_pieces(e: &AlignmentElement, a: f64, b: f64, d: f64, out: &mut Vec<AlignmentElement>) -> Result<(), JunctionError> {
    let turn = (e.heading_at(b) - e.heading_at(a)).abs();
    let n = if e.kind() == ElementKind::Spiral { ((turn / SPIRAL_PIECE_RAD).ceil() as usize).clamp(1, 20_000) } else { 1 };
    for i in 0..n {
        let pa = a + (b - a) * i as f64 / n as f64;
        let pb = a + (b - a) * (i + 1) as f64 / n as f64;
        let (k0, k1) = (e.curvature_at(pa), e.curvature_at(pb));
        let (f0, f1) = (1.0 + k0 * d, 1.0 + k1 * d);
        if !(f0 > 1e-9 && f1 > 1e-9) {
            return Err(JunctionError::geometry(format!(
                "an offset of {:.3} m to the right collapses a curve of radius {:.3} m (the offset is on the inside of the curve and as large as its radius)",
                d,
                1.0 / k0.abs().max(k1.abs())
            )));
        }
        let h = e.heading_at(pa);
        let (x, y) = e.point_at(pa);
        let length = (pb - pa) + d * (e.heading_at(pb) - h);
        out.push(AlignmentElement {
            x: x + d * h.sin(),
            y: y - d * h.cos(),
            heading_rad: h,
            length,
            curvature_start: k0 / f0,
            curvature_end: k1 / f1,
        });
    }
    Ok(())
}

/// A whole alignment offset by a constant `d_right` (negative: to the left). The result has its
/// own stationing (its start station is the original's; stations then run along the offset
/// curve, which differs in length on curves) and carries no profile.
pub fn offset_alignment(a: &Alignment, d_right: f64, name: &str) -> Result<Alignment, JunctionError> {
    let mut out = Vec::new();
    for e in &a.elements {
        offset_pieces(e, 0.0, e.length, d_right, &mut out)?;
    }
    Alignment::from_elements(name, a.start_station_m, out).map_err(|e| JunctionError::geometry(e.to_string()))
}

/// The largest gap between the end of an element and the start of the next, m.
pub fn max_joint_gap(a: &Alignment) -> f64 {
    a.joints().iter().map(|j| j.gap_m).fold(0.0, f64::max)
}

/// Plan point of an alignment at a station (no offset).
pub(crate) fn pt_at(a: &Alignment, s: f64) -> Result<Pt, JunctionError> {
    a.point_at(s).map(|p| (p.x, p.y)).ok_or_else(|| JunctionError::geometry(format!("station {s:.3} is off alignment '{}'", a.name)))
}

/// Station and offset of a plan point on an alignment, or an error.
pub(crate) fn station_offset(a: &Alignment, p: Pt) -> Result<(f64, f64), JunctionError> {
    a.station_offset(p.0, p.1)
        .ok_or_else(|| JunctionError::geometry(format!("the point ({:.3}, {:.3}) has no perpendicular foot on alignment '{}'", p.0, p.1, a.name)))
}

/// A polyline of an alignment between two stations, chords no longer than `max_chord_m`
/// (curves are cut into equal chords).
pub fn polyline_between(a: &Alignment, s0: f64, s1: f64, max_chord_m: f64) -> Vec<Pt> {
    let mut pts: Vec<Pt> = Vec::new();
    let (lo, hi) = (a.start_station_m.max(s0.min(s1)), a.end_station_m().min(s0.max(s1)));
    let mut es = a.start_station_m;
    for e in &a.elements {
        let (b0, b1) = (es.max(lo), (es + e.length).min(hi));
        if b1 - b0 > 1e-12 {
            let n = if e.kind() == ElementKind::Line || !(max_chord_m > 0.0) { 1 } else { (((b1 - b0) / max_chord_m).ceil() as usize).clamp(1, 1_000_000) };
            for i in 0..=n {
                let p = e.point_at(b0 - es + (b1 - b0) * i as f64 / n as f64);
                if pts.last() != Some(&p) {
                    pts.push(p);
                }
            }
        }
        es += e.length;
    }
    pts
}
