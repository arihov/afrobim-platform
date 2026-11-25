// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Laying the marks of a line along an alignment.
//!
//! # The line a marking follows
//!
//! A marking runs beside an alignment at a constant offset `o` (metres, positive to the right
//! looking up-station, the convention of `afrobim-alignment`). On a curve that line is longer or
//! shorter than the alignment it follows. With curvature `k` positive turning left, a step `ds`
//! of station is a step
//!
//! ```text
//!   dd = (1 + o k(s)) ds
//! ```
//!
//! along the line, so between two stations on one element the line is
//!
//! ```text
//!   d = Δs + o Δθ          Δθ = the change of heading of the alignment, radians, left positive
//! ```
//!
//! long: exact for straights, circular arcs and clothoid spirals, whose heading is the integral
//! of `k`. On an arc of radius R it reduces to `Δs (R ± o) / R`.
//!
//! **Marks and gaps are measured along the marking itself**, not along the alignment: a 2 m mark
//! is 2 m of paint whether the line is on the inside or the outside of a bend. The station of
//! each mark end is found by inverting the relation above in closed form (it is a quadratic in
//! `s` on a spiral and linear otherwise).
//!
//! # Where the pattern starts and what happens at the end
//!
//! Traffic Signs Manual Chapter 5 (2018) gives the mark and the gap of each diagram and says
//! nothing on where along a length the pattern is to start or how a remainder is treated (it
//! only asks, in 2.2.3, that a warning line "normally be chosen to maintain the same module
//! length" as the centre line it replaces). The rule here is therefore this crate's own:
//!
//! - The pattern starts with the leading end of a full mark at the anchor: the start station by
//!   default ([`PatternAnchor::Start`]), or the end station with the pattern laid backwards
//!   from it ([`PatternAnchor::End`], for a line that must finish on a full mark, such as a
//!   warning line running up to a junction).
//! - Modules repeat without adjustment: neither mark nor gap is stretched to fit the length.
//! - At the far end the pattern is cut square at the station. A mark that is cut is kept as a
//!   **part mark**: it is drawn, its length is in the painted length, and it is counted
//!   separately from the whole marks. A length that ends in a gap simply ends.
//! - A continuous line is one mark from end to end.
//!
//! A double line is two strands, each laid by this rule along its own line, so a broken strand
//! on a bend keeps its own 1 m marks and 5 m gaps.
//!
//! # Geometry
//!
//! Each mark is a strip of the line's width, square-ended, given as its two edges. On curves
//! the edges are chords that stay within [`CHORD_SAG_M`] of the true curve. Quantities never
//! come from the chords: lengths and areas use the closed forms above.

use afrobim_alignment::{Alignment, AlignmentElement};
use serde::{Deserialize, Serialize};

use crate::catalogue::Pattern;

/// Greatest distance between a drawn chord and the true edge of a mark, m.
pub const CHORD_SAG_M: f64 = 0.001;

/// A station this far outside the alignment, m, is taken to be on its end.
pub const STATION_TOLERANCE_M: f64 = 0.001;

/// Lengths closer than this, m, are the same length (a micron).
const EPS_M: f64 = 1e-6;

/// Which end of the length the pattern is laid from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PatternAnchor {
    /// A full mark starts at the start station; any part mark is at the end.
    #[default]
    Start,
    /// A full mark ends at the end station; any part mark is at the start.
    End,
}

/// One line of a marking to lay out: a single line has one, a double line two.
#[derive(Debug, Clone, PartialEq)]
pub struct StrandSpec {
    /// `"line"`, or `"left line"` / `"right line"` of a double line.
    pub name: String,
    /// Offset of the strand's centre from the alignment, m, positive right.
    pub lateral_m: f64,
    pub width_m: f64,
    pub pattern: Pattern,
}

/// One mark on the ground.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// Distance along the strand from its start to the mark's leading end, m.
    pub from_m: f64,
    /// Distance along the strand to its trailing end, m.
    pub to_m: f64,
    pub start_station_m: f64,
    pub end_station_m: f64,
    /// False for a part mark cut by the end of the length.
    pub whole: bool,
    /// The left edge (looking up-station) in plan, metres, in the direction of increasing
    /// station. Two points on a straight; more on a curve or across an element boundary.
    pub left: Vec<(f64, f64)>,
    /// The right edge, point for point with `left`.
    pub right: Vec<(f64, f64)>,
}

