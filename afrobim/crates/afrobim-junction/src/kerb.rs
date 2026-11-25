// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Kerb returns: the line that joins one road's edge to the other's round a corner.
//!
//! A **quadrant** is the corner between one arm of the primary road and one arm of the secondary
//! road (an arm is the part of a road ahead of, or behind, the crossing). The return runs from
//! the primary road's edge to the secondary road's edge, so its direction of travel is from the
//! primary arm to the secondary arm. `E1` is the primary edge on the side of the secondary arm
//! and `E2` the secondary edge on the side of the primary arm; they meet at the corner `C`.
//!
//! # Simple circular return (derivation)
//!
//! A circle of radius `R` tangent to `E1` and `E2` and lying outside the carriageway (in the
//! unpaved quadrant) has its centre at distance `R` from each, on the outer side of each. The
//! outer offset of `E1` by `R` and the outer offset of `E2` by `R` are the loci of such centres;
//! the centre is where they cross (nearest the corner). The tangent points are the feet of the
//! perpendiculars from the centre to `E1` and `E2`. This holds for any edge shape (straight or
//! curved) that [`crate::geom::offset_alignment`] can offset. For two straights meeting at the
//! corner with interior angle `theta` between the arms, the tangent length (corner to tangent
//! point) is `R / tan(theta/2)` and the arc turns `pi - theta`. For a right-angle cross with
//! the edges at offsets `d1` (primary edge) and `d2` (secondary edge) from the crossing, with the
//! primary along +X, the secondary along +Y and the quadrant `x > 0, y > 0`: the edges are
//! `y = d1` and `x = d2`, the corner `(d2, d1)`, the centre `(d2 + R, d1 + R)`, the tangent points
//! `(d2 + R, d1)` and `(d2, d1 + R)`, and the arc length `R pi / 2`.
//!
//! # Three-centred compound return (this crate's construction)
//!
//! Three arcs of radii `R1`, `R2`, `R3` turning the same way, each tangent to the next, the first
//! tangent to `E1` and the last to `E2`. Radii alone leave two freedoms; they are fixed by two
//! offsets: `o1` is the distance of the end of the first arc from `E1`, and `o3` the distance of
//! the start of the last arc from `E2`. An arc of radius `R` leaving a straight edge tangentially
//! is at distance `R (1 - cos phi)` from the edge after turning `phi`, so
//!
//! ```text
//!   phi1 = acos(1 - o1 / R1),   phi3 = acos(1 - o3 / R3),   phi2 = Delta - phi1 - phi3
//! ```
//!
//! where `Delta = pi - theta` is the angle between the edges' directions (the return's total
//! turn). The middle arc must turn a positive angle. Walking the three arcs from a start point
//! on `E1` heading towards the corner gives the end point relative to the start, `W`. With `e1`
//! the unit direction of `E1` away from the corner and `e2` that of `E2`, the start is
//! `C + t1 e1`, and the end lies on `E2` when `cross(e2, t1 e1 + W) = 0`, so
//! `t1 = -cross(e2, W) / cross(e2, e1)`; the end's tangent length is `t2 = (t1 e1 + W) . e2`.
//! Both must be positive. The construction treats both edges as the straight lines through the
//! corner tangent to them there; the distance of the tangent points from the real edges is
//! reported. Tangency of the middle arc to its neighbours follows from continuing the heading.
//!
//! # Chamfer
//!
//! A straight from the point on `E1` at distance `leg1` from the corner (measured along `E1`
//! towards the primary arm) to the point on `E2` at `leg2` from the corner.

