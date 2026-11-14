//! "Fit to ground": a starting-point profile proposed from the ground along the centreline.
//!
//! **This is a starting point for the engineer, not a design.** It knows nothing about drainage,
//! cover over culverts, sight distance, earthworks balance, structures or standards. Every limit
//! it uses is entered by the user; none is built in.
//!
//! # Method (each step is plain arithmetic)
//!
//! 1. **Sample.** The ground is read from the TIN at the plan point of the centreline every
//!    `sample_spacing_m` of station, and at the end. Stations where the centreline is off the
//!    surface are left out and listed.
//! 2. **Segment.** Starting at the first sample, a straight line is fitted by ordinary least
//!    squares (`min Σ (z_i - a - b s_i)²`, solved with the normal equations) to the samples from
//!    the segment start to a trial end. The trial end is moved on one sample at a time while the
//!    largest vertical distance between a sample and the line stays within `tolerance_m`. When
//!    the next sample would break the tolerance, the segment ends at the last sample that kept
//!    it (but never shorter than `min_segment_length_m`). The next segment starts at that
//!    sample. This greedy rule gives a small number of long segments, and is not optimal.
//! 3. **PVIs.** A PVI is put at the start, the end and each segment boundary. At a boundary its
//!    elevation is the mean of the two neighbouring fitted lines at that station.
//! 4. **Maximum grade.** Working forward from the first PVI, if the grade to the next PVI
//!    exceeds `max_grade` (a ratio, entered) the next PVI's elevation is moved to the limit. Each
//!    such change is listed.
//! 5. **Curves.** At each interior PVI with a grade change, the curve length is
//!    `max(min_curve_length_m, k_crest or k_sag × 100 |A|)` (K values entered, optional),
//!    reduced if needed so that it uses at most half of the straight on each side (so curves
//!    never overlap). A curve held below the requested length is listed.
//!
//! The result reports how far the proposed profile is from the ground at the samples (largest
//! height above the ground, largest depth below it, root mean square) so the size of the
//! earthworks it implies can be judged.

use afrobim_alignment::Alignment;
use afrobim_corridor::Tin;
use serde::{Deserialize, Serialize};

use crate::error::{DesignError, Problem};
use crate::vertical::{CurveSpec, VPvi, VerticalDesign};

/// Label every output of this module carries.
pub const FIT_LABEL: &str = "STARTING POINT ONLY: proposed from the ground by a simple fit. Not a design; check every grade, curve and level.";

/// The user's entries for the fit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FitInputs {
    /// Spacing of the ground samples along the centreline, m.
    pub sample_spacing_m: f64,
    /// Largest vertical distance between a sample and the fitted line within a segment, m.
    pub tolerance_m: f64,
    /// No segment is shorter than this, m (the last one may be).
    pub min_segment_length_m: f64,
    /// Maximum grade, ratio (0.06 is 6 %). Entered by the user.
    pub max_grade: f64,
    /// Minimum vertical curve length, m.
    pub min_curve_length_m: f64,
    /// Minimum K for crests and sags, m per percent, if the user wants curves sized by K.
    #[serde(default)]
    pub k_crest: Option<f64>,
    #[serde(default)]
    pub k_sag: Option<f64>,
}

/// A proposed profile and how it was reached.
#[derive(Debug, Clone, PartialEq)]
pub struct FitResult {
    pub label: &'static str,
    pub design: VerticalDesign,
    /// The ground samples used: (station, ground level).
    pub samples: Vec<(f64, f64)>,
    /// Station ranges where the centreline is off the surface and no sample could be taken.
    pub gaps: Vec<(f64, f64)>,
    /// Largest height of the proposed profile above the ground at a sample, m (fill), and below it (cut depth), and RMS difference.
    pub max_above_ground_m: f64,
    pub max_below_ground_m: f64,
    pub rms_difference_m: f64,
    /// Every adjustment made after the fit (grade limits, shortened curves).
    pub notes: Vec<String>,
}

struct Line {
    a: f64,
    b: f64,
}