impl Mark {
    /// Length along the centre of the strand, m.
    pub fn length_m(&self) -> f64 {
        self.to_m - self.from_m
    }

    /// The mark as one closed outline, counter-clockwise: the right edge up-station, then the
    /// left edge back.
    pub fn outline(&self) -> Vec<(f64, f64)> {
        self.right.iter().chain(self.left.iter().rev()).copied().collect()
    }
}

/// One strand laid out.
#[derive(Debug, Clone, PartialEq)]
pub struct StrandLayout {
    pub name: String,
    pub lateral_m: f64,
    pub width_m: f64,
    pub pattern: Pattern,
    /// Length of the strand's own line between the two stations, m.
    pub length_m: f64,
    /// In order of station.
    pub marks: Vec<Mark>,
}

impl StrandLayout {
    pub fn whole_marks(&self) -> usize {
        self.marks.iter().filter(|m| m.whole).count()
    }

    pub fn part_marks(&self) -> usize {
        self.marks.iter().filter(|m| !m.whole).count()
    }

    /// Sum of the mark lengths, part marks included, m.
    pub fn painted_length_m(&self) -> f64 {
        self.marks.iter().map(Mark::length_m).sum()
    }

    /// Painted length × width, m². Exact for a strip of constant width about its centre line,
    /// on a curve as on a straight.
    pub fn painted_area_m2(&self) -> f64 {
        self.painted_length_m() * self.width_m
    }
}

/// A marking laid out.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// Length of the reference line (the alignment shifted by the marking's offset) between the
    /// two stations, m. For a single line this is the line itself; for a double line it is the
    /// middle of the pair.
    pub overall_length_m: f64,
    pub strands: Vec<StrandLayout>,
}

impl Layout {
    pub fn whole_marks(&self) -> usize {
        self.strands.iter().map(StrandLayout::whole_marks).sum()
    }

    pub fn part_marks(&self) -> usize {
        self.strands.iter().map(StrandLayout::part_marks).sum()
    }

    pub fn painted_length_m(&self) -> f64 {
        self.strands.iter().map(StrandLayout::painted_length_m).sum()
    }

    pub fn painted_area_m2(&self) -> f64 {
        self.strands.iter().map(StrandLayout::painted_area_m2).sum()
    }

