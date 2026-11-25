// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The one place that knows how every kind of element carries coordinates, and so the one place
//! that can move a document from one coordinate system to another.
//!
//! Rules of the application: objects sit at source coordinates; a reprojection happens only when
//! the user runs the TRANSFORM command, has seen the plan (EPSG code, stated accuracy, warnings)
//! and ticked the acknowledgement, and it is written to the transform log of the document. This
//! module does the work and keeps the log; the dialog is `transform_ui`.
//!
//! Every element category is classified by [`how`]:
//!
//! * `Rules`: the category's `data` holds coordinates at known places, listed in [`rules`]
//!   (a path into the JSON and the kind of leaf). The element's own origin follows.
//! * `Alignment`: horizontal elements carry a start point, a heading, a length and curvatures;
//!   the start point is transformed exactly, the heading and length follow the local bearing and
//!   scale change of the transformation (measured numerically along the element).
//! * `Placed`: the data holds stations, offsets and sizes only; the element's origin and site
//!   position are transformed. A safety check refuses the element if its data holds a number that
//!   looks like a coordinate of the document and is not transformed.
//! * `NotGeographic`: settings, sheets, schedules: nothing to move.
//! * `Unsupported`: a category this module has not been taught. A whole-document transform is
//!   refused while one is present (rather than leave a document that is half in each system).
//!
//! Heights are never converted (the kernel has no geoid model). Everything is computed on a copy
//! of the elements and committed only if every position transformed, so a failure changes nothing,
//! and undo (a snapshot of the elements) restores the document exactly.

use afrobim_kernel::crs_plan::{SiteCalibration, TransformLog, TransformPlan};
use afrobim_kernel::{crs_from_model_point, model_point_from_crs, project_setting, set_project_crs, set_project_setting, Document, Element, ElementId, ProjectCrs};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::civil;

pub const LOG_KEY: &str = "transform_log";
pub const CALIBRATIONS_KEY: &str = "site_calibrations";

/// How a category is treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    Rules,
    Alignment,
    Placed,
    /// Annotation (text, dimension, leader): its data is local to the element's origin, so moving the
    /// origin moves the drawing. The turn and scale of the transformation are not applied to it.
    Rigid,
    /// A DXF underlay: its placement (offset, turn, scale) is moved through the transformation.
    Underlay,
    /// A reference model: its placement (offset, turn) is moved; the model stays tied to its own file.
    Reference,
    /// Results derived from the geometry (clashes): cleared, to be worked out again.
    Derived,
    /// A GIS overlay: it keeps its features in WGS 84 and is placed again in the new system, so
    /// nothing in it is moved.
    Reprojected,
    NotGeographic,
    Unsupported,
}

impl How {
    pub fn label(self) -> &'static str {
        match self {
            How::Rules => "coordinates in its data are transformed",
            How::Alignment => "alignment elements are transformed",
            How::Placed => "position is transformed (data holds stations and offsets)",
            How::Rigid => "the drawing moves with its origin; the turn and scale of the transformation are not applied (text stays upright)",
            How::Underlay => "placement (offset, turn, scale) is transformed",
            How::Reference => "placement (offset, turn) is transformed; the model stays tied to its own file and is not scaled",
            How::Derived => "derived results are cleared and are worked out again by running the tests",
            How::Reprojected => "kept in WGS 84 and placed again in the new system: nothing is moved",
            How::NotGeographic => "no coordinates: nothing to move",
            How::Unsupported => "NOT SUPPORTED: cannot be transformed",
        }
    }
}

