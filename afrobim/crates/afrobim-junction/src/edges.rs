//! Lane edge alignments of a road, with turning-lane widening.
//!
//! # Construction
//!
//! The edge on a side of a road is the road alignment offset by `base + w(s)`, where `base` is
//! the entered lane-edge offset and `w(s)` the sum of the entered widenings. A [`Widening`]
//! starting at station `s0` with entry taper `a`, full-width length `l`, exit taper `b` and width
//! `W` is the trapezoid
//!
//! ```text
//!   w(s) = 0                      s <= s0
//!        = W (s - s0) / a         s0 < s < s0 + a          (a = 0: a step to W at s0)
//!        = W                      s0 + a <= s <= s0 + a + l
//!        = W (e - s) / b          e = s0 + a + l, e < s < e + b   (b = 0: a step down at e)
//!        = 0                      beyond
//! ```
//!
//! The stations at which `w` changes slope, and every element boundary of the road, cut the edge
//! into pieces. A piece of constant offset is the road's own element offset exactly (see
//! [`crate::geom`]). A piece in a taper is **one straight element** between the offset points at
//! its two ends: exact on a straight road, a chord on a curved one (the largest distance of the
//! true taper offset curve from the chord, sampled at nine points, is reported). Edges therefore
//! have a heading break at the ends of a taper, as a real taper does.

use afrobim_alignment::{Alignment, AlignmentElement};
use afrobim_corridor::Side;
use serde::{Deserialize, Serialize};

use crate::error::JunctionError;
use crate::geom::{dist, heading_of, max_joint_gap, offset_pieces, sub};

/// A turning-lane widening on one side of a road. All lengths are entered.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Widening {
    /// Station (of the road alignment) where the entry taper starts, m.
    pub start_station_m: f64,
    /// Length of the entry taper, m (0: a step).
    pub taper_length_m: f64,
    /// Length at full width, m.
    pub length_m: f64,
    /// Length of the exit taper, m (0: a step).
    #[serde(default)]
    pub exit_taper_length_m: f64,
    /// Added width of the edge offset at full width, m (zero or more).
    pub width_m: f64,
}

impl Widening {
    fn breakpoints(&self) -> [f64; 4] {
        let b1 = self.start_station_m + self.taper_length_m;
        let b2 = b1 + self.length_m;
        [self.start_station_m, b1, b2, b2 + self.exit_taper_length_m]
    }
    /// The added width at a station, m.
    pub fn at(&self, s: f64) -> f64 {
        let [b0, b1, b2, b3] = self.breakpoints();
        if s <= b0 || s >= b3 {
            0.0
        } else if s < b1 {
            self.width_m * (s - b0) / (b1 - b0)
        } else if s <= b2 {
            self.width_m
        } else {
            self.width_m * (b3 - s) / (b3 - b2)
        }
    }
    fn check(&self, what: &str) -> Result<(), JunctionError> {
        let ok = [self.start_station_m, self.taper_length_m, self.length_m, self.exit_taper_length_m, self.width_m].iter().all(|v| v.is_finite())
            && self.taper_length_m >= 0.0
            && self.length_m >= 0.0
            && self.exit_taper_length_m >= 0.0
            && self.width_m >= 0.0;
        if ok {
            Ok(())
        } else {
            Err(JunctionError::input(format!("{what}: a widening needs finite numbers with lengths and width not negative")))
        }
    }
}

/// Lane edge offsets of a road each side of its alignment, with optional widening tables.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoadEdges {
    /// Distance from the alignment to the left edge, m, greater than zero.
    pub left_offset_m: f64,
    /// Distance from the alignment to the right edge, m, greater than zero.
    pub right_offset_m: f64,
    #[serde(default)]
    pub left_widening: Vec<Widening>,
    #[serde(default)]
    pub right_widening: Vec<Widening>,
}

