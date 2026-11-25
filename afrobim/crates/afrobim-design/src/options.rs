// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Alignment options and their comparison.
//!
//! An [`OptionSet`] holds several named alignment options, each an [`AlignmentDesign`] (the
//! horizontal and vertical design) plus the corridor definition used to measure it. [`compare_options`]
//! builds each option and reports a table of figures; every figure states how it was computed.
//!
//! # The figures (keys in brackets)
//!
//! | figure | how it is computed |
//! |---|---|
//! | length (`length_m`) | sum of element lengths of the alignment |
//! | curves (`curves`) | number of PIs carrying a curve |
//! | minimum radius (`min_radius_m`) | smallest radius over all arcs and spirals (a spiral counts at its smallest radius) |
//! | maximum grade (`max_grade_pct`) | greatest absolute grade over grade lines and the ends of vertical curves |
//! | rise, fall (`rise_m`, `fall_m`) | sum of upward and of downward level changes along the profile, exact: the profile is monotone between PVC, PVT, turning points |
//! | mean absolute grade (`mean_abs_grade_pct`) | `(rise + fall) / profile length` |
//! | turning (`deflection_deg_per_km`) | total absolute change of heading, `Σ (|k0| + |k1|) L / 2` over elements with curvature of one sign (exact for the linear curvature of every element), per km |
//! | share in curves (`curve_share_pct`) | length of arcs and spirals over total length |
//! | cut, fill, balance (`cut_m3`, `fill_m3`, `balance_m3`) | `afrobim_corridor::earthworks` (average end area) on the corridor built from the option's definition on the ground; balance is cut minus fill, unfactored |
//! | unmeasured length (`unmeasured_length_m`) | length of corridor for which a section has no ground or no daylight, so no volume |
//! | land take (`land_take_m2`) | corridor footprint: between consecutive sections that both have daylight points on both sides, `L (w1 + w2) / 2` with `w` the width between the outer ends of the earthworks line (daylight to daylight) and `L` the station difference. Plan width on curves is not corrected for curvature |
//! | crossings (`crossings`) | number of proper intersections of the centreline polyline (chords of at most 2 m) with each user-supplied crossing feature; listed in the option's notes with stations |
//! | cost (`cost`) | the user's [`Estimate`] computed on quantity lines made from this option: `earthworks.cut`, `earthworks.fill`, `alignment.length`, `land.take_area`, plus any extra lines the user supplies. **No rate is built in** |
//!
//! A figure that cannot be computed (no ground, no corridor, no crossing data, no estimate) is
//! reported as not computed with the reason; it is never filled with a default.
//!
//! # Scoring
//!
//! [`score`] ranks options by the **user's own weights** on the figures the user chooses. For
//! each criterion the figures of the options are scaled to 0 to 1 between the smallest and the
//! largest value (the best option gets 1: the smallest value if lower is better, the largest if
//! higher is better; equal values all get 1). The score is `Σ w_i s_i / Σ w_i`. The weights and
//! the scaling are a presentation of the user's preferences, not a finding of the model, and the
//! unweighted figures are always listed beside the scores. An option missing any weighted figure
//! gets no score, with the reason.

use std::collections::BTreeMap;

use afrobim_alignment::{Alignment, ElementKind};
use afrobim_corridor::{earthworks, Corridor, CorridorDefinition, Tin};
use afrobim_cost::{Estimate, QuantitySet};
use afrobim_kernel::ElementId;
use serde::{Deserialize, Serialize};

use crate::design::AlignmentDesign;

/// One alignment option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlignmentOption {
    pub name: String,
    pub design: AlignmentDesign,
    /// The corridor definition used to measure earthworks and land take. Without it those
    /// figures are not computed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corridor: Option<CorridorDefinition>,
    #[serde(default)]
    pub note: String,
}

/// Which way is better for a criterion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    LowerIsBetter,
    HigherIsBetter,
}

