// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Road reserve tools: split a parcel by the right-of-way edge, the land acquisition schedule, the
//! parcel area table.
//!
//! # Split
//!
//! The right-of-way edge is a polyline (given directly, or made from a centreline with
//! [`offset_polyline`]). It is put in a planar arrangement with the flattened parcel boundary
//! (`arrangement.rs`), which cuts the parcel into pieces. A piece is on the **left** of the edge when it
//! lies to the left of a cutter edge, **right** otherwise; a piece touching no cutter edge is placed
//! by the side of the nearest point of the cutter. The caller says which side is the take.
//! `take_area` is the area of the take pieces; `remainder_area` is the parcel's exact area (arcs
//! included) minus the take, so the two always add up to the parcel. The pieces themselves use arcs
//! flattened to `arc_tol_m`; where an arc forms part of a piece the piece area is short by at most
//! (2/3) x arc length x `arc_tol_m` (reported in `arc_area_error_bound_m2`, and `pieces_check_m2`
//! gives remainder pieces plus take pieces minus the parcel area).
//!
//! The edge need not stop at the parcel boundary; parts outside are ignored. An edge that does not
//! cross the parcel leaves it whole on one side.
//!
//! # Schedule
//!
//! One row per parcel: total, take and remainder areas in m2 and hectares (1 ha = 10 000 m2).
//! Acres appear only as a derived display column, converted with [`crate::parcel::ACRE_M2`].

use serde::{Deserialize, Serialize};

