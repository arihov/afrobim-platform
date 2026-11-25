// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Where two alignments cross or meet.
//!
//! # Method
//!
//! Each alignment is cut into short chords (a straight is one chord; a curve or spiral is cut so
//! that no chord turns more than 0.05 rad), optionally with a straight extension of each end
//! along its end tangent. Every pair of chords is tested for intersection. Each hit is then
//! refined on the true elements by Newton's method on
//!
//! ```text
//!   F(sa, sb) = A(sa) - B(sb) = 0,    dF/dsa = (cos ha, sin ha),   dF/dsb = -(cos hb, sin hb)
//! ```
//!
//! whose Jacobian has determinant `sin(ha - hb)`; the iteration stops below 1e-11 m. Hits within
//! 1e-6 m of one another (a crossing at a chord joint is found twice) are one crossing.
//! Parallel overlaps have no determinate crossing and are not reported.
//!
//! The angle between the two lines is `angle = min(|ha - hb|, pi - |ha - hb|)` taken on the
//! wrapped difference, in (0, pi/2]; the skew is `pi/2 - angle` (zero for a right-angle crossing).

use afrobim_alignment::{wrap_pi, Alignment, ElementKind};
use serde::{Deserialize, Serialize};
use std::f64::consts::FRAC_PI_2;

use crate::geom::{cross, dir, sub, Pt};

/// Largest heading change of one chord when an alignment is cut for the search, radians.
const CHORD_TURN_RAD: f64 = 0.05;
/// A crossing within this distance of an end of an alignment is at that end, m.
pub const END_TOLERANCE_M: f64 = 1e-6;

/// Which arms an alignment has either side of a crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arms {
    /// Alignment continues below the crossing station.
    pub behind: bool,
    /// Alignment continues above the crossing station.
    pub ahead: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossingKind {
    /// Both alignments continue on both sides.
    Cross,
    /// One continues on both sides and the other ends at (or is cut off by) it.
    Tee,
    /// Anything else: an end meeting an end, or a crossing on an extension.
    Other,
}

/// A crossing or meeting point of two alignments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Crossing {
    pub x: f64,
    pub y: f64,
    /// Internal station on the first alignment, m (outside its range on an extension).
    pub station_a_m: f64,
    pub station_b_m: f64,
    pub heading_a_rad: f64,
    pub heading_b_rad: f64,
    /// Angle between the two lines, in (0, pi/2], radians.
    pub angle_rad: f64,
    /// `pi/2 - angle_rad`: how far the crossing is from a right angle, radians.
    pub skew_rad: f64,
    pub on_extension_a: bool,
    pub on_extension_b: bool,
    pub arms_a: Arms,
    pub arms_b: Arms,
    pub kind: CrossingKind,
}

struct Seg {
    s0: f64,
    s1: f64,
    p0: Pt,
    p1: Pt,
}

/// Position and heading at a station, extended along the end tangents beyond the ends.
fn ext(a: &Alignment, s: f64) -> (Pt, f64) {
    let (lo, hi) = (a.start_station_m, a.end_station_m());
    if s < lo {
        let p = a.point_at(lo).expect("alignment has elements");
        let d = dir(p.heading_rad);
        return ((p.x + (s - lo) * d.0, p.y + (s - lo) * d.1), p.heading_rad);
    }
    if s > hi {
        let p = a.point_at(hi).expect("alignment has elements");
        let d = dir(p.heading_rad);
        return ((p.x + (s - hi) * d.0, p.y + (s - hi) * d.1), p.heading_rad);
    }
    let p = a.point_at(s).expect("station is on the alignment");
    ((p.x, p.y), p.heading_rad)
}

fn segments(a: &Alignment, extend_m: f64) -> Vec<Seg> {
    let mut out = Vec::new();
    if a.elements.is_empty() {
        return out;
    }
    let (lo, hi) = (a.start_station_m, a.end_station_m());
    if extend_m > 0.0 {
        out.push(Seg { s0: lo - extend_m, s1: lo, p0: ext(a, lo - extend_m).0, p1: ext(a, lo).0 });
    }
    let mut es = lo;
    for e in &a.elements {
        let turn = (e.heading_at(e.length) - e.heading_rad).abs();
        let n = if e.kind() == ElementKind::Line { 1 } else { ((turn / CHORD_TURN_RAD).ceil() as usize).clamp(1, 100_000) };
        for i in 0..n {
            let (u0, u1) = (e.length * i as f64 / n as f64, e.length * (i + 1) as f64 / n as f64);
            out.push(Seg { s0: es + u0, s1: es + u1, p0: e.point_at(u0), p1: e.point_at(u1) });
        }
        es += e.length;
    }
    if extend_m > 0.0 {
        out.push(Seg { s0: hi, s1: hi + extend_m, p0: ext(a, hi).0, p1: ext(a, hi + extend_m).0 });
    }
    out
}

