// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Coordinate geometry: inverse, direct, intersections, offsets.
//!
//! Plane formulae, standard in any surveying text (for example Ghilani and Wolf, *Elementary
//! Surveying*, the chapters on coordinate computations and COGO; or Schofield and Breach,
//! *Engineering Surveying*). Each is also derived in the comments below. They work on the plane
//! grid: no scale factor, convergence or curvature is applied, so distances are grid distances.
//!
//! A bearing is a whole circle bearing, degrees clockwise from north. The unit vector of bearing b
//! is (sin b, cos b) as (east, north). All computations use differences from a local origin, so the
//! size of UTM coordinates does not cost precision.

use serde::{Deserialize, Serialize};

use crate::angle::{format_bearing, normalize_deg};
use crate::error::{CogoError, Result};
use crate::geom::{bearing_of, cross, dir, dot, seg_nearest, Xy};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Inverse {
    /// Plan distance.
    pub distance: f64,
    /// Bearing from a to b; `None` when the points coincide.
    pub bearing_deg: Option<f64>,
    pub dz: Option<f64>,
    /// Grade in percent, dz / distance * 100; `None` without elevations or with zero distance.
    pub grade_pct: Option<f64>,
}

impl Inverse {
    /// `DDD°MM'SS"` for display.
    pub fn bearing_text(&self, sec_decimals: usize) -> Option<String> {
        self.bearing_deg.map(|b| format_bearing(b, sec_decimals))
    }
}

/// Inverse: distance = sqrt(dE^2 + dN^2), bearing = atan2(dE, dN) (dE over dN so that north is 0 and
/// the angle grows clockwise).
pub fn inverse(a: Xy, za: Option<f64>, b: Xy, zb: Option<f64>) -> Inverse {
    let (de, dn) = b.sub(a);
    let distance = de.hypot(dn);
    let dz = match (za, zb) {
        (Some(x), Some(y)) => Some(y - x),
        _ => None,
    };
    Inverse {
        distance,
        bearing_deg: if distance > 0.0 { Some(bearing_of(de, dn)) } else { None },
        dz,
        grade_pct: match dz {
            Some(d) if distance > 0.0 => Some(d / distance * 100.0),
            _ => None,
        },
    }
}

/// Direct (forward) computation: E = E0 + d sin b, N = N0 + d cos b.
pub fn point_by_bearing_distance(from: Xy, bearing_deg: f64, distance: f64) -> Xy {
    let (s, c) = dir(bearing_deg);
    from.add(distance * s, distance * c)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Turn {
    /// Angle measured clockwise from the backsight (angle right).
    Right,
    /// Angle measured anticlockwise from the backsight (angle left).
    Left,
}

/// A point by turned angle and distance from `occupied`, with the zero direction towards `backsight`.
/// Bearing of the new line = bearing(occupied to backsight) + angle (right) or - angle (left).
pub fn point_by_turned_angle(occupied: Xy, backsight: Xy, angle_deg: f64, turn: Turn, distance: f64) -> Result<Xy> {
    let (de, dn) = backsight.sub(occupied);
    if de == 0.0 && dn == 0.0 {
        return Err(CogoError::Invalid("backsight coincides with the occupied point".into()));
    }
    let b0 = bearing_of(de, dn);
    let b = match turn {
        Turn::Right => b0 + angle_deg,
        Turn::Left => b0 - angle_deg,
    };
    Ok(point_by_bearing_distance(occupied, normalize_deg(b), distance))
}

/// A solution of a line meeting a line or circle: the point and the signed distance along each
/// line from its origin (negative: behind the origin).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LineHit {
    pub point: Xy,
    pub along_first: f64,
    pub along_second: Option<f64>,
}

