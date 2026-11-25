// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Vertical geometry notes for profile views: PVI station and level, the grade of each tangent in
//! percent, the length of each vertical curve, its start and end stations, and the K value
//! (length per percent of grade change, L / A) as a computed number.
//!
//! Everything is read from the alignment crate's [`Profile`]. Nothing here checks a K value, a
//! grade or a curve length against a limit: there is no pass or fail, only the numbers.
//!
//! Hand-worked example (also a test): PVIs at (0, 100.0), (200, 104.0 with a 100 m parabola) and
//! (500, 101.0). The grade into the middle PVI is (104 - 100) / 200 = +2.00 %, the grade out of it
//! (101 - 104) / 300 = -1.00 %. The grade change A = (-1.00) - (+2.00) = -3.00 %. The curve is 100
//! m long, half each side of the PVI, so it starts at 150 and ends at 250, and K = L / |A| = 100 /
//! 3 = 33.33 m per percent.
//!
//! For a circular curve L is its horizontal length (the end station less the start station), so
//! K is that length over the grade change; for an asymmetric parabola L is the sum of the two
//! lengths.

use afrobim_alignment::Profile;
use serde::{Deserialize, Serialize};

use crate::geom::Rect;

/// What the vertical annotation of a profile view shows. These are display switches and a display
/// precision, not design values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProfileAnnotations {
    /// PVI station and level, in a labelled block above the plot with a leader down to the PVI.
    pub pvi: bool,
    /// The grade of each tangent in percent, along the tangent.
    pub grades: bool,
    /// Vertical curve length, grade change and start and end stations.
    pub curves: bool,
    /// The computed K value (L / A) with the curve; no limit is applied to it.
    pub k_value: bool,
    /// Decimals for grades, grade changes and K values.
    pub decimals: usize,
    /// Decimals for the stations in the blocks (kept high enough to tell a PVC from a PVI).
    pub station_decimals: usize,
}

impl Default for ProfileAnnotations {
    /// Default only: everything on, grades and K to 2 decimals, stations to 3.
    fn default() -> Self {
        ProfileAnnotations { pvi: true, grades: true, curves: true, k_value: true, decimals: 2, station_decimals: 3 }
    }
}

impl ProfileAnnotations {
    /// All of it off.
    pub fn none() -> ProfileAnnotations {
        ProfileAnnotations { pvi: false, grades: false, curves: false, k_value: false, ..ProfileAnnotations::default() }
    }

    pub fn any(&self) -> bool {
        self.pvi || self.grades || self.curves
    }
}