impl Line {
    fn at(&self, s: f64) -> f64 {
        self.a + self.b * s
    }
}

/// Least squares line through (s, z) points, with s measured from the first point for conditioning.
fn fit_line(pts: &[(f64, f64)]) -> Line {
    let n = pts.len() as f64;
    let s0 = pts[0].0;
    let (mut ss, mut sz, mut sss, mut ssz) = (0.0, 0.0, 0.0, 0.0);
    for &(s, z) in pts {
        let x = s - s0;
        ss += x;
        sz += z;
        sss += x * x;
        ssz += x * z;
    }
    let den = n * sss - ss * ss;
    let b = if den.abs() < 1e-12 { 0.0 } else { (n * ssz - ss * sz) / den };
    let a0 = (sz - b * ss) / n;
    Line { a: a0 - b * s0, b }
}

fn max_residual(line: &Line, pts: &[(f64, f64)]) -> f64 {
    pts.iter().map(|&(s, z)| (z - line.at(s)).abs()).fold(0.0, f64::max)
}

/// Proposes a profile for `alignment` from the ground `tin`. See the module documentation.
pub fn fit_to_ground(alignment: &Alignment, tin: &Tin, inputs: &FitInputs) -> Result<FitResult, DesignError> {
    let mut problems = Vec::new();
    for (what, v) in [
        ("sample spacing", inputs.sample_spacing_m),
        ("tolerance", inputs.tolerance_m),
        ("maximum grade", inputs.max_grade),
        ("minimum curve length", inputs.min_curve_length_m),
    ] {
        if !(v.is_finite() && v > 0.0) {
            problems.push(Problem::whole(format!("The {what} must be greater than zero (got {v}).")));
        }
    }
    if !(inputs.min_segment_length_m.is_finite() && inputs.min_segment_length_m >= 0.0) {
        problems.push(Problem::whole("The minimum segment length must not be negative."));
    }
    if !problems.is_empty() {
        return Err(DesignError::Impossible(problems));
    }
    let (start, end) = (alignment.start_station_m, alignment.end_station_m());
    let mut stations = Vec::new();
    let mut s = start;
    while s < end - 1e-9 {
        stations.push(s);
        s += inputs.sample_spacing_m;
    }
    stations.push(end);

    let mut samples: Vec<(f64, f64)> = Vec::new();
    let mut gaps: Vec<(f64, f64)> = Vec::new();
    let mut open: Option<f64> = None;
    for &st in &stations {
        let z = alignment.point_at(st).and_then(|p| tin.elevation_at(p.x, p.y));
        match z {
            Some(z) => {
                if let Some(g) = open.take() {
                    gaps.push((g, st));
                }
                samples.push((st, z));
            }
            None => {
                if open.is_none() {
                    open = Some(st);
                }
            }
        }
    }
    if let Some(g) = open {
        gaps.push((g, end));
    }
    if samples.len() < 2 {
        return Err(DesignError::Document("The centreline has fewer than two points on the ground surface, so there is nothing to fit.".into()));
    }
    let mut notes = Vec::new();
    for g in &gaps {
        notes.push(format!("The centreline is off the ground surface from station {:.1} to {:.1}; those stations were not sampled.", g.0, g.1));
    }

    // Greedy segmentation.
    let mut boundaries: Vec<usize> = vec![0];
    let mut lines: Vec<Line> = Vec::new();
    let mut i = 0;
    while i < samples.len() - 1 {
        let mut j = i + 1;
        let mut best = fit_line(&samples[i..=j]);
        while j + 1 < samples.len() {
            let trial = fit_line(&samples[i..=j + 1]);
            // Take the next sample while the line still fits, or while the segment is still too short.
            if max_residual(&trial, &samples[i..=j + 1]) <= inputs.tolerance_m || samples[j].0 - samples[i].0 < inputs.min_segment_length_m {
                best = trial;
                j += 1;
            } else {
                break;
            }
        }
        lines.push(best);
        boundaries.push(j);
        i = j;
    }

    // PVI elevations.
    let k = boundaries.len();
    let mut pvis: Vec<VPvi> = Vec::with_capacity(k);
    for (idx, &b) in boundaries.iter().enumerate() {
        let st = samples[b].0;
        let z = if idx == 0 {
            lines[0].at(st)
        } else if idx == k - 1 {
            lines[idx - 1].at(st)
        } else {
            0.5 * (lines[idx - 1].at(st) + lines[idx].at(st))
        };
        pvis.push(VPvi::point(st, z));
    }
    // Make sure the profile spans the alignment even if the ends were off the surface.
    if pvis[0].station_m > start + 1e-6 {
        let g = (pvis[1].elevation_m - pvis[0].elevation_m) / (pvis[1].station_m - pvis[0].station_m);
        pvis.insert(0, VPvi::point(start, pvis[0].elevation_m - g * (pvis[0].station_m - start)));
        notes.push("The profile was extended along its first grade to the start of the alignment.".into());
    }
    if pvis[pvis.len() - 1].station_m < end - 1e-6 {
        let n = pvis.len();
        let g = (pvis[n - 1].elevation_m - pvis[n - 2].elevation_m) / (pvis[n - 1].station_m - pvis[n - 2].station_m);
        let last = pvis[n - 1];
        pvis.push(VPvi::point(end, last.elevation_m + g * (end - last.station_m)));
        notes.push("The profile was extended along its last grade to the end of the alignment.".into());
    }

    // Maximum grade, forward pass.
    for i in 0..pvis.len() - 1 {
        let ds = pvis[i + 1].station_m - pvis[i].station_m;
        let g = (pvis[i + 1].elevation_m - pvis[i].elevation_m) / ds;
        if g.abs() > inputs.max_grade {
            let limited = pvis[i].elevation_m + g.signum() * inputs.max_grade * ds;
            notes.push(format!(
                "The grade from station {:.1} to {:.1} was {:.2} %, above the entered maximum of {:.2} %; the PVI at {:.1} was moved from {:.3} to {:.3} m.",
                pvis[i].station_m,
                pvis[i + 1].station_m,
                g * 100.0,
                inputs.max_grade * 100.0,
                pvis[i + 1].station_m,
                pvis[i + 1].elevation_m,
                limited
            ));
            pvis[i + 1].elevation_m = limited;
        }
    }

    // Curves.
    let grade = |p: &Vec<VPvi>, i: usize| (p[i + 1].elevation_m - p[i].elevation_m) / (p[i + 1].station_m - p[i].station_m);
    for i in 1..pvis.len() - 1 {
        let (g1, g2) = (grade(&pvis, i - 1), grade(&pvis, i));
        let a = g2 - g1;
        if a.abs() < 1e-6 {
            continue;
        }
        let k_min = if a < 0.0 { inputs.k_crest } else { inputs.k_sag };
        let wanted = inputs.min_curve_length_m.max(k_min.map_or(0.0, |k| k * 100.0 * a.abs()));
        let room = (pvis[i].station_m - pvis[i - 1].station_m).min(pvis[i + 1].station_m - pvis[i].station_m);
        let length = wanted.min(room);
        if length < wanted - 1e-9 {
            notes.push(format!(
                "The curve at station {:.1} was held to {:.1} m, below the {:.1} m asked for, because the straights beside it are short.",
                pvis[i].station_m, length, wanted
            ));
        }
        pvis[i].curve = CurveSpec::Length { length_m: length };
    }

    let design = VerticalDesign::new(&format!("{} (fit to ground)", alignment.name), pvis);
    let solved = design.solve()?;
    let (mut above, mut below, mut sq) = (0.0f64, 0.0f64, 0.0);
    for &(s, g) in &samples {
        if let Ok(z) = solved.level_at(s) {
            above = above.max(z - g);
            below = below.max(g - z);
            sq += (z - g) * (z - g);
        }
    }
    Ok(FitResult {
        label: FIT_LABEL,
        design,
        gaps,
        max_above_ground_m: above,
        max_below_ground_m: below,
        rms_difference_m: (sq / samples.len() as f64).sqrt(),
        samples,
        notes,
    })
}