/// The leaf a rule transforms.
#[derive(Debug, Clone, Copy)]
pub enum Leaf {
    /// Any object under the scope with numeric keys `.0` and `.1` (and the level in `.2` if given).
    Obj(&'static str, &'static str, Option<&'static str>),
    /// Any value under the key that is an array of numbers (easting, northing[, level]) or an array of such arrays.
    Arr(&'static str),
    /// The scope node itself is an array of numbers (or an array of such arrays).
    Direct,
}

#[derive(Debug, Clone, Copy)]
pub struct Rule {
    pub scope: &'static str,
    pub leaf: Leaf,
}

const fn r(scope: &'static str, leaf: Leaf) -> Rule {
    Rule { scope, leaf }
}

pub fn how(category: &str) -> How {
    match category {
        c if rules(c).is_some() => How::Rules,
        "alignment" => How::Alignment,
        afrobim_annotate::TEXT_CATEGORY | afrobim_annotate::DIM_CATEGORY | afrobim_annotate::LEADER_CATEGORY => How::Rigid,
        crate::underlay::CATEGORY => How::Underlay,
        // An image keeps its own coordinate system and is re-placed from it, so a transformation
        // of the document never edits it.
        crate::imagery::CATEGORY => How::NotGeographic,
        // A GIS overlay keeps its source in WGS 84 and is re-projected from it (see `gis_overlay::reproject_all`).
        crate::gis_overlay::CATEGORY => How::Reprojected,
        crate::reference_models::CATEGORY => How::Reference,
        crate::coordination_glue::CATEGORY => How::Derived,
        "sign" | "post" | "foundation" | "lighting" | "restraint" | "drain" | "culvert" | "marking" | "marking_ext" | "pavement" | "gantry" => How::Placed,
        afrobim_kernel::PROJECT_SETTINGS_CATEGORY | "project_info" | "ann-settings" | "sheets" | "schedule" | "estimate" | "progress" => How::NotGeographic,
        _ => How::Unsupported,
    }
}

/// The coordinate rules of a category, or None when it has none.
pub fn rules(category: &str) -> Option<&'static [Rule]> {
    const POINTS: &[Rule] = &[r("store", Leaf::Obj("easting", "northing", Some("elevation")))];
    const SURFACE: &[Rule] = &[
        r("surface.definition", Leaf::Obj("x", "y", Some("z"))),
        r("surface.definition", Leaf::Arr("vertices")),
        r("surface.definition", Leaf::Arr("lines")),
        r("surface.definition", Leaf::Arr("a")),
        r("surface.definition", Leaf::Arr("b")),
    ];
    const INSERTION: &[Rule] = &[r("insertion", Leaf::Direct)];
    const FEATURE: &[Rule] = &[r("line", Leaf::Obj("x", "y", Some("z")))];
    const PARCEL: &[Rule] = &[r("", Leaf::Obj("e", "n", None))];
    const PIPENET: &[Rule] = &[r("network", Leaf::Obj("easting_m", "northing_m", None))];
    const INTERSECTION: &[Rule] = &[r("crossing", Leaf::Direct), r("low_points", Leaf::Direct), r("inputs", Leaf::Obj("x", "y", None))];
    const ROUNDABOUT: &[Rule] = &[r("inputs.centre", Leaf::Direct), r("arms", Leaf::Arr("entry_path_points"))];
    const DESIGN: &[Rule] = &[r("", Leaf::Obj("x", "y", None)), r("", Leaf::Arr("polyline"))];
    const OLD_SURFACE: &[Rule] = &[r("", Leaf::Obj("x", "y", Some("z"))), r("", Leaf::Obj("easting", "northing", Some("elevation"))), r("", Leaf::Arr("vertices"))];
    // A point cloud's points stay in their file: the document keeps its footprint and clip polygons
    // (moved here) and a record of the transformations to draw the points through (see `note_cloud_move`).
    const CLOUD: &[Rule] = &[r("extent", Leaf::Direct), r("clip", Leaf::Direct), r("corridor", Leaf::Arr("line"))];
    const FIGURES: &[Rule] = &[r("figures", Leaf::Arr("vertices"))];
    const NONE: &[Rule] = &[];
    Some(match category {
        civil::POINTS => POINTS,
        civil::SURFACE => SURFACE,
        civil::PROFILE_VIEW | civil::SAMPLES | civil::PIPEPROFILE => INSERTION,
        civil::FEATURELINE => FEATURE,
        civil::PARCEL => PARCEL,
        civil::PIPENET => PIPENET,
        civil::INTERSECTION => INTERSECTION,
        civil::ROUNDABOUT => ROUNDABOUT,
        // Hold references by name or id and stations only.
        civil::PROFILE | civil::ASSEMBLY | civil::CORRIDOR | civil::GRADING => NONE,
        "alignment_design" | "alignment_options" => DESIGN,
        afrobim_corridor::SURFACE_CATEGORY | afrobim_corridor::CORRIDOR_CATEGORY => OLD_SURFACE,
        civil::POINTCLOUD => CLOUD,
        civil::FIGURES => FIGURES,
        _ => return None,
    })
}

// ─── the mapper ────────────────────────────────────────────────────────

/// A position mapper: (first, second, level) in, (first, second) out.
pub type MapFn<'a> = dyn Fn(f64, f64, f64) -> Result<(f64, f64), String> + 'a;

struct Mapper<'a> {
    f: &'a MapFn<'a>,
    positions: usize,
    error: Option<String>,
}