    /// Lower-left and upper-right corners of everything drawn, plan metres. `None` when there
    /// are no marks.
    pub fn bounds(&self) -> Option<((f64, f64), (f64, f64))> {
        let mut points = self.strands.iter().flat_map(|s| s.marks.iter()).flat_map(|m| m.left.iter().chain(m.right.iter()));
        let first = *points.next()?;
        Some(points.fold((first, first), |(lo, hi), p| ((lo.0.min(p.0), lo.1.min(p.1)), (hi.0.max(p.0), hi.1.max(p.1)))))
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LayoutError {
    #[error("the alignment has no length")]
    EmptyAlignment,
    #[error("the end station ({end_m:.3} m) must be greater than the start station ({start_m:.3} m)")]
    StationOrder { start_m: f64, end_m: f64 },
    #[error("station {station_m:.3} m is outside the alignment, which runs from {from_m:.3} m to {to_m:.3} m")]
    StationOutside { station_m: f64, from_m: f64, to_m: f64 },
    #[error("at station {station_m:.3} m the line is {lateral_m:.3} m to the inside of a curve of radius {radius_m:.3} m: it would lie at or beyond the centre of the curve")]
    InsideCurve { station_m: f64, lateral_m: f64, radius_m: f64 },
    #[error("a strand has a width that is not greater than zero")]
    BadWidth,
}

/// One edge of a strip: plan points, metres.
type Edge = Vec<(f64, f64)>;

/// The part of one alignment element a strand runs along.
#[derive(Debug, Clone, Copy)]
struct Piece {
    element: usize,
    /// Distance into the element where the strand enters and leaves it, m.
    a: f64,
    b: f64,
    /// Station at `a`.
    station_a: f64,
    /// Distance along the strand at `a` and at `b`.
    d0: f64,
    d1: f64,
}

/// The line at a constant offset from an alignment, between two stations.
struct OffsetLine<'a> {
    alignment: &'a Alignment,
    lateral: f64,
    pieces: Vec<Piece>,
}

/// `(A, B)` with distance along the offset line `g(s) = s (B + A s)` for distance `s` into the element.
fn coefficients(e: &AlignmentElement, lateral: f64) -> (f64, f64) {
    let rate = if e.length > 0.0 { (e.curvature_end - e.curvature_start) / e.length } else { 0.0 };
    (0.5 * lateral * rate, 1.0 + lateral * e.curvature_start)
}

fn g(e: &AlignmentElement, lateral: f64, s: f64) -> f64 {
    let (a, b) = coefficients(e, lateral);
    s * (b + a * s)
}

impl<'a> OffsetLine<'a> {
    /// `reach` is the half-width that must also stay clear of the centre of every curve.
    fn new(alignment: &'a Alignment, start_m: f64, end_m: f64, lateral: f64, reach: f64) -> Result<Self, LayoutError> {
        let mut pieces = Vec::new();
        let mut d = 0.0;
        for (i, (e, station)) in alignment.elements.iter().zip(alignment.element_stations()).enumerate() {
            let a = (start_m - station).max(0.0);
            let b = (end_m - station).min(e.length);
            if e.length <= 0.0 || b <= a {
                continue;
            }
            for s in [a, b] {
                let k = e.curvature_at(s);
                for edge in [lateral - reach, lateral + reach] {
                    if 1.0 + edge * k <= 1e-6 {
                        return Err(LayoutError::InsideCurve { station_m: station + s, lateral_m: edge.abs(), radius_m: 1.0 / k.abs() });
                    }
                }
            }
            let length = g(e, lateral, b) - g(e, lateral, a);
            pieces.push(Piece { element: i, a, b, station_a: station + a, d0: d, d1: d + length });
            d += length;
        }
        if pieces.is_empty() {
            return Err(LayoutError::EmptyAlignment);
        }
        Ok(OffsetLine { alignment, lateral, pieces })
    }

    fn length(&self) -> f64 {
        self.pieces.last().map_or(0.0, |p| p.d1)
    }

    /// The piece holding distance `d` along the line and the distance into its element.
    fn locate(&self, d: f64) -> (usize, f64) {
        let i = self.pieces.partition_point(|p| p.d1 < d).min(self.pieces.len() - 1);
        let p = &self.pieces[i];
        let e = &self.alignment.elements[p.element];
        let (a, b) = coefficients(e, self.lateral);
        // Solve s (b + a s) = t for the root that grows with t.
        let t = g(e, self.lateral, p.a) + (d - p.d0);
        let s = if a.abs() < 1e-15 { t / b } else { 2.0 * t / (b + (b * b + 4.0 * a * t).max(0.0).sqrt()) };
        (i, s.clamp(p.a, p.b))
    }

    fn station(&self, d: f64) -> f64 {
        let (i, s) = self.locate(d);
        self.pieces[i].station_a + (s - self.pieces[i].a)
    }

    /// The plan point `lateral` to the right of the alignment at distance `s` into an element.
    fn point(&self, element: usize, s: f64, lateral: f64) -> (f64, f64) {
        let e = &self.alignment.elements[element];
        let (x, y) = e.point_at(s);
        let h = e.heading_at(s);
        (x + lateral * h.sin(), y - lateral * h.cos())
    }