/// Bearing-bearing intersection. Lines p1 + t d1 and p2 + s d2 with d = (sin b, cos b).
/// Equating: t d1 - s d2 = p2 - p1 = w. Cross both sides with d2: t (d1 x d2) = w x d2, so
/// t = (w x d2) / (d1 x d2); likewise s = (w x d1) / (d1 x d2). d1 x d2 = sin(b1 - b2); it is zero
/// for parallel lines (no unique solution: `NoIntersection`). `parallel_eps` is the smallest
/// |sin(b1 - b2)| accepted.
pub fn intersect_bearing_bearing(p1: Xy, b1: f64, p2: Xy, b2: f64, parallel_eps: f64) -> Result<LineHit> {
    let d1 = dir(b1);
    let d2 = dir(b2);
    let den = cross(d1, d2);
    if den.abs() <= parallel_eps.max(0.0) || den == 0.0 {
        return Err(CogoError::NoIntersection);
    }
    let w = p2.sub(p1);
    let t = cross(w, d2) / den;
    let s = cross(w, d1) / den;
    Ok(LineHit { point: p1.add(t * d1.0, t * d1.1), along_first: t, along_second: Some(s) })
}

/// Bearing-distance intersection: the line from `p1` on `bearing` against the circle (centre `c`,
/// radius `r`). With w = p1 - c, |w + t d|^2 = r^2 gives t^2 + 2 (d.w) t + (w.w - r^2) = 0, so
/// t = -(d.w) +/- sqrt((d.w)^2 - (w.w - r^2)). All real solutions are returned, ascending in t (zero,
/// one when tangent within 1e-12 relative, or two). `along_first` is t; it is negative for a hit
/// behind `p1`.
pub fn intersect_bearing_distance(p1: Xy, bearing_deg: f64, c: Xy, r: f64) -> Result<Vec<LineHit>> {
    if !(r >= 0.0) || !r.is_finite() {
        return Err(CogoError::Invalid("radius must be zero or more".into()));
    }
    let d = dir(bearing_deg);
    let w = p1.sub(c);
    let b = dot(d, w);
    let disc = b * b - (dot(w, w) - r * r);
    let scale = (b * b).max(r * r).max(1e-300);
    let mut ts = Vec::new();
    if disc.abs() <= 1e-12 * scale {
        ts.push(-b);
    } else if disc > 0.0 {
        let s = disc.sqrt();
        ts.push(-b - s);
        ts.push(-b + s);
    }
    Ok(ts.into_iter().map(|t| LineHit { point: p1.add(t * d.0, t * d.1), along_first: t, along_second: None }).collect())
}