/// Newton refinement of a hit on the true alignments (with straight end extensions).
fn refine(a: &Alignment, b: &Alignment, mut sa: f64, mut sb: f64) -> Option<(f64, f64)> {
    for _ in 0..40 {
        let ((ax, ay), ha) = ext(a, sa);
        let ((bx, by), hb) = ext(b, sb);
        let (fx, fy) = (ax - bx, ay - by);
        if fx.hypot(fy) < 1e-11 {
            return Some((sa, sb));
        }
        let (ca, sa_, cb, sb_) = (ha.cos(), ha.sin(), hb.cos(), hb.sin());
        let det = cb * sa_ - ca * sb_;
        if det.abs() < 1e-12 {
            return None;
        }
        let dsa = (fx * sb_ - cb * fy) / det;
        let dsb = (-ca * fy + fx * sa_) / det;
        sa += dsa;
        sb += dsb;
        if dsa.abs() < 1e-13 && dsb.abs() < 1e-13 {
            return Some((sa, sb));
        }
    }
    let ((ax, ay), _) = ext(a, sa);
    let ((bx, by), _) = ext(b, sb);
    ((ax - bx).hypot(ay - by) < 1e-6).then_some((sa, sb))
}

fn arms(a: &Alignment, s: f64) -> Arms {
    Arms { behind: s > a.start_station_m + END_TOLERANCE_M, ahead: s < a.end_station_m() - END_TOLERANCE_M }
}

/// Every crossing of two alignments, in order of station along `a`. `extend_m` extends each end
/// of each alignment along its end tangent for the search (0: none); a crossing found on an
/// extension has `on_extension_a` or `_b` set and a station outside the alignment's range.
pub fn find_crossings(a: &Alignment, b: &Alignment, extend_m: f64) -> Vec<Crossing> {
    let (sa, sb) = (segments(a, extend_m.max(0.0)), segments(b, extend_m.max(0.0)));
    let mut hits: Vec<(f64, f64)> = Vec::new();
    for x in &sa {
        let (xmin, xmax) = (x.p0.0.min(x.p1.0), x.p0.0.max(x.p1.0));
        let (ymin, ymax) = (x.p0.1.min(x.p1.1), x.p0.1.max(x.p1.1));
        for y in &sb {
            let pad = 1e-6;
            if y.p0.0.max(y.p1.0) < xmin - pad || y.p0.0.min(y.p1.0) > xmax + pad || y.p0.1.max(y.p1.1) < ymin - pad || y.p0.1.min(y.p1.1) > ymax + pad {
                continue;
            }
            // x.p0 + t r = y.p0 + u q
            let r = sub(x.p1, x.p0);
            let q = sub(y.p1, y.p0);
            let den = cross(r, q);
            if den.abs() < 1e-14 * (r.0.hypot(r.1) * q.0.hypot(q.1)).max(1e-300) {
                continue;
            }
            let w = sub(y.p0, x.p0);
            let t = cross(w, q) / den;
            let u = cross(w, r) / den;
            // A chord of a curve lies inside the curve, so an alignment that ends exactly on another
            // can miss the chord by the chord's sagitta: allow each chord 10 % of slack at each end, and
            // let the refinement on the true elements decide (it fails to converge on a false hit).
            let slack = 0.1;
            if t < -slack || t > 1.0 + slack || u < -slack || u > 1.0 + slack {
                continue;
            }
            let (t, u) = (t.clamp(0.0, 1.0), u.clamp(0.0, 1.0));
            if let Some((ha, hb)) = refine(a, b, x.s0 + t * (x.s1 - x.s0), y.s0 + u * (y.s1 - y.s0)) {
                let within = |al: &Alignment, s: f64| s >= al.start_station_m - extend_m.max(0.0) - END_TOLERANCE_M && s <= al.end_station_m() + extend_m.max(0.0) + END_TOLERANCE_M;
                if within(a, ha) && within(b, hb) {
                    hits.push((ha, hb));
                }
            }
        }
    }
    hits.sort_by(|p, q| p.0.total_cmp(&q.0));
    let mut kept: Vec<(f64, f64, Pt)> = Vec::new();
    for (s_a, s_b) in hits {
        let (p, _) = ext(a, s_a);
        if kept.iter().all(|k| (k.2 .0 - p.0).hypot(k.2 .1 - p.1) > 1e-6) {
            kept.push((s_a, s_b, p));
        }
    }
    kept.into_iter()
        .map(|(s_a, s_b, p)| {
            let (_, ha) = ext(a, s_a);
            let (_, hb) = ext(b, s_b);
            let d = wrap_pi(ha - hb).abs();
            let angle = d.min(std::f64::consts::PI - d);
            let (arms_a, arms_b) = (arms(a, s_a), arms(b, s_b));
            let on_a = s_a < a.start_station_m - END_TOLERANCE_M || s_a > a.end_station_m() + END_TOLERANCE_M;
            let on_b = s_b < b.start_station_m - END_TOLERANCE_M || s_b > b.end_station_m() + END_TOLERANCE_M;
            let full = |m: Arms| m.behind && m.ahead;
            let one = |m: Arms| m.behind != m.ahead;
            let kind = if on_a || on_b {
                CrossingKind::Other
            } else if full(arms_a) && full(arms_b) {
                CrossingKind::Cross
            } else if (full(arms_a) && one(arms_b)) || (one(arms_a) && full(arms_b)) {
                CrossingKind::Tee
            } else {
                CrossingKind::Other
            };
            Crossing {
                x: p.0,
                y: p.1,
                station_a_m: s_a,
                station_b_m: s_b,
                heading_a_rad: ha.rem_euclid(std::f64::consts::TAU),
                heading_b_rad: hb.rem_euclid(std::f64::consts::TAU),
                angle_rad: angle,
                skew_rad: FRAC_PI_2 - angle,
                on_extension_a: on_a,
                on_extension_b: on_b,
                arms_a,
                arms_b,
                kind,
            }
        })
        .collect()
}
