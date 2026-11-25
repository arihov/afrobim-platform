// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Design checks against a standard whose limits the user supplies.
//!
//! **This crate ships no limits.** A [`DesignBasis`] is a table of rows (for instance one per
//! design speed), every value optional, loaded from JSON or CSV or built in code. Each basis
//! names its `source` (document, edition, tables, pages); that text is printed on every report
//! so a result can always be traced to where its limit came from. A check for which the row
//! holds no limit is reported as `NotChecked`, never silently passed.
//!
//! Values that physics fixes can be left out of the table and derived from the inputs
//! (see [`crate::physics`]): the minimum radius from `max_superelevation` and `max_side_friction`
//! (`R = V² / (12.96 g (e + f))`), the required stopping sight distance from `reaction_time_s`
//! and `longitudinal_friction`, the minimum K from sight distance and the eye, object and
//! headlight inputs. An explicit table value always wins over a derived one.
//!
//! # What is checked
//!
//! | id | check |
//! |---|---|
//! | `H.radius` | each curve radius against the minimum radius |
//! | `H.curve_length` | each curve's total length (spirals included) against the minimum |
//! | `H.spiral_length` | each spiral against the minimum spiral length |
//! | `H.tangent_same`, `H.tangent_reverse` | straight between two curves turning the same way, or opposite ways |
//! | `V.max_grade`, `V.min_grade` | each grade line |
//! | `V.k_crest`, `V.k_sag` | K of each vertical curve |
//! | `V.curve_length` | length of each vertical curve |
//! | `V.ssd` | stopping sight distance provided over each crest and sag, by the numerical sight line test, against the required distance |
//!
//! Not checked: sight distance on horizontal curves (lateral clearance), superelevation
//! rates and runoff, decision and passing sight distance, combinations of horizontal and
//! vertical alignment, and anything that is not geometry.

use serde::{Deserialize, Serialize};

use crate::error::DesignError;
use crate::horizontal::HorizontalSolution;
use crate::physics;
use crate::vertical::{CurveKind, VerticalSolution};

/// One row of limits, for example one design speed. Every value is optional and entered by the user.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StandardRow {
    /// Free label, e.g. `100 km/h rolling terrain`.
    pub label: String,
    pub design_speed_kmh: Option<f64>,
    pub min_radius_m: Option<f64>,
    /// Maximum superelevation `e` (ratio) and side-friction factor `f` (ratio), to derive a minimum radius.
    pub max_superelevation: Option<f64>,
    pub max_side_friction: Option<f64>,
    pub min_curve_length_m: Option<f64>,
    pub min_spiral_length_m: Option<f64>,
    pub min_tangent_same_direction_m: Option<f64>,
    pub min_tangent_reverse_m: Option<f64>,
    /// Ratios (0.06 is 6 %).
    pub max_grade: Option<f64>,
    pub min_grade: Option<f64>,
    pub min_k_crest: Option<f64>,
    pub min_k_sag: Option<f64>,
    pub min_vertical_curve_length_m: Option<f64>,
    pub stopping_sight_distance_m: Option<f64>,
    /// Inputs for deriving the stopping sight distance.
    pub reaction_time_s: Option<f64>,
    pub longitudinal_friction: Option<f64>,
    /// Eye and object heights for crest curves, headlight height and beam angle for sags.
    pub eye_height_m: Option<f64>,
    pub object_height_m: Option<f64>,
    pub headlight_height_m: Option<f64>,
    pub headlight_beam_deg: Option<f64>,
    /// Where each limit comes from, keyed by the field name (`min_radius_m`, `max_grade`, ...): document,
    /// edition, table or figure, section and page. The text is added to the limit basis printed on every
    /// check item. For a field that is `None` the text may say why the standard gives no value. Empty for
    /// rows read from CSV, which carry their source in the `#source` line of the basis.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub limit_sources: std::collections::BTreeMap<String, String>,
}