impl Mapper<'_> {
    fn map(&mut self, e: f64, n: f64, z: f64) -> (f64, f64) {
        if !(e.is_finite() && n.is_finite()) {
            // NaN placeholders (no value) stay as they are.
            return (e, n);
        }
        match (self.f)(e, n, if z.is_finite() { z } else { 0.0 }) {
            Ok(p) => {
                self.positions += 1;
                p
            }
            Err(why) => {
                if self.error.is_none() {
                    self.error = Some(why);
                }
                (e, n)
            }
        }
    }
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn put(v: &mut Value, x: f64) {
    if let Some(n) = serde_json::Number::from_f64(x) {
        *v = Value::Number(n);
    }
}

/// A node that is an array of numbers (a position) or an array of such arrays.
fn map_array(v: &mut Value, m: &mut Mapper) {
    let Value::Array(a) = v else { return };
    if a.len() >= 2 && a[0].is_number() && a[1].is_number() {
        let (e, n) = (num(&a[0]).unwrap_or(f64::NAN), num(&a[1]).unwrap_or(f64::NAN));
        let z = a.get(2).and_then(num).unwrap_or(0.0);
        let (e2, n2) = m.map(e, n, z);
        put(&mut a[0], e2);
        put(&mut a[1], n2);
    } else {
        for x in a.iter_mut() {
            map_array(x, m);
        }
    }
}

fn walk_obj(v: &mut Value, a: &str, b: &str, z: Option<&str>, m: &mut Mapper) {
    match v {
        Value::Object(o) => {
            if let (Some(e), Some(n)) = (o.get(a).and_then(num), o.get(b).and_then(num)) {
                let level = z.and_then(|k| o.get(k)).and_then(num).unwrap_or(0.0);
                let (e2, n2) = m.map(e, n, level);
                if let Some(x) = o.get_mut(a) {
                    put(x, e2);
                }
                if let Some(x) = o.get_mut(b) {
                    put(x, n2);
                }
            }
            for (_, x) in o.iter_mut() {
                walk_obj(x, a, b, z, m);
            }
        }
        Value::Array(arr) => {
            for x in arr {
                walk_obj(x, a, b, z, m);
            }
        }
        _ => {}
    }
}

fn walk_arr(v: &mut Value, key: &str, m: &mut Mapper) {
    match v {
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                if k == key && x.is_array() {
                    map_array(x, m);
                } else {
                    walk_arr(x, key, m);
                }
            }
        }
        Value::Array(arr) => {
            for x in arr {
                walk_arr(x, key, m);
            }
        }
        _ => {}
    }
}

/// Visits the nodes a dotted path names (`inputs.centre`, `arms`, `` for the root); `[]` is not
/// needed because the leaf walkers descend through arrays themselves.
fn scope_nodes<'a>(v: &'a mut Value, path: &str, f: &mut dyn FnMut(&'a mut Value)) {
    if path.is_empty() {
        f(v);
        return;
    }
    let (head, rest) = path.split_once('.').unwrap_or((path, ""));
    match v {
        Value::Object(o) => {
            if let Some(x) = o.get_mut(head) {
                scope_nodes(x, rest, f);
            }
        }
        Value::Array(a) => {
            for x in a.iter_mut() {
                scope_nodes(x, path, f);
            }
        }
        _ => {}
    }
}

fn apply_rules(data: &mut Value, rules: &[Rule], m: &mut Mapper) {
    for rule in rules {
        scope_nodes(data, rule.scope, &mut |node| match rule.leaf {
            Leaf::Obj(a, b, z) => walk_obj(node, a, b, z, m),
            Leaf::Arr(key) => walk_arr(node, key, m),
            Leaf::Direct => map_array(node, m),
        });
    }
}