/// Distance-distance intersection of circles (c1, r1) and (c2, r2). With D = |c2 - c1| and u the unit
/// vector c1 to c2: a = (r1^2 - r2^2 + D^2) / (2 D) is the distance from c1 along u to the chord, and
/// h = sqrt(r1^2 - a^2) the half chord. Solutions c1 + a u +/- h v, v = perpendicular to u. The first
/// solution is on the right of c1 to c2 (v = (u_n, -u_e)), the second on the left; one solution when
/// the circles touch (within 1e-12 relative), none (empty) when they do not meet. Concentric circles
/// give an error.
pub fn intersect_distance_distance(c1: Xy, r1: f64, c2: Xy, r2: f64) -> Result<Vec<Xy>> {
    if !(r1 >= 0.0 && r2 >= 0.0) || !r1.is_finite() || !r2.is_finite() {
        return Err(CogoError::Invalid("radii must be zero or more".into()));
    }
    let (de, dn) = c2.sub(c1);
    let d = de.hypot(dn);
    if d == 0.0 {
        return Err(CogoError::NoIntersection);
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h2 = r1 * r1 - a * a;
    let scale = (r1 * r1).max(r2 * r2).max(1e-300);
    let (ue, un) = (de / d, dn / d);
    let base = c1.add(a * ue, a * un);
    if h2.abs() <= 1e-12 * scale {
        return Ok(vec![base]);
    }
    if h2 < 0.0 {
        return Ok(vec![]);
    }
    let h = h2.sqrt();
    Ok(vec![base.add(h * un, -h * ue), base.add(-h * un, h * ue)])
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LineOffset {
    /// Distance from `a` along a to b of the foot of the perpendicular (may be outside 0..length).
    pub station: f64,
    /// Perpendicular distance, positive to the right of a to b.
    pub offset: f64,
    pub foot: Xy,
    pub within_segment: bool,
}

/// Station and offset of `p` about the line a to b: with u the unit vector a to b and w = p - a,
/// station = w.u, offset (right positive) = u x w taken with a sign such that the right is positive:
/// right normal of u = (u_n, -u_e) so offset = w.(u_n, -u_e) = -(u x w) with x the 2D cross.
pub fn offset_to_line(p: Xy, a: Xy, b: Xy) -> Result<LineOffset> {
    let v = b.sub(a);
    let l = v.0.hypot(v.1);
    if l == 0.0 {
        return Err(CogoError::Invalid("line has zero length".into()));
    }
    let u = (v.0 / l, v.1 / l);
    let w = p.sub(a);
    let station = dot(w, u);
    let offset = w.0 * u.1 - w.1 * u.0;
    Ok(LineOffset { station, offset, foot: a.add(station * u.0, station * u.1), within_segment: (0.0..=l).contains(&station) })
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StationOffset {
    /// Station along the polyline of the nearest point, plus the start station.
    pub station: f64,
    /// Distance to the nearest point, positive to the right of the polyline direction.
    pub offset: f64,
    pub foot: Xy,
    pub segment: usize,
    /// The nearest point is an end of the polyline and the perpendicular falls outside it.
    pub beyond_end: bool,
}

pub fn polyline_length(pl: &[Xy]) -> f64 {
    pl.windows(2).map(|w| w[0].dist(w[1])).sum()
}

/// Station and offset of `p` about a polyline: the nearest point of the polyline, its chainage from
/// the start (plus `start_station`), and the signed distance. Where the nearest point is a vertex
/// shared by two segments (an outside corner), the segment that gives the smaller distance is
/// used, the earlier one on a tie; the offset sign is that segment's.
pub fn station_offset(pl: &[Xy], p: Xy, start_station: f64) -> Result<StationOffset> {
    if pl.len() < 2 {
        return Err(CogoError::Invalid("polyline needs two points".into()));
    }
    let mut best: Option<(f64, usize, f64)> = None;
    for i in 0..pl.len() - 1 {
        if pl[i] == pl[i + 1] {
            continue;
        }
        let (d, t) = seg_nearest(p, pl[i], pl[i + 1]);
        if best.map_or(true, |b| d < b.0 - 1e-12) {
            best = Some((d, i, t));
        }
    }
    let (d, i, t) = best.ok_or_else(|| CogoError::Invalid("polyline has no length".into()))?;
    let len_i = pl[i].dist(pl[i + 1]);
    let before: f64 = pl[..=i].windows(2).map(|w| w[0].dist(w[1])).sum();
    let lo = offset_to_line(p, pl[i], pl[i + 1])?;
    let offset = if d == 0.0 { 0.0 } else { lo.offset.signum() * d };
    let foot = pl[i].add((pl[i + 1].e - pl[i].e) * t, (pl[i + 1].n - pl[i].n) * t);
    let n = pl.len() - 1;
    let beyond_end = (i == 0 && lo.station < 0.0) || (i == n - 1 && lo.station > len_i);
    Ok(StationOffset { station: start_station + before + t * len_i, offset, foot, segment: i, beyond_end })
}

/// Point on the line a to b at `distance` from a (negative: behind a), optionally pushed `offset` to
/// the right of the line direction.
pub fn point_on_line(a: Xy, b: Xy, distance: f64, offset: f64) -> Result<Xy> {
    let v = b.sub(a);
    let l = v.0.hypot(v.1);
    if l == 0.0 {
        return Err(CogoError::Invalid("line has zero length".into()));
    }
    let u = (v.0 / l, v.1 / l);
    Ok(a.add(distance * u.0 + offset * u.1, distance * u.1 - offset * u.0))
}

/// Point at chainage `station` (from `start_station`) along a polyline, pushed `offset` to the
/// right. Beyond either end the end segment is extended in its own direction.
pub fn point_at_station(pl: &[Xy], start_station: f64, station: f64, offset: f64) -> Result<Xy> {
    if pl.len() < 2 {
        return Err(CogoError::Invalid("polyline needs two points".into()));
    }
    let mut s = station - start_station;
    let segs: Vec<(Xy, Xy)> = pl.windows(2).filter(|w| w[0] != w[1]).map(|w| (w[0], w[1])).collect();
    if segs.is_empty() {
        return Err(CogoError::Invalid("polyline has no length".into()));
    }
    if s <= 0.0 {
        return point_on_line(segs[0].0, segs[0].1, s, offset);
    }
    for (i, (a, b)) in segs.iter().enumerate() {
        let l = a.dist(*b);
        if s <= l || i == segs.len() - 1 {
            return point_on_line(*a, *b, s, offset);
        }
        s -= l;
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, t: f64) -> bool {
        (a - b).abs() <= t
    }

    #[test]
    fn inverse_and_direct_at_utm() {
        let a = Xy::new(500000.0, 9999990.0);
        let b = Xy::new(500030.0, 10000030.0);
        let i = inverse(a, Some(10.0), b, Some(12.5));
        assert!(close(i.distance, 50.0, 1e-9));
        assert!(close(i.bearing_deg.unwrap(), (30.0f64).atan2(40.0).to_degrees(), 1e-12));
        assert!(close(i.grade_pct.unwrap(), 5.0, 1e-9));
        let back = point_by_bearing_distance(a, i.bearing_deg.unwrap(), i.distance);
        assert!(close(back.e, b.e, 1e-9) && close(back.n, b.n, 1e-9));
        assert_eq!(inverse(a, None, a, None).bearing_deg, None);
        assert!(inverse(a, None, b, Some(1.0)).grade_pct.is_none());
        assert!(i.bearing_text(1).unwrap().contains('\u{b0}'));
    }

    #[test]
    fn turned_angle() {
        let o = Xy::new(0.0, 0.0);
        let bs = Xy::new(0.0, 10.0); // north
        let p = point_by_turned_angle(o, bs, 90.0, Turn::Right, 5.0).unwrap();
        assert!(close(p.e, 5.0, 1e-12) && close(p.n, 0.0, 1e-12));
        let p = point_by_turned_angle(o, bs, 90.0, Turn::Left, 5.0).unwrap();
        assert!(close(p.e, -5.0, 1e-12));
        assert!(point_by_turned_angle(o, o, 1.0, Turn::Left, 1.0).is_err());
    }

    #[test]
    fn bearing_bearing() {
        let h = intersect_bearing_bearing(Xy::new(0.0, 0.0), 45.0, Xy::new(10.0, 0.0), 315.0, 1e-12).unwrap();
        assert!(close(h.point.e, 5.0, 1e-12) && close(h.point.n, 5.0, 1e-12));
        assert!(close(h.along_first, 50f64.sqrt(), 1e-12));
        assert!(intersect_bearing_bearing(Xy::new(0.0, 0.0), 10.0, Xy::new(10.0, 0.0), 190.0, 1e-12).is_err());
        // behind the origin gives a negative distance
        let h = intersect_bearing_bearing(Xy::new(0.0, 0.0), 225.0, Xy::new(10.0, 0.0), 315.0, 1e-12).unwrap();
        assert!(h.along_first < 0.0);
    }

    #[test]
    fn bearing_distance_all_solutions() {
        let c = Xy::new(0.0, 0.0);
        let h = intersect_bearing_distance(Xy::new(-10.0, 3.0), 90.0, c, 5.0).unwrap();
        assert_eq!(h.len(), 2);
        assert!(close(h[0].point.e, -4.0, 1e-9) && close(h[1].point.e, 4.0, 1e-9));
        let h = intersect_bearing_distance(Xy::new(-10.0, 5.0), 90.0, c, 5.0).unwrap();
        assert_eq!(h.len(), 1);
        assert!(intersect_bearing_distance(Xy::new(-10.0, 6.0), 90.0, c, 5.0).unwrap().is_empty());
        // start inside the circle: one hit behind, one ahead
        let h = intersect_bearing_distance(Xy::new(0.0, 0.0), 0.0, c, 5.0).unwrap();
        assert!(h[0].along_first < 0.0 && h[1].along_first > 0.0);
    }

    #[test]
    fn distance_distance_at_utm() {
        let c1 = Xy::new(500000.0, 9999990.0);
        let c2 = Xy::new(500010.0, 9999990.0);
        let s = intersect_distance_distance(c1, 6.0, c2, 8.0).unwrap();
        assert_eq!(s.len(), 2);
        // a = (36 - 64 + 100)/20 = 3.6; h = 4.8; right of west-to-east is south
        assert!(close(s[0].e, 500003.6, 1e-9) && close(s[0].n, 9999990.0 - 4.8, 1e-9));
        assert!(close(s[1].n, 9999990.0 + 4.8, 1e-9));
        assert_eq!(intersect_distance_distance(c1, 5.0, c2, 5.0).unwrap().len(), 1);
        assert!(intersect_distance_distance(c1, 1.0, c2, 1.0).unwrap().is_empty());
        assert!(intersect_distance_distance(c1, 1.0, c1, 2.0).is_err());
    }

    #[test]
    fn offsets() {
        let a = Xy::new(0.0, 0.0);
        let b = Xy::new(10.0, 0.0);
        let o = offset_to_line(Xy::new(4.0, -3.0), a, b).unwrap();
        assert!(close(o.station, 4.0, 1e-12) && close(o.offset, 3.0, 1e-12)); // south is right of east
        let o = offset_to_line(Xy::new(4.0, 3.0), a, b).unwrap();
        assert!(close(o.offset, -3.0, 1e-12));
        assert!(!offset_to_line(Xy::new(12.0, 1.0), a, b).unwrap().within_segment);

        let pl = [Xy::new(1000000.0, 9000000.0), Xy::new(1000010.0, 9000000.0), Xy::new(1000010.0, 9000010.0)];
        let so = station_offset(&pl, Xy::new(1000012.0, 9000004.0), 100.0).unwrap();
        assert!(close(so.station, 114.0, 1e-9) && close(so.offset, 2.0, 1e-9) && so.segment == 1, "{so:?}");
        // left of the first leg
        let so = station_offset(&pl, Xy::new(1000004.0, 9000001.0), 0.0).unwrap();
        assert!(close(so.station, 4.0, 1e-9) && close(so.offset, -1.0, 1e-9));
        let so = station_offset(&pl, Xy::new(1000010.0, 9000015.0), 0.0).unwrap();
        assert!(so.beyond_end);
        assert!(close(polyline_length(&pl), 20.0, 1e-12));
    }

    #[test]
    fn points_on_lines() {
        let a = Xy::new(0.0, 0.0);
        let b = Xy::new(0.0, 10.0);
        let p = point_on_line(a, b, 4.0, 2.0).unwrap(); // right of north is east
        assert!(close(p.e, 2.0, 1e-12) && close(p.n, 4.0, 1e-12));
        let pl = [a, b, Xy::new(10.0, 10.0)];
        let p = point_at_station(&pl, 100.0, 115.0, 0.0).unwrap();
        assert!(close(p.e, 5.0, 1e-12) && close(p.n, 10.0, 1e-12));
        let p = point_at_station(&pl, 100.0, 95.0, 0.0).unwrap();
        assert!(close(p.n, -5.0, 1e-12));
    }
}