impl StandardRow {
    /// This row with every value that `over` states replacing the value here: the way a user's own table
    /// overrides a built-in one for the limits the user supplies. The label, the sources of replaced values
    /// and `over`'s own sources follow `over`.
    pub fn overlay(&self, over: &StandardRow) -> StandardRow {
        let mut out = self.clone();
        macro_rules! take {
            ($($f:ident),*) => {$(
                if over.$f.is_some() {
                    out.$f = over.$f;
                    out.limit_sources.remove(stringify!($f));
                }
            )*};
        }
        take!(
            design_speed_kmh,
            min_radius_m,
            max_superelevation,
            max_side_friction,
            min_curve_length_m,
            min_spiral_length_m,
            min_tangent_same_direction_m,
            min_tangent_reverse_m,
            max_grade,
            min_grade,
            min_k_crest,
            min_k_sag,
            min_vertical_curve_length_m,
            stopping_sight_distance_m,
            reaction_time_s,
            longitudinal_friction,
            eye_height_m,
            object_height_m,
            headlight_height_m,
            headlight_beam_deg
        );
        for (k, v) in &over.limit_sources {
            out.limit_sources.insert(k.clone(), v.clone());
        }
        if !over.label.is_empty() {
            out.label = over.label.clone();
        }
        out
    }

    /// The source text stored for a limit, if any.
    pub fn source_of(&self, field: &str) -> Option<&str> {
        self.limit_sources.get(field).map(|s| s.as_str())
    }
}

/// A named set of rows and where they come from.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DesignBasis {
    pub name: String,
    /// Document, edition, tables and pages the values come from, or `user input`. Required.
    pub source: String,
    #[serde(default)]
    pub notes: String,
    pub rows: Vec<StandardRow>,
}

const CSV_COLUMNS: [&str; 20] = [
    "label",
    "design_speed_kmh",
    "min_radius_m",
    "max_superelevation",
    "max_side_friction",
    "min_curve_length_m",
    "min_spiral_length_m",
    "min_tangent_same_direction_m",
    "min_tangent_reverse_m",
    "max_grade",
    "min_grade",
    "min_k_crest",
    "min_k_sag",
    "min_vertical_curve_length_m",
    "stopping_sight_distance_m",
    "reaction_time_s",
    "longitudinal_friction",
    "eye_height_m",
    "object_height_m",
    "headlight_height_m",
];

impl DesignBasis {
    /// Reads a basis from JSON (the serde form of this struct).
    pub fn from_json_str(text: &str) -> Result<DesignBasis, DesignError> {
        let b: DesignBasis = serde_json::from_str(text).map_err(|e| DesignError::Input(format!("The limits table is not valid JSON for a design basis: {e}")))?;
        b.validate()?;
        Ok(b)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    fn validate(&self) -> Result<(), DesignError> {
        if self.source.trim().is_empty() {
            return Err(DesignError::Input("A design basis must state its source (document, edition, table, page) or say `user input`.".into()));
        }
        if self.rows.is_empty() {
            return Err(DesignError::Input("The limits table has no rows.".into()));
        }
        Ok(())
    }

    /// Reads a basis from CSV: a header naming columns (any of the field names of [`StandardRow`];
    /// `headlight_beam_deg` too), one row per line, an empty cell for "no limit". The first two
    /// lines may be `#name,<name>` and `#source,<source>`; without a source the basis is refused.
    pub fn from_csv_str(text: &str) -> Result<DesignBasis, DesignError> {
        let mut name = String::from("User limits");
        let mut source = String::new();
        let mut header: Option<Vec<String>> = None;
        let mut rows = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('#') {
                let cells = split_csv(rest);
                match (cells.first().map(|s| s.trim()), cells.get(1)) {
                    (Some("name"), Some(v)) => name = v.trim().to_string(),
                    (Some("source"), Some(v)) => source = v.trim().to_string(),
                    _ => {}
                }
                continue;
            }
            let cells = split_csv(line);
            let Some(h) = &header else {
                header = Some(cells.iter().map(|c| c.trim().to_string()).collect());
                continue;
            };
            let mut row = StandardRow::default();
            for (col, cell) in h.iter().zip(cells.iter()) {
                let cell = cell.trim();
                if col == "label" {
                    row.label = cell.to_string();
                    continue;
                }
                if cell.is_empty() {
                    continue;
                }
                let v: f64 = cell.parse().map_err(|_| DesignError::Input(format!("Line {}: '{cell}' in column {col} is not a number.", n + 1)))?;
                let slot = match col.as_str() {
                    "design_speed_kmh" => &mut row.design_speed_kmh,
                    "min_radius_m" => &mut row.min_radius_m,
                    "max_superelevation" => &mut row.max_superelevation,
                    "max_side_friction" => &mut row.max_side_friction,
                    "min_curve_length_m" => &mut row.min_curve_length_m,
                    "min_spiral_length_m" => &mut row.min_spiral_length_m,
                    "min_tangent_same_direction_m" => &mut row.min_tangent_same_direction_m,
                    "min_tangent_reverse_m" => &mut row.min_tangent_reverse_m,
                    "max_grade" => &mut row.max_grade,
                    "min_grade" => &mut row.min_grade,
                    "min_k_crest" => &mut row.min_k_crest,
                    "min_k_sag" => &mut row.min_k_sag,
                    "min_vertical_curve_length_m" => &mut row.min_vertical_curve_length_m,
                    "stopping_sight_distance_m" => &mut row.stopping_sight_distance_m,
                    "reaction_time_s" => &mut row.reaction_time_s,
                    "longitudinal_friction" => &mut row.longitudinal_friction,
                    "eye_height_m" => &mut row.eye_height_m,
                    "object_height_m" => &mut row.object_height_m,
                    "headlight_height_m" => &mut row.headlight_height_m,
                    "headlight_beam_deg" => &mut row.headlight_beam_deg,
                    other => return Err(DesignError::Input(format!("Unknown column '{other}'. Known columns: {}, headlight_beam_deg.", CSV_COLUMNS.join(", ")))),
                };
                *slot = Some(v);
            }
            rows.push(row);
        }
        let b = DesignBasis { name, source, notes: String::new(), rows };
        b.validate()?;
        Ok(b)
    }