/// A scoring criterion: a figure key, the user's weight and which way is better.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Criterion {
    pub key: String,
    /// The user's weight, zero or more.
    pub weight: f64,
    pub direction: Direction,
}

/// Several options in one document.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OptionSet {
    pub name: String,
    /// The ground surface element (category `surface`) the options are measured against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<ElementId>,
    /// The estimate element (category `estimate`) holding the user's rates and bill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimate: Option<ElementId>,
    pub options: Vec<AlignmentOption>,
    #[serde(default)]
    pub criteria: Vec<Criterion>,
}

/// A line feature the centreline may cross (a river, a road, a pipeline, another alignment).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossingFeature {
    pub name: String,
    /// Free text, e.g. `river`, `road`.
    pub kind: String,
    /// Plan points, m.
    pub polyline: Vec<(f64, f64)>,
}

/// What the comparison needs besides the options.
#[derive(Default)]
pub struct CompareInputs<'a> {
    pub ground: Option<&'a Tin>,
    pub crossings: Vec<CrossingFeature>,
    pub estimate: Option<&'a Estimate>,
    /// Extra quantity lines per option name, added to those measured here before pricing.
    pub extra_quantities: BTreeMap<String, QuantitySet>,
}

/// One figure of one option.
#[derive(Debug, Clone, PartialEq)]
pub struct Figure {
    pub key: String,
    pub label: String,
    pub unit: String,
    pub value: Option<f64>,
    /// An exact text form where the value is exact (money).
    pub exact: Option<String>,
    /// How the value was computed.
    pub how: String,
    /// Why there is no value.
    pub not_computed: Option<String>,
}

/// The figures and notes of one option.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionResult {
    pub name: String,
    /// Same order for every option.
    pub figures: Vec<Figure>,
    pub notes: Vec<String>,
    /// Set when the option could not be built at all.
    pub error: Option<String>,
}

