//! Parcel topology for a site: shared boundaries, gaps and overlaps.
//!
//! Method. Every parcel boundary is flattened (arcs to chords within `arc_tol_m`) and turned
//! anticlockwise, so that a parcel's inside is always to the left of its edges. All rings go into
//! one planar arrangement (see `arrangement.rs`): lines are split where they meet, vertices closer than
//! `snap_tol_m` merge. Then
//! - **shared boundary**: an edge piece used by two or more parcels. Neighbouring parcels use it in
//!   opposite directions (`length`); two parcels using it in the *same* direction both lie on the same
//!   side of it, so they overlap there (`same_side_length`);
//! - **overlap**: a bounded face covered by two or more parcels;
//! - **gap**: a bounded face covered by no parcel. Only gaps enclosed by parcel boundaries are found. A
//!   sliver open to the outside, or a parcel set whose outer edge leaves a strip against nothing,
//!   has no face and is not reported.
//! Coverage of a face is decided at an interior point of the face, by point in ring on the flattened
//! parcel. Faces smaller than `min_area_m2` are left out (set it to 0 to keep all).
//!
//! Accuracy. Areas here are those of the flattened rings, which lie inside the true arcs: for each arc the
//! error is at most (2/3) x arc length x `arc_tol_m` (parabola segment), summed in
//! `arc_area_error_bound_m2`. All three tolerances are the caller's: this module has no defaults.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{CogoError, Result};
use crate::geom::{ring_area, ring_winding, Xy};
use crate::parcel::{ArcGeom, Parcel, Segment};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TopologyOptions {
    pub snap_tol_m: f64,
    pub arc_tol_m: f64,
    pub min_area_m2: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedBoundary {
    /// Indices into the parcel slice, a < b.
    pub a: usize,
    pub b: usize,
    pub parcel_numbers: (u32, u32),
    /// Length where the two use the edge in opposite directions (they abut).
    pub length: f64,
    /// Length where both use it in the same direction (they overlap along it).
    pub same_side_length: f64,
    pub pieces: Vec<[Xy; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AreaIssue {
    /// Parcels covering the area (empty for a gap).
    pub parcels: Vec<usize>,
    pub parcel_numbers: Vec<u32>,
    pub area_m2: f64,
    pub ring: Vec<Xy>,
    pub holes: Vec<Vec<Xy>>,
    pub label_point: Option<Xy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyReport {
    pub shared: Vec<SharedBoundary>,
    pub gaps: Vec<AreaIssue>,
    pub overlaps: Vec<AreaIssue>,
    pub total_gap_area_m2: f64,
    pub total_overlap_area_m2: f64,
    pub arc_area_error_bound_m2: f64,
}

impl TopologyReport {
    /// One line per finding, for a log or a label.
    pub fn summary_lines(&self) -> Vec<String> {
        let mut v = Vec::new();
        for s in &self.shared {
            v.push(format!("shared boundary parcels {} and {}: {:.3} m", s.parcel_numbers.0, s.parcel_numbers.1, s.length));
        }
        for g in &self.gaps {
            v.push(format!("gap {:.4} m2", g.area_m2));
        }
        for o in &self.overlaps {
            v.push(format!("overlap of parcels {:?}: {:.4} m2", o.parcel_numbers, o.area_m2));
        }
        v
    }
}

fn ccw_ring(p: &Parcel, arc_tol: f64) -> Result<Vec<Xy>> {
    let mut r = p.boundary.flatten(arc_tol)?;
    if ring_area(&r) < 0.0 {
        r.reverse();
    }
    Ok(r)
}

pub fn arc_area_error_bound(parcels: &[Parcel], arc_tol: f64) -> Result<f64> {
    let mut s = 0.0;
    for p in parcels {
        for (f, seg) in p.boundary.closed_segments() {
            if let Segment::Arc { to, radius, turn, large } = seg {
                s += 2.0 / 3.0 * ArcGeom::new(f, to, radius, turn, large)?.length() * arc_tol;
            }
        }
    }
    Ok(s)
}

pub fn analyse_site(parcels: &[Parcel], o: &TopologyOptions) -> Result<TopologyReport> {
    if !(o.arc_tol_m > 0.0) || !(o.snap_tol_m > 0.0) || !(o.min_area_m2 >= 0.0) {
        return Err(CogoError::Invalid("tolerances must be positive (minimum area zero or more)".into()));
    }
    let rings: Vec<Vec<Xy>> = parcels.iter().map(|p| ccw_ring(p, o.arc_tol_m)).collect::<Result<_>>()?;
    let lines: Vec<(Vec<Xy>, bool, usize)> = rings.iter().cloned().enumerate().map(|(i, r)| (r, true, i)).collect();
    let arr = crate::arrangement::build(&lines, o.snap_tol_m)?;

    // shared boundaries: directed uses per undirected edge
    let mut uses: BTreeMap<(usize, usize), Vec<(usize, bool)>> = BTreeMap::new();
    for p in &arr.pieces {
        let key = (p.from.min(p.to), p.from.max(p.to));
        uses.entry(key).or_default().push((p.tag, p.from < p.to));
    }
    let mut pairs: BTreeMap<(usize, usize), SharedBoundary> = BTreeMap::new();
    for ((u, v), list) in &uses {
        for i in 0..list.len() {
            for j in i + 1..list.len() {
                let (a, b) = (list[i], list[j]);
                if a.0 == b.0 {
                    continue;
                }
                let (lo, hi) = (a.0.min(b.0), a.0.max(b.0));
                let len = arr.verts[*u].dist(arr.verts[*v]);
                let e = pairs.entry((lo, hi)).or_insert_with(|| SharedBoundary {
                    a: lo,
                    b: hi,
                    parcel_numbers: (parcels[lo].number, parcels[hi].number),
                    length: 0.0,
                    same_side_length: 0.0,
                    pieces: vec![],
                });
                if a.1 == b.1 {
                    e.same_side_length += len;
                } else {
                    e.length += len;
                }
                e.pieces.push([arr.verts[*u], arr.verts[*v]]);
            }
        }
    }

    let (mut gaps, mut overlaps) = (Vec::new(), Vec::new());
    for f in &arr.faces {
        let area = f.net_area();
        if area < o.min_area_m2 || area <= 0.0 {
            continue;
        }
        let Some(p) = f.interior_point() else { continue };
        let cover: Vec<usize> = (0..rings.len()).filter(|i| ring_winding(&rings[*i], p) != 0).collect();
        let issue = |cover: Vec<usize>| AreaIssue {
            parcel_numbers: cover.iter().map(|i| parcels[*i].number).collect(),
            parcels: cover,
            area_m2: area,
            ring: f.ring.clone(),
            holes: f.holes.clone(),
            label_point: Some(p),
        };
        match cover.len() {
            0 => gaps.push(issue(cover)),
            1 => {}
            _ => overlaps.push(issue(cover)),
        }
    }
    let total_gap = gaps.iter().map(|g| g.area_m2).sum();
    let total_overlap = overlaps.iter().map(|g| g.area_m2).sum();
    Ok(TopologyReport {
        shared: pairs.into_values().collect(),
        gaps,
        overlaps,
        total_gap_area_m2: total_gap,
        total_overlap_area_m2: total_overlap,
        arc_area_error_bound_m2: arc_area_error_bound(parcels, o.arc_tol_m)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parcel::Boundary;

    fn sq(n: u32, e0: f64, n0: f64, w: f64, h: f64) -> Parcel {
        Parcel::new(&format!("P{n}"), n, Boundary::polygon(&[Xy::new(e0, n0), Xy::new(e0 + w, n0), Xy::new(e0 + w, n0 + h), Xy::new(e0, n0 + h)]).unwrap())
    }
    fn opts() -> TopologyOptions {
        TopologyOptions { snap_tol_m: 1e-4, arc_tol_m: 1e-3, min_area_m2: 0.0 }
    }

    #[test]
    fn neighbours_share_a_boundary_with_t_junction() {
        // At UTM magnitude. P1 20 wide, P2 and P3 stacked beside it: the shared edge of P1 is split by T junction.
        let (e0, n0) = (500000.0, 9999900.0);
        let ps = [sq(1, e0, n0, 20.0, 20.0), sq(2, e0 + 20.0, n0, 20.0, 10.0), sq(3, e0 + 20.0, n0 + 10.0, 20.0, 10.0)];
        let r = analyse_site(&ps, &opts()).unwrap();
        assert!(r.gaps.is_empty() && r.overlaps.is_empty(), "{r:?}");
        let s12 = r.shared.iter().find(|s| (s.a, s.b) == (0, 1)).unwrap();
        let s13 = r.shared.iter().find(|s| (s.a, s.b) == (0, 2)).unwrap();
        let s23 = r.shared.iter().find(|s| (s.a, s.b) == (1, 2)).unwrap();
        assert!((s12.length - 10.0).abs() < 1e-6 && (s13.length - 10.0).abs() < 1e-6 && (s23.length - 20.0).abs() < 1e-6);
        assert_eq!(s12.same_side_length, 0.0);
    }

    #[test]
    fn overlap_area_and_same_side() {
        let ps = [sq(1, 0.0, 0.0, 10.0, 10.0), sq(2, 5.0, 5.0, 10.0, 10.0)];
        let r = analyse_site(&ps, &opts()).unwrap();
        assert_eq!(r.overlaps.len(), 1);
        assert!((r.overlaps[0].area_m2 - 25.0).abs() < 1e-9);
        assert_eq!(r.overlaps[0].parcel_numbers, vec![1, 2]);
        assert!(r.gaps.is_empty());
        // parcel fully inside another: overlap equals the inner area, outer face has a hole but is covered once
        let ps = [sq(1, 0.0, 0.0, 10.0, 10.0), sq(2, 2.0, 2.0, 3.0, 3.0)];
        let r = analyse_site(&ps, &opts()).unwrap();
        assert!((r.total_overlap_area_m2 - 9.0).abs() < 1e-9, "{r:?}");
        // identical parcels overlap fully and share a same-side boundary
        let ps = [sq(1, 0.0, 0.0, 10.0, 10.0), sq(2, 0.0, 0.0, 10.0, 10.0)];
        let r = analyse_site(&ps, &opts()).unwrap();
        assert!((r.total_overlap_area_m2 - 100.0).abs() < 1e-9);
        assert!((r.shared[0].same_side_length - 40.0).abs() < 1e-9);
    }

    #[test]
    fn enclosed_gap() {
        // Four parcels around a 2 x 2 hole.
        let ps = [sq(1, 0.0, 0.0, 10.0, 4.0), sq(2, 0.0, 6.0, 10.0, 4.0), sq(3, 0.0, 4.0, 4.0, 2.0), sq(4, 6.0, 4.0, 4.0, 2.0)];
        let r = analyse_site(&ps, &opts()).unwrap();
        assert_eq!(r.gaps.len(), 1, "{r:?}");
        assert!((r.gaps[0].area_m2 - 4.0).abs() < 1e-9);
        assert!(r.overlaps.is_empty());
        let mut min = opts();
        min.min_area_m2 = 5.0;
        assert!(analyse_site(&ps, &min).unwrap().gaps.is_empty());
    }

    #[test]
    fn snapping_closes_millimetre_misfit() {
        let ps = [sq(1, 0.0, 0.0, 10.0, 10.0), sq(2, 10.0003, 0.0, 10.0, 10.0)];
        let r = analyse_site(&ps, &TopologyOptions { snap_tol_m: 1e-3, arc_tol_m: 1e-3, min_area_m2: 0.0 }).unwrap();
        assert!(r.gaps.is_empty() && r.overlaps.is_empty());
        assert!((r.shared[0].length - 10.0).abs() < 1e-3);
        let r = analyse_site(&ps, &TopologyOptions { snap_tol_m: 1e-5, arc_tol_m: 1e-3, min_area_m2: 0.0 }).unwrap();
        assert!(r.shared.is_empty());
    }

    #[test]
    fn arcs_shared_between_neighbours() {
        // Two parcels sharing a curved side: left one bulges into the right one's place and vice versa.
        let r = 20.0;
        let left = Parcel::new(
            "L",
            1,
            Boundary {
                start: Xy::new(0.0, 0.0),
                segments: vec![
                    Segment::Line { to: Xy::new(10.0, 0.0) },
                    Segment::Arc { to: Xy::new(10.0, 10.0), radius: r, turn: crate::calc::Turn::Left, large: false },
                    Segment::Line { to: Xy::new(0.0, 10.0) },
                ],
            },
        );
        let right = Parcel::new(
            "R",
            2,
            Boundary {
                start: Xy::new(10.0, 0.0),
                segments: vec![
                    Segment::Line { to: Xy::new(20.0, 0.0) },
                    Segment::Line { to: Xy::new(20.0, 10.0) },
                    Segment::Line { to: Xy::new(10.0, 10.0) },
                    Segment::Arc { to: Xy::new(10.0, 0.0), radius: r, turn: crate::calc::Turn::Right, large: false },
                ],
            },
        );
        let rep = analyse_site(&[left.clone(), right.clone()], &TopologyOptions { snap_tol_m: 1e-6, arc_tol_m: 1e-4, min_area_m2: 0.0 }).unwrap();
        assert!(rep.gaps.is_empty() && rep.overlaps.is_empty(), "{rep:?}");
        let arc_len = left.boundary.perimeter().unwrap() - 10.0 - 10.0 - 10.0;
        assert!((rep.shared[0].length - arc_len).abs() < 1e-3, "{} {}", rep.shared[0].length, arc_len);
        assert!(rep.arc_area_error_bound_m2 > 0.0);
    }

    #[test]
    fn bad_tolerances() {
        let ps = [sq(1, 0.0, 0.0, 1.0, 1.0)];
        assert!(analyse_site(&ps, &TopologyOptions { snap_tol_m: 0.0, arc_tol_m: 1.0, min_area_m2: 0.0 }).is_err());
    }
}