    /// The row with this label.
    pub fn row(&self, label: &str) -> Option<&StandardRow> {
        self.rows.iter().find(|r| r.label == label)
    }
}

fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckStatus {
    Pass,
    Fail,
    /// No limit in the table (and none derivable), so nothing could be checked.
    NotChecked,
}

/// Which way the limit runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bound {
    /// The value must be at least the limit.
    AtLeast,
    /// The value must be at most the limit.
    AtMost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckItem {
    pub id: String,
    /// Where: `PI 3`, `PVI 2`, `grade PVI 1 to 2`.
    pub location: String,
    pub description: String,
    pub value: Option<f64>,
    pub limit: Option<f64>,
    pub bound: Bound,
    pub unit: String,
    pub status: CheckStatus,
    /// How the limit was obtained: from the table, or derived and from which inputs.
    pub limit_basis: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckReport {
    pub basis_name: String,
    pub source: String,
    pub row_label: String,
    pub items: Vec<CheckItem>,
}

impl CheckReport {
    pub fn count(&self, s: CheckStatus) -> usize {
        self.items.iter().filter(|i| i.status == s).count()
    }

    pub fn failures(&self) -> impl Iterator<Item = &CheckItem> {
        self.items.iter().filter(|i| i.status == CheckStatus::Fail)
    }

    /// The report as CSV, the source and row on the first lines.
    pub fn to_csv(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut out = format!("# basis,{}\n# source,{}\n# row,{}\n", q(&self.basis_name), q(&self.source), q(&self.row_label));
        out.push_str("id,location,description,value,limit,bound,unit,status,limit basis,message\n");
        let num = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.4}"));
        for i in &self.items {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{:?},{},{}\n",
                i.id,
                q(&i.location),
                q(&i.description),
                num(i.value),
                num(i.limit),
                if i.bound == Bound::AtLeast { "at least" } else { "at most" },
                i.unit,
                i.status,
                q(&i.limit_basis),
                q(&i.message)
            ));
        }
        out
    }
}

/// Extra settings for a check run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckOptions {
    /// Step of the numerical sight-line test, m.
    pub sight_step_m: f64,
    /// Tolerance on comparisons, to keep a value equal to its limit from failing on rounding.
    pub tolerance: f64,
}

impl Default for CheckOptions {
    fn default() -> Self {
        CheckOptions { sight_step_m: 2.0, tolerance: 1e-9 }
    }
}

struct Limit {
    value: Option<f64>,
    basis: String,
}

fn from_table(row: &StandardRow, field: &str, v: Option<f64>, what: &str) -> Limit {
    let src = row.source_of(field);
    let basis = match (v.is_some(), src) {
        (true, Some(s)) => format!("table value ({what}); source: {s}"),
        (true, None) => format!("table value ({what})"),
        (false, Some(s)) => format!("no {what} in the table; {s}"),
        (false, None) => format!("no {what} in the table"),
    };
    Limit { value: v, basis }
}