    /// The left and right edges of a strip `half_width` either side of the line, from distance
    /// `from` to `to` along it.
    fn edges(&self, from: f64, to: f64, half_width: f64) -> (Edge, Edge) {
        let (i0, s_from) = self.locate(from);
        let (i1, s_to) = self.locate(to);
        let (mut left, mut right): (Edge, Edge) = (Vec::new(), Vec::new());
        for i in i0..=i1 {
            let p = &self.pieces[i];
            let e = &self.alignment.elements[p.element];
            let lo = if i == i0 { s_from } else { p.a };
            let hi = if i == i1 { s_to } else { p.b };
            let k = e.curvature_at(lo).abs().max(e.curvature_at(hi).abs());
            let n = if k < 1e-12 || hi <= lo { 1 } else { ((hi - lo) / (8.0 * CHORD_SAG_M / k).sqrt().clamp(0.05, 10.0)).ceil().max(1.0) as usize };
            for j in 0..=n {
                let s = lo + (hi - lo) * j as f64 / n as f64;
                let (l, r) = (self.point(p.element, s, self.lateral - half_width), self.point(p.element, s, self.lateral + half_width));
                // The end of one element and the start of the next are the same section.
                let repeat = right.last().is_some_and(|q: &(f64, f64)| (q.0 - r.0).hypot(q.1 - r.1) < 1e-9)
                    && left.last().is_some_and(|q: &(f64, f64)| (q.0 - l.0).hypot(q.1 - l.1) < 1e-9);
                if !repeat {
                    left.push(l);
                    right.push(r);
                }
            }
        }
        (left, right)
    }
}

/// The marks of a pattern over a length `total`, as (from, to, whole), in order.
pub(crate) fn intervals(pattern: Pattern, total: f64, anchor: PatternAnchor) -> Vec<(f64, f64, bool)> {
    let Pattern::Broken { mark_mm, gap_mm } = pattern else {
        return vec![(0.0, total, true)];
    };
    let (mark, module) = (mark_mm as f64 / 1000.0, (mark_mm + gap_mm) as f64 / 1000.0);
    let mut out = Vec::new();
    if mark <= 0.0 {
        return out;
    }
    for i in 0.. {
        let lead = i as f64 * module;
        if lead >= total - EPS_M {
            break;
        }
        let trail = (lead + mark).min(total);
        let whole = lead + mark <= total + EPS_M;
        match anchor {
            PatternAnchor::Start => out.push((lead, trail, whole)),
            PatternAnchor::End => out.push((total - trail, total - lead, whole)),
        }
    }
    if anchor == PatternAnchor::End {
        out.reverse();
    }
    out
}

/// Lays strands along an alignment between two stations.
///
/// `offset_m` is the marking's reference offset (used for the overall length); each strand
/// carries its own. Stations within [`STATION_TOLERANCE_M`] of an end of the alignment are
/// taken to be on it. See the module documentation for the rule.
// The negated comparisons are deliberate: they also refuse a NaN.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn lay_out(
    alignment: &Alignment,
    start_station_m: f64,
    end_station_m: f64,
    offset_m: f64,
    strands: &[StrandSpec],
    anchor: PatternAnchor,
) -> Result<Layout, LayoutError> {
    let (from_m, to_m) = (alignment.start_station_m, alignment.end_station_m());
    if alignment.elements.is_empty() || !(to_m > from_m) {
        return Err(LayoutError::EmptyAlignment);
    }
    for station_m in [start_station_m, end_station_m] {
        if !(station_m >= from_m - STATION_TOLERANCE_M && station_m <= to_m + STATION_TOLERANCE_M) {
            return Err(LayoutError::StationOutside { station_m, from_m, to_m });
        }
    }
    let (start, end) = (start_station_m.clamp(from_m, to_m), end_station_m.clamp(from_m, to_m));
    if !(end - start > EPS_M) {
        return Err(LayoutError::StationOrder { start_m: start_station_m, end_m: end_station_m });
    }
    let overall_length_m = OffsetLine::new(alignment, start, end, offset_m, 0.0)?.length();
    let mut laid = Vec::with_capacity(strands.len());
    for strand in strands {
        if !(strand.width_m > 0.0 && strand.width_m.is_finite()) {
            return Err(LayoutError::BadWidth);
        }
        let half = strand.width_m / 2.0;
        let line = OffsetLine::new(alignment, start, end, strand.lateral_m, half)?;
        let length_m = line.length();
        let marks = intervals(strand.pattern, length_m, anchor)
            .into_iter()
            .map(|(from, to, whole)| {
                let (left, right) = line.edges(from, to, half);
                Mark { from_m: from, to_m: to, start_station_m: line.station(from), end_station_m: line.station(to), whole, left, right }
            })
            .collect();
        laid.push(StrandLayout { name: strand.name.clone(), lateral_m: strand.lateral_m, width_m: strand.width_m, pattern: strand.pattern, length_m, marks });
    }
    Ok(Layout { overall_length_m, strands: laid })
}