/// A vertical curve at a PVI.
#[derive(Debug, Clone, PartialEq)]
pub struct CurveNote {
    pub start_station_m: f64,
    pub end_station_m: f64,
    /// Horizontal length, end less start.
    pub length_m: f64,
    /// Grade out less grade in, percent (negative on a crest).
    pub grade_change_percent: f64,
    /// L / |A| in metres per percent; `None` when the grades do not change.
    pub k_m_per_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PviNote {
    pub index: usize,
    pub station_m: f64,
    pub elevation_m: f64,
    pub grade_in_percent: Option<f64>,
    pub grade_out_percent: Option<f64>,
    pub curve: Option<CurveNote>,
}

/// A tangent: the straight between two PVIs, whose grade is shown along the straight part that
/// the curves leave.
#[derive(Debug, Clone, PartialEq)]
pub struct TangentNote {
    pub from_pvi: usize,
    pub to_pvi: usize,
    pub grade_percent: f64,
    /// The straight part: from the end of the curve at the first PVI (or the PVI) to the start of
    /// the curve at the second (or the PVI).
    pub run_start_m: f64,
    pub run_end_m: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VerticalNotes {
    pub pvis: Vec<PviNote>,
    pub tangents: Vec<TangentNote>,
}

/// Reads the vertical geometry of a profile. An error is the profile's own validation message.
pub fn vertical_notes(profile: &Profile) -> Result<VerticalNotes, String> {
    profile.validate().map_err(|e| e.to_string())?;
    let extents = profile.curve_extents().map_err(|e| e.to_string())?;
    let n = profile.pvis.len();
    let grade = |i: usize| profile.grade_between(i).map(|g| g * 100.0);
    let mut out = VerticalNotes::default();
    for (i, p) in profile.pvis.iter().enumerate() {
        let grade_in = if i > 0 { grade(i - 1) } else { None };
        let grade_out = if i + 1 < n { grade(i) } else { None };
        let curve = extents.iter().find(|e| e.pvi == i).map(|e| {
            let a = grade_out.unwrap_or(0.0) - grade_in.unwrap_or(0.0);
            let length = e.end_station_m - e.start_station_m;
            CurveNote {
                start_station_m: e.start_station_m,
                end_station_m: e.end_station_m,
                length_m: length,
                grade_change_percent: a,
                k_m_per_percent: if a.abs() > 1e-9 { Some(length / a.abs()) } else { None },
            }
        });
        out.pvis.push(PviNote { index: i, station_m: p.station_m, elevation_m: p.elevation_m, grade_in_percent: grade_in, grade_out_percent: grade_out, curve });
    }
    for i in 0..n.saturating_sub(1) {
        let g = grade(i).unwrap_or(0.0);
        let start = out.pvis[i].curve.as_ref().map_or(profile.pvis[i].station_m, |c| c.end_station_m);
        let end = out.pvis[i + 1].curve.as_ref().map_or(profile.pvis[i + 1].station_m, |c| c.start_station_m);
        out.tangents.push(TangentNote { from_pvi: i, to_pvi: i + 1, grade_percent: g, run_start_m: start, run_end_m: end });
    }
    Ok(out)
}

/// A signed percentage as text: `+2.00 %`, `-1.00 %`, `+0.00 %`.
pub fn percent_text(v: f64, decimals: usize) -> String {
    let t = format!("{:+.*}", decimals, v);
    // A grade that rounds to nothing is shown without the sign of a tiny negative number.
    let t = if t.starts_with("-0") && t[1..].chars().all(|c| c == '0' || c == '.') { format!("+{}", &t[1..]) } else { t };
    format!("{t} %")
}

/// A block of text above the plot: one PVI's numbers, placed at the PVI's station and staggered
/// into rows so that two blocks never overlap.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteBlock {
    pub pvi: usize,
    /// Paper x of the PVI (where the leader runs).
    pub leader_x: f64,
    /// Left edge of the box, kept inside the plot's width.
    pub left: f64,
    pub width: f64,
    pub height: f64,
    pub lines: Vec<String>,
    /// 0 is the row nearest the plot.
    pub row: usize,
}

impl NoteBlock {
    /// The box in paper mm given where row 0 starts and the pitch between rows.
    pub fn rect(&self, row0_y: f64, row_pitch: f64) -> Rect {
        let y0 = row0_y + self.row as f64 * row_pitch;
        Rect::new(self.left, y0, self.left + self.width, y0 + self.height)
    }
}

/// Gap between neighbouring boxes in a row, and padding inside a box, mm.
pub const BOX_GAP: f64 = 1.5;
pub const BOX_PAD: f64 = 0.8;

/// Line pitch of a block as a multiple of the text height.
pub const LINE_PITCH: f64 = 1.5;

/// The height of a box of `n` lines of text of height `h`.
pub fn box_height(n: usize, h: f64) -> f64 {
    2.0 * BOX_PAD + h + n.saturating_sub(1) as f64 * h * LINE_PITCH
}

/// How a PVI's block is worded: the lines, from the station text and level text given.
pub fn block_lines(note: &PviNote, a: &ProfileAnnotations, station: &dyn Fn(f64) -> String, level_decimals: usize) -> Vec<String> {
    let mut lines = Vec::new();
    if a.pvi {
        lines.push(format!("PVI {}   EL {:.*}", station(note.station_m), level_decimals, note.elevation_m));
    }
    if a.curves {
        if let Some(c) = &note.curve {
            let mut t = format!("L {:.*}   A {}", level_decimals, c.length_m, percent_text(c.grade_change_percent, a.decimals));
            if a.k_value {
                match c.k_m_per_percent {
                    Some(k) => t.push_str(&format!("   K {:.*}", a.decimals, k)),
                    None => t.push_str("   K -"),
                }
            }
            lines.push(t);
            lines.push(format!("PVC {}   PVT {}", station(c.start_station_m), station(c.end_station_m)));
        }
    }
    lines
}

/// Lays the blocks out left to right and puts each in the lowest row where it does not run into
/// the one before it (with [`BOX_GAP`] between). `x_of` turns a station into paper x; the boxes
/// are kept between `x_min` and `x_max`. Returns the blocks and the number of rows.
#[allow(clippy::too_many_arguments)]
pub fn layout_blocks(
    notes: &VerticalNotes,
    a: &ProfileAnnotations,
    from_m: f64,
    to_m: f64,
    x_of: &dyn Fn(f64) -> f64,
    x_min: f64,
    x_max: f64,
    text_h: f64,
    station: &dyn Fn(f64) -> String,
    level_decimals: usize,
    width_of: &dyn Fn(&str) -> f64,
) -> (Vec<NoteBlock>, usize) {
    let mut blocks: Vec<NoteBlock> = Vec::new();
    let mut row_right: Vec<f64> = Vec::new();
    for p in notes.pvis.iter().filter(|p| p.station_m >= from_m - 1e-9 && p.station_m <= to_m + 1e-9) {
        let lines = block_lines(p, a, station, level_decimals);
        if lines.is_empty() {
            continue;
        }
        let width = lines.iter().map(|l| width_of(l)).fold(0.0, f64::max) + 2.0 * BOX_PAD;
        let x = x_of(p.station_m);
        let left = (x - width / 2.0).clamp(x_min, (x_max - width).max(x_min));
        let row = match row_right.iter().position(|r| left >= *r + BOX_GAP) {
            Some(r) => r,
            None => {
                row_right.push(f64::NEG_INFINITY);
                row_right.len() - 1
            }
        };
        row_right[row] = left + width;
        blocks.push(NoteBlock { pvi: p.index, leader_x: x, left, width, height: box_height(lines.len(), text_h), lines, row });
    }
    let rows = row_right.len();
    (blocks, rows)
}

/// The parts of the vertical line at `x` from `y_from` down to `y_to` that are not inside any of
/// `boxes` (a leader does not run through another label's box), as (y high, y low) pairs.
pub fn leader_pieces(x: f64, y_from: f64, y_to: f64, boxes: &[Rect]) -> Vec<(f64, f64)> {
    let (hi, lo) = (y_from.max(y_to), y_from.min(y_to));
    let mut cuts: Vec<(f64, f64)> = boxes.iter().filter(|b| x > b.x0 && x < b.x1 && b.y1 > lo && b.y0 < hi).map(|b| (b.y0, b.y1)).collect();
    cuts.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut out = Vec::new();
    let mut top = hi;
    for (c0, c1) in cuts {
        if c1 < top {
            out.push((top, c1.max(lo)));
        }
        top = top.min(c0);
    }
    if top > lo {
        out.push((top, lo));
    }
    out.retain(|(a, b)| a - b > 1e-9);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_alignment::Pvi;

    #[test]
    fn a_parabola_gives_grades_a_grade_change_curve_ends_and_k() {
        // PVIs (0, 100), (200, 104 with a 100 m parabola), (500, 101): grade in 4 / 200 = +2 %, out
        // -3 / 300 = -1 %, A = -3 %, curve 150 to 250, K = 100 / 3.
        let p = Profile::new("t", vec![Pvi::point(0.0, 100.0), Pvi::parabola(200.0, 104.0, 100.0), Pvi::point(500.0, 101.0)]).unwrap();
        let n = vertical_notes(&p).unwrap();
        assert_eq!(n.pvis.len(), 3);
        let mid = &n.pvis[1];
        assert!((mid.grade_in_percent.unwrap() - 2.0).abs() < 1e-9 && (mid.grade_out_percent.unwrap() + 1.0).abs() < 1e-9);
        let c = mid.curve.as_ref().unwrap();
        assert!((c.start_station_m - 150.0).abs() < 1e-9 && (c.end_station_m - 250.0).abs() < 1e-9 && (c.length_m - 100.0).abs() < 1e-9);
        assert!((c.grade_change_percent + 3.0).abs() < 1e-9);
        assert!((c.k_m_per_percent.unwrap() - 100.0 / 3.0).abs() < 1e-9);
        // The ends carry no curve and one grade each.
        assert!(n.pvis[0].curve.is_none() && n.pvis[0].grade_in_percent.is_none() && n.pvis[2].grade_out_percent.is_none());
        // Tangents: +2 % from 0 to the PVC at 150, -1 % from the PVT at 250 to 500.
        assert_eq!(n.tangents.len(), 2);
        assert_eq!((n.tangents[0].run_start_m, n.tangents[0].run_end_m), (0.0, 150.0));
        assert_eq!((n.tangents[1].run_start_m, n.tangents[1].run_end_m), (250.0, 500.0));
        assert!((n.tangents[1].grade_percent + 1.0).abs() < 1e-9);
    }

    #[test]
    fn an_asymmetric_parabola_has_the_sum_of_its_lengths() {
        // PVIs (0, 100), (300, 103 with 80 m in and 40 m out), (600, 100): grades +1 % and -1 %,
        // A = -2 %, L = 120, curve 220 to 340, K = 120 / 2 = 60.
        let p = Profile::new("t", vec![Pvi::point(0.0, 100.0), Pvi::asymmetric(300.0, 103.0, 80.0, 40.0), Pvi::point(600.0, 100.0)]).unwrap();
        let c = vertical_notes(&p).unwrap().pvis[1].curve.clone().unwrap();
        assert!((c.start_station_m - 220.0).abs() < 1e-9 && (c.end_station_m - 340.0).abs() < 1e-9);
        assert!((c.length_m - 120.0).abs() < 1e-9 && (c.k_m_per_percent.unwrap() - 60.0).abs() < 1e-9);
    }

    #[test]
    fn a_pvi_without_a_curve_has_no_k_and_equal_grades_have_none_either() {
        // Straight through a PVI: both grades +1 %, a 40 m parabola there has A = 0 so no K.
        let p = Profile::new("t", vec![Pvi::point(0.0, 100.0), Pvi::parabola(100.0, 101.0, 40.0), Pvi::point(200.0, 102.0)]).unwrap();
        let n = vertical_notes(&p).unwrap();
        let c = n.pvis[1].curve.as_ref().unwrap();
        assert!(c.grade_change_percent.abs() < 1e-9 && c.k_m_per_percent.is_none());
        let lines = block_lines(&n.pvis[1], &ProfileAnnotations::default(), &|s| format!("{s:.0}"), 3);
        assert_eq!(lines[0], "PVI 100   EL 101.000");
        assert!(lines[1].ends_with("K -"), "{lines:?}");
    }

    #[test]
    fn percent_text_is_signed_and_never_negative_zero() {
        assert_eq!(percent_text(2.0, 2), "+2.00 %");
        assert_eq!(percent_text(-1.0, 2), "-1.00 %");
        assert_eq!(percent_text(-0.0001, 2), "+0.00 %");
    }

    #[test]
    fn block_words_follow_the_switches() {
        let p = Profile::new("t", vec![Pvi::point(0.0, 100.0), Pvi::parabola(200.0, 104.0, 100.0), Pvi::point(500.0, 101.0)]).unwrap();
        let n = vertical_notes(&p).unwrap();
        let st = |s: f64| format!("{s:.1}");
        let all = block_lines(&n.pvis[1], &ProfileAnnotations::default(), &st, 3);
        assert_eq!(all, vec!["PVI 200.0   EL 104.000", "L 100.000   A -3.00 %   K 33.33", "PVC 150.0   PVT 250.0"]);
        let no_k = ProfileAnnotations { k_value: false, ..ProfileAnnotations::default() };
        assert_eq!(block_lines(&n.pvis[1], &no_k, &st, 3)[1], "L 100.000   A -3.00 %");
        let pvi_only = ProfileAnnotations { curves: false, ..ProfileAnnotations::default() };
        assert_eq!(block_lines(&n.pvis[1], &pvi_only, &st, 3).len(), 1);
        assert!(block_lines(&n.pvis[1], &ProfileAnnotations::none(), &st, 3).is_empty());
    }

    #[test]
    fn crowded_blocks_are_staggered_into_rows_and_never_overlap() {
        // Five PVIs 10 m apart at 1 mm per metre: boxes about 23 mm wide cannot sit side by side.
        let p = Profile::new(
            "t",
            vec![Pvi::point(0.0, 100.0), Pvi::point(10.0, 101.0), Pvi::point(20.0, 100.0), Pvi::point(30.0, 101.0), Pvi::point(40.0, 100.0), Pvi::point(300.0, 102.0)],
        )
        .unwrap();
        let n = vertical_notes(&p).unwrap();
        let (blocks, rows) = layout_blocks(&n, &ProfileAnnotations::default(), 0.0, 300.0, &|s| 20.0 + s, 20.0, 320.0, 2.0, &|s| format!("{s:.0}"), 3, &|t| t.len() as f64 * 1.2);
        assert_eq!(blocks.len(), 6);
        // Each box is 18 characters x 1.2 mm + 1.6 mm padding = 23.2 mm wide, so the boxes centred at
        // x = 20, 30, 40, 50 are in rows 0, 1, 2, 3 (the one at 50 is 38.4 to 61.6, too near the
        // first, ending at 43.2 + 1.5 gap), and the one centred at 60 (left 48.4, past 44.7) goes
        // back to row 0, as does the far one.
        assert_eq!(rows, 4);
        assert_eq!(blocks.iter().map(|b| b.row).collect::<Vec<_>>(), vec![0, 1, 2, 3, 0, 0]);
        let rects: Vec<Rect> = blocks.iter().map(|b| b.rect(100.0, 6.0)).collect();
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                let (a, b) = (rects[i], rects[j]);
                let overlap = a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1;
                assert!(!overlap, "blocks {i} and {j} overlap: {a:?} {b:?}");
            }
        }
        // Boxes stay inside the plot's width even at the ends.
        assert!(blocks.iter().all(|b| b.left >= 20.0 && b.left + b.width <= 320.0));
    }

    #[test]
    fn a_leader_stops_at_boxes_it_would_cross() {
        // A leader at x = 10 from y = 50 down to y = 0 passes a box covering y 20 to 30 at x 5 to 15:
        // it is drawn from 50 to 30 and from 20 to 0. A box that does not span x = 10 does not cut it.
        let boxes = [Rect::new(5.0, 20.0, 15.0, 30.0), Rect::new(11.0, 35.0, 20.0, 40.0)];
        assert_eq!(leader_pieces(10.0, 50.0, 0.0, &boxes), vec![(50.0, 30.0), (20.0, 0.0)]);
        assert_eq!(leader_pieces(10.0, 50.0, 0.0, &[]), vec![(50.0, 0.0)]);
    }
}