/// Runs every check on a horizontal and/or vertical solution against one row of a basis.
pub fn run_checks(basis: &DesignBasis, row: &StandardRow, horizontal: Option<&HorizontalSolution>, vertical: Option<&VerticalSolution>, options: &CheckOptions) -> CheckReport {
    let mut items: Vec<CheckItem> = Vec::new();
    let tol = options.tolerance;
    let mut add = |id: &str, location: String, description: String, value: Option<f64>, limit: &Limit, bound: Bound, unit: &str| {
        let (status, message) = match (value, limit.value) {
            (Some(v), Some(l)) => {
                let ok = match bound {
                    Bound::AtLeast => v >= l - tol * l.abs().max(1.0),
                    Bound::AtMost => v <= l + tol * l.abs().max(1.0),
                };
                (
                    if ok { CheckStatus::Pass } else { CheckStatus::Fail },
                    format!("{description}: {v:.3} {unit}, limit {} {l:.3} {unit}.", if bound == Bound::AtLeast { "at least" } else { "at most" }),
                )
            }
            (_, None) => (CheckStatus::NotChecked, format!("{description}: not checked, {}.", limit.basis)),
            (None, _) => (CheckStatus::NotChecked, format!("{description}: not checked, the value could not be computed.")),
        };
        items.push(CheckItem { id: id.into(), location, description, value, limit: limit.value, bound, unit: unit.into(), status, limit_basis: limit.basis.clone(), message });
    };

    if let Some(h) = horizontal {
        let radius_limit = match (row.min_radius_m, row.design_speed_kmh, row.max_superelevation, row.max_side_friction) {
            (Some(r), _, _, _) => from_table(row, "min_radius_m", Some(r), "minimum radius"),
            (None, Some(v), Some(e), Some(f)) => Limit {
                value: physics::min_radius_m(v, e, f),
                basis: format!("derived: R = V^2 / (12.96 g (e + f)) with V = {v} km/h, e = {e}, f = {f}"),
            },
            _ => Limit { value: None, basis: "no minimum radius, and not enough inputs (design speed, maximum e and f) to derive one".into() },
        };
        let len_limit = from_table(row, "min_curve_length_m", row.min_curve_length_m, "minimum curve length");
        let spiral_limit = from_table(row, "min_spiral_length_m", row.min_spiral_length_m, "minimum spiral length");
        for c in &h.curves {
            let loc = format!("PI {}", c.pi_index);
            add("H.radius", loc.clone(), "Curve radius".into(), Some(c.radius_m), &radius_limit, Bound::AtLeast, "m");
            add("H.curve_length", loc.clone(), "Curve length (spirals included)".into(), Some(c.total_length_m), &len_limit, Bound::AtLeast, "m");
            for (which, l) in [("in", c.spiral_in_m), ("out", c.spiral_out_m)] {
                if l > 0.0 {
                    add("H.spiral_length", loc.clone(), format!("Spiral {which} length"), Some(l), &spiral_limit, Bound::AtLeast, "m");
                }
            }
        }
        let same = from_table(row, "min_tangent_same_direction_m", row.min_tangent_same_direction_m, "minimum tangent between curves in the same direction");
        let rev = from_table(row, "min_tangent_reverse_m", row.min_tangent_reverse_m, "minimum tangent between reverse curves");
        for t in &h.tangents {
            if let Some(r) = t.reverse {
                let (id, lim, what) = if r { ("H.tangent_reverse", &rev, "reverse") } else { ("H.tangent_same", &same, "same direction") };
                add(id, format!("PI {} to PI {}", t.from_pi, t.from_pi + 1), format!("Tangent between curves ({what})"), Some(t.straight_m), lim, Bound::AtLeast, "m");
            }
        }
    }

    if let Some(v) = vertical {
        let max_g = from_table(row, "max_grade", row.max_grade, "maximum grade");
        let min_g = from_table(row, "min_grade", row.min_grade, "minimum grade");
        for g in &v.grades {
            let loc = format!("grade PVI {} to {}", g.from_pvi, g.from_pvi + 1);
            add("V.max_grade", loc.clone(), "Grade (absolute)".into(), Some(g.grade.abs() * 100.0), &Limit { value: max_g.value.map(|x| x * 100.0), basis: max_g.basis.clone() }, Bound::AtMost, "%");
            if min_g.value.is_some() {
                add("V.min_grade", loc, "Grade (absolute)".into(), Some(g.grade.abs() * 100.0), &Limit { value: min_g.value.map(|x| x * 100.0), basis: min_g.basis.clone() }, Bound::AtLeast, "%");
            }
        }
        let ssd_limit = match (row.stopping_sight_distance_m, row.design_speed_kmh, row.reaction_time_s, row.longitudinal_friction) {
            (Some(s), ..) => from_table(row, "stopping_sight_distance_m", Some(s), "stopping sight distance"),
            (None, Some(vk), Some(t), Some(f)) => Limit {
                value: physics::stopping_distance_m(vk, t, f, 0.0),
                basis: format!("derived on the level: S = v t + v^2 / (2 g f) with V = {vk} km/h, t = {t} s, f = {f}"),
            },
            _ => Limit { value: None, basis: "no stopping sight distance, and not enough inputs to derive one".into() },
        };
        let k_crest = match (row.min_k_crest, ssd_limit.value, row.eye_height_m, row.object_height_m) {
            (Some(k), ..) => from_table(row, "min_k_crest", Some(k), "minimum K, crest"),
            (None, Some(s), Some(h1), Some(h2)) => Limit {
                value: physics::crest_k_for_sight(s, h1, h2),
                basis: format!("derived: K = S^2 / (200 (sqrt h1 + sqrt h2)^2) with S = {s:.1} m, h1 = {h1} m, h2 = {h2} m (valid while S <= L)"),
            },
            _ => Limit { value: None, basis: "no minimum K for crests, and not enough inputs to derive one".into() },
        };
        let k_sag = match (row.min_k_sag, ssd_limit.value, row.headlight_height_m, row.headlight_beam_deg) {
            (Some(k), ..) => from_table(row, "min_k_sag", Some(k), "minimum K, sag"),
            (None, Some(s), Some(h), Some(a)) => Limit {
                value: physics::sag_k_for_sight(s, h, a),
                basis: format!("derived: K = S^2 / (200 (H + S tan a)) with S = {s:.1} m, H = {h} m, a = {a} deg (valid while S <= L)"),
            },
            _ => Limit { value: None, basis: "no minimum K for sags, and not enough inputs to derive one".into() },
        };
        let vlen = from_table(row, "min_vertical_curve_length_m", row.min_vertical_curve_length_m, "minimum vertical curve length");
        for c in &v.curves {
            let loc = format!("PVI {}", c.pvi_index);
            let (id, lim, what) = match c.kind {
                CurveKind::Crest => ("V.k_crest", &k_crest, "K of crest curve"),
                CurveKind::Sag => ("V.k_sag", &k_sag, "K of sag curve"),
            };
            add(id, loc.clone(), what.into(), Some(c.k), lim, Bound::AtLeast, "m/%");
            add("V.curve_length", loc.clone(), "Vertical curve length".into(), Some(c.length_m), &vlen, Bound::AtLeast, "m");

            // Stopping sight distance provided, by the numerical sight line.
            let provided = ssd_limit.value.and_then(|req| {
                let step = options.sight_step_m;
                match c.kind {
                    CurveKind::Crest => {
                        let (h1, h2) = (row.eye_height_m?, row.object_height_m?);
                        let (d, _) = physics::crest_available_sight_distance(&v.profile, c.pvc_station_m - req, c.pvt_station_m, h1, h2, req, step);
                        Some(d)
                    }
                    CurveKind::Sag => {
                        let (h, a) = (row.headlight_height_m?, row.headlight_beam_deg?);
                        let mut worst = req;
                        let mut s = c.pvc_station_m - req;
                        while s <= c.pvt_station_m {
                            worst = worst.min(physics::sag_sight_distance(&v.profile, s, h, a, req, step.min(1.0)));
                            s += step;
                        }
                        Some(worst)
                    }
                }
            });
            add("V.ssd", loc, format!("Stopping sight distance provided ({})", if c.kind == CurveKind::Crest { "crest, sight line over the curve" } else { "sag, headlight beam" }), provided, &ssd_limit, Bound::AtLeast, "m");
        }
    }

    CheckReport { basis_name: basis.name.clone(), source: basis.source.clone(), row_label: row.label.clone(), items }
}