/// The alignment: each element's start point is moved exactly; its heading, length and curvature
/// follow the local bearing and scale of the mapping, measured along the element over a step of
/// 100 m (a numerical differentiation step, not a design value).
fn map_alignment(data: &mut Value, m: &mut Mapper) {
    const STEP_M: f64 = 100.0;
    let Some(Value::Array(elements)) = data.get_mut("elements") else { return };
    for el in elements.iter_mut() {
        let g = |k: &str| el.get(k).and_then(num).unwrap_or(0.0);
        let (x, y, h, len, k0, k1) = (g("x"), g("y"), g("heading_rad"), g("length"), g("curvature_start"), g("curvature_end"));
        let (x2, y2) = m.map(x, y, 0.0);
        let (px, py) = m.map(x + STEP_M * h.cos(), y + STEP_M * h.sin(), 0.0);
        let (dx, dy) = (px - x2, py - y2);
        let scale = (dx.hypot(dy) / STEP_M).max(f64::MIN_POSITIVE);
        let h2 = dy.atan2(dx).rem_euclid(std::f64::consts::TAU);
        if let Value::Object(o) = el {
            for (k, val) in [("x", x2), ("y", y2), ("heading_rad", h2), ("length", len * scale), ("curvature_start", k0 / scale), ("curvature_end", k1 / scale)] {
                if let Some(slot) = o.get_mut(k) {
                    put(slot, val);
                }
            }
        }
    }
}

/// A placement stored as an offset in model millimetres (first two numbers), a turn in degrees about
/// the file's own origin and, for an underlay, a scale: the offset goes through the mapping, the turn
/// follows the local bearing change of the mapping along the file's x axis, and the scale follows its
/// local scale change when `scale_key` is given (a reference model is not scaled).
fn map_placement(v: &mut Value, offset_key: &str, rotation_key: &str, scale_key: Option<&str>, m: &mut Mapper) {
    const STEP_M: f64 = 100.0;
    let Some(off) = v.get(offset_key).and_then(|o| o.as_array()).map(|a| a.iter().filter_map(num).collect::<Vec<f64>>()) else { return };
    if off.len() < 2 {
        return;
    }
    let rot = v.get(rotation_key).and_then(num).unwrap_or(0.0);
    let (oe, on) = crs_from_model_point(afrobim_kernel::Pt::new(off[0], off[1]));
    let a = rot.to_radians();
    let (e2, n2) = m.map(oe, on, 0.0);
    let (pe, pn) = m.map(oe + STEP_M * a.cos(), on + STEP_M * a.sin(), 0.0);
    let (dx, dy) = (pe - e2, pn - n2);
    let scale = dx.hypot(dy) / STEP_M;
    let turned = dy.atan2(dx).to_degrees();
    let p = model_point_from_crs(e2, n2);
    if let Some(Value::Array(o)) = v.get_mut(offset_key) {
        put(&mut o[0], p.x);
        put(&mut o[1], p.y);
    }
    if let Some(slot) = v.get_mut(rotation_key) {
        put(slot, turned);
    }
    if let (Some(k), true) = (scale_key, scale.is_finite() && scale > 0.0) {
        if let Some(slot) = v.get_mut(k) {
            if let Some(cur) = num(slot) {
                put(slot, cur * scale);
            }
        }
    }
}

// ─── the safety check ──────────────────────────────────────────────────

/// The extent of the document in metres of its own coordinates, widened by a kilometre.
fn extent_m(elements: &[Element]) -> Option<([f64; 2], [f64; 2])> {
    let mut lo = [f64::MAX; 2];
    let mut hi = [f64::MIN; 2];
    for e in elements {
        if e.category == afrobim_kernel::PROJECT_SETTINGS_CATEGORY {
            continue;
        }
        for (x, y) in [(e.origin.x, e.origin.y), (e.origin.x + e.width, e.origin.y + e.height)] {
            let (a, b) = crs_from_model_point(afrobim_kernel::Pt::new(x, y));
            lo = [lo[0].min(a), lo[1].min(b)];
            hi = [hi[0].max(a), hi[1].max(b)];
        }
    }
    (lo[0] <= hi[0]).then_some(([lo[0] - 1000.0, lo[1] - 1000.0], [hi[0] + 1000.0, hi[1] + 1000.0]))
}

/// The path of the first number that looks like a coordinate of the document and did not change.
fn unchanged_coordinate(old: &Value, new: &Value, path: &str, lo: [f64; 2], hi: [f64; 2]) -> Option<String> {
    match (old, new) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64()?, b.as_f64()?);
            let near = |v: f64| (v >= lo[0] && v <= hi[0] && v.abs() > 1.0e4) || (v >= lo[1] && v <= hi[1] && v.abs() > 1.0e4);
            (a.to_bits() == b.to_bits() && near(a)).then(|| path.to_string())
        }
        (Value::Object(a), Value::Object(b)) => a.iter().find_map(|(k, x)| b.get(k).and_then(|y| unchanged_coordinate(x, y, &format!("{path}.{k}"), lo, hi))),
        (Value::Array(a), Value::Array(b)) => a.iter().zip(b).enumerate().find_map(|(i, (x, y))| unchanged_coordinate(x, y, &format!("{path}[{i}]"), lo, hi)),
        _ => None,
    }
}