use crate::calc::station_offset;
use crate::error::{CogoError, Result};
use crate::geom::{cross, dot, ring_area, ring_winding, Xy};
use crate::parcel::{segs_cross, Parcel, ACRE_M2, HECTARE_M2};
use crate::topology::arc_area_error_bound;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SplitOptions {
    pub snap_tol_m: f64,
    pub arc_tol_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Piece {
    pub ring: Vec<Xy>,
    pub holes: Vec<Vec<Xy>>,
    pub area_m2: f64,
    pub side: Side,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitResult {
    pub parcel_area_m2: f64,
    pub take_area_m2: f64,
    pub remainder_area_m2: f64,
    pub take_pieces: Vec<Piece>,
    pub remainder_pieces: Vec<Piece>,
    pub pieces_check_m2: f64,
    pub arc_area_error_bound_m2: f64,
}

/// Split one parcel by the polyline `cutter`; `take_side` is the side of the cutter that is taken.
pub fn split_parcel(parcel: &Parcel, cutter: &[Xy], take_side: Side, o: &SplitOptions) -> Result<SplitResult> {
    if cutter.len() < 2 {
        return Err(CogoError::Invalid("the cutting polyline needs two points".into()));
    }
    if !(o.arc_tol_m > 0.0) {
        return Err(CogoError::Invalid("arc tolerance must be positive".into()));
    }
    let mut ring = parcel.boundary.flatten(o.arc_tol_m)?;
    if ring_area(&ring) < 0.0 {
        ring.reverse();
    }
    let total = parcel.boundary.area()?;
    let arr = crate::arrangement::build(&[(ring.clone(), true, 0), (cutter.to_vec(), false, 1)], o.snap_tol_m)?;
    let mut cut_dir: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    for p in &arr.pieces {
        if p.tag == 1 {
            cut_dir.insert((p.from, p.to));
        }
    }
    let (mut take, mut rest) = (Vec::new(), Vec::new());
    for f in &arr.faces {
        let Some(ip) = f.interior_point() else { continue };
        if ring_winding(&ring, ip) == 0 {
            continue;
        }
        let area = f.net_area();
        let left = f.half_edges.iter().any(|h| cut_dir.contains(h));
        let right = f.half_edges.iter().any(|h| cut_dir.contains(&(h.1, h.0)));
        let side = match (left, right) {
            (true, false) => Side::Left,
            (false, true) => Side::Right,
            _ => {
                // no cutter edge on the piece, or both: use the side of the nearest point
                let so = station_offset(cutter, ip, 0.0)?;
                if so.offset > 0.0 {
                    Side::Right
                } else {
                    Side::Left
                }
            }
        };
        let piece = Piece { ring: f.ring.clone(), holes: f.holes.clone(), area_m2: area, side };
        if side == take_side {
            take.push(piece);
        } else {
            rest.push(piece);
        }
    }
    let take_area: f64 = take.iter().map(|p| p.area_m2).sum();
    let rest_area: f64 = rest.iter().map(|p| p.area_m2).sum();
    Ok(SplitResult {
        parcel_area_m2: total,
        take_area_m2: take_area,
        remainder_area_m2: total - take_area,
        take_pieces: take,
        remainder_pieces: rest,
        pieces_check_m2: take_area + rest_area - total,
        arc_area_error_bound_m2: arc_area_error_bound(std::slice::from_ref(parcel), o.arc_tol_m)?,
    })
}

/// A polyline moved sideways: `distance` positive to the left of the direction of travel, negative
/// to the right. Each segment is moved by the normal (-u_n, u_e); consecutive moved lines meet at their
/// intersection (a mitre). If the mitre point is further than `miter_limit` x |distance| from the
/// original vertex the corner is bevelled (both moved end points kept). Fails with
/// `OffsetSelfIntersects` when a moved segment reverses (an inside corner shorter than the offset) or
/// the result crosses itself. Arcs are not handled: pass a flattened centreline.
pub fn offset_polyline(pl: &[Xy], distance: f64, miter_limit: f64) -> Result<Vec<Xy>> {
    if pl.len() < 2 {
        return Err(CogoError::Invalid("polyline needs two points".into()));
    }
    let segs: Vec<(Xy, Xy)> = pl.windows(2).filter(|w| w[0] != w[1]).map(|w| (w[0], w[1])).collect();
    if segs.is_empty() {
        return Err(CogoError::Invalid("polyline has no length".into()));
    }
    let moved: Vec<(Xy, Xy, (f64, f64))> = segs
        .iter()
        .map(|(a, b)| {
            let (de, dn) = b.sub(*a);
            let l = de.hypot(dn);
            let u = (de / l, dn / l);
            let nrm = (-u.1 * distance, u.0 * distance);
            (a.add(nrm.0, nrm.1), b.add(nrm.0, nrm.1), u)
        })
        .collect();
    let n = moved.len();
    let mut starts: Vec<Xy> = moved.iter().map(|m| m.0).collect();
    let mut ends: Vec<Xy> = moved.iter().map(|m| m.1).collect();
    for i in 0..n - 1 {
        let (a1, _, u1) = moved[i];
        let (a2, _, u2) = moved[i + 1];
        let den = cross(u1, u2);
        if den.abs() < 1e-12 {
            continue; // parallel: keep both moved ends
        }
        let t = cross(a2.sub(a1), u2) / den;
        let x = a1.add(t * u1.0, t * u1.1);
        if x.dist(segs[i].1) <= miter_limit * distance.abs() {
            ends[i] = x;
            starts[i + 1] = x;
        }
    }
    // every moved segment must still run the way its original does
    for i in 0..n {
        if dot(ends[i].sub(starts[i]), moved[i].2) < 0.0 {
            return Err(CogoError::OffsetSelfIntersects);
        }
    }
    let mut out = vec![starts[0]];
    for i in 0..n {
        if *out.last().unwrap() != ends[i] {
            out.push(ends[i]);
        }
        if i + 1 < n && *out.last().unwrap() != starts[i + 1] {
            out.push(starts[i + 1]);
        }
    }
    let m = out.len();
    for i in 0..m.saturating_sub(1) {
        for k in i + 2..m - 1 {
            if segs_cross(out[i], out[i + 1], out[k], out[k + 1]) {
                return Err(CogoError::OffsetSelfIntersects);
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcquisitionRow {
    pub parcel_number: u32,
    pub parcel_name: String,
    pub land_reference: String,
    pub owner: String,
    pub status: String,
    pub total_area_m2: f64,
    pub take_area_m2: f64,
    pub remainder_area_m2: f64,
    pub total_ha: f64,
    pub take_ha: f64,
    pub remainder_ha: f64,
    /// True when the take is more than zero.
    pub affected: bool,
}

/// The acquisition schedule for a set of parcels against one right-of-way edge. Every parcel gets a
/// row; `affected` marks those with a take.
pub fn acquisition_schedule(parcels: &[Parcel], row_edge: &[Xy], take_side: Side, o: &SplitOptions) -> Result<Vec<AcquisitionRow>> {
    parcels
        .iter()
        .map(|p| {
            let r = split_parcel(p, row_edge, take_side, o)?;
            Ok(AcquisitionRow {
                parcel_number: p.number,
                parcel_name: p.name.clone(),
                land_reference: p.land_reference.clone(),
                owner: p.owner.clone(),
                status: p.status.label(),
                total_area_m2: r.parcel_area_m2,
                take_area_m2: r.take_area_m2,
                remainder_area_m2: r.remainder_area_m2,
                total_ha: r.parcel_area_m2 / HECTARE_M2,
                take_ha: r.take_area_m2 / HECTARE_M2,
                remainder_ha: r.remainder_area_m2 / HECTARE_M2,
                affected: r.take_area_m2 > 0.0,
            })
        })
        .collect()
}

pub fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The schedule as CSV. Areas in m2 have `m2_decimals` places, hectares `ha_decimals`. With
/// `include_acres`, three extra columns give the areas in international acres, derived from the m2
/// values (a display only). `only_affected` leaves out parcels with no take.
pub fn schedule_csv(rows: &[AcquisitionRow], m2_decimals: usize, ha_decimals: usize, include_acres: bool, only_affected: bool) -> String {
    let mut s = String::from("Parcel,Name,Land reference,Owner,Status,Total area m2,Take area m2,Remainder area m2,Total ha,Take ha,Remainder ha");
    if include_acres {
        s.push_str(",Total acres (derived),Take acres (derived),Remainder acres (derived)");
    }
    s.push('\n');
    for r in rows.iter().filter(|r| !only_affected || r.affected) {
        let mut f = vec![
            r.parcel_number.to_string(),
            csv_field(&r.parcel_name),
            csv_field(&r.land_reference),
            csv_field(&r.owner),
            csv_field(&r.status),
            format!("{:.*}", m2_decimals, r.total_area_m2),
            format!("{:.*}", m2_decimals, r.take_area_m2),
            format!("{:.*}", m2_decimals, r.remainder_area_m2),
            format!("{:.*}", ha_decimals, r.total_ha),
            format!("{:.*}", ha_decimals, r.take_ha),
            format!("{:.*}", ha_decimals, r.remainder_ha),
        ];
        if include_acres {
            for a in [r.total_area_m2, r.take_area_m2, r.remainder_area_m2] {
                f.push(format!("{:.*}", ha_decimals, a / ACRE_M2));
            }
        }
        s.push_str(&f.join(","));
        s.push('\n');
    }
    s
}

/// Parcel area table as CSV: number, name, land reference, owner, land use, status, area m2, area ha,
/// perimeter m.
pub fn area_table_csv(parcels: &[Parcel], m2_decimals: usize, ha_decimals: usize, include_acres: bool) -> Result<String> {
    let mut s = String::from("Parcel,Name,Land reference,Owner,Land use,Status,Area m2,Area ha,Perimeter m");
    if include_acres {
        s.push_str(",Area acres (derived)");
    }
    s.push('\n');
    for p in parcels {
        let a = p.area()?;
        let mut f = vec![
            p.number.to_string(),
            csv_field(&p.name),
            csv_field(&p.land_reference),
            csv_field(&p.owner),
            csv_field(&p.land_use),
            csv_field(&p.status.label()),
            format!("{:.*}", m2_decimals, a),
            format!("{:.*}", ha_decimals, a / HECTARE_M2),
            format!("{:.*}", m2_decimals, p.perimeter()?),
        ];
        if include_acres {
            f.push(format!("{:.*}", ha_decimals, a / ACRE_M2));
        }
        s.push_str(&f.join(","));
        s.push('\n');
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::Turn;
    use crate::parcel::{Boundary, ParcelStatus, Segment};

    fn sq(n: u32, e0: f64, n0: f64, w: f64, h: f64) -> Parcel {
        let mut p = Parcel::new(&format!("P{n}"), n, Boundary::polygon(&[Xy::new(e0, n0), Xy::new(e0 + w, n0), Xy::new(e0 + w, n0 + h), Xy::new(e0, n0 + h)]).unwrap());
        p.owner = format!("Owner, {n}");
        p
    }
    fn opts() -> SplitOptions {
        SplitOptions { snap_tol_m: 1e-6, arc_tol_m: 1e-4 }
    }

    #[test]
    fn straight_cut_at_utm_magnitude() {
        let (e0, n0) = (500000.0, 9999900.0);
        let p = sq(1, e0, n0, 100.0, 50.0);
        // vertical cutter at x = e0 + 30 running south to north through the parcel and beyond
        let cutter = [Xy::new(e0 + 30.0, n0 - 10.0), Xy::new(e0 + 30.0, n0 + 60.0)];
        // Going north, left is west: take the left (west) strip of width 30.
        let r = split_parcel(&p, &cutter, Side::Left, &opts()).unwrap();
        assert!((r.take_area_m2 - 30.0 * 50.0).abs() < 1e-6, "{r:?}");
        assert!((r.remainder_area_m2 - 70.0 * 50.0).abs() < 1e-6);
        assert!(r.pieces_check_m2.abs() < 1e-6);
        assert_eq!(r.take_pieces.len(), 1);
        let r = split_parcel(&p, &cutter, Side::Right, &opts()).unwrap();
        assert!((r.take_area_m2 - 3500.0).abs() < 1e-6);
    }

    #[test]
    fn cutter_missing_or_covering() {
        let p = sq(1, 0.0, 0.0, 10.0, 10.0);
        // north-going cutter well west of the parcel: the whole parcel is on its right
        let c = [Xy::new(-5.0, -5.0), Xy::new(-5.0, 15.0)];
        let r = split_parcel(&p, &c, Side::Right, &opts()).unwrap();
        assert!((r.take_area_m2 - 100.0).abs() < 1e-9 && r.remainder_area_m2.abs() < 1e-9);
        let r = split_parcel(&p, &c, Side::Left, &opts()).unwrap();
        assert!(r.take_area_m2.abs() < 1e-12 && (r.remainder_area_m2 - 100.0).abs() < 1e-9);
        // cutter running along an edge of the parcel
        let c = [Xy::new(0.0, -5.0), Xy::new(0.0, 15.0)];
        let r = split_parcel(&p, &c, Side::Right, &opts()).unwrap();
        assert!((r.take_area_m2 - 100.0).abs() < 1e-9);
    }

    #[test]
    fn bent_cutter_and_diagonal() {
        let p = sq(1, 0.0, 0.0, 10.0, 10.0);
        // diagonal corner to corner: each half is 50
        let c = [Xy::new(-1.0, -1.0), Xy::new(11.0, 11.0)];
        let r = split_parcel(&p, &c, Side::Left, &opts()).unwrap();
        assert!((r.take_area_m2 - 50.0).abs() < 1e-9, "{r:?}");
        // an L shaped cutter cutting the corner off: vertical at x=8 up to y=4 then east to x=12
        let c = [Xy::new(8.0, -2.0), Xy::new(8.0, 4.0), Xy::new(12.0, 4.0)];
        // take the right: the corner x>8, y<4 (travel north then east: right of the first leg is east, right of the second is south)
        let r = split_parcel(&p, &c, Side::Right, &opts()).unwrap();
        assert!((r.take_area_m2 - 8.0).abs() < 1e-9, "{r:?}");
    }

    #[test]
    fn curved_boundary_split_exact_remainder() {
        // Rectangle with a bulging arc on the east side; cut off the west 10 m strip (no arc in it).
        let r = 30.0;
        let p = Parcel::new(
            "A",
            1,
            Boundary {
                start: Xy::new(0.0, 0.0),
                segments: vec![
                    Segment::Line { to: Xy::new(40.0, 0.0) },
                    Segment::Arc { to: Xy::new(40.0, 20.0), radius: r, turn: Turn::Left, large: false },
                    Segment::Line { to: Xy::new(0.0, 20.0) },
                ],
            },
        );
        let c = [Xy::new(10.0, -1.0), Xy::new(10.0, 21.0)];
        let s = split_parcel(&p, &c, Side::Left, &opts()).unwrap();
        assert!((s.take_area_m2 - 200.0).abs() < 1e-6);
        assert!((s.remainder_area_m2 - (p.area().unwrap() - 200.0)).abs() < 1e-9);
        assert!(s.pieces_check_m2.abs() < s.arc_area_error_bound_m2 + 1e-6, "{} {}", s.pieces_check_m2, s.arc_area_error_bound_m2);
        // a cut through the arc: take the east part
        let c = [Xy::new(35.0, -1.0), Xy::new(35.0, 21.0)];
        let s = split_parcel(&p, &c, Side::Right, &opts()).unwrap();
        assert!(s.take_area_m2 > 0.0 && s.take_area_m2 < 5.0 * 20.0 + 30.0);
    }

    #[test]
    fn offset_polyline_basic() {
        let cl = [Xy::new(0.0, 0.0), Xy::new(100.0, 0.0), Xy::new(100.0, 100.0)];
        // left of east-going is north
        let o = offset_polyline(&cl, 10.0, 4.0).unwrap();
        assert_eq!(o.len(), 3);
        assert!((o[0].n - 10.0).abs() < 1e-12 && (o[1].e - 90.0).abs() < 1e-9 && (o[1].n - 10.0).abs() < 1e-9 && (o[2].e - 90.0).abs() < 1e-9);
        // right offset: outside corner mitre
        let o = offset_polyline(&cl, -10.0, 4.0).unwrap();
        assert!((o[1].e - 110.0).abs() < 1e-9 && (o[1].n + 10.0).abs() < 1e-9);
        // inside corner too short
        let short = [Xy::new(0.0, 0.0), Xy::new(5.0, 0.0), Xy::new(5.0, 100.0)];
        assert_eq!(offset_polyline(&short, 10.0, 4.0), Err(CogoError::OffsetSelfIntersects));
        // bevel at a tight angle
        let sharp = [Xy::new(0.0, 0.0), Xy::new(100.0, 0.0), Xy::new(0.0, 5.0)];
        assert_eq!(offset_polyline(&sharp, -2.0, 1.5).unwrap().len(), 4);
    }

    #[test]
    fn schedule_and_csv() {
        let mut a = sq(1, 0.0, 0.0, 100.0, 100.0);
        a.status = ParcelStatus::ToBeAcquired;
        a.land_reference = "Plot 12".into();
        let b = sq(2, 200.0, 0.0, 100.0, 100.0);
        // ROW edge: vertical at x=20 going north; take the west (left) 20 m of parcel 1 only
        let edge = [Xy::new(20.0, -10.0), Xy::new(20.0, 110.0)];
        // parcel 2 is wholly right of the edge: with take Left it has no take
        let rows = acquisition_schedule(&[a, b], &edge, Side::Left, &opts()).unwrap();
        assert!((rows[0].take_area_m2 - 2000.0).abs() < 1e-6 && (rows[0].remainder_area_m2 - 8000.0).abs() < 1e-6);
        assert!((rows[0].take_ha - 0.2).abs() < 1e-9);
        assert!(rows[0].affected && !rows[1].affected);
        assert_eq!(rows[1].take_area_m2, 0.0);
        let csv = schedule_csv(&rows, 2, 4, true, true);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("derived"));
        assert!(lines[1].starts_with("1,P1,Plot 12,\"Owner, 1\",to be acquired,10000.00,2000.00,8000.00,1.0000,0.2000,0.8000,2.4711"), "{}", lines[1]);
        let all = schedule_csv(&rows, 2, 4, false, false);
        assert_eq!(all.lines().count(), 3);
    }

    #[test]
    fn area_table() {
        let p = sq(3, 0.0, 0.0, 10.0, 20.0);
        let csv = area_table_csv(&[p], 3, 4, true).unwrap();
        let l: Vec<&str> = csv.lines().collect();
        assert!(l[1].starts_with("3,P3,,\"Owner, 3\",,existing,200.000,0.0200,60.000,0.0494"), "{}", l[1]);
    }
}