impl OptionResult {
    pub fn figure(&self, key: &str) -> Option<&Figure> {
        self.figures.iter().find(|f| f.key == key)
    }
    pub fn value(&self, key: &str) -> Option<f64> {
        self.figure(key).and_then(|f| f.value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptionsComparison {
    pub options: Vec<OptionResult>,
}

const KEYS: [(&str, &str, &str); 17] = [
    ("length_m", "Length", "m"),
    ("curves", "Number of curves", "nr"),
    ("min_radius_m", "Minimum radius", "m"),
    ("max_grade_pct", "Maximum grade", "%"),
    ("rise_m", "Total rise", "m"),
    ("fall_m", "Total fall", "m"),
    ("mean_abs_grade_pct", "Mean absolute grade", "%"),
    ("deflection_deg_per_km", "Total turning per km", "deg/km"),
    ("curve_share_pct", "Share of length in curves and spirals", "%"),
    ("cut_m3", "Cut", "m3"),
    ("fill_m3", "Fill", "m3"),
    ("balance_m3", "Balance (cut minus fill)", "m3"),
    ("unmeasured_length_m", "Length with no volume (no ground or daylight)", "m"),
    ("land_take_m2", "Land take (corridor footprint)", "m2"),
    ("crossings", "Crossings", "nr"),
    ("cost", "Cost", ""),
    ("cost_unpriced", "Bill items with no rate", "nr"),
];

fn figure(key: &str, value: Option<f64>, how: &str, why: Option<String>) -> Figure {
    let (_, label, unit) = KEYS.iter().find(|k| k.0 == key).copied().unwrap_or((key, key, ""));
    Figure { key: key.into(), label: label.into(), unit: unit.into(), value, exact: None, how: how.into(), not_computed: if value.is_none() { why.or_else(|| Some("not computed".into())) } else { None } }
}

/// The keys and labels of the figures, in table order.
pub fn figure_keys() -> Vec<(&'static str, &'static str, &'static str)> {
    KEYS.to_vec()
}

/// Intersection of segments (p1,p2) and (p3,p4), proper (interior) crossings only.
fn segment_intersection(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), p4: (f64, f64)) -> Option<(f64, f64)> {
    let (d1, d2) = ((p2.0 - p1.0, p2.1 - p1.1), (p4.0 - p3.0, p4.1 - p3.1));
    let den = d1.0 * d2.1 - d1.1 * d2.0;
    if den.abs() < 1e-14 {
        return None;
    }
    let (w0, w1) = (p3.0 - p1.0, p3.1 - p1.1);
    let t = (w0 * d2.1 - w1 * d2.0) / den;
    let u = (w0 * d1.1 - w1 * d1.0) / den;
    ((0.0..=1.0).contains(&t) && (0.0..1.0).contains(&u)).then_some((p1.0 + t * d1.0, p1.1 + t * d1.1))
}

/// Crossings of the alignment's centreline with a feature: (station, point).
pub fn crossings_with(alignment: &Alignment, feature: &CrossingFeature) -> Vec<(f64, (f64, f64))> {
    let line = alignment.polyline(2.0);
    let mut out: Vec<(f64, (f64, f64))> = Vec::new();
    for a in line.windows(2) {
        for b in feature.polyline.windows(2) {
            if let Some(p) = segment_intersection(a[0], a[1], b[0], b[1]) {
                if out.iter().all(|q| (q.1 .0 - p.0).hypot(q.1 .1 - p.1) > 1e-6) {
                    let station = alignment.station_offset(p.0, p.1).map_or(f64::NAN, |s| s.0);
                    out.push((station, p));
                }
            }
        }
    }
    out
}

/// Total absolute turning of the alignment, radians: `Σ (|k0| + |k1|) L / 2` over elements whose
/// curvature does not change sign. Compound spirals that change sign are summed in two parts.
fn total_turning(alignment: &Alignment) -> f64 {
    alignment
        .elements
        .iter()
        .map(|e| {
            let (k0, k1) = (e.curvature_start, e.curvature_end);
            if k0 * k1 >= 0.0 {
                0.5 * (k0.abs() + k1.abs()) * e.length
            } else {
                // Crosses zero curvature at s0 = L |k0| / (|k0| + |k1|).
                let s0 = e.length * k0.abs() / (k0.abs() + k1.abs());
                0.5 * k0.abs() * s0 + 0.5 * k1.abs() * (e.length - s0)
            }
        })
        .sum()
}

/// Builds each option and tabulates the figures. Options that cannot be built are listed with the error.
pub fn compare_options(set: &OptionSet, inputs: &CompareInputs) -> OptionsComparison {
    OptionsComparison { options: set.options.iter().map(|o| compare_one(o, inputs)).collect() }
}

fn compare_one(option: &AlignmentOption, inputs: &CompareInputs) -> OptionResult {
    let built = match option.design.build() {
        Ok(b) => b,
        Err(e) => {
            let reason = format!("The option cannot be built: {e}");
            return OptionResult { name: option.name.clone(), figures: KEYS.iter().map(|k| figure(k.0, None, "", Some(reason.clone()))).collect(), notes: vec![], error: Some(e.to_string()) };
        }
    };
    let alignment = &built.alignment;
    let mut notes = built.warnings.clone();
    let mut figs: Vec<Figure> = Vec::new();
    let length = alignment.length();
    figs.push(figure("length_m", Some(length), "sum of element lengths", None));
    figs.push(figure("curves", Some(built.horizontal.curves.len() as f64), "PIs with a curve", None));
    figs.push(figure("min_radius_m", alignment.min_radius(), "smallest radius on any arc or spiral", Some("the alignment has no curves".into())));
    let curved: f64 = alignment.elements.iter().filter(|e| e.kind() != ElementKind::Line).map(|e| e.length).sum();
    figs.push(figure("deflection_deg_per_km", (length > 0.0).then(|| total_turning(alignment).to_degrees() / (length / 1000.0)), "sum of |heading change| over all elements, per km", None));
    figs.push(figure("curve_share_pct", (length > 0.0).then(|| 100.0 * curved / length), "length of arcs and spirals over total length", None));

    let no_profile = || Some("the option has no vertical design".to_string());
    match &built.vertical {
        Some(v) => {
            let (rise, fall) = v.rise_and_fall();
            let span = v.profile.range().map_or(0.0, |(a, b)| b - a);
            figs.push(figure("max_grade_pct", Some(v.max_abs_grade() * 100.0), "greatest |grade| over grade lines and curve ends", None));
            figs.push(figure("rise_m", Some(rise), "sum of upward level changes between PVC, PVT, turning points and PVIs", None));
            figs.push(figure("fall_m", Some(fall), "sum of downward level changes, same points", None));
            figs.push(figure("mean_abs_grade_pct", (span > 0.0).then(|| 100.0 * (rise + fall) / span), "(rise + fall) / profile length", None));
        }
        None => {
            for k in ["max_grade_pct", "rise_m", "fall_m", "mean_abs_grade_pct"] {
                figs.push(figure(k, None, "", no_profile()));
            }
        }
    }

    // Earthworks and land take.
    let mut lines = QuantitySet::new();
    lines.push(None, &option.name, "alignment.length", &format!("Alignment {} length", option.name), length, "m", &[("option", option.name.clone())]);
    let mut earth: Option<(f64, f64, f64)> = None;
    let mut land: Option<f64> = None;
    let why = match (&option.corridor, inputs.ground, &built.vertical) {
        (None, _, _) => Some("the option has no corridor definition".to_string()),
        (_, None, _) => Some("no ground surface was supplied".to_string()),
        (_, _, None) => Some("the option has no vertical design, which a corridor needs".to_string()),
        (Some(def), Some(tin), Some(_)) => match Corridor::build(def, alignment, tin) {
            Err(e) => Some(format!("the corridor could not be built: {e}")),
            Ok(c) => {
                let ew = earthworks(&c.sections, &def.volumes);
                earth = Some((ew.total_cut_m3, ew.total_fill_m3, ew.unmeasured_length_m));
                let attrs = [("corridor", option.name.clone()), ("option", option.name.clone())];
                lines.push(None, &option.name, "earthworks.cut", &format!("Earthworks {}, cut", option.name), ew.total_cut_m3, "m3", &attrs);
                lines.push(None, &option.name, "earthworks.fill", &format!("Earthworks {}, fill", option.name), ew.total_fill_m3, "m3", &attrs);
                let width = |s: &afrobim_corridor::Section| {
                    (s.left_daylight.is_some() && s.right_daylight.is_some()).then(|| s.earthworks[s.earthworks.len() - 1].0 - s.earthworks[0].0)
                };
                let mut area = 0.0;
                let mut missing = 0.0;
                for w in c.sections.windows(2) {
                    let l = w[1].station_m - w[0].station_m;
                    match (width(&w[0]), width(&w[1])) {
                        (Some(a), Some(b)) => area += l * (a + b) / 2.0,
                        _ => missing += l,
                    }
                }
                if missing > 1e-9 {
                    notes.push(format!("Land take leaves out {missing:.1} m of corridor where the ground is missing or a side slope does not reach it."));
                }
                land = Some(area);
                lines.push(None, &option.name, "land.take_area", &format!("Land take {}", option.name), area, "m2", &[("option", option.name.clone())]);
                notes.extend(c.notes.iter().cloned());
                None
            }
        },
    };
    figs.push(figure("cut_m3", earth.map(|e| e.0), "average end area volumes of the corridor against the ground", why.clone()));
    figs.push(figure("fill_m3", earth.map(|e| e.1), "average end area volumes of the corridor against the ground", why.clone()));
    figs.push(figure("balance_m3", earth.map(|e| e.0 - e.1), "cut minus fill, unfactored", why.clone()));
    figs.push(figure("unmeasured_length_m", earth.map(|e| e.2), "corridor length between sections lacking ground or daylight", why.clone()));
    figs.push(figure("land_take_m2", land, "sum over section pairs of L (w1 + w2) / 2, daylight to daylight", why.clone()));

    // Crossings.
    if inputs.crossings.is_empty() {
        figs.push(figure("crossings", None, "", Some("no crossing features were supplied".into())));
    } else {
        let mut total = 0usize;
        for f in &inputs.crossings {
            let hits = crossings_with(alignment, f);
            for (s, _) in &hits {
                notes.push(format!("Crosses {} ({}) at station {}.", f.name, f.kind, afrobim_alignment::format_station(*s, 1)));
            }
            total += hits.len();
        }
        figs.push(figure("crossings", Some(total as f64), &format!("intersections of the centreline with {} supplied feature(s)", inputs.crossings.len()), None));
    }

    // Cost.
    match inputs.estimate {
        None => {
            figs.push(figure("cost", None, "", Some("no estimate (user rates) was supplied".into())));
            figs.push(figure("cost_unpriced", None, "", Some("no estimate was supplied".into())));
        }
        Some(est) => {
            if let Some(extra) = inputs.extra_quantities.get(&option.name) {
                lines.extend(extra.clone());
            }
            match est.compute(&lines) {
                Ok(r) => {
                    let mut f = figure("cost", Some(r.grand_total.to_f64()), "the user's estimate computed on this option's quantity lines (earthworks.cut, earthworks.fill, alignment.length, land.take_area and any extra lines)", None);
                    f.unit = r.reporting.code.clone();
                    f.exact = Some(r.grand_total.to_string());
                    figs.push(f);
                    figs.push(figure("cost_unpriced", Some(r.unpriced.len() as f64), "items of the bill that have no rate and are priced at zero", None));
                    if !r.measured.unmapped.is_empty() {
                        notes.push(format!("{} quantity line(s) of this option were taken by no bill item and are not in the cost.", r.measured.unmapped.len()));
                    }
                }
                Err(e) => {
                    figs.push(figure("cost", None, "", Some(format!("the estimate could not be computed: {e}"))));
                    figs.push(figure("cost_unpriced", None, "", Some("the estimate could not be computed".into())));
                }
            }
        }
    }
    // Table order.
    figs.sort_by_key(|f| KEYS.iter().position(|k| k.0 == f.key).unwrap_or(usize::MAX));
    OptionResult { name: option.name.clone(), figures: figs, notes, error: None }
}

/// One criterion's scaled value for one option.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreCell {
    pub key: String,
    pub raw: Option<f64>,
    /// 0 to 1, 1 best.
    pub scaled: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptionScore {
    pub name: String,
    pub cells: Vec<ScoreCell>,
    /// `Σ w s / Σ w`, 0 to 1, `None` when a weighted figure is missing or no weight is positive.
    pub score: Option<f64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreTable {
    pub criteria: Vec<Criterion>,
    pub options: Vec<OptionScore>,
}

/// Weighted scoring on the user's criteria. See the module documentation.
pub fn score(comparison: &OptionsComparison, criteria: &[Criterion]) -> ScoreTable {
    let used: Vec<&Criterion> = criteria.iter().filter(|c| c.weight > 0.0).collect();
    let range = |key: &str| {
        let vals: Vec<f64> = comparison.options.iter().filter_map(|o| o.value(key)).collect();
        (vals.iter().cloned().fold(f64::INFINITY, f64::min), vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max))
    };
    let total_w: f64 = used.iter().map(|c| c.weight).sum();
    let options = comparison
        .options
        .iter()
        .map(|o| {
            let mut cells = Vec::new();
            let mut sum = 0.0;
            let mut missing: Vec<String> = Vec::new();
            for c in criteria {
                let raw = o.value(&c.key);
                let (lo, hi) = range(&c.key);
                let scaled = raw.map(|v| {
                    if hi - lo < 1e-12 {
                        1.0
                    } else if c.direction == Direction::LowerIsBetter {
                        (hi - v) / (hi - lo)
                    } else {
                        (v - lo) / (hi - lo)
                    }
                });
                if c.weight > 0.0 {
                    match scaled {
                        Some(s) => sum += c.weight * s,
                        None => missing.push(c.key.clone()),
                    }
                }
                cells.push(ScoreCell { key: c.key.clone(), raw, scaled });
            }
            let (score, reason) = if used.is_empty() || total_w <= 0.0 {
                (None, Some("no criterion has a positive weight".to_string()))
            } else if !missing.is_empty() {
                (None, Some(format!("no value for: {}", missing.join(", "))))
            } else {
                (Some(sum / total_w), None)
            };
            OptionScore { name: o.name.clone(), cells, score, reason }
        })
        .collect();
    ScoreTable { criteria: criteria.to_vec(), options }
}

fn q(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// The comparison as CSV: one row per figure, one column per option, then a column saying how
/// each figure is computed, then the notes and the reasons for figures not computed. The
/// unweighted figures only; scores are in [`scoring_csv`].
pub fn options_comparison_csv(c: &OptionsComparison) -> String {
    let mut out = String::from("Figure,Unit");
    for o in &c.options {
        out.push(',');
        out.push_str(&q(&o.name));
    }
    out.push_str(",How computed\n");
    for (i, (key, label, unit)) in KEYS.iter().enumerate() {
        let unit = c.options.iter().find_map(|o| o.figures.get(i).filter(|f| f.value.is_some()).map(|f| f.unit.clone())).unwrap_or_else(|| unit.to_string());
        out.push_str(&format!("{},{}", q(label), q(&unit)));
        let mut how = String::new();
        for o in &c.options {
            let f = o.figures.get(i);
            let cell = match f {
                Some(f) if f.exact.is_some() => f.exact.clone().unwrap_or_default(),
                Some(f) if f.value.is_some() => format!("{:.3}", f.value.unwrap_or(0.0)),
                _ => "n/a".to_string(),
            };
            if how.is_empty() {
                how = f.filter(|f| f.value.is_some()).map(|f| f.how.clone()).unwrap_or_default();
            }
            out.push(',');
            out.push_str(&cell);
        }
        let _ = key;
        out.push_str(&format!(",{}\n", q(&how)));
    }
    for o in &c.options {
        for f in &o.figures {
            if let Some(why) = &f.not_computed {
                out.push_str(&format!("# not computed,{},{},{}\n", q(&o.name), q(&f.label), q(why)));
            }
        }
        for n in &o.notes {
            out.push_str(&format!("# note,{},{}\n", q(&o.name), q(n)));
        }
    }
    out
}

/// The scoring table as CSV, labelled as the user's weights.
pub fn scoring_csv(t: &ScoreTable) -> String {
    let mut out = String::from("# Weighted scores use the USER'S OWN weights and scaling (0 = worst option, 1 = best option per criterion). They are not a finding of the model; see the unweighted figures.\n");
    out.push_str("Criterion,Weight,Better when");
    for o in &t.options {
        out.push_str(&format!(",{} (value),{} (scaled)", q(&o.name), q(&o.name)));
    }
    out.push('\n');
    for (i, c) in t.criteria.iter().enumerate() {
        let label = KEYS.iter().find(|k| k.0 == c.key).map_or(c.key.clone(), |k| k.1.to_string());
        out.push_str(&format!("{},{},{}", q(&label), c.weight, if c.direction == Direction::LowerIsBetter { "lower" } else { "higher" }));
        for o in &t.options {
            let cell = &o.cells[i];
            out.push_str(&format!(",{},{}", cell.raw.map_or("n/a".into(), |v| format!("{v:.3}")), cell.scaled.map_or("n/a".into(), |v| format!("{v:.3}"))));
        }
        out.push('\n');
    }
    out.push_str("Weighted score,,");
    for o in &t.options {
        out.push_str(&format!(",{},", o.score.map_or("n/a".into(), |v| format!("{v:.3}"))));
    }
    out.push('\n');
    for o in &t.options {
        if let Some(r) = &o.reason {
            out.push_str(&format!("# no score,{},{}\n", q(&o.name), q(r)));
        }
    }
    out
}