// ─── the survey of a document ──────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct CategoryRow {
    pub category: String,
    pub count: usize,
    pub how: How,
}

/// What a transform would meet: one row per category in scope.
pub fn survey(doc: &Document, ids: Option<&[ElementId]>) -> Vec<CategoryRow> {
    let mut rows: Vec<CategoryRow> = Vec::new();
    for e in doc.elements() {
        if ids.is_some_and(|ids| !ids.contains(&e.id)) {
            continue;
        }
        match rows.iter_mut().find(|r| r.category == e.category) {
            Some(r) => r.count += 1,
            None => rows.push(CategoryRow { category: e.category.clone(), count: 1, how: how(&e.category) }),
        }
    }
    rows
}

/// The categories in scope that cannot be transformed.
pub fn unsupported(doc: &Document, ids: Option<&[ElementId]>) -> Vec<CategoryRow> {
    let mut rows: Vec<CategoryRow> = survey(doc, ids).into_iter().filter(|r| r.how == How::Unsupported).collect();
    // A referenced object belongs to another document: it is read-only here and would be replaced
    // by the next synchronise, so moving it would leave the document half in each system.
    let refs = scope_ids(doc, ids).into_iter().filter(|id| doc.find(*id).is_some_and(afrobim_shortcuts::is_reference)).count();
    if refs > 0 {
        rows.push(CategoryRow { category: "referenced objects (promote or remove them first)".into(), count: refs, how: How::Unsupported });
    }
    rows
}

/// The elements a transform would touch: the whole document or the selection (settings excluded).
pub fn scope_ids(doc: &Document, selected: Option<&[ElementId]>) -> Vec<ElementId> {
    doc.elements().iter().filter(|e| e.category != afrobim_kernel::PROJECT_SETTINGS_CATEGORY).filter(|e| selected.is_none_or(|s| s.contains(&e.id))).map(|e| e.id).collect()
}

/// The extent of the elements in scope, metres, `(min first, min second, max first, max second)` of the source coordinates.
pub fn extent_of(doc: &Document, selected: Option<&[ElementId]>) -> Option<((f64, f64), (f64, f64))> {
    let ids = scope_ids(doc, selected);
    let els: Vec<Element> = doc.elements().iter().filter(|e| ids.contains(&e.id)).cloned().collect();
    extent_m(&els).map(|(lo, hi)| ((lo[0] + 1000.0, lo[1] + 1000.0), (hi[0] - 1000.0, hi[1] - 1000.0)))
}

// ─── the log ───────────────────────────────────────────────────────────

/// One entry of the document's transform log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub what: String,
    pub transform: Option<TransformLog>,
    pub calibration: Option<SiteCalibration>,
    /// Elements changed, positions moved.
    pub elements: usize,
    pub positions: usize,
    /// Notes the transform made (alignment joints, unsupported, heights).
    pub notes: Vec<String>,
    pub when: Option<String>,
}

impl LogEntry {
    pub fn summary(&self) -> String {
        match (&self.transform, &self.calibration) {
            (Some(t), _) => t.summary(),
            (None, Some(c)) => c.sentence(),
            _ => self.what.clone(),
        }
    }
}