use afrobim_alignment::{wrap_pi, Alignment, AlignmentBuilder, AlignmentElement, Turn};
use afrobim_corridor::Side;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::crossing::find_crossings;
use crate::error::JunctionError;
use crate::geom::{add, cross, dir, dist, dot, heading_of, offset_alignment, pt_at, scale, station_offset, sub, Pt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arm {
    /// The part of the road below the crossing station.
    Behind,
    /// The part of the road above the crossing station.
    Ahead,
}

impl Arm {
    pub fn sign(self) -> f64 {
        match self {
            Arm::Behind => -1.0,
            Arm::Ahead => 1.0,
        }
    }
    fn tag(self) -> char {
        match self {
            Arm::Behind => '-',
            Arm::Ahead => '+',
        }
    }
}

/// A corner of the junction: an arm of the primary road and an arm of the secondary road.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Quadrant {
    pub primary_arm: Arm,
    pub secondary_arm: Arm,
}

impl Quadrant {
    /// The four quadrants.
    pub const ALL: [Quadrant; 4] = [
        Quadrant { primary_arm: Arm::Ahead, secondary_arm: Arm::Ahead },
        Quadrant { primary_arm: Arm::Ahead, secondary_arm: Arm::Behind },
        Quadrant { primary_arm: Arm::Behind, secondary_arm: Arm::Behind },
        Quadrant { primary_arm: Arm::Behind, secondary_arm: Arm::Ahead },
    ];
    /// `P+S-`: primary arm ahead, secondary arm behind.
    pub fn label(&self) -> String {
        format!("P{}S{}", self.primary_arm.tag(), self.secondary_arm.tag())
    }
}

/// How the kerb return of a quadrant is formed. Every number is entered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnKind {
    /// One circular arc of this radius tangent to both edges.
    Simple { radius_m: f64 },
    /// Three tangent arcs; see the module documentation. `radius1_m` is the arc on the primary
    /// edge, `radius3_m` the arc on the secondary edge; `offset1_m` and `offset3_m` as defined there.
    Compound { radius1_m: f64, radius2_m: f64, radius3_m: f64, offset1_m: f64, offset3_m: f64 },
    /// A straight between points at these distances from the corner along the primary and secondary edges.
    Chamfer { primary_leg_m: f64, secondary_leg_m: f64 },
}

/// The definition of the return in one quadrant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReturnSpec {
    pub quadrant: Quadrant,
    pub kind: ReturnKind,
}

/// A kerb return as geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KerbReturn {
    pub quadrant: Quadrant,
    pub kind: ReturnKind,
    /// From the primary edge to the secondary edge; stations start at 0 at the primary edge.
    pub alignment: Alignment,
    /// Where the two edges meet.
    pub corner: (f64, f64),
    /// Tangent point on the primary edge, and its station on that edge alignment.
    pub primary_point: (f64, f64),
    pub primary_edge_station_m: f64,
    /// Tangent point on the secondary edge, and its station on that edge alignment.
    pub secondary_point: (f64, f64),
    pub secondary_edge_station_m: f64,
    /// Station of the corner on each edge alignment.
    pub corner_primary_edge_station_m: f64,
    pub corner_secondary_edge_station_m: f64,
    /// Centres of the arcs, in order of travel (empty for a chamfer).
    pub centres: Vec<(f64, f64)>,
    pub length_m: f64,
    /// Total turn of the return (sum of its arcs' angles), radians; 0 for a chamfer.
    pub turn_rad: f64,
    /// Corner to tangent point along the edge, m.
    pub primary_tangent_length_m: f64,
    pub secondary_tangent_length_m: f64,
    /// Distance of the tangent points from the real edges (compound returns assume straight edges), m.
    pub edge_deviation_m: f64,
    /// +1 where the return's direction at its start points the same way as the primary edge's
    /// stationing, -1 against it; the same at the end for the secondary edge.
    pub start_direction_sign: f64,
    pub end_direction_sign: f64,
    /// Side of the return alignment on which the paved quadrant lies.
    pub pavement_side: Side,
    /// The side of the primary road, and of the secondary road, whose edge the return starts and ends on.
    pub primary_side: Side,
    pub secondary_side: Side,
}