/// Stations of points spaced `spacing_m` apart along the line at `lateral_m` from the alignment,
/// from the start station to the end station, with a point at the start and one at each whole
/// spacing after it that does not pass the end. Also returns the length of that line, m.
///
/// Spacing is measured along the offset line itself (as marks and gaps are), so studs on the
/// outside of a bend are further apart in station than studs on the inside.
pub fn stations_along(alignment: &Alignment, start_station_m: f64, end_station_m: f64, lateral_m: f64, spacing_m: f64) -> Result<(Vec<f64>, f64), LayoutError> {
    let (from_m, to_m) = (alignment.start_station_m, alignment.end_station_m());
    if alignment.elements.is_empty() || !(to_m > from_m) {
        return Err(LayoutError::EmptyAlignment);
    }
    for station_m in [start_station_m, end_station_m] {
        if !(station_m >= from_m - STATION_TOLERANCE_M && station_m <= to_m + STATION_TOLERANCE_M) {
            return Err(LayoutError::StationOutside { station_m, from_m, to_m });
        }
    }
    let (start, end) = (start_station_m.clamp(from_m, to_m), end_station_m.clamp(from_m, to_m));
    if !(end - start > EPS_M) {
        return Err(LayoutError::StationOrder { start_m: start_station_m, end_m: end_station_m });
    }
    if !(spacing_m > 0.0 && spacing_m.is_finite()) {
        return Err(LayoutError::BadWidth);
    }
    let line = OffsetLine::new(alignment, start, end, lateral_m, 0.0)?;
    let length = line.length();
    let mut out = Vec::new();
    for i in 0.. {
        let d = i as f64 * spacing_m;
        if d > length + EPS_M {
            break;
        }
        out.push(line.station(d.min(length)));
    }
    Ok((out, length))
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_alignment::{AlignmentBuilder, Turn};

    const CENTRE_1008: Pattern = Pattern::Broken { mark_mm: 2000, gap_mm: 4000 };
    const LANE_1005_1: Pattern = Pattern::Broken { mark_mm: 2000, gap_mm: 7000 };
    const WARNING_1004_1: Pattern = Pattern::Broken { mark_mm: 6000, gap_mm: 3000 };

    fn single(lateral_m: f64, width_m: f64, pattern: Pattern) -> Vec<StrandSpec> {
        vec![StrandSpec { name: "line".into(), lateral_m, width_m, pattern }]
    }

    fn near(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).hypot(a.1 - b.1) < 1e-9
    }

    fn straight() -> Alignment {
        AlignmentBuilder::new("S", 0.0, 0.0, 0.0, 0.0).line(100.0).build().unwrap()
    }

    #[test]
    fn straight_whole_modules() {
        // 60 m of diagram 1008: ten 6 m modules exactly, so ten 2 m marks and no remainder.
        let l = lay_out(&straight(), 0.0, 60.0, 0.0, &single(0.0, 0.1, CENTRE_1008), PatternAnchor::Start).unwrap();
        let s = &l.strands[0];
        assert_eq!((s.whole_marks(), s.part_marks()), (10, 0));
        assert!((l.overall_length_m - 60.0).abs() < 1e-12);
        assert!((s.painted_length_m() - 20.0).abs() < 1e-9);
        assert!((s.painted_area_m2() - 2.0).abs() < 1e-9);
        // The first mark is the rectangle x 0..2, y -0.05..0.05; right edge is -y on a line heading east.
        let first = &s.marks[0];
        assert_eq!(first.outline().len(), 4);
        assert!(near(first.right[0], (0.0, -0.05)) && near(first.right[1], (2.0, -0.05)));
        assert!(near(first.left[0], (0.0, 0.05)) && near(first.left[1], (2.0, 0.05)));
        let last = &s.marks[9];
        assert!(near(last.right[0], (54.0, -0.05)) && near(last.left[1], (56.0, 0.05)));
        assert!((last.start_station_m - 54.0).abs() < 1e-9 && (last.end_station_m - 56.0).abs() < 1e-9);
    }

    #[test]
    fn straight_with_a_part_module_at_the_end() {
        // 61 m: ten whole marks and 1 m of an eleventh.
        let l = lay_out(&straight(), 0.0, 61.0, 0.0, &single(0.0, 0.1, CENTRE_1008), PatternAnchor::Start).unwrap();
        let s = &l.strands[0];
        assert_eq!((s.whole_marks(), s.part_marks()), (10, 1));
        assert!((s.painted_length_m() - 21.0).abs() < 1e-9);
        let part = s.marks.last().unwrap();
        assert!(!part.whole && near(part.right[0], (60.0, -0.05)) && near(part.right[1], (61.0, -0.05)));
        // 62.5 m ends in a gap: eleven whole marks, nothing cut.
        let l = lay_out(&straight(), 0.0, 62.5, 0.0, &single(0.0, 0.1, CENTRE_1008), PatternAnchor::Start).unwrap();
        assert_eq!((l.whole_marks(), l.part_marks()), (11, 0));
        assert!((l.painted_length_m() - 22.0).abs() < 1e-9);
    }

    #[test]
    fn anchored_at_the_end_the_part_mark_moves_to_the_start() {
        let l = lay_out(&straight(), 0.0, 61.0, 0.0, &single(0.0, 0.1, CENTRE_1008), PatternAnchor::End).unwrap();
        let s = &l.strands[0];
        assert_eq!((s.whole_marks(), s.part_marks()), (10, 1));
        let (first, last) = (&s.marks[0], s.marks.last().unwrap());
        assert!(!first.whole && near(first.right[0], (0.0, -0.05)) && near(first.right[1], (1.0, -0.05)));
        assert!(last.whole && near(last.right[0], (59.0, -0.05)) && near(last.right[1], (61.0, -0.05)));
        assert!((s.painted_length_m() - 21.0).abs() < 1e-9);
    }

    #[test]
    fn stations_and_offsets_follow_the_alignment_conventions() {
        // Heading north from (500, 200), first station 1000. Right of north is east.
        let a = AlignmentBuilder::new("N", 1000.0, 500.0, 200.0, std::f64::consts::FRAC_PI_2).line(100.0).build().unwrap();
        let l = lay_out(&a, 1010.0, 1030.0, 3.0, &single(3.0, 0.15, CENTRE_1008), PatternAnchor::Start).unwrap();
        let m = &l.strands[0].marks[1];
        assert!((m.start_station_m - 1016.0).abs() < 1e-9 && (m.end_station_m - 1018.0).abs() < 1e-9);
        assert!(near(m.right[0], (503.075, 216.0)) && near(m.left[0], (502.925, 216.0)));
        assert!(near(m.right[1], (503.075, 218.0)) && near(m.left[1], (502.925, 218.0)));
        assert_eq!(l.whole_marks(), 4); // marks start at 1010, 1016, 1022 and 1028
    }

    /// 150 m of left-hand curve, radius 200 m, starting at the origin heading east: centre (0, 200).
    fn curve() -> Alignment {
        AlignmentBuilder::new("C", 0.0, 0.0, 0.0, 0.0).arc(200.0, 150.0, Turn::Left).build().unwrap()
    }

    /// The point at station `s` and offset `o` (right positive) on [`curve`].
    fn on_curve(s: f64, o: f64) -> (f64, f64) {
        let (phi, r) = (s / 200.0, 200.0 + o);
        (r * phi.sin(), 200.0 - r * phi.cos())
    }

    #[test]
    fn circular_curve_on_the_outside() {
        // Lane line 1005.1 (2 m mark, 7 m gap), 3.5 m to the right of a left-hand curve: the
        // outside, radius 203.5. 90 m of station is 90 x 203.5 / 200 = 91.575 m of line.
        let l = lay_out(&curve(), 0.0, 90.0, 3.5, &single(3.5, 0.1, LANE_1005_1), PatternAnchor::Start).unwrap();
        let s = &l.strands[0];
        assert!((s.length_m - 91.575).abs() < 1e-9);
        assert!((l.overall_length_m - 91.575).abs() < 1e-9);
        // Marks lead at 0, 9, ..., 90 m of line: ten whole, and 1.575 m of the eleventh.
        assert_eq!((s.whole_marks(), s.part_marks()), (10, 1));
        assert!((s.painted_length_m() - 21.575).abs() < 1e-9);
        assert!((s.painted_area_m2() - 2.1575).abs() < 1e-9);
        // The fourth mark runs from 27 m to 29 m of line: stations 27 / 1.0175 and 29 / 1.0175.
        let m = &s.marks[3];
        let (s0, s1) = (27.0 / 1.0175, 29.0 / 1.0175);
        assert!((m.start_station_m - s0).abs() < 1e-9 && (m.end_station_m - s1).abs() < 1e-9);
        assert!(near(m.right[0], on_curve(s0, 3.55)) && near(m.left[0], on_curve(s0, 3.45)));
        assert!(near(*m.right.last().unwrap(), on_curve(s1, 3.55)) && near(*m.left.last().unwrap(), on_curve(s1, 3.45)));
        // Every edge point is on its circle, and the chords stay within the sag limit.
        assert!(m.right.len() > 2);
        for (p, radius) in m.right.iter().map(|p| (p, 203.55)).chain(m.left.iter().map(|p| (p, 203.45))) {
            assert!((p.0.hypot(p.1 - 200.0) - radius).abs() < 1e-9);
        }
        for w in m.right.windows(2) {
            let chord = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            assert!(chord * chord / (8.0 * 203.55) <= CHORD_SAG_M * 1.05);
        }
    }

    #[test]
    fn circular_curve_on_the_inside() {
        // The same line 3.5 m to the left: radius 196.5, 90 x 196.5 / 200 = 88.425 m of line.
        // Marks lead at 0, 9, ..., 81; the tenth ends at 83, and 90 is beyond the end.
        let l = lay_out(&curve(), 0.0, 90.0, -3.5, &single(-3.5, 0.1, LANE_1005_1), PatternAnchor::Start).unwrap();
        let s = &l.strands[0];
        assert!((s.length_m - 88.425).abs() < 1e-9);
        assert_eq!((s.whole_marks(), s.part_marks()), (10, 0));
        assert!((s.painted_length_m() - 20.0).abs() < 1e-9);
        let m = &s.marks[9];
        assert!((m.end_station_m - 83.0 / 0.9825).abs() < 1e-9);
        assert!(near(*m.left.last().unwrap(), on_curve(83.0 / 0.9825, -3.55)));
    }

    /// 30 m east from the origin, then 60 m of right-hand curve, radius 100 m: centre (30, -100).
    fn tangent_then_curve() -> Alignment {
        AlignmentBuilder::new("TC", 0.0, 0.0, 0.0, 0.0).line(30.0).arc(100.0, 60.0, Turn::Right).build().unwrap()
    }

    #[test]
    fn a_mark_across_an_element_boundary() {
        // Warning line 1004.1 (6 m mark, 3 m gap), 150 mm wide, 2 m to the right: on the inside
        // of the right-hand curve, radius 98. Line length 30 + 60 x 0.98 = 88.8 m.
        let l = lay_out(&tangent_then_curve(), 0.0, 90.0, 2.0, &single(2.0, 0.15, WARNING_1004_1), PatternAnchor::Start).unwrap();
        let s = &l.strands[0];
        assert!((s.length_m - 88.8).abs() < 1e-9);
        // Marks lead at 0, 9, ..., 81 (ends at 87); 90 is beyond 88.8.
        assert_eq!((s.whole_marks(), s.part_marks()), (10, 0));
        assert!((s.painted_length_m() - 60.0).abs() < 1e-9);
        assert!((s.painted_area_m2() - 9.0).abs() < 1e-9);
        // The fourth mark leads at 27 m on the straight and trails 3 m of line into the curve:
        // station 30 + 3 / 0.98.
        let m = &s.marks[3];
        let end_station = 30.0 + 3.0 / 0.98;
        assert!((m.start_station_m - 27.0).abs() < 1e-9 && (m.end_station_m - end_station).abs() < 1e-9);
        assert!((m.length_m() - 6.0).abs() < 1e-9);
        assert!(near(m.right[0], (27.0, -2.075)) && near(m.left[0], (27.0, -1.925)));
        // One section exactly on the boundary, not two.
        assert!(near(m.right[1], (30.0, -2.075)) && near(m.left[1], (30.0, -1.925)));
        assert!(m.right[2].0 > 30.0);
        let phi = (end_station - 30.0) / 100.0;
        let on_arc = |r: f64| (30.0 + r * phi.sin(), -100.0 + r * phi.cos());
        assert!(near(*m.right.last().unwrap(), on_arc(100.0 - 2.075)) && near(*m.left.last().unwrap(), on_arc(100.0 - 1.925)));
        // A mark wholly on the straight is still a plain rectangle.
        assert_eq!(s.marks[0].outline().len(), 4);
    }

    #[test]
    fn a_continuous_line_is_one_mark_across_every_boundary() {
        let l = lay_out(&tangent_then_curve(), 10.0, 90.0, 2.0, &single(2.0, 0.1, Pattern::Continuous), PatternAnchor::Start).unwrap();
        let s = &l.strands[0];
        assert_eq!((s.marks.len(), s.whole_marks(), s.part_marks()), (1, 1, 0));
        assert!((s.painted_length_m() - (20.0 + 60.0 * 0.98)).abs() < 1e-9);
        assert!((s.painted_area_m2() - 7.88).abs() < 1e-9);
        assert!(near(s.marks[0].right[0], (10.0, -2.05)));
    }

    #[test]
    fn a_spiral_is_measured_by_its_change_of_heading() {
        // 60 m of clothoid from straight to a 120 m radius turning left: the heading turns
        // L / 2R = 0.25 rad, so a line 3 m to the right (outside) is 60 + 3 x 0.25 = 60.75 m.
        let a = AlignmentBuilder::new("SP", 0.0, 0.0, 0.0, 0.0).spiral(60.0, None, Some(120.0), Turn::Left).build().unwrap();
        let l = lay_out(&a, 0.0, 60.0, 3.0, &single(3.0, 0.1, Pattern::Continuous), PatternAnchor::Start).unwrap();
        assert!((l.overall_length_m - 60.75).abs() < 1e-9);
        // The drawn centre of the strip, summed chord by chord, agrees to well within a millimetre.
        let m = &l.strands[0].marks[0];
        let mid: Vec<(f64, f64)> = m.left.iter().zip(&m.right).map(|(l, r)| ((l.0 + r.0) / 2.0, (l.1 + r.1) / 2.0)).collect();
        let chords: f64 = mid.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum();
        assert!((chords - 60.75).abs() < 1e-3, "{chords}");
        // Halfway along the line is not halfway along the spiral: solve s + 3 s^2 / (2 x 60 x 120) = 30.375.
        let broken = lay_out(&a, 0.0, 60.0, 3.0, &single(3.0, 0.1, Pattern::Broken { mark_mm: 30375, gap_mm: 100000 }), PatternAnchor::Start).unwrap();
        let s = broken.strands[0].marks[0].end_station_m;
        assert!((s + 3.0 * s * s / 14400.0 - 30.375).abs() < 1e-9);
    }

    #[test]
    fn what_cannot_be_laid_is_refused() {
        let strands = single(0.0, 0.1, CENTRE_1008);
        assert!(matches!(lay_out(&straight(), -5.0, 50.0, 0.0, &strands, PatternAnchor::Start), Err(LayoutError::StationOutside { .. })));
        assert!(matches!(lay_out(&straight(), 0.0, 100.5, 0.0, &strands, PatternAnchor::Start), Err(LayoutError::StationOutside { .. })));
        assert!(matches!(lay_out(&straight(), 50.0, 50.0, 0.0, &strands, PatternAnchor::Start), Err(LayoutError::StationOrder { .. })));
        assert!(matches!(lay_out(&straight(), 60.0, 20.0, 0.0, &strands, PatternAnchor::Start), Err(LayoutError::StationOrder { .. })));
        // Half a millimetre past the end is the end.
        assert!(lay_out(&straight(), 0.0, 100.0005, 0.0, &strands, PatternAnchor::Start).is_ok());
        // 250 m to the left of a 200 m left-hand curve is beyond its centre.
        assert!(matches!(lay_out(&curve(), 0.0, 50.0, -250.0, &single(-250.0, 0.1, CENTRE_1008), PatternAnchor::Start), Err(LayoutError::InsideCurve { .. })));
        assert!(matches!(lay_out(&straight(), 0.0, 50.0, 0.0, &single(0.0, 0.0, CENTRE_1008), PatternAnchor::Start), Err(LayoutError::BadWidth)));
    }
}