pub fn log_of(doc: &Document) -> Vec<LogEntry> {
    project_setting(doc, LOG_KEY).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

pub fn push_log_entry(doc: &mut Document, entry: LogEntry) {
    push_log(doc, entry)
}

fn push_log(doc: &mut Document, entry: LogEntry) {
    let mut log = log_of(doc);
    log.push(entry);
    set_project_setting(doc, LOG_KEY, serde_json::to_value(&log).unwrap_or_default());
}

pub fn calibrations_of(doc: &Document) -> Vec<SiteCalibration> {
    project_setting(doc, CALIBRATIONS_KEY).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

pub fn store_calibration(doc: &mut Document, cal: SiteCalibration) {
    let mut all: Vec<SiteCalibration> = calibrations_of(doc).into_iter().filter(|c| c.name != cal.name).collect();
    all.push(cal);
    set_project_setting(doc, CALIBRATIONS_KEY, serde_json::to_value(&all).unwrap_or_default());
}

/// The time, UTC, as `2026-11-16T09:30:00Z`.
pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil from days (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mth <= 2 { y + 1 } else { y };
    format!("{y:04}-{mth:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
}

// ─── applying ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub elements: usize,
    pub positions: usize,
    pub notes: Vec<String>,
}

/// Transforms one element's data and position on the copy. Returns an error naming what stopped it.
fn transform_element(e: &mut Element, m: &mut Mapper, audit: Option<([f64; 2], [f64; 2])>) -> Result<(), String> {
    let class = how(&e.category);
    let before = e.data.clone();
    match class {
        How::Rules => {
            if let Some(rs) = rules(&e.category) {
                apply_rules(&mut e.data, rs, m);
            }
            if e.category == civil::SURFACE && e.data.to_string().contains("\"tin\"") {
                return Err(format!("{} holds a pasted TIN, whose coordinates are kept relative to an origin the transform does not rewrite.", e.name));
            }
        }
        How::Alignment => map_alignment(&mut e.data, m),
        How::Placed | How::NotGeographic | How::Rigid | How::Reprojected => {}
        How::Underlay => map_placement(&mut e.data, "offset_mm", "rotation_deg", Some("scale"), m),
        How::Reference => {
            if let Some(Value::Array(models)) = e.data.get_mut("models") {
                for entry in models.iter_mut() {
                    map_placement(entry, "offset_mm", "rotation_deg", None, m);
                }
            }
        }
        How::Derived => {
            if let Some(Value::Array(clashes)) = e.data.get_mut("results").and_then(|r| r.get_mut("clashes")) {
                clashes.clear();
            }
        }
        How::Unsupported => return Err(format!("{} ({}) is of a kind the transform has not been taught", e.name, e.category)),
    }
    if matches!(class, How::Placed) {
        if let Some((lo, hi)) = audit {
            if let Some(path) = unchanged_coordinate(&before, &e.data, "", lo, hi) {
                return Err(format!("{} ({}) holds a number that looks like a coordinate at {path} and the transform does not know how to move it", e.name, e.category));
            }
        }
    }
    // An assembly or a ground profile sits at the model origin by convention: it has no position to move.
    if !matches!(class, How::NotGeographic | How::Underlay | How::Reference | How::Derived | How::Reprojected) && (!civil::is_civil(&e.category) || civil::has_plan_position(&e.category)) {
        let (oe, on) = crs_from_model_point(e.origin);
        let (ne, nn) = m.map(oe, on, 0.0);
        e.origin = model_point_from_crs(ne, nn);
        if let Some(s) = &mut e.site {
            let (se, sn) = crs_from_model_point(afrobim_kernel::Pt::new(s.x, s.y));
            let (se2, sn2) = m.map(se, sn, s.z / 1000.0);
            let p = model_point_from_crs(se2, sn2);
            s.x = p.x;
            s.y = p.y;
        }
        e.properties.remove("civil.cache");
        if civil::is_civil(&e.category) {
            // A new revision so the drawn scene is rebuilt.
            e.properties.insert(civil::PROP_REV.into(), format!("t{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
        }
    }
    Ok(())
}

/// Applies `map` to the elements in scope. `selected` is None for the whole document. Nothing
/// changes unless every position maps. Returns the report, with the alignment joint gaps stated.
pub fn apply_map(doc: &mut Document, selected: Option<&[ElementId]>, map: &MapFn) -> Result<Report, String> {
    let bad = unsupported(doc, selected);
    if !bad.is_empty() {
        let names: Vec<String> = bad.iter().map(|r| format!("{} ({} element(s))", r.category, r.count)).collect();
        return Err(format!("Cannot transform: the document holds element categories the transform cannot move, and it will not leave a document half in each system. Remove or reproject them elsewhere: {}.", names.join(", ")));
    }
    let ids = scope_ids(doc, selected);
    let mut elements = doc.snapshot();
    let audit = extent_m(&elements.iter().filter(|e| ids.contains(&e.id)).cloned().collect::<Vec<_>>());
    let mut mapper = Mapper { f: map, positions: 0, error: None };
    let mut report = Report::default();
    let mut gap_before = 0.0_f64;
    let mut gap_after = 0.0_f64;
    for e in elements.iter_mut().filter(|e| ids.contains(&e.id)) {
        if e.category == "alignment" {
            gap_before = gap_before.max(joint_gap(&e.data));
        }
        transform_element(e, &mut mapper, audit)?;
        if e.category == "alignment" {
            gap_after = gap_after.max(joint_gap(&e.data));
        }
        report.elements += 1;
    }
    if let Some(why) = mapper.error {
        return Err(format!("Nothing was changed: a position could not be transformed ({why})."));
    }
    report.positions = mapper.positions;
    if gap_before > 0.0 || gap_after > 0.0 {
        report.notes.push(format!("Alignment element joints: the largest gap was {gap_before:.6} m before and is {gap_after:.6} m after (each element follows the local scale and bearing of the transformation, so a joint can open by the difference between neighbouring elements)."));
    }
    doc.restore(elements);
    Ok(report)
}

fn joint_gap(data: &Value) -> f64 {
    let Ok(al) = serde_json::from_value::<afrobim_alignment::Alignment>(data.clone()) else { return 0.0 };
    al.joints().iter().map(|j| j.gap_m).fold(0.0, f64::max)
}

/// Runs a plan over the document or the selection: moves the elements, sets the project system to
/// the target for a whole-document transform, and writes the log entry.
pub fn apply_plan(doc: &mut Document, selected: Option<&[ElementId]>, plan: &TransformPlan, target: &ProjectCrs, what: &str, when: String) -> Result<LogEntry, String> {
    let before = doc.snapshot();
    let f = |e: f64, n: f64, z: f64| plan.apply(e, n, z).map(|p| (p.first, p.second)).map_err(|x| x.to_string());
    let report = apply_map(doc, selected, &f)?;
    // The points of a point cloud stay in their file: the document records the plan they are drawn through.
    let cloud_ids: Vec<ElementId> = scope_ids(doc, selected).into_iter().filter(|id| doc.find(*id).is_some_and(|e| e.category == civil::POINTCLOUD)).collect();
    crate::cloud::note_move(doc, &cloud_ids, plan);
    let mut log = TransformLog::with_counts(what, plan, Some(when.clone()), report.positions, report.positions, 0);
    log.warnings.extend(report.notes.iter().cloned());
    let mut notes = report.notes.clone();
    if selected.is_some() {
        notes.push("Only the selected objects were transformed: the project coordinate system was not changed, so the document now holds objects in two systems.".into());
    } else {
        let old = afrobim_kernel::project_crs(doc);
        crate::imagery::pin_crs(doc, old.as_ref());
        set_project_crs(doc, target);
        // GIS overlays are placed again from their WGS 84 source.
        crate::gis_overlay::reproject_all(doc);
    }
    let entry = LogEntry { what: what.to_string(), transform: Some(log), calibration: None, elements: report.elements, positions: report.positions, notes, when: Some(when) };
    push_log(doc, entry.clone());
    let _ = before;
    Ok(entry)
}

/// Applies a stored site calibration (local grid to the target system) to the whole document.
pub fn apply_calibration(doc: &mut Document, cal: &SiteCalibration, target: &ProjectCrs, when: String) -> Result<LogEntry, String> {
    if doc.elements().iter().any(|e| e.category == civil::POINTCLOUD) {
        return Err("The document holds a point cloud, whose points stay in their file and cannot be moved by a site calibration. Remove the cloud first, or use TRANSFORM for a datum change.".into());
    }
    let f = |e: f64, n: f64, _z: f64| Ok(cal.to_target(e, n));
    let report = apply_map(doc, None, &f)?;
    crate::imagery::move_unpinned(doc, &|e, n| cal.to_target(e, n));
    set_project_crs(doc, target);
    crate::gis_overlay::reproject_all(doc);
    let entry = LogEntry { what: format!("Site calibration {}", cal.name), transform: None, calibration: Some(cal.clone()), elements: report.elements, positions: report.positions, notes: report.notes, when: Some(when) };
    push_log(doc, entry.clone());
    Ok(entry)
}

/// The bearing and scale change of a mapping at a position, from a 1 km baseline measured both ways.
pub fn effects_text(plan: &TransformPlan, e: f64, n: f64, h: f64) -> String {
    match plan.point_effects(e, n, h) {
        Some(p) => p.sentence(),
        None => "The scale and bearing change is not available for this pair of systems.".into(),
    }
}