/// Everything a return construction needs about its quadrant.
pub struct QuadrantFrame<'a> {
    pub quadrant: Quadrant,
    pub e1: &'a Alignment,
    pub e1_side: Side,
    pub e2: &'a Alignment,
    pub e2_side: Side,
    /// The crossing point.
    pub origin: Pt,
    /// Unit vector of the primary arm and of the secondary arm, from the crossing.
    pub dir_a: Pt,
    pub dir_b: Pt,
}

fn fit_err(q: &Quadrant, m: impl Into<String>) -> JunctionError {
    JunctionError::ReturnDoesNotFit { quadrant: q.label(), message: m.into() }
}

fn check_positive(q: &Quadrant, what: &str, v: f64) -> Result<(), JunctionError> {
    if v.is_finite() && v > 0.0 {
        Ok(())
    } else {
        Err(JunctionError::input(format!("return {}: {what} must be greater than zero (got {v})", q.label())))
    }
}

/// Builds the return of a quadrant.
pub fn build_return(f: &QuadrantFrame, kind: &ReturnKind, name: &str) -> Result<KerbReturn, JunctionError> {
    let q = f.quadrant;
    let corners = find_crossings(f.e1, f.e2, 0.0);
    let corner = corners
        .into_iter()
        .min_by(|a, b| dist((a.x, a.y), f.origin).total_cmp(&dist((b.x, b.y), f.origin)))
        .ok_or_else(|| fit_err(&q, "the two edges do not meet (the extents are too short, or the roads are parallel)"))?;
    let c: Pt = (corner.x, corner.y);
    let (cs1, cs2) = (corner.station_a_m, corner.station_b_m);
    let ex1 = e_dir(f.e1, cs1, f.dir_a);
    let ex2 = e_dir(f.e2, cs2, f.dir_b);

    let (elements, centres, turn_rad): (Vec<AlignmentElement>, Vec<Pt>, f64) = match kind {
        ReturnKind::Simple { radius_m } => {
            let r = *radius_m;
            check_positive(&q, "the radius", r)?;
            let o1 = offset_alignment(f.e1, f.e1_side.sign() * r, "offset 1")?;
            let o2 = offset_alignment(f.e2, f.e2_side.sign() * r, "offset 2")?;
            let centre = find_crossings(&o1, &o2, 0.0)
                .into_iter()
                .min_by(|a, b| dist((a.x, a.y), c).total_cmp(&dist((b.x, b.y), c)))
                .ok_or_else(|| fit_err(&q, format!("the edges offset by the radius {r:.3} m do not meet within the junction extent: the radius is too large for the angle between the edges, their offsets or the extent")))?;
            let centre: Pt = (centre.x, centre.y);
            let (s1, o_1) = station_offset(f.e1, centre)?;
            let (s2, o_2) = station_offset(f.e2, centre)?;
            for (o, which) in [(o_1, "primary"), (o_2, "secondary")] {
                if (o.abs() - r).abs() > 1e-5 * r.max(1.0) {
                    return Err(fit_err(&q, format!("the centre is {:.4} m from the {which} edge, not the radius {r:.4} m: the edge curves too sharply for a circle of this radius", o.abs())));
                }
            }
            let (t1, t2) = (pt_at(f.e1, s1)?, pt_at(f.e2, s2)?);
            let (v1, v2) = (sub(t1, centre), sub(t2, centre));
            let ccw = cross(v1, v2) > 0.0;
            let phi = cross(v1, v2).abs().atan2(dot(v1, v2));
            let h = heading_of(v1) + if ccw { PI / 2.0 } else { -PI / 2.0 };
            let turn = if ccw { Turn::Left } else { Turn::Right };
            (vec![AlignmentElement::arc(t1.0, t1.1, h, r, r * phi, turn)], vec![centre], phi)
        }
        ReturnKind::Chamfer { primary_leg_m, secondary_leg_m } => {
            check_positive(&q, "the primary leg", *primary_leg_m)?;
            check_positive(&q, "the secondary leg", *secondary_leg_m)?;
            let s1 = cs1 + sign_along(f.e1, cs1, f.dir_a) * primary_leg_m;
            let s2 = cs2 + sign_along(f.e2, cs2, f.dir_b) * secondary_leg_m;
            let (t1, t2) = (
                pt_at(f.e1, s1).map_err(|_| fit_err(&q, format!("the primary leg of {primary_leg_m:.3} m runs off the end of the edge")))?,
                pt_at(f.e2, s2).map_err(|_| fit_err(&q, format!("the secondary leg of {secondary_leg_m:.3} m runs off the end of the edge")))?,
            );
            let v = sub(t2, t1);
            (vec![AlignmentElement::line(t1.0, t1.1, heading_of(v), dist(t1, t2))], vec![], 0.0)
        }
        ReturnKind::Compound { radius1_m, radius2_m, radius3_m, offset1_m, offset3_m } => {
            for (w, v) in [("the first radius", radius1_m), ("the second radius", radius2_m), ("the third radius", radius3_m)] {
                check_positive(&q, w, *v)?;
            }
            for (w, v, r) in [("the first offset", offset1_m, radius1_m), ("the third offset", offset3_m, radius3_m)] {
                if !(v.is_finite() && *v >= 0.0 && *v <= *r) {
                    return Err(JunctionError::input(format!("return {}: {w} must be from 0 to its radius (got {v})", q.label())));
                }
            }
            let h0 = heading_of(scale(ex1, -1.0));
            let delta_signed = wrap_pi(heading_of(ex2) - h0);
            let (sgn, delta) = (delta_signed.signum(), delta_signed.abs());
            let phi1 = (1.0 - offset1_m / radius1_m).acos();
            let phi3 = (1.0 - offset3_m / radius3_m).acos();
            let phi2 = delta - phi1 - phi3;
            if !(phi2 > 1e-9) {
                return Err(fit_err(
                    &q,
                    format!(
                        "the first and last arcs already turn {:.4} deg of the {:.4} deg between the edges: the offsets are too large for the angle",
                        (phi1 + phi3).to_degrees(),
                        delta.to_degrees()
                    ),
                ));
            }
            let arcs = [(*radius1_m, phi1), (*radius2_m, phi2), (*radius3_m, phi3)];
            let (mut pos, mut h) = ((0.0, 0.0), h0);
            let mut centres_rel = Vec::new();
            for (r, phi) in arcs {
                let centre = add(pos, scale((-(h).sin(), h.cos()), sgn * r));
                centres_rel.push(centre);
                let chord = 2.0 * r * (phi / 2.0).sin();
                pos = add(pos, scale(dir(h + sgn * phi / 2.0), chord));
                h += sgn * phi;
            }
            let w = pos;
            let t1 = -cross(ex2, w) / cross(ex2, ex1);
            let start = add(c, scale(ex1, t1));
            let t2 = dot(add(scale(ex1, t1), w), ex2);
            if !(t1 > 0.0 && t2 > 0.0) {
                return Err(fit_err(&q, format!("the compound curve would start {t1:.3} m and end {t2:.3} m from the corner along the edges: a tangent length is not positive")));
            }
            let mut b = AlignmentBuilder::new(name, 0.0, start.0, start.1, h0);
            let turn = if sgn > 0.0 { Turn::Left } else { Turn::Right };
            for (r, phi) in arcs {
                if phi > 1e-12 {
                    b = b.arc(r, r * phi, turn);
                }
            }
            let al = b.build().map_err(|e| JunctionError::geometry(e.to_string()))?;
            let centres = centres_rel.into_iter().map(|p| add(start, p)).collect();
            (al.elements, centres, delta)
        }
    };

    let alignment = Alignment::from_elements(name, 0.0, elements).map_err(|e| JunctionError::geometry(e.to_string()))?;
    let length_m = alignment.length();
    let p_start = alignment.point_at(0.0).ok_or_else(|| JunctionError::geometry("empty return"))?;
    let p_end = alignment.point_at(length_m).ok_or_else(|| JunctionError::geometry("empty return"))?;
    let (t1, t2): (Pt, Pt) = ((p_start.x, p_start.y), (p_end.x, p_end.y));
    let (s1, o1) = f.e1.station_offset(t1.0, t1.1).ok_or_else(|| fit_err(&q, "the tangent point on the primary edge lies beyond the end of the edge alignment: lengthen the junction extent"))?;
    let (s2, o2) = f.e2.station_offset(t2.0, t2.1).ok_or_else(|| fit_err(&q, "the tangent point on the secondary edge lies beyond the end of the edge alignment: lengthen the junction extent"))?;
    if matches!(kind, ReturnKind::Simple { .. }) && (o1.abs() > 1e-4 || o2.abs() > 1e-4) {
        return Err(JunctionError::geometry(format!("internal check: a tangent point is {:.6} m off its edge", o1.abs().max(o2.abs()))));
    }
    // The tangent points must be on the arm side of the corner.
    for (s, cs, e, dir_arm, which) in [(s1, cs1, f.e1, f.dir_a, "primary"), (s2, cs2, f.e2, f.dir_b, "secondary")] {
        let along = sign_along(e, cs, dir_arm) * (s - cs);
        if along < -1e-9 {
            return Err(fit_err(&q, format!("the tangent point on the {which} edge falls {:.3} m on the wrong side of the corner: the radius is too large for the angle and the offsets", -along)));
        }
    }
    let hd = |e: &Alignment, s: f64| e.point_at(s).map(|p| dir(p.heading_rad)).unwrap_or((1.0, 0.0));
    let start_direction_sign = dot(dir(p_start.heading_rad), hd(f.e1, s1)).signum();
    let end_direction_sign = dot(dir(p_end.heading_rad), hd(f.e2, s2)).signum();
    let ccw_total = alignment.elements.first().map(|e| e.curvature_start > 0.0).unwrap_or(false);
    let pavement_side = if turn_rad == 0.0 {
        // A chamfer: the paved quadrant is on the side of the corner.
        if cross(dir(p_start.heading_rad), sub(c, t1)) > 0.0 {
            Side::Left
        } else {
            Side::Right
        }
    } else if ccw_total {
        Side::Right
    } else {
        Side::Left
    };
    Ok(KerbReturn {
        quadrant: q,
        kind: kind.clone(),
        alignment,
        corner: c,
        primary_point: t1,
        primary_edge_station_m: s1,
        secondary_point: t2,
        secondary_edge_station_m: s2,
        corner_primary_edge_station_m: cs1,
        corner_secondary_edge_station_m: cs2,
        centres,
        length_m,
        turn_rad,
        primary_tangent_length_m: (s1 - cs1).abs(),
        secondary_tangent_length_m: (s2 - cs2).abs(),
        edge_deviation_m: o1.abs().max(o2.abs()),
        start_direction_sign,
        end_direction_sign,
        pavement_side,
        primary_side: f.e1_side,
        secondary_side: f.e2_side,
    })
}

/// +1 where the edge's stationing at `station` runs the same way as `arm_dir`, else -1.
fn sign_along(e: &Alignment, station: f64, arm_dir: Pt) -> f64 {
    let h = e.point_at(station).map(|p| dir(p.heading_rad)).unwrap_or((1.0, 0.0));
    if dot(h, arm_dir) >= 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// Unit direction of an edge at a station, pointing the way of `arm_dir`.
fn e_dir(e: &Alignment, station: f64, arm_dir: Pt) -> Pt {
    let h = e.point_at(station).map(|p| dir(p.heading_rad)).unwrap_or((1.0, 0.0));
    scale(h, sign_along(e, station, arm_dir))
}