impl RoadEdges {
    pub fn symmetric(offset_m: f64) -> RoadEdges {
        RoadEdges { left_offset_m: offset_m, right_offset_m: offset_m, left_widening: vec![], right_widening: vec![] }
    }
    pub fn base(&self, side: Side) -> f64 {
        match side {
            Side::Left => self.left_offset_m,
            Side::Right => self.right_offset_m,
        }
    }
    pub fn widening(&self, side: Side) -> &[Widening] {
        match side {
            Side::Left => &self.left_widening,
            Side::Right => &self.right_widening,
        }
    }
    /// The edge offset (magnitude, from the alignment) at a station, m.
    pub fn offset_at(&self, side: Side, station_m: f64) -> f64 {
        self.base(side) + self.widening(side).iter().map(|w| w.at(station_m)).sum::<f64>()
    }
    pub fn check(&self, what: &str) -> Result<(), JunctionError> {
        for (n, v) in [("left", self.left_offset_m), ("right", self.right_offset_m)] {
            if !(v.is_finite() && v > 0.0) {
                return Err(JunctionError::input(format!("{what}: the {n} edge offset must be greater than zero (got {v})")));
            }
        }
        self.left_widening.iter().chain(&self.right_widening).try_for_each(|w| w.check(what))
    }
}

/// An edge alignment and what is known about its accuracy.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeBuild {
    pub alignment: Alignment,
    /// Largest distance of a taper chord from the true taper offset curve, m (0 on a straight road).
    pub max_taper_chord_deviation_m: f64,
    /// Largest gap at a joint of the edge, m.
    pub max_joint_gap_m: f64,
}

/// The edge of `road` on `side` between internal stations `s0` and `s1`. The edge alignment's start
/// station is `s0` (the road station of its first point); its stations then run along the edge.
pub fn build_edge(road: &Alignment, side: Side, edges: &RoadEdges, s0: f64, s1: f64, name: &str) -> Result<EdgeBuild, JunctionError> {
    let (s0, s1) = (s0.max(road.start_station_m), s1.min(road.end_station_m()));
    if !(s1 - s0 > 1e-6) {
        return Err(JunctionError::geometry(format!("the edge of '{}' has no length between stations {s0:.3} and {s1:.3}", road.name)));
    }
    let sign = side.sign();
    let ws = edges.widening(side);
    let mut bps: Vec<f64> = vec![s0, s1];
    for w in ws {
        bps.extend(w.breakpoints());
    }
    bps.extend(road.element_stations());
    bps.retain(|s| *s >= s0 - 1e-12 && *s <= s1 + 1e-12);
    bps.sort_by(f64::total_cmp);
    bps.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let off = |s: f64| edges.offset_at(side, s);
    let starts = road.element_stations();
    let mut out: Vec<AlignmentElement> = Vec::new();
    let mut dev = 0.0f64;
    for w in bps.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (oa, ob) = (off(a + 1e-9 * (b - a)), off(b - 1e-9 * (b - a)));
        if (oa - ob).abs() < 1e-9 {
            let (i, _) = road.locate(0.5 * (a + b)).ok_or_else(|| JunctionError::geometry("edge piece off the road"))?;
            offset_pieces(&road.elements[i], a - starts[i], b - starts[i], sign * 0.5 * (oa + ob), &mut out)?;
        } else {
            let pa = road.point_at_offset(a, sign * off(a)).ok_or_else(|| JunctionError::geometry("taper start off the road"))?;
            let pb = road.point_at_offset(b, sign * off(b)).ok_or_else(|| JunctionError::geometry("taper end off the road"))?;
            let len = dist(pa, pb);
            out.push(AlignmentElement::line(pa.0, pa.1, heading_of(sub(pb, pa)), len));
            for k in 1..10 {
                let t = a + (b - a) * k as f64 / 10.0;
                let q = road.point_at_offset(t, sign * off(t)).ok_or_else(|| JunctionError::geometry("taper off the road"))?;
                let u = sub(pb, pa);
                let dv = ((q.0 - pa.0) * u.1 - (q.1 - pa.1) * u.0).abs() / len;
                dev = dev.max(dv);
            }
        }
    }
    let alignment = Alignment::from_elements(name, s0, out).map_err(|e| JunctionError::geometry(e.to_string()))?;
    let max_joint_gap_m = max_joint_gap(&alignment);
    Ok(EdgeBuild { alignment, max_taper_chord_deviation_m: dev, max_joint_gap_m })
}
